use font_core::{
    source_font::{master_cff::Font, master_project::Edit, master_sample::Sampler, SourceFont},
    Editor,
};
use std::sync::Arc;
const FONT: &[u8] = include_bytes!("data/variations/cff2.otf");
#[test]
fn symbolic_cff2_rewrite_and_master_edit_keep_source_variation() {
    let face = ttf_parser::Face::parse(FONT, 0).unwrap();
    let axes = vec!["wght".into(), "wdth".into()];
    let font = Font::parse(
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CFF2"))
            .unwrap(),
        &axes,
        4,
        1000,
    )
    .unwrap();
    let glyph = font.glyph(1).unwrap();
    let (table, error) = font
        .rewrite(&[(1, glyph.clone())].into(), &font.supports)
        .unwrap();
    assert_eq!(error, 0.);
    let again = Font::parse(&table, &axes, 4, 1000).unwrap();
    for x in [-1., 0., 0.125, 0.5, 0.75, 1.] {
        for y in [-1., 0., 0.5, 1.] {
            let loc = [("wght".into(), x), ("wdth".into(), y)].into();
            assert_eq!(
                glyph.points(&font.weights(&loc).unwrap()).unwrap(),
                again
                    .glyph(1)
                    .unwrap()
                    .points(&again.weights(&loc).unwrap())
                    .unwrap()
            );
        }
    }
    let mut e = Editor::new(SourceFont::variable_document(FONT, 0, None).unwrap()).unwrap();
    e.edit_source_master(
        0,
        Edit::Register {
            name: "Default".into(),
        },
    )
    .unwrap();
    let id = e
        .document()
        .source_font
        .as_ref()
        .unwrap()
        .masters
        .as_ref()
        .unwrap()
        .masters
        .entries[0]
        .id;
    e.edit_source_master(e.revision(), Edit::Capture { id, glyph: 1 })
        .unwrap();
    let before = e.document().clone();
    let source = before.source_font.as_ref().unwrap();
    let shape = source
        .masters
        .as_ref()
        .unwrap()
        .shape(source, id, 1)
        .unwrap();
    let original = shape.contours[0].start;
    e.edit_source_master(
        e.revision(),
        Edit::Point {
            id,
            glyph: 1,
            index: 0,
            position: [original[0] + 25., original[1] + 10.],
        },
    )
    .unwrap();
    let source = e.document().source_font.as_ref().unwrap();
    let out = source.export().unwrap();
    source.verify_output(&out, false).unwrap();
    let mut old = Sampler::new(Arc::new(FONT.to_vec()), 0).unwrap();
    let mut new = Sampler::new(Arc::new(out), 0).unwrap();
    for x in [-1., 0., 0.25, 0.5, 1.] {
        for y in [-1., 0., 1.] {
            let loc = [("wght".into(), x), ("wdth".into(), y)].into();
            let a = old.sample_normalized(1, &loc).unwrap().shape;
            let b = new.sample_normalized(1, &loc).unwrap().shape;
            assert!((b.contours[0].start[0] - a.contours[0].start[0] - 25.).abs() < 0.02);
            assert!((b.contours[0].start[1] - a.contours[0].start[1] - 10.).abs() < 0.02);
            assert!((b.advance - a.advance).abs() < 0.02);
        }
    }
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
}

fn cloud(contours: &[font_core::Contour]) -> Vec<font_core::Point> {
    let mut out = vec![];
    let lerp = |a: font_core::Point, b: font_core::Point, t: f64| {
        [a[0] * (1. - t) + b[0] * t, a[1] * (1. - t) + b[1] * t]
    };
    for c in contours {
        let mut start = c.start;
        for segment in &c.segments {
            for step in 0..=16 {
                let t = step as f64 / 16.;
                let p = match *segment {
                    font_core::Segment::Line { end } => lerp(start, end, t),
                    font_core::Segment::Quadratic { control, end } => {
                        lerp(lerp(start, control, t), lerp(control, end, t), t)
                    }
                    font_core::Segment::Cubic {
                        control1,
                        control2,
                        end,
                    } => lerp(
                        lerp(lerp(start, control1, t), lerp(control1, control2, t), t),
                        lerp(lerp(control1, control2, t), lerp(control2, end, t), t),
                        t,
                    ),
                };
                out.push(p);
            }
            start = match *segment {
                font_core::Segment::Line { end }
                | font_core::Segment::Quadratic { end, .. }
                | font_core::Segment::Cubic { end, .. } => end,
            };
        }
    }
    out
}
#[test]
fn nested_blends_subroutines_flex_hints_and_font_matrices_match_native_geometry() {
    for bytes in [
        include_bytes!("data/variations/nested-blend-cff2.otf").as_slice(),
        include_bytes!("data/variations/matrix-cff2.otf").as_slice(),
    ] {
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        let axes = vec!["wght".into(), "wdth".into()];
        let font = Font::parse(
            face.raw_face()
                .table(ttf_parser::Tag::from_bytes(b"CFF2"))
                .unwrap(),
            &axes,
            4,
            face.units_per_em(),
        )
        .unwrap();
        let glyph = font.glyph(1).unwrap();
        let mut native = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
        let (rewritten, error) = font
            .rewrite_sfnt(bytes, &[(1, glyph.clone())].into(), &font.supports)
            .unwrap();
        assert!(error < 0.01);
        let mut output = Sampler::new(Arc::new(rewritten), 0).unwrap();
        for x in [-1., 0., 0.375, 1.] {
            for y in [-1., 0., 1.] {
                let location = [("wght".into(), x), ("wdth".into(), y)].into();
                let expected = cloud(&glyph.contours(&font.weights(&location).unwrap()).unwrap());
                let source = native
                    .sample_normalized(1, &location)
                    .unwrap()
                    .shape
                    .contours;
                let actual = output
                    .sample_normalized(1, &location)
                    .unwrap()
                    .shape
                    .contours;
                for p in cloud(&source) {
                    let distance = expected
                        .iter()
                        .map(|q| (p[0] - q[0]).hypot(p[1] - q[1]))
                        .fold(f64::INFINITY, f64::min);
                    assert!(
                        distance < 0.08,
                        "source {x},{y}: {p:?}, distance {distance}"
                    );
                }
                for p in cloud(&actual) {
                    let distance = expected
                        .iter()
                        .map(|q| (p[0] - q[0]).hypot(p[1] - q[1]))
                        .fold(f64::INFINITY, f64::min);
                    assert!(
                        distance < 0.08,
                        "output {x},{y}: {p:?}, distance {distance}"
                    );
                }
            }
        }
    }
}

#[test]
fn nonconforming_cff2_matrix_is_rejected_before_a_master_edit_can_be_saved() {
    let bytes = include_bytes!("data/variations/invalid-matrix-cff2.otf");
    let face = ttf_parser::Face::parse(bytes, 0).unwrap();
    let axes = vec!["wght".into(), "wdth".into()];
    let font = Font::parse(
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CFF2"))
            .unwrap(),
        &axes,
        4,
        face.units_per_em(),
    )
    .unwrap();
    assert!(font.glyph(1).is_err());
}

#[test]
fn varying_flex_direction_remains_a_native_conditional_and_protects_coupled_endpoints() {
    let bytes = include_bytes!("data/variations/variable-flex-cff2.otf");
    let face = ttf_parser::Face::parse(bytes, 0).unwrap();
    let axes = vec!["wght".into(), "wdth".into()];
    let font = Font::parse(
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CFF2"))
            .unwrap(),
        &axes,
        4,
        1000,
    )
    .unwrap();
    let glyph = font.glyph(1).unwrap();
    let (rewritten, _) = font
        .rewrite_sfnt(bytes, &[(1, glyph)].into(), &font.supports)
        .unwrap();
    let mut before = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
    let mut after = Sampler::new(Arc::new(rewritten), 0).unwrap();
    for x in [
        -1., -0.75001, -0.75, -0.74999, -0.25001, -0.25, -0.24999, 0., 1.,
    ] {
        let loc = [("wght".into(), x), ("wdth".into(), 0.)].into();
        assert_eq!(
            before.sample_normalized(1, &loc).unwrap().shape.contours,
            after.sample_normalized(1, &loc).unwrap().shape.contours
        );
    }
    let mut e = Editor::new(SourceFont::variable_document(bytes, 0, None).unwrap()).unwrap();
    e.edit_source_master(
        0,
        Edit::Register {
            name: "Default".into(),
        },
    )
    .unwrap();
    let id = e
        .document()
        .source_font
        .as_ref()
        .unwrap()
        .masters
        .as_ref()
        .unwrap()
        .masters
        .entries[0]
        .id;
    e.edit_source_master(e.revision(), Edit::Capture { id, glyph: 1 })
        .unwrap();
    let source = e.document().source_font.as_ref().unwrap();
    let shape = source
        .masters
        .as_ref()
        .unwrap()
        .shape(source, id, 1)
        .unwrap();
    let points = source.masters.as_ref().unwrap().topologies[&1]
        .physical_points(&shape.contours)
        .unwrap();
    e.edit_source_master(
        e.revision(),
        Edit::Point {
            id,
            glyph: 1,
            index: 1,
            position: [points[1][0] + 5.12345, points[1][1]],
        },
    )
    .unwrap();
    let before = e.document().clone();
    assert!(e
        .edit_source_master(
            e.revision(),
            Edit::Point {
                id,
                glyph: 1,
                index: 0,
                position: [points[0][0] + 5., points[0][1]]
            }
        )
        .is_err());
    assert_eq!(e.document(), &before);
    let source = e.document().source_font.as_ref().unwrap();
    let output = source.export().unwrap();
    source.verify_output(&output, false).unwrap();
    let mut original = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
    let mut edited = Sampler::new(Arc::new(output), 0).unwrap();
    let end = |s: &font_core::Segment| match *s {
        font_core::Segment::Line { end }
        | font_core::Segment::Quadratic { end, .. }
        | font_core::Segment::Cubic { end, .. } => end,
    };
    for x in [
        -1., -0.75001, -0.75, -0.74999, -0.25001, -0.25, -0.24999, 0., 1.,
    ] {
        let loc = [("wght".into(), x), ("wdth".into(), 0.)].into();
        let a = original.sample_normalized(1, &loc).unwrap().shape;
        let b = edited.sample_normalized(1, &loc).unwrap().shape;
        assert_eq!(
            end(&a.contours[0].segments[1]),
            end(&b.contours[0].segments[1])
        );
    }
}

#[test]
fn color_variable_master_edits_retain_paints_and_palettes() {
    for bytes in [
        include_bytes!("data/variations/color-cff2.otf").as_slice(),
        include_bytes!("data/variations/color-gvar.ttf").as_slice(),
    ] {
        let doc = SourceFont::variable_document(bytes, 0, None).unwrap();
        let glyph = doc
            .glyphs
            .values()
            .find(|g| g.id != 0 && !g.contours.is_empty())
            .unwrap()
            .id;
        let mut editor = Editor::new(doc).unwrap();
        editor
            .edit_source_master(
                0,
                Edit::Register {
                    name: "Default".into(),
                },
            )
            .unwrap();
        let id = editor
            .document()
            .source_font
            .as_ref()
            .unwrap()
            .masters
            .as_ref()
            .unwrap()
            .masters
            .entries[0]
            .id;
        editor
            .edit_source_master(editor.revision(), Edit::Capture { id, glyph })
            .unwrap();
        let source = editor.document().source_font.as_ref().unwrap();
        assert!(source.summary().master_editable);
        let shape = source
            .masters
            .as_ref()
            .unwrap()
            .shape(source, id, glyph)
            .unwrap();
        let p = shape.contours[0].start;
        let original = source.export().unwrap();
        editor
            .edit_source_master(
                editor.revision(),
                Edit::Point {
                    id,
                    glyph,
                    index: 0,
                    position: [p[0] + 20., p[1]],
                },
            )
            .unwrap();
        let source = editor.document().source_font.as_ref().unwrap();
        let output = source.export().unwrap();
        source.verify_output(&output, false).unwrap();
        let old = ttf_parser::Face::parse(&original, 0).unwrap();
        let new = ttf_parser::Face::parse(&output, 0).unwrap();
        for tag in [b"COLR", b"CPAL", b"SVG ", b"sbix", b"CBDT", b"CBLC"] {
            assert_eq!(
                old.raw_face().table(ttf_parser::Tag::from_bytes(tag)),
                new.raw_face().table(ttf_parser::Tag::from_bytes(tag))
            );
        }
    }
}
