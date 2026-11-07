//! CFF2 paths with symbolic nested blends, independent of rasterizer point pruning.
pub(crate) mod codec;
mod export;
pub use export::export;
mod interpreter;
mod value;
mod write;
use super::{
    master_store,
    masters::{Location, Support},
};
use crate::{Contour, Error, Result, Segment};
use codec::{Dict, Reader};
use std::collections::BTreeMap;
pub use value::{Point, Value};
#[derive(Clone)]
pub struct Path {
    pub start: Point,
    pub segments: Vec<Curve>,
}
#[derive(Clone)]
pub enum Curve {
    Line(Point),
    Cubic(Point, Point, Point),
}
#[derive(Clone)]
pub struct Glyph {
    pub(crate) branches: Vec<Branch>,
    pub paths: Vec<Path>,
    pub matrix: [f64; 6],
}
#[derive(Clone)]
pub(crate) struct Branch {
    pub path: usize,
    pub segment: usize,
    pub sum: Point,
    pub last: Value,
}
impl Glyph {
    fn branch_weights(&self, weights: &[f64]) -> Result<Vec<f64>> {
        let mut out = weights.to_vec();
        if self.branches.is_empty() {
            return Ok(out);
        }
        out.resize(0x8000 + self.branches.len() * 2, 0.);
        for (i, b) in self.branches.iter().enumerate() {
            let x = b.sum[0].eval(&out)?;
            let y = b.sum[1].eval(&out)?;
            let last = b.last.eval(&out)?;
            let delta = if x.abs() > y.abs() {
                [last, -y]
            } else {
                [-x, last]
            };
            out[0x8000 + i * 2] = delta[0];
            out[0x8000 + i * 2 + 1] = delta[1];
        }
        Ok(out)
    }

    pub fn points(&self, weights: &[f64]) -> Result<Vec<crate::Point>> {
        let weights = self.branch_weights(weights)?;
        self.values()
            .iter()
            .map(|p| Ok([p[0].eval(&weights)?, p[1].eval(&weights)?]))
            .collect()
    }
    pub fn values(&self) -> Vec<&Point> {
        let mut out = vec![];
        for p in &self.paths {
            out.push(&p.start);
            for s in &p.segments {
                match s {
                    Curve::Line(p) => out.push(p),
                    Curve::Cubic(a, b, c) => out.extend([a, b, c]),
                }
            }
        }
        out
    }
    pub fn contours(&self, weights: &[f64]) -> Result<Vec<Contour>> {
        let weights = self.branch_weights(weights)?;
        let point = |p: &Point| -> Result<crate::Point> {
            Ok([p[0].eval(&weights)?, p[1].eval(&weights)?])
        };
        self.paths
            .iter()
            .map(|p| {
                Ok(Contour {
                    start: point(&p.start)?,
                    segments: p
                        .segments
                        .iter()
                        .map(|c| match c {
                            Curve::Line(p) => Ok(Segment::Line { end: point(p)? }),
                            Curve::Cubic(a, b, c) => Ok(Segment::Cubic {
                                control1: point(a)?,
                                control2: point(b)?,
                                end: point(c)?,
                            }),
                        })
                        .chain(std::iter::once(Ok(Segment::Line {
                            end: point(&p.start)?,
                        })))
                        .collect::<Result<_>>()?,
                    closed: true,
                    identity: Default::default(),
                    smooth: Default::default(),
                })
            })
            .collect()
    }
}
pub struct Font<'a> {
    pub bytes: &'a [u8],
    pub top: Vec<Dict>,
    pub top_end: usize,
    pub header: usize,
    pub chars: Vec<&'a [u8]>,
    pub globals: Vec<&'a [u8]>,
    pub fonts: Vec<Vec<Dict>>,
    pub locals: Vec<Vec<&'a [u8]>>,
    pub selected: Vec<usize>,
    pub supports: Vec<Support>,
    pub region_sets: Vec<Vec<u16>>,
    pub store: Vec<u8>,
    pub axes: Vec<String>,
    pub upm: u16,
}
impl<'a> Font<'a> {
    pub fn parse(bytes: &'a [u8], axes: &[String], glyphs: u16, upm: u16) -> Result<Self> {
        if bytes.len() > 128 * 1024 * 1024 || bytes.first() != Some(&2) {
            return Err(Error::InvalidFile);
        }
        let mut r = Reader { bytes, at: 2 };
        let header = usize::from(r.byte()?);
        let len = usize::from(r.word()?);
        if header < 5 {
            return Err(Error::InvalidFile);
        }
        let top_end = header.checked_add(len).ok_or(Error::InvalidFile)?;
        let top = codec::dict(bytes.get(header..top_end).ok_or(Error::InvalidFile)?)?;
        let (chars, _) = codec::index(bytes, codec::offset(&top, 17)?.ok_or(Error::InvalidFile)?)?;
        if chars.len() != usize::from(glyphs) {
            return Err(Error::InvalidFile);
        }
        let (globals, _) = codec::index(bytes, top_end)?;
        let rawfonts = codec::offset(&top, 1236)?
            .map(|at| codec::index(bytes, at).map(|p| p.0))
            .transpose()?
            .unwrap_or_default();
        let fonts = if rawfonts.is_empty() {
            vec![vec![]]
        } else {
            rawfonts
                .into_iter()
                .map(codec::dict)
                .collect::<Result<Vec<_>>>()?
        };
        let mut locals = vec![];
        for font in &fonts {
            let private = font.iter().find(|d| d.op == 18);
            let subrs = if let Some(p) = private {
                if p.values.len() != 2 || p.values.iter().any(|v| *v < 0. || v.fract() != 0.) {
                    return Err(Error::InvalidFile);
                }
                let start = p.values[1] as usize;
                let size = p.values[0] as usize;
                let dict = codec::dict(
                    bytes
                        .get(start..start.checked_add(size).ok_or(Error::InvalidFile)?)
                        .ok_or(Error::InvalidFile)?,
                )?;
                codec::offset(&dict, 19)?
                    .map(|offset| {
                        codec::index(bytes, start.checked_add(offset).ok_or(Error::InvalidFile)?)
                            .map(|p| p.0)
                    })
                    .transpose()?
                    .unwrap_or_default()
            } else {
                vec![]
            };
            locals.push(subrs);
        }
        let selected = select(bytes, codec::offset(&top, 1237)?, glyphs, fonts.len())?;
        let store = if let Some(at) = codec::offset(&top, 24)? {
            let mut r = Reader { bytes, at };
            let len = usize::from(r.word()?);
            if len == 65535 {
                bytes.get(r.at..).ok_or(Error::InvalidFile)?.to_vec()
            } else {
                r.take(len)?.to_vec()
            }
        } else {
            let mut b = vec![0, 1, 0, 0, 0, 8, 0, 0];
            b.extend((axes.len() as u16).to_be_bytes());
            b.extend([0, 0]);
            b
        };
        let parsed = master_store::Store::parse(&store, axes)?;
        let supports = (0..parsed.region_count())
            .map(|i| parsed.support(i as u16))
            .collect::<Result<Vec<_>>>()?;
        let mut r = Reader {
            bytes: &store,
            at: 6,
        };
        let count = r.word()?;
        let region_sets = (0..count)
            .map(|i| parsed.regions_for_data(i))
            .collect::<Result<_>>()?;
        Ok(Self {
            bytes,
            top,
            top_end,
            header,
            chars,
            globals,
            fonts,
            locals,
            selected,
            supports,
            region_sets,
            store,
            axes: axes.to_vec(),
            upm,
        })
    }
    pub fn glyph(&self, id: u16) -> Result<Glyph> {
        interpreter::parse(self, id)
    }
    pub fn weights(&self, location: &Location) -> Result<Vec<f64>> {
        self.supports.iter().map(|s| s.weight(location)).collect()
    }
    pub fn rewrite(
        &self,
        glyphs: &BTreeMap<u16, Glyph>,
        supports: &[Support],
    ) -> Result<(Vec<u8>, f64)> {
        write::font(self, glyphs, supports)
    }
}
fn select(bytes: &[u8], offset: Option<usize>, glyphs: u16, fonts: usize) -> Result<Vec<usize>> {
    let Some(at) = offset else {
        return if fonts == 1 {
            Ok(vec![0; usize::from(glyphs)])
        } else {
            Err(Error::InvalidFile)
        };
    };
    let mut r = Reader { bytes, at };
    let format = r.byte()?;
    let mut out = vec![];
    if format == 0 {
        for _ in 0..glyphs {
            out.push(usize::from(r.byte()?));
        }
    } else if format == 3 || format == 4 {
        let count = if format == 3 {
            usize::from(r.word()?)
        } else {
            r.long()? as usize
        };
        if count == 0 || count > usize::from(glyphs) {
            return Err(Error::InvalidFile);
        }
        let mut ranges = vec![];
        for _ in 0..count {
            let start = if format == 3 {
                u32::from(r.word()?)
            } else {
                r.long()?
            };
            let fd = if format == 3 {
                u16::from(r.byte()?)
            } else {
                r.word()?
            };
            ranges.push((start, usize::from(fd)));
        }
        let end = if format == 3 {
            u32::from(r.word()?)
        } else {
            r.long()?
        };
        if ranges[0].0 != 0
            || end != u32::from(glyphs)
            || ranges.windows(2).any(|w| w[0].0 >= w[1].0)
        {
            return Err(Error::InvalidFile);
        }
        for i in 0..ranges.len() {
            let limit = ranges.get(i + 1).map_or(end, |r| r.0);
            if limit > end {
                return Err(Error::InvalidFile);
            }
            out.extend(std::iter::repeat_n(
                ranges[i].1,
                (limit - ranges[i].0) as usize,
            ));
        }
    } else {
        return Err(Error::InvalidFile);
    }
    if out.iter().any(|&v| v >= fonts) {
        return Err(Error::InvalidFile);
    }
    Ok(out)
}

impl Font<'_> {
    pub fn rewrite_sfnt(
        &self,
        bytes: &[u8],
        glyphs: &BTreeMap<u16, Glyph>,
        supports: &[Support],
    ) -> Result<(Vec<u8>, f64)> {
        if glyphs.is_empty() {
            return Ok((bytes.to_vec(), 0.));
        }
        super::master_tables::protect_cff_point_references(
            bytes,
            &glyphs.keys().copied().collect(),
        )?;
        let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
        let (cff, error) = self.rewrite(glyphs, supports)?;
        let mut tables: BTreeMap<_, _> = face
            .raw_face()
            .table_records
            .into_iter()
            .map(|r| {
                Ok((
                    r.tag.to_bytes(),
                    face.raw_face()
                        .table(r.tag)
                        .ok_or(Error::InvalidFile)?
                        .to_vec(),
                ))
            })
            .collect::<Result<_>>()?;
        tables.insert(*b"CFF2", cff);
        crate::imported_layout::points::materialize_edit_anchors(&mut tables)?;
        let zero = self.axes.iter().map(|a| (a.clone(), 0.)).collect();
        let weights = supports
            .iter()
            .map(|s| s.weight(&zero))
            .collect::<Result<Vec<_>>>()?;
        let old_weights = self.weights(&zero)?;
        let mut old: Vec<_> = (0..face.number_of_glyphs())
            .map(|id| {
                face.glyph_bounding_box(ttf_parser::GlyphId(id)).map(|b| {
                    [
                        i32::from(b.x_min),
                        i32::from(b.y_min),
                        i32::from(b.x_max),
                        i32::from(b.y_max),
                    ]
                })
            })
            .collect();
        let mut new = old.clone();
        let bounds = |points: Vec<crate::Point>| -> Result<Option<[i32; 4]>> {
            let mut bbox = None::<[f64; 4]>;
            for p in points {
                let b = bbox.get_or_insert([p[0], p[1], p[0], p[1]]);
                for axis in 0..2 {
                    b[axis] = b[axis].min(p[axis]);
                    b[axis + 2] = b[axis + 2].max(p[axis]);
                }
            }
            Ok(bbox.map(|b| {
                [
                    b[0].floor() as i32,
                    b[1].floor() as i32,
                    b[2].ceil() as i32,
                    b[3].ceil() as i32,
                ]
            }))
        };
        for (&id, g) in glyphs {
            old[usize::from(id)] = bounds(self.glyph(id)?.points(&old_weights)?)?;
            new[usize::from(id)] = bounds(g.points(&weights)?)?;
        }
        super::master_tables::update_outline_bounds(
            &mut tables,
            &old,
            &new,
            &glyphs.keys().copied().collect(),
        )?;
        tables.remove(b"DSIG");
        let bytes = crate::color_fonts::assemble(tables, true).map_err(|_| Error::InvalidFile)?;

        Ok((bytes, error))
    }
}
