use font_core::{
    floating_bounds::{recover, Bounds, Monitor, Rect},
    workspace::{Dock, Panel, Placement, Workspace},
};
fn monitor(name: &str, x: f64, y: f64, w: f64, h: f64, scale: f64) -> Monitor {
    Monitor {
        name: Some(name.into()),
        work_area: Rect {
            x,
            y,
            width: w,
            height: h,
        },
        scale,
    }
}
fn bounds(m: &Monitor) -> Bounds {
    Bounds {
        position: [
            m.work_area.x + 100. * m.scale,
            m.work_area.y + 100. * m.scale,
        ],
        inner_size: [320., 480.],
        outer_size: [320., 508.],
        monitor: m.clone(),
    }
}
fn contained(b: &Bounds) {
    let r = b.physical_rect();
    let a = b.monitor.work_area;
    assert!(
        r.x >= a.x - 0.001
            && r.y >= a.y - 0.001
            && r.x + r.width <= a.x + a.width + 0.001
            && r.y + r.height <= a.y + a.height + 0.001,
        "{b:?}"
    );
}
#[test]
fn unchanged_monitor_restores_exact_position_and_inner_size() {
    let m = monitor("Main", 0., 24., 1920., 1000., 1.);
    let b = bounds(&m);
    assert_eq!(recover(&b, &[m], 0).unwrap(), b);
}
#[test]
fn disconnected_display_returns_to_main_including_entire_title_bar() {
    let old = monitor("External", 1920., 0., 2560., 1440., 1.);
    let b = bounds(&old);
    let main = monitor("Main", 0., 24., 1440., 820., 2.);
    let result = recover(&b, std::slice::from_ref(&main), 0).unwrap();
    assert_eq!(result.monitor, main);
    contained(&result);
    assert_eq!(result.outer_size[1] - result.inner_size[1], 28.);
}
#[test]
fn moved_display_and_dpi_change_preserve_logical_offset_and_size() {
    let old = monitor("External", 1920., 0., 2560., 1600., 2.);
    let b = bounds(&old);
    let now = monitor("External", -2560., 50., 2560., 1600., 1.25);
    let result = recover(&b, std::slice::from_ref(&now), 0).unwrap();
    assert_eq!(result.position, [-2435., 175.]);
    assert_eq!(result.inner_size, b.inner_size);
    assert_eq!(result.outer_size, b.outer_size);
    contained(&result);
}
#[test]
fn resolution_shrink_fits_frame_without_minimum_size_leaving_the_screen() {
    let old = monitor("Main", 0., 24., 3840., 2120., 2.);
    let mut b = bounds(&old);
    b.position = [3200., 1600.];
    b.inner_size = [1400., 900.];
    b.outer_size = [1416., 940.];
    for (w, h, scale) in [(640., 360., 2.), (200., 150., 1.), (32., 32., 2.)] {
        let result = recover(&b, &[monitor("Main", 0., 24., w, h, scale)], 0).unwrap();
        contained(&result);
        assert!(result.inner_size[0] >= 1. && result.inner_size[1] >= 1.);
    }
}
#[test]
fn gaps_negative_coordinates_and_duplicate_names_are_handled() {
    let left = monitor("Same", -1920., -500., 1920., 1080., 1.);
    let right = monitor("Same", 500., 24., 1920., 1080., 1.);
    let b = bounds(&left);
    let result = recover(&b, &[right.clone(), left.clone()], 0).unwrap();
    assert_eq!(result, b);
    let old = monitor("Removed", 0., 24., 400., 800., 1.);
    let mut gap = bounds(&old);
    gap.position = [100., 100.];
    let result = recover(&gap, &[left, right], 1).unwrap();
    assert_eq!(result.monitor.work_area.x, 500.);
    contained(&result);
}
#[test]
fn partial_visibility_and_screens_without_names_choose_an_actual_work_area() {
    let mut m = monitor("", 0., 24., 1200., 800., 1.);
    m.name = None;
    let mut b = bounds(&m);
    b.position = [-100., -300.];
    let result = recover(&b, &[m], 0).unwrap();
    contained(&result);
    assert!(result.position[1] >= 32.);
}
#[test]
fn invalid_values_are_rejected_and_bad_saved_bounds_do_not_erase_layout() {
    let m = monitor("Main", 0., 24., 1440., 850., 1.);
    let b = bounds(&m);
    for invalid in [f64::NAN, f64::INFINITY, -1., 0., 1e30] {
        let mut bad = b.clone();
        bad.inner_size[0] = invalid;
        assert!(recover(&bad, std::slice::from_ref(&m), 0).is_err());
    }
    assert!(recover(&b, &[], 0).is_err());
    let mut bad = b.clone();
    bad.position[0] = 1e30;
    let mut w = Workspace {
        left_width: 300.,
        ..Workspace::default()
    };
    w.floating_bounds.insert(Panel::Metrics, bad);
    w.place(
        0,
        Panel::Metrics,
        Placement::Floating {
            return_to: Dock::Right,
        },
    )
    .unwrap();
    let w = w.restore().unwrap();
    assert!(w.floating_bounds.is_empty());
    assert_eq!(w.left_width, 300.);
    assert!(matches!(
        w.panels[&Panel::Metrics],
        Placement::Floating { .. }
    ));
}
#[test]
fn bounds_roundtrip_survives_redock_refloat_and_old_workspace_migration() {
    let mut w = Workspace::default();
    let b = bounds(&monitor("Main", 0., 24., 1920., 1000., 1.));
    w.floating_bounds.insert(Panel::Metrics, b.clone());
    w.place(
        0,
        Panel::Metrics,
        Placement::Floating {
            return_to: Dock::Right,
        },
    )
    .unwrap();
    let restored: Workspace = serde_json::from_slice(&serde_json::to_vec(&w).unwrap()).unwrap();
    let mut restored = restored.restore().unwrap();
    assert_eq!(restored.floating_bounds[&Panel::Metrics], b);
    assert!(matches!(
        restored.panels[&Panel::Metrics],
        Placement::Floating { .. }
    ));
    restored.redock(Panel::Metrics).unwrap();
    assert_eq!(restored.floating_bounds[&Panel::Metrics], b);
    restored
        .place(
            restored.revision,
            Panel::Metrics,
            Placement::Floating {
                return_to: Dock::Right,
            },
        )
        .unwrap();
    let mut old = serde_json::to_value(&restored).unwrap();
    old.as_object_mut().unwrap().remove("floating_bounds");
    let old: Workspace = serde_json::from_value(old).unwrap();
    assert!(matches!(
        old.restore().unwrap().panels[&Panel::Metrics],
        Placement::Floating { .. }
    ));
}
#[test]
fn repeated_recovery_is_idempotent_and_does_not_drift() {
    let old = monitor("External", 3000., -200., 3000., 1800., 2.);
    let mut b = bounds(&old);
    b.inner_size = [800., 1000.];
    b.outer_size = [816., 1040.];
    let now = monitor("Main", 0., 24., 1280., 740., 1.5);
    let first = recover(&b, std::slice::from_ref(&now), 0).unwrap();
    let mut result = first.clone();
    for _ in 0..100 {
        result = recover(&result, std::slice::from_ref(&now), 0).unwrap();
    }
    assert_eq!(result, first);
    contained(&result);
}
