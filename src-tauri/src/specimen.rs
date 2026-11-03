//! Print documents are assembled on the Rust worker; font bytes never cross IPC.
use crate::{font_catalog, font_paths};
use serde::{Deserialize, Serialize};
use tauri::{Manager, WebviewWindow};
#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub title: String,
    pub samples: Vec<String>,
    pub size: u32,
    pub source: String,
    pub locale: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            title: "Font Specimens".into(),
            samples: vec![
                "美しい文字で、日々の風景を描く。".into(),
                "The quick brown fox jumps over the lazy dog. 0123456789".into(),
            ],
            size: 24,
            source: "installed".into(),
            locale: "ja".into(),
        }
    }
}
fn validate(s: &Settings) -> Result<(), String> {
    if s.title.chars().count() > 100
        || s.samples.is_empty()
        || s.samples.len() > 8
        || s.samples
            .iter()
            .any(|v| v.trim().is_empty() || v.chars().count() > 200)
        || !(12..=36).contains(&s.size)
        || !["installed", "folder"].contains(&s.source.as_str())
    {
        return Err("specimen_invalid".into());
    }
    Ok(())
}
#[derive(Serialize)]
pub struct Report {
    count: usize,
    skipped: usize,
    truncated: bool,
}
#[tauri::command]
pub fn specimen_settings(window: WebviewWindow) -> Result<Settings, String> {
    crate::allowed(&window)?;
    let path = window
        .app_handle()
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("specimen-settings.json");
    Ok(std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .filter(|s| validate(s).is_ok())
        .unwrap_or_default())
}
#[tauri::command]
pub async fn specimen_create(
    window: WebviewWindow,
    settings: Settings,
) -> Result<Option<Report>, String> {
    crate::allowed(&window)?;
    validate(&settings)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let locations = if settings.source == "folder" {
        let Some(folder) = rfd::AsyncFileDialog::new()
            .set_parent(&window)
            .pick_folder()
            .await
        else {
            return Ok(None);
        };
        vec![font_paths::Location {
            source: "file",
            path: folder.path().into(),
            status: "pending".into(),
        }]
    } else {
        font_paths::current()?
            .into_iter()
            .filter(|l| l.source != "adobe")
            .collect()
    };
    let root = window
        .app_handle()
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("specimens");
    let prefs = window
        .app_handle()
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("specimen-settings.json");
    tauri::async_runtime::spawn_blocking(move || {
        let catalog = font_catalog::scan(locations);
        let mut entries = catalog.entries;
        entries.sort_by_key(|e| {
            (
                e.family_ja.clone().unwrap_or_else(|| e.family.clone()),
                e.style.clone(),
                e.path.clone(),
                e.index,
            )
        });
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let dir = root.join(stamp.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut body = String::new();
        let mut styles = String::new();
        let mut skipped = catalog.skipped;
        let mut count = 0;
        let mut bytes = 0usize;
        let mut truncated = catalog.truncated;
        for e in entries {
            if count >= 256 {
                truncated = true;
                break;
            }
            let result = (|| {
                let data = font_core::font_io::read(&e.path)?;
                font_core::specimen::standalone(&data, e.index)
            })();
            let Ok(data) = result else {
                skipped += 1;
                continue;
            };
            bytes += data.len();
            if bytes > 256 * 1024 * 1024 {
                truncated = true;
                break;
            }
            let name = format!("font{count}.otf");
            std::fs::write(dir.join(&name), &data).map_err(|e| e.to_string())?;
            let face = ttf_parser::Face::parse(&data, 0).map_err(|_| "font_invalid")?;
            let english = face
                .names()
                .into_iter()
                .filter(|n| {
                    n.name_id == 1
                        && ((n.platform_id == ttf_parser::PlatformId::Windows
                            && n.language_id == 0x409)
                            || (n.platform_id == ttf_parser::PlatformId::Macintosh
                                && n.language_id == 0))
                })
                .find_map(|n| n.to_string())
                .unwrap_or_else(|| e.family.clone());
            let missing = settings
                .samples
                .iter()
                .flat_map(|s| s.chars())
                .filter(|c| !c.is_whitespace() && face.glyph_index(*c).is_none())
                .count();
            styles.push_str(&format!(
                "@font-face{{font-family:s{count};src:url('{name}');}}"
            ));
            body.push_str(&font_core::specimen::card(
                count,
                &english,
                e.family_ja.as_deref(),
                &e.style,
                &settings.samples,
                missing,
            ));
            count += 1;
        }
        if count == 0 {
            let _ = std::fs::remove_dir_all(&dir);
            return Err("specimen_empty".into());
        }
        let html = font_core::specimen::html(
            &settings.title,
            &styles,
            &body,
            settings.size,
            &settings.locale,
            skipped,
            truncated,
        );
        let path = dir.join("index.html");
        std::fs::write(&path, html).map_err(|e| e.to_string())?;
        if let Some(parent) = prefs.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            prefs,
            serde_json::to_vec(&settings).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        crate::font_library::open_target(path.to_str().ok_or("font_invalid_path")?)?;
        Ok(Some(Report {
            count,
            skipped,
            truncated,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_empty_samples() {
        let mut s = Settings::default();
        assert!(validate(&s).is_ok());
        s.samples = vec![" ".into()];
        assert!(validate(&s).is_err());
        s.samples = vec!["x".repeat(201)];
        assert!(validate(&s).is_err());
        s.samples = vec!["x".into(); 9];
        assert!(validate(&s).is_err());
        s = Settings::default();
        s.size = 100;
        assert!(validate(&s).is_err());
        s = Settings::default();
        s.source = "/etc".into();
        assert!(validate(&s).is_err());
    }
}
