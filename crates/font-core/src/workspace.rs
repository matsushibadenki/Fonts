//! Panel locations are Rust-owned. Windows only render a snapshot of this state.
use crate::{
    dock_tree::{DockNode, Side},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Glyphs,
    Metrics,
    Outline,
    Snippets,
}
impl Panel {
    pub const ALL: [Self; 4] = [Self::Glyphs, Self::Metrics, Self::Outline, Self::Snippets];
    pub fn label(self) -> &'static str {
        match self {
            Self::Glyphs => "panel-glyphs",
            Self::Metrics => "panel-metrics",
            Self::Outline => "panel-outline",
            Self::Snippets => "panel-snippets",
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
    Snippets,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    #[serde(default)]
    pub trees: BTreeMap<Dock, DockNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub floating_bounds: BTreeMap<Panel, crate::floating_bounds::Bounds>,
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
            trees: [
                (Dock::Left, DockNode::tabs(Panel::Glyphs)),
                (
                    Dock::Right,
                    DockNode::Tabs {
                        panels: vec![Panel::Metrics, Panel::Snippets],
                        active: Panel::Metrics,
                    },
                ),
                (Dock::Bottom, DockNode::tabs(Panel::Outline)),
            ]
            .into(),
            floating_bounds: BTreeMap::new(),
            schema_version: 1,
            revision: 0,
            panels: [
                (Panel::Glyphs, Placement::Docked { dock: Dock::Left }),
                (Panel::Metrics, Placement::Docked { dock: Dock::Right }),
                (Panel::Snippets, Placement::Docked { dock: Dock::Right }),
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
            || self.panels.len() != 4
            || !Panel::ALL.iter().all(|p| self.panels.contains_key(p))
            || !self.left_width.is_finite()
            || !(160. ..=360.).contains(&self.left_width)
            || !self.right_width.is_finite()
            || !(180. ..=360.).contains(&self.right_width)
            || !self.bottom_height.is_finite()
            || !(100. ..=520.).contains(&self.bottom_height)
        {
            return Err(Error::InvalidDocument);
        }
        for bounds in self.floating_bounds.values() {
            bounds.validate()?;
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
        let mut seen = Vec::new();
        for (dock, tree) in &self.trees {
            let mut panels = Vec::new();
            tree.collect(&mut panels, 0)?;
            for p in panels {
                let owner = match self.panels[&p] {
                    Placement::Docked { dock } => dock,
                    Placement::Floating { return_to } => return_to,
                };
                if owner != *dock || seen.contains(&p) {
                    return Err(Error::InvalidDocument);
                }
                seen.push(p);
            }
        }
        if seen.len() != 4 {
            return Err(Error::InvalidDocument);
        }
        Ok(())
    }
    fn remove_slot(&mut self, panel: Panel) {
        self.trees = std::mem::take(&mut self.trees)
            .into_iter()
            .filter_map(|(d, n)| n.remove(panel).map(|n| (d, n)))
            .collect();
    }
    fn append_slot(&mut self, dock: Dock, panel: Panel) {
        if let Some(tree) = self.trees.get_mut(&dock) {
            let mut panels = Vec::new();
            // Already validated; every existing tree has a tab target.
            let _ = tree.collect(&mut panels, 0);
            tree.insert(panels[0], panel, Side::Tab);
        } else {
            self.trees.insert(dock, DockNode::tabs(panel));
        }
    }
    pub fn place(&mut self, expected: u64, panel: Panel, placement: Placement) -> Result<()> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        let next = self.revision.checked_add(1).ok_or(Error::Conflict)?;
        match placement {
            Placement::Docked { dock } => {
                let old_dock = match self.panels[&panel] {
                    Placement::Docked { dock } => dock,
                    Placement::Floating { return_to } => return_to,
                };
                if old_dock != dock {
                    self.remove_slot(panel);
                    self.append_slot(dock, panel);
                }
                if let Some(tree) = self.trees.get_mut(&dock) {
                    tree.activate(panel);
                }
            }
            Placement::Floating { return_to } => {
                let old_dock = match self.panels[&panel] {
                    Placement::Docked { dock } => dock,
                    Placement::Floating { return_to } => return_to,
                };
                if old_dock != return_to {
                    self.remove_slot(panel);
                    self.append_slot(return_to, panel);
                }
            }
        }
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
            Dock::Bottom => (100., 520.),
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
        self.trees
            .get_mut(dock)
            .ok_or(Error::InvalidDocument)?
            .activate(panel);
        self.active.insert(*dock, panel);
        self.revision = next;
        Ok(())
    }
    pub fn relative(
        &mut self,
        expected: u64,
        panel: Panel,
        target: Panel,
        side: Side,
    ) -> Result<()> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        self.validate()?;
        if panel == target {
            return Err(Error::InvalidDocument);
        }
        let Some(Placement::Docked { dock }) = self.panels.get(&target).copied() else {
            return Err(Error::InvalidDocument);
        };
        let mut next = self.clone();
        next.remove_slot(panel);
        if !next
            .trees
            .get_mut(&dock)
            .ok_or(Error::InvalidDocument)?
            .insert(target, panel, side)
        {
            return Err(Error::InvalidDocument);
        }
        next.panels.insert(panel, Placement::Docked { dock });
        next.place(expected, panel, Placement::Docked { dock })?;
        // place would otherwise migrate the newly inserted slot a second time.
        if dock == Dock::Bottom && matches!(side, Side::Top | Side::Bottom) {
            next.bottom_height = next.bottom_height.max(300.);
        }
        if matches!(side, Side::Left | Side::Right) {
            match dock {
                Dock::Left => next.left_width = next.left_width.max(320.),
                Dock::Right => next.right_width = next.right_width.max(320.),
                Dock::Bottom => {}
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn resize_split(
        &mut self,
        expected: u64,
        dock: Dock,
        path: &[u8],
        value: f64,
    ) -> Result<()> {
        if expected != self.revision {
            return Err(Error::Conflict);
        }
        let revision = self.revision.checked_add(1).ok_or(Error::Conflict)?;
        self.trees
            .get_mut(&dock)
            .ok_or(Error::InvalidDocument)?
            .resize(path, value)?;
        self.revision = revision;
        Ok(())
    }
    pub fn redock(&mut self, panel: Panel) -> Result<()> {
        if let Some(Placement::Floating { return_to }) = self.panels.get(&panel).copied() {
            self.place(self.revision, panel, Placement::Docked { dock: return_to })?;
        }
        Ok(())
    }
    /// Upgrade legacy panel catalogs without losing saved floating placements.
    pub fn restore(mut self) -> Result<Self> {
        if self.schema_version == 1
            && self.panels.len() == 3
            && [Panel::Glyphs, Panel::Metrics, Panel::Outline]
                .iter()
                .all(|p| self.panels.contains_key(p))
        {
            self.panels
                .insert(Panel::Snippets, Placement::Docked { dock: Dock::Right });
            self.active.entry(Dock::Right).or_insert(Panel::Snippets);
        }
        if self.panels.len() != 4 || !Panel::ALL.iter().all(|p| self.panels.contains_key(p)) {
            return Err(Error::InvalidDocument);
        }
        if self.trees.is_empty() {
            for panel in Panel::ALL {
                let dock = match self.panels[&panel] {
                    Placement::Docked { dock } => dock,
                    Placement::Floating { return_to } => return_to,
                };
                self.append_slot(dock, panel);
            }
            for (dock, panel) in &self.active {
                if let Some(tree) = self.trees.get_mut(dock) {
                    tree.activate(*panel);
                }
            }
        }
        self.floating_bounds.retain(|_, b| b.validate().is_ok());
        self.validate()?;
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
        let restored = restored.restore().unwrap();
        assert_eq!(
            restored.panels[&Panel::Outline],
            Placement::Floating {
                return_to: Dock::Right
            }
        );
        restored.validate().unwrap();
    }
    #[test]
    fn legacy_workspace_gains_snippets_without_losing_existing_panel_locations() {
        let mut w = Workspace::default();
        w.panels.remove(&Panel::Snippets);
        w.left_width = 280.;
        let restored = w.restore().unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.left_width, 280.);
        assert_eq!(
            restored.panels[&Panel::Snippets],
            Placement::Docked { dock: Dock::Right }
        );
        assert_eq!(restored.active[&Dock::Right], Panel::Metrics);
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
        assert!(w.restore().is_err());
    }
}
