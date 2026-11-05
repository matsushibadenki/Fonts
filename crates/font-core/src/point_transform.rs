//! Atomic transforms of mixed anchor/control selections; each coordinate is transformed once.
use crate::{
    geometry::{self, EditPoint},
    Contour, Error, Point, Result, Segment,
};
use std::collections::BTreeSet;
pub type Selection = BTreeSet<(usize, EditPoint)>;

pub fn transform(
    contours: &mut Vec<Contour>,
    selection: &Selection,
    scale: Point,
    angle: f64,
    delta: Point,
) -> Result<()> {
    if selection.is_empty()
        || !scale
            .into_iter()
            .chain(delta)
            .chain([angle])
            .all(f64::is_finite)
    {
        return Err(Error::InvalidGeometry);
    }
    let mut selected = Selection::new();
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for &(ci, mut p) in selection {
        let c = contours.get(ci).ok_or(Error::InvalidGeometry)?;
        if let EditPoint::Anchor(i) = p {
            p = EditPoint::Anchor(geometry::canonical_anchor(c, i));
        }
        let q = geometry::point_position(c, p).ok_or(Error::InvalidGeometry)?;
        for k in 0..2 {
            min[k] = min[k].min(q[k]);
            max[k] = max[k].max(q[k]);
        }
        selected.insert((ci, p));
    }
    let pivot = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
    let (sin, cos) = angle.to_radians().sin_cos();
    let apply = |p: &mut Point| {
        let x = (p[0] - pivot[0]) * scale[0];
        let y = (p[1] - pivot[1]) * scale[1];
        *p = [
            pivot[0] + x * cos - y * sin + delta[0],
            pivot[1] + x * sin + y * cos + delta[1],
        ];
    };
    let mut next = contours.clone();
    for (ci, (old, c)) in contours.iter().zip(&mut next).enumerate() {
        geometry::validate(old)?;
        if selected
            .range((ci, EditPoint::Anchor(0))..=(ci, EditPoint::Control2(usize::MAX)))
            .next()
            .is_none()
        {
            continue;
        }
        let anchor =
            |i| selected.contains(&(ci, EditPoint::Anchor(geometry::canonical_anchor(old, i))));
        if anchor(0) {
            apply(&mut c.start);
        }
        for (i, s) in c.segments.iter_mut().enumerate() {
            let a = anchor(i);
            let b = anchor(i + 1);
            let c1 = selected.contains(&(ci, EditPoint::Control1(i)));
            let c2 = selected.contains(&(ci, EditPoint::Control2(i)));
            match s {
                Segment::Line { end } => {
                    if b {
                        apply(end);
                    }
                }
                Segment::Quadratic { control, end } => {
                    if c1 || (a && b) {
                        apply(control);
                    }
                    if b {
                        apply(end);
                    }
                }
                Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => {
                    if a || c1 {
                        apply(control1);
                    }
                    if b || c2 {
                        apply(control2);
                    }
                    if b {
                        apply(end);
                    }
                }
            }
        }
        // Explicit selection wins over coupling; retain smooth only where tangents still agree.
        c.smooth = c
            .smooth
            .iter()
            .copied()
            .filter(|&i| {
                let incoming = if i > 0 {
                    Some(i - 1)
                } else if c.closed
                    && c.segments.last().is_some_and(|s| match s {
                        Segment::Line { end }
                        | Segment::Quadratic { end, .. }
                        | Segment::Cubic { end, .. } => *end == c.start,
                    })
                {
                    c.segments.len().checked_sub(1)
                } else {
                    None
                };
                let Some(j) = incoming else {
                    return false;
                };
                let (Some(p), Some(a), Some(b)) = (
                    geometry::point_position(c, EditPoint::Anchor(i)),
                    geometry::point_position(c, EditPoint::Control2(j)),
                    geometry::point_position(c, EditPoint::Control1(i)),
                ) else {
                    return false;
                };
                if [
                    EditPoint::Anchor(i),
                    EditPoint::Control2(j),
                    EditPoint::Control1(i),
                ]
                .into_iter()
                .all(|point| {
                    geometry::point_position(c, point) == geometry::point_position(old, point)
                }) {
                    return true;
                }
                let u = [a[0] - p[0], a[1] - p[1]];
                let v = [b[0] - p[0], b[1] - p[1]];
                let lengths = u[0].hypot(u[1]) * v[0].hypot(v[1]);
                (u[0] * v[1] - u[1] * v[0]).abs() <= 1e-8 * lengths.max(1.)
                    && u[0] * v[0] + u[1] * v[1] <= 1e-8
            })
            .collect();
        geometry::validate(c)?;
    }
    *contours = next;
    Ok(())
}
