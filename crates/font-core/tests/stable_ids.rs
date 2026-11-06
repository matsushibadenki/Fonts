use font_core::{
    geometry::{self, EditPoint},
    identity::{self, NodeRef, Uuid},
    path_edit, storage, Contour, Editor, FontDocument, Segment,
};
fn contour(x: f64) -> Contour {
    Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [x, 0.],
        segments: vec![
            Segment::Cubic {
                control1: [x + 20., 60.],
                control2: [x + 80., 60.],
                end: [x + 100., 0.],
            },
            Segment::Quadratic {
                control: [x + 150., -60.],
                end: [x + 200., 0.],
            },
            Segment::Line {
                end: [x + 300., 100.],
            },
        ],
        closed: false,
    }
}
fn editor() -> Editor {
    let mut d = FontDocument::new("Stable IDs", "AB").unwrap();
    d.glyphs.get_mut(&1).unwrap().contours = vec![contour(0.), contour(500.)];
    Editor::new(d).unwrap()
}
fn reference(e: &Editor, ci: usize, p: EditPoint) -> NodeRef {
    NodeRef {
        definition: None,
        document: e.document().document_uuid,
        node: identity::node_id(&e.document().glyphs[&1].contours[ci], p).unwrap(),
    }
}
fn position(e: &Editor, r: NodeRef) -> Option<[f64; 2]> {
    let a = identity::resolve(e.document(), r)?;
    geometry::point_position(&e.document().glyphs[&a.glyph].contours[a.contour], a.point)
}
fn commit(e: &mut Editor, change: impl FnOnce(&mut Vec<Contour>)) {
    let mut g = e.document().glyphs[&1].clone();
    change(&mut g.contours);
    e.replace_glyph(e.revision(), g).unwrap();
}
#[test]
fn addition_deletion_and_handle_split_preserve_unaffected_references() {
    let mut e = editor();
    let end = reference(&e, 0, EditPoint::Anchor(1));
    let later = reference(&e, 0, EditPoint::Anchor(2));
    let c1 = reference(&e, 0, EditPoint::Control1(0));
    let c2 = reference(&e, 0, EditPoint::Control2(0));
    let original = e.document().clone();
    commit(&mut e, |c| geometry::split_segment(&mut c[0], 0).unwrap());
    assert_eq!(
        identity::resolve(e.document(), end).unwrap().point,
        EditPoint::Anchor(2)
    );
    assert_eq!(
        identity::resolve(e.document(), c1).unwrap().point,
        EditPoint::Control1(0)
    );
    assert_eq!(
        identity::resolve(e.document(), c2).unwrap().point,
        EditPoint::Control2(1)
    );
    let added = reference(&e, 0, EditPoint::Anchor(1));
    assert_ne!(added.node, end.node);
    commit(&mut e, |c| {
        geometry::delete_segment_end(&mut c[0], 1).unwrap()
    });
    assert!(identity::resolve(e.document(), end).is_none());
    assert!(identity::resolve(e.document(), c2).is_none());
    assert!(identity::resolve(e.document(), c1).is_some());
    assert_eq!(position(&e, later), Some([200., 0.]));
    let deleted = e.document().clone();
    e.undo(e.revision()).unwrap();
    assert!(identity::resolve(e.document(), end).is_some());
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &deleted);
    e.undo(e.revision()).unwrap();
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &original);
    assert!(identity::resolve(e.document(), added).is_none());
}
#[test]
fn cut_and_coincident_join_keep_both_endpoint_aliases_and_contour_aliases() {
    let mut e = editor();
    let cut = reference(&e, 0, EditPoint::Anchor(1));
    let beyond = reference(&e, 0, EditPoint::Control1(1));
    let cid = e.document().glyphs[&1].contours[0].identity.contours[0];
    commit(&mut e, |c| {
        let parts = path_edit::split(&c[0], 1).unwrap();
        c.splice(0..=0, parts);
    });
    let duplicate = reference(&e, 1, EditPoint::Anchor(0));
    let right_cid = e.document().glyphs[&1].contours[1].identity.contours[0];
    assert_ne!(cut.node, duplicate.node);
    assert_eq!(identity::resolve(e.document(), cut).unwrap().contour, 0);
    assert_eq!(identity::resolve(e.document(), beyond).unwrap().contour, 1);
    commit(&mut e, |c| {
        let result = path_edit::join(&c[0], false, &c[1], false).unwrap();
        c.splice(0..=1, [result]);
    });
    assert_eq!(
        identity::resolve(e.document(), cut),
        identity::resolve(e.document(), duplicate)
    );
    assert_eq!(identity::resolve_contour(e.document(), cid), Some((1, 0)));
    assert_eq!(
        identity::resolve_contour(e.document(), right_cid),
        Some((1, 0))
    );
    assert_eq!(identity::resolve(e.document(), beyond).unwrap().contour, 0);
    e.document().validate().unwrap();
}
#[test]
fn join_all_orientations_and_reverse_keep_anchor_and_handle_positions() {
    for start in [false, true] {
        for end in [false, true] {
            let mut e = editor();
            let refs: Vec<_> = (0..2)
                .flat_map(|ci| {
                    geometry::edit_points(&e.document().glyphs[&1].contours[ci])
                        .into_iter()
                        .map(move |(p, _)| (ci, p))
                })
                .map(|(ci, p)| reference(&e, ci, p))
                .collect();
            let positions: Vec<_> = refs.iter().map(|r| position(&e, *r)).collect();
            commit(&mut e, |c| {
                let result = path_edit::join(&c[0], start, &c[1], end).unwrap();
                *c = vec![result];
            });
            for (r, p) in refs.iter().zip(&positions) {
                assert_eq!(position(&e, *r), *p);
            }
            commit(&mut e, |c| path_edit::reverse(&mut c[0]).unwrap());
            for (r, p) in refs.iter().zip(&positions) {
                assert_eq!(position(&e, *r), *p);
            }
        }
    }
}
#[test]
fn closed_cut_closure_conversion_and_independent_copy_have_distinct_ids() {
    let mut e = editor();
    commit(&mut e, |c| path_edit::set_closed(&mut c[0], true).unwrap());
    let refs: Vec<_> = geometry::edit_points(&e.document().glyphs[&1].contours[0])
        .iter()
        .map(|(p, _)| reference(&e, 0, *p))
        .collect();
    let positions: Vec<_> = refs.iter().map(|r| position(&e, *r)).collect();
    commit(&mut e, |c| {
        c[0] = path_edit::split(&c[0], 1).unwrap().remove(0);
    });
    for (r, p) in refs.iter().zip(&positions) {
        assert_eq!(position(&e, *r), *p);
    }
    let n = e.document().glyphs[&1].contours[0].segments.len();
    assert_ne!(
        reference(&e, 0, EditPoint::Anchor(0)),
        reference(&e, 0, EditPoint::Anchor(n))
    );
    let old = reference(&e, 0, EditPoint::Anchor(0));
    commit(&mut e, |c| {
        let mut copied = c[0].clone();
        copied.identity = Default::default();
        c.push(copied);
    });
    assert_ne!(old, reference(&e, 2, EditPoint::Anchor(0)));
    assert!(identity::resolve(
        e.document(),
        NodeRef {
            definition: None,
            document: Uuid::new_v4(),
            node: old.node
        }
    )
    .is_none());
}
#[test]
fn project_save_migration_and_roundtrip_keep_uuids_and_references() {
    let e = editor();
    let r = reference(&e, 0, EditPoint::Anchor(2));
    let dir = std::env::temp_dir().join(format!(
        "fonts-identities-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
    std::fs::create_dir(&dir).unwrap();
    let p = dir.join("project.json");
    storage::save(&p, e.document()).unwrap();
    let (doc, migrated) = storage::load_with_migration(&p).unwrap();
    assert!(!migrated);
    assert_eq!(&doc, e.document());
    assert!(identity::resolve(&doc, r).is_some());
    let mut legacy = serde_json::to_value(&doc).unwrap();
    legacy["schema_version"] = 1.into();
    legacy.as_object_mut().unwrap().remove("document_uuid");
    for glyph in legacy["glyphs"].as_object_mut().unwrap().values_mut() {
        for c in glyph["contours"].as_array_mut().unwrap() {
            c.as_object_mut().unwrap().remove("identity");
        }
    }
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&p, &bytes).unwrap();
    let (upgraded, migrated) = storage::load_with_migration(&p).unwrap();
    assert!(migrated);
    assert_eq!(upgraded.schema_version, 2);
    assert!(!upgraded.document_uuid.is_nil());
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    storage::save(&p, &upgraded).unwrap();
    let (reloaded, migrated) = storage::load_with_migration(&p).unwrap();
    assert!(!migrated);
    assert_eq!(reloaded, upgraded);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn duplicate_or_malformed_identity_is_rejected_without_modifying_history() {
    let mut e = editor();
    let before = e.document().clone();
    let mut g = before.glyphs[&1].clone();
    g.contours[0].identity.anchors[1] = g.contours[0].identity.anchors[0].clone();
    assert!(e.replace_glyph(e.revision(), g).is_err());
    assert_eq!(e.document(), &before);
    assert!(!e.can_undo());
    let mut bad = before.clone();
    bad.glyphs.get_mut(&1).unwrap().contours[0]
        .identity
        .controls
        .clear();
    assert!(bad.validate().is_err());
}

#[test]
fn references_follow_contours_between_glyphs_and_through_history() {
    let mut e = editor();
    let r = reference(&e, 1, EditPoint::Anchor(2));
    let before = e.document().clone();
    let mut a = before.glyphs[&1].clone();
    let mut b = before.glyphs[&2].clone();
    b.contours.push(a.contours.remove(1));
    e.replace_glyphs(e.revision(), vec![a, b]).unwrap();
    let address = identity::resolve(e.document(), r).unwrap();
    assert_eq!(address.glyph, 2);
    assert_eq!(address.contour, 0);
    e.undo(e.revision()).unwrap();
    assert_eq!(identity::resolve(e.document(), r).unwrap().glyph, 1);
    e.redo(e.revision()).unwrap();
    assert_eq!(identity::resolve(e.document(), r).unwrap().glyph, 2);
}
#[test]
fn snippet_copies_do_not_reuse_deleted_source_ids() {
    let mut e = editor();
    let r = reference(&e, 0, EditPoint::Anchor(1));
    let info = font_core::snippets::Info {
        name: "copy".into(),
        tags: String::new(),
        category: "shape".into(),
        source: String::new(),
        license: "MIT".into(),
        favorite: false,
    };
    let key = e
        .capture_snippet(e.revision(), 1, &[0].into_iter().collect(), info)
        .unwrap();
    let snippet = e.document().snippets.entries[&key].clone();
    commit(&mut e, |c| {
        c.remove(0);
    });
    assert!(identity::resolve(e.document(), r).is_none());
    let placement = snippet.placement(1000, [300., 300.]).unwrap();
    e.insert_snippet(e.revision(), 1, &snippet, &placement)
        .unwrap();
    assert!(identity::resolve(e.document(), r).is_none());
    assert_ne!(reference(&e, 1, EditPoint::Anchor(1)).node, r.node);
}
#[test]
fn project_uuids_never_change_font_output() {
    let mut a = editor().document().clone();
    for c in &mut a.glyphs.get_mut(&1).unwrap().contours {
        c.closed = true;
    }
    let mut b = a.clone();
    b.document_uuid = Uuid::new_v4();
    for c in &mut b.glyphs.get_mut(&1).unwrap().contours {
        c.identity = Default::default();
    }
    identity::upgrade(&mut b).unwrap();
    assert_ne!(a, b);
    assert!(a.same_font_data(&b));
    for format in [
        font_core::font_formats::Format::Ttf,
        font_core::font_formats::Format::Otf,
    ] {
        assert_eq!(
            font_core::font_formats::export(&a, format).unwrap(),
            font_core::font_formats::export(&b, format).unwrap()
        );
    }
}

#[test]
fn coincident_closure_retains_endpoint_aliases_and_undo_restores_distinct_nodes() {
    let mut e = editor();
    commit(&mut e, |c| {
        let start = c[0].start;
        if let Segment::Line { end } = &mut c[0].segments[2] {
            *end = start;
        }
    });
    let start = reference(&e, 0, EditPoint::Anchor(0));
    let end = reference(&e, 0, EditPoint::Anchor(3));
    assert_ne!(start, end);
    commit(&mut e, |c| path_edit::set_closed(&mut c[0], true).unwrap());
    assert_eq!(
        identity::resolve(e.document(), start),
        identity::resolve(e.document(), end)
    );
    commit(&mut e, |c| path_edit::set_closed(&mut c[0], false).unwrap());
    let new_end = reference(&e, 0, EditPoint::Anchor(3));
    assert_ne!(new_end, end);
    assert_ne!(new_end, start);
    e.undo(e.revision()).unwrap();
    e.undo(e.revision()).unwrap();
    assert_ne!(
        identity::resolve(e.document(), start),
        identity::resolve(e.document(), end)
    );
}
