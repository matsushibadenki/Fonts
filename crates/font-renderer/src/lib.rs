//! GPU resources and outline meshes stay in Rust. No frames cross the IPC boundary.
use font_core::{geometry, Contour, Glyph, Point};
pub use wgpu;
use wgpu::util::DeviceExt;
mod reference;
mod thumbnail;
pub use reference::EditorOverlay;

#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
    pub pan: Point,
    pub advance: f64,
    pub ascender: f64,
    pub descender: f64,
}
impl Viewport {
    pub fn scale(self) -> f64 {
        ((self.width - 100.) / self.advance.max(600.))
            .min((self.height - 100.) / (self.ascender - self.descender).max(1000.))
            .max(0.02)
            * self.zoom
    }
    pub fn screen(self, p: Point) -> Point {
        [
            self.width / 2. + self.pan[0] + (p[0] - self.advance / 2.) * self.scale(),
            self.height / 2. + self.pan[1]
                - (p[1] - (self.ascender + self.descender) / 2.) * self.scale(),
        ]
    }
    pub fn document(self, p: Point) -> Point {
        [
            (p[0] - self.width / 2. - self.pan[0]) / self.scale() + self.advance / 2.,
            -(p[1] - self.height / 2. - self.pan[1]) / self.scale()
                + (self.ascender + self.descender) / 2.,
        ]
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}
const INK: [f32; 4] = [0.12, 0.13, 0.14, 1.];
const ACCENT: [f32; 4] = [0.67, 0.34, 0.24, 1.];
const GUIDE: [f32; 4] = [0.76, 0.77, 0.78, 1.];
struct Mesh {
    images: Vec<reference::ImageQuad>,
    triangles: Vec<Vertex>,
    decoration: Vec<Vertex>,
    cover: Vec<Vertex>,
    clip: Option<[u32; 4]>,
}
fn vertex(v: Viewport, p: Point, color: [f32; 4]) -> Vertex {
    Vertex {
        position: [
            (p[0] / v.width * 2. - 1.) as f32,
            (1. - p[1] / v.height * 2.) as f32,
        ],
        color,
    }
}
fn rectangle(out: &mut Vec<Vertex>, v: Viewport, a: Point, b: Point, color: [f32; 4]) {
    for p in [a, [b[0], a[1]], b, a, b, [a[0], b[1]]] {
        out.push(vertex(v, p, color));
    }
}
fn line(out: &mut Vec<Vertex>, v: Viewport, a: Point, b: Point, width: f64, color: [f32; 4]) {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]);
    if len < 1e-8 {
        return;
    }
    let n = [-d[1] / len * width / 2., d[0] / len * width / 2.];
    let corners = [
        [a[0] + n[0], a[1] + n[1]],
        [a[0] - n[0], a[1] - n[1]],
        [b[0] - n[0], b[1] - n[1]],
        [b[0] + n[0], b[1] + n[1]],
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(vertex(v, corners[i], color));
    }
}
fn mesh(contours: &[Contour], v: Viewport, nodes: bool, fill: bool) -> Result<Mesh, String> {
    mesh_options(contours, v, nodes, fill, true)
}
fn mesh_options(
    contours: &[Contour],
    v: Viewport,
    nodes: bool,
    fill: bool,
    guides_enabled: bool,
) -> Result<Mesh, String> {
    let mut m = Mesh {
        images: vec![],
        triangles: vec![],
        decoration: vec![],
        cover: vec![],
        clip: None,
    };
    // Glyph previews use even-odd stencil filling, including concave contours and holes.
    let mut budget = 100_000usize;
    for contour in contours {
        let path =
            geometry::display_path(std::slice::from_ref(contour)).map_err(|e| e.to_string())?;
        let cubic = lumapaint_core::bezier::cubic_contours(&path)?;
        let mut at = cubic[0].0[0];
        let mut estimated = 1usize;
        for segment in cubic[0].0[1..].as_chunks::<3>().0 {
            let length: f32 = [at, segment[0], segment[1], segment[2]]
                .windows(2)
                .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
                .sum();
            estimated = estimated.saturating_add((length / 2.).ceil().clamp(8., 4096.) as usize);
            if estimated > budget {
                return Err("outline_too_complex".into());
            }
            at = segment[2];
        }
        budget -= estimated;
        let mut flat = lumapaint_core::bezier::flattened(&cubic[0].0);
        if flat.len() > 100_000 {
            return Err("outline_too_complex".into());
        }
        if contour.closed && flat.first() != flat.last() {
            flat.push(flat[0]);
        }
        let screen: Vec<_> = flat
            .iter()
            .map(|p| v.screen([p[0] as f64, -p[1] as f64]))
            .collect();
        if fill && contour.closed {
            for i in 1..screen.len().saturating_sub(1) {
                for p in [screen[0], screen[i], screen[i + 1]] {
                    m.triangles.push(vertex(v, p, INK));
                }
            }
        }
        for p in screen.windows(2) {
            line(
                &mut m.decoration,
                v,
                p[0],
                p[1],
                1.3,
                if nodes { INK } else { ACCENT },
            );
        }
        if nodes {
            let mut at = contour.start;
            for segment in &contour.segments {
                match *segment {
                    font_core::Segment::Line { end } => at = end,
                    font_core::Segment::Quadratic { control, end } => {
                        line(
                            &mut m.decoration,
                            v,
                            v.screen(at),
                            v.screen(control),
                            0.8,
                            GUIDE,
                        );
                        line(
                            &mut m.decoration,
                            v,
                            v.screen(end),
                            v.screen(control),
                            0.8,
                            GUIDE,
                        );
                        at = end;
                    }
                    font_core::Segment::Cubic {
                        control1,
                        control2,
                        end,
                    } => {
                        line(
                            &mut m.decoration,
                            v,
                            v.screen(at),
                            v.screen(control1),
                            0.8,
                            GUIDE,
                        );
                        line(
                            &mut m.decoration,
                            v,
                            v.screen(end),
                            v.screen(control2),
                            0.8,
                            GUIDE,
                        );
                        at = end;
                    }
                }
            }
            for (kind, p) in geometry::edit_points(contour) {
                if matches!(kind, geometry::EditPoint::Anchor(_)) {
                    continue;
                }
                let p = v.screen(p);
                for i in 0..16 {
                    let angle = |i: usize| i as f64 * std::f64::consts::TAU / 16.;
                    let q = |i| [p[0] + angle(i).cos() * 3.5, p[1] + angle(i).sin() * 3.5];
                    for point in [p, q(i), q(i + 1)] {
                        m.decoration.push(vertex(v, point, ACCENT));
                    }
                }
            }
            for p in anchors(contour) {
                let p = v.screen(p);
                rectangle(
                    &mut m.decoration,
                    v,
                    [p[0] - 3., p[1] - 3.],
                    [p[0] + 3., p[1] + 3.],
                    ACCENT,
                );
            }
        }
    }
    if guides_enabled {
        // Draw guides first by prepending their vertices.
        let mut guides = Vec::new();
        for y in [v.ascender, 0., v.descender] {
            let y = v.screen([0., y])[1];
            line(
                &mut guides,
                v,
                [0., y],
                [v.width, y],
                if y == v.screen([0., 0.])[1] { 1. } else { 0.6 },
                GUIDE,
            );
        }
        for x in [0., v.advance] {
            let x = v.screen([x, 0.])[0];
            line(
                &mut guides,
                v,
                [x, 0.],
                [x, v.height],
                0.6,
                [0.87, 0.86, 0.84, 1.],
            );
        }
        guides.append(&mut m.decoration);
        m.decoration = guides;
    }
    rectangle(&mut m.cover, v, [0., 0.], [v.width, v.height], INK);
    Ok(m)
}
pub fn anchors(c: &Contour) -> Vec<Point> {
    std::iter::once(c.start)
        .chain(c.segments.iter().map(|s| match *s {
            font_core::Segment::Line { end }
            | font_core::Segment::Quadratic { end, .. }
            | font_core::Segment::Cubic { end, .. } => end,
        }))
        .collect()
}

#[derive(Clone, Copy)]
pub struct DrawOptions {
    pub nodes: bool,
    pub fill: bool,
}
struct Attachments {
    size: [u32; 2],
    format: wgpu::TextureFormat,
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
}
pub struct Gpu {
    images: std::cell::RefCell<reference::ImageGpu>,
    attachments: std::cell::RefCell<Option<Attachments>>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    stencil_pipeline: wgpu::RenderPipeline,
    fill_pipeline: wgpu::RenderPipeline,
    lines_pipeline: wgpu::RenderPipeline,
    pub adapter_name: String,
}
impl Gpu {
    pub async fn new(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: surface,
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Fonts GPU"),
                required_limits: wgpu::Limits {
                    max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d.min(16384),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glyph"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glyph.wgsl").into()),
        });
        let pipeline = |mode: u8| {
            let face = wgpu::StencilFaceState {
                compare: if mode == 1 {
                    wgpu::CompareFunction::NotEqual
                } else {
                    wgpu::CompareFunction::Always
                },
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: if mode == 0 {
                    wgpu::StencilOperation::Invert
                } else if mode == 1 {
                    wgpu::StencilOperation::Zero
                } else {
                    wgpu::StencilOperation::Keep
                },
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("glyph stencil/fill/lines"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x4],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: if mode == 0 {
                            wgpu::ColorWrites::empty()
                        } else {
                            wgpu::ColorWrites::ALL
                        },
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState {
                        front: face,
                        back: face,
                        read_mask: 0xff,
                        write_mask: if mode <= 1 { 0xff } else { 0 },
                    },
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 4,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let stencil_pipeline = pipeline(0);
        let fill_pipeline = pipeline(1);
        let lines_pipeline = pipeline(2);
        Ok(Self {
            images: std::cell::RefCell::new(reference::ImageGpu::new(&device, format)),
            attachments: std::cell::RefCell::new(None),
            device,
            queue,
            stencil_pipeline,
            fill_pipeline,
            lines_pipeline,
            adapter_name,
        })
    }
    pub fn render(
        &self,
        target: &wgpu::TextureView,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        contours: &[Contour],
        viewport: Viewport,
        options: DrawOptions,
    ) -> Result<(), String> {
        self.render_meshes(
            target,
            size,
            format,
            vec![mesh(contours, viewport, options.nodes, options.fill)?],
        )
    }
    fn render_meshes(
        &self,
        target: &wgpu::TextureView,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        meshes: Vec<Mesh>,
    ) -> Result<(), String> {
        let mut images = self.images.borrow_mut();
        let image_buffers = images.prepare(&self.device, &self.queue, &meshes)?;
        let buffer = |label, vertices: &[Vertex]| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(if vertices.is_empty() {
                        &[Vertex {
                            position: [0., 0.],
                            color: INK,
                        }]
                    } else {
                        vertices
                    }),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        };
        let buffers: Vec<_> = meshes
            .iter()
            .map(|m| {
                (
                    buffer("outline", &m.triangles),
                    buffer("cover", &m.cover),
                    buffer("guides/nodes", &m.decoration),
                )
            })
            .collect();
        let texture = |format| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("glyph 4x attachments"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 4,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        let mut attachments = self.attachments.borrow_mut();
        if attachments
            .as_ref()
            .is_none_or(|a| a.size != size || a.format != format)
        {
            *attachments = Some(Attachments {
                size,
                format,
                msaa: texture(format).create_view(&Default::default()),
                depth: texture(wgpu::TextureFormat::Depth24PlusStencil8)
                    .create_view(&Default::default()),
            });
        }
        let attachments = attachments.as_ref().unwrap();
        let msaa = &attachments.msaa;
        let depth = &attachments.depth;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("font frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa,
                    resolve_target: Some(target),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_stencil_reference(0);
            for ((m, (fans, cover, lines)), image_buffers) in
                meshes.iter().zip(&buffers).zip(&image_buffers)
            {
                let [x, y, w, h] = m.clip.unwrap_or([0, 0, size[0], size[1]]);
                pass.set_scissor_rect(x, y, w, h);
                pass.set_pipeline(&images.pipeline);
                for (quad, buffer) in m.images.iter().zip(image_buffers) {
                    pass.set_bind_group(0, &images.cache[quad.asset.id()], &[]);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..6, 0..1);
                }
                pass.set_pipeline(&self.stencil_pipeline);
                pass.set_vertex_buffer(0, fans.slice(..));
                pass.draw(0..m.triangles.len() as u32, 0..1);
                pass.set_pipeline(&self.fill_pipeline);
                pass.set_vertex_buffer(0, cover.slice(..));
                pass.draw(0..m.cover.len() as u32, 0..1);
                pass.set_pipeline(&self.lines_pipeline);
                pass.set_vertex_buffer(0, lines.slice(..));
                pass.draw(0..m.decoration.len() as u32, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
}
pub struct Renderer {
    pub gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}
impl Renderer {
    pub fn replace_surface(&mut self, surface: wgpu::Surface<'static>) {
        surface.configure(&self.gpu.device, &self.config);
        self.surface = surface;
    }

    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        size: [u32; 2],
    ) -> Result<Self, String> {
        // BGRA sRGB is supported by macOS Metal native surfaces.
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let gpu = Gpu::new(instance, Some(&surface), format).await?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size[0],
            height: size[1],
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&gpu.device, &config);
        Ok(Self {
            gpu,
            surface,
            config,
        })
    }
    pub fn draw(
        &mut self,
        size: [u32; 2],
        glyph: &Glyph,
        v: Viewport,
        options: DrawOptions,
        overlay: EditorOverlay<'_>,
    ) -> Result<(), String> {
        let mut m = mesh(&glyph.contours, v, options.nodes, options.fill)?;
        if options.nodes {
            if let Some((ci, point)) = overlay.node {
                if let Some(c) = glyph.contours.get(ci) {
                    if let Some((_, p)) = geometry::edit_points(c)
                        .into_iter()
                        .find(|(kind, _)| *kind == point)
                    {
                        let p = v.screen(p);
                        let color = [0.12, 0.42, 0.85, 1.];
                        for (a, b) in [
                            ([-6., -6.], [6., -6.]),
                            ([6., -6.], [6., 6.]),
                            ([6., 6.], [-6., 6.]),
                            ([-6., 6.], [-6., -6.]),
                        ] {
                            line(
                                &mut m.decoration,
                                v,
                                [p[0] + a[0], p[1] + a[1]],
                                [p[0] + b[0], p[1] + b[1]],
                                1.5,
                                color,
                            );
                        }
                    }
                }
            }
        }
        reference::add_images(&mut m, v, &overlay);
        self.draw_meshes(size, vec![m])
    }
    pub fn draw_proof(
        &mut self,
        size: [u32; 2],
        logical: [f64; 2],
        font: &font_core::proof::Prepared,
        reference: Option<&font_core::proof::Prepared>,
        settings: &font_core::proof::Settings,
    ) -> Result<(), String> {
        let width = if reference.is_some() {
            logical[0] / 2.
        } else {
            logical[0]
        };
        let mut meshes = Vec::new();
        let mut vertices = 0;
        for (column, font) in std::iter::once(font).chain(reference).enumerate() {
            let layout = font_core::proof::layout(font, settings, width);
            for placement in layout
                .placements
                .iter()
                .filter(|p| p.bottom >= settings.scroll && p.top - settings.scroll < logical[1])
            {
                let scale = settings.size / f64::from(font.document.units_per_em);
                let mut v = Viewport {
                    width: logical[0],
                    height: logical[1],
                    zoom: 1.,
                    pan: [0., 0.],
                    advance: 600.,
                    ascender: 800.,
                    descender: -200.,
                };
                v.zoom = scale / v.scale();
                v.pan = [
                    placement.x + column as f64 * width - logical[0] / 2. + 300. * scale,
                    placement.baseline - settings.scroll - logical[1] / 2. - 300. * scale,
                ];
                let mut m = mesh(
                    &font.document.glyphs[&placement.glyph].contours,
                    v,
                    false,
                    true,
                )?;
                m.decoration.clear();
                if placement.missing {
                    let a = [
                        placement.x + column as f64 * width,
                        placement.baseline - settings.scroll - settings.size * 0.7,
                    ];
                    let b = [
                        a[0] + settings.size * 0.5,
                        placement.baseline - settings.scroll,
                    ];
                    for (p, q) in [
                        (a, [b[0], a[1]]),
                        ([b[0], a[1]], b),
                        (b, [a[0], b[1]]),
                        ([a[0], b[1]], a),
                        (a, b),
                    ] {
                        line(&mut m.decoration, v, p, q, 1., ACCENT);
                    }
                }
                let pixel_x =
                    (column as f64 * width / logical[0] * f64::from(size[0])).round() as u32;
                let end = (((column + 1) as f64 * width / logical[0] * f64::from(size[0])).round()
                    as u32)
                    .min(size[0]);
                m.clip = Some([pixel_x, 0, end - pixel_x, size[1]]);
                vertices += m.triangles.len();
                if vertices > 4_000_000 {
                    return Err("proof_too_complex".into());
                }
                meshes.push(m);
            }
        }
        self.draw_meshes(size, meshes)
    }
    fn draw_meshes(&mut self, size: [u32; 2], meshes: Vec<Mesh>) -> Result<(), String> {
        if size[0] == 0 || size[1] == 0 || size[0] > 16384 || size[1] > 16384 {
            return Err("invalid_surface_size".into());
        }
        if [self.config.width, self.config.height] != size {
            self.config.width = size[0];
            self.config.height = size[1];
            self.surface.configure(&self.gpu.device, &self.config);
        }
        let mut acquired = self.surface.get_current_texture();
        if matches!(acquired, wgpu::CurrentSurfaceTexture::Outdated) {
            self.surface.configure(&self.gpu.device, &self.config);
            acquired = self.surface.get_current_texture();
        }
        let (frame, reconfigure) = match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => return Err("surface_lost".into()),
            wgpu::CurrentSurfaceTexture::Outdated => return Err("surface_outdated".into()),
            wgpu::CurrentSurfaceTexture::Validation => return Err("surface_validation".into()),
        };
        self.gpu.render_meshes(
            &frame.texture.create_view(&Default::default()),
            size,
            self.config.format,
            meshes,
        )?;
        self.gpu.queue.present(frame);
        if reconfigure {
            self.surface.configure(&self.gpu.device, &self.config);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_roundtrip() {
        let v = Viewport {
            width: 800.,
            height: 600.,
            zoom: 1.7,
            pan: [21., -9.],
            advance: 640.,
            ascender: 800.,
            descender: -200.,
        };
        let p = [123.456, 789.012];
        let q = v.document(v.screen(p));
        assert!((p[0] - q[0]).abs() < 1e-8 && (p[1] - q[1]).abs() < 1e-8);
    }
}

#[cfg(test)]
mod proof_gpu;

#[cfg(test)]
mod reference_gpu;
