use super::*;
use crate::{geometry, identity::Uuid, snippets::Info, Editor};
fn editor() -> (Editor, Uuid) {
    let mut doc = FontDocument::new("Compound", "ABC").unwrap();
    doc.glyphs.get_mut(&1).unwrap().contours = vec![Contour {
        start: [10., 0.],
        segments: vec![
            Segment::Quadratic {
                control: [150., 400.],
                end: [300., 0.],
            },
            Segment::Line { end: [10., 0.] },
        ],
        closed: true,
        identity: Default::default(),
        smooth: Default::default(),
    }];
    let mut e = Editor::new(doc).unwrap();
    let id = e
        .capture_component(
            e.revision(),
            1,
            &[0].into(),
            Info {
                name: "Part".into(),
                tags: "".into(),
                category: "radical".into(),
                source: "".into(),
                license: "".into(),
                favorite: false,
            },
        )
        .unwrap();
    (e, id)
}
fn tables(bytes: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
    let f = ttf_parser::RawFace::parse(bytes, 0).unwrap();
    f.table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), f.table(r.tag).unwrap().to_vec()))
        .collect()
}
fn pack(mut t: BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
    t.get_mut(b"head").unwrap()[8..12].fill(0);
    crate::color_fonts::assemble(t, false).unwrap()
}
fn at(t: &BTreeMap<[u8; 4], Vec<u8>>, id: usize) -> usize {
    u32::from_be_bytes(t[b"loca"][id * 4..id * 4 + 4].try_into().unwrap()) as usize
}
#[test]
fn linked_composite_export_import_edit_save_and_woff2_keep_structure() {
    let (mut e, id) = editor();
    for (g, m) in [
        (2, [1., 0., 0., 1., 50., 20.]),
        (2, [0.5, 0., 0., 0.5, 300., 200.]),
        (3, [0., 1., -1., 0., 500., 0.]),
    ] {
        e.place_component(e.revision(), id, g, m).unwrap();
    }
    let source = e.document().clone();
    let output = export(&source).unwrap();
    assert_eq!(e.document(), &source);
    assert_eq!(output.composites, 2);
    assert_eq!(output.helpers, 0);
    crate::variations::verify(&output.bytes, &output.expected).unwrap();
    let decoded = decode(&output.bytes, 0).unwrap();
    assert_eq!(decoded[&2].parts.len(), 2);
    let imported = import(&output.bytes, 0).unwrap();
    assert_eq!(imported.components.definitions.len(), 1);
    assert_eq!(imported.components.instances.len(), 3);
    assert_eq!(imported.glyphs[&1].codepoints, source.glyphs[&1].codepoints);
    let mut e = Editor::new(imported).unwrap();
    let id = *e.document().components.definitions.keys().next().unwrap();
    let before = e.document().clone();
    let mut contours = before.components.definitions[&id].contours.as_ref().clone();
    geometry::split_segment(&mut contours[0], 0).unwrap();
    e.edit_component_contours(e.revision(), id, contours)
        .unwrap();
    let changed = e.document().clone();
    assert_ne!(before.glyphs[&2], changed.glyphs[&2]);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
    e.redo(e.revision()).unwrap();
    assert_eq!(e.document(), &changed);
    let file = std::env::temp_dir().join(format!("components-{}.json", Uuid::new_v4()));
    crate::storage::save(&file, &changed).unwrap();
    assert_eq!(crate::storage::load(&file).unwrap(), changed);
    std::fs::remove_file(file).unwrap();
    let woff = crate::font_formats::woff2::encode_sfnt(output.bytes.clone(), 9).unwrap();
    let sfnt = crate::font_formats::woff2::decode(&woff).unwrap();
    let back = import(&sfnt, 0).unwrap();
    assert_eq!(back.components.instances.len(), 3);
}
#[test]
fn mixed_outlines_have_helpers_and_unrepresentable_transforms_are_explicit() {
    let (mut e, id) = editor();
    let mut g = e.document().glyphs[&2].clone();
    g.contours = e.document().glyphs[&1].contours.clone();
    for c in &mut g.contours {
        c.identity = Default::default();
    }
    e.replace_glyph(e.revision(), g).unwrap();
    let instance = e
        .place_component(e.revision(), id, 2, [1., 0., 0., 1., 400., 0.])
        .unwrap();
    let out = export(e.document()).unwrap();
    assert_eq!(out.helpers, 1);
    assert_eq!(decode(&out.bytes, 0).unwrap()[&2].parts.len(), 2);
    e.transform_component(e.revision(), instance, [1., 0., 0., 1., 0.1, 0.])
        .unwrap();
    assert_eq!(
        export(e.document()).err().unwrap(),
        "font_component_transform"
    );
    assert!(crate::font_io::export(e.document()).is_ok());
}
#[test]
fn point_attachment_scaled_offsets_byte_arguments_and_invalid_graphs() {
    let (mut e, id) = editor();
    e.place_component(e.revision(), id, 2, [1., 0., 0., 1., 0., 0.])
        .unwrap();
    e.place_component(e.revision(), id, 2, [1., 0., 0., 1., 100., 0.])
        .unwrap();
    let out = export(e.document()).unwrap();
    let original = tables(&out.bytes);
    let offset = at(&original, 2);
    // The second component aligns its raw point 0 with accumulated raw point 1.
    let mut t = original.clone();
    let glyph = t.get_mut(b"glyf").unwrap();
    let second = offset + 10 + 16;
    glyph[second..second + 2].copy_from_slice(&0x1481u16.to_be_bytes());
    glyph[second + 4..second + 6].copy_from_slice(&1u16.to_be_bytes());
    glyph[second + 6..second + 8].copy_from_slice(&0u16.to_be_bytes());
    let bytes = pack(t);
    let shapes = decode(&bytes, 0).unwrap();
    let point = shapes[&2].rings[0][1].0;
    let attached = shapes[&2].rings[1][0].0;
    assert_eq!(point, attached);
    let doc = import(&bytes, 0).unwrap();
    assert_eq!(doc.components.instances.len(), 2);
    doc.validate().unwrap();
    // Explicit scaled offsets: 100*0.5 produces a 50-unit translation.
    let mut t = original.clone();
    let glyph = t.get_mut(b"glyf").unwrap();
    glyph[second..second + 2].copy_from_slice(&0x0883u16.to_be_bytes());
    glyph[second + 8..second + 10].copy_from_slice(&8192i16.to_be_bytes());
    glyph[second + 14..second + 16].copy_from_slice(&8192i16.to_be_bytes());
    let shapes = decode(&pack(t), 0).unwrap();
    assert_eq!(shapes[&2].parts[1].matrix[4], 50.);
    for (field, value) in [(0, 0x1C83u16), (2, 2u16), (2, 65535u16)] {
        let mut t = original.clone();
        t.get_mut(b"glyf").unwrap()[offset + 10 + field..offset + 12 + field]
            .copy_from_slice(&value.to_be_bytes());
        assert!(decode(&pack(t), 0).is_err());
    }
}
#[test]
fn malformed_loca_instructions_and_zero_contours_are_bounded() {
    let (e, _) = editor();
    let out = export(e.document()).unwrap();
    let original = tables(&out.bytes);
    for cut in [0, 1, 4, original[b"loca"].len() - 1] {
        let mut t = original.clone();
        t.get_mut(b"loca").unwrap().truncate(cut);
        assert!(decode(&pack(t), 0).is_err());
    }
    let shape = Shape {
        depth: 1,
        metrics: false,
        rings: vec![vec![
            ([0., 0.], false),
            ([100., 100.], false),
            ([200., 0.], false),
        ]],
        parts: vec![],
    };
    let c = shape.contours();
    assert_eq!(c[0].start, [100., 0.]);
    geometry::validate(&c[0]).unwrap();
    let mut reader = Reader { b: &[], p: 0 };
    assert!(simple(&mut reader, 0).unwrap().rings.is_empty());
}

#[test]
fn independent_freetype_raster_matches_flattened_ttf_and_nested_import() {
    let (mut e, id) = editor();
    e.place_component(e.revision(), id, 2, [1., 0., 0., 1., 40., 20.])
        .unwrap();
    e.place_component(e.revision(), id, 2, [0.5, 0., 0., 0.5, 300., 200.])
        .unwrap();
    let linked = export(e.document()).unwrap();
    let flat = font_io::export(e.document()).unwrap();
    let lib = freetype::Library::init().unwrap();
    for size in [9, 24, 64] {
        let render = |bytes: Vec<u8>| {
            let f = lib.new_memory_face(bytes, 0).unwrap();
            f.set_pixel_sizes(0, size).unwrap();
            f.load_glyph(
                2,
                freetype::face::LoadFlag::NO_HINTING
                    | freetype::face::LoadFlag::NO_AUTOHINT
                    | freetype::face::LoadFlag::RENDER,
            )
            .unwrap();
            let g = f.glyph();
            let b = g.bitmap();
            (
                g.bitmap_left(),
                g.bitmap_top(),
                b.width(),
                b.rows(),
                b.buffer().to_vec(),
            )
        };
        let a = render(linked.bytes.clone());
        let b = render(flat.clone());
        assert_eq!((a.0, a.1, a.2, a.3), (b.0, b.1, b.2, b.3));
        let error =
            a.4.iter()
                .zip(&b.4)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap_or(0);
        assert!(
            error <= 16,
            "two-stage subpixel quantization at {size}px: {error}"
        );
    }
    let outline = |bytes: Vec<u8>| {
        let f = lib.new_memory_face(bytes, 0).unwrap();
        f.load_glyph(
            2,
            freetype::face::LoadFlag::NO_SCALE
                | freetype::face::LoadFlag::NO_HINTING
                | freetype::face::LoadFlag::NO_BITMAP,
        )
        .unwrap();
        let points = f
            .glyph()
            .outline()
            .unwrap()
            .points()
            .iter()
            .map(|p| (p.x, p.y))
            .collect::<Vec<_>>();
        points
    };
    assert_eq!(outline(linked.bytes.clone()), outline(flat));
    // Reference the exported composite as a child to exercise two-level input.
    let mut t = tables(&linked.bytes);
    let offset = at(&t, 3);
    let old_end = at(&t, 4);
    let mut record = vec![];
    record.extend((-1i16).to_be_bytes());
    record.extend([0; 8]);
    record.extend(0x1003u16.to_be_bytes());
    record.extend(2u16.to_be_bytes());
    record.extend(10i16.to_be_bytes());
    record.extend(20i16.to_be_bytes());
    while record.len() % 4 != 0 {
        record.push(0);
    }
    let diff = record.len() as isize - (old_end - offset) as isize;
    t.get_mut(b"glyf").unwrap().splice(offset..old_end, record);
    let loca = t.get_mut(b"loca").unwrap();
    for chunk in loca.as_chunks_mut::<4>().0.iter_mut().skip(4) {
        let n = u32::from_be_bytes(*chunk);
        chunk.copy_from_slice(&((n as isize + diff) as u32).to_be_bytes());
    }
    let bytes = pack(t);
    let doc = import(&bytes, 0).unwrap();
    assert!(doc.components.has_nested());
    assert!(doc.schema_version >= 4);
    doc.validate().unwrap();
}

#[test]
fn missing_source_gets_one_shared_helper_and_roundtrips_do_not_grow() {
    let (mut e, id) = editor();
    e.place_component(e.revision(), id, 2, [1., 0., 0., 1., 10., 0.])
        .unwrap();
    let mut source = e.document().glyphs[&1].clone();
    source.contours.clear();
    e.replace_glyph(e.revision(), source).unwrap();
    let a = export(e.document()).unwrap();
    assert_eq!(a.helpers, 1);
    let loaded = import(&a.bytes, 0).unwrap();
    let b = export(&loaded).unwrap();
    assert_eq!(b.helpers, 0);
    assert_eq!(b.expected.glyphs.len(), a.expected.glyphs.len());
    assert_eq!(
        decode(&a.bytes, 0).unwrap()[&2].rings,
        decode(&b.bytes, 0).unwrap()[&2].rings
    );
}

#[test]
fn nested_export_preserves_shared_descendants_after_reimport() {
    let (mut e, leaf) = editor();
    e.place_component(e.revision(), leaf, 2, [1., 0., 0., 1., 10., 0.])
        .unwrap();
    let parent = e
        .capture_component(
            e.revision(),
            2,
            &[0].into(),
            Info {
                name: "Parent".into(),
                tags: "".into(),
                category: "radical".into(),
                source: "".into(),
                license: "".into(),
                favorite: false,
            },
        )
        .unwrap();
    e.place_component(e.revision(), parent, 3, [1., 0., 0., 1., 50., 0.])
        .unwrap();
    let output = export(e.document()).unwrap();
    let imported = import(&output.bytes, 0).unwrap();
    assert!(imported.components.has_nested());
    let leaf = *imported
        .components
        .definitions
        .iter()
        .find(|(_, d)| d.source_glyph == 1)
        .unwrap()
        .0;
    let mut e = Editor::new(imported).unwrap();
    let before = e.document().clone();
    let mut contours = before.components.definitions[&leaf]
        .contours
        .as_ref()
        .clone();
    geometry::move_point(
        &mut contours[0],
        geometry::EditPoint::Control1(0),
        [150., 350.],
    )
    .unwrap();
    e.edit_component_contours(e.revision(), leaf, contours)
        .unwrap();
    assert_ne!(before.glyphs[&2], e.document().glyphs[&2]);
    assert_ne!(before.glyphs[&3], e.document().glyphs[&3]);
    e.undo(e.revision()).unwrap();
    assert_eq!(e.document(), &before);
}

#[test]
fn cached_components_cannot_bypass_depth_limits() {
    let mut doc = FontDocument::new("Depth", "ABCDEFGHIJKLMNOPQRST").unwrap();
    let (e, _) = editor();
    doc.glyphs.get_mut(&1).unwrap().contours = e.document().glyphs[&1].contours.clone();
    for c in &mut doc.glyphs.get_mut(&1).unwrap().contours {
        c.identity = Default::default();
    }
    let base = font_io::export(&doc).unwrap();
    let mut t = tables(&base);
    let a = at(&t, 1);
    let b = at(&t, 2);
    let simple = t[b"glyf"][a..b].to_vec();
    let count = doc.glyphs.len();
    let mut glyf = vec![];
    let mut loca = vec![];
    for id in 0..count {
        loca.extend((glyf.len() as u32).to_be_bytes());
        if id == 1 {
            glyf.extend(&simple);
        } else if id > 1 {
            glyf.extend((-1i16).to_be_bytes());
            glyf.extend([0; 8]);
            glyf.extend(0x1003u16.to_be_bytes());
            glyf.extend(((id - 1) as u16).to_be_bytes());
            glyf.extend([0; 4]);
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
    }
    loca.extend((glyf.len() as u32).to_be_bytes());
    t.insert(*b"glyf", glyf);
    t.insert(*b"loca", loca);
    assert!(decode(&pack(t), 0).is_err());
}

#[test]
fn component_metrics_are_resolved_without_executing_hints() {
    let (mut e, id) = editor();
    let mut source = e.document().glyphs[&1].clone();
    source.advance = 777.;
    e.replace_glyph(e.revision(), source).unwrap();
    e.place_component(e.revision(), id, 2, [1., 0., 0., 1., 10., 0.])
        .unwrap();
    let output = export(e.document()).unwrap();
    let mut t = tables(&output.bytes);
    let offset = at(&t, 2);
    let g = t.get_mut(b"glyf").unwrap();
    let f = u16::from_be_bytes([g[offset + 10], g[offset + 11]]) | 0x200;
    g[offset + 10..offset + 12].copy_from_slice(&f.to_be_bytes());
    let imported = import(&pack(t), 0).unwrap();
    assert_eq!(imported.glyphs[&2].advance, 777.);
}
