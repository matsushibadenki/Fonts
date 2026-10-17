use font_core::{geometry, storage, Contour, Editor, Error, FontDocument, GlyphStatus, Segment};

fn outline() -> Vec<Contour> {
    vec![
        Contour {
            smooth: Default::default(),
            start: [0., 0.],
            segments: vec![
                Segment::Line { end: [500., 0.] },
                Segment::Quadratic {
                    control: [250., 1000.],
                    end: [0., 0.],
                },
            ],
            closed: true,
        },
        Contour {
            smooth: Default::default(),
            start: [200., 100.],
            segments: vec![Segment::Cubic {
                control1: [200., 200.],
                control2: [300., 200.],
                end: [200., 100.],
            }],
            closed: true,
        },
    ]
}

#[test]
fn unicode_sets_are_deduplicated_and_space_can_be_reviewed() {
    let doc = FontDocument::new("試作字体", "AA あ中😀").unwrap();
    assert_eq!(doc.glyphs.len(), 6);
    let mut editor = Editor::new(doc).unwrap();
    let mut space = editor.document().glyphs[&2].clone();
    assert_eq!(space.codepoints.iter().next(), Some(&' '));
    space.status = GlyphStatus::Reviewed;
    editor.replace_glyph(0, space).unwrap();
    assert!(editor.document().glyphs[&2].contours.is_empty());
}

#[test]
fn stale_window_edits_and_invalid_changes_are_atomic() {
    let mut editor = Editor::new(FontDocument::new("Demo", "AB").unwrap()).unwrap();
    let original = editor.document().clone();
    let mut a = original.glyphs[&1].clone();
    a.contours = outline();
    assert_eq!(editor.replace_glyph(0, a.clone()), Ok(1));
    assert_eq!(editor.replace_glyph(0, a.clone()), Err(Error::Conflict));
    a.advance = f64::NAN;
    assert_eq!(editor.replace_glyph(1, a), Err(Error::InvalidDocument));
    assert_eq!(editor.revision(), 1);
    assert_eq!(editor.undo(1), Ok(2));
    assert_eq!(editor.document(), &original);
    assert_eq!(editor.redo(2), Ok(3));
    assert_eq!(editor.document().glyphs[&1].contours, outline());
    editor.undo(3).unwrap();
    let mut b = editor.document().glyphs[&2].clone();
    b.advance = 700.;
    editor.replace_glyph(4, b).unwrap();
    assert_eq!(editor.redo(5), Err(Error::EmptyHistory));
}

#[test]
fn duplicate_unicode_and_notdef_mutation_are_rejected() {
    let mut editor = Editor::new(FontDocument::new("Demo", "AB").unwrap()).unwrap();
    let mut b = editor.document().glyphs[&2].clone();
    b.codepoints = ['A'].into();
    assert_eq!(editor.replace_glyph(0, b), Err(Error::InvalidDocument));
    let mut notdef = editor.document().glyphs[&0].clone();
    notdef.name = "renamed".into();
    assert_eq!(editor.replace_glyph(0, notdef), Err(Error::InvalidDocument));
    assert_eq!(editor.revision(), 0);
}

#[test]
fn display_adapter_preserves_holes_closure_and_quadratic_shape() {
    let contours = outline();
    let path = geometry::display_path(&contours).unwrap();
    let parsed = lumapaint_core::bezier::cubic_contours(&path).unwrap();
    assert_eq!(parsed.len(), 2);
    assert!(parsed.iter().all(|(_, closed)| *closed));
    // First quadratic: midpoint is (250, 500), reflected into display Y-down.
    let segment = [
        parsed[0].0[3],
        parsed[0].0[4],
        parsed[0].0[5],
        parsed[0].0[6],
    ];
    let midpoint = lumapaint_core::bezier::evaluate(&segment, 0.5);
    assert!((midpoint[0] - 250.).abs() < 0.001);
    assert!((midpoint[1] + 500.).abs() < 0.001);
    assert_eq!(contours, outline());
    let mut open = contours[0].clone();
    open.closed = false;
    assert!(!geometry::display_path(&[open]).unwrap().ends_with('Z'));
}

#[test]
fn save_reload_preserves_f64_curves_and_failed_save_keeps_old_file() {
    let dir = std::env::temp_dir().join(format!("fonts-workflow-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("demo.fontproj.json");
    let mut doc = FontDocument::new("字体 Demo", "Aあ字").unwrap();
    doc.glyphs.get_mut(&1).unwrap().contours = outline();
    doc.glyphs.get_mut(&1).unwrap().advance = 600.123456789;
    storage::save(&path, &doc).unwrap();
    assert_eq!(storage::load(&path).unwrap(), doc);
    doc.family = "Changed".into();
    storage::save(&path, &doc).unwrap();
    let good = std::fs::read(&path).unwrap();
    doc.units_per_em = 0;
    assert_eq!(storage::save(&path, &doc), Err(Error::InvalidDocument));
    assert_eq!(std::fs::read(&path).unwrap(), good);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let mut json: serde_json::Value = serde_json::from_slice(&good).unwrap();
    json["schema_version"] = 99.into();
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(storage::load(&path), Err(Error::UnsupportedVersion));
    std::fs::write(&path, b"broken json").unwrap();
    assert_eq!(storage::load(&path), Err(Error::InvalidFile));
    std::fs::remove_dir_all(dir).unwrap();
}
