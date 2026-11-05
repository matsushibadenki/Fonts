//! Bounded nonzero filled-area operations with original Bézier boundary reconstruction.
mod curves;
use crate::{geometry, Contour, Error, Result, Segment};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::overlay::{FloatOverlay, OverlayOptions},
};
type Point = [f64; 2];
const LIMIT: usize = 8192;
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
    let mut sources = curves::Sources::new();
    let (a, b) = sources.polygons(left, right)?;
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
    let mut options = OverlayOptions::<f64, i64>::default();
    options.preserve_input_collinear = true;
    options.preserve_output_collinear = true;
    options.clean_result = false;
    let shapes =
        FloatOverlay::<Point, i64>::from_subj_and_clip_custom(&a, &b, options, Default::default())
            .overlay(rule, FillRule::NonZero);
    sources.restore(shapes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f64, y: f64, w: f64, h: f64) -> Contour {
        Contour {
            identity: Default::default(),
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
        assert!(out.iter().any(|c| c
            .segments
            .iter()
            .any(|s| matches!(s, Segment::Cubic { .. }))));
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
