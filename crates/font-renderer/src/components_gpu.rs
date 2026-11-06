use super::*;
use font_core::{
    geometry::{self, EditPoint},
    snippets::Info,
    Editor, FontDocument, Segment,
};
#[test]
#[ignore = "Requires a real GPU adapter"]
fn nested_component_update_renders_the_new_outline() {
    render_component_edit(false, false);
}
#[test]
#[ignore = "Requires a real GPU adapter"]
fn dedicated_definition_edit_renders_without_source() {
    render_component_edit(true, false);
}
#[test]
#[ignore = "Requires a real GPU adapter"]
fn truetype_import_renders_nested_shared_update() {
    render_component_edit(true, true);
}
fn render_component_edit(direct: bool, roundtrip: bool) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let gpu = pollster::block_on(Gpu::new(&instance, None, format)).unwrap();
    let mut doc = FontDocument::new("Components", "ABC").unwrap();
    doc.glyphs.get_mut(&1).unwrap().contours = vec![Contour {
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
    }];
    let mut editor = Editor::new(doc).unwrap();
    let mut id = editor
        .capture_component(
            editor.revision(),
            1,
            &[0].into(),
            Info {
                name: "Part".into(),
                tags: "".into(),
                category: "radical".into(),
                source: "".into(),
                license: "".into(),
                favorite: false,
            },
        )
        .unwrap();
    editor
        .place_component(editor.revision(), id, 2, [1., 0., 0., 1., 0., 0.])
        .unwrap();
    let parent = editor
        .capture_component(
            editor.revision(),
            2,
            &[0].into(),
            Info {
                name: "Parent".into(),
                tags: "".into(),
                category: "radical".into(),
                source: "".into(),
                license: "".into(),
                favorite: false,
            },
        )
        .unwrap();
    editor
        .place_component(editor.revision(), parent, 3, [1., 0., 0., 1., 0., 0.])
        .unwrap();
    if roundtrip {
        let bytes = font_core::truetype_components::export(editor.document())
            .unwrap()
            .bytes;
        editor = Editor::new(font_core::truetype_components::import(&bytes, 0).unwrap()).unwrap();
        id = *editor
            .document()
            .components
            .definitions
            .iter()
            .find(|(_, d)| d.source_glyph == 1)
            .unwrap()
            .0;
        assert!(editor.document().components.has_nested());
    }
    let mut source = editor.document().glyphs[&1].clone();
    geometry::move_point(
        &mut source.contours[0],
        EditPoint::Control1(0),
        [300., 600.],
    )
    .unwrap();
    let mut glyph = editor.document().glyphs[&3].clone();
    if direct {
        let contours = source.contours.clone();
        source.contours.clear();
        editor.replace_glyph(editor.revision(), source).unwrap();
        editor
            .edit_component_contours(editor.revision(), id, contours)
            .unwrap();
        glyph.contours = editor.document().components.definitions[&id]
            .contours
            .as_ref()
            .clone();
        assert_eq!(
            glyph.contours[0].segments,
            editor.document().glyphs[&3].contours[0].segments
        );
    } else {
        editor.replace_glyph(editor.revision(), source).unwrap();
        editor
            .update_component(editor.revision(), id, 1, &[0].into())
            .unwrap();
        glyph = editor.document().glyphs[&3].clone();
    }
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
    assert!(red([200., 150.]) < 80, "curved body must contain ink");
    assert!(
        red([300., 400.]) > 240,
        "old shared component silhouette must be cleared"
    );
    assert!(red([120., 650.]) > 240, "outside must remain white");
}
