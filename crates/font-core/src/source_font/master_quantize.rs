//! Quantize one outline correction once, then split it into representable gvar tuples.
use super::{master_gvar::Tuple, master_tuple, masters::Support};
use crate::{Error, Point, Result};

pub struct Quantized {
    pub tuples: Vec<Tuple>,
    /// Maximum absolute rounding error of a coordinate before applying the region scalar.
    pub max_error: f64,
}
/// `deltas` includes the four phantom points. Zero deltas remain explicitly touched:
/// omitting them in a simple glyph would allow IUP to alter the intended correction.
/// A nonzero constant correction must be applied to the base glyph, not a zero-peak tuple.
pub fn quantize(support: &Support, deltas: &[Point]) -> Result<Quantized> {
    if !(4..=1_000_000).contains(&deltas.len()) {
        return Err(Error::TooLarge);
    }
    let peak = support
        .axes
        .iter()
        .map(|(tag, r)| (tag.clone(), r[1]))
        .collect();
    support.weight(&peak)?;
    if support.axes.values().all(|r| r[1] == 0.) && deltas.iter().any(|p| *p != [0.; 2]) {
        return Err(Error::InvalidGeometry);
    }
    let mut max_error = 0f64;
    let mut remaining = Vec::with_capacity(deltas.len());
    let mut parts = 0usize;
    for delta in deltas {
        let mut value = [0i32; 2];
        for axis in 0..2 {
            let v = delta[axis];
            if !v.is_finite() || v.abs() > 1_000_000_000. {
                return Err(Error::InvalidGeometry);
            }
            let rounded = v.round();
            max_error = max_error.max((rounded - v).abs());
            value[axis] = rounded as i32;
            let pieces = if value[axis] >= 0 {
                (value[axis] + 32766) / 32767
            } else {
                (-value[axis] + 32767) / 32768
            };
            parts = parts.max(pieces as usize);
        }
        remaining.push(value);
    }
    if parts == 0 {
        return Ok(Quantized {
            tuples: vec![],
            max_error,
        });
    }
    // Every split tuple supplies ALL physical points, including zero corrections, to disable IUP.
    // Dense all-point runs are compact for most glyphs; individual payloads must still fit u16.
    if parts > 4095
        || parts
            .checked_mul(deltas.len())
            .is_none_or(|v| v > 4_000_000)
    {
        return Err(Error::TooLarge);
    }
    let mut tuples = Vec::with_capacity(parts);
    for _ in 0..parts {
        let mut values = Vec::with_capacity(remaining.len());
        for point in &mut remaining {
            let mut delta = [0i16; 2];
            for axis in 0..2 {
                delta[axis] = point[axis].clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                point[axis] -= i32::from(delta[axis]);
            }
            values.push(delta);
        }
        if packed_size(&values)? <= usize::from(u16::MAX) {
            push(&mut tuples, support, values)?;
        } else {
            // A tuple's serialized data size is u16 even when its glyph has many points.
            // Partition nonzero corrections, retaining explicitly touched zeros in every tuple.
            for start in (0..values.len()).step_by(4096) {
                let end = (start + 4096).min(values.len());
                if values[start..end].iter().all(|v| *v == [0; 2]) {
                    continue;
                }
                if (tuples.len() + 1)
                    .checked_mul(values.len())
                    .is_none_or(|n| n > 4_000_000)
                {
                    return Err(Error::TooLarge);
                }
                let mut block = vec![[0; 2]; values.len()];
                block[start..end].copy_from_slice(&values[start..end]);
                if packed_size(&block)? > usize::from(u16::MAX) {
                    return Err(Error::TooLarge);
                }
                push(&mut tuples, support, block)?;
            }
        }
    }
    Ok(Quantized { tuples, max_error })
}
fn packed_size(values: &[[i16; 2]]) -> Result<usize> {
    Ok(
        1 + master_tuple::write_deltas(&values.iter().map(|v| v[0]).collect::<Vec<_>>())?.len()
            + master_tuple::write_deltas(&values.iter().map(|v| v[1]).collect::<Vec<_>>())?.len(),
    )
}
fn push(tuples: &mut Vec<Tuple>, support: &Support, values: Vec<[i16; 2]>) -> Result<()> {
    if tuples.len() >= 4095
        || (tuples.len() + 1)
            .checked_mul(values.len())
            .is_none_or(|n| n > 4_000_000)
    {
        return Err(Error::TooLarge);
    }
    tuples.push(Tuple {
        support: support.clone(),
        points: None,
        deltas: values,
    });
    Ok(())
}
