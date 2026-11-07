use super::*;
use crate::source_font::{master_glyf, master_gvar, master_sample::Sampler};
use std::{collections::BTreeMap, sync::Arc};
const FONT: &[u8] = include_bytes!("../../../tests/data/variations/gvar.ttf");
fn axes() -> [String; 2] {
    ["wght".into(), "wdth".into()]
}
fn source() -> (Glyph, Vec<Tuple>) {
    let raw = ttf_parser::RawFace::parse(FONT, 0).unwrap();
    (
        master_glyf::read(FONT, 0, 1).unwrap().unwrap(),
        master_gvar::read(
            raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap(),
            &axes(),
            4,
            1,
            8,
        )
        .unwrap(),
    )
}
// Fixture-specific SFNT assembly, including the horizontal phantom origin and global bbox.
fn font(plan: &Rebased) -> Vec<u8> {
    let face = ttf_parser::Face::parse(FONT, 0).unwrap();
    let raw = face.raw_face();
    let mut tables = BTreeMap::new();
    for record in raw.table_records {
        tables.insert(
            record.tag.to_bytes(),
            raw.table(record.tag).unwrap().to_vec(),
        );
    }
    let loca = &tables[b"loca"];
    let offsets: Vec<_> = loca
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| usize::from(u16::from_be_bytes(*v)) * 2)
        .collect();
    let old = &tables[b"glyf"];
    let mut glyf = vec![];
    let mut loca = vec![];
    let mut bounds = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
    let mut min_lsb = i16::MAX;
    let mut min_rsb = i16::MAX;
    let mut max_extent = i16::MIN;
    for glyph in 0..4 {
        loca.extend((glyf.len() as u32).to_be_bytes());
        let bytes = if glyph == 1 {
            &plan.base.bytes
        } else {
            &old[offsets[glyph]..offsets[glyph + 1]]
        };
        if bytes.len() >= 10 && bytes[..2] != [0, 0] {
            let bbox: Vec<_> = bytes[2..10]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| i16::from_be_bytes(*v))
                .collect();
            for axis in 0..2 {
                bounds[axis] = bounds[axis].min(bbox[axis]);
                bounds[axis + 2] = bounds[axis + 2].max(bbox[axis + 2]);
            }
            let lsb = if glyph == 1 {
                plan.base.bounds[0]
            } else {
                face.glyph_hor_side_bearing(ttf_parser::GlyphId(glyph as u16))
                    .unwrap()
            };
            let advance = face
                .glyph_hor_advance(ttf_parser::GlyphId(glyph as u16))
                .unwrap() as i16;
            min_lsb = min_lsb.min(lsb);
            min_rsb = min_rsb.min(advance - lsb - (bbox[2] - bbox[0]));
            max_extent = max_extent.max(lsb + bbox[2] - bbox[0]);
        }
        glyf.extend(bytes);
    }
    loca.extend((glyf.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", loca);
    let head = tables.get_mut(b"head").unwrap();
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    for (index, value) in bounds.into_iter().enumerate() {
        head[36 + index * 2..38 + index * 2].copy_from_slice(&value.to_be_bytes());
    }
    // hmtx shares its final advance across trailing glyphs; locate glyph one's actual lsb.
    let metrics = usize::from(u16::from_be_bytes(
        tables[b"hhea"][34..36].try_into().unwrap(),
    ));
    let lsb_at = if 1 < metrics {
        6
    } else {
        metrics * 4 + (1 - metrics) * 2
    };
    tables.get_mut(b"hmtx").unwrap()[lsb_at..lsb_at + 2]
        .copy_from_slice(&plan.base.bounds[0].to_be_bytes());
    let hhea = tables.get_mut(b"hhea").unwrap();
    for (index, value) in [min_lsb, min_rsb, max_extent].into_iter().enumerate() {
        hhea[12 + index * 2..14 + index * 2].copy_from_slice(&value.to_be_bytes());
    }
    let gvar = master_gvar::replace(&tables[b"gvar"], &axes(), 4, 1, 8, &plan.tuples).unwrap();
    tables.insert(*b"gvar", gvar);
    crate::color_fonts::assemble(tables, false).unwrap()
}

#[test]
fn default_outline_rebasing_preserves_source_variation_in_reopened_font() {
    let (glyph, tuples) = source();
    let mut original = Sampler::new(Arc::new(FONT.to_vec()), 0).unwrap();
    for translation in [true, false] {
        let mut points = glyph.points.clone();
        if translation {
            for p in &mut points {
                p[0] += 10.;
                p[1] += 25.;
            }
        } else {
            points[1][0] -= 50.;
            points[1][1] += 20.;
        }
        let plan = rebase(&glyph, &tuples, &points).unwrap();
        if translation {
            assert_eq!(plan.tuples, tuples);
        } else {
            assert!(plan.tuples.iter().any(|t| t.points.is_none()));
        }
        assert!(plan.errors.is_empty());
        let plans = BTreeMap::from([(1, plan)]);
        let output = crate::source_font::master_tables::apply(FONT, &plans).unwrap();
        let plan = &plans[&1];
        let reread = master_glyf::read(&output, 0, 1).unwrap().unwrap();
        assert_eq!(reread.points, points);
        let mut exported = Sampler::new(Arc::new(output), 0).unwrap();
        for wght in [-1., -0.5, 0., 0.5, 1.] {
            for wdth in [-1., 0., 1.] {
                let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                assert_eq!(plan.error_bound(&location).unwrap(), 0.);
                let mut expected = glyph.sample_points(&tuples, &location).unwrap();
                for (index, p) in expected.iter_mut().enumerate() {
                    for axis in 0..2 {
                        p[axis] += points[index][axis] - glyph.points[index][axis];
                    }
                }
                let contours = glyph.topology.contours(&expected).unwrap();
                let actual = exported.sample_normalized(1, &location).unwrap();
                assert_eq!(actual.shape.contours[0].start, contours[0].start);
                assert_eq!(actual.shape.contours[0].segments, contours[0].segments);
                assert_eq!(
                    actual.shape.advance,
                    original
                        .sample_normalized(1, &location)
                        .unwrap()
                        .shape
                        .advance
                );
                assert_eq!(
                    original.sample_normalized(3, &location).unwrap().shape,
                    exported.sample_normalized(3, &location).unwrap().shape
                );
            }
        }
    }
}

#[test]
fn fractional_iup_and_default_coordinate_rounding_have_a_measured_bound() {
    let (mut glyph, _) = source();
    glyph.points[1][0] = 210.;
    let tuple = Tuple {
        support: Support {
            axes: [("wght".into(), [0., 1., 1.])].into(),
        },
        points: Some(vec![0, 2]),
        deltas: vec![[0, 0], [1, 3]],
    };
    let mut points = glyph.points.clone();
    points[1][0] = 230.25;
    let plan = rebase(&glyph, std::slice::from_ref(&tuple), &points).unwrap();
    assert_eq!(plan.base.max_error, 0.25);
    assert_eq!(plan.errors.len(), 1);
    let relocated = glyph.relocated(plan.base.points.clone());
    for value in [0., 0.25, 0.5, 1.] {
        let location = [("wght".into(), value), ("wdth".into(), 0.)].into();
        let mut expected = glyph
            .sample_points(std::slice::from_ref(&tuple), &location)
            .unwrap();
        for (index, p) in expected.iter_mut().enumerate() {
            for axis in 0..2 {
                p[axis] += points[index][axis] - glyph.points[index][axis];
            }
        }
        let actual = relocated.sample_points(&plan.tuples, &location).unwrap();
        let bound = plan.error_bound(&location).unwrap();
        for (a, b) in actual.iter().zip(expected) {
            for axis in 0..2 {
                assert!((a[axis] - b[axis]).abs() <= bound + 1e-10);
            }
        }
    }
    assert!(plan
        .error_bound(&[("wght".into(), f32::NAN)].into())
        .is_err());
}

#[test]
fn base_encoder_preserves_topology_strips_instructions_and_rejects_unrepresentable_points() {
    let (glyph, _) = source();
    let encoded = glyph.encode_base(&glyph.points).unwrap();
    assert_eq!(&encoded.bytes[12..14], &[0, 0]);
    let mut hinted = rebase(&glyph, &[], &glyph.points).unwrap();
    hinted.base.bytes[14] |= 0x40;
    hinted.base.bytes[12..14].copy_from_slice(&2u16.to_be_bytes());
    hinted.base.bytes.splice(14..14, [0xb0, 0]);
    let imported = master_glyf::read(&font(&hinted), 0, 1).unwrap().unwrap();
    let rewritten = imported.encode_base(&imported.points).unwrap();
    assert_eq!(&rewritten.bytes[12..14], &[0, 0]);
    assert_ne!(rewritten.bytes[14] & 0x40, 0);
    let mut invalid = glyph.points.clone();
    invalid[0][0] = f64::NAN;
    assert!(glyph.encode_base(&invalid).is_err());
    invalid[0][0] = 40000.;
    assert!(glyph.encode_base(&invalid).is_err());
    invalid[0][0] = -30000.;
    invalid[1][0] = 30000.;
    assert!(glyph.encode_base(&invalid).is_err());
    assert!(glyph.encode_base(&[]).is_err());
    let empty = master_glyf::read(FONT, 0, 2).unwrap().unwrap();
    assert_eq!(empty.encode_base(&[]).unwrap().bounds, [0; 4]);
}
