//! Pure monitor recovery. Desktop positions/work areas are physical; sizes are logical.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Monitor {
    pub name: Option<String>,
    pub work_area: Rect,
    pub scale: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub position: [f64; 2],
    pub inner_size: [f64; 2],
    pub outer_size: [f64; 2],
    pub monitor: Monitor,
}
impl Monitor {
    pub fn validate(&self) -> Result<()> {
        let r = self.work_area;
        if ![r.x, r.y, r.width, r.height, self.scale]
            .iter()
            .all(|v| v.is_finite())
            || r.x.abs() > 10_000_000.
            || r.y.abs() > 10_000_000.
            || !(16. ..=1_000_000.).contains(&r.width)
            || !(16. ..=1_000_000.).contains(&r.height)
            || !(0.25..=8.).contains(&self.scale)
            || self.name.as_ref().is_some_and(|s| s.len() > 1024)
        {
            return Err(Error::InvalidDocument);
        }
        Ok(())
    }
}
impl Bounds {
    pub fn validate(&self) -> Result<()> {
        self.monitor.validate()?;
        if self
            .position
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 10_000_000.)
        {
            return Err(Error::InvalidDocument);
        }
        for i in 0..2 {
            if !self.inner_size[i].is_finite()
                || !self.outer_size[i].is_finite()
                || !(1. ..=32768.).contains(&self.inner_size[i])
                || self.outer_size[i] < self.inner_size[i]
                || self.outer_size[i] - self.inner_size[i] > 512.
            {
                return Err(Error::InvalidDocument);
            }
        }
        Ok(())
    }
    pub fn physical_rect(&self) -> Rect {
        Rect {
            x: self.position[0],
            y: self.position[1],
            width: self.outer_size[0] * self.monitor.scale,
            height: self.outer_size[1] * self.monitor.scale,
        }
    }
}
fn overlap(a: Rect, b: Rect) -> f64 {
    ((a.x + a.width).min(b.x + b.width) - a.x.max(b.x)).max(0.)
        * ((a.y + a.height).min(b.y + b.height) - a.y.max(b.y)).max(0.)
}
/// Prefer the same named display, disambiguated by old work area; otherwise use overlap,
/// then the main window's display. Never span a gap between displays.
pub fn recover(saved: &Bounds, monitors: &[Monitor], fallback: usize) -> Result<Bounds> {
    saved.validate()?;
    if monitors.is_empty() {
        return Err(Error::InvalidDocument);
    }
    for m in monitors {
        m.validate()?;
    }
    let best = |indices: Vec<usize>, rect: Rect| {
        indices.into_iter().max_by(|a, b| {
            let score = |i: usize| {
                let r = monitors[i].work_area;
                let dx = (rect.x + rect.width / 2.) - (r.x + r.width / 2.);
                let dy = (rect.y + rect.height / 2.) - (r.y + r.height / 2.);
                (overlap(rect, r), -(dx * dx + dy * dy))
            };
            let a = score(*a);
            let b = score(*b);
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        })
    };
    let named = if let Some(name) = &saved.monitor.name {
        monitors
            .iter()
            .enumerate()
            .filter(|(_, m)| m.name.as_ref() == Some(name))
            .map(|(i, _)| i)
            .collect()
    } else {
        vec![]
    };
    let matched = best(named, saved.monitor.work_area)
        .or_else(|| monitors.iter().position(|m| *m == saved.monitor));
    let index = matched
        .or_else(|| {
            let rect = saved.physical_rect();
            best(
                (0..monitors.len())
                    .filter(|i| overlap(rect, monitors[*i].work_area) > 0.)
                    .collect(),
                rect,
            )
        })
        .unwrap_or(fallback.min(monitors.len() - 1));
    let target = &monitors[index];
    let area = target.work_area;
    let s = target.scale;
    let mut result = saved.clone();
    result.monitor = target.clone();
    // Preserve the user's offset in logical units when this display changes scale or position.
    if matched.is_some() {
        result.position = [
            area.x + (saved.position[0] - saved.monitor.work_area.x) / saved.monitor.scale * s,
            area.y + (saved.position[1] - saved.monitor.work_area.y) / saved.monitor.scale * s,
        ];
    }
    let margin = if area.width >= 64. * s && area.height >= 64. * s {
        8. * s
    } else {
        0.
    };
    for i in 0..2 {
        let (origin, extent) = if i == 0 {
            (area.x, area.width)
        } else {
            (area.y, area.height)
        };
        let chrome = (saved.outer_size[i] - saved.inner_size[i])
            .min((extent - 2. * margin) / s - 1.)
            .max(0.);
        let available = ((extent - 2. * margin) / s - chrome).max(1.);
        let minimum: f64 = if i == 0 { 260. } else { 240. };
        result.inner_size[i] = saved.inner_size[i]
            .max(minimum.min(available))
            .min(available);
        result.outer_size[i] = result.inner_size[i] + chrome;
        let low = origin + margin;
        let high = (origin + extent - margin - result.outer_size[i] * s).max(low);
        result.position[i] = result.position[i].clamp(low, high);
    }
    result.validate()?;
    Ok(result)
}
