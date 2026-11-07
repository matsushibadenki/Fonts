use font_core::source_font::master_tuple::*;
#[test]
fn packed_point_runs_roundtrip_zero_index_large_gaps_and_all_points() {
    for points in [
        vec![0],
        vec![1, 2, 300, 301, 1000, 65535, 65538],
        (0..400).collect(),
    ] {
        let b = write_points(Some(&points), 65539).unwrap();
        let (out, n) = read_points(&b, 65539).unwrap();
        assert_eq!(out, Some(points));
        assert_eq!(n, b.len());
    }
    assert_eq!(
        read_points(&write_points(None, 10).unwrap(), 10).unwrap(),
        (None, 1)
    );
    for b in [
        vec![],
        vec![0x80],
        vec![2, 1, 0, 0],
        vec![1, 0x80, 0],
        vec![1, 1, 0, 1],
    ] {
        assert!(read_points(&b, 10).is_err());
    }
    assert!(write_points(Some(&[1, 1]), 3).is_err());
    assert!(write_points(Some(&[]), 3).is_err());
}
#[test]
fn signed_delta_runs_keep_word_byte_zero_and_sixty_four_boundaries() {
    let values: Vec<i16> = (0..300)
        .map(|i| match i % 7 {
            0 => 0,
            1 => -128,
            2 => 127,
            3 => -129,
            4 => 128,
            5 => i16::MIN,
            _ => i16::MAX,
        })
        .collect();
    for values in [
        &values[..],
        &[0; 130][..],
        &[-5; 129][..],
        &[500; 129][..],
        &[][..],
    ] {
        let b = write_deltas(values).unwrap();
        let (out, n) = read_deltas(&b, values.len()).unwrap();
        assert_eq!(out, values);
        assert_eq!(n, b.len());
        if !b.is_empty() {
            assert!(read_deltas(&b[..b.len() - 1], values.len()).is_err());
        }
    }
    assert!(read_deltas(&[0x81], 1).is_err());
    assert_eq!(read_deltas(&[0xc0], 1).unwrap(), (vec![0], 1));
}
#[test]
fn iup_keeps_fractional_values_wraps_contours_and_never_fills_phantoms() {
    let points = [
        [245., 100.],
        [260., 200.],
        [305., 150.],
        [0., 0.],
        [100., 0.],
        [0., 0.],
        [0., 0.],
    ];
    let deltas = [
        Some([28., -62.]),
        None,
        Some([-42., -57.]),
        None,
        None,
        Some([100., 0.]),
        None,
    ];
    let out = infer_deltas(&points, &[2], &deltas, 4).unwrap();
    assert_eq!(out[1], [10.5, -57.]);
    assert_eq!(out[3], [0., 0.]);
    assert_eq!(out[5], [100., 0.]);
    let reverse = [[305., 150.], [260., 200.], [245., 100.]];
    let out = infer_deltas(
        &reverse,
        &[2],
        &[Some([-42., -57.]), None, Some([28., -62.])],
        0,
    )
    .unwrap();
    assert_eq!(out[1], [10.5, -57.]);
}
#[test]
fn iup_single_reference_empty_contours_equal_coordinates_and_components_are_distinct() {
    let points = [
        [0., 0.],
        [0., 10.],
        [0., 20.],
        [0., 30.],
        [0., 40.],
        [0., 50.],
    ];
    let out = infer_deltas(
        &points,
        &[2, 5],
        &[None, Some([7., 9.]), None, None, None, None],
        0,
    )
    .unwrap();
    assert_eq!(
        out,
        [[7., 9.], [7., 9.], [7., 9.], [0.; 2], [0.; 2], [0.; 2]]
    );
    let out = infer_deltas(
        &points,
        &[5],
        &[Some([2., 3.]), None, Some([4., 5.]), None, None, None],
        0,
    )
    .unwrap();
    assert_eq!(out[1][0], 0.);
    assert_eq!(out[1][1], 4.);
    let out = infer_deltas(
        &points,
        &[],
        &[None, Some([10., 20.]), None, None, None, None],
        4,
    )
    .unwrap();
    assert_eq!(out[0], [0.; 2]);
    assert_eq!(out[1], [10., 20.]);
    let out = infer_deltas(
        &points,
        &[5],
        &[Some([2., 3.]), None, Some([2., 5.]), None, None, None],
        0,
    )
    .unwrap();
    assert_eq!(out[1][0], 2.);
    assert!(infer_deltas(&points, &[4], &[None; 6], 0).is_err());
    assert!(infer_deltas(&points, &[2, 2, 5], &[None; 6], 0).is_err());
}
