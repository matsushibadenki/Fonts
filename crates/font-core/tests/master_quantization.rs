use font_core::source_font::{master_gvar, master_quantize, masters::Support};

#[test]
fn quantization_rounds_once_and_split_tuples_keep_zero_points_touched() {
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    let deltas = [
        [70000.2, -70000.3],
        [0., 0.],
        [0., 100.49],
        [0., 0.],
        [0., 0.],
        [123.5, -123.5],
        [0., 0.],
        [0., 0.],
    ];
    let encoded = master_quantize::quantize(&support, &deltas).unwrap();
    assert_eq!(encoded.tuples.len(), 3);
    assert_eq!(encoded.max_error, 0.5);
    assert!(encoded.tuples.iter().all(|t| t.points.is_none()));
    let points = [
        [0., 0.],
        [50., 0.],
        [100., 0.],
        [150., 0.],
        [0., 0.],
        [500., 0.],
        [0., 0.],
        [0., 0.],
    ];
    for value in [0., 0.125, 0.5, 1.] {
        let location = [("wght".into(), value)].into();
        let out = master_gvar::evaluate(&encoded.tuples, &points, &[3], &location).unwrap();
        for (i, p) in out.iter().enumerate() {
            for axis in 0..2 {
                assert!(
                    (p[axis] - points[i][axis] - deltas[i][axis] * f64::from(value)).abs()
                        <= encoded.max_error * f64::from(value) + 1e-8
                );
            }
        }
        assert_eq!(out[1], points[1]);
    }
    assert!(master_gvar::encode_glyph(&encoded.tuples, &["wght".into()], 8).is_ok());
}

#[test]
fn negative_limit_zero_corrections_bounds_and_invalid_values() {
    let support = Support {
        axes: [("wght".into(), [-1., -1., 0.])].into(),
    };
    let out = master_quantize::quantize(&support, &[[-32768., 32767.]; 4]).unwrap();
    assert_eq!(out.tuples.len(), 1);
    assert_eq!(out.max_error, 0.);
    let out = master_quantize::quantize(&support, &[[0.4, -0.4]; 4]).unwrap();
    assert!(out.tuples.is_empty());
    assert_eq!(out.max_error, 0.4);
    assert!(master_quantize::quantize(&support, &[[f64::NAN, 0.]; 4]).is_err());
    assert!(master_quantize::quantize(&support, &[[1e9, 0.]; 4]).is_err());
    assert!(master_quantize::quantize(&support, &[[0.; 2]; 3]).is_err());
    let invalid = Support {
        axes: [("wght".into(), [-1., 0.5, 1.])].into(),
    };
    assert!(master_quantize::quantize(&invalid, &[[0.; 2]; 4]).is_err());
    assert!(master_quantize::quantize(&Support::default(), &[[1., 0.]; 4]).is_err());
}

#[test]
fn large_glyph_payloads_split_without_allowing_iup_to_fill_the_other_blocks() {
    let support = Support {
        axes: [("wght".into(), [0., 1., 1.])].into(),
    };
    let mut deltas: Vec<_> = (0..20000)
        .map(|i| [1000. + f64::from(i % 100), -2000. - f64::from(i % 100)])
        .collect();
    deltas.extend([[0.; 2]; 4]);
    let quantized = master_quantize::quantize(&support, &deltas).unwrap();
    assert!(quantized.tuples.len() > 1);
    assert_eq!(quantized.max_error, 0.);
    assert!(master_gvar::encode_glyph(&quantized.tuples, &["wght".into()], deltas.len()).is_ok());
    let points = vec![[0.; 2]; deltas.len()];
    let result = master_gvar::evaluate(
        &quantized.tuples,
        &points,
        &[19999],
        &[("wght".into(), 1.)].into(),
    )
    .unwrap();
    assert_eq!(result, deltas);
}
