//! Stable project-local definitions with validated materialized instances.
//! Materialization is a document invariant: drawing, proof and every exporter see the same outlines.
use crate::{
    identity::{self, Uuid},
    snippets::{self, Info, Snippet},
    Change, Contour, Editor, Error, FontDocument, Glyph, GlyphId, GlyphStatus, Point, Result,
    Segment,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub info: Info,
    pub upm: u16,
    pub contours: Arc<Vec<Contour>>,
    pub source_glyph: GlyphId,
}
impl Definition {
    pub fn snippet(&self) -> Snippet {
        Snippet {
            info: self.info.clone(),
            upm: self.upm,
            contours: self.contours.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub definition: Uuid,
    pub glyph: GlyphId,
    pub matrix: [f64; 6],
    pub nodes: Arc<BTreeMap<Uuid, Uuid>>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub definitions: BTreeMap<Uuid, Definition>,
    pub instances: BTreeMap<Uuid, Instance>,
}
impl Library {
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.instances.is_empty()
    }
    pub fn linked_contours(&self, glyph: GlyphId) -> BTreeSet<Uuid> {
        self.instances
            .values()
            .filter(|i| i.glyph == glyph)
            .flat_map(|i| {
                self.definitions
                    .get(&i.definition)
                    .into_iter()
                    .flat_map(move |d| {
                        d.contours.iter().filter_map(move |c| {
                            c.identity
                                .contours
                                .first()
                                .and_then(|id| i.nodes.get(id))
                                .copied()
                        })
                    })
            })
            .collect()
    }
    pub fn protect_glyph(&self, old: &Glyph, next: &Glyph) -> Result<()> {
        let linked = self.linked_contours(old.id);
        if linked.is_empty() {
            return Ok(());
        }
        let actual: BTreeMap<_, _> = next
            .contours
            .iter()
            .filter_map(|c| c.identity.contours.first().map(|id| (*id, c)))
            .collect();
        for c in &old.contours {
            if c.identity
                .contours
                .first()
                .is_some_and(|id| linked.contains(id) && actual.get(id).copied() != Some(c))
            {
                return Err(Error::ComponentLinked);
            }
        }
        Ok(())
    }
    pub fn linked(&self, glyph: GlyphId, contour: Uuid) -> bool {
        self.instances
            .values()
            .filter(|i| i.glyph == glyph)
            .any(|i| {
                self.definitions.get(&i.definition).is_some_and(|d| {
                    d.contours.iter().any(|c| {
                        c.identity.contours.first().and_then(|id| i.nodes.get(id)) == Some(&contour)
                    })
                })
            })
    }
    pub fn check_glyph(&self, glyph: &Glyph) -> Result<()> {
        let actual: BTreeMap<_, _> = glyph
            .contours
            .iter()
            .filter_map(|c| c.identity.contours.first().map(|id| (*id, c)))
            .collect();
        for i in self.instances.values().filter(|i| i.glyph == glyph.id) {
            let d = self
                .definitions
                .get(&i.definition)
                .ok_or(Error::InvalidDocument)?;
            for expected in materialize(d, i)? {
                let id = expected.identity.contours[0];
                if actual.get(&id).copied() != Some(&expected) {
                    return Err(Error::ComponentLinked);
                }
            }
        }
        Ok(())
    }
    pub fn validate(&self, doc: &FontDocument) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        if self.definitions.len() > 256
            || self.instances.len() > 4096
            || self
                .definitions
                .values()
                .map(|d| {
                    d.contours
                        .iter()
                        .map(|c| c.segments.len() + 1)
                        .sum::<usize>()
                })
                .sum::<usize>()
                > 65536
            || self
                .instances
                .values()
                .map(|i| i.nodes.len())
                .sum::<usize>()
                > 1_000_000
        {
            return Err(Error::TooLarge);
        }
        let mut ids: BTreeSet<_> = std::iter::once(doc.document_uuid).collect();
        for glyph in doc.glyphs.values() {
            for c in &glyph.contours {
                ids.extend(
                    c.identity
                        .contours
                        .iter()
                        .chain(c.identity.anchors.iter().flatten())
                        .chain(c.identity.controls.iter().flatten().flatten())
                        .copied(),
                );
            }
        }
        for (&id, d) in &self.definitions {
            if id.is_nil() || !ids.insert(id) {
                return Err(Error::InvalidDocument);
            }
            d.snippet().validate()?;
            let mut local = BTreeSet::new();
            for c in d.contours.iter() {
                identity::validate_contour(c)?;
                if c.identity.is_empty() {
                    return Err(Error::InvalidDocument);
                }
                for id in contour_ids(c) {
                    if id.is_nil() || !local.insert(*id) {
                        return Err(Error::InvalidDocument);
                    }
                }
            }
        }
        for &id in self.instances.keys() {
            if id.is_nil() || !ids.insert(id) {
                return Err(Error::InvalidDocument);
            }
        }
        let mut owned = BTreeSet::new();
        for (&id, i) in &self.instances {
            if id.is_nil() || i.nodes.len() > 65536 || !doc.glyphs.contains_key(&i.glyph) {
                return Err(Error::InvalidDocument);
            }
            let d = self
                .definitions
                .get(&i.definition)
                .ok_or(Error::InvalidDocument)?;
            let live: BTreeSet<_> = d.contours.iter().flat_map(contour_ids).copied().collect();
            let mut mapped = BTreeSet::new();
            for (&source, &target) in i.nodes.iter() {
                if source.is_nil()
                    || target.is_nil()
                    || !mapped.insert(target)
                    || !owned.insert(target)
                {
                    return Err(Error::InvalidDocument);
                }
                if !live.contains(&source) && ids.contains(&target) {
                    return Err(Error::InvalidDocument);
                }
            }
        }
        let glyphs: BTreeSet<_> = self.instances.values().map(|i| i.glyph).collect();
        for id in glyphs {
            self.check_glyph(&doc.glyphs[&id]).map_err(|e| {
                if e == Error::ComponentLinked {
                    Error::InvalidDocument
                } else {
                    e
                }
            })?;
        }
        Ok(())
    }
}
fn contour_ids(c: &Contour) -> impl Iterator<Item = &Uuid> {
    c.identity
        .contours
        .iter()
        .chain(c.identity.anchors.iter().flatten())
        .chain(c.identity.controls.iter().flatten().flatten())
}
fn map_ids(c: &mut Contour, mut f: impl FnMut(Uuid) -> Result<Uuid>) -> Result<()> {
    for id in c
        .identity
        .contours
        .iter_mut()
        .chain(c.identity.anchors.iter_mut().flatten())
        .chain(c.identity.controls.iter_mut().flatten().flatten())
    {
        *id = f(*id)?;
    }
    Ok(())
}
fn materialize(d: &Definition, i: &Instance) -> Result<Vec<Contour>> {
    let [a, b, c, e, x, y] = i.matrix;
    if !i.matrix.iter().all(|v| v.is_finite()) || (a * e - b * c).abs() < 1e-12 {
        return Err(Error::InvalidGeometry);
    }
    let point = |p: Point| [a * p[0] + c * p[1] + x, b * p[0] + e * p[1] + y];
    let mut out = d.contours.as_ref().clone();
    for contour in &mut out {
        contour.start = point(contour.start);
        for s in &mut contour.segments {
            match s {
                Segment::Line { end } => *end = point(*end),
                Segment::Quadratic { control, end } => {
                    *control = point(*control);
                    *end = point(*end);
                }
                Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => {
                    *control1 = point(*control1);
                    *control2 = point(*control2);
                    *end = point(*end);
                }
            }
        }
        map_ids(contour, |id| {
            i.nodes.get(&id).copied().ok_or(Error::InvalidDocument)
        })?;
        crate::geometry::validate(contour)?;
    }
    Ok(out)
}
pub fn matrix(d: &Definition, p: &crate::reference_images::ReferenceImage) -> Result<[f64; 6]> {
    p.validate()?;
    let (min, max) = snippets::bounds(&d.contours)?;
    let (sin, cos) = p.angle.to_radians().sin_cos();
    let sx = p.width / (max[0] - min[0]);
    let sy = p.height / (max[1] - min[1]);
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    Ok([
        sx * cos,
        sx * sin,
        -sy * sin,
        sy * cos,
        p.center[0] - sx * cos * center[0] + sy * sin * center[1],
        p.center[1] - sx * sin * center[0] - sy * cos * center[1],
    ])
}
pub fn placement(d: &Definition, i: &Instance) -> Result<crate::reference_images::ReferenceImage> {
    let (min, max) = snippets::bounds(&d.contours)?;
    let [a, b, c, e, x, y] = i.matrix;
    let center = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
    let mut p = d.snippet().placement(
        d.upm,
        [
            a * center[0] + c * center[1] + x,
            b * center[0] + e * center[1] + y,
        ],
    )?;
    p.width = (max[0] - min[0]) * a.hypot(b);
    p.height = (max[1] - min[1]) * c.hypot(e);
    p.angle = b.atan2(a).to_degrees();
    Ok(p)
}
fn source(doc: &FontDocument, glyph: GlyphId, selection: &BTreeSet<usize>) -> Result<Vec<Contour>> {
    if selection.is_empty() {
        return Err(Error::InvalidGeometry);
    }
    let g = doc.glyphs.get(&glyph).ok_or(Error::MissingGlyph)?;
    let linked = doc.components.linked_contours(glyph);
    selection
        .iter()
        .map(|&n| {
            let c = g.contours.get(n).ok_or(Error::InvalidGeometry)?;
            if c.identity
                .contours
                .first()
                .is_some_and(|&id| linked.contains(&id))
            {
                return Err(Error::ComponentLinked);
            }
            Ok(c.clone())
        })
        .collect()
}
impl Editor {
    fn commit_components(&mut self, expected: u64, mut doc: FontDocument) -> Result<()> {
        let revision = self.check_revision(expected)?;
        doc.schema_version = 3;
        doc.validate()?;
        if doc.components == self.document.components && doc.glyphs == self.document.glyphs {
            return Ok(());
        }
        let before: Vec<_> = self
            .document
            .glyphs
            .values()
            .filter(|g| doc.glyphs.get(&g.id) != Some(*g))
            .cloned()
            .collect();
        let after: Vec<_> = before.iter().map(|g| doc.glyphs[&g.id].clone()).collect();
        let change = Change {
            before,
            after,
            before_schema: Some(self.document.schema_version),
            after_schema: Some(doc.schema_version),
            before_components: Some(self.document.components.clone()),
            after_components: Some(doc.components.clone()),
            ..Default::default()
        };
        self.document = doc;
        self.undo.push_back(change);
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(())
    }
    pub fn capture_component(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        selection: &BTreeSet<usize>,
        info: Info,
    ) -> Result<Uuid> {
        self.check_revision(expected)?;
        let contours = source(&self.document, glyph, selection)?;
        let id = Uuid::new_v4();
        let mut doc = self.document.clone();
        doc.components.definitions.insert(
            id,
            Definition {
                info,
                upm: doc.units_per_em,
                contours: Arc::new(contours),
                source_glyph: glyph,
            },
        );
        self.commit_components(expected, doc)?;
        Ok(id)
    }
    pub fn rename_component(&mut self, expected: u64, id: Uuid, info: Info) -> Result<()> {
        self.check_revision(expected)?;
        let mut doc = self.document.clone();
        doc.components
            .definitions
            .get_mut(&id)
            .ok_or(Error::InvalidDocument)?
            .info = info;
        self.commit_components(expected, doc)
    }
    pub fn place_component(
        &mut self,
        expected: u64,
        definition: Uuid,
        glyph: GlyphId,
        matrix: [f64; 6],
    ) -> Result<Uuid> {
        self.check_revision(expected)?;
        let mut doc = self.document.clone();
        let d = doc
            .components
            .definitions
            .get(&definition)
            .ok_or(Error::InvalidDocument)?;
        let id = Uuid::new_v4();
        let mut instance = Instance {
            definition,
            glyph,
            matrix,
            nodes: Default::default(),
        };
        let contours = regenerate(d, &mut instance)?;
        let g = doc.glyphs.get_mut(&glyph).ok_or(Error::MissingGlyph)?;
        g.contours.extend(contours);
        g.status = GlyphStatus::Draft;
        doc.components.instances.insert(id, instance);
        self.commit_components(expected, doc)?;
        Ok(id)
    }
    pub fn update_component(
        &mut self,
        expected: u64,
        id: Uuid,
        glyph: GlyphId,
        selection: &BTreeSet<usize>,
    ) -> Result<()> {
        self.check_revision(expected)?;
        let contours = source(&self.document, glyph, selection)?;
        let mut doc = self.document.clone();
        let old = doc
            .components
            .definitions
            .get(&id)
            .ok_or(Error::InvalidDocument)?
            .clone();
        let next = Definition {
            contours: Arc::new(contours),
            source_glyph: glyph,
            ..old.clone()
        };
        if next == old {
            return Ok(());
        }
        for instance in doc
            .components
            .instances
            .values_mut()
            .filter(|i| i.definition == id)
        {
            let g = doc
                .glyphs
                .get_mut(&instance.glyph)
                .ok_or(Error::MissingGlyph)?;
            replace_instance(g, &old, &next, instance)?;
        }
        doc.components.definitions.insert(id, next);
        self.commit_components(expected, doc)
    }
    pub fn transform_component(&mut self, expected: u64, id: Uuid, matrix: [f64; 6]) -> Result<()> {
        self.check_revision(expected)?;
        if self
            .document
            .components
            .instances
            .get(&id)
            .ok_or(Error::InvalidDocument)?
            .matrix
            == matrix
        {
            return Ok(());
        }
        let mut doc = self.document.clone();
        let i = doc
            .components
            .instances
            .get_mut(&id)
            .ok_or(Error::InvalidDocument)?;
        let d = doc
            .components
            .definitions
            .get(&i.definition)
            .ok_or(Error::InvalidDocument)?;
        let old = i.clone();
        i.matrix = matrix;
        let g = doc.glyphs.get_mut(&i.glyph).ok_or(Error::MissingGlyph)?;
        let old_contours = materialize(d, &old)?;
        let replacement = regenerate(d, i)?;
        replace_contours(g, &old_contours, replacement)?;
        self.commit_components(expected, doc)
    }
    pub fn detach_component(&mut self, expected: u64, id: Uuid) -> Result<()> {
        self.check_revision(expected)?;
        let mut doc = self.document.clone();
        doc.components
            .instances
            .remove(&id)
            .ok_or(Error::InvalidDocument)?;
        self.commit_components(expected, doc)
    }
    pub fn remove_component(&mut self, expected: u64, id: Uuid) -> Result<()> {
        self.check_revision(expected)?;
        let mut doc = self.document.clone();
        if doc
            .components
            .instances
            .values()
            .any(|i| i.definition == id)
        {
            return Err(Error::ComponentInUse);
        }
        doc.components
            .definitions
            .remove(&id)
            .ok_or(Error::InvalidDocument)?;
        self.commit_components(expected, doc)
    }
}
fn regenerate(d: &Definition, i: &mut Instance) -> Result<Vec<Contour>> {
    for id in d.contours.iter().flat_map(contour_ids) {
        Arc::make_mut(&mut i.nodes)
            .entry(*id)
            .or_insert_with(Uuid::new_v4);
    }
    materialize(d, i)
}
fn replace_instance(
    g: &mut Glyph,
    old: &Definition,
    new: &Definition,
    i: &mut Instance,
) -> Result<()> {
    let previous = materialize(old, i)?;
    let next = regenerate(new, i)?;
    replace_contours(g, &previous, next)
}
fn replace_contours(g: &mut Glyph, old: &[Contour], next: Vec<Contour>) -> Result<()> {
    let ids: BTreeSet<_> = old.iter().map(|c| c.identity.contours[0]).collect();
    let at = g
        .contours
        .iter()
        .position(|c| ids.contains(&c.identity.contours[0]))
        .ok_or(Error::InvalidDocument)?;
    g.contours
        .retain(|c| !ids.contains(&c.identity.contours[0]));
    g.contours.splice(at..at, next);
    g.status = GlyphStatus::Draft;
    Ok(())
}
