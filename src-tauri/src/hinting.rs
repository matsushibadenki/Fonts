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
    mode: hinting::Mode,
) -> Result<Option<Report>, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let e = font_library::entry(window.app_handle(), id)?;
    let source = e.path.clone();
    let (bytes, info) = tauri::async_runtime::spawn_blocking(move || {
        let original = font_io::read(&e.path)?;
        let info = hinting::info(&original, e.index)?;
        let bytes = match mode {
            hinting::Mode::Auto => hinting::autohint(&original, e.index)?,
            hinting::Mode::Remove => hinting::export(&original, e.index, true)?,
            hinting::Mode::Preserve => hinting::export(&original, e.index, false)?,
        };
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
    let suffix = match mode {
        hinting::Mode::Auto => "Autohinted",
        hinting::Mode::Remove => "Unhinted",
        hinting::Mode::Preserve => "Preserved",
    };
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

/// At most one comparison per application; image bytes never cross JSON IPC.
#[derive(Default)]
pub struct Previews(std::sync::Mutex<PreviewImages>);
#[derive(Default)]
struct PreviewImages {
    generation: u64,
    owner: String,
    images: Vec<Vec<u8>>,
}
#[derive(Serialize)]
pub struct Comparison {
    generation: u64,
    sample: String,
    missing: usize,
    auto_error: Option<String>,
    columns: usize,
    sizes: [u32; 4],
}
#[tauri::command]
pub async fn font_hinting_compare(
    window: WebviewWindow,
    id: u64,
    text: String,
) -> Result<Comparison, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let entry = font_library::entry(window.app_handle(), id)?;
    let preview = tauri::async_runtime::spawn_blocking(move || {
        font_core::hinting_preview::compare(&font_io::read(&entry.path)?, entry.index, &text)
    })
    .await
    .map_err(|e| e.to_string())??;
    let state = window.state::<Previews>();
    let mut images = state.0.lock().map_err(|_| "state_poisoned")?;
    images.generation = images.generation.checked_add(1).ok_or("hinting_preview")?;
    images.owner = window.label().to_owned();
    let result = Comparison {
        generation: images.generation,
        sample: preview.sample,
        missing: preview.missing,
        auto_error: preview.auto_error,
        columns: preview.images.len() / 4,
        sizes: font_core::hinting_preview::SIZES,
    };
    images.images = preview.images;
    Ok(result)
}
impl Previews {
    pub fn resource(&self, window: &str, path: &str) -> Option<Vec<u8>> {
        let mut parts = path.strip_prefix('/')?.strip_suffix(".png")?.split('-');
        let generation: u64 = parts.next()?.parse().ok()?;
        let index: usize = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let images = self.0.lock().ok()?;
        if images.owner != window || images.generation != generation {
            return None;
        }
        images.images.get(index).cloned()
    }
}
#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn images_are_scoped_to_owner_and_generation() {
        let previews = Previews(std::sync::Mutex::new(PreviewImages {
            generation: 2,
            owner: "library".into(),
            images: vec![vec![1]],
        }));
        assert_eq!(previews.resource("library", "/2-0.png"), Some(vec![1]));
        for (owner, path) in [
            ("main", "/2-0.png"),
            ("library", "/1-0.png"),
            ("library", "/2-1.png"),
            ("library", "/2-0.png/x"),
            ("library", "/etc/passwd"),
        ] {
            assert!(previews.resource(owner, path).is_none());
        }
    }
}
