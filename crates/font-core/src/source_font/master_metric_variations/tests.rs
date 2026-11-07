use super::*;
use crate::source_font::master_sample::Sampler;
use std::sync::Arc;
const FONT: &[u8] = include_bytes!("../../../tests/data/variations/gvar.ttf");
#[test]
fn metric_corrections_preserve_original_advances_and_outlines_when_reopened() {
    for font in [
        FONT,
        include_bytes!("../../../tests/data/variations/cff2.otf").as_slice(),
    ] {
        check_font(font);
    }
}
fn check_font(font: &[u8]) {
    let raw = ttf_parser::RawFace::parse(font, 0).unwrap();
    let source = raw.table(ttf_parser::Tag::from_bytes(b"HVAR")).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    let changes = [
        (
            1,
            vec![(Support::default(), 20.25), (support.clone(), 40.4)],
        ),
        (3, vec![(support.clone(), -30.)]),
    ]
    .into();
    let edited = edit(source, &axes, 4, &changes, false).unwrap();
    let original_store = Store::parse(&source[offset(source, 4).unwrap()..], &axes).unwrap();
    let original_map = master_store::read_map(&source[offset(source, 8).unwrap()..], 4).unwrap();
    let new_store =
        Store::parse(&edited.table[offset(&edited.table, 4).unwrap()..], &axes).unwrap();
    let new_map =
        master_store::read_map(&edited.table[offset(&edited.table, 8).unwrap()..], 4).unwrap();
    assert_eq!(edited.base[&1], 20);
    let applied = apply_font(font, &changes, &BTreeMap::new()).unwrap();
    let face = ttf_parser::Face::parse(&applied.bytes, 0).unwrap();
    let hhea = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"hhea"))
        .unwrap();
    assert_eq!(u16::from_be_bytes(hhea[10..12].try_into().unwrap()), 520);
    assert_eq!(u16::from_be_bytes(hhea[34..36].try_into().unwrap()), 4);
    assert_eq!(face.glyph_hor_advance(ttf_parser::GlyphId(1)).unwrap(), 520);
    assert_eq!(
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(b"HVAR"))
            .unwrap(),
        edited.table
    );
    let output = applied.bytes.clone();
    let mut before = Sampler::new(Arc::new(font.to_vec()), 0).unwrap();
    let mut after = Sampler::new(Arc::new(output), 0).unwrap();
    for wght in [-1., 0., 0.25, 0.5, 1.] {
        for wdth in [-1., 0., 1.] {
            let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            for glyph in 0..4 {
                let old = before.sample_normalized(glyph, &location).unwrap().shape;
                let new = after.sample_normalized(glyph, &location).unwrap().shape;
                assert_eq!(old.contours, new.contours);
                let weight = support.weight(&location).unwrap();
                let delta = match glyph {
                    1 => 20.25 + 40.4 * weight,
                    3 => -30. * weight,
                    _ => 0.,
                };
                let index = glyph as usize;
                let stored_delta = new_store.delta(new_map[index], &location).unwrap()
                    - original_store
                        .delta(original_map[index], &location)
                        .unwrap()
                    + f64::from(edited.base.get(&(glyph as u16)).copied().unwrap_or(0));
                assert!(
                    (stored_delta - delta).abs()
                        <= edited.error_bound(glyph as u16, &location).unwrap() + 1e-9
                );
                assert_eq!(
                    applied.error_bound(glyph as u16, &location, false).unwrap(),
                    edited.error_bound(glyph as u16, &location).unwrap()
                );
                assert!(
                    (new.advance - old.advance - delta).abs()
                        // FreeType rounds the combined HVAR result to a design unit before scaling.
                        <= edited.error_bound(glyph as u16, &location).unwrap() + 0.52,
                    "glyph {glyph}, {location:?}: old {}, new {}, expected delta {delta}, bound {}",
                    old.advance,
                    new.advance,
                    edited.error_bound(glyph as u16, &location).unwrap()
                );
            }
        }
    }
}
#[test]
fn invalid_metrics_are_rejected_and_vertical_origin_mapping_is_preserved() {
    let raw = ttf_parser::RawFace::parse(FONT, 0).unwrap();
    let hvar = raw.table(ttf_parser::Tag::from_bytes(b"HVAR")).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    assert_eq!(
        edit(hvar, &axes, 4, &BTreeMap::new(), false).unwrap().table,
        hvar
    );
    assert!(edit(hvar, &axes, 4, &[(4, vec![])].into(), false).is_err());
    assert!(edit(
        hvar,
        &axes,
        4,
        &[(1, vec![(Support::default(), f64::NAN)])].into(),
        false
    )
    .is_err());
    let mut vvar = hvar[..20].to_vec();
    for at in [4, 8, 12, 16] {
        let original = offset(hvar, at).unwrap();
        vvar[at..at + 4]
            .copy_from_slice(&(if original == 0 { 0 } else { original + 4 } as u32).to_be_bytes());
    }
    let origin = offset(hvar, 8).unwrap() + 4;
    vvar.extend((origin as u32).to_be_bytes());
    vvar.extend(&hvar[20..]);
    let edited = edit(
        &vvar,
        &axes,
        4,
        &[(
            1,
            vec![(
                Support {
                    axes: [("wght".into(), [0., 1., 1.])].into(),
                },
                50.,
            )],
        )]
        .into(),
        true,
    )
    .unwrap();
    assert_eq!(&edited.table[20..24], &vvar[20..24]);
    assert_eq!(&edited.table[24..vvar.len()], &vvar[24..]);
    let index =
        master_store::read_map(&edited.table[offset(&edited.table, 8).unwrap()..], 4).unwrap()[1];
    let store = Store::parse(&edited.table[offset(&edited.table, 4).unwrap()..], &axes).unwrap();
    assert!(
        store
            .delta(index, &[("wght".into(), 1.), ("wdth".into(), 0.)].into())
            .unwrap()
            > 0.
    );
}

fn vertical_font(font: &[u8]) -> Vec<u8> {
    let raw = ttf_parser::RawFace::parse(font, 0).unwrap();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let hvar = &tables[b"HVAR"];
    let mut vvar = hvar[..20].to_vec();
    for at in [4, 8, 12, 16] {
        let value = offset(hvar, at).unwrap();
        vvar[at..at + 4]
            .copy_from_slice(&((if value == 0 { 0 } else { value + 4 }) as u32).to_be_bytes());
    }
    vvar.extend(0u32.to_be_bytes());
    vvar.extend(&hvar[20..]);
    let mut vhea = vec![0; 36];
    vhea[..4].copy_from_slice(&0x00011000u32.to_be_bytes());
    for (at, value) in [
        (4, 500i16),
        (6, -500),
        (10, 1000),
        (12, 100),
        (14, 200),
        (16, 800),
        (20, 1),
        (34, 1),
    ] {
        vhea[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    let mut vmtx = 1000u16.to_be_bytes().to_vec();
    for _ in 0..4 {
        vmtx.extend(100i16.to_be_bytes());
    }
    tables.insert(*b"VVAR", vvar);
    tables.insert(*b"vhea", vhea);
    tables.insert(*b"vmtx", vmtx);
    tables.insert(*b"DSIG", vec![0, 0, 0, 1, 0, 0, 0, 0]);
    crate::color_fonts::assemble(tables, font.starts_with(b"OTTO")).unwrap()
}
fn remove_table(font: &[u8], tag: &[u8; 4]) -> Vec<u8> {
    let raw = ttf_parser::RawFace::parse(font, 0).unwrap();
    let tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .filter(|r| r.tag.to_bytes() != *tag)
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    crate::color_fonts::assemble(tables, font.starts_with(b"OTTO")).unwrap()
}
#[test]
fn missing_hvar_is_initialized_from_original_gvar_or_constant_cff2_metrics() {
    for original in [
        FONT,
        include_bytes!("../../../tests/data/variations/cff2.otf").as_slice(),
    ] {
        let source = remove_table(original, b"HVAR");
        let support = Support {
            axes: [("wght".into(), [0., 1., 1.])].into(),
        };
        let changes = [(1, vec![(Support::default(), 10.), (support.clone(), 20.)])].into();
        let result = apply_font(&source, &changes, &BTreeMap::new()).unwrap();
        let raw = ttf_parser::RawFace::parse(&result.bytes, 0).unwrap();
        assert!(raw.table(ttf_parser::Tag::from_bytes(b"HVAR")).is_some());
        let mut before = Sampler::new(Arc::new(source), 0).unwrap();
        let mut after = Sampler::new(Arc::new(result.bytes.clone()), 0).unwrap();
        for wght in [-1., -0.5, 0., 0.125, 0.25, 0.375, 0.75, 1.] {
            for wdth in [-1., 0., 0.5, 1.] {
                let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                for glyph in 0..4 {
                    let a = before.sample_normalized(glyph, &location).unwrap().shape;
                    let b = after.sample_normalized(glyph, &location).unwrap().shape;
                    assert_eq!(a.contours, b.contours);
                    let delta = if glyph == 1 {
                        10. + 20. * support.weight(&location).unwrap()
                    } else {
                        0.
                    };
                    assert!(
                        (b.advance - a.advance - delta).abs() <= 0.52,
                        "glyph {glyph}, {location:?}: old {}, new {}, delta {delta}",
                        a.advance,
                        b.advance
                    );
                }
            }
        }
    }
}
#[test]
fn vertical_metric_font_updates_match_freetype_and_keep_horizontal_metrics() {
    use freetype::{face::LoadFlag, ffi};
    for original in [
        FONT,
        include_bytes!("../../../tests/data/variations/cff2.otf").as_slice(),
    ] {
        for missing in [false, true] {
            let source = vertical_font(original);
            let source = if missing {
                remove_table(&source, b"VVAR")
            } else {
                source
            };
            let support = Support {
                axes: [("wght".into(), [0., 1., 1.])].into(),
            };
            let changes = [
                (
                    1,
                    vec![(Support::default(), 30.25), (support.clone(), 20.4)],
                ),
                (3, vec![(support.clone(), -11.)]),
            ]
            .into();
            let result = apply_font(&source, &BTreeMap::new(), &changes).unwrap();
            let face = ttf_parser::Face::parse(&result.bytes, 0).unwrap();
            assert_eq!(face.glyph_ver_advance(ttf_parser::GlyphId(1)), Some(1030));
            let vhea = face
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"vhea"))
                .unwrap();
            assert_eq!(u16::from_be_bytes(vhea[10..12].try_into().unwrap()), 1030);
            assert_eq!(u16::from_be_bytes(vhea[34..36].try_into().unwrap()), 4);
            assert!(face
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"DSIG"))
                .is_none());
            let library = freetype::Library::init().unwrap();
            let mut before = library.new_memory_face(source, 0).unwrap();
            let mut after = library.new_memory_face(result.bytes.clone(), 0).unwrap();
            for wght in [-1f32, 0., 0.25, 0.5, 1.] {
                for wdth in [-1f32, 0., 1.] {
                    let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                    let coordinates = [
                        (f64::from(wght) * 65536.) as ffi::FT_Fixed,
                        (f64::from(wdth) * 65536.) as ffi::FT_Fixed,
                    ];
                    for face in [&mut before, &mut after] {
                        // SAFETY: Both worker-local faces are live, and the two normalized coordinates
                        // match these fixtures' fvar axis order. FreeType consumes them synchronously.
                        assert_eq!(
                            unsafe {
                                ffi::FT_Set_Var_Blend_Coordinates(
                                    face.raw_mut(),
                                    2,
                                    coordinates.as_ptr(),
                                )
                            },
                            0
                        );
                    }
                    for glyph in 0..4 {
                        let flags = LoadFlag::NO_SCALE
                            | LoadFlag::NO_HINTING
                            | LoadFlag::NO_AUTOHINT
                            | LoadFlag::NO_BITMAP
                            | LoadFlag::VERTICAL_LAYOUT;
                        before.load_glyph(glyph, flags).unwrap();
                        after.load_glyph(glyph, flags).unwrap();
                        let old = before.glyph().metrics();
                        let new = after.glyph().metrics();
                        assert_eq!(old.horiAdvance, new.horiAdvance);
                        assert_eq!(old.vertBearingY, new.vertBearingY);
                        let weight = support.weight(&location).unwrap();
                        let delta = match glyph {
                            1 => 30.25 + 20.4 * weight,
                            3 => -11. * weight,
                            _ => 0.,
                        };
                        assert!(
                            ((new.vertAdvance - old.vertAdvance) as f64 - delta).abs()
                                <= result.error_bound(glyph as u16, &location, true).unwrap()
                                    + 0.52
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn font_metric_updates_reject_invalid_defaults_without_mutating_the_source() {
    let unchanged = FONT.to_vec();
    for delta in [-1000., 70000.] {
        assert!(apply_font(
            FONT,
            &[(1, vec![(Support::default(), delta)])].into(),
            &BTreeMap::new()
        )
        .is_err());
    }
    assert_eq!(
        apply_font(FONT, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .bytes,
        FONT
    );
    let vertical = apply_font(
        FONT,
        &BTreeMap::new(),
        &[(1, vec![(Support::default(), 10.)])].into(),
    )
    .unwrap();
    assert!(vertical.generated_vertical_metrics);
    assert_eq!(
        ttf_parser::Face::parse(&vertical.bytes, 0)
            .unwrap()
            .glyph_ver_advance(ttf_parser::GlyphId(1)),
        Some(1010)
    );
    assert_eq!(FONT, unchanged);
}

fn inherited_font() -> Vec<u8> {
    inherited_from(FONT)
}
fn inherited_from(font: &[u8]) -> Vec<u8> {
    use crate::source_font::master_gvar;
    let raw = ttf_parser::RawFace::parse(font, 0).unwrap();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let offsets: Vec<_> = tables[b"loca"]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| usize::from(u16::from_be_bytes(*v)) * 2)
        .collect();
    let mut glyf = vec![];
    let mut loca = vec![];
    for glyph in 0..4 {
        loca.extend((glyf.len() as u32).to_be_bytes());
        if glyph == 3 {
            for value in [-1i16, 20, 0, 400, 700, 0x203, 1, 0, 0] {
                glyf.extend(value.to_be_bytes());
            }
        } else {
            glyf.extend(&tables[b"glyf"][offsets[glyph]..offsets[glyph + 1]]);
        }
    }
    loca.extend((glyf.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", loca);
    tables.get_mut(b"head").unwrap()[50..52].copy_from_slice(&1u16.to_be_bytes());
    let maxp = tables.get_mut(b"maxp").unwrap();
    for (at, value) in [(8, 4u16), (10, 1), (28, 1), (30, 1)] {
        maxp[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    let gvar = master_gvar::replace(
        &tables[b"gvar"],
        &["wght".into(), "wdth".into()],
        4,
        3,
        8,
        &[],
    )
    .unwrap();
    tables.insert(*b"gvar", gvar);
    tables.remove(b"HVAR");
    crate::color_fonts::assemble(tables, false).unwrap()
}
#[test]
fn initialization_preserves_unedited_inherited_metrics_and_supports_independent_overrides() {
    let source = inherited_font();
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    let changed = apply_font(
        &source,
        &[(2, vec![(support.clone(), 20.)])].into(),
        &BTreeMap::new(),
    )
    .unwrap();
    let mut before = Sampler::new(Arc::new(source.clone()), 0).unwrap();
    let mut after = Sampler::new(Arc::new(changed.bytes), 0).unwrap();
    for wght in [-1., 0., 0.5, 1.] {
        let location = [("wght".into(), wght), ("wdth".into(), 0.)].into();
        let a = before.sample_normalized(3, &location).unwrap().shape;
        let b = after.sample_normalized(3, &location).unwrap().shape;
        assert_eq!(a, b);
    }
    for glyph in [1, 3] {
        let output = apply_font(
            &source,
            &[(glyph, vec![(support.clone(), 20.)])].into(),
            &BTreeMap::new(),
        )
        .unwrap();
        let raw = ttf_parser::RawFace::parse(&output.bytes, 0).unwrap();
        let loca = raw.table(ttf_parser::Tag::from_bytes(b"loca")).unwrap();
        let glyf = raw.table(ttf_parser::Tag::from_bytes(b"glyf")).unwrap();
        let start = offset(loca, 12).unwrap();
        assert_eq!(
            u16::from_be_bytes(glyf[start + 10..start + 12].try_into().unwrap()) & 0x200,
            0
        );
        let mut after = Sampler::new(Arc::new(output.bytes), 0).unwrap();
        for wght in [-1., 0., 0.5, 1.] {
            for wdth in [-1., 0., 1.] {
                let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                for id in 1..4 {
                    let a = before.sample_normalized(id, &location).unwrap().shape;
                    let b = after.sample_normalized(id, &location).unwrap().shape;
                    assert_eq!(a.contours, b.contours);
                    let delta = if id == u32::from(glyph) {
                        20. * support.weight(&location).unwrap()
                    } else {
                        0.
                    };
                    assert!(
                        (b.advance - a.advance - delta).abs() < 0.02,
                        "changed {glyph}, glyph {id}, {location:?}: {} -> {}",
                        a.advance,
                        b.advance
                    );
                }
            }
        }
    }
}
#[test]
fn moving_horizontal_origins_keep_their_gvar_function_during_metric_edits() {
    use crate::source_font::{master_glyf, master_gvar};
    let raw = ttf_parser::RawFace::parse(FONT, 0).unwrap();
    let axes = ["wght".into(), "wdth".into()];
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let mut tuples = master_gvar::read(&tables[b"gvar"], &axes, 4, 1, 8).unwrap();
    let glyph = master_glyf::read(FONT, 0, 1).unwrap().unwrap();
    tuples[1].deltas = glyph
        .expanded(&tuples[1])
        .unwrap()
        .into_iter()
        .map(|p| p.map(|v| v as i16))
        .collect();
    tuples[1].points = None;
    tuples[1].deltas[4][0] = 7;
    tuples[1].deltas[5][0] += 7;
    let gvar = master_gvar::replace(&tables[b"gvar"], &axes, 4, 1, 8, &tuples).unwrap();
    tables.insert(*b"gvar", gvar);
    tables.remove(b"HVAR");
    let source = crate::color_fonts::assemble(tables, false).unwrap();
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    for target in [1u16, 2, 3] {
        let output = apply_font(
            &source,
            &[(
                target,
                vec![(Support::default(), 5.25), (support.clone(), 40.4)],
            )]
            .into(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(output.retained_gvar_metrics, [true, false]);
        assert!(ttf_parser::RawFace::parse(&output.bytes, 0)
            .unwrap()
            .table(ttf_parser::Tag::from_bytes(b"HVAR"))
            .is_none());
        let mut before = Sampler::new(Arc::new(source.clone()), 0).unwrap();
        let mut after = Sampler::new(Arc::new(output.bytes.clone()), 0).unwrap();
        for wght in [-1., 0., 0.125, 0.25, 0.5, 0.75, 1.] {
            for wdth in [-1., 0., 0.5, 1.] {
                let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                for glyph in 0..4 {
                    let a = before.sample_normalized(glyph, &location).unwrap().shape;
                    let b = after.sample_normalized(glyph, &location).unwrap().shape;
                    assert_eq!(a.contours, b.contours);
                    let delta = if glyph == u32::from(target) {
                        5.25 + 40.4 * support.weight(&location).unwrap()
                    } else {
                        0.
                    };
                    assert!(
                        (b.advance - a.advance - delta).abs()
                            <= output.error_bound(glyph as u16, &location, false).unwrap() + 0.002
                    );
                }
            }
        }
    }
}

fn inherited_origin_fixture(edit_vertical: bool) -> Vec<u8> {
    use crate::source_font::{master_glyf, master_gvar};
    let base = inherited_from(&vertical_font(FONT));
    let raw = ttf_parser::RawFace::parse(&base, 0).unwrap();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let at = offset(&tables[b"loca"], 12).unwrap();
    let glyf = tables.get_mut(b"glyf").unwrap();
    for (p, v) in [(2, 60i16), (4, 20), (6, 440), (8, 720), (14, 40), (16, 20)] {
        glyf[at + p..at + p + 2].copy_from_slice(&v.to_be_bytes());
    }
    for (p, v) in [(40, 440i16), (42, 720)] {
        tables.get_mut(b"head").unwrap()[p..p + 2].copy_from_slice(&v.to_be_bytes());
    }
    for (p, v) in [(14, 60i16), (16, 440)] {
        tables.get_mut(b"hhea").unwrap()[p..p + 2].copy_from_slice(&v.to_be_bytes());
    }
    tables.get_mut(b"hmtx").unwrap()[8..10].copy_from_slice(&60i16.to_be_bytes());
    tables.get_mut(b"vmtx").unwrap()[8..10].copy_from_slice(&30i16.to_be_bytes());
    tables.get_mut(b"vhea").unwrap()[12..14].copy_from_slice(&30i16.to_be_bytes());
    let axes = ["wght".into(), "wdth".into()];
    let mut child = master_gvar::read(&tables[b"gvar"], &axes, 4, 1, 8).unwrap();
    let glyph = master_glyf::read(&base, 0, 1).unwrap().unwrap();
    child[1].deltas = glyph
        .expanded(&child[1])
        .unwrap()
        .into_iter()
        .map(|p| p.map(|v| v as i16))
        .collect();
    child[1].points = None;
    if edit_vertical {
        child[1].deltas[4][0] = 7;
        child[1].deltas[5][0] += 7;
    } else {
        child[1].deltas[6][1] = 30;
        child[1].deltas[7][1] = -10;
        tables.remove(b"VVAR");
    }
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    let parent = vec![master_gvar::Tuple {
        support,
        points: None,
        deltas: vec![[0, 0], [99, 0], [111, 0], [0, 200], [0, 180]],
    }];
    let changes = [(1, (8, child.as_slice())), (3, (5, parent.as_slice()))].into();
    let gvar = master_gvar::replace_many(&tables[b"gvar"], &axes, 4, &changes).unwrap();
    tables.insert(*b"gvar", gvar);
    crate::color_fonts::assemble(tables, false).unwrap()
}
#[test]
fn independent_inherited_edits_preserve_opposite_axis_and_variable_origins() {
    use freetype::{face::LoadFlag, ffi};
    for vertical in [false, true] {
        for target in [1u16, 3] {
            let source = inherited_origin_fixture(vertical);
            let support = Support {
                axes: [("wght".into(), [0., 1., 1.])].into(),
            };
            let changes = [(
                target,
                vec![(Support::default(), 15.), (support.clone(), 20.)],
            )]
            .into();
            let empty = BTreeMap::new();
            let output = if vertical {
                apply_font(&source, &empty, &changes)
            } else {
                apply_font(&source, &changes, &empty)
            }
            .unwrap();
            let input_raw = ttf_parser::RawFace::parse(&source, 0).unwrap();
            let output_raw = ttf_parser::RawFace::parse(&output.bytes, 0).unwrap();
            let axes = ["wght".into(), "wdth".into()];
            let child_tuples = crate::source_font::master_gvar::read(
                input_raw
                    .table(ttf_parser::Tag::from_bytes(b"gvar"))
                    .unwrap(),
                &axes,
                4,
                1,
                8,
            )
            .unwrap();
            let parent_tuples = crate::source_font::master_gvar::read(
                output_raw
                    .table(ttf_parser::Tag::from_bytes(b"gvar"))
                    .unwrap(),
                &axes,
                4,
                3,
                5,
            )
            .unwrap();
            let mut old_shapes = Sampler::new(Arc::new(source.clone()), 0).unwrap();
            let mut new_shapes = Sampler::new(Arc::new(output.bytes.clone()), 0).unwrap();
            let library = freetype::Library::init().unwrap();
            let mut before = library.new_memory_face(source.clone(), 0).unwrap();
            let mut after = library.new_memory_face(output.bytes.clone(), 0).unwrap();
            for face in [&before, &after] {
                face.set_char_size(4_096_000, 0, 72, 72).unwrap();
            }
            for wght in [-1f32, 0., 0.25, 0.5, 1.] {
                for wdth in [-1f32, 0., 1.] {
                    let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                    // Compare stored functions exactly, separately from rasterizer rounding.
                    let original_phantoms = phantom_values(&child_tuples, 4, &location);
                    let copied_phantoms = phantom_values(&parent_tuples, 1, &location);
                    let start = if vertical { 0 } else { 2 };
                    for phantom in start..start + 2 {
                        assert_eq!(original_phantoms[phantom], copied_phantoms[phantom]);
                    }
                    let coordinates = [
                        (f64::from(wght) * 65536.) as ffi::FT_Fixed,
                        (f64::from(wdth) * 65536.) as ffi::FT_Fixed,
                    ];
                    for face in [&mut before, &mut after] {
                        // SAFETY: The two live faces own their buffers; coordinates match the fixture's axes.
                        assert_eq!(
                            unsafe {
                                ffi::FT_Set_Var_Blend_Coordinates(
                                    face.raw_mut(),
                                    2,
                                    coordinates.as_ptr(),
                                )
                            },
                            0
                        );
                    }
                    for glyph in [1u32, 3] {
                        let a = old_shapes
                            .sample_normalized(glyph, &location)
                            .unwrap()
                            .shape
                            .contours;
                        let b = new_shapes
                            .sample_normalized(glyph, &location)
                            .unwrap()
                            .shape
                            .contours;
                        let value = 7. * support.weight(&location).unwrap();
                        let shift = if vertical && glyph == 3 {
                            value - (value + 0.5).floor()
                        } else {
                            0.
                        };
                        assert!(
                            shift.abs()
                                <= output
                                    .native_origin_rounding
                                    .get(&(glyph as u16))
                                    .map_or(0., |v| v[0])
                        );
                        assert_eq!(b[0].start, [a[0].start[0] + shift, a[0].start[1]]);
                        for (a, b) in a[0].segments.iter().zip(&b[0].segments) {
                            match (a, b) {
                                (
                                    crate::Segment::Line { end: a },
                                    crate::Segment::Line { end: b },
                                ) => assert_eq!(*b, [a[0] + shift, a[1]]),
                                _ => panic!("line fixture"),
                            }
                        }
                        let flags = LoadFlag::NO_SCALE
                            | LoadFlag::NO_HINTING
                            | LoadFlag::NO_AUTOHINT
                            | LoadFlag::NO_BITMAP;
                        before.load_glyph(glyph, flags).unwrap();
                        after.load_glyph(glyph, flags).unwrap();
                        let old = before.glyph().metrics();
                        let new = after.glyph().metrics();
                        let delta = if glyph == u32::from(target) {
                            15. + 20. * support.weight(&location).unwrap()
                        } else {
                            0.
                        };
                        let hd = (new.horiAdvance - old.horiAdvance) as f64;
                        let vd = (new.vertAdvance - old.vertAdvance) as f64;
                        assert!(
                            (hd - if vertical { 0. } else { delta }).abs() < 0.02,
                            "horizontal, target {target}, glyph {glyph}, {location:?}, delta {hd}"
                        );
                        assert!(
                            (vd - if vertical { delta } else { 0. }).abs() < 0.02,
                            "vertical, target {target}, glyph {glyph}, {location:?}, delta {vd}"
                        );
                        assert_eq!(
                            old.vertBearingY, new.vertBearingY,
                            "vertical origin, target {target}, glyph {glyph}, {location:?}"
                        );
                        let scaled =
                            LoadFlag::NO_HINTING | LoadFlag::NO_AUTOHINT | LoadFlag::NO_BITMAP;
                        before.load_glyph(glyph, scaled).unwrap();
                        after.load_glyph(glyph, scaled).unwrap();
                        let old = before.glyph().metrics().vertBearingY as f64 / 4096.;
                        let new = after.glyph().metrics().vertBearingY as f64 / 4096.;
                        // The fixture's yMax is integral, so FreeType's final font-unit
                        // rounding of the bearing makes both routes identical here.
                        assert!((new - old).abs() < 0.001);
                        if !vertical && glyph == 3 {
                            assert_eq!(output.native_origin_rounding[&3][1], 1.);
                        }
                    }
                }
            }
        }
    }
}
fn phantom_values(
    tuples: &[crate::source_font::master_gvar::Tuple],
    physical: usize,
    location: &Location,
) -> [[f64; 2]; 4] {
    let mut out = [[0.; 2]; 4];
    for tuple in tuples {
        let weight = tuple.support.weight(location).unwrap();
        for (phantom, value) in out.iter_mut().enumerate() {
            let index = physical + phantom;
            let index = tuple
                .points
                .as_ref()
                .map_or(Some(index), |points| points.binary_search(&index).ok());
            if let Some(index) = index {
                for (axis, coordinate) in value.iter_mut().enumerate() {
                    *coordinate += f64::from(tuple.deltas[index][axis]) * weight;
                }
            }
        }
    }
    out
}

#[test]
fn nested_inherited_ancestors_keep_their_metrics_when_the_leaf_is_edited() {
    use crate::source_font::master_gvar;
    let base = inherited_font();
    let raw = ttf_parser::RawFace::parse(&base, 0).unwrap();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let offsets: Vec<_> = tables[b"loca"]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_be_bytes(*v) as usize)
        .collect();
    let mut glyf = vec![];
    let mut loca = vec![];
    for glyph in 0..4 {
        loca.extend((glyf.len() as u32).to_be_bytes());
        if glyph == 0 {
            for value in [-1i16, 20, 0, 400, 700, 0x203, 3, 0, 0] {
                glyf.extend(value.to_be_bytes());
            }
        } else {
            glyf.extend(&tables[b"glyf"][offsets[glyph]..offsets[glyph + 1]]);
        }
    }
    loca.extend((glyf.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", loca);
    tables.get_mut(b"hmtx").unwrap()[2..4].copy_from_slice(&20i16.to_be_bytes());
    tables.get_mut(b"maxp").unwrap()[30..32].copy_from_slice(&2u16.to_be_bytes());
    let gvar = master_gvar::replace(
        &tables[b"gvar"],
        &["wght".into(), "wdth".into()],
        4,
        0,
        4,
        &[],
    )
    .unwrap();
    tables.insert(*b"gvar", gvar);
    let source = crate::color_fonts::assemble(tables, false).unwrap();
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    for target in [0u16, 1, 3] {
        let result = apply_font(
            &source,
            &[(
                target,
                vec![(Support::default(), 10.), (support.clone(), 20.)],
            )]
            .into(),
            &BTreeMap::new(),
        )
        .unwrap();
        let mut before = Sampler::new(Arc::new(source.clone()), 0).unwrap();
        let mut after = Sampler::new(Arc::new(result.bytes), 0).unwrap();
        for wght in [-1., 0., 0.5, 1.] {
            for wdth in [-1., 0., 1.] {
                let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                for glyph in 0..4 {
                    let a = before.sample_normalized(glyph, &location).unwrap().shape;
                    let b = after.sample_normalized(glyph, &location).unwrap().shape;
                    assert_eq!(a.contours, b.contours);
                    let delta = if glyph == u32::from(target) {
                        10. + 20. * support.weight(&location).unwrap()
                    } else {
                        0.
                    };
                    assert!(
                        (b.advance - a.advance - delta).abs() < 0.02,
                        "target {target}, glyph {glyph}, {location:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn generated_vertical_metrics_keep_global_height_variation_and_center_default_origins() {
    use freetype::{face::LoadFlag, ffi};
    let cff = include_bytes!("../../../tests/data/variations/cff2.otf");
    let inherited = remove_table(
        &remove_table(&inherited_origin_fixture(false), b"vhea"),
        b"vmtx",
    );
    for source in [FONT.to_vec(), cff.to_vec(), inherited] {
        let support = Support {
            axes: [("wght".into(), [0., 1., 1.])].into(),
        };
        let changes = [(2, vec![(Support::default(), 20.), (support.clone(), 40.)])].into();
        let result = apply_font(&source, &BTreeMap::new(), &changes).unwrap();
        assert!(result.generated_vertical_metrics);
        let parsed = ttf_parser::Face::parse(&result.bytes, 0).unwrap();
        assert_eq!(parsed.glyph_ver_advance(ttf_parser::GlyphId(1)), Some(1000));
        assert_eq!(parsed.glyph_ver_advance(ttf_parser::GlyphId(2)), Some(1020));
        assert_eq!(
            parsed.glyph_ver_side_bearing(ttf_parser::GlyphId(1)),
            Some(150)
        );
        assert_eq!(
            parsed.glyph_ver_side_bearing(ttf_parser::GlyphId(3)),
            Some(150)
        );
        let vhea = parsed
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"vhea"))
            .unwrap();
        assert_eq!(u16::from_be_bytes(vhea[10..12].try_into().unwrap()), 1020);
        assert_eq!(u16::from_be_bytes(vhea[34..36].try_into().unwrap()), 4);
        let mut old_shapes = Sampler::new(Arc::new(source.clone()), 0).unwrap();
        let mut new_shapes = Sampler::new(Arc::new(result.bytes.clone()), 0).unwrap();
        let library = freetype::Library::init().unwrap();
        let mut before = library.new_memory_face(source, 0).unwrap();
        let mut after = library.new_memory_face(result.bytes, 0).unwrap();
        for wght in [-1f32, -0.5, 0., 0.25, 0.5, 1.] {
            for wdth in [-1f32, 0., 1.] {
                let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                let coordinates = [
                    (f64::from(wght) * 65536.) as ffi::FT_Fixed,
                    (f64::from(wdth) * 65536.) as ffi::FT_Fixed,
                ];
                for face in [&mut before, &mut after] {
                    // SAFETY: Each face owns its buffers; coordinates match the two fixture axes.
                    assert_eq!(
                        unsafe {
                            ffi::FT_Set_Var_Blend_Coordinates(
                                face.raw_mut(),
                                2,
                                coordinates.as_ptr(),
                            )
                        },
                        0
                    );
                }
                for glyph in 0..4 {
                    let a = old_shapes
                        .sample_normalized(glyph, &location)
                        .unwrap()
                        .shape;
                    let b = new_shapes
                        .sample_normalized(glyph, &location)
                        .unwrap()
                        .shape;
                    assert_eq!(a.contours, b.contours);
                    let flags = LoadFlag::NO_SCALE
                        | LoadFlag::NO_HINTING
                        | LoadFlag::NO_AUTOHINT
                        | LoadFlag::NO_BITMAP
                        | LoadFlag::VERTICAL_LAYOUT;
                    before.load_glyph(glyph, flags).unwrap();
                    after.load_glyph(glyph, flags).unwrap();
                    let old = before.glyph().metrics();
                    let new = after.glyph().metrics();
                    assert_eq!(old.horiAdvance, new.horiAdvance);
                    let delta = if glyph == 2 {
                        20. + 40. * support.weight(&location).unwrap()
                    } else {
                        0.
                    };
                    assert!(
                        ((new.vertAdvance - old.vertAdvance) as f64 - delta).abs() < 0.52,
                        "glyph {glyph}, {location:?}: {} -> {}, delta {delta}, source asc {} desc {}",
                        old.vertAdvance,
                        new.vertAdvance,before.ascender(),before.descender()
                    );
                    if glyph == 1 || glyph == 3 {
                        assert_eq!(new.vertBearingY, 150);
                    }
                }
            }
        }
    }
}
#[test]
fn generated_cff2_metrics_respect_explicit_vorg_origins() {
    let source = include_bytes!("../../../tests/data/variations/cff2.otf");
    let raw = ttf_parser::RawFace::parse(source, 0).unwrap();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let mut vorg = vec![];
    for value in [1i16, 0, 800, 1, 3, 850] {
        vorg.extend(value.to_be_bytes());
    }
    tables.insert(*b"VORG", vorg.clone());
    let source = crate::color_fonts::assemble(tables, true).unwrap();
    let output = apply_font(
        &source,
        &BTreeMap::new(),
        &[(2, vec![(Support::default(), 20.)])].into(),
    )
    .unwrap();
    let face = ttf_parser::Face::parse(&output.bytes, 0).unwrap();
    assert_eq!(
        face.glyph_ver_side_bearing(ttf_parser::GlyphId(1)),
        Some(100)
    );
    assert_eq!(
        face.glyph_ver_side_bearing(ttf_parser::GlyphId(3)),
        Some(150)
    );
    assert_eq!(
        face.raw_face()
            .table(ttf_parser::Tag::from_bytes(b"VORG"))
            .unwrap(),
        vorg
    );
}

#[test]
fn mvar_zero_delta_restores_original_metrics_after_axis_changes() {
    use freetype::{face::LoadFlag, ffi};
    for source in [
        FONT,
        include_bytes!("../../../tests/data/variations/cff2.otf").as_slice(),
    ] {
        let library = freetype::Library::init().unwrap();
        let mut face = library.new_memory_face(source.to_vec(), 0).unwrap();
        for (value, expected) in [
            (1., 1100),
            (0., 1000),
            (-1., 940),
            (0., 1000),
            (-0.5, 970),
            (0., 1000),
        ] {
            let coordinates = [(value * 65536.) as ffi::FT_Fixed, 0];
            // SAFETY: The live face owns its buffers and has the fixture's two axes.
            assert_eq!(
                unsafe {
                    ffi::FT_Set_Var_Blend_Coordinates(face.raw_mut(), 2, coordinates.as_ptr())
                },
                0
            );
            face.load_glyph(
                1,
                LoadFlag::NO_SCALE
                    | LoadFlag::NO_HINTING
                    | LoadFlag::NO_AUTOHINT
                    | LoadFlag::NO_BITMAP
                    | LoadFlag::VERTICAL_LAYOUT,
            )
            .unwrap();
            assert_eq!(face.glyph().metrics().vertAdvance, expected);
        }
    }
}
#[test]
fn vertical_creation_fallbacks_and_partial_tables_are_validated() {
    let no_mvar = remove_table(FONT, b"MVAR");
    for no_os2 in [false, true] {
        let source = if no_os2 {
            remove_table(&no_mvar, b"OS/2")
        } else {
            let raw = ttf_parser::RawFace::parse(&no_mvar, 0).unwrap();
            let mut tables: BTreeMap<_, _> = raw
                .table_records
                .into_iter()
                .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
                .collect();
            tables.get_mut(b"OS/2").unwrap()[68..72].fill(0);
            crate::color_fonts::assemble(tables, false).unwrap()
        };
        let result = apply_font(
            &source,
            &BTreeMap::new(),
            &[(2, vec![(Support::default(), 0.)])].into(),
        )
        .unwrap();
        let face = ttf_parser::Face::parse(&result.bytes, 0).unwrap();
        assert_eq!(face.glyph_ver_advance(ttf_parser::GlyphId(1)), Some(1000));
    }
    let vertical = vertical_font(FONT);
    let changes = [(2, vec![(Support::default(), 10.)])].into();
    assert!(apply_font(
        &remove_table(&vertical, b"vmtx"),
        &BTreeMap::new(),
        &changes
    )
    .is_err());
    let only_vvar = remove_table(&remove_table(&vertical, b"vhea"), b"vmtx");
    assert!(apply_font(&only_vvar, &BTreeMap::new(), &changes).is_err());
}
