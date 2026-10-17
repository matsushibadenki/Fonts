//! Clipboard payloads stay native; the WebView sends only a revision guard.
use crate::AppState;
use tauri::{Manager, WebviewWindow};
static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        BUSY.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[tauri::command]
pub async fn paste_paths(
    window: WebviewWindow,
    epoch: u64,
    revision: u64,
    glyph: font_core::GlyphId,
) -> Result<(), String> {
    if window.label() != "main" && !window.label().starts_with("panel-") {
        return Err("invalid_window".into());
    }
    if BUSY
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        return Err("edit_in_progress".into());
    }
    let _guard = Guard;
    let app = window.app_handle().clone();
    {
        let state = app.state::<AppState>();
        let m = state.0.lock().map_err(|_| "state_poisoned")?;
        check(&m, epoch, revision, glyph)?;
    }
    let (tx, rx) = futures_channel::oneshot::channel();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(read());
        })
        .map_err(|e| e.to_string())?;
    let candidates = rx.await.map_err(|_| "clipboard_invalid")??;
    let contours = tauri::async_runtime::spawn_blocking(move || {
        let mut error = "clipboard_unsupported";
        for (kind, bytes) in candidates {
            match font_core::path_paste::decode(&kind, &bytes) {
                Ok(c) => return Ok(c),
                Err(e) => error = e,
            }
        }
        Err(error.to_owned())
    })
    .await
    .map_err(|_| "clipboard_invalid")??;
    {
        let state = app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        check(&m, epoch, revision, glyph)?;
        let inserted = m
            .editor
            .paste_paths(revision, glyph, contours)
            .map_err(str::to_owned)?;
        m.selected = None;
        m.snippets.contours = inserted.collect();
        m.tool = font_core::workspace::Tool::Nodes;
    }
    crate::publish(&app);
    Ok(())
}
fn check(
    m: &crate::model::Model,
    epoch: u64,
    revision: u64,
    glyph: font_core::GlyphId,
) -> Result<(), String> {
    if crate::files::busy() || m.proof.active || m.has_draft() {
        return Err("edit_in_progress".into());
    }
    if m.epoch != epoch || m.editor.revision() != revision || m.glyph != glyph {
        return Err("Conflict".into());
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn read() -> Result<Vec<(String, Vec<u8>)>, String> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;
    let board = NSPasteboard::generalPasteboard();
    let mut out = vec![];
    if let Some(value) = board.stringForType(&NSString::from_str("com.lumapaint.selection.v1")) {
        if value.length() > font_core::path_paste::MAX_BYTES {
            return Err("clipboard_too_large".into());
        }
        let bytes = value.to_string().into_bytes();
        if bytes.len() > font_core::path_paste::MAX_BYTES {
            return Err("clipboard_too_large".into());
        }
        out.push(("lumapaint".to_owned(), bytes));
    }

    for (kind, types) in [
        (
            "svg",
            &[
                "public.svg-image",
                "image/svg+xml",
                "com.adobe.svg",
                "public.utf8-plain-text",
            ][..],
        ),
        ("pdf", &["com.adobe.pdf", "public.pdf"][..]),
        (
            "aicb",
            &[
                "com.adobe.illustrator.aicb",
                "com.adobe.encapsulated-postscript",
                "com.adobe.postscript",
                "CorePasteboardFlavorType 0x41494342",
            ][..],
        ),
    ] {
        for ty in types {
            let ty = NSString::from_str(ty);
            if let Some(data) = board.dataForType(&ty) {
                if data.length() > font_core::path_paste::MAX_BYTES {
                    return Err("clipboard_too_large".into());
                }
                let bytes = data.to_vec();
                if kind == "svg" && !String::from_utf8_lossy(&bytes).contains("<svg") {
                    continue;
                }
                out.push((kind.to_owned(), bytes));
                break;
            }
        }
    }
    if out.is_empty() {
        Err("clipboard_unsupported".into())
    } else {
        Ok(out)
    }
}
#[cfg(not(target_os = "macos"))]
fn read() -> Result<Vec<(String, Vec<u8>)>, String> {
    Err("clipboard_platform_pending".into())
}
#[tauri::command]
pub fn paste_text(window: WebviewWindow) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    let _ = window;
    #[cfg(target_os = "macos")]
    {
        use objc2::{msg_send, runtime::AnyObject};
        use objc2_app_kit::NSApplication;
        window.run_on_main_thread(||{if let Some(mtm)=objc2::MainThreadMarker::new(){let app=NSApplication::sharedApplication(mtm);unsafe{let _:bool=msg_send![&*app, sendAction:objc2::sel!(paste:), to:std::ptr::null::<AnyObject>(), from:std::ptr::null::<AnyObject>()];}}}).map_err(|e|e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paste_guard_rejects_stale_documents_and_modal_edits() {
        let mut m = crate::model::Model::new(Default::default());
        assert!(check(&m, m.epoch, m.editor.revision(), m.glyph).is_ok());
        assert!(check(&m, m.epoch + 1, m.editor.revision(), m.glyph).is_err());
        assert!(check(&m, m.epoch, m.editor.revision() + 1, m.glyph).is_err());
        assert!(check(&m, m.epoch, m.editor.revision(), m.glyph + 1).is_err());
        m.pen.push([0., 0.]);
        assert!(check(&m, m.epoch, m.editor.revision(), m.glyph).is_err());
        m.pen.clear();
        m.proof.active = true;
        assert!(check(&m, m.epoch, m.editor.revision(), m.glyph).is_err());
    }
}
