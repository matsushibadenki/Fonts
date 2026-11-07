//! Apply one topology operation to the baseline variation function and every registered master.
use super::{
    master_cff::{self, Curve, Point as VPoint},
    master_glyf, master_gvar,
    master_points::Topology,
    master_quantize,
    master_rebase::Rebased,
    master_tables,
};
use crate::{Contour, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Split { contour: usize, segment: usize },
    Delete { contour: usize, segment: usize },
}
impl Operation {
    pub fn apply(&self, contours: &mut [Contour]) -> Result<()> {
        match *self {
            Self::Split { contour, segment } => crate::geometry::split_segment(
                contours.get_mut(contour).ok_or(Error::InvalidGeometry)?,
                segment,
            ),
            Self::Delete { contour, segment } => crate::geometry::delete_segment_end(
                contours.get_mut(contour).ok_or(Error::InvalidGeometry)?,
                segment,
            ),
        }
    }
    pub fn symbolic(&self, glyph: &mut master_cff::Glyph) -> Result<()> {
        let (ci, index) = match *self {
            Self::Split { contour, segment } | Self::Delete { contour, segment } => {
                (contour, segment)
            }
        };
        for branch in &mut glyph.branches {
            if branch.path == ci {
                if index >= branch.segment && index <= branch.segment + 1 {
                    return Err(Error::InvalidGeometry);
                }
                if index < branch.segment {
                    match self {
                        Self::Split { .. } => branch.segment += 1,
                        Self::Delete { .. } => branch.segment -= 1,
                    }
                }
            }
        }
        let path = glyph.paths.get_mut(ci).ok_or(Error::InvalidGeometry)?;
        match self {
            Self::Delete { .. } => {
                if index >= path.segments.len() {
                    return Err(Error::InvalidGeometry);
                }
                path.segments.remove(index);
            }
            Self::Split { .. } => {
                let start = if index == 0 {
                    path.start.clone()
                } else {
                    match path.segments.get(index - 1).ok_or(Error::InvalidGeometry)? {
                        Curve::Line(p) | Curve::Cubic(_, _, p) => p.clone(),
                    }
                };
                let mid = |a: &VPoint, b: &VPoint| -> Result<VPoint> {
                    Ok([a[0].add(&b[0])?.scale(0.5)?, a[1].add(&b[1])?.scale(0.5)?])
                };
                if index == path.segments.len() {
                    path.segments.push(Curve::Line(mid(&start, &path.start)?));
                } else {
                    let (left, right) = match path.segments[index].clone() {
                        Curve::Line(end) => (Curve::Line(mid(&start, &end)?), Curve::Line(end)),
                        Curve::Cubic(a, b, end) => {
                            let p = mid(&start, &a)?;
                            let q = mid(&a, &b)?;
                            let r = mid(&b, &end)?;
                            let s = mid(&p, &q)?;
                            let t = mid(&q, &r)?;
                            let u = mid(&s, &t)?;
                            (Curve::Cubic(p, s, u.clone()), Curve::Cubic(t, r, end))
                        }
                    };
                    path.segments.splice(index..=index, [left, right]);
                }
            }
        }
        Ok(())
    }
}
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub errors: BTreeMap<u16, Vec<(super::masters::Support, f64)>>,
}
pub fn prepare(bytes: &[u8], operations: &BTreeMap<u16, Vec<Operation>>) -> Result<Vec<u8>> {
    Ok(prepare_report(bytes, operations)?.bytes)
}
pub fn prepare_report(
    bytes: &[u8],
    operations: &BTreeMap<u16, Vec<Operation>>,
) -> Result<Prepared> {
    if operations.is_empty() {
        return Ok(Prepared {
            bytes: bytes.to_vec(),
            errors: BTreeMap::new(),
        });
    }
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::InvalidFile)?;
    let axes: Vec<_> = face
        .variation_axes()
        .into_iter()
        .map(|a| a.tag.to_string())
        .collect();
    if let Some(cff) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"CFF2")) {
        let font =
            master_cff::Font::parse(cff, &axes, face.number_of_glyphs(), face.units_per_em())?;
        let mut glyphs = BTreeMap::new();
        for (&id, ops) in operations {
            let mut g = font.glyph(id)?;
            for op in ops {
                op.symbolic(&mut g)?;
            }
            glyphs.insert(id, g);
        }
        let (bytes, error) = font.rewrite_sfnt(bytes, &glyphs, &font.supports)?;
        return Ok(Prepared {
            bytes,
            errors: glyphs
                .keys()
                .map(|&id| (id, vec![(super::masters::Support::default(), error)]))
                .collect(),
        });
    }
    let raw_gvar = face
        .raw_face()
        .table(ttf_parser::Tag::from_bytes(b"gvar"))
        .ok_or(Error::InvalidFile)?;
    let table = master_gvar::Table::parse(raw_gvar, &axes, face.number_of_glyphs())?;
    let mut plans = BTreeMap::new();
    for (&id, ops) in operations {
        let g = master_glyf::read(bytes, 0, id)?.ok_or(Error::InvalidGeometry)?;
        let source = table.glyph(id, g.points.len() + 4)?;
        let original = g.topology.contours(&g.points)?;
        let mut base = original.clone();
        for op in ops {
            op.apply(&mut base)?;
        }
        let (topology, points) = Topology::quadratic(&base)?;
        let mut new = master_glyf::Glyph::from_points(topology.rings().to_vec(), points.clone())?;
        new.topology = topology.clone();
        new.overlap = g.overlap;
        let mut encoded = new.encode_base(&points)?;
        let old_ids = g.topology.point_ids()?;
        let new_ids: BTreeMap<_, _> = topology
            .point_ids()?
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        encoded.remap = Some(old_ids.iter().map(|id| new_ids.get(id).copied()).collect());
        let mut tuples = vec![];
        let mut errors = vec![];
        for tuple in source {
            let expanded = g.expanded(&tuple)?;
            let region: Vec<_> = g
                .points
                .iter()
                .zip(&expanded)
                .map(|(a, b)| [a[0] + b[0], a[1] + b[1]])
                .collect();
            let mut contours = g.topology.contours(&region)?;
            for op in ops {
                op.apply(&mut contours)?;
            }
            topology.align(&mut contours)?;
            let points2 = topology.physical_points(&contours)?;
            let mut deltas: Vec<_> = points2
                .iter()
                .zip(&points)
                .map(|(a, b)| [a[0] - b[0], a[1] - b[1]])
                .collect();
            deltas.extend_from_slice(&expanded[g.points.len()..]);
            let q = master_quantize::quantize(&tuple.support, &deltas)?;
            if q.max_error > 0. {
                errors.push((tuple.support.clone(), q.max_error));
            }
            tuples.extend(q.tuples);
        }
        plans.insert(
            id,
            Rebased {
                base: encoded,
                tuples,
                errors,
            },
        );
    }
    let errors = plans
        .iter()
        .map(|(&id, p)| {
            let mut errors = p.errors.clone();
            if p.base.max_error > 0. {
                errors.push((super::masters::Support::default(), p.base.max_error));
            }
            (id, errors)
        })
        .collect();
    Ok(Prepared {
        bytes: master_tables::apply(bytes, &plans)?,
        errors,
    })
}
