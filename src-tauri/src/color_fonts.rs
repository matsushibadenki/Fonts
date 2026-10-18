use crate::{font_library, AppState};
use font_core::color_fonts::{self, Info};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, WebviewWindow};
#[derive(Clone)]
pub struct Source {
    id: u64,
    index: u32,
    bytes: Arc<Vec<u8>>,
    info: Info,
}
#[tauri::command]
pub async fn color_font_open(window: WebviewWindow, id: u64) -> Result<Info, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let e = font_library::entry(window.app_handle(), id)?;
    let source = tauri::async_runtime::spawn_blocking(move || {
        let bytes = font_core::font_io::read(&e.path)?;
        let info = color_fonts::info(&bytes, e.index)?;
        Ok::<_, String>(Source {
            id,
            index: e.index,
            bytes: Arc::new(bytes),
            info,
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    let info = source.info.clone();
    window
        .app_handle()
        .state::<font_library::Library>()
        .0
        .lock()
        .map_err(|_| "state_poisoned")?
        .color = Some(source);
    Ok(info)
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Preview,
    Export,
    Release,
}
#[derive(Serialize, Default)]
pub struct Report {
    path: Option<std::path::PathBuf>,
    bytes: usize,
    verified: bool,
    missing: usize,
}
#[derive(Deserialize)]
pub struct Request {
    id: u64,
    action: Action,
    text: String,
    size: u32,
    palette: u16,
    epoch: u64,
    revision: u64,
}
#[tauri::command]
pub async fn color_font_action(
    window: WebviewWindow,
    request: Request,
) -> Result<Option<Report>, String> {
    let Request {
        id,
        action,
        text,
        size,
        palette,
        epoch,
        revision,
    } = request;
    crate::allowed(&window)?;
    let app = window.app_handle().clone();
    if matches!(action, Action::Release) {
        let library = app.state::<font_library::Library>();
        let mut l = library.0.lock().map_err(|_| "state_poisoned")?;
        if l.color.as_ref().is_some_and(|s| s.id == id) {
            l.color = None;
        }
        return Ok(None);
    }
    let _guard = crate::files::FileGuard::acquire()?;
    let source = app
        .state::<font_library::Library>()
        .0
        .lock()
        .map_err(|_| "state_poisoned")?
        .color
        .as_ref()
        .filter(|s| s.id == id)
        .cloned()
        .ok_or("color_expired")?;
    match action {
        Action::Preview => {
            let (generation, dimensions) = {
                let state = app.state::<AppState>();
                let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                if m.epoch != epoch || m.editor.revision() != revision {
                    return Err("Conflict".into());
                }
                if m.has_draft() {
                    return Err("edit_in_progress".into());
                }
                m.proof.generation += 1;
                (
                    m.proof.generation,
                    [
                        m.size[0].clamp(256., 2048.) as u32,
                        m.size[1].clamp(256., 1024.) as u32,
                    ],
                )
            };
            let preview = tauri::async_runtime::spawn_blocking(move || {
                color_fonts::preview(
                    &source.bytes,
                    source.index,
                    &text,
                    size,
                    palette,
                    dimensions,
                )
            })
            .await
            .map_err(|e| e.to_string())??;
            let missing = preview.missing;
            let state = app.state::<AppState>();
            let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
            if m.epoch != epoch
                || m.editor.revision() != revision
                || m.proof.generation != generation
            {
                return Err("Conflict".into());
            }
            m.proof.color = Some(Arc::new(preview));
            m.proof.font = None;
            m.proof.source = None;
            m.proof.reference = None;
            m.proof.active = true;
            drop(m);
            crate::publish(&app);
            if let Some(w) = app.get_webview_window("main") {
                w.show().map_err(|e| e.to_string())?;
                w.set_focus().map_err(|e| e.to_string())?;
            }
            Ok(Some(Report {
                missing,
                ..Default::default()
            }))
        }
        Action::Export => {
            let original = source.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                color_fonts::export(&source.bytes, source.index)
            })
            .await
            .map_err(|e| e.to_string())??;
            let filename: String = original
                .info
                .family
                .chars()
                .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
                .take(80)
                .collect();
            let name = if filename.is_empty() {
                "ColorFont"
            } else {
                &filename
            };
            let ext = original.info.extension.clone();
            let dialog = rfd::AsyncFileDialog::new()
                .set_parent(&window)
                .set_file_name(format!("{name}-Color.{ext}"));
            #[cfg(not(target_os = "macos"))]
            let dialog = dialog.add_filter("OpenType color font", &[&ext]);
            let Some(file) = dialog.save_file().await else {
                return Ok(None);
            };
            let mut path = file.path().to_path_buf();
            if path.extension().is_none() {
                path.set_extension(&ext);
                if path.exists() {
                    return Err("font_output_exists".into());
                }
            }
            if !path
                .extension()
                .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(&ext))
            {
                return Err("font_extension_mismatch".into());
            }
            if font_library::protected(&path)? {
                return Err("font_protected_location".into());
            }
            let report = Report {
                path: Some(path.clone()),
                bytes: bytes.len(),
                verified: true,
                missing: 0,
            };
            tauri::async_runtime::spawn_blocking(move || {
                font_core::storage::write_bytes(&path, &bytes).map_err(|e| e.to_string())?;
                let saved = font_core::font_io::read(&path)?;
                if saved != bytes {
                    return Err("color_invalid_output".into());
                }
                color_fonts::verify(&saved, &original.bytes, original.index)
            })
            .await
            .map_err(|e| e.to_string())??;
            Ok(Some(report))
        }
        Action::Release => unreachable!(),
    }
}
