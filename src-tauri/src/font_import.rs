//! File bytes and collection selection stay in Rust until an editable document is ready.
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, WebviewWindow};
#[derive(Clone, Serialize)]
pub struct FaceChoice {
    index: u32,
    family: String,
    style: String,
    glyphs: u16,
}
#[derive(Clone, Serialize)]
pub struct Choices {
    error: Option<String>,
    id: u64,
    filename: String,
    faces: Vec<FaceChoice>,
}
struct Source {
    id: u64,
    owner: String,
    bytes: Arc<Vec<u8>>,
    count: u32,
    components: bool,
}
static SOURCE: Mutex<Option<Source>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);
const EXTENSIONS: &[&str] = &[
    "ttf", "otf", "ttc", "otc", "woff2", "pfa", "pfb", "cff", "cid",
];
fn prepare(path: &Path) -> Result<(Vec<u8>, Vec<FaceChoice>), String> {
    let raw = font_core::font_io::read(path)?;
    let bytes = if raw.starts_with(b"wOF2") {
        font_core::font_formats::woff2::decode(&raw)?
    } else {
        raw
    };
    let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    if count > 256 {
        return Err("font_too_large".into());
    }
    let mut faces = vec![];
    if count > 1 {
        for index in 0..count {
            let f = ttf_parser::Face::parse(&bytes, index).map_err(|_| "font_invalid")?;
            faces.push(FaceChoice {
                index,
                family: font_core::font_io::name(&f, 1),
                style: font_core::font_io::name(&f, 2),
                glyphs: f.number_of_glyphs(),
            });
        }
    }
    Ok((bytes, faces))
}
pub(crate) async fn open_path(window: &WebviewWindow, path: PathBuf) -> Result<(), String> {
    open_path_mode(window, path, false).await
}
async fn open_path_mode(
    window: &WebviewWindow,
    path: PathBuf,
    components: bool,
) -> Result<(), String> {
    let input = path.clone();
    let (bytes, faces) = tauri::async_runtime::spawn_blocking(move || prepare(&input))
        .await
        .map_err(|e| e.to_string())??;
    if faces.len() > 1 {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let count = faces.len() as u32;
        let choices = Choices {
            error: None,
            id,
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            faces,
        };
        *SOURCE.lock().map_err(|_| "state_poisoned")? = Some(Source {
            id,
            owner: window.label().into(),
            bytes: Arc::new(bytes),
            count,
            components,
        });
        window
            .emit_to(window.label(), "font-import-choices", Some(choices))
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let bytes = Arc::new(bytes);
    let work_bytes = bytes.clone();
    let doc = tauri::async_runtime::spawn_blocking(move || {
        import_document(&work_bytes, 0, components, false)
    })
    .await
    .map_err(|e| e.to_string())?;
    let doc = match doc {
        Ok(doc) => doc,
        Err(error) if error.starts_with("layout_") => {
            let face = ttf_parser::Face::parse(&bytes, 0).map_err(|_| "font_invalid")?;
            let faces = vec![FaceChoice {
                index: 0,
                family: font_core::font_io::name(&face, 1),
                style: font_core::font_io::name(&face, 2),
                glyphs: face.number_of_glyphs(),
            }];
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            *SOURCE.lock().map_err(|_| "state_poisoned")? = Some(Source {
                id,
                owner: window.label().into(),
                bytes: bytes.clone(),
                count: 1,
                components,
            });
            let choices = Choices {
                id,
                filename: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                faces,
                error: Some(error),
            };
            window
                .emit_to(window.label(), "font-import-choices", Some(choices))
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    crate::files::add_document(window, doc, None).await?;
    *SOURCE.lock().map_err(|_| "state_poisoned")? = None;
    window
        .emit_to(
            window.label(),
            "font-import-choices",
            Option::<Choices>::None,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn import_document(
    bytes: &[u8],
    index: u32,
    components: bool,
    outlines_only: bool,
) -> Result<font_core::FontDocument, String> {
    if components {
        if outlines_only {
            font_core::truetype_components::import_outlines(bytes, index)
        } else {
            font_core::truetype_components::import(bytes, index)
        }
    } else {
        if outlines_only {
            font_core::font_io::import_outlines(bytes, index)
        } else {
            font_core::font_io::import(bytes, index)
        }
    }
}
#[tauri::command]
pub async fn font_import_choose(
    window: WebviewWindow,
    components: Option<bool>,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .add_filter("Font", EXTENSIONS)
        .pick_file()
        .await
    else {
        return Ok(());
    };
    open_path_mode(
        &window,
        file.path().to_path_buf(),
        components.unwrap_or(false),
    )
    .await
}
#[tauri::command]
pub async fn font_import_face(
    window: WebviewWindow,
    id: u64,
    index: u32,
    outlines_only: Option<bool>,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let (bytes, components) = {
        let guard = SOURCE.lock().map_err(|_| "state_poisoned")?;
        let source = guard
            .as_ref()
            .filter(|s| s.id == id && s.owner == window.label())
            .ok_or("font_import_expired")?;
        if index >= source.count {
            return Err("font_invalid".into());
        }
        (source.bytes.clone(), source.components)
    };
    let doc = tauri::async_runtime::spawn_blocking(move || {
        import_document(&bytes, index, components, outlines_only.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())??;
    crate::files::add_document(&window, doc, None).await?;
    release(&window, id)
}
fn release(window: &WebviewWindow, id: u64) -> Result<(), String> {
    let mut guard = SOURCE.lock().map_err(|_| "state_poisoned")?;
    if guard
        .as_ref()
        .is_some_and(|s| s.id == id && s.owner == window.label())
    {
        *guard = None
    }
    Ok(())
}
#[tauri::command]
pub fn font_import_release(window: WebviewWindow, id: u64) -> Result<(), String> {
    crate::allowed(&window)?;
    release(&window, id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepares_woff2_and_collections_without_filtering_glyphs() {
        let root = std::env::temp_dir().join(format!("fonts-import-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bytes = include_bytes!("../../crates/font-core/tests/data/color/collection.ttc");
        let file = root.join("collection.ttc");
        std::fs::write(&file, bytes).unwrap();
        let (_, faces) = prepare(&file).unwrap();
        assert_eq!(faces.len(), 2);
        assert_eq!(faces[0].index, 0);
        assert_eq!(faces[1].index, 1);
        let mut doc = font_core::FontDocument::new("Import Test", "A😀").unwrap();
        doc.glyphs.get_mut(&2).unwrap().codepoints.clear();
        doc.glyphs.get_mut(&1).unwrap().codepoints.insert('Ｂ');
        let bytes =
            font_core::font_formats::export(&doc, font_core::font_formats::Format::Woff2).unwrap();
        let file = root.join("test.woff2");
        std::fs::write(&file, bytes).unwrap();
        let (bytes, faces) = prepare(&file).unwrap();
        assert!(faces.is_empty());
        let imported = font_core::font_io::import(&bytes, 0).unwrap();
        assert_eq!(imported.glyphs.len(), doc.glyphs.len());
        assert!(imported.glyphs[&2].codepoints.is_empty());
        assert!(imported.glyphs[&1].codepoints.contains(&'Ｂ'));
        assert!(imported.glyphs.contains_key(&0));
        std::fs::remove_dir_all(root).unwrap();
    }
}
