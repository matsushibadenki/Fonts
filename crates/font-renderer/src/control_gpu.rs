use super::*;
use font_core::{geometry::EditPoint, Segment};
#[test]
#[ignore = "Requires a real GPU adapter"]
fn mixed_selection_renders_blue_handle_rings_at_transformed_positions() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format)).unwrap();
    let mut glyph = font_core::Glyph::new(1, 'A');
    glyph.contours = vec![Contour {
        identity: Default::default(),
        smooth: Default::default(),
        start: [100., 0.],
        segments: vec![Segment::Cubic {
            control1: [180., 500.],
            control2: [420., 500.],
            end: [500., 0.],
        }],
        closed: false,
    }];
    let controls = [(0, EditPoint::Control1(0)), (0, EditPoint::Control2(0))]
        .into_iter()
        .collect();
    let anchors = [(0, 0)].into_iter().collect();
    let selected = [
        (0, EditPoint::Anchor(0)),
        (0, EditPoint::Control1(0)),
        (0, EditPoint::Control2(0)),
    ]
    .into_iter()
    .collect();
    font_core::point_transform::transform(&mut glyph.contours, &selected, [1., 1.], 0., [0., 200.])
        .unwrap();
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
            fill: false,
        },
        EditorOverlay {
            controls: &controls,
            nodes: &anchors,
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
    let blue_near = |q: [f64; 2]| {
        let p = v.screen(q);
        let mut count = 0;
        for y in (p[1] as usize - 8)..=(p[1] as usize + 8) {
            for x in (p[0] as usize - 8)..=(p[0] as usize + 8) {
                let i = (y * 256 + x) * 4;
                let px = &bytes[i..i + 4];
                if px[2] > 120 && (px[0] as u16) * 2 < px[2] as u16 && px[1] > 20 {
                    count += 1;
                }
            }
        }
        count
    };
    assert!(blue_near([180., 700.]) > 20);
    assert!(blue_near([420., 700.]) > 20);
    assert!(blue_near([100., 200.]) > 20);
    assert_eq!(blue_near([180., 500.]), 0);
}
