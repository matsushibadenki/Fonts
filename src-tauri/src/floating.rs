//! Native geometry is sampled on the main thread; only bounded metadata lives in Workspace.
use crate::AppState;
use font_core::{
    floating_bounds::{self, Bounds, Monitor, Rect},
    workspace::{Panel, Placement},
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{
    Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
#[derive(Default)]
struct Control {
    dirty: BTreeMap<Panel, Instant>,
    monitors: Vec<Monitor>,
}
#[derive(Default)]
pub struct Runtime {
    control: Mutex<Control>,
    queued: AtomicBool,
    stop: AtomicBool,
}
fn convert(m: &tauri::Monitor) -> Monitor {
    let r = m.work_area();
    Monitor {
        name: m.name().cloned(),
        scale: m.scale_factor(),
        work_area: Rect {
            x: f64::from(r.position.x),
            y: f64::from(r.position.y),
            width: f64::from(r.size.width),
            height: f64::from(r.size.height),
        },
    }
}
fn monitors(app: &tauri::AppHandle) -> Vec<Monitor> {
    let Some(w) = app.get_webview_window("main") else {
        return vec![];
    };
    let mut list = w
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(convert)
        .filter(|m| m.validate().is_ok())
        .collect::<Vec<_>>();
    list.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.work_area.x.total_cmp(&b.work_area.x))
            .then(a.work_area.y.total_cmp(&b.work_area.y))
    });
    list
}
fn fallback(app: &tauri::AppHandle, list: &[Monitor]) -> usize {
    app.get_webview_window("main")
        .and_then(|w| {
            w.current_monitor()
                .ok()
                .flatten()
                .or_else(|| w.primary_monitor().ok().flatten())
        })
        .map(|m| convert(&m))
        .and_then(|m| list.iter().position(|v| *v == m))
        .unwrap_or(0)
}
fn normal(w: &WebviewWindow) -> bool {
    !w.is_minimized().unwrap_or(true)
        && !w.is_maximized().unwrap_or(true)
        && !w.is_fullscreen().unwrap_or(true)
}
fn sample(w: &WebviewWindow, list: &[Monitor]) -> Option<Bounds> {
    if !normal(w) {
        return None;
    }
    let scale = w.scale_factor().ok()?;
    let pos = w.outer_position().ok()?;
    let inner = w.inner_size().ok()?.to_logical::<f64>(scale);
    let outer = w.outer_size().ok()?.to_logical::<f64>(scale);
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| convert(&m))
        .filter(|m| m.validate().is_ok())
        .or_else(|| {
            list.iter()
                .min_by(|a, b| {
                    let distance = |m: &Monitor| {
                        let r = m.work_area;
                        let x = f64::from(pos.x) - (r.x + r.width / 2.);
                        let y = f64::from(pos.y) - (r.y + r.height / 2.);
                        x * x + y * y
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .cloned()
        })?;
    let b = Bounds {
        position: [f64::from(pos.x), f64::from(pos.y)],
        inner_size: [inner.width, inner.height],
        outer_size: [outer.width.max(inner.width), outer.height.max(inner.height)],
        monitor,
    };
    b.validate().ok()?;
    Some(b)
}
fn saved(app: &tauri::AppHandle, panel: Panel) -> Option<Bounds> {
    app.state::<AppState>()
        .0
        .lock()
        .ok()?
        .workspace
        .floating_bounds
        .get(&panel)
        .cloned()
}
fn store(app: &tauri::AppHandle, panel: Panel, b: Bounds) -> bool {
    let state = app.state::<AppState>();
    let Ok(mut m) = state.0.lock() else {
        return false;
    };
    if !matches!(
        m.workspace.panels.get(&panel),
        Some(Placement::Floating { .. })
    ) || m.workspace.floating_bounds.get(&panel) == Some(&b)
    {
        return false;
    }
    // Geometry does not invalidate optimistic layout commands or trigger WebView redraws.
    m.workspace.floating_bounds.insert(panel, b);
    true
}
fn apply(
    w: &WebviewWindow,
    mut b: Bounds,
    list: &[Monitor],
    fallback: usize,
) -> Result<Bounds, String> {
    // Decoration dimensions may change with OS/theme. Keep the saved inner size, use current chrome.
    if let Some(current) = sample(w, list) {
        for i in 0..2 {
            b.outer_size[i] = b.inner_size[i] + (current.outer_size[i] - current.inner_size[i]);
        }
    }
    let b = floating_bounds::recover(&b, list, fallback).map_err(|e| e.to_string())?;
    w.set_min_size(Some(LogicalSize::new(
        260f64.min(b.inner_size[0]),
        240f64.min(b.inner_size[1]),
    )))
    .map_err(|e| e.to_string())?;
    let current = w
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(w.scale_factor().map_err(|e| e.to_string())?);
    if (current.width - b.inner_size[0]).abs() > 0.5
        || (current.height - b.inner_size[1]).abs() > 0.5
    {
        w.set_size(LogicalSize::new(b.inner_size[0], b.inner_size[1]))
            .map_err(|e| e.to_string())?;
    }
    let position = w.outer_position().map_err(|e| e.to_string())?;
    let target = PhysicalPosition::new(b.position[0].round() as i32, b.position[1].round() as i32);
    if position != target {
        w.set_position(target).map_err(|e| e.to_string())?;
    }
    Ok(b)
}
/// Build hidden, fit using actual native decorations, then reveal only after state is committed.
pub fn create(
    app: &tauri::AppHandle,
    panel: Panel,
    previous: Option<&Bounds>,
) -> Result<(WebviewWindow, Option<Bounds>), String> {
    let w = WebviewWindowBuilder::new(
        app,
        panel.label(),
        WebviewUrl::App(format!("index.html?panel={}", panel.label()).into()),
    )
    .disable_drag_drop_handler()
    .title(format!("Fonts — {:?}", panel))
    .inner_size(320., 480.)
    .min_inner_size(1., 1.)
    .visible(false)
    .focused(false)
    .center()
    .build()
    .map_err(|e| e.to_string())?;
    let list = monitors(app);
    if list.is_empty() {
        return Ok((w, None));
    }
    let main = fallback(app, &list);
    let desired = previous.cloned().or_else(|| {
        sample(&w, &list).map(|mut b| {
            b.monitor = list[main].clone();
            let r = b.monitor.work_area;
            let s = b.monitor.scale;
            b.position = [
                r.x + (r.width - b.outer_size[0] * s) / 2.,
                r.y + (r.height - b.outer_size[1] * s) / 2.,
            ];
            b
        })
    });
    match desired.map(|b| apply(&w, b, &list, main)).transpose() {
        Ok(b) => Ok((w, b)),
        Err(e) => {
            let _ = w.destroy();
            Err(e)
        }
    }
}
fn redock_failed(app: &tauri::AppHandle, panel: Panel) {
    if let Ok(mut m) = app.state::<AppState>().0.lock() {
        let _ = m.workspace.redock(panel);
    }
    if let Some(w) = app.get_webview_window(panel.label()) {
        let _ = w.destroy();
    }
    let locale = app
        .state::<AppState>()
        .0
        .lock()
        .map(|m| m.workspace.locale)
        .unwrap_or_default();
    let message = match locale {
        font_core::workspace::Locale::En => {
            "The panel could not be restored and has been returned to its dock."
        }
        font_core::workspace::Locale::Ja => "パネルを復元できなかったため、ドックへ戻しました。",
        font_core::workspace::Locale::ZhCn => "无法恢复面板，已将其放回停靠区。",
    };
    let _ = app.emit("canvas-error", message);
}
pub fn reveal(app: &tauri::AppHandle, panel: Panel, focus: bool) {
    if let Some(w) = app.get_webview_window(panel.label()) {
        if w.show().is_err() {
            redock_failed(app, panel);
        } else if focus {
            let _ = w.set_focus();
        }
    }
}
pub fn restore(app: &tauri::AppHandle) {
    for p in Panel::ALL {
        let floating = app
            .state::<AppState>()
            .0
            .lock()
            .map(|m| matches!(m.workspace.panels[&p], Placement::Floating { .. }))
            .unwrap_or(false);
        if !floating {
            continue;
        }
        match create(app, p, saved(app, p).as_ref()) {
            Ok((_, b)) => {
                if let Some(b) = b {
                    store(app, p, b);
                }
                reveal(app, p, false);
            }
            Err(_) => redock_failed(app, p),
        }
    }
    let list = monitors(app);
    if let Ok(mut c) = app.state::<Runtime>().control.lock() {
        c.monitors = list;
    }
    crate::persist(app);
}
pub fn capture(app: &tauri::AppHandle, panel: Panel) {
    if let Some(w) = app.get_webview_window(panel.label()) {
        if let Some(b) = sample(&w, &monitors(app)) {
            store(app, panel, b);
        }
    }
}
pub fn flush(app: &tauri::AppHandle) {
    for panel in Panel::ALL {
        capture(app, panel);
    }
}
pub fn event(app: &tauri::AppHandle, label: &str, event: &tauri::WindowEvent) {
    if matches!(
        event,
        tauri::WindowEvent::Moved(_)
            | tauri::WindowEvent::Resized(_)
            | tauri::WindowEvent::ScaleFactorChanged { .. }
    ) {
        if let Some(panel) = Panel::from_label(label) {
            if let Some(runtime) = app.try_state::<Runtime>() {
                if let Ok(mut c) = runtime.control.lock() {
                    c.dirty.insert(panel, Instant::now());
                }
            }
        }
    }
}
fn tick(app: &tauri::AppHandle) {
    if crate::EXIT_CONFIRMED.load(Ordering::Relaxed) {
        return;
    }
    let list = monitors(app);
    if list.is_empty() {
        return;
    }
    let main = fallback(app, &list);
    let state = app.state::<Runtime>();
    let (changed, due) = {
        let Ok(mut c) = state.control.lock() else {
            return;
        };
        let changed = c.monitors != list;
        c.monitors = list.clone();
        let due = c
            .dirty
            .iter()
            .filter(|(_, t)| t.elapsed() >= Duration::from_millis(300))
            .map(|(p, _)| *p)
            .collect::<Vec<_>>();
        for p in &due {
            c.dirty.remove(p);
        }
        (changed, due)
    };
    let mut dirty = false;
    for p in Panel::ALL {
        if !changed && !due.contains(&p) {
            continue;
        }
        let floating = app
            .state::<AppState>()
            .0
            .lock()
            .map(|m| matches!(m.workspace.panels[&p], Placement::Floating { .. }))
            .unwrap_or(false);
        if !floating {
            continue;
        }
        let Some(w) = app.get_webview_window(p.label()) else {
            continue;
        };
        if !normal(&w) {
            continue;
        }
        let candidate = if changed {
            saved(app, p).or_else(|| sample(&w, &list))
        } else {
            sample(&w, &list)
        };
        if let Some(b) = candidate {
            match apply(&w, b, &list, main) {
                Ok(b) => dirty |= store(app, p, b),
                Err(_) => {
                    redock_failed(app, p);
                    dirty = true;
                    crate::publish(app);
                }
            }
        }
    }
    if dirty {
        crate::persist(app);
    }
}
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        let Some(s) = app.try_state::<Runtime>() else {
            break;
        };
        if s.stop.load(Ordering::Acquire) {
            break;
        }
        if s.queued.swap(true, Ordering::AcqRel) {
            continue;
        }
        let handle = app.clone();
        if app
            .run_on_main_thread(move || {
                if !handle.state::<Runtime>().stop.load(Ordering::Acquire) {
                    tick(&handle);
                }
                handle
                    .state::<Runtime>()
                    .queued
                    .store(false, Ordering::Release);
            })
            .is_err()
        {
            s.queued.store(false, Ordering::Release);
            break;
        }
    });
}
pub fn stop(app: &tauri::AppHandle) {
    if let Some(s) = app.try_state::<Runtime>() {
        s.stop.store(true, Ordering::Release);
    }
}
