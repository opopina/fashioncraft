//! Fabric textures for the cloth shader: every fabric image resized into one layer of a
//! mipmapped sRGB texture array, so the single cloth draw shows them all. With no textures
//! the array holds one white layer that no vertex uses. Maps (normal, roughness, metalness,
//! opacity, displacement, occlusion) are data, not colours: their layers are stored
//! sRGB-encoded so that sampling returns their values, a DirectX normal map with its green
//! flipped and a glossiness map inverted, so the shader reads every map one way.
use super::graphics::{GraphicImage, mip_chain};
use super::*;

/// Side of every layer; images are resized to it, as each repeat spans the whole layer.
const LAYER_SIDE: u32 = 1024;

/// How an image is stored in its layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerKind {
    /// Colours, decoded from sRGB when sampled.
    Color,
    /// Values (a normal, roughness, metalness, opacity, height or occlusion map) returned as
    /// they are.
    Data,
    /// A DirectX normal map, its green flipped to read the OpenGL way.
    FlipGreen,
    /// A glossiness map, inverted to read as roughness.
    Invert,
}

pub(super) struct FabricMaps {
    /// Keys of the images in layer order, and how each is stored.
    keys: Vec<(u64, LayerKind)>,
    /// The pixels of the layers, kept for the photoreal render.
    layers: Vec<std::sync::Arc<GraphicImage>>,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    pub bind: wgpu::BindGroup,
}

/// The byte that an sRGB texture decodes to `value`/255.
fn srgb_encoded(value: u8) -> u8 {
    let v = value as f64 / 255.;
    let e = if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (e * 255.).round().clamp(0., 255.) as u8
}

pub(super) fn layer_pixels(image: &GraphicImage, kind: LayerKind) -> Option<GraphicImage> {
    let [w, h] = image.size;
    let buffer = image::RgbaImage::from_raw(w, h, image.rgba.clone())?;
    let resized = if [w, h] == [LAYER_SIDE; 2] {
        buffer
    } else {
        image::imageops::resize(
            &buffer,
            LAYER_SIDE,
            LAYER_SIDE,
            image::imageops::FilterType::Triangle,
        )
    };
    let mut rgba = resized.into_raw();
    if kind != LayerKind::Color {
        for (k, byte) in rgba.iter_mut().enumerate() {
            let value = match (kind, k % 4) {
                (_, 3) => continue,
                (LayerKind::FlipGreen, 1) | (LayerKind::Invert, _) => 255 - *byte,
                _ => *byte,
            };
            *byte = srgb_encoded(value);
        }
    }
    Some(GraphicImage {
        key: image.key,
        size: [LAYER_SIDE; 2],
        rgba,
    })
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fabric maps"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// A texture array with one layer per image (a white pixel when there are none).
fn array(device: &wgpu::Device, queue: &wgpu::Queue, layers: &[GraphicImage]) -> wgpu::TextureView {
    let white = GraphicImage {
        key: 0,
        size: [1, 1],
        rgba: vec![255; 4],
    };
    let layers: Vec<&GraphicImage> = if layers.is_empty() {
        vec![&white]
    } else {
        layers.iter().collect()
    };
    let chains: Vec<_> = layers.iter().map(|l| mip_chain(l)).collect();
    let size = layers[0].size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fabric maps"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: layers.len() as u32,
        },
        mip_level_count: chains[0].len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, chain) in chains.iter().enumerate() {
        for (level, ([w, h], pixels)) in chain.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
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
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

impl FabricMaps {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fabric maps"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let view = array(device, queue, &[]);
        let bind = bind_group(device, layout, &view, &sampler);
        Self {
            keys: vec![],
            layers: vec![],
            view,
            sampler,
            bind,
        }
    }

    /// Bind the same textures to a rebuilt pipeline's layout.
    /// The maps bound for another pipeline's layout of the same two entries.
    pub fn bind_for(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        bind_group(device, layout, &self.view, &self.sampler)
    }

    pub fn rebind(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout) {
        self.bind = bind_group(device, layout, &self.view, &self.sampler);
    }

    /// The layer of each image of each material (`None` where it has none), rebuilding the
    /// array when the set of images or the way they are stored changed.
    pub fn update<const N: usize>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        images: &[[Option<(&GraphicImage, LayerKind)>; N]],
    ) -> Vec<[Option<u32>; N]> {
        let mut distinct: Vec<(&GraphicImage, LayerKind)> = vec![];
        let layers = images
            .iter()
            .map(|maps| {
                let mut out = [None; N];
                for (k, image) in maps.iter().enumerate() {
                    let Some((image, kind)) = *image else {
                        continue;
                    };
                    let at = distinct
                        .iter()
                        .position(|(d, stored)| d.key == image.key && *stored == kind);
                    out[k] = Some(at.unwrap_or_else(|| {
                        distinct.push((image, kind));
                        distinct.len() - 1
                    }) as u32);
                }
                out
            })
            .collect();
        let keys: Vec<(u64, LayerKind)> = distinct.iter().map(|(d, kind)| (d.key, *kind)).collect();
        if keys != self.keys {
            let pixels: Vec<GraphicImage> = distinct
                .iter()
                .filter_map(|(d, kind)| layer_pixels(d, *kind))
                .collect();
            if pixels.len() == distinct.len() {
                self.view = array(device, queue, &pixels);
                self.bind = bind_group(device, layout, &self.view, &self.sampler);
                self.keys = keys;
                self.layers = pixels.into_iter().map(std::sync::Arc::new).collect();
            } else {
                return vec![[None; N]; images.len()];
            }
        }
        layers
    }

    /// The pixels of every layer, in order, as the shader reads them.
    pub fn layers(&self) -> &[std::sync::Arc<GraphicImage>] {
        &self.layers
    }
}
