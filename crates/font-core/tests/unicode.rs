use font_core::{unicode, Editor, Error, FontDocument};

#[test]
fn scalar_input_preserves_supplementary_combining_and_private_characters() {
    let literal = unicode::parse("あ中😀𠀀e\u{301}\u{e000}\u{10ffff}😀", false).unwrap();
    assert_eq!(literal.len(), 8);
    let codes = unicode::parse("U+3042, 4E2D 0x1F600 U+20000 0065 0301 E000 10FFFF", true).unwrap();
    assert_eq!(literal, codes);
    assert_eq!(
        unicode::parse("U+0041–U+0043", true)
            .unwrap()
            .into_iter()
            .collect::<String>(),
        "ABC"
    );
    assert_eq!(unicode::parse("AB", false).unwrap().len(), 2);
}

#[test]
fn invalid_scalar_ranges_are_rejected_without_partial_results() {
    for input in [
        "",
        "U+D800",
        "DFFF",
        "110000",
        "-1",
        "U+0043-U+0041",
        "D7FF-E000",
        "41-42-43",
        "U+",
        "😀",
        "0x+41",
    ] {
        assert_eq!(
            unicode::parse(input, true),
            Err(Error::InvalidUnicode),
            "{input}"
        );
    }
    assert_eq!(unicode::parse("0000-10FFFF", true), Err(Error::TooLarge));
    assert_eq!(unicode::parse("0000-0FFF", true).unwrap().len(), 4096);
    assert_eq!(unicode::parse("0000-1000", true).unwrap().len(), 4097);
    assert_eq!(unicode::parse("10000-1FFFE", true), Err(Error::TooLarge));
}

#[test]
fn addition_is_atomic_deduplicated_and_undoable_with_lossless_storage() {
    let mut e = Editor::new(FontDocument::new("日本語 中文 😀", "A").unwrap()).unwrap();
    let chars = unicode::parse("A😀𠀀e\u{301}", false).unwrap();
    let ids = e.add_characters(0, &chars).unwrap();
    assert_eq!(ids.len(), 4);
    assert_eq!(e.document().glyphs.len(), 6);
    assert_eq!(e.add_characters(0, &chars), Err(Error::Conflict));
    assert!(e.add_characters(1, &chars).unwrap().is_empty());
    assert_eq!(e.revision(), 1);
    let added = e.document().clone();
    assert!(added.glyphs.values().any(|g| g.name == "u1F600"));
    let encoded = serde_json::to_string(&added).unwrap();
    let restored: FontDocument = serde_json::from_str(&encoded).unwrap();
    restored.validate().unwrap();
    assert_eq!(added, restored);
    let path = std::env::temp_dir().join(format!(
        "fonts-unicode-roundtrip-{}.json",
        std::process::id()
    ));
    font_core::storage::save(&path, &added).unwrap();
    assert_eq!(font_core::storage::load(&path).unwrap(), added);
    std::fs::remove_file(path).unwrap();
    e.undo(1).unwrap();
    assert_eq!(e.document().glyphs.len(), 2);
    e.redo(2).unwrap();
    assert_eq!(e.document(), &added);
    e.undo(3).unwrap();
    e.add_characters(4, &unicode::parse("あ", false).unwrap())
        .unwrap();
    assert!(!e.can_redo());
}

#[test]
fn generated_names_do_not_collide_with_unencoded_glyph_names() {
    let mut doc = FontDocument::new("Test", "A").unwrap();
    doc.glyphs.get_mut(&1).unwrap().name = "u1F600".into();
    let mut e = Editor::new(doc).unwrap();
    e.add_characters(0, &unicode::parse("😀", false).unwrap())
        .unwrap();
    e.document().validate().unwrap();
    assert_eq!(e.document().glyphs[&2].name, "u1F600.1");
}
