use super::*;
use crate::source_font::{
    master_export::Exported,
    master_metric_variations,
    master_points::Topology,
    masters::{Masters, Support},
};
use std::{collections::BTreeSet, sync::Arc};
pub fn export(
    bytes: &[u8],
    masters: &Masters,
    topologies: &BTreeMap<u16, Topology>,
) -> Result<Exported> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    let cff = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CFF2"))
        .ok_or(Error::InvalidFile)?;
    let font = Font::parse(cff, &axes, face.number_of_glyphs(), face.units_per_em())?;
    let mut supports = font.supports.clone();
    let mut master_regions = vec![];
    for m in &masters.entries {
        if m.support.axes.is_empty() {
            master_regions.push(None);
        } else {
            let id = if let Some(i) = supports.iter().position(|s| *s == m.support) {
                i
            } else {
                supports.push(m.support.clone());
                supports.len() - 1
            };
            master_regions.push(Some(u16::try_from(id).map_err(|_| Error::TooLarge)?));
        }
    }
    if supports.len() >= 0x8000 {
        return Err(Error::TooLarge);
    }
    let ids: BTreeSet<_> = masters
        .entries
        .iter()
        .flat_map(|m| m.glyphs.keys().copied())
        .collect();
    let mut sampler = crate::source_font::master_sample::Sampler::new(Arc::new(bytes.to_vec()), 0)?;
    let mut glyphs = BTreeMap::new();
    let mut complexity = 0usize;
    let mut advances = BTreeMap::new();
    for id in ids {
        let gid = u16::try_from(id).map_err(|_| Error::MissingGlyph)?;
        let topology = topologies.get(&gid).ok_or(Error::InvalidGeometry)?;
        if !topology.is_cubic() {
            return Err(Error::InvalidGeometry);
        }
        let mut glyph = font.glyph(gid)?;
        let width = glyph
            .values()
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::TooLarge)?;
        if width
            .checked_mul(masters.entries.len())
            .is_none_or(|n| n > 2_000_000)
        {
            return Err(Error::TooLarge);
        }
        let mut residuals = vec![];
        for m in &masters.entries {
            let mut row = vec![0.; glyph.values().len() * 2 + 1];
            if let Some(target) = m.glyphs.get(&id) {
                let points = topology.physical_points(&target.contours)?;
                let source = glyph.points(&font.weights(&m.location)?)?;
                if points.len() != source.len() {
                    return Err(Error::InvalidGeometry);
                }
                row[0] = target.advance - sampler.sample_normalized(id, &m.location)?.shape.advance;
                for ((out, target), source) in row[1..]
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(points)
                    .zip(source)
                {
                    *out = [target[0] - source[0], target[1] - source[1]];
                }
            }
            residuals.push(row);
        }
        let coefficients = masters.coefficients(&residuals)?;
        if coefficients.iter().flatten().all(|v| *v == 0.) {
            continue;
        }
        let mut metric = vec![];
        for ((m, values), &region) in masters
            .entries
            .iter()
            .zip(coefficients)
            .zip(&master_regions)
        {
            if values[0] != 0. {
                metric.push((m.support.clone(), values[0]));
            }
            let mut at = 1;
            let mut apply = |point: &mut Point| -> Result<()> {
                for value in point {
                    let delta = Value::constant(values[at]);
                    let delta = if let Some(region) = region {
                        delta.region(region)?
                    } else {
                        delta
                    };
                    *value = value.add(&delta)?;
                    at += 1;
                }
                Ok(())
            };
            for path in &mut glyph.paths {
                apply(&mut path.start)?;
                for curve in &mut path.segments {
                    match curve {
                        Curve::Line(p) => apply(p)?,
                        Curve::Cubic(a, b, c) => {
                            apply(a)?;
                            apply(b)?;
                            apply(c)?;
                        }
                    }
                }
            }
        }
        if !metric.is_empty() {
            advances.insert(gid, metric);
        }
        complexity = complexity
            .checked_add(
                glyph
                    .values()
                    .iter()
                    .flat_map(|p| p.iter())
                    .map(|v| v.0.len())
                    .sum::<usize>(),
            )
            .ok_or(Error::TooLarge)?;
        if complexity > 2_000_000 {
            return Err(Error::TooLarge);
        }
        glyphs.insert(gid, glyph);
    }
    let (bytes, error) = font.rewrite_sfnt(bytes, &glyphs, &supports)?;
    let metrics = master_metric_variations::apply_font(&bytes, &advances, &BTreeMap::new())?;
    Ok(Exported {
        native_outline_rounding: BTreeMap::new(),
        metrics,
        outline_errors: glyphs
            .keys()
            .map(|&id| (id, vec![(Support::default(), error)]))
            .collect(),
    })
}
