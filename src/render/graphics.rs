//! Graphics (prints and logos) drawn over the cloth: one texture per image, alpha-blended
//! clipped triangles lifted off the fabric. Textures stay resident while their image is
//! drawn and are dropped the first frame it is not.
use super::*;
use crate::garment::graphics::GraphicTriangle;
use std::collections::HashMap;

/// Floats per graphic vertex: position, normal, image coordinates, opacity, and the fabric's
/// look there (weave coordinates and parameters, map coordinates, layers and relief).
const GRAPHIC_FLOATS: usize = 20;

/// Decoded straight-alpha RGBA8 pixels of a graphic's image, in sRGB. `key` identifies the
/// pixels: equal keys must mean equal pixels, as the uploaded texture is reused.
#[derive(Clone, Debug)]
pub struct GraphicImage {
    pub key: u64,
    pub size: [u32; 2],
    pub rgba: Vec<u8>,
}

/// One graphic to draw this frame.
#[derive(Clone, Copy)]
pub struct GraphicDraw<'a> {
    pub image: &'a GraphicImage,
    pub triangles: &'a [GraphicTriangle],
    /// Multiplies the image's alpha.
    pub opacity: f32,
}

pub(super) struct GraphicLayer {
    pipeline: wgpu::RenderPipeline,
    view_bind: wgpu::BindGroup,
    /// The fabric maps of this frame, for the relief under the prints.
    maps: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    textures: HashMap<u64, wgpu::BindGroup>,
    vertices: wgpu::Buffer,
    capacity: u64,
    /// Texture and vertex range of each graphic of the last prepared frame.
    batches: Vec<(u64, std::ops::Range<u32>)>,
}

/// Mip chain of straight-alpha RGBA8 pixels, halving with a 2×2 box filter.
pub(super) fn mip_chain(image: &GraphicImage) -> Vec<([u32; 2], Vec<u8>)> {
    let mut levels = vec![(image.size, image.rgba.clone())];
    while let Some(([w, h], pixels)) = levels.last()
        && (*w > 1 || *h > 1)
    {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (2 * x + dx).min(w - 1);
                        let sy = (2 * y + dy).min(h - 1);
                        sum += u32::from(pixels[((sy * w + sx) * 4 + c) as usize]);
                    }
                    next[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        levels.push(([nw, nh], next));
    }
    levels
}

impl GraphicLayer {
    pub fn new(device: &wgpu::Device, samples: u32, view_uniform: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cloth graphics"),
            source: wgpu::ShaderSource::Wgsl(include_str!("graphic.wgsl").into()),
        });
        let attrs = wgpu::vertex_attr_array![
            0=>Float32x3, 1=>Float32x3, 2=>Float32x2, 3=>Float32,
            4=>Float32x2, 5=>Float32x3, 6=>Float32x2, 7=>Float32x4
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cloth graphics"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (GRAPHIC_FLOATS * std::mem::size_of::<f32>()) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // Tested against the cloth and the body, but overlapping prints blend instead of
            // hiding each other.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("graphics camera"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_uniform.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("graphics"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let capacity = 1024;
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("graphics-vertices"),
            size: capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            view_bind,
            maps: None,
            sampler,
            textures: HashMap::new(),
            vertices,
            capacity,
            batches: Vec::new(),
        }
    }

    fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &GraphicImage,
    ) -> wgpu::BindGroup {
        let levels = mip_chain(image);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("graphic image"),
            size: wgpu::Extent3d {
                width: image.size[0],
                height: image.size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, ([w, h], pixels)) in levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("graphic image"),
            layout: &self.pipeline.get_bind_group_layout(1),
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
        })
    }

    /// The layout the fabric maps are bound with (group 2).
    pub fn maps_layout(&self) -> wgpu::BindGroupLayout {
        self.pipeline.get_bind_group_layout(2)
    }

    /// Upload new images and this frame's triangles, each with the fabric look under it
    /// (`looks`, per draw and triangle); images no longer drawn are released.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draws: &[GraphicDraw],
        looks: &[Vec<Option<Surface>>],
        maps: wgpu::BindGroup,
    ) {
        self.maps = Some(maps);
        self.textures
            .retain(|key, _| draws.iter().any(|d| d.image.key == *key));
        let mut data = Vec::<f32>::new();
        self.batches.clear();
        for (d, draw) in draws.iter().enumerate() {
            if draw.image.size[0] == 0
                || draw.image.size[1] == 0
                || draw.image.rgba.len() != (draw.image.size[0] * draw.image.size[1] * 4) as usize
            {
                continue;
            }
            if !self.textures.contains_key(&draw.image.key) {
                let bind = self.upload(device, queue, draw.image);
                self.textures.insert(draw.image.key, bind);
            }
            let start = (data.len() / GRAPHIC_FLOATS) as u32;
            for (i, t) in draw.triangles.iter().enumerate() {
                let look = looks.get(d).and_then(|l| l.get(i).copied().flatten());
                for k in 0..3 {
                    data.extend_from_slice(&t.points[k].as_vec3().to_array());
                    data.extend_from_slice(&t.normals[k].as_vec3().to_array());
                    data.extend_from_slice(&t.uv[k].as_vec2().to_array());
                    data.push(draw.opacity.clamp(0., 1.));
                    match &look {
                        Some(s) => {
                            data.extend_from_slice(&s.uv[k]);
                            data.extend_from_slice(&s.weave);
                            data.extend_from_slice(&s.map_uv[k]);
                            data.extend_from_slice(&s.map);
                        }
                        None => {
                            data.extend_from_slice(&[0.; 7]);
                            data.extend_from_slice(&[-1., 0., -1., -1.]);
                        }
                    }
                }
            }
            let end = (data.len() / GRAPHIC_FLOATS) as u32;
            if end > start {
                self.batches.push((draw.image.key, start..end));
            }
        }
        let bytes: &[u8] = bytemuck::cast_slice(&data);
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two();
            self.vertices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("graphics-vertices"),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytes);
        }
    }

    /// Draw the prepared graphics; call after the cloth and the body.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.batches.is_empty() {
            return;
        }
        let Some(maps) = &self.maps else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.view_bind, &[]);
        pass.set_bind_group(2, maps, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        for (key, range) in &self.batches {
            if let Some(bind) = self.textures.get(key) {
                pass.set_bind_group(1, bind, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_halves_down_to_one_pixel_and_averages() {
        let image = GraphicImage {
            key: 1,
            size: [4, 2],
            rgba: (0..32).map(|i| if i % 8 < 4 { 0 } else { 200 }).collect(),
        };
        let levels = mip_chain(&image);
        let sizes: Vec<_> = levels.iter().map(|(s, _)| *s).collect();
        assert_eq!(sizes, vec![[4, 2], [2, 1], [1, 1]]);
        // Each 2×2 block mixes one black and one light column: the mean is 100.
        assert!(levels[1].1.iter().all(|&c| c == 100));
        assert!(levels[2].1.iter().all(|&c| c == 100));
    }
}
