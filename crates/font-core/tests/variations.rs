use font_core::{
    font_formats::{self, Format},
    variations::{self, Coordinates},
};
const TTF: &[u8] = include_bytes!("data/variations/gvar.ttf");
const CFF: &[u8] = include_bytes!("data/variations/cff2.otf");
#[test]
fn gvar_cff2_axes_bake_outlines_hvar_and_mvar() {
    for bytes in [TTF, CFF] {
        let axes = variations::axes(bytes, 0).unwrap();
        assert_eq!(axes.len(), 2);
        assert_eq!(axes[0].tag, "wght");
        assert_eq!(
            (axes[0].min, axes[0].default, axes[0].max),
            (100., 400., 900.)
        );
        let light = variations::instantiate(bytes, 0, &[("wght".into(), 100.)].into()).unwrap();
        let heavy = variations::instantiate(bytes, 0, &[("wght".into(), 900.)].into()).unwrap();
        let a = light
            .glyphs
            .values()
            .find(|g| g.codepoints.contains(&'A'))
            .unwrap();
        let b = heavy
            .glyphs
            .values()
            .find(|g| g.codepoints.contains(&'A'))
            .unwrap();
        assert_eq!(a.advance, 300.);
        assert_eq!(b.advance, 800.);
        assert_ne!(a.contours, b.contours);
        assert!(heavy.ascender > light.ascender);
        assert_eq!(heavy.metadata.weight, 900);
        assert!(heavy.metadata.bold);
        for format in [Format::Ttf, Format::Otf, Format::Woff2] {
            let output = font_formats::export(&heavy, format).unwrap();
            let sfnt = if format == Format::Woff2 {
                font_formats::woff2::decode(&output).unwrap()
            } else {
                output
            };
            variations::verify(&sfnt, &heavy).unwrap();
            let face = ttf_parser::Face::parse(&sfnt, 0).unwrap();
            assert!(!face.is_variable());
            for tag in [b"fvar", b"gvar", b"CFF2", b"HVAR", b"MVAR", b"avar"] {
                assert!(face
                    .raw_face()
                    .table(ttf_parser::Tag::from_bytes(tag))
                    .is_none());
            }
        }
    }
}
#[test]
fn axes_are_validated_without_clamping_or_silent_defaults() {
    for bytes in [TTF, CFF] {
        for coords in [
            [("xxxx".into(), 400.)].into(),
            [("wght".into(), 99.)].into(),
            [("wght".into(), 901.)].into(),
            [("wght".into(), f32::NAN)].into(),
            [("wght".into(), f32::INFINITY)].into(),
        ] {
            assert!(variations::instantiate(bytes, 0, &coords).is_err());
        }
        let implicit = variations::instantiate(bytes, 0, &Coordinates::new()).unwrap();
        let explicit = variations::instantiate(bytes, 0, &[("wght".into(), 400.)].into()).unwrap();
        assert_eq!(implicit, explicit);
        assert!(variations::axes(bytes, 1).is_err());
    }
    assert!(variations::axes(b"bad", 0).is_err());
}
#[test]
fn static_validation_rejects_variable_corrupt_or_wrong_repertoire() {
    let doc = variations::instantiate(TTF, 0, &Coordinates::new()).unwrap();
    assert!(variations::verify(TTF, &doc).is_err());
    assert!(variations::verify(b"bad", &doc).is_err());
    let mut wrong = doc.clone();
    wrong.glyphs.get_mut(&1).unwrap().advance += 100.;
    let output = font_formats::export(&doc, Format::Ttf).unwrap();
    assert!(variations::verify(&output, &wrong).is_err());
    assert!(variations::axes(&output, 0).is_err());
}

#[test]
fn multiple_axes_and_avar_mapping_are_applied_together() {
    for bytes in [TTF, CFF] {
        let doc = variations::instantiate(
            bytes,
            0,
            &[("wght".into(), 650.), ("wdth".into(), 125.)].into(),
        )
        .unwrap();
        // wght .5 -> avar .75: width 400 + 300*.75 + 100.
        let a = doc
            .glyphs
            .values()
            .find(|g| g.codepoints.contains(&'A'))
            .unwrap();
        assert_eq!(a.advance, 825.);
        assert_eq!(doc.metadata.width, 7);
    }
}

#[test]
fn named_instances_select_all_axes_localize_and_preserve_exported_style() {
    for bytes in [
        include_bytes!("data/variations/named-gvar.ttf").as_slice(),
        include_bytes!("data/variations/named-cff2.otf").as_slice(),
    ] {
        let catalog = variations::catalog(bytes, 0).unwrap();
        assert_eq!(catalog.instances.len(), 3);
        let default = &catalog.instances[0];
        assert_eq!(default.id, 0);
        assert_eq!(default.name, "Regular");
        assert_eq!(default.names["ja"], "標準");
        assert_eq!(default.names["zh_cn"], "常规");
        let bold = catalog.instances.iter().find(|i| i.name == "Bold").unwrap();
        assert_eq!(bold.coordinates["wdth"], 100.);
        assert_eq!(bold.coordinates["wght"], 900.);
        let doc = variations::instantiate_named(bytes, 0, bold.id, &bold.coordinates).unwrap();
        assert_eq!(doc.metadata.style, "Bold");
        assert_eq!(doc.metadata.weight, 900);
        assert!(doc.metadata.bold);
        for format in [Format::Ttf, Format::Otf, Format::Woff2] {
            let output = font_formats::export(&doc, format).unwrap();
            let sfnt = if format == Format::Woff2 {
                font_formats::woff2::decode(&output).unwrap()
            } else {
                output
            };
            variations::verify(&sfnt, &doc).unwrap();
            let face = ttf_parser::Face::parse(&sfnt, 0).unwrap();
            assert_eq!(font_core::font_io::name(&face, 2), "Bold");
        }
        let restored: font_core::FontDocument =
            serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(restored.metadata.style, "Bold");
        let mut wrong = bold.coordinates.clone();
        wrong.insert("wdth".into(), 75.);
        assert!(variations::instantiate_named(bytes, 0, bold.id, &wrong).is_err());
        assert!(variations::instantiate_named(bytes, 0, usize::MAX, &bold.coordinates).is_err());
    }
    assert_eq!(variations::catalog(TTF, 0).unwrap().instances.len(), 1);
}
