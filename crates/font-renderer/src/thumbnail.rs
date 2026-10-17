//! Small offscreen previews. Raw pixels never cross the JavaScript IPC bridge.
use super::*;

pub const THUMBNAIL_SIZE: u32 = 128;

/// Fit the entire control hull, including negative bearings and tall marks.
fn fitted_viewport(contours: &[Contour]) -> Result<Viewport, String> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for c in contours {
        geometry::validate(c).map_err(|e| e.to_string())?;
        let mut include = |p: Point| {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        };
        include(c.start);
        for segment in &c.segments {
            match *segment {
                font_core::Segment::Line { end } => include(end),
                font_core::Segment::Quadratic { control, end } => {
                    include(control);
                    include(end);
                }
                font_core::Segment::Cubic {
                    control1,
                    control2,
                    end,
                } => {
                    include(control1);
                    include(control2);
                    include(end);
                }
            }
        }
    }
    let mut v = Viewport {
        width: 128.,
        height: 128.,
        zoom: 1.,
        pan: [0., 0.],
        advance: 1000.,
        ascender: 1000.,
        descender: 0.,
    };
    if min[0].is_finite() {
        let scale = 104. / (max[0] - min[0]).max(max[1] - min[1]).max(1.);
        v.zoom = scale / v.scale();
        v.pan = [
            -(min[0] + max[0] - 1000.) / 2. * scale,
            (min[1] + max[1] - 1000.) / 2. * scale,
        ];
    }
    Ok(v)
}

impl Gpu {
    /// Fixed-size RGBA readback for the native thumbnail resource service.
    pub fn thumbnail(&self, contours: &[Contour]) -> Result<Vec<u8>, String> {
        let v = fitted_viewport(contours)?;
        let mut m = mesh_options(contours, v, false, true, false)?;
        for vertex in &mut m.decoration {
            vertex.color = INK;
        }
        let size = THUMBNAIL_SIZE;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph thumbnail"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.render_meshes(
            &texture.create_view(&Default::default()),
            [size, size],
            wgpu::TextureFormat::Rgba8Unorm,
            vec![m],
        )?;
        // 128 RGBA pixels = 512 bytes, already aligned to COPY_BYTES_PER_ROW_ALIGNMENT.
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("thumbnail readback"),
            size: (size * size * 4) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
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
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let pixels = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| e.to_string())?
            .to_vec();
        buffer.unmap();
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_bearings_and_tall_marks_fit_inside_padding() {
        let contours = vec![Contour {
            start: [-800., -400.],
            segments: vec![font_core::Segment::Quadratic {
                control: [1200., 2400.],
                end: [300., 100.],
            }],
            closed: false,
            smooth: Default::default(),
        }];
        let v = fitted_viewport(&contours).unwrap();
        for point in [[-800., -400.], [1200., 2400.], [300., 100.]] {
            assert!(v.screen(point).iter().all(|p| *p >= 11.9 && *p <= 116.1));
        }
    }
    #[test]
    fn invalid_geometry_is_rejected_before_gpu_work() {
        let contours = vec![Contour {
            start: [f64::NAN, 0.],
            segments: vec![],
            closed: false,
            smooth: Default::default(),
        }];
        assert!(fitted_viewport(&contours).is_err());
        assert!(fitted_viewport(&[]).unwrap().scale().is_finite());
    }
}
