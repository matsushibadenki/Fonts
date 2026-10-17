//! Adapted from LumaPaint canvas_macos.rs; source hash recorded in docs/reuse-native.json.
use super::CanvasRequest;
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize};
pub(super) fn native_frame(
    bounds: NSRect,
    safe: NSEdgeInsets,
    flipped: bool,
    request: &CanvasRequest,
) -> Option<NSRect> {
    // WKWebView's CSS viewport starts inside its safe area, while child NSViews use
    // the full native bounds (including the macOS title bar). Respect both origins.
    let content_width = bounds.size.width - safe.left - safe.right;
    let content_height = bounds.size.height - safe.top - safe.bottom;
    let left = request.x.max(0.0);
    let top = request.y.max(0.0);
    let width = (request.x + request.width).min(content_width) - left;
    let height = (request.y + request.height).min(content_height) - top;
    if width < 1.0 || height < 1.0 {
        return None;
    }
    let y = if flipped {
        safe.top + top
    } else {
        safe.bottom + content_height - top - height
    };
    Some(NSRect::new(
        NSPoint::new(bounds.origin.x + safe.left + left, bounds.origin.y + y),
        NSSize::new(width, height),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn css_canvas_respects_safe_area_and_flipped_coordinate_system() {
        let bounds = NSRect::new(NSPoint::new(10., 20.), NSSize::new(1000., 800.));
        let safe = NSEdgeInsets {
            top: 30.,
            bottom: 10.,
            left: 5.,
            right: 5.,
        };
        let request = CanvasRequest {
            x: 100.,
            y: 40.,
            width: 300.,
            height: 200.,
            visible: true,
        };
        let flipped = native_frame(bounds, safe, true, &request).unwrap();
        assert_eq!(flipped.origin, NSPoint::new(115., 90.));
        let normal = native_frame(bounds, safe, false, &request).unwrap();
        assert_eq!(normal.origin, NSPoint::new(115., 550.));
        let outside = CanvasRequest {
            x: 2000.,
            ..request
        };
        assert!(native_frame(bounds, safe, true, &outside).is_none());
    }
}
