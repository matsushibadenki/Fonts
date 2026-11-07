//! Worker tracing with optimistic document checks and an owned, nonpersistent GPU draft.
use crate::{model::Model, AppState};
use font_core::{image_trace::Settings, Glyph, GlyphId};
use serde::Deserialize;
use tauri::{Manager, WebviewWindow};
pub struct Draft {
    pub glyph: Glyph,
    pub revision: u64,
    pub owner: String,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Preview { id: u64, settings: Settings },
    Apply,
    Cancel,
}
impl Model {
    pub fn trace_context(&self, epoch: u64, revision: u64, glyph: GlyphId) -> Result<(), String> {
        self.image_context(epoch, revision, glyph)?;
        if self.images.drag.is_some()
            || self.master_edit.is_some()
            || self.editor.document().source_font.is_some()
        {
            return Err("trace_unavailable".into());
        }
        Ok(())
    }
    pub fn accept_trace(
        &mut self,
        epoch: u64,
        request: font_core::identity::Uuid,
        id: u64,
        draft: Draft,
    ) -> Result<(), String> {
        self.trace_context(epoch, draft.revision, draft.glyph.id)?;
        if self.images.trace_request != Some(request) || self.images.selected != Some(id) {
            return Err("Conflict".into());
        }
        if self
            .images
            .trace
            .as_ref()
            .is_some_and(|d| d.owner != draft.owner)
        {
            return Err("edit_in_progress".into());
        }
        self.images.trace = Some(draft);
        Ok(())
    }
    pub fn finish_trace(&mut self, owner: &str, apply: bool) -> Result<(), String> {
        let draft = self.images.trace.as_ref().ok_or("Conflict")?;
        if draft.owner != owner
            || draft.revision != self.editor.revision()
            || draft.glyph.id != self.glyph
        {
            return Err("Conflict".into());
        }
        if apply {
            self.editor
                .replace_glyph(draft.revision, draft.glyph.clone())
                .map_err(|e| e.to_string())?;
        }
        self.images.trace = None;
        self.images.trace_request = None;
        self.selected = None;
        self.selected_nodes.clear();
        self.selected_controls.clear();
        Ok(())
    }
}
#[tauri::command]
pub async fn reference_trace(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    glyph: GlyphId,
    action: Action,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let app = window.app_handle().clone();
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        match action {
            Action::Preview { id, settings } => {
                let request = font_core::identity::Uuid::new_v4();
                let (asset, placement, mut target) = {
                    let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                    m.trace_context(epoch, revision, glyph)?;
                    if m.images.trace.as_ref().is_some_and(|d| d.owner != owner) {
                        return Err("edit_in_progress".into());
                    }
                    m.images.trace_request = Some(request);
                    m.images.trace = None;
                    let refs = &m.editor.document().reference_images;
                    let placement = refs
                        .for_glyph(glyph)
                        .iter()
                        .find(|p| p.id == id)
                        .cloned()
                        .ok_or("InvalidDocument")?;
                    let asset = refs
                        .assets()
                        .get(&placement.asset_id)
                        .cloned()
                        .ok_or("InvalidDocument")?;
                    (
                        asset,
                        placement,
                        m.editor
                            .document()
                            .glyphs
                            .get(&glyph)
                            .cloned()
                            .ok_or("MissingGlyph")?,
                    )
                };
                crate::publish(&app);
                let contours = font_core::image_trace::trace(&asset, &placement, settings)
                    .map_err(|e| match e {
                        font_core::Error::TooLarge => "trace_too_large".into(),
                        _ => e.to_string(),
                    })?;
                if contours.is_empty() {
                    return Err("trace_empty".into());
                }
                target.contours.extend(contours);
                target.status = font_core::GlyphStatus::Draft;
                target.validate().map_err(|e| e.to_string())?;
                let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                m.accept_trace(
                    epoch,
                    request,
                    id,
                    Draft {
                        glyph: target,
                        revision,
                        owner,
                    },
                )?;
            }
            Action::Apply | Action::Cancel => {
                let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                m.trace_context(epoch, revision, glyph)?;
                m.finish_trace(&owner, matches!(action, Action::Apply))?;
            }
        }
        crate::publish(&app);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_core::{
        image_trace::{trace, Settings},
        reference_images::ImageAsset,
        workspace::Workspace,
    };
    fn model() -> Model {
        let mut m = Model::new(Workspace::default());
        let mut bytes = vec![];
        {
            let mut encoder = png::Encoder::new(&mut bytes, 5, 5);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let pixels: Vec<_> = (0..25)
                .flat_map(|i| {
                    if i == 12 {
                        [255, 255, 255, 255]
                    } else {
                        [0, 0, 0, 255]
                    }
                })
                .collect();
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
        }
        m.import_image(0, 0, 1, ImageAsset::from_bytes(bytes).unwrap())
            .unwrap();
        m
    }
    fn draft(m: &mut Model) -> (font_core::identity::Uuid, u64, Draft) {
        let refs = &m.editor.document().reference_images;
        let image = &refs.for_glyph(1)[0];
        let id = image.id;
        let contours = trace(
            &refs.assets()[&image.asset_id],
            image,
            Settings {
                min_area: 0.,
                ..Default::default()
            },
        )
        .unwrap();
        let mut glyph = m.editor.document().glyphs[&1].clone();
        glyph.contours.extend(contours);
        let request = font_core::identity::Uuid::new_v4();
        m.images.trace_request = Some(request);
        (
            request,
            id,
            Draft {
                glyph,
                revision: m.editor.revision(),
                owner: "main".into(),
            },
        )
    }
    #[test]
    fn preview_is_transient_cancel_rejects_late_workers_apply_is_one_undo_and_recovers() {
        let mut m = model();
        let before = m.editor.document().clone();
        let (request, id, d) = draft(&mut m);
        m.action(crate::model::Action::Cancel).unwrap();
        assert!(m.accept_trace(0, request, id, d).is_err());
        m.images.selected = Some(id);
        let (request, id, d) = draft(&mut m);
        m.accept_trace(0, request, id, d).unwrap();
        assert!(m.has_draft());
        assert_eq!(m.editor.document(), &before);
        assert_eq!(
            m.visible_glyph().contours.len(),
            before.glyphs[&1].contours.len() + 2
        );
        assert!(m.finish_trace("panel-outline", true).is_err());
        m.finish_trace("main", false).unwrap();
        assert_eq!(m.editor.document(), &before);
        let (request, id, d) = draft(&mut m);
        m.accept_trace(0, request, id, d).unwrap();
        m.finish_trace("main", true).unwrap();
        let after = m.editor.document().clone();
        assert_eq!(m.editor.revision(), 2);
        assert!(!m.has_draft());
        assert_eq!(after.reference_images, before.reference_images);
        m.action(crate::model::Action::Undo { revision: 2 })
            .unwrap();
        assert_eq!(m.editor.document(), &before);
        m.action(crate::model::Action::Redo { revision: 3 })
            .unwrap();
        assert_eq!(m.editor.document(), &after);
        let recovered: font_core::FontDocument =
            serde_json::from_slice(&serde_json::to_vec(&after).unwrap()).unwrap();
        recovered.validate().unwrap();
        assert_eq!(recovered, after);
        let root = std::env::temp_dir().join(format!(
            "fonts-trace-recovery-{}",
            font_core::identity::Uuid::new_v4()
        ));
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        store
            .checkpoint(&after, m.epoch, m.editor.revision(), None)
            .unwrap();
        drop(store);
        let store = crate::recovery_store::Store::new(root.clone()).unwrap();
        let (list, _) = store.candidates().unwrap();
        let (doc, lease) = store.candidate(&list[0].id).unwrap();
        assert_eq!(doc, after);
        drop(lease);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();

        let (request, id, d) = draft(&mut m);
        m.replace_document(font_core::FontDocument::new("Other", "B").unwrap(), None)
            .unwrap();
        assert!(m.accept_trace(0, request, id, d).is_err());
        assert!(m.images.trace.is_none());
    }
}
