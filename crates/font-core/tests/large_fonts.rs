use font_core::{
    font_formats::{self, Format},
    font_io, unicode, Editor, FontDocument,
};

#[test]
fn twenty_thousand_cjk_glyphs_edit_and_roundtrip_with_bmp_compatibility() {
    let mut editor = Editor::new(FontDocument::new("日本語 中文 QA", "").unwrap()).unwrap();
    let chars = unicode::parse("4E00-9FFF 20000-200FF", true).unwrap();
    editor.add_characters(0, &chars).unwrap();
    let id = editor
        .document()
        .glyphs
        .values()
        .find(|g| g.codepoints.contains(&'中'))
        .unwrap()
        .id;
    let mut glyph = editor.document().glyphs[&id].clone();
    glyph.advance = 999.;
    editor.replace_glyph(1, glyph).unwrap();
    editor.undo(2).unwrap();
    editor.redo(3).unwrap();
    for format in [Format::Ttf, Format::Otf, Format::CidOtf] {
        let bytes = font_formats::export(editor.document(), format).unwrap();
        let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
        let bmp = face
            .tables()
            .cmap
            .unwrap()
            .subtables
            .into_iter()
            .find(|s| s.platform_id == ttf_parser::PlatformId::Windows && s.encoding_id == 1)
            .unwrap();
        for c in &chars {
            let gid = face.glyph_index(*c).unwrap();
            if (*c as u32) < 0xffff {
                assert_eq!(bmp.glyph_index(*c as u32), Some(gid));
            }
        }
        let restored = font_io::import(&bytes, 0).unwrap();
        assert_eq!(restored.glyphs.len(), editor.document().glyphs.len());
        assert_eq!(restored.glyphs[&id].advance, 999.);
    }
}

#[test]
fn dense_bmp_with_shuffled_glyph_ids_uses_glyph_array() {
    let chars: String = (0x4e00..0x9fff).filter_map(char::from_u32).collect();
    let mut doc = FontDocument::new("Shuffled CJK", &chars).unwrap();
    let n = doc.glyphs.len() as u32 - 1;
    for (id, glyph) in doc.glyphs.iter_mut().filter(|(id, _)| **id != 0) {
        glyph.codepoints = [char::from_u32(0x4e00 + (n - *id)).unwrap()].into();
    }
    let bytes = font_io::export(&doc).unwrap();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    let bmp = face
        .tables()
        .cmap
        .unwrap()
        .subtables
        .into_iter()
        .find(|s| s.encoding_id == 1)
        .unwrap();
    for c in chars.chars() {
        assert_eq!(bmp.glyph_index(c as u32), face.glyph_index(c));
    }
}

#[test]
fn sfnt_glyph_limit_is_inclusive_and_addition_above_it_is_atomic() {
    let chars = unicode::parse("10000-1FFFD", true).unwrap();
    assert_eq!(chars.len(), 65534);
    let mut editor = Editor::new(FontDocument::new("Maximum repertoire", "").unwrap()).unwrap();
    editor.add_characters(0, &chars).unwrap();
    let bytes = font_io::export(editor.document()).unwrap();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    assert_eq!(face.number_of_glyphs(), 65535);
    assert_eq!(face.glyph_index('\u{1fffd}').unwrap().0, 65534);
    assert_eq!(
        editor.add_characters(1, &['中'].into()),
        Err(font_core::Error::TooLarge)
    );
    assert_eq!(editor.document().glyphs.len(), 65535);
    assert_eq!(editor.revision(), 1);
}
