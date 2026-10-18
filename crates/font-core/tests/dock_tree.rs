use font_core::{
    dock_tree::{Axis, DockNode, Side},
    workspace::{Dock, Panel, Placement, Workspace},
    Error,
};
fn tree(w: &Workspace) -> String {
    serde_json::to_string(&w.trees).unwrap()
}
fn relative(w: &mut Workspace, p: Panel, t: Panel, s: Side) {
    w.relative(w.revision, p, t, s).unwrap();
    w.validate().unwrap();
}
#[test]
fn nested_axes_tabs_and_inner_resize_roundtrip() {
    let mut w = Workspace::default();
    relative(&mut w, Panel::Outline, Panel::Metrics, Side::Bottom);
    relative(&mut w, Panel::Snippets, Panel::Metrics, Side::Right);
    relative(&mut w, Panel::Glyphs, Panel::Snippets, Side::Bottom);
    let DockNode::Split { axis, first, .. } = &w.trees[&Dock::Right] else {
        panic!()
    };
    assert_eq!(*axis, Axis::Vertical);
    assert!(matches!(
        **first,
        DockNode::Split {
            axis: Axis::Horizontal,
            ..
        }
    ));
    w.resize_split(w.revision, Dock::Right, &[0, 1], 0.65)
        .unwrap();
    let restored: Workspace = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    let restored = restored.restore().unwrap();
    assert_eq!(tree(&restored), tree(&w));
    relative(&mut w, Panel::Glyphs, Panel::Metrics, Side::Tab);
    assert!(w.trees[&Dock::Right]
        .clone()
        .remove(Panel::Outline)
        .unwrap()
        .collect(&mut vec![], 0)
        .is_ok());
    w.activate(w.revision, Panel::Metrics).unwrap();
    w.validate().unwrap();
}
#[test]
fn float_keeps_slot_ratio_and_close_restores_it() {
    let mut w = Workspace::default();
    relative(&mut w, Panel::Snippets, Panel::Metrics, Side::Bottom);
    w.resize_split(w.revision, Dock::Right, &[], 0.7).unwrap();
    let before = tree(&w);
    w.place(
        w.revision,
        Panel::Snippets,
        Placement::Floating {
            return_to: Dock::Right,
        },
    )
    .unwrap();
    assert_eq!(tree(&w), before);
    w.validate().unwrap();
    let mut restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    restored = restored.restore().unwrap();
    restored.redock(Panel::Snippets).unwrap();
    assert_eq!(tree(&restored), before);
    restored.validate().unwrap();
}
#[test]
fn floating_to_other_split_and_cross_dock_collapses_empty_nodes() {
    let mut w = Workspace::default();
    relative(&mut w, Panel::Snippets, Panel::Metrics, Side::Bottom);
    w.place(
        w.revision,
        Panel::Snippets,
        Placement::Floating {
            return_to: Dock::Right,
        },
    )
    .unwrap();
    relative(&mut w, Panel::Snippets, Panel::Outline, Side::Left);
    assert!(matches!(w.trees[&Dock::Right], DockNode::Tabs { .. }));
    assert!(matches!(w.trees[&Dock::Bottom], DockNode::Split { .. }));
    w.place(
        w.revision,
        Panel::Snippets,
        Placement::Docked { dock: Dock::Left },
    )
    .unwrap();
    assert!(matches!(w.trees[&Dock::Bottom], DockNode::Tabs { .. }));
    w.validate().unwrap();
}
#[test]
fn old_json_migrates_and_invalid_tree_is_rejected() {
    let w = Workspace::default();
    let mut json = serde_json::to_value(&w).unwrap();
    json.as_object_mut().unwrap().remove("trees");
    let old: Workspace = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(tree(&old.restore().unwrap()), tree(&w));
    json["panels"].as_object_mut().unwrap().remove("snippets");
    let old: Workspace = serde_json::from_value(json).unwrap();
    old.restore().unwrap().validate().unwrap();
    let mut bad = w.clone();
    bad.trees.insert(Dock::Left, DockNode::tabs(Panel::Metrics));
    assert!(bad.restore().is_err());
    let mut bad = w;
    bad.trees.remove(&Dock::Bottom);
    assert!(bad.restore().is_err());
}
#[test]
fn bad_commands_conflicts_and_overflow_are_atomic() {
    let mut w = Workspace::default();
    relative(&mut w, Panel::Snippets, Panel::Metrics, Side::Left);
    let before = serde_json::to_string(&w).unwrap();
    for value in [f64::NAN, f64::INFINITY, 0., 1., 0.14, 0.86] {
        assert!(w.resize_split(w.revision, Dock::Right, &[], value).is_err());
    }
    assert!(w.resize_split(w.revision, Dock::Right, &[0], 0.5).is_err());
    assert!(w.resize_split(w.revision, Dock::Right, &[2], 0.5).is_err());
    assert_eq!(
        w.relative(0, Panel::Outline, Panel::Glyphs, Side::Top),
        Err(Error::Conflict)
    );
    assert!(w
        .relative(w.revision, Panel::Glyphs, Panel::Glyphs, Side::Top)
        .is_err());
    assert_eq!(serde_json::to_string(&w).unwrap(), before);
    w.revision = u64::MAX;
    let before = serde_json::to_string(&w).unwrap();
    assert!(w
        .relative(w.revision, Panel::Outline, Panel::Glyphs, Side::Top)
        .is_err());
    assert_eq!(serde_json::to_string(&w).unwrap(), before);
}
#[test]
fn all_move_combinations_keep_exactly_one_slot_per_panel() {
    let mut w = Workspace::default();
    for round in 0..20 {
        for p in Panel::ALL {
            for target in Panel::ALL {
                if p != target {
                    relative(
                        &mut w,
                        p,
                        target,
                        [Side::Left, Side::Top, Side::Right, Side::Bottom, Side::Tab][round % 5],
                    );
                }
            }
        }
    }
    for p in Panel::ALL {
        let dock = match w.panels[&p] {
            Placement::Docked { dock } => dock,
            _ => unreachable!(),
        };
        w.place(w.revision, p, Placement::Floating { return_to: dock })
            .unwrap();
        w.validate().unwrap();
    }
    for p in Panel::ALL {
        w.redock(p).unwrap();
        w.validate().unwrap();
    }
}
