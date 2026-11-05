use super::*;
use font_core::{path_boolean, Segment};
#[test]
#[ignore = "Requires a real GPU adapter"]
fn curved_boolean_result_renders_fill_hole_and_editable_handles() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format)).unwrap();
    let a = Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [100., 0.],
        segments: vec![
            Segment::Quadratic {
                control: [300., 1000.],
                end: [500., 0.],
            },
            Segment::Line { end: [100., 0.] },
        ],
        closed: true,
    };
    let b = Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [250., 100.],
        segments: vec![
            Segment::Line { end: [350., 100.] },
            Segment::Line { end: [350., 250.] },
            Segment::Line { end: [250., 250.] },
        ],
        closed: true,
    };
    let contours = path_boolean::combine(&[a], &[b], "difference").unwrap();
    assert!(contours
        .iter()
        .flat_map(|c| &c.segments)
        .any(|s| matches!(s, Segment::Quadratic { .. })));
    let mut glyph = Glyph::new(1, 'A');
    glyph.contours = contours;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("readback"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let v = Viewport {
        width: 256.,
        height: 256.,
        zoom: 1.,
        pan: [0., 0.],
        advance: 600.,
        ascender: 800.,
        descender: -200.,
    };
    let m = editor_mesh(
        &glyph,
        v,
        DrawOptions {
            nodes: true,
            fill: true,
        },
        EditorOverlay {
            controls: &Default::default(),
            nodes: &Default::default(),
            node: None,
            marquee: None,
            images: &[],
            assets: &Default::default(),
            selected_image: None,
        },
    )
    .unwrap();
    gpu.render_meshes(
        &texture.create_view(&Default::default()),
        [256, 256],
        format,
        vec![m],
    )
    .unwrap();
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test readback"),
        size: 256 * 256 * 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(256),
            },
        },
        wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).unwrap();
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = buffer
        .slice(..)
        .get_mapped_range()
        .expect("readback buffer is mapped");
    let red = |p: Point| {
        let s = v.screen(p);
        let i = (s[1].round() as usize * 256 + s[0].round() as usize) * 4;
        bytes[i]
    };
    assert!(red([200., 200.]) < 80, "curved body must contain ink");
    assert!(red([300., 175.]) > 240, "Boolean hole must remain white");
    assert!(red([120., 650.]) > 240, "outside must remain white");
}
