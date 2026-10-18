//! Bounded, Rust-owned trees. Floating panels retain a dormant slot until explicitly moved.
use crate::{workspace::Panel, Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
    Tab,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DockNode {
    Tabs {
        panels: Vec<Panel>,
        active: Panel,
    },
    Split {
        axis: Axis,
        ratio: f64,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
}
impl DockNode {
    pub fn tabs(panel: Panel) -> Self {
        Self::Tabs {
            panels: vec![panel],
            active: panel,
        }
    }
    pub fn collect(&self, out: &mut Vec<Panel>, depth: usize) -> Result<()> {
        if depth > 3 {
            return Err(Error::InvalidDocument);
        }
        match self {
            Self::Tabs { panels, active } => {
                if panels.is_empty() || panels.len() > 4 || !panels.contains(active) {
                    return Err(Error::InvalidDocument);
                }
                out.extend(panels);
            }
            Self::Split {
                ratio,
                first,
                second,
                ..
            } => {
                if !ratio.is_finite() || !(0.15..=0.85).contains(ratio) {
                    return Err(Error::InvalidDocument);
                }
                first.collect(out, depth + 1)?;
                second.collect(out, depth + 1)?;
            }
        }
        Ok(())
    }
    pub fn remove(self, panel: Panel) -> Option<Self> {
        match self {
            Self::Tabs { mut panels, active } => {
                panels.retain(|p| *p != panel);
                let active = if panels.contains(&active) {
                    active
                } else {
                    *panels.first()?
                };
                Some(Self::Tabs { panels, active })
            }
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(panel), second.remove(panel)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (a, b) => a.or(b),
            },
        }
    }
    pub fn insert(&mut self, target: Panel, panel: Panel, side: Side) -> bool {
        match self {
            Self::Tabs { panels, active } if panels.contains(&target) => {
                if side == Side::Tab {
                    panels.push(panel);
                    *active = panel;
                } else {
                    let old = self.clone();
                    let new = Self::tabs(panel);
                    let before = matches!(side, Side::Left | Side::Top);
                    *self = Self::Split {
                        axis: if matches!(side, Side::Left | Side::Right) {
                            Axis::Horizontal
                        } else {
                            Axis::Vertical
                        },
                        ratio: 0.5,
                        first: Box::new(if before { new.clone() } else { old.clone() }),
                        second: Box::new(if before { old } else { new }),
                    };
                }
                true
            }
            Self::Split { first, second, .. } => {
                first.insert(target, panel, side) || second.insert(target, panel, side)
            }
            _ => false,
        }
    }
    pub fn activate(&mut self, panel: Panel) -> bool {
        match self {
            Self::Tabs { panels, active } if panels.contains(&panel) => {
                *active = panel;
                true
            }
            Self::Split { first, second, .. } => first.activate(panel) || second.activate(panel),
            _ => false,
        }
    }
    pub fn resize(&mut self, path: &[u8], value: f64) -> Result<()> {
        if !value.is_finite() || !(0.15..=0.85).contains(&value) || path.len() > 3 {
            return Err(Error::InvalidDocument);
        }
        match self {
            Self::Split {
                ratio,
                first,
                second,
                ..
            } => {
                if let Some((head, tail)) = path.split_first() {
                    match head {
                        0 => first.resize(tail, value),
                        1 => second.resize(tail, value),
                        _ => Err(Error::InvalidDocument),
                    }
                } else {
                    *ratio = value;
                    Ok(())
                }
            }
            _ => Err(Error::InvalidDocument),
        }
    }
}
