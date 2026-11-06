//! Read back the same texture/mesh path used by Renderer::draw_color.
use super::*;

#[test]
#[ignore = "Requires a real GPU adapter"]
fn colrv1_colors_gradients_alpha_and_palette_survive_wgpu() {
    check_color_image(
        include_bytes!("../../font-core/tests/data/color/colrv1-paints.ttf"),
        &["A", "B", "C", "F", "G", "I", "J"],
    );
}

#[test]
#[ignore = "Requires a real GPU adapter"]
fn svg_colors_gradients_alpha_and_palette_survive_wgpu() {
    check_color_image(
        include_bytes!("../../font-core/tests/data/color/svg-paints-gzip.ttf"),
        &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"],
    );
}

#[test]
#[ignore = "Requires a real GPU adapter"]
fn svg_embedded_images_survive_wgpu() {
    check_color_image(
        include_bytes!("../../font-core/tests/data/color/svg-images.ttf"),
        &["A", "B", "C"],
    );
}

fn check_color_image(source: &[u8], glyphs: &[&str]) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format)).unwrap();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("COLRv1 readback"),
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
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("COLRv1 readback"),
        size: 256 * 256 * 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for palette in [0, 1, 0] {
        for &glyph in glyphs {
            let preview =
                font_core::color_fonts::preview(source, 0, glyph, 100, palette, [256, 256])
                    .unwrap();
            let mut mesh = Mesh {
                images: vec![],
                triangles: vec![],
                decoration: vec![],
                cover: vec![],
                clip: None,
            };
            reference::add_color(&mut mesh, [256., 256.], &preview.image);
            gpu.render_meshes(
                &texture.create_view(&Default::default()),
                [256, 256],
                format,
                vec![mesh],
            )
            .unwrap();
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
            let pixels = buffer.slice(..).get_mapped_range().unwrap();
            // Verify both orientation and sRGB/alpha handling against the Rust asset.
            for y in [40, 65, 85, 95, 110, 135] {
                for x in [5, 23, 45, 51, 65, 78, 100] {
                    let at = (y * 256 + x) * 4;
                    let source = &preview.image.rgba()[at..at + 4];
                    for c in 0..3 {
                        let s = f64::from(source[c]) / 255.;
                        let linear = if s <= 0.04045 {
                            s / 12.92
                        } else {
                            ((s + 0.055) / 1.055).powf(2.4)
                        };
                        let alpha = f64::from(source[3]) / 255.;
                        let blended = linear * alpha + 1. - alpha;
                        let srgb = if blended <= 0.0031308 {
                            blended * 12.92
                        } else {
                            1.055 * blended.powf(1. / 2.4) - 0.055
                        };
                        let expected = (srgb * 255.).round() as u8;
                        assert!(
                            pixels[at + c].abs_diff(expected) <= 3,
                            "{glyph} palette {palette} ({x},{y}) channel {c}: {} != {expected}",
                            pixels[at + c]
                        );
                    }
                    assert_eq!(pixels[at + 3], 255);
                }
            }
            drop(pixels);
            buffer.unmap();
            assert_eq!(gpu.images.borrow().cache.len(), 1);
            assert!(gpu.images.borrow().cache.contains_key(preview.image.id()));
        }
    }
}

#[test]
#[ignore = "Requires a real GPU adapter"]
fn project_palette_edits_survive_storage_and_wgpu() {
    let bytes = include_bytes!("../../font-core/tests/data/color/colrv1-paints.ttf");
    let doc = font_core::source_font::SourceFont::color_document(bytes, 0).unwrap();
    let mut editor = font_core::Editor::new(doc).unwrap();
    editor
        .edit_source_palette(0, 0, Some(0), Some([20, 40, 60, 128]))
        .unwrap();
    let file = std::env::temp_dir().join(format!(
        "color-gpu-{}.json",
        editor.document().document_uuid
    ));
    font_core::storage::save(&file, editor.document()).unwrap();
    let doc = font_core::storage::load(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    let exported = doc.source_font.unwrap().export().unwrap();
    check_color_image(&exported, &["A", "B", "F", "G", "I"]);
}

#[test]
#[ignore = "Requires a real GPU adapter"]
fn variable_project_axes_and_palette_reach_wgpu() {
    use font_core::source_font::{variable::Edit, SourceFont};
    for bytes in [
        include_bytes!("../../font-core/tests/data/variations/color-gvar.ttf").as_slice(),
        include_bytes!("../../font-core/tests/data/variations/color-cff2.otf").as_slice(),
    ] {
        let mut editor =
            font_core::Editor::new(SourceFont::variable_document(bytes, 0, None).unwrap()).unwrap();
        editor
            .edit_source_palette(0, 0, Some(0), Some([20, 40, 60, 128]))
            .unwrap();
        editor
            .edit_source_variations(
                editor.revision(),
                Edit::Coordinates {
                    values: [("wght".into(), 650.), ("wdth".into(), 125.)].into(),
                },
            )
            .unwrap();
        let file = std::env::temp_dir().join(format!(
            "variable-color-gpu-{}.json",
            editor.document().document_uuid
        ));
        font_core::storage::save(&file, editor.document()).unwrap();
        let doc = font_core::storage::load(&file).unwrap();
        std::fs::remove_file(file).unwrap();
        let out = doc.source_font.unwrap().static_bytes().unwrap();
        check_color_image(&out, &["A"]);
    }
}
