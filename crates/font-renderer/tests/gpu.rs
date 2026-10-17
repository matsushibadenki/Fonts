//! Real GPU readback: proves holes survive stencil filling, not just a successful compile.
use font_core::{Contour, Segment};
use font_renderer::{wgpu, DrawOptions, Gpu, Viewport};
#[test]
#[ignore = "Requires access to a real GPU adapter; run explicitly on a desktop host"]
fn gpu_fill_preserves_a_hole() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format))
        .expect("GPU adapter required for this test");
    let polygon = |pts: &[[f64; 2]]| Contour {
        smooth: Default::default(),
        start: pts[0],
        segments: pts[1..].iter().map(|&end| Segment::Line { end }).collect(),
        closed: true,
    };
    let contours = vec![
        polygon(&[[100., 0.], [500., 0.], [500., 600.], [100., 600.]]),
        polygon(&[[200., 200.], [400., 200.], [400., 400.], [200., 400.]]),
    ];
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
    gpu.render(
        &texture.create_view(&Default::default()),
        [256, 256],
        format,
        &contours,
        v,
        DrawOptions {
            nodes: false,
            fill: true,
        },
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
    let pixel = |p| {
        let s = v.screen(p);
        let index = (s[1].round() as usize * 256 + s[0].round() as usize) * 4;
        bytes[index]
    };
    assert!(pixel([150., 300.]) < 80, "body should be ink");
    assert!(pixel([310., 310.]) > 240, "hole should remain white");
    assert!(pixel([50., 300.]) > 240, "outside should be white");
}

#[test]
#[ignore = "Requires access to a real GPU adapter; run explicitly on a desktop host"]
fn thumbnails_show_unencoded_outlines_holes_and_empty_glyphs_without_guides() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu =
        pollster::block_on(Gpu::new(&instance, None, wgpu::TextureFormat::Rgba8Unorm)).unwrap();
    let polygon = |pts: &[[f64; 2]]| Contour {
        start: pts[0],
        segments: pts[1..].iter().map(|&end| Segment::Line { end }).collect(),
        closed: true,
        smooth: Default::default(),
    };
    // No Unicode or font family participates in rendering; fit negative bearings as well.
    let outer = polygon(&[
        [-3000., -1000.],
        [-2000., -1000.],
        [-2000., 0.],
        [-3000., 0.],
    ]);
    let hole = polygon(&[
        [-2700., -700.],
        [-2300., -700.],
        [-2300., -300.],
        [-2700., -300.],
    ]);
    let image = gpu.thumbnail(&[outer.clone(), hole]).unwrap();
    assert_eq!(image.len(), 128 * 128 * 4);
    let red = |x: usize, y: usize| image[(y * 128 + x) * 4];
    assert!(red(25, 64) < 80, "glyph body must contain ink");
    assert!(red(64, 64) > 240, "hole must remain white");
    assert!(red(2, 64) > 240, "fitted outline must retain its margin");
    assert_ne!(
        image,
        gpu.thumbnail(&[outer]).unwrap(),
        "distinct outlines must not share a Unicode fallback"
    );
    let empty = gpu.thumbnail(&[]).unwrap();
    assert!(
        empty
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [255, 255, 255, 255]),
        "empty glyph must not show editor guides"
    );
}
