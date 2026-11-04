use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use tauri::WebviewWindow;
#[derive(Clone, Serialize)]
pub struct Row {
    path: PathBuf,
    postscript: String,
    cff: String,
    status: String,
}
#[derive(Clone, Serialize)]
pub struct Report {
    generation: u64,
    rows: Vec<Row>,
    truncated: bool,
}
static REPORT: Mutex<Option<Report>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
#[derive(Clone, Default, Serialize)]
pub struct Progress {
    running: bool,
    phase: String,
    completed: usize,
    total: Option<usize>,
    discovered: usize,
    current: Option<PathBuf>,
}
static PROGRESS: Mutex<Progress> = Mutex::new(Progress {
    running: false,
    phase: String::new(),
    completed: 0,
    total: None,
    discovered: 0,
    current: None,
});
static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct Operation;
impl Operation {
    fn begin(phase: &str) -> Result<Self, String> {
        RUNNING
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map_err(|_| "name_busy")?;
        let operation = Self;
        *PROGRESS.lock().map_err(|_| "state_poisoned")? = Progress {
            running: true,
            phase: phase.into(),
            ..Default::default()
        };
        Ok(operation)
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut p) = PROGRESS.lock() {
            p.running = false;
            p.current = None;
        }
        RUNNING.store(false, std::sync::atomic::Ordering::Release);
    }
}
fn progress(update: impl FnOnce(&mut Progress)) {
    if let Ok(mut p) = PROGRESS.lock() {
        update(&mut p)
    }
}
#[tauri::command]
pub fn font_names_progress(window: WebviewWindow) -> Result<Progress, String> {
    crate::allowed(&window)?;
    Ok(PROGRESS.lock().map_err(|_| "state_poisoned")?.clone())
}
#[tauri::command]
pub fn font_names_clear(window: WebviewWindow) -> Result<(), String> {
    crate::allowed(&window)?;
    clear_history()
}
fn clear_history() -> Result<(), String> {
    let _operation = Operation::begin("clearing")?;
    *REPORT.lock().map_err(|_| "state_poisoned")? = None;
    *PROGRESS.lock().map_err(|_| "state_poisoned")? = Progress::default();
    Ok(())
}
#[tauri::command]
pub async fn font_names_scan(window: WebviewWindow) -> Result<Option<Report>, String> {
    crate::allowed(&window)?;
    let operation = Operation::begin("choosing")?;
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .pick_folder()
        .await
    else {
        return Ok(None);
    };
    let folder = folder.path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        progress(|p| p.phase = "discovering".into());
        let mut paths = vec![];
        let mut pending = vec![(folder, 0)];
        let mut truncated = false;
        let mut examined = 0;
        while let Some((dir, depth)) = pending.pop() {
            progress(|p| p.current = Some(dir.clone()));
            for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
                examined += 1;
                if examined > 20000 {
                    truncated = true;
                    pending.clear();
                    break;
                }
                let entry = entry.map_err(|e| e.to_string())?;
                let ty = entry.file_type().map_err(|e| e.to_string())?;
                let path = entry.path();
                if ty.is_symlink() {
                    continue;
                }
                if ty.is_dir() && entry.file_name() != "Fonts-NameFixed" {
                    if depth < 8 {
                        pending.push((path, depth + 1))
                    } else {
                        truncated = true
                    }
                } else if ty.is_file()
                    && path.extension().is_some_and(|e| {
                        ["otf", "ttf", "ttc", "otc", "woff2"]
                            .contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
                    })
                {
                    paths.push(path);
                    progress(|p| p.discovered = paths.len());
                }
                if paths.len() + pending.len() >= 1000 {
                    truncated = true;
                    break;
                }
            }
            if truncated && paths.len() + pending.len() >= 1000 {
                break;
            }
        }
        paths.sort();
        progress(|p| {
            p.phase = "scanning".into();
            p.total = Some(paths.len());
            p.current = None;
        });
        let mut rows = vec![];
        let mut total = 0u64;
        for path in paths {
            progress(|p| p.current = Some(path.clone()));
            let mut row = Row {
                path: path.clone(),
                postscript: String::new(),
                cff: String::new(),
                status: "unsupported".into(),
            };
            if let Ok(meta) = std::fs::metadata(&path) {
                total = total.saturating_add(meta.len());
                if total > 512 * 1024 * 1024 {
                    truncated = true;
                    break;
                }
            }
            match font_core::font_io::read(&path).and_then(|b| font_core::font_names::inspect(&b)) {
                Ok(Some(i)) => {
                    row.postscript = i.postscript;
                    row.cff = i.cff;
                    row.status = if i.mismatch { "mismatch" } else { "matched" }.into()
                }
                Ok(None) => {}
                Err(_) => row.status = "invalid".into(),
            }
            rows.push(row);
            progress(|p| p.completed = rows.len());
        }
        let report = Report {
            generation: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            rows,
            truncated,
        };
        *REPORT.lock().map_err(|_| "state_poisoned")? = Some(report.clone());
        progress(|p| {
            p.phase = "complete".into();
            p.total = Some(report.rows.len());
            p.completed = report.rows.len();
        });
        Ok(Some(report))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn font_names_repair(window: WebviewWindow, generation: u64) -> Result<Report, String> {
    crate::allowed(&window)?;
    let operation = Operation::begin("repairing")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        let mut guard = REPORT.lock().map_err(|_| "state_poisoned")?;
        let report = guard
            .as_mut()
            .filter(|r| r.generation == generation)
            .ok_or("name_expired")?;
        progress(|p| {
            p.total = Some(
                report
                    .rows
                    .iter()
                    .filter(|r| r.status == "mismatch")
                    .count(),
            )
        });
        for row in &mut report.rows {
            if row.status != "mismatch" {
                continue;
            }
            progress(|p| p.current = Some(row.path.clone()));
            let result = (|| -> Result<(), String> {
                let bytes = font_core::font_io::read(&row.path)?;
                let current = font_core::font_names::inspect(&bytes)?.ok_or("name_invalid")?;
                if current.postscript != row.postscript || current.cff != row.cff {
                    return Err("name_changed".into());
                }
                let fixed = font_core::font_names::repair(&bytes)?;
                let folder = row
                    .path
                    .parent()
                    .ok_or("name_invalid")?
                    .join("Fonts-NameFixed");
                if folder.exists() {
                    if std::fs::symlink_metadata(&folder)
                        .map_err(|e| e.to_string())?
                        .file_type()
                        .is_symlink()
                    {
                        return Err("name_invalid".into());
                    }
                } else {
                    std::fs::create_dir(&folder).map_err(|e| e.to_string())?
                }
                let output = folder.join(row.path.file_name().ok_or("name_invalid")?);
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&output)
                    .map_err(|e| e.to_string())?;
                if let Err(e) = file.write_all(&fixed).and_then(|_| file.sync_all()) {
                    drop(file);
                    let _ = std::fs::remove_file(&output);
                    return Err(e.to_string());
                }
                if std::fs::read(&output).map_err(|e| e.to_string())? != fixed {
                    return Err("name_invalid".into());
                }
                Ok(())
            })();
            row.status = if result.is_ok() { "fixed" } else { "failed" }.into();
            progress(|p| p.completed += 1);
        }
        progress(|p| p.phase = "complete".into());
        Ok(report.clone())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_is_readable_while_running_and_clear_cannot_race_an_operation() {
        let operation = Operation::begin("discovering").unwrap();
        progress(|p| {
            p.discovered = 3;
            p.current = Some(PathBuf::from("font.otf"));
        });
        let p = PROGRESS.lock().unwrap().clone();
        assert!(p.running);
        assert_eq!(p.discovered, 3);
        assert!(clear_history().is_err());
        assert!(Operation::begin("repairing").is_err());
        progress(|p| {
            p.phase = "scanning".into();
            p.total = Some(3);
            p.completed = 2;
        });
        assert_eq!(PROGRESS.lock().unwrap().completed, 2);
        drop(operation);
        assert!(!PROGRESS.lock().unwrap().running);
        assert!(PROGRESS.lock().unwrap().current.is_none());
        *REPORT.lock().unwrap() = Some(Report {
            generation: 42,
            rows: vec![],
            truncated: false,
        });
        clear_history().unwrap();
        assert!(REPORT.lock().unwrap().is_none());
        assert_eq!(PROGRESS.lock().unwrap().completed, 0);
        assert!(!RUNNING.load(std::sync::atomic::Ordering::Acquire));
        // Error/cancellation paths release the guard and permit another operation.
        let another = Operation::begin("choosing").unwrap();
        drop(another);
        assert!(Operation::begin("repairing").is_ok());
    }
}
