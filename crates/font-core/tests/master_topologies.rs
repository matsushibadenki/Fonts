use font_core::{
    source_font::{master_project::Edit, master_topology::Operation, variable, SourceFont},
    Editor,
};
#[test]
fn split_and_delete_update_every_master_and_survive_undo_and_project_roundtrip() {
    for bytes in [
        include_bytes!("data/variations/gvar.ttf").as_slice(),
        include_bytes!("data/variations/cff2.otf").as_slice(),
    ] {
        let mut e = Editor::new(SourceFont::variable_document(bytes, 0, None).unwrap()).unwrap();
        e.edit_source_master(
            0,
            Edit::Register {
                name: "Default".into(),
            },
        )
        .unwrap();
        let id = e
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
        e.edit_source_master(e.revision(), Edit::Capture { id, glyph: 1 })
            .unwrap();
        e.edit_source_variations(
            e.revision(),
            variable::Edit::Coordinates {
                values: [("wght".into(), 900.), ("wdth".into(), 125.)].into(),
            },
        )
        .unwrap();
        e.edit_source_master(
            e.revision(),
            Edit::Register {
                name: "Wide bold".into(),
            },
        )
        .unwrap();
        let second = e
            .document()
            .source_font
            .as_ref()
            .unwrap()
            .masters
            .as_ref()
            .unwrap()
            .masters
            .entries[1]
            .id;
        e.edit_source_master(
            e.revision(),
            Edit::Capture {
                id: second,
                glyph: 1,
            },
        )
        .unwrap();
        let before = e.document().clone();
        let source = before.source_font.as_ref().unwrap();
        let shape = source
            .masters
            .as_ref()
            .unwrap()
            .shape(source, id, 1)
            .unwrap();
        let anchor = shape.contours[0].identity.anchors[1].clone();
        e.edit_source_master(
            e.revision(),
            Edit::Topology {
                glyph: 1,
                operation: Operation::Split {
                    contour: 0,
                    segment: 0,
                },
            },
        )
        .unwrap();
        let split = e.document().clone();
        let source = split.source_font.as_ref().unwrap();
        let actual = source
            .masters
            .as_ref()
            .unwrap()
            .shape(source, id, 1)
            .unwrap();
        assert_eq!(
            actual.contours[0].segments.len(),
            shape.contours[0].segments.len() + 1
        );
        assert_eq!(actual.contours[0].identity.anchors[2], anchor);
        let project = source.masters.as_ref().unwrap();
        let other = project.shape(source, second, 1).unwrap();
        let original_source = before.source_font.as_ref().unwrap();
        let original_other = original_source
            .masters
            .as_ref()
            .unwrap()
            .shape(original_source, second, 1)
            .unwrap();
        assert_eq!(
            other.contours[0].segments.len(),
            original_other.contours[0].segments.len() + 1
        );
        assert_eq!(other.contours[0].identity, actual.contours[0].identity);
        assert_ne!(other.contours[0].segments, actual.contours[0].segments);
        let out = source.export().unwrap();
        source.verify_output(&out, false).unwrap();
        let web = font_core::font_formats::export(&split, font_core::font_formats::Format::Woff2)
            .unwrap();
        source.verify_output(&web, true).unwrap();

        let decoded: font_core::FontDocument =
            serde_json::from_slice(&serde_json::to_vec(&split).unwrap()).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded, split);
        e.edit_source_master(
            e.revision(),
            Edit::Topology {
                glyph: 1,
                operation: Operation::Delete {
                    contour: 0,
                    segment: 0,
                },
            },
        )
        .unwrap();
        let source = e.document().source_font.as_ref().unwrap();
        let deleted = source
            .masters
            .as_ref()
            .unwrap()
            .shape(source, id, 1)
            .unwrap();
        assert_eq!(deleted.contours[0].segments, shape.contours[0].segments);
        assert_eq!(deleted.contours[0].identity.anchors[1], anchor);
        let other = source
            .masters
            .as_ref()
            .unwrap()
            .shape(source, second, 1)
            .unwrap();
        assert_eq!(
            other.contours[0].segments,
            original_other.contours[0].segments
        );
        assert_eq!(other.contours[0].identity, deleted.contours[0].identity);

        source
            .verify_output(&source.export().unwrap(), false)
            .unwrap();
        e.undo(e.revision()).unwrap();
        assert_eq!(e.document(), &split);
        e.undo(e.revision()).unwrap();
        assert_eq!(e.document(), &before);
    }
}
