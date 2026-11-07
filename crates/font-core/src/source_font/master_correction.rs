//! Master residuals expressed in source variation regions, retaining physical point identity.
use super::{
    master_glyf::Glyph,
    master_gvar::Tuple,
    masters::{Masters, Support},
};
use crate::{Error, GlyphId, Point, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    pub support: Support,
    pub advance: f64,
    pub points: Vec<Point>,
}
/// `advances` are original source advances sampled in registry order (including HVAR).
/// Constant-region corrections are returned explicitly: writers must apply them to the base.
pub fn solve(
    glyph: &Glyph,
    id: GlyphId,
    masters: &Masters,
    tuples: &[Tuple],
    advances: &[f64],
) -> Result<Vec<Correction>> {
    masters.validate()?;
    if advances.len() != masters.entries.len()
        || advances
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=65535.).contains(v))
    {
        return Err(Error::InvalidGeometry);
    }
    let count = glyph.topology.point_count()?;
    let width = count
        .checked_mul(2)
        .and_then(|v| v.checked_add(1))
        .ok_or(Error::TooLarge)?;
    if width
        .checked_mul(masters.entries.len())
        .is_none_or(|v| v > 2_000_000)
    {
        return Err(Error::TooLarge);
    }
    let mut residuals = Vec::with_capacity(masters.entries.len());
    for (master, advance) in masters.entries.iter().zip(advances) {
        let mut row = vec![0.; width];
        if let Some(target) = master.glyphs.get(&id) {
            target.validate()?;
            row[0] = target.advance - advance;
            for (out, delta) in row[1..]
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(glyph.outline_delta(tuples, &master.location, &target.contours)?)
            {
                *out = delta;
            }
        }
        residuals.push(row);
    }
    masters
        .coefficients(&residuals)?
        .into_iter()
        .zip(&masters.entries)
        .map(|(values, master)| {
            Ok(Correction {
                support: master.support.clone(),
                advance: values[0],
                points: values[1..].as_chunks::<2>().0.to_vec(),
            })
        })
        .collect()
}
