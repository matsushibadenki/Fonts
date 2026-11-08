//! Admission checks before handing a single-font transformed WOFF2 to the bounded codec.
use super::*;
pub(super) fn present(bytes: &[u8]) -> Result<bool> {
    if bytes.len() < 48
        || bytes.len() > crate::font_io::MAX_FONT_BYTES as usize
        || bytes.get(..4) != Some(b"wOF2")
        || u32at(bytes, 8)? as usize != bytes.len()
        || u16at(bytes, 14)? != 0
        || bytes[28..48] != [0; 20]
    {
        return Err(bad());
    }
    let count = usize::from(u16at(bytes, 12)?);
    if count == 0 || count > 4095 {
        return Err(bad());
    }
    let mut at = 48;
    let mut stored = 0usize;
    let mut padded = 12 + 16 * count;
    let mut seen = std::collections::BTreeSet::new();
    let mut glyf = None;
    let mut loca = None;
    let mut hmtx = false;
    for _ in 0..count {
        let flags = *bytes.get(at).ok_or_else(bad)?;
        at += 1;
        let version = flags >> 6;
        let tag = if flags & 63 == 63 {
            let t = bytes
                .get(at..at + 4)
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?;
            at += 4;
            t
        } else {
            TAGS[usize::from(flags & 63)]
        };
        if !seen.insert(tag) {
            return Err(bad());
        }
        let transformed = if tag == *b"glyf" || tag == *b"loca" {
            match version {
                0 => true,
                3 => false,
                _ => return Err(bad()),
            }
        } else if tag == *b"hmtx" {
            match version {
                0 => false,
                1 => true,
                _ => return Err(bad()),
            }
        } else {
            if version != 0 {
                return Err(bad());
            }
            false
        };
        let original = read128(bytes, &mut at)?;
        let size = if transformed {
            read128(bytes, &mut at)?
        } else {
            original
        };
        if tag == *b"glyf" {
            glyf = Some(transformed);
        }
        if tag == *b"loca" {
            loca = Some(transformed);
            if transformed && size != 0 {
                return Err(bad());
            }
        }
        if tag == *b"hmtx" {
            hmtx = transformed;
        }
        stored = stored.checked_add(size).ok_or_else(bad)?;
        padded = padded
            .checked_add(original.checked_add(3).ok_or_else(bad)? & !3)
            .ok_or_else(bad)?;
        if stored > crate::font_io::MAX_FONT_BYTES as usize
            || padded > crate::font_io::MAX_FONT_BYTES as usize
        {
            return Err(bad());
        }
    }
    if glyf != loca || hmtx && glyf != Some(true) {
        return Err(bad());
    }
    let compressed = u32at(bytes, 20)? as usize;
    let end = at.checked_add(compressed).ok_or_else(bad)?;
    if padded != u32at(bytes, 16)? as usize
        || end > bytes.len()
        || (end + 3) & !3 != bytes.len()
        || bytes[end..].iter().any(|b| *b != 0)
    {
        return Err(bad());
    }
    let yes = glyf == Some(true);
    if yes && bytes.get(4..8) != Some(b"\0\x01\0\0") {
        return Err(bad());
    }
    Ok(yes)
}
pub(super) fn decode(bytes: &[u8]) -> Result<Vec<u8>> {
    let out = font_hinting_native::woff2::decode(bytes).map_err(|_| bad())?;
    if !out.len().is_multiple_of(4) || checksum(&out) != 0xb1b0afba {
        return Err(bad());
    }
    let face = ttf_parser::Face::parse(&out, 0).map_err(|_| bad())?;
    let mut previous = None;
    for record in face.raw_face().table_records {
        if previous.is_some_and(|tag| tag >= record.tag) {
            return Err(bad());
        }
        face.raw_face().table(record.tag).ok_or_else(bad)?;
        previous = Some(record.tag);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> FontDocument {
        let chars: String = (0x4e00..0x5000).filter_map(char::from_u32).collect();
        let mut d = FontDocument::new("Transform verification", &chars).unwrap();
        for (&id, g) in &mut d.glyphs {
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
        d
    }
    #[test]
    fn coupled_transforms_are_smaller_and_keep_physical_geometry_and_tables() {
        let d = document();
        let source = crate::font_io::export(&d).unwrap();
        let plain_tables = sfnt_tables(&source).unwrap();
        let output = encode_sfnt(source.clone(), 9).unwrap();
        let plain = super::super::encode_profile(source.clone(), 9, false).unwrap();
        assert!(output.len() < plain.len());
        let mut pos = 48;
        let mut versions = std::collections::BTreeMap::new();
        for _ in 0..u16at(&output, 12).unwrap() {
            let flags = output[pos];
            pos += 1;
            let tag = if flags & 63 == 63 {
                let t: [u8; 4] = output[pos..pos + 4].try_into().unwrap();
                pos += 4;
                t
            } else {
                TAGS[usize::from(flags & 63)]
            };
            read128(&output, &mut pos).unwrap();
            let version = flags >> 6;
            if (tag == *b"glyf" || tag == *b"loca") && version == 0
                || tag == *b"hmtx" && version == 1
            {
                read128(&output, &mut pos).unwrap();
            }
            versions.insert(tag, version);
        }
        assert_eq!(versions[b"glyf"], 0);
        assert_eq!(versions[b"loca"], 0);
        assert_eq!(versions[b"hmtx"], 1);
        eprintln!(
            "WOFF2 candidate comparison: {} -> {} bytes",
            plain.len(),
            output.len()
        );
        let mut rotated = d.clone();
        let c = &mut rotated.glyphs.get_mut(&1).unwrap().contours[0];
        let start = c.start;
        c.start = match c.segments.remove(0) {
            crate::Segment::Line { end } => end,
            _ => unreachable!(),
        };
        c.segments.push(crate::Segment::Line { end: start });
        let bytes = crate::font_io::export(&rotated).unwrap();
        assert!(
            !font_hinting_native::woff2::equivalent(&source, &bytes),
            "same silhouette with reordered physical points must not be accepted"
        );

        assert!(
            present(&output).unwrap(),
            "large fixture must actually exercise transforms"
        );
        let decoded = super::super::decode(&output).unwrap();
        assert!(font_hinting_native::woff2::equivalent(&source, &decoded));
        let src = ttf_parser::Face::parse(&source, 0).unwrap();
        let dst = ttf_parser::Face::parse(&decoded, 0).unwrap();
        assert_eq!(src.number_of_glyphs(), dst.number_of_glyphs());
        for (tag, data) in plain_tables {
            if ![b"head", b"glyf", b"loca"].contains(&&tag) {
                assert_eq!(
                    dst.raw_face()
                        .table(ttf_parser::Tag::from_bytes(&tag))
                        .unwrap(),
                    data
                );
            }
        }
        eprintln!(
            "Coupled WOFF2: source {} bytes, output {} bytes",
            source.len(),
            output.len()
        );
        if let Some(dir) = std::env::var_os("FONTS_WOFF2_FIXTURES") {
            std::fs::create_dir_all(&dir).unwrap();
            let root = std::path::Path::new(&dir);
            std::fs::write(root.join("coupled.woff2"), &output).unwrap();
            std::fs::write(root.join("coupled.sfnt"), &source).unwrap();
        }
        let mut corrupted = decoded.clone();
        let raw = ttf_parser::RawFace::parse(&decoded, 0).unwrap();
        let table = raw
            .table_records
            .into_iter()
            .find(|r| r.tag == ttf_parser::Tag::from_bytes(b"hmtx"))
            .unwrap();
        corrupted[table.offset as usize] ^= 1;
        assert!(!font_hinting_native::woff2::equivalent(&source, &corrupted));
        for at in [8, 12, 14, 16, 20, 28, 48] {
            let mut bytes = output.clone();
            bytes[at] ^= 255;
            assert!(super::super::decode(&bytes).is_err(), "offset {at}");
        }
    }
    #[test]
    fn nonzero_empty_bearings_are_preserved_and_unsafe_normalization_falls_back() {
        let source = crate::font_io::export(&document()).unwrap();
        let mut tables: std::collections::BTreeMap<_, _> =
            sfnt_tables(&source).unwrap().into_iter().collect();
        tables.get_mut(b"hmtx").unwrap()[2..4].copy_from_slice(&77i16.to_be_bytes());
        let source = crate::color_fonts::assemble(tables.clone(), false).unwrap();
        let encoded = font_hinting_native::woff2::encode(&source, 9).unwrap();
        let decoded = super::super::decode(&encoded).unwrap();
        assert!(
            font_hinting_native::woff2::equivalent(&source, &decoded),
            "empty glyph LSB cannot be synthesized as zero"
        );
        let count = usize::from(u16at(&tables[b"maxp"], 4).unwrap());
        let long = u16at(&tables[b"head"], 50).unwrap() == 1;
        let locations = &tables[b"loca"];
        let glyf = &tables[b"glyf"];
        let at = |i| {
            if long {
                u32at(locations, i * 4).unwrap() as usize
            } else {
                usize::from(u16at(locations, i * 2).unwrap()) * 2
            }
        };
        let mut records: Vec<_> = (0..count)
            .map(|i| glyf[at(i)..at(i + 1)].to_vec())
            .collect();
        records[0] = vec![0, 0, 0, 9, 0, 0, 0, 9, 0, 0];
        let mut data = vec![];
        let mut loca = vec![];
        for r in records {
            loca.extend((data.len() as u32).to_be_bytes());
            data.extend(r);
            if !data.len().is_multiple_of(2) {
                data.push(0);
            }
        }
        loca.extend((data.len() as u32).to_be_bytes());
        tables.insert(*b"glyf", data.clone());
        tables.insert(*b"loca", loca);
        tables.get_mut(b"head").unwrap()[50..52].copy_from_slice(&1u16.to_be_bytes());
        let source = crate::color_fonts::assemble(tables, false).unwrap();
        let web = encode_sfnt(source, 9).unwrap();
        assert!(
            !present(&web).unwrap(),
            "an unrepresentable empty bbox must retain the null profile"
        );
        let decoded = super::super::decode(&web).unwrap();
        assert_eq!(
            ttf_parser::Face::parse(&decoded, 0)
                .unwrap()
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"glyf"))
                .unwrap(),
            data
        );
    }
    #[test]
    fn variable_composite_origin_and_color_source_tables_survive_reference_transform() {
        let fixtures: &[&[u8]] = &[
            include_bytes!("../../../tests/data/variations/gvar.ttf"),
            include_bytes!("../../../tests/data/variations/composite-gvar.ttf"),
            include_bytes!("../../../tests/data/variations/moving-origins.ttf"),
            include_bytes!("../../../tests/data/variations/color-gvar.ttf"),
        ];
        for source in fixtures {
            let web = font_hinting_native::woff2::encode(source, 9).unwrap();
            assert!(present(&web).unwrap());
            let decoded = super::super::decode(&web).unwrap();
            assert!(font_hinting_native::woff2::equivalent(source, &decoded));
            crate::color_fonts::verify_source(&web, source, true).unwrap();
            let mut a = crate::source_font::master_sample::Sampler::new(
                std::sync::Arc::new(source.to_vec()),
                0,
            )
            .unwrap();
            let mut b =
                crate::source_font::master_sample::Sampler::new(std::sync::Arc::new(decoded), 0)
                    .unwrap();
            for x in [-1., 0., 0.375, 1.] {
                for y in [-1., 0., 1.] {
                    let location = [("wght".into(), x), ("wdth".into(), y)].into();
                    for glyph in [0, 1, 3] {
                        let p = a.sample_normalized(glyph, &location).unwrap().shape;
                        let q = b.sample_normalized(glyph, &location).unwrap().shape;
                        assert_eq!(p.advance, q.advance);
                        assert_eq!(
                            p.contours.iter().map(|c| c.start).collect::<Vec<_>>(),
                            q.contours.iter().map(|c| c.start).collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}
