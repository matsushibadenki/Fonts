//! Periodic Rust worker: committed revisions only, no WebView timers or font data IPC.
use crate::{
    recovery_store::{Candidate, Store},
    AppState,
};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, WebviewWindow};
pub struct Recovery {
    inner: Mutex<Runtime>,
    stopped: AtomicBool,
}
struct Runtime {
    store: Store,
    last: Option<(u64, u64)>,
    updated: Option<u64>,
    error: Option<String>,
}
#[derive(Serialize)]
pub struct Status {
    candidates: Vec<Candidate>,
    invalid: usize,
    updated: Option<u64>,
    error: Option<String>,
}
impl Recovery {
    pub fn new(root: std::path::PathBuf) -> Result<Self, String> {
        Ok(Self {
            inner: Mutex::new(Runtime {
                store: Store::new(root)?,
                last: None,
                updated: None,
                error: None,
            }),
            stopped: AtomicBool::new(false),
        })
    }
}
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut observed = None;
        let mut changed = Instant::now();
        let mut flushed = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let recovery = app.state::<Recovery>();
            if recovery.stopped.load(Ordering::Acquire) {
                break;
            }
            let (token, dirty, in_progress) = {
                let state = app.state::<AppState>();
                let Ok(m) = state.0.lock() else {
                    continue;
                };
                (
                    (m.epoch, m.editor.revision()),
                    m.saved_revision != Some(m.editor.revision())
                        && (m.epoch > 0 || m.editor.revision() > 0),
                    m.has_draft(),
                )
            };
            if observed != Some(token) {
                observed = Some(token);
                changed = Instant::now();
            }
            if !due(dirty, in_progress, changed.elapsed(), flushed.elapsed()) {
                continue;
            }
            let Ok(_guard) = crate::files::FileGuard::acquire() else {
                continue;
            };
            if recovery.stopped.load(Ordering::Acquire) {
                break;
            }
            let result = (|| {
                let mut r = recovery.inner.lock().map_err(|_| "state_poisoned")?;
                if dirty {
                    if r.last == Some(token) {
                        return Ok(false);
                    }
                    let (doc, origin) = {
                        let state = app.state::<AppState>();
                        let m = state.0.lock().map_err(|_| "state_poisoned")?;
                        if (m.epoch, m.editor.revision()) != token || m.has_draft() {
                            return Ok(false);
                        }
                        (m.editor.document().clone(), m.path.clone())
                    };
                    r.store.checkpoint(&doc, token.0, token.1, origin)?;
                    r.last = Some(token);
                    r.updated = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    );
                } else if r.last.is_some() {
                    {
                        let state = app.state::<AppState>();
                        let m = state.0.lock().map_err(|_| "state_poisoned")?;
                        if (m.epoch, m.editor.revision()) != token
                            || m.saved_revision != Some(m.editor.revision())
                        {
                            return Ok(false);
                        }
                    }
                    r.store.clear()?;
                    r.last = None;
                    r.updated = None;
                } else {
                    return Ok(false);
                }
                r.error = None;
                Ok::<_, String>(true)
            })();
            match result {
                Ok(true) => {
                    flushed = Instant::now();
                    let _ = app.emit("recovery-changed", ());
                }
                Ok(false) => {}
                Err(error) => {
                    if let Ok(mut r) = recovery.inner.lock() {
                        r.error = Some(error);
                    }
                    let _ = app.emit("recovery-changed", ());
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    });
}
/// Called only after explicit discard/replacement, while file actions are serialized.
pub fn clear_current(app: &tauri::AppHandle) {
    if let Some(recovery) = app.try_state::<Recovery>() {
        if let Ok(mut r) = recovery.inner.lock() {
            match r.store.clear() {
                Ok(()) => {
                    r.last = None;
                    r.updated = None;
                    r.error = None;
                }
                Err(e) => r.error = Some(e),
            }
        }
        let _ = app.emit("recovery-changed", ());
    }
}
pub fn stop(app: &tauri::AppHandle) {
    if let Some(r) = app.try_state::<Recovery>() {
        r.stopped.store(true, Ordering::Release);
    }
    clear_current(app);
}
#[tauri::command]
pub async fn recovery_query(window: WebviewWindow) -> Result<Status, String> {
    crate::allowed(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let recovery = app.state::<Recovery>();
        let r = recovery.inner.lock().map_err(|_| "state_poisoned")?;
        let (candidates, invalid) = r.store.candidates()?;
        Ok(Status {
            candidates,
            invalid,
            updated: r.updated,
            error: r.error.clone(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn recovery_action(
    window: WebviewWindow,
    id: String,
    restore: bool,
) -> Result<bool, String> {
    if window.label() != "main" {
        return Err("main_window_only".into());
    }
    let _guard = crate::files::FileGuard::acquire()?;
    let app = window.app_handle().clone();
    if restore {
        let handle = app.clone();
        let input = id.clone();
        let (document, lease) = tauri::async_runtime::spawn_blocking(move || {
            let state = handle.state::<Recovery>();
            let r = state.inner.lock().map_err(|_| "state_poisoned")?;
            r.store.candidate(&input)
        })
        .await
        .map_err(|e| e.to_string())??;
        if !crate::files::replace(&window, document, None).await? {
            return Ok(false);
        }
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let (doc, epoch, revision) = {
                let state = handle.state::<AppState>();
                let m = state.0.lock().map_err(|_| "state_poisoned")?;
                (m.editor.document().clone(), m.epoch, m.editor.revision())
            };
            let state = handle.state::<Recovery>();
            let mut r = state.inner.lock().map_err(|_| "state_poisoned")?;
            r.store.checkpoint(&doc, epoch, revision, None)?;
            r.last = Some((epoch, revision));
            r.updated = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            r.error = None;
            drop(lease);
            r.store.dismiss(&id)
        })
        .await
        .map_err(|e| e.to_string())??;
    } else {
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let state = handle.state::<Recovery>();
            let r = state.inner.lock().map_err(|_| "state_poisoned")?;
            r.store.dismiss(&id)
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    let _ = app.emit("recovery-changed", ());
    Ok(true)
}

fn due(dirty: bool, in_progress: bool, quiet: Duration, since: Duration) -> bool {
    !in_progress && (!dirty || quiet >= Duration::from_secs(3) || since >= Duration::from_secs(30))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debounce_continuous_edit_and_draft_exclusion() {
        assert!(!due(
            true,
            false,
            Duration::from_secs(2),
            Duration::from_secs(5)
        ));
        assert!(due(
            true,
            false,
            Duration::from_secs(3),
            Duration::from_secs(5)
        ));
        assert!(due(true, false, Duration::ZERO, Duration::from_secs(30)));
        assert!(!due(
            true,
            true,
            Duration::from_secs(60),
            Duration::from_secs(60)
        ));
        assert!(due(false, false, Duration::ZERO, Duration::ZERO));
    }
}
