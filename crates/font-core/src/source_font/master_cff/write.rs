use super::{
    codec,
    value::{self, Point, Value},
    Curve, Font, Glyph,
};
use crate::{
    source_font::{
        master_store::{Builder, Store},
        masters::Support,
    },
    Error, Result,
};
use std::collections::{BTreeMap, BTreeSet};
struct Writer {
    bytes: Vec<u8>,
    regions: Vec<u16>,
    stack: usize,
    error: f64,
    budget: usize,
}
impl Writer {
    fn number(&mut self, v: f64) -> Result<()> {
        self.error += codec::number(&mut self.bytes, v)?;
        self.stack += 1;
        if self.stack > 513 || self.bytes.len() > 8 * 1024 * 1024 {
            return Err(Error::TooLarge);
        }
        Ok(())
    }
    fn value(&mut self, value: &Value) -> Result<()> {
        self.budget += 1;
        if self.budget > 1_000_000 {
            return Err(Error::TooLarge);
        }
        let base = value.0.get(&vec![]).copied().unwrap_or(0.);
        let mut groups = BTreeMap::<u16, Value>::new();
        for (key, &coefficient) in &value.0 {
            if let Some((&region, tail)) = key.split_first() {
                *groups
                    .entry(region)
                    .or_default()
                    .0
                    .entry(tail.to_vec())
                    .or_default() += coefficient;
            }
        }
        self.number(base)?;
        if groups.is_empty() {
            return Ok(());
        }
        let regions = self.regions.clone();
        for region in &regions {
            self.value(&groups.remove(region).unwrap_or_default())?;
        }
        if !groups.is_empty() {
            return Err(Error::InvalidFile);
        }
        self.number(1.)?;
        self.bytes.push(16);
        self.stack = self
            .stack
            .checked_sub(regions.len() + 1)
            .ok_or(Error::InvalidFile)?;
        Ok(())
    }
    fn point(&mut self, p: &Point) -> Result<()> {
        self.value(&p[0])?;
        self.value(&p[1])
    }
    fn op(&mut self, op: u8) {
        self.bytes.push(op);
        self.stack = 0;
    }
}
fn glyph(glyph: &Glyph, store: &mut Builder<'_>, supports: &[Support]) -> Result<(Vec<u8>, f64)> {
    let mut used: BTreeSet<_> = glyph
        .values()
        .iter()
        .flat_map(|p| {
            p.iter()
                .flat_map(|v| v.0.keys().flat_map(|k| k.iter().copied()))
        })
        .filter(|r| *r < 0x8000)
        .collect();
    used.extend(
        glyph
            .branches
            .iter()
            .flat_map(|b| b.sum.iter().chain(std::iter::once(&b.last)))
            .flat_map(|v| v.0.keys().flat_map(|k| k.iter().copied()))
            .filter(|r| *r < 0x8000),
    );
    if used.len() > 64 {
        return Err(Error::TooLarge);
    }
    let regions: Vec<_> = used.into_iter().collect();
    let selected = regions
        .iter()
        .map(|&r| {
            supports
                .get(usize::from(r))
                .cloned()
                .ok_or(Error::InvalidFile)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut w = Writer {
        bytes: vec![],
        regions,
        stack: 0,
        error: 0.,
        budget: 0,
    };
    if !w.regions.is_empty() {
        let index = store.add_regions(&selected)?;
        w.number(f64::from(index))?;
        w.op(15);
    }
    let m = glyph.matrix;
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-20 {
        return Err(Error::InvalidGeometry);
    }
    let inverse = |p: &Point| -> Result<Point> {
        let x = p[0].plus(&Value::constant(m[4]), -1.)?;
        let y = p[1].plus(&Value::constant(m[5]), -1.)?;
        Ok([
            x.scale(m[3] / det)?.add(&y.scale(-m[2] / det)?)?,
            x.scale(-m[1] / det)?.add(&y.scale(m[0] / det)?)?,
        ])
    };
    let mut cursor = value::zero();
    for (path_index, path) in glyph.paths.iter().enumerate() {
        let start = inverse(&path.start)?;
        w.point(&value::sub(&start, &cursor)?)?;
        w.op(21);
        cursor = start;
        let mut segment = 0;
        while segment < path.segments.len() {
            if let Some((branch_index, branch)) = glyph
                .branches
                .iter()
                .enumerate()
                .find(|(_, b)| b.path == path_index && b.segment == segment)
            {
                let (Curve::Cubic(a, b, c), Curve::Cubic(d, e, f)) = (
                    &path.segments[segment],
                    path.segments
                        .get(segment + 1)
                        .ok_or(Error::InvalidGeometry)?,
                ) else {
                    return Err(Error::InvalidGeometry);
                };
                let a = inverse(a)?;
                let b = inverse(b)?;
                let c = inverse(c)?;
                let d = inverse(d)?;
                let e = inverse(e)?;
                let f = inverse(f)?;
                let sum = value::sub(&e, &cursor)?;
                let matches = |a: &Value, b: &Value| -> Result<bool> {
                    Ok(a.plus(b, -1.)?.0.values().all(|v| v.abs() < 1e-8))
                };
                for axis in 0..2 {
                    if !matches(&sum[axis], &branch.sum[axis])? {
                        return Err(Error::InvalidGeometry);
                    }
                    let end = e[axis].add(
                        &Value::constant(1.).region((0x8000 + branch_index * 2 + axis) as u16)?,
                    )?;
                    if !matches(&f[axis], &end)? {
                        return Err(Error::InvalidGeometry);
                    }
                }
                let args = [
                    value::sub(&a, &cursor)?,
                    value::sub(&b, &a)?,
                    value::sub(&c, &b)?,
                    value::sub(&d, &c)?,
                ];
                let mut total = value::zero();
                for arg in args {
                    let mut quantized = value::zero();
                    for axis in 0..2 {
                        for (key, &coefficient) in &arg[axis].0 {
                            let rounded = (coefficient * 65536.).round() / 65536.;
                            w.error += (coefficient - rounded).abs() * 2.;
                            if rounded != 0. {
                                quantized[axis].0.insert(key.clone(), rounded);
                            }
                        }
                    }
                    w.point(&quantized)?;
                    total = value::add(&total, &quantized)?;
                }
                w.point(&value::sub(&branch.sum, &total)?)?;
                w.value(&branch.last)?;
                w.bytes.extend([12, 37]);
                w.stack = 0;
                cursor = f;
                segment += 2;
                continue;
            }
            let curve = &path.segments[segment];
            match curve {
                Curve::Line(p) => {
                    let p = inverse(p)?;
                    w.point(&value::sub(&p, &cursor)?)?;
                    w.op(5);
                    cursor = p;
                }
                Curve::Cubic(a, b, c) => {
                    let a = inverse(a)?;
                    let b = inverse(b)?;
                    let c = inverse(c)?;
                    w.point(&value::sub(&a, &cursor)?)?;
                    w.point(&value::sub(&b, &a)?)?;
                    w.point(&value::sub(&c, &b)?)?;
                    w.op(8);
                    cursor = c;
                }
            }
            segment += 1;
        }
    }
    // Every serialized relative operand can contribute to a later point. Matrix norm
    // converts the cumulative fixed-point error to a conservative font-unit bound.
    Ok((
        w.bytes,
        w.error * (m[0].abs() + m[2].abs()).max(m[1].abs() + m[3].abs()),
    ))
}
pub fn font(
    font: &Font<'_>,
    glyphs: &BTreeMap<u16, Glyph>,
    supports: &[Support],
) -> Result<(Vec<u8>, f64)> {
    if glyphs.is_empty() {
        return Ok((font.bytes.to_vec(), 0.));
    }
    let mut store = Builder::new(Store::parse(&font.store, &font.axes)?);
    let mut chars: Vec<_> = font.chars.iter().map(|b| b.to_vec()).collect();
    let mut error = 0f64;
    for (&id, g) in glyphs {
        let (bytes, bound) = glyph(g, &mut store, supports)?;
        *chars.get_mut(usize::from(id)).ok_or(Error::MissingGlyph)? = bytes;
        error = error.max(bound);
    }
    let chars = codec::write_index(&chars)?;
    let store = store.finish()?;
    let mut entries = font.top.clone();
    for op in [17, 1236, 24] {
        if !entries.iter().any(|d| d.op == op) {
            entries.push(codec::Dict {
                op,
                values: vec![0.],
                raw: vec![],
            });
        }
    }
    let top = |shift: i32, chars: i32, fonts: i32, store: i32| -> Result<Vec<u8>> {
        let mut out = vec![];
        for d in &entries {
            match d.op {
                17 => codec::dict_values(&mut out, 17, &[chars]),
                1236 => codec::dict_values(&mut out, 1236, &[fonts]),
                24 => codec::dict_values(&mut out, 24, &[store]),
                1237 => {
                    let offset = codec::offset(&entries, 1237)?.ok_or(Error::InvalidFile)?;
                    codec::dict_values(
                        &mut out,
                        1237,
                        &[(offset as i32).checked_add(shift).ok_or(Error::TooLarge)?],
                    );
                }
                _ => out.extend(&d.raw),
            }
        }
        Ok(out)
    };
    let placeholder = top(0, 0, 0, 0)?;
    let new_top_end = font
        .header
        .checked_add(placeholder.len())
        .ok_or(Error::TooLarge)?;
    let shift = i32::try_from(new_top_end).map_err(|_| Error::TooLarge)? - font.top_end as i32;
    let body = font.bytes.get(font.top_end..).ok_or(Error::InvalidFile)?;
    let chars_at = new_top_end + body.len();
    let fonts_at = chars_at + chars.len();
    let mut fonts = vec![];
    for dict in &font.fonts {
        let mut out = vec![];
        for d in dict {
            if d.op == 18 {
                if d.values.len() != 2 {
                    return Err(Error::InvalidFile);
                }
                codec::dict_values(
                    &mut out,
                    18,
                    &[
                        d.values[0] as i32,
                        (d.values[1] as i32)
                            .checked_add(shift)
                            .ok_or(Error::TooLarge)?,
                    ],
                );
            } else {
                out.extend(&d.raw);
            }
        }
        fonts.push(out);
    }
    let fonts = codec::write_index(&fonts)?;
    let store_at = fonts_at + fonts.len();
    if store_at + store.len() + 2 > 128 * 1024 * 1024 {
        return Err(Error::TooLarge);
    }
    let top = top(shift, chars_at as i32, fonts_at as i32, store_at as i32)?;
    let mut out = font.bytes[..font.header].to_vec();
    out[3..5].copy_from_slice(
        &u16::try_from(top.len())
            .map_err(|_| Error::TooLarge)?
            .to_be_bytes(),
    );
    out.extend(top);
    out.extend(body);
    out.extend(chars);
    out.extend(fonts);
    out.extend((store.len().min(65535) as u16).to_be_bytes());
    out.extend(store);
    Ok((out, error))
}
