use crate::AppState;
use font_core::workspace::{Locale, Panel};
use tauri::{
    menu::{
        AboutMetadata, CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
    },
    Manager,
};

#[derive(Clone, Copy)]
#[repr(usize)]
enum Word {
    File,
    Edit,
    View,
    Window,
    Help,
    New,
    Open,
    Save,
    SaveAs,
    Close,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    All,
    ZoomIn,
    ZoomOut,
    Fit,
    Nodes,
    Fill,
    Minimize,
    Maximize,
    Fullscreen,
    Panels,
    Glyphs,
    Metrics,
    Outline,
    Reset,
    Main,
    Guide,
    About,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    Library,
}
fn text(locale: Locale, key: Word) -> &'static str {
    const EN: [&str; 37] = [
        "File",
        "Edit",
        "View",
        "Window",
        "Help",
        "New Blank Document",
        "Open…",
        "Save",
        "Save As…",
        "Close Window",
        "Undo",
        "Redo",
        "Cut",
        "Copy",
        "Paste",
        "Select All",
        "Zoom In",
        "Zoom Out",
        "Fit to Window",
        "Show Nodes",
        "Filled Preview",
        "Minimize",
        "Zoom Window",
        "Enter Full Screen",
        "Panels",
        "Glyphs",
        "Metrics",
        "Outline",
        "Reset Panel Layout",
        "Main Window",
        "Quick Guide",
        "About Fonts",
        "Hide Fonts",
        "Hide Others",
        "Show All",
        "Quit Fonts",
        "Font Library…",
    ];
    const JA: [&str; 37] = [
        "ファイル",
        "編集",
        "表示",
        "ウインドウ",
        "ヘルプ",
        "新規空ドキュメント",
        "開く…",
        "保存",
        "別名で保存…",
        "ウインドウを閉じる",
        "元に戻す",
        "やり直す",
        "切り取り",
        "コピー",
        "貼り付け",
        "すべてを選択",
        "拡大",
        "縮小",
        "全体を表示",
        "ノードを表示",
        "塗りのプレビュー",
        "しまう",
        "拡大／縮小",
        "フルスクリーンにする",
        "パネル",
        "文字一覧",
        "メトリクス",
        "輪郭",
        "パネル配置を戻す",
        "メインウインドウ",
        "クイックガイド",
        "Fontsについて",
        "Fontsを隠す",
        "ほかを隠す",
        "すべてを表示",
        "Fontsを終了",
        "フォントライブラリ…",
    ];
    const ZH: [&str; 37] = [
        "文件",
        "编辑",
        "显示",
        "窗口",
        "帮助",
        "新建空白文档",
        "打开…",
        "保存",
        "另存为…",
        "关闭窗口",
        "撤销",
        "重做",
        "剪切",
        "复制",
        "粘贴",
        "全选",
        "放大",
        "缩小",
        "适合窗口",
        "显示节点",
        "填充预览",
        "最小化",
        "缩放窗口",
        "进入全屏",
        "面板",
        "字符",
        "度量",
        "轮廓",
        "重置面板布局",
        "主窗口",
        "快速指南",
        "关于 Fonts",
        "隐藏 Fonts",
        "隐藏其他",
        "显示全部",
        "退出 Fonts",
        "字体库…",
    ];
    (match locale {
        Locale::En => EN,
        Locale::Ja => JA,
        Locale::ZhCn => ZH,
    })[key as usize]
}
pub fn build(app: &tauri::AppHandle, locale: Locale) -> tauri::Result<Menu<tauri::Wry>> {
    use Word::*;
    let t = |key| text(locale, key);
    let item = |id, key, shortcut| MenuItem::with_id(app, id, t(key), true, shortcut);
    let check = |id, key| CheckMenuItem::with_id(app, id, t(key), true, false, None::<&str>);
    let separator = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                "Fonts",
                true,
                &[
                    &PredefinedMenuItem::about(
                        app,
                        Some(t(About)),
                        Some(AboutMetadata {
                            name: Some("Fonts".into()),
                            version: Some(env!("CARGO_PKG_VERSION").into()),
                            ..Default::default()
                        }),
                    )?,
                    &separator()?,
                    &PredefinedMenuItem::hide(app, Some(t(Hide)))?,
                    &PredefinedMenuItem::hide_others(app, Some(t(HideOthers)))?,
                    &PredefinedMenuItem::show_all(app, Some(t(ShowAll)))?,
                    &separator()?,
                    &item("quit", Quit, Some("CmdOrCtrl+Q"))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t(File),
                true,
                &[
                    &item("new", New, Some("CmdOrCtrl+N"))?,
                    &item("open", Open, Some("CmdOrCtrl+O"))?,
                    &item("font_library", Library, Some("CmdOrCtrl+Shift+F"))?,
                    &separator()?,
                    &item("save", Save, Some("CmdOrCtrl+S"))?,
                    &item("save_as", SaveAs, Some("CmdOrCtrl+Shift+S"))?,
                    &separator()?,
                    &PredefinedMenuItem::close_window(app, Some(t(Close)))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t(Edit),
                true,
                &[
                    &item("undo", Undo, Some("CmdOrCtrl+Z"))?,
                    &item("redo", Redo, Some("CmdOrCtrl+Shift+Z"))?,
                    &separator()?,
                    &PredefinedMenuItem::cut(app, Some(t(Cut)))?,
                    &PredefinedMenuItem::copy(app, Some(t(Copy)))?,
                    &PredefinedMenuItem::paste(app, Some(t(Paste)))?,
                    &PredefinedMenuItem::select_all(app, Some(t(All)))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t(View),
                true,
                &[
                    &item("zoom_in", ZoomIn, Some("CmdOrCtrl+Plus"))?,
                    &item("zoom_out", ZoomOut, Some("CmdOrCtrl+-"))?,
                    &item("fit", Fit, Some("CmdOrCtrl+0"))?,
                    &separator()?,
                    &check("nodes", Nodes)?,
                    &check("fill", Fill)?,
                    &separator()?,
                    &PredefinedMenuItem::fullscreen(app, Some(t(Fullscreen)))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t(Window),
                true,
                &[
                    &PredefinedMenuItem::minimize(app, Some(t(Minimize)))?,
                    &PredefinedMenuItem::maximize(app, Some(t(Maximize)))?,
                    &separator()?,
                    &item("main_window", Main, None)?,
                    &Submenu::with_items(
                        app,
                        t(Panels),
                        true,
                        &[
                            &item("panel_glyphs", Glyphs, None)?,
                            &item("panel_metrics", Metrics, None)?,
                            &item("panel_outline", Outline, None)?,
                        ],
                    )?,
                    &item("reset_panels", Reset, None)?,
                ],
            )?,
            &Submenu::with_items(app, t(Help), true, &[&item("quick_guide", Guide, None)?])?,
        ],
    )
}
fn find_item(items: &[MenuItemKind<tauri::Wry>], id: &str) -> Option<MenuItemKind<tauri::Wry>> {
    for item in items {
        if item.id().as_ref() == id {
            return Some(item.clone());
        }
        if let MenuItemKind::Submenu(submenu) = item {
            if let Ok(children) = submenu.items() {
                if let Some(found) = find_item(&children, id) {
                    return Some(found);
                }
            }
        }
    }
    None
}
/// Update existing menu items instead of reconstructing the menu on every pointer edit.
pub fn refresh(app: &tauri::AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(state) = handle.try_state::<AppState>() else {
            return;
        };
        let (undo, redo, nodes, fill, zoom) = {
            let m = state.0.lock().unwrap();
            (
                !m.text_focus.is_empty() || m.editor.can_undo(),
                !m.text_focus.is_empty() || m.editor.can_redo(),
                m.nodes,
                m.fill,
                m.zoom,
            )
        };
        let Some(menu) = handle.menu() else {
            return;
        };
        let Ok(items) = menu.items() else {
            return;
        };
        for (id, enabled) in [
            ("undo", undo),
            ("redo", redo),
            ("zoom_in", zoom < 8.),
            ("zoom_out", zoom > 0.2),
        ] {
            if let Some(MenuItemKind::MenuItem(i)) = find_item(&items, id) {
                let _ = i.set_enabled(enabled);
            }
        }
        for (id, checked) in [("nodes", nodes), ("fill", fill)] {
            if let Some(MenuItemKind::Check(i)) = find_item(&items, id) {
                let _ = i.set_checked(checked);
            }
        }
    });
}
pub fn panel(id: &str) -> Option<Panel> {
    match id {
        "panel_glyphs" => Some(Panel::Glyphs),
        "panel_metrics" => Some(Panel::Metrics),
        "panel_outline" => Some(Panel::Outline),
        _ => None,
    }
}
