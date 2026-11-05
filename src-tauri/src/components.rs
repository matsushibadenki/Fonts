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
    id: Uuid,
    definition: Uuid,
    glyph: u32,
    name: String,
}
#[derive(Serialize)]
pub struct Summary {
    active: bool,
    selected: Option<Uuid>,
    definitions: Vec<Definition>,
    instances: Vec<Instance>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Mode { active: bool },
    Choose { id: Uuid },
    Capture { info: Info },
    Rename { id: Uuid, info: Info },
    Update { id: Uuid },
    Remove { id: Uuid },
    Begin { id: Uuid },
    Transform { id: Uuid },
    Detach { id: Uuid },
    Source { id: Uuid },
    Usage { id: Uuid },
}
impl Model {
    pub fn component_summary(&self) -> Summary {
        let library = &self.editor.document().components;
        Summary {
            active: self.snippets.component_mode,
            selected: self
                .snippets
                .component
                .filter(|id| library.definitions.contains_key(id)),
            definitions: library
                .definitions
                .iter()
                .enumerate()
                .map(|(thumbnail, (&id, d))| Definition {
                    thumbnail,
                    id,
                    info: d.info.clone(),
                    uses: library
                        .instances
                        .values()
                        .filter(|i| i.definition == id)
                        .count(),
                    source: self
                        .editor
                        .document()
                        .glyphs
                        .get(&d.source_glyph)
                        .is_some_and(|g| {
                            d.contours.iter().any(|c| {
                                g.contours.iter().any(|s| {
                                    s.identity.contours.first() == c.identity.contours.first()
                                })
                            })
                        }),
                })
                .collect(),
            instances: library
                .instances
                .iter()
                .map(|(&id, i)| Instance {
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
        let selection: std::collections::BTreeSet<_> = self
            .snippets
            .contours
            .iter()
            .filter_map(|&n| {
                self.editor.document().glyphs[&self.glyph]
                    .contours
                    .get(n)
                    .and_then(|c| c.identity.contours.first().copied())
            })
            .collect();
        let revision = self.editor.revision();
        let result: font_core::Result<()> = match action {
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
                let glyph = &self.editor.document().glyphs[&self.glyph];
                self.snippets.contours = glyph
                    .contours
                    .iter()
                    .enumerate()
                    .filter_map(|(n, c)| {
                        d.contours
                            .iter()
                            .any(|s| s.identity.contours.first() == c.identity.contours.first())
                            .then_some(n)
                    })
                    .collect();
                if self.snippets.contours.is_empty() {
                    return Err("component_source_missing".into());
                }
                return Ok(());
            }
        };
        result.map_err(|e| e.to_string())?;
        self.selected = None;
        self.selected_nodes.clear();
        self.selected_controls.clear();
        self.snippets.contours = self.editor.document().glyphs[&self.glyph]
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
}
