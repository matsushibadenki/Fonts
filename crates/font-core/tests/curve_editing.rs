use font_core::{
    geometry::{self, EditPoint},
    Contour, Segment,
};
fn contour() -> Contour {
    Contour {
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![
            Segment::Cubic {
                control1: [10., 100.],
                control2: [90., 100.],
                end: [100., 0.],
            },
            Segment::Cubic {
                control1: [90., -100.],
                control2: [10., -100.],
                end: [0., 0.],
            },
        ],
        closed: true,
    }
}
#[test]
fn cubic_handles_are_independent_and_joined_anchor_translates_attached_handles() {
    let mut c = contour();
    assert_eq!(geometry::edit_points(&c).len(), 6);
    geometry::move_point(&mut c, EditPoint::Control1(0), [20., 120.]).unwrap();
    assert_eq!(
        c.segments[0],
        Segment::Cubic {
            control1: [20., 120.],
            control2: [90., 100.],
            end: [100., 0.]
        }
    );
    geometry::move_point(&mut c, EditPoint::Anchor(0), [5., 7.]).unwrap();
    assert_eq!(c.start, [5., 7.]);
    assert_eq!(
        c.segments[0],
        Segment::Cubic {
            control1: [25., 127.],
            control2: [90., 100.],
            end: [100., 0.]
        }
    );
    assert_eq!(
        c.segments[1],
        Segment::Cubic {
            control1: [90., -100.],
            control2: [15., -93.],
            end: [5., 7.]
        }
    );
    geometry::move_point(&mut c, EditPoint::Anchor(1), [110., 10.]).unwrap();
    assert_eq!(
        c.segments[0],
        Segment::Cubic {
            control1: [25., 127.],
            control2: [100., 110.],
            end: [110., 10.]
        }
    );
    assert_eq!(
        c.segments[1],
        Segment::Cubic {
            control1: [100., -90.],
            control2: [15., -93.],
            end: [5., 7.]
        }
    );
}
#[test]
fn quadratic_handles_and_invalid_moves_preserve_other_points() {
    let mut c = Contour {
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![Segment::Quadratic {
            control: [40., 80.],
            end: [100., 0.],
        }],
        closed: false,
    };
    geometry::move_point(&mut c, EditPoint::Control1(0), [50., 90.]).unwrap();
    geometry::move_point(&mut c, EditPoint::Anchor(1), [110., 0.]).unwrap();
    assert_eq!(
        c.segments[0],
        Segment::Quadratic {
            control: [50., 90.],
            end: [110., 0.]
        }
    );
    let before = c.clone();
    for (target, p) in [
        (EditPoint::Control2(0), [1., 2.]),
        (EditPoint::Anchor(9), [1., 2.]),
        (EditPoint::Control1(0), [f64::NAN, 0.]),
        (EditPoint::Control1(0), [1e10, 0.]),
    ] {
        assert!(geometry::move_point(&mut c, target, p).is_err());
        assert_eq!(c, before);
    }
}
