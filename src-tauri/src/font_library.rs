use crate::{
    font_catalog::{self, Catalog, Entry},
    font_paths,
    font_store::{ManagedFont, Store},
    AppState,
};
use font_core::font_formats::Format;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Manager, WebviewWindow};
pub struct Library(pub Mutex<LibraryState>);
#[derive(Default)]
pub struct LibraryState {
    pub(crate) catalog: Catalog,
    next: u64,
    pub(crate) color: Option<crate::color_fonts::Source>,
}
#[derive(Serialize)]
pub struct Page {
    entries: Vec<Entry>,
    locations: Vec<font_paths::Location>,
    managed: Vec<ManagedFont>,
    total: usize,
    page: usize,
    skipped: usize,
    truncated: bool,
    platform: &'static str,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    Scan,
    SystemManager,
    AdobeManager,
    Choose,
    Import {
        id: u64,
    },
    Copy {
        id: u64,
    },
    Convert {
        id: u64,
        format: Format,
    },
    ApplyCmap,
    Export {
        family: String,
        #[serde(default)]
        autohint: bool,
        #[serde(default)]
        woff2: font_core::font_formats::woff2::Options,
        #[serde(default = "ttf_format")]
        format: Format,
    },
    Install {
        family: String,
    },
    Activate {
        id: String,
        active: bool,
    },
    Reveal {
        location: usize,
    },
}
fn ttf_format() -> Format {
    Format::Ttf
}

fn store(app: &tauri::AppHandle) -> Result<Store, String> {
    Ok(Store {
        root: app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("managed-fonts"),
        user: font_paths::user_directory()?,
    })
}
pub fn open(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("font-library") {
        w.show().map_err(|e| e.to_string())?;
        w.unminimize().map_err(|e| e.to_string())?;
        return w.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(
        app,
        "font-library",
        tauri::WebviewUrl::App("index.html?library=1".into()),
    )
    .title("Fonts — Library")
    .inner_size(1000., 720.)
    .min_inner_size(760., 520.)
    .disable_drag_drop_handler()
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn font_library_query(
    window: WebviewWindow,
    query: String,
    source: String,
    page: usize,
) -> Result<Page, String> {
    crate::allowed(&window)?;
    let app = window.app_handle();
    let state = app.state::<Library>();
    let state = state.0.lock().map_err(|_| "state_poisoned")?;
    let q = font_catalog::normalize_query(&query);
    let matches: Vec<_> = state
        .catalog
        .entries
        .iter()
        .filter(|e| (source.is_empty() || e.source == source) && e.matches_query(&q))
        .collect();
    let total = matches.len();
    let page = page.min(total.saturating_sub(1) / 64);
    Ok(Page {
        entries: matches
            .into_iter()
            .skip(page * 64)
            .take(64)
            .cloned()
            .collect(),
        locations: state.catalog.locations.clone(),
        managed: store(app)?
            .list()?
            .into_iter()
            .filter(|font| {
                let family = font_catalog::normalize_query(&font.family);
                q.split_whitespace().all(|term| family.contains(term))
            })
            .collect(),
        total,
        page,
        skipped: state.catalog.skipped,
        truncated: state.catalog.truncated,
        platform: std::env::consts::OS,
    })
}
pub(crate) fn entry(app: &tauri::AppHandle, id: u64) -> Result<Entry, String> {
    app.state::<Library>()
        .0
        .lock()
        .map_err(|_| "state_poisoned")?
        .catalog
        .entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or("font_not_found".into())
}
fn index(app: &tauri::AppHandle, mut catalog: Catalog) -> Result<(), String> {
    let state = app.state::<Library>();
    let mut state = state.0.lock().map_err(|_| "state_poisoned")?;
    for e in &mut catalog.entries {
        state.next = state.next.checked_add(1).ok_or("font_too_large")?;
        e.id = state.next;
    }
    state.catalog = catalog;
    Ok(())
}
pub(crate) fn protected(path: &std::path::Path) -> Result<bool, String> {
    let parent = path
        .parent()
        .ok_or("font_invalid_path")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    Ok(font_paths::current()?
        .iter()
        .filter_map(|r| r.path.canonicalize().ok())
        .any(|root| font_paths::under(&parent, &root)))
}
#[derive(serde::Serialize)]
pub struct ExportReport {
    path: PathBuf,
    bytes: usize,
    verified: bool,
}
#[tauri::command]
pub async fn font_library_action(
    window: WebviewWindow,
    operation: Operation,
) -> Result<Option<ExportReport>, String> {
    crate::allowed(&window)?;
    let _guard = crate::files::FileGuard::acquire()?;
    let app = window.app_handle().clone();
    let return_to_editor = matches!(&operation, Operation::Import { .. });
    let restore_focus = matches!(
        &operation,
        Operation::Choose
            | Operation::Copy { .. }
            | Operation::Export { .. }
            | Operation::Convert { .. }
            | Operation::ApplyCmap
    );
    let mut report = None;
    match operation {
        Operation::SystemManager => {
            tauri::async_runtime::spawn_blocking(open_system_manager)
                .await
                .map_err(|e| e.to_string())??;
        }
        Operation::AdobeManager => {
            tauri::async_runtime::spawn_blocking(|| {
                open_target("https://fonts.adobe.com/my_fonts")
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        Operation::Scan => {
            let locations = font_paths::current()?;
            let catalog = tauri::async_runtime::spawn_blocking(move || {
                #[cfg(target_os = "linux")]
                let locations = {
                    let mut roots = locations;
                    font_catalog::configured_locations(&mut roots);
                    roots
                };
                font_catalog::scan(locations)
            })
            .await
            .map_err(|e| e.to_string())?;
            index(&app, catalog)?;
        }
        Operation::Choose => {
            let dialog = rfd::AsyncFileDialog::new().set_parent(&window);
            #[cfg(not(target_os = "macos"))]
            let dialog = dialog.add_filter(
                "Font / PostScript",
                &["ttf", "otf", "ttc", "otc", "pfa", "pfb", "cid", "cff", "ps"],
            );
            let paths = dialog.pick_files().await;
            if let Some(paths) = paths {
                if paths.len() > 256 {
                    return Err("font_batch_limit".into());
                }
                let entries = tauri::async_runtime::spawn_blocking(move || {
                    let mut entries = Vec::new();
                    for path in paths {
                        entries.extend(font_catalog::inspect(path.path(), "file")?);
                    }
                    Ok::<_, String>(entries)
                })
                .await
                .map_err(|e| e.to_string())??;
                let state = app.state::<Library>();
                let mut state = state.0.lock().map_err(|_| "state_poisoned")?;
                for mut e in entries {
                    state.next = state.next.checked_add(1).ok_or("font_too_large")?;
                    e.id = state.next;
                    state.catalog.entries.push(e);
                }
            }
        }
        Operation::Import { id } => {
            let e = entry(&app, id)?;
            let doc = tauri::async_runtime::spawn_blocking(move || {
                font_core::font_io::import(&font_core::font_io::read(&e.path)?, e.index)
            })
            .await
            .map_err(|e| e.to_string())??;
            crate::files::add_document(&window, doc, None).await?;
        }
        Operation::Copy { id } => {
            let e = entry(&app, id)?;
            let parent = window.clone();
            let name = e
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let path = tauri::async_runtime::spawn_blocking(move || {
                rfd::FileDialog::new()
                    .set_parent(&parent)
                    .set_file_name(name)
                    .save_file()
            })
            .await
            .map_err(|e| e.to_string())?;
            if let Some(path) = path {
                if protected(&path)? {
                    return Err("font_protected_location".into());
                }
                tauri::async_runtime::spawn_blocking(move || {
                    let bytes = font_core::font_io::read(&e.path)?;
                    font_core::storage::write_bytes(&path, &bytes).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;
            }
        }
        Operation::Export {
            family,
            format,
            woff2,
            autohint,
        } => {
            let mut doc = document(&app)?;
            doc.family = family;
            report = save_font_with_hinting(&window, doc, format, woff2, autohint).await?;
        }
        Operation::Convert { id, format } => {
            let e = entry(&app, id)?;
            let mut doc = tauri::async_runtime::spawn_blocking(move || {
                font_core::font_io::import(&font_core::font_io::read(&e.path)?, e.index)
            })
            .await
            .map_err(|e| e.to_string())??;
            if let Some(family) = doc.family.strip_suffix(" Edited") {
                doc.family = family.to_owned();
            }
            report = save_font(&window, doc, format, Default::default()).await?;
        }
        Operation::ApplyCmap => {
            let (epoch, revision) = {
                let state = app.state::<AppState>();
                let m = state.0.lock().map_err(|_| "state_poisoned")?;
                (m.epoch, m.editor.revision())
            };
            let dialog = rfd::AsyncFileDialog::new().set_parent(&window);
            #[cfg(not(target_os = "macos"))]
            let dialog = dialog.add_filter("Unicode / CID CMap", &["cmap", "txt"]);
            let path = dialog
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf());
            if let Some(path) = path {
                let mapping = tauri::async_runtime::spawn_blocking(move || {
                    use std::io::Read;
                    let mut bytes = vec![];
                    std::fs::File::open(path)
                        .map_err(|e| e.to_string())?
                        .take(4 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    if bytes.len() > 4 * 1024 * 1024 {
                        return Err("font_too_large".into());
                    }
                    font_core::font_formats::cmap::parse(
                        std::str::from_utf8(&bytes).map_err(|_| "font_cmap_invalid")?,
                    )
                })
                .await
                .map_err(|e| e.to_string())??;
                {
                    let state = app.state::<AppState>();
                    let mut m = state.0.lock().map_err(|_| "state_poisoned")?;
                    if m.epoch != epoch || m.editor.revision() != revision {
                        return Err("Conflict".into());
                    }
                    if m.has_draft() {
                        return Err("edit_in_progress".into());
                    }
                    let glyphs =
                        font_core::font_formats::cmap::apply(m.editor.document(), &mapping)?;
                    m.editor
                        .replace_glyphs(revision, glyphs)
                        .map_err(|e| e.to_string())?;
                }
                crate::publish(&app);
            }
        }
        Operation::Install { family } => {
            let mut doc = document(&app)?;
            doc.family = family;
            let family = doc.family.clone();
            let bytes = tauri::async_runtime::spawn_blocking(move || {
                font_core::font_formats::export(&doc, Format::Ttf)
            })
            .await
            .map_err(|e| e.to_string())??;
            let store = store(&app)?;
            tauri::async_runtime::spawn_blocking(move || {
                let id = store.add(&bytes, family)?;
                store.set_active(&id, true, crate::font_activation::change)
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        Operation::Activate { id, active } => {
            let store = store(&app)?;
            tauri::async_runtime::spawn_blocking(move || {
                store.set_active(&id, active, crate::font_activation::change)
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        Operation::Reveal { location } => {
            let path = app
                .state::<Library>()
                .0
                .lock()
                .map_err(|_| "state_poisoned")?
                .catalog
                .locations
                .get(location)
                .map(|r| r.path.clone())
                .ok_or("font_not_found")?;
            if !path.is_dir() {
                return Err("font_folder_missing".into());
            }
            tauri::async_runtime::spawn_blocking(move || reveal(path))
                .await
                .map_err(|e| e.to_string())??;
        }
    }
    let target = if return_to_editor {
        app.get_webview_window("main")
    } else if restore_focus {
        Some(window)
    } else {
        None
    };
    if let Some(w) = target {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }

    Ok(report)
}
fn reveal(path: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

fn open_system_manager() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .args(["-a", "Font Book"])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        open_target("ms-settings:fonts")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if std::process::Command::new("font-manager").spawn().is_ok() {
            return Ok(());
        }
        let path = font_paths::user_directory()?;
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        reveal(path)
    }
}
fn open_target(target: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                op: *const u16,
                file: *const u16,
                params: *const u16,
                dir: *const u16,
                show: i32,
            ) -> isize;
        }
        let value: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
        // SAFETY: target is a fixed internal URL, all optional parameters are null.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                value.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result > 32 {
            Ok(())
        } else {
            Err("font_open_failed".into())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let program = "/usr/bin/open";
        #[cfg(not(target_os = "macos"))]
        let program = "xdg-open";
        std::process::Command::new(program)
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn document(app: &tauri::AppHandle) -> Result<font_core::FontDocument, String> {
    let state = app.state::<AppState>();
    let m = state.0.lock().map_err(|_| "state_poisoned")?;
    if m.has_draft() {
        return Err("edit_in_progress".into());
    }
    Ok(m.editor.document().clone())
}
pub(crate) async fn save_font(
    window: &WebviewWindow,
    doc: font_core::FontDocument,
    format: Format,
    woff2: font_core::font_formats::woff2::Options,
) -> Result<Option<ExportReport>, String> {
    save_font_with_hinting(window, doc, format, woff2, false).await
}
async fn save_font_with_hinting(
    window: &WebviewWindow,
    doc: font_core::FontDocument,
    format: Format,
    woff2: font_core::font_formats::woff2::Options,
    autohint: bool,
) -> Result<Option<ExportReport>, String> {
    if autohint
        && !matches!(
            format,
            Format::Ttf
                | Format::Otf
                | Format::CidOtf
                | Format::Cff
                | Format::CidCff
                | Format::Woff2
        )
    {
        return Err("autohint_unsupported".into());
    }
    let parent = window.clone();
    let name = doc.family.clone();
    let verify = autohint || matches!(format, Format::Ttf | Format::Otf | Format::Woff2);
    let expected = doc.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        if autohint {
            woff2.validate()?;
            let base_format = if format == Format::Woff2 {
                match woff2.outline {
                    font_core::font_formats::woff2::Outline::TrueType => Format::Ttf,
                    font_core::font_formats::woff2::Outline::Cff => Format::Otf,
                }
            } else {
                format
            };
            let base = font_core::font_formats::export(&doc, base_format)?;
            if matches!(base_format, Format::Ttf | Format::Otf | Format::CidOtf) {
                font_core::variations::verify(&base, &doc)?;
            }
            let hinted = font_core::hinting::autohint(&base, 0)?;
            if format == Format::Woff2 {
                font_core::font_formats::woff2::encode_sfnt(hinted, woff2.quality)
            } else {
                Ok(hinted)
            }
        } else if format == Format::Woff2 {
            font_core::font_formats::woff2::export(&doc, woff2)
        } else {
            font_core::font_formats::export(&doc, format)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if verify && !autohint {
        let decoded;
        let sfnt = if format == Format::Woff2 {
            decoded = font_core::font_formats::woff2::decode(&bytes)?;
            decoded.as_slice()
        } else {
            &bytes
        };
        font_core::variations::verify(sfnt, &expected)?;
    }
    let filename: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
        .take(80)
        .collect();
    let filename = if filename.is_empty() {
        "Fonts".into()
    } else {
        filename
    };
    let dialog = rfd::AsyncFileDialog::new()
        .set_parent(&parent)
        .set_title(format.label())
        .set_file_name(format!("{filename}.{}", format.extension()));
    #[cfg(not(target_os = "macos"))]
    let dialog = dialog.add_filter(format.label(), &[format.extension()]);
    let path = dialog
        .save_file()
        .await
        .map(|file| file.path().to_path_buf());
    if let Some(path) = path {
        let path = if path.extension().is_none() {
            let path = path.with_extension(format.extension());
            if path.exists() {
                return Err("font_output_exists".into());
            }
            path
        } else {
            path
        };
        if !path.extension().is_some_and(|s| {
            s.to_string_lossy().eq_ignore_ascii_case(format.extension())
                || ((format == Format::Cid || format == Format::Pfa)
                    && s.to_string_lossy().eq_ignore_ascii_case("ps"))
        }) {
            return Err("font_extension_mismatch".into());
        }
        if protected(&path)? {
            return Err("font_protected_location".into());
        }
        let report = ExportReport {
            path: path.clone(),
            bytes: bytes.len(),
            verified: verify,
        };
        tauri::async_runtime::spawn_blocking(move || {
            font_core::storage::write_bytes(&path, &bytes).map_err(|e| e.to_string())?;
            if verify {
                let saved = font_core::font_io::read(&path)?;
                if saved != bytes {
                    return Err(if format == Format::Woff2 {
                        "woff2_invalid"
                    } else {
                        "variation_invalid_output"
                    }
                    .into());
                }
                if format == Format::Woff2 {
                    font_core::font_formats::woff2::decode(&saved)?;
                }
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| e.to_string())??;
        return Ok(Some(report));
    }
    Ok(None)
}
