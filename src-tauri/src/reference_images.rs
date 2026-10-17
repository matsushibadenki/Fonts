//! Shared image selection/gestures and bounded worker import. No pixels cross IPC.
use crate::{model::Model, AppState};
use font_core::{
    reference_images::{ImageAsset, ReferenceImage},
    reference_transform::{self, Handle},
    workspace::{Locale, Tool},
    GlyphId, Point,
};
use serde::Deserialize;
use tauri::{Manager, WebviewWindow};

#[derive(Default)]
pub struct Images {
    pub selected: Option<u64>,
    pub drag: Option<Drag>,
}
pub struct Drag {
    original: ReferenceImage,
    pub draft: ReferenceImage,
    start: Point,
    handle: Option<Handle>,
    revision: u64,
    glyph: GlyphId,
    token: String,
    pub owner: String,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Select { id: Option<u64> },
    Update { image: ReferenceImage },
    Remove { id: u64 },
    Reorder { id: u64, index: usize },
    OpacityBegin { id: u64, token: String },
    OpacityMove { token: String, value: f64 },
    OpacityEnd { token: String },
    Cancel { token: String },
}
impl Model {
    pub fn has_draft(&self) -> bool {
        self.drag.is_some() || self.images.drag.is_some() || !self.pen.is_empty()
    }
    pub fn visible_images(&self) -> Vec<ReferenceImage> {
        self.editor
            .document()
            .reference_images
            .for_glyph(self.glyph)
            .iter()
            .map(|image| {
                self.images
                    .drag
                    .as_ref()
                    .filter(|d| d.glyph == self.glyph && d.draft.id == image.id)
                    .map(|d| d.draft.clone())
                    .unwrap_or_else(|| image.clone())
            })
            .collect()
    }
    fn image(&self, id: u64) -> Result<ReferenceImage, String> {
        self.editor
            .document()
            .reference_images
            .for_glyph(self.glyph)
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| "InvalidDocument".into())
    }
    pub fn image_context(&self, epoch: u64, revision: u64, glyph: GlyphId) -> Result<(), String> {
        if self.epoch != epoch || self.editor.revision() != revision || self.glyph != glyph {
            return Err("Conflict".into());
        }
        if self.proof.active || self.drag.is_some() || !self.pen.is_empty() {
            return Err("edit_in_progress".into());
        }
        Ok(())
    }
    pub fn import_image(
        &mut self,
        epoch: u64,
        revision: u64,
        glyph: GlyphId,
        asset: ImageAsset,
    ) -> Result<(), String> {
        self.image_context(epoch, revision, glyph)?;
        if self.images.drag.is_some() {
            return Err("edit_in_progress".into());
        }
        let id = self
            .editor
            .place_reference_image(revision, glyph, asset)
            .map_err(|e| e.to_string())?;
        self.images.selected = Some(id);
        self.selected = None;
        self.tool = Tool::Images;
        use font_core::workspace::{Dock, Panel, Placement};
        if let Some(Placement::Docked { dock }) =
            self.workspace.panels.get(&Panel::Outline).copied()
        {
            self.workspace
                .activate(self.workspace.revision, Panel::Outline)
                .map_err(|e| e.to_string())?;
            if dock == Dock::Bottom && self.workspace.bottom_height < 250. {
                self.workspace
                    .resize(self.workspace.revision, Dock::Bottom, 250.)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    fn image_drag(&mut self, token: &str, owner: &str) -> Result<&mut Drag, String> {
        self.images
            .drag
            .as_mut()
            .filter(|d| d.token == token && d.owner == owner)
            .ok_or_else(|| "Conflict".into())
    }
    fn finish_image_drag(&mut self, token: &str, owner: &str) -> Result<(), String> {
        self.image_drag(token, owner)?;
        let drag = self.images.drag.take().unwrap();
        self.editor
            .update_reference_image(drag.revision, drag.glyph, drag.draft)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn image_action(&mut self, action: Action, owner: &str) -> Result<(), String> {
        if self.images.drag.is_some()
            && !matches!(
                action,
                Action::OpacityMove { .. } | Action::OpacityEnd { .. } | Action::Cancel { .. }
            )
        {
            return Err("edit_in_progress".into());
        }
        let revision = self.editor.revision();
        match action {
            Action::Select { id } => {
                if let Some(id) = id {
                    self.image(id)?;
                }
                self.images.selected = id;
                self.selected = None;
                self.tool = Tool::Images;
            }
            Action::Update { image } => {
                self.editor
                    .update_reference_image(revision, self.glyph, image)
                    .map_err(|e| e.to_string())?;
            }
            Action::Remove { id } => {
                self.editor
                    .remove_reference_image(revision, self.glyph, id)
                    .map_err(|e| e.to_string())?;
                if self.images.selected == Some(id) {
                    self.images.selected = None;
                }
            }
            Action::Reorder { id, index } => {
                self.editor
                    .reorder_reference_image(revision, self.glyph, id, index)
                    .map_err(|e| e.to_string())?;
            }
            Action::OpacityBegin { id, token } => {
                if token.is_empty() || token.len() > 100 {
                    return Err("Conflict".into());
                }
                let image = self.image(id)?;
                self.images.selected = Some(id);
                self.images.drag = Some(Drag {
                    original: image.clone(),
                    draft: image,
                    start: [0., 0.],
                    handle: None,
                    revision,
                    glyph: self.glyph,
                    token,
                    owner: owner.into(),
                });
            }
            Action::OpacityMove { token, value } => {
                let drag = self.image_drag(&token, owner)?;
                if drag.handle.is_some() {
                    return Err("Conflict".into());
                }
                let mut draft = drag.draft.clone();
                draft.opacity = value;
                draft.validate().map_err(|e| e.to_string())?;
                drag.draft = draft;
            }
            Action::OpacityEnd { token } => {
                if self.image_drag(&token, owner)?.handle.is_some() {
                    return Err("Conflict".into());
                }
                self.finish_image_drag(&token, owner)?;
            }
            Action::Cancel { token } => {
                self.image_drag(&token, owner)?;
                self.images.drag = None;
            }
        }
        Ok(())
    }
    pub fn image_pointer(
        &mut self,
        phase: &str,
        point: Point,
        shift: bool,
        token: &str,
    ) -> Result<(), String> {
        if self.tool != Tool::Images || token.is_empty() || token.len() > 100 {
            return Err("Conflict".into());
        }
        if point.iter().any(|v| !v.is_finite()) {
            return Err("invalid_point".into());
        }
        let viewport = self.viewport();
        let p = viewport.document(point);
        match phase {
            "down" => {
                if self.images.drag.is_some() {
                    return Err("edit_in_progress".into());
                }
                let selected = self
                    .images
                    .selected
                    .and_then(|id| self.image(id).ok())
                    .filter(|p| p.visible && !p.locked);
                let hit_handle = selected.as_ref().and_then(|image| {
                    reference_transform::handles(image, viewport.scale())
                        .into_iter()
                        .find(|(_, p)| {
                            let q = viewport.screen(*p);
                            (q[0] - point[0]).hypot(q[1] - point[1]) <= 8.
                        })
                        .map(|(handle, _)| (image.clone(), handle))
                });
                let hit = hit_handle.or_else(|| {
                    self.editor
                        .document()
                        .reference_images
                        .for_glyph(self.glyph)
                        .iter()
                        .rev()
                        .find(|i| i.hit_test(p))
                        .map(|i| (i.clone(), Handle::Move))
                });
                self.images.selected = hit.as_ref().map(|(image, _)| image.id);
                if let Some((image, handle)) = hit {
                    self.images.drag = Some(Drag {
                        original: image.clone(),
                        draft: image,
                        start: p,
                        handle: Some(handle),
                        revision: self.editor.revision(),
                        glyph: self.glyph,
                        token: token.into(),
                        owner: "main".into(),
                    });
                }
            }
            "move" => {
                if self.images.drag.is_none() {
                    return Ok(());
                }
                let d = self.image_drag(token, "main")?;
                d.draft = reference_transform::transform(
                    &d.original,
                    d.handle.ok_or("Conflict")?,
                    d.start,
                    p,
                    shift,
                )
                .map_err(|e| e.to_string())?;
            }
            "up" => {
                if self.images.drag.is_some() {
                    self.finish_image_drag(token, "main")?;
                }
            }
            "cancel" => {
                if self.images.drag.is_some() {
                    self.image_drag(token, "main")?;
                    self.images.drag = None;
                }
            }
            _ => return Err("invalid_pointer_phase".into()),
        }
        Ok(())
    }
}
#[tauri::command]
pub async fn reference_import(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    glyph: GlyphId,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let app = window.app_handle().clone();
    let locale = {
        let state = app.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        m.image_context(epoch, revision, glyph)?;
        if m.has_draft() {
            return Err("edit_in_progress".into());
        }
        m.workspace.locale
    };
    let asset = tauri::async_runtime::spawn_blocking(move || {
        let title = match locale {
            Locale::En => "Place reference image",
            Locale::Ja => "アタリ画像を配置",
            Locale::ZhCn => "放置参考图像",
        };
        let path = rfd::FileDialog::new()
            .set_parent(&window)
            .set_title(title)
            .add_filter("PNG / JPEG", &["png", "jpg", "jpeg"])
            .pick_file();
        path.map(|p| {
            ImageAsset::load(&p).map_err(|e| match e {
                font_core::Error::TooLarge => "image_too_large".to_string(),
                _ => "image_invalid".to_string(),
            })
        })
        .transpose()
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some(asset) = asset {
        let state = app.state::<AppState>();
        state
            .0
            .lock()
            .map_err(|_| "state_poisoned")?
            .import_image(epoch, revision, glyph, asset)
            .map_err(|e| {
                if e == "TooLarge" {
                    "image_too_large".into()
                } else {
                    e
                }
            })?;
        crate::publish(&app);
    }
    Ok(())
}
#[tauri::command]
pub fn reference_action(
    window: WebviewWindow,
    state: tauri::State<AppState>,
    epoch: u64,
    revision: u64,
    glyph: GlyphId,
    action: Action,
) -> Result<(), String> {
    crate::allowed(&window)?;
    {
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        m.image_context(epoch, revision, glyph)?;
        m.image_action(action, window.label())?;
    }
    crate::publish(window.app_handle());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action as Edit;
    use font_core::workspace::Workspace;
    fn asset() -> ImageAsset {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255, 0, 0, 255, 0, 255, 0, 255])
                .unwrap();
        }
        ImageAsset::from_bytes(bytes).unwrap()
    }
    fn model() -> Model {
        let mut m = Model::new(Workspace::default());
        m.import_image(0, 0, 1, asset()).unwrap();
        m
    }
    #[test]
    fn pointer_gesture_commits_once_and_cancel_preserves_redo() {
        let mut m = model();
        let original = m.editor.document().clone();
        let center = m.visible_images()[0].center;
        let start = m.viewport().screen(center);
        m.image_pointer("down", start, false, "first").unwrap();
        assert!(m.has_draft());
        for offset in [20., 40., 60.] {
            m.image_pointer("move", [start[0] + offset, start[1]], false, "first")
                .unwrap();
        }
        assert_eq!(m.editor.document(), &original);
        assert_eq!(m.editor.revision(), 1);
        m.image_pointer("up", start, false, "first").unwrap();
        assert_eq!(m.editor.revision(), 2);
        assert!(
            (m.visible_images()[0].center[0] - center[0] - 60. / m.viewport().scale()).abs() < 1e-8
        );
        m.action(Edit::Undo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document(), &original);
        m.image_pointer("down", start, false, "second").unwrap();
        m.image_pointer("move", [start[0], start[1] + 30.], false, "second")
            .unwrap();
        m.image_pointer("cancel", start, false, "second").unwrap();
        assert_eq!(m.editor.document(), &original);
        assert!(m.editor.can_redo());
        assert!(!m.has_draft());
        m.action(Edit::Redo { revision: 3 }).unwrap();
        assert_eq!(m.editor.revision(), 4);
    }
    #[test]
    fn glyph_switch_tool_change_and_old_tokens_cannot_commit_a_draft() {
        let mut m = model();
        let original = m.editor.document().clone();
        let center = m.viewport().screen(m.visible_images()[0].center);
        m.image_pointer("down", center, false, "a").unwrap();
        m.image_pointer("move", [center[0] + 40., center[1]], false, "a")
            .unwrap();
        m.action(Edit::SelectGlyph { id: 2 }).unwrap();
        assert!(!m.has_draft());
        assert!(m.images.selected.is_none());
        assert_eq!(m.editor.document(), &original);
        assert!(m.image_context(0, 1, 1).is_err());
        m.action(Edit::SelectGlyph { id: 1 }).unwrap();
        m.image_pointer("down", center, false, "b").unwrap();
        assert!(m.image_pointer("up", center, false, "a").is_err());
        assert!(m.has_draft());
        m.action(Edit::Tool { tool: Tool::Nodes }).unwrap();
        assert!(!m.has_draft());
        assert_eq!(m.editor.document(), &original);
    }
    #[test]
    fn opacity_gesture_is_owned_by_window_and_is_one_undo_step() {
        let mut m = model();
        let before = m.editor.document().clone();
        let id = m.visible_images()[0].id;
        m.image_action(
            Action::OpacityBegin {
                id,
                token: "opacity".into(),
            },
            "panel-outline",
        )
        .unwrap();
        assert!(m
            .image_action(
                Action::OpacityMove {
                    token: "opacity".into(),
                    value: 0.1
                },
                "main"
            )
            .is_err());
        for value in [0.1, 0.2, 0.] {
            m.image_action(
                Action::OpacityMove {
                    token: "opacity".into(),
                    value,
                },
                "panel-outline",
            )
            .unwrap();
        }
        assert_eq!(m.editor.document(), &before);
        assert_eq!(m.visible_images()[0].opacity, 0.);
        assert!(m.image_action(Action::Remove { id }, "main").is_err());
        m.image_action(
            Action::OpacityEnd {
                token: "opacity".into(),
            },
            "panel-outline",
        )
        .unwrap();
        assert_eq!(m.editor.revision(), 2);
        assert_eq!(m.visible_images()[0].opacity, 0.);
        m.action(Edit::Undo { revision: 2 }).unwrap();
        assert_eq!(m.editor.document(), &before);
    }
    #[test]
    fn hidden_locked_and_overlapping_images_use_correct_hit_priority() {
        let mut m = model();
        m.import_image(0, 1, 1, asset()).unwrap();
        let center = m.viewport().screen(m.visible_images()[0].center);
        m.images.selected = None;
        m.image_pointer("down", center, false, "a").unwrap();
        assert_eq!(m.images.selected, Some(2));
        m.image_pointer("cancel", center, false, "a").unwrap();
        let mut image = m.visible_images()[1].clone();
        image.locked = true;
        m.image_action(Action::Update { image }, "main").unwrap();
        m.image_pointer("down", center, false, "b").unwrap();
        assert_eq!(m.images.selected, Some(1));
        m.image_pointer("cancel", center, false, "b").unwrap();
        let mut image = m.visible_images()[0].clone();
        image.visible = false;
        m.image_action(Action::Update { image }, "main").unwrap();
        m.image_pointer("down", center, false, "c").unwrap();
        assert!(m.images.selected.is_none());
        assert!(!m.has_draft());
        m.image_action(Action::Select { id: Some(2) }, "panel-outline")
            .unwrap();
        assert_eq!(m.images.selected, Some(2));
    }
    #[test]
    fn stale_import_and_proof_are_rejected_and_snapshots_never_contain_pixels() {
        let mut m = model();
        let doc = m.editor.document().clone();
        for (epoch, revision, glyph) in [(1, 1, 1), (0, 0, 1), (0, 1, 2)] {
            assert!(m.import_image(epoch, revision, glyph, asset()).is_err());
        }
        assert_eq!(m.editor.document(), &doc);
        m.proof.active = true;
        assert!(m.image_context(0, 1, 1).is_err());
        m.proof.active = false;
        let snapshot = serde_json::to_value(m.snapshot()).unwrap();
        assert!(snapshot.get("assets").is_none());
        assert_eq!(snapshot["images"][0]["id"], 1);
        assert!(!snapshot.to_string().contains("rgba"));
        assert!(!snapshot.to_string().contains("\"encoded\":"));
        let p = m.viewport().screen(m.visible_images()[0].center);
        m.image_pointer("down", p, false, "a").unwrap();
        assert!(m.import_image(0, 1, 1, asset()).is_err());
        m.replace_document(font_core::FontDocument::new("Other", "B").unwrap(), None)
            .unwrap();
        assert!(m.images.selected.is_none());
        assert!(!m.has_draft());
        assert!(m.visible_images().is_empty());
    }
}
