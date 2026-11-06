//! Project-owned source shaping tables, retained only with compatible glyph indexing and units.
mod points;
mod sfnt;
use crate::{
    class_kerning::ClassKerning, kerning::Kerning, Change, Editor, Error, FontDocument, GlyphId,
    Result,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
const LIMIT: usize = 32 * 1024 * 1024;
const TAGS: &[&str] = &[
    "GSUB", "GPOS", "GDEF", "BASE", "JSTF", "kern", "vhea", "vmtx", "VORG", "UVS ",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    #[serde(with = "bytes")]
    pub data: Arc<Vec<u8>>,
}
mod bytes {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    pub fn serialize<S: serde::Serializer>(
        v: &Arc<Vec<u8>>,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v.as_slice()))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Arc<Vec<u8>>, D::Error> {
        let value = String::deserialize(d)?;
        if value.len() > LIMIT * 4 / 3 + 4 {
            return Err(serde::de::Error::custom("layout too large"));
        }
        let value = STANDARD.decode(value).map_err(serde::de::Error::custom)?;
        if value.len() > LIMIT {
            return Err(serde::de::Error::custom("layout too large"));
        }
        Ok(Arc::new(value))
    }
}
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedLayout {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub normalized_points: bool,
    pub units: u16,
    pub glyphs: Arc<Vec<GlyphId>>,
    pub tables: BTreeMap<String, Table>,
    pub kerning: Kerning,
    pub classes: ClassKerning,
}
#[derive(Serialize)]
pub struct Summary {
    pub present: bool,
    pub positioning: bool,
    pub substitution: bool,
    pub vertical: bool,
    pub variations: bool,
}
impl ImportedLayout {
    pub fn summary(&self) -> Summary {
        Summary {
            present: !self.is_empty(),
            positioning: self.positioning(),
            substitution: self.tables.contains_key("GSUB"),
            vertical: self.tables.contains_key("vhea") || self.tables.contains_key("VORG"),
            variations: self.tables.contains_key("UVS "),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
    pub fn positioning(&self) -> bool {
        self.tables.contains_key("GPOS") || self.tables.contains_key("kern")
    }
    pub fn validate(&self) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        if !(16..=16384).contains(&self.units)
            || self.glyphs.is_empty()
            || self.glyphs.len() > 65535
            || self.glyphs[0] != 0
            || self.glyphs.windows(2).any(|p| p[0] >= p[1])
            || self.tables.len() > TAGS.len()
            || self.tables.keys().any(|s| !TAGS.contains(&s.as_str()))
            || self.tables.values().map(|v| v.data.len()).sum::<usize>() > LIMIT
        {
            return Err(Error::InvalidDocument);
        }
        self.kerning
            .validate_glyphs(|id| self.glyphs.binary_search(id).is_ok())?;
        self.classes
            .validate_glyphs(|id| self.glyphs.binary_search(id).is_ok())?;
        sfnt::validate_tables(self).map_err(|_| Error::InvalidDocument)?;
        if self.tables.contains_key("vhea") != self.tables.contains_key("vmtx") {
            return Err(Error::InvalidDocument);
        }
        Ok(())
    }
    pub fn compatible(&self, doc: &FontDocument) -> bool {
        self.is_empty()
            || (self.units == doc.units_per_em
                && self.glyphs.iter().copied().eq(doc
                    .glyphs
                    .keys()
                    .copied()
                    .take(self.glyphs.len())))
    }
    pub fn import(doc: &FontDocument, bytes: &[u8], index: u32) -> crate::font_io::Result<Self> {
        let raw = ttf_parser::RawFace::parse(bytes, index).map_err(|_| "layout_invalid")?;
        let mut tables = BTreeMap::new();
        for &tag in TAGS {
            if tag == "UVS " {
                continue;
            }
            let t: [u8; 4] = tag.as_bytes().try_into().map_err(|_| "layout_invalid")?;
            if let Some(data) = raw.table(ttf_parser::Tag::from_bytes(&t)) {
                // Our exact generated GPOS is already represented by the editable pair/class model.
                if tag == "GPOS"
                    && crate::kerning::gpos(doc)
                        .map_err(|_| "layout_invalid")?
                        .as_deref()
                        == Some(data)
                {
                    continue;
                }
                tables.insert(
                    tag.into(),
                    Table {
                        data: Arc::new(data.to_vec()),
                    },
                );
            }
        }
        if let Some(cmap) = raw.table(ttf_parser::Tag::from_bytes(b"cmap")) {
            if let Some(data) = sfnt::variation_cmap(cmap)? {
                tables.insert(
                    "UVS ".into(),
                    Table {
                        data: Arc::new(data),
                    },
                );
            }
        }
        if let (Some(h), Some(m)) = (tables.get("vhea"), tables.get("vmtx")) {
            let face = ttf_parser::Face::parse(bytes, index).map_err(|_| "layout_invalid")?;
            let (default_h, default_m) = sfnt::default_vertical(doc, &face)?;
            if h.data.as_ref() == &default_h && m.data.as_ref() == &default_m {
                tables.remove("vhea");
                tables.remove("vmtx");
            }
        }
        if tables.is_empty() {
            return Ok(Self::default());
        }
        let mut value = Self {
            normalized_points: false,
            units: doc.units_per_em,
            glyphs: Arc::new(doc.glyphs.keys().copied().collect()),
            tables,
            kerning: doc.kerning.clone(),
            classes: doc.class_kerning.clone(),
        };
        points::normalize(&mut value, bytes, index)?;
        value.validate().map_err(|_| "layout_invalid")?;
        Ok(value)
    }
    pub fn apply(
        &self,
        doc: &FontDocument,
        font: Vec<u8>,
        cff: bool,
    ) -> crate::font_io::Result<Vec<u8>> {
        sfnt::apply(self, doc, font, cff)
    }
}
impl Editor {
    pub fn release_imported_positioning(&mut self, expected: u64) -> Result<u64> {
        self.release_imported_layout(expected, true)
    }
    pub fn release_imported_layout(
        &mut self,
        expected: u64,
        positioning_only: bool,
    ) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        if self.document.imported_layout.is_empty()
            || (positioning_only && !self.document.imported_layout.positioning())
        {
            return Ok(self.revision);
        }
        let before = self.document.imported_layout.clone();
        let mut after = before.clone();
        for tag in ["GPOS", "kern", "JSTF"] {
            after.tables.remove(tag);
        }
        if !positioning_only || after.is_empty() {
            after = ImportedLayout::default();
        }
        self.document.imported_layout = after.clone();
        self.undo.push_back(Change {
            before_layout: Some(before),
            after_layout: Some(after),
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(revision)
    }
}
