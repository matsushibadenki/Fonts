//! Mixed control/anchor selection, native pointer transactions and recovery integration.
use crate::model::{Action, Model};
use font_core::{
    geometry::{self, EditPoint::*},
    identity::{self, NodeRef, Uuid},
    workspace::{Tool, Workspace},
    Contour, Segment,
};
fn model() -> Model {
    let mut m = Model::new(Workspace::default());
    let mut g = m.editor.document().glyphs[&m.glyph].clone();
    g.contours = vec![Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![
            Segment::Cubic {
                control1: [100., 200.],
                control2: [300., 200.],
                end: [400., 0.],
            },
            Segment::Quadratic {
                control: [600., -200.],
                end: [800., 0.],
            },
        ],
        closed: false,
    }];
    m.editor.replace_glyph(m.editor.revision(), g).unwrap();
    m.tool = Tool::Nodes;
    m.zoom = 2.;
    m.size = [1200., 1200.];
    m
}
fn pointer(m: &mut Model, phase: &str, p: [f64; 2], shift: bool) {
    let q = m.viewport().screen(p);
    m.pointer_input(phase, q[0], q[1], false, shift).unwrap();
}
fn pos(m: &Model, p: geometry::EditPoint) -> [f64; 2] {
    geometry::point_position(&m.editor.document().glyphs[&m.glyph].contours[0], p).unwrap()
}
#[test]
fn shift_select_mixed_drag_cancel_and_one_step_history() {
    let mut m = model();
    let before = m.editor.document().clone();
    let revision = m.editor.revision();
    for (p, shift) in [
        ([0., 0.], false),
        ([100., 200.], true),
        ([300., 200.], true),
    ] {
        pointer(&mut m, "down", p, shift);
        pointer(&mut m, "up", p, shift);
    }
    assert_eq!(m.point_selection().len(), 3);
    assert_eq!(m.selected_controls.len(), 2);
    assert_eq!(m.editor.revision(), revision);
    pointer(&mut m, "down", [100., 200.], false);
    pointer(&mut m, "move", [120., 230.], false);
    let draft = m.visible_glyph();
    assert_eq!(
        geometry::point_position(&draft.contours[0], Control1(0)),
        Some([120., 230.])
    );
    pointer(&mut m, "cancel", [120., 230.], false);
    assert_eq!(m.editor.document(), &before);
    pointer(&mut m, "down", [100., 200.], false);
    pointer(&mut m, "move", [120., 230.], false);
    pointer(&mut m, "move", [140., 250.], false);
    pointer(&mut m, "up", [140., 250.], false);
    assert_eq!(pos(&m, Anchor(0)), [40., 50.]);
    assert_eq!(pos(&m, Control1(0)), [140., 250.]);
    assert_eq!(pos(&m, Control2(0)), [340., 250.]);
    assert_eq!(pos(&m, Anchor(1)), [400., 0.]);
    let after = m.editor.document().clone();
    assert_eq!(m.editor.revision(), revision + 1);
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
}
#[test]
fn controls_rectangle_shift_toggle_cancel_and_selection_snapshot() {
    let mut m = model();
    let revision = m.editor.revision();
    pointer(&mut m, "down", [50., 150.], false);
    pointer(&mut m, "move", [350., 250.], false);
    pointer(&mut m, "up", [350., 250.], false);
    assert!(m.selected_nodes.is_empty());
    assert_eq!(m.selected_controls.len(), 2);
    let snapshot = serde_json::to_value(m.snapshot()).unwrap();
    assert_eq!(snapshot["selected_controls"].as_array().unwrap().len(), 2);
    pointer(&mut m, "down", [100., 200.], true);
    pointer(&mut m, "up", [100., 200.], true);
    assert_eq!(m.selected_controls, [(0, Control2(0))].into());
    pointer(&mut m, "down", [500., -250.], true);
    pointer(&mut m, "move", [650., -150.], true);
    assert_eq!(m.selected_controls.len(), 2);
    pointer(&mut m, "cancel", [0., 0.], false);
    assert_eq!(m.selected_controls, [(0, Control2(0))].into());
    assert_eq!(m.editor.revision(), revision);
    m.action(Action::SelectAllControls).unwrap();
    assert_eq!(m.selected_controls.len(), 3);
    m.action(Action::MoveSelected {
        dx: 7.,
        dy: -3.,
        revision,
    })
    .unwrap();
    assert_eq!(pos(&m, Control1(1)), [607., -203.]);
    assert_eq!(pos(&m, Anchor(0)), [0., 0.]);
    m.action(Action::ClearNodes).unwrap();
    assert!(m.point_selection().is_empty());
}
#[test]
fn transform_save_recovery_reopen_and_uuid_ref() {
    let mut m = model();
    let original = m.editor.document().clone();
    m.action(Action::SelectAllControls).unwrap();
    let r = NodeRef {
        definition: None,
        document: original.document_uuid,
        node: identity::node_id(&original.glyphs[&m.glyph].contours[0], Control1(0)).unwrap(),
    };
    let revision = m.editor.revision();
    assert!(m
        .action(Action::TransformSelected {
            scale: [2., 1.],
            angle: 30.,
            revision: revision + 1
        })
        .is_err());
    assert_eq!(m.editor.document(), &original);
    m.action(Action::TransformSelected {
        scale: [2., 1.],
        angle: 30.,
        revision,
    })
    .unwrap();
    let after = m.editor.document().clone();
    assert_ne!(after, original);
    let root = std::env::temp_dir().join(format!("fonts-control-recovery-{}", Uuid::new_v4()));
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
    let mut recovered = model();
    recovered.replace_document(doc, None).unwrap();
    recovered
        .action(Action::SelectStableNode {
            reference: r,
            revision: recovered.editor.revision(),
        })
        .unwrap();
    assert_eq!(recovered.selected_controls, [(0, Control1(0))].into());
    recovered
        .action(Action::MoveSelected {
            dx: 1.,
            dy: 2.,
            revision: recovered.editor.revision(),
        })
        .unwrap();
    drop(lease);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
