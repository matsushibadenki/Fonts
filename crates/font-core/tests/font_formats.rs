use font_core::{
    font_formats::{self, cmap, Format},
    font_io, Contour, FontDocument, Segment,
};
fn source() -> FontDocument {
    let mut d = FontDocument::new("Formats Test", "A😀𠀀").unwrap();
    d.metadata.copyright = "Copyright Example".into();
    d.metadata.license = "Test license".into();
    d.glyphs.get_mut(&1).unwrap().contours = vec![
        Contour {
            smooth: Default::default(),
            start: [10., 20.],
            segments: vec![
                Segment::Cubic {
                    control1: [100., 800.],
                    control2: [450., 800.],
                    end: [590., 20.],
                },
                Segment::Line { end: [10., 20.] },
            ],
            closed: true,
        },
        Contour {
            smooth: Default::default(),
            start: [200., 150.],
            segments: vec![Segment::Quadratic {
                control: [300., 350.],
                end: [400., 150.],
            }],
            closed: true,
        },
    ];
    d
}
#[test]
fn all_outline_formats_are_readable_and_convert_back() {
    let d = source();
    for format in [
        Format::Ttf,
        Format::Otf,
        Format::CidOtf,
        Format::Pfa,
        Format::Pfb,
        Format::Cid,
        Format::Cff,
        Format::CidCff,
    ] {
        let bytes =
            font_formats::export(&d, format).unwrap_or_else(|e| panic!("{format:?} export: {e}"));
        let mut imported =
            font_io::import(&bytes, 0).unwrap_or_else(|e| panic!("{format:?} import: {e}"));
        assert_eq!(imported.glyphs.len(), 4, "{format:?}");
        assert_eq!(imported.glyphs[&1].contours.len(), 2, "{format:?}");
        assert!(
            (imported.glyphs[&1].advance - 600.).abs() < 1.,
            "{format:?}"
        );
        if matches!(format, Format::Cid | Format::CidCff) {
            let map =
                cmap::parse(std::str::from_utf8(&cmap::export(&d).unwrap()).unwrap()).unwrap();
            for g in cmap::apply(&imported, &map).unwrap() {
                imported.glyphs.insert(g.id, g);
            }
        }
        assert!(
            imported
                .glyphs
                .values()
                .any(|g| g.codepoints.contains(&'😀')),
            "{format:?} supplementary mapping"
        );
        font_io::export(&imported).unwrap_or_else(|e| panic!("{format:?} -> TTF: {e}"));
    }
}
#[test]
fn cmap_preserves_aliases_and_cids_and_rejects_sequences() {
    let mut d = source();
    d.glyphs.get_mut(&1).unwrap().codepoints.insert('À');
    d.glyphs.get_mut(&1).unwrap().cid = Some(42);
    let bytes = cmap::export(&d).unwrap();
    let map = cmap::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(map[&42].len(), 2);
    assert!(cmap::parse("1 beginbfchar <0001> <00660069> endbfchar").is_err());
    assert!(cmap::parse("/Base usecmap 1 begincidchar <0041> 1 endcidchar").is_err());
    assert!(cmap::parse("1 begincidchar <D800> 1 endcidchar").is_err());
    let map = cmap::parse("1 beginbfrange <0001> <0003> [<0041> <D83DDE00> <D840DC00>] endbfrange")
        .unwrap();
    assert_eq!(map[&2], ['😀'].into());
    let mut cleared = d.clone();
    for g in cleared.glyphs.values_mut() {
        g.codepoints.clear();
    }
    for g in cmap::apply(
        &cleared,
        &cmap::parse(std::str::from_utf8(&bytes).unwrap()).unwrap(),
    )
    .unwrap()
    {
        cleared.glyphs.insert(g.id, g);
    }
    assert_eq!(cleared.glyphs, d.glyphs);
}

#[test]
fn cid_ros_sparse_numbers_and_metadata_survive_conversion() {
    let mut d = source();
    d.metadata.cid_system = Some(font_core::CidSystem {
        registry: "Adobe".into(),
        ordering: "Japan1".into(),
        supplement: 7,
    });
    for (g, cid) in d.glyphs.values_mut().zip([0, 42, 3, 9]) {
        g.cid = Some(cid);
    }
    for format in [Format::CidOtf, Format::CidCff, Format::Cid] {
        let bytes = font_formats::export(&d, format).unwrap();
        let imported = font_io::import(&bytes, 0).unwrap_or_else(|e| panic!("{format:?}: {e}"));
        assert_eq!(
            imported.metadata.cid_system, d.metadata.cid_system,
            "{format:?}"
        );
        let g = imported
            .glyphs
            .values()
            .find(|g| g.cid == Some(42))
            .unwrap();
        assert_eq!(g.contours.len(), 2, "{format:?}");
        let map = cmap::parse(std::str::from_utf8(&cmap::export(&d).unwrap()).unwrap()).unwrap();
        let mut mapped = imported.clone();
        for g in cmap::apply(&mapped, &map).unwrap() {
            mapped.glyphs.insert(g.id, g);
        }
        assert!(mapped
            .glyphs
            .values()
            .any(|g| g.cid == Some(3) && g.codepoints.contains(&'😀')));
    }
}
#[test]
fn style_and_rights_survive_opentype_and_legacy_notices_are_escaped() {
    let mut d = source();
    d.metadata.style = "Bold Italic".into();
    d.metadata.weight = 700;
    d.metadata.width = 5;
    d.metadata.bold = true;
    d.metadata.italic = true;
    d.metadata.italic_angle = -12.;
    d.metadata.copyright = "Notice (test) \\ encoded".into();
    d.metadata.license_url = "https://example.invalid/license".into();
    for format in [Format::Ttf, Format::Otf, Format::CidOtf] {
        let imported = font_io::import(&font_formats::export(&d, format).unwrap(), 0).unwrap();
        assert_eq!(imported.metadata.style, d.metadata.style);
        assert_eq!(imported.metadata.weight, 700);
        assert!(imported.metadata.bold && imported.metadata.italic);
        assert_eq!(imported.metadata.italic_angle, -12.);
        assert_eq!(imported.metadata.copyright, d.metadata.copyright);
        assert_eq!(imported.metadata.license, d.metadata.license);
        assert_eq!(imported.metadata.license_url, d.metadata.license_url);
    }
    for format in [Format::Pfa, Format::Pfb, Format::Cid, Format::Cff] {
        let imported = font_io::import(&font_formats::export(&d, format).unwrap(), 0).unwrap();
        assert_eq!(
            imported.metadata.copyright, d.metadata.copyright,
            "{format:?}"
        );
        assert_eq!(imported.metadata.italic_angle, -12., "{format:?}");
    }
}
#[test]
fn type1_expands_unicode_aliases_and_cmap_edit_is_one_undo_transaction() {
    let mut d = source();
    d.glyphs.get_mut(&1).unwrap().codepoints.insert('À');
    let imported = font_io::import(&font_formats::export(&d, Format::Pfb).unwrap(), 0).unwrap();
    assert!(imported
        .glyphs
        .values()
        .any(|g| g.codepoints.contains(&'À')));
    assert!(imported
        .glyphs
        .values()
        .any(|g| g.codepoints.contains(&'A')));
    let mut editor = font_core::Editor::new(source()).unwrap();
    let before = editor.document().clone();
    let map = cmap::parse("1 begincidchar <0042> 1 endcidchar").unwrap();
    let glyphs = cmap::apply(editor.document(), &map).unwrap();
    editor.replace_glyphs(0, glyphs).unwrap();
    assert!(editor.document().glyphs[&1].codepoints.contains(&'B'));
    editor.undo(1).unwrap();
    assert_eq!(editor.document(), &before);
    editor.redo(2).unwrap();
    assert!(editor.document().glyphs[&1].codepoints.contains(&'B'));
}
#[test]
fn invalid_cmap_cid_and_open_contours_fail_atomically() {
    for text in [
        "1 beginbfchar <😃> <0041> endbfchar",
        "1 beginbfchar <0001> <D800> endbfchar",
        "/CMapName /90ms-RKSJ-H 1 begincidchar <8140> 1 endcidchar",
        "2 begincidchar <0041> 1 endcidchar",
        "1 begincidrange <0043> <0041> 1 endcidrange",
    ] {
        assert!(cmap::parse(text).is_err(), "{text}");
    }
    let mut d = source();
    d.glyphs.get_mut(&1).unwrap().contours[0].closed = false;
    for format in [
        Format::Ttf,
        Format::Otf,
        Format::CidOtf,
        Format::Pfa,
        Format::Pfb,
        Format::Cid,
        Format::Cff,
        Format::CidCff,
    ] {
        assert_eq!(
            font_formats::export(&d, format),
            Err("font_open_contour".into()),
            "{format:?}"
        );
    }
}
#[test]
fn bundled_engine_version_is_pinned() {
    unsafe extern "C" {
        fn FT_Library_Version(
            library: freetype::ffi::FT_Library,
            major: *mut i32,
            minor: *mut i32,
            patch: *mut i32,
        );
    }
    let library = freetype::Library::init().unwrap();
    let (mut major, mut minor, mut patch) = (0, 0, 0);
    unsafe {
        FT_Library_Version(library.raw(), &mut major, &mut minor, &mut patch);
    }
    assert_eq!((major, minor, patch), (2, 14, 3));
}
