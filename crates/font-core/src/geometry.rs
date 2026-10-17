//! Font units use Y-up/f64. LumaPaint display paths use Y-down/f32.
use crate::{Contour, Error, Point, Result, Segment};

pub fn validate(contour: &Contour) -> Result<()> {
    fn point(p: Point) -> bool {
        p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.)
    }
    if !point(contour.start)
        || contour.segments.len() > 65536
        || contour.smooth.iter().any(|&i| {
            i > contour.segments.len()
                || canonical_anchor(contour, i) != i
                || !smooth_capable(contour, i)
        })
    {
        return Err(Error::InvalidGeometry);
    }
    for segment in &contour.segments {
        let valid = match *segment {
            Segment::Line { end } => point(end),
            Segment::Quadratic { control, end } => point(control) && point(end),
            Segment::Cubic {
                control1,
                control2,
                end,
            } => point(control1) && point(control2) && point(end),
        };
        if !valid {
            return Err(Error::InvalidGeometry);
        }
    }
    Ok(())
}

/// Display conversion only. Never replace the f64 source with this approximation.
/// Every contour remains separate, preserving holes and closure flags.
pub fn display_path(contours: &[Contour]) -> Result<String> {
    let mut result = String::new();
    for contour in contours {
        validate(contour)?;
        let map = |p: Point| [p[0] as f32, -p[1] as f32];
        let mut points = vec![map(contour.start)];
        let mut at = contour.start;
        for segment in &contour.segments {
            let (a, b, end) = match *segment {
                Segment::Line { end } => (at, end, end),
                Segment::Quadratic { control, end } => {
                    let lerp =
                        |p: Point| std::array::from_fn(|i| p[i] + (control[i] - p[i]) * 2. / 3.);
                    (lerp(at), lerp(end), end)
                }
                Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => (control1, control2, end),
            };
            points.extend([map(a), map(b), map(end)]);
            at = end;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(
            &lumapaint_core::bezier::path_data(&points, contour.closed)
                .map_err(|_| Error::InvalidGeometry)?,
        );
    }
    Ok(result)
}

/// A point address is valid for the duration of a revision-guarded drag.
/// Persistent node IDs remain a separate future document-format change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditPoint {
    Anchor(usize),
    Control1(usize),
    Control2(usize),
}
pub fn edit_points(c: &Contour) -> Vec<(EditPoint, Point)> {
    let mut points = vec![(EditPoint::Anchor(0), c.start)];
    for (i, segment) in c.segments.iter().enumerate() {
        let end = match *segment {
            Segment::Line { end } => end,
            Segment::Quadratic { control, end } => {
                points.push((EditPoint::Control1(i), control));
                end
            }
            Segment::Cubic {
                control1,
                control2,
                end,
            } => {
                points.push((EditPoint::Control1(i), control1));
                points.push((EditPoint::Control2(i), control2));
                end
            }
        };
        if !(c.closed && i + 1 == c.segments.len() && end == c.start) {
            points.push((EditPoint::Anchor(i + 1), end));
        }
    }
    points
}
fn endpoint(s: &mut Segment) -> &mut Point {
    match s {
        Segment::Line { end } | Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => end,
    }
}
/// Independent handles; moving a cubic anchor translates its attached handles.
/// A shared quadratic control stays fixed when an anchor moves.
pub fn move_point(c: &mut Contour, target: EditPoint, position: Point) -> Result<()> {
    let mut next = c.clone();
    match target {
        EditPoint::Control1(i) => match next.segments.get_mut(i) {
            Some(Segment::Quadratic { control, .. }) => *control = position,
            Some(Segment::Cubic { control1, .. }) => *control1 = position,
            _ => return Err(Error::InvalidGeometry),
        },
        EditPoint::Control2(i) => match next.segments.get_mut(i) {
            Some(Segment::Cubic { control2, .. }) => *control2 = position,
            _ => return Err(Error::InvalidGeometry),
        },
        EditPoint::Anchor(mut i) => {
            let count = next.segments.len();
            if i > count {
                return Err(Error::InvalidGeometry);
            }
            let joined = next.closed
                && next
                    .segments
                    .last_mut()
                    .is_some_and(|s| *endpoint(s) == next.start);
            if joined && i == count {
                i = 0;
            }
            let old = if i == 0 {
                next.start
            } else {
                *endpoint(&mut next.segments[i - 1])
            };
            let delta = [position[0] - old[0], position[1] - old[1]];
            if i == 0 {
                next.start = position;
            } else {
                *endpoint(&mut next.segments[i - 1]) = position;
            }
            if joined && i == 0 {
                *endpoint(next.segments.last_mut().unwrap()) = position;
            }
            let incoming = if i > 0 {
                Some(i - 1)
            } else if joined {
                Some(count - 1)
            } else {
                None
            };
            if let Some(index) = incoming {
                if let Segment::Cubic { control2, .. } = &mut next.segments[index] {
                    for axis in 0..2 {
                        control2[axis] += delta[axis];
                    }
                }
            }
            if let Some(Segment::Cubic { control1, .. }) = next.segments.get_mut(i) {
                for axis in 0..2 {
                    control1[axis] += delta[axis];
                }
            }
        }
    }
    if matches!(target, EditPoint::Control1(_) | EditPoint::Control2(_)) {
        couple_handle(&mut next, target);
    }
    validate(&next)?;
    *c = next;
    Ok(())
}

/// Split a segment at t=1/2 using de Casteljau subdivision. The locus is unchanged.
pub fn split_segment(c: &mut Contour, index: usize) -> Result<()> {
    validate(c)?;
    if c.segments.len() >= 65536 {
        return Err(Error::InvalidGeometry);
    }
    let start = if index == 0 {
        c.start
    } else {
        let mut previous = c
            .segments
            .get(index - 1)
            .ok_or(Error::InvalidGeometry)?
            .clone();
        *endpoint(&mut previous)
    };
    let mid = |a: Point, b: Point| std::array::from_fn(|i| (a[i] + b[i]) / 2.);
    let (left, right) = match *c.segments.get(index).ok_or(Error::InvalidGeometry)? {
        Segment::Line { end } => (
            Segment::Line {
                end: mid(start, end),
            },
            Segment::Line { end },
        ),
        Segment::Quadratic { control, end } => {
            let a = mid(start, control);
            let b = mid(control, end);
            let m = mid(a, b);
            (
                Segment::Quadratic { control: a, end: m },
                Segment::Quadratic { control: b, end },
            )
        }
        Segment::Cubic {
            control1,
            control2,
            end,
        } => {
            let a = mid(start, control1);
            let b = mid(control1, control2);
            let d = mid(control2, end);
            let e = mid(a, b);
            let f = mid(b, d);
            let m = mid(e, f);
            (
                Segment::Cubic {
                    control1: a,
                    control2: e,
                    end: m,
                },
                Segment::Cubic {
                    control1: f,
                    control2: d,
                    end,
                },
            )
        }
    };
    c.smooth = c
        .smooth
        .iter()
        .map(|&i| if i > index { i + 1 } else { i })
        .collect();
    if matches!(left, Segment::Cubic { .. }) {
        c.smooth.insert(index + 1);
    }
    c.segments.splice(index..=index, [left, right]);
    Ok(())
}

/// Remove the chosen segment's end anchor and its incoming segment.
/// The following segment keeps its type and handles, now starting at the preceding anchor.
/// Explicit duplicate closure endpoints are retained; the start anchor cannot be removed.
pub fn delete_segment_end(c: &mut Contour, index: usize) -> Result<()> {
    validate(c)?;
    if !can_delete_segment_end(c, index) {
        return Err(Error::InvalidGeometry);
    }
    c.smooth = c
        .smooth
        .iter()
        .filter(|&&i| i != index && i != index + 1)
        .map(|&i| if i > index { i - 1 } else { i })
        .collect();
    c.segments.remove(index);
    Ok(())
}

pub fn can_delete_segment_end(c: &Contour, index: usize) -> bool {
    let end = |s: &Segment| match *s {
        Segment::Line { end } | Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => end,
    };
    let joined = c.closed && c.segments.last().is_some_and(|s| end(s) == c.start);
    index < c.segments.len()
        && c.segments.len()
            > if c.closed {
                if joined {
                    3
                } else {
                    2
                }
            } else {
                1
            }
        && !(joined && index + 1 == c.segments.len())
}

#[cfg(test)]
mod topology_tests {
    use super::*;
    fn at(start: Point, segment: &Segment, t: f64) -> Point {
        let lerp = |a: Point, b: Point| std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t);
        match *segment {
            Segment::Line { end } => lerp(start, end),
            Segment::Quadratic { control, end } => lerp(lerp(start, control), lerp(control, end)),
            Segment::Cubic {
                control1,
                control2,
                end,
            } => lerp(
                lerp(lerp(start, control1), lerp(control1, control2)),
                lerp(lerp(control1, control2), lerp(control2, end)),
            ),
        }
    }
    #[test]
    fn subdivision_preserves_line_quadratic_and_cubic_at_sampled_parameters() {
        for segment in [
            Segment::Line { end: [600., 0.] },
            Segment::Quadratic {
                control: [300., 800.],
                end: [600., 0.],
            },
            Segment::Cubic {
                control1: [100., 700.],
                control2: [500., 600.],
                end: [600., 0.],
            },
        ] {
            let mut c = Contour {
                smooth: Default::default(),
                start: [10., 20.],
                segments: vec![segment.clone()],
                closed: false,
            };
            split_segment(&mut c, 0).unwrap();
            let middle = at(c.start, &c.segments[0], 1.);
            for n in 0..=100 {
                let t = n as f64 / 100.;
                let actual = if t <= 0.5 {
                    at(c.start, &c.segments[0], t * 2.)
                } else {
                    at(middle, &c.segments[1], (t - 0.5) * 2.)
                };
                let expected = at(c.start, &segment, t);
                assert!((actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 1e-9);
            }
        }
    }
    #[test]
    fn deletion_keeps_following_handles_and_protects_closure_and_minimum() {
        let mut c = Contour {
            smooth: Default::default(),
            start: [0., 0.],
            segments: vec![
                Segment::Line { end: [10., 0.] },
                Segment::Line { end: [20., 0.] },
                Segment::Cubic {
                    control1: [30., 10.],
                    control2: [40., 10.],
                    end: [50., 0.],
                },
                Segment::Line { end: [0., 0.] },
            ],
            closed: true,
        };
        let original = c.clone();
        assert!(delete_segment_end(&mut c, 3).is_err());
        assert_eq!(c, original);
        delete_segment_end(&mut c, 0).unwrap();
        assert_eq!(c.segments, original.segments[1..]);
        assert!(delete_segment_end(&mut c, 0).is_err());
        let before = c.clone();
        assert!(split_segment(&mut c, 99).is_err());
        assert_eq!(c, before);
        split_segment(&mut c, 2).unwrap();
        assert!(c.closed);
        assert_eq!(at([0., 0.], c.segments.last().unwrap(), 1.), c.start);
    }
}

fn end(s: &Segment) -> Point {
    match *s {
        Segment::Line { end } | Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => end,
    }
}
/// Canonical start anchor for explicitly joined closed contours.
pub fn canonical_anchor(c: &Contour, i: usize) -> usize {
    if c.closed && i == c.segments.len() && c.segments.last().is_some_and(|s| end(s) == c.start) {
        0
    } else {
        i
    }
}
fn neighbors(c: &Contour, i: usize) -> Option<(usize, usize, Point)> {
    let i = canonical_anchor(c, i);
    let incoming = if i > 0 {
        i - 1
    } else if c.closed && c.segments.last().is_some_and(|s| end(s) == c.start) {
        c.segments.len().checked_sub(1)?
    } else {
        return None;
    };
    (i < c.segments.len()).then(|| {
        (
            incoming,
            i,
            if i == 0 {
                c.start
            } else {
                end(&c.segments[i - 1])
            },
        )
    })
}
pub fn smooth_capable(c: &Contour, i: usize) -> bool {
    neighbors(c, i).is_some_and(|(a, b, _)| {
        matches!(c.segments[a], Segment::Cubic { .. })
            && matches!(c.segments[b], Segment::Cubic { .. })
    })
}
fn cubic(c: &mut Contour, i: usize) {
    let start = if i == 0 {
        c.start
    } else {
        end(&c.segments[i - 1])
    };
    let lerp = |a: Point, b: Point, t: f64| std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
    c.segments[i] = match c.segments[i] {
        Segment::Line { end } => Segment::Cubic {
            control1: lerp(start, end, 1. / 3.),
            control2: lerp(start, end, 2. / 3.),
            end,
        },
        Segment::Quadratic { control, end } => Segment::Cubic {
            control1: lerp(start, control, 2. / 3.),
            control2: lerp(end, control, 2. / 3.),
            end,
        },
        ref s => s.clone(),
    };
}
/// Smooth aligns handles while preserving each length. Corner releases coupling without moving points.
pub fn set_smooth(c: &mut Contour, anchor: usize, smooth: bool) -> Result<()> {
    validate(c)?;
    if anchor > c.segments.len() {
        return Err(Error::InvalidGeometry);
    }
    let mut next = c.clone();
    let anchor = canonical_anchor(&next, anchor);
    if !smooth {
        next.smooth.remove(&anchor);
        *c = next;
        return Ok(());
    }
    if next.closed && next.segments.last().is_some_and(|s| end(s) != next.start) {
        if next.segments.len() >= 65536 {
            return Err(Error::InvalidGeometry);
        }
        next.segments.push(Segment::Line { end: next.start });
    }
    let (a, b, p) = neighbors(&next, anchor).ok_or(Error::InvalidGeometry)?;
    cubic(&mut next, a);
    cubic(&mut next, b);
    let (u, v) = match (&next.segments[a], &next.segments[b]) {
        (Segment::Cubic { control2, .. }, Segment::Cubic { control1, .. }) => {
            (*control2, *control1)
        }
        _ => unreachable!(),
    };
    let incoming = [p[0] - u[0], p[1] - u[1]];
    let outgoing = [v[0] - p[0], v[1] - p[1]];
    let len = |v: Point| v[0].hypot(v[1]);
    let direction = if len(outgoing) > 1e-9 {
        outgoing
    } else {
        incoming
    };
    let d = len(direction);
    if d <= 1e-9 {
        return Err(Error::InvalidGeometry);
    }
    if let Segment::Cubic { control2, .. } = &mut next.segments[a] {
        *control2 = std::array::from_fn(|k| p[k] - direction[k] / d * len(incoming));
    }
    if let Segment::Cubic { control1, .. } = &mut next.segments[b] {
        *control1 = std::array::from_fn(|k| p[k] + direction[k] / d * len(outgoing));
    }
    next.smooth.insert(anchor);
    validate(&next)?;
    *c = next;
    Ok(())
}
fn couple_handle(c: &mut Contour, target: EditPoint) {
    let (anchor, outgoing) = match target {
        EditPoint::Control1(i) => (i, true),
        EditPoint::Control2(i) => (canonical_anchor(c, i + 1), false),
        _ => return,
    };
    if !c.smooth.contains(&anchor) {
        return;
    }
    let Some((a, b, p)) = neighbors(c, anchor) else {
        return;
    };
    let (u, v) = match (&c.segments[a], &c.segments[b]) {
        (Segment::Cubic { control2, .. }, Segment::Cubic { control1, .. }) => {
            (*control2, *control1)
        }
        _ => return,
    };
    let (moved, other) = if outgoing { (v, u) } else { (u, v) };
    let d = [moved[0] - p[0], moved[1] - p[1]];
    let length = d[0].hypot(d[1]);
    if length <= 1e-9 {
        return;
    }
    let opposite = (other[0] - p[0]).hypot(other[1] - p[1]);
    let q = std::array::from_fn(|k| p[k] - d[k] / length * opposite);
    if outgoing {
        if let Segment::Cubic { control2, .. } = &mut c.segments[a] {
            *control2 = q;
        }
    } else if let Segment::Cubic { control1, .. } = &mut c.segments[b] {
        *control1 = q;
    }
}

#[cfg(test)]
mod smooth_tests {
    use super::*;
    fn shape() -> Contour {
        Contour {
            smooth: Default::default(),
            start: [0., 0.],
            segments: vec![
                Segment::Quadratic {
                    control: [40., 80.],
                    end: [100., 0.],
                },
                Segment::Cubic {
                    control1: [140., 60.],
                    control2: [180., 40.],
                    end: [200., 0.],
                },
            ],
            closed: false,
        }
    }
    fn assert_tangent(c: &Contour) {
        let Segment::Cubic {
            control2: u,
            end: p,
            ..
        } = c.segments[0]
        else {
            panic!()
        };
        let Segment::Cubic { control1: v, .. } = c.segments[1] else {
            panic!()
        };
        let a = [u[0] - p[0], u[1] - p[1]];
        let b = [v[0] - p[0], v[1] - p[1]];
        assert!((a[0] * b[1] - a[1] * b[0]).abs() < 1e-8);
        assert!(a[0] * b[0] + a[1] * b[1] <= 0.);
    }
    #[test]
    fn smooth_promotes_quadratic_and_couples_both_handles_corner_releases() {
        let mut c = shape();
        set_smooth(&mut c, 1, true).unwrap();
        assert_tangent(&c);
        assert!(c.smooth.contains(&1));
        let Segment::Cubic { control2: u, .. } = c.segments[0] else {
            panic!()
        };
        let old_length = (u[0] - 100.).hypot(u[1]);
        move_point(&mut c, EditPoint::Control1(1), [160., 90.]).unwrap();
        assert_tangent(&c);
        let Segment::Cubic { control2: u, .. } = c.segments[0] else {
            panic!()
        };
        assert!(((u[0] - 100.).hypot(u[1]) - old_length).abs() < 1e-8);
        move_point(&mut c, EditPoint::Control2(0), [50., -10.]).unwrap();
        assert_tangent(&c);
        move_point(&mut c, EditPoint::Anchor(1), [110., 20.]).unwrap();
        let before = c.segments.clone();
        set_smooth(&mut c, 1, false).unwrap();
        assert_eq!(c.segments, before);
        move_point(&mut c, EditPoint::Control1(1), [140., 130.]).unwrap();
        if let (Segment::Cubic { control2: a, .. }, Segment::Cubic { control2: b, .. }) =
            (&before[0], &c.segments[0])
        {
            assert_eq!(a, b)
        } else {
            panic!()
        }
        let before = c.clone();
        assert!(set_smooth(&mut c, 0, true).is_err());
        assert_eq!(c, before);
    }
    #[test]
    fn implicit_closure_start_modes_and_legacy_project_roundtrip() {
        let mut c = shape();
        c.closed = true;
        set_smooth(&mut c, 0, true).unwrap();
        assert_eq!(c.segments.len(), 3);
        assert!(c.smooth.contains(&0));
        move_point(&mut c, EditPoint::Control2(2), [-50., -50.]).unwrap();
        assert_eq!(end(c.segments.last().unwrap()), c.start);
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Contour>(&json).unwrap(), c);
        let old = r#"{"start":[0,0],"segments":[{"kind":"line","end":[1,1]}],"closed":false}"#;
        assert!(serde_json::from_str::<Contour>(old)
            .unwrap()
            .smooth
            .is_empty());
        split_segment(&mut c, 0).unwrap();
        validate(&c).unwrap();
        delete_segment_end(&mut c, 0).unwrap();
        validate(&c).unwrap();
        c.smooth.insert(999);
        assert!(validate(&c).is_err());
    }
}
