use font_core::{
    class_kerning::{Class, ClassKerning, Edit, Side},
    font_formats::{self, Format},
    kerning::Pair,
    proof, Contour, Editor, FontDocument, Segment,
};
use std::collections::BTreeSet;
fn members(ids: &[u32]) -> BTreeSet<u32> {
    ids.iter().copied().collect()
}
fn apply(e: &mut Editor, edit: Edit) {
    e.edit_class_kerning(e.revision(), edit).unwrap();
}
fn source() -> Editor {
    let mut d = FontDocument::new("Class Kerning QA", "ATVW ").unwrap();
    for id in 1..=4 {
        let pts = if id <= 2 {
            vec![
                [50., 0.],
                [300., 700.],
                [550., 0.],
                [440., 0.],
                [300., 430.],
                [160., 0.],
            ]
        } else {
            vec![
                [50., 700.],
                [160., 700.],
                [300., 200.],
                [440., 700.],
                [550., 700.],
                [300., 0.],
            ]
        };
        d.glyphs.get_mut(&id).unwrap().contours = vec![Contour {
            start: pts[0],
            segments: pts[1..]
                .iter()
                .map(|end| Segment::Line { end: *end })
                .collect(),
            closed: true,
            smooth: Default::default(),
        }];
    }
    let mut e = Editor::new(d).unwrap();
    apply(
        &mut e,
        Edit::Upsert {
            side: Side::Left,
            id: None,
            name: "A / T".into(),
            glyphs: members(&[1, 2]),
        },
    );
    apply(
        &mut e,
        Edit::Upsert {
            side: Side::Right,
            id: None,
            name: "V / W".into(),
            glyphs: members(&[3, 4]),
        },
    );
    apply(
        &mut e,
        Edit::Pair {
            left: 1,
            right: 2,
            value: -80,
        },
    );
    e
}
#[test]
fn membership_edits_are_atomic_disjoint_per_side_and_undoable() {
    let mut e = source();
    let before = e.document().clone();
    for edit in [
        Edit::Upsert {
            side: Side::Left,
            id: None,
            name: "Overlap".into(),
            glyphs: members(&[1]),
        },
        Edit::Upsert {
            side: Side::Right,
            id: Some(2),
            name: "Empty".into(),
            glyphs: members(&[]),
        },
        Edit::Upsert {
            side: Side::Right,
            id: Some(2),
            name: "Missing".into(),
            glyphs: members(&[999]),
        },
        Edit::Pair {
            left: 2,
            right: 1,
            value: -90,
        },
    ] {
        assert!(e.edit_class_kerning(e.revision(), edit).is_err());
        assert_eq!(e.document(), &before);
    }
    assert!(e
        .edit_class_kerning(
            0,
            Edit::Remove {
                side: Side::Left,
                id: 1
            }
        )
        .is_err());
    apply(
        &mut e,
        Edit::Upsert {
            side: Side::Left,
            id: Some(1),
            name: "Renamed".into(),
            glyphs: members(&[1, 2, 3]),
        },
    );
    assert_eq!(e.document().kerning_value(3, 4), -80);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    apply(
        &mut e,
        Edit::Remove {
            side: Side::Right,
            id: 2,
        },
    );
    assert!(e.document().class_kerning.pairs.is_empty());
    assert_eq!(e.document().kerning_value(1, 3), 0);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert!(e.document().class_kerning.right.is_empty());
}
#[test]
fn explicit_and_zero_exceptions_override_without_adding() {
    let mut e = source();
    assert_eq!(e.document().kerning_value(1, 3), -80);
    e.set_kerning(e.revision(), 1, 3, 20).unwrap();
    assert_eq!(e.document().kerning_value(1, 3), 20);
    e.zero_kerning_exception(e.revision(), 1, 4).unwrap();
    assert_eq!(e.document().kerning_value(1, 4), 0);
    assert_eq!(e.document().kerning_value(2, 4), -80);
    e.set_kerning(e.revision(), 1, 4, 0).unwrap();
    assert_eq!(e.document().kerning_value(1, 4), -80);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document().kerning_value(1, 4), 0);
}
#[test]
fn class_only_and_mixed_gpos_roundtrip_and_fixtures() {
    let mut e = source();
    for mixed in [false, true] {
        if mixed {
            e.set_kerning(e.revision(), 1, 3, 20).unwrap();
            e.zero_kerning_exception(e.revision(), 1, 4).unwrap();
        }
        for format in [Format::Ttf, Format::Otf, Format::CidOtf] {
            let bytes = font_formats::export(e.document(), format).unwrap();
            let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
            let lookup = face.tables().gpos.unwrap().lookups.get(0).unwrap();
            assert_eq!(lookup.subtables.len(), if mixed { 2 } else { 1 });
            let sub: ttf_parser::gpos::PositioningSubtable =
                lookup.subtables.get(if mixed { 1 } else { 0 }).unwrap();
            assert!(matches!(
                sub,
                ttf_parser::gpos::PositioningSubtable::Pair(
                    ttf_parser::gpos::PairAdjustment::Format2 { .. }
                )
            ));
            let d = font_core::font_io::import(&bytes, 0).unwrap();
            assert_eq!(d.kerning, e.document().kerning);
            for l in 1..=4 {
                for r in 1..=4 {
                    assert_eq!(d.kerning_value(l, r), e.document().kerning_value(l, r));
                }
            }
            if let Ok(root) = std::env::var("FONTS_CLASS_QA_DIR") {
                std::fs::create_dir_all(&root).unwrap();
                std::fs::write(
                    std::path::Path::new(&root).join(format!(
                        "{}-{}.{}",
                        if mixed { "mixed" } else { "classes" },
                        format.label().replace(' ', "-"),
                        format.extension()
                    )),
                    bytes,
                )
                .unwrap();
                std::fs::write(
                    std::path::Path::new(&root).join(if mixed {
                        "mixed.fontproj.json"
                    } else {
                        "classes.fontproj.json"
                    }),
                    serde_json::to_vec_pretty(e.document()).unwrap(),
                )
                .unwrap();
            }
        }
    }
}
#[test]
fn subset_sparse_ids_and_proof_agree_with_export() {
    let mut e = source();
    e.set_kerning(e.revision(), 1, 3, 20).unwrap();
    e.zero_kerning_exception(e.revision(), 1, 4).unwrap();
    let mut d = e.document().clone();
    let mut g = d.glyphs.remove(&4).unwrap();
    g.id = 900;
    g.codepoints.insert('Ｗ');
    d.glyphs.insert(900, g);
    d.class_kerning.right[0].glyphs = members(&[3, 900]);
    d.kerning
        .0
        .iter_mut()
        .filter(|p| p.right == 4)
        .for_each(|p| p.right = 900);
    let subset = proof::subset(&d, "AＷ").unwrap();
    assert_eq!(subset.class_kerning.left[0].glyphs, members(&[1]));
    assert_eq!(subset.class_kerning.right[0].glyphs, members(&[900]));
    assert!(proof::subset(&d, "A")
        .unwrap()
        .class_kerning
        .pairs
        .is_empty());
    let f = proof::prepare(proof::subset(&d, "AV AT TV TＷ").unwrap(), "AV AT TV TＷ").unwrap();
    let mut settings = proof::Settings {
        text: "AV".into(),
        size: 100.,
        ..Default::default()
    };
    let l = proof::layout(&f, &settings, 1000.);
    assert!((l.placements[1].x - 86.).abs() < 0.001);
    settings.text = "TV".into();
    let on = proof::layout(&f, &settings, 1000.);
    assert!((on.placements[1].x - 76.).abs() < 0.001);
    settings.kerning = false;
    let off = proof::layout(&f, &settings, 1000.);
    assert!((off.placements[1].x - 84.).abs() < 0.001);
    settings.text = "T\nV".into();
    assert_eq!(proof::layout(&f, &settings, 1000.).placements[1].x, 24.);
}
#[test]
fn project_storage_preserves_names_ids_and_old_projects() {
    let e = source();
    let path =
        std::env::temp_dir().join(format!("fonts-class-storage-{}.json", std::process::id()));
    font_core::storage::save(&path, e.document()).unwrap();
    assert_eq!(&font_core::storage::load(&path).unwrap(), e.document());
    std::fs::remove_file(path).unwrap();
    let mut json = serde_json::to_value(e.document()).unwrap();
    json.as_object_mut().unwrap().remove("class_kerning");
    assert!(serde_json::from_value::<FontDocument>(json)
        .unwrap()
        .class_kerning
        .is_empty());
}
#[test]
fn unsupported_formats_refuse_to_discard_class_pairs() {
    let e = source();
    for f in [
        Format::Pfa,
        Format::Pfb,
        Format::Cff,
        Format::CidCff,
        Format::Cid,
    ] {
        assert_eq!(
            font_formats::export(e.document(), f).unwrap_err(),
            "kerning_format_unsupported"
        );
    }
}
#[test]
fn matrix_and_membership_limits_stay_exportable() {
    let mut d = FontDocument::new(
        "Large classes",
        &(0x1000..0x1100)
            .filter_map(char::from_u32)
            .collect::<String>(),
    )
    .unwrap();
    let mut c = ClassKerning::default();
    for i in 1..=64 {
        c.left.push(Class {
            id: i,
            name: format!("L{i}"),
            glyphs: members(&[i]),
        });
        c.right.push(Class {
            id: 64 + i,
            name: format!("R{i}"),
            glyphs: members(&[64 + i]),
        });
    }
    c.next_id = 129;
    c.pairs = (1..=64)
        .flat_map(|left| {
            (65..=128).map(move |right| Pair {
                left,
                right,
                value: -50,
            })
        })
        .collect();
    d.class_kerning = c;
    d.kerning.0 = (1..=64)
        .flat_map(|left| {
            (65..=128).map(move |right| Pair {
                left,
                right,
                value: 20,
            })
        })
        .collect();
    let bytes = font_core::font_io::export(&d).unwrap();
    let loaded = font_core::font_io::import(&bytes, 0).unwrap();
    assert_eq!(loaded.class_kerning.pairs.len(), 4096);
    assert_eq!(loaded.kerning_value(32, 96), 20);
    let mut e = Editor::new(d).unwrap();
    assert!(e
        .edit_class_kerning(
            0,
            Edit::Upsert {
                side: Side::Left,
                id: None,
                name: "Overflow".into(),
                glyphs: members(&[129])
            }
        )
        .is_err());
    assert_eq!(e.revision(), 0);
}
#[test]
fn sparse_membership_capacity_and_malformed_gpos_are_bounded() {
    let mut d = FontDocument::new(
        "Member capacity",
        &(0x4000..0x5001)
            .filter_map(char::from_u32)
            .collect::<String>(),
    )
    .unwrap();
    d.class_kerning = ClassKerning {
        next_id: 3,
        left: vec![Class {
            id: 1,
            name: "Odd".into(),
            glyphs: (1..=4096).filter(|g| g % 2 == 1).collect(),
        }],
        right: vec![Class {
            id: 2,
            name: "Even".into(),
            glyphs: (1..=4096).filter(|g| g % 2 == 0).collect(),
        }],
        pairs: vec![Pair {
            left: 1,
            right: 2,
            value: -100,
        }],
    };
    let bytes = font_core::font_io::export(&d).unwrap();
    let loaded = font_core::font_io::import(&bytes, 0).unwrap();
    assert_eq!(loaded.class_kerning.left[0].glyphs.len(), 2048);
    assert_eq!(loaded.kerning_value(4095, 4096), -100);
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    let data = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"GPOS"))
        .unwrap();
    for n in [0, 10, 55, data.len() / 2, data.len() - 1] {
        assert!(font_core::kerning::read_gpos(&data[..n]).is_none());
    }
    let mut e = Editor::new(d).unwrap();
    let mut too_many = e.document().class_kerning.left[0].glyphs.clone();
    too_many.insert(4097);
    assert!(e
        .edit_class_kerning(
            0,
            Edit::Upsert {
                side: Side::Left,
                id: Some(1),
                name: "Odd".into(),
                glyphs: too_many
            }
        )
        .is_err());
    assert_eq!(e.revision(), 0);
}
