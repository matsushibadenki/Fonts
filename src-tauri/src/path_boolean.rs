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
        operation,
    } = request;
    let indices: std::collections::BTreeSet<_> = left.iter().copied().collect();
    if indices.is_empty()
        || indices.len() != left.len()
        || indices.contains(&right)
        || indices.len() > 1024
    {
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
    let output =
        tauri::async_runtime::spawn_blocking(move || compute(source, &indices, right, &operation))
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

fn compute(
    mut source: font_core::Glyph,
    indices: &std::collections::BTreeSet<usize>,
    right: usize,
    operation: &str,
) -> Result<font_core::Glyph, String> {
    if indices.is_empty() || indices.contains(&right) {
        return Err("boolean_geometry".into());
    }
    let a = indices
        .iter()
        .map(|&i| source.contours.get(i).cloned().ok_or("boolean_geometry"))
        .collect::<Result<Vec<_>, _>>()?;
    let b = source
        .contours
        .get(right)
        .cloned()
        .ok_or("boolean_geometry")?;
    let result = font_core::path_boolean::combine(&a, &[b], operation).map_err(|e| {
        if e == font_core::Error::TooLarge {
            "boolean_limit"
        } else {
            "boolean_geometry"
        }
    })?;
    let mut consumed = indices.clone();
    consumed.insert(right);
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
}
