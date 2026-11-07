//! Plan all simple TrueType master corrections before mutating any font table.
use super::{
    master_correction, master_glyf, master_gvar, master_metric_variations, master_quantize,
    master_rebase,
    master_sample::Sampler,
    master_tables,
    masters::{Location, Masters, Support},
};
use crate::{Error, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub struct Exported {
    pub native_outline_rounding: BTreeMap<u16, f64>,
    pub metrics: master_metric_variations::Applied,
    pub outline_errors: BTreeMap<u16, Vec<(Support, f64)>>,
}
impl Exported {
    pub fn outline_error(&self, glyph: u16, location: &Location) -> Result<f64> {
        Support::default().weight(location)?;
        self.outline_errors
            .get(&glyph)
            .into_iter()
            .flatten()
            .try_fold(0., |sum, (support, error)| {
                Ok(sum + support.weight(location)?.abs() * error)
            })
    }
}
/// Targets use physical glyf coordinates and stable topology identities, not rasterizer origins.
/// Unsupported glyphs fail atomically; no registered override is silently dropped.
pub fn export(
    bytes: &[u8],
    masters: &Masters,
    topologies: &BTreeMap<u16, super::master_points::Topology>,
) -> Result<Exported> {
    masters.validate()?;
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    if axes.is_empty() {
        return Err(Error::InvalidDocument);
    }
    let ids: BTreeSet<_> = masters
        .entries
        .iter()
        .flat_map(|m| m.glyphs.keys().copied())
        .collect();
    if ids.is_empty() {
        return Ok(Exported {
            native_outline_rounding: BTreeMap::new(),
            metrics: master_metric_variations::apply_font(
                bytes,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )?,
            outline_errors: BTreeMap::new(),
        });
    }
    if face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"CFF2"))
        .is_some()
    {
        return super::master_cff::export(bytes, masters, topologies);
    }
    let gvar = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"gvar"))
        .ok_or(Error::InvalidDocument)?;
    let table = master_gvar::Table::parse(gvar, &axes, face.number_of_glyphs())?;
    let mut sampler = Sampler::new(Arc::new(bytes.to_vec()), 0)?;
    let mut plans = BTreeMap::new();
    let mut advances = BTreeMap::new();
    let mut outline_errors = BTreeMap::new();
    for id in ids {
        let gid = u16::try_from(id).map_err(|_| Error::MissingGlyph)?;
        let mut glyph = master_glyf::read(bytes, 0, gid)?.ok_or(Error::InvalidGeometry)?;
        let topology = topologies.get(&gid).ok_or(Error::InvalidGeometry)?;
        if topology.rings() != glyph.topology.rings() {
            return Err(Error::InvalidGeometry);
        }
        topology.physical_points(&topology.contours(&glyph.points)?)?;
        glyph.topology = topology.clone();
        let tuples = table.glyph(gid, glyph.points.len() + 4)?;
        let source_advances = masters
            .entries
            .iter()
            .map(|m| {
                sampler
                    .sample_normalized(id, &m.location)
                    .map(|s| s.shape.advance)
            })
            .collect::<Result<Vec<_>>>()?;
        let corrections = master_correction::solve(&glyph, id, masters, &tuples, &source_advances)?;
        if corrections
            .iter()
            .all(|c| c.advance == 0. && c.points.iter().all(|p| *p == [0.; 2]))
        {
            continue;
        }
        let mut base = glyph.points.clone();
        for c in &corrections {
            if c.support.axes.values().all(|a| a[1] == 0.) {
                for (point, delta) in base.iter_mut().zip(&c.points) {
                    point[0] += delta[0];
                    point[1] += delta[1];
                }
            }
        }
        let mut plan = master_rebase::rebase(&glyph, &tuples, &base)?;
        let mut errors = plan.errors.clone();
        if plan.base.max_error != 0. {
            errors.push((Support::default(), plan.base.max_error));
        }
        let mut metric = Vec::new();
        for c in corrections {
            if c.advance != 0. {
                metric.push((c.support.clone(), c.advance));
            }
            if c.support.axes.values().all(|a| a[1] == 0.) {
                continue;
            }
            let mut deltas = c.points;
            deltas.extend([[0.; 2]; 4]);
            let q = master_quantize::quantize(&c.support, &deltas)?;
            if q.max_error != 0. {
                errors.push((c.support.clone(), q.max_error));
            }
            plan.tuples.extend(q.tuples);
        }
        if !metric.is_empty() {
            advances.insert(gid, metric);
        }
        outline_errors.insert(gid, errors);
        plans.insert(gid, plan);
    }
    let outlines = master_tables::apply(bytes, &plans)?;
    let metrics = master_metric_variations::apply_font(&outlines, &advances, &BTreeMap::new())?;
    Ok(Exported {
        native_outline_rounding: BTreeMap::new(),
        metrics,
        outline_errors,
    })
}
