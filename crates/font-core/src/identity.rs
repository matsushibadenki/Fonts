//! Persistent identities. Geometry addresses are transient; references use document/node UUIDs.
use crate::{
    geometry::{self, EditPoint},
    Contour, Error, FontDocument, GlyphId, Result, Segment,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
pub use uuid::Uuid;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identities {
    /// First UUID is primary; aliases keep merged contour references valid.
    pub contours: Vec<Uuid>,
    /// Explicit duplicate closure endpoints use an empty slot and resolve to anchor zero.
    pub anchors: Vec<Vec<Uuid>>,
    pub controls: Vec<[Option<Uuid>; 2]>,
}
impl Identities {
    pub fn is_empty(&self) -> bool {
        self.contours.is_empty() && self.anchors.is_empty() && self.controls.is_empty()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    pub document: Uuid,
    pub node: Uuid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address {
    pub glyph: GlyphId,
    pub contour: usize,
    pub point: EditPoint,
}
fn handle_count(s: &Segment) -> usize {
    match s {
        Segment::Line { .. } => 0,
        Segment::Quadratic { .. } => 1,
        Segment::Cubic { .. } => 2,
    }
}
pub fn ensure_contour(c: &mut Contour) -> Result<bool> {
    let mut changed = false;
    if c.identity.is_empty() {
        changed = true;
        c.identity = Identities {
            contours: vec![Uuid::new_v4()],
            anchors: vec![Vec::new(); c.segments.len() + 1],
            controls: vec![[None; 2]; c.segments.len()],
        };
    }
    if c.identity.anchors.len() != c.segments.len() + 1
        || c.identity.controls.len() != c.segments.len()
    {
        return Err(Error::InvalidGeometry);
    }
    if c.identity.contours.is_empty() {
        changed = true;
        c.identity.contours.push(Uuid::new_v4());
    }
    for i in 0..c.identity.anchors.len() {
        if geometry::canonical_anchor(c, i) != i {
            if !c.identity.anchors[i].is_empty() {
                return Err(Error::InvalidGeometry);
            }
        } else if c.identity.anchors[i].is_empty() {
            changed = true;
            c.identity.anchors[i].push(Uuid::new_v4());
        }
    }
    for (s, ids) in c.segments.iter().zip(&mut c.identity.controls) {
        for (i, id) in ids.iter_mut().enumerate() {
            if i < handle_count(s) {
                if id.is_none() {
                    changed = true;
                    *id = Some(Uuid::new_v4());
                }
            } else if id.is_some() {
                return Err(Error::InvalidGeometry);
            }
        }
    }
    Ok(changed)
}
pub fn validate_contour(c: &Contour) -> Result<()> {
    if c.identity.is_empty() {
        return Ok(());
    }
    if c.identity.contours.is_empty()
        || c.identity.contours.len() > 1024
        || c.identity.anchors.len() != c.segments.len() + 1
        || c.identity.controls.len() != c.segments.len()
    {
        return Err(Error::InvalidGeometry);
    }
    for (i, ids) in c.identity.anchors.iter().enumerate() {
        if ids.len() > 1024 || (geometry::canonical_anchor(c, i) == i) == ids.is_empty() {
            return Err(Error::InvalidGeometry);
        }
    }
    for (s, ids) in c.segments.iter().zip(&c.identity.controls) {
        for (i, id) in ids.iter().enumerate() {
            if id.is_some() != (i < handle_count(s)) {
                return Err(Error::InvalidGeometry);
            }
        }
    }
    Ok(())
}
pub fn validate_document(doc: &FontDocument) -> Result<()> {
    let mut seen = HashSet::new();
    if !doc.document_uuid.is_nil() {
        seen.insert(doc.document_uuid);
    }
    for g in doc.glyphs.values() {
        for c in &g.contours {
            validate_contour(c)?;
            for id in c
                .identity
                .contours
                .iter()
                .chain(c.identity.anchors.iter().flatten())
                .chain(c.identity.controls.iter().flatten().flatten())
            {
                if id.is_nil() || !seen.insert(*id) {
                    return Err(Error::InvalidDocument);
                }
            }
        }
    }
    Ok(())
}
pub fn upgrade(doc: &mut FontDocument) -> Result<bool> {
    if !matches!(doc.schema_version, 1..=3) {
        return Err(Error::UnsupportedVersion);
    }
    let mut changed = doc.schema_version < 2
        || doc.document_uuid.is_nil()
        || doc
            .glyphs
            .values()
            .any(|g| g.contours.iter().any(|c| c.identity.is_empty()));
    if doc.document_uuid.is_nil() {
        doc.document_uuid = Uuid::new_v4();
    }
    for g in doc.glyphs.values_mut() {
        for c in &mut g.contours {
            changed |= ensure_contour(c)?;
        }
    }
    let version = if doc.components.is_empty() { 2 } else { 3 };
    changed |= doc.schema_version < version;
    doc.schema_version = doc.schema_version.max(version);
    validate_document(doc)?;
    Ok(changed)
}
pub fn node_id(c: &Contour, p: EditPoint) -> Option<Uuid> {
    match p {
        EditPoint::Anchor(i) => c
            .identity
            .anchors
            .get(geometry::canonical_anchor(c, i))?
            .first()
            .copied(),
        EditPoint::Control1(i) => c.identity.controls.get(i)?[0],
        EditPoint::Control2(i) => c.identity.controls.get(i)?[1],
    }
}
pub fn resolve(doc: &FontDocument, reference: NodeRef) -> Option<Address> {
    if reference.document != doc.document_uuid || reference.node.is_nil() {
        return None;
    }
    for (&glyph, g) in &doc.glyphs {
        for (contour, c) in g.contours.iter().enumerate() {
            for (i, ids) in c.identity.anchors.iter().enumerate() {
                if ids.contains(&reference.node) {
                    return Some(Address {
                        glyph,
                        contour,
                        point: EditPoint::Anchor(i),
                    });
                }
            }
            for (i, ids) in c.identity.controls.iter().enumerate() {
                for (k, id) in ids.iter().enumerate() {
                    if *id == Some(reference.node) {
                        return Some(Address {
                            glyph,
                            contour,
                            point: if k == 0 {
                                EditPoint::Control1(i)
                            } else {
                                EditPoint::Control2(i)
                            },
                        });
                    }
                }
            }
        }
    }
    None
}
pub fn resolve_contour(doc: &FontDocument, id: Uuid) -> Option<(GlyphId, usize)> {
    doc.glyphs.iter().find_map(|(&g, glyph)| {
        glyph
            .contours
            .iter()
            .position(|c| c.identity.contours.contains(&id))
            .map(|c| (g, c))
    })
}
/// Adjust the identity of an explicit duplicate closure endpoint when changing open/closed mode.
pub fn closure(c: &mut Contour) {
    if c.identity.is_empty() {
        return;
    }
    let n = c.segments.len();
    if n == 0 {
        return;
    }
    if geometry::canonical_anchor(c, n) == 0 {
        let aliases = std::mem::take(&mut c.identity.anchors[n]);
        c.identity.anchors[0].extend(aliases);
    }
    // Opening leaves a new endpoint at the same location; its UUID is assigned on commit.
}
/// Materialize an implicit closing edge without inventing a second anchor identity.
pub fn append_closure(c: &mut Contour) {
    c.segments.push(Segment::Line { end: c.start });
    if !c.identity.is_empty() {
        c.identity.anchors.push(Vec::new());
        c.identity.controls.push([None; 2]);
    }
}

/// Make room for uncommitted IDs, retaining existing identities. Filling UUIDs happens at commit.
pub fn layout(c: &mut Contour) {
    if c.identity.is_empty() {
        c.identity.anchors = vec![Vec::new(); c.segments.len() + 1];
        c.identity.controls = vec![[None; 2]; c.segments.len()];
    }
}

pub fn validate_layout(c: &Contour) -> Result<()> {
    if c.identity.is_empty() {
        return Ok(());
    }
    if c.identity.anchors.len() != c.segments.len() + 1
        || c.identity.controls.len() != c.segments.len()
        || c.identity.contours.len() > 1024
        || c.identity.anchors.iter().any(|ids| ids.len() > 1024)
    {
        return Err(Error::InvalidGeometry);
    }
    Ok(())
}
fn ids(c: &Contour) -> impl Iterator<Item = &Uuid> {
    c.identity
        .contours
        .iter()
        .chain(c.identity.anchors.iter().flatten())
        .chain(c.identity.controls.iter().flatten().flatten())
}
pub fn validate_replacement(doc: &FontDocument, glyph: &crate::Glyph) -> Result<()> {
    let mut added = HashSet::new();
    for c in &glyph.contours {
        validate_contour(c)?;
        for id in ids(c) {
            if id.is_nil()
                || *id == doc.document_uuid
                || doc.components.definitions.contains_key(id)
                || doc.components.instances.contains_key(id)
                || !added.insert(*id)
            {
                return Err(Error::InvalidDocument);
            }
        }
    }
    if let Some(old) = doc.glyphs.get(&glyph.id) {
        for c in &old.contours {
            for id in ids(c) {
                added.remove(id);
            }
        }
    }
    if added.is_empty() {
        return Ok(());
    }
    if doc
        .components
        .instances
        .values()
        .any(|i| i.nodes.values().any(|id| added.contains(id)))
    {
        return Err(Error::InvalidDocument);
    }
    for other in doc.glyphs.values().filter(|g| g.id != glyph.id) {
        for c in &other.contours {
            if ids(c).any(|id| added.contains(id)) {
                return Err(Error::InvalidDocument);
            }
        }
    }
    Ok(())
}

pub fn resolve_in_glyph(doc: &FontDocument, glyph: GlyphId, reference: NodeRef) -> Option<Address> {
    if reference.document != doc.document_uuid || reference.node.is_nil() {
        return None;
    }
    let g = doc.glyphs.get(&glyph)?;
    for (contour, c) in g.contours.iter().enumerate() {
        for (i, ids) in c.identity.anchors.iter().enumerate() {
            if ids.contains(&reference.node) {
                return Some(Address {
                    glyph,
                    contour,
                    point: EditPoint::Anchor(i),
                });
            }
        }
        for (i, ids) in c.identity.controls.iter().enumerate() {
            for (k, id) in ids.iter().enumerate() {
                if *id == Some(reference.node) {
                    return Some(Address {
                        glyph,
                        contour,
                        point: if k == 0 {
                            EditPoint::Control1(i)
                        } else {
                            EditPoint::Control2(i)
                        },
                    });
                }
            }
        }
    }
    None
}
