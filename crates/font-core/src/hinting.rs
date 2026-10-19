//! Hint bytecode is never executed here. Editing exports fresh unhinted outlines;
//! source export can preserve all tables or remove TrueType instructions explicitly.
use crate::font_io::{Result, MAX_FONT_BYTES};
use serde::{Deserialize, Serialize};
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
    pub can_autohint: bool,
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
    if bytes.starts_with(&[1, 0]) {
        if bytes.len() as u64 > MAX_FONT_BYTES {
            return Err("font_too_large".into());
        }
        let raw = crate::font_formats::legacy_info(bytes, index)?;
        return Ok(Info {
            family: raw.family,
            extension: "cff".into(),
            true_type: false,
            can_autohint: false,
            instructed_glyphs: 0,
            instruction_bytes: 0,
            tables: vec![],
        });
    }
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
        can_autohint: true_type
            && !face.is_variable()
            && [b"COLR", b"sbix", b"CBDT", b"SVG ", b"VARC"]
                .iter()
                .all(|tag| face.raw_face().table(Tag::from_bytes(tag)).is_none()),
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
    if bytes.starts_with(&[1, 0]) {
        if !remove {
            return Ok(bytes.to_vec());
        }
        let doc = crate::font_formats::legacy_import(bytes, index)?;
        let scaffold = crate::font_formats::export(&doc, crate::font_formats::Format::Otf)?;
        let parsed = parse(&scaffold, 0)?;
        let mut tables = BTreeMap::new();
        for r in parsed.raw_face().table_records {
            let tag = r.tag.to_bytes();
            let mut data = table(&parsed, &tag)?.to_vec();
            if &tag == b"head" {
                data[8..12].fill(0);
            }
            tables.insert(tag, data);
        }
        tables.insert(*b"CFF ", bytes.to_vec());
        let wrapped = crate::color_fonts::assemble(tables, true)?;
        let removed = font_hinting_native::dehint_cff(&wrapped, 0)?;
        let actual = crate::font_formats::legacy_import(&removed, 0)?;
        if doc != actual {
            return Err("hinting_invalid".into());
        }
        return Ok(removed);
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
    if remove && metadata.true_type {
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
    if remove && !metadata.true_type {
        let tag = if face.raw_face().table(Tag::from_bytes(b"CFF ")).is_some() {
            *b"CFF "
        } else {
            *b"CFF2"
        };
        tables.insert(tag, font_hinting_native::dehint_cff(bytes, index)?);
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
    if remove && !metadata.true_type {
        verify_geometry(bytes, index, &out, false)?;
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Preserve,
    Remove,
    Auto,
}

/// Generate actual TrueType bytecode; no rasterized contours are baked into the outline.
pub fn autohint(bytes: &[u8], index: u32) -> Result<Vec<u8>> {
    if !info(bytes, index)?.can_autohint {
        return Err("autohint_unsupported".into());
    }
    // Start from a clean standalone face, including for previously auto-hinted inputs.
    let clean = export(bytes, index, true)?;
    let native = font_hinting_native::autohint(&clean, 8, 50)?;
    verify_geometry(&clean, 0, &native, true)?;
    // Canonical SFNT padding/directory is required by the WOFF2 null-transform profile.
    let result = export(&native, 0, false)?;
    let inspected = info(&result, 0)?;
    if inspected.instructed_glyphs == 0 || inspected.instruction_bytes == 0 {
        return Err("autohint_empty".into());
    }
    Ok(result)
}
#[derive(Default, PartialEq)]
struct Trace {
    data: Vec<(u8, [f32; 6])>,
    overflow: bool,
}
impl Trace {
    fn add(&mut self, op: u8, p: [f32; 6]) {
        if self.data.len() < 1_000_000 {
            self.data.push((op, p));
        } else {
            self.overflow = true;
        }
    }
}
impl ttf_parser::OutlineBuilder for Trace {
    fn move_to(&mut self, x: f32, y: f32) {
        self.add(0, [x, y, 0., 0., 0., 0.]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.add(1, [x, y, 0., 0., 0., 0.]);
    }
    fn quad_to(&mut self, x: f32, y: f32, a: f32, b: f32) {
        self.add(2, [x, y, a, b, 0., 0.]);
    }
    fn curve_to(&mut self, x: f32, y: f32, a: f32, b: f32, c: f32, d: f32) {
        self.add(3, [x, y, a, b, c, d]);
    }
    fn close(&mut self) {
        self.add(4, [0.; 6]);
    }
}
fn verify_geometry(source: &[u8], index: u32, output: &[u8], allow_helper: bool) -> Result<()> {
    let a = parse(source, index)?;
    let b = parse(output, 0)?;
    if a.units_per_em() != b.units_per_em()
        || (!allow_helper && a.number_of_glyphs() != b.number_of_glyphs())
        || b.number_of_glyphs() < a.number_of_glyphs()
    {
        return Err("hinting_invalid".into());
    }
    let mut total = 0;
    for n in 0..a.number_of_glyphs() {
        let id = ttf_parser::GlyphId(n);
        let mut x = Trace::default();
        let mut y = Trace::default();
        let ab = a.outline_glyph(id, &mut x);
        let bb = b.outline_glyph(id, &mut y);
        if !allow_helper
            && (a.glyph_name(id) != b.glyph_name(id)
                || a.tables().cff.and_then(|c| c.glyph_cid(id))
                    != b.tables().cff.and_then(|c| c.glyph_cid(id)))
        {
            return Err("hinting_invalid".into());
        }
        total += x.data.len();
        if x.overflow
            || y.overflow
            || total > crate::font_io::MAX_OUTLINE_SEGMENTS
            || ab != bb
            || x != y
            || a.glyph_hor_advance(id) != b.glyph_hor_advance(id)
            || a.glyph_ver_advance(id) != b.glyph_ver_advance(id)
        {
            return Err("hinting_invalid".into());
        }
    }
    if let Some(cmap) = a.tables().cmap {
        let mut valid = true;
        for sub in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
            sub.codepoints(|cp| {
                if let Some(c) = char::from_u32(cp) {
                    if a.glyph_index(c) != b.glyph_index(c) {
                        valid = false;
                    }
                }
            });
        }
        if !valid {
            return Err("hinting_invalid".into());
        }
    }
    Ok(())
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
    fn generated_bytecode_preserves_geometry_and_renders_at_small_sizes() {
        let bytes = source();
        let out = autohint(&bytes, 0).unwrap();
        assert!(info(&out, 0).unwrap().instructed_glyphs > 0);
        assert!(table(&Face::parse(&out, 0).unwrap(), b"fpgm").is_ok());
        let library = freetype::Library::init().unwrap();
        let face = library.new_memory_face(out.clone(), 0).unwrap();
        for size in [8, 12, 16, 24, 50] {
            face.set_pixel_sizes(0, size).unwrap();
            face.load_char(
                'A' as usize,
                freetype::face::LoadFlag::RENDER
                    | freetype::face::LoadFlag::NO_AUTOHINT
                    | freetype::face::LoadFlag::PEDANTIC,
            )
            .unwrap();
            assert!(face.glyph().bitmap().width() > 0);
        }
        let regenerated = autohint(&out, 0).unwrap();
        assert!(info(&regenerated, 0).unwrap().instruction_bytes > 0);
    }
    #[test]
    fn cff_stems_masks_subroutines_and_private_hints_are_removed_without_other_table_changes() {
        for (bytes, tag) in [
            (
                include_bytes!("../tests/data/hinting/cff-hints.otf").as_slice(),
                *b"CFF ",
            ),
            (
                include_bytes!("../tests/data/hinting/cid-hints.otf").as_slice(),
                *b"CFF ",
            ),
            (
                include_bytes!("../tests/data/hinting/cff2-hints.otf").as_slice(),
                *b"CFF2",
            ),
        ] {
            let removed = export(bytes, 0, true).unwrap();
            let a = tables(bytes);
            let b = tables(&removed);
            assert_ne!(a[&tag], b[&tag]);
            for (t, data) in a {
                if t != tag && !DROP.contains(&t) {
                    assert_eq!(data, b[&t]);
                }
            }
            if tag == *b"CFF2" {
                for weight in [100., 400., 900.] {
                    let coords = [("wght".into(), weight)].into();
                    assert_eq!(
                        crate::variations::instantiate(bytes, 0, &coords).unwrap(),
                        crate::variations::instantiate(&removed, 0, &coords).unwrap()
                    );
                }
            }
            let lib = freetype::Library::init().unwrap();
            let f = lib.new_memory_face(removed, 0).unwrap();
            for size in [8, 12, 24] {
                f.set_pixel_sizes(0, size).unwrap();
                f.load_char('A' as usize, freetype::face::LoadFlag::RENDER)
                    .unwrap();
                assert!(f.glyph().bitmap().width() > 0);
            }
        }
        let original = include_bytes!("../tests/data/hinting/cff-hints.otf");
        let raw = table(&Face::parse(original, 0).unwrap(), b"CFF ").unwrap();
        let clean = export(raw, 0, true).unwrap();
        assert_ne!(raw, clean);
        assert_eq!(export(raw, 0, false).unwrap(), raw);
    }
    #[test]
    fn auto_rejects_variable_color_and_empty_fonts_and_woff2_retains_generated_programs() {
        for bytes in [
            include_bytes!("../tests/data/variations/gvar.ttf").as_slice(),
            include_bytes!("../tests/data/color/colrv0.ttf").as_slice(),
        ] {
            assert!(!info(bytes, 0).unwrap().can_autohint);
            assert_eq!(autohint(bytes, 0).unwrap_err(), "autohint_unsupported");
        }
        let empty = crate::font_io::export(&FontDocument::new("Empty", "").unwrap()).unwrap();
        assert!(autohint(&empty, 0).is_err());
        let hinted = autohint(&source(), 0).unwrap();
        let web = crate::font_formats::woff2::encode_sfnt(hinted.clone(), 9).unwrap();
        let decoded = crate::font_formats::woff2::decode(&web).unwrap();
        assert_eq!(
            info(&hinted, 0).unwrap().instruction_bytes,
            info(&decoded, 0).unwrap().instruction_bytes
        );
        verify_geometry(&hinted, 0, &decoded, false).unwrap();
    }
    #[test]
    fn autohint_does_not_override_font_restrictions() {
        let bytes = source();
        let mut t = tables(&bytes);
        t.get_mut(b"OS/2").unwrap()[8..10].copy_from_slice(&2u16.to_be_bytes());
        let restricted = crate::color_fonts::assemble(t, false).unwrap();
        assert_eq!(autohint(&restricted, 0).unwrap_err(), "autohint_native_15");
    }
    #[test]
    fn cff_preserves_and_removes_hints() {
        let doc = FontDocument::new("CFF", "A").unwrap();
        let bytes = crate::font_formats::export(&doc, crate::font_formats::Format::Otf).unwrap();
        assert!(!info(&bytes, 0).unwrap().true_type);
        assert_eq!(tables(&export(&bytes, 0, false).unwrap()), tables(&bytes));
        let removed = export(&bytes, 0, true).unwrap();
        verify_geometry(&bytes, 0, &removed, false).unwrap();
    }
}
