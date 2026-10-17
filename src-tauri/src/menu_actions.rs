//! Native menus dispatch the same Rust document actions as toolbar commands.
use crate::{model::Action, AppState, LayoutAction};
use font_core::workspace::{Locale, Placement};
use tauri::{Emitter, Manager};
fn error(app: &tauri::AppHandle, result: Result<(), String>) {
    if let Err(e) = result {
        let _ = app.emit("canvas-error", e);
    }
}
fn main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
}
pub fn handle(app: &tauri::AppHandle, id: &str) {
    match id {
        "font_library" => error(app, crate::font_library::open(app)),
        "quit" => {
            if crate::files::busy() || crate::font_batch::busy() {
                return;
            }
            if crate::confirm_discard(app) {
                crate::recovery::stop(app);
                crate::EXIT_CONFIRMED.store(true, std::sync::atomic::Ordering::Relaxed);
                app.exit(0);
            }
        }
        "new" | "open" | "save" | "save_as" => {
            let Some(window) = main_window(app) else {
                return;
            };
            let handle = app.clone();
            let action = id.to_owned();
            tauri::async_runtime::spawn(async move {
                let result = match action.as_str() {
                    "new" => crate::files::new(window).await,
                    "open" => crate::files::open(window).await,
                    _ => crate::files::save(window, action == "save_as").await,
                };
                error(&handle, result);
            });
        }
        "undo" | "redo" | "paste" => {
            let window = app
                .webview_windows()
                .into_values()
                .find(|w| w.is_focused().unwrap_or(false))
                .or_else(|| main_window(app));
            if let Some(w) = window {
                let _ = app.emit_to(w.label(), "menu-edit", id);
            }
        }
        "zoom_in" | "zoom_out" | "fit" | "nodes" | "fill" => {
            let state = app.state::<AppState>();
            let result = (|| {
                let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                let action = match id {
                    "zoom_in" => Action::Zoom {
                        value: (m.zoom * 1.2).min(8.),
                    },
                    "zoom_out" => Action::Zoom {
                        value: (m.zoom / 1.2).max(0.2),
                    },
                    "fit" => Action::ResetView,
                    "nodes" => Action::Nodes,
                    _ => Action::Fill,
                };
                m.action(action)
            })();
            error(app, result);
            crate::publish(app);
        }
        "reset_panels" => dispatch_layout(app, LayoutAction::Reset),
        "main_window" => {
            if let Some(w) = main_window(app) {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        "quick_guide" => {
            let locale = app.state::<AppState>().0.lock().unwrap().workspace.locale;
            let text=match locale{Locale::Ja=>"文字一覧から文字を選択。ノードをドラッグして編集します。\nペン：クリックで点を追加、Enterで閉じる、Escで取り消し。\nパネル見出しをドラッグしてドッキング。分離アイコンでフローティング。\n⌘N 新規　⌘O 開く　⌘S 保存　⌘Z 元に戻す。\nフォントライブラリ：⌘⇧F。TTF/OTF/TTCを読み込み、静的TTFを書き出せます。",Locale::En=>"Choose a glyph, then drag its nodes to edit.\nPen: click to add points, Enter to close, Esc to cancel.\nDrag panel headers to dock; use the pop-out icon to float.\n⌘N New · ⌘O Open · ⌘S Save · ⌘Z Undo.\nFont Library: ⌘⇧F. Import TTF/OTF/TTC and export static TTF.",Locale::ZhCn=>"选择字符并拖动节点编辑。\n钢笔：点击添加节点，Enter 闭合，Esc 取消。\n拖动面板标题进行停靠，使用分离图标浮动面板。\n⌘N 新建 · ⌘O 打开 · ⌘S 保存 · ⌘Z 撤销。\n字体库：⌘⇧F。导入TTF/OTF/TTC，导出静态TTF。"};
            if let Some(w) = main_window(app) {
                rfd::MessageDialog::new()
                    .set_parent(&w)
                    .set_title("Fonts")
                    .set_description(text)
                    .show();
            }
        }
        _ => {
            if let Some(panel) = crate::menu::panel(id) {
                let placement = app.state::<AppState>().0.lock().unwrap().workspace.panels[&panel];
                if let Placement::Docked { .. } = placement {
                    dispatch_layout(app, LayoutAction::Activate { panel });
                } else if let Some(w) = app.get_webview_window(panel.label()) {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        }
    }
}
fn dispatch_layout(app: &tauri::AppHandle, action: LayoutAction) {
    let revision = app.state::<AppState>().0.lock().unwrap().workspace.revision;
    if let Some(window) = main_window(app) {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            error(&handle, crate::layout(window, action, revision).await);
        });
    }
}
