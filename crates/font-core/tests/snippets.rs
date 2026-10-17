use font_core::{
    font_formats::{self, Format},
    proof,
    snippets::{builtins, Info, Snippet, MAX_SNIPPETS},
    storage, Editor, Error, FontDocument, Segment,
};
use std::{collections::BTreeSet, sync::Arc};
fn editor() -> Editor {
    Editor::new(FontDocument::new("Snippets 日本語 中文", "AB").unwrap()).unwrap()
}
fn info() -> Info {
    Info {
        name: "口 偏旁".into(),
        tags: "へん 简体中文".into(),
        category: "radical".into(),
        source: "Original".into(),
        license: "CC0-1.0".into(),
        favorite: true,
    }
}
#[test]
fn original_templates_are_valid_and_upm_is_preserved_before_transform() {
    for source in builtins().values() {
        source.validate().unwrap();
        let p = source.placement(2000, [600., 700.]).unwrap();
        assert_eq!((p.width, p.height), (1000., 1000.));
        let mut rotated = p;
        rotated.angle = 90.;
        let cs = source.transformed(&rotated).unwrap();
        assert_eq!(cs.len(), source.contours.len());
        assert!(cs.iter().all(|c| c.closed));
    }
    let source = &builtins()[&2];
    let p = source.placement(1000, [300., 300.]).unwrap();
    let transformed = source.transformed(&p).unwrap();
    for (a, b) in font_core::geometry::edit_points(&transformed[0])
        .iter()
        .zip(font_core::geometry::edit_points(&source.contours[0]))
    {
        assert_eq!(a.0, b.0);
        assert!((a.1[0] - b.1[0]).hypot(a.1[1] - b.1[1]) < 1e-9);
    }
}
#[test]
fn transformed_curves_and_smooth_anchors_are_independent() {
    let mut e = editor();
    let s = builtins()[&2].clone();
    let mut p = s.placement(1000, [300., 300.]).unwrap();
    p.angle = 30.;
    p.width = 700.;
    p.height = 200.;
    e.insert_snippet(0, 1, &s, &p).unwrap();
    let placed = e.document().glyphs[&1].clone();
    assert_eq!(placed.contours[0].smooth, s.contours[0].smooth);
    assert_ne!(&placed.contours, &*s.contours);
    let mut changed = placed.clone();
    if let Segment::Cubic { control1, .. } = &mut changed.contours[0].segments[0] {
        control1[0] += 20.;
    }
    e.replace_glyph(1, changed).unwrap();
    assert_eq!(s, builtins()[&2]);
    e.undo(2).unwrap();
    assert_eq!(e.document().glyphs[&1], placed);
}
#[test]
fn capture_duplicate_metadata_delete_and_placement_have_independent_undo() {
    let mut e = editor();
    let s = &builtins()[&5];
    let p = s.placement(1000, [300., 300.]).unwrap();
    e.insert_snippet(0, 1, s, &p).unwrap();
    let id = e
        .capture_snippet(1, 1, &BTreeSet::from([0, 1]), info())
        .unwrap();
    let snapshot = e.document().clone();
    let source = snapshot.snippets.entries[&id].clone();
    let copy = e.add_snippet(2, source.clone()).unwrap();
    assert!(Arc::ptr_eq(
        &source.contours,
        &e.document().snippets.entries[&copy].contours
    ));
    let mut metadata = info();
    metadata.name = "新しい名前".into();
    e.update_snippet(3, id, metadata).unwrap();
    e.insert_snippet(4, 2, &source, &p).unwrap();
    let placed = e.document().glyphs[&2].contours.clone();
    e.remove_snippet(5, id).unwrap();
    let final_doc = e.document().clone();
    assert_eq!(e.document().glyphs[&2].contours, placed);
    e.undo(6).unwrap();
    assert!(e.document().snippets.entries.contains_key(&id));
    e.undo(7).unwrap();
    assert!(e.document().glyphs[&2].contours.is_empty());
    e.redo(8).unwrap();
    assert_eq!(e.document().glyphs[&2].contours, placed);
    while e.can_undo() {
        e.undo(e.revision()).unwrap();
    }
    assert!(e.document().snippets.is_empty());
    assert!(e.document().glyphs[&1].contours.is_empty());
    while e.can_redo() {
        e.redo(e.revision()).unwrap();
    }
    assert_eq!(e.document(), &final_doc);
}
#[test]
fn bad_selection_stale_revision_and_invalid_geometry_leave_history_unchanged() {
    let mut e = editor();
    let before = e.document().clone();
    assert!(e.capture_snippet(0, 1, &BTreeSet::new(), info()).is_err());
    assert!(e
        .capture_snippet(0, 1, &BTreeSet::from([9]), info())
        .is_err());
    assert_eq!(
        e.add_snippet(99, builtins()[&1].clone()),
        Err(Error::Conflict)
    );
    let mut p = builtins()[&1].placement(1000, [0., 0.]).unwrap();
    p.width = -1.;
    assert!(e.insert_snippet(0, 1, &builtins()[&1], &p).is_err());
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), 0);
    assert!(!e.can_undo());
}
#[test]
fn library_and_per_template_limits_are_atomic_and_old_projects_still_load() {
    let mut e = editor();
    for i in 0..MAX_SNIPPETS {
        e.add_snippet(i as u64, builtins()[&1].clone()).unwrap();
    }
    let before = e.document().clone();
    assert_eq!(
        e.add_snippet(MAX_SNIPPETS as u64, builtins()[&1].clone()),
        Err(Error::TooLarge)
    );
    assert_eq!(e.document(), &before);
    let mut s = builtins()[&1].clone();
    s.contours = Arc::new(vec![s.contours[0].clone(); 129]);
    assert!(s.validate().is_err());
    let mut s = builtins()[&1].clone();
    s.info.name.clear();
    assert!(s.validate().is_err());
    let mut dense = builtins()[&1].clone();
    let mut c = dense.contours[0].clone();
    c.segments = (0..16383)
        .map(|i| Segment::Line {
            end: if i % 2 == 0 { [50., 550.] } else { [550., 50.] },
        })
        .collect();
    dense.contours = Arc::new(vec![c]);
    dense.validate().unwrap();
    let mut budget = editor();
    for i in 0..4 {
        budget.add_snippet(i, dense.clone()).unwrap();
    }
    let before = budget.document().clone();
    assert_eq!(budget.add_snippet(4, dense), Err(Error::TooLarge));
    assert_eq!(budget.document(), &before);
    assert_eq!(budget.revision(), 4);
    let mut value = serde_json::to_value(FontDocument::new("Old", "A").unwrap()).unwrap();
    assert!(value.get("snippets").is_none());
    value.as_object_mut().unwrap().remove("reference_images");
    let old: FontDocument = serde_json::from_value(value).unwrap();
    old.validate().unwrap();
    assert!(old.snippets.is_empty());
}
#[test]
fn saved_templates_and_inserted_copies_survive_without_source_and_export_as_outlines() {
    let mut e = editor();
    let id = e
        .add_snippet(
            0,
            Snippet {
                info: info(),
                ..builtins()[&5].clone()
            },
        )
        .unwrap();
    let source = e.document().snippets.entries[&id].clone();
    let p = source.placement(1000, [300., 300.]).unwrap();
    e.insert_snippet(1, 1, &source, &p).unwrap();
    let path = std::env::temp_dir().join(format!("fonts-snippets-{}.json", std::process::id()));
    storage::save(&path, e.document()).unwrap();
    let loaded = storage::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(&loaded, e.document());
    let mut plain = loaded.clone();
    plain.snippets = Default::default();
    for format in [Format::Ttf, Format::Otf, Format::Cff] {
        assert_eq!(
            font_formats::export(&loaded, format).unwrap(),
            font_formats::export(&plain, format).unwrap()
        );
    }
    let prepared = proof::prepare(proof::subset(&loaded, "A").unwrap(), "A").unwrap();
    assert!(prepared.document.snippets.is_empty());
    let mut bad = serde_json::to_value(&loaded).unwrap();
    bad["snippets"]["entries"][id.to_string()]["upm"] = 0.into();
    let bad: FontDocument = serde_json::from_value(bad).unwrap();
    assert!(bad.validate().is_err());
}
