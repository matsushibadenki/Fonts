use super::*;
use crate::source_font::{master_project::Edit, master_sample::Sampler, SourceFont};
use std::sync::Arc;
fn composite() -> Vec<u8> {
    let bytes = include_bytes!("../../../tests/data/variations/gvar.ttf");
    fixture(bytes)
}
fn fixture(bytes: &[u8]) -> Vec<u8> {
    let face = ttf_parser::Face::parse(bytes, 0).unwrap();
    let raw = face.raw_face();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let loca = &tables[b"loca"];
    let glyf = &tables[b"glyf"];
    let mut records: Vec<_> = (0..4)
        .map(|i| {
            glyf[usize::from(u16::from_be_bytes(
                loca[i * 2..i * 2 + 2].try_into().unwrap(),
            )) * 2
                ..usize::from(u16::from_be_bytes(
                    loca[i * 2 + 2..i * 2 + 4].try_into().unwrap(),
                )) * 2]
                .to_vec()
        })
        .collect();
    let mut record = vec![];
    for value in [-1i16, 60, 20, 440, 720] {
        record.extend(value.to_be_bytes());
    }
    record.extend(0x203u16.to_be_bytes());
    record.extend(1u16.to_be_bytes());
    record.extend(40i16.to_be_bytes());
    record.extend(20i16.to_be_bytes());
    records[3] = record;
    let mut data = vec![];
    let mut offsets = vec![];
    for record in records {
        offsets.extend((data.len() as u32).to_be_bytes());
        data.extend(record);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    offsets.extend((data.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", data);
    tables.insert(*b"loca", offsets);
    tables.get_mut(b"head").unwrap()[50..52].copy_from_slice(&1i16.to_be_bytes());
    let t = master_gvar::Tuple {
        support: Support {
            axes: [("wght".into(), [0., 1., 1.])].into(),
        },
        points: None,
        deltas: vec![[17, 9], [0, 0], [0, 0], [0, 0], [0, 0]],
    };
    let gvar = master_gvar::replace_reindexed(
        &tables[b"gvar"],
        &["wght".into(), "wdth".into()],
        4,
        &[(3, (5, std::slice::from_ref(&t)))].into(),
        Some(&[(3, 8)].into()),
    )
    .unwrap();
    tables.insert(*b"gvar", gvar);
    crate::color_fonts::assemble(tables, false).unwrap()
}
#[test]
fn composed_child_variations_and_metric_origins_survive_independent_master_edits() {
    let bytes = composite();
    let normalized = normalize(&bytes, &[3].into()).unwrap();
    let mut a = Sampler::new(Arc::new(bytes.clone()), 0).unwrap();
    let mut b = Sampler::new(Arc::new(normalized), 0).unwrap();
    for x in [-1., 0., 0.125, 0.5, 0.75, 1.] {
        for y in [-1., 0., 0.5, 1.] {
            let location = [("wght".into(), x), ("wdth".into(), y)].into();
            let old = a.sample_normalized(3, &location).unwrap().shape;
            let new = b.sample_normalized(3, &location).unwrap().shape;
            assert!((old.advance - new.advance).abs() < 0.02);
            assert!(
                (old.contours[0].start[0] - new.contours[0].start[0]).abs() <= 0.52,
                "{x},{y}: {:?} {:?}",
                old.contours[0].start,
                new.contours[0].start
            );
            assert!((old.contours[0].start[1] - new.contours[0].start[1]).abs() <= 0.52);
        }
    }
    let mut editor =
        crate::Editor::new(SourceFont::variable_document(&bytes, 0, None).unwrap()).unwrap();
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
        .edit_source_master(editor.revision(), Edit::Capture { id, glyph: 3 })
        .unwrap();
    let before = editor.document().clone();
    let source = before.source_font.as_ref().unwrap();
    let shape = source
        .masters
        .as_ref()
        .unwrap()
        .shape(source, id, 3)
        .unwrap();
    let start = shape.contours[0].start;
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Point {
                id,
                glyph: 3,
                index: 0,
                position: [start[0] + 20., start[1]],
            },
        )
        .unwrap();
    let source = editor.document().source_font.as_ref().unwrap();
    let out = source.export().unwrap();
    source.verify_output(&out, false).unwrap();
    let mut native = Sampler::new(Arc::new(out), 0).unwrap();
    let sample = native
        .sample_design(3, &[("wght".into(), 400.), ("wdth".into(), 100.)].into())
        .unwrap();
    assert!((sample.shape.contours[0].start[0] - start[0] - 20.).abs() < 0.02);
    editor.undo(editor.revision()).unwrap();
    assert_eq!(editor.document(), &before);
}

#[test]
fn topology_changes_migrate_component_and_gdef_point_indices_and_protect_live_references() {
    let bytes = composite();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    let raw = face.raw_face();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let loca = &tables[b"loca"];
    let glyf = &tables[b"glyf"];
    let offsets: Vec<_> = loca
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_be_bytes(*b) as usize)
        .collect();
    let mut records: Vec<_> = offsets
        .windows(2)
        .map(|w| glyf[w[0]..w[1]].to_vec())
        .collect();
    let mut record = vec![];
    for value in [-1i16, 20, 0, 780, 700] {
        record.extend(value.to_be_bytes());
    }
    for word in [0x223u16, 1, 0, 0, 1, 1, 1, 0] {
        record.extend(word.to_be_bytes());
    }
    records[3] = record;
    let mut data = vec![];
    let mut locations = vec![];
    for record in records {
        locations.extend((data.len() as u32).to_be_bytes());
        data.extend(record);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    locations.extend((data.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", data);
    tables.insert(*b"loca", locations);
    let empty: &[master_gvar::Tuple] = &[];
    let gvar = master_gvar::replace_reindexed(
        &tables[b"gvar"],
        &["wght".into(), "wdth".into()],
        4,
        &[(3, (6, empty))].into(),
        Some(&[(3, 5)].into()),
    )
    .unwrap();
    tables.insert(*b"gvar", gvar);
    let words = [
        1u16, 0, 0, 12, 28, 0, 10, 1, 6, 1, 1, 1, 1, 1, 14, 1, 6, 1, 4, 2, 1, 1, 1, 1,
    ];
    let gdef: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    tables.insert(*b"GDEF", gdef);
    let bytes = crate::color_fonts::assemble(tables, false).unwrap();
    let mut editor =
        crate::Editor::new(SourceFont::variable_document(&bytes, 0, None).unwrap()).unwrap();
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
        .edit_source_master(editor.revision(), Edit::Capture { id, glyph: 1 })
        .unwrap();
    editor
        .edit_source_master(
            editor.revision(),
            Edit::Topology {
                glyph: 1,
                operation: crate::source_font::master_topology::Operation::Split {
                    contour: 0,
                    segment: 0,
                },
            },
        )
        .unwrap();
    let source = editor.document().source_font.as_ref().unwrap();
    let output = source.export().unwrap();
    source.verify_output(&output, false).unwrap();
    let parsed = ttf_parser::Face::parse(&output, 0).unwrap();
    let gdef = parsed
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"GDEF"))
        .unwrap();
    let word = |at| u16::from_be_bytes([gdef[at], gdef[at + 1]]) as usize;
    let attach = word(6);
    let point = attach + word(attach + 4);
    assert_eq!(word(point + 2), 2);
    let list = word(8);
    let row = list + word(list + 4);
    let caret = row + word(row + 2);
    assert_eq!(word(caret + 2), 2);
    let mut old = Sampler::new(Arc::new(bytes), 0).unwrap();
    let mut new = Sampler::new(Arc::new(output), 0).unwrap();
    for x in [-1., 0., 0.25, 0.5, 1.] {
        let location = [("wght".into(), x), ("wdth".into(), 0.)].into();
        let a = old.sample_normalized(3, &location).unwrap().shape;
        let b = new.sample_normalized(3, &location).unwrap().shape;
        assert_eq!(a.advance, b.advance);
        assert_eq!(a.contours[1].start, b.contours[1].start);
    }
    let before = editor.document().clone();
    assert!(editor
        .edit_source_master(
            editor.revision(),
            Edit::Topology {
                glyph: 1,
                operation: crate::source_font::master_topology::Operation::Delete {
                    contour: 0,
                    segment: 1
                }
            }
        )
        .is_err());
    assert_eq!(editor.document(), &before);
}

#[test]
fn nested_scaled_components_keep_their_variation_functions_with_reported_rounding() {
    let bytes = composite();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    let raw = face.raw_face();
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| (r.tag.to_bytes(), raw.table(r.tag).unwrap().to_vec()))
        .collect();
    let loca = &tables[b"loca"];
    let glyf = &tables[b"glyf"];
    let offsets: Vec<_> = loca
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_be_bytes(*b) as usize)
        .collect();
    let mut records: Vec<_> = offsets
        .windows(2)
        .map(|w| glyf[w[0]..w[1]].to_vec())
        .collect();
    let mut record = vec![];
    for value in [-1i16, 40, 10, 230, 360] {
        record.extend(value.to_be_bytes());
    }
    for value in [0x20bu16, 3, 10, 0, 8192] {
        record.extend(value.to_be_bytes());
    }
    records[0] = record;
    let mut data = vec![];
    let mut loca = vec![];
    for record in records {
        loca.extend((data.len() as u32).to_be_bytes());
        data.extend(record);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    loca.extend((data.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", data);
    tables.insert(*b"loca", loca);
    // The original empty .notdef has four phantom points. Inserting a component
    // shifts all four indices; rebuild its source tuples before testing the font.
    let axes = ["wght".into(), "wdth".into()];
    let tuples: Vec<_> = master_gvar::read(&tables[b"gvar"], &axes, 4, 0, 4)
        .unwrap()
        .into_iter()
        .map(|tuple| {
            let mut deltas = vec![[0; 2]];
            deltas.extend(
                tuple
                    .expand(&[[0.; 2]; 4], &[])
                    .unwrap()
                    .into_iter()
                    .map(|p| p.map(|v| v as i16)),
            );
            master_gvar::Tuple {
                support: tuple.support,
                points: None,
                deltas,
            }
        })
        .collect();
    let gvar = master_gvar::replace_reindexed(
        &tables[b"gvar"],
        &axes,
        4,
        &[(0, (5, tuples.as_slice()))].into(),
        Some(&[(0, 4)].into()),
    )
    .unwrap();
    tables.insert(*b"gvar", gvar);
    let bytes = crate::color_fonts::assemble(tables, false).unwrap();
    let normalized = normalize_report(&bytes, &[0, 3].into()).unwrap();
    let mut before = Sampler::new(Arc::new(bytes), 0).unwrap();
    let mut after = Sampler::new(Arc::new(normalized.bytes), 0).unwrap();
    for x in [-1., 0., 0.125, 0.375, 0.5, 0.75, 1.] {
        for y in [-1., 0., 0.5, 1.] {
            let location = [("wght".into(), x), ("wdth".into(), y)].into();
            for id in [0u16, 1, 3] {
                let a = before
                    .sample_normalized(u32::from(id), &location)
                    .unwrap()
                    .shape;
                let b = after
                    .sample_normalized(u32::from(id), &location)
                    .unwrap()
                    .shape;
                assert_eq!(a.advance, b.advance);
                let bound = normalized.native.get(&id).copied().unwrap_or(0.)
                    + normalized
                        .errors
                        .get(&id)
                        .into_iter()
                        .flatten()
                        .map(|(s, e)| s.weight(&location).unwrap() * e)
                        .sum::<f64>()
                    + 0.02;
                for (a, b) in a.contours.iter().zip(&b.contours) {
                    for axis in 0..2 {
                        assert!(
                            (a.start[axis] - b.start[axis]).abs() <= bound,
                            "glyph {id}, {location:?}: {:?} {:?}, bound {bound}",
                            a.start,
                            b.start
                        );
                    }
                }
            }
        }
    }
}
