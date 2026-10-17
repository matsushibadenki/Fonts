//! Native NSView lifetime is confined to the AppKit main thread.
use crate::{model::Model, CanvasRequest};
use font_renderer::{wgpu, Renderer};
use objc2::{define_class, msg_send, rc::Retained, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use std::{cell::RefCell, ffi::c_void, ptr::NonNull};

#[path = "native_frame.rs"]
mod frame;

define_class!(
    #[unsafe(super(NSView))]
    #[ivars = ()]
    struct FontCanvasView;
    impl FontCanvasView {
        // Rust draws in a native child, while the WebView receives tiny pointer commands.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self,_point:NSPoint)->*mut NSView {std::ptr::null_mut()}
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self)->bool {true}
    }
);
impl FontCanvasView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        // SAFETY: allocated and initialized once, only on the main thread.
        let view = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(view),initWithFrame:frame] }
    }
}
struct Canvas {
    // Drop GPU surface BEFORE the retained native view, as in LumaPaint.
    renderer: Renderer,
    instance: wgpu::Instance,
    view: Retained<FontCanvasView>,
    size: [u32; 2],
}
impl Drop for Canvas {
    fn drop(&mut self) {
        self.view.removeFromSuperview();
    }
}
thread_local! {static CANVAS:RefCell<Option<Canvas>>=const {RefCell::new(None)};}
pub fn destroy() {
    CANVAS.with(|s| {
        s.borrow_mut().take();
    });
}
pub fn redraw(model: &Model) -> Result<(), String> {
    CANVAS.with(|s| {
        if let Some(c) = s.borrow_mut().as_mut() {
            draw(c, model)?;
        }
        Ok(())
    })
}
pub fn sync(
    parent: *mut c_void,
    request: CanvasRequest,
    model: &mut Model,
) -> Result<String, String> {
    let mtm = MainThreadMarker::new().ok_or("requires_main_thread")?;
    if !request.visible || request.width < 1. || request.height < 1. {
        CANVAS.with(|s| {
            if let Some(c) = s.borrow().as_ref() {
                c.view.setHidden(true);
            }
        });
        return Ok("hidden".into());
    }
    if ![request.x, request.y, request.width, request.height]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err("invalid_viewport".into());
    }
    // SAFETY: Tauri lends a live WKWebView in with_webview on the UI thread.
    let parent = unsafe { parent.cast::<NSView>().as_ref() }.ok_or("missing_webview")?;
    let frame = frame::native_frame(
        parent.bounds(),
        parent.safeAreaInsets(),
        parent.isFlipped(),
        &request,
    )
    .ok_or("empty_viewport")?;
    let scale = parent
        .window()
        .ok_or("missing_window")?
        .backingScaleFactor();
    let size = [
        (frame.size.width * scale).round().max(1.) as u32,
        (frame.size.height * scale).round().max(1.) as u32,
    ];
    if size.iter().any(|s| *s > 16384) {
        return Err("invalid_surface_size".into());
    }
    model.size = [frame.size.width, frame.size.height];
    CANVAS.with(|s| {
        let mut slot = s.borrow_mut();
        if slot.is_none() {
            let view = FontCanvasView::new(mtm, frame);
            view.setClipsToBounds(true);
            parent.addSubview(&view);
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::METAL,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            let handle = AppKitWindowHandle::new(NonNull::from(&*view).cast());
            // SAFETY: Canvas retains this NSView until after Renderer and its surface drop.
            let surface = unsafe {
                instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())),
                    raw_window_handle: RawWindowHandle::AppKit(handle),
                })
            }
            .map_err(|e| e.to_string())?;
            let renderer = match pollster::block_on(Renderer::new(&instance, surface, size)) {
                Ok(r) => r,
                Err(e) => {
                    view.removeFromSuperview();
                    return Err(e);
                }
            };
            *slot = Some(Canvas {
                renderer,
                instance,
                view,
                size,
            });
        }
        let c = slot.as_mut().unwrap();
        c.size = size;
        c.view.setFrame(frame);
        c.view.setHidden(false);
        draw(c, model)?;
        Ok(format!("Metal · {}", c.renderer.gpu.adapter_name))
    })
}

fn draw(c: &mut Canvas, model: &Model) -> Result<(), String> {
    match draw_frame(c, model) {
        Err(error) if error == "surface_lost" => {
            let handle = AppKitWindowHandle::new(NonNull::from(&*c.view).cast());
            // SAFETY: Canvas retains its NSView until after the renderer's surface drops.
            let surface = unsafe {
                c.instance
                    .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                        raw_display_handle: Some(RawDisplayHandle::AppKit(
                            AppKitDisplayHandle::new(),
                        )),
                        raw_window_handle: RawWindowHandle::AppKit(handle),
                    })
            }
            .map_err(|e| e.to_string())?;
            c.renderer.replace_surface(surface);
            draw_frame(c, model)
        }
        result => result,
    }
}

fn draw_frame(c: &mut Canvas, model: &Model) -> Result<(), String> {
    if model.proof.active {
        if let Some(font) = model.proof.font.as_ref() {
            return c.renderer.draw_proof(
                c.size,
                model.size,
                font,
                model.proof.reference.as_deref(),
                &model.proof.settings,
            );
        }
    }
    c.renderer.draw(
        c.size,
        &model.visible_glyph(),
        model.viewport(),
        font_renderer::DrawOptions {
            nodes: model.nodes,
            fill: model.fill,
        },
        font_renderer::EditorOverlay {
            node: model.selected,
            images: &model.visible_images(),
            assets: model.editor.document().reference_images.assets(),
            selected_image: (model.tool == font_core::workspace::Tool::Images)
                .then_some(model.images.selected)
                .flatten(),
        },
    )
}
