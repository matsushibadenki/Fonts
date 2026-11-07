//! Resolve TrueType component geometry as linear region functions before decomposition.
use super::{
    master_cff::{codec::Reader, Value},
    master_glyf, master_gvar, master_metric_variations, master_quantize, master_tables,
    masters::Support,
};
use crate::{Error, Point, Result};
use std::collections::{BTreeMap, BTreeSet};
type VPoint = [Value; 2];
#[derive(Clone)]
struct Flat {
    rounding: f64,
    rings: Vec<Vec<bool>>,
    points: Vec<VPoint>,
}
struct Resolver<'a> {
    bytes: &'a [u8],
    records: Vec<&'a [u8]>,
    gvar: master_gvar::Table<'a>,
    supports: Vec<Support>,
    cache: BTreeMap<u16, Flat>,
    active: BTreeSet<u16>,
    budget: usize,
}
impl Resolver<'_> {
    fn region(&mut self, support: &Support) -> Result<u16> {
        let index = if let Some(i) = self.supports.iter().position(|s| s == support) {
            i
        } else {
            self.supports.push(support.clone());
            self.supports.len() - 1
        };
        u16::try_from(index).map_err(|_| Error::TooLarge)
    }
    fn deltas(&mut self, id: u16, base: &[Point], ends: &[usize]) -> Result<Vec<VPoint>> {
        let mut out: Vec<_> = base.iter().map(|p| p.map(Value::constant)).collect();
        for tuple in self.gvar.glyph(id, base.len())? {
            let region = self.region(&tuple.support)?;
            let values = tuple.expand(base, ends)?;
            for (p, delta) in out.iter_mut().zip(values) {
                for i in 0..2 {
                    p[i] = p[i].add(&Value::constant(delta[i]).region(region)?)?;
                }
            }
        }
        Ok(out)
    }
    fn glyph(&mut self, id: u16) -> Result<Flat> {
        if let Some(g) = self.cache.get(&id) {
            return Ok(g.clone());
        }
        if self.active.len() >= 16 || !self.active.insert(id) {
            return Err(Error::InvalidFile);
        }
        let mut out = if let Some(g) = master_glyf::read(self.bytes, 0, id)? {
            let mut base = g.points.clone();
            base.extend([[0.; 2]; 4]);
            let mut ends = vec![];
            let mut at = 0;
            for ring in g.topology.rings() {
                at += ring.len();
                ends.push(at - 1);
            }
            let mut points = self.deltas(id, &base, &ends)?;
            points.truncate(g.points.len());
            Flat {
                rounding: 0.,
                rings: g.topology.rings().to_vec(),
                points,
            }
        } else {
            let data = *self
                .records
                .get(usize::from(id))
                .ok_or(Error::MissingGlyph)?;
            let mut r = Reader {
                bytes: data,
                at: 10,
            };
            let mut parts = vec![];
            loop {
                if parts.len() >= 4096 {
                    return Err(Error::TooLarge);
                }
                let flags = r.word()?;
                let child = r.word()?;
                let xy = flags & 2 != 0;
                let args = if flags & 1 != 0 {
                    let a = r.word()?;
                    let b = r.word()?;
                    if xy {
                        [f64::from(a as i16), f64::from(b as i16)]
                    } else {
                        [f64::from(a), f64::from(b)]
                    }
                } else {
                    let a = r.byte()?;
                    let b = r.byte()?;
                    if xy {
                        [f64::from(a as i8), f64::from(b as i8)]
                    } else {
                        [f64::from(a), f64::from(b)]
                    }
                };
                let mut m = [1., 0., 0., 1.];
                if flags & 8 != 0 {
                    m[0] = f64::from(r.word()? as i16) / 16384.;
                    m[3] = m[0];
                } else if flags & 64 != 0 {
                    m[0] = f64::from(r.word()? as i16) / 16384.;
                    m[3] = f64::from(r.word()? as i16) / 16384.;
                } else if flags & 128 != 0 {
                    for v in &mut m {
                        *v = f64::from(r.word()? as i16) / 16384.;
                    }
                }
                if flags & 0x1800 == 0x1800 {
                    return Err(Error::InvalidFile);
                }
                parts.push((flags, child, args, m));
                if flags & 32 == 0 {
                    break;
                }
            }
            let base: Vec<_> = parts.iter().map(|p| p.2).chain([[0.; 2]; 4]).collect();
            let offsets = self.deltas(id, &base, &[])?;
            let mut flat = Flat {
                rounding: 0.,
                rings: vec![],
                points: vec![],
            };
            for (i, (flags, child, args, m)) in parts.into_iter().enumerate() {
                let child = self.glyph(child)?;
                let transform = |p: &VPoint| -> Result<VPoint> {
                    Ok([
                        p[0].scale(m[0])?.add(&p[1].scale(m[2])?)?,
                        p[0].scale(m[1])?.add(&p[1].scale(m[3])?)?,
                    ])
                };
                let mut points = child
                    .points
                    .iter()
                    .map(transform)
                    .collect::<Result<Vec<_>>>()?;
                let delta = if flags & 2 != 0 {
                    let mut p = offsets[i].clone();
                    if flags & 0x800 != 0 {
                        p[0] = p[0].scale((m[0].hypot(m[2]) * 65536.).round() / 65536.)?;
                        p[1] = p[1].scale((m[1].hypot(m[3]) * 65536.).round() / 65536.)?;
                    }
                    p
                } else {
                    let a = flat
                        .points
                        .get(args[0] as usize)
                        .ok_or(Error::InvalidFile)?;
                    let b = points.get(args[1] as usize).ok_or(Error::InvalidFile)?;
                    [a[0].plus(&b[0], -1.)?, a[1].plus(&b[1], -1.)?]
                };
                for p in &mut points {
                    p[0] = p[0].add(&delta[0])?;
                    p[1] = p[1].add(&delta[1])?;
                }
                let scale = (m[0].abs() + m[2].abs()).max(m[1].abs() + m[3].abs());
                let moving = offsets[i].iter().any(|v| {
                    v.0.iter()
                        .any(|(key, value)| !key.is_empty() && *value != 0.)
                });
                let placement = if flags & 2 == 0 {
                    flat.rounding + scale * child.rounding
                } else if moving {
                    0.5 * if flags & 0x800 != 0 {
                        m[0].hypot(m[2]).max(m[1].hypot(m[3]))
                    } else {
                        1.
                    }
                } else {
                    0.
                };
                flat.rounding = flat
                    .rounding
                    .max(scale * child.rounding + placement + 1. / 64.);
                flat.rings.extend(child.rings);
                flat.points.extend(points);
                if flat.points.len() > 65535 || flat.rings.len() > 4096 {
                    return Err(Error::TooLarge);
                }
            }
            flat
        };
        self.active.remove(&id);
        self.budget += out
            .points
            .iter()
            .flat_map(|p| p.iter())
            .map(|v| v.0.len())
            .sum::<usize>();
        if self.budget > 2_000_000 {
            return Err(Error::TooLarge);
        }
        out.points.shrink_to_fit();
        self.cache.insert(id, out.clone());
        Ok(out)
    }
}
pub fn is_composite(bytes: &[u8], id: u16) -> Result<bool> {
    Ok(master_glyf::read(bytes, 0, id)?.is_none())
}
pub struct Normalized {
    pub(crate) metric_origins: BTreeMap<u16, [f64; 2]>,
    pub(crate) metric_advances: BTreeMap<u16, [f64; 2]>,
    pub bytes: Vec<u8>,
    pub errors: BTreeMap<u16, Vec<(Support, f64)>>,
    pub native: BTreeMap<u16, f64>,
}
pub fn normalize(bytes: &[u8], selected: &BTreeSet<u16>) -> Result<Vec<u8>> {
    Ok(normalize_report(bytes, selected)?.bytes)
}
pub fn normalize_report(bytes: &[u8], selected: &BTreeSet<u16>) -> Result<Normalized> {
    if selected.is_empty() {
        return Ok(Normalized {
            metric_origins: BTreeMap::new(),
            metric_advances: BTreeMap::new(),
            bytes: bytes.to_vec(),
            errors: BTreeMap::new(),
            native: BTreeMap::new(),
        });
    }
    let inherited = master_metric_variations::freeze_inherited(bytes)?;
    let frozen = inherited.bytes;
    let face = ttf_parser::Face::parse(&frozen, 0).map_err(|_| Error::InvalidFile)?;
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    let raw = face.raw_face();
    let table = |tag| {
        raw.table(ttf_parser::Tag::from_bytes(tag))
            .ok_or(Error::InvalidFile)
    };
    let loca = table(b"loca")?;
    let glyf = table(b"glyf")?;
    let long = table(b"head")?.get(50..52) == Some(&[0, 1]);
    let mut r = Reader { bytes: loca, at: 0 };
    let mut offsets = vec![];
    for _ in 0..=face.number_of_glyphs() {
        offsets.push(if long {
            r.long()? as usize
        } else {
            usize::from(r.word()?) * 2
        });
    }
    let records = offsets
        .windows(2)
        .map(|w| glyf.get(w[0]..w[1]).ok_or(Error::InvalidFile))
        .collect::<Result<Vec<_>>>()?;
    let gvar = master_gvar::Table::parse(table(b"gvar")?, &axes, face.number_of_glyphs())?;
    let mut resolver = Resolver {
        bytes: &frozen,
        records: records.clone(),
        gvar,
        supports: vec![],
        cache: BTreeMap::new(),
        active: BTreeSet::new(),
        budget: 0,
    };
    let mut newrecords: Vec<_> = records.iter().map(|r| r.to_vec()).collect();
    let mut replacements = BTreeMap::new();
    let mut source_counts = BTreeMap::new();
    let mut bearings = [BTreeMap::new(), BTreeMap::new()];
    let mut plans = BTreeMap::new();
    let mut reports = BTreeMap::new();
    let mut native: BTreeMap<_, _> = inherited
        .origins
        .iter()
        .map(|(&id, v)| (id, v[0]))
        .collect();
    for &id in selected {
        let flat = resolver.glyph(id)?;
        let points = flat
            .points
            .iter()
            .map(|p| {
                Ok([
                    p[0].0.get(&vec![]).copied().unwrap_or(0.),
                    p[1].0.get(&vec![]).copied().unwrap_or(0.),
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        let mut g = master_glyf::Glyph::from_points(flat.rings, points.clone())?;
        g.overlap = true;
        let encoded = g.encode_base(&points)?;
        let mut tuples = vec![];
        let mut errors = vec![];
        if encoded.max_error > 0. {
            errors.push((Support::default(), encoded.max_error));
        }
        native.insert(
            id,
            flat.rounding + inherited.origins.get(&id).map_or(0., |v| v[0]),
        );
        let used: BTreeSet<_> = flat
            .points
            .iter()
            .flat_map(|p| {
                p.iter()
                    .flat_map(|v| v.0.keys().flat_map(|key| key.iter().copied()))
            })
            .collect();
        if used
            .len()
            .checked_mul(points.len() + 4)
            .is_none_or(|n| n > 4_000_000)
        {
            return Err(Error::TooLarge);
        }
        for region in used {
            let support = &resolver.supports[usize::from(region)];
            let mut deltas: Vec<_> = flat
                .points
                .iter()
                .map(|p| {
                    [
                        p[0].0.get(&vec![region]).copied().unwrap_or(0.),
                        p[1].0.get(&vec![region]).copied().unwrap_or(0.),
                    ]
                })
                .collect();
            deltas.extend([[0.; 2]; 4]);
            let quantized = master_quantize::quantize(support, &deltas)?;
            if quantized.max_error > 0. {
                errors.push((support.clone(), quantized.max_error));
            }
            tuples.extend(quantized.tuples);
        }
        // Preserve the already detached source phantom functions at their new physical indices.
        let count = records[usize::from(id)]
            .get(..2)
            .map(|b| i16::from_be_bytes([b[0], b[1]]))
            .ok_or(Error::InvalidFile)?;
        if count >= 0 {
            return Err(Error::InvalidGeometry);
        }
        let mut components = Reader {
            bytes: records[usize::from(id)],
            at: 10,
        };
        let mut n = 0;
        loop {
            let f = components.word()?;
            components.word()?;
            components.take(if f & 1 != 0 { 4 } else { 2 })?;
            components.take(if f & 8 != 0 {
                2
            } else if f & 64 != 0 {
                4
            } else if f & 128 != 0 {
                8
            } else {
                0
            })?;
            n += 1;
            if f & 32 == 0 {
                break;
            }
        }
        source_counts.insert(id, n + 4);
        for tuple in resolver.gvar.glyph(id, n + 4)? {
            let expanded = tuple.expand(&vec![[0.; 2]; n + 4], &[])?;
            let mut deltas = vec![[0.; 2]; points.len()];
            deltas.extend_from_slice(&expanded[n..]);
            tuples.extend(master_quantize::quantize(&tuple.support, &deltas)?.tuples);
        }
        let old = face.glyph_bounding_box(ttf_parser::GlyphId(id));
        if let Some(old) = old {
            if let Some(lsb) = face.glyph_hor_side_bearing(ttf_parser::GlyphId(id)) {
                bearings[0].insert(
                    id,
                    i16::try_from(
                        i32::from(lsb) + i32::from(encoded.bounds[0]) - i32::from(old.x_min),
                    )
                    .map_err(|_| Error::InvalidGeometry)?,
                );
            }
            if let Some(tsb) = face.glyph_ver_side_bearing(ttf_parser::GlyphId(id)) {
                bearings[1].insert(
                    id,
                    i16::try_from(
                        i32::from(tsb) + i32::from(old.y_max) - i32::from(encoded.bounds[3]),
                    )
                    .map_err(|_| Error::InvalidGeometry)?,
                );
            }
        }
        newrecords[usize::from(id)] = encoded.bytes.clone();
        replacements.insert(id, (points.len() + 4, tuples.clone()));
        reports.insert(id, errors.clone());
        plans.insert(
            id,
            super::master_rebase::Rebased {
                base: encoded,
                tuples,
                errors,
            },
        );
    }
    let mut tables: BTreeMap<_, _> = raw
        .table_records
        .into_iter()
        .map(|r| {
            Ok((
                r.tag.to_bytes(),
                raw.table(r.tag).ok_or(Error::InvalidFile)?.to_vec(),
            ))
        })
        .collect::<Result<_>>()?;
    let mut data = vec![];
    let mut locations = vec![];
    for record in newrecords {
        locations.extend((data.len() as u32).to_be_bytes());
        data.extend(record);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    locations.extend((data.len() as u32).to_be_bytes());
    tables.insert(*b"glyf", data);
    tables.insert(*b"loca", locations);
    tables.get_mut(b"head").ok_or(Error::InvalidFile)?[50..52].copy_from_slice(&1i16.to_be_bytes());
    let changes = replacements
        .iter()
        .map(|(&id, (n, t))| (id, (*n, t.as_slice())))
        .collect();
    tables.insert(
        *b"gvar",
        master_gvar::replace_reindexed(
            table(b"gvar")?,
            &axes,
            face.number_of_glyphs(),
            &changes,
            Some(&source_counts),
        )?,
    );
    for (vertical, values) in bearings.iter().enumerate() {
        master_tables::set_bearings(&mut tables, values, vertical == 1)?;
    }
    tables.remove(b"DSIG");
    let prepared = crate::color_fonts::assemble(tables, false).map_err(|_| Error::InvalidFile)?;
    Ok(Normalized {
        metric_origins: inherited.origins,
        metric_advances: inherited.advances,
        bytes: master_tables::apply(&prepared, &plans)?,
        errors: reports,
        native,
    })
}

/// Propagate positive error bounds through the actual output component graph.
pub(super) fn propagate_reports(
    bytes: &[u8],
    errors: &mut BTreeMap<u16, Vec<(Support, f64)>>,
    native: &mut BTreeMap<u16, f64>,
) -> Result<()> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let raw = face.raw_face();
    let Some(glyf) = raw.table(ttf_parser::Tag::from_bytes(b"glyf")) else {
        return Ok(());
    };
    let loca = raw
        .table(ttf_parser::Tag::from_bytes(b"loca"))
        .ok_or(Error::InvalidFile)?;
    let long = raw
        .table(ttf_parser::Tag::from_bytes(b"head"))
        .and_then(|h| h.get(50..52))
        == Some(&[0, 1]);
    let mut r = Reader { bytes: loca, at: 0 };
    let mut offsets = vec![];
    for _ in 0..=face.number_of_glyphs() {
        offsets.push(if long {
            r.long()? as usize
        } else {
            usize::from(r.word()?) * 2
        });
    }
    let mut graph = vec![];
    for range in offsets.windows(2) {
        let record = glyf.get(range[0]..range[1]).ok_or(Error::InvalidFile)?;
        let mut parts = vec![];
        if record.get(..2) == Some(&[255, 255]) {
            let mut r = Reader {
                bytes: record,
                at: 10,
            };
            loop {
                if parts.len() > 4096 {
                    return Err(Error::TooLarge);
                }
                let flags = r.word()?;
                let child = r.word()?;
                r.take(if flags & 1 != 0 { 4 } else { 2 })?;
                let mut m = [1f64, 0., 0., 1.];
                if flags & 8 != 0 {
                    m[0] = f64::from(r.word()? as i16) / 16384.;
                    m[3] = m[0];
                } else if flags & 64 != 0 {
                    m[0] = f64::from(r.word()? as i16) / 16384.;
                    m[3] = f64::from(r.word()? as i16) / 16384.;
                } else if flags & 128 != 0 {
                    for v in &mut m {
                        *v = f64::from(r.word()? as i16) / 16384.;
                    }
                }
                parts.push((
                    child,
                    (m[0].abs() + m[2].abs()).max(m[1].abs() + m[3].abs()),
                    flags & 2 != 0,
                ));
                if flags & 32 == 0 {
                    break;
                }
            }
        }
        graph.push(parts);
    }
    fn merge(
        out: &mut Vec<(Support, f64)>,
        source: &[(Support, f64)],
        factor: f64,
        maximum: bool,
    ) -> Result<()> {
        for (support, value) in source {
            let value = value * factor;
            if !value.is_finite() || value > 1e9 {
                return Err(Error::TooLarge);
            }
            if let Some((_, old)) = out.iter_mut().find(|(s, _)| s == support) {
                *old = if maximum {
                    old.max(value)
                } else {
                    *old + value
                };
            } else if value != 0. {
                out.push((support.clone(), value));
            }
        }
        Ok(())
    }
    type Bounds = (Vec<(Support, f64)>, f64);
    fn visit(
        id: u16,
        graph: &[Vec<(u16, f64, bool)>],
        own: &BTreeMap<u16, Vec<(Support, f64)>>,
        rounding: &BTreeMap<u16, f64>,
        cache: &mut BTreeMap<u16, Bounds>,
        active: &mut BTreeSet<u16>,
    ) -> Result<Bounds> {
        if let Some(result) = cache.get(&id) {
            return Ok(result.clone());
        }
        if active.len() >= 16 || !active.insert(id) {
            return Err(Error::InvalidFile);
        }
        let mut result = (vec![], 0f64);
        for &(child, scale, xy) in graph.get(usize::from(id)).ok_or(Error::MissingGlyph)? {
            let b = visit(child, graph, own, rounding, cache, active)?;
            if xy {
                merge(&mut result.0, &b.0, scale, true)?;
                result.1 = result.1.max(b.1 * scale);
            } else {
                merge(&mut result.0, &b.0, 2. * scale, false)?;
                result.1 += b.1 * scale * 2.;
            }
        }
        if let Some(e) = own.get(&id) {
            merge(&mut result.0, e, 1., false)?;
        }
        result.1 += rounding.get(&id).copied().unwrap_or(0.);
        if !result.1.is_finite() || result.1 > 1e9 {
            return Err(Error::TooLarge);
        }
        active.remove(&id);
        cache.insert(id, result.clone());
        Ok(result)
    }
    let mut cache = BTreeMap::new();
    let mut active = BTreeSet::new();
    for id in 0..face.number_of_glyphs() {
        visit(id, &graph, errors, native, &mut cache, &mut active)?;
    }
    for (id, (bound, rounding)) in cache {
        if !bound.is_empty() {
            errors.insert(id, bound);
        }
        if rounding > 0. {
            native.insert(id, rounding);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
