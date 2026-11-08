//! Compare bounded Brotli candidates without modifying tables or increasing payload size.
use super::*;
use brotli::enc::{backward_references::BrotliEncoderMode, BrotliEncoderParams};

fn baseline(stream: &[u8], quality: u32) -> Result<Vec<u8>> {
    let mut compressed = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 65536, quality, 22);
        writer.write_all(stream).map_err(|_| bad())?;
    }
    Ok(compressed)
}
/// A discarded candidate must not allocate more output than the known-good baseline.
struct Candidate {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Candidate {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("candidate exceeds baseline"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn compress(stream: &[u8], quality: u32) -> Result<Vec<u8>> {
    let mut best = baseline(stream, quality)?;
    // Standard Brotli window only (no large-window extension). Include the window gap.
    let lgwin =
        (usize::BITS - (stream.len().saturating_add(15)).leading_zeros()).clamp(10, 22) as i32;
    for (mode, q9_5) in [
        (BrotliEncoderMode::BROTLI_MODE_GENERIC, false),
        (BrotliEncoderMode::BROTLI_MODE_FONT, false),
        (BrotliEncoderMode::BROTLI_MODE_GENERIC, true),
    ] {
        if q9_5 && quality != 9 {
            continue;
        }
        let limit = best.len().saturating_sub(1);
        let mut candidate = Candidate {
            bytes: Vec::with_capacity(limit),
            limit,
        };
        let params = BrotliEncoderParams {
            mode,
            quality: quality as i32,
            lgwin,
            size_hint: stream.len(),
            q9_5,
            ..Default::default()
        };
        let mut input = stream;
        // Optimization is optional. Failed or over-budget candidates retain the best output.
        if brotli::BrotliCompress(&mut input, &mut candidate, &params).is_ok() {
            best = candidate.bytes;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_font_tables_never_grow_and_decode_exactly_at_every_quality() {
        let fonts: &[&[u8]] = &[
            include_bytes!("../../../tests/data/variations/gvar.ttf"),
            include_bytes!("../../../tests/data/variations/nested-blend-cff2.otf"),
            include_bytes!("../../../tests/data/variations/color-gvar.ttf"),
        ];
        let mut improved = 0;
        for font in fonts {
            let stream: Vec<_> = sfnt_tables(font)
                .unwrap()
                .into_iter()
                .flat_map(|(_, data)| data)
                .collect();
            for quality in [1, 5, 9, 11] {
                let before = baseline(&stream, quality).unwrap();
                let after = compress(&stream, quality).unwrap();
                assert!(after.len() <= before.len());
                if after.len() < before.len() {
                    improved += 1;
                }
                let mut decoded = vec![];
                brotli::Decompressor::new(after.as_slice(), 65536)
                    .read_to_end(&mut decoded)
                    .unwrap();
                assert_eq!(decoded, stream);
                eprintln!(
                    "WOFF2 payload q{quality}: {} -> {} bytes",
                    before.len(),
                    after.len()
                );
            }
        }
        assert!(
            improved > 0,
            "At least one representative font must benefit"
        );
    }
    #[test]
    fn empty_and_incompressible_payloads_fall_back_without_growth() {
        for stream in [
            vec![],
            (0..100000u32)
                .map(|n| n.wrapping_mul(2654435761).rotate_left(n % 31) as u8)
                .collect(),
        ] {
            let before = baseline(&stream, 5).unwrap();
            let after = compress(&stream, 5).unwrap();
            assert!(after.len() <= before.len());
            let mut decoded = vec![];
            brotli::Decompressor::new(after.as_slice(), 65536)
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, stream);
        }
    }
}

#[cfg(test)]
mod large_test {
    use super::*;
    #[test]
    fn generated_multiglyph_font_exports_with_identical_tables_and_smaller_payload() {
        let chars: String = (0x4e00..0x5000).filter_map(char::from_u32).collect();
        let mut doc = FontDocument::new("Compression", &chars).unwrap();
        for (&id, g) in &mut doc.glyphs {
            if id == 0 {
                continue;
            }
            let x = f64::from(id * 37 % 701);
            let y = f64::from(id * 53 % 349);
            g.contours = vec![crate::Contour {
                identity: Default::default(),
                smooth: Default::default(),
                closed: true,
                start: [x, y],
                segments: vec![
                    crate::Segment::Line { end: [x + 100., y] },
                    crate::Segment::Line {
                        end: [x + 100., y + 500.],
                    },
                    crate::Segment::Line { end: [x, y + 500.] },
                ],
            }];
        }
        let font = export(&doc, Options::default()).unwrap();
        let sfnt = decode(&font).unwrap();
        let stream: Vec<_> = sfnt_tables(&sfnt)
            .unwrap()
            .into_iter()
            .flat_map(|(_, v)| v)
            .collect();
        let before = baseline(&stream, 9).unwrap();
        let after = compress(&stream, 9).unwrap();
        assert!(after.len() <= before.len());
        eprintln!(
            "WOFF2 512 glyph payload: {} -> {} bytes",
            before.len(),
            after.len()
        );
        if let Some(dir) = std::env::var_os("FONTS_WOFF2_FIXTURES") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(std::path::Path::new(&dir).join("optimized.woff2"), font).unwrap();
            std::fs::write(std::path::Path::new(&dir).join("optimized.sfnt"), sfnt).unwrap();
        }
    }
}
