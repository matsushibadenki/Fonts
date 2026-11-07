//! Commands and bounded point pages; font bytes and editable outlines remain in Rust.
use crate::{model::Model, AppState};
use font_core::{identity::Uuid, source_font::master_project::Edit};
use serde::Serialize;
use tauri::{Manager, WebviewWindow};
impl Model {
    pub fn refresh_master_edit(&mut self) {
        let Some(id) = self.master_edit.as_ref().map(|e| e.0) else {
            return;
        };
        let shape = self.editor.document().source_font.as_ref().and_then(|s| {
            s.masters.as_ref().and_then(|p| {
                p.topologies
                    .get(&(self.glyph as u16))
                    .and_then(|_| p.shape(s, id, self.glyph).ok())
            })
        });
        self.master_origin = self
            .editor
            .document()
            .source_font
            .as_ref()
            .and_then(|s| {
                s.masters
                    .as_ref()
                    .and_then(|p| p.frame(s, id, self.glyph).ok())
            })
            .unwrap_or([0.; 2]);
        if let Some(shape) = shape {
            let mut glyph = self.editor.document().glyphs[&self.glyph].clone();
            glyph.advance = shape.advance;
            glyph.contours = shape.contours;
            translate(&mut glyph, self.master_origin);
            self.master_edit = Some((id, glyph));
        } else {
            self.master_edit = None;
        }
    }
}
#[tauri::command]
pub async fn source_master_edit(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    mut operation: Edit,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let app = window.app_handle().clone();
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".to_string());
        }
        if m.has_draft() {
            return Err("edit_in_progress".into());
        }
        if let Edit::Point {
            id,
            glyph,
            position,
            ..
        } = &mut operation
        {
            if m.master_edit
                .as_ref()
                .is_some_and(|e| e.0 == *id && e.1.id == *glyph)
            {
                position[0] -= m.master_origin[0];
                position[1] -= m.master_origin[1];
            }
        }
        m.editor
            .edit_source_master(revision, operation)
            .map_err(|e| e.to_string())?;
        m.refresh_master_edit();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    crate::publish(&app);
    Ok(())
}
#[tauri::command]
pub async fn source_master_open(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    id: Option<Uuid>,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let app = window.app_handle().clone();
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".to_string());
        }
        if m.has_draft() {
            return Err("edit_in_progress".into());
        }
        if let Some(id) = id {
            let glyph = m.glyph;
            m.editor
                .edit_source_master(revision, Edit::Capture { id, glyph })
                .map_err(|e| e.to_string())?;
            let g = m.editor.document().glyphs[&glyph].clone();
            m.master_edit = Some((id, g));
            m.refresh_master_edit();
            m.tool = font_core::workspace::Tool::Nodes;
            m.nodes = true;
        } else {
            m.master_edit = None;
        }
        m.drag = None;
        m.selected = None;
        m.selected_nodes.clear();
        m.selected_controls.clear();
        let generation = m.proof.generation.wrapping_add(1);
        m.proof = Default::default();
        m.proof.generation = generation;
        m.zoom = 1.;
        m.pan = [0., 0.];
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    crate::publish(&app);
    Ok(())
}
#[derive(Serialize)]
pub struct PointPage {
    points: Vec<(usize, bool, font_core::Point)>,
    total: usize,
    advance: f64,
}
#[tauri::command]
pub fn source_master_points(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    offset: usize,
) -> Result<PointPage, String> {
    crate::allowed(&window)?;
    let state = window.state::<AppState>();
    let m = state.0.lock().map_err(|_| "state_poisoned")?;
    if m.epoch != epoch || m.editor.revision() != revision {
        return Err("Conflict".into());
    }
    let (_, glyph) = m.master_edit.as_ref().ok_or("InvalidDocument")?;
    let source = m
        .editor
        .document()
        .source_font
        .as_ref()
        .ok_or("InvalidDocument")?;
    let project = source.masters.as_ref().ok_or("InvalidDocument")?;
    let topology = project
        .topologies
        .get(&(glyph.id as u16))
        .ok_or("InvalidDocument")?;
    let points = topology
        .physical_points(&glyph.contours)
        .map_err(|e| e.to_string())?;
    let on_curve: Vec<_> = topology.on_curve_flags();
    Ok(PointPage {
        total: points.len(),
        advance: glyph.advance,
        points: points
            .into_iter()
            .zip(on_curve)
            .enumerate()
            .skip(offset)
            .take(64)
            .map(|(index, (p, on))| (index, on, p))
            .collect(),
    })
}

pub(crate) fn translate(glyph: &mut font_core::Glyph, offset: font_core::Point) {
    let apply = |p: &mut font_core::Point| {
        p[0] += offset[0];
        p[1] += offset[1];
    };
    for c in &mut glyph.contours {
        apply(&mut c.start);
        for s in &mut c.segments {
            match s {
                font_core::Segment::Line { end } => apply(end),
                font_core::Segment::Quadratic { control, end } => {
                    apply(control);
                    apply(end);
                }
                font_core::Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => {
                    apply(control1);
                    apply(control2);
                    apply(end);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Action, recovery_store::Store};
    use font_core::{source_font::SourceFont, workspace::Workspace};
    #[test]
    fn master_drag_is_one_history_step_and_recovers_with_source_and_point_ids() {
        for (bytes, selected_glyph, moving) in [
            (
                include_bytes!("../../crates/font-core/tests/data/variations/gvar.ttf").as_slice(),
                1,
                false,
            ),
            (
                include_bytes!(
                    "../../crates/font-core/tests/data/variations/nested-blend-cff2.otf"
                )
                .as_slice(),
                1,
                false,
            ),
            (
                include_bytes!("../../crates/font-core/tests/data/variations/composite-gvar.ttf")
                    .as_slice(),
                3,
                false,
            ),
            (
                include_bytes!("../../crates/font-core/tests/data/variations/moving-origins.ttf")
                    .as_slice(),
                1,
                true,
            ),
        ] {
            let mut m = Model::new(Workspace::default());
            m.replace_document(SourceFont::variable_document(bytes, 0, None).unwrap(), None)
                .unwrap();
            m.action(Action::SelectGlyph { id: selected_glyph })
                .unwrap();
            if moving {
                m.editor
                    .edit_source_variations(
                        m.editor.revision(),
                        font_core::source_font::variable::Edit::Coordinates {
                            values: [("wght".into(), 900.), ("wdth".into(), 100.)].into(),
                        },
                    )
                    .unwrap();
            }

            m.editor
                .edit_source_master(
                    m.editor.revision(),
                    Edit::Register {
                        name: "Default".into(),
                    },
                )
                .unwrap();
            let id = m
                .editor
                .document()
                .source_font
                .as_ref()
                .unwrap()
                .masters
                .as_ref()
                .unwrap()
                .masters
                .entries[0]
                .id;
            let glyph = m.glyph;
            m.editor
                .edit_source_master(m.editor.revision(), Edit::Capture { id, glyph })
                .unwrap();
            m.master_edit = Some((id, m.editor.document().glyphs[&glyph].clone()));
            m.refresh_master_edit();
            m.tool = font_core::workspace::Tool::Nodes;
            m.nodes = true;
            let before = m.editor.document().clone();
            let original = m.editing_glyph().contours[0].start;
            let at = m.viewport().screen(original);
            m.pointer("down", at[0], at[1]).unwrap();
            m.pointer("move", at[0] + 12., at[1] - 8.).unwrap();
            m.pointer("move", at[0] + 24., at[1] - 16.).unwrap();
            m.pointer("up", at[0] + 24., at[1] - 16.).unwrap();
            let changed = m.editor.document().clone();
            let document_id = m.document_id;
            m.open_document(font_core::FontDocument::new("Other", "B").unwrap(), None)
                .unwrap();
            assert!(m.master_edit.is_none());
            m.activate_document(document_id).unwrap();
            assert_eq!(m.master_edit.as_ref().unwrap().0, id);
            assert_ne!(changed, before);
            let position = m.editing_glyph().contours[0].start;
            assert_ne!(position, original);
            m.action(Action::Undo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(*m.editor.document(), before);
            assert_eq!(m.editing_glyph().contours[0].start, original);
            m.action(Action::Redo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(*m.editor.document(), changed);
            assert_eq!(m.editing_glyph().contours[0].start, position);
            let other = if glyph == 3 { 1 } else { 3 };
            m.action(Action::SelectGlyph { id: other }).unwrap();
            let source = m.editor.document().source_font.as_ref().unwrap();
            assert!(source
                .masters
                .as_ref()
                .unwrap()
                .topologies
                .contains_key(&(other as u16)));
            assert!(m.master_edit.is_some());
            m.action(Action::Undo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert!(m.master_edit.is_none());
            m.action(Action::SelectGlyph { id: glyph }).unwrap();
            let root = std::env::temp_dir().join(format!("fonts-master-recovery-{id}"));
            let store = Store::new(root.clone()).unwrap();
            store
                .checkpoint(&changed, m.epoch, m.editor.revision(), None)
                .unwrap();
            drop(store);
            let store = Store::new(root.clone()).unwrap();
            let (list, _) = store.candidates().unwrap();
            let (doc, lease) = store.candidate(&list[0].id).unwrap();
            assert_eq!(doc, changed);
            m.replace_document(doc, None).unwrap();
            assert!(m.master_edit.is_none());
            let exported = m
                .editor
                .document()
                .source_font
                .as_ref()
                .unwrap()
                .export()
                .unwrap();
            m.editor
                .document()
                .source_font
                .as_ref()
                .unwrap()
                .verify_output(&exported, false)
                .unwrap();
            drop(lease);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
