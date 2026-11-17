//! Minimal DEFLATE (RFC 1951) / ZLIB (RFC 1950) decompressor.
//!
//! APK entries are usually deflate-compressed, and `AndroidManifest.xml` /
//! `resources.arsc` must be read before anything can be installed, so the
//! project carries its own small inflater instead of pulling a dependency.
//! The implementation is a straight port of the classic
//! `tinf`/`puff` design: canonical Huffman decoding with a fast lookup table.

use std::fmt;

#[derive(Debug)]
pub enum InflateError {
    Truncated,
    BadBlockType(u32),
    BadStoredLength,
    BadCode,
    BadDistance,
    NeedDict,
    ZlibHeader,
    ZlibChecksum,
    OutputTooLarge,
    BadLength,
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InflateError::Truncated => write!(f, "unexpected end of compressed data"),
            InflateError::BadBlockType(t) => write!(f, "invalid deflate block type {t}"),
            InflateError::BadStoredLength => write!(f, "stored block length mismatch"),
            InflateError::BadCode => write!(f, "invalid Huffman code"),
            InflateError::BadDistance => write!(f, "invalid back-reference distance"),
            InflateError::NeedDict => write!(f, "deflate stream requires a preset dictionary"),
            InflateError::ZlibHeader => write!(f, "invalid zlib header"),
            InflateError::ZlibChecksum => write!(f, "zlib Adler-32 checksum mismatch"),
            InflateError::OutputTooLarge => write!(f, "decompressed data exceeds the requested limit"),
            InflateError::BadLength => write!(f, "invalid length code"),
        }
    }
}

impl std::error::Error for InflateError {}

const MAX_BITS: usize = 15;
const MAX_LIT_CODES: usize = 288;
const MAX_DIST_CODES: usize = 32;
const MAX_CODES: usize = MAX_LIT_CODES + MAX_DIST_CODES;
/// Fast lookup table size (9 bits = 512 entries, the classic compromise).
const FAST_BITS: u32 = 9;

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, bit_buf: 0, bit_count: 0 }
    }

    #[inline]
    fn need(&mut self, n: u32) -> Result<(), InflateError> {
        while self.bit_count < n {
            let byte = *self.data.get(self.pos).ok_or(InflateError::Truncated)?;
            self.pos += 1;
            self.bit_buf |= (byte as u32) << self.bit_count;
            self.bit_count += 8;
        }
        Ok(())
    }

    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit_buf & ((1u32 << n) - 1);
        self.bit_buf >>= n;
        self.bit_count -= n;
        Ok(v)
    }

    #[inline]
    fn align(&mut self) {
        let drop = self.bit_count % 8;
        self.bit_buf >>= drop;
        self.bit_count -= drop;
    }

    /// Rewind whole bytes that were read into the bit buffer but not consumed.
    fn sync_to_byte(&mut self) -> usize {
        self.pos - (self.bit_count / 8) as usize
    }

    fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
        self.bit_buf = 0;
        self.bit_count = 0;
    }
}

/// Canonical Huffman decoding table in the style of zlib's `inflate_table`.
struct HuffTable {
    count: [u16; MAX_BITS + 1],
    symbol: Vec<u16>,
    /// fast[sym] = (length << 9) | symbol  (0 = slow path needed)
    fast: Vec<u16>,
    max_len: u32,
}

impl HuffTable {
    fn new(lengths: &[u8]) -> Result<HuffTable, InflateError> {
        let mut count = [0u16; MAX_BITS + 1];
        for &l in lengths {
            if l as usize > MAX_BITS {
                return Err(InflateError::BadCode);
            }
            count[l as usize] += 1;
        }
        count[0] = 0;

        // Check for an over-subscribed or incomplete set of lengths.
        let mut left: i32 = 1;
        for len in 1..=MAX_BITS {
            left <<= 1;
            left -= count[len] as i32;
            if left < 0 {
                return Err(InflateError::BadCode);
            }
        }

        let mut offsets = [0u16; MAX_BITS + 1];
        for len in 1..MAX_BITS {
            offsets[len + 1] = offsets[len] + count[len];
        }
        let total: usize = lengths.iter().filter(|&&l| l > 0).count();
        let mut symbol = vec![0u16; total];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offsets[l as usize] as usize] = sym as u16;
                offsets[l as usize] += 1;
            }
        }

        let max_len = (1..=MAX_BITS).rev().find(|&l| count[l] != 0).unwrap_or(0) as u32;

        // Build the fast table for codes up to FAST_BITS long.
        let mut fast = vec![0u16; 1 << FAST_BITS];
        let mut next_code = [0u32; MAX_BITS + 2];
        let mut code: u32 = 0;
        for len in 1..=MAX_BITS {
            code = (code + count[len - 1] as u32) << 1;
            next_code[len] = code;
        }
        let mut idx = 0usize;
        for (sym, &l) in lengths.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let code = next_code[l as usize];
            next_code[l as usize] += 1;
            if (l as u32) <= FAST_BITS {
                // Reverse the code bits (deflate stores Huffman codes LSB-first).
                let mut rev = 0u32;
                for i in 0..l as u32 {
                    rev |= ((code >> i) & 1) << (l as u32 - 1 - i);
                }
                let step = 1u32 << l;
                let mut fill = rev;
                while fill < (1 << FAST_BITS) {
                    fast[fill as usize] = ((l as u16) << 9) | sym as u16;
                    fill += step;
                }
            }
            idx += 1;
        }
        let _ = idx;
        Ok(HuffTable { count, symbol, fast, max_len })
    }

    fn decode(&self, br: &mut BitReader<'_>) -> Result<u16, InflateError> {
        br.need(FAST_BITS.min(self.max_len.max(1)))?;
        let peek = (br.bit_buf & ((1 << FAST_BITS) - 1)) as usize;
        let entry = self.fast[peek];
        if entry != 0 {
            let len = (entry >> 9) as u32;
            br.bit_buf >>= len;
            br.bit_count -= len;
            return Ok(entry & 0x1FF);
        }
        // Slow path: bit-by-bit canonical decoding.
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..=MAX_BITS {
            code |= br.bits(1)? as i32;
            let count = self.count[len] as i32;
            if code - first < count {
                let i = (index + (code - first)) as usize;
                return self
                    .symbol
                    .get(i)
                    .copied()
                    .ok_or(InflateError::BadCode);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(InflateError::BadCode)
    }
}

fn build_fixed_tables() -> (HuffTable, HuffTable) {
    let mut lit_lengths = [0u8; MAX_LIT_CODES];
    for (i, item) in lit_lengths.iter_mut().enumerate() {
        *item = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist_lengths = [5u8; MAX_DIST_CODES];
    (
        HuffTable::new(&lit_lengths).expect("fixed literal table"),
        HuffTable::new(&dist_lengths).expect("fixed distance table"),
    )
}

/// Inflate a raw deflate stream, appending to `out`.
pub fn inflate_into(data: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), InflateError> {
    let mut br = BitReader::new(data);
    loop {
        let last = br.bits(1)?;
        let block_type = br.bits(2)?;
        match block_type {
            0 => inflate_stored(&mut br, data, out, limit)?,
            1 => {
                let (lit, dist) = build_fixed_tables();
                inflate_huffman(&mut br, out, &lit, &dist, limit)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_tables(&mut br)?;
                inflate_huffman(&mut br, out, &lit, &dist, limit)?;
            }
            other => return Err(InflateError::BadBlockType(other)),
        }
        if last == 1 {
            break;
        }
    }
    Ok(())
}

fn inflate_stored(
    br: &mut BitReader<'_>,
    data: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<(), InflateError> {
    br.align();
    let pos = br.sync_to_byte();
    br.set_pos(pos);
    let len = read_u16(data, pos)?;
    let nlen = read_u16(data, pos + 2)?;
    if len != !nlen {
        return Err(InflateError::BadStoredLength);
    }
    let start = pos + 4;
    let end = start + len as usize;
    if end > data.len() {
        return Err(InflateError::Truncated);
    }
    if out.len() + len as usize > limit {
        return Err(InflateError::OutputTooLarge);
    }
    out.extend_from_slice(&data[start..end]);
    br.set_pos(end);
    Ok(())
}

fn read_u16(data: &[u8], at: usize) -> Result<u16, InflateError> {
    data.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(InflateError::Truncated)
}

fn read_dynamic_tables(br: &mut BitReader<'_>) -> Result<(HuffTable, HuffTable), InflateError> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;
    if hlit > MAX_LIT_CODES || hdist > MAX_DIST_CODES {
        return Err(InflateError::BadCode);
    }
    let mut cl_lengths = [0u8; 19];
    for &idx in CODE_LENGTH_ORDER.iter().take(hclen) {
        cl_lengths[idx] = br.bits(3)? as u8;
    }
    let cl_table = HuffTable::new(&cl_lengths)?;

    let total = hlit + hdist;
    let mut lengths = vec![0u8; MAX_CODES];
    let mut i = 0usize;
    while i < total {
        let sym = cl_table.decode(br)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(InflateError::BadCode);
                }
                let prev = lengths[i - 1];
                let repeat = 3 + br.bits(2)? as usize;
                for _ in 0..repeat {
                    if i >= total {
                        return Err(InflateError::BadCode);
                    }
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                let repeat = 3 + br.bits(3)? as usize;
                i = (i + repeat).min(total);
            }
            18 => {
                let repeat = 11 + br.bits(7)? as usize;
                i = (i + repeat).min(total);
            }
            _ => return Err(InflateError::BadCode),
        }
    }

    let lit = HuffTable::new(&lengths[..hlit])?;
    let dist = HuffTable::new(&lengths[hlit..hlit + hdist])?;
    Ok((lit, dist))
}

fn inflate_huffman(
    br: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    lit: &HuffTable,
    dist: &HuffTable,
    limit: usize,
) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(br)? as usize;
        if sym < 256 {
            if out.len() >= limit {
                return Err(InflateError::OutputTooLarge);
            }
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let idx = sym - 257;
            if idx >= LENGTH_BASE.len() {
                return Err(InflateError::BadLength);
            }
            let len = LENGTH_BASE[idx] as usize + br.bits(LENGTH_EXTRA[idx] as u32)? as usize;
            let dsym = dist.decode(br)? as usize;
            if dsym >= DIST_BASE.len() {
                return Err(InflateError::BadDistance);
            }
            let d = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if d > out.len() {
                return Err(InflateError::BadDistance);
            }
            if out.len() + len > limit {
                return Err(InflateError::OutputTooLarge);
            }
            let start = out.len() - d;
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
}

/// Decompress a zlib (RFC 1950) stream.
pub fn zlib_decompress(data: &[u8], limit: usize) -> Result<Vec<u8>, InflateError> {
    if data.len() < 6 {
        return Err(InflateError::ZlibHeader);
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0F != 8 {
        return Err(InflateError::ZlibHeader);
    }
    if ((cmf as u16) << 8 | flg as u16) % 31 != 0 {
        return Err(InflateError::ZlibHeader);
    }
    if flg & 0x20 != 0 {
        return Err(InflateError::NeedDict);
    }
    let mut out = Vec::new();
    inflate_into(&data[2..], &mut out, limit)?;
    if data.len() >= 6 {
        let expected = u32::from_be_bytes([
            data[data.len() - 4],
            data[data.len() - 3],
            data[data.len() - 2],
            data[data.len() - 1],
        ]);
        if adler32(&out) != expected {
            return Err(InflateError::ZlibChecksum);
        }
    }
    Ok(out)
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test vectors live outside src/ so the release binary stays small.
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/inflate_vectors.rs"));

    #[test]
    fn adler32_known_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
    }

    #[test]
    fn decodes_real_zlib_streams() {
        let expected: [&[u8]; 2] = [b"hello hello hello hello", b"abcabcabcabcabcabc"];
        for (i, compressed) in VECTORS.iter().enumerate() {
            let out = zlib_decompress(compressed, 1 << 20)
                .unwrap_or_else(|e| panic!("vector {i} failed: {e}"));
            assert_eq!(out.len(), VECTOR_LENS[i], "vector {i} length");
            if i == 0 {
                assert_eq!(out.as_slice(), expected[0]);
            }
            if i == 1 {
                assert_eq!(out.as_slice(), RANDOM4000_SEED7);
            }
            if i == 2 {
                assert_eq!(out.as_slice(), [b"abcabcabc".repeat(50)].concat().as_slice());
            }
            if i == 4 {
                assert!(out.iter().all(|&b| b == b'x'));
            }
        }
    }

    #[test]
    fn rejects_truncated_and_oversized() {
        assert!(zlib_decompress(&VECTORS[0][..8], 1 << 20).is_err());
        assert!(matches!(
            zlib_decompress(&VECTORS[4], 1000),
            Err(InflateError::OutputTooLarge)
        ));
    }
}
