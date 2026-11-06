//! Real GPU readback: proves holes survive stencil filling, not just a successful compile.
use super::{mesh, wgpu, Gpu, Viewport};
use font_core::{Contour, Segment};
#[test]
#[ignore = "Requires access to a real GPU adapter; run explicitly on a desktop host"]
fn proof_layers_preserve_holes_and_overlapping_glyphs() {
    readback(false);
}
#[test]
#[ignore = "Requires access to a real GPU adapter; run explicitly on a desktop host"]
fn vertical_shaped_proof_renders_rotated_and_substituted_glyphs() {
    readback(true);
}
fn readback(shaped: bool) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format))
        .expect("GPU adapter required for this test");
    let polygon = |pts: &[[f64; 2]]| Contour {
        identity: Default::default(),
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
    let mut a = mesh(&contours, v, false, true).unwrap();
    a.decoration.clear();
    let mut b = mesh(
        &[polygon(&[
            [300., 100.],
            [350., 100.],
            [350., 500.],
            [300., 500.],
        ])],
        v,
        false,
        true,
    )
    .unwrap();
    b.decoration.clear();
    let meshes = if shaped {
        let bytes = include_bytes!("../../font-core/tests/data/layout/shaping.ttf");
        let doc = font_core::font_io::import(bytes, 0).unwrap();
        let text = "A字\u{e0100}、fi";
        let p = font_core::proof::prepare(doc, text).unwrap();
        let settings = font_core::proof::Settings {
            text: text.into(),
            size: 64.,
            column_height: 200.,
            direction: font_core::proof::Direction::Vertical,
            ..Default::default()
        };
        let layout = font_core::proof::layout(&p, &settings, 256.);
        assert_eq!(layout.placements.len(), 4);
        assert!(layout.placements[0].rotated);
        super::proof_meshes([256, 256], [256., 256.], &p, None, &settings).unwrap()
    } else {
        vec![a, b]
    };
    gpu.render_meshes(
        &texture.create_view(&Default::default()),
        [256, 256],
        format,
        meshes,
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
    if shaped {
        let source = include_bytes!("../../font-core/tests/data/layout/shaping.ttf");
        let doc = font_core::font_io::import(source, 0).unwrap();
        let p = font_core::proof::prepare(doc, "A字\u{e0100}、fi").unwrap();
        let settings = font_core::proof::Settings {
            text: "A字\u{e0100}、fi".into(),
            size: 64.,
            column_height: 200.,
            direction: font_core::proof::Direction::Vertical,
            ..Default::default()
        };
        let layout = font_core::proof::layout(&p, &settings, 256.);
        for placement in &layout.placements {
            let mut ink = 0;
            for y in (placement.top.max(0.) as usize)..(placement.bottom.min(256.) as usize) {
                for x in (placement.left.max(0.) as usize)
                    ..((placement.left + placement.width).min(256.) as usize)
                {
                    if bytes[(y * 256 + x) * 4] < 100 {
                        ink += 1;
                    }
                }
            }
            assert!(ink > 8, "shaped glyph {} must render ink", placement.glyph);
        }
        return;
    }
    let pixel = |p| {
        let s = v.screen(p);
        let index = (s[1].round() as usize * 256 + s[0].round() as usize) * 4;
        bytes[index]
    };
    assert!(pixel([150., 300.]) < 80, "body should be ink");
    assert!(pixel([250., 310.]) > 240, "uncovered hole stays white");
    assert!(
        pixel([325., 310.]) < 80,
        "overlapping glyph must cover the hole"
    );
    assert!(pixel([50., 300.]) > 240, "outside should be white");
}
