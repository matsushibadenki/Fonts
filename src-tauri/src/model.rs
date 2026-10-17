use font_core::{
    workspace::{Locale, Tool, Workspace},
    Contour, Editor, FontDocument, Glyph, GlyphId, GlyphStatus, Point, Segment,
};
use font_renderer::Viewport;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct Model {
    pub images: crate::reference_images::Images,
    pub editor: Editor,
    pub proof: crate::proof::Proof,
    pub epoch: u64,
    pub text_focus: std::collections::BTreeSet<String>,
    pub workspace: Workspace,
    pub glyph: GlyphId,
    pub tool: Tool,
    pub zoom: f64,
    pub pan: Point,
    pub nodes: bool,
    pub fill: bool,
    pub size: [f64; 2],
    pub drag: Option<Drag>,
    pub pen: Vec<Point>,
    pub selected: Option<(usize, font_core::geometry::EditPoint)>,
    pub path: Option<PathBuf>,
    pub saved_revision: Option<u64>,
}
pub struct Drag {
    pub original: Glyph,
    pub contour: usize,
    pub anchor: font_core::geometry::EditPoint,
    pub draft: Glyph,
    pub revision: u64,
}
#[derive(Serialize)]
pub struct GlyphSummary {
    id: GlyphId,
    character: String,
    codepoints: Vec<u32>,
    name: String,
    cid: Option<u32>,
    reviewed: bool,
    has_outline: bool,
}
#[derive(Serialize)]
pub struct ContourSummary {
    nodes: usize,
    closed: bool,
    segments: Vec<SegmentSummary>,
    start: Point,
}
#[derive(Serialize)]
pub struct SegmentSummary {
    end: Point,
    deletable: bool,
    closure: bool,
}
#[derive(Serialize)]
pub struct SelectionSummary {
    contour: usize,
    anchor: usize,
    smooth: bool,
    can_smooth: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    images: Vec<font_core::reference_images::ReferenceImage>,
    image_selected: Option<u64>,
    image_editing: bool,
    selection: Option<SelectionSummary>,
    proof: crate::proof::Summary,
    properties: font_core::FontProperties,
    presets: Vec<(&'static str, usize)>,
    family: String,
    epoch: u64,
    revision: u64,
    pub workspace: Workspace,
    glyph: GlyphId,
    character: String,
    codepoints: Vec<u32>,
    name: String,
    cid: Option<u32>,
    advance: f64,
    ascender: f64,
    descender: f64,
    upm: u16,
    unencoded: usize,
    cid_system: Option<font_core::CidSystem>,
    contours: Vec<ContourSummary>,
    glyphs: Vec<GlyphSummary>,
    tool: Tool,
    zoom: f64,
    nodes: bool,
    fill: bool,
    can_undo: bool,
    can_redo: bool,
    dirty: bool,
    reviewed: bool,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SelectNode {
        id: GlyphId,
        contour: usize,
        anchor: usize,
        revision: u64,
    },
    NodeMode {
        id: GlyphId,
        contour: usize,
        anchor: usize,
        smooth: bool,
        revision: u64,
    },
    SplitSegment {
        id: GlyphId,
        contour: usize,
        segment: usize,
        revision: u64,
    },
    DeleteSegmentEnd {
        id: GlyphId,
        contour: usize,
        segment: usize,
        revision: u64,
    },
    AssignUnicode {
        id: GlyphId,
        input: String,
        revision: u64,
    },
    FontProperties {
        properties: font_core::FontProperties,
        revision: u64,
    },
    AddPreset {
        preset: String,
        revision: u64,
    },
    AddCharacters {
        input: String,
        codepoints: bool,
        revision: u64,
    },
    SelectGlyph {
        id: GlyphId,
    },
    Tool {
        tool: Tool,
    },
    Zoom {
        value: f64,
    },
    Pan {
        dx: f64,
        dy: f64,
    },
    Locale {
        locale: Locale,
    },
    Nodes,
    Fill,
    Review {
        id: GlyphId,
        revision: u64,
    },
    Advance {
        value: f64,
        revision: u64,
        id: GlyphId,
    },
    Undo {
        revision: u64,
    },
    Redo {
        revision: u64,
    },
    FinishPen,
    Cancel,
    ResetView,
}
impl Model {
    pub const STARTER_CHARACTERS: &'static str =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz0123456789あいうえお中文";
    pub fn new(workspace: Workspace) -> Self {
        let mut doc =
            FontDocument::new("Untitled Sans", Self::STARTER_CHARACTERS).expect("valid starter");
        let polygon = |p: &[Point]| Contour {
            smooth: Default::default(),
            start: p[0],
            segments: p[1..].iter().map(|&end| Segment::Line { end }).collect(),
            closed: true,
        };
        let a = doc.glyphs.get_mut(&1).unwrap();
        a.advance = 640.;
        a.contours = vec![
            polygon(&[
                [30., 0.],
                [270., 720.],
                [370., 720.],
                [610., 0.],
                [505., 0.],
                [440., 200.],
                [200., 200.],
                [135., 0.],
            ]),
            polygon(&[[235., 300.], [405., 300.], [320., 565.]]),
        ];
        Self {
            images: Default::default(),
            editor: Editor::new(doc).unwrap(),
            epoch: 0,
            text_focus: Default::default(),
            workspace,
            glyph: 1,
            tool: Tool::Nodes,
            zoom: 1.,
            pan: [0., 0.],
            nodes: true,
            fill: false,
            size: [600., 600.],
            drag: None,
            pen: vec![],
            selected: None,
            proof: crate::proof::Proof::default(),
            path: None,
            saved_revision: None,
        }
    }
    pub fn replace_document(
        &mut self,
        document: FontDocument,
        path: Option<PathBuf>,
    ) -> Result<(), String> {
        let editor = Editor::new(document).map_err(|e| e.to_string())?;
        let epoch = self.epoch.checked_add(1).ok_or("document_epoch_overflow")?;
        let glyph = editor
            .document()
            .glyphs
            .keys()
            .copied()
            .find(|id| *id != 0)
            .unwrap_or(0);
        self.images = Default::default();
        self.proof = crate::proof::Proof::default();
        self.editor = editor;
        self.epoch = epoch;
        self.glyph = glyph;
        self.drag = None;
        self.selected = None;
        self.pen.clear();
        self.zoom = 1.;
        self.pan = [0., 0.];
        self.saved_revision = path.as_ref().map(|_| 0);
        self.path = path;
        Ok(())
    }
    pub fn mark_saved(&mut self, epoch: u64, revision: u64, path: PathBuf) {
        if self.epoch == epoch {
            self.path = Some(path);
            self.saved_revision = Some(revision);
        }
    }
    pub fn viewport(&self) -> Viewport {
        let doc = self.editor.document();
        Viewport {
            width: self.size[0],
            height: self.size[1],
            zoom: self.zoom,
            pan: self.pan,
            advance: doc.glyphs[&self.glyph].advance,
            ascender: doc.ascender,
            descender: doc.descender,
        }
    }
    pub fn visible_glyph(&self) -> Glyph {
        let mut g = self
            .drag
            .as_ref()
            .map(|d| d.draft.clone())
            .unwrap_or_else(|| self.editor.document().glyphs[&self.glyph].clone());
        if let Some(&start) = self.pen.first() {
            g.contours.push(Contour {
                smooth: Default::default(),
                start,
                segments: self.pen[1..]
                    .iter()
                    .map(|&end| Segment::Line { end })
                    .collect(),
                closed: false,
            });
        }
        g
    }
    fn selection_summary(&self) -> Option<SelectionSummary> {
        let (contour, point) = self.selected?;
        let g = self
            .drag
            .as_ref()
            .map(|d| &d.draft)
            .unwrap_or(&self.editor.document().glyphs[&self.glyph]);
        let c = g.contours.get(contour)?;
        use font_core::geometry::{canonical_anchor, EditPoint};
        let anchor = canonical_anchor(
            c,
            match point {
                EditPoint::Anchor(i) => i,
                EditPoint::Control1(i) => i,
                EditPoint::Control2(i) => i + 1,
            },
        );
        if anchor > c.segments.len() {
            return None;
        }
        Some(SelectionSummary {
            contour,
            anchor,
            smooth: c.smooth.contains(&anchor),
            can_smooth: c.closed || (anchor > 0 && anchor < c.segments.len()),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        let doc = self.editor.document();
        let g = &doc.glyphs[&self.glyph];
        Snapshot {
            images: self.visible_images(),
            image_selected: self.images.selected,
            image_editing: self.images.drag.is_some(),
            selection: self.selection_summary(),
            proof: self.proof.summary(self.size[0]),
            properties: self.editor.properties(),
            presets: font_core::unicode::preset_counts().clone(),
            family: doc.family.clone(),
            epoch: self.epoch,
            revision: self.editor.revision(),
            workspace: self.workspace.clone(),
            glyph: self.glyph,
            codepoints: g.codepoints.iter().map(|c| *c as u32).collect(),
            name: g.name.clone(),
            cid: g.cid,
            character: g
                .codepoints
                .iter()
                .next()
                .map(char::to_string)
                .unwrap_or_else(|| ".notdef".into()),
            advance: g.advance,
            ascender: doc.ascender,
            descender: doc.descender,
            upm: doc.units_per_em,
            unencoded: doc
                .glyphs
                .values()
                .filter(|g| g.id != 0 && g.codepoints.is_empty())
                .count(),
            cid_system: doc.metadata.cid_system.clone(),
            contours: g
                .contours
                .iter()
                .map(|c| ContourSummary {
                    nodes: font_core::geometry::edit_points(c)
                        .iter()
                        .filter(|(p, _)| matches!(p, font_core::geometry::EditPoint::Anchor(_)))
                        .count(),
                    start: c.start,
                    closed: c.closed,
                    segments: c
                        .segments
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let end = match *s {
                                Segment::Line { end }
                                | Segment::Quadratic { end, .. }
                                | Segment::Cubic { end, .. } => end,
                            };
                            SegmentSummary {
                                end,
                                deletable: font_core::geometry::can_delete_segment_end(c, i),
                                closure: c.closed && i + 1 == c.segments.len() && end == c.start,
                            }
                        })
                        .collect(),
                })
                .collect(),
            glyphs: doc
                .glyphs
                .values()
                .filter(|g| g.id != 0)
                .map(|g| GlyphSummary {
                    id: g.id,
                    codepoints: g.codepoints.iter().map(|c| *c as u32).collect(),
                    name: g.name.clone(),
                    cid: g.cid,
                    character: g
                        .codepoints
                        .iter()
                        .next()
                        .map(char::to_string)
                        .unwrap_or_default(),
                    reviewed: g.status == GlyphStatus::Reviewed,
                    has_outline: !g.contours.is_empty(),
                })
                .collect(),
            tool: self.tool,
            zoom: self.zoom,
            nodes: self.nodes,
            fill: self.fill,
            can_undo: self.editor.can_undo(),
            can_redo: self.editor.can_redo(),
            dirty: self.saved_revision != Some(self.editor.revision()),
            reviewed: g.status == GlyphStatus::Reviewed,
        }
    }
    pub fn action(&mut self, a: Action) -> Result<(), String> {
        // Modal edits cannot be interleaved with commands from a floating window.
        if self.drag.is_some() && !matches!(a, Action::Cancel) {
            return Err("edit_in_progress".into());
        }
        if self.images.drag.is_some()
            && !matches!(
                a,
                Action::Cancel
                    | Action::SelectGlyph { .. }
                    | Action::Tool { .. }
                    | Action::Undo { .. }
                    | Action::Redo { .. }
            )
        {
            return Err("edit_in_progress".into());
        }
        if matches!(
            a,
            Action::Cancel
                | Action::SelectGlyph { .. }
                | Action::Tool { .. }
                | Action::Undo { .. }
                | Action::Redo { .. }
                | Action::AddCharacters { .. }
                | Action::AddPreset { .. }
        ) {
            self.images = Default::default();
        }
        match a {
            Action::SelectNode {
                id,
                contour,
                anchor,
                revision,
            } => {
                if revision != self.editor.revision() {
                    return Err("Conflict".into());
                }
                if id != self.glyph || !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                let c = self.editor.document().glyphs[&id]
                    .contours
                    .get(contour)
                    .ok_or("InvalidGeometry")?;
                if anchor > c.segments.len() {
                    return Err("InvalidGeometry".into());
                }
                self.selected = Some((
                    contour,
                    font_core::geometry::EditPoint::Anchor(font_core::geometry::canonical_anchor(
                        c, anchor,
                    )),
                ));
            }
            Action::NodeMode {
                id,
                contour,
                anchor,
                smooth,
                revision,
            } => {
                if id != self.glyph || !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                let mut g = self.editor.document().glyphs[&id].clone();
                let c = g.contours.get_mut(contour).ok_or("InvalidGeometry")?;
                font_core::geometry::set_smooth(c, anchor, smooth).map_err(|e| e.to_string())?;
                let anchor = font_core::geometry::canonical_anchor(c, anchor);
                g.status = GlyphStatus::Draft;
                self.editor
                    .replace_glyph(revision, g)
                    .map_err(|e| e.to_string())?;
                self.selected = Some((contour, font_core::geometry::EditPoint::Anchor(anchor)));
            }

            Action::SplitSegment {
                id,
                contour,
                segment,
                revision,
            }
            | Action::DeleteSegmentEnd {
                id,
                contour,
                segment,
                revision,
            } => {
                if !self.pen.is_empty() || id != self.glyph {
                    return Err("edit_in_progress".into());
                }
                let split = matches!(a, Action::SplitSegment { .. });
                let mut g = self
                    .editor
                    .document()
                    .glyphs
                    .get(&id)
                    .ok_or("missing_glyph")?
                    .clone();
                let c = g.contours.get_mut(contour).ok_or("InvalidGeometry")?;
                if split {
                    font_core::geometry::split_segment(c, segment)
                } else {
                    font_core::geometry::delete_segment_end(c, segment)
                }
                .map_err(|e| e.to_string())?;
                g.status = GlyphStatus::Draft;
                self.editor
                    .replace_glyph(revision, g)
                    .map_err(|e| e.to_string())?;
                self.selected = Some((
                    contour,
                    font_core::geometry::EditPoint::Anchor(if split {
                        segment + 1
                    } else {
                        segment
                    }),
                ));
            }

            Action::AssignUnicode {
                id,
                input,
                revision,
            } => {
                if !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                let codes = if input.trim().is_empty() {
                    Default::default()
                } else {
                    font_core::unicode::parse(&input, true).map_err(|e| e.to_string())?
                };
                self.editor
                    .assign_unicode(revision, id, codes)
                    .map_err(|e| e.to_string())?;
            }
            Action::FontProperties {
                properties,
                revision,
            } => {
                if !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                self.editor
                    .set_properties(revision, properties)
                    .map_err(|e| e.to_string())?;
            }
            Action::AddPreset { preset, revision } => {
                if !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                let codes = font_core::unicode::preset(&preset).map_err(|e| e.to_string())?;
                let ids = self
                    .editor
                    .add_characters(revision, &codes)
                    .map_err(|e| e.to_string())?;
                if let Some(id) = ids.first() {
                    self.selected = None;
                    self.glyph = *id;
                }
            }

            Action::AddCharacters {
                input,
                codepoints,
                revision,
            } => {
                let characters =
                    font_core::unicode::parse(&input, codepoints).map_err(|e| e.to_string())?;
                let ids = self
                    .editor
                    .add_characters(revision, &characters)
                    .map_err(|e| e.to_string())?;
                if let Some(id) = ids.first() {
                    self.selected = None;
                    self.glyph = *id;
                    self.pen.clear();
                }
            }
            Action::SelectGlyph { id } => {
                if !self.editor.document().glyphs.contains_key(&id) {
                    return Err("missing_glyph".into());
                }
                self.glyph = id;
                self.selected = None;
                self.pen.clear();
            }
            Action::Tool { tool } => {
                self.selected = None;
                self.tool = tool;
                self.pen.clear();
            }
            Action::Zoom { value } => {
                if !value.is_finite() || !(0.2..=8.).contains(&value) {
                    return Err("invalid_zoom".into());
                }
                self.zoom = value;
            }
            Action::Pan { dx, dy } => {
                if !dx.is_finite() || !dy.is_finite() {
                    return Err("invalid_pan".into());
                }
                self.pan[0] = (self.pan[0] + dx).clamp(-10000., 10000.);
                self.pan[1] = (self.pan[1] + dy).clamp(-10000., 10000.);
            }
            Action::Locale { locale } => {
                self.workspace.revision =
                    self.workspace.revision.checked_add(1).ok_or("Conflict")?;
                self.workspace.locale = locale;
            }
            Action::Nodes => self.nodes = !self.nodes,
            Action::Fill => self.fill = !self.fill,
            Action::Review { id, revision } => {
                let mut g = self
                    .editor
                    .document()
                    .glyphs
                    .get(&id)
                    .ok_or("missing_glyph")?
                    .clone();
                g.status = if g.status == GlyphStatus::Reviewed {
                    GlyphStatus::Draft
                } else {
                    GlyphStatus::Reviewed
                };
                self.editor
                    .replace_glyph(revision, g)
                    .map_err(|e| e.to_string())?;
            }
            Action::Advance {
                value,
                revision,
                id,
            } => {
                let mut g = self
                    .editor
                    .document()
                    .glyphs
                    .get(&id)
                    .ok_or("missing_glyph")?
                    .clone();
                g.advance = value;
                self.editor
                    .replace_glyph(revision, g)
                    .map_err(|e| e.to_string())?;
            }
            Action::Undo { revision } => {
                self.selected = None;
                self.pen.clear();
                self.editor.undo(revision).map_err(|e| e.to_string())?;
                if !self.editor.document().glyphs.contains_key(&self.glyph) {
                    self.glyph = self
                        .editor
                        .document()
                        .glyphs
                        .keys()
                        .copied()
                        .find(|id| *id != 0)
                        .unwrap_or(0);
                }
            }
            Action::Redo { revision } => {
                self.selected = None;
                self.pen.clear();
                self.editor.redo(revision).map_err(|e| e.to_string())?;
            }
            Action::FinishPen => {
                if self.pen.len() < 3 {
                    return Err("need_three_points".into());
                }
                let mut g = self.editor.document().glyphs[&self.glyph].clone();
                g.contours.push(Contour {
                    smooth: Default::default(),
                    start: self.pen[0],
                    segments: self.pen[1..]
                        .iter()
                        .map(|&end| Segment::Line { end })
                        .collect(),
                    closed: true,
                });
                g.status = GlyphStatus::Draft;
                self.editor
                    .replace_glyph(self.editor.revision(), g)
                    .map_err(|e| e.to_string())?;
                self.pen.clear();
            }
            Action::Cancel => {
                self.drag = None;
                self.pen.clear();
            }
            Action::ResetView => {
                self.zoom = 1.;
                self.pan = [0., 0.];
            }
        }
        Ok(())
    }
    pub fn pointer(&mut self, phase: &str, x: f64, y: f64) -> Result<(), String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("invalid_point".into());
        }
        if self.proof.active {
            if phase == "down" {
                if let Some(font) = self.proof.font.as_ref() {
                    let width = if self.proof.reference.is_some() {
                        self.size[0] / 2.
                    } else {
                        self.size[0]
                    };
                    if x < width {
                        let layout = font_core::proof::layout(font, &self.proof.settings, width);
                        if let Some(id) = layout
                            .placements
                            .iter()
                            .find(|p| {
                                x >= p.x
                                    && x <= p.x + p.width
                                    && y + self.proof.settings.scroll >= p.top
                                    && y + self.proof.settings.scroll <= p.bottom
                            })
                            .and_then(|p| p.source)
                        {
                            self.selected = None;
                            self.images = Default::default();
                            self.glyph = id;
                            self.proof.active = false;
                            self.proof.generation += 1;
                        }
                    }
                }
            }
            return Ok(());
        }
        let p = self.viewport().document([x, y]);
        match phase {
            "down" if self.tool == Tool::Pen => {
                if self.pen.len() >= 4096 {
                    return Err("too_many_points".into());
                }
                if self
                    .pen
                    .last()
                    .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) * self.viewport().scale() > 3.)
                {
                    self.pen.push(p);
                }
            }
            "down" if self.tool == Tool::Nodes && self.nodes => {
                self.drag = None;
                let g = &self.editor.document().glyphs[&self.glyph];
                let v = self.viewport();
                let found = g
                    .contours
                    .iter()
                    .enumerate()
                    .flat_map(|(ci, c)| {
                        font_core::geometry::edit_points(c)
                            .into_iter()
                            .map(move |(point, p)| (ci, point, p))
                    })
                    .filter_map(|(ci, point, p)| {
                        let q = v.screen(p);
                        let distance = (q[0] - x).hypot(q[1] - y);
                        (distance < 10.).then_some((ci, point, distance))
                    })
                    .min_by(|a, b| a.2.total_cmp(&b.2));
                self.selected = found.map(|(ci, point, _)| (ci, point));
                if let Some((contour, anchor, _)) = found {
                    self.drag = Some(Drag {
                        original: g.clone(),
                        draft: g.clone(),
                        contour,
                        anchor,
                        revision: self.editor.revision(),
                    });
                }
            }
            "move" => {
                if let Some(d) = self.drag.as_mut() {
                    let c = &mut d.draft.contours[d.contour];
                    font_core::geometry::move_point(c, d.anchor, p).map_err(|e| e.to_string())?;
                }
            }
            "up" => {
                if let Some(mut d) = self.drag.take() {
                    if d.original != d.draft {
                        d.draft.status = GlyphStatus::Draft;
                        self.editor
                            .replace_glyph(d.revision, d.draft)
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            "cancel" => self.drag = None,
            "down" => {}
            _ => return Err("invalid_pointer_phase".into()),
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_is_shared_with_canvas_and_modes_are_undoable() {
        let mut m = Model::new(Workspace::default());
        let p = m.viewport().screen([270., 720.]);
        m.pointer("down", p[0], p[1]).unwrap();
        m.pointer("up", p[0], p[1]).unwrap();
        let selection = m.snapshot().selection.unwrap();
        assert_eq!(selection.contour, 0);
        assert_eq!(selection.anchor, 1);
        m.action(Action::NodeMode {
            id: 1,
            contour: 0,
            anchor: 1,
            smooth: true,
            revision: 0,
        })
        .unwrap();
        let smooth = m.editor.document().clone();
        assert!(m.snapshot().selection.unwrap().smooth);
        assert!(m
            .action(Action::NodeMode {
                id: 1,
                contour: 0,
                anchor: 1,
                smooth: false,
                revision: 0
            })
            .is_err());
        assert_eq!(m.editor.document(), &smooth);
        m.action(Action::SelectNode {
            id: 1,
            contour: 0,
            anchor: 2,
            revision: 1,
        })
        .unwrap();
        assert_eq!(m.snapshot().selection.unwrap().anchor, 2);
        assert_eq!(m.editor.revision(), 1);
        m.action(Action::Undo { revision: 1 }).unwrap();
        assert!(m.snapshot().selection.is_none());
        m.action(Action::Redo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document(), &smooth);
        m.action(Action::SelectGlyph { id: 2 }).unwrap();
        assert!(m.selected.is_none());
    }
    #[test]
    fn topology_edits_are_revision_guarded_undoable_and_exportable() {
        let mut m = Model::new(Workspace::default());
        let original = m.editor.document().clone();
        m.action(Action::SplitSegment {
            id: 1,
            contour: 0,
            segment: 0,
            revision: 0,
        })
        .unwrap();
        assert_eq!(m.editor.document().glyphs[&1].contours[0].segments.len(), 8);
        let split = m.editor.document().clone();
        assert!(m
            .action(Action::DeleteSegmentEnd {
                id: 1,
                contour: 0,
                segment: 0,
                revision: 0
            })
            .is_err());
        assert_eq!(m.editor.document(), &split);
        let json = serde_json::to_vec(m.editor.document()).unwrap();
        let restored: FontDocument = serde_json::from_slice(&json).unwrap();
        assert_eq!(restored, split);
        let bytes = font_core::font_io::export(&restored).unwrap();
        let imported = font_core::font_io::import(&bytes, 0).unwrap();
        assert!(imported
            .glyphs
            .values()
            .any(|g| g.codepoints.contains(&'A') && !g.contours.is_empty()));
        m.action(Action::DeleteSegmentEnd {
            id: 1,
            contour: 0,
            segment: 0,
            revision: 1,
        })
        .unwrap();
        assert_eq!(m.editor.document(), &original);
        m.action(Action::Undo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document(), &split);
        m.action(Action::Redo { revision: 3 }).unwrap();
        assert_eq!(m.editor.document(), &original);
        m.pen.push([0., 0.]);
        assert!(m
            .action(Action::SplitSegment {
                id: 1,
                contour: 0,
                segment: 0,
                revision: 4
            })
            .is_err());
        m.pen.clear();
        let p = m.viewport().screen([30., 0.]);
        m.pointer("down", p[0], p[1]).unwrap();
        assert!(m
            .action(Action::SplitSegment {
                id: 1,
                contour: 0,
                segment: 0,
                revision: 4
            })
            .is_err());
        assert_eq!(m.editor.document(), &original);
    }
    #[test]
    fn proof_click_returns_to_source_glyph_and_document_switch_clears_reference() {
        let mut m = Model::new(Workspace::default());
        let settings = font_core::proof::Settings {
            text: "A".into(),
            ..Default::default()
        };
        let font = font_core::proof::prepare(
            font_core::proof::subset(m.editor.document(), &settings.text).unwrap(),
            &settings.text,
        )
        .unwrap();
        let layout = font_core::proof::layout(&font, &settings, m.size[0]);
        m.proof.font = Some(std::sync::Arc::new(font));
        m.proof.reference = m.proof.font.clone();
        m.proof.settings = settings;
        m.proof.active = true;
        let p = &layout.placements[0];
        m.pointer("down", p.x + 2., (p.top + p.bottom) / 2.)
            .unwrap();
        assert!(!m.proof.active);
        assert_eq!(m.glyph, 1);
        assert_eq!(m.editor.revision(), 0);
        m.replace_document(FontDocument::new("Other", "中").unwrap(), None)
            .unwrap();
        assert!(m.proof.font.is_none());
        assert!(m.proof.reference.is_none());
    }
    #[test]
    fn curve_handle_drag_is_one_transaction_cancel_and_undo_are_lossless() {
        let mut m = Model::new(Workspace::default());
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        g.contours = vec![Contour {
            smooth: Default::default(),
            start: [0., 0.],
            segments: vec![Segment::Cubic {
                control1: [100., 600.],
                control2: [500., 600.],
                end: [600., 0.],
            }],
            closed: true,
        }];
        m.editor.replace_glyph(0, g.clone()).unwrap();
        let start = m.viewport().screen([100., 600.]);
        let finish = m.viewport().screen([150., 650.]);
        m.pointer("down", start[0], start[1]).unwrap();
        assert!(matches!(
            m.drag.as_ref().unwrap().anchor,
            font_core::geometry::EditPoint::Control1(0)
        ));
        m.pointer("move", finish[0], finish[1]).unwrap();
        m.pointer("cancel", finish[0], finish[1]).unwrap();
        assert_eq!(m.editor.document().glyphs[&m.glyph], g);
        m.pointer("down", start[0], start[1]).unwrap();
        for _ in 0..5 {
            m.pointer("move", finish[0], finish[1]).unwrap();
        }
        m.pointer("up", finish[0], finish[1]).unwrap();
        assert_eq!(m.editor.revision(), 2);
        let edited = m.editor.document().glyphs[&m.glyph].clone();
        assert_ne!(edited, g);
        m.editor.undo(2).unwrap();
        assert_eq!(m.editor.document().glyphs[&m.glyph], g);
        m.editor.redo(3).unwrap();
        assert_eq!(m.editor.document().glyphs[&m.glyph], edited);
        m.nodes = false;
        m.pointer("down", start[0], start[1]).unwrap();
        assert!(m.drag.is_none());
    }
    #[test]
    fn unicode_addition_undo_repairs_selection_and_preserves_mappings() {
        let mut m = Model::new(Workspace::default());
        let count = m.editor.document().glyphs.len();
        m.action(Action::AddCharacters {
            input: "U+1F600 U+20000 0301".into(),
            codepoints: true,
            revision: 0,
        })
        .unwrap();
        assert_eq!(m.editor.document().glyphs.len(), count + 3);
        assert_eq!(m.snapshot().codepoints, vec![0x301]);
        m.action(Action::Undo { revision: 1 }).unwrap();
        assert!(m.editor.document().glyphs.contains_key(&m.glyph));
        m.snapshot();
        m.action(Action::Redo { revision: 2 }).unwrap();
        assert!(m
            .snapshot()
            .glyphs
            .iter()
            .any(|g| g.codepoints == vec![0x20000]));
        let before = m.editor.document().clone();
        assert!(m
            .action(Action::AddCharacters {
                input: "D800".into(),
                codepoints: true,
                revision: 3
            })
            .is_err());
        assert_eq!(m.editor.document(), &before);
    }
    #[test]
    fn opening_a_document_resets_edit_history_but_preserves_workspace() {
        let mut m = Model::new(Workspace::default());
        m.workspace.locale = Locale::ZhCn;
        m.pen.push([1., 2.]);
        m.zoom = 2.;
        let doc = FontDocument::new("Opened", "").unwrap();
        m.replace_document(doc.clone(), Some(PathBuf::from("opened.fontproj.json")))
            .unwrap();
        assert_eq!(m.editor.document(), &doc);
        assert_eq!(m.glyph, 0);
        assert_eq!(m.epoch, 1);
        assert_eq!(m.workspace.locale, Locale::ZhCn);
        assert!(m.pen.is_empty());
        assert_eq!(m.zoom, 1.);
        assert!(!m.editor.can_undo());
        assert_eq!(m.saved_revision, Some(0));
    }
    #[test]
    fn blank_document_can_add_characters_and_undo_back_to_empty() {
        let mut m = Model::new(Workspace::default());
        m.selected = Some((0, font_core::geometry::EditPoint::Anchor(1)));
        m.replace_document(FontDocument::new("Untitled Sans", "").unwrap(), None)
            .unwrap();
        assert!(m.snapshot().glyphs.is_empty());
        assert!(m.snapshot().contours.is_empty());
        assert!(m.selected.is_none());
        assert!(m.path.is_none());
        assert!(!m.editor.can_undo());
        m.action(Action::AddCharacters {
            input: "あ中".into(),
            codepoints: false,
            revision: 0,
        })
        .unwrap();
        assert_eq!(m.snapshot().glyphs.len(), 2);
        m.action(Action::Undo { revision: 1 }).unwrap();
        assert_eq!(m.glyph, 0);
        assert!(m.snapshot().glyphs.is_empty());
        m.action(Action::Redo { revision: 2 }).unwrap();
        assert_eq!(m.snapshot().glyphs.len(), 2);
    }
    #[test]
    fn stale_save_cannot_mark_a_replacement_document_as_saved() {
        let mut m = Model::new(Workspace::default());
        let epoch = m.epoch;
        m.replace_document(FontDocument::new("New", "B").unwrap(), None)
            .unwrap();
        m.mark_saved(epoch, 0, PathBuf::from("old.fontproj.json"));
        assert!(m.path.is_none());
        assert!(m.saved_revision.is_none());
        m.mark_saved(m.epoch, 0, PathBuf::from("new.fontproj.json"));
        assert_eq!(m.saved_revision, Some(0));
    }
    #[test]
    fn invalid_open_keeps_existing_document_and_draft() {
        let mut m = Model::new(Workspace::default());
        let before = m.editor.document().clone();
        m.pen.push([42., 15.]);
        let mut invalid = before.clone();
        invalid.units_per_em = 0;
        assert!(m.replace_document(invalid, None).is_err());
        assert_eq!(m.editor.document(), &before);
        assert_eq!(m.pen, vec![[42., 15.]]);
        assert_eq!(m.epoch, 0);
    }
    #[test]
    fn drag_is_one_undo_step_and_draft_never_leaks_to_document() {
        let mut m = Model::new(Workspace::default());
        let before = m.editor.document().clone();
        let p = m.viewport().screen([30., 0.]);
        m.pointer("down", p[0], p[1]).unwrap();
        m.pointer("move", p[0] + 20., p[1] - 20.).unwrap();
        assert_eq!(m.editor.document(), &before);
        assert_ne!(m.visible_glyph(), before.glyphs[&1]);
        m.pointer("up", p[0] + 20., p[1] - 20.).unwrap();
        assert_eq!(m.editor.revision(), 1);
        m.action(Action::Undo { revision: 1 }).unwrap();
        assert_eq!(m.editor.document(), &before);
    }
    #[test]
    fn pen_commits_closed_contour_and_cancel_discards_it() {
        let mut m = Model::new(Workspace::default());
        m.action(Action::Tool { tool: Tool::Pen }).unwrap();
        for p in [[200., 200.], [300., 200.], [250., 300.]] {
            m.pointer("down", p[0], p[1]).unwrap();
        }
        assert_eq!(m.editor.document().glyphs[&1].contours.len(), 2);
        m.action(Action::FinishPen).unwrap();
        assert_eq!(m.editor.document().glyphs[&1].contours.len(), 3);
        assert!(m.editor.document().glyphs[&1].contours[2].closed);
    }
}
