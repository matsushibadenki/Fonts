//! Rebase a simple glyph while retaining the original variation function within reported rounding error.
use super::{
    master_glyf::{Encoded, Glyph},
    master_gvar::Tuple,
    master_quantize,
    masters::{Location, Support},
};
use crate::{Error, Point, Result};
pub struct Rebased {
    pub base: Encoded,
    pub tuples: Vec<Tuple>,
    pub errors: Vec<(Support, f64)>,
}
impl Rebased {
    pub fn error_bound(&self, location: &Location) -> Result<f64> {
        Support::default().weight(location)?;
        let mut bound = self.base.max_error;
        for (support, error) in &self.errors {
            bound += support.weight(location)?.abs() * error;
        }
        Ok(bound)
    }
}

#[cfg(test)]
mod tests;
/// Preserve sparse tuples when their IUP result stays identical; otherwise make their deltas explicit.
pub fn rebase(glyph: &Glyph, source: &[Tuple], points: &[Point]) -> Result<Rebased> {
    if source.len() > 4095 {
        return Err(Error::TooLarge);
    }
    let base = glyph.encode_base(points)?;
    let relocated = glyph.relocated(base.points.clone());
    let mut tuples = vec![];
    let mut errors = vec![];
    let mut explicit = 0usize;
    for tuple in source {
        let old = glyph.expanded(tuple)?;
        let candidates = if old == relocated.expanded(tuple)? {
            vec![tuple.clone()]
        } else {
            let quantized = master_quantize::quantize(&tuple.support, &old)?;
            if quantized.max_error > 0. {
                errors.push((tuple.support.clone(), quantized.max_error));
            }
            quantized.tuples
        };
        for candidate in candidates {
            explicit = explicit
                .checked_add(candidate.deltas.len())
                .ok_or(Error::TooLarge)?;
            if explicit > 4_000_000 || tuples.len() >= 4095 {
                return Err(Error::TooLarge);
            }
            tuples.push(candidate);
        }
    }
    Ok(Rebased {
        base,
        tuples,
        errors,
    })
}
