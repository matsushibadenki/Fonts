use font_core::{
    source_font::{master_project::Edit, master_sample::Sampler, variable, SourceFont},
    Editor,
};
use freetype::{face::LoadFlag, ffi};
use std::sync::Arc;
#[test]
fn moving_horizontal_and_vertical_origins_survive_outline_metric_topology_and_range_edits() {
    let bytes = include_bytes!("data/variations/moving-origins.ttf");
    let mut editor = Editor::new(SourceFont::variable_document(bytes, 0, None).unwrap()).unwrap();
    editor
        .edit_source_variations(
            0,
            variable::Edit::Coordinates {
                values: [("wght".into(), 900.), ("wdth".into(), 100.)].into(),
            },
        )
        .unwrap();
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Register {
                name: "Heavy".into(),
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
        .edit_source_master(editor.revision(), Edit::Capture { id, glyph: 1 })
        .unwrap();
    let source = editor.document().source_font.as_ref().unwrap();
    let project = source.masters.as_ref().unwrap();
    let shape = project.shape(source, id, 1).unwrap();
    assert!((project.frame(source, id, 1).unwrap()[0] + 48.).abs() < 0.02);
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Point {
                id,
                glyph: 1,
                index: 0,
                position: [shape.contours[0].start[0] + 25., shape.contours[0].start[1]],
            },
        )
        .unwrap();
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Advance {
                id,
                glyph: 1,
                value: shape.advance + 35.,
            },
        )
        .unwrap();
    let source = editor.document().source_font.as_ref().unwrap();
    let output = source.export().unwrap();
    source.verify_output(&output, false).unwrap();
    let mut old = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
    let mut new = Sampler::new(Arc::new(output.clone()), 0).unwrap();
    let library = freetype::Library::init().unwrap();
    let mut a = library.new_memory_face(bytes.to_vec(), 0).unwrap();
    let mut b = library.new_memory_face(output, 0).unwrap();
    for x in [-1., 0., 0.125, 0.25, 0.5, 0.75, 1.] {
        for y in [-1., 0., 0.5, 1.] {
            let location = [("wght".into(), x), ("wdth".into(), y)].into();
            let weight: f64 = if x > 0. { f64::from(x) } else { 0. };
            let original = old.sample_normalized(1, &location).unwrap().shape;
            let actual = new.sample_normalized(1, &location).unwrap().shape;
            assert!(
                (actual.contours[0].start[0] - original.contours[0].start[0] - 25. * weight).abs()
                    < 0.02
            );
            assert!((actual.advance - original.advance - 35. * weight).abs() < 0.02);
            let coordinates = [
                (f64::from(x) * 65536.).round() as ffi::FT_Fixed,
                (f64::from(y) * 65536.).round() as ffi::FT_Fixed,
            ];
            for f in [&mut a, &mut b] {
                // SAFETY: the face and two fixed coordinates remain alive during this synchronous call.
                assert_eq!(
                    unsafe {
                        ffi::FT_Set_Var_Blend_Coordinates(f.raw_mut(), 2, coordinates.as_ptr())
                    },
                    0
                );
                f.load_glyph(
                    1,
                    LoadFlag::NO_SCALE
                        | LoadFlag::NO_HINTING
                        | LoadFlag::NO_AUTOHINT
                        | LoadFlag::NO_BITMAP,
                )
                .unwrap();
            }
            let before = a.glyph().metrics();
            let after = b.glyph().metrics();
            assert_eq!(before.vertAdvance, after.vertAdvance);
            assert_eq!(
                before.vertBearingY + before.horiBearingY,
                after.vertBearingY + after.horiBearingY
            );
        }
    }
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Topology {
                glyph: 1,
                operation: font_core::source_font::master_topology::Operation::Split {
                    contour: 0,
                    segment: 0,
                },
            },
        )
        .unwrap();
    editor
        .edit_source_variations(
            editor.revision(),
            variable::Edit::Range {
                tag: "wght".into(),
                range: variable::Range {
                    min: 400.,
                    default: 650.,
                    max: 900.,
                },
            },
        )
        .unwrap();
    let source = editor.document().source_font.as_ref().unwrap();
    let output = source.export().unwrap();
    source.verify_output(&output, false).unwrap();
    let instance = source.static_document().unwrap();
    let point = instance.glyphs[&1].contours[0].start;
    assert!((point[0] + 3.).abs() < 0.1, "{point:?}");
}
