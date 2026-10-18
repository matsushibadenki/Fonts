//! Bake supported OpenType variations into the editor's static outline model.
use crate::{font_io, FontDocument};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ttf_parser::{Face, Tag};
pub type Coordinates = BTreeMap<String, f32>;
#[derive(Serialize)]
pub struct Axis {
    pub tag: String,
    pub name: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
    pub hidden: bool,
}
pub fn axes(bytes: &[u8], index: u32) -> Result<Vec<Axis>, String> {
    let face = Face::parse(bytes, index).map_err(|_| "font_invalid")?;
    let raw = face
        .raw_face()
        .table(Tag::from_bytes(b"fvar"))
        .ok_or("variation_unsupported")?;
    let u16_at = |at: usize| {
        raw.get(at..at + 2)
            .map(|s| u16::from_be_bytes([s[0], s[1]]) as usize)
    };
    let offset = u16_at(4).ok_or("variation_unsupported")?;
    let stride = u16_at(10).ok_or("variation_unsupported")?;
    if stride != 20 {
        return Err("variation_unsupported".into());
    }
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .enumerate()
        .map(|(i, a)| Axis {
            tag: a.tag.to_string(),
            name: font_io::name(&face, a.name_id),
            min: a.min_value,
            default: a.def_value,
            max: a.max_value,
            hidden: u16_at(offset + i * stride + 16).is_some_and(|flags| flags & 1 != 0),
        })
        .collect();
    if axes.is_empty() || axes.len() >= 64 {
        return Err("variation_unsupported".into());
    }
    Ok(axes)
}
#[derive(Deserialize)]
pub struct Request {
    pub id: u64,
    pub coordinates: Coordinates,
}
pub fn instantiate(
    bytes: &[u8],
    index: u32,
    coordinates: &Coordinates,
) -> Result<FontDocument, String> {
    let raw = ttf_parser::RawFace::parse(bytes, index).map_err(|_| "font_invalid")?;
    if [b"COLR", b"sbix", b"CBDT", b"SVG "]
        .iter()
        .any(|tag| raw.table(Tag::from_bytes(tag)).is_some())
    {
        return Err("color_requires_preserve".into());
    }
    let axes = axes(bytes, index)?;
    if coordinates
        .keys()
        .any(|tag| !axes.iter().any(|a| &a.tag == tag))
    {
        return Err("variation_axis".into());
    }
    let mut face = Face::parse(bytes, index).map_err(|_| "font_invalid")?;
    for a in &axes {
        let value = coordinates.get(&a.tag).copied().unwrap_or(a.default);
        if !value.is_finite() || value < a.min || value > a.max {
            return Err("variation_range".into());
        }
        let tag: [u8; 4] = a.tag.as_bytes().try_into().map_err(|_| "variation_axis")?;
        face.set_variation(Tag::from_bytes(&tag), value)
            .ok_or("variation_unsupported")?;
    }
    // ttf-parser applies MVAR to OS/2 metrics, but not to non-zero hhea metrics.
    let hhea_variation = !face
        .tables()
        .os2
        .is_some_and(|t| t.use_typographic_metrics());
    let metric = |tag| {
        face.tables()
            .mvar
            .and_then(|m| m.metric_offset(Tag::from_bytes(tag), face.variation_coordinates()))
            .unwrap_or(0.)
    };
    let ascender = f64::from(face.ascender())
        + if hhea_variation && face.tables().hhea.ascender != 0 {
            f64::from(metric(b"hasc"))
        } else {
            0.
        };
    let descender = f64::from(face.descender())
        + if hhea_variation && face.tables().hhea.descender != 0 {
            f64::from(metric(b"hdsc"))
        } else {
            0.
        };
    let family = font_io::name(&face, 1);
    let mut doc = font_io::import_face(bytes, index, face)?;
    doc.family = family;
    doc.ascender = ascender;
    doc.descender = descender;
    let value = |tag: &str| {
        axes.iter()
            .find(|a| a.tag == tag)
            .map(|a| coordinates.get(tag).copied().unwrap_or(a.default))
    };
    if let Some(w) = value("wght") {
        doc.metadata.weight = w.round().clamp(1., 1000.) as u16;
        doc.metadata.bold = w >= 700.;
    }
    if let Some(w) = value("wdth") {
        let widths = [50., 62.5, 75., 87.5, 100., 112.5, 125., 150., 200.];
        doc.metadata.width = (widths.iter().position(|x| w <= *x).unwrap_or(8) + 1) as u16;
    }
    if let Some(s) = value("slnt") {
        doc.metadata.italic_angle = f64::from(s);
        doc.metadata.italic = s != 0.;
    }
    if let Some(i) = value("ital") {
        doc.metadata.italic |= i >= 0.5;
    }
    doc.metadata.style = axes
        .iter()
        .map(|a| {
            format!(
                "{} {}",
                a.tag,
                coordinates.get(&a.tag).copied().unwrap_or(a.default)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    // Variable positioning requires a shaping engine; never present its default deltas as baked kerning.
    doc.kerning = Default::default();
    doc.class_kerning = Default::default();
    doc.validate().map_err(|e| e.to_string())?;
    Ok(doc)
}
/// Reparse static output and check glyph repertoire, advances and outline presence.
pub fn verify(bytes: &[u8], expected: &FontDocument) -> Result<(), String> {
    let face = Face::parse(bytes, 0).map_err(|_| "variation_invalid_output")?;
    if face.is_variable()
        || face.number_of_glyphs() as usize != expected.glyphs.len()
        || [
            b"fvar", b"gvar", b"CFF2", b"HVAR", b"MVAR", b"avar", b"VVAR",
        ]
        .iter()
        .any(|tag| face.raw_face().table(Tag::from_bytes(tag)).is_some())
    {
        return Err("variation_invalid_output".into());
    }
    let actual = font_io::import(bytes, 0)?;
    if actual.units_per_em != expected.units_per_em
        || actual.ascender != expected.ascender.round()
        || actual.descender != expected.descender.round()
    {
        return Err("variation_invalid_output".into());
    }
    for (a, b) in actual.glyphs.values().zip(expected.glyphs.values()) {
        if (a.advance - b.advance.round()).abs() > 0.01
            || a.codepoints != b.codepoints
            || a.contours.is_empty() != b.contours.is_empty()
        {
            return Err("variation_invalid_output".into());
        }
    }
    Ok(())
}
