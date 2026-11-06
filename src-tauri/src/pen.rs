//! Rust-owned uncommitted pen draft. One finished contour is one history entry.
use crate::model::Model;
use font_core::{Contour, GlyphStatus, Point, Segment};

#[derive(Default)]
pub struct Draft {
    pub handles: Vec<Point>,
    pub incoming: Vec<Point>,
    pub source: Option<(usize, Contour)>,
    pub pressed: Option<usize>,
    closing: bool,
    independent: bool,
}
fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn constrain(delta: Point, shift: bool) -> Point {
    if !shift {
        return delta;
    }
    let angle = (delta[1].atan2(delta[0]) / std::f64::consts::FRAC_PI_4).round()
        * std::f64::consts::FRAC_PI_4;
    let length = delta[0].hypot(delta[1]);
    [angle.cos() * length, angle.sin() * length]
}
impl Model {
    pub fn pen_contour(&self, closed: bool) -> Option<Contour> {
        let &start = self.pen.first()?;
        let handle = |i: usize| self.pen_edit.handles.get(i).copied().unwrap_or([0., 0.]);
        let incoming = |i: usize| {
            self.pen_edit
                .incoming
                .get(i)
                .copied()
                .unwrap_or_else(|| handle(i))
        };
        let mut segments = Vec::new();
        let count = self.pen.len();
        for i in 1..count + usize::from(closed && self.pen_edit.source.is_none()) {
            let prev = i - 1;
            let next = i % count;
            let end = self.pen[next];
            if handle(prev) == [0., 0.] && incoming(next) == [0., 0.] {
                if next != 0 {
                    segments.push(Segment::Line { end });
                }
            } else {
                segments.push(Segment::Cubic {
                    control1: add(self.pen[prev], handle(prev)),
                    control2: sub(end, incoming(next)),
                    end,
                });
            }
        }
        // Coupling applies only where both adjacent segments have handles.
        let mut contour = Contour {
            identity: Default::default(),
            start,
            segments,
            closed: closed && self.pen_edit.source.is_none(),
            smooth: Default::default(),
        };
        contour.smooth = (0..count)
            .filter(|&i| {
                handle(i) != [0., 0.]
                    && handle(i) == incoming(i)
                    && font_core::geometry::smooth_capable(&contour, i)
            })
            .collect();
        if let Some((_, base)) = &self.pen_edit.source {
            contour = if contour.segments.is_empty() {
                base.clone()
            } else {
                font_core::path_edit::join(base, false, &contour, false).ok()?
            };
            if closed {
                font_core::path_edit::set_closed(&mut contour, true).ok()?;
            }
        }
        Some(contour)
    }
    pub fn finish_pen(&mut self, closed: bool) -> Result<(), String> {
        if self.pen.len()
            < if closed && self.pen_edit.source.is_none() {
                3
            } else {
                2
            }
        {
            return Err(if closed {
                "need_three_points"
            } else {
                "need_two_points"
            }
            .into());
        }
        if self.pen_edit.pressed.is_some() {
            return Err("edit_in_progress".into());
        }
        let contour = self.pen_contour(closed).ok_or("InvalidGeometry")?;
        font_core::geometry::validate(&contour).map_err(|e| e.to_string())?;
        let mut glyph = self.editing_glyph().clone();
        if let Some((index, _)) = &self.pen_edit.source {
            glyph.contours[*index] = contour;
        } else {
            glyph.contours.push(contour);
        }
        glyph.status = GlyphStatus::Draft;
        self.commit_geometry(self.editor.revision(), glyph)?;
        self.pen.clear();
        self.pen_edit = Default::default();
        Ok(())
    }
    pub fn continue_path(
        &mut self,
        id: font_core::GlyphId,
        index: usize,
        anchor: usize,
        revision: u64,
    ) -> Result<(), String> {
        if self.has_draft() || id != self.glyph {
            return Err("edit_in_progress".into());
        }
        if revision != self.editor.revision() {
            return Err("Conflict".into());
        }
        let mut source = self
            .editing_glyph()
            .contours
            .get(index)
            .ok_or("InvalidGeometry")?
            .clone();
        if source.closed
            || source.segments.is_empty()
            || !(anchor == 0 || anchor == source.segments.len())
        {
            return Err("InvalidGeometry".into());
        }
        if source.segments.len() >= 65536 {
            return Err("too_many_points".into());
        }
        if anchor == 0 {
            font_core::path_edit::reverse(&mut source).map_err(|e| e.to_string())?;
        }
        let (end, tangent) = match *source.segments.last().unwrap() {
            Segment::Line { end } => (end, [0., 0.]),
            Segment::Quadratic { control, end } => (end, sub(end, control).map(|v| v * 2. / 3.)),
            Segment::Cubic { control2, end, .. } => (end, sub(end, control2)),
        };
        let tangent = if add(end, tangent)
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
        {
            tangent
        } else {
            [0., 0.]
        };
        self.pen = vec![end];
        self.pen_edit = Draft {
            source: Some((index, source)),
            handles: vec![tangent],
            incoming: vec![[0., 0.]],
            ..Default::default()
        };
        self.selected = None;
        self.selected_nodes.clear();
        self.selected_controls.clear();
        self.images = Default::default();
        self.snippets.contours.clear();
        self.proof.active = false;
        self.proof.generation += 1;
        self.tool = font_core::workspace::Tool::Pen;
        Ok(())
    }
    pub fn pen_pointer(&mut self, phase: &str, screen: Point, shift: bool) -> Result<(), String> {
        self.pen_pointer_modified(phase, screen, shift, false)
    }
    pub fn pen_pointer_modified(
        &mut self,
        phase: &str,
        screen: Point,
        shift: bool,
        alt: bool,
    ) -> Result<(), String> {
        if crate::updates::installing() {
            return Err("update_installing".into());
        }
        if !screen.iter().all(|v| v.is_finite()) {
            return Err("invalid_point".into());
        }
        let mut point = self.viewport().document(screen);
        if !point.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) {
            return Err("invalid_point".into());
        }
        match phase {
            "down" => {
                if self.pen_edit.pressed.is_some() && !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                if self.pen.is_empty() {
                    self.pen_edit = Default::default();
                }
                if self.pen.len() >= 2
                    && self.pen.len()
                        + self
                            .pen_edit
                            .source
                            .as_ref()
                            .map_or(0, |(_, c)| c.segments.len())
                        >= 3
                    && {
                        let start = self.viewport().screen(
                            self.pen_edit
                                .source
                                .as_ref()
                                .map_or(self.pen[0], |(_, c)| c.start),
                        );
                        (start[0] - screen[0]).hypot(start[1] - screen[1]) < 10.
                    }
                {
                    self.pen_edit.closing = true;
                    return Ok(());
                }
                if self.pen.len() >= 4096
                    || self
                        .pen_edit
                        .source
                        .as_ref()
                        .is_some_and(|(_, c)| c.segments.len() + self.pen.len() > 65536)
                {
                    return Err("too_many_points".into());
                }
                if let Some(&last) = self.pen.last() {
                    point = add(last, constrain(sub(point, last), shift));
                    if sub(point, last)[0].hypot(sub(point, last)[1]) * self.viewport().scale()
                        <= 3.
                    {
                        return Ok(());
                    }
                }
                if !point.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) {
                    return Err("invalid_point".into());
                }
                self.pen_edit.handles.resize(self.pen.len(), [0., 0.]);
                self.pen_edit.incoming.resize(self.pen.len(), [0., 0.]);
                self.pen_edit.independent = alt;
                self.pen_edit.pressed = Some(self.pen.len());
                self.pen.push(point);
                self.pen_edit.handles.push([0., 0.]);
                self.pen_edit.incoming.push([0., 0.]);
            }
            "move" => {
                if let Some(i) = self.pen_edit.pressed {
                    let delta = constrain(sub(point, self.pen[i]), shift);
                    if !add(self.pen[i], delta)
                        .iter()
                        .chain(sub(self.pen[i], delta).iter())
                        .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
                    {
                        return Err("invalid_point".into());
                    }
                    self.pen_edit.handles[i] =
                        if delta[0].hypot(delta[1]) * self.viewport().scale() > 3. {
                            delta
                        } else {
                            [0., 0.]
                        };
                    self.pen_edit.independent |= alt;
                    if !self.pen_edit.independent {
                        self.pen_edit.incoming[i] = self.pen_edit.handles[i];
                    }
                }
            }
            "up" => {
                self.pen_edit.pressed = None;
                if self.pen_edit.closing {
                    self.pen_edit.closing = false;
                    self.finish_pen(true)?;
                }
            }
            "cancel" => {
                if self.pen_edit.pressed.take().is_some() {
                    self.pen.pop();
                    self.pen_edit.handles.truncate(self.pen.len());
                    self.pen_edit.incoming.truncate(self.pen.len());
                }
                self.pen_edit.closing = false;
            }
            _ => return Err("invalid_pointer_phase".into()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action;
    use font_core::workspace::{Tool, Workspace};
    fn model() -> Model {
        let mut m = Model::new(Workspace::default());
        m.tool = Tool::Pen;
        m
    }
    fn point(m: &mut Model, at: Point, handle: Point, shift: bool) {
        let start = m.viewport().screen(at);
        let end = m.viewport().screen(add(at, handle));
        m.pen_pointer("down", start, shift).unwrap();
        m.pen_pointer("move", end, shift).unwrap();
        m.pen_pointer("up", end, shift).unwrap();
    }
    #[test]
    fn curves_commit_once_undo_redo_and_save_recovery_representation() {
        let mut m = model();
        let original = m.editor.document().clone();
        point(&mut m, [0., 0.], [80., 20.], false);
        point(&mut m, [300., 300.], [80., 0.], false);
        point(&mut m, [500., 0.], [0., 0.], false);
        assert_eq!(m.editor.revision(), 0);
        assert_ne!(m.visible_glyph(), m.editor.document().glyphs[&m.glyph]);
        m.action(Action::FinishPen).unwrap();
        let contour = m.editor.document().glyphs[&m.glyph]
            .contours
            .last()
            .unwrap();
        assert!(contour.closed);
        assert_eq!(
            contour.segments[0],
            Segment::Cubic {
                control1: [80., 20.],
                control2: [220., 300.],
                end: [300., 300.]
            }
        );
        let finished = m.editor.document().clone();
        let bytes = serde_json::to_vec(&finished).unwrap();
        let restored: font_core::FontDocument = serde_json::from_slice(&bytes).unwrap();
        restored.validate().unwrap();
        assert_eq!(finished, restored);
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &original);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &finished);
    }
    #[test]
    fn draft_cancel_backspace_shift_and_start_click() {
        let mut m = model();
        point(&mut m, [0., 0.], [0., 0.], false);
        point(&mut m, [300., 100.], [50., 45.], true);
        let at = m.pen[1];
        assert!(at[1].abs() < 1e-8);
        let handle = m.pen_edit.handles[1];
        let angle = handle[1].atan2(handle[0]) / std::f64::consts::FRAC_PI_4;
        assert!((angle - angle.round()).abs() < 1e-8);
        m.action(Action::BackPen).unwrap();
        assert_eq!(m.pen.len(), 1);
        let p = m.viewport().screen([300., 200.]);
        m.pen_pointer("down", p, false).unwrap();
        m.pen_pointer("cancel", p, false).unwrap();
        assert_eq!(m.pen.len(), 1);
        point(&mut m, [300., 200.], [0., 0.], false);
        point(&mut m, [500., 0.], [0., 0.], false);
        let p = m.viewport().screen([0., 0.]);
        m.pen_pointer("down", p, false).unwrap();
        m.pen_pointer("up", p, false).unwrap();
        assert!(m.pen.is_empty());
        assert_eq!(m.editor.revision(), 1);
        assert!(
            m.editor.document().glyphs[&m.glyph]
                .contours
                .last()
                .unwrap()
                .closed
        );
    }
    #[test]
    fn open_finish_path_operations_revision_guards_and_invalid_input() {
        let mut m = model();
        point(&mut m, [0., 0.], [0., 0.], false);
        point(&mut m, [300., 200.], [70., 10.], false);
        m.action(Action::FinishOpenPen).unwrap();
        let index = m.editor.document().glyphs[&m.glyph].contours.len() - 1;
        assert!(!m.editor.document().glyphs[&m.glyph].contours[index].closed);
        let before = m.editor.document().clone();
        assert!(m
            .action(Action::PathOperation {
                id: m.glyph,
                contour: index,
                segment: 0,
                operation: "reverse".into(),
                revision: 0
            })
            .is_err());
        assert_eq!(m.editor.document(), &before);
        for operation in ["reverse", "straighten", "toggle_closed"] {
            let previous = m.editor.document().clone();
            m.action(Action::PathOperation {
                id: m.glyph,
                contour: index,
                segment: 0,
                operation: operation.into(),
                revision: m.editor.revision(),
            })
            .unwrap();
            m.action(Action::Undo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document(), &previous);
            m.action(Action::Redo {
                revision: m.editor.revision(),
            })
            .unwrap();
        }
        assert!(m.pen_pointer("down", [f64::NAN, 0.], false).is_err());
    }
    fn open_model() -> Model {
        let mut m = model();
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        let c = Contour {
            identity: Default::default(),
            start: [0., 0.],
            segments: vec![
                Segment::Cubic {
                    control1: [30., 0.],
                    control2: [80., 100.],
                    end: [100., 100.],
                },
                Segment::Cubic {
                    control1: [130., 100.],
                    control2: [170., 0.],
                    end: [200., 0.],
                },
            ],
            closed: false,
            smooth: [1].into_iter().collect(),
        };
        let mut other = c.clone();
        other.start = [400., 0.];
        other.smooth.clear();
        g.contours = vec![c, other];
        m.editor.replace_glyph(m.editor.revision(), g).unwrap();
        m
    }
    #[test]
    fn continue_both_ends_replaces_source_and_cancel_preserves_original() {
        for anchor in [0, 2] {
            let mut m = open_model();
            let original = m.editor.document().clone();
            m.proof.active = true;
            m.action(Action::ContinuePath {
                id: m.glyph,
                contour: 0,
                anchor,
                revision: m.editor.revision(),
            })
            .unwrap();
            assert!(!m.proof.active);
            let base = m.pen_edit.source.as_ref().unwrap().1.clone();
            assert_eq!(m.visible_glyph().contours.len(), 2);
            assert_eq!(m.editor.document(), &original);
            point(&mut m, [700., 200.], [30., 10.], false);
            m.action(Action::FinishOpenPen).unwrap();
            let result = m.editor.document().clone();
            assert_eq!(result.glyphs[&m.glyph].contours.len(), 2);
            assert_eq!(
                &result.glyphs[&m.glyph].contours[0].segments[..base.segments.len()],
                &base.segments
            );
            m.action(Action::Undo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document(), &original);
            m.action(Action::Redo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document(), &result);
            m.action(Action::ContinuePath {
                id: m.glyph,
                contour: 0,
                anchor: 0,
                revision: m.editor.revision(),
            })
            .unwrap();
            m.action(Action::BackPen).unwrap();
            assert!(!m.has_draft());
            assert_eq!(m.editor.document(), &result);
        }
        let mut m = open_model();
        let original = m.editor.document().clone();
        m.action(Action::ContinuePath {
            id: m.glyph,
            contour: 0,
            anchor: 2,
            revision: m.editor.revision(),
        })
        .unwrap();
        point(&mut m, [700., 200.], [20., 10.], false);
        m.action(Action::Cancel).unwrap();
        assert_eq!(m.editor.document(), &original);
        assert!(m
            .action(Action::ContinuePath {
                id: m.glyph,
                contour: 0,
                anchor: 1,
                revision: m.editor.revision()
            })
            .is_err());
        assert!(m
            .action(Action::ContinuePath {
                id: m.glyph,
                contour: 0,
                anchor: 0,
                revision: 0
            })
            .is_err());
    }
    #[test]
    fn join_is_one_undo_and_same_contour_opposite_end_closes() {
        for (first, target) in [(0, 1), (1, 0)] {
            let mut m = open_model();
            let original = m.editor.document().clone();
            m.action(Action::JoinPaths {
                id: m.glyph,
                contour: first,
                anchor: 0,
                target,
                target_end: true,
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document().glyphs[&m.glyph].contours.len(), 1);
            m.action(Action::Undo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document(), &original);
            m.action(Action::Redo {
                revision: m.editor.revision(),
            })
            .unwrap();
            assert_eq!(m.editor.document().glyphs[&m.glyph].contours.len(), 1);
        }
        let mut m = open_model();
        let original = m.editor.document().clone();
        assert!(m
            .action(Action::JoinPaths {
                id: m.glyph,
                contour: 0,
                anchor: 1,
                target: 1,
                target_end: false,
                revision: m.editor.revision()
            })
            .is_err());
        assert!(m
            .action(Action::JoinPaths {
                id: m.glyph,
                contour: 0,
                anchor: 0,
                target: 1,
                target_end: false,
                revision: 0
            })
            .is_err());
        assert_eq!(m.editor.document(), &original);
        m.action(Action::JoinPaths {
            id: m.glyph,
            contour: 0,
            anchor: 0,
            target: 0,
            target_end: true,
            revision: m.editor.revision(),
        })
        .unwrap();
        assert!(m.editor.document().glyphs[&m.glyph].contours[0].closed);
    }
    #[test]
    fn alt_pen_adjusts_outgoing_without_touching_incoming() {
        let mut m = model();
        point(&mut m, [0., 0.], [0., 0.], false);
        let at = m.viewport().screen([300., 100.]);
        m.pen_pointer("down", at, false).unwrap();
        let p = m.viewport().screen([330., 120.]);
        m.pen_pointer("move", p, false).unwrap();
        let p = m.viewport().screen([370., 60.]);
        m.pen_pointer_modified("move", p, false, true).unwrap();
        m.pen_pointer("move", p, false).unwrap(); // Releasing Alt keeps this gesture independent.
        m.pen_pointer("up", p, false).unwrap();
        point(&mut m, [600., 0.], [0., 0.], false);
        m.action(Action::FinishOpenPen).unwrap();
        let c = m.editor.document().glyphs[&m.glyph]
            .contours
            .last()
            .unwrap();
        match c.segments[0] {
            Segment::Cubic { control2, .. } => assert_eq!(control2, [270., 80.]),
            _ => panic!(),
        }
        match c.segments[1] {
            Segment::Cubic { control1, .. } => assert_eq!(control1, [370., 60.]),
            _ => panic!(),
        }
        assert!(!c.smooth.contains(&1));
    }
    #[test]
    fn alt_node_drag_is_atomic_and_asymmetric_contours_survive_disk_recovery() {
        let mut m = open_model();
        m.tool = Tool::Nodes;
        let original = m.editor.document().clone();
        let from = m.viewport().screen([80., 100.]);
        let to = m.viewport().screen([70., 160.]);
        m.pointer_modified("down", from[0], from[1], true).unwrap();
        m.pointer_modified("move", to[0], to[1], true).unwrap();
        m.pointer("cancel", to[0], to[1]).unwrap();
        assert_eq!(m.editor.document(), &original);
        m.pointer_modified("down", from[0], from[1], true).unwrap();
        m.pointer_modified("move", to[0], to[1], true).unwrap();
        m.pointer("up", to[0], to[1]).unwrap();
        let final_doc = m.editor.document().clone();
        let c = &final_doc.glyphs[&m.glyph].contours[0];
        assert!(!c.smooth.contains(&1));
        match c.segments[1] {
            Segment::Cubic { control1, .. } => assert_eq!(control1, [130., 100.]),
            _ => panic!(),
        }
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &original);
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(m.editor.document(), &final_doc);
        m.action(Action::ContinuePath {
            id: m.glyph,
            contour: 0,
            anchor: 2,
            revision: m.editor.revision(),
        })
        .unwrap();
        point(&mut m, [700., -100.], [20., 10.], false);
        m.action(Action::FinishOpenPen).unwrap();
        m.action(Action::JoinPaths {
            id: m.glyph,
            contour: 0,
            anchor: 3,
            target: 1,
            target_end: false,
            revision: m.editor.revision(),
        })
        .unwrap();
        let final_doc = m.editor.document().clone();
        let root = std::env::temp_dir().join(format!(
            "fonts-path-join-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("project.json");
        font_core::storage::save(&path, &final_doc).unwrap();
        assert_eq!(font_core::storage::load(&path).unwrap(), final_doc);
        let store = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
        store
            .checkpoint(&final_doc, 1, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let scanner = crate::recovery_store::Store::new(root.join("recovery")).unwrap();
        let (list, _) = scanner.candidates().unwrap();
        assert_eq!(list.len(), 1);
        let (recovered, lease) = scanner.candidate(&list[0].id).unwrap();
        assert_eq!(recovered, final_doc);
        drop(lease);
        drop(scanner);
        std::fs::remove_dir_all(root).unwrap();
    }
}
