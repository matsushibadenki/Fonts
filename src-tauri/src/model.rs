use font_core::{
    workspace::{Locale, Tool, Workspace},
    Contour, Editor, FontDocument, Glyph, GlyphId, GlyphStatus, Point, Segment,
};
use font_renderer::Viewport;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct Model {
    pub component_edit: Option<crate::component_edit::Editing>,
    pub parked: std::collections::BTreeMap<u64, Box<Model>>,
    pub document_id: u64,
    pub next_document_id: u64,
    pub snippets: crate::snippets::State,
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
    pub marquee: Option<crate::node_marquee::Marquee>,
    pub boolean_b: (u64, GlyphId, u64, std::collections::BTreeSet<usize>),
    pub drag: Option<Drag>,
    pub pen: Vec<Point>,
    pub pen_edit: crate::pen::Draft,
    pub selected_controls: font_core::point_transform::Selection,
    pub selected_nodes: std::collections::BTreeSet<(usize, usize)>,
    pub selected: Option<(usize, font_core::geometry::EditPoint)>,
    pub path: Option<PathBuf>,
    pub saved_revision: Option<u64>,
}
pub struct Drag {
    pub controls: font_core::point_transform::Selection,
    pub points: std::collections::BTreeSet<(usize, usize)>,
    pub origin: Point,
    pub position: Point,
    pub independent: bool,
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
    linked: bool,
    uuid: Option<font_core::identity::Uuid>,
    start_id: Option<font_core::identity::Uuid>,
    nodes: usize,
    closed: bool,
    segments: Vec<SegmentSummary>,
    start: Point,
}
#[derive(Serialize)]
pub struct SegmentSummary {
    node_id: Option<font_core::identity::Uuid>,
    end: Point,
    deletable: bool,
    closure: bool,
}
#[derive(Serialize)]
pub struct SelectionSummary {
    node_id: Option<font_core::identity::Uuid>,
    contour: usize,
    anchor: usize,
    smooth: bool,
    can_smooth: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    source_font: Option<font_core::source_font::Summary>,
    imported_layout: font_core::imported_layout::Summary,
    components: crate::components::Summary,
    document_uuid: font_core::identity::Uuid,
    documents: Vec<crate::documents::Summary>,
    document_id: u64,
    class_kerning: font_core::class_kerning::ClassKerning,
    kerning: Vec<font_core::kerning::Pair>,
    kerning_total: usize,
    snippets: crate::snippets::Summary,
    images: Vec<font_core::reference_images::ReferenceImage>,
    image_selected: Option<u64>,
    image_editing: bool,
    selection: Option<SelectionSummary>,
    pen_points: usize,
    pen_continuing: bool,
    selected_controls: Vec<(usize, font_core::geometry::EditPoint)>,
    selected_nodes: Vec<(usize, usize)>,
    boolean_right: Vec<usize>,
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
    SourceVariation {
        revision: u64,
        operation: font_core::source_font::variable::Edit,
    },
    SourcePalette {
        revision: u64,
        palette: u16,
        entry: Option<u16>,
        color: Option<[u8; 4]>,
    },
    ReleaseImportedLayout {
        revision: u64,
        positioning_only: bool,
    },
    ClassKerning {
        operation: font_core::class_kerning::Edit,
        revision: u64,
    },
    KerningException {
        left: GlyphId,
        right: GlyphId,
        revision: u64,
    },
    Kerning {
        left: GlyphId,
        right: GlyphId,
        value: i16,
        revision: u64,
    },
    SelectStableNode {
        reference: font_core::identity::NodeRef,
        revision: u64,
    },
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
    FinishOpenPen,
    SelectAllNodes,
    ClearNodes,
    SetBooleanB,
    ClearBooleanB,
    MoveSelected {
        dx: f64,
        dy: f64,
        revision: u64,
    },
    TransformSelected {
        scale: Point,
        angle: f64,
        revision: u64,
    },
    SelectAllControls,
    SplitPath {
        id: GlyphId,
        contour: usize,
        anchor: usize,
        revision: u64,
    },
    ContinuePath {
        id: GlyphId,
        contour: usize,
        anchor: usize,
        revision: u64,
    },
    JoinPaths {
        id: GlyphId,
        contour: usize,
        anchor: usize,
        target: usize,
        target_end: bool,
        revision: u64,
    },
    BackPen,
    PathOperation {
        id: GlyphId,
        contour: usize,
        segment: usize,
        operation: String,
        revision: u64,
    },
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
            identity: Default::default(),
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
            component_edit: None,
            parked: Default::default(),
            document_id: 1,
            next_document_id: 2,
            snippets: Default::default(),
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
            marquee: None,
            boolean_b: Default::default(),
            drag: None,
            pen: vec![],
            pen_edit: Default::default(),
            selected: None,
            selected_controls: Default::default(),
            selected_nodes: Default::default(),
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
        self.snippets.clear_transient();
        self.images = Default::default();
        self.proof = crate::proof::Proof::default();
        self.component_edit = None;
        self.editor = editor;
        self.epoch = epoch;
        self.glyph = glyph;
        self.drag = None;
        self.selected = None;
        self.selected_nodes.clear();
        self.selected_controls.clear();
        self.marquee = None;
        self.pen.clear();
        self.pen_edit = Default::default();
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
            advance: self.editing_glyph().advance,
            ascender: self
                .component_edit
                .as_ref()
                .map_or(doc.ascender, |e| e.ascender),
            descender: self
                .component_edit
                .as_ref()
                .map_or(doc.descender, |e| e.descender),
        }
    }
    pub fn point_selection(&self) -> font_core::point_transform::Selection {
        self.selected_nodes
            .iter()
            .map(|&(ci, i)| (ci, font_core::geometry::EditPoint::Anchor(i)))
            .chain(self.selected_controls.iter().copied())
            .collect()
    }
    pub fn visible_glyph(&self) -> Glyph {
        let mut g = self
            .drag
            .as_ref()
            .map(|d| d.draft.clone())
            .unwrap_or_else(|| self.editing_glyph().clone());
        if let Some(d) = &self.snippets.draft {
            if let Some((_, Some(id))) = d.reference {
                if let Some(i) = self.editor.document().components.instances.get(&id) {
                    let ids: std::collections::BTreeSet<_> = i.nodes.values().collect();
                    g.contours
                        .retain(|c| !c.identity.contours.first().is_some_and(|v| ids.contains(v)));
                }
            }
            if let Ok(contours) = d.source.transformed(&d.placement) {
                g.contours.extend(contours);
            }
        }
        if let Some(contour) = self.pen_contour(false) {
            if let Some((index, _)) = &self.pen_edit.source {
                g.contours[*index] = contour;
            } else {
                g.contours.push(contour);
            }
        }
        g
    }
    fn selection_summary(&self) -> Option<SelectionSummary> {
        let (contour, point) = self.selected?;
        let g = self
            .drag
            .as_ref()
            .map(|d| &d.draft)
            .unwrap_or(self.editing_glyph());
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
            node_id: font_core::identity::node_id(c, point),
            contour,
            anchor,
            smooth: c.smooth.contains(&anchor),
            can_smooth: c.closed || (anchor > 0 && anchor < c.segments.len()),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        let doc = self.editor.document();
        let g = self.editing_glyph();
        let linked = self.locked_contours();
        Snapshot {
            source_font: doc.source_font.as_ref().map(|s| s.summary()),
            imported_layout: doc.imported_layout.summary(),
            components: self.component_summary(),
            document_uuid: doc.document_uuid,
            pen_points: self.pen.len(),
            pen_continuing: self.pen_edit.source.is_some(),
            selected_controls: self.selected_controls.iter().copied().collect(),
            selected_nodes: self.selected_nodes.iter().copied().collect(),
            boolean_right: self.boolean_operand_b(),
            documents: self.document_summaries(),
            document_id: self.document_id,
            class_kerning: doc.class_kerning.clone(),
            kerning: doc
                .kerning
                .0
                .iter()
                .filter(|p| p.left == self.glyph)
                .copied()
                .collect(),
            kerning_total: doc.kerning.0.len(),
            snippets: self.snippet_summary(),
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
                .unwrap_or_else(|| {
                    if self.editing_id().is_some() {
                        g.name.clone()
                    } else {
                        ".notdef".into()
                    }
                }),
            advance: g.advance,
            ascender: self
                .component_edit
                .as_ref()
                .map_or(doc.ascender, |e| e.ascender),
            descender: self
                .component_edit
                .as_ref()
                .map_or(doc.descender, |e| e.descender),
            upm: self.editing_upm(),
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
                    linked: c
                        .identity
                        .contours
                        .first()
                        .is_some_and(|&id| linked.contains(&id)),
                    uuid: c.identity.contours.first().copied(),
                    start_id: font_core::identity::node_id(
                        c,
                        font_core::geometry::EditPoint::Anchor(0),
                    ),
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
                                node_id: font_core::identity::node_id(
                                    c,
                                    font_core::geometry::EditPoint::Anchor(i + 1),
                                ),
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
            dirty: self.unsaved(),
            reviewed: g.status == GlyphStatus::Reviewed,
        }
    }
    pub fn action(&mut self, a: Action) -> Result<(), String> {
        if self.editor.document().source_font.is_some() && matches!(&a, Action::Tool { .. }) {
            return Err("SourceFontLocked".into());
        }
        if self.component_edit.is_some()
            && (matches!(
                &a,
                Action::Review { .. }
                    | Action::Advance { .. }
                    | Action::AssignUnicode { .. }
                    | Action::AddPreset { .. }
                    | Action::AddCharacters { .. }
                    | Action::ClassKerning { .. }
                    | Action::Kerning { .. }
                    | Action::KerningException { .. }
            ) || matches!(&a, Action::Tool { tool: Tool::Images }))
        {
            return Err("component_edit_unavailable".into());
        }

        if crate::updates::installing() {
            return Err("update_installing".into());
        }
        if self.snippets.draft.is_some() {
            if matches!(
                a,
                Action::Cancel
                    | Action::SelectGlyph { .. }
                    | Action::Tool { .. }
                    | Action::Undo { .. }
                    | Action::Redo { .. }
            ) {
                self.snippets.draft = None;
                if self.tool == Tool::Snippets {
                    self.tool = Tool::Nodes;
                }
            } else {
                return Err("edit_in_progress".into());
            }
        }
        if matches!(
            a,
            Action::SelectGlyph { .. }
                | Action::Undo { .. }
                | Action::Redo { .. }
                | Action::AddCharacters { .. }
                | Action::AddPreset { .. }
                | Action::SplitSegment { .. }
                | Action::DeleteSegmentEnd { .. }
        ) {
            self.snippets.contours.clear();
        }
        // Modal edits cannot be interleaved with commands from a floating window.
        if (self.drag.is_some() || self.marquee.is_some()) && !matches!(a, Action::Cancel) {
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
            Action::SourceVariation {
                revision,
                operation,
            } => {
                self.editor
                    .edit_source_variations(revision, operation)
                    .map_err(|e| {
                        if e == font_core::Error::InvalidGeometry {
                            "variation_range".into()
                        } else {
                            e.to_string()
                        }
                    })?;
            }
            Action::SourcePalette {
                revision,
                palette,
                entry,
                color,
            } => {
                self.editor
                    .edit_source_palette(revision, palette, entry, color)
                    .map_err(|e| e.to_string())?;
            }
            Action::ReleaseImportedLayout {
                revision,
                positioning_only,
            } => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                self.editor
                    .release_imported_layout(revision, positioning_only)
                    .map_err(|e| e.to_string())?;
            }
            Action::ClassKerning {
                operation,
                revision,
            } => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                self.editor
                    .edit_class_kerning(revision, operation)
                    .map_err(|e| e.to_string())?;
            }
            Action::KerningException {
                left,
                right,
                revision,
            } => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                self.editor
                    .zero_kerning_exception(revision, left, right)
                    .map_err(|e| e.to_string())?;
            }
            Action::Kerning {
                left,
                right,
                value,
                revision,
            } => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                self.editor
                    .set_kerning(revision, left, right, value)
                    .map_err(|e| e.to_string())?;
            }
            Action::SelectStableNode {
                reference,
                revision,
            } => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                if revision != self.editor.revision() {
                    return Err("Conflict".into());
                }
                let doc = self.editor.document();
                let address = if reference.definition.is_some() {
                    font_core::identity::resolve_definition_reference(doc, self.glyph, reference)
                } else if self.component_edit.is_some() {
                    None
                } else {
                    font_core::identity::resolve_in_glyph(doc, self.glyph, reference)
                        .or_else(|| font_core::identity::resolve(doc, reference))
                }
                .ok_or("node_reference_missing")?;
                if let Some(id) = reference.definition {
                    if self.editing_id() != Some(id) {
                        self.enter_component_edit(id)?;
                    }
                }
                if address.glyph != self.glyph {
                    self.images = Default::default();
                    self.snippets.clear_transient();
                    self.proof.active = false;
                }
                self.glyph = address.glyph;
                self.selected = Some((address.contour, address.point));
                self.selected_nodes.clear();
                self.selected_controls.clear();
                if let font_core::geometry::EditPoint::Anchor(i) = address.point {
                    self.selected_nodes.insert((address.contour, i));
                } else {
                    self.selected_controls
                        .insert((address.contour, address.point));
                }
            }
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
                let geometry = self.editing_glyph().clone();
                let c = geometry.contours.get(contour).ok_or("InvalidGeometry")?;
                if anchor > c.segments.len() {
                    return Err("InvalidGeometry".into());
                }
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.selected_nodes
                    .insert((contour, font_core::geometry::canonical_anchor(c, anchor)));
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
                let mut g = self.editing_glyph().clone();
                let c = g.contours.get_mut(contour).ok_or("InvalidGeometry")?;
                font_core::geometry::set_smooth(c, anchor, smooth).map_err(|e| e.to_string())?;
                let anchor = font_core::geometry::canonical_anchor(c, anchor);
                g.status = GlyphStatus::Draft;
                self.commit_geometry(revision, g)?;
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
                let mut g = self.editing_glyph().clone();
                let c = g.contours.get_mut(contour).ok_or("InvalidGeometry")?;
                if split {
                    font_core::geometry::split_segment(c, segment)
                } else {
                    font_core::geometry::delete_segment_end(c, segment)
                }
                .map_err(|e| e.to_string())?;
                g.status = GlyphStatus::Draft;
                self.commit_geometry(revision, g)?;
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
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
                    self.selected_nodes.clear();
                    self.selected_controls.clear();
                    self.marquee = None;
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
                    self.selected_nodes.clear();
                    self.selected_controls.clear();
                    self.marquee = None;
                    self.glyph = *id;
                    self.pen.clear();
                    self.pen_edit = Default::default();
                }
            }
            Action::SelectGlyph { id } => {
                self.exit_component_edit()?;
                if !self.editor.document().glyphs.contains_key(&id) {
                    return Err("missing_glyph".into());
                }
                self.glyph = id;
                self.selected = None;
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.pen.clear();
                self.pen_edit = Default::default();
            }
            Action::Tool { tool } => {
                self.selected = None;
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.tool = tool;
                self.pen.clear();
                self.pen_edit = Default::default();
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
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.pen.clear();
                self.pen_edit = Default::default();
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
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.pen.clear();
                self.pen_edit = Default::default();
                self.editor.redo(revision).map_err(|e| e.to_string())?;
            }
            Action::SetBooleanB => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                let selected = &self.snippets.contours;
                if selected.is_empty()
                    || selected.len() > 1024
                    || selected.iter().any(|&i| {
                        self.editing_glyph()
                            .contours
                            .get(i)
                            .is_none_or(|c| !c.closed)
                    })
                {
                    return Err("boolean_geometry".into());
                }
                self.boolean_b = (
                    self.epoch,
                    self.glyph,
                    self.editor.revision(),
                    selected.clone(),
                );
                self.snippets.contours.clear();
            }
            Action::ClearBooleanB => self.boolean_b.3.clear(),
            Action::SelectAllNodes => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                self.tool = Tool::Nodes;
                self.nodes = true;
                self.proof.active = false;
                self.proof.generation += 1;
                self.selected_controls.clear();
                self.selected_nodes = self
                    .editing_glyph()
                    .contours
                    .iter()
                    .enumerate()
                    .flat_map(|(ci, c)| {
                        font_core::geometry::edit_points(c)
                            .into_iter()
                            .filter_map(move |(p, _)| {
                                if let font_core::geometry::EditPoint::Anchor(i) = p {
                                    Some((ci, i))
                                } else {
                                    None
                                }
                            })
                    })
                    .collect();
                self.selected = None;
            }
            Action::ClearNodes => {
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.selected = None;
            }
            Action::MoveSelected { dx, dy, revision } => {
                if revision != self.editor.revision() {
                    return Err("Conflict".into());
                }
                if dx == 0. && dy == 0. {
                    return Ok(());
                }
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                let mut glyph = self.editing_glyph().clone();
                font_core::point_transform::transform(
                    &mut glyph.contours,
                    &self.point_selection(),
                    [1., 1.],
                    0.,
                    [dx, dy],
                )
                .map_err(|e| e.to_string())?;
                glyph.status = GlyphStatus::Draft;
                self.commit_geometry(revision, glyph)?;
            }
            Action::SelectAllControls => {
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                self.tool = Tool::Nodes;
                self.nodes = true;
                self.proof.active = false;
                self.proof.generation += 1;
                self.selected_nodes.clear();
                self.selected = None;
                self.selected_controls = self
                    .editing_glyph()
                    .contours
                    .iter()
                    .enumerate()
                    .flat_map(|(ci, c)| {
                        font_core::geometry::edit_points(c)
                            .into_iter()
                            .filter_map(move |(p, _)| {
                                (!matches!(p, font_core::geometry::EditPoint::Anchor(_)))
                                    .then_some((ci, p))
                            })
                    })
                    .collect();
            }
            Action::TransformSelected {
                scale,
                angle,
                revision,
            } => {
                if revision != self.editor.revision() {
                    return Err("Conflict".into());
                }
                if self.has_draft() {
                    return Err("edit_in_progress".into());
                }
                if scale == [1., 1.] && angle == 0. {
                    return Ok(());
                }
                let mut glyph = self.editing_glyph().clone();
                font_core::point_transform::transform(
                    &mut glyph.contours,
                    &self.point_selection(),
                    scale,
                    angle,
                    [0., 0.],
                )
                .map_err(|e| e.to_string())?;
                glyph.status = GlyphStatus::Draft;
                self.commit_geometry(revision, glyph)?;
            }
            Action::SplitPath {
                id,
                contour,
                anchor,
                revision,
            } => {
                if self.has_draft() || id != self.glyph {
                    return Err("edit_in_progress".into());
                }
                let mut glyph = self.editing_glyph().clone();
                let parts = font_core::path_edit::split(
                    glyph.contours.get(contour).ok_or("InvalidGeometry")?,
                    anchor,
                )
                .map_err(|e| e.to_string())?;
                glyph.contours.splice(contour..=contour, parts);
                glyph.status = GlyphStatus::Draft;
                self.commit_geometry(revision, glyph)?;
                self.selected = None;
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.snippets.contours.clear();
            }
            Action::ContinuePath {
                id,
                contour,
                anchor,
                revision,
            } => self.continue_path(id, contour, anchor, revision)?,
            Action::JoinPaths {
                id,
                contour,
                anchor,
                target,
                target_end,
                revision,
            } => {
                if self.has_draft() || id != self.glyph {
                    return Err("edit_in_progress".into());
                }
                if revision != self.editor.revision() {
                    return Err("Conflict".into());
                }
                let mut glyph = self.editing_glyph().clone();
                let first = glyph.contours.get(contour).ok_or("InvalidGeometry")?;
                if first.closed
                    || first.segments.is_empty()
                    || !(anchor == 0 || anchor == first.segments.len())
                {
                    return Err("InvalidGeometry".into());
                }
                if contour == target {
                    if (anchor == 0) == !target_end {
                        return Err("InvalidGeometry".into());
                    }
                    font_core::path_edit::set_closed(&mut glyph.contours[contour], true)
                        .map_err(|e| e.to_string())?;
                } else {
                    let second = glyph.contours.get(target).ok_or("InvalidGeometry")?;
                    let joined = font_core::path_edit::join(first, anchor == 0, second, target_end)
                        .map_err(|e| e.to_string())?;
                    glyph.contours[contour] = joined;
                    glyph.contours.remove(target);
                }
                glyph.status = GlyphStatus::Draft;
                self.commit_geometry(revision, glyph)?;
                self.selected = None;
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.snippets.contours.clear();
            }
            Action::FinishPen => self.finish_pen(true)?,
            Action::FinishOpenPen => self.finish_pen(false)?,
            Action::BackPen => {
                if self.pen.len() == 1 && self.pen_edit.source.is_some() {
                    self.pen.clear();
                    self.pen_edit = Default::default();
                } else {
                    self.pen.pop();
                    self.pen_edit.handles.truncate(self.pen.len());
                    self.pen_edit.incoming.truncate(self.pen.len());
                }
                self.pen_edit.pressed = None;
            }
            Action::PathOperation {
                id,
                contour,
                segment,
                operation,
                revision,
            } => {
                if id != self.glyph || !self.pen.is_empty() {
                    return Err("edit_in_progress".into());
                }
                let mut g = self.editing_glyph().clone();
                let c = g.contours.get_mut(contour).ok_or("InvalidGeometry")?;
                match operation.as_str() {
                    "reverse" => font_core::path_edit::reverse(c),
                    "toggle_closed" => font_core::path_edit::set_closed(c, !c.closed),
                    "straighten" => font_core::path_edit::straighten(c, segment),
                    _ => return Err("InvalidGeometry".into()),
                }
                .map_err(|e| e.to_string())?;
                g.status = GlyphStatus::Draft;
                self.commit_geometry(revision, g)?;
                self.selected = None;
                self.selected_nodes.clear();
                self.selected_controls.clear();
                self.marquee = None;
                self.snippets.contours.clear();
            }
            Action::Cancel => {
                self.drag = None;
                self.pen.clear();
                self.pen_edit = Default::default();
                self.cancel_marquee();
            }
            Action::ResetView => {
                if self.component_edit.is_some() {
                    self.fit_component_view();
                } else {
                    self.zoom = 1.;
                    self.pan = [0., 0.];
                }
            }
        }
        self.refresh_component_edit();
        Ok(())
    }
    pub fn pointer(&mut self, phase: &str, x: f64, y: f64) -> Result<(), String> {
        self.pointer_modified(phase, x, y, false)
    }
    pub fn pointer_modified(
        &mut self,
        phase: &str,
        x: f64,
        y: f64,
        alt: bool,
    ) -> Result<(), String> {
        self.pointer_input(phase, x, y, alt, false)
    }
    pub fn pointer_input(
        &mut self,
        phase: &str,
        x: f64,
        y: f64,
        alt: bool,
        shift: bool,
    ) -> Result<(), String> {
        if crate::updates::installing() {
            return Err("update_installing".into());
        }
        if !x.is_finite() || !y.is_finite() {
            return Err("invalid_point".into());
        }
        if self.editor.document().source_font.is_some() {
            return Ok(());
        }
        if self.proof.active {
            if self.proof.source.is_some() || self.proof.color.is_some() {
                return Ok(());
            }
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
                                x >= p.left
                                    && x <= p.left + p.width
                                    && y + self.proof.settings.scroll >= p.top
                                    && y + self.proof.settings.scroll <= p.bottom
                            })
                            .and_then(|p| p.source)
                        {
                            self.selected = None;
                            self.selected_nodes.clear();
                            self.selected_controls.clear();
                            self.marquee = None;
                            self.snippets.clear_transient();
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
        if self.tool == Tool::Pen {
            return self.pen_pointer(phase, [x, y], false);
        }
        let p = self.viewport().document([x, y]);
        let valid_point = p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.);
        if matches!(phase, "down" | "move") && !valid_point {
            return Err("invalid_point".into());
        }
        match phase {
            "down" if self.tool == Tool::Nodes && self.nodes => {
                self.drag = None;
                let g = self.editing_glyph().clone();
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
                if found.is_none() {
                    self.begin_marquee(p, shift);
                    return Ok(());
                }
                if let Some((ci, point, _)) = found {
                    let contained = match point {
                        font_core::geometry::EditPoint::Anchor(i) => {
                            self.selected_nodes.contains(&(ci, i))
                        }
                        _ => self.selected_controls.contains(&(ci, point)),
                    };
                    if !shift && !contained {
                        self.selected_nodes.clear();
                        self.selected_controls.clear();
                    }
                    let keep = !shift || !contained;
                    match point {
                        font_core::geometry::EditPoint::Anchor(i) => {
                            if keep {
                                self.selected_nodes.insert((ci, i));
                            } else {
                                self.selected_nodes.remove(&(ci, i));
                            }
                        }
                        _ => {
                            if keep {
                                self.selected_controls.insert((ci, point));
                            } else {
                                self.selected_controls.remove(&(ci, point));
                            }
                        }
                    }
                    if !keep {
                        self.selected = self.point_selection().first().copied();
                        return Ok(());
                    }
                }
                self.selected = found.map(|(ci, point, _)| (ci, point));
                if let Some((contour, anchor, _)) = found {
                    let locked = self.locked_contours();
                    if self.point_selection().iter().any(|&(ci, _)| {
                        g.contours[ci]
                            .identity
                            .contours
                            .first()
                            .is_some_and(|&id| locked.contains(&id))
                    }) {
                        return Ok(());
                    }
                    let origin = p;
                    let position =
                        font_core::geometry::point_position(&g.contours[contour], anchor)
                            .ok_or("InvalidGeometry")?;
                    self.drag = Some(Drag {
                        controls: self.selected_controls.clone(),
                        points: self.selected_nodes.clone(),
                        origin,
                        position,
                        independent: alt,
                        original: g.clone(),
                        draft: g.clone(),
                        contour,
                        anchor,
                        revision: self.editor.revision(),
                    });
                }
            }
            "move" => {
                if self.marquee.is_some() {
                    self.update_marquee(p);
                    return Ok(());
                }
                if let Some(d) = self.drag.as_mut() {
                    if d.points.len() + d.controls.len() > 1 {
                        let mut contours = d.original.contours.clone();
                        font_core::point_transform::transform(
                            &mut contours,
                            &d.points
                                .iter()
                                .map(|&(ci, i)| (ci, font_core::geometry::EditPoint::Anchor(i)))
                                .chain(d.controls.iter().copied())
                                .collect(),
                            [1., 1.],
                            0.,
                            [p[0] - d.origin[0], p[1] - d.origin[1]],
                        )
                        .map_err(|e| e.to_string())?;
                        d.draft.contours = contours;
                        return Ok(());
                    }
                    let p = [
                        d.position[0] + p[0] - d.origin[0],
                        d.position[1] + p[1] - d.origin[1],
                    ];
                    let c = &mut d.draft.contours[d.contour];
                    d.independent |= alt;
                    if d.independent {
                        font_core::path_edit::move_independent(c, d.anchor, p)
                    } else {
                        font_core::geometry::move_point(c, d.anchor, p)
                    }
                    .map_err(|e| e.to_string())?;
                }
            }
            "up" => {
                if self.marquee.is_some() {
                    if valid_point {
                        self.update_marquee(p);
                    }
                    self.marquee = None;
                    return Ok(());
                }
                if let Some(mut d) = self.drag.take() {
                    if d.original != d.draft {
                        d.draft.status = GlyphStatus::Draft;
                        self.commit_geometry(d.revision, d.draft)?;
                    }
                }
            }
            "cancel" => {
                self.drag = None;
                self.cancel_marquee();
            }
            "down" => {}
            _ => return Err("invalid_pointer_phase".into()),
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn color_preview_is_isolated_from_the_editor_and_serialized_pixels() {
        let mut m = Model::new(Workspace::default());
        let original = m.editor.document().clone();
        let preview = font_core::color_fonts::preview(
            include_bytes!("../../crates/font-core/tests/data/color/colrv0.ttf"),
            0,
            "A",
            96,
            0,
            [1024, 512],
        )
        .unwrap();
        m.proof.color = Some(std::sync::Arc::new(preview));
        m.proof.active = true;
        m.pointer("down", 100., 100.).unwrap();
        assert!(m.proof.active);
        assert_eq!(m.editor.document(), &original);
        assert_eq!(m.editor.revision(), 0);
        let summary = serde_json::to_value(m.snapshot()).unwrap();
        assert_eq!(summary["proof"]["color"], true);
        assert!(summary["proof"]["source"]
            .as_str()
            .unwrap()
            .contains("COLRv0"));
        assert!(summary["proof"].get("image").is_none());
        assert!(summary["proof"].get("rgba").is_none());
    }

    #[test]
    fn external_variable_preview_never_selects_an_editor_glyph() {
        let mut m = Model::new(Workspace::default());
        let before = m.editor.document().clone();
        let settings = font_core::proof::Settings::default();
        let prepared = font_core::proof::prepare(
            font_core::proof::subset(&before, &settings.text).unwrap(),
            &settings.text,
        )
        .unwrap();
        let layout = font_core::proof::layout(&prepared, &settings, m.size[0]);
        let p = layout.placements.first().unwrap();
        m.proof.font = Some(std::sync::Arc::new(prepared));
        m.proof.source = Some(std::sync::Arc::new(before.clone()));
        m.proof.active = true;
        m.pointer("down", p.x + p.width / 2., (p.top + p.bottom) / 2.)
            .unwrap();
        assert!(m.proof.active);
        assert_eq!(m.editor.document(), &before);
        assert_eq!(m.editor.revision(), 0);
    }

    #[test]
    fn class_commands_share_revision_and_modal_guards() {
        use font_core::class_kerning::{Edit, Side};
        let mut m = super::Model::new(Default::default());
        let op = || Edit::Upsert {
            side: Side::Left,
            id: None,
            name: "Left".into(),
            glyphs: [1].into(),
        };
        assert!(m
            .action(super::Action::ClassKerning {
                operation: op(),
                revision: 1
            })
            .is_err());
        m.pen.push([0., 0.]);
        assert!(m
            .action(super::Action::ClassKerning {
                operation: op(),
                revision: 0
            })
            .is_err());
        m.pen.clear();
        m.action(super::Action::ClassKerning {
            operation: op(),
            revision: 0,
        })
        .unwrap();
        assert_eq!(m.editor.document().class_kerning.left.len(), 1);
        m.action(super::Action::Undo { revision: 1 }).unwrap();
        assert!(m.editor.document().class_kerning.is_empty());
        m.action(super::Action::Redo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document().class_kerning.left[0].name, "Left");
    }
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
    fn imported_shaping_recovery_and_vertical_ivs_click_keep_source_glyph() {
        let bytes = include_bytes!("../../crates/font-core/tests/data/layout/shaping.ttf");
        let mut m = Model::new(Workspace::default());
        m.replace_document(font_core::font_io::import(bytes, 0).unwrap(), None)
            .unwrap();
        let before = m.editor.document().clone();
        m.editor
            .release_imported_positioning(m.editor.revision())
            .unwrap();
        m.editor.undo(m.editor.revision()).unwrap();
        assert_eq!(m.editor.document(), &before);
        let root =
            std::env::temp_dir().join(format!("fonts-layout-recovery-{}", before.document_uuid));
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        store
            .checkpoint(&before, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, before);
        m.replace_document(doc, None).unwrap();
        let settings = font_core::proof::Settings {
            text: "字\u{e0100}".into(),
            direction: font_core::proof::Direction::Vertical,
            ..Default::default()
        };
        let font = font_core::proof::prepare(
            font_core::proof::subset(m.editor.document(), &settings.text).unwrap(),
            &settings.text,
        )
        .unwrap();
        let layout = font_core::proof::layout(&font, &settings, m.size[0]);
        let p = &layout.placements[0];
        let expected = before
            .glyphs
            .values()
            .find(|g| g.name == "ideograph.alt")
            .unwrap()
            .id;
        assert_eq!(p.source, Some(expected));
        m.proof.font = Some(std::sync::Arc::new(font));
        m.proof.settings = settings;
        m.proof.active = true;
        m.pointer("down", p.left + p.width / 2., (p.top + p.bottom) / 2.)
            .unwrap();
        assert_eq!(m.glyph, expected);
        assert!(!m.proof.active);
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn curve_handle_drag_is_one_transaction_cancel_and_undo_are_lossless() {
        let mut m = Model::new(Workspace::default());
        let mut g = m.editor.document().glyphs[&m.glyph].clone();
        g.contours = vec![Contour {
            identity: Default::default(),
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
        let g = m.editor.document().glyphs[&m.glyph].clone();
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
            m.pointer("up", p[0], p[1]).unwrap();
        }
        assert_eq!(m.editor.document().glyphs[&1].contours.len(), 2);
        m.action(Action::FinishPen).unwrap();
        assert_eq!(m.editor.document().glyphs[&1].contours.len(), 3);
        assert!(m.editor.document().glyphs[&1].contours[2].closed);
    }
}
