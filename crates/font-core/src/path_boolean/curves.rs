//! Provenance-guided reconstruction. Overlay chords determine topology, never output geometry.
use super::*;
type Polygons = Vec<Vec<Point>>;
const FLATNESS: f64 = 0.001;
const MATCH: f64 = 0.000001;
const JOIN: f64 = 0.0000001;
fn mix(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
#[derive(Clone)]
pub(super) struct Curve {
    points: Vec<Point>,
}
impl Curve {
    fn at(&self, t: f64) -> Point {
        let mut p = self.points.clone();
        for n in (1..p.len()).rev() {
            for i in 0..n {
                p[i] = mix(p[i], p[i + 1], t);
            }
        }
        p[0]
    }
    fn derivative(&self, t: f64) -> Point {
        let n = self.points.len() - 1;
        Self {
            points: self
                .points
                .windows(2)
                .map(|p| {
                    [
                        (p[1][0] - p[0][0]) * n as f64,
                        (p[1][1] - p[0][1]) * n as f64,
                    ]
                })
                .collect(),
        }
        .at(t)
    }
    fn split(&self, t: f64) -> (Self, Self) {
        let mut p = self.points.clone();
        let mut a = vec![p[0]];
        let mut b = vec![*p.last().unwrap()];
        for n in (1..p.len()).rev() {
            for i in 0..n {
                p[i] = mix(p[i], p[i + 1], t);
            }
            a.push(p[0]);
            b.push(p[n - 1]);
        }
        b.reverse();
        (Self { points: a }, Self { points: b })
    }
    fn piece(&self, a: f64, b: f64) -> Self {
        if a > b {
            let mut p = self.piece(b, a);
            p.points.reverse();
            return p;
        }
        let left = if b == 1. {
            self.clone()
        } else {
            self.split(b).0
        };
        if a == 0. {
            left
        } else {
            left.split(a / b).1
        }
    }
    fn segment(&self, end: Point) -> Segment {
        match self.points.as_slice() {
            [_, _] => Segment::Line { end },
            [_, control, _] => Segment::Quadratic {
                control: *control,
                end,
            },
            [_, control1, control2, _] => Segment::Cubic {
                control1: *control1,
                control2: *control2,
                end,
            },
            _ => unreachable!(),
        }
    }
}
#[derive(Clone, Copy)]
struct Edge {
    source: usize,
    a: Point,
    b: Point,
    lo: f64,
    hi: f64,
}
impl Edge {
    fn bounds(self) -> [f64; 4] {
        [
            self.a[0].min(self.b[0]),
            self.a[1].min(self.b[1]),
            self.a[0].max(self.b[0]),
            self.a[1].max(self.b[1]),
        ]
    }
}
pub(super) struct Sources {
    curves: Vec<Curve>,
    edges: Vec<Edge>,
}
impl Sources {
    pub fn new() -> Self {
        Self {
            curves: vec![],
            edges: vec![],
        }
    }
    pub fn polygons(
        &mut self,
        left: &[Contour],
        right: &[Contour],
    ) -> Result<(Polygons, Polygons)> {
        let mut paths = Vec::new();
        for c in left.iter().chain(right) {
            geometry::validate(c)?;
            if !c.closed || c.segments.is_empty() {
                return Err(Error::InvalidGeometry);
            }
            let mut ids = Vec::new();
            let mut at = c.start;
            for segment in &c.segments {
                let points = match segment {
                    Segment::Line { end } => vec![at, *end],
                    Segment::Quadratic { control, end } => vec![at, *control, *end],
                    Segment::Cubic {
                        control1,
                        control2,
                        end,
                    } => vec![at, *control1, *control2, *end],
                };
                at = *points.last().unwrap();
                ids.push(self.curves.len());
                self.curves.push(Curve { points });
                if self.curves.len() > LIMIT {
                    return Err(Error::TooLarge);
                }
            }
            if at != c.start {
                ids.push(self.curves.len());
                self.curves.push(Curve {
                    points: vec![at, c.start],
                });
            }
            paths.push(ids);
        }
        // Share tessellation for identical/subdivided Béziers. Otherwise different sampling grids
        // would create polygon slivers between geometrically coincident input boundaries.
        let bounds = |c: &Curve| {
            c.points.iter().fold(
                [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ],
                |mut b, p| {
                    b[0] = b[0].min(p[0]);
                    b[1] = b[1].min(p[1]);
                    b[2] = b[2].max(p[0]);
                    b[3] = b[3].max(p[1]);
                    b
                },
            )
        };
        let boxes: Vec<_> = self.curves.iter().map(bounds).collect();
        let mut order: Vec<_> = (0..self.curves.len()).collect();
        order.sort_by(|&a, &b| {
            let size = |i: usize| (boxes[i][2] - boxes[i][0]).hypot(boxes[i][3] - boxes[i][1]);
            size(b).total_cmp(&size(a)).then(a.cmp(&b))
        });
        let mut maps = vec![(0usize, 0., 1.); self.curves.len()];
        let mut masters = Vec::new();
        let mut comparisons = 0;
        let mut coincidence_checks = 0;
        for i in order {
            let mut mapped = None;
            for &j in &masters {
                comparisons += 1;
                if comparisons > 2_000_000 {
                    return Err(Error::TooLarge);
                }
                let a = boxes[i];
                let b: [f64; 4] = boxes[j];
                if a[0] < b[0] - JOIN
                    || a[1] < b[1] - JOIN
                    || a[2] > b[2] + JOIN
                    || a[3] > b[3] + JOIN
                {
                    continue;
                }
                coincidence_checks += 1;
                if coincidence_checks > 16_384 {
                    return Err(Error::TooLarge);
                }
                if let Some((lo, hi)) = subcurve(&self.curves[j], &self.curves[i]) {
                    mapped = Some((j, lo, hi));
                    break;
                }
            }
            maps[i] = mapped.unwrap_or((i, 0., 1.));
            if mapped.is_none() {
                masters.push(i);
            }
        }
        let mut cuts = vec![vec![0., 1.]; self.curves.len()];
        for &(i, a, b) in &maps {
            cuts[i].extend([a, b]);
        }
        let mut ranges = vec![0..0; self.curves.len()];
        let mut work = 0;
        for i in masters {
            cuts[i].sort_by(f64::total_cmp);
            cuts[i].dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            let start = self.edges.len();
            for pair in cuts[i].windows(2) {
                let piece = self.curves[i].piece(pair[0], pair[1]);
                self.flatten(&piece, i, pair[0], pair[1], 0, &mut Vec::new(), &mut work)?;
            }
            ranges[i] = start..self.edges.len();
        }
        let mut polygons = Vec::new();
        let mut count = 0;
        for path in paths {
            let mut points = Vec::new();
            for original in path {
                let (i, a, b) = maps[original];
                let low = a.min(b);
                let high = a.max(b);
                let edges = &self.edges[ranges[i].clone()];
                let mut append = |e: &Edge| {
                    if e.lo >= low - 1e-12 && e.hi <= high + 1e-12 {
                        let (p, q) = if a < b { (e.a, e.b) } else { (e.b, e.a) };
                        if points.is_empty() {
                            points.push(p);
                        }
                        points.push(q);
                    }
                };
                if a < b {
                    for e in edges {
                        append(e);
                    }
                } else {
                    for e in edges.iter().rev() {
                        append(e);
                    }
                }
            }
            if points
                .first()
                .zip(points.last())
                .is_some_and(|(&a, &b)| distance(a, b) <= JOIN)
            {
                points.pop();
            }
            if points.len() < 3 {
                return Err(Error::InvalidGeometry);
            }
            count += points.len() + 1;
            if count > LIMIT {
                return Err(Error::TooLarge);
            }
            polygons.push(points);
        }
        let b = polygons.split_off(left.len());
        Ok((polygons, b))
    }
    #[allow(clippy::too_many_arguments)]
    fn flatten(
        &mut self,
        c: &Curve,
        source: usize,
        lo: f64,
        hi: f64,
        depth: usize,
        points: &mut Vec<Point>,
        work: &mut usize,
    ) -> Result<()> {
        let a = c.points[0];
        let b = *c.points.last().unwrap();
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = d[0] * d[0] + d[1] * d[1];
        let flat = c.points[1..c.points.len() - 1].iter().all(|&p| {
            let t = if len == 0. {
                0.
            } else {
                ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len
            }
            .clamp(0., 1.);
            distance(p, mix(a, b, t)) <= FLATNESS
        });
        if flat {
            *work += 1;
            if *work > LIMIT {
                return Err(Error::TooLarge);
            }
            if a != b {
                self.edges.push(Edge {
                    source,
                    a,
                    b,
                    lo,
                    hi,
                });
                points.push(b);
            }
            return Ok(());
        }
        if depth >= 24 {
            return Err(Error::TooLarge);
        }
        let (left, right) = c.split(0.5);
        let mid = (lo + hi) * 0.5;
        self.flatten(&left, source, lo, mid, depth + 1, points, work)?;
        self.flatten(&right, source, mid, hi, depth + 1, points, work)
    }
    pub fn restore(&self, shapes: Vec<Vec<Vec<Point>>>) -> Result<Vec<Contour>> {
        let index = Index::new(self.edges.clone());
        let mut result = vec![];
        let mut count = 0;
        let mut budget = 0;
        for points in shapes.into_iter().flatten() {
            count += points.len();
            if count > 32768 {
                return Err(Error::TooLarge);
            }
            if points.len() < 3 {
                continue;
            }
            let mut runs: Vec<Run> = vec![];
            for (&a, &b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                let mut candidates = vec![];
                index.query(mix(a, b, 0.5), &mut candidates, &mut budget)?;
                let mut found = vec![];
                for edge in candidates {
                    if let Some((t, u)) = edge_parameters(edge, a, b) {
                        found.push((edge, t, u));
                    }
                }
                // Prefer continuity when duplicate/coincident inputs describe the same boundary.
                found.sort_by_key(|(edge, _, _)| {
                    (
                        runs.last().is_none_or(|r| r.source != edge.source),
                        edge.source,
                    )
                });
                let (edge, t, u) = found.first().copied().ok_or(Error::CurveBooleanAmbiguous)?;
                let curve = &self.curves[edge.source];
                let start = Parameter {
                    t: nearest(curve, a, t, edge.lo, edge.hi),
                    lo: edge.lo,
                    hi: edge.hi,
                };
                let end = Parameter {
                    t: nearest(curve, b, u, edge.lo, edge.hi),
                    lo: edge.lo,
                    hi: edge.hi,
                };
                if let Some(last) = runs.last_mut().filter(|r| {
                    r.source == edge.source
                        && (r.end.t - start.t).abs() < 1e-8
                        && (r.end.t - r.start.t) * (end.t - start.t) > 0.
                }) {
                    last.end = end;
                } else {
                    runs.push(Run {
                        source: edge.source,
                        start,
                        end,
                    });
                }
            }
            if runs.len() > 1 {
                let last = *runs.last().unwrap();
                let first = runs[0];
                if last.source == first.source
                    && (last.end.t - first.start.t).abs() < 1e-8
                    && (last.end.t - last.start.t) * (first.end.t - first.start.t) > 0.
                {
                    runs[0].start = last.start;
                    runs.pop();
                }
            }
            if runs.is_empty() {
                continue;
            }
            let directions: Vec<_> = runs
                .iter()
                .map(|r| (r.end.t - r.start.t).signum())
                .collect();
            let mut joins = vec![];
            for i in 0..runs.len() {
                let j = (i + 1) % runs.len();
                let (t, u, p) = intersection(
                    &self.curves[runs[i].source],
                    runs[i].end,
                    &self.curves[runs[j].source],
                    runs[j].start,
                )?;
                runs[i].end.t = t;
                runs[j].start.t = u;
                joins.push(p);
            }
            let mut contour = Contour {
                identity: Default::default(),
                smooth: Default::default(),
                start: *joins.last().unwrap(),
                segments: vec![],
                closed: true,
            };
            for (i, r) in runs.iter().enumerate() {
                if (r.end.t - r.start.t) * directions[i] < 1e-12 {
                    return Err(Error::CurveBooleanAmbiguous);
                }
                let piece = self.curves[r.source].piece(r.start.t, r.end.t);
                contour.segments.push(piece.segment(joins[i]));
            }
            geometry::validate(&contour)?;
            result.push(contour);
        }
        Ok(result)
    }
}
#[derive(Clone, Copy)]
struct Parameter {
    t: f64,
    lo: f64,
    hi: f64,
}
#[derive(Clone, Copy)]
struct Run {
    source: usize,
    start: Parameter,
    end: Parameter,
}
fn edge_parameters(e: Edge, a: Point, b: Point) -> Option<(f64, f64)> {
    let d = [e.b[0] - e.a[0], e.b[1] - e.a[1]];
    let len = d[0] * d[0] + d[1] * d[1];
    let project = |p: Point| {
        let t = ((p[0] - e.a[0]) * d[0] + (p[1] - e.a[1]) * d[1]) / len;
        if t < -MATCH / len.sqrt()
            || t > 1. + MATCH / len.sqrt()
            || distance(p, mix(e.a, e.b, t)) > MATCH
        {
            None
        } else {
            Some(e.lo + t.clamp(0., 1.) * (e.hi - e.lo))
        }
    };
    Some((project(a)?, project(b)?))
}
fn nearest(c: &Curve, p: Point, mut t: f64, lo: f64, hi: f64) -> f64 {
    for _ in 0..12 {
        let q = c.at(t);
        let d = c.derivative(t);
        let len = d[0] * d[0] + d[1] * d[1];
        if len < 1e-24 {
            break;
        }
        let next = (t - ((q[0] - p[0]) * d[0] + (q[1] - p[1]) * d[1]) / len).clamp(lo, hi);
        if (next - t).abs() < 1e-14 {
            break;
        }
        t = next;
    }
    t
}
/// Certify equality by comparing Bézier control polygons after degree elevation.
fn subcurve(master: &Curve, candidate: &Curve) -> Option<(f64, f64)> {
    if master.points == candidate.points {
        return Some((0., 1.));
    }
    if master.points.iter().rev().eq(candidate.points.iter()) {
        return Some((1., 0.));
    }
    let locate = |p: Point| {
        (0..=16)
            .map(|i| nearest(master, p, i as f64 / 16., 0., 1.))
            .min_by(|a, b| distance(master.at(*a), p).total_cmp(&distance(master.at(*b), p)))
    };
    let a = locate(candidate.points[0])?;
    let b = locate(*candidate.points.last()?)?;
    if (a - b).abs() < 1e-12
        || distance(master.at(a), candidate.points[0]) > JOIN
        || distance(master.at(b), *candidate.points.last()?) > JOIN
    {
        return None;
    }
    let elevate = |mut p: Vec<Point>, degree: usize| {
        while p.len() < degree {
            let n = p.len();
            let mut q = vec![p[0]];
            for i in 1..n {
                q.push(mix(p[i], p[i - 1], i as f64 / n as f64));
            }
            q.push(*p.last().unwrap());
            p = q;
        }
        p
    };
    let degree = master.points.len().max(candidate.points.len());
    let p = elevate(master.piece(a, b).points, degree);
    let q = elevate(candidate.points.clone(), degree);
    p.iter()
        .zip(q)
        .all(|(&p, q)| distance(p, q) <= JOIN)
        .then_some((a, b))
}
fn intersection(
    a: &Curve,
    first: Parameter,
    b: &Curve,
    second: Parameter,
) -> Result<(f64, f64, Point)> {
    let mut t = first.t;
    let mut u = second.t;
    let bounds = |p: Parameter| {
        (
            (p.lo - (p.hi - p.lo) * 2.).max(0.),
            (p.hi + (p.hi - p.lo) * 2.).min(1.),
        )
    };
    let (lo, hi) = bounds(first);
    let (low, high) = bounds(second);
    for _ in 0..40 {
        let p = a.at(t);
        let q = b.at(u);
        if distance(p, q) <= JOIN {
            return Ok((t, u, mix(p, q, 0.5)));
        }
        let d = a.derivative(t);
        let e = b.derivative(u);
        let r = [q[0] - p[0], q[1] - p[1]];
        let cross = d[0] * e[1] - d[1] * e[0];
        if cross.abs() < 1e-20 {
            break;
        }
        let dt = (r[0] * e[1] - r[1] * e[0]) / cross;
        let du = (r[0] * d[1] - r[1] * d[0]) / cross;
        let old = distance(p, q);
        let mut rate = 1.;
        let mut accepted = false;
        for _ in 0..12 {
            let next = (t + dt * rate).clamp(lo, hi);
            let other = (u + du * rate).clamp(low, high);
            if distance(a.at(next), b.at(other)) < old {
                t = next;
                u = other;
                accepted = true;
                break;
            }
            rate *= 0.5;
        }
        if !accepted {
            break;
        }
    }
    Err(Error::CurveBooleanAmbiguous)
}
struct Index {
    bounds: [f64; 4],
    edges: Vec<Edge>,
    children: Option<(Box<Index>, Box<Index>)>,
}
impl Index {
    fn new(mut edges: Vec<Edge>) -> Self {
        let bounds = edges.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |mut a, e| {
                let b = e.bounds();
                a[0] = a[0].min(b[0]);
                a[1] = a[1].min(b[1]);
                a[2] = a[2].max(b[2]);
                a[3] = a[3].max(b[3]);
                a
            },
        );
        let children = if edges.len() > 8 {
            let axis = usize::from(bounds[3] - bounds[1] > bounds[2] - bounds[0]);
            edges.sort_by(|a, b| {
                let a = a.bounds();
                let b = b.bounds();
                (a[axis] + a[axis + 2]).total_cmp(&(b[axis] + b[axis + 2]))
            });
            let right = edges.split_off(edges.len() / 2);
            Some((
                Box::new(Self::new(std::mem::take(&mut edges))),
                Box::new(Self::new(right)),
            ))
        } else {
            None
        };
        Self {
            bounds,
            edges,
            children,
        }
    }
    fn query(&self, p: Point, out: &mut Vec<Edge>, work: &mut usize) -> Result<()> {
        *work += 1;
        if *work > 4_000_000 {
            return Err(Error::TooLarge);
        }
        if p[0] < self.bounds[0] - MATCH
            || p[0] > self.bounds[2] + MATCH
            || p[1] < self.bounds[1] - MATCH
            || p[1] > self.bounds[3] + MATCH
        {
            return Ok(());
        }
        if let Some((a, b)) = &self.children {
            a.query(p, out, work)?;
            b.query(p, out, work)?;
        } else {
            out.extend(&self.edges);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subdivided_coincident_boundaries_do_not_leave_boolean_slivers() {
        let c = Curve {
            points: vec![[0., 0.], [0., 200.], [200., 200.], [200., 0.]],
        };
        let (a, b) = c.split(0.37);
        let make = |segments| Contour {
            identity: Default::default(),
            smooth: Default::default(),
            start: [0., 0.],
            segments,
            closed: true,
        };
        let whole = make(vec![c.segment([200., 0.]), Segment::Line { end: [0., 0.] }]);
        let divided = make(vec![
            a.segment(*a.points.last().unwrap()),
            b.segment([200., 0.]),
            Segment::Line { end: [0., 0.] },
        ]);
        for op in ["union", "intersection"] {
            let out = combine(
                std::slice::from_ref(&whole),
                std::slice::from_ref(&divided),
                op,
            )
            .unwrap();
            assert_eq!(out.len(), 1, "{op}");
        }
        for op in ["difference", "xor"] {
            let out = combine(
                std::slice::from_ref(&whole),
                std::slice::from_ref(&divided),
                op,
            )
            .unwrap();
            assert!(out.is_empty(), "{op}: {}", out.len());
        }
    }
    #[test]
    fn unresolved_parallel_curve_join_is_rejected_instead_of_changing_geometry() {
        let a = Curve {
            points: vec![[0., 0.], [1., 0.]],
        };
        let b = Curve {
            points: vec![[0., 0.001], [1., 0.001]],
        };
        let p = Parameter {
            t: 0.5,
            lo: 0.,
            hi: 1.,
        };
        assert_eq!(
            intersection(&a, p, &b, p),
            Err(Error::CurveBooleanAmbiguous)
        );
    }
}
