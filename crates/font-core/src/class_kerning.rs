//! Disjoint left/right classes with stable document IDs; explicit glyph pairs win.
use crate::{kerning::Pair, Change, Editor, Error, FontDocument, GlyphId, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_CLASSES: usize = 64;
pub const MAX_MEMBERS: usize = 2048;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Class {
    pub id: u32,
    pub name: String,
    pub glyphs: BTreeSet<GlyphId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassKerning {
    pub next_id: u32,
    pub left: Vec<Class>,
    pub right: Vec<Class>,
    pub pairs: Vec<Pair>,
}
impl Default for ClassKerning {
    fn default() -> Self {
        Self {
            next_id: 1,
            left: vec![],
            right: vec![],
            pairs: vec![],
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Upsert {
        side: Side,
        id: Option<u32>,
        name: String,
        glyphs: BTreeSet<GlyphId>,
    },
    Remove {
        side: Side,
        id: u32,
    },
    Pair {
        left: u32,
        right: u32,
        value: i16,
    },
}
impl ClassKerning {
    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty() && self.pairs.is_empty()
    }
    pub fn classes(&self, side: Side) -> &[Class] {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
    fn classes_mut(&mut self, side: Side) -> &mut Vec<Class> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
    pub fn get(&self, left: GlyphId, right: GlyphId) -> i16 {
        let l = self.left.iter().find(|c| c.glyphs.contains(&left));
        let r = self.right.iter().find(|c| c.glyphs.contains(&right));
        l.zip(r)
            .and_then(|(l, r)| {
                self.pairs
                    .binary_search_by_key(&(l.id, r.id), |p| (p.left, p.right))
                    .ok()
            })
            .map(|i| self.pairs[i].value)
            .unwrap_or(0)
    }
    pub fn validate(&self, doc: &FontDocument) -> Result<()> {
        let mut ids = BTreeSet::new();
        for classes in [&self.left, &self.right] {
            if classes.len() > MAX_CLASSES {
                return Err(Error::TooLarge);
            }
            let mut members = BTreeSet::new();
            let mut names = BTreeSet::new();
            let mut last = 0;
            for c in classes {
                if c.id <= last
                    || c.id >= self.next_id
                    || !ids.insert(c.id)
                    || c.name.trim() != c.name
                    || c.name.is_empty()
                    || c.name.chars().count() > 80
                    || c.name.chars().any(char::is_control)
                    || !names.insert(&c.name)
                    || c.glyphs.is_empty()
                {
                    return Err(Error::InvalidDocument);
                }
                last = c.id;
                for id in &c.glyphs {
                    if *id == 0 || !doc.glyphs.contains_key(id) || !members.insert(*id) {
                        return Err(Error::InvalidDocument);
                    }
                }
            }
            if members.len() > MAX_MEMBERS {
                return Err(Error::TooLarge);
            }
        }
        if self.next_id == 0 || self.pairs.len() > crate::kerning::MAX_PAIRS {
            return Err(Error::TooLarge);
        }
        let mut last = None;
        for p in &self.pairs {
            let key = (p.left, p.right);
            if p.value == 0
                || last.is_some_and(|k| k >= key)
                || !self.left.iter().any(|c| c.id == p.left)
                || !self.right.iter().any(|c| c.id == p.right)
            {
                return Err(Error::InvalidDocument);
            }
            last = Some(key);
        }
        Ok(())
    }
    pub fn subset(&self, ids: &BTreeSet<GlyphId>) -> Self {
        let mut s = self.clone();
        for classes in [&mut s.left, &mut s.right] {
            for c in classes.iter_mut() {
                c.glyphs.retain(|id| ids.contains(id));
            }
            classes.retain(|c| !c.glyphs.is_empty());
        }
        s.pairs.retain(|p| {
            s.left.iter().any(|c| c.id == p.left) && s.right.iter().any(|c| c.id == p.right)
        });
        s
    }
}
impl FontDocument {
    pub fn kerning_value(&self, left: GlyphId, right: GlyphId) -> i16 {
        self.kerning
            .lookup(left, right)
            .unwrap_or_else(|| self.class_kerning.get(left, right))
    }
}
impl Editor {
    pub fn edit_class_kerning(&mut self, expected: u64, edit: Edit) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        let before = self.document.class_kerning.clone();
        let mut after = before.clone();
        match edit {
            Edit::Upsert {
                side,
                id,
                name,
                glyphs,
            } => {
                let name = name.trim().to_owned();
                let id = if let Some(id) = id {
                    if !after.classes(side).iter().any(|c| c.id == id) {
                        return Err(Error::InvalidDocument);
                    }
                    id
                } else {
                    let id = after.next_id;
                    after.next_id = id.checked_add(1).ok_or(Error::TooLarge)?;
                    id
                };
                let classes = after.classes_mut(side);
                let c = Class { id, name, glyphs };
                match classes.binary_search_by_key(&id, |c| c.id) {
                    Ok(i) => classes[i] = c,
                    Err(i) => classes.insert(i, c),
                }
            }
            Edit::Remove { side, id } => {
                let classes = after.classes_mut(side);
                let i = classes
                    .binary_search_by_key(&id, |c| c.id)
                    .map_err(|_| Error::InvalidDocument)?;
                classes.remove(i);
                after.pairs.retain(|p| match side {
                    Side::Left => p.left != id,
                    Side::Right => p.right != id,
                });
            }
            Edit::Pair { left, right, value } => {
                if !after.left.iter().any(|c| c.id == left)
                    || !after.right.iter().any(|c| c.id == right)
                {
                    return Err(Error::InvalidDocument);
                }
                match after
                    .pairs
                    .binary_search_by_key(&(left, right), |p| (p.left, p.right))
                {
                    Ok(i) => {
                        if value == 0 {
                            after.pairs.remove(i);
                        } else {
                            after.pairs[i].value = value;
                        }
                    }
                    Err(i) => {
                        if value != 0 {
                            after.pairs.insert(i, Pair { left, right, value });
                        }
                    }
                }
            }
        }
        after.validate(&self.document)?;
        if before == after {
            return Ok(self.revision);
        }
        self.document.class_kerning = after.clone();
        self.undo.push_back(Change {
            before_classes: Some(before),
            after_classes: Some(after),
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
