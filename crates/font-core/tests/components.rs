use font_core::{
    components,
    geometry::{self, EditPoint},
    identity::{self, NodeRef, Uuid},
    snippets::Info,
    storage, Editor, Error, FontDocument,
};
use std::collections::BTreeSet;
fn info(name: &str) -> Info {
    Info {
        name: name.into(),
        tags: "radical".into(),
        category: "radical".into(),
        source: "Original".into(),
        license: "MIT".into(),
        favorite: false,
    }
}
fn editor() -> Editor {
    let mut doc = FontDocument::new("Linked", "AB").unwrap();
    doc.glyphs.get_mut(&1).unwrap().contours = font_core::snippets::builtins()[&2]
        .contours
        .as_ref()
        .clone();
    Editor::new(doc).unwrap()
}
fn capture(e: &mut Editor) -> Uuid {
    e.capture_component(e.revision(), 1, &[0].into(), info("Part"))
        .unwrap()
}
fn instance(e: &Editor, id: Uuid) -> &font_core::Contour {
    let i = &e.document().components.instances[&id];
    let d = &e.document().components.definitions[&i.definition];
    let target = i.nodes[&d.contours[0].identity.contours[0]];
    e.document().glyphs[&i.glyph]
        .contours
        .iter()
        .find(|c| c.identity.contours[0] == target)
        .unwrap()
}
fn update_source(e: &mut Editor) {
    let mut g = e.document().glyphs[&1].clone();
    let p = g.contours[0].start;
    geometry::move_point(
        &mut g.contours[0],
        EditPoint::Anchor(0),
        [p[0] + 30., p[1] + 10.],
    )
    .unwrap();
    geometry::split_segment(&mut g.contours[0], 0).unwrap();
    e.replace_glyph(e.revision(), g).unwrap();
}
#[test]
fn shared_update_keeps_instance_transforms_and_surviving_uuid_references() {
    let mut e = editor();
    let d = capture(&mut e);
    let a = e
        .place_component(e.revision(), d, 1, [0.5, 0., 0., 0.5, 600., 0.])
        .unwrap();
    let b = e
        .place_component(e.revision(), d, 2, [0., 1., -2., 0., 1000., -50.])
        .unwrap();
    let c = e
        .place_component(e.revision(), d, 2, [-1., 0., 0., 1., 700., 0.])
        .unwrap();
    let ref_node = NodeRef {
        document: e.document().document_uuid,
        node: identity::node_id(instance(&e, b), EditPoint::Anchor(1)).unwrap(),
    };
    let old = instance(&e, b).identity.clone();
    let before = e.document().clone();
    update_source(&mut e);
    assert_eq!(instance(&e, b), &before.glyphs[&2].contours[0]);
    e.update_component(e.revision(), d, 1, &[0].into()).unwrap();
    e.document().validate().unwrap();
    let p = e.document().glyphs[&1].contours[0].start;
    assert_eq!(instance(&e, a).start, [p[0] * 0.5 + 600., p[1] * 0.5]);
    assert_eq!(instance(&e, b).start, [-p[1] * 2. + 1000., p[0] - 50.]);
    assert_eq!(instance(&e, c).start, [700. - p[0], p[1]]);
    assert_eq!(instance(&e, b).identity.contours, old.contours);
    assert!(identity::resolve(e.document(), ref_node).is_some());
    let ids = e.document().components.instances[&b].nodes.clone();
    e.rename_component(e.revision(), d, info("Renamed"))
        .unwrap();
    assert_eq!(e.document().components.instances[&b].nodes, ids);
}
#[test]
fn linked_edits_and_nested_components_are_blocked_detach_is_independent() {
    let mut e = editor();
    let d = capture(&mut e);
    let id = e
        .place_component(e.revision(), d, 2, [1., 0., 0., 1., 0., 0.])
        .unwrap();
    let before = e.document().clone();
    let mut g = before.glyphs[&2].clone();
    g.contours[0].start[0] += 1.;
    assert_eq!(
        e.replace_glyph(e.revision(), g),
        Err(Error::ComponentLinked)
    );
    assert_eq!(
        e.capture_component(e.revision(), 2, &[0].into(), info("Nested")),
        Err(Error::ComponentLinked)
    );
    assert_eq!(
        e.update_component(e.revision(), d, 2, &[0].into()),
        Err(Error::ComponentLinked)
    );
    assert_eq!(
        e.remove_component(e.revision(), d),
        Err(Error::ComponentInUse)
    );
    assert_eq!(e.document(), &before);
    e.detach_component(e.revision(), id).unwrap();
    let detached = e.document().glyphs[&2].clone();
    update_source(&mut e);
    e.update_component(e.revision(), d, 1, &[0].into()).unwrap();
    assert_eq!(e.document().glyphs[&2], detached);
    e.remove_component(e.revision(), d).unwrap();
    assert!(e.document().components.is_empty());
}
#[test]
fn every_component_transaction_is_one_undo_with_schema_restoration() {
    let mut e = editor();
    let original = e.document().clone();
    let d = capture(&mut e);
    assert_eq!(e.document().schema_version, 3);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &original);
    e.redo(e.revision()).unwrap();
    let a = e
        .place_component(e.revision(), d, 1, [1., 0., 0., 1., 600., 0.])
        .unwrap();
    let b = e
        .place_component(e.revision(), d, 2, [0.5, 0.2, 0., 0.5, 0., 0.])
        .unwrap();
    update_source(&mut e);
    let before = e.document().clone();
    e.update_component(e.revision(), d, 1, &[0].into()).unwrap();
    let after = e.document().clone();
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &after);
    e.detach_component(e.revision(), a).unwrap();
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &after);
    let old = instance(&e, b).identity.clone();
    e.transform_component(e.revision(), b, [0., 1., -1., 0., 500., 0.])
        .unwrap();
    assert_eq!(instance(&e, b).identity, old);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &after);
}
#[test]
fn embedded_save_legacy_load_proof_and_export_use_exactly_the_displayed_outlines() {
    let mut e = editor();
    let d = capture(&mut e);
    e.place_component(e.revision(), d, 2, [0.4, 0.1, 0., 0.6, 30., 0.])
        .unwrap();
    // The original outlines may disappear; embedded definitions still reproduce every instance.
    let mut source = e.document().glyphs[&1].clone();
    source.contours.clear();
    e.replace_glyph(e.revision(), source).unwrap();
    let path = std::env::temp_dir().join(format!("fonts-components-{}.json", Uuid::new_v4()));
    storage::save(&path, e.document()).unwrap();
    let (doc, migrated) = storage::load_with_migration(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!migrated);
    assert_eq!(&doc, e.document());
    let mut flattened = doc.clone();
    flattened.components = Default::default();
    assert_eq!(
        font_core::font_io::export(&doc).unwrap(),
        font_core::font_io::export(&flattened).unwrap()
    );
    for format in [
        font_core::font_formats::Format::Ttf,
        font_core::font_formats::Format::Otf,
        font_core::font_formats::Format::Woff2,
    ] {
        let bytes = font_core::font_formats::export(&doc, format).unwrap();
        assert!(!bytes.is_empty());
        if format != font_core::font_formats::Format::Woff2 {
            assert!(ttf_parser::Face::parse(&bytes, 0).is_ok());
        }
    }
    let subset = font_core::proof::subset(&doc, "AB").unwrap();
    assert_eq!(subset.glyphs[&2].contours, doc.glyphs[&2].contours);
    assert!(font_core::proof::prepare(subset, "AB").is_ok());
    let old = FontDocument::new("Legacy", "A").unwrap();
    assert_eq!(Editor::new(old).unwrap().document().schema_version, 2);
}
#[test]
fn corrupted_references_materializations_and_identity_maps_are_rejected() {
    let mut e = editor();
    let d = capture(&mut e);
    let id = e
        .place_component(e.revision(), d, 2, [1., 0., 0., 1., 0., 0.])
        .unwrap();
    let source = e.document().clone();
    let mut collision = source.glyphs[&1].clone();
    collision.contours[0].identity.contours[0] = d;
    assert_eq!(
        e.replace_glyph(e.revision(), collision),
        Err(Error::InvalidDocument)
    );
    assert_eq!(e.document(), &source);
    let mut bad = source.clone();
    bad.components.definitions.remove(&d);
    assert!(bad.validate().is_err());
    let mut bad = source.clone();
    bad.glyphs.get_mut(&2).unwrap().contours[0].start[0] += 1.;
    assert!(bad.validate().is_err());
    let mut bad = source.clone();
    bad.components.instances.get_mut(&id).unwrap().matrix[0] = f64::NAN;
    assert!(bad.validate().is_err());
    let mut bad = source.clone();
    let i = bad.components.instances.get_mut(&id).unwrap();
    let target = *i.nodes.values().next().unwrap();
    for value in std::sync::Arc::make_mut(&mut i.nodes).values_mut() {
        *value = target;
    }
    assert!(bad.validate().is_err());
    let before = e.document().clone();
    let revision = e.revision();
    assert!(e.transform_component(revision, id, [0.; 6]).is_err());
    assert!(e
        .update_component(revision, d, 1, &BTreeSet::new())
        .is_err());
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), revision);
}
#[test]
fn preview_and_committed_transform_agree_in_upm_rotation_and_size() {
    let mut e = editor();
    let d = capture(&mut e);
    let def = &e.document().components.definitions[&d];
    let mut p = def.snippet().placement(2000, [300., 500.]).unwrap();
    p.angle = 37.;
    p.height *= 0.7;
    let expected = def.snippet().transformed(&p).unwrap();
    let matrix = components::matrix(def, &p).unwrap();
    let id = e.place_component(e.revision(), d, 2, matrix).unwrap();
    let actual = instance(&e, id);
    for ((_, a), (_, b)) in geometry::edit_points(actual)
        .into_iter()
        .zip(geometry::edit_points(&expected[0]))
    {
        assert!((a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    }
    let i = &e.document().components.instances[&id];
    let restored = components::placement(&e.document().components.definitions[&d], i).unwrap();
    assert!((restored.width - p.width).abs() < 1e-9 && (restored.angle - p.angle).abs() < 1e-9);
}

#[test]
fn joining_and_splitting_definition_contours_keep_instance_point_aliases() {
    let mut doc = FontDocument::new("Topology", "AB").unwrap();
    let mut a = font_core::snippets::builtins()[&1].contours[0].clone();
    a.closed = false;
    let mut b = a.clone();
    b.start[0] += 600.;
    for s in &mut b.segments {
        if let font_core::Segment::Line { end } = s {
            end[0] += 600.;
        }
    }
    doc.glyphs.get_mut(&1).unwrap().contours = vec![a, b];
    let mut e = Editor::new(doc).unwrap();
    let d = e
        .capture_component(e.revision(), 1, &[0, 1].into(), info("Join"))
        .unwrap();
    let i = e
        .place_component(e.revision(), d, 2, [1., 0., 0., 1., 0., 0.])
        .unwrap();
    let old = e.document().glyphs[&2].contours.clone();
    let references: Vec<_> = old
        .iter()
        .map(|c| NodeRef {
            document: e.document().document_uuid,
            node: identity::node_id(c, EditPoint::Anchor(1)).unwrap(),
        })
        .collect();
    let mut source = e.document().glyphs[&1].clone();
    let joined =
        font_core::path_edit::join(&source.contours[0], false, &source.contours[1], false).unwrap();
    source.contours = vec![joined];
    e.replace_glyph(e.revision(), source).unwrap();
    e.update_component(e.revision(), d, 1, &[0].into()).unwrap();
    let joined = e.document().clone();
    assert_eq!(joined.glyphs[&2].contours.len(), 1);
    for c in &old {
        assert!(identity::resolve_contour(&joined, c.identity.contours[0]).is_some());
    }
    for r in &references {
        assert!(identity::resolve(&joined, *r).is_some());
    }
    let mut source = joined.glyphs[&1].clone();
    source.contours = font_core::path_edit::split(&source.contours[0], 2).unwrap();
    e.replace_glyph(e.revision(), source).unwrap();
    e.update_component(e.revision(), d, 1, &[0, 1].into())
        .unwrap();
    for r in references {
        assert!(identity::resolve(e.document(), r).is_some());
    }
    assert_eq!(e.document().glyphs[&2].contours.len(), 2);
    assert_eq!(
        e.document().components.instances[&i].matrix,
        [1., 0., 0., 1., 0., 0.]
    );
}
