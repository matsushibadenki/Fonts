use font_core::{
    identity::Uuid,
    source_font::masters::{Location, Master, Masters, Shape, Support},
    Contour, Segment,
};
fn shape(t: f64) -> Shape {
    Shape {
        advance: 600. + t * 20.,
        contours: vec![Contour {
            identity: Default::default(),
            smooth: Default::default(),
            start: [100. + 20. * t, 0.],
            segments: vec![Segment::Cubic {
                control1: [120. + 20. * t, 300.],
                control2: [400. + 20. * t, 300.],
                end: [500. + 20. * t, 0.],
            }],
            closed: false,
        }],
    }
}
fn shifted(mut s: Shape, delta: f64) -> Shape {
    s.advance += delta * 2.;
    s.contours[0].start[0] += delta;
    if let Segment::Cubic {
        control1,
        control2,
        end,
    } = &mut s.contours[0].segments[0]
    {
        control1[0] += delta;
        control2[0] += delta;
        end[0] += delta;
    }
    s
}
fn loc(value: f32) -> Location {
    [("wght".into(), value)].into()
}
fn master(value: f32, support: Option<[f32; 3]>, target: Option<Shape>) -> Master {
    Master {
        id: Uuid::new_v4(),
        name: format!("Master {value}"),
        design_location: [("wght".into(), 400. + value * 500.)].into(),
        location: loc(value),
        support: Support {
            axes: support
                .map(|r| [("wght".into(), r)].into())
                .unwrap_or_default(),
        },
        glyphs: target.map(|s| [(1, s)].into()).unwrap_or_default(),
    }
}
#[test]
fn overlapping_supports_interpolate_absolute_targets_without_erasing_source_variation() {
    let samples = vec![shape(0.), shape(0.5), shape(1.)];
    let targets: Vec<_> = samples
        .iter()
        .cloned()
        .zip([10., 30., 50.])
        .map(|(s, d)| shifted(s, d))
        .collect();
    let masters = Masters {
        entries: vec![
            master(0., None, Some(targets[0].clone())),
            master(0.5, Some([0., 0.5, 1.]), Some(targets[1].clone())),
            master(1., Some([0., 1., 1.]), Some(targets[2].clone())),
        ],
    };
    masters.validate().unwrap();
    for (i, t) in [0., 0.5, 1.].into_iter().enumerate() {
        let actual = masters
            .interpolate(1, &loc(t), &samples[i], &samples)
            .unwrap();
        assert_eq!(actual, targets[i]);
    }
    let actual = masters
        .interpolate(1, &loc(0.75), &shape(0.75), &samples)
        .unwrap();
    assert_eq!(actual, shifted(shape(0.75), 40.));
    let serialized = serde_json::to_vec(&masters).unwrap();
    let restored: Masters = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(restored, masters);
    restored.validate().unwrap();
    assert_eq!(
        restored
            .interpolate(1, &loc(0.75), &shape(0.75), &samples)
            .unwrap(),
        actual
    );
}
#[test]
fn untouched_masters_constrain_other_corrections_and_unedited_glyphs_keep_source_shape() {
    let samples = vec![shape(0.), shape(0.5), shape(1.)];
    let masters = Masters {
        entries: vec![
            master(0., None, Some(shifted(samples[0].clone(), 10.))),
            master(
                0.5,
                Some([0., 0.5, 1.]),
                Some(shifted(samples[1].clone(), 30.)),
            ),
            master(1., Some([0., 1., 1.]), None),
        ],
    };
    assert_eq!(
        masters
            .interpolate(1, &loc(1.), &samples[2], &samples)
            .unwrap(),
        samples[2]
    );
    assert_eq!(
        masters
            .interpolate(2, &loc(0.75), &shape(0.75), &samples)
            .unwrap(),
        shape(0.75)
    );
}
#[test]
fn multi_axis_tents_and_negative_coordinates_are_evaluated_together() {
    let support = Support {
        axes: [
            ("wght".into(), [0., 0.5, 1.]),
            ("wdth".into(), [-1., -0.5, 0.]),
        ]
        .into(),
    };
    assert_eq!(
        support
            .weight(&[("wght".into(), 0.25), ("wdth".into(), -0.25)].into())
            .unwrap(),
        0.25
    );
    assert_eq!(
        support
            .weight(&[("wght".into(), 0.5), ("wdth".into(), -0.5)].into())
            .unwrap(),
        1.
    );
    assert_eq!(
        support
            .weight(&[("wght".into(), 0.5), ("wdth".into(), 0.)].into())
            .unwrap(),
        0.
    );
}
#[test]
fn singular_registries_incompatible_topology_and_nonfinite_data_are_rejected() {
    let a = master(0.5, Some([0., 0.5, 1.]), Some(shape(0.5)));
    let mut b = a.clone();
    b.id = Uuid::new_v4();
    assert!(Masters {
        entries: vec![a.clone(), b]
    }
    .validate()
    .is_err());
    let mut b = master(1., Some([0., 1., 1.]), Some(shape(1.)));
    b.glyphs.get_mut(&1).unwrap().contours[0].segments[0] = Segment::Line { end: [500., 0.] };
    assert!(Masters {
        entries: vec![a.clone(), b]
    }
    .validate()
    .is_err());
    let masters = Masters { entries: vec![a] };
    let mut incompatible = shape(0.5);
    incompatible.contours[0].closed = true;
    assert!(masters
        .interpolate(1, &loc(0.5), &incompatible, &[shape(0.5)])
        .is_err());
    assert!(masters.weights(&loc(f32::NAN)).is_err());
    let bad = Support {
        axes: [("wght".into(), [0.5, 0., 1.])].into(),
    };
    assert!(bad.weight(&loc(0.5)).is_err());
}

#[test]
fn worker_sampler_preserves_gvar_cff2_topology_and_applies_avar_only_once() {
    use font_core::source_font::master_sample::Sampler;
    use std::sync::Arc;
    for bytes in [
        include_bytes!("data/variations/gvar.ttf").as_slice(),
        include_bytes!("data/variations/cff2.otf").as_slice(),
    ] {
        let mut sampler = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
        assert!(sampler.sample_design(0, &Location::new()).is_ok());
        let base = sampler.sample_design(1, &Location::new()).unwrap();
        assert!((base.shape.advance - 500.).abs() < 0.02);
        let heavy = sampler
            .sample_design(1, &[("wght".into(), 650.), ("wdth".into(), 125.)].into())
            .unwrap();
        assert!((heavy.shape.advance - 825.).abs() < 0.02);
        assert_eq!(heavy.location["wght"], 0.75);
        assert_eq!(heavy.location["wdth"], 1.);
        assert!(base.shape.compatible(&heavy.shape));
        let normalized = sampler
            .sample_normalized(1, &[("wght".into(), 0.75), ("wdth".into(), 1.)].into())
            .unwrap();
        assert_eq!(normalized.shape, heavy.shape);
        assert!((normalized.design_location["wght"] - 650.).abs() < 0.02);
        let light = sampler
            .sample_design(1, &[("wght".into(), 100.)].into())
            .unwrap();
        assert!(light.shape.compatible(&heavy.shape));
        assert!((light.shape.advance - 300.).abs() < 0.02);
        assert!(sampler
            .sample_design(1, &[("wght".into(), 901.)].into())
            .is_err());
        assert!(sampler.sample_normalized(1, &loc(0.5)).is_err());
        assert!(sampler.sample_design(65535, &Location::new()).is_err());
        let empty = sampler.sample_design(2, &Location::new()).unwrap();
        assert!(empty.shape.contours.is_empty());
        assert!((empty.shape.advance - 500.).abs() < 0.02);
    }
}

#[test]
fn export_coefficients_match_cardinal_preview_with_overlapping_regions_and_multiple_columns() {
    let masters = Masters {
        entries: vec![
            master(0., None, None),
            master(0.5, Some([0., 0.5, 1.]), None),
            master(1., Some([0., 1., 1.]), None),
        ],
    };
    let residuals = vec![
        vec![10., 20., -3.],
        vec![37., 74., 6.],
        vec![50., 100., -9.],
    ];
    let coefficients = masters.coefficients(&residuals).unwrap();
    assert_eq!(coefficients[0], residuals[0]);
    assert_eq!(coefficients[1], [7., 14., 12.]);
    assert_eq!(coefficients[2], [40., 80., -6.]);
    for coordinate in [0., 0.125, 0.5, 0.75, 1.] {
        let location = loc(coordinate);
        let weights = masters.weights(&location).unwrap();
        for column in 0..3 {
            let preview: f64 = weights
                .iter()
                .zip(&residuals)
                .map(|(w, r)| w * r[column])
                .sum();
            let output: f64 = masters
                .entries
                .iter()
                .zip(&coefficients)
                .map(|(m, c)| m.support.weight(&location).unwrap() * c[column])
                .sum();
            assert!((preview - output).abs() < 1e-10);
        }
    }
    assert!(masters.coefficients(&[vec![0.], vec![0.]]).is_err());
    assert!(masters.coefficients(&[vec![0.], vec![], vec![0.]]).is_err());
    assert!(masters
        .coefficients(&[vec![f64::NAN], vec![f64::NAN], vec![f64::NAN]])
        .is_err());
    let singular = Masters {
        entries: vec![
            master(1., Some([0., 1., 1.]), None),
            master(1., Some([0., 1., 1.]), None),
        ],
    };
    assert!(singular.coefficients(&[vec![0.], vec![0.]]).is_err());
    assert!(Masters::default().coefficients(&[]).unwrap().is_empty());
}
