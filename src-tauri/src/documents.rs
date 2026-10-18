//! Document tabs share the Rust model and workspace; no WebView owns a document.
use crate::{model::Model, AppState};
use serde::Serialize;
use tauri::{Manager, WebviewWindow};
#[derive(Serialize)]
pub struct Summary {
    pub id: u64,
    pub family: String,
    pub path: Option<std::path::PathBuf>,
    pub dirty: bool,
}
impl Model {
    pub fn all_documents(&self) -> impl Iterator<Item = &Model> {
        std::iter::once(self).chain(self.parked.values().map(Box::as_ref))
    }
    pub fn document(&self, id: u64) -> Option<&Model> {
        if self.document_id == id {
            Some(self)
        } else {
            self.parked.get(&id).map(Box::as_ref)
        }
    }
    pub fn document_summaries(&self) -> Vec<Summary> {
        let mut list = self
            .all_documents()
            .map(|m| Summary {
                id: m.document_id,
                family: m.editor.document().family.clone(),
                path: m.path.clone(),
                dirty: m.unsaved(),
            })
            .collect::<Vec<_>>();
        list.sort_by_key(|d| d.id);
        list
    }
    pub fn confirmation_name(&self) -> String {
        match &self.path {
            Some(p) => format!("{} — {}", self.editor.document().family, p.display()),
            None => format!("{} · {}", self.editor.document().family, self.document_id),
        }
    }
    pub fn unsaved(&self) -> bool {
        (self.saved_revision != Some(self.editor.revision()) || self.has_draft())
            && !(self.document_id == 1
                && self.editor.revision() == 0
                && !self.has_draft()
                && self.path.is_none())
    }
    pub fn activate_document(&mut self, id: u64) -> Result<(), String> {
        if id == self.document_id {
            return Ok(());
        }
        if self.has_draft() {
            return Err("edit_in_progress".into());
        }
        let epoch = self.epoch.checked_add(1).ok_or("document_epoch_overflow")?;
        let mut target = self.parked.remove(&id).ok_or("document_not_found")?;
        let parked = std::mem::take(&mut self.parked);
        std::mem::swap(self, &mut target);
        self.parked = parked;
        self.workspace = target.workspace.clone();
        self.size = target.size;
        self.text_focus = std::mem::take(&mut target.text_focus);
        self.next_document_id = target.next_document_id;
        self.epoch = epoch;
        self.parked.insert(target.document_id, target);
        Ok(())
    }
    pub fn open_document(
        &mut self,
        doc: font_core::FontDocument,
        path: Option<std::path::PathBuf>,
    ) -> Result<(), String> {
        if self.has_draft() {
            return Err("edit_in_progress".into());
        }
        if let Some(p) = &path {
            if let Some(id) = self.path_owner(p) {
                return self.activate_document(id);
            }
        }
        if self.parked.len() >= 15 {
            return Err("document_limit".into());
        }
        let id = self.next_document_id;
        let next = id.checked_add(1).ok_or("document_epoch_overflow")?;
        // Validate the new document before changing any existing state.
        let mut target = Model::new(self.workspace.clone());
        target.replace_document(doc, path)?;
        target.document_id = id;
        self.parked.insert(id, Box::new(target));
        if let Err(e) = self.activate_document(id) {
            self.parked.remove(&id);
            return Err(e);
        }
        self.next_document_id = next;
        Ok(())
    }
    pub fn path_owner(&self, path: &std::path::Path) -> Option<u64> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.all_documents()
            .find(|m| {
                m.path
                    .as_ref()
                    .is_some_and(|p| p.canonicalize().unwrap_or_else(|_| p.clone()) == path)
            })
            .map(|m| m.document_id)
    }
    pub fn close_document(&mut self, id: u64) -> Result<(), String> {
        if self.document(id).ok_or("document_not_found")?.has_draft() {
            return Err("edit_in_progress".into());
        }
        if id == self.document_id {
            if let Some(&next) = self.parked.keys().next() {
                self.activate_document(next)?;
            } else {
                self.open_document(
                    font_core::FontDocument::new("Untitled Sans", "").map_err(|e| e.to_string())?,
                    None,
                )?;
                self.saved_revision = Some(0);
            }
        }
        self.parked.remove(&id);
        Ok(())
    }
}
#[tauri::command]
pub async fn document_action(window: WebviewWindow, id: u64, close: bool) -> Result<bool, String> {
    crate::allowed(&window)?;
    if crate::font_batch::busy() {
        return Err("file_operation_in_progress".into());
    }
    let _guard = crate::files::FileGuard::acquire()?;
    let app = window.app_handle().clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let result = (|| {
                if close {
                    let (dirty, family, locale, revision) = {
                        let state = app.state::<AppState>();
                        let m = state.0.lock().map_err(|_| "state_poisoned")?;
                        let d = m.document(id).ok_or("document_not_found")?;
                        (
                            d.unsaved(),
                            d.confirmation_name(),
                            m.workspace.locale,
                            d.editor.revision(),
                        )
                    };
                    if dirty && !crate::confirm_documents(&app, &[family], locale) {
                        return Ok(false);
                    }
                    {
                        let state = app.state::<AppState>();
                        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                        if m.document(id)
                            .ok_or("document_not_found")?
                            .editor
                            .revision()
                            != revision
                        {
                            return Err("Conflict".into());
                        }
                        if m.document_id == id && m.has_draft() {
                            m.action(crate::model::Action::Cancel)?;
                        }
                        m.close_document(id)?;
                    }
                    crate::recovery::clear_document(&app, id);
                } else {
                    app.state::<AppState>()
                        .0
                        .lock()
                        .map_err(|_| "state_poisoned")?
                        .activate_document(id)?;
                }
                crate::publish(&app);
                Ok(true)
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action;
    use font_core::{
        workspace::{Locale, Workspace},
        FontDocument,
    };
    fn advance(m: &mut Model, value: f64) {
        m.action(Action::Advance {
            id: m.glyph,
            revision: m.editor.revision(),
            value,
        })
        .unwrap();
    }
    fn value(m: &Model) -> f64 {
        m.editor.document().glyphs[&m.glyph].advance
    }
    #[test]
    fn documents_keep_independent_undo_redo_selection_and_view() {
        let mut m = Model::new(Workspace::default());
        advance(&mut m, 700.);
        m.zoom = 2.;
        m.pan = [15., 20.];
        m.proof.settings.text = "A中".into();
        m.open_document(FontDocument::new("Second", "中").unwrap(), None)
            .unwrap();
        let second = m.document_id;
        advance(&mut m, 900.);
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert!(m.editor.can_redo());
        let epoch = m.epoch;
        m.activate_document(1).unwrap();
        assert!(m.epoch > epoch);
        assert_eq!(value(&m), 700.);
        assert_eq!(m.zoom, 2.);
        assert_eq!(m.pan, [15., 20.]);
        assert_eq!(m.proof.settings.text, "A中");
        m.action(Action::Undo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(value(&m), 640.);
        m.activate_document(second).unwrap();
        assert!(m.editor.can_redo());
        m.action(Action::Redo {
            revision: m.editor.revision(),
        })
        .unwrap();
        assert_eq!(value(&m), 900.);
        m.activate_document(1).unwrap();
        assert!(m.editor.can_redo());
        assert_eq!(value(&m), 640.);
    }
    #[test]
    fn switching_preserves_shared_workspace_and_uses_fresh_epoch() {
        let mut m = Model::new(Workspace::default());
        m.open_document(FontDocument::new("B", "B").unwrap(), None)
            .unwrap();
        m.workspace.locale = Locale::ZhCn;
        m.workspace
            .resize(m.workspace.revision, font_core::workspace::Dock::Left, 300.)
            .unwrap();
        let epoch = m.epoch;
        m.activate_document(1).unwrap();
        assert_eq!(m.workspace.locale, Locale::ZhCn);
        assert_eq!(m.workspace.left_width, 300.);
        assert!(m.epoch > epoch);
        m.mark_saved(epoch, 0, "wrong.json".into());
        assert!(m.path.is_none());
    }
    #[test]
    fn duplicate_path_activates_existing_without_replacing_unsaved_history() {
        let mut m = Model::new(Workspace::default());
        let p = std::env::temp_dir().join("fonts-doc-duplicate-test.json");
        m.open_document(FontDocument::new("Saved", "A").unwrap(), Some(p.clone()))
            .unwrap();
        let id = m.document_id;
        advance(&mut m, 888.);
        m.open_document(FontDocument::new("Other", "中").unwrap(), None)
            .unwrap();
        m.open_document(FontDocument::new("Disk", "B").unwrap(), Some(p))
            .unwrap();
        assert_eq!(m.document_id, id);
        assert_eq!(m.editor.document().family, "Saved");
        assert_eq!(value(&m), 888.);
        assert_eq!(m.all_documents().count(), 3);
        assert!(m.unsaved());
    }
    #[test]
    fn close_inactive_and_last_do_not_lose_another_document() {
        let mut m = Model::new(Workspace::default());
        advance(&mut m, 777.);
        m.open_document(FontDocument::new("B", "中").unwrap(), None)
            .unwrap();
        let id = m.document_id;
        m.close_document(id).unwrap();
        assert_eq!(m.document_id, 1);
        assert_eq!(value(&m), 777.);
        assert!(m.editor.can_undo());
        m.close_document(1).unwrap();
        assert_eq!(m.all_documents().count(), 1);
        assert!(!m.unsaved());
        assert!(m.document(1).is_none());
    }
    #[test]
    fn drafts_invalid_documents_limits_and_bad_ids_leave_documents_intact() {
        let mut m = Model::new(Workspace::default());
        m.pen.push([1., 2.]);
        assert!(m
            .open_document(FontDocument::new("B", "B").unwrap(), None)
            .is_err());
        m.pen.clear();
        let mut bad = FontDocument::new("Invalid", "A").unwrap();
        bad.units_per_em = 0;
        assert!(m.open_document(bad, None).is_err());
        assert_eq!(m.all_documents().count(), 1);
        assert!(m.activate_document(999).is_err());
        assert_eq!(m.document_id, 1);
        for _ in 0..15 {
            m.open_document(FontDocument::new("B", "B").unwrap(), None)
                .unwrap();
        }
        let id = m.document_id;
        assert!(m
            .open_document(FontDocument::new("Overflow", "B").unwrap(), None)
            .is_err());
        assert_eq!(m.document_id, id);
        assert_eq!(m.all_documents().count(), 16);
    }
    #[test]
    fn dirty_inactive_documents_and_save_markers_are_independent() {
        let mut m = Model::new(Workspace::default());
        advance(&mut m, 700.);
        let rev = m.editor.revision();
        m.mark_saved(m.epoch, rev, "a.json".into());
        assert!(!m.unsaved());
        m.open_document(FontDocument::new("B", "B").unwrap(), None)
            .unwrap();
        let second = m.document_id;
        advance(&mut m, 999.);
        m.activate_document(1).unwrap();
        assert!(!m.unsaved());
        assert!(m.document(second).unwrap().unsaved());
        assert_eq!(m.document_summaries().iter().filter(|d| d.dirty).count(), 1);
    }
}
