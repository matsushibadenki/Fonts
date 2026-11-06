use font_core::{
    geometry::{self, EditPoint::*},
    identity,
    point_transform::{self, Selection},
    storage, Contour, Editor, FontDocument, Glyph, GlyphStatus, Segment,
};
fn contour() -> Contour {
    Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![
            Segment::Cubic {
                control1: [20., 60.],
                control2: [80., 60.],
                end: [100., 0.],
            },
            Segment::Quadratic {
                control: [150., -60.],
                end: [200., 0.],
            },
        ],
        closed: false,
    }
}
fn pos(c: &Contour, p: geometry::EditPoint) -> [f64; 2] {
    geometry::point_position(c, p).unwrap()
}
#[test]
fn mixed_transform_moves_attached_and_explicit_handles_once() {
    let mut c = vec![contour()];
    let points: Selection = [
        (0, Anchor(0)),
        (0, Control1(0)),
        (0, Control2(0)),
        (0, Control1(1)),
    ]
    .into();
    point_transform::transform(&mut c, &points, [1., 1.], 0., [10., 20.]).unwrap();
    assert_eq!(c[0].start, [10., 20.]);
    assert_eq!(pos(&c[0], Control1(0)), [30., 80.]);
    assert_eq!(pos(&c[0], Control2(0)), [90., 80.]);
    assert_eq!(pos(&c[0], Anchor(1)), [100., 0.]);
    assert_eq!(pos(&c[0], Control1(1)), [160., -40.]);
}
#[test]
fn affine_uses_selection_bounds_and_preserves_unselected_points() {
    let mut c = vec![contour(), contour()];
    let before = c.clone();
    point_transform::transform(
        &mut c,
        &[(0, Control1(0)), (0, Control2(0))].into(),
        [2., 1.],
        90.,
        [0., 0.],
    )
    .unwrap();
    let a = pos(&c[0], Control1(0));
    let b = pos(&c[0], Control2(0));
    assert!((a[0] - 50.).abs() < 1e-9 && (a[1] - 0.).abs() < 1e-9);
    assert!((b[0] - 50.).abs() < 1e-9 && (b[1] - 120.).abs() < 1e-9);
    assert_eq!(c[0].start, before[0].start);
    assert_eq!(c[1], before[1]);
    assert_eq!(pos(&c[0], Control1(1)), pos(&before[0], Control1(1)));
}
#[test]
fn closed_alias_and_quadratic_shared_handle_are_not_transformed_twice() {
    let mut c = contour();
    c.closed = true;
    c.segments.push(Segment::Line { end: c.start });
    let mut c = vec![c];
    point_transform::transform(
        &mut c,
        &[
            (0, Anchor(0)),
            (0, Anchor(3)),
            (0, Anchor(1)),
            (0, Anchor(2)),
            (0, Control1(1)),
        ]
        .into(),
        [1., 1.],
        0.,
        [5., 7.],
    )
    .unwrap();
    assert_eq!(c[0].start, [5., 7.]);
    assert_eq!(pos(&c[0], Anchor(3)), [5., 7.]);
    assert_eq!(pos(&c[0], Control1(1)), [155., -53.]);
}
#[test]
fn invalid_transforms_are_atomic() {
    let original = vec![contour()];
    let mut c = original.clone();
    for selected in [
        Selection::new(),
        [(3, Control1(0))].into(),
        [(0, Control2(1))].into(),
        [(0, Anchor(999))].into(),
    ] {
        assert!(point_transform::transform(&mut c, &selected, [1., 1.], 0., [1., 2.]).is_err());
        assert_eq!(c, original);
    }
    for (scale, angle, delta) in [
        ([f64::NAN, 1.], 0., [0., 0.]),
        ([1., 1.], f64::INFINITY, [0., 0.]),
        ([1., 1.], 0., [1e7, 0.]),
    ] {
        assert!(point_transform::transform(
            &mut c,
            &[(0, Control1(0))].into(),
            scale,
            angle,
            delta
        )
        .is_err());
        assert_eq!(c, original);
    }
}
#[test]
fn smooth_constraints_survive_whole_affine_and_release_only_broken_tangents() {
    let mut c = contour();
    c.segments.push(Segment::Cubic {
        control1: [230., 0.],
        control2: [270., 50.],
        end: [300., 0.],
    });
    geometry::set_smooth(&mut c, 1, true).unwrap();
    let mut all = vec![c.clone()];
    let points = geometry::edit_points(&c)
        .into_iter()
        .map(|(p, _)| (0, p))
        .collect();
    point_transform::transform(&mut all, &points, [-2., 0.5], 37., [10., 20.]).unwrap();
    assert!(all[0].smooth.contains(&1));
    let mut partial = vec![c];
    point_transform::transform(
        &mut partial,
        &[(0, Control2(0))].into(),
        [1., 1.],
        0.,
        [0., 30.],
    )
    .unwrap();
    assert!(!partial[0].smooth.contains(&1));
}
#[test]
fn uuid_history_and_real_storage_preserve_batch_results() {
    let mut doc = FontDocument::new("Controls", "").unwrap();
    doc.glyphs.insert(
        1,
        Glyph {
            id: 1,
            name: "A".into(),
            codepoints: ['A'].into(),
            cid: None,
            advance: 600.,
            contours: vec![contour()],
            status: GlyphStatus::Draft,
        },
    );
    let mut editor = Editor::new(doc).unwrap();
    let before = editor.document().clone();
    let mut g = before.glyphs[&1].clone();
    let id = identity::node_id(&g.contours[0], Control1(0)).unwrap();
    point_transform::transform(
        &mut g.contours,
        &[(0, Control1(0)), (0, Control2(0))].into(),
        [1.5, 0.8],
        25.,
        [3., 4.],
    )
    .unwrap();
    editor.replace_glyph(editor.revision(), g).unwrap();
    let after = editor.document().clone();
    editor.undo(editor.revision()).unwrap();
    assert_eq!(editor.document(), &before);
    editor.redo(editor.revision()).unwrap();
    assert_eq!(editor.document(), &after);
    let path =
        std::env::temp_dir().join(format!("fonts-controls-{}.json", identity::Uuid::new_v4()));
    storage::save(&path, &after).unwrap();
    let loaded = storage::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, after);
    assert!(identity::resolve(
        &loaded,
        identity::NodeRef {
            definition: None,
            document: after.document_uuid,
            node: id
        }
    )
    .is_some());
}
