//! Sparse, bounded source gvar decoding. Coordinates are normalized after avar.
use super::{
    master_tuple,
    masters::{Location, Support},
};
use crate::{Error, Point, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct Tuple {
    pub support: Support,
    pub points: Option<Vec<usize>>,
    pub deltas: Vec<[i16; 2]>,
}
impl Tuple {
    pub fn expand(&self, points: &[Point], ends: &[usize]) -> Result<Vec<Point>> {
        if points.len() > 1_000_000 {
            return Err(Error::TooLarge);
        }
        let mut deltas = vec![None; points.len()];
        if let Some(indices) = &self.points {
            if indices.len() != self.deltas.len() || indices.windows(2).any(|v| v[0] >= v[1]) {
                return Err(Error::InvalidFile);
            }
            for (&index, &delta) in indices.iter().zip(&self.deltas) {
                *deltas.get_mut(index).ok_or(Error::InvalidFile)? = Some(delta.map(f64::from));
            }
        } else {
            if self.deltas.len() != points.len() {
                return Err(Error::InvalidFile);
            }
            for (out, &delta) in deltas.iter_mut().zip(&self.deltas) {
                *out = Some(delta.map(f64::from));
            }
        }
        master_tuple::infer_deltas(points, ends, &deltas, 4)
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::InvalidFile)?;
        let out = self.bytes.get(self.at..end).ok_or(Error::InvalidFile)?;
        self.at = end;
        Ok(out)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| Error::InvalidFile)?,
        ))
    }
    fn u32(&mut self) -> Result<usize> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| Error::InvalidFile)?) as usize)
    }
    fn coordinates(&mut self, count: usize) -> Result<Vec<f32>> {
        (0..count)
            .map(|_| {
                let v = f32::from(self.u16()? as i16) / 16384.;
                if !(-1. ..=1.).contains(&v) {
                    return Err(Error::InvalidFile);
                }
                Ok(v)
            })
            .collect()
    }
}
/// Decode a selected glyph without expanding every tuple to every outline point.
/// `point_count` includes four phantom points (or component count plus four).
pub fn read(
    table: &[u8],
    axes: &[String],
    glyph_count: u16,
    glyph: u16,
    point_count: usize,
) -> Result<Vec<Tuple>> {
    Table::parse(table, axes, glyph_count)?.glyph(glyph, point_count)
}
/// Shared metadata is parsed once when inspecting or rewriting many glyphs.
pub struct Table<'a> {
    data: &'a [u8],
    axes: Vec<String>,
    peaks: Vec<Vec<f32>>,
    offsets: Vec<usize>,
}
impl<'a> Table<'a> {
    pub fn parse(table: &'a [u8], axes: &[String], glyph_count: u16) -> Result<Self> {
        if table.len() > 256 * 1024 * 1024 {
            return Err(Error::TooLarge);
        }
        if axes.is_empty()
            || axes.len() > 63
            || axes.iter().any(|s| s.len() != 4 || !s.is_ascii())
            || axes.iter().collect::<std::collections::BTreeSet<_>>().len() != axes.len()
        {
            return Err(Error::InvalidFile);
        }
        let mut r = Reader {
            bytes: table,
            at: 0,
        };
        if r.u16()? != 1 || r.u16()? != 0 || usize::from(r.u16()?) != axes.len() {
            return Err(Error::InvalidFile);
        }
        let shared_count = usize::from(r.u16()?);
        if shared_count > 4096 {
            return Err(Error::InvalidFile);
        }
        let shared_at = r.u32()?;
        if r.u16()? != glyph_count {
            return Err(Error::InvalidFile);
        }
        let flags = r.u16()?;
        if flags & !1 != 0 {
            return Err(Error::InvalidFile);
        }
        let data_at = r.u32()?;
        let mut previous = 0;
        let mut offsets = Vec::with_capacity(usize::from(glyph_count) + 1);
        for _ in 0..=glyph_count {
            let offset = if flags & 1 != 0 {
                r.u32()?
            } else {
                usize::from(r.u16()?) * 2
            };
            if offset < previous || data_at.checked_add(offset).is_none_or(|n| n > table.len()) {
                return Err(Error::InvalidFile);
            }
            offsets.push(offset);
            previous = offset;
        }
        if data_at < r.at {
            return Err(Error::InvalidFile);
        }
        let shared_size = shared_count * axes.len() * 2;
        let shared_end = shared_at
            .checked_add(shared_size)
            .ok_or(Error::InvalidFile)?;
        if shared_size != 0 && (shared_at < r.at || shared_end > data_at) {
            return Err(Error::InvalidFile);
        }
        let mut shared = Reader {
            bytes: table.get(shared_at..shared_end).ok_or(Error::InvalidFile)?,
            at: 0,
        };
        let peaks: Vec<Vec<f32>> = (0..shared_count)
            .map(|_| shared.coordinates(axes.len()))
            .collect::<Result<_>>()?;
        Ok(Self {
            data: &table[data_at..],
            axes: axes.to_vec(),
            peaks,
            offsets,
        })
    }
    pub fn glyph(&self, glyph: u16, point_count: usize) -> Result<Vec<Tuple>> {
        if point_count > 1_000_000 {
            return Err(Error::TooLarge);
        }
        if point_count < 4 {
            return Err(Error::InvalidFile);
        }
        let index = usize::from(glyph);
        let start = *self.offsets.get(index).ok_or(Error::InvalidFile)?;
        let end = *self.offsets.get(index + 1).ok_or(Error::InvalidFile)?;
        decode_glyph(&self.data[start..end], &self.axes, &self.peaks, point_count)
    }
}
fn decode_glyph(
    data: &[u8],
    axes: &[String],
    peaks: &[Vec<f32>],
    point_count: usize,
) -> Result<Vec<Tuple>> {
    if data.is_empty() {
        return Ok(vec![]);
    }
    let mut r = Reader { bytes: data, at: 0 };
    let count = r.u16()?;
    if count & 0x7000 != 0 || count & 0xfff == 0 {
        return Err(Error::InvalidFile);
    }
    let data_offset = usize::from(r.u16()?);
    let mut headers = Vec::new();
    for _ in 0..count & 0xfff {
        let size = usize::from(r.u16()?);
        let flags = r.u16()?;
        if flags & 0x1000 != 0 {
            return Err(Error::InvalidFile);
        }
        let peak = if flags & 0x8000 != 0 {
            r.coordinates(axes.len())?
        } else {
            peaks
                .get(usize::from(flags & 0xfff))
                .ok_or(Error::InvalidFile)?
                .clone()
        };
        let (start, end) = if flags & 0x4000 != 0 {
            (r.coordinates(axes.len())?, r.coordinates(axes.len())?)
        } else {
            (
                peak.iter().map(|v| v.min(0.)).collect(),
                peak.iter().map(|v| v.max(0.)).collect(),
            )
        };
        let mut support = Support::default();
        for (i, tag) in axes.iter().enumerate() {
            let (s, p, e) = (start[i], peak[i], end[i]);
            // OpenType ignores an axis with zero peak or an invalid region.
            if p != 0. && s <= p && p <= e && !(s < 0. && e > 0.) {
                support.axes.insert(tag.clone(), [s, p, e]);
            }
        }
        headers.push((size, flags, support));
    }
    if data_offset < r.at || data_offset > data.len() {
        return Err(Error::InvalidFile);
    }
    let mut at = data_offset;
    let shared_points = if count & 0x8000 != 0 {
        let (points, used) = master_tuple::read_points(&data[at..], point_count)?;
        at += used;
        points
    } else {
        None
    };
    let mut total = 0usize;
    let mut out = Vec::with_capacity(headers.len());
    for (size, flags, support) in headers {
        let end = at.checked_add(size).ok_or(Error::InvalidFile)?;
        let bytes = data.get(at..end).ok_or(Error::InvalidFile)?;
        at = end;
        let (points, used) = if flags & 0x2000 != 0 {
            master_tuple::read_points(bytes, point_count)?
        } else {
            (shared_points.clone(), 0)
        };
        let n = points.as_ref().map_or(point_count, Vec::len);
        total = total.checked_add(n).ok_or(Error::TooLarge)?;
        if total > 4_000_000 {
            return Err(Error::TooLarge);
        }
        let (x, nx) = master_tuple::read_deltas(&bytes[used..], n)?;
        let (y, ny) = master_tuple::read_deltas(&bytes[used + nx..], n)?;
        if used + nx + ny != bytes.len() {
            return Err(Error::InvalidFile);
        }
        out.push(Tuple {
            support,
            points,
            deltas: x.into_iter().zip(y).map(|(x, y)| [x, y]).collect(),
        });
    }
    Ok(out)
}

/// Add original tuple variation to raw points; phantom points are passed explicitly.
pub fn evaluate(
    tuples: &[Tuple],
    points: &[Point],
    ends: &[usize],
    location: &Location,
) -> Result<Vec<Point>> {
    if points.len() > 1_000_000 || tuples.len() > 4095 {
        return Err(Error::TooLarge);
    }
    Support::default().weight(location)?;
    let mut out = points.to_vec();
    for tuple in tuples {
        let weight = tuple.support.weight(location)?;
        if weight == 0. {
            continue;
        }
        for (point, delta) in out.iter_mut().zip(tuple.expand(points, ends)?) {
            for axis in 0..2 {
                point[axis] += weight * delta[axis];
            }
        }
    }
    if out
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err(Error::InvalidGeometry);
    }
    Ok(out)
}

/// Encode a glyph using embedded peaks and private point runs. Source IUP is retained.
pub fn encode_glyph(tuples: &[Tuple], axes: &[String], point_count: usize) -> Result<Vec<u8>> {
    if tuples.is_empty() {
        return Ok(vec![]);
    }
    if tuples.len() > 4095
        || axes.is_empty()
        || axes.len() > 63
        || !(4..=1_000_000).contains(&point_count)
    {
        return Err(Error::TooLarge);
    }
    if axes.iter().any(|s| s.len() != 4 || !s.is_ascii())
        || axes.iter().collect::<std::collections::BTreeSet<_>>().len() != axes.len()
    {
        return Err(Error::InvalidGeometry);
    }
    let mut header = Vec::new();
    let mut data = Vec::new();
    let mut total = 0usize;
    for tuple in tuples {
        if tuple.support.axes.keys().any(|tag| !axes.contains(tag)) {
            return Err(Error::InvalidGeometry);
        }
        let location = tuple
            .support
            .axes
            .iter()
            .map(|(tag, v)| (tag.clone(), v[1]))
            .collect();
        tuple.support.weight(&location)?;
        let expected = tuple.points.as_ref().map_or(point_count, Vec::len);
        total = total.checked_add(expected).ok_or(Error::TooLarge)?;
        if total > 4_000_000 || tuple.deltas.len() != expected {
            return Err(Error::TooLarge);
        }
        let mut payload = master_tuple::write_points(tuple.points.as_deref(), point_count)?;
        payload.extend(master_tuple::write_deltas(
            &tuple.deltas.iter().map(|p| p[0]).collect::<Vec<_>>(),
        )?);
        payload.extend(master_tuple::write_deltas(
            &tuple.deltas.iter().map(|p| p[1]).collect::<Vec<_>>(),
        )?);
        header.extend(
            u16::try_from(payload.len())
                .map_err(|_| Error::TooLarge)?
                .to_be_bytes(),
        );
        header.extend(0xe000u16.to_be_bytes());
        for component in [1, 0, 2] {
            for tag in axes {
                let coordinate = tuple.support.axes.get(tag).map_or(0., |v| v[component]);
                let fixed = coordinate * 16384.;
                // Do not silently move a user-defined interpolation region during export.
                if fixed != fixed.round() {
                    return Err(Error::InvalidGeometry);
                }
                header.extend((fixed as i16).to_be_bytes());
            }
        }
        data.extend(payload);
    }
    let mut out = (tuples.len() as u16).to_be_bytes().to_vec();
    out.extend(
        u16::try_from(header.len() + 4)
            .map_err(|_| Error::TooLarge)?
            .to_be_bytes(),
    );
    out.extend(header);
    out.extend(data);
    if !out.len().is_multiple_of(2) {
        out.push(0);
    }
    Ok(out)
}

/// Replace one glyph's tuples; all other glyph data and shared peaks remain byte-for-byte intact.
pub fn replace(
    table: &[u8],
    axes: &[String],
    glyph_count: u16,
    glyph: u16,
    point_count: usize,
    tuples: &[Tuple],
) -> Result<Vec<u8>> {
    replace_many(
        table,
        axes,
        glyph_count,
        &[(glyph, (point_count, tuples))].into(),
    )
}
/// Rebuild the table once for a batch of edited glyphs.
pub fn replace_many(
    table: &[u8],
    axes: &[String],
    glyph_count: u16,
    replacements: &std::collections::BTreeMap<u16, (usize, &[Tuple])>,
) -> Result<Vec<u8>> {
    if replacements.is_empty() {
        return Ok(table.to_vec());
    }
    let parsed = Table::parse(table, axes, glyph_count)?;
    let mut encoded = std::collections::BTreeMap::new();
    let mut encoded_size = 0usize;
    for (&glyph, &(point_count, tuples)) in replacements {
        parsed.glyph(glyph, point_count)?;
        let bytes = encode_glyph(tuples, axes, point_count)?;
        encoded_size = encoded_size
            .checked_add(bytes.len())
            .ok_or(Error::TooLarge)?;
        if encoded_size > 256 * 1024 * 1024 {
            return Err(Error::TooLarge);
        }
        encoded.insert(glyph, bytes);
    }
    let mut r = Reader {
        bytes: table,
        at: 6,
    };
    let shared_count = r.u16()?;
    let shared_at = r.u32()?;
    r.u16()?;
    let flags = r.u16()?;
    let data_at = r.u32()?;
    let mut offsets = Vec::with_capacity(usize::from(glyph_count) + 1);
    for _ in 0..=glyph_count {
        offsets.push(if flags & 1 != 0 {
            r.u32()?
        } else {
            usize::from(r.u16()?) * 2
        });
    }
    let peaks = &table[shared_at..shared_at + usize::from(shared_count) * axes.len() * 2];
    let new_shared_at = 20 + offsets.len() * 4;
    let new_data_at = new_shared_at + peaks.len();
    let mut payload: Vec<u8> = Vec::new();
    let mut new_offsets = Vec::with_capacity(offsets.len());
    for id in 0..usize::from(glyph_count) {
        new_offsets.push(payload.len() as u32);
        payload.extend(if let Some(replacement) = encoded.get(&(id as u16)) {
            replacement.as_slice()
        } else {
            &table[data_at + offsets[id]..data_at + offsets[id + 1]]
        });
        if new_data_at + payload.len() > 256 * 1024 * 1024 {
            return Err(Error::TooLarge);
        }
    }
    new_offsets.push(payload.len() as u32);
    let mut out = Vec::new();
    for v in [1, 0, axes.len() as u16, shared_count] {
        out.extend(v.to_be_bytes());
    }
    out.extend((new_shared_at as u32).to_be_bytes());
    out.extend(glyph_count.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend((new_data_at as u32).to_be_bytes());
    for offset in new_offsets {
        out.extend(offset.to_be_bytes());
    }
    out.extend(peaks);
    out.extend(payload);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_font::master_sample::Sampler;
    use std::{collections::BTreeMap, sync::Arc};

    #[test]
    fn rebuilt_font_keeps_source_variation_and_applies_additional_outline_correction() {
        let bytes = include_bytes!("../../tests/data/variations/gvar.ttf");
        let raw = ttf_parser::RawFace::parse(bytes, 0).unwrap();
        let table = raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap();
        let axes = ["wght".into(), "wdth".into()];
        let mut tuples = read(table, &axes, 4, 1, 8).unwrap();
        let mut original = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
        for corrected in [false, true] {
            if corrected {
                let source = crate::source_font::master_glyf::read(bytes, 0, 1)
                    .unwrap()
                    .unwrap();
                let location = [("wght".into(), 1.), ("wdth".into(), 0.)].into();
                let mut edited = source.contours(&tuples, &location).unwrap();
                for anchor in 0..4 {
                    let node = crate::geometry::EditPoint::Anchor(anchor);
                    let position = crate::geometry::point_position(&edited[0], node).unwrap();
                    edited = source
                        .topology
                        .move_node(&edited, 0, node, [position[0], position[1] + 50.])
                        .unwrap();
                }
                let delta = source.outline_delta(&tuples, &location, &edited).unwrap();
                assert_eq!(delta, [[0., 50.]; 4]);
                tuples.push(Tuple {
                    support: Support {
                        axes: [("wght".into(), [0., 1., 1.])].into(),
                    },
                    points: Some(vec![0, 1, 2, 3]),
                    deltas: delta.into_iter().map(|p| p.map(|v| v as i16)).collect(),
                });
            }
            let mut tables = BTreeMap::new();
            for record in raw.table_records {
                tables.insert(
                    record.tag.to_bytes(),
                    raw.table(record.tag).unwrap().to_vec(),
                );
            }
            tables.insert(*b"gvar", replace(table, &axes, 4, 1, 8, &tuples).unwrap());
            let output = crate::color_fonts::assemble(tables, false).unwrap();
            let mut exported = Sampler::new(Arc::new(output), 0).unwrap();
            for wght in [-1., 0., 0.5, 1.] {
                for wdth in [-1., 0., 1.] {
                    let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
                    for glyph in 0..4 {
                        let base = original.sample_normalized(glyph, &location).unwrap();
                        let result = exported.sample_normalized(glyph, &location).unwrap();
                        assert_eq!(base.shape.advance, result.shape.advance);
                        if corrected && glyph == 1 && wght > 0. {
                            let a = base.shape.contours[0].start;
                            let b = result.shape.contours[0].start;
                            assert!((a[0] - b[0]).abs() < 0.02);
                            assert!((b[1] - a[1] - 50. * f64::from(wght)).abs() < 0.02);
                        } else {
                            assert_eq!(base.shape, result.shape);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn multiple_master_outline_edits_survive_native_font_reopening() {
        use crate::source_font::{
            master_correction, master_glyf, master_quantize,
            masters::{Master, Masters, Shape},
        };
        let bytes = include_bytes!("../../tests/data/variations/gvar.ttf");
        let raw = ttf_parser::RawFace::parse(bytes, 0).unwrap();
        let table = raw.table(ttf_parser::Tag::from_bytes(b"gvar")).unwrap();
        let axes = ["wght".into(), "wdth".into()];
        let mut tuples = read(table, &axes, 4, 1, 8).unwrap();
        let glyph = master_glyf::read(bytes, 0, 1).unwrap().unwrap();
        let mut native = Sampler::new(Arc::new(bytes.to_vec()), 0).unwrap();
        let mut entries = vec![];
        let mut samples = vec![];
        for (index, (wght, wdth, delta)) in [(0., 0., 0.), (0.5, 0.5, 37.), (1., 1., 48.)]
            .into_iter()
            .enumerate()
        {
            let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            let sample = native.sample_normalized(1, &location).unwrap();
            let source = Shape {
                advance: sample.shape.advance,
                contours: glyph.contours(&tuples, &location).unwrap(),
            };
            let mut points = glyph.sample_points(&tuples, &location).unwrap();
            for p in &mut points {
                p[1] += delta;
            }
            let target = Shape {
                advance: source.advance,
                contours: glyph.topology.contours(&points).unwrap(),
            };
            let support = match index {
                0 => Support::default(),
                1 => Support {
                    axes: [("wght".into(), [0., 0.5, 1.])].into(),
                },
                _ => Support {
                    axes: [("wght".into(), [0., 1., 1.]), ("wdth".into(), [0., 1., 1.])].into(),
                },
            };
            entries.push(Master {
                id: crate::identity::Uuid::new_v4(),
                name: format!("Master {index}"),
                design_location: sample.design_location,
                location,
                support,
                glyphs: [(1, target)].into(),
            });
            samples.push(source);
        }
        let masters = Masters { entries };
        let advances: Vec<_> = samples.iter().map(|s| s.advance).collect();
        let corrections =
            master_correction::solve(&glyph, 1, &masters, &tuples, &advances).unwrap();
        for correction in corrections {
            assert_eq!(correction.advance, 0.);
            let mut deltas = correction.points;
            deltas.extend([[0.; 2]; 4]);
            let encoded = master_quantize::quantize(&correction.support, &deltas).unwrap();
            assert_eq!(encoded.max_error, 0.);
            tuples.extend(encoded.tuples);
        }
        let mut tables = BTreeMap::new();
        for record in raw.table_records {
            tables.insert(
                record.tag.to_bytes(),
                raw.table(record.tag).unwrap().to_vec(),
            );
        }
        tables.insert(*b"gvar", replace(table, &axes, 4, 1, 8, &tuples).unwrap());
        let output = crate::color_fonts::assemble(tables, false).unwrap();
        let mut exported = Sampler::new(Arc::new(output), 0).unwrap();
        for (wght, wdth) in [
            (0., 0.),
            (0.5, 0.5),
            (1., 1.),
            (0.25, 0.75),
            (0.75, 0.5),
            (-1., -1.),
        ] {
            let location = [("wght".into(), wght), ("wdth".into(), wdth)].into();
            let original = native.sample_normalized(1, &location).unwrap();
            let source = Shape {
                advance: original.shape.advance,
                contours: glyph
                    .contours(&read(table, &axes, 4, 1, 8).unwrap(), &location)
                    .unwrap(),
            };
            let preview = masters
                .interpolate(1, &location, &source, &samples)
                .unwrap();
            let actual = exported.sample_normalized(1, &location).unwrap();
            assert!((actual.shape.advance - preview.advance).abs() < 0.02);
            assert_eq!(actual.shape.contours[0].start, preview.contours[0].start);
            assert_eq!(
                actual.shape.contours[0].segments,
                preview.contours[0].segments
            );
            assert_eq!(
                native.sample_normalized(3, &location).unwrap().shape,
                exported.sample_normalized(3, &location).unwrap().shape
            );
        }
    }
}
