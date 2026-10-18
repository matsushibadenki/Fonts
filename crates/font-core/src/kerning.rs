//! Explicit horizontal glyph pairs in font units. Zero removes the pair.
use crate::{Change, Editor, Error, FontDocument, GlyphId, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_PAIRS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub left: GlyphId,
    pub right: GlyphId,
    pub value: i16,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kerning(pub Vec<Pair>);
impl Kerning {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn lookup(&self, left: GlyphId, right: GlyphId) -> Option<i16> {
        self.0
            .binary_search_by_key(&(left, right), |p| (p.left, p.right))
            .ok()
            .map(|i| self.0[i].value)
    }
    pub fn get(&self, left: GlyphId, right: GlyphId) -> i16 {
        self.0
            .binary_search_by_key(&(left, right), |p| (p.left, p.right))
            .ok()
            .map(|i| self.0[i].value)
            .unwrap_or(0)
    }
    pub fn validate(&self, doc: &FontDocument) -> Result<()> {
        if self.0.len() > MAX_PAIRS {
            return Err(Error::TooLarge);
        }
        let mut previous = None;
        for p in &self.0 {
            let key = (p.left, p.right);
            if p.left == 0
                || p.right == 0
                || !doc.glyphs.contains_key(&p.left)
                || !doc.glyphs.contains_key(&p.right)
                || previous.is_some_and(|v| v >= key)
            {
                return Err(Error::InvalidDocument);
            }
            previous = Some(key);
        }
        Ok(())
    }
    pub fn subset(&self, ids: &BTreeSet<GlyphId>) -> Self {
        Self(
            self.0
                .iter()
                .filter(|p| ids.contains(&p.left) && ids.contains(&p.right))
                .copied()
                .collect(),
        )
    }
}
impl Editor {
    pub fn set_kerning(
        &mut self,
        expected: u64,
        left: GlyphId,
        right: GlyphId,
        value: i16,
    ) -> Result<u64> {
        self.write_kerning(
            expected,
            left,
            right,
            if value == 0 { None } else { Some(value) },
        )
    }
    pub fn zero_kerning_exception(
        &mut self,
        expected: u64,
        left: GlyphId,
        right: GlyphId,
    ) -> Result<u64> {
        self.write_kerning(expected, left, right, Some(0))
    }
    fn write_kerning(
        &mut self,
        expected: u64,
        left: GlyphId,
        right: GlyphId,
        value: Option<i16>,
    ) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        if left == 0
            || right == 0
            || !self.document.glyphs.contains_key(&left)
            || !self.document.glyphs.contains_key(&right)
        {
            return Err(Error::MissingGlyph);
        }
        let before = self.document.kerning.clone();
        let mut after = before.clone();
        match after
            .0
            .binary_search_by_key(&(left, right), |p| (p.left, p.right))
        {
            Ok(i) => {
                if let Some(value) = value {
                    after.0[i].value = value;
                } else {
                    after.0.remove(i);
                }
            }
            Err(i) => {
                if let Some(value) = value {
                    after.0.insert(i, Pair { left, right, value });
                }
            }
        }
        after.validate(&self.document)?;
        if before == after {
            return Ok(self.revision);
        }
        self.document.kerning = after.clone();
        self.undo.push_back(Change {
            before_kerning: Some(before),
            after_kerning: Some(after),
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
#[path = "kerning_gpos.rs"]
mod gpos;
pub use gpos::{decode as read_gpos, encode as gpos};
