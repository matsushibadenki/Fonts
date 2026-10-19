//! Hint bytecode is never executed here. Editing exports fresh unhinted outlines;
//! source export can preserve all tables or remove TrueType instructions explicitly.
use crate::font_io::{Result, MAX_FONT_BYTES};
use serde::Serialize;
use std::collections::BTreeMap;
use ttf_parser::{Face, Tag};
const DROP: [[u8; 4]; 9] = [
    *b"fpgm", *b"prep", *b"cvt ", *b"cvar", *b"hdmx", *b"LTSH", *b"VDMX", *b"gasp", *b"DSIG",
];
#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub family: String,
    pub extension: String,
    pub true_type: bool,
    pub instructed_glyphs: usize,
    pub instruction_bytes: usize,
    pub tables: Vec<String>,
}
fn word(data: &[u8], at: usize) -> Result<u16> {
    let b = data.get(at..at + 2).ok_or("hinting_invalid")?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
fn table<'a>(face: &Face<'a>, tag: &[u8; 4]) -> Result<&'a [u8]> {
    face.raw_face()
        .table(Tag::from_bytes(tag))
        .ok_or_else(|| "hinting_invalid".into())
}
// Returns instruction length and a copy with instructions removed. Component and
// point bytes are retained verbatim, including original overlap/transform flags.
fn strip_glyph(data: &[u8]) -> Result<(usize, Vec<u8>)> {
    if data.is_empty() {
        return Ok((0, vec![]));
    }
    if data.len() < 10 {
        return Err("hinting_invalid".into());
    }
    let contours = word(data, 0)? as i16;
    let mut out = data.to_vec();
    let at = if contours >= 0 {
        let at = 10 + contours as usize * 2;
        // Zero-contour glyphs may contain only the header (no instructionLength).
        if contours == 0 && data.len() == 10 {
            return Ok((0, out));
        }
        at
    } else {
        let mut at = 10;
        let mut instructions = false;
        loop {
            let flags = word(data, at)?;
            word(data, at + 2)?; // component glyph index
            instructions |= flags & 0x100 != 0;
            out[at..at + 2].copy_from_slice(&(flags & !0x100).to_be_bytes());
            at += 4 + if flags & 1 != 0 { 4 } else { 2 };
            let transforms = [8, 0x40, 0x80]
                .iter()
                .filter(|bit| flags & **bit != 0)
                .count();
            if transforms > 1 {
                return Err("hinting_invalid".into());
            }
            at += if flags & 8 != 0 {
                2
            } else if flags & 0x40 != 0 {
                4
            } else if flags & 0x80 != 0 {
                8
            } else {
                0
            };
            if at > data.len() {
                return Err("hinting_invalid".into());
            }
            if flags & 0x20 == 0 {
                break;
            }
        }
        if !instructions {
            return Ok((0, out));
        }
        let size = word(data, at)? as usize;
        let end = at + 2 + size;
        if end > data.len() {
            return Err("hinting_invalid".into());
        }
        out.drain(at..end);
        return Ok((size, out));
    };
    let size = word(data, at)? as usize;
    let end = at + 2 + size;
    if end > data.len() {
        return Err("hinting_invalid".into());
    }
    out[at..at + 2].fill(0);
    out.drain(at + 2..end);
    Ok((size, out))
}
fn glyphs<'a>(face: &Face<'a>) -> Result<Vec<&'a [u8]>> {
    let glyf = table(face, b"glyf")?;
    let loca = table(face, b"loca")?;
    let long = match word(table(face, b"head")?, 50)? {
        0 => false,
        1 => true,
        _ => return Err("hinting_invalid".into()),
    };
    let count = face.number_of_glyphs() as usize;
    let mut offsets = Vec::with_capacity(count + 1);
    for i in 0..=count {
        let offset = if long {
            let b = loca.get(i * 4..i * 4 + 4).ok_or("hinting_invalid")?;
            u32::from_be_bytes(b.try_into().unwrap()) as usize
        } else {
            word(loca, i * 2)? as usize * 2
        };
        if offset > glyf.len() || offsets.last().is_some_and(|n| *n > offset) {
            return Err("hinting_invalid".into());
        }
        offsets.push(offset);
    }
    Ok(offsets.windows(2).map(|n| &glyf[n[0]..n[1]]).collect())
}
fn parse(bytes: &[u8], index: u32) -> Result<Face<'_>> {
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err("font_too_large".into());
    }
    Face::parse(bytes, index).map_err(|_| "font_invalid".into())
}
pub fn info(bytes: &[u8], index: u32) -> Result<Info> {
    let face = parse(bytes, index)?;
    let true_type = face.raw_face().table(Tag::from_bytes(b"glyf")).is_some();
    if !true_type
        && face.raw_face().table(Tag::from_bytes(b"CFF ")).is_none()
        && face.raw_face().table(Tag::from_bytes(b"CFF2")).is_none()
    {
        return Err("hinting_unsupported".into());
    }
    let mut info = Info {
        family: crate::font_io::name(&face, 1),
        extension: if true_type { "ttf" } else { "otf" }.into(),
        true_type,
        instructed_glyphs: 0,
        instruction_bytes: 0,
        tables: vec![],
    };
    for tag in &DROP[..8] {
        if let Some(data) = face.raw_face().table(Tag::from_bytes(tag)) {
            info.tables.push(String::from_utf8_lossy(tag).trim().into());
            if [b"fpgm", b"prep"].contains(&tag) {
                info.instruction_bytes += data.len();
            }
        }
    }
    if true_type {
        for data in glyphs(&face)? {
            let (size, _) = strip_glyph(data)?;
            info.instruction_bytes += size;
            info.instructed_glyphs += usize::from(size != 0);
        }
    }
    Ok(info)
}
pub fn export(bytes: &[u8], index: u32, remove: bool) -> Result<Vec<u8>> {
    let metadata = info(bytes, index)?;
    if remove && !metadata.true_type {
        return Err("hinting_unsupported".into());
    }
    let face = parse(bytes, index)?;
    let mut tables = BTreeMap::new();
    for r in face.raw_face().table_records {
        let tag = r.tag.to_bytes();
        if &tag == b"DSIG" || (remove && DROP.contains(&tag)) {
            continue;
        }
        let mut data = table(&face, &tag)?.to_vec();
        if &tag == b"head" {
            data.get_mut(8..12).ok_or("hinting_invalid")?.fill(0);
        }
        if tables.insert(tag, data).is_some() {
            return Err("hinting_invalid".into());
        }
    }
    if remove {
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for data in glyphs(&face)? {
            loca.extend((glyf.len() as u32).to_be_bytes());
            glyf.extend(strip_glyph(data)?.1);
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        loca.extend((glyf.len() as u32).to_be_bytes());
        tables.insert(*b"glyf", glyf);
        tables.insert(*b"loca", loca);
        let head = tables.get_mut(b"head").ok_or("hinting_invalid")?;
        let flags = word(head, 16)? & !0x14; // instructions do not depend on ppem or alter advance
        head[16..18].copy_from_slice(&flags.to_be_bytes());
        head[50..52].copy_from_slice(&1u16.to_be_bytes());
        let maxp = tables.get_mut(b"maxp").ok_or("hinting_invalid")?;
        if maxp.len() < 32 || maxp[..4] != [0, 1, 0, 0] {
            return Err("hinting_invalid".into());
        }
        maxp[14..28].fill(0);
        maxp[14..16].copy_from_slice(&1u16.to_be_bytes()); // maxZones
    }
    let cff = face.raw_face().table(Tag::from_bytes(b"CFF ")).is_some()
        || face.raw_face().table(Tag::from_bytes(b"CFF2")).is_some();
    let out = crate::color_fonts::assemble(tables, cff)?;
    let checked = info(&out, 0)?;
    if remove
        && (checked.instruction_bytes != 0
            || checked.instructed_glyphs != 0
            || !checked.tables.is_empty())
    {
        return Err("hinting_invalid".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Contour, FontDocument, Segment};
    fn source() -> Vec<u8> {
        let mut doc = FontDocument::new("Hint Test", "AB").unwrap();
        doc.glyphs.get_mut(&1).unwrap().contours.push(Contour {
            smooth: Default::default(),
            start: [0., 0.],
            closed: true,
            segments: vec![
                Segment::Line { end: [250., 700.] },
                Segment::Line { end: [500., 0.] },
            ],
        });
        crate::font_io::export(&doc).unwrap()
    }
    fn tables(bytes: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
        let face = Face::parse(bytes, 0).unwrap();
        face.raw_face()
            .table_records
            .into_iter()
            .map(|r| {
                let tag = r.tag.to_bytes();
                let mut data = table(&face, &tag).unwrap().to_vec();
                if &tag == b"head" {
                    data[8..12].fill(0);
                }
                (tag, data)
            })
            .collect()
    }
    fn hinted() -> Vec<u8> {
        let bytes = source();
        let face = Face::parse(&bytes, 0).unwrap();
        let mut t = tables(&bytes);
        let mut glyf = vec![];
        let mut loca = vec![];
        for (i, original) in glyphs(&face).unwrap().into_iter().enumerate() {
            loca.extend((glyf.len() as u32).to_be_bytes());
            let data = if i == 0 {
                // Instructions on a zero-contour glyph, operating on phantom points.
                let mut d = vec![0; 10];
                d.extend([0, 1, 0x00]);
                d
            } else if i == 1 {
                let mut d = original.to_vec();
                let at = 10 + word(&d, 0).unwrap() as usize * 2;
                d[at..at + 2].copy_from_slice(&1u16.to_be_bytes());
                d.insert(at + 2, 0x00);
                d
            } else {
                // Composite references A; byte arguments, uniform scale, instructions.
                let mut d = vec![0xff, 0xff, 0, 0, 0, 0, 1, 0xf4, 2, 0xbc];
                d.extend([1, 0x0a, 0, 1, 0, 0, 0x40, 0, 0, 1, 0x00]);
                d
            };
            glyf.extend(data);
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        loca.extend((glyf.len() as u32).to_be_bytes());
        t.insert(*b"glyf", glyf);
        t.insert(*b"loca", loca);
        t.get_mut(b"head").unwrap()[50..52].copy_from_slice(&1u16.to_be_bytes());
        t.get_mut(b"head").unwrap()[16..18].copy_from_slice(&0x14u16.to_be_bytes());
        for tag in &DROP {
            t.insert(*tag, vec![0; 4]);
        }
        t.insert(*b"TEST", vec![1, 2, 3]);
        crate::color_fonts::assemble(t, false).unwrap()
    }
    #[test]
    fn preserve_keeps_all_payloads_and_refreshes_checksums() {
        let bytes = hinted();
        let out = export(&bytes, 0, false).unwrap();
        let mut expected = tables(&bytes);
        expected.remove(b"DSIG");
        assert_eq!(tables(&out), expected);
        assert_eq!(
            out.chunks(4).fold(0u32, |s, b| s
                .wrapping_add(u32::from_be_bytes(b.try_into().unwrap()))),
            0xB1B0AFBA
        );
        assert_eq!(info(&out, 0).unwrap().instructed_glyphs, 3);
    }
    #[test]
    fn removal_keeps_outlines_metrics_and_unrelated_tables() {
        let bytes = hinted();
        let out = export(&bytes, 0, true).unwrap();
        let metadata = info(&out, 0).unwrap();
        assert_eq!(metadata.instructed_glyphs, 0);
        assert_eq!(metadata.instruction_bytes, 0);
        assert!(metadata.tables.is_empty());
        let a = crate::font_io::import(&bytes, 0).unwrap();
        let b = crate::font_io::import(&out, 0).unwrap();
        assert_eq!(a, b);
        let t = tables(&out);
        for (tag, data) in tables(&bytes) {
            if !DROP.contains(&tag) && ![b"head", b"maxp", b"glyf", b"loca"].contains(&&tag) {
                assert_eq!(t[&tag], data);
            }
        }
        assert_eq!(word(&t[b"head"], 16).unwrap() & 0x14, 0);
        assert_eq!(word(&t[b"maxp"], 14).unwrap(), 1);
        assert!(t[b"maxp"][16..28].iter().all(|b| *b == 0));
        // Native scaler can load and render simple and composite glyphs at small sizes.
        let lib = freetype::Library::init().unwrap();
        let face = lib.new_memory_face(out.clone(), 0).unwrap();
        face.set_pixel_sizes(0, 12).unwrap();
        for c in ['A', 'B'] {
            face.load_char(c as usize, freetype::face::LoadFlag::RENDER)
                .unwrap();
            assert!(face.glyph().bitmap().width() > 0);
        }
        assert_eq!(tables(&export(&out, 0, true).unwrap()), t); // idempotent
                                                                // Editing always regenerates unhinted outlines; no original programs reappear.
        let edited = crate::font_io::export(&a).unwrap();
        assert_eq!(info(&edited, 0).unwrap().instruction_bytes, 0);
    }
    #[test]
    fn rejects_truncated_instructions_and_invalid_loca() {
        for data in [
            vec![0; 9],
            vec![0; 11],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 20],
            vec![255; 12],
        ] {
            assert!(strip_glyph(&data).is_err());
        }
        let bytes = hinted();
        let mut t = tables(&bytes);
        t.get_mut(b"loca").unwrap()[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        let bad = crate::color_fonts::assemble(t, false).unwrap();
        assert!(info(&bad, 0).is_err());
        assert!(export(&bad, 0, true).is_err());
        assert!(info(&bytes, 1).is_err());
        assert!(info(&bytes[..20], 0).is_err());
    }
    #[test]
    fn header_only_empty_glyph_and_composite_transform_variants() {
        assert_eq!(strip_glyph(&[0; 10]).unwrap().0, 0);
        for transform in [0, 8, 0x40, 0x80] {
            let mut d = vec![255, 255, 0, 0, 0, 0, 0, 0, 0, 0];
            let flags = 0x101u16 | transform;
            d.extend(flags.to_be_bytes());
            d.extend([0, 1, 0, 0, 0, 0]);
            d.resize(
                d.len()
                    + match transform {
                        8 => 2,
                        0x40 => 4,
                        0x80 => 8,
                        _ => 0,
                    },
                0,
            );
            d.extend([0, 1, 0]);
            let (size, out) = strip_glyph(&d).unwrap();
            assert_eq!(size, 1);
            assert_eq!(word(&out, 10).unwrap() & 0x100, 0);
        }
    }
    #[test]
    fn collection_extracts_selected_face_and_short_loca_is_supported() {
        let bytes = hinted();
        let mut t = tables(&bytes);
        let loca = t[b"loca"]
            .chunks(4)
            .flat_map(|b| ((u32::from_be_bytes(b.try_into().unwrap()) / 2) as u16).to_be_bytes())
            .collect();
        t.insert(*b"loca", loca);
        t.get_mut(b"head").unwrap()[50..52].fill(0);
        let mut sfnt = crate::color_fonts::assemble(t, false).unwrap();
        let count = word(&sfnt, 4).unwrap() as usize;
        for i in 0..count {
            let at = 12 + i * 16 + 8;
            let offset = u32::from_be_bytes(sfnt[at..at + 4].try_into().unwrap()) + 16;
            sfnt[at..at + 4].copy_from_slice(&offset.to_be_bytes());
        }
        let mut ttc = b"ttcf".to_vec();
        ttc.extend(0x00010000u32.to_be_bytes());
        ttc.extend(1u32.to_be_bytes());
        ttc.extend(16u32.to_be_bytes());
        ttc.extend(sfnt);
        assert_eq!(info(&ttc, 0).unwrap().instructed_glyphs, 3);
        let out = export(&ttc, 0, true).unwrap();
        assert_eq!(info(&out, 0).unwrap().instructed_glyphs, 0);
        assert_eq!(
            crate::font_io::import(&ttc, 0).unwrap(),
            crate::font_io::import(&out, 0).unwrap()
        );
        let preserved = export(&ttc, 0, false).unwrap();
        assert_eq!(info(&preserved, 0).unwrap().instructed_glyphs, 3);
    }
    #[test]
    fn multiple_components_keep_component_data_and_clear_all_instruction_flags() {
        let mut data = vec![255, 255, 0, 0, 0, 0, 0, 0, 0, 0];
        data.extend([1, 0x22, 0, 1, 0, 0]); // more components, instruction flag
        data.extend([1, 0x02, 0, 1, 0, 0]); // last component
        data.extend([0, 2, 0, 0]);
        let (size, out) = strip_glyph(&data).unwrap();
        assert_eq!(size, 2);
        assert_eq!(out.len(), 22);
        assert_eq!(word(&out, 10).unwrap(), 0x22);
        assert_eq!(word(&out, 16).unwrap(), 2);
        data.pop();
        assert!(strip_glyph(&data).is_err());
    }
    #[test]
    fn cff_preserves_and_rejects_true_type_removal() {
        let doc = FontDocument::new("CFF", "A").unwrap();
        let bytes = crate::font_formats::export(&doc, crate::font_formats::Format::Otf).unwrap();
        assert!(!info(&bytes, 0).unwrap().true_type);
        assert_eq!(tables(&export(&bytes, 0, false).unwrap()), tables(&bytes));
        assert_eq!(export(&bytes, 0, true).unwrap_err(), "hinting_unsupported");
    }
}
