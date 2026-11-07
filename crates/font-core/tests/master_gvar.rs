use font_core::source_font::{
    master_glyf,
    master_gvar::{self, Tuple},
    master_sample::Sampler,
    master_tuple,
    masters::Location,
};
use std::sync::Arc;

#[test]
fn source_sparse_gvar_matches_freetype_across_axes_and_avar() {
    let bytes = include_bytes!("data/variations/gvar.ttf");
    let raw = ttf_parser::RawFace::parse(bytes, 0).unwrap();
    let table = raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    let tuples = master_gvar::read(table, &axes, 4, 1, 8).unwrap();
    let glyph = master_glyf::read(bytes, 0, 1).unwrap().unwrap();
    let original_ids = glyph
        .contours(&tuples, &[("wght".into(), 0.), ("wdth".into(), 0.)].into())
        .unwrap()[0]
        .identity
        .clone();
    assert_eq!(tuples.len(), 4);
    assert!(tuples.iter().any(|t| t.points.is_some()));
    let points = [
        [20., 0.],
        [400., 0.],
        [400., 700.],
        [20., 700.],
        [0., 0.],
        [500., 0.],
        [0., 0.],
        [0., 0.],
    ];
    let mut sampler = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
    for wght in [-1., -0.5, 0., 0.25, 0.75, 1.] {
        for wdth in [-1., 0., 0.5, 1.] {
            let location: Location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            let out = master_gvar::evaluate(&tuples, &points, &[3], &location).unwrap();
            let native = sampler.sample_normalized(1, &location).unwrap();
            let contours = glyph.contours(&tuples, &location).unwrap();
            assert_eq!(contours[0].identity, original_ids);
            assert_eq!(contours[0].start, native.shape.contours[0].start);
            assert_eq!(contours[0].segments, native.shape.contours[0].segments);
            assert_eq!(glyph.topology.physical_points(&contours).unwrap(), out[..4]);
            assert!((native.shape.advance - (out[5][0] - out[4][0])).abs() < 0.02);
            let bounds = native.shape.contours[0]
                .segments
                .iter()
                .flat_map(|s| match s {
                    font_core::Segment::Line { end } => vec![*end],
                    _ => panic!("fixture has only lines"),
                })
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), p| {
                    (min.min(p[0]), max.max(p[0]))
                });
            assert!((bounds.0 - out[0][0]).abs() < 0.02);
            assert!((bounds.1 - out[1][0]).abs() < 0.02);
        }
    }
    let space = master_gvar::read(table, &axes, 4, 2, 4).unwrap();
    let space_points = [[0., 0.], [500., 0.], [0., 0.], [0., 0.]];
    let space_location = [("wght".into(), 1.), ("wdth".into(), 1.)].into();
    let out = master_gvar::evaluate(&space, &space_points, &[], &space_location).unwrap();
    assert!(
        (out[1][0]
            - out[0][0]
            - sampler
                .sample_normalized(2, &space_location)
                .unwrap()
                .shape
                .advance)
            .abs()
            < 0.02
    );
    for len in 0..table.len() {
        assert!(master_gvar::read(&table[..len], &axes, 4, 1, 8).is_err());
    }
}

#[test]
fn selected_simple_glyph_reader_keeps_empty_outlines_and_rejects_truncated_sources() {
    let bytes = include_bytes!("data/variations/gvar.ttf");
    for index in [0, 2] {
        let glyph = master_glyf::read(bytes, 0, index).unwrap().unwrap();
        assert!(glyph.points.is_empty());
        assert!(glyph.contours(&[], &Location::new()).unwrap().is_empty());
    }
    assert!(master_glyf::read(bytes, 0, 4).is_err());
    assert!(master_glyf::read(bytes, 1, 1).is_err());
    assert!(master_glyf::read(&bytes[..50], 0, 1).is_err());
}

fn word(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_be_bytes());
}
fn table(private: bool, long: bool) -> Vec<u8> {
    let packed = master_tuple::write_points(Some(&[0, 2, 5]), 8).unwrap();
    let mut payload = if private { packed.clone() } else { vec![] };
    payload.extend(master_tuple::write_deltas(&[0, 100, 200]).unwrap());
    payload.extend(master_tuple::write_deltas(&[0, 20, 0]).unwrap());
    let mut glyph = vec![];
    word(&mut glyph, if private { 1 } else { 0x8001 });
    word(&mut glyph, 14);
    word(&mut glyph, payload.len() as u16);
    word(&mut glyph, 0x4000 | if private { 0x2000 } else { 0 });
    // Shared peak .5, intermediate bounds .25 .. 1.
    word(&mut glyph, 4096);
    word(&mut glyph, 16384);
    // Header is twelve bytes. Two bytes of legal padding before serialized data.
    word(&mut glyph, 0);
    if !private {
        glyph.extend(packed);
    }
    glyph.extend(payload);
    if glyph.len() % 2 != 0 {
        glyph.push(0);
    }
    let shared_at = if long { 28 } else { 24 };
    let data_at = shared_at + 2;
    let mut out = vec![];
    for v in [1, 0, 1, 1] {
        word(&mut out, v);
    }
    out.extend((shared_at as u32).to_be_bytes());
    word(&mut out, 1);
    word(&mut out, u16::from(long));
    out.extend((data_at as u32).to_be_bytes());
    if long {
        out.extend(0u32.to_be_bytes());
        out.extend((glyph.len() as u32).to_be_bytes());
    } else {
        word(&mut out, 0);
        word(&mut out, (glyph.len() / 2) as u16);
    }
    word(&mut out, 8192);
    out.extend(glyph);
    out
}

#[test]
fn shared_peak_private_and_shared_points_intermediate_and_offset_formats() {
    for private in [false, true] {
        for long in [false, true] {
            let tuples =
                master_gvar::read(&table(private, long), &["wght".into()], 1, 0, 8).unwrap();
            assert_eq!(tuples[0].points, Some(vec![0, 2, 5]));
            assert_eq!(tuples[0].deltas, [[0, 0], [100, 20], [200, 0]]);
            for (coordinate, expected) in [
                (0., 0.),
                (0.25, 0.),
                (0.375, 0.5),
                (0.5, 1.),
                (0.75, 0.5),
                (1., 0.),
            ] {
                assert_eq!(
                    tuples[0]
                        .support
                        .weight(&[("wght".into(), coordinate)].into())
                        .unwrap(),
                    expected
                );
            }
        }
    }
}

#[test]
fn malformed_offsets_headers_and_public_tuple_shapes_are_rejected() {
    let good = table(true, true);
    for (index, value) in [
        (0, 2),
        (5, 2),
        (15, 2),
        (20, 255),
        (27, 255),
        (31, 0x70),
        (35, 0x10),
    ] {
        let mut bad = good.clone();
        bad[index] = value;
        assert!(
            master_gvar::read(&bad, &["wght".into()], 1, 0, 8).is_err(),
            "index {index}"
        );
    }
    assert!(master_gvar::read(&good, &["wght".into()], 1, 1, 8).is_err());
    assert!(master_gvar::read(&good, &["wght".into()], 1, 0, 3).is_err());
    let bad = Tuple {
        support: Default::default(),
        points: Some(vec![99]),
        deltas: vec![[0, 0]],
    };
    assert!(bad.expand(&[[0., 0.]; 8], &[3]).is_err());
    assert!(bad
        .support
        .weight(&[("wght".into(), f32::NAN)].into())
        .is_err());
}

#[test]
fn replacing_one_glyph_roundtrips_sparse_source_and_keeps_other_glyphs() {
    let raw = ttf_parser::RawFace::parse(include_bytes!("data/variations/gvar.ttf"), 0).unwrap();
    let source = raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    let mut tuples = master_gvar::read(source, &axes, 4, 1, 8).unwrap();
    let baseline = tuples.clone();
    let roundtrip = master_gvar::replace(source, &axes, 4, 1, 8, &tuples).unwrap();
    assert_eq!(
        master_gvar::read(&roundtrip, &axes, 4, 1, 8).unwrap(),
        baseline
    );
    tuples[0].deltas[0][1] = 33;
    let changed = master_gvar::replace(source, &axes, 4, 1, 8, &tuples).unwrap();
    assert_eq!(master_gvar::read(&changed, &axes, 4, 1, 8).unwrap(), tuples);
    for (glyph, points) in [(0, 4), (2, 4), (3, 8)] {
        assert_eq!(
            master_gvar::read(&changed, &axes, 4, glyph, points).unwrap(),
            master_gvar::read(source, &axes, 4, glyph, points).unwrap()
        );
    }
    let cleared = master_gvar::replace(source, &axes, 4, 1, 8, &[]).unwrap();
    assert!(master_gvar::read(&cleared, &axes, 4, 1, 8)
        .unwrap()
        .is_empty());
    tuples[0].support.axes.get_mut("wght").unwrap()[0] = -0.123456;
    assert!(master_gvar::encode_glyph(&tuples, &axes, 8).is_err());
}

#[test]
fn batch_replacement_matches_individual_updates_without_changing_unselected_glyphs() {
    let raw = ttf_parser::RawFace::parse(include_bytes!("data/variations/gvar.ttf"), 0).unwrap();
    let table = raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    let mut a = master_gvar::read(table, &axes, 4, 1, 8).unwrap();
    let mut b = master_gvar::read(table, &axes, 4, 3, 8).unwrap();
    a[0].deltas[0][1] += 30;
    b[0].deltas[0][1] -= 20;
    let replacements = [(1, (8, a.as_slice())), (3, (8, b.as_slice()))].into();
    let batch = master_gvar::replace_many(table, &axes, 4, &replacements).unwrap();
    let sequential = master_gvar::replace(
        &master_gvar::replace(table, &axes, 4, 1, 8, &a).unwrap(),
        &axes,
        4,
        3,
        8,
        &b,
    )
    .unwrap();
    assert_eq!(batch, sequential);
    assert_eq!(master_gvar::read(&batch, &axes, 4, 1, 8).unwrap(), a);
    assert_eq!(master_gvar::read(&batch, &axes, 4, 3, 8).unwrap(), b);
}
