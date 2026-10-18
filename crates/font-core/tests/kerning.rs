use font_core::{
    font_formats::{self, Format},
    font_io,
    kerning::{Kerning, Pair},
    proof, Editor, FontDocument,
};
fn document() -> FontDocument {
    FontDocument::new("Kerning QA", "AVTo 中").unwrap()
}
fn gpos_value(bytes: &[u8], left: char, right: char) -> i16 {
    use ttf_parser::{
        gpos::{PairAdjustment, PositioningSubtable},
        Face,
    };
    let f = Face::parse(bytes, 0).unwrap();
    let table = f.tables().gpos.unwrap();
    assert_eq!(table.features.get(0).unwrap().tag.to_bytes(), *b"kern");
    let lookup = table.lookups.get(0).unwrap();
    let PositioningSubtable::Pair(PairAdjustment::Format1 { coverage, sets }) =
        lookup.subtables.get(0).unwrap()
    else {
        panic!("not PairPos format 1")
    };
    let index = coverage.get(f.glyph_index(left).unwrap()).unwrap();
    let (a, b) = sets
        .get(index)
        .unwrap()
        .get(f.glyph_index(right).unwrap())
        .unwrap();
    assert_eq!(b.x_advance, 0);
    a.x_advance
}
#[test]
fn edits_are_atomic_zero_removes_and_history_restores() {
    let mut e = Editor::new(document()).unwrap();
    e.set_kerning(0, 1, 2, -100).unwrap();
    assert_eq!(e.document().kerning.get(1, 2), -100);
    assert!(e.set_kerning(0, 1, 2, -80).is_err());
    let original = e.document().clone();
    assert!(e.set_kerning(1, 1, 999, -100).is_err());
    assert!(e.set_kerning(1, 0, 1, 50).is_err());
    assert_eq!(e.document(), &original);
    assert_eq!(e.set_kerning(1, 1, 2, -100).unwrap(), 1);
    e.set_kerning(1, 1, 2, 0).unwrap();
    assert!(e.document().kerning.is_empty());
    e.undo(2).unwrap();
    assert_eq!(e.document(), &original);
    e.redo(3).unwrap();
    assert!(e.document().kerning.is_empty());
    e.undo(4).unwrap();
    e.set_kerning(5, 2, 1, 30).unwrap();
    assert!(!e.can_redo());
}
#[test]
fn project_roundtrip_old_schema_and_validation() {
    let mut d = document();
    d.kerning = Kerning(vec![Pair {
        left: 1,
        right: 2,
        value: -120,
    }]);
    let data = serde_json::to_vec(&d).unwrap();
    let loaded: FontDocument = serde_json::from_slice(&data).unwrap();
    assert_eq!(loaded, d);
    let mut value = serde_json::to_value(&d).unwrap();
    value.as_object_mut().unwrap().remove("kerning");
    assert!(serde_json::from_value::<FontDocument>(value)
        .unwrap()
        .kerning
        .is_empty());
    for bad in [
        vec![Pair {
            left: 1,
            right: 999,
            value: 1,
        }],
        vec![
            Pair {
                left: 1,
                right: 2,
                value: 1
            };
            2
        ],
    ] {
        d.kerning = Kerning(bad);
        assert!(d.validate().is_err());
    }
}
#[test]
fn export_gpos_is_readable_for_ttf_and_both_otf_flavors() {
    let mut e = Editor::new(document()).unwrap();
    e.set_kerning(0, 1, 2, -120).unwrap();
    e.set_kerning(1, 3, 4, 25).unwrap();
    for format in [Format::Ttf, Format::Otf, Format::CidOtf] {
        let bytes = font_formats::export(e.document(), format).unwrap();
        assert_eq!(gpos_value(&bytes, 'A', 'V'), -120);
        assert_eq!(gpos_value(&bytes, 'T', 'o'), 25);
        assert_eq!(
            font_io::import(&bytes, 0).unwrap().kerning,
            e.document().kerning
        );
        let sum = bytes
            .chunks(4)
            .map(|p| {
                let mut a = [0; 4];
                a[..p.len()].copy_from_slice(p);
                u32::from_be_bytes(a)
            })
            .fold(0u32, u32::wrapping_add);
        assert_eq!(sum, 0xb1b0afba);
        if let Ok(root) = std::env::var("FONTS_KERNING_QA_DIR") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!(
                    "pair-{}.{}",
                    format.label().replace(' ', "-"),
                    format.extension()
                )),
                bytes,
            )
            .unwrap();
        }
    }
}
#[test]
fn sparse_ids_and_unicode_aliases_do_not_change_pairs() {
    let mut d = document();
    let mut g = d.glyphs.remove(&2).unwrap();
    g.id = 900;
    g.codepoints.insert('Ｖ');
    d.glyphs.insert(900, g);
    d.kerning = Kerning(vec![Pair {
        left: 1,
        right: 900,
        value: -80,
    }]);
    let bytes = font_io::export(&d).unwrap();
    assert_eq!(gpos_value(&bytes, 'A', 'V'), -80);
    assert_eq!(gpos_value(&bytes, 'A', 'Ｖ'), -80);
    let subset = proof::subset(&d, "AＶ").unwrap();
    let f = proof::prepare(subset, "AＶ").unwrap();
    let s = proof::Settings {
        size: 100.,
        text: "AＶ".into(),
        ..Default::default()
    };
    let l = proof::layout(&f, &s, 1000.);
    assert_eq!(l.placements[1].source, Some(900));
    assert!(
        (l.placements[1].x - l.placements[0].x - (d.glyphs[&1].advance - 80.) * 0.1).abs() < 0.001
    );
}
#[test]
fn proof_toggle_tracking_wrap_and_newlines() {
    let mut e = Editor::new(document()).unwrap();
    e.set_kerning(0, 1, 2, -100).unwrap();
    let f = proof::prepare(proof::subset(e.document(), "AV\nAV\tV").unwrap(), "AV").unwrap();
    let mut s = proof::Settings {
        text: "AV".into(),
        size: 100.,
        tracking: 0.1,
        ..Default::default()
    };
    let on = proof::layout(&f, &s, 1000.);
    s.kerning = false;
    let off = proof::layout(&f, &s, 1000.);
    assert!((off.placements[1].x - on.placements[1].x - 10.).abs() < 0.001);
    s.kerning = true;
    s.tracking = 0.;
    s.text = "AV\nV\tV".into();
    let l = proof::layout(&f, &s, 1000.);
    assert_eq!(l.placements[2].x, 24.);
    assert!(l.placements[2].baseline > l.placements[1].baseline);
    s.text = "AV".into();
    let l = proof::layout(&f, &s, 90.);
    assert_eq!(l.placements[1].x, 24.);
    assert!(l.placements[1].baseline > l.placements[0].baseline);
}
#[test]
fn standalone_formats_do_not_silently_drop_kerning() {
    let mut d = document();
    d.kerning = Kerning(vec![Pair {
        left: 1,
        right: 2,
        value: -10,
    }]);
    for format in [
        Format::Pfa,
        Format::Pfb,
        Format::Cff,
        Format::CidCff,
        Format::Cid,
    ] {
        assert_eq!(
            font_formats::export(&d, format).unwrap_err(),
            "kerning_format_unsupported"
        );
    }
}
#[test]
fn unencoded_glyphs_can_be_paired_and_exported() {
    let mut d = document();
    d.glyphs.get_mut(&2).unwrap().codepoints.clear();
    let mut e = Editor::new(d).unwrap();
    e.set_kerning(0, 1, 2, i16::MIN).unwrap();
    assert!(font_io::export(e.document()).is_ok());
    e.set_kerning(1, 1, 2, i16::MAX).unwrap();
    assert!(font_io::export(e.document()).is_ok());
}
#[test]
fn pair_limit_remains_exportable() {
    let mut d = FontDocument::new(
        "Many pairs",
        &("!"
            .chars()
            .chain((0x1000..0x1050).filter_map(char::from_u32)))
        .collect::<String>(),
    )
    .unwrap();
    d.kerning = Kerning(
        (1..=64)
            .flat_map(|left| {
                (1..=64).map(move |right| Pair {
                    left,
                    right,
                    value: -1,
                })
            })
            .collect(),
    );
    d.validate().unwrap();
    let bytes = font_io::export(&d).unwrap();
    assert_eq!(font_io::import(&bytes, 0).unwrap().kerning, d.kerning);
    let mut e = Editor::new(d).unwrap();
    assert!(e.set_kerning(0, 65, 1, -1).is_err());
    assert_eq!(e.revision(), 0);
}
