//! File actions are serialized and never attach a saved snapshot to a different document.
use crate::AppState;
use font_core::FontDocument;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WebviewWindow};
static BUSY: AtomicBool = AtomicBool::new(false);
pub(crate) struct FileGuard;
impl FileGuard {
    pub(crate) fn acquire() -> Result<Self, String> {
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "file_operation_in_progress".into())
    }
}
impl Drop for FileGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
pub fn busy() -> bool {
    BUSY.load(Ordering::Acquire)
}
async fn on_main<T: Send + 'static>(
    window: &WebviewWindow,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f());
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??
}
pub async fn save(window: WebviewWindow, save_as: bool) -> Result<(), String> {
    let _guard = FileGuard::acquire()?;
    let app = window.app_handle().clone();
    let (existing, locale) = {
        let state = app.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        (m.path.clone(), m.workspace.locale)
    };
    let path = if let Some(path) = existing.clone().filter(|_| !save_as) {
        path
    } else {
        let parent = window.clone();
        let selected = tauri::async_runtime::spawn_blocking(move || {
            let title = match locale {
                font_core::workspace::Locale::En => "Save Project",
                font_core::workspace::Locale::Ja => "プロジェクトを保存",
                font_core::workspace::Locale::ZhCn => "保存项目",
            };
            let mut dialog = rfd::FileDialog::new()
                .set_parent(&parent)
                .set_title(title)
                .add_filter("Fonts project", &["json"]);
            if let Some(path) = existing.as_ref() {
                if let Some(p) = path.parent() {
                    dialog = dialog.set_directory(p);
                }
                if let Some(name) = path.file_name() {
                    dialog = dialog.set_file_name(name.to_string_lossy());
                }
            } else {
                dialog = dialog.set_file_name("Untitled.fontproj.json");
            }
            dialog.save_file()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(path) = selected else {
            return Ok(());
        };
        path
    };
    let state = app.state::<AppState>();
    let (doc, epoch, revision) = {
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.has_draft() {
            return Err("edit_in_progress".into());
        }
        (m.editor.document().clone(), m.epoch, m.editor.revision())
    };
    let output = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        font_core::storage::save(&output, &doc).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    {
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        m.mark_saved(epoch, revision, path);
    }
    crate::publish(&app);
    Ok(())
}
pub async fn open(window: WebviewWindow) -> Result<(), String> {
    let _guard = FileGuard::acquire()?;
    let parent = window.clone();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_parent(&parent)
            .add_filter("Fonts project", &["json"])
            .pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = selected else {
        return Ok(());
    };
    let input = path.clone();
    // Parse before asking to discard the current document. Invalid input never replaces it.
    let document = tauri::async_runtime::spawn_blocking(move || {
        font_core::storage::load(&input).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    replace(&window, document, Some(path)).await.map(|_| ())
}
pub async fn new(window: WebviewWindow) -> Result<(), String> {
    let _guard = FileGuard::acquire()?;
    let document = FontDocument::new("Untitled Sans", "").map_err(|e| e.to_string())?;
    replace(&window, document, None).await.map(|_| ())
}
pub(crate) async fn replace(
    window: &WebviewWindow,
    document: FontDocument,
    path: Option<std::path::PathBuf>,
) -> Result<bool, String> {
    let app = window.app_handle().clone();
    on_main(window, move || {
        if !crate::confirm_discard(&app) {
            return Ok(false);
        }
        {
            let state = app.state::<AppState>();
            let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
            m.replace_document(document, path)?;
        }
        crate::recovery::clear_current(&app);
        crate::publish(&app);
        Ok(true)
    })
    .await
}
