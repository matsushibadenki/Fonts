//! Direct definition edits; source glyphs are provenance only.
use super::*;
impl Library {
    pub fn definition_linked_contours(&self, id: Uuid) -> BTreeSet<Uuid> {
        self.definitions
            .get(&id)
            .into_iter()
            .flat_map(|d| d.children.values())
            .flat_map(|child| {
                self.definitions
                    .get(&child.definition)
                    .into_iter()
                    .flat_map(move |source| {
                        source.contours.iter().filter_map(move |c| {
                            c.identity
                                .contours
                                .first()
                                .and_then(|id| child.nodes.get(id))
                                .copied()
                        })
                    })
            })
            .collect()
    }
    pub fn check_child_link(&self, parent: Uuid, definition: Uuid) -> Result<()> {
        if !self.definitions.contains_key(&definition) {
            return Err(Error::InvalidDocument);
        }
        let mut lib = self.clone();
        lib.definitions
            .get_mut(&parent)
            .ok_or(Error::InvalidDocument)?
            .children
            .insert(
                Uuid::new_v4(),
                Child {
                    definition,
                    matrix: [1., 0., 0., 1., 0., 0.],
                    nodes: Default::default(),
                    source_instance: None,
                },
            );
        lib.order().map(|_| ())
    }
}
impl Editor {
    pub fn edit_component_contours(
        &mut self,
        expected: u64,
        id: Uuid,
        mut contours: Vec<Contour>,
    ) -> Result<()> {
        self.check_revision(expected)?;
        if contours.is_empty() {
            return Err(Error::ComponentEmpty);
        }
        let old = self
            .document
            .components
            .definitions
            .get(&id)
            .ok_or(Error::InvalidDocument)?;
        let protected = self.document.components.definition_linked_contours(id);
        let actual: BTreeMap<_, _> = contours
            .iter()
            .filter_map(|c| c.identity.contours.first().map(|id| (*id, c)))
            .collect();
        for c in old.contours.iter() {
            if c.identity
                .contours
                .first()
                .is_some_and(|id| protected.contains(id) && actual.get(id).copied() != Some(c))
            {
                return Err(Error::ComponentLinked);
            }
        }
        for c in &mut contours {
            identity::ensure_contour(c)?;
        }
        if old.contours.as_ref() == &contours {
            return Ok(());
        }
        let mut doc = self.document.clone();
        doc.components.definitions.get_mut(&id).unwrap().contours = Arc::new(contours);
        nesting::propagate(&self.document.components, &mut doc)?;
        self.commit_components(expected, doc)
    }
    pub fn insert_component_child(
        &mut self,
        expected: u64,
        parent: Uuid,
        definition: Uuid,
        matrix: [f64; 6],
    ) -> Result<Uuid> {
        self.check_revision(expected)?;
        self.document
            .components
            .check_child_link(parent, definition)?;
        let mut doc = self.document.clone();
        let source = doc.components.definitions[&definition].clone();
        let mut projected = Instance {
            definition,
            glyph: 0,
            matrix,
            nodes: Default::default(),
        };
        let contours = regenerate(&source, &mut projected)?;
        let id = Uuid::new_v4();
        let d = doc.components.definitions.get_mut(&parent).unwrap();
        Arc::make_mut(&mut d.contours).extend(contours);
        d.children.insert(
            id,
            Child {
                definition,
                matrix,
                nodes: projected.nodes,
                source_instance: None,
            },
        );
        nesting::propagate(&self.document.components, &mut doc)?;
        self.commit_components(expected, doc)?;
        Ok(id)
    }
}
