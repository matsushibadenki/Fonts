//! Explicit horizontal glyph pairs in font units. Zero removes the pair.
use crate::{Change, Editor, Error, FontDocument, GlyphId, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
            if p.value == 0
                || p.left == 0
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
                if value == 0 {
                    after.0.remove(i);
                } else {
                    after.0[i].value = value;
                }
            }
            Err(i) => {
                if value != 0 {
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
fn u16b(v: &mut Vec<u8>, n: u16) {
    v.extend(n.to_be_bytes());
}
fn set(v: &mut [u8], i: usize, n: u16) {
    v[i..i + 2].copy_from_slice(&n.to_be_bytes());
}
fn word(v: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(v.get(i..i + 2)?.try_into().ok()?))
}
/// GPOS 1.0 / kern feature / PairPos format 1, Value1.xAdvance only.
pub fn gpos(doc: &FontDocument) -> Result<Option<Vec<u8>>> {
    if doc.kerning.is_empty() {
        return Ok(None);
    }
    doc.kerning.validate(doc)?;
    let dense: BTreeMap<_, _> = doc
        .glyphs
        .keys()
        .enumerate()
        .map(|(i, id)| (*id, i as u16))
        .collect();
    let mut groups: BTreeMap<u16, Vec<(u16, i16)>> = BTreeMap::new();
    for p in &doc.kerning.0 {
        groups
            .entry(dense[&p.left])
            .or_default()
            .push((dense[&p.right], p.value));
    }
    let mut sub = vec![];
    for n in [1, 0, 4, 0, groups.len() as u16] {
        u16b(&mut sub, n);
    }
    sub.resize(10 + 2 * groups.len(), 0);
    for (i, pairs) in groups.values().enumerate() {
        let offset = u16::try_from(sub.len()).map_err(|_| Error::TooLarge)?;
        set(&mut sub, 10 + 2 * i, offset);
        u16b(&mut sub, pairs.len() as u16);
        for (right, value) in pairs {
            u16b(&mut sub, *right);
            u16b(&mut sub, *value as u16);
        }
    }
    let coverage = u16::try_from(sub.len()).map_err(|_| Error::TooLarge)?;
    set(&mut sub, 2, coverage);
    u16b(&mut sub, 1);
    u16b(&mut sub, groups.len() as u16);
    for left in groups.keys() {
        u16b(&mut sub, *left);
    }
    let mut out = vec![];
    for n in [1, 0, 10, 30, 44, 1] {
        u16b(&mut out, n);
    }
    out.extend(b"DFLT");
    for n in [8, 4, 0, 0, 0xffff, 1, 0, 1] {
        u16b(&mut out, n);
    }
    out.extend(b"kern");
    for n in [8, 0, 1, 0, 1, 4, 2, 0, 1, 8] {
        u16b(&mut out, n);
    }
    out.extend(sub);
    Ok(Some(out))
}
/// Read our deliberately restricted global pair layout; arbitrary imported shaping is not flattened.
pub fn read_gpos(data: &[u8]) -> Option<Kerning> {
    if data.get(..10)? != [0, 1, 0, 0, 0, 10, 0, 30, 0, 44]
        || data.get(12..16)? != b"DFLT"
        || data.get(32..36)? != b"kern"
        || data.get(44..56)? != [0, 1, 0, 4, 0, 2, 0, 0, 0, 1, 0, 8]
    {
        return None;
    }
    let sub = data.get(56..)?;
    if word(sub, 0)? != 1 || word(sub, 4)? != 4 || word(sub, 6)? != 0 {
        return None;
    }
    let coverage = usize::from(word(sub, 2)?);
    let count = usize::from(word(sub, 8)?);
    if count > MAX_PAIRS
        || word(sub, coverage)? != 1
        || usize::from(word(sub, coverage + 2)?) != count
    {
        return None;
    }
    let mut pairs = vec![];
    for i in 0..count {
        let left = u32::from(word(sub, coverage + 4 + i * 2)?);
        let offset = usize::from(word(sub, 10 + i * 2)?);
        let n = usize::from(word(sub, offset)?);
        if pairs.len() + n > MAX_PAIRS {
            return None;
        }
        for j in 0..n {
            let right = u32::from(word(sub, offset + 2 + j * 4)?);
            let value = word(sub, offset + 4 + j * 4)? as i16;
            if value != 0 {
                pairs.push(Pair { left, right, value });
            }
        }
    }
    Some(Kerning(pairs))
}
