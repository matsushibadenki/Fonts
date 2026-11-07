//! Project-owned source fonts. Specialized editing preserves tables ordinary outline editing cannot.
pub mod master_sample;
pub mod master_tuple;
pub mod masters;
mod names;
mod palette;
pub mod variable;
use crate::{Change, Editor, Error, FontDocument, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
const LIMIT: usize = 128 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Color,
    Variable,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFont {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<variable::Variable>,
    pub kind: Kind,
    #[serde(with = "data")]
    pub bytes: Arc<Vec<u8>>,
    pub signature: String,
    pub palette: u16,
    pub colors: BTreeMap<String, [u8; 4]>,
}
mod data {
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
            return Err(serde::de::Error::custom("source font too large"));
        }
        let bytes = STANDARD.decode(value).map_err(serde::de::Error::custom)?;
        if bytes.len() > LIMIT {
            return Err(serde::de::Error::custom("source font too large"));
        }
        Ok(Arc::new(bytes))
    }
}
fn signature(doc: &FontDocument) -> Result<String> {
    let glyphs: Vec<_> = doc
        .glyphs
        .values()
        .map(|g| {
            (
                g.id,
                &g.name,
                &g.codepoints,
                g.cid,
                g.advance,
                g.contours
                    .iter()
                    .map(|c| (c.start, &c.segments, c.closed))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let bytes = serde_json::to_vec(&(
        &doc.family,
        &doc.metadata,
        doc.units_per_em,
        doc.ascender,
        doc.descender,
        glyphs,
        &doc.kerning,
        &doc.class_kerning,
    ))
    .map_err(|_| Error::InvalidDocument)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect())
}
impl SourceFont {
    pub fn validate(&self, doc: &FontDocument) -> Result<()> {
        if self.bytes.len() > LIMIT
            || self.bytes.is_empty()
            || self.signature != signature(doc)?
            || !doc.imported_layout.is_empty()
            || !doc.components.is_empty()
        {
            return Err(Error::InvalidDocument);
        }
        if let Some(v) = &self.variable {
            v.validate(&self.bytes)
                .map_err(|_| Error::InvalidDocument)?;
        }
        match self.kind {
            Kind::Color => {
                let info =
                    crate::color_fonts::info(&self.bytes, 0).map_err(|_| Error::InvalidDocument)?;
                if self.palette >= info.palettes.max(1) || self.colors.len() > 4096 {
                    return Err(Error::InvalidDocument);
                }
            }
            Kind::Variable => {
                if self.variable.is_none() || self.palette != 0 || !self.colors.is_empty() {
                    return Err(Error::InvalidDocument);
                }
            }
        }
        palette::validate(&self.bytes, &self.colors).map_err(|_| Error::InvalidDocument)
    }
    pub fn color_document(bytes: &[u8], index: u32) -> crate::font_io::Result<FontDocument> {
        Self::document(crate::color_fonts::export(bytes, index)?, Kind::Color, None)
    }
    pub fn variable_document(
        bytes: &[u8],
        index: u32,
        coordinates: Option<crate::variations::Coordinates>,
    ) -> crate::font_io::Result<FontDocument> {
        crate::variations::axes(bytes, index)?;
        let face = ttf_parser::Face::parse(bytes, index).map_err(|_| "font_invalid")?;
        let color = [b"COLR", b"sbix", b"CBDT", b"SVG "].iter().any(|tag| {
            face.raw_face()
                .table(ttf_parser::Tag::from_bytes(tag))
                .is_some()
        });
        let bytes = if color {
            crate::color_fonts::export(bytes, index)?
        } else {
            crate::hinting::export(bytes, index, false)?
        };
        Self::document(
            bytes,
            if color { Kind::Color } else { Kind::Variable },
            coordinates,
        )
    }
    fn document(
        bytes: Vec<u8>,
        kind: Kind,
        coordinates: Option<crate::variations::Coordinates>,
    ) -> crate::font_io::Result<FontDocument> {
        if bytes.len() > LIMIT {
            return Err("source_font_limit".into());
        }
        let face = ttf_parser::Face::parse(&bytes, 0).map_err(|_| "font_invalid")?;
        let mut variable = if face.is_variable() {
            Some(variable::Variable::new(&bytes)?)
        } else {
            None
        };
        if let Some(coords) = coordinates {
            variable
                .as_mut()
                .ok_or("variation_unsupported")?
                .coordinates = coords;
        }
        if let Some(v) = &variable {
            v.validate(&bytes)?;
        }
        let family = crate::font_io::name(&face, 1);
        let mut doc = crate::font_io::import_source_face(&bytes, face)?;
        doc.family = if family.trim().is_empty() {
            "Untitled Font".into()
        } else {
            family
        };
        doc.schema_version = 6;
        doc.source_font = Some(Self {
            variable,
            kind,
            bytes: Arc::new(bytes),
            signature: signature(&doc).map_err(|_| "font_invalid")?,
            palette: 0,
            colors: BTreeMap::new(),
        });
        doc.validate().map_err(|e| e.to_string())?;
        Ok(doc)
    }
    pub fn export_format(
        &self,
        format: crate::font_formats::Format,
    ) -> crate::font_io::Result<Vec<u8>> {
        use crate::font_formats::Format;
        let otf = self.bytes.starts_with(b"OTTO");
        match format {
            Format::Ttf if !otf => self.export(),
            Format::Otf if otf => self.export(),
            Format::Woff2 => crate::font_formats::woff2::encode_sfnt(self.export()?, 9),
            _ => Err("source_font_format".into()),
        }
    }
    pub fn export(&self) -> crate::font_io::Result<Vec<u8>> {
        let bytes = palette::apply(&self.bytes, &self.colors)?;
        match &self.variable {
            Some(v) => v.export(&bytes),
            None => Ok(bytes),
        }
    }
    pub fn static_bytes(&self) -> crate::font_io::Result<Vec<u8>> {
        let bytes = palette::apply(&self.bytes, &self.colors)?;
        self.variable
            .as_ref()
            .ok_or("variation_unsupported")?
            .static_bytes(&bytes)
    }
    pub fn static_document(&self) -> crate::font_io::Result<FontDocument> {
        match self.kind {
            Kind::Variable => crate::variations::instantiate(
                &self.bytes,
                0,
                &self
                    .variable
                    .as_ref()
                    .ok_or("variation_unsupported")?
                    .coordinates,
            ),
            Kind::Color => {
                let mut doc = Self::color_document(&self.static_bytes()?, 0)?;
                doc.source_font.as_mut().ok_or("color_invalid")?.palette = self.palette;
                doc.validate().map_err(|e| e.to_string())?;
                Ok(doc)
            }
        }
    }
    pub fn verify_output(&self, output: &[u8], woff2: bool) -> crate::font_io::Result<()> {
        crate::color_fonts::verify_source(output, &self.export()?, woff2)
    }
    pub fn palette_colors(&self) -> crate::font_io::Result<Vec<[u8; 4]>> {
        palette::colors(&self.bytes, self.palette, &self.colors)
    }
}
impl Editor {
    pub fn edit_source_palette(
        &mut self,
        expected: u64,
        palette: u16,
        entry: Option<u16>,
        color: Option<[u8; 4]>,
    ) -> Result<u64> {
        let revision = self.check_revision_unrestricted(expected)?;
        let before = self
            .document
            .source_font
            .clone()
            .ok_or(Error::InvalidDocument)?;
        let mut after = before.clone();
        let info = crate::color_fonts::info(&after.bytes, 0).map_err(|_| Error::InvalidDocument)?;
        if palette >= info.palettes.max(1) {
            return Err(Error::InvalidDocument);
        }
        after.palette = palette;
        if let Some(entry) = entry {
            if usize::from(entry)
                >= after
                    .palette_colors()
                    .map_err(|_| Error::InvalidDocument)?
                    .len()
            {
                return Err(Error::InvalidDocument);
            }
            let key = format!("{palette}:{entry}");
            if let Some(color) = color {
                after.colors.insert(key, color);
            } else {
                after.colors.remove(&key);
            }
        }
        if after.colors.len() > 4096 {
            return Err(Error::TooLarge);
        }
        palette::validate(&after.bytes, &after.colors).map_err(|_| Error::InvalidDocument)?;
        if before == after {
            return Ok(self.revision);
        }
        self.document.source_font = Some(after.clone());
        self.undo.push_back(Change {
            before_source: Some(Some(before)),
            after_source: Some(Some(after)),
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

#[derive(Serialize)]
pub struct Summary {
    pub kind: Kind,
    pub extension: String,
    pub palette: u16,
    pub palettes: u16,
    pub colors: Vec<[u8; 4]>,
    pub sample: String,
    pub variable: Option<variable::Summary>,
}
impl SourceFont {
    pub fn summary(&self) -> Summary {
        let info = crate::color_fonts::info(&self.bytes, 0).ok();
        Summary {
            variable: self
                .variable
                .as_ref()
                .and_then(|v| v.summary(&self.bytes).ok()),
            kind: self.kind,
            extension: if self.bytes.starts_with(b"OTTO") {
                "otf"
            } else {
                "ttf"
            }
            .into(),
            palette: self.palette,
            palettes: info.as_ref().map_or(0, |i| i.palettes),
            colors: self.palette_colors().unwrap_or_default(),
            sample: info
                .map(|i| i.sample)
                .unwrap_or_else(|| "ABC あいう 中文 0123456789".into()),
        }
    }
}
