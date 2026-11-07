use font_core::source_font::{
    master_sample::Sampler,
    master_store::{self, Builder, Store},
    masters::Support,
};
use std::sync::Arc;
fn offset(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn synthetic(long: bool) -> Vec<u8> {
    let mut bytes = 1u16.to_be_bytes().to_vec();
    bytes.extend(12u32.to_be_bytes());
    bytes.extend(1u16.to_be_bytes());
    bytes.extend(28u32.to_be_bytes());
    for v in [1, 2, 0, 16384, 16384, -16384, -16384, 0] {
        bytes.extend((v as i16).to_be_bytes());
    }
    for v in [2u16, if long { 0x8001 } else { 1 }, 2, 0, 1] {
        bytes.extend(v.to_be_bytes());
    }
    if long {
        for (a, b) in [(-70000i32, -300i16), (70000, 300)] {
            bytes.extend(a.to_be_bytes());
            bytes.extend(b.to_be_bytes());
        }
    } else {
        for (a, b) in [(-200i16, 127i8), (300, -128)] {
            bytes.extend(a.to_be_bytes());
            bytes.push(b as u8);
        }
    }
    bytes
}
#[test]
fn real_metric_store_matches_freetype_for_all_glyphs_and_two_axes() {
    let font = include_bytes!("data/variations/gvar.ttf");
    let raw = ttf_parser::RawFace::parse(font, 0).unwrap();
    let hvar = raw.table(ttf_parser::Tag::from_bytes(b"HVAR")).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    let store = Store::parse(&hvar[offset(hvar, 4)..], &axes).unwrap();
    let map = master_store::read_map(&hvar[offset(hvar, 8)..], 4).unwrap();
    let mut sampler = Sampler::new(Arc::new(font.to_vec()), 0).unwrap();
    for wght in [-1., -0.5, 0., 0.25, 0.75, 1.] {
        for wdth in [-1., 0., 0.5, 1.] {
            let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            for (glyph, &index) in map.iter().enumerate() {
                let expected = 500. + store.delta(index, &location).unwrap();
                let actual = sampler
                    .sample_normalized(glyph as u32, &location)
                    .unwrap()
                    .shape
                    .advance;
                assert!((expected - actual).abs() < 0.02);
            }
        }
    }
}
#[test]
fn signed_short_byte_and_long_words_roundtrip_with_appended_corrections() {
    for long in [false, true] {
        let bytes = synthetic(long);
        let axes = ["wght".into()];
        let store = Store::parse(&bytes, &axes).unwrap();
        assert_eq!(
            store.row((0, 0)).unwrap(),
            if long {
                vec![(0, -70000), (1, -300)]
            } else {
                vec![(0, -200), (1, 127)]
            }
        );
        let old = store.row((0, 0)).unwrap();
        let mut builder = Builder::new(store);
        let support = Support {
            axes: [("wght".into(), [0., 1., 1.])].into(),
        };
        let (index, errors) = builder.add((0, 1), &[(support, 70000.4)]).unwrap();
        assert_eq!(errors.len(), 1);
        assert!((errors[0].1 - 0.4).abs() < 1e-9);
        let output = builder.finish().unwrap();
        let store = Store::parse(&output, &axes).unwrap();
        assert_eq!(store.row((0, 0)).unwrap(), old);
        assert_eq!(
            store.delta(index, &[("wght".into(), 1.)].into()).unwrap(),
            if long { 140000. } else { 70300. }
        );
        assert_eq!(
            store.delta(index, &[("wght".into(), -1.)].into()).unwrap(),
            if long { 300. } else { -128. }
        );
        assert!(store
            .delta(index, &[("wght".into(), f32::NAN)].into())
            .is_err());
        assert!(store.row((0, 2)).is_err());
        assert!(store.row((65535, 65535)).unwrap().is_empty());
        for len in 0..bytes.len() {
            assert!(Store::parse(&bytes[..len], &axes).is_err());
        }
    }
}
#[test]
fn index_map_formats_packing_repeated_last_entry_and_sentinel_are_preserved() {
    let indices = [(0, 0), (1, 60000), (65535, 65535)];
    assert_eq!(
        master_store::read_map(&master_store::write_map(&indices).unwrap(), 3).unwrap(),
        indices
    );
    let map = [1, 0x11, 0, 0, 0, 2, 0, 14, 0, 23];
    assert_eq!(
        master_store::read_map(&map, 5).unwrap(),
        [(3, 2), (5, 3), (5, 3), (5, 3), (5, 3)]
    );
    for bytes in [
        &[][..],
        &[0, 0, 0, 0],
        &[0, 0xc0, 0, 1, 0],
        &[0, 0x0f, 0, 1, 0],
        &map[..9],
    ] {
        assert!(master_store::read_map(bytes, 3).is_err());
    }
}
