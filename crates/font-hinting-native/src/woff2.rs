//! Bounded ownership bridge. Native transforms use the same Rust Brotli implementation.
use std::{ffi::c_int, io::Read};
const LIMIT: usize = 256 * 1024 * 1024;
extern "C" {
    fn fonts_woff2_encode(
        data: *const u8,
        len: usize,
        quality: c_int,
        out: *mut *mut u8,
        size: *mut usize,
    ) -> c_int;
    fn fonts_woff2_decode(
        data: *const u8,
        len: usize,
        out: *mut *mut u8,
        size: *mut usize,
    ) -> c_int;
    fn fonts_woff2_equivalent(a: *const u8, an: usize, b: *const u8, bn: usize) -> c_int;
}
pub fn encode(bytes: &[u8], quality: u32) -> Result<Vec<u8>, String> {
    if bytes.len() > LIMIT || !(1..=11).contains(&quality) {
        return Err("woff2_invalid".into());
    }
    super::process(bytes, |out, size| unsafe {
        fonts_woff2_encode(bytes.as_ptr(), bytes.len(), quality as c_int, out, size)
    })
}
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !bounded_header(bytes) {
        return Err("woff2_invalid".into());
    }
    super::process(bytes, |out, size| unsafe {
        fonts_woff2_decode(bytes.as_ptr(), bytes.len(), out, size)
    })
}
pub fn equivalent(a: &[u8], b: &[u8]) -> bool {
    if a.len() > LIMIT || b.len() > LIMIT {
        return false;
    }
    unsafe { fonts_woff2_equivalent(a.as_ptr(), a.len(), b.as_ptr(), b.len()) == 1 }
}
// The reference decoder allocates its transformed stream before calling Brotli.
// Check both declared output and stream sizes at this public native boundary too.
fn bounded_header(b: &[u8]) -> bool {
    if b.len() < 48 || b.len() > LIMIT || b.get(..8) != Some(b"wOF2\0\x01\0\0") {
        return false;
    }
    let word = |p| u16::from_be_bytes([b[p], b[p + 1]]);
    let long = |p| u32::from_be_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]]) as usize;
    let count = usize::from(word(12));
    if count == 0
        || count > 4095
        || long(8) != b.len()
        || long(16) > LIMIT
        || word(14) != 0
        || b[28..48] != [0; 20]
    {
        return false;
    }
    fn base(b: &[u8], at: &mut usize) -> Option<usize> {
        let mut n = 0u32;
        for i in 0..5 {
            let v = *b.get(*at)?;
            *at += 1;
            if (i == 0 && v == 128) || n & 0xfe000000 != 0 {
                return None;
            }
            n = (n << 7) | u32::from(v & 127);
            if v & 128 == 0 {
                return Some(n as usize);
            }
        }
        None
    }
    let mut at = 48;
    let mut total = 0usize;
    for _ in 0..count {
        let Some(&flags) = b.get(at) else {
            return false;
        };
        at += 1;
        let tag = if flags & 63 == 63 {
            let Some(t) = b.get(at..at + 4) else {
                return false;
            };
            at += 4;
            t
        } else {
            match flags & 63 {
                10 => b"glyf",
                11 => b"loca",
                3 => b"hmtx",
                _ => b"????",
            }
        };
        let version = flags >> 6;
        let transformed = if tag == b"glyf" || tag == b"loca" {
            match version {
                0 => true,
                3 => false,
                _ => return false,
            }
        } else if tag == b"hmtx" {
            match version {
                0 => false,
                1 => true,
                _ => return false,
            }
        } else {
            if version != 0 {
                return false;
            }
            false
        };
        let Some(original) = base(b, &mut at) else {
            return false;
        };
        if original > LIMIT {
            return false;
        }
        let stored = if transformed {
            let Some(n) = base(b, &mut at) else {
                return false;
            };
            n
        } else {
            original
        };
        let Some(next) = total.checked_add(stored) else {
            return false;
        };
        total = next;
        if total > LIMIT {
            return false;
        }
    }
    at.checked_add(long(20)).is_some_and(|end| end <= b.len())
}
// SAFETY: Called only by the bundled codec with live buffers and writable length fields.
#[no_mangle]
unsafe extern "C" fn fonts_woff2_brotli_encode(
    quality: c_int,
    window: c_int,
    mode: c_int,
    n: usize,
    input: *const u8,
    size: *mut usize,
    output: *mut u8,
) -> c_int {
    std::panic::catch_unwind(|| {
        if n > LIMIT
            || input.is_null()
            || size.is_null()
            || output.is_null()
            || *size > LIMIT
            || !(0..=11).contains(&quality)
            || !(10..=24).contains(&window)
        {
            return 0;
        }
        let data = std::slice::from_raw_parts(input, n);
        let mut cursor = std::io::Cursor::new(std::slice::from_raw_parts_mut(output, *size));
        let mut reader = data;
        use brotli::enc::{backward_references::BrotliEncoderMode::*, BrotliEncoderParams};
        let params = BrotliEncoderParams {
            quality,
            lgwin: window,
            size_hint: n,
            mode: if mode == 2 {
                BROTLI_MODE_FONT
            } else if mode == 1 {
                BROTLI_MODE_TEXT
            } else {
                BROTLI_MODE_GENERIC
            },
            ..Default::default()
        };
        if brotli::BrotliCompress(&mut reader, &mut cursor, &params).is_err() {
            return 0;
        }
        *size = cursor.position() as usize;
        1
    })
    .unwrap_or(0)
}
#[no_mangle]
unsafe extern "C" fn fonts_woff2_brotli_decode(
    n: usize,
    input: *const u8,
    size: *mut usize,
    output: *mut u8,
) -> c_int {
    std::panic::catch_unwind(|| {
        if n > LIMIT || input.is_null() || size.is_null() || output.is_null() || *size > LIMIT {
            return 0;
        }
        let data = std::slice::from_raw_parts(input, n);
        let dst = std::slice::from_raw_parts_mut(output, *size);
        let mut reader = brotli::Decompressor::new(data, 65536);
        let mut at = 0;
        loop {
            match reader.read(&mut dst[at..]) {
                Ok(0) => break,
                Ok(len) => at += len,
                Err(_) => return 0,
            }
        }
        let mut extra = [0];
        if reader.read(&mut extra).ok() != Some(0) {
            return 0;
        }
        *size = at;
        1 // BROTLI_DECODER_RESULT_SUCCESS
    })
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_decoder_rejects_claimed_stream_bombs_before_native_allocation() {
        let mut bytes = vec![0; 60];
        bytes[..8].copy_from_slice(b"wOF2\0\x01\0\0");
        bytes[8..12].copy_from_slice(&60u32.to_be_bytes());
        bytes[12..14].copy_from_slice(&1u16.to_be_bytes());
        bytes[16..20].copy_from_slice(&64u32.to_be_bytes());
        // glyf transform: small original table, huge compressed-stream expansion claim.
        bytes[48] = 10;
        bytes[49] = 1;
        bytes[50..55].copy_from_slice(&[0x8f, 0xff, 0xff, 0xff, 0x7f]);
        assert!(!bounded_header(&bytes));
        assert!(decode(&bytes).is_err());
        assert!(encode(&[], 9).is_err());
        assert!(!equivalent(&[], &[]));
    }
}
