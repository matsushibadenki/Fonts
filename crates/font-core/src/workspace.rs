//! Panel locations are Rust-owned. Windows only render a snapshot of this state.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Glyphs,
    Metrics,
    Outline,
}
impl Panel {
    pub const ALL: [Self; 3] = [Self::Glyphs, Self::Metrics, Self::Outline];
    pub fn label(self) -> &'static str {
        match self {
            Self::Glyphs => "panel-glyphs",
            Self::Metrics => "panel-metrics",
            Self::Outline => "panel-outline",
        }
    }
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.label() == label)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dock {
    Left,
    Right,
    Bottom,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Placement {
    Docked { dock: Dock },
    Floating { return_to: Dock },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    En,
    #[default]
    Ja,
    ZhCn,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Nodes,
    Pen,
    Pan,
    Images,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub schema_version: u32,
    pub revision: u64,
    pub panels: BTreeMap<Panel, Placement>,
    pub active: BTreeMap<Dock, Panel>,
    pub locale: Locale,
    pub left_width: f64,
    pub right_width: f64,
    pub bottom_height: f64,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            panels: [
                (Panel::Glyphs, Placement::Docked { dock: Dock::Left }),
                (Panel::Metrics, Placement::Docked { dock: Dock::Right }),
                (Panel::Outline, Placement::Docked { dock: Dock::Bottom }),
            ]
            .into(),
            active: [
                (Dock::Left, Panel::Glyphs),
                (Dock::Right, Panel::Metrics),
                (Dock::Bottom, Panel::Outline),
            ]
            .into(),
            locale: Locale::Ja,
            left_width: 208.,
            right_width: 240.,
            bottom_height: 128.,
        }
    }
}
impl Workspace {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.panels.len() != 3
            || !Panel::ALL.iter().all(|p| self.panels.contains_key(p))
            || !self.left_width.is_finite()
            || !(160. ..=360.).contains(&self.left_width)
            || !self.right_width.is_finite()
            || !(180. ..=360.).contains(&self.right_width)
            || !self.bottom_height.is_finite()
            || !(100. ..=260.).contains(&self.bottom_height)
        {
            return Err(Error::InvalidDocument);
        }
        for dock in [Dock::Left, Dock::Right, Dock::Bottom] {
            let has_panels = self
                .panels
                .values()
                .any(|v| *v == Placement::Docked { dock });
            if has_panels != self.active.contains_key(&dock)
                || self
                    .active
                    .get(&dock)
                    .is_some_and(|p| self.panels.get(p) != Some(&Placement::Docked { dock }))
            {
                return Err(Error::InvalidDocument);
            }
        }
        Ok(())
    }
    pub fn place(&mut self, expected: u64, panel: Panel, placement: Placement) -> Result<()> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        let next = self.revision.checked_add(1).ok_or(Error::Conflict)?;
        self.panels.insert(panel, placement);
        for dock in [Dock::Left, Dock::Right, Dock::Bottom] {
            if let Some((&p, _)) = self
                .panels
                .iter()
                .find(|(_, v)| **v == Placement::Docked { dock })
            {
                if !self
                    .active
                    .get(&dock)
                    .is_some_and(|p| self.panels.get(p) == Some(&Placement::Docked { dock }))
                {
                    self.active.insert(dock, p);
                }
            } else {
                self.active.remove(&dock);
            }
        }
        if let Placement::Docked { dock } = placement {
            self.active.insert(dock, panel);
        }
        self.revision = next;
        Ok(())
    }
    pub fn resize(&mut self, expected: u64, dock: Dock, value: f64) -> Result<()> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        let (min, max) = match dock {
            Dock::Left => (160., 360.),
            Dock::Right => (180., 360.),
            Dock::Bottom => (100., 260.),
        };
        if !value.is_finite() || !(min..=max).contains(&value) {
            return Err(Error::InvalidDocument);
        }
        let next = self.revision.checked_add(1).ok_or(Error::Conflict)?;
        match dock {
            Dock::Left => self.left_width = value,
            Dock::Right => self.right_width = value,
            Dock::Bottom => self.bottom_height = value,
        }
        self.revision = next;
        Ok(())
    }
    pub fn activate(&mut self, expected: u64, panel: Panel) -> Result<()> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        let Some(Placement::Docked { dock }) = self.panels.get(&panel) else {
            return Err(Error::InvalidDocument);
        };
        let next = self.revision.checked_add(1).ok_or(Error::Conflict)?;
        self.active.insert(*dock, panel);
        self.revision = next;
        Ok(())
    }
    pub fn redock(&mut self, panel: Panel) -> Result<()> {
        if let Some(Placement::Floating { return_to }) = self.panels.get(&panel).copied() {
            self.place(self.revision, panel, Placement::Docked { dock: return_to })?;
        }
        Ok(())
    }
    /// Floating geometry is intentionally not restored until monitor recovery exists.
    pub fn restore_docked(mut self) -> Result<Self> {
        self.validate()?;
        for p in Panel::ALL {
            self.redock(p)?;
        }
        Ok(self)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn move_float_close_and_restore_never_lose_a_panel() {
        let mut w = Workspace::default();
        w.place(0, Panel::Outline, Placement::Docked { dock: Dock::Right })
            .unwrap();
        assert_eq!(w.active[&Dock::Right], Panel::Outline);
        assert!(!w.active.contains_key(&Dock::Bottom));
        w.place(
            1,
            Panel::Outline,
            Placement::Floating {
                return_to: Dock::Right,
            },
        )
        .unwrap();
        assert_eq!(w.active[&Dock::Right], Panel::Metrics);
        w.validate().unwrap();
        assert_eq!(
            w.place(0, Panel::Glyphs, Placement::Docked { dock: Dock::Right }),
            Err(Error::Conflict)
        );
        let restored: Workspace =
            serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        let restored = restored.restore_docked().unwrap();
        assert_eq!(
            restored.panels[&Panel::Outline],
            Placement::Docked { dock: Dock::Right }
        );
        restored.validate().unwrap();
    }
    #[test]
    fn resizes_are_bounded_and_revision_checked() {
        let mut w = Workspace::default();
        w.resize(0, Dock::Left, 280.).unwrap();
        assert_eq!(w.left_width, 280.);
        assert_eq!(w.resize(0, Dock::Right, 250.), Err(Error::Conflict));
        assert_eq!(
            w.resize(1, Dock::Bottom, f64::INFINITY),
            Err(Error::InvalidDocument)
        );
        assert_eq!(w.revision, 1);
        w.validate().unwrap();
    }
    #[test]
    fn invalid_layout_and_future_versions_are_rejected() {
        let w = Workspace {
            left_width: f64::NAN,
            ..Workspace::default()
        };
        assert!(w.validate().is_err());
        let mut w = Workspace::default();
        w.active.insert(Dock::Left, Panel::Metrics);
        assert!(w.validate().is_err());
        let w = Workspace {
            schema_version: 99,
            ..Workspace::default()
        };
        assert!(w.restore_docked().is_err());
    }
}
