use font_core::{font_io, Contour, FontDocument, Segment};
fn source() -> FontDocument {
    let mut doc = FontDocument::new("試作 中文 Test", "A😀𠀀\u{301}").unwrap();
    doc.glyphs.get_mut(&1).unwrap().contours = vec![
        Contour {
            smooth: Default::default(),
            start: [10., 0.],
            segments: vec![
                Segment::Line { end: [300., 700.] },
                Segment::Quadratic {
                    control: [500., 700.],
                    end: [580., 0.],
                },
            ],
            closed: true,
        },
        Contour {
            smooth: Default::default(),
            start: [200., 100.],
            segments: vec![
                Segment::Line { end: [400., 100.] },
                Segment::Line { end: [300., 400.] },
            ],
            closed: true,
        },
    ];
    doc
}
#[test]
fn true_type_roundtrip_unicode_curves_holes_metrics_names_and_checksum() {
    let doc = source();
    let bytes = font_io::export(&doc).unwrap();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    assert_eq!(face.number_of_glyphs(), 5);
    assert_eq!(font_io::name(&face, 1), doc.family);
    assert_eq!(face.glyph_index('😀').unwrap().0, 2);
    assert_eq!(face.glyph_index('𠀀').unwrap().0, 3);
    assert_eq!(face.glyph_hor_advance(ttf_parser::GlyphId(1)), Some(600));
    let sum = bytes.chunks(4).fold(0u32, |s, b| {
        s.wrapping_add(u32::from_be_bytes(b.try_into().unwrap()))
    });
    assert_eq!(sum, 0xB1B0AFBA);
    let imported = font_io::import(&bytes, 0).unwrap();
    assert_eq!(imported.glyphs[&1].contours.len(), 2);
    assert_eq!(imported.glyphs[&2].codepoints, doc.glyphs[&2].codepoints);
    assert_eq!(imported.units_per_em, 1000);
    assert!(imported.glyphs[&1].contours.iter().all(|c| c.closed));
    assert_eq!(face.glyph_index('A').unwrap().0, 1);
    let bmp = face
        .tables()
        .cmap
        .unwrap()
        .subtables
        .into_iter()
        .find(|t| t.platform_id == ttf_parser::PlatformId::Windows && t.encoding_id == 1)
        .unwrap();
    assert_eq!(bmp.glyph_index('A' as u32).unwrap().0, 1);
}
#[test]
fn cubic_conversion_and_open_or_out_of_range_rejection() {
    let mut doc = source();
    doc.glyphs.get_mut(&1).unwrap().contours[0].segments = vec![Segment::Cubic {
        control1: [100., 1000.],
        control2: [500., 1000.],
        end: [600., 0.],
    }];
    let bytes = font_io::export(&doc).unwrap();
    let imported = font_io::import(&bytes, 0).unwrap();
    assert!(imported.glyphs[&1].contours[0].segments.len() > 2);
    doc.glyphs.get_mut(&1).unwrap().contours[0].closed = false;
    assert_eq!(font_io::export(&doc), Err("font_open_contour".into()));
    doc.glyphs.get_mut(&1).unwrap().contours[0].closed = true;
    doc.glyphs.get_mut(&1).unwrap().contours[0].start = [50000., 0.];
    assert_eq!(font_io::export(&doc), Err("font_coordinate_range".into()));
    assert!(font_io::import(b"invalid", 0).is_err());
}
#[test]
fn sparse_ids_are_remapped_and_empty_font_is_readable() {
    let mut doc = source();
    let mut g = doc.glyphs.remove(&2).unwrap();
    g.id = 100;
    doc.glyphs.insert(100, g);
    let bytes = font_io::export(&doc).unwrap();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    assert_eq!(face.glyph_index('😀').unwrap().0, 4);
    let mut empty = FontDocument::new("Empty", " ").unwrap();
    empty.units_per_em = 16384;
    let bytes = font_io::export(&empty).unwrap();
    assert_eq!(
        ttf_parser::Face::parse(&bytes, 0)
            .unwrap()
            .number_of_glyphs(),
        2
    );
}
