//! Signed update state belongs to Rust. Only version/progress crosses IPC.
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};
static INSTALLING: AtomicBool = AtomicBool::new(false);
pub fn installing() -> bool {
    INSTALLING.load(Ordering::Acquire)
}
#[derive(Clone, Serialize)]
pub struct Status {
    pub current: String,
    pub phase: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    pub supported: bool,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            current: env!("CARGO_PKG_VERSION").into(),
            phase: "idle".into(),
            version: None,
            notes: None,
            downloaded: 0,
            total: None,
            error: None,
            supported: cfg!(target_os = "macos"),
        }
    }
}
#[derive(Default)]
pub struct Runtime {
    status: Status,
    update: Option<Arc<Update>>,
    bytes: Option<Vec<u8>>,
}
#[derive(Default)]
pub struct Updates(pub Mutex<Runtime>);
impl Updates {
    pub fn new(current: String) -> Self {
        Self(Mutex::new(Runtime {
            status: Status {
                current,
                ..Default::default()
            },
            ..Default::default()
        }))
    }
}
fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        Err("main_window_only".into())
    } else {
        Ok(())
    }
}
fn publish(app: &tauri::AppHandle) {
    let _ = app.emit("updater-changed", ());
}
fn failed(app: &tauri::AppHandle, code: &str) {
    if let Ok(mut r) = app.state::<Updates>().0.lock() {
        r.status.phase = "error".into();
        r.status.error = Some(code.into());
    }
    publish(app);
}
#[tauri::command]
pub fn updater_status(window: WebviewWindow) -> Result<Status, String> {
    main_only(&window)?;
    let state = window.state::<Updates>();
    let r = state.0.lock().map_err(|_| "state_poisoned")?;
    Ok(r.status.clone())
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Check,
    Download,
    Install,
}
#[tauri::command]
pub async fn updater_action(window: WebviewWindow, operation: Operation) -> Result<(), String> {
    main_only(&window)?;
    let app = window.app_handle().clone();
    if matches!(operation, Operation::Install) {
        return install(window).await;
    }
    let update = {
        let state = app.state::<Updates>();
        let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
        if matches!(
            r.status.phase.as_str(),
            "checking" | "downloading" | "installing"
        ) {
            return Err("update_busy".into());
        }
        match operation {
            Operation::Check => {
                r.status = Status {
                    current: r.status.current.clone(),
                    phase: "checking".into(),
                    ..Default::default()
                };
                r.update = None;
                r.bytes = None;
                None
            }
            Operation::Download => {
                let update = r.update.clone().ok_or("update_check_first")?;
                r.bytes = None;
                r.status.phase = "downloading".into();
                r.status.error = None;
                r.status.downloaded = 0;
                r.status.total = None;
                Some(update)
            }
            _ => unreachable!(),
        }
    };
    publish(&app);
    if let Some(update) = update {
        let handle = app.clone();
        let mut last = std::time::Instant::now();
        match update
            .download(
                move |chunk, total| {
                    if let Ok(mut r) = handle.state::<Updates>().0.lock() {
                        r.status.downloaded = r.status.downloaded.saturating_add(chunk as u64);
                        r.status.total = total;
                    }
                    if last.elapsed().as_millis() >= 100 {
                        publish(&handle);
                        last = std::time::Instant::now();
                    }
                },
                || {},
            )
            .await
        {
            Ok(bytes) => {
                let state = app.state::<Updates>();
                let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
                r.bytes = Some(bytes);
                r.status.phase = "ready".into();
            }
            Err(e) => {
                eprintln!("update download failed: {e}");
                failed(&app, "update_download_failed");
                return Ok(());
            }
        }
    } else {
        let result = match app
            .updater_builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
        {
            Ok(u) => u.check().await,
            Err(e) => Err(e),
        };
        match result {
            Ok(Some(update)) => {
                if !trusted_url(update.download_url.as_str()) {
                    failed(&app, "update_source_invalid");
                    return Ok(());
                }
                let state = app.state::<Updates>();
                let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
                r.status.version = Some(update.version.clone());
                r.status.notes = update
                    .body
                    .as_ref()
                    .map(|s| s.chars().take(12000).collect());
                r.status.phase = "available".into();
                r.update = Some(Arc::new(update));
            }
            Ok(None) => {
                app.state::<Updates>()
                    .0
                    .lock()
                    .map_err(|_| "state_poisoned")?
                    .status
                    .phase = "current".into();
            }
            Err(e) => {
                eprintln!("update check failed: {e}");
                failed(&app, "update_check_failed");
                return Ok(());
            }
        }
    }
    publish(&app);
    Ok(())
}
fn trusted_url(url: &str) -> bool {
    url.starts_with("https://github.com/matsushibadenki/Fonts/releases/download/")
}
fn clean(model: &crate::model::Model) -> bool {
    model
        .all_documents()
        .all(|m| !m.unsaved() && !m.has_draft())
}
async fn install(window: WebviewWindow) -> Result<(), String> {
    let (tx, rx) = futures_channel::oneshot::channel();
    let app = window.app_handle().clone();
    window
        .run_on_main_thread(move || {
            let result = (|| {
                if !cfg!(target_os = "macos") {
                    return Err("update_platform_pending".into());
                }
                let guard = crate::files::FileGuard::acquire()?;
                if crate::font_batch::busy() {
                    return Err("file_operation_in_progress".into());
                }
                let state = app.state::<Updates>();
                let mut r = state.0.lock().map_err(|_| "state_poisoned")?;
                if r.status.phase != "ready" {
                    return Err("update_download_first".into());
                }
                let update = r.update.clone().ok_or("update_download_first")?;
                let model = app.state::<AppState>();
                let m = model.0.lock().map_err(|_| "state_poisoned")?;
                if !clean(&m) {
                    return Err("update_unsaved".into());
                }
                let bytes = r.bytes.take().ok_or("update_download_first")?;
                INSTALLING.store(true, Ordering::Release);
                r.status.phase = "installing".into();
                publish(&app);
                let result = update.install(&bytes);
                if let Err(e) = result {
                    INSTALLING.store(false, Ordering::Release);
                    r.status.phase = "ready".into();
                    r.bytes = Some(bytes);
                    eprintln!("update install failed: {e}");
                    return Err("update_install_failed".into());
                }
                drop(m);
                drop(r);
                crate::persist(&app);
                crate::floating::flush(&app);
                crate::EXIT_CONFIRMED.store(true, Ordering::Relaxed);
                drop(guard);
                app.restart();
                #[allow(unreachable_code)]
                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    rx.await.map_err(|_| "update_install_failed".to_owned())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_are_exact_repository_assets() {
        assert!(trusted_url(
            "https://github.com/matsushibadenki/Fonts/releases/download/v0.1.2/Fonts.app.tar.gz"
        ));
        for url in [
            "http://github.com/matsushibadenki/Fonts/releases/download/v1/a",
            "https://github.com.evil.test/matsushibadenki/Fonts/releases/download/v1/a",
            "https://github.com/another/Fonts/releases/download/v1/a",
        ] {
            assert!(!trusted_url(url));
        }
    }
    #[test]
    fn all_documents_must_be_clean_before_install() {
        let mut m = crate::model::Model::new(Default::default());
        assert!(clean(&m));
        m.open_document(font_core::FontDocument::new("New", "A").unwrap(), None)
            .unwrap();
        assert!(!clean(&m));
        m.activate_document(1).unwrap();
        assert!(!clean(&m));
    }
    #[test]
    fn default_status_is_not_a_successful_check() {
        let s = Status::default();
        assert_eq!(s.phase, "idle");
        assert!(s.version.is_none());
        assert!(s.error.is_none());
    }
}
