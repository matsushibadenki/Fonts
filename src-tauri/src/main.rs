mod clipboard;
mod color_fonts;
mod documents;
mod files;
mod floating;
mod font_activation;
mod font_batch;
mod font_catalog;
mod font_library;
mod font_paths;
mod font_store;
mod hinting;
mod menu;
mod menu_actions;
mod model;
#[cfg(target_os = "macos")]
mod native;
mod proof;
mod recovery;
mod recovery_store;
mod reference_images;
mod snippets;
mod thumbnails;
mod variations;
use font_core::workspace::{Dock, Panel, Placement, Workspace};
use model::{Action, Model, Snapshot};
use serde::Deserialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewWindow};

struct AppState(Mutex<Model>);
#[derive(Clone, Copy, Deserialize)]
pub struct CanvasRequest {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
}
fn workspace_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|p| p.join("workspace.json"))
}
fn persist(app: &tauri::AppHandle) {
    if let Some(path) = workspace_file(app) {
        let model = app.state::<AppState>();
        let model = model.0.lock().unwrap();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(&model.workspace) {
            let temp = path.with_extension("json.tmp");
            if std::fs::write(&temp, bytes).is_ok() {
                let _ = std::fs::rename(temp, path);
            }
        }
    }
}
fn publish(app: &tauri::AppHandle) {
    menu::refresh(app);
    let _ = app.emit("state-changed", ());
    #[cfg(target_os = "macos")]
    if let Some(w) = app.get_webview_window("main") {
        let app = app.clone();
        let _ = w.run_on_main_thread(move || {
            let state = app.state::<AppState>();
            if let Ok(m) = state.0.lock() {
                if let Err(e) = native::redraw(&m) {
                    let _ = app.emit("canvas-error", e);
                }
            };
        });
    }
}
fn allowed(window: &WebviewWindow) -> Result<(), String> {
    if (window.label() == "main" || window.label() == "font-library")
        || Panel::from_label(window.label()).is_some()
    {
        Ok(())
    } else {
        Err("unknown_window".into())
    }
}
#[tauri::command]
fn snapshot(window: WebviewWindow, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    allowed(&window)?;
    Ok(state.0.lock().map_err(|_| "state_poisoned")?.snapshot())
}
#[tauri::command]
fn edit(
    window: WebviewWindow,
    state: tauri::State<AppState>,
    action: Action,
    epoch: u64,
) -> Result<u64, String> {
    allowed(&window)?;
    let (layout_changed, revision) = {
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch {
            return Err("Conflict".into());
        }
        let before = m.workspace.revision;
        m.action(action)?;
        (m.workspace.revision != before, m.editor.revision())
    };
    publish(window.app_handle());
    if layout_changed {
        persist(window.app_handle());
        let app = window.app_handle().clone();
        window
            .run_on_main_thread(move || {
                let locale = app.state::<AppState>().0.lock().unwrap().workspace.locale;
                if let Ok(menu) = menu::build(&app, locale) {
                    let _ = app.set_menu(menu);
                    menu::refresh(&app);
                }
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(revision)
}
#[derive(Deserialize)]
struct PointerRequest {
    phase: String,
    revision: u64,
    glyph: u32,
    shift: bool,
    token: String,
    epoch: u64,
    x: f64,
    y: f64,
}
#[tauri::command]
fn pointer(
    window: WebviewWindow,
    state: tauri::State<AppState>,
    request: PointerRequest,
) -> Result<(), String> {
    let PointerRequest {
        phase,
        revision,
        glyph,
        shift,
        token,
        epoch,
        x,
        y,
    } = request;
    if window.label() != "main" {
        return Err("main_window_only".into());
    }
    {
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if m.epoch != epoch {
            return Err("Conflict".into());
        }
        if m.snippets.draft.is_some() && !m.proof.active {
            if m.glyph != glyph || m.editor.revision() != revision {
                return Err("Conflict".into());
            }
            m.snippet_pointer(&phase, [x, y], shift, &token)?;
        } else if m.tool == font_core::workspace::Tool::Images && !m.proof.active {
            m.image_context(epoch, revision, glyph)?;
            m.image_pointer(&phase, [x, y], shift, &token)?;
        } else {
            if m.glyph != glyph || m.editor.revision() != revision {
                return Err("Conflict".into());
            }
            m.pointer(&phase, x, y)?;
        }
    }
    // Pointer moves redraw on demand but do not broadcast document snapshots.
    if phase == "up" || phase == "down" || phase == "cancel" {
        publish(window.app_handle());
    } else {
        #[cfg(target_os = "macos")]
        {
            let app = window.app_handle().clone();
            window
                .run_on_main_thread(move || {
                    let state = app.state::<AppState>();
                    if let Ok(m) = state.0.lock() {
                        if let Err(e) = native::redraw(&m) {
                            let _ = app.emit("canvas-error", e);
                        }
                    };
                })
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[tauri::command]
async fn canvas(window: WebviewWindow, request: CanvasRequest) -> Result<String, String> {
    if window.label() != "main" {
        return Err("main_window_only".into());
    }
    #[cfg(target_os = "macos")]
    {
        let app = window.app_handle().clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        window
            .with_webview(move |view| {
                let result = (|| {
                    let state = app.state::<AppState>();
                    let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                    native::sync(view.inner(), request, &mut m)
                })();
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string()))
            .await
            .map_err(|e| e.to_string())??
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err("native_platform_pending".into())
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LayoutAction {
    Relative {
        panel: Panel,
        target: Panel,
        side: font_core::dock_tree::Side,
    },
    ResizeSplit {
        dock: Dock,
        path: Vec<u8>,
        value: f64,
    },
    Dock {
        panel: Panel,
        dock: Dock,
    },
    Float {
        panel: Panel,
    },
    Activate {
        panel: Panel,
    },
    Resize {
        dock: Dock,
        value: f64,
    },
    Reset,
}
#[tauri::command]
async fn layout(window: WebviewWindow, action: LayoutAction, revision: u64) -> Result<(), String> {
    allowed(&window)?;
    let app = window.app_handle().clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let result = (|| {
                floating::flush(&app);
                let state = app.state::<AppState>();
                let mut candidate = {
                    let m = state.0.lock().map_err(|_| "state_poisoned")?;
                    if m.workspace.revision != revision {
                        return Err("Conflict".into());
                    }
                    m.workspace.clone()
                };
                let mut close = None;
                let mut created = None;
                match action {
                    LayoutAction::Relative {
                        panel,
                        target,
                        side,
                    } => candidate
                        .relative(revision, panel, target, side)
                        .map_err(|e| e.to_string())?,
                    LayoutAction::ResizeSplit { dock, path, value } => candidate
                        .resize_split(revision, dock, &path, value)
                        .map_err(|e| e.to_string())?,
                    LayoutAction::Float { panel } => {
                        if let Some(w) = app.get_webview_window(panel.label()) {
                            w.set_focus().map_err(|e| e.to_string())?;
                            return Ok(());
                        }
                        let return_to = match candidate.panels[&panel] {
                            Placement::Docked { dock } => dock,
                            Placement::Floating { return_to } => return_to,
                        };
                        candidate
                            .place(revision, panel, Placement::Floating { return_to })
                            .map_err(|e| e.to_string())?;
                        let (_, bounds) =
                            floating::create(&app, panel, candidate.floating_bounds.get(&panel))?;
                        if let Some(bounds) = bounds {
                            candidate.floating_bounds.insert(panel, bounds);
                        }
                        created = Some(panel);
                    }
                    LayoutAction::Dock { panel, dock } => {
                        candidate
                            .place(revision, panel, Placement::Docked { dock })
                            .map_err(|e| e.to_string())?;
                        close = Some(panel);
                    }
                    LayoutAction::Activate { panel } => candidate
                        .activate(revision, panel)
                        .map_err(|e| e.to_string())?,
                    LayoutAction::Resize { dock, value } => candidate
                        .resize(revision, dock, value)
                        .map_err(|e| e.to_string())?,
                    LayoutAction::Reset => {
                        candidate = Workspace {
                            revision: revision.checked_add(1).ok_or("Conflict")?,
                            locale: candidate.locale,
                            ..Workspace::default()
                        };
                    }
                }
                {
                    let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                    if m.workspace.revision != revision {
                        drop(m);
                        if let Some(panel) = created {
                            if let Some(w) = app.get_webview_window(panel.label()) {
                                let _ = w.destroy();
                            }
                        }
                        return Err("Conflict".into());
                    }
                    m.workspace = candidate.clone();
                }
                if let Some(panel) = created {
                    floating::reveal(&app, panel, true);
                }
                if let Some(panel) = close {
                    if let Some(w) = app.get_webview_window(panel.label()) {
                        let _ = w.close();
                    }
                }
                for panel in Panel::ALL {
                    if matches!(candidate.panels[&panel], Placement::Docked { .. }) {
                        if let Some(w) = app.get_webview_window(panel.label()) {
                            let _ = w.close();
                        }
                    }
                }
                persist(&app);
                publish(&app);
                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??
}
#[tauri::command]
async fn new_project(window: WebviewWindow) -> Result<(), String> {
    allowed(&window)?;
    files::new(window).await
}
#[tauri::command]
async fn save_project(window: WebviewWindow) -> Result<(), String> {
    allowed(&window)?;
    files::save(window, false).await
}
#[tauri::command]
fn text_focus(
    window: WebviewWindow,
    state: tauri::State<AppState>,
    focused: bool,
) -> Result<(), String> {
    allowed(&window)?;
    {
        let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
        if focused {
            m.text_focus.insert(window.label().into());
        } else {
            m.text_focus.remove(window.label());
        }
    }
    menu::refresh(window.app_handle());
    Ok(())
}
#[tauri::command]
fn open_font_library(window: WebviewWindow) -> Result<(), String> {
    allowed(&window)?;
    font_library::open(window.app_handle())
}
static EXIT_CONFIRMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn confirm_discard(app: &tauri::AppHandle) -> bool {
    let state = app.state::<AppState>();
    let (names, locale) = {
        let m = state.0.lock().unwrap();
        (
            m.all_documents()
                .filter(|d| d.unsaved())
                .map(|d| d.confirmation_name())
                .collect::<Vec<_>>(),
            m.workspace.locale,
        )
    };
    confirm_documents(app, &names, locale)
}
fn confirm_documents(
    app: &tauri::AppHandle,
    names: &[String],
    locale: font_core::workspace::Locale,
) -> bool {
    use font_core::workspace::Locale;
    if names.is_empty() {
        return true;
    }
    let message = match locale {
        Locale::En => "Discard unsaved changes?",
        Locale::Ja => "未保存の変更を破棄しますか？",
        Locale::ZhCn => "放弃未保存的更改？",
    };
    let (discard, cancel) = match locale {
        Locale::En => ("Discard", "Cancel"),
        Locale::Ja => ("破棄", "キャンセル"),
        Locale::ZhCn => ("放弃", "取消"),
    };
    let Some(parent) = app.get_webview_window("main") else {
        return true;
    };
    rfd::MessageDialog::new()
        .set_parent(&parent)
        .set_title("Fonts")
        .set_description(format!("{message}\n\n{}", names.join("\n")))
        .set_buttons(rfd::MessageButtons::OkCancelCustom(
            discard.into(),
            cancel.into(),
        ))
        .show()
        == rfd::MessageDialogResult::Custom(discard.into())
}
fn main() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("hintpreview", |context, request| {
            let image = context
                .app_handle()
                .state::<hinting::Previews>()
                .resource(context.webview_label(), request.uri().path());
            tauri::http::Response::builder()
                .status(if image.is_some() { 200 } else { 404 })
                .header("Content-Type", "image/png")
                .header("Cache-Control", "no-store")
                .header("X-Content-Type-Options", "nosniff")
                .body(image.unwrap_or_default())
                .unwrap()
        })
        .register_asynchronous_uri_scheme_protocol("thumbnail", |context, request, responder| {
            if context.webview_label() != "main"
                && context.webview_label() != "panel-glyphs"
                && context.webview_label() != "panel-snippets"
            {
                responder.respond(
                    tauri::http::Response::builder()
                        .status(403)
                        .body(Vec::<u8>::new())
                        .unwrap(),
                );
                return;
            }
            let app = context.app_handle();
            app.state::<thumbnails::Thumbnails>().request(
                app.clone(),
                request.uri().path(),
                responder,
            );
        })
        .menu(|app| menu::build(app, font_core::workspace::Locale::Ja))
        .on_menu_event(|app, event| menu_actions::handle(app, event.id().as_ref()))
        .setup(|app| {
            let workspace = workspace_file(app.handle())
                .and_then(|p| std::fs::read(p).ok())
                .filter(|b| b.len() < 65536)
                .and_then(|b| serde_json::from_slice::<Workspace>(&b).ok())
                .and_then(|w| w.restore().ok())
                .unwrap_or_default();
            let locale = workspace.locale;
            app.manage(AppState(Mutex::new(Model::new(workspace))));
            app.manage(floating::Runtime::default());
            app.manage(thumbnails::Thumbnails::new());
            app.manage(hinting::Previews::default());
            app.manage(font_batch::Batch::default());
            app.manage(font_library::Library(Mutex::new(
                font_library::LibraryState::default(),
            )));
            app.manage(recovery::Recovery::new(
                app.path().app_data_dir()?.join("recovery"),
            )?);
            recovery::start(app.handle().clone());
            app.set_menu(menu::build(app.handle(), locale)?)?;
            menu::refresh(app.handle());
            floating::restore(app.handle());
            floating::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            floating::event(window.app_handle(), window.label(), event);
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let state = window.state::<AppState>();
                if let Ok(mut m) = state.0.lock() {
                    if !EXIT_CONFIRMED.load(std::sync::atomic::Ordering::Relaxed) {
                        if let Some(panel) = Panel::from_label(window.label()) {
                            let _ = m.workspace.redock(panel);
                        }
                    }
                    m.text_focus.remove(window.label());
                    if m.snippets
                        .draft
                        .as_ref()
                        .is_some_and(|d| d.owner == window.label() || window.label() == "main")
                    {
                        m.snippets.draft = None;
                        if m.tool == font_core::workspace::Tool::Snippets {
                            m.tool = font_core::workspace::Tool::Nodes;
                        }
                    }
                    if m.images
                        .drag
                        .as_ref()
                        .is_some_and(|d| d.owner == window.label())
                    {
                        m.images.drag = None;
                    }
                }
                if Panel::from_label(window.label()).is_some()
                    && !EXIT_CONFIRMED.load(std::sync::atomic::Ordering::Relaxed)
                {
                    persist(window.app_handle());
                }
                publish(window.app_handle());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(panel) = Panel::from_label(window.label()) {
                    let app = window.app_handle();
                    floating::capture(app, panel);
                    let state = app.state::<AppState>();
                    if !EXIT_CONFIRMED.load(std::sync::atomic::Ordering::Relaxed) {
                        if let Ok(mut m) = state.0.lock() {
                            let _ = m.workspace.redock(panel);
                        }
                    }
                    persist(app);
                    publish(app);
                } else if window.label() == "main" {
                    if files::busy() || font_batch::busy() {
                        api.prevent_close();
                        return;
                    }
                    if !confirm_discard(window.app_handle()) {
                        api.prevent_close();
                        return;
                    }
                    floating::flush(window.app_handle());
                    floating::stop(window.app_handle());
                    recovery::stop(window.app_handle());
                    EXIT_CONFIRMED.store(true, std::sync::atomic::Ordering::Relaxed);
                    persist(window.app_handle());
                    for p in Panel::ALL {
                        if let Some(w) = window.app_handle().get_webview_window(p.label()) {
                            let _ = w.destroy();
                        }
                    }
                    if let Some(w) = window.app_handle().get_webview_window("font-library") {
                        let _ = w.destroy();
                    }
                    #[cfg(target_os = "macos")]
                    native::destroy();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            documents::document_action,
            clipboard::paste_paths,
            clipboard::paste_text,
            font_batch::font_batch_start,
            font_batch::font_batch_query,
            font_batch::font_batch_cancel,
            proof::proof_action,
            recovery::recovery_query,
            recovery::recovery_action,
            reference_images::reference_import,
            reference_images::reference_action,
            snippets::snippet_action,
            snapshot,
            edit,
            pointer,
            canvas,
            layout,
            save_project,
            new_project,
            text_focus,
            open_font_library,
            font_library::font_library_query,
            hinting::font_hinting_info,
            hinting::font_hinting_compare,
            hinting::font_hinting_export,
            color_fonts::color_font_open,
            color_fonts::color_font_action,
            variations::variation_axes,
            variations::variation_action,
            font_library::font_library_action
        ])
        .build(tauri::generate_context!())
        .expect("Fonts application failed")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if files::busy() || font_batch::busy() {
                    api.prevent_exit();
                    return;
                }
                if !EXIT_CONFIRMED.load(std::sync::atomic::Ordering::Relaxed)
                    && !confirm_discard(app)
                {
                    api.prevent_exit();
                    return;
                }
                EXIT_CONFIRMED.store(true, std::sync::atomic::Ordering::Relaxed);
                floating::flush(app);
                floating::stop(app);
                recovery::stop(app);
                persist(app);
                #[cfg(target_os = "macos")]
                native::destroy();
            }
        });
}
