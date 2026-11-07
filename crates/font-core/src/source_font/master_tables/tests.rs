use super::*;
use crate::{snippets::Info, source_font::master_rebase, Contour, Editor, FontDocument, Segment};
fn info(name: &str) -> Info {
    Info {
        name: name.into(),
        tags: "".into(),
        category: "radical".into(),
        source: "".into(),
        license: "MIT".into(),
        favorite: false,
    }
}
fn fixture() -> Vec<u8> {
    let mut doc = FontDocument::new("Base update", "ABC").unwrap();
    doc.glyphs.get_mut(&1).unwrap().contours = vec![Contour {
        start: [10., 0.],
        segments: vec![
            Segment::Line { end: [300., 0.] },
            Segment::Line { end: [300., 300.] },
            Segment::Line { end: [10., 300.] },
            Segment::Line { end: [10., 0.] },
        ],
        closed: true,
        smooth: Default::default(),
        identity: Default::default(),
    }];
    let mut editor = Editor::new(doc).unwrap();
    let leaf = editor
        .capture_component(editor.revision(), 1, &[0].into(), info("Leaf"))
        .unwrap();
    editor
        .place_component(editor.revision(), leaf, 2, [0.5, 0., 0., 0.5, 40., 20.])
        .unwrap();
    let parent = editor
        .capture_component(editor.revision(), 2, &[0].into(), info("Parent"))
        .unwrap();
    editor
        .place_component(editor.revision(), parent, 3, [1., 0., 0., 1., 50., 0.])
        .unwrap();
    let bytes = crate::truetype_components::export(editor.document())
        .unwrap()
        .bytes;
    let raw = ttf_parser::RawFace::parse(&bytes, 0).unwrap();
    let mut tables: Tables = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let count = usize::from(
        ttf_parser::Face::parse(&bytes, 0)
            .unwrap()
            .number_of_glyphs(),
    );
    let mut vhea = vec![0; 36];
    vhea[..4].copy_from_slice(&0x00011000u32.to_be_bytes());
    vhea[10..12].copy_from_slice(&1000u16.to_be_bytes());
    vhea[20..22].copy_from_slice(&1u16.to_be_bytes());
    vhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    let mut vmtx = 1000u16.to_be_bytes().to_vec();
    for _ in 0..count {
        vmtx.extend(100i16.to_be_bytes());
    }
    tables.insert(*b"vhea", vhea);
    tables.insert(*b"vmtx", vmtx);
    tables.insert(*b"DSIG", vec![0, 0, 0, 1, 0, 0, 0, 0]);
    let locations = offsets(&tables, count).unwrap();
    let mut records: Vec<_> = locations
        .windows(2)
        .map(|w| tables[b"glyf"][w[0]..w[1]].to_vec())
        .collect();
    let record = &mut records[2];
    let (_, flags, end) = component_records(record).unwrap();
    let at = flags[0];
    let value = word(record, at).unwrap() | 0x100;
    record[at..at + 2].copy_from_slice(&value.to_be_bytes());
    record.truncate(end);
    record.extend([0, 2, 0xb0, 0]);
    pack(&mut tables, &records).unwrap();
    crate::color_fonts::assemble(tables, false).unwrap()
}
#[test]
fn nested_ancestors_base_metrics_and_header_bounds_follow_the_changed_leaf() {
    let source = fixture();
    let old = ttf_parser::Face::parse(&source, 0).unwrap();
    let glyph = master_glyf::read(&source, 0, 1).unwrap().unwrap();
    let points: Vec<_> = glyph
        .points
        .iter()
        .map(|p| [p[0] + 20., p[1] + 30.])
        .collect();
    let plan = master_rebase::rebase(&glyph, &[], &points).unwrap();
    let output = apply(&source, &BTreeMap::from([(1, plan)])).unwrap();
    let new = ttf_parser::Face::parse(&output, 0).unwrap();
    assert_eq!(new.number_of_glyphs(), old.number_of_glyphs());
    assert!(new
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"DSIG"))
        .is_none());
    assert_eq!(
        word(
            new.raw_face()
                .table(ttf_parser::Tag::from_bytes(b"head"))
                .unwrap(),
            50
        )
        .unwrap(),
        1
    );
    for tag in [b"hhea", b"vhea"] {
        assert_eq!(
            word(
                new.raw_face()
                    .table(ttf_parser::Tag::from_bytes(tag))
                    .unwrap(),
                34
            )
            .unwrap(),
            new.number_of_glyphs()
        );
    }
    let shapes = crate::truetype_components::decode(&output, 0).unwrap();
    let library = freetype::Library::init().unwrap();
    let native = library.new_memory_face(output.clone(), 0).unwrap();
    let mut union = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
    for id in 0..new.number_of_glyphs() {
        let gid = ttf_parser::GlyphId(id);
        assert_eq!(old.glyph_hor_advance(gid), new.glyph_hor_advance(gid));
        assert_eq!(old.glyph_ver_advance(gid), new.glyph_ver_advance(gid));
        if let (Some(before), Some(after)) =
            (old.glyph_bounding_box(gid), new.glyph_bounding_box(gid))
        {
            assert_eq!(
                new.glyph_hor_side_bearing(gid).unwrap(),
                old.glyph_hor_side_bearing(gid).unwrap() + after.x_min - before.x_min
            );
            assert_eq!(
                new.glyph_ver_side_bearing(gid).unwrap(),
                old.glyph_ver_side_bearing(gid).unwrap() + before.y_max - after.y_max
            );
            union[0] = union[0].min(after.x_min);
            union[1] = union[1].min(after.y_min);
            union[2] = union[2].max(after.x_max);
            union[3] = union[3].max(after.y_max);
            let points: Vec<_> = shapes[&id].rings.iter().flatten().map(|p| p.0).collect();
            let bbox = [
                points.iter().map(|p| p[0] as i16).min().unwrap(),
                points.iter().map(|p| p[1] as i16).min().unwrap(),
                points.iter().map(|p| p[0] as i16).max().unwrap(),
                points.iter().map(|p| p[1] as i16).max().unwrap(),
            ];
            assert_eq!(bbox, [after.x_min, after.y_min, after.x_max, after.y_max]);
            native
                .load_glyph(
                    u32::from(id),
                    freetype::face::LoadFlag::NO_SCALE
                        | freetype::face::LoadFlag::NO_HINTING
                        | freetype::face::LoadFlag::NO_BITMAP,
                )
                .unwrap();
            let outline = native.glyph().outline().unwrap();
            let origin = i64::from(after.x_min - new.glyph_hor_side_bearing(gid).unwrap());
            let actual: Vec<_> = outline
                .points()
                .iter()
                .map(|p| [p.x + origin, p.y])
                .collect();
            assert_eq!(
                actual,
                points
                    .iter()
                    .map(|p| [p[0] as i64, p[1] as i64])
                    .collect::<Vec<_>>()
            );
        }
    }
    let global = new.global_bounding_box();
    assert_eq!(
        union,
        [global.x_min, global.y_min, global.x_max, global.y_max]
    );
    assert_eq!(
        new.glyph_bounding_box(ttf_parser::GlyphId(1))
            .unwrap()
            .x_min,
        30
    );
    assert_eq!(
        new.glyph_bounding_box(ttf_parser::GlyphId(2))
            .unwrap()
            .x_min,
        55
    );
    let raw = new.raw_face();
    let mut tables = Tables::new();
    for record in raw.table_records {
        tables.insert(
            record.tag.to_bytes(),
            raw.table(record.tag).unwrap().to_vec(),
        );
    }
    let locations = offsets(&tables, usize::from(new.number_of_glyphs())).unwrap();
    let record = &tables[b"glyf"][locations[2]..locations[3]];
    assert!(component_records(record)
        .unwrap()
        .1
        .iter()
        .all(|&at| word(record, at).unwrap() & 0x100 == 0));
}
#[test]
fn invalid_replacements_are_rejected_without_changing_the_source() {
    let source = fixture();
    let original = source.clone();
    assert_eq!(apply(&source, &BTreeMap::new()).unwrap(), source);
    let glyph = master_glyf::read(&source, 0, 1).unwrap().unwrap();
    let mut plan = master_rebase::rebase(&glyph, &[], &glyph.points).unwrap();
    plan.base.bytes[2] ^= 1;
    assert!(apply(&source, &BTreeMap::from([(1, plan)])).is_err());
    let plan = master_rebase::rebase(&glyph, &[], &glyph.points).unwrap();
    assert!(apply(&source, &BTreeMap::from([(65535, plan)])).is_err());
    assert_eq!(source, original);
}

#[test]
fn two_rebased_variable_glyphs_preserve_their_design_space_and_shared_advances() {
    use crate::source_font::master_sample::Sampler;
    use std::sync::Arc;
    let source = include_bytes!("../../../tests/data/variations/gvar.ttf");
    let raw = ttf_parser::RawFace::parse(source, 0).unwrap();
    let mut plans = BTreeMap::new();
    for (id, dx, dy) in [(1, 10., 20.), (3, -5., 15.)] {
        let glyph = master_glyf::read(source, 0, id).unwrap().unwrap();
        let tuples = master_gvar::read(
            raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap(),
            &["wght".into(), "wdth".into()],
            4,
            id,
            8,
        )
        .unwrap();
        let points: Vec<_> = glyph
            .points
            .iter()
            .map(|p| [p[0] + dx, p[1] + dy])
            .collect();
        plans.insert(id, master_rebase::rebase(&glyph, &tuples, &points).unwrap());
    }
    let output = apply(source, &plans).unwrap();
    let mut before = Sampler::new(Arc::new(source.to_vec()), 0).unwrap();
    let mut after = Sampler::new(Arc::new(output), 0).unwrap();
    for wght in [-1., 0., 0.5, 1.] {
        for wdth in [-1., 0., 1.] {
            let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            for (id, dx, dy) in [(1, 10., 20.), (3, -5., 15.)] {
                let old = before.sample_normalized(id, &location).unwrap().shape;
                let new = after.sample_normalized(id, &location).unwrap().shape;
                assert_eq!(old.advance, new.advance);
                assert_eq!(
                    new.contours[0].start,
                    [old.contours[0].start[0] + dx, old.contours[0].start[1] + dy]
                );
                for (a, b) in old.contours[0]
                    .segments
                    .iter()
                    .zip(&new.contours[0].segments)
                {
                    match (a, b) {
                        (Segment::Line { end: a }, Segment::Line { end: b }) => {
                            assert_eq!(*b, [a[0] + dx, a[1] + dy])
                        }
                        _ => panic!("line fixture"),
                    }
                }
            }
            assert_eq!(
                before.sample_normalized(2, &location).unwrap().shape,
                after.sample_normalized(2, &location).unwrap().shape
            );
        }
    }
}

#[test]
fn optional_sidebearing_caches_are_cleared_without_modifying_metric_variation_payloads() {
    let raw =
        ttf_parser::RawFace::parse(include_bytes!("../../../tests/data/variations/gvar.ttf"), 0)
            .unwrap();
    let mut hvar = raw
        .table(ttf_parser::Tag::from_bytes(b"HVAR"))
        .unwrap()
        .to_vec();
    let mapping = hvar[8..12].to_vec();
    hvar[12..16].copy_from_slice(&mapping);
    hvar[16..20].copy_from_slice(&mapping);
    let mut vvar = hvar[..20].to_vec();
    for at in [4, 8, 12, 16] {
        let offset = u32::from_be_bytes(vvar[at..at + 4].try_into().unwrap());
        vvar[at..at + 4].copy_from_slice(&(offset + 4).to_be_bytes());
    }
    let origin_mapping = u32::from_be_bytes(mapping.try_into().unwrap()) + 4;
    vvar.extend(origin_mapping.to_be_bytes());
    vvar.extend(&hvar[20..]);
    let mut tables = BTreeMap::from([(*b"HVAR", hvar.clone()), (*b"VVAR", vvar.clone())]);
    metrics::clear_sidebearing_maps(&mut tables).unwrap();
    for (tag, original) in [(b"HVAR", hvar), (b"VVAR", vvar)] {
        let changed = &tables[tag];
        assert_eq!(&changed[..12], &original[..12]);
        assert_eq!(&changed[12..20], &[0; 8]);
        assert_eq!(&changed[20..], &original[20..]);
    }
    tables.insert(*b"HVAR", vec![0; 19]);
    assert!(metrics::clear_sidebearing_maps(&mut tables).is_err());
}
