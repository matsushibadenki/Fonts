//! Transient, Rust-owned rectangle selection. It never edits the document or history.
use crate::model::Model;
use font_core::{
    geometry::{self, EditPoint},
    Point,
};
use std::collections::BTreeSet;
pub struct Marquee {
    pub start: Point,
    pub end: Point,
    controls: font_core::point_transform::Selection,
    original: BTreeSet<(usize, usize)>,
    primary: Option<(usize, EditPoint)>,
    additive: bool,
}
impl Model {
    pub fn begin_marquee(&mut self, p: Point, additive: bool) {
        let original = self.selected_nodes.clone();
        let controls = self.selected_controls.clone();
        let primary = self.selected;
        if !additive {
            self.selected_nodes.clear();
            self.selected_controls.clear();
            self.selected = None;
        }
        self.marquee = Some(Marquee {
            start: p,
            end: p,
            controls,
            original,
            primary,
            additive,
        });
    }
    pub fn update_marquee(&mut self, p: Point) {
        let Some(m) = self.marquee.as_mut() else {
            return;
        };
        m.end = p;
        let min = [m.start[0].min(p[0]), m.start[1].min(p[1])];
        let max = [m.start[0].max(p[0]), m.start[1].max(p[1])];
        let mut selected = if m.additive {
            m.original.clone()
        } else {
            BTreeSet::new()
        };
        let mut controls = if m.additive {
            m.controls.clone()
        } else {
            Default::default()
        };
        if (max[0] - min[0]).hypot(max[1] - min[1]) * self.viewport().scale() >= 3. {
            for (ci, c) in self.editor.document().glyphs[&self.glyph]
                .contours
                .iter()
                .enumerate()
            {
                for (point, q) in geometry::edit_points(c) {
                    if q[0] >= min[0] && q[0] <= max[0] && q[1] >= min[1] && q[1] <= max[1] {
                        match point {
                            EditPoint::Anchor(i) => {
                                selected.insert((ci, i));
                            }
                            _ => {
                                controls.insert((ci, point));
                            }
                        }
                    }
                }
            }
        }
        self.selected = selected.first().map(|&(ci, i)| (ci, EditPoint::Anchor(i)));
        self.selected_controls = controls;
        self.selected_nodes = selected;
        self.selected = self
            .selected
            .or_else(|| self.selected_controls.first().copied());
    }
    pub fn cancel_marquee(&mut self) {
        if let Some(m) = self.marquee.take() {
            self.selected_controls = m.controls;
            self.selected_nodes = m.original;
            self.selected = m.primary;
        }
    }
    pub fn marquee_bounds(&self) -> Option<[Point; 2]> {
        self.marquee.as_ref().map(|m| [m.start, m.end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action;
    use font_core::{
        workspace::{Tool, Workspace},
        Contour, Segment,
    };
    fn model() -> Model {
        let mut m = Model::new(Workspace::default());
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        let c = |x| Contour {
            identity: Default::default(),
            start: [x, 0.],
            segments: vec![
                Segment::Line {
                    end: [x + 100., 0.],
                },
                Segment::Line {
                    end: [x + 100., 100.],
                },
                Segment::Line { end: [x, 100.] },
                Segment::Line { end: [x, 0.] },
            ],
            closed: true,
            smooth: Default::default(),
        };
        g.contours = vec![c(0.), c(400.)];
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        m.tool = Tool::Nodes;
        m
    }
    fn pointer(m: &mut Model, phase: &str, p: Point, shift: bool) {
        let s = m.viewport().screen(p);
        m.pointer_input(phase, s[0], s[1], false, shift).unwrap();
    }
    #[test]
    fn rectangle_selection_in_both_directions_is_view_independent_and_history_free() {
        for reverse in [false, true] {
            for zoom in [0.2, 1., 8.] {
                let mut m = model();
                m.zoom = zoom;
                m.size = [1200., 1200.];
                m.pan = [40., -25.];
                let before = m.editor.document().clone();
                let revision = m.editor.revision();
                let pad = (20. / m.viewport().scale()).max(30.);
                let (a, b) = if reverse {
                    ([100. + pad, 100. + pad], [-pad, -pad])
                } else {
                    ([-pad, -pad], [100. + pad, 100. + pad])
                };
                pointer(&mut m, "down", a, false);
                assert!(m.has_draft());
                pointer(&mut m, "move", b, false);
                assert_eq!(
                    m.selected_nodes,
                    [(0, 0), (0, 1), (0, 2), (0, 3)].into_iter().collect()
                );
                assert!(m.marquee_bounds().is_some());
                pointer(&mut m, "up", b, false);
                assert!(!m.has_draft());
                assert!(m.marquee_bounds().is_none());
                assert_eq!(m.editor.document(), &before);
                assert_eq!(m.editor.revision(), revision);
            }
        }
    }
    #[test]
    fn additive_selection_cancel_and_empty_click_preserve_expected_selection() {
        let mut m = model();
        m.selected_nodes.insert((1, 0));
        m.selected = Some((1, EditPoint::Anchor(0)));
        let original = m.selected_nodes.clone();
        pointer(&mut m, "down", [-30., -30.], true);
        pointer(&mut m, "move", [130., 130.], true);
        assert_eq!(m.selected_nodes.len(), 5);
        pointer(&mut m, "cancel", [0., 0.], false);
        assert_eq!(m.selected_nodes, original);
        assert_eq!(m.selected, Some((1, EditPoint::Anchor(0))));
        pointer(&mut m, "down", [-30., -30.], true);
        pointer(&mut m, "move", [130., 130.], true);
        pointer(&mut m, "up", [130., 130.], true);
        assert_eq!(m.selected_nodes.len(), 5);
        pointer(&mut m, "down", [-100., -100.], false);
        m.action(Action::Cancel).unwrap();
        assert_eq!(m.selected_nodes.len(), 5);
        pointer(&mut m, "down", [-100., -100.], false);
        pointer(&mut m, "up", [-100., -100.], false);
        assert!(m.selected_nodes.is_empty());
    }
    #[test]
    fn rectangle_then_move_is_one_undo_and_modal_edits_are_blocked() {
        let mut m = model();
        let before = m.editor.document().clone();
        assert!(m.pointer_input("down", f64::MAX, 0., false, false).is_err());
        assert!(m.marquee.is_none());
        pointer(&mut m, "down", [-30., -30.], false);
        assert!(m.action(Action::SelectAllNodes).is_err());
        assert!(m.activate_document(99).is_err());
        assert!(m
            .image_context(m.epoch, m.editor.revision(), m.glyph)
            .is_err());
        assert!(m
            .snippet_action(crate::snippets::Action::Open, "panel-outline")
            .is_err());
        pointer(&mut m, "up", [130., 130.], false);
        m.action(Action::MoveSelected {
            dx: 20.,
            dy: 10.,
            revision: m.editor.revision(),
        })
        .unwrap();
        let changed = m.editor.document().clone();
        assert_eq!(changed.glyphs[&m.glyph].contours[0].start, [20., 10.]);
        assert_eq!(
            changed.glyphs[&m.glyph].contours[1],
            before.glyphs[&m.glyph].contours[1]
        );
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &changed);
    }
}
