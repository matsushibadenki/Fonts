use font_core::{font_io, variations};
use font_hinting_native::variations::{self as native, AxisRange};
const TTF: &[u8] = include_bytes!("data/variations/named-gvar.ttf");
const CFF: &[u8] = include_bytes!("data/variations/named-cff2.otf");
fn range(tag: &[u8; 4], minimum: f32, default: f32, maximum: f32) -> AxisRange {
    AxisRange {
        tag: u32::from_be_bytes(*tag),
        minimum,
        maximum,
        default,
    }
}
fn compare(a: &font_core::FontDocument, b: &font_core::FontDocument) {
    assert_eq!(a.units_per_em, b.units_per_em);
    assert!((a.ascender - b.ascender).abs() <= 1.);
    assert!((a.descender - b.descender).abs() <= 1.);
    for (a, b) in a.glyphs.values().zip(b.glyphs.values()) {
        assert_eq!(a.codepoints, b.codepoints);
        assert!((a.advance - b.advance).abs() <= 1.);
        assert_eq!(a.contours.len(), b.contours.len());
        for (a, b) in a.contours.iter().zip(&b.contours) {
            let a = font_core::geometry::edit_points(a);
            let b = font_core::geometry::edit_points(b);
            assert_eq!(a.len(), b.len());
            for ((_, a), (_, b)) in a.iter().zip(b) {
                assert!(
                    (a[0] - b[0]).abs() <= 1.5 && (a[1] - b[1]).abs() <= 1.5,
                    "{a:?} != {b:?}"
                );
            }
        }
    }
}
#[test]
fn native_axis_restriction_rebases_gvar_cff2_avar_and_metrics() {
    for source in [TTF, CFF] {
        let output = native::subset(
            source,
            0,
            &[
                range(b"wght", 200., 500., 800.),
                range(b"wdth", 80., 110., 120.),
            ],
        )
        .unwrap();
        assert!(!font_io::name(&ttf_parser::Face::parse(&output, 0).unwrap(), 1).is_empty());
        variations::catalog(&output, 0).unwrap();
        assert_eq!(
            ttf_parser::Face::parse(&output, 0)
                .unwrap()
                .weight()
                .to_number(),
            500
        );
        let axes = variations::axes(&output, 0).unwrap();
        assert_eq!(
            (axes[0].min, axes[0].default, axes[0].max),
            (200., 500., 800.)
        );
        for (wght, wdth) in [(200., 80.), (500., 110.), (650., 120.), (800., 90.)] {
            let coords = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            let expected = variations::instantiate(source, 0, &coords).unwrap();
            let actual = variations::instantiate(&output, 0, &coords).unwrap_or_else(|e| {
                panic!(
                    "{} {wght}/{wdth}: {e}",
                    font_core::font_formats::identify(source)
                )
            });
            compare(&expected, &actual);
        }
    }
}
#[test]
fn native_full_pinning_removes_axes_and_bakes_original_design_space() {
    for source in [TTF, CFF] {
        let coords = [("wght".into(), 650.), ("wdth".into(), 125.)].into();
        let expected = variations::instantiate(source, 0, &coords).unwrap();
        let output = native::subset(
            source,
            0,
            &[
                range(b"wght", 650., 650., 650.),
                range(b"wdth", 125., 125., 125.),
            ],
        )
        .unwrap();
        let face = ttf_parser::Face::parse(&output, 0).unwrap();
        assert!(!face.is_variable());
        assert!(face.tables().cff2.is_none());
        assert_eq!(face.weight().to_number(), 650);
        let actual = font_io::import(&output, 0).unwrap();
        compare(&expected, &actual);
        assert_eq!(face.number_of_glyphs() as usize, expected.glyphs.len());
    }
    assert!(native::subset(b"bad", 0, &[]).is_err());
    assert!(native::subset(TTF, 0, &[range(b"wght", 99., 400., 900.)]).is_err());
    assert!(native::subset(TTF, 0, &[range(b"xxxx", 100., 400., 900.)]).is_err());
}

#[test]
fn variable_projects_keep_axes_settings_history_storage_and_variable_output() {
    use font_core::{
        font_formats::{self, Format},
        source_font::{
            variable::{Edit, Range},
            SourceFont,
        },
        storage, Editor,
    };
    for bytes in [TTF, CFF] {
        let doc = SourceFont::variable_document(bytes, 0, None).unwrap();
        let mut e = Editor::new(doc).unwrap();
        let before = e.document().clone();
        e.edit_source_variations(
            0,
            Edit::Range {
                tag: "wght".into(),
                range: Range {
                    min: 200.,
                    default: 500.,
                    max: 800.,
                },
            },
        )
        .unwrap();
        e.edit_source_variations(
            e.revision(),
            Edit::Coordinates {
                values: [("wght".into(), 650.), ("wdth".into(), 125.)].into(),
            },
        )
        .unwrap();
        let changed = e.document().clone();
        let path =
            std::env::temp_dir().join(format!("variable-project-{}.json", changed.document_uuid));
        storage::save(&path, &changed).unwrap();
        let loaded = storage::load(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded, changed);
        let source = loaded.source_font.as_ref().unwrap();
        let output = source.export().unwrap();
        let axes = variations::axes(&output, 0).unwrap();
        assert_eq!(axes[0].default, 500.);
        assert_eq!(axes[0].min, 200.);
        assert_eq!(axes[0].max, 800.);
        source.verify_output(&output, false).unwrap();
        let woff = font_formats::export(&loaded, Format::Woff2).unwrap();
        source.verify_output(&woff, true).unwrap();
        let actual = source.static_document().unwrap();
        let expected = variations::instantiate(
            bytes,
            0,
            &[("wght".into(), 650.), ("wdth".into(), 125.)].into(),
        )
        .unwrap();
        compare(&expected, &actual);
        assert!(actual.source_font.is_none());
        e.undo(e.revision()).unwrap();
        e.undo(e.revision()).unwrap();
        assert_eq!(e.document(), &before);
        e.redo(e.revision()).unwrap();
        e.redo(e.revision()).unwrap();
        assert_eq!(e.document(), &changed);
        let rev = e.revision();
        assert!(e
            .edit_source_variations(
                rev,
                Edit::Range {
                    tag: "wght".into(),
                    range: Range {
                        min: 99.,
                        default: 400.,
                        max: 800.
                    }
                }
            )
            .is_err());
        assert_eq!(e.document(), &changed);
        assert_eq!(e.revision(), rev);
    }
}

#[test]
fn variable_color_projects_keep_palette_changes_when_pinning_axes() {
    use font_core::{
        color_fonts,
        source_font::{
            variable::{Edit, Range},
            SourceFont,
        },
        Editor,
    };
    for bytes in [
        include_bytes!("data/variations/color-gvar.ttf").as_slice(),
        include_bytes!("data/variations/color-cff2.otf").as_slice(),
    ] {
        let mut e = Editor::new(SourceFont::variable_document(bytes, 0, None).unwrap()).unwrap();
        e.edit_source_palette(0, 0, Some(0), Some([20, 40, 60, 128]))
            .unwrap();
        let preview = |source: &SourceFont| {
            color_fonts::preview(&source.static_bytes().unwrap(), 0, "A", 100, 0, [256, 256])
                .unwrap()
        };
        e.edit_source_variations(
            e.revision(),
            Edit::Coordinates {
                values: [("wght".into(), 100.), ("wdth".into(), 100.)].into(),
            },
        )
        .unwrap();
        let light = preview(e.document().source_font.as_ref().unwrap());
        e.edit_source_variations(
            e.revision(),
            Edit::Coordinates {
                values: [("wght".into(), 900.), ("wdth".into(), 100.)].into(),
            },
        )
        .unwrap();
        let heavy = preview(e.document().source_font.as_ref().unwrap());
        assert_eq!(
            light
                .image
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3])
                .max(),
            Some(64)
        );
        let ink = |p: &color_fonts::Preview| {
            p.image
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 20)
                .count()
        };
        assert!(ink(&heavy) > ink(&light) * 2);
        let px = heavy
            .image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .find(|p| p[3] >= 125)
            .unwrap();
        for (a, b) in px.iter().zip([20u8, 40, 60, 128]) {
            assert!(a.abs_diff(b) <= 2);
        }
        e.edit_source_variations(
            e.revision(),
            Edit::Range {
                tag: "wght".into(),
                range: Range {
                    min: 200.,
                    default: 500.,
                    max: 800.,
                },
            },
        )
        .unwrap();
        let source = e.document().source_font.as_ref().unwrap();
        let output = source.export().unwrap();
        color_fonts::info(&output, 0).unwrap();
        assert_eq!(variations::axes(&output, 0).unwrap()[0].default, 500.);
        let doc = source.static_document().unwrap();
        let static_source = doc.source_font.unwrap();
        assert!(static_source.variable.is_none());
        assert_eq!(
            static_source.palette_colors().unwrap()[0],
            [20, 40, 60, 128]
        );
        assert!(!ttf_parser::Face::parse(&static_source.bytes, 0)
            .unwrap()
            .is_variable());
    }
}

#[test]
fn static_conversion_resolves_variable_kerning_and_preserves_ligatures_and_ivs() {
    use font_core::proof::{self, Settings};
    for source in [
        include_bytes!("data/variations/shaping-gvar.ttf").as_slice(),
        include_bytes!("data/variations/shaping-cff2.otf").as_slice(),
    ] {
        for (weight, step) in [(100., 22.), (900., 68.)] {
            let doc =
                variations::instantiate(source, 0, &[("wght".into(), weight)].into()).unwrap();
            assert!(doc.imported_layout.positioning());
            assert!(doc.imported_layout.tables.contains_key("GSUB"));
            assert!(doc.imported_layout.tables.contains_key("UVS "));
            let p = proof::prepare(doc, "AA A A A\u{e0100}").unwrap();
            let settings = Settings {
                text: "AA".into(),
                size: 100.,
                ..Default::default()
            };
            let layout = proof::layout(&p, &settings, 1000.);
            assert_eq!(layout.placements.len(), 2);
            assert!((layout.placements[1].x - layout.placements[0].x - step).abs() < 0.1);
            for text in ["A A", "A\u{e0100}"] {
                let settings = Settings {
                    text: text.into(),
                    size: 100.,
                    ..Default::default()
                };
                let layout = proof::layout(&p, &settings, 1000.);
                assert_eq!(layout.placements.len(), 1);
                assert_eq!(layout.placements[0].source, Some(3));
            }
        }
        let default = font_io::import(source, 0).unwrap();
        assert!(default.imported_layout.positioning());
        assert!(default.imported_layout.tables.contains_key("UVS "));
    }
}
