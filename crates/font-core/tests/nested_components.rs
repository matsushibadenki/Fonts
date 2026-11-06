use font_core::{
    geometry::{self, EditPoint},
    identity::{self, NodeRef, Uuid},
    snippets::Info,
    storage, Contour, Editor, Error, FontDocument, Segment,
};
fn info(n: &str) -> Info {
    Info {
        name: n.into(),
        tags: "".into(),
        category: "radical".into(),
        source: "".into(),
        license: "".into(),
        favorite: false,
    }
}
fn editor() -> Editor {
    let mut d = FontDocument::new("Nested", "ABCDEFGHIJKLMNOPQRST").unwrap();
    d.glyphs.get_mut(&1).unwrap().contours = vec![Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [100., 0.],
        segments: vec![
            Segment::Quadratic {
                control: [300., 600.],
                end: [500., 0.],
            },
            Segment::Line { end: [100., 0.] },
        ],
        closed: true,
    }];
    Editor::new(d).unwrap()
}
fn capture(e: &mut Editor, g: u32) -> Uuid {
    let selected = (0..e.document().glyphs[&g].contours.len()).collect();
    e.capture_component(e.revision(), g, &selected, info("Part"))
        .unwrap()
}
fn place(e: &mut Editor, id: Uuid, g: u32, m: [f64; 6]) -> Uuid {
    e.place_component(e.revision(), id, g, m).unwrap()
}
const ID: [f64; 6] = [1., 0., 0., 1., 0., 0.];
fn chain() -> (Editor, Uuid, Uuid, Uuid) {
    let mut e = editor();
    let c = capture(&mut e, 1);
    place(&mut e, c, 2, [2., 0., 0., 1., 100., 0.]);
    let b = capture(&mut e, 2);
    place(&mut e, b, 3, [0., 1., -1., 0., 1000., 0.]);
    let a = capture(&mut e, 3);
    place(&mut e, a, 4, [0.5, 0., 0., 0.5, 0., 20.]);
    place(&mut e, a, 5, ID);
    (e, c, b, a)
}
#[test]
fn recursive_updates_keep_transforms_ids_and_one_atomic_history_step() {
    let (mut e, c, b, a) = chain();
    assert_eq!(e.document().schema_version, 4);
    assert_eq!(e.document().components.definitions[&b].children.len(), 1);
    assert_eq!(e.document().components.definitions[&a].children.len(), 1);
    let r = NodeRef {
        definition: None,
        document: e.document().document_uuid,
        node: identity::node_id(&e.document().glyphs[&4].contours[0], EditPoint::Anchor(0))
            .unwrap(),
    };
    let mut g = e.document().glyphs[&1].clone();
    geometry::move_point(&mut g.contours[0], EditPoint::Anchor(0), [120., 10.]).unwrap();
    geometry::split_segment(&mut g.contours[0], 0).unwrap();
    e.replace_glyph(e.revision(), g).unwrap();
    let before = e.document().clone();
    e.update_component(e.revision(), c, 1, &[0].into()).unwrap();
    let after = e.document().clone();
    assert_eq!(after.glyphs[&4].contours[0].start, [495., 190.]);
    assert_eq!(after.glyphs[&5].contours[0].start, [990., 340.]);
    assert!(identity::resolve(&after, r).is_some());
    after.validate().unwrap();
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &after);
    // The parent's source instance and its cache acquire different new IDs. Explicit refresh reuses
    // the old child mapping rather than replacing IDs on every ancestor update.
    let revision = e.revision();
    e.update_component(revision, b, 2, &[0].into()).unwrap();
    assert_eq!(e.revision(), revision);
    assert_eq!(e.document(), &after);
}
#[test]
fn direct_indirect_and_serialized_cycles_are_rejected_without_changing_history() {
    let (mut e, c, _b, a) = chain();
    let before = e.document().clone();
    let revision = e.revision();
    assert_eq!(
        e.update_component(revision, c, 2, &[0].into()),
        Err(Error::ComponentCycle)
    );
    assert_eq!(
        e.update_component(revision, c, 4, &[0].into()),
        Err(Error::ComponentCycle)
    );
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), revision);
    let mut bad = before.clone();
    let child = bad
        .components
        .definitions
        .get_mut(&a)
        .unwrap()
        .children
        .values_mut()
        .next()
        .unwrap();
    child.definition = a;
    assert_eq!(bad.validate(), Err(Error::ComponentCycle));
    let mut missing = before;
    missing
        .components
        .definitions
        .get_mut(&a)
        .unwrap()
        .children
        .values_mut()
        .next()
        .unwrap()
        .definition = Uuid::new_v4();
    assert_eq!(missing.validate(), Err(Error::InvalidDocument));
}
#[test]
fn child_detach_remove_transform_and_dependency_delete_protection() {
    let (mut e, c, b, _a) = chain();
    let child = *e.document().components.definitions[&b]
        .children
        .keys()
        .next()
        .unwrap();
    let roots: Vec<_> = e
        .document()
        .components
        .instances
        .iter()
        .filter_map(|(&id, i)| (i.definition == c).then_some(id))
        .collect();
    for id in roots {
        e.detach_component(e.revision(), id).unwrap();
    }
    let original = e.document().clone();
    assert_eq!(
        e.remove_component(e.revision(), c),
        Err(Error::ComponentInUse)
    );
    e.transform_child(e.revision(), b, child, [2., 0., 0., 1., 200., 50.])
        .unwrap();
    let changed = e.document().clone();
    assert_ne!(changed.glyphs[&4], original.glyphs[&4]);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &original);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &changed);
    e.detach_child(e.revision(), b, child, false).unwrap();
    let independent = e.document().glyphs[&4].clone();
    let mut g = e.document().glyphs[&1].clone();
    geometry::move_point(&mut g.contours[0], EditPoint::Control1(0), [300., 300.]).unwrap();
    e.replace_glyph(e.revision(), g).unwrap();
    e.update_component(e.revision(), c, 1, &[0].into()).unwrap();
    assert_eq!(e.document().glyphs[&4], independent);
}
#[test]
fn repeated_children_diamonds_and_partial_selection() {
    let mut e = editor();
    let c = capture(&mut e, 1);
    place(&mut e, c, 2, ID);
    place(&mut e, c, 2, [1., 0., 0., 1., 600., 0.]);
    let b = capture(&mut e, 2);
    place(&mut e, b, 3, ID);
    let before = e.document().clone();
    assert_eq!(
        e.capture_component(e.revision(), 3, &[0].into(), info("Partial")),
        Err(Error::ComponentPartial)
    );
    assert_eq!(e.document(), &before);
    let a = capture(&mut e, 3);
    place(&mut e, a, 4, ID);
    assert_eq!(e.document().components.definitions[&b].children.len(), 2);
    let one = *e.document().components.definitions[&b]
        .children
        .keys()
        .next()
        .unwrap();
    e.detach_child(e.revision(), b, one, true).unwrap();
    assert_eq!(e.document().glyphs[&4].contours.len(), 1);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document().glyphs[&4].contours.len(), 2);
}
#[test]
fn depth_and_expansion_budgets_are_atomic() {
    let mut e = editor();
    let mut id = capture(&mut e, 1);
    for glyph in 2..=16 {
        place(&mut e, id, glyph, ID);
        id = capture(&mut e, glyph);
    }
    place(&mut e, id, 17, ID);
    let before = e.document().clone();
    assert_eq!(
        e.capture_component(e.revision(), 17, &[0].into(), info("Too deep")),
        Err(Error::ComponentDepth)
    );
    assert_eq!(e.document(), &before);
    let mut e = editor();
    let mut id = capture(&mut e, 1);
    for glyph in 2..=8 {
        place(&mut e, id, glyph, ID);
        place(&mut e, id, glyph, [1., 0., 0., 1., 1., 0.]);
        id = capture(&mut e, glyph);
    }
    place(&mut e, id, 9, ID);
    place(&mut e, id, 9, ID);
    let before = e.document().clone();
    let selection = (0..e.document().glyphs[&9].contours.len()).collect();
    assert_eq!(
        e.capture_component(e.revision(), 9, &selection, info("Expansion")),
        Err(Error::TooLarge)
    );
    assert_eq!(e.document(), &before);
}
#[test]
fn v4_save_restart_and_all_exports_use_the_same_materialized_outlines() {
    let (e, _, _, _) = chain();
    let path = std::env::temp_dir().join(format!("fonts-nested-{}.json", Uuid::new_v4()));
    storage::save(&path, e.document()).unwrap();
    let (doc, migrated) = storage::load_with_migration(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!migrated);
    assert_eq!(&doc, e.document());
    let mut flattened = doc.clone();
    flattened.components = Default::default();
    for format in [
        font_core::font_formats::Format::Ttf,
        font_core::font_formats::Format::Otf,
        font_core::font_formats::Format::Woff2,
    ] {
        assert_eq!(
            font_core::font_formats::export(&doc, format).unwrap(),
            font_core::font_formats::export(&flattened, format).unwrap()
        );
    }
    let proof = font_core::proof::subset(&doc, "ABCDE").unwrap();
    assert_eq!(proof.glyphs[&4].contours, doc.glyphs[&4].contours);
    assert!(font_core::proof::prepare(proof, "ABCDE").is_ok());
}
#[test]
fn nested_identity_tampering_is_rejected() {
    let (e, _, b, _) = chain();
    let source = e.document().clone();
    let mut bad = source.clone();
    let child = bad
        .components
        .definitions
        .get_mut(&b)
        .unwrap()
        .children
        .values_mut()
        .next()
        .unwrap();
    child.matrix[4] += 1.;
    assert!(bad.validate().is_err());
    let mut bad = source.clone();
    let child = bad
        .components
        .definitions
        .get_mut(&b)
        .unwrap()
        .children
        .values_mut()
        .next()
        .unwrap();
    let id = *child.nodes.values().next().unwrap();
    for v in std::sync::Arc::make_mut(&mut child.nodes).values_mut() {
        *v = id;
    }
    assert!(bad.validate().is_err());
    let mut bad = source.clone();
    let old = *bad.components.definitions[&b]
        .children
        .keys()
        .next()
        .unwrap();
    let child = bad
        .components
        .definitions
        .get_mut(&b)
        .unwrap()
        .children
        .remove(&old)
        .unwrap();
    bad.components
        .definitions
        .get_mut(&b)
        .unwrap()
        .children
        .insert(source.document_uuid, child);
    assert!(bad.validate().is_err());
}

#[test]
fn dedicated_definition_edit_without_source_preserves_scoped_ids_history_and_storage() {
    let (mut e, leaf, parent, _) = chain();
    let mut source = e.document().glyphs[&1].clone();
    source.contours.clear();
    e.replace_glyph(e.revision(), source).unwrap();
    let before = e.document().clone();
    let mut contours = before.components.definitions[&leaf]
        .contours
        .as_ref()
        .clone();
    let reference = NodeRef {
        document: before.document_uuid,
        definition: Some(leaf),
        node: identity::node_id(&contours[0], EditPoint::Anchor(0)).unwrap(),
    };
    geometry::move_point(&mut contours[0], EditPoint::Anchor(0), [120., 10.]).unwrap();
    geometry::split_segment(&mut contours[0], 0).unwrap();
    e.edit_component_contours(e.revision(), leaf, contours)
        .unwrap();
    let after = e.document().clone();
    assert!(after.glyphs[&1].contours.is_empty());
    assert_eq!(after.glyphs.len(), before.glyphs.len());
    assert_ne!(after.glyphs[&4], before.glyphs[&4]);
    assert_ne!(
        after.components.definitions[&parent],
        before.components.definitions[&parent]
    );
    assert!(identity::resolve(&after, reference).is_none());
    assert!(identity::resolve_definition_reference(&after, 0, reference).is_some());
    let rev = e.revision();
    e.edit_component_contours(
        rev,
        leaf,
        after.components.definitions[&leaf]
            .contours
            .as_ref()
            .clone(),
    )
    .unwrap();
    assert_eq!(e.revision(), rev);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &after);
    let path = std::env::temp_dir().join(format!("definition-{}.json", Uuid::new_v4()));
    storage::save(&path, &after).unwrap();
    let loaded = storage::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, after);
    let serialized = serde_json::to_string(&reference).unwrap();
    let restored = serde_json::from_str(&serialized).unwrap();
    assert!(identity::resolve_definition_reference(&loaded, 0, restored).is_some());
    assert!(identity::resolve_definition_reference(
        &loaded,
        0,
        NodeRef {
            document: Uuid::new_v4(),
            ..reference
        }
    )
    .is_none());
}

#[test]
fn definition_child_insertion_is_atomic_locked_and_cycle_checked() {
    let mut e = editor();
    let a = capture(&mut e, 1);
    let b = capture(&mut e, 1);
    place(&mut e, a, 2, ID);
    let before = e.document().clone();
    let child = e.insert_component_child(e.revision(), a, b, ID).unwrap();
    let after = e.document().clone();
    assert_eq!(after.glyphs[&2].contours.len(), 2);
    let mut contours = after.components.definitions[&a].contours.as_ref().clone();
    contours.pop();
    assert_eq!(
        e.edit_component_contours(e.revision(), a, contours),
        Err(Error::ComponentLinked)
    );
    assert_eq!(
        e.edit_component_contours(e.revision(), b, vec![]),
        Err(Error::ComponentEmpty)
    );
    assert_eq!(
        e.insert_component_child(e.revision(), b, a, ID),
        Err(Error::ComponentCycle)
    );
    assert_eq!(e.document(), &after);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &after);
    e.detach_child(e.revision(), a, child, false).unwrap();
    let mut contours = e.document().components.definitions[&a]
        .contours
        .as_ref()
        .clone();
    geometry::move_point(&mut contours[1], EditPoint::Anchor(0), [150., 30.]).unwrap();
    e.edit_component_contours(e.revision(), a, contours)
        .unwrap();
    e.document().validate().unwrap();
}

#[test]
fn definition_is_editable_when_original_glyph_is_absent() {
    let mut e = editor();
    let id = capture(&mut e, 1);
    let mut doc = e.document().clone();
    doc.glyphs.remove(&1);
    let mut e = Editor::new(doc).unwrap();
    let mut contours = e.document().components.definitions[&id]
        .contours
        .as_ref()
        .clone();
    geometry::split_segment(&mut contours[0], 0).unwrap();
    e.edit_component_contours(e.revision(), id, contours)
        .unwrap();
    assert!(!e.document().glyphs.contains_key(&1));
    assert_eq!(
        e.document().components.definitions[&id].contours[0]
            .segments
            .len(),
        3
    );
    e.document().validate().unwrap();
}
