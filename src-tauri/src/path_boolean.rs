//! Compute filled-area operations off the UI thread and commit only to the captured revision.
use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WebviewWindow};
static BUSY: AtomicBool = AtomicBool::new(false);
struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
#[derive(serde::Deserialize)]
pub struct Request {
    epoch: u64,
    revision: u64,
    glyph: u32,
    left: Vec<usize>,
    right: usize,
    #[serde(default)]
    right_group: Vec<usize>,
    operation: String,
}
#[tauri::command]
pub async fn boolean_paths(window: WebviewWindow, request: Request) -> Result<(), String> {
    crate::allowed(&window)?;
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("edit_in_progress".into());
    }
    let _guard = Guard;
    let Request {
        epoch,
        revision,
        glyph,
        left,
        right,
        right_group,
        operation,
    } = request;
    let indices: std::collections::BTreeSet<_> = left.iter().copied().collect();
    let right_indices: std::collections::BTreeSet<_> = if right_group.is_empty() {
        [right].into_iter().collect()
    } else {
        right_group.iter().copied().collect()
    };
    if right_indices.is_empty()
        || right_indices.len() > 1024
        || (!right_group.is_empty() && right_indices.len() != right_group.len())
        || !indices.is_disjoint(&right_indices)
    {
        return Err("boolean_geometry".into());
    }
    if indices.is_empty() || indices.len() != left.len() || indices.len() > 1024 {
        return Err("boolean_geometry".into());
    }
    let source = {
        let state = window.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision || m.glyph != glyph {
            return Err("Conflict".into());
        }
        if m.has_draft() || crate::updates::installing() {
            return Err("edit_in_progress".into());
        }
        m.editor.document().glyphs[&glyph].clone()
    };
    let output = tauri::async_runtime::spawn_blocking(move || {
        compute_group(source, &indices, &right_indices, &operation)
    })
    .await
    .map_err(|_| "boolean_failed")??;
    {
        let state = window.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        commit(&mut m, epoch, revision, glyph, output)?;
    }
    crate::publish(window.app_handle());
    Ok(())
}

impl crate::model::Model {
    pub fn boolean_operand_b(&self) -> Vec<usize> {
        if self.boolean_b.0 == self.epoch
            && self.boolean_b.1 == self.glyph
            && self.boolean_b.2 == self.editor.revision()
        {
            self.boolean_b.3.iter().copied().collect()
        } else {
            Vec::new()
        }
    }
}
#[cfg(test)]
fn compute(
    source: font_core::Glyph,
    indices: &std::collections::BTreeSet<usize>,
    right: usize,
    operation: &str,
) -> Result<font_core::Glyph, String> {
    compute_group(source, indices, &[right].into_iter().collect(), operation)
}

fn compute_group(
    mut source: font_core::Glyph,
    indices: &std::collections::BTreeSet<usize>,
    right: &std::collections::BTreeSet<usize>,
    operation: &str,
) -> Result<font_core::Glyph, String> {
    if indices.is_empty() || right.is_empty() || !indices.is_disjoint(right) {
        return Err("boolean_geometry".into());
    }
    let a = indices
        .iter()
        .map(|&i| source.contours.get(i).cloned().ok_or("boolean_geometry"))
        .collect::<Result<Vec<_>, _>>()?;
    let b = right
        .iter()
        .map(|&i| source.contours.get(i).cloned().ok_or("boolean_geometry"))
        .collect::<Result<Vec<_>, _>>()?;
    let result = font_core::path_boolean::combine(&a, &b, operation).map_err(|e| {
        if e == font_core::Error::TooLarge {
            "boolean_limit"
        } else if e == font_core::Error::CurveBooleanAmbiguous {
            "boolean_curve_ambiguous"
        } else {
            "boolean_geometry"
        }
    })?;
    let mut consumed = indices.clone();
    consumed.extend(right);
    let first = *consumed.first().unwrap();
    let mut output = Some(result);
    let mut contours = Vec::new();
    for (i, c) in source.contours.into_iter().enumerate() {
        if i == first {
            contours.extend(output.take().unwrap());
        }
        if !consumed.contains(&i) {
            contours.push(c);
        }
    }
    source.contours = contours;
    source.status = font_core::GlyphStatus::Draft;
    Ok(source)
}
fn commit(
    m: &mut crate::model::Model,
    epoch: u64,
    revision: u64,
    glyph: u32,
    output: font_core::Glyph,
) -> Result<(), String> {
    if m.epoch != epoch || m.glyph != glyph || m.editor.revision() != revision {
        return Err("Conflict".into());
    }
    if m.has_draft() || crate::updates::installing() {
        return Err("edit_in_progress".into());
    }
    m.editor
        .replace_glyph(revision, output)
        .map_err(|e| e.to_string())?;
    m.selected = None;
    m.selected_nodes.clear();
    m.selected_controls.clear();
    m.snippets.contours.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, Model};
    use font_core::{
        workspace::{Tool, Workspace},
        Contour, Segment,
    };
    fn rect(x: f64) -> Contour {
        Contour {
            identity: Default::default(),
            start: [x, 0.],
            segments: vec![
                Segment::Line {
                    end: [x + 100., 0.],
                },
                Segment::Line {
                    end: [x + 100., 100.],
                },
                Segment::Line { end: [x, 100.] },
            ],
            closed: true,
            smooth: Default::default(),
        }
    }
    fn model() -> Model {
        let mut m = Model::new(Workspace::default());
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        g.contours = vec![rect(0.), rect(50.), rect(400.)];
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        m.tool = Tool::Nodes;
        m
    }
    #[test]
    fn point_selection_group_drag_and_numeric_move_are_one_history_step() {
        let mut m = model();
        let click = |m: &mut Model, p: [f64; 2], shift| {
            let s = m.viewport().screen(p);
            m.pointer_input("down", s[0], s[1], false, shift).unwrap();
            m.pointer("up", s[0], s[1]).unwrap();
        };
        click(&mut m, [0., 0.], false);
        click(&mut m, [100., 0.], true);
        assert_eq!(m.selected_nodes.len(), 2);
        let before = m.editor.document().clone();
        let a = m.viewport().screen([0., 0.]);
        let b = m.viewport().screen([20., 30.]);
        m.pointer("down", a[0], a[1]).unwrap();
        m.pointer("move", b[0], b[1]).unwrap();
        assert_eq!(m.editor.document(), &before);
        m.pointer("up", b[0], b[1]).unwrap();
        assert_eq!(
            m.editor.document().glyphs[&m.glyph].contours[0].start,
            [20., 30.]
        );
        assert_eq!(m.editor.revision(), 2);
        m.action(Action::Undo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        m.action(Action::SelectAllNodes).unwrap();
        m.action(Action::MoveSelected {
            dx: 10.,
            dy: 20.,
            revision: m.editor.revision(),
        })
        .unwrap();
        assert!(m
            .action(Action::MoveSelected {
                dx: 1.,
                dy: 0.,
                revision: 0
            })
            .is_err());
        m.action(Action::ClearNodes).unwrap();
        assert!(m.selected_nodes.is_empty());
    }
    #[test]
    fn toggling_points_and_cancel_dont_change_document() {
        let mut m = model();
        let before = m.editor.document().clone();
        let p = m.viewport().screen([0., 0.]);
        for _ in 0..2 {
            m.pointer_input("down", p[0], p[1], false, true).unwrap();
            m.pointer("up", p[0], p[1]).unwrap();
        }
        assert!(m.selected_nodes.is_empty());
        assert!(m.selected.is_none());
        assert_eq!(m.editor.document(), &before);
        // Off-center clicks inside the hit target select without snapping the outline.
        m.pointer_input("down", p[0] + 2., p[1] + 2., false, true)
            .unwrap();
        m.pointer("move", p[0] + 2., p[1] + 2.).unwrap();
        m.pointer("up", p[0] + 2., p[1] + 2.).unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(Action::SelectAllNodes).unwrap();
        m.pointer("down", p[0], p[1]).unwrap();
        m.pointer("move", p[0] + 20., p[1] + 20.).unwrap();
        m.pointer("cancel", 0., 0.).unwrap();
        assert_eq!(m.editor.document(), &before);
    }
    #[test]
    fn boolean_commit_guards_history_and_disk_recovery() {
        let mut m = model();
        let before = m.editor.document().clone();
        let source = before.glyphs[&m.glyph].clone();
        let output = compute(source, &[0].into_iter().collect(), 1, "difference").unwrap();
        assert_eq!(
            output.contours.last(),
            before.glyphs[&m.glyph].contours.last()
        );
        let epoch = m.epoch;
        assert!(commit(&mut m, epoch + 1, 1, 1, output.clone()).is_err());
        let glyph = m.glyph;
        commit(&mut m, epoch, 1, glyph, output.clone()).unwrap();
        assert!(commit(&mut m, epoch, 1, glyph, output).is_err());
        let final_doc = m.editor.document().clone();
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &final_doc);
        m.action(Action::SelectAllNodes).unwrap();
        m.action(Action::MoveSelected {
            dx: 7.,
            dy: -3.,
            revision: m.editor.revision(),
        })
        .unwrap();
        m.action(Action::SplitPath {
            id: m.glyph,
            contour: 0,
            anchor: 1,
            revision: m.editor.revision(),
        })
        .unwrap();
        let final_doc = m.editor.document().clone();
        let root = std::env::temp_dir().join(format!(
            "fonts-path-processing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("project.json");
        font_core::storage::save(&path, &final_doc).unwrap();
        assert_eq!(font_core::storage::load(&path).unwrap(), final_doc);
        let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
        store
            .checkpoint(&final_doc, epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let scanner = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
        let (list, _) = scanner.candidates().unwrap();
        let (doc, lease) = scanner.candidate(&list[0].id).unwrap();
        assert_eq!(doc, final_doc);
        drop(lease);
        drop(scanner);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cut_and_group_move_roundtrip_without_dropping_curves() {
        let mut m = model();
        m.action(Action::SplitPath {
            id: m.glyph,
            contour: 0,
            anchor: 1,
            revision: m.editor.revision(),
        })
        .unwrap();
        assert!(!m.editor.document().glyphs[&m.glyph].contours[0].closed);
        m.action(Action::SplitPath {
            id: m.glyph,
            contour: 0,
            anchor: 1,
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document().glyphs[&m.glyph].contours.len(), 4);
        m.action(Action::SelectAllNodes).unwrap();
        m.action(Action::MoveSelected {
            dx: 20.,
            dy: 10.,
            revision: m.editor.revision(),
        })
        .unwrap();
        let doc = m.editor.document().clone();
        let restored: font_core::FontDocument =
            serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(doc, restored);
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &doc);
    }
    #[test]
    fn grouped_b_keeps_hole_winding_and_selection_context() {
        let mut m = model();
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        let mut hole = font_core::Contour {
            identity: Default::default(),
            start: [25., 25.],
            segments: vec![
                font_core::Segment::Line { end: [75., 25.] },
                font_core::Segment::Line { end: [75., 75.] },
                font_core::Segment::Line { end: [25., 75.] },
            ],
            closed: true,
            smooth: Default::default(),
        };
        font_core::path_edit::reverse(&mut hole).unwrap();
        g.contours = vec![rect(0.), rect(0.), hole, rect(400.)];
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        let before = m.editor.document().clone();
        let revision = m.editor.revision();
        m.snippets.contours = [1, 2].into_iter().collect();
        m.action(Action::SetBooleanB).unwrap();
        assert_eq!(m.boolean_operand_b(), vec![1, 2]);
        assert!(m.snippets.contours.is_empty());
        assert_eq!(m.editor.revision(), revision);
        let source = before.glyphs[&m.glyph].clone();
        let output = compute_group(
            source.clone(),
            &[0].into_iter().collect(),
            &[1, 2].into_iter().collect(),
            "difference",
        )
        .unwrap();
        assert_eq!(output.contours.len(), 2);
        assert_eq!(output.contours[1], source.contours[3]);
        let ring = &output.contours[0];
        let points = std::iter::once(ring.start).chain(ring.segments.iter().map(|s| match s {
            font_core::Segment::Line { end } => *end,
            _ => panic!(),
        }));
        assert!(points
            .clone()
            .all(|p| p[0] >= 25. && p[0] <= 75. && p[1] >= 25. && p[1] <= 75.));
        let epoch = m.epoch;
        let glyph = m.glyph;
        commit(&mut m, epoch, revision, glyph, output).unwrap();
        assert!(m.boolean_operand_b().is_empty());
        let final_doc = m.editor.document().clone();
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &final_doc);
        let roundtrip: font_core::FontDocument =
            serde_json::from_slice(&serde_json::to_vec(&final_doc).unwrap()).unwrap();
        roundtrip.validate().unwrap();
        assert_eq!(roundtrip, final_doc);
        assert!(compute_group(
            source,
            &[0].into_iter().collect(),
            &[0, 1].into_iter().collect(),
            "union"
        )
        .is_err());
    }
    #[test]
    fn b_assignment_rejects_invalid_selection_and_never_leaks_across_contexts() {
        let mut m = model();
        assert!(m.action(Action::SetBooleanB).is_err());
        m.snippets.contours.insert(0);
        m.action(Action::SetBooleanB).unwrap();
        assert_eq!(m.boolean_operand_b(), vec![0]);
        m.epoch += 1;
        assert!(m.boolean_operand_b().is_empty());
        m.epoch -= 1;
        m.glyph = 0;
        assert!(m.boolean_operand_b().is_empty());
        m.glyph = 1;
        m.action(Action::ClearBooleanB).unwrap();
        assert!(m.boolean_operand_b().is_empty());
    }
    fn curved(x: f64) -> Contour {
        Contour {
            identity: Default::default(),
            smooth: Default::default(),
            start: [x, 0.],
            segments: vec![
                Segment::Quadratic {
                    control: [x + 100., 200.],
                    end: [x + 200., 0.],
                },
                Segment::Line { end: [x, 0.] },
            ],
            closed: true,
        }
    }
    #[test]
    fn curved_boolean_commits_history_save_and_recovery_with_unselected_contours_intact() {
        for operation in ["union", "difference", "intersection", "xor"] {
            let mut m = model();
            let mut g = m.editor.document().glyphs[&m.glyph].clone();
            g.contours = vec![curved(0.), curved(80.), rect(400.)];
            m.editor.replace_glyph(m.editor.revision(), g).unwrap();
            let before = m.editor.document().clone();
            let untouched = before.glyphs[&m.glyph].contours[2].clone();
            let output = compute_group(
                before.glyphs[&m.glyph].clone(),
                &[0].into(),
                &[1].into(),
                operation,
            )
            .unwrap();
            assert!(output
                .contours
                .iter()
                .flat_map(|c| &c.segments)
                .any(|s| matches!(s, Segment::Quadratic { .. })));
            assert!(output.contours.contains(&untouched));
            let epoch = m.epoch;
            let revision = m.editor.revision();
            let glyph = m.glyph;
            m.selected_controls
                .insert((0, font_core::geometry::EditPoint::Control1(0)));
            commit(&mut m, epoch, revision, glyph, output.clone()).unwrap();
            assert!(m.selected_controls.is_empty());
            let after = m.editor.document().clone();
            assert!(commit(&mut m, epoch, revision, glyph, output).is_err());
            assert_eq!(m.editor.document(), &after);
            m.action(Action::Undo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document(), &before);
            m.action(Action::Redo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document(), &after);
            let root = std::env::temp_dir().join(format!(
                "fonts-curve-boolean-{}",
                font_core::identity::Uuid::new_v4()
            ));
            std::fs::create_dir(&root).unwrap();
            let path = root.join("project.json");
            font_core::storage::save(&path, &after).unwrap();
            assert_eq!(font_core::storage::load(&path).unwrap(), after);
            let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
            store
                .checkpoint(&after, m.epoch, m.editor.revision(), None)
                .unwrap();
            drop(store);
            let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
            let (list, _) = store.candidates().unwrap();
            let (doc, lease) = store.candidate(&list[0].id).unwrap();
            assert_eq!(doc, after);
            doc.validate().unwrap();
            drop(lease);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
