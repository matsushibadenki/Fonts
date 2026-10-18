use font_core::{
    class_kerning::{Class, ClassKerning},
    font_formats::{
        self,
        woff2::{self, Options, Outline},
        Format,
    },
    kerning::Pair,
    Contour, FontDocument, Segment,
};
fn source() -> FontDocument {
    let mut d = FontDocument::new("WOFF2 日本 中文", "AV中 😀").unwrap();
    d.glyphs.get_mut(&1).unwrap().contours = vec![Contour {
        start: [30., 0.],
        segments: vec![
            Segment::Cubic {
                control1: [0., 700.],
                control2: [550., 700.],
                end: [580., 0.],
            },
            Segment::Line { end: [30., 0.] },
        ],
        closed: true,
        smooth: Default::default(),
    }];
    d.kerning.0.push(Pair {
        left: 1,
        right: 2,
        value: -80,
    });
    d.class_kerning = ClassKerning {
        next_id: 3,
        left: vec![Class {
            id: 1,
            name: "Left".into(),
            glyphs: [1].into(),
        }],
        right: vec![Class {
            id: 2,
            name: "Right".into(),
            glyphs: [2, 3].into(),
        }],
        pairs: vec![Pair {
            left: 1,
            right: 2,
            value: -50,
        }],
    };
    d
}
#[test]
fn both_outline_formats_keep_unicode_names_metrics_and_gpos() {
    let d = source();
    let original = d.clone();
    for outline in [Outline::TrueType, Outline::Cff] {
        for quality in [1, 5, 9, 11] {
            let bytes = woff2::export(&d, Options { outline, quality }).unwrap();
            assert_eq!(&bytes[..4], b"wOF2");
            assert_eq!(bytes.len() % 4, 0);
            let decoded = woff2::decode(&bytes).unwrap();
            if quality == 9 {
                if let Some(dir) = std::env::var_os("FONTS_WOFF2_FIXTURES") {
                    let root = std::path::PathBuf::from(dir);
                    std::fs::create_dir_all(&root).unwrap();
                    let name = if outline == Outline::TrueType {
                        "truetype"
                    } else {
                        "cff"
                    };
                    std::fs::write(root.join(format!("{name}.woff2")), &bytes).unwrap();
                    std::fs::write(root.join(format!("{name}.sfnt")), &decoded).unwrap();
                }
            }
            let f = ttf_parser::Face::parse(&decoded, 0).unwrap();
            assert!(f.glyph_index('😀').is_some());
            assert!(f.tables().gpos.is_some());
            assert_eq!(f.number_of_glyphs(), d.glyphs.len() as u16);
            assert_eq!(
                f.glyph_hor_advance(f.glyph_index('A').unwrap()),
                Some(d.glyphs[&1].advance as u16)
            );
            assert_eq!(f.tables().cff.is_some(), outline == Outline::Cff);
        }
    }
    assert_eq!(d, original);
    assert_eq!(Format::Woff2.extension(), "woff2");
    assert!(font_formats::export(&d, Format::Woff2).is_ok());
}
#[test]
fn invalid_options_and_corrupt_files_are_rejected() {
    let d = source();
    assert!(woff2::export(
        &d,
        Options {
            quality: 0,
            ..Default::default()
        }
    )
    .is_err());
    assert!(woff2::export(
        &d,
        Options {
            quality: 12,
            ..Default::default()
        }
    )
    .is_err());
    let b = woff2::export(&d, Options::default()).unwrap();
    for len in [0, 4, 47, b.len() - 1] {
        assert!(woff2::decode(&b[..len]).is_err());
    }
    for i in [0, 8, 12, 14, 16, 20, 28, 48] {
        let mut bad = b.clone();
        bad[i] ^= 255;
        assert!(woff2::decode(&bad).is_err(), "offset {i}");
    }
    let mut extra = b;
    extra.push(0);
    assert!(woff2::decode(&extra).is_err());
}
#[test]
fn empty_font_and_space_only_font_are_valid() {
    for text in ["", " "] {
        for outline in [Outline::TrueType, Outline::Cff] {
            let d = FontDocument::new("Empty", text).unwrap();
            let b = woff2::export(
                &d,
                Options {
                    outline,
                    ..Default::default()
                },
            )
            .unwrap();
            woff2::decode(&b).unwrap();
        }
    }
}
#[test]
fn rejected_open_contours_never_produce_woff2() {
    let mut d = source();
    d.glyphs.get_mut(&1).unwrap().contours[0].closed = false;
    for outline in [Outline::TrueType, Outline::Cff] {
        assert!(woff2::export(
            &d,
            Options {
                outline,
                ..Default::default()
            }
        )
        .is_err());
    }
}
