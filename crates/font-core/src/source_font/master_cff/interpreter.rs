use super::{
    codec::{self, Reader},
    value::{self, Point, Value},
    Curve, Font, Glyph, Path,
};
use crate::{Error, Result};
struct Parser<'a, 'b> {
    font: &'a Font<'b>,
    fd: usize,
    stack: Vec<Value>,
    position: Point,
    paths: Vec<Path>,
    branches: Vec<super::Branch>,
    stems: usize,
    vs: usize,
    vs_seen: bool,
    blended: bool,
    budget: usize,
}
impl Parser<'_, '_> {
    fn count(&self, n: usize) -> Result<()> {
        if self.stack.len() != n {
            return Err(Error::InvalidFile);
        }
        Ok(())
    }
    fn delta(&mut self, p: Point) -> Result<Point> {
        self.position = value::add(&self.position, &p)?;
        self.budget += self.position.iter().map(|v| v.0.len()).sum::<usize>();
        if self.budget > 2_000_000 {
            return Err(Error::TooLarge);
        }
        Ok(self.position.clone())
    }
    fn line(&mut self, p: Point) -> Result<()> {
        let end = self.delta(p)?;
        self.paths
            .last_mut()
            .ok_or(Error::InvalidFile)?
            .segments
            .push(Curve::Line(end));
        Ok(())
    }
    fn curve(&mut self, a: Point, b: Point, c: Point) -> Result<()> {
        let a = self.delta(a)?;
        let b = self.delta(b)?;
        let c = self.delta(c)?;
        self.paths
            .last_mut()
            .ok_or(Error::InvalidFile)?
            .segments
            .push(Curve::Cubic(a, b, c));
        Ok(())
    }
    fn run(&mut self, bytes: &[u8], depth: usize) -> Result<()> {
        if depth > 10 {
            return Err(Error::TooLarge);
        }
        let mut r = Reader { bytes, at: 0 };
        while r.at < bytes.len() {
            self.budget += 1;
            if self.budget > 1_000_000 {
                return Err(Error::TooLarge);
            }
            let b = r.byte()?;
            if b >= 32 || b == 28 || b == 255 {
                self.stack.push(Value::constant(r.number(b, false)?));
                if self.stack.len() > 513 {
                    return Err(Error::TooLarge);
                }
                continue;
            }
            if b == 10 || b == 29 {
                let operand = self.stack.pop().ok_or(Error::InvalidFile)?.number()?;
                let subrs = if b == 10 {
                    &self.font.locals[self.fd]
                } else {
                    &self.font.globals
                };
                let bias = if subrs.len() < 1240 {
                    107
                } else if subrs.len() < 33900 {
                    1131
                } else {
                    32768
                };
                if operand.fract() != 0. {
                    return Err(Error::InvalidFile);
                }
                let index = operand as i64 + bias;
                let data = *subrs
                    .get(usize::try_from(index).map_err(|_| Error::InvalidFile)?)
                    .ok_or(Error::InvalidFile)?;
                self.run(data, depth + 1)?;
                continue;
            }
            if b == 11 {
                if depth == 0 {
                    return Err(Error::InvalidFile);
                }
                return Ok(());
            }
            if b == 15 {
                self.count(1)?;
                if self.vs_seen || self.blended {
                    return Err(Error::InvalidFile);
                }
                let value = self.stack.pop().ok_or(Error::InvalidFile)?.number()?;
                if value < 0. || value.fract() != 0. {
                    return Err(Error::InvalidFile);
                }
                self.vs = value as usize;
                self.font
                    .region_sets
                    .get(self.vs)
                    .ok_or(Error::InvalidFile)?;
                self.vs_seen = true;
                continue;
            }
            if b == 16 {
                let n = self.stack.pop().ok_or(Error::InvalidFile)?.number()?;
                if n < 1. || n > 513. || n.fract() != 0. {
                    return Err(Error::InvalidFile);
                }
                let n = n as usize;
                let regions = self
                    .font
                    .region_sets
                    .get(self.vs)
                    .ok_or(Error::InvalidFile)?;
                let len = n.checked_mul(regions.len() + 1).ok_or(Error::TooLarge)?;
                let start = self
                    .stack
                    .len()
                    .checked_sub(len)
                    .ok_or(Error::InvalidFile)?;
                let args = self.stack.split_off(start);
                for i in 0..n {
                    let mut value = args[i].clone();
                    for (j, &region) in regions.iter().enumerate() {
                        let delta = &args[n + i * regions.len() + j];
                        let support = self
                            .font
                            .supports
                            .get(usize::from(region))
                            .ok_or(Error::InvalidFile)?;
                        value = value.add(&if support.axes.is_empty() {
                            delta.clone()
                        } else {
                            delta.region(region)?
                        })?;
                    }
                    self.stack.push(value);
                }
                self.blended = true;
                continue;
            }
            if matches!(b, 1 | 3 | 18 | 23 | 19 | 20) {
                if !self.stack.len().is_multiple_of(2) {
                    return Err(Error::InvalidFile);
                }
                self.stems += self.stack.len() / 2;
                self.stack.clear();
                if self.stems > 65535 {
                    return Err(Error::TooLarge);
                }
                if b == 19 || b == 20 {
                    r.take(self.stems.div_ceil(8))?;
                }
                continue;
            }
            let args = std::mem::take(&mut self.stack);
            let z = Value::constant(0.);
            match b {
                4 | 21 | 22 => {
                    let n = if b == 21 { 2 } else { 1 };
                    if args.len() != n {
                        return Err(Error::InvalidFile);
                    }
                    let p = match b {
                        4 => [z.clone(), args[0].clone()],
                        22 => [args[0].clone(), z.clone()],
                        _ => [args[0].clone(), args[1].clone()],
                    };
                    let start = self.delta(p)?;
                    if self.paths.len() >= 4096 {
                        return Err(Error::TooLarge);
                    }
                    self.paths.push(Path {
                        start,
                        segments: vec![],
                    });
                }
                5 => {
                    if args.is_empty() || !args.len().is_multiple_of(2) {
                        return Err(Error::InvalidFile);
                    }
                    for pair in args.as_chunks::<2>().0 {
                        self.line(pair.clone())?;
                    }
                }
                6 | 7 => {
                    if args.is_empty() {
                        return Err(Error::InvalidFile);
                    }
                    for (i, v) in args.into_iter().enumerate() {
                        let mut p = value::zero();
                        p[(usize::from(b == 7) + i) % 2] = v;
                        self.line(p)?;
                    }
                }
                8 => {
                    if args.is_empty() || !args.len().is_multiple_of(6) {
                        return Err(Error::InvalidFile);
                    }
                    for a in args.as_chunks::<6>().0 {
                        self.curve(
                            [a[0].clone(), a[1].clone()],
                            [a[2].clone(), a[3].clone()],
                            [a[4].clone(), a[5].clone()],
                        )?;
                    }
                }
                24 => {
                    if args.len() < 8 || !(args.len() - 2).is_multiple_of(6) {
                        return Err(Error::InvalidFile);
                    }
                    let split = args.len() - 2;
                    for a in args[..split].as_chunks::<6>().0 {
                        self.curve(
                            [a[0].clone(), a[1].clone()],
                            [a[2].clone(), a[3].clone()],
                            [a[4].clone(), a[5].clone()],
                        )?;
                    }
                    self.line([args[split].clone(), args[split + 1].clone()])?;
                }
                25 => {
                    if args.len() < 8 || !(args.len() - 6).is_multiple_of(2) {
                        return Err(Error::InvalidFile);
                    }
                    let split = args.len() - 6;
                    for p in args[..split].as_chunks::<2>().0 {
                        self.line(p.clone())?;
                    }
                    let a = &args[split..];
                    self.curve(
                        [a[0].clone(), a[1].clone()],
                        [a[2].clone(), a[3].clone()],
                        [a[4].clone(), a[5].clone()],
                    )?;
                }
                26 | 27 => {
                    let extra = args.len() % 4;
                    if args.len() < 4 || extra > 1 {
                        return Err(Error::InvalidFile);
                    }
                    let axis = usize::from(b == 26);
                    for (i, a) in args[extra..].as_chunks::<4>().0.iter().enumerate() {
                        let mut p = value::zero();
                        let mut q = value::zero();
                        let mut s = value::zero();
                        p[axis] = a[0].clone();
                        if i == 0 && extra == 1 {
                            p[1 - axis] = args[0].clone();
                        }
                        q[axis] = a[1 + axis].clone();
                        q[1 - axis] = a[2 - axis].clone();
                        s[axis] = a[3].clone();
                        self.curve(p, q, s)?;
                    }
                }
                30 | 31 => {
                    let extra = args.len() % 4;
                    if args.len() < 4 || extra > 1 {
                        return Err(Error::InvalidFile);
                    }
                    let len = args.len() - extra;
                    for (i, a) in args[..len].as_chunks::<4>().0.iter().enumerate() {
                        let axis = (usize::from(b == 30) + i) % 2;
                        let mut p = value::zero();
                        p[axis] = a[0].clone();
                        let q = [a[1].clone(), a[2].clone()];
                        let mut s = value::zero();
                        s[1 - axis] = a[3].clone();
                        if extra == 1 && i + 1 == len / 4 {
                            s[axis] = args[len].clone();
                        }
                        self.curve(p, q, s)?;
                    }
                }
                12 => match r.byte()? {
                    34 => {
                        if args.len() != 7 {
                            return Err(Error::InvalidFile);
                        }
                        self.curve(
                            [args[0].clone(), z.clone()],
                            [args[1].clone(), args[2].clone()],
                            [args[3].clone(), z.clone()],
                        )?;
                        self.curve(
                            [args[4].clone(), z.clone()],
                            [args[5].clone(), args[2].scale(-1.)?],
                            [args[6].clone(), z.clone()],
                        )?;
                    }
                    35 => {
                        if args.len() != 13 {
                            return Err(Error::InvalidFile);
                        }
                        for a in args[..12].as_chunks::<6>().0 {
                            self.curve(
                                [a[0].clone(), a[1].clone()],
                                [a[2].clone(), a[3].clone()],
                                [a[4].clone(), a[5].clone()],
                            )?;
                        }
                    }
                    36 => {
                        if args.len() != 9 {
                            return Err(Error::InvalidFile);
                        }
                        self.curve(
                            [args[0].clone(), args[1].clone()],
                            [args[2].clone(), args[3].clone()],
                            [args[4].clone(), z.clone()],
                        )?;
                        let dy = args[1].add(&args[3])?.add(&args[7])?.scale(-1.)?;
                        self.curve(
                            [args[5].clone(), z.clone()],
                            [args[6].clone(), args[7].clone()],
                            [args[8].clone(), dy],
                        )?;
                    }
                    37 => {
                        if args.len() != 11 {
                            return Err(Error::InvalidFile);
                        }
                        let mut sum = value::zero();
                        for p in args[..10].as_chunks::<2>().0 {
                            sum = value::add(&sum, p)?;
                        }
                        let abs = |v: [f64; 2]| {
                            if v[0] <= 0. && v[1] >= 0. {
                                [0., v[0].abs().max(v[1].abs())]
                            } else {
                                [v[0].abs().min(v[1].abs()), v[0].abs().max(v[1].abs())]
                            }
                        };
                        let x = abs(sum[0].interval());
                        let y = abs(sum[1].interval());
                        let end = if x[0] > y[1] || y[0] >= x[1] {
                            let axis = usize::from(y[0] >= x[1]);
                            let mut end = [sum[0].scale(-1.)?, sum[1].scale(-1.)?];
                            end[axis] = args[10].clone();
                            end
                        } else {
                            if self.branches.len() >= 64 {
                                return Err(Error::TooLarge);
                            }
                            let index = self.branches.len();
                            self.branches.push(super::Branch {
                                path: self.paths.len().checked_sub(1).ok_or(Error::InvalidFile)?,
                                segment: self
                                    .paths
                                    .last()
                                    .ok_or(Error::InvalidFile)?
                                    .segments
                                    .len(),
                                sum,
                                last: args[10].clone(),
                            });
                            [
                                Value::constant(1.).region((0x8000 + index * 2) as u16)?,
                                Value::constant(1.).region((0x8000 + index * 2 + 1) as u16)?,
                            ]
                        };
                        self.curve(
                            [args[0].clone(), args[1].clone()],
                            [args[2].clone(), args[3].clone()],
                            [args[4].clone(), args[5].clone()],
                        )?;
                        self.curve(
                            [args[6].clone(), args[7].clone()],
                            [args[8].clone(), args[9].clone()],
                            end,
                        )?;
                    }
                    _ => return Err(Error::InvalidFile),
                },
                _ => return Err(Error::InvalidFile),
            }
        }
        Ok(())
    }
}
fn matrix(dict: &[codec::Dict], default: [f64; 6]) -> Result<[f64; 6]> {
    let Some(d) = dict.iter().find(|d| d.op == 1207) else {
        return Ok(default);
    };
    let m: [f64; 6] = d
        .values
        .as_slice()
        .try_into()
        .map_err(|_| Error::InvalidFile)?;
    if m.iter().any(|v| !v.is_finite()) || (m[0] * m[3] - m[1] * m[2]).abs() < 1e-20 {
        return Err(Error::InvalidGeometry);
    }
    Ok(m)
}
pub fn parse(font: &Font<'_>, id: u16) -> Result<Glyph> {
    let fd = *font
        .selected
        .get(usize::from(id))
        .ok_or(Error::MissingGlyph)?;
    let mut p = Parser {
        font,
        fd,
        stack: vec![],
        position: value::zero(),
        paths: vec![],
        branches: vec![],
        stems: 0,
        vs: 0,
        vs_seen: false,
        blended: false,
        budget: 0,
    };
    p.run(font.chars[usize::from(id)], 0)?;
    if !p.stack.is_empty() {
        return Err(Error::InvalidFile);
    }
    let a = matrix(&font.top, [0.001, 0., 0., 0.001, 0., 0.])?;
    let scale = f64::from(font.upm);
    if (a[0] * scale - 1.).abs() > 1e-6
        || a[0] != a[3]
        || [a[1], a[2], a[4], a[5]].iter().any(|&v| v != 0.)
    {
        return Err(Error::InvalidGeometry);
    }
    let b = matrix(&font.fonts[fd], [1., 0., 0., 1., 0., 0.])?;
    if b != [1., 0., 0., 1., 0., 0.] {
        return Err(Error::InvalidGeometry);
    }
    let m = [1., 0., 0., 1., 0., 0.];
    let transform = |p: &Point| -> Result<Point> {
        Ok([
            p[0].scale(m[0])?
                .add(&p[1].scale(m[2])?)?
                .add(&Value::constant(m[4]))?,
            p[0].scale(m[1])?
                .add(&p[1].scale(m[3])?)?
                .add(&Value::constant(m[5]))?,
        ])
    };
    for path in &mut p.paths {
        path.start = transform(&path.start)?;
        for curve in &mut path.segments {
            match curve {
                Curve::Line(p) => *p = transform(p)?,
                Curve::Cubic(a, b, c) => {
                    *a = transform(a)?;
                    *b = transform(b)?;
                    *c = transform(c)?;
                }
            }
        }
    }
    Ok(Glyph {
        paths: p.paths,
        branches: p.branches,
        matrix: m,
    })
}
