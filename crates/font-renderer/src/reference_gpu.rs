use super::*;
use font_core::{
    reference_images::{ImageAsset, ReferenceImage},
    Segment,
};
#[test]
#[ignore = "Requires a real GPU adapter"]
fn images_render_orientation_alpha_rotation_order_and_outline_with_texture_reuse() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format)).unwrap();
    let asset =
        ImageAsset::from_bytes(include_bytes!("../tests/fixtures/reference.png").to_vec()).unwrap();
    let assets = [(asset.id().to_string(), asset.clone())].into();
    let image = ReferenceImage {
        id: 1,
        asset_id: asset.id().into(),
        center: [300., 300.],
        width: 800.,
        height: 800.,
        angle: 0.,
        opacity: 1.,
        visible: true,
        locked: false,
    };
    let v = Viewport {
        width: 256.,
        height: 256.,
        zoom: 1.,
        pan: [0., 0.],
        advance: 600.,
        ascender: 800.,
        descender: -200.,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("image test"),
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
    let render = |images: &[ReferenceImage], selected, contours: &[Contour]| {
        let mut m = mesh_options(contours, v, false, true, false).unwrap();
        reference::add_images(
            &mut m,
            v,
            &EditorOverlay {
                node: None,
                images,
                assets: &assets,
                selected_image: selected,
            },
        );
        gpu.render_meshes(
            &texture.create_view(&Default::default()),
            [256, 256],
            format,
            vec![m],
        )
        .unwrap();
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image test readback"),
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
        buffer.slice(..).get_mapped_range().unwrap().to_vec()
    };
    let pixel = |bytes: &[u8], p| {
        let s = v.screen(p);
        let offset = (s[1].round() as usize * 256 + s[0].round() as usize) * 4;
        <[u8; 4]>::try_from(&bytes[offset..offset + 4]).unwrap()
    };
    let bytes = render(std::slice::from_ref(&image), None, &[]);
    let red = pixel(&bytes, [100., 500.]);
    assert!(
        red[0] > 245 && red[1] < 12 && red[2] < 12,
        "top-left: {red:?}"
    );
    let blue = pixel(&bytes, [100., 100.]);
    assert!(blue[2] > 245 && blue[0] < 12, "bottom-left: {blue:?}");
    let alpha = pixel(&bytes, [500., 100.]);
    assert!(
        alpha[0] > 245 && alpha[1] > 245 && (110..145).contains(&alpha[2]),
        "source alpha: {alpha:?}"
    );
    assert_eq!(gpu.images.borrow().cache.len(), 1);
    let mut half = image.clone();
    half.opacity = 0.5;
    let bytes = render(&[half], None, &[]);
    let red = pixel(&bytes, [100., 500.]);
    assert!(red[0] > 245 && (120..145).contains(&red[1]));
    let mut rotated = image.clone();
    rotated.id = 2;
    rotated.angle = 90.;
    let bytes = render(&[image.clone(), rotated.clone()], None, &[]);
    let red = pixel(&bytes, rotated.to_document([-0.25, 0.25]));
    assert!(red[0] > 245 && red[1] < 12);
    assert_eq!(
        gpu.images.borrow().cache.len(),
        1,
        "repeated transforms reuse one texture"
    );
    let outline = Contour {
        start: [50., 450.],
        segments: vec![
            Segment::Line { end: [150., 450.] },
            Segment::Line { end: [150., 550.] },
            Segment::Line { end: [50., 550.] },
        ],
        closed: true,
        smooth: Default::default(),
    };
    let bytes = render(std::slice::from_ref(&image), None, &[outline]);
    let ink = pixel(&bytes, [100., 500.]);
    assert!(
        ink[0] < 60 && ink[1] < 60 && ink[2] < 60,
        "outline is in front: {ink:?}"
    );
    let mut hidden = image.clone();
    hidden.visible = false;
    assert!(render(&[hidden], Some(1), &[]).iter().all(|b| *b == 255));
    assert!(gpu.images.borrow().cache.is_empty());
    let mut transparent = image;
    transparent.opacity = 0.;
    let bytes = render(std::slice::from_ref(&transparent), Some(1), &[]);
    assert_eq!(pixel(&bytes, [300., 300.]), [255; 4]);
    let handle = pixel(
        &bytes,
        transparent.to_document([3. / v.scale() / transparent.width, 0.5]),
    );
    assert!(handle[2] > handle[0], "handles stay opaque: {handle:?}");
    assert!(gpu.images.borrow().cache.is_empty());
}
