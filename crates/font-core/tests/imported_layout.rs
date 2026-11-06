use font_core::{
    font_formats::{self, Format},
    font_io,
    proof::{self, Direction, Orientation, Settings},
    storage, Editor, Error, FontDocument,
};
fn source() -> &'static [u8] {
    include_bytes!("data/layout/shaping.ttf")
}
fn document() -> FontDocument {
    Editor::new(font_io::import(source(), 0).unwrap())
        .unwrap()
        .document()
        .clone()
}
fn glyph(doc: &FontDocument, name: &str) -> u32 {
    doc.glyphs.values().find(|g| g.name == name).unwrap().id
}
#[test]
fn imported_tables_save_export_and_extended_metrics_keep_original_shaping() {
    let doc = document();
    assert_eq!(doc.schema_version, 5);
    assert!(doc.imported_layout.positioning());
    assert!(doc.imported_layout.normalized_points);
    assert!(doc.imported_layout.tables.contains_key("GSUB"));
    assert!(doc.imported_layout.tables.contains_key("UVS "));
    for format in [Format::Ttf, Format::Otf, Format::Woff2] {
        let bytes = font_formats::export(&doc, format).unwrap();
        let sfnt = if format == Format::Woff2 {
            font_formats::woff2::decode(&bytes).unwrap()
        } else {
            bytes
        };
        let face = ttf_parser::Face::parse(&sfnt, 0).unwrap();

        for tag in [b"GSUB", b"GPOS", b"GDEF"] {
            assert_eq!(
                face.raw_face().table(ttf_parser::Tag::from_bytes(tag)),
                Some(
                    doc.imported_layout.tables[std::str::from_utf8(tag).unwrap()]
                        .data
                        .as_slice()
                )
            );
        }
        assert_eq!(
            face.glyph_variation_index('字', '\u{e0100}').unwrap().0 as u32,
            glyph(&doc, "ideograph.alt")
        );
        assert_eq!(
            face.glyph_ver_advance(ttf_parser::GlyphId(glyph(&doc, "A") as u16)),
            Some(1000)
        );
    }
    let file = std::env::temp_dir().join(format!("layout-{}.json", doc.document_uuid));
    storage::save(&file, &doc).unwrap();
    let loaded = storage::load(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    assert_eq!(loaded, doc);
    let mut e = Editor::new(loaded).unwrap();
    e.add_characters(e.revision(), &['C'].into()).unwrap();
    let bytes = font_io::export(e.document()).unwrap();
    let f = ttf_parser::Face::parse(&bytes, 0).unwrap();
    assert_eq!(f.glyph_ver_advance(f.glyph_index('C').unwrap()), Some(1000));
    assert_eq!(
        font_formats::export(&doc, Format::Cff).err().unwrap(),
        "layout_format_unsupported"
    );
}
#[test]
fn positioning_release_is_explicit_undoable_and_keeps_substitution_and_ivs() {
    let mut e = Editor::new(document()).unwrap();
    let before = e.document().clone();
    let a = glyph(&before, "A");
    let b = glyph(&before, "B");
    assert_eq!(
        e.set_kerning(e.revision(), a, b, -100),
        Err(Error::ImportedPositioningLinked)
    );
    e.release_imported_positioning(e.revision()).unwrap();
    assert!(!e.document().imported_layout.positioning());
    assert!(e.document().imported_layout.tables.contains_key("GSUB"));
    assert!(e.document().imported_layout.tables.contains_key("UVS "));
    e.set_kerning(e.revision(), a, b, -100).unwrap();
    assert!(font_io::export(e.document()).is_ok());
    e.undo(e.revision()).unwrap();
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.release_imported_layout(e.revision(), false).unwrap();
    assert!(e.document().imported_layout.is_empty());
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
}
#[test]
fn proof_uses_ligatures_ivs_offsets_and_vertical_forms_with_rotation() {
    let doc = document();
    let settings = Settings {
        text: "fi字\u{e0100}A\u{301}".into(),
        size: 100.,
        ..Default::default()
    };
    let subset = proof::subset(&doc, &settings.text).unwrap();
    assert_eq!(subset.glyphs.len(), doc.glyphs.len());
    let p = proof::prepare(subset, &settings.text).unwrap();
    assert!(p.missing.is_empty());
    let layout = proof::layout(&p, &settings, 1000.);
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "fi"))));
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "ideograph.alt"))));
    assert_eq!(layout.placements.len(), 4);
    let base = layout
        .placements
        .iter()
        .find(|g| g.source == Some(glyph(&doc, "A")))
        .unwrap();
    let accent = layout
        .placements
        .iter()
        .find(|g| g.source == Some(glyph(&doc, "acutecomb")))
        .unwrap();
    assert!(accent.baseline < base.baseline);
    let v = Settings {
        direction: Direction::Vertical,
        text: "A字、（ーfi".into(),
        size: 64.,
        column_height: 180.,
        ..Default::default()
    };
    p.warm_shaping(&v).unwrap();
    let layout = proof::layout(&p, &v, 500.);
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "A")) && g.rotated));
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "comma.vert")) && !g.rotated));
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "paren.vert")) && !g.rotated));
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "longmark")) && g.rotated));
    assert!(layout
        .placements
        .iter()
        .any(|g| g.source == Some(glyph(&doc, "fi")) && g.rotated));
    let u = Settings {
        orientation: Orientation::Upright,
        ..v.clone()
    };
    let upright = proof::layout(&p, &u, 500.);
    assert!(upright.placements.iter().all(|g| !g.rotated));
    let r = layout.placements.iter().find(|g| g.rotated).unwrap();
    assert!(r.left.is_finite() && r.width > 0. && r.bottom >= r.top);
}
#[test]
fn source_index_changes_and_tampered_project_tables_are_not_silently_exported() {
    let mut doc = document();
    doc.units_per_em = 2000;
    assert_eq!(font_io::export(&doc).err().unwrap(), "layout_index_changed");
    let mut doc = document();
    doc.glyphs.remove(&2);
    assert_eq!(font_io::export(&doc).err().unwrap(), "layout_index_changed");
    let mut doc = document();
    doc.imported_layout
        .tables
        .insert("evil".into(), doc.imported_layout.tables["GSUB"].clone());
    assert!(doc.validate().is_err());
}

#[test]
fn mixed_bidi_runs_and_script_itemization_keep_visual_glyph_order() {
    let doc = document();
    for (text, names) in [
        ("AאבB", vec!["A", "bet", "alef", "B"]),
        ("אבA", vec!["A", "bet", "alef"]),
    ] {
        let settings = Settings {
            text: text.into(),
            ..Default::default()
        };
        let p = proof::prepare(proof::subset(&doc, text).unwrap(), text).unwrap();
        let layout = proof::layout(&p, &settings, 1000.);
        assert_eq!(
            layout
                .placements
                .iter()
                .map(|g| g.source.unwrap())
                .collect::<Vec<_>>(),
            names
                .iter()
                .map(|name| glyph(&doc, name))
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn generated_vertical_defaults_do_not_turn_plain_fonts_into_opaque_source_projects() {
    let doc = FontDocument::new("Plain", "AB").unwrap();
    for format in [Format::Ttf, Format::Otf, Format::Woff2] {
        let bytes = font_formats::export(&doc, format).unwrap();
        let sfnt = if format == Format::Woff2 {
            font_formats::woff2::decode(&bytes).unwrap()
        } else {
            bytes
        };
        let f = ttf_parser::Face::parse(&sfnt, 0).unwrap();
        assert_eq!(f.glyph_ver_advance(ttf_parser::GlyphId(1)), Some(1000));
        let loaded = font_io::import(&sfnt, 0).unwrap();
        assert!(loaded.imported_layout.is_empty());
        assert_eq!(loaded.schema_version, 2);
    }
    let mut bad = document();
    bad.imported_layout.tables.get_mut("GPOS").unwrap().data = std::sync::Arc::new(vec![]);
    assert!(bad.validate().is_err());
}

#[test]
fn tracking_stays_outside_combining_clusters() {
    let doc = document();
    let text = "A\u{301}B";
    let p = proof::prepare(proof::subset(&doc, text).unwrap(), text).unwrap();
    for direction in [Direction::Horizontal, Direction::Vertical] {
        let a = Settings {
            text: text.into(),
            direction,
            size: 100.,
            ..Default::default()
        };
        let b = Settings {
            tracking: 0.5,
            ..a.clone()
        };
        let plain = proof::layout(&p, &a, 1000.);
        let spaced = proof::layout(&p, &b, 1000.);
        assert_eq!(plain.placements.len(), 3);
        for i in 0..2 {
            assert_eq!(plain.placements[i].x, spaced.placements[i].x);
            assert_eq!(plain.placements[i].baseline, spaced.placements[i].baseline);
        }
        let advance = if direction == Direction::Vertical {
            spaced.placements[2].baseline - plain.placements[2].baseline
        } else {
            spaced.placements[2].x - plain.placements[2].x
        };
        assert!((advance - 50.).abs() < 0.001);
    }
}
