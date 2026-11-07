//! Cardinal corrections on compatible master outlines; source variation remains the baseline.
use crate::{geometry, identity::Uuid, Contour, Error, Glyph, GlyphId, Result, Segment};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub type Location = BTreeMap<String, f32>;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Support {
    pub axes: BTreeMap<String, [f32; 3]>,
}
impl Support {
    pub fn weight(&self, location: &Location) -> Result<f64> {
        if location
            .values()
            .any(|v| !v.is_finite() || !(-1. ..=1.).contains(v))
        {
            return Err(Error::InvalidGeometry);
        }
        let mut weight = 1.;
        for (tag, &[start, peak, end]) in &self.axes {
            if tag.len() != 4
                || !tag.is_ascii()
                || ![start, peak, end].iter().all(|v| v.is_finite())
                || start < -1.
                || end > 1.
                || start > peak
                || peak > end
                || (start < 0. && end > 0. && peak != 0.)
                || (peak == 0. && [start, peak, end] != [0.; 3])
            {
                return Err(Error::InvalidGeometry);
            }
            if peak == 0. {
                continue;
            }
            let value = *location.get(tag).ok_or(Error::InvalidGeometry)?;
            if !value.is_finite() || !(-1. ..=1.).contains(&value) {
                return Err(Error::InvalidGeometry);
            }
            let factor = if value == peak {
                1.
            } else if value <= start || value >= end {
                0.
            } else if value < peak {
                f64::from(value - start) / f64::from(peak - start)
            } else {
                f64::from(end - value) / f64::from(end - peak)
            };
            weight *= factor;
        }
        Ok(weight)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub advance: f64,
    pub contours: Vec<Contour>,
}
impl Shape {
    pub fn from_glyph(glyph: &Glyph) -> Self {
        Self {
            advance: glyph.advance,
            contours: glyph.contours.clone(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !self.advance.is_finite()
            || !(0. ..=65535.).contains(&self.advance)
            || self.contours.len() > 4096
        {
            return Err(Error::InvalidGeometry);
        }
        for c in &self.contours {
            geometry::validate(c)?;
        }
        Ok(())
    }
    pub fn compatible(&self, other: &Self) -> bool {
        self.contours.len() == other.contours.len()
            && self.contours.iter().zip(&other.contours).all(|(a, b)| {
                a.closed == b.closed
                    && a.identity == b.identity
                    && a.segments.len() == b.segments.len()
                    && a.segments
                        .iter()
                        .zip(&b.segments)
                        .all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b))
            })
    }
    fn values(&self) -> Vec<f64> {
        let mut values = vec![self.advance];
        for c in &self.contours {
            values.extend(c.start);
            for s in &c.segments {
                match s {
                    Segment::Line { end } => values.extend(end),
                    Segment::Quadratic { control, end } => {
                        values.extend(control);
                        values.extend(end);
                    }
                    Segment::Cubic {
                        control1,
                        control2,
                        end,
                    } => {
                        values.extend(control1);
                        values.extend(control2);
                        values.extend(end);
                    }
                }
            }
        }
        values
    }
    fn with_values(&self, values: &[f64]) -> Result<Self> {
        if values
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
        {
            return Err(Error::InvalidGeometry);
        }
        let mut out = self.clone();
        let mut at = 1;
        out.advance = *values.first().ok_or(Error::InvalidGeometry)?;
        let point = |at: &mut usize| -> Result<[f64; 2]> {
            let p = values.get(*at..*at + 2).ok_or(Error::InvalidGeometry)?;
            *at += 2;
            Ok([p[0], p[1]])
        };
        for c in &mut out.contours {
            c.start = point(&mut at)?;
            for s in &mut c.segments {
                match s {
                    Segment::Line { end } => *end = point(&mut at)?,
                    Segment::Quadratic { control, end } => {
                        *control = point(&mut at)?;
                        *end = point(&mut at)?;
                    }
                    Segment::Cubic {
                        control1,
                        control2,
                        end,
                    } => {
                        *control1 = point(&mut at)?;
                        *control2 = point(&mut at)?;
                        *end = point(&mut at)?;
                    }
                }
            }
        }
        if at != values.len() {
            return Err(Error::InvalidGeometry);
        }
        out.validate()?;
        Ok(out)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Master {
    pub id: Uuid,
    pub name: String,
    /// Original user-space coordinates, retained for display and source normalization checks.
    pub design_location: Location,
    /// Normalized, avar-mapped coordinates.
    pub location: Location,
    pub support: Support,
    /// Absolute edited shapes. Missing entries retain the sampled source shape at this master.
    pub glyphs: BTreeMap<GlyphId, Shape>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Masters {
    pub entries: Vec<Master>,
}
impl Masters {
    fn validate_structure(&self) -> Result<()> {
        if self.entries.len() > 64 {
            return Err(Error::TooLarge);
        }
        let mut ids = BTreeSet::new();
        let mut constants = 0;
        let mut values = 0usize;
        let mut topology = BTreeMap::<GlyphId, &Shape>::new();
        let axes = self
            .entries
            .first()
            .map(|m| m.location.keys().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        for m in &self.entries {
            if m.id.is_nil()
                || !ids.insert(m.id)
                || m.name.trim().is_empty()
                || m.name.len() > 1024
                || axes.len() > 63
                || m.location.keys().cloned().collect::<BTreeSet<_>>() != axes
                || m.design_location.keys().cloned().collect::<BTreeSet<_>>() != axes
                || m.location
                    .values()
                    .any(|v| !v.is_finite() || !(-1. ..=1.).contains(v))
                || m.design_location
                    .values()
                    .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
                || axes.iter().any(|a| a.len() != 4 || !a.is_ascii())
                || m.support.axes.keys().any(|a| !axes.contains(a))
                || m.glyphs.keys().any(|&id| id >= 65535)
            {
                return Err(Error::InvalidGeometry);
            }
            if m.support.weight(&m.location)? <= 1e-6 {
                return Err(Error::InvalidGeometry);
            }
            if m.support.axes.values().all(|s| s[1] == 0.) {
                constants += 1;
                if m.location.values().any(|&v| v != 0.) || constants > 1 {
                    return Err(Error::InvalidGeometry);
                }
            }
            for (&id, shape) in &m.glyphs {
                shape.validate()?;
                if let Some(previous) = topology.insert(id, shape) {
                    if !previous.compatible(shape) {
                        return Err(Error::InvalidGeometry);
                    }
                }
                values = values
                    .checked_add(shape.values().len())
                    .ok_or(Error::TooLarge)?;
                if values > 2_000_000 {
                    return Err(Error::TooLarge);
                }
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_structure()?;
        if let Some(master) = self.entries.first() {
            self.weights(&master.location)?;
        }
        Ok(())
    }
    /// Cardinal weights satisfy every registered master, even when support regions overlap.
    pub fn weights(&self, location: &Location) -> Result<Vec<f64>> {
        self.validate_structure()?;
        if self.entries.is_empty() {
            return Ok(vec![]);
        }
        if location.keys().ne(self.entries[0].location.keys())
            || location
                .values()
                .any(|v| !v.is_finite() || !(-1. ..=1.).contains(v))
        {
            return Err(Error::InvalidGeometry);
        }
        let n = self.entries.len();
        let mut matrix = vec![vec![0.; n + 1]; n];
        // Transposed interpolation matrix: A^T lambda = support(current).
        for (j, master) in self.entries.iter().enumerate() {
            for (i, sample) in self.entries.iter().enumerate() {
                matrix[j][i] = master.support.weight(&sample.location)?;
            }
            matrix[j][n] = master.support.weight(location)?;
        }
        let weights: Vec<_> = solve(matrix, n)?.into_iter().map(|r| r[0]).collect();
        if weights
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
        {
            return Err(Error::InvalidGeometry);
        }
        Ok(weights)
    }
    /// Convert absolute residuals at masters to coefficients of the registered support regions.
    /// Rows follow master order; columns are caller-defined scalar coordinates or metrics.
    pub fn coefficients(&self, residuals: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
        self.validate_structure()?;
        let n = self.entries.len();
        if residuals.len() != n {
            return Err(Error::InvalidGeometry);
        }
        if n == 0 {
            return Ok(vec![]);
        }
        let width = residuals[0].len();
        if width.checked_mul(n).is_none_or(|v| v > 2_000_000) {
            return Err(Error::TooLarge);
        }
        if residuals
            .iter()
            .any(|r| r.len() != width || r.iter().any(|v| !v.is_finite() || v.abs() > 2_000_000.))
        {
            return Err(Error::InvalidGeometry);
        }
        let mut matrix = Vec::with_capacity(n);
        for (sample, residual) in self.entries.iter().zip(residuals) {
            let mut row = self
                .entries
                .iter()
                .map(|master| master.support.weight(&sample.location))
                .collect::<Result<Vec<_>>>()?;
            row.extend(residual);
            matrix.push(row);
        }
        solve(matrix, n)
    }
    /// Source samples must use this registry's master order and the current source topology.
    pub fn interpolate(
        &self,
        glyph: GlyphId,
        location: &Location,
        current: &Shape,
        samples: &[Shape],
    ) -> Result<Shape> {
        current.validate()?;
        if samples.len() != self.entries.len() {
            return Err(Error::InvalidGeometry);
        }
        let weights = self.weights(location)?;
        let mut out = current.values();
        for ((master, sample), weight) in self.entries.iter().zip(samples).zip(weights) {
            sample.validate()?;
            if !current.compatible(sample) {
                return Err(Error::InvalidGeometry);
            }
            if let Some(target) = master.glyphs.get(&glyph) {
                if !sample.compatible(target) {
                    return Err(Error::InvalidGeometry);
                }
                for ((out, target), base) in
                    out.iter_mut().zip(target.values()).zip(sample.values())
                {
                    *out += weight * (target - base);
                }
            }
        }
        current.with_values(&out)
    }
}

/// Gaussian elimination with partial pivoting and multiple right-hand sides.
fn solve(mut matrix: Vec<Vec<f64>>, n: usize) -> Result<Vec<Vec<f64>>> {
    let width = matrix.first().map_or(0, Vec::len);
    for column in 0..n {
        let pivot = (column..n)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))
            .ok_or(Error::InvalidGeometry)?;
        if matrix[pivot][column].abs() < 1e-10 {
            return Err(Error::InvalidGeometry);
        }
        matrix.swap(column, pivot);
        let scale = matrix[column][column];
        for value in &mut matrix[column][column..] {
            *value /= scale;
        }
        let row = matrix[column].clone();
        for (i, target) in matrix.iter_mut().enumerate() {
            if i == column {
                continue;
            }
            let factor = target[column];
            for j in column..width {
                target[j] -= factor * row[j];
            }
        }
    }
    let out: Vec<Vec<f64>> = matrix.into_iter().map(|r| r[n..].to_vec()).collect();
    if out
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000_000.)
    {
        return Err(Error::TooLarge);
    }
    Ok(out)
}
