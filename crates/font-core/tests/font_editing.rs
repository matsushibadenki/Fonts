use font_core::{
    font_formats::{self, Format},
    font_io, unicode, Contour, Editor, Error, FontDocument, Segment,
};
#[test]
fn unicode_aliases_conflict_clear_and_history_keep_the_glyph_intact() {
    let mut doc = FontDocument::new("Mapping", "AB").unwrap();
    doc.glyphs.get_mut(&1).unwrap().cid = Some(42);
    let mut e = Editor::new(doc).unwrap();
    let before = e.document().clone();
    assert_eq!(
        e.assign_unicode(0, 1, ['B'].into()),
        Err(Error::UnicodeConflict)
    );
    assert_eq!(e.document(), &before);
    assert_eq!(e.revision(), 0);
    let codes = unicode::parse("0041 FF21 20000", true).unwrap();
    e.assign_unicode(0, 1, codes.clone()).unwrap();
    assert_eq!(e.document().glyphs[&1].cid, Some(42));
    assert_eq!(e.assign_unicode(1, 1, codes), Ok(1));
    for format in [Format::Ttf, Format::Otf, Format::Pfb] {
        let bytes = font_formats::export(e.document(), format).unwrap();
        let read = font_io::import(&bytes, 0).unwrap();
        for c in ['A', 'Ａ', '𠀀'] {
            assert!(
                read.glyphs.values().any(|g| g.codepoints.contains(&c)),
                "{format:?} {c}"
            );
        }
    }
    e.assign_unicode(1, 1, Default::default()).unwrap();
    assert!(e.document().glyphs[&1].codepoints.is_empty());
    for format in [Format::Pfb, Format::Cff] {
        let bytes = font_formats::export(e.document(), format).unwrap();
        let read = font_io::import(&bytes, 0).unwrap();
        assert!(
            read.glyphs[&1].codepoints.is_empty(),
            "cleared mapping was inferred again: {format:?}"
        );
    }

    e.undo(2).unwrap();
    assert_eq!(e.document().glyphs[&1].codepoints.len(), 3);
    e.undo(3).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(4).unwrap();
    assert_eq!(e.document().glyphs[&1].codepoints.len(), 3);
    assert_eq!(
        e.assign_unicode(5, 0, ['Q'].into()),
        Err(Error::InvalidDocument)
    );
    assert_eq!(
        e.assign_unicode(0, 1, Default::default()),
        Err(Error::Conflict)
    );
}
#[test]
fn metadata_is_one_transaction_interleaved_with_glyph_changes_and_survives_exports() {
    let mut e = Editor::new(FontDocument::new("Before", "A").unwrap()).unwrap();
    let before = e.properties();
    let mut p = before.clone();
    p.family = "日本語 中文 Edited".into();
    p.metadata.style = "Medium Italic".into();
    p.metadata.weight = 500;
    p.metadata.width = 5;
    p.metadata.italic = true;
    p.metadata.italic_angle = -12.;
    p.metadata.copyright = "Copyright QA".into();
    p.metadata.license = "QA license".into();
    p.metadata.license_url = "https://example.org/license".into();
    e.set_properties(0, p.clone()).unwrap();
    let mut g = e.document().glyphs[&1].clone();
    g.advance = 888.;
    e.replace_glyph(1, g).unwrap();
    e.undo(2).unwrap();
    assert_eq!(e.properties(), p);
    e.undo(3).unwrap();
    assert_eq!(e.properties(), before);
    e.redo(4).unwrap();
    e.redo(5).unwrap();
    assert_eq!(e.document().glyphs[&1].advance, 888.);
    let json = serde_json::to_vec(e.document()).unwrap();
    let read: FontDocument = serde_json::from_slice(&json).unwrap();
    assert_eq!(&read, e.document());
    for format in [Format::Ttf, Format::Otf] {
        let b = font_formats::export(e.document(), format).unwrap();
        let read = font_io::import(&b, 0).unwrap();
        assert_eq!(read.metadata, p.metadata);
        let face = ttf_parser::Face::parse(&b, 0).unwrap();
        assert_eq!(font_io::name(&face, 1), p.family);
    }
    let mut invalid = p.clone();
    invalid.metadata.width = 10;
    let unchanged = e.document().clone();
    assert_eq!(e.set_properties(6, invalid), Err(Error::InvalidDocument));
    assert_eq!(e.document(), &unchanged);
    assert_eq!(e.set_properties(6, p), Ok(6));
}
#[test]
fn presets_preserve_existing_outlines_are_atomic_and_enforce_total_capacity() {
    let mut d = FontDocument::new("Presets", "中").unwrap();
    let shape = Contour {
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![Segment::Line { end: [100., 100.] }],
        closed: true,
    };
    d.glyphs.get_mut(&1).unwrap().contours = vec![shape.clone()];
    let mut e = Editor::new(d).unwrap();
    assert_eq!(unicode::preset("ascii").unwrap().len(), 95);
    assert_eq!(unicode::preset("cjk").unwrap().len(), 20992);
    assert!(unicode::preset("cjk_ext_b").unwrap().contains(&'𠀀'));
    assert!(unicode::preset("not_a_preset").is_err());
    for (id, count) in unicode::preset_counts() {
        assert_eq!(unicode::preset(id).unwrap().len(), *count);
    }
    e.add_characters(0, &unicode::preset("cjk").unwrap())
        .unwrap();
    assert_eq!(e.document().glyphs[&1].contours, vec![shape]);
    assert_eq!(e.document().glyphs.len(), 20993);
    assert!(e
        .add_characters(1, &unicode::preset("cjk").unwrap())
        .unwrap()
        .is_empty());
    e.undo(1).unwrap();
    assert_eq!(e.document().glyphs.len(), 2);
    e.redo(2).unwrap();
    e.add_characters(3, &unicode::preset("cjk_ext_b").unwrap())
        .unwrap();
    let count = e.document().glyphs.len();
    assert_eq!(
        e.add_characters(4, &unicode::preset("cjk_ext_a").unwrap()),
        Err(Error::TooLarge)
    );
    assert_eq!(e.document().glyphs.len(), count);
    assert_eq!(e.revision(), 4);
}
