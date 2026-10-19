use crate::{font_library, AppState};
use font_core::{
    font_formats::Format,
    variations::{self, Request},
};
use serde::Deserialize;
use tauri::{Manager, WebviewWindow};
#[tauri::command]
pub async fn variation_axes(
    window: WebviewWindow,
    id: u64,
) -> Result<Vec<variations::Axis>, String> {
    crate::allowed(&window)?;
    let e = font_library::entry(window.app_handle(), id)?;
    tauri::async_runtime::spawn_blocking(move || {
        variations::axes(&font_core::font_io::read(&e.path)?, e.index)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn variation_catalog(
    window: WebviewWindow,
    id: u64,
) -> Result<variations::Catalog, String> {
    crate::allowed(&window)?;
    let e = font_library::entry(window.app_handle(), id)?;
    tauri::async_runtime::spawn_blocking(move || {
        variations::catalog(&font_core::font_io::read(&e.path)?, e.index)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Preview,
    Import,
    Export,
}
#[tauri::command]
pub async fn variation_action(
    window: WebviewWindow,
    request: Request,
    action: Action,
    text: String,
    epoch: u64,
    revision: u64,
    format: Format,
) -> Result<Option<font_library::ExportReport>, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let app = window.app_handle().clone();
    let settings = font_core::proof::Settings {
        text,
        ..Default::default()
    };
    settings.validate()?;
    let generation = {
        let state = app.state::<AppState>();
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch || m.editor.revision() != revision {
            return Err("Conflict".into());
        }
        if m.has_draft() {
            return Err("edit_in_progress".into());
        }
        m.proof.generation += 1;
        m.proof.generation
    };
    let e = font_library::entry(&app, request.id)?;
    let doc = tauri::async_runtime::spawn_blocking(move || {
        let bytes = font_core::font_io::read(&e.path)?;
        match request.instance {
            Some(id) => variations::instantiate_named(&bytes, e.index, id, &request.coordinates),
            None => variations::instantiate(&bytes, e.index, &request.coordinates),
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    match action {
        Action::Preview => {
            let copy = font_core::proof::subset(&doc, &settings.text)?;
            let text = settings.text.clone();
            let prepared = tauri::async_runtime::spawn_blocking(move || {
                font_core::proof::prepare(copy, &text)
            })
            .await
            .map_err(|e| e.to_string())??;
            let state = app.state::<AppState>();
            let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
            if m.epoch != epoch
                || m.editor.revision() != revision
                || m.proof.generation != generation
            {
                return Err("Conflict".into());
            }
            m.proof.color = None;
            m.proof.source = Some(std::sync::Arc::new(doc));
            m.proof.font = Some(std::sync::Arc::new(prepared));
            m.proof.reference = None;
            m.proof.settings = settings;
            m.proof.active = true;
            drop(m);
            crate::publish(&app);
            if let Some(w) = app.get_webview_window("main") {
                w.show().map_err(|e| e.to_string())?;
            }
        }
        Action::Import => {
            crate::files::add_document(&window, doc, None).await?;
            if let Some(main) = app.get_webview_window("main") {
                main.show().map_err(|e| e.to_string())?;
                main.set_focus().map_err(|e| e.to_string())?;
            }
        }
        Action::Export => {
            if !matches!(format, Format::Ttf | Format::Otf | Format::Woff2) {
                return Err("variation_unsupported".into());
            }
            return font_library::save_font(&window, doc, format, Default::default()).await;
        }
    }
    Ok(None)
}
