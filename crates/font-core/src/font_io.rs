//! Static outline interchange. Original layout/hinting/variation/color tables are not round-tripped.
use crate::{Contour, FontDocument, Glyph, GlyphStatus, Segment};
use std::collections::{BTreeMap, BTreeSet};
use ttf_parser::{Face, GlyphId, OutlineBuilder};
pub type Result<T> = std::result::Result<T, String>;
pub const MAX_OUTLINE_SEGMENTS: usize = 16_000_000;
pub const MAX_FONT_BYTES: u64 = 256 * 1024 * 1024;

pub fn read(path: &std::path::Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("font_invalid".into());
    }
    let mut bytes = Vec::new();
    f.take(MAX_FONT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err("font_too_large".into());
    }
    Ok(bytes)
}
pub fn name(face: &Face<'_>, id: u16) -> String {
    face.names()
        .into_iter()
        .filter(|n| n.name_id == id)
        .find_map(|n| n.to_string())
        .unwrap_or_default()
}
pub fn import(bytes: &[u8], index: u32) -> Result<FontDocument> {
    if !crate::font_formats::is_sfnt(bytes) {
        return crate::font_formats::legacy_import(bytes, index);
    }
    let face = Face::parse(bytes, index).map_err(|_| "font_invalid")?;
    if face.tables().glyf.is_none() && face.tables().cff.is_none() && face.tables().cff2.is_none() {
        return Err("font_no_outlines".into());
    }
    let mut doc =
        FontDocument::new(format!("{} Edited", name(&face, 1)), "").map_err(|e| e.to_string())?;
    doc.metadata.style = name(&face, 2);
    doc.metadata.weight = face.weight().to_number();
    doc.metadata.width = face.width().to_number();
    doc.metadata.italic_angle = f64::from(face.italic_angle());
    doc.metadata.bold = face.is_bold();
    doc.metadata.italic = face.is_italic();
    doc.metadata.copyright = name(&face, 0);
    doc.metadata.license = name(&face, 13);
    doc.metadata.license_url = name(&face, 14);
    doc.metadata.cid_system = crate::font_formats::cid_system(bytes, index);
    doc.units_per_em = face.units_per_em();
    doc.ascender = f64::from(face.ascender());
    doc.descender = f64::from(face.descender());
    let mut mapping: BTreeMap<u16, BTreeSet<char>> = BTreeMap::new();
    if let Some(cmap) = face.tables().cmap {
        let mut seen = BTreeSet::new();
        for sub in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
            sub.codepoints(|n| {
                if let Some(c) = char::from_u32(n) {
                    if seen.insert(c) {
                        if let Some(id) = face.glyph_index(c) {
                            if id.0 != 0 {
                                mapping.entry(id.0).or_default().insert(c);
                            }
                        }
                    }
                }
            });
        }
    }
    let mut names = BTreeSet::new();
    let mut total = 0;
    for id in 0..face.number_of_glyphs() {
        let mut builder = Outline::default();
        let bounds = face.outline_glyph(GlyphId(id), &mut builder);
        if builder.failed || (bounds.is_none() && !builder.contours.is_empty()) {
            return Err("font_invalid_outline".into());
        }
        total += builder.points;
        if total > MAX_OUTLINE_SEGMENTS {
            return Err("font_too_large".into());
        }
        let codepoints = mapping.remove(&id).unwrap_or_default();
        let base = if id == 0 {
            ".notdef".into()
        } else {
            face.glyph_name(GlyphId(id))
                .filter(|n| !n.is_empty() && n.len() <= 220)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("glyph{id}"))
        };
        let mut glyph_name = base.clone();
        let mut suffix = 0;
        while !names.insert(glyph_name.clone()) {
            suffix += 1;
            glyph_name = format!("{base}.{suffix}");
        }
        doc.glyphs.insert(
            u32::from(id),
            Glyph {
                id: u32::from(id),
                name: glyph_name,
                codepoints,
                cid: face
                    .tables()
                    .cff
                    .and_then(|cff| cff.glyph_cid(GlyphId(id)))
                    .map(u32::from),
                advance: f64::from(
                    face.glyph_hor_advance(GlyphId(id))
                        .unwrap_or(face.units_per_em()),
                ),
                contours: builder.contours,
                status: GlyphStatus::Draft,
            },
        );
    }
    doc.validate().map_err(|e| e.to_string())?;
    Ok(doc)
}
#[derive(Default)]
struct Outline {
    contours: Vec<Contour>,
    points: usize,
    failed: bool,
}
impl Outline {
    fn segment(&mut self, s: Segment) {
        self.points += 1;
        if self.points > 65536 {
            self.failed = true;
            return;
        }
        if let Some(c) = self.contours.last_mut() {
            c.segments.push(s);
        } else {
            self.failed = true;
        }
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.contours.len() >= 4096 {
            self.failed = true;
            return;
        }
        self.contours.push(Contour {
            smooth: Default::default(),
            start: [x.into(), y.into()],
            segments: vec![],
            closed: false,
        });
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.segment(Segment::Line {
            end: [x.into(), y.into()],
        });
    }
    fn quad_to(&mut self, x: f32, y: f32, ex: f32, ey: f32) {
        self.segment(Segment::Quadratic {
            control: [x.into(), y.into()],
            end: [ex.into(), ey.into()],
        });
    }
    fn curve_to(&mut self, x: f32, y: f32, x2: f32, y2: f32, ex: f32, ey: f32) {
        self.segment(Segment::Cubic {
            control1: [x.into(), y.into()],
            control2: [x2.into(), y2.into()],
            end: [ex.into(), ey.into()],
        });
    }
    fn close(&mut self) {
        if let Some(c) = self.contours.last_mut() {
            c.closed = true;
        }
    }
}

fn u16be(v: &mut Vec<u8>, n: u16) {
    v.extend(n.to_be_bytes());
}
fn i16be(v: &mut Vec<u8>, n: i16) {
    v.extend(n.to_be_bytes());
}
fn u32be(v: &mut Vec<u8>, n: u32) {
    v.extend(n.to_be_bytes());
}
fn set16(v: &mut [u8], at: usize, n: u16) {
    v[at..at + 2].copy_from_slice(&n.to_be_bytes());
}
fn set32(v: &mut [u8], at: usize, n: u32) {
    v[at..at + 4].copy_from_slice(&n.to_be_bytes());
}
fn coord(n: f64) -> Result<i16> {
    if !n.is_finite() || !(-32768. ..=32767.).contains(&n.round()) {
        Err("font_coordinate_range".into())
    } else {
        Ok(n.round() as i16)
    }
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, b| {
        let mut x = [0; 4];
        x[..b.len()].copy_from_slice(b);
        sum.wrapping_add(u32::from_be_bytes(x))
    })
}

// Cubic -> quadratic by adaptive subdivision, <=0.5 design unit pre-rounding error.
fn cubic(points: &mut Vec<([i16; 2], bool)>, p: [[f64; 2]; 4], depth: u8) -> Result<()> {
    let q1 = std::array::from_fn::<_, 2, _>(|i| (3. * p[1][i] - p[0][i]) / 2.);
    let q2 = std::array::from_fn::<_, 2, _>(|i| (3. * p[2][i] - p[3][i]) / 2.);
    let err = (q1[0] - q2[0]).hypot(q1[1] - q2[1]);
    if err <= 1. {
        points.push((
            [coord((q1[0] + q2[0]) / 2.)?, coord((q1[1] + q2[1]) / 2.)?],
            false,
        ));
        points.push(([coord(p[3][0])?, coord(p[3][1])?], true));
    } else {
        if depth >= 16 {
            return Err("font_curve_complexity".into());
        }
        let mid = |a: [f64; 2], b: [f64; 2]| std::array::from_fn(|i| (a[i] + b[i]) / 2.);
        let a = mid(p[0], p[1]);
        let b = mid(p[1], p[2]);
        let c = mid(p[2], p[3]);
        let d = mid(a, b);
        let e = mid(b, c);
        let f = mid(d, e);
        cubic(points, [p[0], a, d, f], depth + 1)?;
        cubic(points, [f, e, c, p[3]], depth + 1)?;
    }
    if points.len() > 65535 {
        return Err("font_curve_complexity".into());
    }
    Ok(())
}
/// Export a new unhinted static TrueType font. Reject open contours and out-of-range coordinates.
pub fn export(doc: &FontDocument) -> Result<Vec<u8>> {
    doc.validate().map_err(|e| e.to_string())?;
    let count = doc.glyphs.len() as u16;
    let mut glyf = vec![];
    let mut loca = vec![];
    let mut hmtx = vec![];
    let mut bbox = [0i16; 4];
    let mut max_points = 0u16;
    let mut max_contours = 0u16;
    let mut max_advance = 0u16;
    let mut min_lsb = 0i16;
    let mut min_rsb = 0i16;
    let mut max_extent = 0i16;
    let mut mappings = BTreeMap::new();
    for (index, g) in doc.glyphs.values().enumerate() {
        u32be(&mut loca, glyf.len() as u32);
        for c in &g.codepoints {
            mappings.insert(*c as u32, index as u16);
        }
        let mut points = vec![];
        let mut ends = vec![];
        for c in &g.contours {
            if !c.closed {
                return Err("font_open_contour".into());
            }
            if c.segments.is_empty() {
                return Err("font_invalid_outline".into());
            }
            points.push(([coord(c.start[0])?, coord(c.start[1])?], true));
            let mut at = c.start;
            for s in &c.segments {
                let end = match *s {
                    Segment::Line { end } => {
                        points.push(([coord(end[0])?, coord(end[1])?], true));
                        end
                    }
                    Segment::Quadratic { control, end } => {
                        points.push(([coord(control[0])?, coord(control[1])?], false));
                        points.push(([coord(end[0])?, coord(end[1])?], true));
                        end
                    }
                    Segment::Cubic {
                        control1,
                        control2,
                        end,
                    } => {
                        cubic(&mut points, [at, control1, control2, end], 0)?;
                        end
                    }
                };
                at = end;
                if points.len() > 65535 {
                    return Err("font_curve_complexity".into());
                }
            }
            ends.push((points.len() - 1) as u16);
        }
        max_points = max_points.max(points.len() as u16);
        max_contours = max_contours.max(ends.len() as u16);
        let mut b = [0i16; 4];
        if let Some((first, _)) = points.first() {
            b = [first[0], first[1], first[0], first[1]];
            for (p, _) in &points {
                b[0] = b[0].min(p[0]);
                b[1] = b[1].min(p[1]);
                b[2] = b[2].max(p[0]);
                b[3] = b[3].max(p[1]);
            }
        }
        bbox = [
            bbox[0].min(b[0]),
            bbox[1].min(b[1]),
            bbox[2].max(b[2]),
            bbox[3].max(b[3]),
        ];
        let advance = g.advance.round() as u16;
        max_advance = max_advance.max(advance);
        min_lsb = min_lsb.min(b[0]);
        max_extent = max_extent.max(b[2]);
        min_rsb = min_rsb.min(coord(f64::from(advance) - f64::from(b[2]))?);
        u16be(&mut hmtx, advance);
        i16be(&mut hmtx, b[0]);
        if !points.is_empty() {
            i16be(&mut glyf, ends.len() as i16);
            for n in b {
                i16be(&mut glyf, n);
            }
            for n in ends {
                u16be(&mut glyf, n);
            }
            u16be(&mut glyf, 0);
            glyf.extend(points.iter().map(|(_, on)| u8::from(*on)));
            for axis in 0..2 {
                let mut prev = 0i16;
                for (p, _) in &points {
                    i16be(&mut glyf, p[axis].wrapping_sub(prev));
                    prev = p[axis];
                }
            }
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
    }
    u32be(&mut loca, glyf.len() as u32);
    let mut head = vec![0; 54];
    set32(&mut head, 0, 0x10000);
    set32(&mut head, 4, 0x10000);
    set32(&mut head, 12, 0x5F0F3CF5);
    set16(&mut head, 16, 3);
    set16(&mut head, 18, doc.units_per_em);
    // 2026-01-01 in the OpenType 1904 epoch; deterministic builds.
    head[20..28].copy_from_slice(&3_850_080_000u64.to_be_bytes());
    head[28..36].copy_from_slice(&3_850_080_000u64.to_be_bytes());
    for (i, n) in bbox.iter().enumerate() {
        set16(&mut head, 36 + i * 2, *n as u16);
    }
    set16(
        &mut head,
        44,
        u16::from(doc.metadata.bold) | (u16::from(doc.metadata.italic) * 2),
    );
    set16(&mut head, 46, 8);
    set16(&mut head, 48, 2);
    set16(&mut head, 50, 1);
    let asc = coord(doc.ascender)?;
    let desc = coord(doc.descender)?;
    let mut hhea = vec![0; 36];
    set32(&mut hhea, 0, 0x10000);
    set16(&mut hhea, 4, asc as u16);
    set16(&mut hhea, 6, desc as u16);
    set16(&mut hhea, 10, max_advance);
    set16(&mut hhea, 12, min_lsb as u16);
    set16(&mut hhea, 14, min_rsb as u16);
    set16(&mut hhea, 16, max_extent as u16);
    set16(&mut hhea, 18, 1);
    set16(&mut hhea, 34, count);
    let mut maxp = vec![0; 32];
    set32(&mut maxp, 0, 0x10000);
    set16(&mut maxp, 4, count);
    set16(&mut maxp, 6, max_points);
    set16(&mut maxp, 8, max_contours);
    set16(&mut maxp, 14, 1);
    let mut cmap = vec![];
    u16be(&mut cmap, 0);
    u16be(&mut cmap, 3);
    // Unicode full repertoire and Windows full repertoire use format 12; BMP compatibility uses format 4.
    for (platform, encoding, offset) in [
        (0u16, 4u16, 28u32),
        (3, 10, 28),
        (3, 1, 44 + mappings.len() as u32 * 12),
    ] {
        u16be(&mut cmap, platform);
        u16be(&mut cmap, encoding);
        u32be(&mut cmap, offset);
    }
    u16be(&mut cmap, 12);
    u16be(&mut cmap, 0);
    u32be(&mut cmap, 16 + mappings.len() as u32 * 12);
    u32be(&mut cmap, 0);
    u32be(&mut cmap, mappings.len() as u32);
    for (c, g) in &mappings {
        u32be(&mut cmap, *c);
        u32be(&mut cmap, *c);
        u32be(&mut cmap, u32::from(*g));
    }
    cmap.extend(bmp_cmap(&mappings));
    let ps: String = doc
        .family
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(50)
        .collect();
    let ps = if ps.is_empty() {
        "FontsExport".into()
    } else {
        ps
    };
    let style = if doc.metadata.style.is_empty() {
        "Regular"
    } else {
        &doc.metadata.style
    };
    let ps_style: String = style
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(40)
        .collect();
    let strings = [
        (0, doc.metadata.copyright.clone()),
        (1, doc.family.clone()),
        (2, style.into()),
        (3, format!("Fonts:{ps}:1.0")),
        (4, format!("{} {style}", doc.family)),
        (5, "Version 1.000".into()),
        (6, format!("{ps}-{ps_style}")),
        (13, doc.metadata.license.clone()),
        (14, doc.metadata.license_url.clone()),
    ];
    let mut name_table = vec![];
    let mut name_data = vec![];
    u16be(&mut name_table, 0);
    u16be(&mut name_table, strings.len() as u16);
    u16be(&mut name_table, (6 + strings.len() * 12) as u16);
    for (id, s) in strings {
        let data: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
        if name_data.len() + data.len() > 65535 {
            return Err("font_metadata_too_large".into());
        }
        for n in [3, 1, 0x409, id, data.len() as u16, name_data.len() as u16] {
            u16be(&mut name_table, n);
        }
        name_data.extend(data);
    }
    name_table.extend(name_data);
    let mut os2 = vec![0; 96];
    set16(&mut os2, 0, 4);
    set16(&mut os2, 2, (u32::from(max_advance) / 2) as u16);
    set16(
        &mut os2,
        4,
        if doc.metadata.weight == 0 {
            400
        } else {
            doc.metadata.weight
        },
    );
    set16(
        &mut os2,
        6,
        if doc.metadata.width == 0 {
            5
        } else {
            doc.metadata.width
        },
    );
    set16(&mut os2, 10, 650);
    set16(&mut os2, 12, 600);
    set16(&mut os2, 20, 650);
    set16(&mut os2, 22, 600);
    set16(&mut os2, 26, 350);
    set16(&mut os2, 28, 50);
    set16(&mut os2, 30, 250);
    os2[58..62].copy_from_slice(b"FNTS");
    set16(
        &mut os2,
        62,
        if !doc.metadata.bold && !doc.metadata.italic {
            0x40
        } else {
            u16::from(doc.metadata.italic) | (u16::from(doc.metadata.bold) * 32)
        },
    );
    set16(
        &mut os2,
        64,
        mappings.keys().next().copied().unwrap_or(0).min(65535) as u16,
    );
    set16(
        &mut os2,
        66,
        mappings.keys().next_back().copied().unwrap_or(0).min(65535) as u16,
    );
    set16(&mut os2, 68, asc as u16);
    set16(&mut os2, 70, desc as u16);
    set16(&mut os2, 74, bbox[3].max(asc).max(0) as u16);
    set16(&mut os2, 76, bbox[1].min(desc).unsigned_abs());
    set16(&mut os2, 86, (doc.units_per_em / 2).max(1));
    set16(
        &mut os2,
        88,
        (u32::from(doc.units_per_em) * 7 / 10).max(1) as u16,
    );
    set16(&mut os2, 94, 1);
    if mappings.keys().any(|c| *c > 0xFFFF) {
        set32(&mut os2, 46, 1 << 25);
    }
    let mut post = vec![0; 32];
    set32(&mut post, 0, 0x30000);
    set32(
        &mut post,
        4,
        (doc.metadata.italic_angle * 65536.).round() as i32 as u32,
    );
    set16(&mut post, 8, (-100i16) as u16);
    set16(&mut post, 10, 50);
    let tables: BTreeMap<[u8; 4], Vec<u8>> = [
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"hmtx", hmtx),
        (*b"glyf", glyf),
        (*b"loca", loca),
        (*b"cmap", cmap),
        (*b"name", name_table),
        (*b"OS/2", os2),
        (*b"post", post),
    ]
    .into();
    let mut output = vec![];
    u32be(&mut output, 0x10000);
    u16be(&mut output, 10);
    u16be(&mut output, 128);
    u16be(&mut output, 3);
    u16be(&mut output, 32);
    let mut offset = 12 + 16 * tables.len();
    let mut data = vec![];
    let mut head_offset = 0;
    for (tag, bytes) in tables {
        output.extend(tag);
        u32be(&mut output, checksum(&bytes));
        u32be(&mut output, offset as u32);
        u32be(&mut output, bytes.len() as u32);
        if tag == *b"head" {
            head_offset = offset;
        }
        data.extend(&bytes);
        while data.len() % 4 != 0 {
            data.push(0);
        }
        offset = 172 + data.len();
    }
    output.extend(data);
    let sum = checksum(&output);
    set32(
        &mut output,
        head_offset + 8,
        0xB1B0AFBAu32.wrapping_sub(sum),
    );
    Face::parse(&output, 0).map_err(|_| "font_invalid_output")?;
    Ok(output)
}

/// Compact BMP runs: consecutive glyph IDs use idDelta; other contiguous
/// characters share a glyphIdArray. Format 12 always retains the full repertoire.
fn bmp_cmap(mappings: &BTreeMap<u32, u16>) -> Vec<u8> {
    let mut runs: Vec<(u16, u16, Vec<u16>)> = Vec::new();
    for (&c, &g) in mappings.range(..0xFFFF) {
        let c = c as u16;
        if let Some((_, end, glyphs)) = runs.last_mut() {
            if end.checked_add(1) == Some(c) {
                *end = c;
                glyphs.push(g);
                continue;
            }
        }
        runs.push((c, c, vec![g]));
    }
    let mut arrays = Vec::new();
    let mut segments = Vec::new();
    for (start, end, glyphs) in runs {
        let delta = glyphs[0].wrapping_sub(start);
        if glyphs
            .iter()
            .enumerate()
            .all(|(i, g)| *g == start.wrapping_add(i as u16).wrapping_add(delta))
        {
            segments.push((start, end, delta, None));
        } else {
            segments.push((start, end, 0, Some(arrays.len())));
            arrays.extend(glyphs);
        }
    }
    // A pathological sparse repertoire may exceed format 4's 16-bit byte length.
    // Keep a valid sentinel table in that case; format 12 is complete.
    if 16 + (segments.len() + 1) * 8 + arrays.len() * 2 > u16::MAX as usize {
        segments.clear();
        arrays.clear();
    }
    segments.push((0xFFFF, 0xFFFF, 1, None));
    let n = segments.len();
    let power = 1usize << n.ilog2();
    let mut out = Vec::new();
    for v in [
        4,
        (16 + n * 8 + arrays.len() * 2) as u16,
        0,
        (n * 2) as u16,
        (power * 2) as u16,
        n.ilog2() as u16,
        ((n - power) * 2) as u16,
    ] {
        u16be(&mut out, v);
    }
    for s in &segments {
        u16be(&mut out, s.1);
    }
    u16be(&mut out, 0);
    for s in &segments {
        u16be(&mut out, s.0);
    }
    for s in &segments {
        u16be(&mut out, s.2);
    }
    for (i, s) in segments.iter().enumerate() {
        u16be(
            &mut out,
            s.3.map(|offset| ((n - i + offset) * 2) as u16).unwrap_or(0),
        );
    }
    for g in arrays {
        u16be(&mut out, g);
    }
    out
}
