use crate::font_library;
use font_core::{font_io, hinting};
use serde::Serialize;
use tauri::{Manager, WebviewWindow};
#[tauri::command]
pub async fn font_hinting_info(window: WebviewWindow, id: u64) -> Result<hinting::Info, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let e = font_library::entry(window.app_handle(), id)?;
    tauri::async_runtime::spawn_blocking(move || hinting::info(&font_io::read(&e.path)?, e.index))
        .await
        .map_err(|e| e.to_string())?
}
#[derive(Serialize)]
pub struct Report {
    path: std::path::PathBuf,
    bytes: usize,
    verified: bool,
}
#[tauri::command]
pub async fn font_hinting_export(
    window: WebviewWindow,
    id: u64,
    remove: bool,
) -> Result<Option<Report>, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let e = font_library::entry(window.app_handle(), id)?;
    let source = e.path.clone();
    let (bytes, info) = tauri::async_runtime::spawn_blocking(move || {
        let original = font_io::read(&e.path)?;
        let info = hinting::info(&original, e.index)?;
        let bytes = hinting::export(&original, e.index, remove)?;
        Ok::<_, String>((bytes, info))
    })
    .await
    .map_err(|e| e.to_string())??;
    let name: String = info
        .family
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        .take(80)
        .collect();
    let name = if name.is_empty() { "Font" } else { &name };
    let suffix = if remove { "Unhinted" } else { "Preserved" };
    let ext = &info.extension;
    let dialog = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_file_name(format!("{name}-{suffix}.{ext}"));
    #[cfg(not(target_os = "macos"))]
    let dialog = dialog.add_filter("OpenType", &[ext]);
    let Some(file) = dialog.save_file().await else {
        return Ok(None);
    };
    let mut path = file.path().to_path_buf();
    if path.extension().is_none() {
        path.set_extension(ext);
        if path.exists() {
            return Err("font_output_exists".into());
        }
    }
    if !path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
    {
        return Err("font_extension_mismatch".into());
    }
    if font_library::protected(&path)?
        || (path.exists() && path.canonicalize().ok() == source.canonicalize().ok())
    {
        return Err("font_protected_location".into());
    }
    let report = Report {
        path: path.clone(),
        bytes: bytes.len(),
        verified: true,
    };
    tauri::async_runtime::spawn_blocking(move || {
        font_core::storage::write_bytes(&path, &bytes).map_err(|e| e.to_string())?;
        let saved = font_io::read(&path)?;
        if saved != bytes {
            return Err("hinting_invalid".into());
        }
        hinting::info(&saved, 0)?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Some(report))
}
