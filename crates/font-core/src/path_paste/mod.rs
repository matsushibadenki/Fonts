//! Bounded vector-only clipboard interchange. No PostScript, SVG script or external resource executes.
use crate::{geometry, Contour, Editor, GlyphId, GlyphStatus, Point, Segment};
mod pdf;
mod postscript;
mod svg;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SEGMENTS: usize = 65536;
pub type Result<T> = std::result::Result<T, &'static str>;
pub type Matrix = [f64; 6];
pub const IDENTITY: Matrix = [1., 0., 0., 1., 0., 0.];
pub fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}
pub fn map(m: Matrix, p: Point) -> Point {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}
pub fn transform(c: &Contour, m: Matrix) -> Result<Contour> {
    let c = Contour {
        start: map(m, c.start),
        segments: c
            .segments
            .iter()
            .map(|s| match *s {
                Segment::Line { end } => Segment::Line { end: map(m, end) },
                Segment::Quadratic { control, end } => Segment::Quadratic {
                    control: map(m, control),
                    end: map(m, end),
                },
                Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => Segment::Cubic {
                    control1: map(m, control1),
                    control2: map(m, control2),
                    end: map(m, end),
                },
            })
            .collect(),
        closed: c.closed,
        smooth: c.smooth.clone(),
    };
    geometry::validate(&c).map_err(|_| "clipboard_invalid")?;
    Ok(c)
}
pub fn validate(contours: &[Contour]) -> Result<()> {
    if contours.is_empty() {
        return Err("clipboard_empty");
    }
    if contours.len() > 4096
        || contours.iter().map(|c| c.segments.len() + 1).sum::<usize>() > MAX_SEGMENTS
    {
        return Err("clipboard_too_large");
    }
    for c in contours {
        geometry::validate(c).map_err(|_| "clipboard_invalid")?;
    }
    Ok(())
}
pub fn decode(kind: &str, bytes: &[u8]) -> Result<Vec<Contour>> {
    if bytes.len() > MAX_BYTES {
        return Err("clipboard_too_large");
    }
    let result = match kind {
        "lumapaint" => svg::lumapaint(std::str::from_utf8(bytes).map_err(|_| "clipboard_invalid")?),
        "svg" => svg::document(std::str::from_utf8(bytes).map_err(|_| "clipboard_invalid")?),
        "pdf" => pdf::decode(bytes),
        "aicb" => postscript::decode(std::str::from_utf8(bytes).map_err(|_| "clipboard_invalid")?),
        _ => Err("clipboard_unsupported"),
    }?;
    validate(&result)?;
    Ok(result)
}
impl Editor {
    /// Fit the whole selection with preserved aspect ratio; append atomically in one undo step.
    pub fn paste_paths(
        &mut self,
        revision: u64,
        id: GlyphId,
        contours: Vec<Contour>,
    ) -> Result<std::ops::Range<usize>> {
        self.check_revision(revision).map_err(|_| "Conflict")?;
        validate(&contours)?;
        let doc = self.document();
        let mut glyph = doc.glyphs.get(&id).ok_or("clipboard_no_glyph")?.clone();
        if id == 0 {
            return Err("clipboard_no_glyph");
        }
        let first = glyph.contours.len();
        if first + contours.len() > 4096 {
            return Err("clipboard_too_large");
        }
        let (min, max) = crate::snippets::bounds(&contours).map_err(|_| "clipboard_invalid")?;
        let width = max[0] - min[0];
        let height = max[1] - min[1];
        let advance = if glyph.advance > 0. {
            glyph.advance
        } else {
            f64::from(doc.units_per_em)
        };
        let mut scale = f64::INFINITY;
        if width > 1e-9 {
            scale = scale.min(advance * 0.9 / width);
        }
        if height > 1e-9 {
            scale = scale.min((doc.ascender - doc.descender) * 0.9 / height);
        }
        if !scale.is_finite() || scale <= 0. {
            return Err("clipboard_empty");
        }
        let m = [
            scale,
            0.,
            0.,
            scale,
            advance / 2. - (min[0] + max[0]) * scale / 2.,
            (doc.ascender + doc.descender) / 2. - (min[1] + max[1]) * scale / 2.,
        ];
        let transformed = contours
            .iter()
            .map(|c| transform(c, m))
            .collect::<Result<Vec<_>>>()?;
        glyph.contours.extend(transformed);
        glyph.status = GlyphStatus::Draft;
        let end = glyph.contours.len();
        self.replace_glyph(revision, glyph).map_err(|e| match e {
            crate::Error::Conflict => "Conflict",
            _ => "clipboard_invalid",
        })?;
        Ok(first..end)
    }
}
#[derive(Default)]
pub(super) struct Path {
    pub contours: Vec<Contour>,
    current: Option<Contour>,
    pub segments: usize,
}
impl Path {
    pub fn move_to(&mut self, p: Point) -> Result<()> {
        self.finish()?;
        self.current = Some(Contour {
            start: p,
            segments: vec![],
            closed: false,
            smooth: Default::default(),
        });
        Ok(())
    }
    pub fn at(&self) -> Result<Point> {
        let c = self.current.as_ref().ok_or("clipboard_invalid")?;
        Ok(c.segments
            .last()
            .map(|s| match *s {
                Segment::Line { end }
                | Segment::Quadratic { end, .. }
                | Segment::Cubic { end, .. } => end,
            })
            .unwrap_or(c.start))
    }
    pub fn segment(&mut self, s: Segment) -> Result<()> {
        self.segments += 1;
        if self.segments > MAX_SEGMENTS {
            return Err("clipboard_too_large");
        }
        self.current
            .as_mut()
            .ok_or("clipboard_invalid")?
            .segments
            .push(s);
        Ok(())
    }
    pub fn close(&mut self) -> Result<()> {
        self.current.as_mut().ok_or("clipboard_invalid")?.closed = true;
        self.finish()
    }
    pub fn finish(&mut self) -> Result<()> {
        if let Some(c) = self.current.take() {
            if !c.segments.is_empty() {
                geometry::validate(&c).map_err(|_| "clipboard_invalid")?;
                self.contours.push(c);
                if self.contours.len() > 4096 {
                    return Err("clipboard_too_large");
                }
            }
        }
        Ok(())
    }
    pub fn take(&mut self) -> Result<Vec<Contour>> {
        self.finish()?;
        Ok(std::mem::take(&mut self.contours))
    }
}
/// Convert nested even-odd contours into opposite winding for nonzero font outlines.
/// Intersection cases are rejected rather than changing their filled region silently.
pub(super) fn even_odd(contours: &mut [Contour]) -> Result<()> {
    if contours.len() > 512 {
        return Err("clipboard_too_large");
    }
    if contours
        .iter()
        .map(|c| c.segments.len() * 24 + 1)
        .sum::<usize>()
        > 16384
    {
        return Err("clipboard_too_large");
    }
    let polys: Vec<Vec<Point>> = contours.iter().map(flatten).collect();
    let cross = |a: Point, b: Point, c: Point| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut checks = 0;
    for i in 0..polys.len() {
        for j in i..polys.len() {
            let a = &polys[i];
            let b = &polys[j];
            for x in 0..a.len() {
                for y in 0..b.len() {
                    if i == j && (x >= y || y == x + 1 || (x == 0 && y == a.len() - 1)) {
                        continue;
                    }
                    checks += 1;
                    if checks > 2_000_000 {
                        return Err("clipboard_too_large");
                    }
                    let (p, q, r, t) = (a[x], a[(x + 1) % a.len()], b[y], b[(y + 1) % b.len()]);
                    if cross(p, q, r) * cross(p, q, t) < -1e-12
                        && cross(r, t, p) * cross(r, t, q) < -1e-12
                    {
                        return Err("clipboard_outline_required");
                    }
                }
            }
        }
    }

    let area = |p: &[Point]| {
        p.iter()
            .zip(p.iter().cycle().skip(1))
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum::<f64>()
    };
    let inside = |p: Point, poly: &[Point]| {
        let mut yes = false;
        for (a, b) in poly.iter().zip(poly.iter().cycle().skip(1)) {
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                yes = !yes;
            }
        }
        yes
    };
    for i in 0..contours.len() {
        if !contours[i].closed {
            continue;
        }
        let depth = polys
            .iter()
            .enumerate()
            .filter(|(j, p)| *j != i && inside(polys[i][0], p))
            .count();
        let positive = depth % 2 == 0;
        if (area(&polys[i]) > 0.) != positive {
            reverse(&mut contours[i]);
        }
    }
    Ok(())
}
fn flatten(c: &Contour) -> Vec<Point> {
    let mut p = vec![c.start];
    let mut at = c.start;
    for s in &c.segments {
        let end = match *s {
            Segment::Line { end } => {
                p.push(end);
                end
            }
            Segment::Quadratic { control, end } => {
                for i in 1..=24 {
                    let t = i as f64 / 24.;
                    p.push(std::array::from_fn(|j| {
                        (1. - t).powi(2) * at[j] + 2. * (1. - t) * t * control[j] + t * t * end[j]
                    }));
                }
                end
            }
            Segment::Cubic {
                control1,
                control2,
                end,
            } => {
                for i in 1..=24 {
                    let t = i as f64 / 24.;
                    p.push(std::array::from_fn(|j| {
                        (1. - t).powi(3) * at[j]
                            + 3. * (1. - t).powi(2) * t * control1[j]
                            + 3. * (1. - t) * t * t * control2[j]
                            + t.powi(3) * end[j]
                    }));
                }
                end
            }
        };
        at = end;
    }
    p
}
fn reverse(c: &mut Contour) {
    let mut at = c.start;
    let mut segments = vec![];
    for s in &c.segments {
        segments.push(match *s {
            Segment::Line { end } => {
                let s = Segment::Line { end: at };
                at = end;
                s
            }
            Segment::Quadratic { control, end } => {
                let s = Segment::Quadratic { control, end: at };
                at = end;
                s
            }
            Segment::Cubic {
                control1,
                control2,
                end,
            } => {
                let s = Segment::Cubic {
                    control1: control2,
                    control2: control1,
                    end: at,
                };
                at = end;
                s
            }
        });
    }
    segments.reverse();
    c.start = at;
    c.segments = segments;
    c.smooth.clear();
}
