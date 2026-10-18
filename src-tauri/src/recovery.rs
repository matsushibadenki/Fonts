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
struct Checkpoint {
    store: Store,
    last: Option<u64>,
}
struct Runtime {
    store: Store,
    root: std::path::PathBuf,
    documents: std::collections::BTreeMap<u64, Checkpoint>,
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
                store: Store::new(root.clone())?,
                root,
                documents: Default::default(),
                updated: None,
                error: None,
            }),
            stopped: AtomicBool::new(false),
        })
    }
}
fn checkpoint(app: &tauri::AppHandle, r: &mut Runtime, id: u64) -> Result<bool, String> {
    let snapshot = {
        let state = app.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        let Some(d) = m.document(id) else {
            return Ok(false);
        };
        if d.has_draft() {
            return Ok(false);
        }
        if !d.unsaved() {
            if let Some(c) = r.documents.get_mut(&id) {
                if c.last.is_some() {
                    c.store.clear()?;
                    c.last = None;
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if r.documents
            .get(&id)
            .is_some_and(|c| c.last == Some(d.editor.revision()))
        {
            return Ok(false);
        }
        (
            d.editor.document().clone(),
            d.editor.revision(),
            d.path.clone(),
        )
    };
    if !r.documents.contains_key(&id) {
        r.documents.insert(
            id,
            Checkpoint {
                store: Store::new(r.root.clone())?,
                last: None,
            },
        );
    }
    let c = r.documents.get_mut(&id).ok_or("recovery_unavailable")?;
    c.store
        .checkpoint(&snapshot.0, id, snapshot.1, snapshot.2)?;
    c.last = Some(snapshot.1);
    r.updated = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );
    r.error = None;
    Ok(true)
}
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut observed = std::collections::BTreeMap::<u64, (u64, Instant, Instant)>::new();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let recovery = app.state::<Recovery>();
            if recovery.stopped.load(Ordering::Acquire) {
                break;
            }
            let docs = {
                let state = app.state::<AppState>();
                let Ok(m) = state.0.lock() else {
                    continue;
                };
                m.all_documents()
                    .map(|d| {
                        (
                            d.document_id,
                            d.editor.revision(),
                            d.unsaved(),
                            d.has_draft(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            observed.retain(|id, _| docs.iter().any(|d| d.0 == *id));
            for (id, revision, dirty, draft) in docs {
                let o = observed
                    .entry(id)
                    .or_insert((revision, Instant::now(), Instant::now()));
                if o.0 != revision {
                    o.0 = revision;
                    o.1 = Instant::now();
                }
                if !due(dirty, draft, o.1.elapsed(), o.2.elapsed()) {
                    continue;
                }
                let Ok(_guard) = crate::files::FileGuard::acquire() else {
                    continue;
                };
                if recovery.stopped.load(Ordering::Acquire) {
                    break;
                }
                let Ok(mut r) = recovery.inner.lock() else {
                    continue;
                };
                match checkpoint(&app, &mut r, id) {
                    Ok(true) => {
                        o.2 = Instant::now();
                        let _ = app.emit("recovery-changed", ());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        r.error = Some(e);
                        o.1 = Instant::now();
                        let _ = app.emit("recovery-changed", ());
                    }
                }
            }
        }
    });
}
pub fn clear_document(app: &tauri::AppHandle, id: u64) {
    if let Some(recovery) = app.try_state::<Recovery>() {
        if let Ok(mut r) = recovery.inner.lock() {
            if let Some(c) = r.documents.get(&id) {
                match c.store.clear() {
                    Ok(()) => {
                        r.documents.remove(&id);
                    }
                    Err(e) => r.error = Some(e),
                }
            }
            let _ = app.emit("recovery-changed", ());
        }
    }
}
pub fn stop(app: &tauri::AppHandle) {
    if let Some(recovery) = app.try_state::<Recovery>() {
        recovery.stopped.store(true, Ordering::Release);
        if let Ok(mut r) = recovery.inner.lock() {
            for c in r.documents.values() {
                let _ = c.store.clear();
            }
            r.documents.clear();
        }
    }
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
        let doc_id = crate::files::add_document(&window, document, None).await?;
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let state = handle.state::<Recovery>();
            let mut r = state.inner.lock().map_err(|_| "state_poisoned")?;
            checkpoint(&handle, &mut r, doc_id)?;
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
