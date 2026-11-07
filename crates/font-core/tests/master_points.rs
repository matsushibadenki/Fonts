use font_core::{
    geometry::{self, EditPoint},
    identity,
    source_font::master_points::{Binding, Topology},
    Segment,
};

#[test]
fn every_small_on_off_curve_pattern_roundtrips_without_coordinate_dependent_topology() {
    for count in 1..=6 {
        for mask in 0..1 << count {
            let ring = (0..count).map(|i| mask & (1 << i) != 0).collect();
            let topology = Topology::new(vec![ring]).unwrap();
            let zero = vec![[0.; 2]; count];
            let varied: Vec<_> = (0..count)
                .map(|i| [i as f64 * 13., (i * i) as f64 * 7.])
                .collect();
            let a = topology.contours(&zero).unwrap();
            let b = topology.contours(&varied).unwrap();
            assert_eq!(a[0].identity, b[0].identity);
            assert_eq!(a[0].segments.len(), b[0].segments.len());
            assert_eq!(topology.physical_points(&a).unwrap(), zero);
            assert_eq!(topology.physical_points(&b).unwrap(), varied);
            for (address, _) in geometry::edit_points(&a[0]) {
                assert_eq!(
                    identity::node_id(&a[0], address),
                    identity::node_id(&b[0], address)
                );
            }
            let restored: Topology =
                serde_json::from_str(&serde_json::to_string(&topology).unwrap()).unwrap();
            assert_eq!(restored.contours(&varied).unwrap(), b);
        }
    }
}

#[test]
fn coincident_last_physical_point_keeps_its_own_identity_and_closure() {
    let topology = Topology::new(vec![vec![true; 3]]).unwrap();
    let a = topology
        .contours(&[[0., 0.], [100., 200.], [0., 0.]])
        .unwrap();
    let b = topology
        .contours(&[[0., 0.], [100., 200.], [30., 50.]])
        .unwrap();
    assert_eq!(a[0].segments.len(), 3);
    assert_eq!(a[0].identity, b[0].identity);
    assert_ne!(
        identity::node_id(&a[0], EditPoint::Anchor(0)),
        identity::node_id(&a[0], EditPoint::Anchor(2))
    );
    assert_eq!(
        identity::node_id(&a[0], EditPoint::Anchor(0)),
        identity::node_id(&a[0], EditPoint::Anchor(3))
    );
}

#[test]
fn physical_and_implied_node_moves_rebuild_consistent_quadratics_with_stable_ids() {
    let topology = Topology::new(vec![vec![false; 3]]).unwrap();
    let points = [[0., 0.], [100., 0.], [100., 100.]];
    let source = topology.contours(&points).unwrap();
    assert_eq!(
        topology.binding(0, EditPoint::Anchor(0)).unwrap(),
        Binding::Midpoint(2, 0)
    );
    let moved = topology
        .move_node(&source, 0, EditPoint::Anchor(0), [70., 80.])
        .unwrap();
    assert_eq!(
        topology.physical_points(&moved).unwrap(),
        [[20., 30.], [100., 0.], [120., 130.]]
    );
    assert_eq!(source[0].identity, moved[0].identity);
    let control = topology
        .move_node(&source, 0, EditPoint::Control1(0), [10., 20.])
        .unwrap();
    assert_eq!(control[0].start, [55., 60.]);
    assert_eq!(
        topology.physical_points(&control).unwrap(),
        [[10., 20.], [100., 0.], [100., 100.]]
    );
    let mut inconsistent = source.clone();
    if let Segment::Quadratic { control, .. } = &mut inconsistent[0].segments[0] {
        *control = [10., 20.];
    }
    assert!(topology.physical_points(&inconsistent).is_err());
    assert_eq!(topology.physical_points(&source).unwrap(), points);
}

#[test]
fn malformed_topologies_and_incompatible_edits_are_rejected_atomically() {
    assert!(Topology::new(vec![vec![]]).is_err());
    assert!(Topology::new(vec![vec![true; 65537]]).is_err());
    let topology = Topology::new(vec![vec![true; 2], vec![false; 2]]).unwrap();
    assert!(topology.contours(&[[0.; 2]; 3]).is_err());
    let source = topology.contours(&[[0.; 2]; 4]).unwrap();
    let mut changed = source.clone();
    changed[0].closed = false;
    assert!(topology.physical_points(&changed).is_err());
    let mut changed = source.clone();
    changed[0].identity = Default::default();
    assert!(topology.physical_points(&changed).is_err());
    assert!(topology
        .move_node(&source, 0, EditPoint::Anchor(0), [f64::NAN, 0.])
        .is_err());
    assert!(topology.binding(0, EditPoint::Control2(0)).is_err());
    let mut json = serde_json::to_value(&topology).unwrap();
    json["identities"][1] = json["identities"][0].clone();
    let invalid: Topology = serde_json::from_value(json).unwrap();
    assert!(invalid.contours(&[[0.; 2]; 4]).is_err());
    let empty = Topology::new(vec![]).unwrap();
    assert!(empty.contours(&[]).unwrap().is_empty());
}
