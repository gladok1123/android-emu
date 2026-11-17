//! Small utilities: http download, hashing, formatting, process helpers.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Human readable byte size, e.g. `1.4 GB`.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", bytes, UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

pub fn human_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

/// Sanitize a string so it can be used as a Windows file/directory name.
pub fn sanitize_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        let bad = matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            || (ch as u32) < 0x20;
        out.push(if bad { '_' } else { ch });
    }
    let trimmed = out.trim().trim_end_matches('.').to_string();
    let trimmed = if trimmed.is_empty() { "app".to_string() } else { trimmed };
    // Windows reserved device names.
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(&trimmed)) {
        format!("{trimmed}_")
    } else {
        trimmed
    }
}

/// Simple deterministic hash (FNV-1a 64), used for stable ids and colors.
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Extract every PNG found in a blob (used to pull launcher icons out of ELF
/// binaries for Android-x86, where icons are not stored as separate files).
pub fn extract_pngs(data: &[u8], max: usize) -> Vec<(usize, Vec<u8>)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 8 <= data.len() && out.len() < max {
        if data[i..i + 8] == SIG {
            if let Some(len) = png_length(&data[i..]) {
                out.push((i, data[i..i + len].to_vec()));
                i += len;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn png_length(png: &[u8]) -> Option<usize> {
    let mut pos = 8usize;
    loop {
        if pos + 8 > png.len() {
            return None;
        }
        let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
        let ctype = &png[pos + 4..pos + 8];
        pos = pos.checked_add(12 + len)?;
        if pos > png.len() {
            return None;
        }
        if ctype == b"IEND" {
            return Some(pos);
        }
    }
}

pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Download `url` to `dest` using `curl.exe` (present on Windows 10 1803+).
/// Returns the number of bytes written.
pub fn download_with_curl(
    curl: &Path,
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(DownloadProgress),
) -> std::io::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("part");
    let mut child = std::process::Command::new(curl)
        .arg("-L")
        .arg("--fail")
        .arg("--retry")
        .arg("3")
        .arg("--connect-timeout")
        .arg("20")
        .arg("-o")
        .arg(&tmp)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    // Poll the partial file size while curl works.
    let mut last = 0u64;
    let mut stalled = 0;
    loop {
        match child.try_wait()? {
            Some(status) => {
                if !status.success() {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("download failed with exit code {:?}", status.code()),
                    ));
                }
                break;
            }
            None => {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let size = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
                if size != last {
                    last = size;
                    stalled = 0;
                } else {
                    stalled += 1;
                }
                on_progress(DownloadProgress { downloaded: size, total: None });
                if stalled > 600 {
                    let _ = child.kill();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "download stalled",
                    ));
                }
            }
        }
    }
    std::fs::rename(&tmp, dest)?;
    Ok(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(last))
}

/// Search for an executable in `PATH`.
pub fn which(exe: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(exe);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let cand = dir.join(format!("{exe}.exe"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

pub fn copy_file(src: &Path, dst: &Path) -> std::io::Result<u64> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)
}

pub fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn write_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = File::create(path)?;
    f.write_all(data)
}

/// Quote a command line argument for Windows `CreateProcess` style parsing.
pub fn win_quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_file_names() {
        assert_eq!(sanitize_file_name("a/b:c*d"), "a_b_c_d");
        assert_eq!(sanitize_file_name("CON"), "CON_");
        assert_eq!(sanitize_file_name("   "), "app");
        assert_eq!(sanitize_file_name("Моё приложение"), "Моё приложение");
    }

    #[test]
    fn quotes_windows_args() {
        assert_eq!(win_quote("plain"), "plain");
        assert_eq!(win_quote("a b"), "\"a b\"");
        assert_eq!(win_quote("c:\\x\\"), "c:\\x\\");
    }

    #[test]
    fn extracts_png_blob() {
        let mut data = vec![0u8; 16];
        // minimal 1x1 png
        let png: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82,
            0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 196,
            137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 248, 207, 192, 240,
            31, 0, 5, 0, 1, 255, 86, 199, 47, 13, 0, 0, 0, 0, 73, 69,
            78, 68, 174, 66, 96, 130,
        ];
        data.extend_from_slice(png);
        let found = extract_pngs(&data, 4);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 16);
        assert_eq!(found[0].1.len(), png.len());
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(3 * 1024 * 1024), "3.0 MB");
    }
}
