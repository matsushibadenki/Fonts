//! Transactional contour operations in font units.
use crate::{geometry, Contour, Result, Segment};

fn end(s: &Segment) -> crate::Point {
    match *s {
        Segment::Line { end } | Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => end,
    }
}

/// Reverse traversal while keeping closed contours' start position and exact curves.
pub fn reverse(c: &mut Contour) -> Result<()> {
    geometry::validate(c)?;
    let mut next = c.clone();
    if next.closed && next.segments.last().is_some_and(|s| end(s) != next.start) {
        next.segments.push(Segment::Line { end: next.start });
    }
    let mut at = next.start;
    let mut reversed = Vec::with_capacity(next.segments.len());
    for s in &next.segments {
        reversed.push(match *s {
            Segment::Line { .. } => Segment::Line { end: at },
            Segment::Quadratic { control, .. } => Segment::Quadratic { control, end: at },
            Segment::Cubic {
                control1, control2, ..
            } => Segment::Cubic {
                control1: control2,
                control2: control1,
                end: at,
            },
        });
        at = end(s);
    }
    let n = reversed.len();
    next.start = at;
    reversed.reverse();
    next.segments = reversed;
    next.smooth = c
        .smooth
        .iter()
        .map(|&i| if c.closed && i == 0 { 0 } else { n - i })
        .collect();
    geometry::validate(&next)?;
    *c = next;
    Ok(())
}

/// Toggle closure without discarding any curve or control point.
pub fn set_closed(c: &mut Contour, closed: bool) -> Result<()> {
    let mut next = c.clone();
    next.closed = closed;
    next.smooth = next
        .smooth
        .iter()
        .copied()
        .filter(|&i| geometry::smooth_capable(&next, i))
        .collect();
    geometry::validate(&next)?;
    *c = next;
    Ok(())
}

pub fn straighten(c: &mut Contour, segment: usize) -> Result<()> {
    let mut next = c.clone();
    let s = next
        .segments
        .get_mut(segment)
        .ok_or(crate::Error::InvalidGeometry)?;
    *s = Segment::Line { end: end(s) };
    let a = geometry::canonical_anchor(&next, segment);
    let b = geometry::canonical_anchor(&next, segment + 1);
    next.smooth.remove(&a);
    next.smooth.remove(&b);
    geometry::validate(&next)?;
    *c = next;
    Ok(())
}

/// Join selected endpoints, reversing only traversal when necessary. No source curve is flattened.
pub fn join(
    first: &Contour,
    first_start: bool,
    second: &Contour,
    second_end: bool,
) -> Result<Contour> {
    geometry::validate(first)?;
    geometry::validate(second)?;
    if first.closed || second.closed || first.segments.is_empty() || second.segments.is_empty() {
        return Err(crate::Error::InvalidGeometry);
    }
    let mut a = first.clone();
    let mut b = second.clone();
    if first_start {
        reverse(&mut a)?;
    }
    if second_end {
        reverse(&mut b)?;
    }
    if end(a.segments.last().unwrap()) != b.start {
        a.segments.push(Segment::Line { end: b.start });
    }
    let offset = a.segments.len();
    a.smooth.extend(b.smooth.iter().map(|i| i + offset));
    a.segments.extend(b.segments);
    geometry::validate(&a)?;
    Ok(a)
}

/// Break tangent coupling without moving the opposite handle or changing the curve type.
pub fn move_independent(
    c: &mut Contour,
    target: geometry::EditPoint,
    position: crate::Point,
) -> Result<()> {
    let mut next = c.clone();
    let anchor = match target {
        geometry::EditPoint::Control1(i) => i,
        geometry::EditPoint::Control2(i) => i + 1,
        geometry::EditPoint::Anchor(_) => return geometry::move_point(c, target, position),
    };
    let anchor = geometry::canonical_anchor(&next, anchor);
    next.smooth.remove(&anchor);
    geometry::move_point(&mut next, target, position)?;
    *c = next;
    Ok(())
}

/// Cut at an anchor: open contours produce two paths; closed contours open without losing an edge.
pub fn split(c: &Contour, anchor: usize) -> Result<Vec<Contour>> {
    geometry::validate(c)?;
    if anchor > c.segments.len() || c.segments.is_empty() {
        return Err(crate::Error::InvalidGeometry);
    }
    let anchor = geometry::canonical_anchor(c, anchor);
    if c.closed {
        let mut next = c.clone();
        if end(next.segments.last().unwrap()) != next.start {
            next.segments.push(Segment::Line { end: next.start });
        }
        let n = next.segments.len();
        next.start = if anchor == 0 {
            c.start
        } else {
            end(&c.segments[anchor - 1])
        };
        next.segments.rotate_left(anchor);
        next.closed = false;
        next.smooth = c
            .smooth
            .iter()
            .filter(|&&i| i != anchor)
            .map(|&i| (i + n - anchor) % n)
            .collect();
        geometry::validate(&next)?;
        Ok(vec![next])
    } else {
        if anchor == 0 || anchor == c.segments.len() {
            return Err(crate::Error::InvalidGeometry);
        }
        let mut left = c.clone();
        left.segments.truncate(anchor);
        left.smooth.retain(|&i| i < anchor);
        let mut right = c.clone();
        right.start = end(&c.segments[anchor - 1]);
        right.segments = c.segments[anchor..].to_vec();
        right.smooth = c
            .smooth
            .iter()
            .filter(|&&i| i > anchor)
            .map(|i| i - anchor)
            .collect();
        geometry::validate(&left)?;
        geometry::validate(&right)?;
        Ok(vec![left, right])
    }
}
pub fn move_anchors(
    contours: &mut Vec<Contour>,
    anchors: &std::collections::BTreeSet<(usize, usize)>,
    delta: crate::Point,
) -> Result<()> {
    if anchors.is_empty() || !delta.iter().all(|v| v.is_finite()) {
        return Err(crate::Error::InvalidGeometry);
    }
    for c in contours.iter() {
        geometry::validate(c)?;
    }
    let mut selected = std::collections::BTreeSet::new();
    for &(ci, i) in anchors {
        let c = contours.get(ci).ok_or(crate::Error::InvalidGeometry)?;
        if i > c.segments.len() {
            return Err(crate::Error::InvalidGeometry);
        }
        selected.insert((ci, geometry::canonical_anchor(c, i)));
    }
    let shift = |p: &mut crate::Point| {
        p[0] += delta[0];
        p[1] += delta[1];
    };
    let mut next = contours.clone();
    for (ci, (original, c)) in contours.iter().zip(next.iter_mut()).enumerate() {
        if selected.contains(&(ci, 0)) {
            shift(&mut c.start);
        }
        for (i, s) in c.segments.iter_mut().enumerate() {
            let a = selected.contains(&(ci, geometry::canonical_anchor(original, i)));
            let b = selected.contains(&(ci, geometry::canonical_anchor(original, i + 1)));
            match s {
                Segment::Line { end } => {
                    if b {
                        shift(end);
                    }
                }
                Segment::Quadratic { control, end } => {
                    if a && b {
                        shift(control);
                    }
                    if b {
                        shift(end);
                    }
                }
                Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => {
                    if a {
                        shift(control1);
                    }
                    if b {
                        shift(control2);
                        shift(end);
                    }
                }
            }
        }
        geometry::validate(c)?;
    }
    *contours = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mixed(closed: bool) -> Contour {
        Contour {
            start: [0., 0.],
            segments: vec![
                Segment::Line { end: [100., 0.] },
                Segment::Quadratic {
                    control: [180., 40.],
                    end: [100., 100.],
                },
                Segment::Cubic {
                    control1: [80., 140.],
                    control2: [-40., 80.],
                    end: [0., 0.],
                },
            ],
            closed,
            smooth: Default::default(),
        }
    }
    #[test]
    fn reverse_exactly_swaps_cubic_handles_and_preserves_quadratic() {
        for closed in [false, true] {
            let original = mixed(closed);
            let mut c = original.clone();
            reverse(&mut c).unwrap();
            assert_eq!(
                c.segments[0],
                Segment::Cubic {
                    control1: [-40., 80.],
                    control2: [80., 140.],
                    end: [100., 100.]
                }
            );
            assert_eq!(
                c.segments[1],
                Segment::Quadratic {
                    control: [180., 40.],
                    end: [100., 0.]
                }
            );
            reverse(&mut c).unwrap();
            assert_eq!(c, original);
        }
        let mut c = mixed(true);
        c.segments.pop();
        reverse(&mut c).unwrap();
        assert_eq!(c.start, [0., 0.]);
        assert_eq!(c.segments[0], Segment::Line { end: [100., 100.] });
    }
    #[test]
    fn open_close_preserves_curves_and_straighten_is_transactional() {
        let original = mixed(true);
        let mut c = original.clone();
        set_closed(&mut c, false).unwrap();
        assert!(!c.closed);
        assert_eq!(c.segments, original.segments);
        set_closed(&mut c, true).unwrap();
        assert_eq!(c, original);
        assert!(straighten(&mut c, 99).is_err());
        assert_eq!(c, original);
        straighten(&mut c, 1).unwrap();
        assert_eq!(c.segments[1], Segment::Line { end: [100., 100.] });
    }
    #[test]
    fn reverse_and_open_remap_coupled_handles() {
        let mut c = mixed(true);
        geometry::set_smooth(&mut c, 1, true).unwrap();
        geometry::set_smooth(&mut c, 0, true).unwrap();
        let original = c.clone();
        reverse(&mut c).unwrap();
        assert!(c.smooth.contains(&2));
        assert!(c.smooth.contains(&0));
        reverse(&mut c).unwrap();
        assert_eq!(c, original);
        set_closed(&mut c, false).unwrap();
        assert!(!c.smooth.contains(&0));
        geometry::validate(&c).unwrap();
    }
    #[test]
    fn join_all_endpoint_orientations_preserves_curves_and_adds_only_connector() {
        let a = mixed(false);
        let mut b = mixed(false);
        b.start = [400., 0.];
        for start in [false, true] {
            for end in [false, true] {
                let mut expected_a = a.clone();
                let mut expected_b = b.clone();
                if start {
                    reverse(&mut expected_a).unwrap();
                }
                if end {
                    reverse(&mut expected_b).unwrap();
                }
                let c = join(&a, start, &b, end).unwrap();
                assert_eq!(c.start, expected_a.start);
                assert_eq!(&c.segments[..a.segments.len()], &expected_a.segments);
                assert_eq!(
                    &c.segments[c.segments.len() - b.segments.len()..],
                    &expected_b.segments
                );
                geometry::validate(&c).unwrap();
            }
        }
        let joined = join(&a, false, &a, false).unwrap();
        assert_eq!(joined.segments.len(), a.segments.len() * 2);
        let mut closed = a.clone();
        closed.closed = true;
        assert!(join(&a, false, &closed, false).is_err());
    }
    #[test]
    fn independent_handle_preserves_opposite_and_invalid_moves_are_atomic() {
        let mut c = mixed(true);
        geometry::set_smooth(&mut c, 1, true).unwrap();
        let original = c.clone();
        let opposite = match c.segments[1] {
            Segment::Cubic { control1, .. } => control1,
            _ => panic!(),
        };
        move_independent(&mut c, geometry::EditPoint::Control2(0), [80., 40.]).unwrap();
        assert!(!c.smooth.contains(&1));
        match c.segments[1] {
            Segment::Cubic { control1, .. } => assert_eq!(control1, opposite),
            _ => panic!(),
        }
        let changed = c.clone();
        assert!(
            move_independent(&mut c, geometry::EditPoint::Control2(0), [f64::NAN, 0.]).is_err()
        );
        assert_eq!(c, changed);
        let mut closed = original;
        geometry::set_smooth(&mut closed, 0, true).unwrap();
        let n = closed.segments.len();
        move_independent(
            &mut closed,
            geometry::EditPoint::Control2(n - 1),
            [20., 30.],
        )
        .unwrap();
        assert!(!closed.smooth.contains(&0));
    }
    #[test]
    fn cut_preserves_every_curve_and_remaps_smooth_points() {
        let mut c = mixed(false);
        geometry::set_smooth(&mut c, 1, true).unwrap();
        let parts = split(&c, 1).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].segments, c.segments[..1]);
        assert_eq!(parts[1].segments, c.segments[1..]);
        assert!(parts.iter().all(|c| !c.closed && !c.smooth.contains(&0)));
        assert!(split(&c, 0).is_err());
        let mut closed = mixed(true);
        geometry::set_smooth(&mut closed, 1, true).unwrap();
        let open = split(&closed, 2).unwrap().remove(0);
        assert!(!open.closed);
        assert_eq!(open.start, [100., 100.]);
        assert_eq!(open.segments.len(), closed.segments.len());
        assert_eq!(open.segments[0], closed.segments[2]);
        geometry::validate(&open).unwrap();
    }
    #[test]
    fn batch_move_translates_shared_handles_once_and_is_atomic() {
        let mut c = mixed(true);
        geometry::set_smooth(&mut c, 1, true).unwrap();
        let original = vec![c.clone()];
        let mut all = original.clone();
        let anchors = [(0, 0), (0, 1), (0, 2), (0, 3)].into_iter().collect();
        move_anchors(&mut all, &anchors, [10., 20.]).unwrap();
        assert_eq!(all[0].start, [10., 20.]);
        for (p, q) in geometry::edit_points(&c)
            .into_iter()
            .zip(geometry::edit_points(&all[0]))
        {
            assert_eq!(p.0, q.0);
            assert_eq!(q.1, [p.1[0] + 10., p.1[1] + 20.]);
        }
        let before = all.clone();
        assert!(move_anchors(&mut all, &anchors, [f64::NAN, 0.]).is_err());
        assert_eq!(all, before);
        assert!(move_anchors(&mut all, &[(9, 0)].into_iter().collect(), [1., 0.]).is_err());
        assert_eq!(all, before);
        let mut q = vec![Contour {
            start: [0., 0.],
            segments: vec![Segment::Quadratic {
                control: [50., 100.],
                end: [100., 0.],
            }],
            closed: false,
            smooth: Default::default(),
        }];
        move_anchors(&mut q, &[(0, 0), (0, 1)].into_iter().collect(), [20., 30.]).unwrap();
        assert_eq!(
            q[0].segments[0],
            Segment::Quadratic {
                control: [70., 130.],
                end: [120., 30.]
            }
        );
    }
}
