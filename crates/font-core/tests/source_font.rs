use font_core::{
    font_formats::{self, Format},
    source_font::SourceFont,
    storage, Editor, Error,
};
const COLR: &[u8] = include_bytes!("data/color/colrv0.ttf");
#[test]
fn palette_edits_are_independent_undoable_and_saved_with_source() {
    let doc = SourceFont::color_document(COLR, 0).unwrap();
    assert_eq!(doc.schema_version, 6);
    let mut e = Editor::new(doc).unwrap();
    let original = e.document().clone();
    let old = e
        .document()
        .source_font
        .as_ref()
        .unwrap()
        .palette_colors()
        .unwrap();
    e.edit_source_palette(0, 0, Some(0), Some([12, 34, 56, 78]))
        .unwrap();
    assert_eq!(
        e.document()
            .source_font
            .as_ref()
            .unwrap()
            .palette_colors()
            .unwrap()[0],
        [12, 34, 56, 78]
    );
    let file =
        std::env::temp_dir().join(format!("color-project-{}.json", e.document().document_uuid));
    storage::save(&file, e.document()).unwrap();
    let loaded = storage::load(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    assert_eq!(loaded, *e.document());
    let sfnt = font_formats::export(&loaded, Format::Ttf).unwrap();
    let actual = ttf_parser::Face::parse(&sfnt, 0).unwrap();
    let color = ttf_parser::cpal::Table::parse(
        actual
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CPAL"))
            .unwrap(),
    )
    .unwrap()
    .get(0, 0)
    .unwrap();
    assert_eq!(
        [color.red, color.green, color.blue, color.alpha],
        [12, 34, 56, 78]
    );
    let source = ttf_parser::Face::parse(COLR, 0).unwrap();
    for record in source.raw_face().table_records {
        if ![b"head", b"CPAL", b"DSIG"].contains(&&record.tag.to_bytes()) {
            assert_eq!(
                source.raw_face().table(record.tag),
                actual.raw_face().table(record.tag)
            );
        }
    }
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &original);
    assert_eq!(
        e.document()
            .source_font
            .as_ref()
            .unwrap()
            .palette_colors()
            .unwrap(),
        old
    );
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &loaded);
    assert_eq!(
        e.add_characters(e.revision(), &['Z'].into()),
        Err(Error::SourceFontLocked)
    );
    let revision = e.revision();
    assert!(e
        .edit_source_palette(revision, 0, Some(u16::MAX), Some([0; 4]))
        .is_err());
    assert_eq!(e.revision(), revision);
    let woff = font_formats::export(e.document(), Format::Woff2).unwrap();
    let decoded = font_formats::woff2::decode(&woff).unwrap();
    let f = ttf_parser::Face::parse(&decoded, 0).unwrap();
    assert_eq!(
        ttf_parser::cpal::Table::parse(
            f.raw_face()
                .table(ttf_parser::Tag::from_bytes(b"CPAL"))
                .unwrap()
        )
        .unwrap()
        .get(0, 0)
        .unwrap()
        .alpha,
        78
    );
    assert_eq!(
        font_formats::export(e.document(), Format::Otf).unwrap_err(),
        "source_font_format"
    );
}
#[test]
fn every_color_kind_can_be_saved_without_source_file_or_vector_outlines() {
    for bytes in [
        include_bytes!("data/color/sbix.ttf").as_slice(),
        include_bytes!("data/color/cbdt.ttf").as_slice(),
        include_bytes!("data/color/svg.ttf").as_slice(),
        include_bytes!("data/color/colrv1.ttf").as_slice(),
        include_bytes!("data/color/colrv0.otf").as_slice(),
    ] {
        let doc = SourceFont::color_document(bytes, 0).unwrap();
        let e = Editor::new(doc).unwrap();
        let serialized = serde_json::to_vec(e.document()).unwrap();
        let loaded: font_core::FontDocument = serde_json::from_slice(&serialized).unwrap();
        loaded.validate().unwrap();
        let out = loaded.source_font.unwrap().export().unwrap();
        font_core::color_fonts::verify(&out, bytes, 0).unwrap();
    }
}
