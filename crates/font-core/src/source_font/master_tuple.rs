//! Bounded gvar tuple codecs and unrounded IUP reconstruction.
//! See https://learn.microsoft.com/en-us/typography/opentype/spec/gvar
use crate::{Error, Point, Result};
const LIMIT: usize = 1_000_000;
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self.bytes.get(self.at).ok_or(Error::InvalidFile)?;
        self.at += 1;
        Ok(b)
    }
    fn word(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes([self.byte()?, self.byte()?]))
    }
}
pub fn read_points(bytes: &[u8], point_count: usize) -> Result<(Option<Vec<usize>>, usize)> {
    if point_count > LIMIT {
        return Err(Error::TooLarge);
    }
    let mut r = Reader { bytes, at: 0 };
    let first = r.byte()?;
    if first == 0 {
        return Ok((None, r.at));
    }
    let count = if first & 0x80 != 0 {
        (usize::from(first & 0x7f) << 8) | usize::from(r.byte()?)
    } else {
        usize::from(first)
    };
    if count == 0 || count > point_count {
        return Err(Error::InvalidFile);
    }
    let mut points = Vec::with_capacity(count);
    let mut previous = 0usize;
    while points.len() < count {
        let control = r.byte()?;
        let n = usize::from(control & 0x7f) + 1;
        if n > count - points.len() {
            return Err(Error::InvalidFile);
        }
        for _ in 0..n {
            let delta = usize::from(if control & 0x80 != 0 {
                r.word()?
            } else {
                u16::from(r.byte()?)
            });
            let point = previous.checked_add(delta).ok_or(Error::InvalidFile)?;
            if point >= point_count || (!points.is_empty() && point <= previous) {
                return Err(Error::InvalidFile);
            }
            points.push(point);
            previous = point;
        }
    }
    Ok((Some(points), r.at))
}
pub fn write_points(points: Option<&[usize]>, point_count: usize) -> Result<Vec<u8>> {
    if point_count > LIMIT {
        return Err(Error::TooLarge);
    }
    let Some(points) = points else {
        return Ok(vec![0]);
    };
    if points.is_empty()
        || points.len() > 32767
        || points.iter().any(|&p| p >= point_count)
        || points.windows(2).any(|p| p[0] >= p[1])
    {
        return Err(Error::InvalidGeometry);
    }
    let count = points.len();
    let mut out = if count < 128 {
        vec![count as u8]
    } else {
        vec![0x80 | ((count >> 8) as u8), (count & 255) as u8]
    };
    let mut deltas = Vec::with_capacity(count);
    let mut previous = 0;
    for &p in points {
        deltas.push(u16::try_from(p - previous).map_err(|_| Error::TooLarge)?);
        previous = p;
    }
    let mut at = 0;
    while at < count {
        let wide = deltas[at] > 255;
        let end = (at + 1..count.min(at + 128))
            .find(|&i| (deltas[i] > 255) != wide)
            .unwrap_or(count.min(at + 128));
        out.push((if wide { 0x80 } else { 0 }) | (end - at - 1) as u8);
        for &v in &deltas[at..end] {
            if wide {
                out.extend(v.to_be_bytes());
            } else {
                out.push(v as u8);
            }
        }
        at = end;
    }
    Ok(out)
}
pub fn read_deltas(bytes: &[u8], count: usize) -> Result<(Vec<i16>, usize)> {
    if count > LIMIT {
        return Err(Error::TooLarge);
    }
    let mut r = Reader { bytes, at: 0 };
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let control = r.byte()?;
        let n = usize::from(control & 0x3f) + 1;
        if n > count - out.len() {
            return Err(Error::InvalidFile);
        }
        for _ in 0..n {
            out.push(if control & 0x80 != 0 {
                0
            } else if control & 0x40 != 0 {
                r.word()? as i16
            } else {
                i16::from(r.byte()? as i8)
            });
        }
    }
    Ok((out, r.at))
}
pub fn write_deltas(values: &[i16]) -> Result<Vec<u8>> {
    if values.len() > LIMIT {
        return Err(Error::TooLarge);
    }
    let mut out = vec![];
    let mut at = 0;
    let kind = |v: i16| {
        if v == 0 {
            0
        } else if i8::try_from(v).is_ok() {
            1
        } else {
            2
        }
    };
    while at < values.len() {
        let k = kind(values[at]);
        let end = (at + 1..values.len().min(at + 64))
            .find(|&i| kind(values[i]) != k)
            .unwrap_or(values.len().min(at + 64));
        out.push(
            (match k {
                0 => 0x80,
                1 => 0,
                _ => 0x40,
            }) | (end - at - 1) as u8,
        );
        for &v in &values[at..end] {
            match k {
                0 => {}
                1 => out.push(v as i8 as u8),
                _ => out.extend(v.to_be_bytes()),
            }
        }
        at = end;
    }
    Ok(out)
}
fn interpolate(value: f64, a: f64, b: f64, da: f64, db: f64) -> f64 {
    if a == b {
        return if da == db { da } else { 0. };
    }
    let (a, b, da, db) = if a < b {
        (a, b, da, db)
    } else {
        (b, a, db, da)
    };
    if value <= a {
        da
    } else if value >= b {
        db
    } else {
        da + (db - da) * (value - a) / (b - a)
    }
}
/// Components and phantom points are not interpolated. An empty contour list handles composites.
pub fn infer_deltas(
    points: &[Point],
    ends: &[usize],
    deltas: &[Option<Point>],
    phantoms: usize,
) -> Result<Vec<Point>> {
    if points.len() > LIMIT {
        return Err(Error::TooLarge);
    }
    if points.len() != deltas.len()
        || phantoms > points.len()
        || points
            .iter()
            .flatten()
            .chain(deltas.iter().flatten().flatten())
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err(Error::InvalidGeometry);
    }
    let physical = points.len() - phantoms;
    if !ends.is_empty()
        && (ends.last().copied() != physical.checked_sub(1)
            || ends.windows(2).any(|p| p[0] >= p[1]))
    {
        return Err(Error::InvalidGeometry);
    }
    let mut out: Vec<_> = deltas.iter().map(|p| p.unwrap_or([0.; 2])).collect();
    let mut start = 0;
    for &end in ends {
        if end < start || end >= physical {
            return Err(Error::InvalidGeometry);
        }
        let touched: Vec<_> = (start..=end).filter(|&i| deltas[i].is_some()).collect();
        if touched.len() == 1 {
            for p in &mut out[start..=end] {
                *p = deltas[touched[0]].ok_or(Error::InvalidGeometry)?;
            }
        } else if !touched.is_empty() {
            let n = end - start + 1;
            for (i, &a) in touched.iter().enumerate() {
                let b = touched[(i + 1) % touched.len()];
                let mut at = start + (a - start + 1) % n;
                while at != b {
                    let da = deltas[a].ok_or(Error::InvalidGeometry)?;
                    let db = deltas[b].ok_or(Error::InvalidGeometry)?;
                    for axis in 0..2 {
                        out[at][axis] = interpolate(
                            points[at][axis],
                            points[a][axis],
                            points[b][axis],
                            da[axis],
                            db[axis],
                        );
                    }
                    at = start + (at - start + 1) % n;
                }
            }
        }
        start = end + 1;
    }
    Ok(out)
}
