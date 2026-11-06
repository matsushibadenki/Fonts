//! Proof the actual static TrueType output. Font bytes and outlines stay in Rust.
mod orientation;
mod shaping;
use crate::{font_io, FontDocument, GlyphId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Mixed,
    Upright,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_column_height")]
    pub column_height: f64,
    #[serde(default = "kerning_default")]
    pub kerning: bool,
    pub text: String,
    pub size: f64,
    pub tracking: f64,
    pub leading: f64,
    pub scroll: f64,
}
fn default_language() -> String {
    "und".into()
}
fn default_column_height() -> f64 {
    600.
}
fn kerning_default() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            orientation: Orientation::Mixed,
            direction: Direction::Horizontal,
            language: default_language(),
            column_height: default_column_height(),
            kerning: true,
            text: "ABC あいう 中文\nThe quick brown fox jumps over the lazy dog.\n0123456789"
                .into(),
            size: 64.,
            tracking: 0.,
            leading: 1.5,
            scroll: 0.,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.language.len() > 64
            || !self
                .language
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !self.column_height.is_finite()
            || !(120. ..=2400.).contains(&self.column_height)
            || self.text.len() > 16384
            || self.text.chars().count() > 2048
            || !self.size.is_finite()
            || !(12. ..=160.).contains(&self.size)
            || !self.tracking.is_finite()
            || !(-0.1..=1.).contains(&self.tracking)
            || !self.leading.is_finite()
            || !(0.8..=3.).contains(&self.leading)
            || !self.scroll.is_finite()
            || !(0. ..=1_000_000.).contains(&self.scroll)
        {
            return Err("proof_invalid_settings".into());
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct Prepared {
    bytes: std::sync::Arc<Vec<u8>>,
    shaped: shaping::Cache,
    pub document: FontDocument,
    pub original: BTreeMap<GlyphId, GlyphId>,
    pub mapping: BTreeMap<char, GlyphId>,
    pub missing: Vec<char>,
    pub empty: usize,
}
pub fn subset(doc: &FontDocument, text: &str) -> Result<FontDocument, String> {
    if !doc.imported_layout.is_empty() {
        let mut copy = doc.clone();
        copy.components = Default::default();
        copy.reference_images = Default::default();
        copy.snippets = Default::default();
        return Ok(copy);
    }
    let chars: BTreeSet<char> = text.chars().filter(|c| !c.is_control()).collect();
    let mut copy = FontDocument {
        source_font: None,
        imported_layout: Default::default(),
        components: Default::default(),
        document_uuid: doc.document_uuid,
        class_kerning: Default::default(),
        kerning: Default::default(),
        snippets: Default::default(),
        reference_images: Default::default(),
        glyphs: BTreeMap::new(),
        family: doc.family.clone(),
        metadata: doc.metadata.clone(),
        schema_version: doc.schema_version,
        units_per_em: doc.units_per_em,
        ascender: doc.ascender,
        descender: doc.descender,
    };
    for (id, g) in &doc.glyphs {
        if *id == 0 || g.codepoints.iter().any(|c| chars.contains(c)) {
            copy.glyphs.insert(*id, g.clone());
        }
    }
    copy.class_kerning = doc
        .class_kerning
        .subset(&copy.glyphs.keys().copied().collect());
    copy.kerning = doc.kerning.subset(&copy.glyphs.keys().copied().collect());
    Ok(copy)
}
pub fn prepare(subset: FontDocument, text: &str) -> Result<Prepared, String> {
    let original = subset
        .glyphs
        .keys()
        .enumerate()
        .map(|(dense, id)| (dense as u32, *id))
        .collect();
    let bytes = font_io::export(&subset)?;
    let document = font_io::import(&bytes, 0)?;
    let mapping: BTreeMap<_, _> = document
        .glyphs
        .iter()
        .flat_map(|(id, g)| g.codepoints.iter().map(move |c| (*c, *id)))
        .collect();
    let missing = text
        .chars()
        .filter(|c| !c.is_control() && !matches!(*c,'\u{200c}'|'\u{200d}'|'\u{fe00}'..='\u{fe0f}'|'\u{e0100}'..='\u{e01ef}') && !mapping.contains_key(c))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let empty = document
        .glyphs
        .values()
        .filter(|g| {
            g.id != 0
                && g.contours.is_empty()
                && g.codepoints
                    .iter()
                    .any(|c| !c.is_whitespace() && text.contains(*c))
        })
        .count();
    let mut prepared = Prepared {
        bytes: std::sync::Arc::new(bytes),
        shaped: Default::default(),
        document,
        original,
        mapping,
        missing,
        empty,
    };
    prepared.missing = prepared.shaped_missing(text)?;
    prepared.warm_shaping(&Settings {
        text: text.into(),
        ..Default::default()
    })?;
    Ok(prepared)
}
#[derive(Clone, Debug)]
pub struct Placement {
    pub left: f64,
    pub rotated: bool,
    pub glyph: GlyphId,
    pub source: Option<GlyphId>,
    pub missing: bool,
    pub x: f64,
    pub baseline: f64,
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
}
pub struct Layout {
    pub placements: Vec<Placement>,
    pub height: f64,
}
/// HarfBuzz shapes the exported font; all placement and outline data stays in Rust.
pub fn layout(font: &Prepared, settings: &Settings, width: f64) -> Layout {
    shaping::layout(font, settings, width)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_export_metrics_supplementary_missing_and_click_mapping() {
        let mut doc = FontDocument::new("Proof", "A中𠀀 ").unwrap();
        doc.glyphs.get_mut(&1).unwrap().advance = 777.;
        let s = Settings {
            text: "A中𠀀?\nAA".into(),
            ..Default::default()
        };
        let f = prepare(subset(&doc, &s.text).unwrap(), &s.text).unwrap();
        assert_eq!(f.missing, vec!['?']);
        let l = layout(&f, &s, 800.);
        assert_eq!(l.placements.len(), 6);
        assert_eq!(l.placements[0].source, Some(1));
        assert!((l.placements[1].x - 24. - 777. * s.size / 1000.).abs() < 1e-8);
        assert!(l.placements[4].baseline > l.placements[0].baseline);
        assert_eq!(l.placements[3].source, None);
    }
    #[test]
    fn wrapping_limits_and_empty_text() {
        let doc = FontDocument::new("Proof", "AB ").unwrap();
        let mut s = Settings {
            text: "AB AB".into(),
            ..Default::default()
        };
        let f = prepare(subset(&doc, &s.text).unwrap(), &s.text).unwrap();
        let l = layout(&f, &s, 160.);
        assert!(l.placements.last().unwrap().baseline > l.placements[0].baseline);
        s.text.clear();
        assert!(layout(&f, &s, 160.).placements.is_empty());
        s.size = f64::NAN;
        assert!(s.validate().is_err());
        s = Settings {
            text: "A".repeat(2049),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }
}
