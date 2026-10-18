//! Proof the actual static TrueType output. Font bytes and outlines stay in Rust.
use crate::{font_io, FontDocument, GlyphId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "kerning_default")]
    pub kerning: bool,
    pub text: String,
    pub size: f64,
    pub tracking: f64,
    pub leading: f64,
    pub scroll: f64,
}
fn kerning_default() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self {
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
        if self.text.len() > 16384
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
    pub document: FontDocument,
    pub original: BTreeMap<GlyphId, GlyphId>,
    pub mapping: BTreeMap<char, GlyphId>,
    pub missing: Vec<char>,
    pub empty: usize,
}
pub fn subset(doc: &FontDocument, text: &str) -> Result<FontDocument, String> {
    let chars: BTreeSet<char> = text.chars().filter(|c| !c.is_control()).collect();
    let mut copy = FontDocument {
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
        .filter(|c| !c.is_control() && !mapping.contains_key(c))
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
    Ok(Prepared {
        document,
        original,
        mapping,
        missing,
        empty,
    })
}
#[derive(Clone, Debug)]
pub struct Placement {
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
/// Horizontal scalar layout; break Latin words together, CJK per scalar.
/// No imported GSUB/GPOS survives the current static export pipeline.
pub fn layout(font: &Prepared, settings: &Settings, width: f64) -> Layout {
    let scale = settings.size / f64::from(font.document.units_per_em);
    let left = 24.;
    let right = (width - 24.).max(left + 1.);
    let line_height = (font.document.ascender - font.document.descender)
        .max(f64::from(font.document.units_per_em))
        * scale
        * settings.leading;
    let mut baseline = 24. + font.document.ascender * scale;
    let mut x = left;
    let mut placements = Vec::new();
    let text = settings.text.replace("\r\n", "\n").replace('\r', "\n");
    let mut tokens: Vec<String> = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '\'' {
            word.push(c);
        } else {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            tokens.push(c.to_string());
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    let advance = |c: char| {
        (if c == '\t' {
            settings.size * 2.
        } else {
            font.mapping
                .get(&c)
                .map(|id| font.document.glyphs[id].advance * scale)
                .unwrap_or(settings.size * 0.6)
        }) + settings.tracking * settings.size
    };
    let mut previous = None;
    let pair = |left: Option<GlyphId>, c: char| -> f64 {
        if !settings.kerning || c.is_control() {
            return 0.;
        }
        left.zip(font.mapping.get(&c).copied())
            .map(|(l, r)| f64::from(font.document.kerning.get(l, r)) * scale)
            .unwrap_or(0.)
    };
    for token in tokens {
        if token == "\n" {
            previous = None;
            x = left;
            baseline += line_height;
            continue;
        }
        let mut preceding = previous;
        let token_width: f64 = token
            .chars()
            .map(|c| {
                let w = advance(c).max(0.) + pair(preceding, c);
                preceding = if c.is_control() {
                    None
                } else {
                    font.mapping.get(&c).copied()
                };
                w
            })
            .sum();
        if x > left && x + token_width > right && token_width <= right - left {
            x = left;
            previous = None;
            baseline += line_height;
        }
        for c in token.chars() {
            if c.is_control() && c != '\t' {
                continue;
            }
            let width = advance(c).max(0.);
            let mut adjustment = pair(previous, c);
            if x > left && x + adjustment + width > right {
                x = left;
                baseline += line_height;
                adjustment = 0.;
            }
            x += adjustment;
            let glyph = font.mapping.get(&c).copied().unwrap_or(0);
            placements.push(Placement {
                glyph,
                missing: c != '\t' && !font.mapping.contains_key(&c),
                source: font
                    .mapping
                    .get(&c)
                    .and_then(|id| font.original.get(id))
                    .copied(),
                x,
                baseline,
                width: width.max(settings.size * 0.15),
                top: baseline - font.document.ascender * scale,
                bottom: baseline - font.document.descender * scale,
            });
            x += width;
            previous = if c.is_control() {
                None
            } else {
                font.mapping.get(&c).copied()
            };
        }
    }
    Layout {
        placements,
        height: baseline - font.document.descender * scale + 24.,
    }
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
