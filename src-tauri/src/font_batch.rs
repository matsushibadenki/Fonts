//! Sequential, bounded font conversion. No font bytes cross IPC.
use crate::{font_catalog::Entry, font_library::Library};
use font_core::{
    font_formats::{self, Format},
    font_io,
};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Emitter, Manager, WebviewWindow};
static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn busy() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Acquire)
}
struct Guard;
impl Guard {
    fn acquire() -> Result<Self, String> {
        ACTIVE
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map(|_| Self)
            .map_err(|_| "file_operation_in_progress".into())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    }
}
#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub running: bool,
    pub cancelled: bool,
    pub cancel_requested: bool,
    pub total: usize,
    pub completed: usize,
    pub current: String,
    pub directory: Option<PathBuf>,
    pub results: Vec<Item>,
}
#[derive(Clone, Serialize)]
pub struct Item {
    pub source: PathBuf,
    pub family: String,
    pub style: String,
    pub index: u32,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}
#[derive(Default)]
pub struct Batch(pub Mutex<Runtime>);
#[derive(Default)]
pub struct Runtime {
    pub status: Status,
    pub cancel: bool,
}
fn stem(family: &str, style: &str, index: u32) -> String {
    let s: String = format!("{family} {style} face-{index}")
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        .take(100)
        .collect();
    let s = s.trim().trim_end_matches('.');
    if s.is_empty() {
        "Font".into()
    } else {
        format!("Font-{s}")
    }
}
fn directory(parent: &Path) -> Result<PathBuf, String> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for n in 0..10000 {
        let p = parent.join(format!("Fonts-converted-{time}-{n}"));
        match fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("font_output_exists".into())
}
fn write_unique(dir: &Path, name: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    for n in 0..10000 {
        let file = if n == 0 {
            format!("{name}.{ext}")
        } else {
            format!("{name}-{n}.{ext}")
        };
        let path = dir.join(file);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                let result = f.write_all(bytes).and_then(|_| f.sync_all());
                drop(f);
                if let Err(e) = result {
                    let _ = fs::remove_file(&path);
                    return Err(e.to_string());
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("font_output_exists".into())
}
fn convert(entry: &Entry, format: Format) -> Result<Vec<u8>, String> {
    if !entry.outline {
        return Err("font_no_outlines".into());
    }
    let mut doc = font_io::import(&font_io::read(&entry.path)?, entry.index)?;
    if let Some(family) = doc.family.strip_suffix(" Edited") {
        doc.family = family.to_owned();
    }
    font_formats::export(&doc, format)
}
#[tauri::command]
pub fn font_batch_query(window: WebviewWindow) -> Result<Status, String> {
    crate::allowed(&window)?;
    Ok(window
        .state::<Batch>()
        .0
        .lock()
        .map_err(|_| "state_poisoned")?
        .status
        .clone())
}
#[tauri::command]
pub fn font_batch_cancel(window: WebviewWindow) -> Result<(), String> {
    crate::allowed(&window)?;
    {
        let state = window.state::<Batch>();
        let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
        if r.status.running {
            r.cancel = true;
            r.status.cancel_requested = true;
        }
    }
    let _ = window.app_handle().emit("font-batch-changed", ());
    Ok(())
}
#[tauri::command]
pub async fn font_batch_start(
    window: WebviewWindow,
    ids: Vec<u64>,
    format: Format,
) -> Result<(), String> {
    crate::allowed(&window)?;
    let _guard = Guard::acquire()?;
    if ids.is_empty() || ids.len() > 256 {
        return Err("font_batch_limit".into());
    }
    let app = window.app_handle().clone();
    let entries = {
        let state = app.state::<Library>();
        let state = state.0.lock().map_err(|_| "state_poisoned")?;
        let unique: std::collections::BTreeSet<_> = ids.into_iter().collect();
        unique
            .into_iter()
            .map(|id| {
                state
                    .catalog
                    .entries
                    .iter()
                    .find(|e| e.id == id)
                    .cloned()
                    .ok_or_else(|| "font_not_found".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .pick_folder()
        .await
    else {
        return Ok(());
    };
    let parent = folder.path().to_path_buf();
    if crate::font_library::protected(&parent.join("batch-output"))? {
        return Err("font_protected_location".into());
    }
    let dir = directory(&parent)?;
    {
        let state = app.state::<Batch>();
        let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
        *r = Runtime {
            status: Status {
                running: true,
                total: entries.len(),
                directory: Some(dir.clone()),
                ..Default::default()
            },
            cancel: false,
        };
    }
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<Batch>();
        run(&state.0, entries, format, &dir, || {
            let _ = handle.emit("font-batch-changed", ());
        })?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    {
        let state = app.state::<Batch>();
        let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
        r.status.running = false;
        r.status.current.clear();
    }
    let _ = app.emit("font-batch-changed", ());
    result
}
fn run(
    runtime: &Mutex<Runtime>,
    entries: Vec<Entry>,
    format: Format,
    dir: &Path,
    progress: impl Fn(),
) -> Result<(), String> {
    for entry in entries {
        {
            let mut r = runtime.lock().map_err(|_| "state_poisoned")?;
            if r.cancel {
                r.status.cancelled = true;
                break;
            }
            r.status.current = format!("{} {}", entry.family, entry.style);
        }
        progress();
        let bytes = convert(&entry, format);
        {
            let mut r = runtime.lock().map_err(|_| "state_poisoned")?;
            if r.cancel {
                r.status.cancelled = true;
                break;
            }
        }
        let outcome = bytes.and_then(|bytes| {
            write_unique(
                dir,
                &stem(&entry.family, &entry.style, entry.index),
                format.extension(),
                &bytes,
            )
        });
        let item = match outcome {
            Ok(path) => Item {
                source: entry.path,
                family: entry.family,
                style: entry.style,
                index: entry.index,
                path: Some(path),
                error: None,
            },
            Err(error) => Item {
                source: entry.path,
                family: entry.family,
                style: entry.style,
                index: entry.index,
                path: None,
                error: Some(error),
            },
        };
        {
            let mut r = runtime.lock().map_err(|_| "state_poisoned")?;
            r.status.results.push(item);
            r.status.completed += 1;
        }
        progress();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_do_not_escape_or_overwrite_and_failures_keep_sources() {
        let root = std::env::temp_dir().join(format!(
            "fonts-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let dir = directory(&root).unwrap();
        let name = stem("../CON: 日本語/中文", "Bold*", 0);
        assert!(!name.contains('/'));
        let a = write_unique(&dir, &name, "otf", b"one").unwrap();
        let b = write_unique(&dir, &name, "otf", b"two").unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read(a).unwrap(), b"one");
        assert_eq!(fs::read(b).unwrap(), b"two");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn converting_faces_uses_the_requested_format() {
        let root =
            std::env::temp_dir().join(format!("fonts-batch-source-{}.ttf", std::process::id()));
        let doc = font_core::FontDocument::new("Batch QA", "A中").unwrap();
        let original = font_io::export(&doc).unwrap();
        fs::write(&root, &original).unwrap();
        let e = Entry {
            id: 1,
            family: "Batch QA".into(),
            family_ja: None,
            style_ja: None,
            search_text: String::new(),
            style: "Regular".into(),
            format: "TrueType".into(),
            cid: false,
            path: root.clone(),
            index: 0,
            source: "file".into(),
            glyphs: 3,
            variable: false,
            outline: true,
        };
        let output = convert(&e, Format::Otf).unwrap();
        assert_eq!(&output[..4], b"OTTO");
        assert_eq!(fs::read(&root).unwrap(), original);
        let mut bad = e.clone();
        bad.index = 99;
        assert!(convert(&bad, Format::Otf).is_err());
        fs::remove_file(root).unwrap();
    }
    #[test]
    fn batch_continues_after_errors_and_cancel_keeps_completed_outputs() {
        let root = std::env::temp_dir().join(format!(
            "fonts-batch-loop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let source = root.join("source.ttf");
        let bytes =
            font_io::export(&font_core::FontDocument::new("Batch loop", "A").unwrap()).unwrap();
        fs::write(&source, &bytes).unwrap();
        let good = Entry {
            id: 1,
            family: "Batch loop".into(),
            family_ja: None,
            style_ja: None,
            search_text: String::new(),
            style: "Regular".into(),
            format: "TrueType".into(),
            cid: false,
            path: source.clone(),
            index: 0,
            source: "file".into(),
            glyphs: 2,
            variable: false,
            outline: true,
        };
        let mut bad = good.clone();
        bad.index = 9;
        let dir = directory(&root).unwrap();
        let runtime = Mutex::new(Runtime::default());
        run(
            &runtime,
            vec![bad, good.clone(), good.clone()],
            Format::Otf,
            &dir,
            || {},
        )
        .unwrap();
        {
            let r = runtime.lock().unwrap();
            assert_eq!(r.status.completed, 3);
            assert!(r.status.results[0].error.is_some());
            assert!(r.status.results[1].error.is_none());
            assert_ne!(r.status.results[1].path, r.status.results[2].path);
        }
        let dir = directory(&root).unwrap();
        let runtime = Mutex::new(Runtime::default());
        run(
            &runtime,
            vec![good.clone(), good],
            Format::Otf,
            &dir,
            || {
                let mut r = runtime.lock().unwrap();
                if r.status.completed == 1 {
                    r.cancel = true;
                }
            },
        )
        .unwrap();
        let r = runtime.lock().unwrap();
        assert!(r.status.cancelled);
        assert_eq!(r.status.completed, 1);
        assert!(r.status.results[0].path.as_ref().unwrap().is_file());
        assert_eq!(fs::read(source).unwrap(), bytes);
        drop(r);
        fs::remove_dir_all(root).unwrap();
    }
}
