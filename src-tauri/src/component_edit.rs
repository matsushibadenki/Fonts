//! A Rust-owned canvas target. The real Editor and its persisted document never change ownership.
use crate::model::Model;
use font_core::{identity::Uuid, workspace::Tool, Glyph, Point};
pub struct Editing {
    pub id: Uuid,
    pub glyph: Glyph,
    pub revision: u64,
    pub ascender: f64,
    pub descender: f64,
    return_view: (f64, Point, Tool, bool, bool),
}
impl Model {
    pub fn editing_id(&self) -> Option<Uuid> {
        self.component_edit.as_ref().map(|e| e.id)
    }
    pub fn editing_upm(&self) -> u16 {
        let doc = self.editor.document();
        self.editing_id()
            .map_or(doc.units_per_em, |id| doc.components.definitions[&id].upm)
    }
    pub fn editing_glyph(&self) -> &Glyph {
        self.master_edit
            .as_ref()
            .map(|e| &e.1)
            .or_else(|| self.component_edit.as_ref().map(|e| &e.glyph))
            .unwrap_or(&self.editor.document().glyphs[&self.glyph])
    }
    pub fn locked_contours(&self) -> std::collections::BTreeSet<Uuid> {
        match self.editing_id() {
            Some(id) => self
                .editor
                .document()
                .components
                .definition_linked_contours(id),
            None => self
                .editor
                .document()
                .components
                .linked_contours(self.glyph),
        }
    }
    pub fn commit_geometry(&mut self, revision: u64, glyph: Glyph) -> Result<(), String> {
        if glyph.id != self.glyph {
            return Err("Conflict".into());
        }
        if let Some((id, _)) = &self.master_edit {
            let id = *id;
            let mut glyph = glyph;
            crate::masters::translate(&mut glyph, [-self.master_origin[0], -self.master_origin[1]]);
            self.editor
                .edit_source_master(
                    revision,
                    font_core::source_font::master_project::Edit::Replace {
                        id,
                        glyph: glyph.id,
                        shape: font_core::source_font::masters::Shape::from_glyph(&glyph),
                    },
                )
                .map_err(|e| e.to_string())?;
            self.refresh_master_edit();
        } else if let Some(id) = self.editing_id() {
            self.editor
                .edit_component_contours(revision, id, glyph.contours)
                .map_err(|e| e.to_string())?;
        } else {
            self.editor
                .replace_glyph(revision, glyph)
                .map_err(|e| e.to_string())?;
        }
        self.refresh_component_edit();
        Ok(())
    }
    fn definition_view(&self, id: Uuid) -> Option<(Glyph, f64, f64)> {
        let d = self.editor.document().components.definitions.get(&id)?;
        let (min, max) = font_core::snippets::bounds(&d.contours).ok()?;
        let mut g = Glyph::new(self.glyph, ' ');
        g.name = d.info.name.clone();
        g.codepoints.clear();
        g.advance = (max[0] - min[0]).max(f64::from(d.upm) * 0.6);
        g.contours = d.contours.as_ref().clone();
        let margin = ((max[1] - min[1]) * 0.05).max(10.);
        Some((g, max[1] + margin, min[1] - margin))
    }
    pub fn refresh_component_edit(&mut self) {
        let Some(edit) = self.component_edit.as_ref() else {
            return;
        };
        if edit.revision == self.editor.revision() {
            return;
        }
        let id = edit.id;
        if let Some((mut glyph, _, _)) = self.definition_view(id) {
            let edit = self.component_edit.as_mut().unwrap();
            // Keep the canvas frame stable across gestures; Fit recalculates it explicitly.
            glyph.advance = edit.glyph.advance;
            edit.glyph = glyph;
            edit.revision = self.editor.revision();
        } else {
            self.leave_component_edit_unchecked();
        }
    }
    pub fn fit_component_view(&mut self) {
        if let Some((glyph, ascender, descender)) =
            self.editing_id().and_then(|id| self.definition_view(id))
        {
            let edit = self.component_edit.as_mut().unwrap();
            edit.glyph.advance = glyph.advance;
            edit.ascender = ascender;
            edit.descender = descender;
        }
        let Some(edit) = self.component_edit.as_ref() else {
            return;
        };
        let Ok((min, max)) = font_core::snippets::bounds(&edit.glyph.contours) else {
            return;
        };
        self.zoom = 1.;
        self.pan = [0., 0.];
        let base = self.viewport().scale();
        let scale = ((self.size[0] - 100.).max(10.) / (max[0] - min[0]).max(1.))
            .min((self.size[1] - 100.).max(10.) / (max[1] - min[1]).max(1.))
            * 0.9;
        self.zoom = (scale / base).clamp(0.2, 8.);
        self.pan[0] =
            (self.editing_glyph().advance / 2. - (min[0] + max[0]) / 2.) * self.viewport().scale();
    }
    pub fn enter_component_edit(&mut self, id: Uuid) -> Result<(), String> {
        if self.has_draft() {
            return Err("edit_in_progress".into());
        }
        let (glyph, ascender, descender) = self.definition_view(id).ok_or("InvalidDocument")?;
        if self.editing_id() == Some(id) {
            return Ok(());
        }
        let epoch = self.epoch.checked_add(1).ok_or("document_epoch_overflow")?;
        let return_view = self
            .component_edit
            .as_ref()
            .map(|e| e.return_view)
            .unwrap_or((self.zoom, self.pan, self.tool, self.nodes, self.fill));
        self.clear_edit_selection();
        self.proof.active = false;
        self.proof.generation += 1;
        self.component_edit = Some(Editing {
            id,
            glyph,
            ascender,
            descender,
            revision: self.editor.revision(),
            return_view,
        });
        self.epoch = epoch;
        self.tool = Tool::Nodes;
        self.nodes = true;
        self.snippets.component_mode = true;
        self.snippets.component = Some(id);
        self.fit_component_view();
        Ok(())
    }
    pub fn exit_component_edit(&mut self) -> Result<(), String> {
        if self.has_draft() {
            return Err("edit_in_progress".into());
        }
        self.leave_component_edit_unchecked();
        Ok(())
    }
    fn leave_component_edit_unchecked(&mut self) {
        if let Some(e) = self.component_edit.take() {
            self.clear_edit_selection();
            (self.zoom, self.pan, self.tool, self.nodes, self.fill) = e.return_view;
            self.epoch = self.epoch.saturating_add(1);
        }
    }
    fn clear_edit_selection(&mut self) {
        self.selected = None;
        self.selected_nodes.clear();
        self.selected_controls.clear();
        self.marquee = None;
        self.boolean_b = Default::default();
        self.drag = None;
        self.pen.clear();
        self.pen_edit = Default::default();
        self.images = Default::default();
        self.snippets.clear_transient();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        components::Action as ComponentAction, model::Action, snippets::Action as SnippetAction,
    };
    use font_core::{
        geometry::EditPoint,
        identity::{self, NodeRef},
        snippets::Info,
        workspace::Workspace,
    };
    fn model() -> (Model, Uuid) {
        let mut m = Model::new(Workspace::default());
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        g.contours = font_core::snippets::builtins()[&2]
            .contours
            .as_ref()
            .clone();
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        let id = m
            .editor
            .capture_component(
                m.editor.revision(),
                m.glyph,
                &[0].into(),
                Info {
                    name: "Part".into(),
                    tags: "".into(),
                    category: "radical".into(),
                    source: "".into(),
                    license: "".into(),
                    favorite: false,
                },
            )
            .unwrap();
        (m, id)
    }
    #[test]
    fn definition_canvas_native_drag_split_history_and_recovery_without_source() {
        let (mut m, id) = model();
        let mut source = m.editor.document().glyphs[&m.glyph].clone();
        source.contours.clear();
        m.editor.replace_glyph(m.editor.revision(), source).unwrap();
        m.editor
            .place_component(m.editor.revision(), id, m.glyph, [1., 0., 0., 1., 0., 0.])
            .unwrap();
        let before = m.editor.document().clone();
        let view = (m.zoom, m.pan, m.tool, m.nodes, m.fill);
        let epoch = m.epoch;
        m.component_action(ComponentAction::EditDefinition { id }, "main")
            .unwrap();
        assert_eq!(m.editor.document(), &before);
        assert!(m.epoch > epoch);
        assert!(m.visible_images().is_empty());
        assert_eq!(
            m.image_context(m.epoch, m.editor.revision(), m.glyph),
            Err("component_edit_unavailable".into())
        );
        assert!(m
            .action(Action::Advance {
                id: m.glyph,
                revision: m.editor.revision(),
                value: 800.
            })
            .is_err());
        let start = m.editing_glyph().contours[0].start;
        let frame = m.viewport();
        let p = frame.screen(start);
        m.pointer_input("down", p[0], p[1], false, false).unwrap();
        assert!(m.drag.is_some());
        m.pointer_input("move", p[0] + 15., p[1] - 10., false, false)
            .unwrap();
        m.pointer_input("up", p[0] + 15., p[1] - 10., false, false)
            .unwrap();
        let changed = m.editor.document().clone();
        assert_eq!(
            (
                m.viewport().advance,
                m.viewport().ascender,
                m.viewport().descender
            ),
            (frame.advance, frame.ascender, frame.descender)
        );
        assert_ne!(changed, before);
        assert_ne!(changed.components.definitions[&id].contours[0].start, start);
        assert_eq!(
            m.editing_glyph().contours,
            changed.components.definitions[&id]
                .contours
                .as_ref()
                .clone()
        );
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &before);
        assert_eq!(m.editing_glyph().contours[0].start, start);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &changed);
        m.action(Action::SplitSegment {
            id: m.glyph,
            contour: 0,
            segment: 0,
            revision: m.editor.revision(),
        })
        .unwrap();
        let after = m.editor.document().clone();
        let reference = NodeRef {
            document: after.document_uuid,
            definition: Some(id),
            node: identity::node_id(&m.editing_glyph().contours[0], EditPoint::Anchor(1)).unwrap(),
        };
        let root =
            std::env::temp_dir().join(format!("fonts-definition-recovery-{}", Uuid::new_v4()));
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        store
            .checkpoint(&after, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, after);
        let (mut reopened, _) = model();
        reopened.replace_document(doc, None).unwrap();
        assert!(reopened.editing_id().is_none());
        reopened
            .action(Action::SelectStableNode {
                reference,
                revision: reopened.editor.revision(),
            })
            .unwrap();
        assert_eq!(reopened.editing_id(), Some(id));
        assert_eq!(reopened.selected.unwrap().1, EditPoint::Anchor(1));
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
        m.component_action(ComponentAction::ExitEdit, "main")
            .unwrap();
        assert_eq!((m.zoom, m.pan, m.tool, m.nodes, m.fill), view);
        assert_eq!(m.editor.document(), &after);
    }
    #[test]
    fn snippet_and_nested_reference_target_definition_and_reject_cycles() {
        let (mut m, id) = model();
        m.enter_component_edit(id).unwrap();
        let before = m.editor.document().glyphs.clone();
        m.snippet_action(
            SnippetAction::Begin {
                key: "builtin:1".into(),
            },
            "main",
        )
        .unwrap();
        assert!(m.exit_component_edit().is_err());
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        assert_eq!(m.editing_glyph().contours.len(), 2);
        assert_eq!(m.editor.document().glyphs, before);
        assert_eq!(
            m.begin_component(id, None, "main"),
            Err("ComponentCycle".into())
        );
        let child = m
            .editor
            .capture_component(
                m.editor.revision(),
                m.glyph,
                &[0].into(),
                Info {
                    name: "Child".into(),
                    tags: "".into(),
                    category: "radical".into(),
                    source: "".into(),
                    license: "".into(),
                    favorite: false,
                },
            )
            .unwrap();
        m.refresh_component_edit();
        m.begin_component(child, None, "main").unwrap();
        m.snippet_action(SnippetAction::Commit, "main").unwrap();
        assert_eq!(
            m.editor.document().components.definitions[&id]
                .children
                .len(),
            1
        );
        assert_eq!(m.editing_glyph().contours.len(), 3);
        assert_eq!(m.locked_contours().len(), 1);
        assert_eq!(m.editor.document().glyphs, before);
        m.editor.document().validate().unwrap();
    }
    #[test]
    fn undo_registration_exits_mode_and_scoped_reference_never_selects_source() {
        let (mut m, id) = model();
        let c = &m.editor.document().components.definitions[&id].contours[0];
        let reference = NodeRef {
            document: m.editor.document().document_uuid,
            definition: Some(id),
            node: identity::node_id(c, EditPoint::Anchor(0)).unwrap(),
        };
        m.action(Action::SelectStableNode {
            reference,
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editing_id(), Some(id));
        assert!(m
            .action(Action::SelectStableNode {
                reference: NodeRef {
                    definition: None,
                    ..reference
                },
                revision: m.editor.revision()
            })
            .is_err());
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert!(m.editing_id().is_none());
        assert!(!m.editor.document().components.definitions.contains_key(&id));
    }
    #[test]
    fn absent_source_pen_and_bulk_move_edit_only_definition() {
        let (mut m, id) = model();
        let source = m.glyph;
        let mut doc = m.editor.document().clone();
        doc.glyphs.remove(&source);
        m.replace_document(doc, None).unwrap();
        m.enter_component_edit(id).unwrap();
        let before = m.editor.document().glyphs.clone();
        m.action(Action::SelectAllNodes).unwrap();
        m.action(Action::MoveSelected {
            dx: 10.,
            dy: 20.,
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document().glyphs, before);
        m.action(Action::Tool { tool: Tool::Pen }).unwrap();
        for p in [[50., 50.], [200., 300.], [400., 50.]] {
            let q = m.viewport().screen(p);
            m.pointer_input("down", q[0], q[1], false, false).unwrap();
            m.pointer_input("up", q[0], q[1], false, false).unwrap();
        }
        m.action(Action::FinishOpenPen).unwrap();
        assert_eq!(m.editing_glyph().contours.len(), 2);
        assert_eq!(m.editor.document().glyphs, before);
        assert!(!m.editor.document().glyphs.contains_key(&source));
        let contours = m.editing_glyph().contours.clone();
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editing_glyph().contours.len(), 1);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editing_glyph().contours, contours);
    }
}
