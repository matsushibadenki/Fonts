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
/// Named fvar settings, in user coordinates (before avar mapping).
#[derive(Clone, Serialize)]
pub struct Instance {
    pub id: usize,
    pub name: String,
    pub names: BTreeMap<String, String>,
    pub coordinates: Coordinates,
}
#[derive(Serialize)]
pub struct Catalog {
    pub axes: Vec<Axis>,
    pub instances: Vec<Instance>,
}
fn instance_name(face: &Face<'_>, id: u16) -> (String, BTreeMap<String, String>) {
    let records: Vec<_> = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == id)
        .filter_map(|n| {
            n.to_string().filter(|s| !s.trim().is_empty()).map(|s| {
                (
                    n.platform_id,
                    n.language_id,
                    s.chars().take(256).collect::<String>(),
                )
            })
        })
        .collect();
    let en = records
        .iter()
        .find(|(p, l, _)| *p == ttf_parser::PlatformId::Windows && *l == 0x409)
        .or_else(|| {
            records
                .iter()
                .find(|(p, l, _)| *p == ttf_parser::PlatformId::Windows && *l & 0x3ff == 9)
        })
        .or_else(|| records.first())
        .map(|(_, _, s)| s.clone())
        .unwrap_or_default();
    let mut names = BTreeMap::new();
    names.insert("en".into(), en.clone());
    for (locale, language) in [("ja", 0x411), ("zh_cn", 0x804)] {
        let value = records
            .iter()
            .find(|(p, l, _)| *p == ttf_parser::PlatformId::Windows && *l == language)
            .map(|(_, _, s)| s.clone())
            .unwrap_or_else(|| en.clone());
        names.insert(locale.into(), value);
    }
    (en, names)
}
pub fn catalog(bytes: &[u8], index: u32) -> Result<Catalog, String> {
    let axes = axes(bytes, index)?;
    let face = Face::parse(bytes, index).map_err(|_| "font_invalid")?;
    let raw = face
        .raw_face()
        .table(Tag::from_bytes(b"fvar"))
        .ok_or("variation_unsupported")?;
    let word = |at: usize| -> Result<usize, String> {
        let b = raw.get(at..at + 2).ok_or("variation_instance")?;
        Ok(u16::from_be_bytes([b[0], b[1]]) as usize)
    };
    let offset = word(4)?;
    let axis_count = word(8)?;
    let stride = word(10)?;
    let count = word(12)?;
    let size = word(14)?;
    if offset < 16
        || axis_count != axes.len()
        || stride != 20
        || count > 4096
        || (size != 4 + 4 * axis_count && size != 6 + 4 * axis_count)
    {
        return Err("variation_instance".into());
    }
    let start = offset + axis_count * stride;
    raw.get(start..start + count * size)
        .ok_or("variation_instance")?;
    let default_coordinates: Coordinates =
        axes.iter().map(|a| (a.tag.clone(), a.default)).collect();
    let name_id = if instance_name(&face, 17).0.is_empty() {
        2
    } else {
        17
    };
    let (name, names) = instance_name(&face, name_id);
    let mut instances = vec![Instance {
        id: 0,
        name,
        names,
        coordinates: default_coordinates.clone(),
    }];
    let mut default_seen = false;
    for i in 0..count {
        let at = start + i * size;
        let id = word(at)? as u16;
        if word(at + 2)? != 0 || !(id == 2 || id == 17 || (256..32768).contains(&id)) {
            return Err("variation_instance".into());
        }
        let mut coordinates = Coordinates::new();
        for (n, a) in axes.iter().enumerate() {
            let p = at + 4 + n * 4;
            let v = i32::from_be_bytes(raw[p..p + 4].try_into().unwrap()) as f64 / 65536.;
            if v < f64::from(a.min) || v > f64::from(a.max) {
                return Err("variation_range".into());
            }
            coordinates.insert(a.tag.clone(), v as f32);
        }
        if (id == 2 || id == 17) && coordinates != default_coordinates {
            return Err("variation_instance".into());
        }
        if size == 6 + 4 * axis_count {
            let ps_id = word(at + 4 + 4 * axis_count)?;
            if !(ps_id == 0xffff || ps_id == 6 || (256..32768).contains(&ps_id))
                || (ps_id == 6 && coordinates != default_coordinates)
            {
                return Err("variation_instance".into());
            }
        }
        let (name, names) = instance_name(&face, id);
        let instance = Instance {
            id: i + 1,
            name,
            names,
            coordinates,
        };
        if instance.coordinates == default_coordinates {
            if !default_seen {
                instances[0] = Instance { id: 0, ..instance };
                default_seen = true;
            }
        } else if !instances
            .iter()
            .any(|v| v.coordinates == instance.coordinates)
        {
            instances.push(instance);
        }
    }
    Ok(Catalog { axes, instances })
}
pub fn instantiate_named(
    bytes: &[u8],
    index: u32,
    id: usize,
    coordinates: &Coordinates,
) -> Result<FontDocument, String> {
    let instance = catalog(bytes, index)?
        .instances
        .into_iter()
        .find(|i| i.id == id)
        .ok_or("variation_instance")?;
    if &instance.coordinates != coordinates {
        return Err("variation_instance".into());
    }
    let mut doc = instantiate(bytes, index, &instance.coordinates)?;
    if !instance.name.is_empty() {
        doc.metadata.style = instance.name;
    }
    doc.validate().map_err(|e| e.to_string())?;
    Ok(doc)
}
#[derive(Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(default)]
    pub instance: Option<usize>,
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

#[cfg(test)]
mod named_tests {
    use super::*;
    fn mutated(edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let bytes = include_bytes!("../tests/data/variations/named-gvar.ttf");
        let face = Face::parse(bytes, 0).unwrap();
        let mut tables: BTreeMap<_, _> = face
            .raw_face()
            .table_records
            .into_iter()
            .map(|r| {
                (
                    r.tag.to_bytes(),
                    face.raw_face().table(r.tag).unwrap().to_vec(),
                )
            })
            .collect();
        edit(tables.get_mut(b"fvar").unwrap());
        crate::color_fonts::assemble(tables, false).unwrap()
    }
    #[test]
    fn named_records_validate_stride_bounds_flags_and_coordinates() {
        for (at, value) in [(4, 0u16), (12, 65535), (14, 1), (58, 1), (68, 1), (70, 2)] {
            let bytes = mutated(|f| f[at..at + 2].copy_from_slice(&value.to_be_bytes()));
            assert!(catalog(&bytes, 0).is_err(), "offset {at}");
        }
        let bytes = mutated(|f| f[60..64].copy_from_slice(&(901i32 * 65536).to_be_bytes()));
        assert!(catalog(&bytes, 0).is_err());
        let bytes = mutated(|f| f.truncate(f.len() - 1));
        assert!(catalog(&bytes, 0).is_err());
        let bytes = mutated(|f| {
            // Duplicate named coordinates retain the first entry, including the default.
            let size = u16::from_be_bytes([f[14], f[15]]) as usize;
            let first = f[56..56 + size].to_vec();
            f[56 + size..56 + 2 * size].copy_from_slice(&first);
        });
        assert_eq!(catalog(&bytes, 0).unwrap().instances.len(), 2);
    }
    #[test]
    fn optional_postscript_ids_and_missing_translations_use_fallbacks() {
        let bytes = mutated(|f| {
            let size = u16::from_be_bytes([f[14], f[15]]) as usize;
            let count = u16::from_be_bytes([f[12], f[13]]) as usize;
            let mut compact = f[..56].to_vec();
            compact[14..16].copy_from_slice(&12u16.to_be_bytes());
            for n in 0..count {
                compact.extend_from_slice(&f[56 + n * size..56 + n * size + 12]);
            }
            // The existing axis name has only English localization in this fixture.
            compact[68..70].copy_from_slice(&256u16.to_be_bytes());
            *f = compact;
        });
        let instances = catalog(&bytes, 0).unwrap().instances;
        assert_eq!(instances.len(), 3);
        assert_eq!(instances[1].names["ja"], instances[1].name);
        assert_eq!(instances[1].names["zh_cn"], instances[1].name);
    }
}
