use font_core::{
    geometry, path_boolean::combine, path_edit, storage, Contour, Editor, FontDocument, Glyph,
    Segment,
};
type P = [f64; 2];
fn rect(x: f64, y: f64, w: f64, h: f64) -> Contour {
    Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [x, y],
        segments: vec![
            Segment::Line { end: [x + w, y] },
            Segment::Line {
                end: [x + w, y + h],
            },
            Segment::Line { end: [x, y + h] },
        ],
        closed: true,
    }
}
fn circle(x: f64, y: f64, r: f64) -> Contour {
    let k = r * 0.5522847498307936;
    Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [x + r, y],
        segments: vec![
            Segment::Cubic {
                control1: [x + r, y + k],
                control2: [x + k, y + r],
                end: [x, y + r],
            },
            Segment::Cubic {
                control1: [x - k, y + r],
                control2: [x - r, y + k],
                end: [x - r, y],
            },
            Segment::Cubic {
                control1: [x - r, y - k],
                control2: [x - k, y - r],
                end: [x, y - r],
            },
            Segment::Cubic {
                control1: [x + k, y - r],
                control2: [x + r, y - k],
                end: [x + r, y],
            },
        ],
        closed: true,
    }
}
fn eval(at: P, s: &Segment, t: f64) -> P {
    if t == 1. {
        return match *s {
            Segment::Line { end } | Segment::Quadratic { end, .. } | Segment::Cubic { end, .. } => {
                end
            }
        };
    }
    let mix = |a: P, b: P| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    match *s {
        Segment::Line { end } => mix(at, end),
        Segment::Quadratic { control, end } => mix(mix(at, control), mix(control, end)),
        Segment::Cubic {
            control1,
            control2,
            end,
        } => mix(
            mix(mix(at, control1), mix(control1, control2)),
            mix(mix(control1, control2), mix(control2, end)),
        ),
    }
}
fn inside(contours: &[Contour], p: P) -> bool {
    let mut winding = 0;
    for c in contours {
        let mut at = c.start;
        let mut points = vec![at];
        for s in &c.segments {
            let n = if matches!(s, Segment::Line { .. }) {
                1
            } else {
                200
            };
            for i in 1..=n {
                points.push(eval(at, s, i as f64 / n as f64));
            }
            at = eval(at, s, 1.);
        }
        points.push(c.start);
        for q in points.windows(2) {
            let cross =
                (q[1][0] - q[0][0]) * (p[1] - q[0][1]) - (p[0] - q[0][0]) * (q[1][1] - q[0][1]);
            if q[0][1] <= p[1] && q[1][1] > p[1] && cross > 0. {
                winding += 1;
            }
            if q[1][1] <= p[1] && q[0][1] > p[1] && cross < 0. {
                winding -= 1;
            }
        }
    }
    winding != 0
}
fn assert_region(a: &[Contour], b: &[Contour], op: &str, out: &[Contour]) {
    for x in 0..27 {
        for y in 0..25 {
            let p = [-145. + x as f64 * 13.17, -142. + y as f64 * 12.13];
            let left = inside(a, p);
            let right = inside(b, p);
            let expected = match op {
                "union" => left || right,
                "difference" => left && !right,
                "intersection" => left && right,
                "xor" => left != right,
                _ => panic!(),
            };
            assert_eq!(inside(out, p), expected, "{op} at {p:?}");
        }
    }
}
#[test]
fn all_four_operations_retain_cubic_curves_and_correct_regions() {
    let a = vec![circle(0., 0., 100.)];
    let b = vec![circle(85., 20., 100.)];
    for op in ["union", "difference", "intersection", "xor"] {
        let out = combine(&a, &b, op).unwrap_or_else(|e| panic!("{op}: {e}"));
        assert!(out
            .iter()
            .flat_map(|c| &c.segments)
            .any(|s| matches!(s, Segment::Cubic { .. })));
        assert!(out.iter().map(|c| c.segments.len()).sum::<usize>() < 20);
        for c in &out {
            geometry::validate(c).unwrap();
            assert_eq!(eval(c.start, c.segments.last().unwrap(), 1.), c.start);
        }
        assert_region(&a, &b, op, &out);
    }
}
#[test]
fn quadratic_intersections_are_on_the_original_curve() {
    let a = Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [0., 0.],
        segments: vec![
            Segment::Quadratic {
                control: [100., 200.],
                end: [200., 0.],
            },
            Segment::Line { end: [0., 0.] },
        ],
        closed: true,
    };
    let out = combine(&[a], &[rect(-10., 50., 220., 100.)], "intersection").unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].segments.len(), 2);
    let mut at = out[0].start;
    let mut curves = 0;
    for s in &out[0].segments {
        if matches!(s, Segment::Quadratic { .. }) {
            curves += 1;
            for i in 0..=20 {
                let p = eval(at, s, i as f64 / 20.);
                assert!((p[1] - (2. * p[0] - p[0] * p[0] / 100.)).abs() < 1e-6);
            }
        }
        at = eval(at, s, 1.);
    }
    assert_eq!(curves, 1);
}
#[test]
fn uncut_curves_and_reversed_holes_keep_original_controls() {
    let mut hole = circle(0., 0., 30.);
    path_edit::reverse(&mut hole).unwrap();
    let a = vec![circle(0., 0., 100.), hole];
    let b = vec![circle(250., 0., 30.)];
    let out = combine(&a, &b, "union").unwrap();
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|c| c.segments.len() == 4
        && c.segments
            .iter()
            .all(|s| matches!(s, Segment::Cubic { .. }))));
    assert_region(&a, &b, "union", &out);
    for c in a.iter().chain(&b) {
        for segment in &c.segments {
            let Segment::Cubic {
                control1, control2, ..
            } = segment
            else {
                panic!()
            };
            assert!(out.iter().flat_map(|c| &c.segments).any(|s| match s {
                Segment::Cubic {
                    control1: a,
                    control2: b,
                    ..
                } => (*a == *control1 && *b == *control2) || (*b == *control1 && *a == *control2),
                _ => false,
            }));
        }
    }
}
#[test]
fn coincidence_tangency_and_empty_outputs_do_not_make_slivers() {
    let a = circle(0., 0., 100.);
    let mut reverse = a.clone();
    path_edit::reverse(&mut reverse).unwrap();
    for b in [a.clone(), reverse] {
        for op in ["union", "intersection"] {
            let out = combine(std::slice::from_ref(&a), std::slice::from_ref(&b), op).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].segments.len(), 4);
        }
        for op in ["xor", "difference"] {
            assert!(
                combine(std::slice::from_ref(&a), std::slice::from_ref(&b), op)
                    .unwrap()
                    .is_empty()
            );
        }
    }
    let b = circle(200., 0., 100.);
    let out = combine(
        std::slice::from_ref(&a),
        std::slice::from_ref(&b),
        "intersection",
    )
    .unwrap();
    assert!(out.is_empty());
    let out = combine(std::slice::from_ref(&a), std::slice::from_ref(&b), "union").unwrap();
    assert_eq!(out.len(), 2);
}
#[test]
fn coincident_straight_beziers_keep_degree_with_nonuniform_parameterization() {
    let mut a = rect(0., 0., 100., 100.);
    a.segments[0] = Segment::Cubic {
        control1: [1., 0.],
        control2: [4., 0.],
        end: [100., 0.],
    };
    let out = combine(&[a], &[rect(50., -50., 100., 100.)], "intersection").unwrap();
    assert_eq!(out.len(), 1);
    assert!(out[0]
        .segments
        .iter()
        .any(|s| matches!(s, Segment::Cubic { .. })));
}
#[test]
fn groups_crossings_and_self_crossing_inputs_follow_nonzero_winding() {
    let a = vec![circle(-40., 0., 90.), circle(40., 0., 90.)];
    let b = vec![circle(0., 70., 80.), circle(0., -70., 80.)];
    for op in ["union", "intersection", "difference", "xor"] {
        let out = combine(&a, &b, op).unwrap_or_else(|e| panic!("group {op}: {e}"));
        assert_region(&a, &b, op, &out);
    }
    let a = vec![Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [-100., -100.],
        segments: vec![
            Segment::Cubic {
                control1: [-30., -30.],
                control2: [30., 30.],
                end: [100., 100.],
            },
            Segment::Line { end: [-100., 100.] },
            Segment::Line { end: [100., -100.] },
        ],
        closed: true,
    }];
    let b = vec![rect(-40., -120., 80., 240.)];
    for op in ["union", "intersection", "difference", "xor"] {
        let out = combine(&a, &b, op).unwrap();
        assert_region(&a, &b, op, &out);
    }
}
#[test]
fn curve_results_survive_history_storage_and_both_font_formats() {
    let a = circle(0., 0., 100.);
    let b = circle(85., 20., 100.);
    let mut doc = FontDocument::new("Curve boolean", "").unwrap();
    let mut glyph = Glyph::new(1, 'A');
    glyph.contours = vec![a.clone(), b.clone()];
    doc.glyphs.insert(1, glyph);
    let mut editor = Editor::new(doc).unwrap();
    let before = editor.document().clone();
    let mut glyph = before.glyphs[&1].clone();
    glyph.contours = combine(&[a], &[b], "union").unwrap();
    editor.replace_glyph(editor.revision(), glyph).unwrap();
    let after = editor.document().clone();
    editor.undo(editor.revision()).unwrap();
    assert_eq!(editor.document(), &before);
    editor.redo(editor.revision()).unwrap();
    assert_eq!(editor.document(), &after);
    let path =
        std::env::temp_dir().join(format!("fonts-curve-boolean-{}.json", after.document_uuid));
    storage::save(&path, &after).unwrap();
    assert_eq!(storage::load(&path).unwrap(), after);
    std::fs::remove_file(path).unwrap();
    // Actual exported fonts are independently parsed by ttf-parser.
    for bytes in [
        font_core::font_io::export(&after).unwrap(),
        font_core::font_formats::export(&after, font_core::font_formats::Format::Otf).unwrap(),
    ] {
        assert!(ttf_parser::Face::parse(&bytes, 0).is_ok());
    }
}
