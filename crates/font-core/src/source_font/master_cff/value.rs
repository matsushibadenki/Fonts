//! Bounded polynomials in variation-region scalars; nested CFF2 blends remain exact.
use crate::{Error, Result};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Value(pub BTreeMap<Vec<u16>, f64>);
impl Value {
    pub fn constant(v: f64) -> Self {
        Self([(vec![], v)].into())
    }
    pub fn number(&self) -> Result<f64> {
        if self.0.keys().any(|k| !k.is_empty()) {
            return Err(Error::InvalidGeometry);
        }
        Ok(*self.0.get(&vec![]).unwrap_or(&0.))
    }
    pub fn add(&self, other: &Self) -> Result<Self> {
        self.plus(other, 1.)
    }
    pub fn plus(&self, other: &Self, scale: f64) -> Result<Self> {
        let mut out = self.clone();
        for (key, value) in &other.0 {
            *out.0.entry(key.clone()).or_default() += value * scale;
        }
        out.0.retain(|_, v| *v != 0.);
        if out.0.len() > 4096 || out.0.values().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Err(Error::TooLarge);
        }
        Ok(out)
    }
    pub fn scale(&self, scale: f64) -> Result<Self> {
        Self::default().plus(self, scale)
    }
    pub fn region(&self, region: u16) -> Result<Self> {
        let mut out = Self::default();
        for (key, &v) in &self.0 {
            if key.len() >= 16 {
                return Err(Error::TooLarge);
            }
            let mut key = key.clone();
            key.push(region);
            key.sort_unstable();
            *out.0.entry(key).or_default() += v;
        }
        Ok(out)
    }
    pub fn eval(&self, weights: &[f64]) -> Result<f64> {
        self.0.iter().try_fold(0., |sum, (key, &v)| {
            let factor = key.iter().try_fold(v, |v, &r| {
                Ok(v * weights.get(usize::from(r)).ok_or(Error::InvalidFile)?)
            })?;
            Ok(sum + factor)
        })
    }
    pub fn interval(&self) -> [f64; 2] {
        let base = self.0.get(&vec![]).copied().unwrap_or(0.);
        self.0
            .iter()
            .filter(|(k, _)| !k.is_empty())
            .fold([base, base], |[min, max], (_, v)| {
                [min + v.min(0.), max + v.max(0.)]
            })
    }
}
pub type Point = [Value; 2];
pub fn zero() -> Point {
    [Value::constant(0.), Value::constant(0.)]
}
pub fn add(a: &Point, b: &Point) -> Result<Point> {
    Ok([a[0].add(&b[0])?, a[1].add(&b[1])?])
}
pub fn sub(a: &Point, b: &Point) -> Result<Point> {
    Ok([a[0].plus(&b[0], -1.)?, a[1].plus(&b[1], -1.)?])
}
