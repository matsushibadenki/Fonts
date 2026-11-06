//! Commands and small summaries; the document, instances and previews remain Rust-owned.
use crate::{
    model::{Action as ModelAction, Model},
    AppState,
};
use font_core::{identity::Uuid, snippets::Info};
use serde::{Deserialize, Serialize};
use tauri::{Manager, WebviewWindow};
#[derive(Serialize)]
pub struct Definition {
    id: Uuid,
    info: Info,
    uses: usize,
    source: bool,
    thumbnail: usize,
}
#[derive(Serialize)]
pub struct Instance {
    affected: bool,
    id: Uuid,
    definition: Uuid,
    glyph: u32,
    name: String,
}
#[derive(Serialize)]
pub struct Child {
    parent: Uuid,
    id: Uuid,
    definition: Uuid,
    name: String,
    placement: Option<font_core::reference_images::ReferenceImage>,
    can_remove: bool,
}
#[derive(Serialize)]
pub struct Summary {
    editing: Option<EditingSummary>,
    children: Vec<Child>,
    active: bool,
    selected: Option<Uuid>,
    definitions: Vec<Definition>,
    instances: Vec<Instance>,
}
#[derive(Serialize)]
pub struct EditingSummary {
    id: Uuid,
    name: String,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    EditDefinition {
        id: Uuid,
    },
    ExitEdit,
    Mode {
        active: bool,
    },
    Choose {
        id: Uuid,
    },
    Capture {
        info: Info,
    },
    Rename {
        id: Uuid,
        info: Info,
    },
    Update {
        id: Uuid,
    },
    Remove {
        id: Uuid,
    },
    Begin {
        id: Uuid,
    },
    Transform {
        id: Uuid,
    },
    Detach {
        id: Uuid,
    },
    Source {
        id: Uuid,
    },
    Usage {
        id: Uuid,
    },
    ChildDetach {
        parent: Uuid,
        id: Uuid,
        remove: bool,
    },
    ChildTransform {
        parent: Uuid,
        id: Uuid,
        placement: font_core::reference_images::ReferenceImage,
    },
}
impl Model {
    fn component_source_indices(&self, id: Uuid) -> std::collections::BTreeSet<usize> {
        let doc = self.editor.document();
        let Some(d) = doc.components.definitions.get(&id) else {
            return Default::default();
        };
        let Some(g) = doc.glyphs.get(&d.source_glyph) else {
            return Default::default();
        };
        let mut ids: std::collections::BTreeSet<_> = d
            .contours
            .iter()
            .filter_map(|c| c.identity.contours.first().copied())
            .collect();
        for child in d.children.values() {
            if let Some(i) = child
                .source_instance
                .and_then(|id| doc.components.instances.get(&id))
                .filter(|i| i.glyph == d.source_glyph)
            {
                for c in doc.components.definitions[&i.definition].contours.iter() {
                    if let Some(target) = c.identity.contours.first().and_then(|id| i.nodes.get(id))
                    {
                        ids.insert(*target);
                    }
                }
            }
        }
        g.contours
            .iter()
            .enumerate()
            .filter_map(|(n, c)| {
                c.identity
                    .contours
                    .first()
                    .is_some_and(|id| ids.contains(id))
                    .then_some(n)
            })
            .collect()
    }
    pub fn component_summary(&self) -> Summary {
        let library = &self.editor.document().components;
        let selected = self
            .snippets
            .component
            .filter(|id| library.definitions.contains_key(id))
            .or_else(|| library.definitions.keys().next().copied());
        let mut affected: std::collections::BTreeSet<_> = selected.into_iter().collect();
        for id in library.order().unwrap_or_default() {
            if library.definitions[&id]
                .children
                .values()
                .any(|c| affected.contains(&c.definition))
            {
                affected.insert(id);
            }
        }
        let mut uses = std::collections::BTreeMap::<Uuid, usize>::new();
        for id in library.instances.values().map(|i| i.definition).chain(
            library
                .definitions
                .values()
                .flat_map(|d| d.children.values().map(|c| c.definition)),
        ) {
            *uses.entry(id).or_default() += 1;
        }
        Summary {
            editing: self.editing_id().and_then(|id| {
                library.definitions.get(&id).map(|d| EditingSummary {
                    id,
                    name: d.info.name.clone(),
                })
            }),
            children: library
                .definitions
                .iter()
                .flat_map(|(&parent, d)| {
                    d.children.iter().map(move |(&id, c)| {
                        let source = &library.definitions[&c.definition];
                        let [a, b, x, y, _, _] = c.matrix;
                        let editable = selected == Some(parent)
                            && a * y - b * x > 0.
                            && (a * x + b * y).abs() <= 1e-8 * (a.hypot(b) * x.hypot(y)).max(1.);
                        Child {
                            parent,
                            id,
                            definition: c.definition,
                            name: source.info.name.clone(),
                            placement: editable
                                .then(|| {
                                    font_core::components::placement(source, &c.instance()).ok()
                                })
                                .flatten(),
                            can_remove: d.contours.len() > source.contours.len(),
                        }
                    })
                })
                .collect(),
            active: self.snippets.component_mode,
            selected,
            definitions: library
                .definitions
                .iter()
                .enumerate()
                .map(|(thumbnail, (&id, d))| Definition {
                    thumbnail,
                    id,
                    info: d.info.clone(),
                    uses: uses.get(&id).copied().unwrap_or(0),
                    source: !self.component_source_indices(id).is_empty(),
                })
                .collect(),
            instances: library
                .instances
                .iter()
                .map(|(&id, i)| Instance {
                    affected: affected.contains(&i.definition),
                    id,
                    definition: i.definition,
                    glyph: i.glyph,
                    name: self.editor.document().glyphs[&i.glyph].name.clone(),
                })
                .collect(),
        }
    }
    pub fn component_action(&mut self, action: Action, owner: &str) -> Result<(), String> {
        if self.has_draft() || self.proof.active {
            return Err("edit_in_progress".into());
        }
        if self.editing_id().is_some()
            && matches!(action, Action::Capture { .. } | Action::Update { .. })
        {
            return Err("component_edit_unavailable".into());
        }
        let selection: std::collections::BTreeSet<_> = self
            .snippets
            .contours
            .iter()
            .filter_map(|&n| {
                self.editing_glyph()
                    .contours
                    .get(n)
                    .and_then(|c| c.identity.contours.first().copied())
            })
            .collect();
        let revision = self.editor.revision();
        let result: font_core::Result<()> = match action {
            Action::EditDefinition { id } => return self.enter_component_edit(id),
            Action::ExitEdit => return self.exit_component_edit(),
            Action::Mode { active } => {
                self.snippets.component_mode = active;
                Ok(())
            }
            Action::Choose { id } => {
                if !self
                    .editor
                    .document()
                    .components
                    .definitions
                    .contains_key(&id)
                {
                    return Err("InvalidDocument".into());
                }
                self.snippets.component = Some(id);
                Ok(())
            }
            Action::Capture { info } => self
                .editor
                .capture_component(revision, self.glyph, &self.snippets.contours, info)
                .map(|id| self.snippets.component = Some(id)),
            Action::Rename { id, info } => self.editor.rename_component(revision, id, info),
            Action::Update { id } => {
                self.editor
                    .update_component(revision, id, self.glyph, &self.snippets.contours)
            }
            Action::Remove { id } => self.editor.remove_component(revision, id),
            Action::Begin { id } => return self.begin_component(id, None, owner),
            Action::Transform { id } => {
                let i = self
                    .editor
                    .document()
                    .components
                    .instances
                    .get(&id)
                    .ok_or("InvalidDocument")?
                    .clone();
                self.goto_component_glyph(i.glyph, i.definition)?;
                return self.begin_component(i.definition, Some(id), owner);
            }
            Action::Detach { id } => self.editor.detach_component(revision, id),
            Action::Usage { id } => {
                let i = self
                    .editor
                    .document()
                    .components
                    .instances
                    .get(&id)
                    .ok_or("InvalidDocument")?
                    .clone();
                return self.goto_component_glyph(i.glyph, i.definition);
            }
            Action::ChildDetach { parent, id, remove } => {
                self.editor.detach_child(revision, parent, id, remove)
            }
            Action::ChildTransform {
                parent,
                id,
                placement,
            } => {
                let lib = &self.editor.document().components;
                let child = lib
                    .definitions
                    .get(&parent)
                    .and_then(|d| d.children.get(&id))
                    .ok_or("InvalidDocument")?;
                let source = lib
                    .definitions
                    .get(&child.definition)
                    .ok_or("InvalidDocument")?;
                let [a, b, c, d, _, _] = child.matrix;
                if a * d - b * c <= 0.
                    || (a * c + b * d).abs() > 1e-8 * (a.hypot(b) * c.hypot(d)).max(1.)
                {
                    return Err("InvalidGeometry".into());
                }
                let current = font_core::components::placement(source, &child.instance())
                    .map_err(|e| e.to_string())?;
                if current == placement {
                    return Ok(());
                }
                let matrix =
                    font_core::components::matrix(source, &placement).map_err(|e| e.to_string())?;
                self.editor.transform_child(revision, parent, id, matrix)
            }
            Action::Source { id } => {
                let d = self
                    .editor
                    .document()
                    .components
                    .definitions
                    .get(&id)
                    .ok_or("InvalidDocument")?
                    .clone();
                self.goto_component_glyph(d.source_glyph, id)?;
                self.snippets.contours = self.component_source_indices(id);
                if self.snippets.contours.is_empty() {
                    return Err("component_source_missing".into());
                }
                return Ok(());
            }
        };
        result.map_err(|e| e.to_string())?;
        self.refresh_component_edit();
        self.selected = None;
        self.selected_nodes.clear();
        self.selected_controls.clear();
        self.snippets.contours = self
            .editing_glyph()
            .contours
            .iter()
            .enumerate()
            .filter_map(|(n, c)| {
                c.identity
                    .contours
                    .first()
                    .is_some_and(|id| selection.contains(id))
                    .then_some(n)
            })
            .collect();
        Ok(())
    }
    fn goto_component_glyph(&mut self, glyph: u32, definition: Uuid) -> Result<(), String> {
        self.action(ModelAction::SelectGlyph { id: glyph })?;
        self.snippets.component_mode = true;
        self.snippets.component = Some(definition);
        Ok(())
    }
}
#[tauri::command]
pub fn component_action(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    glyph: u32,
    action: Action,
) -> Result<(), String> {
    crate::allowed(&window)?;
    {
        let state = window.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision || m.glyph != glyph {
            return Err("Conflict".into());
        }
        if crate::updates::installing() {
            return Err("edit_in_progress".into());
        }
        m.component_action(action, window.label())?;
    }
    crate::publish(window.app_handle());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snippets::Action as SnippetAction;
    use font_core::{
        geometry::{self, EditPoint},
        identity::NodeRef,
        workspace::Workspace,
        FontDocument,
    };
    fn model() -> Model {
        let mut m = Model::new(Workspace::default());
        m.replace_document(FontDocument::new("Components", "AB").unwrap(), None)
            .unwrap();
        m.action(ModelAction::SelectGlyph { id: 1 }).unwrap();
        let mut g = m.editor.document().glyphs[&1].clone();
        g.contours = font_core::snippets::builtins()[&2]
            .contours
            .as_ref()
            .clone();
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        m.snippets.contours.insert(0);
        m.component_action(
            Action::Capture {
                info: Info {
                    name: "Part".into(),
                    tags: "".into(),
                    category: "radical".into(),
                    source: "".into(),
                    license: "".into(),
                    favorite: false,
                },
            },
            "main",
        )
        .unwrap();
        m
    }
    #[test]
    fn reference_preview_transform_cancel_commit_shared_update_history_and_recovery() {
        let mut m = model();
        let id = m.snippets.component.unwrap();
        m.action(ModelAction::SelectGlyph { id: 2 }).unwrap();
        let original = m.editor.document().clone();
        m.component_action(Action::Begin { id }, "main").unwrap();
        assert!(m.snippets.draft.as_ref().unwrap().reference.is_some());
        let placement = m.snippets.draft.as_ref().unwrap().placement.clone();
        let expected = m
            .snippets
            .draft
            .as_ref()
            .unwrap()
            .source
            .transformed(&placement)
            .unwrap();
        assert_eq!(m.visible_glyph().contours, expected);
        assert!(m.component_action(Action::Remove { id }, "main").is_err());
        m.snippet_action(SnippetAction::Cancel, "main").unwrap();
        assert_eq!(m.editor.document(), &original);
        m.component_action(Action::Begin { id }, "main").unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        let instance = *m
            .editor
            .document()
            .components
            .instances
            .keys()
            .next()
            .unwrap();
        let before = m.editor.document().clone();
        m.component_action(Action::Transform { id: instance }, "main")
            .unwrap();
        assert_eq!(
            m.visible_glyph().contours.len(),
            before.glyphs[&2].contours.len()
        );
        let mut p = m.snippets.draft.as_ref().unwrap().placement.clone();
        p.center[0] += 30.;
        p.angle = 25.;
        m.snippet_action(SnippetAction::Transform { placement: p }, "main")
            .unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        let transformed = m.editor.document().clone();
        let unchanged_revision = m.editor.revision();
        m.component_action(Action::Transform { id: instance }, "main")
            .unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        assert_eq!(m.editor.revision(), unchanged_revision);
        assert_eq!(m.editor.document(), &transformed);

        assert_ne!(transformed.glyphs[&2], before.glyphs[&2]);
        m.action(ModelAction::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(ModelAction::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &transformed);
        m.component_action(Action::Source { id }, "main").unwrap();
        assert_eq!(m.glyph, 1);
        assert_eq!(m.snippets.contours, [0].into());
        let mut g = m.editor.document().glyphs[&1].clone();
        geometry::split_segment(&mut g.contours[0], 0).unwrap();
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        m.component_action(Action::Update { id }, "main").unwrap();
        let after = m.editor.document().clone();
        let c = &after.glyphs[&2].contours[0];
        let reference = NodeRef {
            definition: None,
            document: after.document_uuid,
            node: font_core::identity::node_id(c, EditPoint::Anchor(1)).unwrap(),
        };
        let root =
            std::env::temp_dir().join(format!("fonts-components-recovery-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("project.json");
        font_core::storage::save(&path, &after).unwrap();
        let loaded = font_core::storage::load(&path).unwrap();
        assert_eq!(loaded, after);
        let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
        store
            .checkpoint(&after, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, after);
        let mut reopened = model();
        reopened.replace_document(doc, None).unwrap();
        reopened
            .action(ModelAction::SelectStableNode {
                reference,
                revision: reopened.editor.revision(),
            })
            .unwrap();
        assert_eq!(reopened.glyph, 2);
        reopened
            .component_action(Action::Detach { id: instance }, "main")
            .unwrap();
        assert!(reopened.editor.document().components.instances.is_empty());
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_pointer_cannot_move_linked_nodes_and_summary_tracks_uses() {
        let mut m = model();
        let id = m.snippets.component.unwrap();
        m.action(ModelAction::SelectGlyph { id: 2 }).unwrap();
        m.component_action(Action::Begin { id }, "main").unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        let before = m.editor.document().clone();
        let p = m.viewport().screen(before.glyphs[&2].contours[0].start);
        m.pointer_input("down", p[0], p[1], false, false).unwrap();
        assert!(m.drag.is_none());
        m.pointer_input("move", p[0] + 20., p[1] + 20., false, false)
            .unwrap();
        m.pointer_input("up", p[0] + 20., p[1] + 20., false, false)
            .unwrap();
        assert_eq!(m.editor.document(), &before);
        let summary = serde_json::to_value(m.snapshot()).unwrap();
        assert_eq!(summary["components"]["definitions"][0]["uses"], 1);
        assert_eq!(summary["contours"][0]["linked"], true);
        assert_eq!(
            m.component_action(Action::Remove { id }, "main"),
            Err("ComponentInUse".into())
        );
    }
    #[test]
    fn nested_composition_source_navigation_child_edits_and_recovery() {
        let mut m = model();
        m.editor
            .add_characters(m.editor.revision(), &['C', 'D'].into())
            .unwrap();
        let leaf = m.snippets.component.unwrap();
        m.action(ModelAction::SelectGlyph { id: 2 }).unwrap();
        m.component_action(Action::Begin { id: leaf }, "main")
            .unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        m.component_action(
            Action::Capture {
                info: Info {
                    name: "Parent".into(),
                    tags: "".into(),
                    category: "radical".into(),
                    source: "".into(),
                    license: "".into(),
                    favorite: false,
                },
            },
            "main",
        )
        .unwrap();
        let parent = m.snippets.component.unwrap();
        let child = *m.editor.document().components.definitions[&parent]
            .children
            .keys()
            .next()
            .unwrap();
        m.action(ModelAction::SelectGlyph { id: 3 }).unwrap();
        m.component_action(Action::Begin { id: parent }, "main")
            .unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        m.component_action(Action::Source { id: leaf }, "main")
            .unwrap();
        let mut source = m.editor.document().glyphs[&1].clone();
        let opened = font_core::path_edit::split(&source.contours[0], 1)
            .unwrap()
            .remove(0);
        source.contours = font_core::path_edit::split(&opened, 2).unwrap();
        m.editor.replace_glyph(m.editor.revision(), source).unwrap();
        m.snippets.contours = [0, 1].into();
        m.component_action(Action::Update { id: leaf }, "main")
            .unwrap();
        m.component_action(Action::Source { id: parent }, "main")
            .unwrap();
        assert_eq!(m.glyph, 2);
        assert_eq!(m.snippets.contours, [0, 1].into());
        let before = m.editor.document().clone();
        let revision = m.editor.revision();
        m.component_action(Action::Update { id: parent }, "main")
            .unwrap();
        assert_eq!(m.editor.revision(), revision);
        assert_eq!(m.editor.document(), &before);
        let pose = font_core::components::placement(
            &before.components.definitions[&leaf],
            &before.components.definitions[&parent].children[&child].instance(),
        )
        .unwrap();
        let mut placement = pose.clone();
        placement.center[0] += 40.;
        m.component_action(
            Action::ChildTransform {
                parent,
                id: child,
                placement,
            },
            "main",
        )
        .unwrap();
        let changed = m.editor.document().clone();
        assert_ne!(changed.glyphs[&3], before.glyphs[&3]);
        m.action(ModelAction::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(ModelAction::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &changed);
        let summary = serde_json::to_value(m.snapshot()).unwrap();
        assert_eq!(
            summary["components"]["children"].as_array().unwrap().len(),
            1
        );
        m.component_action(Action::Choose { id: leaf }, "main")
            .unwrap();
        let impact = serde_json::to_value(m.snapshot()).unwrap();
        assert_eq!(
            impact["components"]["instances"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["affected"] == true)
                .count(),
            2
        );
        let root = std::env::temp_dir().join(format!("fonts-nested-recovery-{}", Uuid::new_v4()));
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        store
            .checkpoint(&changed, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, changed);
        doc.validate().unwrap();
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
        m.component_action(
            Action::ChildDetach {
                parent,
                id: child,
                remove: false,
            },
            "main",
        )
        .unwrap();
        assert!(m.editor.document().components.definitions[&parent]
            .children
            .is_empty());
    }
}
