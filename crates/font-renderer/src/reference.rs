use super::*;
use font_core::{
    reference_images::{ImageAsset, ReferenceImage},
    reference_transform::{handles, Handle},
};
use std::collections::{BTreeMap, BTreeSet};

pub struct EditorOverlay<'a> {
    pub node: Option<(usize, geometry::EditPoint)>,
    pub images: &'a [ReferenceImage],
    pub assets: &'a BTreeMap<String, ImageAsset>,
    pub selected_image: Option<u64>,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct ImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
    opacity: f32,
}
pub(super) struct ImageQuad {
    pub asset: ImageAsset,
    pub vertices: [ImageVertex; 6],
}
pub(super) struct ImageGpu {
    pub pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pub cache: BTreeMap<String, wgpu::BindGroup>,
}
impl ImageGpu {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reference image"),
            source: wgpu::ShaderSource::Wgsl(include_str!("reference.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reference image"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reference image"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ImageVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reference image"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
            cache: BTreeMap::new(),
        }
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Mesh],
    ) -> Result<Vec<Vec<wgpu::Buffer>>, String> {
        let used: BTreeSet<_> = meshes
            .iter()
            .flat_map(|m| m.images.iter().map(|q| q.asset.id()))
            .collect();
        // Keep only this frame's visible assets. Switching documents, glyphs or proof releases textures.
        self.cache.retain(|id, _| used.contains(id.as_str()));
        let mut buffers = Vec::new();
        for mesh in meshes {
            let mut group = Vec::new();
            for quad in &mesh.images {
                let a = &quad.asset;
                if !self.cache.contains_key(a.id()) {
                    if a.width() > device.limits().max_texture_dimension_2d
                        || a.height() > device.limits().max_texture_dimension_2d
                    {
                        return Err("image_gpu_limit".into());
                    }
                    let size = wgpu::Extent3d {
                        width: a.width(),
                        height: a.height(),
                        depth_or_array_layers: 1,
                    };
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("reference pixels"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        a.rgba(),
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(a.width() * 4),
                            rows_per_image: Some(a.height()),
                        },
                        size,
                    );
                    let view = texture.create_view(&Default::default());
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("reference image"),
                        layout: &self.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                        ],
                    });
                    self.cache.insert(a.id().into(), bind);
                }
                group.push(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("reference quad"),
                        contents: bytemuck::cast_slice(&quad.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                );
            }
            buffers.push(group);
        }
        Ok(buffers)
    }
}
pub(super) fn add_images(mesh: &mut Mesh, v: Viewport, overlay: &EditorOverlay<'_>) {
    for image in overlay
        .images
        .iter()
        .filter(|i| i.visible && i.opacity > 0.)
    {
        if let Some(asset) = overlay.assets.get(&image.asset_id) {
            let corners = [[-0.5, 0.5], [0.5, 0.5], [0.5, -0.5], [-0.5, -0.5]];
            let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
            let vertices = [0, 1, 2, 0, 2, 3].map(|i| ImageVertex {
                position: vertex(v, v.screen(image.to_document(corners[i])), INK).position,
                uv: uv[i],
                opacity: image.opacity as f32,
            });
            mesh.images.push(ImageQuad {
                asset: asset.clone(),
                vertices,
            });
        }
    }
    let Some(image) = overlay
        .images
        .iter()
        .find(|i| Some(i.id) == overlay.selected_image && i.visible && !i.locked)
    else {
        return;
    };
    let color = [0.12, 0.42, 0.85, 1.];
    let corners = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    for i in 0..4 {
        line(
            &mut mesh.decoration,
            v,
            v.screen(image.to_document(corners[i])),
            v.screen(image.to_document(corners[(i + 1) % 4])),
            1.2,
            color,
        );
    }
    for (handle, p) in handles(image, v.scale()) {
        let p = v.screen(p);
        if handle == Handle::Rotate {
            line(
                &mut mesh.decoration,
                v,
                v.screen(image.to_document([0., 0.5])),
                p,
                1.2,
                color,
            );
            for i in 0..20 {
                let q = |n: usize| {
                    let a = n as f64 * std::f64::consts::TAU / 20.;
                    [p[0] + a.cos() * 4., p[1] + a.sin() * 4.]
                };
                for pt in [p, q(i), q(i + 1)] {
                    mesh.decoration.push(vertex(v, pt, color));
                }
            }
        } else {
            rectangle(
                &mut mesh.decoration,
                v,
                [p[0] - 4., p[1] - 4.],
                [p[0] + 4., p[1] + 4.],
                color,
            );
            rectangle(
                &mut mesh.decoration,
                v,
                [p[0] - 2.5, p[1] - 2.5],
                [p[0] + 2.5, p[1] + 2.5],
                [1.; 4],
            );
        }
    }
}
