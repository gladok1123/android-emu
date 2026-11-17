//! Reader for the ZIP container format used by APK files.

use crate::inflate;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug)]
pub enum ZipError {
    Io(String),
    BadFormat(&'static str),
    EntryNotFound(String),
    Inflate(inflate::InflateError),
    EntryTooLarge(String),
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::Io(e) => write!(f, "i/o error: {e}"),
            ZipError::BadFormat(m) => write!(f, "not a valid ZIP/APK archive: {m}"),
            ZipError::EntryNotFound(n) => write!(f, "entry not found in archive: {n}"),
            ZipError::Inflate(e) => write!(f, "decompression failed: {e}"),
            ZipError::EntryTooLarge(n) => write!(f, "entry {n} is too large"),
        }
    }
}

impl std::error::Error for ZipError {}

impl From<inflate::InflateError> for ZipError {
    fn from(e: inflate::InflateError) -> Self {
        ZipError::Inflate(e)
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub compression: u16,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub local_header_offset: u64,
}

#[derive(Debug)]
pub struct ZipArchive {
    path: std::path::PathBuf,
    pub entries: Vec<Entry>,
}

const EOCD_SIG: u32 = 0x0605_4b50;
const CEN_SIG: u32 = 0x0201_4b50;
const LOC_SIG: u32 = 0x0403_4b50;

/// Refuse to decompress absurd entries (protects against zip bombs).
const MAX_ENTRY_SIZE: usize = 256 * 1024 * 1024;

impl ZipArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<ZipArchive, ZipError> {
        let path = path.as_ref().to_path_buf();
        let mut f = File::open(&path).map_err(|e| ZipError::Io(e.to_string()))?;
        let file_len = f
            .metadata()
            .map_err(|e| ZipError::Io(e.to_string()))?
            .len();
        if file_len < 22 {
            return Err(ZipError::BadFormat("file too small"));
        }

        // Locate the End Of Central Directory record (search backwards, up to 64 KiB comment).
        let search_len = file_len.min(22 + 0xFFFF);
        let mut tail = vec![0u8; search_len as usize];
        f.seek(SeekFrom::End(-(search_len as i64)))
            .map_err(|e| ZipError::Io(e.to_string()))?;
        f.read_exact(&mut tail)
            .map_err(|e| ZipError::Io(e.to_string()))?;
        let eocd_pos = (0..tail.len().saturating_sub(21))
            .rev()
            .find(|&i| u32::from_le_bytes([tail[i], tail[i + 1], tail[i + 2], tail[i + 3]]) == EOCD_SIG)
            .ok_or(ZipError::BadFormat("end of central directory not found"))?;

        let eocd = &tail[eocd_pos..];
        let total_entries = u16::from_le_bytes([eocd[10], eocd[11]]) as u64;
        let cd_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]) as u64;
        let cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]) as u64;

        // Some APKs use the "Zip64" locator when offsets do not fit in 32 bits; not
        // needed for realistic APKs but fall back gracefully if the CD looks bogus.
        let (cd_offset, cd_size) = if cd_offset == 0xFFFF_FFFF || cd_offset + cd_size > file_len {
            let mut fallback = (cd_offset, cd_size);
            if let Some(z64) = find_zip64_eocd(&mut f, &tail, eocd_pos, file_len) {
                fallback = z64;
            }
            fallback
        } else {
            (cd_offset, cd_size)
        };
        if cd_offset + 8 > file_len {
            return Err(ZipError::BadFormat("central directory offset out of range"));
        }

        let mut cd = vec![0u8; cd_size.min(file_len - cd_offset) as usize];
        f.seek(SeekFrom::Start(cd_offset))
            .map_err(|e| ZipError::Io(e.to_string()))?;
        f.read_exact(&mut cd)
            .map_err(|e| ZipError::Io(e.to_string()))?;

        let mut entries = Vec::with_capacity(total_entries as usize);
        let mut pos = 0usize;
        while pos + 46 <= cd.len() {
            let sig = le_u32(&cd, pos)?;
            if sig != CEN_SIG {
                break;
            }
            let flags = le_u16(&cd, pos + 8)?;
            let compression = le_u16(&cd, pos + 10)?;
            let compressed_size = le_u32(&cd, pos + 20)? as u64;
            let uncompressed_size = le_u32(&cd, pos + 24)? as u64;
            let name_len = le_u16(&cd, pos + 28)? as usize;
            let extra_len = le_u16(&cd, pos + 30)? as usize;
            let comment_len = le_u16(&cd, pos + 32)? as usize;
            let local_header_offset = le_u32(&cd, pos + 42)? as u64;
            let name_start = pos + 46;
            let name_end = name_start + name_len;
            if name_end > cd.len() {
                return Err(ZipError::BadFormat("truncated central directory entry"));
            }
            let mut name_bytes = cd[name_start..name_end].to_vec();
            if flags & 0x800 == 0 {
                // Not flagged as UTF-8; APK names are effectively always UTF-8/ASCII.
                // Keep the bytes as-is (lossy conversion) which is fine for our use.
            }
            // Handle Zip64 extra field when sizes/offsets are 0xFFFFFFFF.
            let mut csize = compressed_size;
            let mut usize = uncompressed_size;
            let mut loff = local_header_offset;
            if csize == 0xFFFF_FFFF || usize == 0xFFFF_FFFF || loff == 0xFFFF_FFFF {
                let extra_start = name_end;
                let extra_end = (extra_start + extra_len).min(cd.len());
                let mut ex = extra_start;
                while ex + 4 <= extra_end {
                    let id = le_u16(&cd, ex)?;
                    let sz = le_u16(&cd, ex + 2)? as usize;
                    if id == 0x0001 {
                        let mut p = ex + 4;
                        if usize == 0xFFFF_FFFF && p + 8 <= cd.len() {
                            usize = le_u64(&cd, p)?;
                            p += 8;
                        }
                        if csize == 0xFFFF_FFFF && p + 8 <= cd.len() {
                            csize = le_u64(&cd, p)?;
                            p += 8;
                        }
                        if loff == 0xFFFF_FFFF && p + 8 <= cd.len() {
                            loff = le_u64(&cd, p)?;
                        }
                    }
                    ex += 4 + sz;
                }
            }
            // Restore non-UTF8 safe lossy conversion.
            let name = String::from_utf8_lossy(&mut name_bytes).to_string();
            entries.push(Entry {
                name,
                compression,
                compressed_size: csize,
                uncompressed_size: usize,
                local_header_offset: loff,
            });
            pos = name_end + extra_len + comment_len;
        }

        Ok(ZipArchive { path, entries })
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// Read and (if needed) decompress a single entry into memory.
    pub fn read_entry(&self, name: &str) -> Result<Vec<u8>, ZipError> {
        let entry = self
            .find(name)
            .ok_or_else(|| ZipError::EntryNotFound(name.to_string()))?
            .clone();
        let mut f = File::open(&self.path).map_err(|e| ZipError::Io(e.to_string()))?;
        let mut header = [0u8; 30];
        f.seek(SeekFrom::Start(entry.local_header_offset))
            .map_err(|e| ZipError::Io(e.to_string()))?;
        f.read_exact(&mut header)
            .map_err(|e| ZipError::Io(e.to_string()))?;
        if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) != LOC_SIG {
            return Err(ZipError::BadFormat("bad local file header"));
        }
        let name_len = u16::from_le_bytes([header[26], header[27]]) as u64;
        let extra_len = u16::from_le_bytes([header[28], header[29]]) as u64;
        let data_start = entry.local_header_offset + 30 + name_len + extra_len;
        f.seek(SeekFrom::Start(data_start))
            .map_err(|e| ZipError::Io(e.to_string()))?;
        let mut raw = vec![0u8; entry.compressed_size as usize];
        f.read_exact(&mut raw)
            .map_err(|e| ZipError::Io(e.to_string()))?;

        match entry.compression {
            0 => {
                if raw.len() > MAX_ENTRY_SIZE {
                    return Err(ZipError::EntryTooLarge(entry.name));
                }
                Ok(raw)
            }
            8 => {
                let limit = (entry.uncompressed_size as usize)
                    .min(MAX_ENTRY_SIZE)
                    .max(64);
                let mut out = Vec::with_capacity(limit.min(1 << 20));
                inflate::inflate_into(&raw, &mut out, limit)?;
                Ok(out)
            }
            other => Err(ZipError::BadFormat(match other {
                9 => "unsupported compression method (deflate64)",
                12 => "unsupported compression method (bzip2)",
                14 => "unsupported compression method (lzma)",
                _ => "unsupported compression method",
            })),
        }
    }

    /// Read an entry of a known small size, avoiding huge allocations.
    pub fn read_entry_limited(&self, name: &str, limit: usize) -> Result<Vec<u8>, ZipError> {
        let data = self.read_entry(name)?;
        if data.len() > limit {
            return Err(ZipError::EntryTooLarge(name.to_string()));
        }
        Ok(data)
    }
}

fn le_u16(d: &[u8], at: usize) -> Result<u16, ZipError> {
    d.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(ZipError::BadFormat("truncated"))
}

fn le_u32(d: &[u8], at: usize) -> Result<u32, ZipError> {
    d.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ZipError::BadFormat("truncated"))
}

fn le_u64(d: &[u8], at: usize) -> Result<u64, ZipError> {
    d.get(at..at + 8)
        .map(|s| {
            u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
        })
        .ok_or(ZipError::BadFormat("truncated"))
}

fn find_zip64_eocd(
    _f: &mut File,
    tail: &[u8],
    eocd_pos: usize,
    _file_len: u64,
) -> Option<(u64, u64)> {
    // Zip64 end of central directory locator is 20 bytes right before the EOCD.
    if eocd_pos < 20 {
        return None;
    }
    let loc = &tail[eocd_pos - 20..eocd_pos];
    if u32::from_le_bytes([loc[0], loc[1], loc[2], loc[3]]) != 0x0706_4b50 {
        return None;
    }
    let z64_offset = u64::from_le_bytes([
        loc[8], loc[9], loc[10], loc[11], loc[12], loc[13], loc[14], loc[15],
    ]);
    let mut rec = [0u8; 56];
    let f = _f;
    f.seek(SeekFrom::Start(z64_offset)).ok()?;
    f.read_exact(&mut rec).ok()?;
    if u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]) != 0x0606_4b50 {
        return None;
    }
    let cd_size = u64::from_le_bytes([
        rec[40], rec[41], rec[42], rec[43], rec[44], rec[45], rec[46], rec[47],
    ]);
    let cd_offset = u64::from_le_bytes([
        rec[48], rec[49], rec[50], rec[51], rec[52], rec[53], rec[54], rec[55],
    ]);
    Some((cd_offset, cd_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_repo_readme_zip() {
        // A tiny archive assembled by hand: one stored file "a.txt" -> "hi".
        let mut data: Vec<u8> = Vec::new();
        let payload = b"hi";
        // local header
        data.extend_from_slice(&LOC_SIG.to_le_bytes());
        data.extend_from_slice(&20u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // crc (unchecked)
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(&5u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(b"a.txt");
        data.extend_from_slice(payload);
        let cd_start = data.len();
        data.extend_from_slice(&CEN_SIG.to_le_bytes());
        data.extend_from_slice(&20u16.to_le_bytes());
        data.extend_from_slice(&20u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(&5u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(b"a.txt");
        let cd_size = data.len() - cd_start;
        data.extend_from_slice(&EOCD_SIG.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&(cd_size as u32).to_le_bytes());
        data.extend_from_slice(&(cd_start as u32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());

        let dir = std::env::temp_dir().join("android-emu-zip-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.zip");
        std::fs::write(&p, &data).unwrap();
        let zip = ZipArchive::open(&p).unwrap();
        assert_eq!(zip.entries.len(), 1);
        assert_eq!(zip.read_entry("a.txt").unwrap(), b"hi");
        let _ = std::fs::remove_file(&p);
    }
}
