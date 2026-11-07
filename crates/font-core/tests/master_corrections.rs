use font_core::{
    geometry::{self, EditPoint},
    identity::Uuid,
    source_font::{
        master_correction, master_glyf, master_gvar,
        master_sample::Sampler,
        masters::{Location, Master, Masters, Shape, Support},
    },
};
use std::sync::Arc;

#[test]
fn solved_physical_corrections_and_metrics_match_preview_and_preserve_unedited_masters() {
    let bytes = include_bytes!("data/variations/gvar.ttf");
    let raw = ttf_parser::RawFace::parse(bytes, 0).unwrap();
    let tuples = master_gvar::read(
        raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap(),
        &["wght".into(), "wdth".into()],
        4,
        1,
        8,
    )
    .unwrap();
    let glyph = master_glyf::read(bytes, 0, 1).unwrap().unwrap();
    let mut sampler = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
    for edit_middle in [false, true] {
        let mut entries = vec![];
        let mut samples = vec![];
        for (index, (wght, wdth, delta)) in [(0., 0., 10.), (0.5, 0.5, 37.), (1., 1., 50.)]
            .into_iter()
            .enumerate()
        {
            let location: Location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            let native = sampler.sample_normalized(1, &location).unwrap();
            let sample = Shape {
                advance: native.shape.advance,
                contours: glyph.contours(&tuples, &location).unwrap(),
            };
            let mut edited = sample.contours.clone();
            for anchor in 0..4 {
                let node = EditPoint::Anchor(anchor);
                let position = geometry::point_position(&edited[0], node).unwrap();
                edited = glyph
                    .topology
                    .move_node(&edited, 0, node, [position[0], position[1] + delta])
                    .unwrap();
            }
            let support = match index {
                0 => Support::default(),
                1 => Support {
                    axes: [("wght".into(), [0., 0.5, 1.])].into(),
                },
                _ => Support {
                    axes: [("wght".into(), [0., 1., 1.]), ("wdth".into(), [0., 1., 1.])].into(),
                },
            };
            let edited = Shape {
                advance: sample.advance + delta * 2.,
                contours: edited,
            };
            entries.push(Master {
                id: Uuid::new_v4(),
                name: format!("Master {index}"),
                design_location: native.design_location,
                location,
                support,
                glyphs: if index != 1 || edit_middle {
                    [(1, edited)].into()
                } else {
                    Default::default()
                },
            });
            samples.push(sample);
        }
        let masters = Masters { entries };
        let advances: Vec<_> = samples.iter().map(|s| s.advance).collect();
        let corrections =
            master_correction::solve(&glyph, 1, &masters, &tuples, &advances).unwrap();
        assert_eq!(corrections[0].support, Support::default());
        for (wght, wdth) in [
            (0., 0.),
            (0.5, 0.5),
            (1., 1.),
            (0.25, 0.75),
            (0.75, 0.5),
            (-0.5, -1.),
        ] {
            let location: Location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            let mut points = glyph.sample_points(&tuples, &location).unwrap();
            let mut advance = sampler
                .sample_normalized(1, &location)
                .unwrap()
                .shape
                .advance;
            let current = Shape {
                advance,
                contours: glyph.contours(&tuples, &location).unwrap(),
            };
            let preview = masters
                .interpolate(1, &location, &current, &samples)
                .unwrap();
            for correction in &corrections {
                let weight = correction.support.weight(&location).unwrap();
                advance += weight * correction.advance;
                for (p, d) in points.iter_mut().zip(&correction.points) {
                    for axis in 0..2 {
                        p[axis] += weight * d[axis];
                    }
                }
            }
            assert!((advance - preview.advance).abs() < 1e-9);
            assert_eq!(glyph.topology.contours(&points).unwrap(), preview.contours);
        }
        let untouched = master_correction::solve(&glyph, 3, &masters, &tuples, &advances).unwrap();
        assert!(untouched
            .iter()
            .all(|c| c.advance == 0. && c.points.iter().all(|p| *p == [0.; 2])));
        assert!(master_correction::solve(&glyph, 1, &masters, &tuples, &[500.]).is_err());
        let mut invalid = masters.clone();
        invalid.entries[0].glyphs.get_mut(&1).unwrap().contours[0].identity = Default::default();
        assert!(master_correction::solve(&glyph, 1, &invalid, &tuples, &advances).is_err());
    }
}
