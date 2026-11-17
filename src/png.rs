//! Tiny PNG toolkit: CRC32, an uncompressed-deflate encoder, a procedural
//! letter-icon generator and a nearest-neighbour resizer.
//!
//! Everything the launcher needs to produce `.ico` files for shortcuts is here;
//! no image libraries are involved.

use crate::inflate;

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 12);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let mut crc_input = Vec::with_capacity(4 + payload.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(payload);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    out
}

/// Wrap raw bytes in a zlib stream using *stored* (uncompressed) deflate blocks.
/// Output is larger, but encoding is trivial and always valid.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut pos = 0;
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while pos < data.len() {
        let take = (data.len() - pos).min(65535);
        let last = pos + take == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(&data[pos..pos + take]);
        pos += take;
    }
    out.extend_from_slice(&inflate::adler32(data).to_be_bytes());
    out
}

/// Encode RGBA8 pixels (row-major, `w*h*4` bytes) as an 8-bit RGBA PNG.
pub fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * height * 4 + height) as usize);
    for y in 0..height {
        raw.push(0); // filter: none
        let start = (y * width * 4) as usize;
        let end = start + (width * 4) as usize;
        raw.extend_from_slice(&pixels[start..end.min(pixels.len())]);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // colour type: RGBA
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    out.extend_from_slice(&chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&chunk(b"IDAT", &zlib_store(&raw)));
    out.extend_from_slice(&chunk(b"IEND", &[]));
    out
}

/// 5x7 bitmap font, enough for the ASCII letters used on launcher icons.
fn glyph(ch: char) -> Option<[u8; 7]> {
    let g = match ch.to_ascii_uppercase() {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '-' => [0, 0, 0, 0x1F, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0x0C, 0x0C],
        ' ' => [0; 7],
        _ => return None,
    };
    Some(g)
}

/// Generate a rounded-square launcher icon with up to two initials.
pub fn letter_icon(label: &str, seed: &str) -> Vec<u8> {
    const SIZE: u32 = 128;
    let letters: Vec<char> = label
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect();
    let hash = crate::util::fnv1a64(seed.as_bytes());
    // Pleasant pastel-strong gradient (seed stable per package).
    let hue = (hash % 360) as f32;
    let (r0, g0, b0) = hsv_to_rgb(hue, 0.62, 0.92);
    let (r1, g1, b1) = hsv_to_rgb((hue + 28.0) % 360.0, 0.72, 0.62);

    let mut px = vec![0u8; (SIZE * SIZE * 4) as usize];
    let radius = SIZE as f32 * 0.22;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let t = (x + y) as f32 / (2.0 * SIZE as f32);
            let mut r = lerp(r0, r1, t);
            let mut g = lerp(g0, g1, t);
            let mut b = lerp(b0, b1, t);
            let alpha = rounded_alpha(x as f32, y as f32, SIZE as f32, radius);
            // Draw the initials.
            let idx = ((y * SIZE + x) * 4) as usize;
            if alpha > 0.0 {
                for (li, ch) in letters.iter().enumerate() {
                    if let Some(bits) = glyph(*ch) {
                        let scale = 9.0;
                        let glyph_w = 5.0 * scale;
                        let total_w = letters.len() as f32 * (glyph_w + scale);
                        let start_x = (SIZE as f32 - total_w + scale) / 2.0
                            + li as f32 * (glyph_w + scale);
                        let start_y = (SIZE as f32 - 7.0 * scale) / 2.0;
                        let gx = (x as f32 - start_x) / scale;
                        let gy = (y as f32 - start_y) / scale;
                        if gx >= 0.0 && gx < 5.0 && gy >= 0.0 && gy < 7.0 {
                            let row = bits[gy as usize];
                            let bit = (row >> (4 - gx as usize)) & 1;
                            if bit == 1 {
                                r = 1.0;
                                g = 1.0;
                                b = 1.0;
                            }
                        }
                    }
                }
            }
            px[idx] = (r * 255.0) as u8;
            px[idx + 1] = (g * 255.0) as u8;
            px[idx + 2] = (b * 255.0) as u8;
            px[idx + 3] = (alpha * 255.0) as u8;
        }
    }
    encode_rgba(SIZE, SIZE, &px)
}

fn rounded_alpha(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    let cx = x.clamp(radius, size - radius);
    let cy = y.clamp(radius, size - radius);
    let dx = x - cx;
    let dy = y - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    // 1px anti-aliasing at the border.
    (radius + 0.5 - dist).clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let hh = h / 60.0;
    let x = c * (1.0 - ((hh % 2.0) - 1.0).abs());
    let (r, g, b) = match hh as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r + m, g + m, b + m)
}

/// Nearest-neighbour resize of an RGBA8 image.
pub fn resize_rgba(
    src: &[u8],
    sw: u32,
    sh: u32,
    dw: u32,
    dh: u32,
) -> Option<Vec<u8>> {
    if sw == 0 || sh == 0 || src.len() < (sw * sh * 4) as usize {
        return None;
    }
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        let sy = (y as u64 * sh as u64 / dh as u64) as u32;
        for x in 0..dw {
            let sx = (x as u64 * sw as u64 / dw as u64) as u32;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_icon_is_valid_png() {
        let png = letter_icon("WhatsApp", "com.whatsapp");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        // Round-trip through our own inflater to check the IDAT stream.
        let idat_pos = find_chunk(&png, b"IDAT").expect("IDAT present");
        let (len, payload) = idat_pos;
        let _ = len;
        let raw = inflate::zlib_decompress(payload, 1 << 22).expect("IDAT inflates");
        assert_eq!(raw.len(), (128 * (128 * 4 + 1)) as usize);
    }

    fn find_chunk<'a>(png: &'a [u8], kind: &[u8; 4]) -> Option<(usize, &'a [u8])> {
        let mut pos = 8;
        while pos + 8 <= png.len() {
            let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]])
                as usize;
            if &png[pos + 4..pos + 8] == kind {
                return Some((pos, &png[pos + 8..pos + 8 + len]));
            }
            pos += 12 + len;
        }
        None
    }

    #[test]
    fn resize_works() {
        let src = vec![7u8; 4 * 4 * 4];
        let out = resize_rgba(&src, 4, 4, 2, 2).unwrap();
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|&b| b == 7));
    }
}
