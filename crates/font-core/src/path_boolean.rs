//! Bounded filled-area operations. Curves are explicitly flattened at 0.01 font units.
use crate::{geometry, Contour, Error, Result, Segment};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::single::SingleFloatOverlay,
};
type Point = [f64; 2];
const LIMIT: usize = 8192;
fn flatten(c: [Point; 4], depth: usize, out: &mut Vec<Point>, work: &mut usize) -> Result<()> {
    let a = c[0];
    let b = c[3];
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0] * d[0] + d[1] * d[1];
    let flat = c[1..3].iter().all(|p| {
        let t = if len == 0. {
            0.
        } else {
            ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len
        }
        .clamp(0., 1.);
        (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1]) <= 0.01
    });
    if flat {
        *work += 1;
        if *work > LIMIT {
            return Err(Error::TooLarge);
        }
        out.push(b);
        return Ok(());
    }
    if depth >= 24 {
        return Err(Error::TooLarge);
    }
    let mid = |a: Point, b: Point| [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
    let p = mid(c[0], c[1]);
    let q = mid(c[1], c[2]);
    let r = mid(c[2], c[3]);
    let s = mid(p, q);
    let t = mid(q, r);
    let u = mid(s, t);
    flatten([c[0], p, s, u], depth + 1, out, work)?;
    flatten([u, t, r, c[3]], depth + 1, out, work)
}
fn polygons(contours: &[Contour], work: &mut usize) -> Result<Vec<Vec<Point>>> {
    let mut result = Vec::new();
    for c in contours {
        geometry::validate(c)?;
        if !c.closed || c.segments.is_empty() {
            return Err(Error::InvalidGeometry);
        }
        *work += 1;
        if *work > LIMIT {
            return Err(Error::TooLarge);
        }
        let mut points = vec![c.start];
        let mut at = c.start;
        for s in &c.segments {
            let (a, b, end) = match *s {
                Segment::Line { end } => {
                    *work += 1;
                    if *work > LIMIT {
                        return Err(Error::TooLarge);
                    }
                    points.push(end);
                    at = end;
                    continue;
                }
                Segment::Quadratic { control, end } => {
                    let blend =
                        |p: Point| std::array::from_fn(|i| p[i] + (control[i] - p[i]) * 2. / 3.);
                    (blend(at), blend(end), end)
                }
                Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => (control1, control2, end),
            };
            flatten([at, a, b, end], 0, &mut points, work)?;
            at = end;
        }
        if points.last() == points.first() {
            points.pop();
        }
        if points.len() < 3 {
            return Err(Error::InvalidGeometry);
        }
        result.push(points);
    }
    Ok(result)
}
pub fn combine(left: &[Contour], right: &[Contour], operation: &str) -> Result<Vec<Contour>> {
    if left.is_empty() || right.is_empty() {
        return Err(Error::InvalidGeometry);
    }
    let rule = match operation {
        "union" => OverlayRule::Union,
        "difference" => OverlayRule::Difference,
        "intersection" => OverlayRule::Intersect,
        "xor" => OverlayRule::Xor,
        _ => return Err(Error::InvalidGeometry),
    };
    let mut work = 0;
    let a = polygons(left, &mut work)?;
    let b = polygons(right, &mut work)?;
    // Bound potential intersection growth before the overlay library allocates its graph.
    let mut bounds = Vec::new();
    for polygon in a.iter().chain(&b) {
        for (p, q) in polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
        {
            bounds.push([
                p[0].min(q[0]),
                p[1].min(q[1]),
                p[0].max(q[0]),
                p[1].max(q[1]),
            ]);
        }
    }
    bounds.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut attempts = 0;
    let mut candidates = 0;
    for (i, first) in bounds.iter().enumerate() {
        for second in &bounds[i + 1..] {
            if second[0] > first[2] {
                break;
            }
            attempts += 1;
            if attempts > 2_000_000 {
                return Err(Error::TooLarge);
            }
            if first[1] <= second[3] && second[1] <= first[3] {
                candidates += 1;
                if candidates > 100_000 {
                    return Err(Error::TooLarge);
                }
            }
        }
    }
    let shapes = a.overlay_as::<i64>(&b, rule, FillRule::NonZero);
    let mut result = Vec::new();
    let mut count = 0;
    for points in shapes.into_iter().flatten() {
        count += points.len();
        if count > 32768 {
            return Err(Error::TooLarge);
        }
        if points.len() < 3 {
            continue;
        }
        let c = Contour {
            start: points[0],
            segments: points[1..]
                .iter()
                .map(|&end| Segment::Line { end })
                .collect(),
            closed: true,
            smooth: Default::default(),
        };
        geometry::validate(&c)?;
        result.push(c);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f64, y: f64, w: f64, h: f64) -> Contour {
        Contour {
            start: [x, y],
            segments: vec![
                Segment::Line { end: [x + w, y] },
                Segment::Line {
                    end: [x + w, y + h],
                },
                Segment::Line { end: [x, y + h] },
            ],
            closed: true,
            smooth: Default::default(),
        }
    }
    fn area(cs: &[Contour]) -> f64 {
        cs.iter()
            .map(|c| {
                let mut at = c.start;
                let mut sum = 0.;
                for s in &c.segments {
                    let Segment::Line { end } = s else { panic!() };
                    sum += at[0] * end[1] - end[0] * at[1];
                    at = *end;
                }
                sum += at[0] * c.start[1] - c.start[0] * at[1];
                sum / 2.
            })
            .sum::<f64>()
            .abs()
    }
    #[test]
    fn rectangle_operations_and_empty_results_have_expected_areas() {
        let a = rect(0., 0., 100., 100.);
        let b = rect(50., 0., 100., 100.);
        for (op, expected) in [
            ("union", 15000.),
            ("difference", 5000.),
            ("intersection", 5000.),
            ("xor", 10000.),
        ] {
            let c = combine(std::slice::from_ref(&a), std::slice::from_ref(&b), op).unwrap();
            assert!((area(&c) - expected).abs() < 0.001, "{op}");
        }
        assert!(combine(
            std::slice::from_ref(&a),
            std::slice::from_ref(&a),
            "difference"
        )
        .unwrap()
        .is_empty());
        assert!(combine(&[a], &[rect(500., 0., 10., 10.)], "intersection")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn nonzero_holes_are_preserved_and_disjoint_shapes_are_separate() {
        let outer = rect(0., 0., 100., 100.);
        let mut hole = rect(25., 25., 50., 50.);
        crate::path_edit::reverse(&mut hole).unwrap();
        let c = combine(&[outer, hole], &[rect(200., 0., 10., 10.)], "union").unwrap();
        assert_eq!(c.len(), 3);
        assert!((area(&c) - 7600.).abs() < 0.001);
    }
    #[test]
    fn curves_are_bounded_and_invalid_open_or_excessive_inputs_fail() {
        let mut c = rect(0., 0., 100., 100.);
        c.segments[0] = Segment::Cubic {
            control1: [20., -40.],
            control2: [80., -40.],
            end: [100., 0.],
        };
        let out = combine(
            std::slice::from_ref(&c),
            &[rect(200., 0., 10., 10.)],
            "union",
        )
        .unwrap();
        assert!(out.iter().any(|c| c.segments.len() > 10));
        let mut open = c.clone();
        open.closed = false;
        assert!(combine(&[open], std::slice::from_ref(&c), "union").is_err());
        let overlaps = vec![rect(0., 0., 100., 100.); 512];
        assert_eq!(
            combine(&overlaps, &[rect(200., 0., 10., 10.)], "union"),
            Err(Error::TooLarge)
        );
        let huge = vec![c.clone(); 2000];
        assert_eq!(combine(&huge, &[c], "union"), Err(Error::TooLarge));
    }
}
