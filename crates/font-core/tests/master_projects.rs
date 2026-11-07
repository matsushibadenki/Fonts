use font_core::{
    font_formats::{self, Format},
    source_font::{master_project::Edit, SourceFont},
    storage, Editor,
};
const FONT: &[u8] = include_bytes!("data/variations/gvar.ttf");
#[test]
fn registered_master_geometry_survives_history_storage_and_all_output_paths() {
    let doc = SourceFont::variable_document(FONT, 0, None).unwrap();
    let mut editor = Editor::new(doc).unwrap();
    let original = editor.document().clone();
    editor
        .edit_source_master(
            0,
            Edit::Register {
                name: "Default".into(),
            },
        )
        .unwrap();
    let id = editor
        .document()
        .source_font
        .as_ref()
        .unwrap()
        .masters
        .as_ref()
        .unwrap()
        .masters
        .entries[0]
        .id;
    editor
        .edit_source_master(editor.revision(), Edit::Capture { id, glyph: 1 })
        .unwrap();
    let before = editor.document().clone();
    assert_eq!(
        before.source_font.as_ref().unwrap().export().unwrap(),
        original.source_font.as_ref().unwrap().export().unwrap()
    );
    let source = before.source_font.as_ref().unwrap();
    let project = source.masters.as_ref().unwrap();
    let mut shape = project.shape(source, id, 1).unwrap();
    let topology = &project.topologies[&1];
    let mut points = topology.physical_points(&shape.contours).unwrap();
    for p in &mut points {
        p[1] += 15.;
    }
    shape.contours = topology.contours(&points).unwrap();
    shape.advance += 20.;
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Replace {
                id,
                glyph: 1,
                shape: shape.clone(),
            },
        )
        .unwrap();
    let edited = editor.document().clone();
    assert_eq!(edited.schema_version, 7);
    editor.undo(editor.revision()).unwrap();
    assert_eq!(*editor.document(), before);
    editor.redo(editor.revision()).unwrap();
    assert_eq!(*editor.document(), edited);
    let path = std::env::temp_dir().join(format!("master-project-{}.json", id));
    storage::save(&path, editor.document()).unwrap();
    let loaded = storage::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded, edited);
    let source = loaded.source_font.as_ref().unwrap();
    assert_eq!(
        source
            .masters
            .as_ref()
            .unwrap()
            .shape(source, id, 1)
            .unwrap(),
        shape
    );
    for format in [Format::Ttf, Format::Woff2] {
        let bytes = font_formats::export(&loaded, format).unwrap();
        source
            .verify_output(&bytes, format == Format::Woff2)
            .unwrap();
    }
    let static_bytes = source.static_bytes().unwrap();
    let face = ttf_parser::Face::parse(&static_bytes, 0).unwrap();
    assert!(!face.is_variable());
    assert_eq!(
        face.glyph_hor_advance(ttf_parser::GlyphId(1)),
        Some(shape.advance as u16)
    );
    let static_doc = source.static_document().unwrap();
    assert_eq!(static_doc.glyphs[&1].advance, shape.advance);
    let old = editor.document().clone();
    assert!(editor
        .edit_source_master(
            editor.revision(),
            Edit::Register {
                name: "Duplicate".into()
            }
        )
        .is_err());
    assert_eq!(*editor.document(), old);
    let mut invalid = shape;
    invalid.contours[0].identity = Default::default();
    assert!(editor
        .edit_source_master(
            editor.revision(),
            Edit::Replace {
                id,
                glyph: 1,
                shape: invalid
            }
        )
        .is_err());
    assert_eq!(*editor.document(), old);
    while editor.can_undo() {
        editor.undo(editor.revision()).unwrap();
    }
    assert_eq!(*editor.document(), original);
}
