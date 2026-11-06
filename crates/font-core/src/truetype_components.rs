mod export;
pub use export::{export, Export};
// Bounded TrueType component decoding in design units, including point attachment.
use crate::{font_io, Contour, FontDocument, Point, Segment};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = font_io::Result<T>;
#[derive(Clone, Debug)]
pub(crate) struct Part {
    pub glyph: u16,
    pub matrix: [f64; 6],
}
#[derive(Clone, Default)]
pub(crate) struct Shape {
    pub depth: usize,
    pub metrics: bool,
    pub rings: Vec<Vec<(Point, bool)>>,
    pub parts: Vec<Part>,
}
impl Shape {
    pub fn contours(&self) -> Vec<Contour> {
        self.rings
            .iter()
            .filter(|r| !r.is_empty())
            .map(|ring| {
                let n = ring.len();
                let (start, from, count) = if ring[0].1 {
                    (ring[0].0, 1, n - 1)
                } else if ring[n - 1].1 {
                    (ring[n - 1].0, 0, n - 1)
                } else {
                    (mid(ring[n - 1].0, ring[0].0), 0, n)
                };
                let mut c = Contour {
                    start,
                    segments: vec![],
                    closed: true,
                    smooth: Default::default(),
                    identity: Default::default(),
                };
                let mut i = 0;
                while i < count {
                    let (p, on) = ring[(from + i) % n];
                    if on {
                        c.segments.push(Segment::Line { end: p });
                        i += 1;
                    } else {
                        let (next, on) = if i + 1 < count {
                            ring[(from + i + 1) % n]
                        } else {
                            (start, true)
                        };
                        let end = if on { next } else { mid(p, next) };
                        c.segments.push(Segment::Quadratic { control: p, end });
                        i += if on && i + 1 < count { 2 } else { 1 };
                    }
                }
                let end = c.segments.last().map(|s| match s {
                    Segment::Line { end }
                    | Segment::Quadratic { end, .. }
                    | Segment::Cubic { end, .. } => *end,
                });
                if end != Some(start) {
                    c.segments.push(Segment::Line { end: start });
                }
                c
            })
            .collect()
    }
}
fn mid(a: Point, b: Point) -> Point {
    [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.]
}
fn bad() -> String {
    "font_invalid_components".into()
}
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.p.checked_add(n).ok_or_else(bad)?;
        let b = self.b.get(self.p..end).ok_or_else(bad)?;
        self.p = end;
        Ok(b)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| bad())?,
        ))
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    fn fixed(&mut self) -> Result<f64> {
        Ok(f64::from(self.i16()?) / 16384.)
    }
}
pub(crate) fn decode(bytes: &[u8], index: u32) -> Result<BTreeMap<u16, Shape>> {
    let raw = ttf_parser::RawFace::parse(bytes, index).map_err(|_| bad())?;
    let table = |tag: &[u8; 4]| raw.table(ttf_parser::Tag::from_bytes(tag)).ok_or_else(bad);
    let head = table(b"head")?;
    let maxp = table(b"maxp")?;
    let loca = table(b"loca")?;
    let glyf = table(b"glyf")?;
    let long = match head.get(50..52) {
        Some([0, 0]) => false,
        Some([0, 1]) => true,
        _ => return Err(bad()),
    };
    let count = u16::from_be_bytes(
        maxp.get(4..6)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    );
    let mut r = Reader { b: loca, p: 0 };
    let mut offsets = vec![];
    for _ in 0..=count {
        offsets.push(if long {
            u32::from_be_bytes(r.take(4)?.try_into().map_err(|_| bad())?) as usize
        } else {
            usize::from(r.u16()?) * 2
        });
    }
    if offsets.windows(2).any(|w| w[0] > w[1]) || offsets.last().copied().unwrap_or(0) > glyf.len()
    {
        return Err(bad());
    }
    let mut out = BTreeMap::new();
    let mut active = BTreeSet::new();
    let mut total = 0;
    for id in 0..count {
        visit(id, &offsets, glyf, &mut out, &mut active, &mut total)?;
    }
    Ok(out)
}
fn visit(
    id: u16,
    offsets: &[usize],
    glyf: &[u8],
    out: &mut BTreeMap<u16, Shape>,
    active: &mut BTreeSet<u16>,
    total: &mut usize,
) -> Result<Shape> {
    if let Some(s) = out.get(&id) {
        if active.len() + s.depth > 17 {
            return Err("font_component_limit".into());
        }
        return Ok(s.clone());
    }
    if active.len() >= 17 || !active.insert(id) {
        return Err(bad());
    }
    let i = usize::from(id);
    let start = *offsets.get(i).ok_or_else(bad)?;
    let end = *offsets.get(i + 1).ok_or_else(bad)?;
    let b = glyf.get(start..end).ok_or_else(bad)?;
    let mut shape = if b.is_empty() {
        Shape::default()
    } else {
        let mut r = Reader { b, p: 0 };
        let count = r.i16()?;
        r.take(8)?;
        if count >= 0 {
            simple(&mut r, count as usize)?
        } else if count == -1 {
            let mut shape = Shape::default();
            let mut instructions = false;
            loop {
                if shape.parts.len() >= 4096 {
                    return Err("font_component_limit".into());
                }
                let f = r.u16()?;
                let child = r.u16()?;
                // Mutually exclusive transform and offset modes; reserved bits cannot affect rendering silently.
                if f & 0xE010 != 0
                    || [0x8, 0x40, 0x80].iter().filter(|&&v| f & v != 0).count() > 1
                    || f & 0x1800 == 0x1800
                {
                    return Err(bad());
                }
                let xy = f & 2 != 0;
                let (x, y) = if f & 1 != 0 {
                    if xy {
                        (f64::from(r.i16()?), f64::from(r.i16()?))
                    } else {
                        (f64::from(r.u16()?), f64::from(r.u16()?))
                    }
                } else if xy {
                    (f64::from(r.u8()? as i8), f64::from(r.u8()? as i8))
                } else {
                    (f64::from(r.u8()?), f64::from(r.u8()?))
                };
                let mut m = [1., 0., 0., 1., 0., 0.];
                if f & 8 != 0 {
                    m[0] = r.fixed()?;
                    m[3] = m[0];
                } else if f & 0x40 != 0 {
                    m[0] = r.fixed()?;
                    m[3] = r.fixed()?;
                } else if f & 0x80 != 0 {
                    m[0] = r.fixed()?;
                    m[1] = r.fixed()?;
                    m[2] = r.fixed()?;
                    m[3] = r.fixed()?;
                }
                if f & 0x200 != 0 {
                    if shape.metrics || m[1] != 0. || m[2] != 0. {
                        return Err(bad());
                    }
                    shape.metrics = true;
                }
                let source = visit(child, offsets, glyf, out, active, total)?;
                shape.depth = shape.depth.max(source.depth + 1);
                let point = |p: Point| [m[0] * p[0] + m[2] * p[1], m[1] * p[0] + m[3] * p[1]];
                let delta = if xy {
                    if f & 0x800 != 0 {
                        point([x, y])
                    } else {
                        [x, y]
                    }
                } else {
                    let a = shape
                        .rings
                        .iter()
                        .flatten()
                        .nth(x as usize)
                        .ok_or_else(bad)?
                        .0;
                    let b = point(
                        source
                            .rings
                            .iter()
                            .flatten()
                            .nth(y as usize)
                            .ok_or_else(bad)?
                            .0,
                    );
                    [a[0] - b[0], a[1] - b[1]]
                };
                let rings: Vec<_> = source
                    .rings
                    .iter()
                    .map(|ring| {
                        ring.iter()
                            .map(|&(p, on)| {
                                let p = point(p);
                                ([p[0] + delta[0], p[1] + delta[1]], on)
                            })
                            .collect()
                    })
                    .collect();
                m[4] = delta[0];
                m[5] = delta[1];
                shape.rings.extend(rings);
                shape.parts.push(Part {
                    glyph: child,
                    matrix: m,
                });
                if shape.rings.iter().map(Vec::len).sum::<usize>() > 65535
                    || shape.rings.len() > 4096
                {
                    return Err("font_component_limit".into());
                }
                instructions |= f & 0x100 != 0;
                if f & 0x20 == 0 {
                    break;
                }
            }
            if instructions {
                let n = usize::from(r.u16()?);
                r.take(n)?;
            }
            shape
        } else {
            return Err(bad());
        }
    };
    shape.depth = shape.depth.max(1);
    let points = shape.rings.iter().map(Vec::len).sum::<usize>();
    *total += points;
    if *total > font_io::MAX_OUTLINE_SEGMENTS
        || shape
            .rings
            .iter()
            .flatten()
            .any(|(p, _)| p.iter().any(|v| !v.is_finite() || v.abs() > 32768.))
    {
        return Err("font_component_limit".into());
    }
    active.remove(&id);
    out.insert(id, shape.clone());
    Ok(shape)
}
fn simple(r: &mut Reader<'_>, count: usize) -> Result<Shape> {
    if count == 0 && r.p == r.b.len() {
        return Ok(Shape::default());
    }
    let mut ends = vec![];
    for _ in 0..count {
        let end = usize::from(r.u16()?);
        if ends.last().is_some_and(|&p| p >= end) {
            return Err(bad());
        }
        ends.push(end);
    }
    let n = ends.last().map_or(0, |v| v + 1);
    let instructions = usize::from(r.u16()?);
    r.take(instructions)?;
    let mut flags = vec![];
    while flags.len() < n {
        let f = r.u8()?;
        if f & 0x80 != 0 {
            return Err(bad());
        }
        let copies = if f & 8 != 0 {
            usize::from(r.u8()?) + 1
        } else {
            1
        };
        if flags.len() + copies > n {
            return Err(bad());
        }
        flags.extend(std::iter::repeat_n(f, copies));
    }
    let mut points = vec![([0., 0.], false); n];
    for axis in 0..2 {
        let mut at = 0i32;
        for (i, &f) in flags.iter().enumerate() {
            let short = if axis == 0 { 2 } else { 4 };
            let same = if axis == 0 { 16 } else { 32 };
            let delta = if f & short != 0 {
                let v = i32::from(r.u8()?);
                if f & same != 0 {
                    v
                } else {
                    -v
                }
            } else if f & same != 0 {
                0
            } else {
                i32::from(r.i16()?)
            };
            at = at.checked_add(delta).ok_or_else(bad)?;
            points[i].0[axis] = f64::from(at);
            points[i].1 = f & 1 != 0;
        }
    }
    let mut rings = vec![];
    let mut start = 0;
    for end in ends {
        rings.push(points[start..=end].to_vec());
        start = end + 1;
    }
    Ok(Shape {
        depth: 1,
        metrics: false,
        rings,
        parts: vec![],
    })
}
/// Explicit structure-preserving import. Failure leaves callers' current document untouched.
pub fn import(bytes: &[u8], index: u32) -> Result<FontDocument> {
    import_mode(bytes, index, true)
}
pub fn import_outlines(bytes: &[u8], index: u32) -> Result<FontDocument> {
    import_mode(bytes, index, false)
}
fn import_mode(bytes: &[u8], index: u32, keep_layout: bool) -> Result<FontDocument> {
    let face = ttf_parser::Face::parse(bytes, index).map_err(|_| bad())?;
    if face.is_variable() {
        return Err("font_invalid_components".into());
    }
    let doc = if keep_layout {
        font_io::import(bytes, index)?
    } else {
        font_io::import_outlines(bytes, index)?
    };
    let shapes = decode(bytes, index)?;
    crate::components::import_truetype(doc, &shapes).map_err(|e| match e {
        crate::Error::TooLarge | crate::Error::ComponentDepth => "font_component_limit".into(),
        _ => bad(),
    })
}

#[cfg(test)]
mod tests;

pub(crate) fn apply_shapes(
    doc: &mut FontDocument,
    shapes: &BTreeMap<u16, Shape>,
    bytes: &[u8],
    index: u32,
) -> Result<()> {
    if shapes.values().any(|s| s.metrics) {
        let library = freetype::Library::init().map_err(|_| bad())?;
        let face = library
            .new_memory_face(bytes.to_vec(), index as isize)
            .map_err(|_| bad())?;
        for (&id, shape) in shapes {
            if shape.metrics {
                face.load_glyph(
                    u32::from(id),
                    freetype::face::LoadFlag::NO_SCALE
                        | freetype::face::LoadFlag::NO_HINTING
                        | freetype::face::LoadFlag::NO_BITMAP,
                )
                .map_err(|_| bad())?;
                doc.glyphs.get_mut(&u32::from(id)).ok_or_else(bad)?.advance =
                    face.glyph().advance().x as f64;
            }
        }
    }
    for (&id, shape) in shapes {
        doc.glyphs.get_mut(&u32::from(id)).ok_or_else(bad)?.contours = shape.contours();
    }
    Ok(())
}
