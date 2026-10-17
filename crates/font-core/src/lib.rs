//! Rust-owned font documents. No WebView or renderer owns editable font data.
pub mod font_formats;
pub mod font_io;
pub mod geometry;
pub mod path_paste;
pub mod proof;
pub mod reference_images;
pub mod reference_transform;
pub mod snippets;
pub mod storage;
pub mod unicode;
pub mod workspace;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type GlyphId = u32;
pub type Point = [f64; 2];
pub type Result<T> = std::result::Result<T, Error>;

/// Stable codes; UI translates these without parsing human-readable errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    Conflict,
    InvalidDocument,
    InvalidGeometry,
    MissingGlyph,
    EmptyHistory,
    UnsupportedVersion,
    Io,
    InvalidFile,
    TooLarge,
    InvalidUnicode,
    UnicodeConflict,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Segment {
    Line {
        end: Point,
    },
    Quadratic {
        control: Point,
        end: Point,
    },
    Cubic {
        control1: Point,
        control2: Point,
        end: Point,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contour {
    /// Anchor indices with coupled tangent handles; absent in older projects.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub smooth: BTreeSet<usize>,
    pub start: Point,
    pub segments: Vec<Segment>,
    /// Open contours are valid while editing; font export will require closure.
    pub closed: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphStatus {
    #[default]
    Draft,
    Reviewed,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glyph {
    pub id: GlyphId,
    pub name: String,
    pub codepoints: BTreeSet<char>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cid: Option<u32>,
    pub advance: f64,
    pub contours: Vec<Contour>,
    pub status: GlyphStatus,
}
impl Glyph {
    pub fn new(id: GlyphId, character: char) -> Self {
        Self {
            id,
            name: if character as u32 <= 0xFFFF {
                format!("uni{:04X}", character as u32)
            } else {
                format!("u{:05X}", character as u32)
            },
            codepoints: [character].into(),
            cid: None,
            advance: 600.,
            contours: vec![],
            status: GlyphStatus::Draft,
        }
    }
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || !self.advance.is_finite()
            || !(0. ..=65535.).contains(&self.advance)
            || self.cid.is_some_and(|cid| {
                cid > 65535 || (self.id == 0 && cid != 0) || (self.id != 0 && cid == 0)
            })
            || self.contours.len() > 4096
        {
            return Err(Error::InvalidDocument);
        }
        for contour in &self.contours {
            geometry::validate(contour)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CidSystem {
    pub registry: String,
    pub ordering: String,
    pub supplement: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FontMetadata {
    pub style: String,
    pub weight: u16,
    pub width: u16,
    pub italic_angle: f64,
    pub bold: bool,
    pub italic: bool,
    pub copyright: String,
    pub license: String,
    pub license_url: String,
    pub cid_system: Option<CidSystem>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontDocument {
    #[serde(default, skip_serializing_if = "snippets::Library::is_empty")]
    pub snippets: snippets::Library,
    #[serde(
        default,
        skip_serializing_if = "reference_images::ReferenceImages::is_default"
    )]
    pub reference_images: reference_images::ReferenceImages,
    pub schema_version: u32,
    pub family: String,
    pub units_per_em: u16,
    pub ascender: f64,
    pub descender: f64,
    pub glyphs: BTreeMap<GlyphId, Glyph>,
    #[serde(default)]
    pub metadata: FontMetadata,
}
impl FontDocument {
    pub fn new(family: impl Into<String>, characters: &str) -> Result<Self> {
        let mut glyphs = BTreeMap::new();
        glyphs.insert(
            0,
            Glyph {
                id: 0,
                name: ".notdef".into(),
                codepoints: BTreeSet::new(),
                cid: None,
                advance: 600.,
                contours: vec![],
                status: GlyphStatus::Draft,
            },
        );
        let mut seen = BTreeSet::new();
        for c in characters.chars().filter(|c| seen.insert(*c)) {
            let id = u32::try_from(glyphs.len()).map_err(|_| Error::TooLarge)?;
            glyphs.insert(id, Glyph::new(id, c));
        }
        let document = Self {
            snippets: Default::default(),
            reference_images: Default::default(),
            schema_version: 1,
            family: family.into(),
            units_per_em: 1000,
            ascender: 800.,
            descender: -200.,
            glyphs,
            metadata: FontMetadata::default(),
        };
        document.validate()?;
        Ok(document)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if self.family.trim().is_empty()
            || self.family.len() > 1024
            || !(16..=16384).contains(&self.units_per_em)
            || !self.ascender.is_finite()
            || !self.descender.is_finite()
            || self.ascender <= 0.
            || self.descender > 0.
            || self.metadata.style.len() > 1024
            || self.metadata.weight > 1000
            || self.metadata.width > 9
            || !self.metadata.italic_angle.is_finite()
            || self.metadata.italic_angle.abs() > 90.
            || [
                &self.metadata.copyright,
                &self.metadata.license,
                &self.metadata.license_url,
            ]
            .iter()
            .any(|s| s.len() > 32768)
            || self.metadata.cid_system.as_ref().is_some_and(|c| {
                c.registry.is_empty()
                    || c.ordering.is_empty()
                    || c.registry.len() > 256
                    || c.ordering.len() > 256
                    || c.supplement < 0
            })
            || self.glyphs.len() > 65535
            || !self
                .glyphs
                .get(&0)
                .is_some_and(|g| g.name == ".notdef" && g.codepoints.is_empty())
        {
            return Err(Error::InvalidDocument);
        }
        self.reference_images.validate(&self.glyphs)?;
        self.snippets.validate()?;
        let mut cids = BTreeSet::new();
        let mut codes = BTreeSet::new();
        let mut names = BTreeSet::new();
        for (id, glyph) in &self.glyphs {
            glyph.validate()?;
            if glyph
                .cid
                .is_some_and(|cid| cid > 65535 || !cids.insert(cid))
                || *id != glyph.id
                || !names.insert(&glyph.name)
                || !glyph.codepoints.iter().all(|c| codes.insert(*c))
            {
                return Err(Error::InvalidDocument);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontProperties {
    pub family: String,
    pub metadata: FontMetadata,
}
#[derive(Clone, Default)]
struct Change {
    before_snippets: Option<snippets::Library>,
    after_snippets: Option<snippets::Library>,
    before_references: Option<reference_images::ReferenceImages>,
    after_references: Option<reference_images::ReferenceImages>,
    before: Vec<Glyph>,
    after: Vec<Glyph>,
    before_properties: Option<FontProperties>,
    after_properties: Option<FontProperties>,
}

/// Central state shared by all future window commands under an application lock.
/// Histories retain changed glyphs only, never whole font snapshots.
pub struct Editor {
    document: FontDocument,
    revision: u64,
    undo: VecDeque<Change>,
    redo: Vec<Change>,
    history_limit: usize,
}
impl Editor {
    pub fn new(document: FontDocument) -> Result<Self> {
        document.validate()?;
        Ok(Self {
            document,
            revision: 0,
            undo: VecDeque::new(),
            redo: vec![],
            history_limit: 128,
        })
    }
    pub fn document(&self) -> &FontDocument {
        &self.document
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    fn check_revision(&self, expected: u64) -> Result<u64> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        self.revision.checked_add(1).ok_or(Error::Conflict)
    }
    /// One gesture commits one validated replacement. Failure changes nothing.
    pub fn replace_glyph(&mut self, expected: u64, glyph: Glyph) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        let before = self
            .document
            .glyphs
            .get(&glyph.id)
            .ok_or(Error::MissingGlyph)?;
        glyph.validate()?;
        if glyph.id == 0 && (glyph.name != ".notdef" || !glyph.codepoints.is_empty()) {
            return Err(Error::InvalidDocument);
        }
        if self.document.glyphs.values().any(|g| {
            g.id != glyph.id
                && (g.name == glyph.name
                    || !g.codepoints.is_disjoint(&glyph.codepoints)
                    || (g.cid.is_some() && g.cid == glyph.cid))
        }) {
            return Err(Error::InvalidDocument);
        }
        if before == &glyph {
            return Ok(self.revision);
        }
        let change = Change {
            before: vec![before.clone()],
            after: vec![glyph.clone()],
            ..Default::default()
        };
        self.document.glyphs.insert(glyph.id, glyph);
        self.undo.push_back(change);
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(revision)
    }
    /// Add a bounded batch atomically. Existing mappings are preserved; duplicates are no-ops.
    pub fn add_characters(
        &mut self,
        expected: u64,
        characters: &BTreeSet<char>,
    ) -> Result<Vec<GlyphId>> {
        let revision = self.check_revision(expected)?;
        if characters.len() > unicode::MAX_BATCH {
            return Err(Error::TooLarge);
        }
        let existing: BTreeSet<char> = self
            .document
            .glyphs
            .values()
            .flat_map(|g| g.codepoints.iter().copied())
            .collect();
        let missing: Vec<char> = characters.difference(&existing).copied().collect();
        if self.document.glyphs.len() + missing.len() > 65535 {
            return Err(Error::TooLarge);
        }
        if missing.is_empty() {
            return Ok(vec![]);
        }
        let mut id = *self
            .document
            .glyphs
            .keys()
            .next_back()
            .ok_or(Error::InvalidDocument)?;
        let mut names: BTreeSet<String> = self
            .document
            .glyphs
            .values()
            .map(|g| g.name.clone())
            .collect();
        let mut after = Vec::with_capacity(missing.len());
        for c in missing {
            id = id.checked_add(1).ok_or(Error::TooLarge)?;
            let mut glyph = Glyph::new(id, c);
            let base = glyph.name.clone();
            let mut suffix = 1;
            while !names.insert(glyph.name.clone()) {
                glyph.name = format!("{base}.{suffix}");
                suffix += 1;
            }
            after.push(glyph);
        }
        let ids = after.iter().map(|g| g.id).collect();
        for g in &after {
            self.document.glyphs.insert(g.id, g.clone());
        }
        self.undo.push_back(Change {
            before: vec![],
            after,
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(ids)
    }
    /// Validate and commit a complete mapping edit as one undo transaction.
    pub fn replace_glyphs(&mut self, expected: u64, glyphs: Vec<Glyph>) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        let mut candidate = self.document.clone();
        let mut before = vec![];
        let mut after = vec![];
        let mut ids = BTreeSet::new();
        for glyph in glyphs {
            if !ids.insert(glyph.id) {
                return Err(Error::InvalidDocument);
            }
            let old = candidate.glyphs.get(&glyph.id).ok_or(Error::MissingGlyph)?;
            if old != &glyph {
                before.push(old.clone());
                after.push(glyph.clone());
                candidate.glyphs.insert(glyph.id, glyph);
            }
        }
        candidate.validate()?;
        if after.is_empty() {
            return Ok(self.revision);
        }
        self.document = candidate;
        self.undo.push_back(Change {
            before,
            after,
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(revision)
    }
    pub fn assign_unicode(
        &mut self,
        expected: u64,
        id: GlyphId,
        codepoints: BTreeSet<char>,
    ) -> Result<u64> {
        self.check_revision(expected)?;
        if id == 0 {
            return Err(Error::InvalidDocument);
        }
        if codepoints.len() > unicode::MAX_BATCH {
            return Err(Error::TooLarge);
        }
        if self
            .document
            .glyphs
            .values()
            .any(|g| g.id != id && !g.codepoints.is_disjoint(&codepoints))
        {
            return Err(Error::UnicodeConflict);
        }
        let mut glyph = self
            .document
            .glyphs
            .get(&id)
            .ok_or(Error::MissingGlyph)?
            .clone();
        glyph.codepoints = codepoints;
        self.replace_glyph(expected, glyph)
    }
    pub fn properties(&self) -> FontProperties {
        FontProperties {
            family: self.document.family.clone(),
            metadata: self.document.metadata.clone(),
        }
    }
    pub fn set_properties(&mut self, expected: u64, properties: FontProperties) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        let mut validation = FontDocument::new(properties.family.clone(), "")?;
        validation.metadata = properties.metadata.clone();
        validation.validate()?;
        let before = self.properties();
        if before == properties {
            return Ok(self.revision);
        }
        self.document.family = properties.family.clone();
        self.document.metadata = properties.metadata.clone();
        self.undo.push_back(Change {
            before_properties: Some(before),
            after_properties: Some(properties),
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(revision)
    }
    pub fn undo(&mut self, expected: u64) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        let change = self.undo.pop_back().ok_or(Error::EmptyHistory)?;
        for glyph in &change.after {
            self.document.glyphs.remove(&glyph.id);
        }
        for glyph in &change.before {
            self.document.glyphs.insert(glyph.id, glyph.clone());
        }
        if let Some(p) = &change.before_properties {
            self.document.family = p.family.clone();
            self.document.metadata = p.metadata.clone();
        }
        if let Some(references) = &change.before_references {
            self.document.reference_images = references.clone();
        }
        if let Some(s) = &change.before_snippets {
            self.document.snippets = s.clone();
        }
        self.redo.push(change);
        self.revision = revision;
        Ok(revision)
    }
    pub fn redo(&mut self, expected: u64) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        let change = self.redo.pop().ok_or(Error::EmptyHistory)?;
        for glyph in &change.after {
            self.document.glyphs.insert(glyph.id, glyph.clone());
        }
        if let Some(p) = &change.after_properties {
            self.document.family = p.family.clone();
            self.document.metadata = p.metadata.clone();
        }
        if let Some(references) = &change.after_references {
            self.document.reference_images = references.clone();
        }
        if let Some(s) = &change.after_snippets {
            self.document.snippets = s.clone();
        }
        self.undo.push_back(change);
        self.revision = revision;
        Ok(revision)
    }
}
