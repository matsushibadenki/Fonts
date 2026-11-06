//! Bounded DAG evaluation. Definition contours are validated projections, never a second source.
use super::*;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Child {
    pub definition: Uuid,
    pub matrix: [f64; 6],
    pub nodes: Arc<BTreeMap<Uuid, Uuid>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_instance: Option<Uuid>,
}
impl Child {
    pub fn instance(&self) -> Instance {
        Instance {
            definition: self.definition,
            glyph: 0,
            matrix: self.matrix,
            nodes: self.nodes.clone(),
        }
    }
}
impl Library {
    pub fn has_nested(&self) -> bool {
        self.definitions.values().any(|d| !d.children.is_empty())
    }
    pub fn is_header(&self, id: &Uuid) -> bool {
        self.definitions.contains_key(id)
            || self.instances.contains_key(id)
            || self
                .definitions
                .values()
                .any(|d| d.children.contains_key(id))
    }
    pub fn order(&self) -> Result<Vec<Uuid>> {
        fn visit(
            lib: &Library,
            id: Uuid,
            marks: &mut BTreeMap<Uuid, u8>,
            depths: &mut BTreeMap<Uuid, usize>,
            out: &mut Vec<Uuid>,
        ) -> Result<usize> {
            match marks.get(&id) {
                Some(1) => return Err(Error::ComponentCycle),
                Some(2) => return Ok(depths[&id]),
                _ => {}
            }
            let d = lib.definitions.get(&id).ok_or(Error::InvalidDocument)?;
            marks.insert(id, 1);
            let mut depth = 1;
            for child in d.children.values() {
                depth = depth.max(visit(lib, child.definition, marks, depths, out)? + 1);
                if depth > 16 {
                    return Err(Error::ComponentDepth);
                }
            }
            depths.insert(id, depth);
            marks.insert(id, 2);
            out.push(id);
            Ok(depth)
        }
        if self.definitions.len() > 256
            || self
                .definitions
                .values()
                .map(|d| d.children.len())
                .sum::<usize>()
                > 4096
        {
            return Err(Error::TooLarge);
        }
        let mut marks = BTreeMap::new();
        let mut depths = BTreeMap::new();
        let mut out = vec![];
        for &id in self.definitions.keys() {
            visit(self, id, &mut marks, &mut depths, &mut out)?;
        }
        Ok(out)
    }
    pub(super) fn validate_nested(&self, doc: &FontDocument) -> Result<()> {
        self.order()?;
        let headers: BTreeSet<_> = self
            .definitions
            .keys()
            .chain(self.instances.keys())
            .chain(self.definitions.values().flat_map(|d| d.children.keys()))
            .copied()
            .chain([doc.document_uuid])
            .collect();
        let mut child_ids = BTreeSet::new();
        let root_ids: BTreeSet<_> = doc
            .glyphs
            .values()
            .flat_map(|g| g.contours.iter().flat_map(contour_ids))
            .copied()
            .collect();
        let mut total = self
            .instances
            .values()
            .map(|i| i.nodes.len())
            .sum::<usize>();
        for d in self.definitions.values() {
            let actual: BTreeMap<_, _> = d
                .contours
                .iter()
                .map(|c| (c.identity.contours.first().copied(), c))
                .collect();
            let local: BTreeSet<_> = d.contours.iter().flat_map(contour_ids).copied().collect();
            if !local.is_disjoint(&headers) {
                return Err(Error::InvalidDocument);
            }
            let mut owned = BTreeSet::new();
            for (&id, child) in &d.children {
                if id.is_nil()
                    || !child_ids.insert(id)
                    || self.definitions.contains_key(&id)
                    || self.instances.contains_key(&id)
                    || root_ids.contains(&id)
                    || id == doc.document_uuid
                    || child.nodes.len() > 65536
                {
                    return Err(Error::InvalidDocument);
                }
                total += child.nodes.len();
                if total > 1_000_000 {
                    return Err(Error::TooLarge);
                }
                let source = self
                    .definitions
                    .get(&child.definition)
                    .ok_or(Error::InvalidDocument)?;
                let live: BTreeSet<_> = source
                    .contours
                    .iter()
                    .flat_map(contour_ids)
                    .copied()
                    .collect();
                for (&from, &to) in child.nodes.iter() {
                    if from.is_nil()
                        || to.is_nil()
                        || !owned.insert(to)
                        || headers.contains(&to)
                        || (!live.contains(&from) && local.contains(&to))
                    {
                        return Err(Error::InvalidDocument);
                    }
                }
                for c in materialize(source, &child.instance())? {
                    if actual.get(&c.identity.contours.first().copied()).copied() != Some(&c) {
                        return Err(Error::InvalidDocument);
                    }
                }
            }
        }
        Ok(())
    }
}
type Source = (Vec<Contour>, BTreeMap<Uuid, Child>);
// Whole placed instances become child links. Arbitrary partial references would lose provenance.
pub(super) fn from_source(
    doc: &FontDocument,
    glyph: GlyphId,
    selection: &BTreeSet<usize>,
    old: Option<&Definition>,
) -> Result<Source> {
    if selection.is_empty() {
        return Err(Error::InvalidGeometry);
    }
    let g = doc.glyphs.get(&glyph).ok_or(Error::MissingGlyph)?;
    let mut ownership = BTreeMap::new();
    for (&id, i) in doc
        .components
        .instances
        .iter()
        .filter(|(_, i)| i.glyph == glyph)
    {
        let d = &doc.components.definitions[&i.definition];
        for c in d.contours.iter() {
            ownership.insert(i.nodes[&c.identity.contours[0]], id);
        }
    }
    let mut seen = BTreeSet::new();
    let mut contours = vec![];
    let mut children = BTreeMap::new();
    let selected_ids: BTreeSet<_> = selection
        .iter()
        .map(|&n| {
            g.contours
                .get(n)
                .and_then(|c| c.identity.contours.first())
                .copied()
                .ok_or(Error::InvalidGeometry)
        })
        .collect::<Result<_>>()?;
    for &n in selection {
        let c = &g.contours[n];
        if let Some(&source_id) = ownership.get(&c.identity.contours[0]) {
            if !seen.insert(source_id) {
                continue;
            }
            let i = &doc.components.instances[&source_id];
            let d = &doc.components.definitions[&i.definition];
            if d.contours
                .iter()
                .any(|c| !selected_ids.contains(&i.nodes[&c.identity.contours[0]]))
            {
                return Err(Error::ComponentPartial);
            }
            let previous = old.and_then(|d| {
                d.children
                    .iter()
                    .find(|(_, c)| c.source_instance == Some(source_id))
            });
            let id = previous.map_or_else(Uuid::new_v4, |(&id, _)| id);
            let mut child = Child {
                definition: i.definition,
                matrix: i.matrix,
                nodes: previous.map_or_else(|| i.nodes.clone(), |(_, c)| c.nodes.clone()),
                source_instance: Some(source_id),
            };
            let mut instance = child.instance();
            let projected = regenerate(d, &mut instance)?;
            child.nodes = instance.nodes;
            contours.extend(projected);
            children.insert(id, child);
        } else {
            contours.push(c.clone());
        }
    }
    Ok((contours, children))
}
// Evaluate children first, then refresh each affected glyph once as one Editor transaction.
pub(super) fn propagate(old: &Library, doc: &mut FontDocument) -> Result<()> {
    for id in doc.components.order()? {
        let mut d = doc.components.definitions[&id].clone();

        for child in d.children.values_mut() {
            let source = &doc.components.definitions[&child.definition];
            let before = old
                .definitions
                .get(&child.definition)
                .ok_or(Error::InvalidDocument)?;
            if before.contours == source.contours {
                continue;
            }
            let previous = materialize(before, &child.instance())?;
            let mut i = child.instance();
            let next = regenerate(source, &mut i)?;
            replace_cached(Arc::make_mut(&mut d.contours), &previous, next)?;
            child.nodes = i.nodes;
        }
        d.snippet().validate()?;
        doc.components.definitions.insert(id, d);
    }
    for instance in doc.components.instances.values_mut() {
        let before = old
            .definitions
            .get(&instance.definition)
            .ok_or(Error::InvalidDocument)?;
        let after = &doc.components.definitions[&instance.definition];
        if before.contours != after.contours {
            replace_instance(
                doc.glyphs
                    .get_mut(&instance.glyph)
                    .ok_or(Error::MissingGlyph)?,
                before,
                after,
                instance,
            )?;
        }
    }
    Ok(())
}
fn replace_cached(contours: &mut Vec<Contour>, old: &[Contour], next: Vec<Contour>) -> Result<()> {
    let ids: BTreeSet<_> = old.iter().map(|c| c.identity.contours[0]).collect();
    let remaining: Vec<_> = contours
        .iter()
        .filter(|c| !ids.contains(&c.identity.contours[0]))
        .collect();
    if remaining.len() + next.len() > 128
        || remaining
            .iter()
            .map(|c| c.segments.len() + 1)
            .chain(next.iter().map(|c| c.segments.len() + 1))
            .sum::<usize>()
            > 16384
    {
        return Err(Error::TooLarge);
    }
    let at = contours
        .iter()
        .position(|c| ids.contains(&c.identity.contours[0]))
        .ok_or(Error::InvalidDocument)?;
    contours.retain(|c| !ids.contains(&c.identity.contours[0]));
    contours.splice(at..at, next);
    Ok(())
}
impl Editor {
    pub fn detach_child(
        &mut self,
        expected: u64,
        parent: Uuid,
        id: Uuid,
        remove: bool,
    ) -> Result<()> {
        self.check_revision(expected)?;
        let mut doc = self.document.clone();
        let d = doc
            .components
            .definitions
            .get_mut(&parent)
            .ok_or(Error::InvalidDocument)?;
        let child = d.children.remove(&id).ok_or(Error::InvalidDocument)?;
        if remove {
            let source = self
                .document
                .components
                .definitions
                .get(&child.definition)
                .ok_or(Error::InvalidDocument)?;
            let previous = materialize(source, &child.instance())?;
            replace_cached(Arc::make_mut(&mut d.contours), &previous, vec![])?;
        }
        propagate(&self.document.components, &mut doc)?;
        self.commit_components(expected, doc)
    }
    pub fn transform_child(
        &mut self,
        expected: u64,
        parent: Uuid,
        id: Uuid,
        matrix: [f64; 6],
    ) -> Result<()> {
        self.check_revision(expected)?;
        let mut doc = self.document.clone();
        let source_id = doc
            .components
            .definitions
            .get(&parent)
            .and_then(|d| d.children.get(&id))
            .ok_or(Error::InvalidDocument)?
            .definition;
        let source = doc
            .components
            .definitions
            .get(&source_id)
            .ok_or(Error::InvalidDocument)?
            .clone();
        let d = doc.components.definitions.get_mut(&parent).unwrap();
        let child = d.children.get_mut(&id).unwrap();
        if child.matrix == matrix {
            return Ok(());
        }
        let old = materialize(&source, &child.instance())?;
        child.matrix = matrix;
        let next = materialize(&source, &child.instance())?;
        replace_cached(Arc::make_mut(&mut d.contours), &old, next)?;
        propagate(&self.document.components, &mut doc)?;
        self.commit_components(expected, doc)
    }
}
