//! Photoreal render: a progressive path tracer in a wgpu compute shader.
//!
//! The scene is what the viewport draws, taken from the same vertex stream: the garment's
//! faces (with their thickness and rims), trims, prints and the avatar, each triangle with the
//! viewport's material (front and back colours, roughness, metalness, weave, fabric texture and
//! its maps). A bounding-volume hierarchy ([`super::bvh`]) built on the CPU orders them for the
//! GPU's rays. Light comes from the viewport's key light (a small sun), from the environment
//! (an equirectangular HDR image, or the studio's backdrop colours) and from bounces between
//! surfaces and the floor. Each [`PathTracer::step`] adds one sample per pixel; the image is
//! the running mean, tone mapped. [`RenderWorker`] runs it on a thread with its own device, so
//! the editor never waits for it.
use super::{GraphicDraw, GraphicImage, VERTEX_FLOATS, appearance::Appearance, bvh::Bvh};
use eframe::wgpu;
use glam::Vec3;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// The shader, for validation without a GPU.
pub const SHADER: &str = include_str!("path_trace.wgsl");

/// Irradiance the key light brings at full gain, relative units (a white surface facing it
/// looks about as bright as in the viewport).
const SUN_IRRADIANCE: f32 = 2.0;
/// Cosine of the sun's angular radius (about 1.5°): its shadows have soft edges.
const SUN_COS_RADIUS: f32 = 0.99966;
/// Radiance of the studio sky per unit of its colour at full ambient gain.
const SKY_RADIANCE: f32 = 1.5;
/// Widest environment image kept; larger ones are reduced.
const MAX_ENVIRONMENT_WIDTH: u32 = 2048;
/// GPU time one dispatch aims at, so other GPU work (the editor's frames) gets its turns.
const DISPATCH_BUDGET: Duration = Duration::from_millis(12);

/// Where the render looks from: the viewport's camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceCamera {
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    /// Tangent of half the vertical field of view.
    pub tan_half_fov: f32,
    pub orthographic: bool,
    /// Half the view's height for an orthographic camera, m.
    pub half_height: f32,
    /// Distance to the orbit's target, m: where the lens focuses by default.
    pub distance: f32,
}

impl TraceCamera {
    pub fn from_camera(camera: &super::Camera) -> Self {
        let tan_half_fov = (camera.fov_radians() * 0.5).tan();
        Self {
            eye: camera.eye(),
            forward: camera.forward(),
            right: camera.right(),
            up: camera.up(),
            tan_half_fov,
            orthographic: camera.orthographic,
            half_height: camera.distance * tan_half_fov,
            distance: camera.distance,
        }
    }
}

/// What the render draws.
#[derive(Clone)]
pub struct TraceScene {
    /// Triangles: three vertices of the viewport's vertex floats each.
    pub vertices: Vec<f32>,
    /// The images the triangles' layers name, as the viewport's texture array holds them.
    pub layers: Vec<Arc<GraphicImage>>,
    pub camera: TraceCamera,
    /// The viewport's light rig, as its shader receives it.
    pub lighting: [f32; 8],
    /// The viewport's backdrop and floor.
    pub look: Appearance,
    /// Height of the floor, m.
    pub ground_y: f32,
}

/// A material for triangles added by hand ([`TraceScene::add_triangle`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceMaterial {
    /// Colour of the side the winding normal faces away from (the fabric's front), linear.
    pub color: [f32; 3],
    /// Colour of the other side, linear.
    pub back: [f32; 3],
    pub roughness: f32,
    pub metalness: f32,
    /// Layer of the colour image, and of the normal, roughness, metalness and opacity maps.
    pub color_layer: Option<u32>,
    pub normal_layer: Option<u32>,
    pub roughness_layer: Option<u32>,
    pub metalness_layer: Option<u32>,
    pub opacity_layer: Option<u32>,
    /// Weave kind code (0 none), thread spacing (m) and contrast.
    pub weave: [f32; 3],
}

impl Default for TraceMaterial {
    fn default() -> Self {
        Self {
            color: [0.8; 3],
            back: [0.8; 3],
            roughness: 0.8,
            metalness: 0.,
            color_layer: None,
            normal_layer: None,
            roughness_layer: None,
            metalness_layer: None,
            opacity_layer: None,
            weave: [0.; 3],
        }
    }
}

impl TraceScene {
    /// An empty scene seen by `camera`, with the viewport's default light, backdrop and floor.
    pub fn new(camera: TraceCamera) -> Self {
        let look = Appearance::default();
        Self {
            vertices: vec![],
            layers: vec![],
            camera,
            lighting: look.lighting.uniform(),
            look,
            ground_y: 0.,
        }
    }

    /// Add a triangle with its corners' normals and texture coordinates (one repeat spans
    /// [0, 1]; the weave reads them as warp and weft metres).
    pub fn add_triangle(
        &mut self,
        corners: [Vec3; 3],
        normals: [Vec3; 3],
        uv: [[f32; 2]; 3],
        m: &TraceMaterial,
    ) {
        let layer = |l: Option<u32>| l.map_or(-1., |l| l as f32);
        for k in 0..3 {
            let mut v = [0f32; VERTEX_FLOATS];
            v[..4].copy_from_slice(&corners[k].extend(1.).to_array());
            v[4..7].copy_from_slice(&normals[k].to_array());
            v[7..10].copy_from_slice(&m.color);
            v[14] = m.roughness;
            v[15..18].copy_from_slice(&m.back);
            v[18..20].copy_from_slice(&uv[k]);
            v[20..23].copy_from_slice(&m.weave);
            v[23..25].copy_from_slice(&uv[k]);
            v[25..29].copy_from_slice(&[
                layer(m.color_layer),
                0.,
                layer(m.normal_layer),
                layer(m.roughness_layer),
            ]);
            v[29..33].copy_from_slice(&[
                layer(m.metalness_layer),
                layer(m.opacity_layer),
                -1.,
                -1.,
            ]);
            v[33..37].copy_from_slice(&[m.metalness, 0., 1., 0.]);
            self.vertices.extend_from_slice(&v);
        }
    }

    /// Add prints (graphics) lying on the cloth: their images go on new layers, their
    /// triangles let light through where the image is clear.
    pub fn add_prints(&mut self, prints: &[GraphicDraw]) {
        for print in prints {
            let Some(pixels) =
                super::fabric_maps::layer_pixels(print.image, super::LayerKind::Color)
            else {
                continue;
            };
            let layer = self.layers.len() as f32;
            self.layers.push(Arc::new(pixels));
            for t in print.triangles {
                for k in 0..3 {
                    let mut v = [0f32; VERTEX_FLOATS];
                    v[..4].copy_from_slice(&t.points[k].as_vec3().extend(1.).to_array());
                    v[4..7].copy_from_slice(&t.normals[k].as_vec3().to_array());
                    v[7..10].copy_from_slice(&[1.; 3]);
                    v[14] = 0.6;
                    v[15..18].copy_from_slice(&[1.; 3]);
                    v[23..25].copy_from_slice(&t.uv[k].as_vec2().to_array());
                    v[25..29].copy_from_slice(&[layer, 0., -1., -1.]);
                    v[29..33].copy_from_slice(&[-1.; 4]);
                    v[33..37].copy_from_slice(&[0., 0., print.opacity.clamp(0., 1.), 1.]);
                    self.vertices.extend_from_slice(&v);
                }
            }
        }
    }

    pub fn triangle_count(&self) -> usize {
        self.vertices.len() / (3 * VERTEX_FLOATS)
    }
}

/// An HDR environment: linear colours of an equirectangular image, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
    /// The file's name, to show.
    pub name: String,
}

impl Environment {
    /// Read an equirectangular image (Radiance `.hdr`, or an ordinary image taken as sRGB),
    /// reduced to at most [`MAX_ENVIRONMENT_WIDTH`] wide.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let image =
            image::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let image = if image.width() > MAX_ENVIRONMENT_WIDTH {
            image.resize(
                MAX_ENVIRONMENT_WIDTH,
                MAX_ENVIRONMENT_WIDTH / 2,
                image::imageops::FilterType::Triangle,
            )
        } else {
            image
        };
        let hdr = matches!(
            image,
            image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_)
        );
        let rgb = image.to_rgb32f();
        let decode = |v: f32| {
            if hdr {
                v
            } else if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let pixels = rgb
            .pixels()
            .map(|p| {
                p.0.map(|v| decode(v).max(0.))
                    .map(|v| if v.is_finite() { v } else { 0. })
            })
            .collect();
        Ok(Self {
            width: rgb.width(),
            height: rgb.height(),
            pixels,
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
    }

    /// The same light from every direction.
    pub fn uniform(color: [f32; 3]) -> Self {
        Self {
            width: 8,
            height: 4,
            pixels: vec![color; 32],
            name: "uniform".into(),
        }
    }

    /// Sampling tables: each row's cumulative distribution (width + 1 values), then the rows'
    /// (height + 1 values), by luminance times the row's solid angle.
    pub fn distribution(&self) -> Vec<f32> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = Vec::with_capacity(h * (w + 1) + h + 1);
        let mut rows = Vec::with_capacity(h);
        for j in 0..h {
            let sin = (std::f32::consts::PI * (j as f32 + 0.5) / h as f32).sin();
            let start = out.len();
            out.push(0.);
            let mut sum = 0f32;
            for i in 0..w {
                let [r, g, b] = self.pixels[j * w + i];
                // A floor keeps even black skies sampleable.
                sum += (0.2126 * r + 0.7152 * g + 0.0722 * b).max(1e-6) * sin;
                out.push(sum);
            }
            for v in &mut out[start..] {
                *v /= sum.max(1e-30);
            }
            rows.push(sum);
        }
        let total: f32 = rows.iter().sum();
        out.push(0.);
        let mut acc = 0.;
        for r in rows {
            acc += r;
            out.push(acc / total.max(1e-30));
        }
        out
    }
}

/// How to render.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceSettings {
    pub width: u32,
    pub height: u32,
    /// Samples per pixel to stop at.
    pub samples: u32,
    /// Most bounces a path makes.
    pub bounces: u32,
    /// Exposure, stops.
    pub exposure_ev: f32,
    /// Lens aperture radius, mm; zero keeps everything sharp.
    pub aperture_mm: f32,
    /// Distance in focus, m.
    pub focus_m: f32,
    /// Light from an HDR image instead of the studio's sky colours.
    pub environment: Option<Arc<Environment>>,
    /// Brightness of the environment (the image or the studio sky).
    pub environment_gain: f32,
    /// Turn of the environment about the vertical, degrees.
    pub environment_turn_degrees: f32,
    /// Show the HDR image behind the scene instead of the studio backdrop.
    pub environment_background: bool,
    /// Light the scene with the viewport's key light too.
    pub key_light: bool,
    /// Draw the floor.
    pub floor: bool,
}

impl Default for TraceSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            samples: 256,
            bounces: 6,
            exposure_ev: 0.,
            aperture_mm: 0.,
            focus_m: 2.5,
            environment: None,
            environment_gain: 1.,
            environment_turn_degrees: 0.,
            environment_background: true,
            key_light: true,
            floor: true,
        }
    }
}

/// Largest image side the render accepts.
pub const MAX_SIDE: u32 = 8192;

impl TraceSettings {
    /// Settings with every value in its range.
    pub fn sanitized(&self) -> Self {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        Self {
            width: self.width.clamp(16, MAX_SIDE),
            height: self.height.clamp(16, MAX_SIDE),
            samples: self.samples.clamp(1, 1 << 16),
            bounces: self.bounces.min(32),
            exposure_ev: finite(self.exposure_ev, 0.).clamp(-10., 10.),
            aperture_mm: finite(self.aperture_mm, 0.).clamp(0., 500.),
            focus_m: finite(self.focus_m, 2.5).clamp(0.01, 1000.),
            environment_gain: finite(self.environment_gain, 1.).clamp(0., 100.),
            environment_turn_degrees: finite(self.environment_turn_degrees, 0.),
            ..self.clone()
        }
    }
}

/// The viewport's sRGB byte as linear light.
fn linear(byte: u8) -> f32 {
    let s = f32::from(byte) / 255.;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// The light that the tone curve shows as `display` (an sRGB value in [0, 1]) at unit
/// exposure: the inverse of the shader's `tone_map`.
fn untone(display: f32) -> f32 {
    let d = display.clamp(0., 1.);
    let y = if d <= 0.04045 {
        d / 12.92
    } else {
        ((d + 0.055) / 1.055).powf(2.4)
    }
    .min(0.98);
    let a = 2.43 * y - 2.51;
    let b = 0.59 * y - 0.03;
    let c = 0.14 * y;
    ((-b - (b * b - 4. * a * c).max(0.).sqrt()) / (2. * a)).max(0.)
}

/// The scene as the GPU reads it.
struct Uploaded {
    nodes: Vec<super::bvh::Node>,
    triangles: Vec<[f32; 4]>,
    materials: Vec<[f32; 24]>,
    /// Triangle corners in leaf order, for focusing by a ray.
    corners: Vec<[Vec3; 3]>,
    bvh: Bvh,
}

/// Turn the viewport's vertex floats into triangles, shared materials and their tree.
fn prepare(vertices: &[f32]) -> Uploaded {
    let stride = 3 * VERTEX_FLOATS;
    let count = vertices.len() / stride;
    let mut corners = Vec::with_capacity(count);
    let mut material_of = Vec::with_capacity(count);
    let mut materials: Vec<[f32; 24]> = vec![];
    let mut index = std::collections::HashMap::<[u32; 24], u32>::new();
    for t in 0..count {
        let v = |k: usize| &vertices[t * stride + k * VERTEX_FLOATS..][..VERTEX_FLOATS];
        corners.push([0, 1, 2].map(|k| Vec3::from_slice(&v(k)[..3])));
        let first = v(0);
        let mut m = [0f32; 24];
        m[..3].copy_from_slice(&first[7..10]);
        m[3] = first[14];
        m[4..7].copy_from_slice(&first[15..18]);
        m[8..11].copy_from_slice(&first[20..23]);
        m[12..16].copy_from_slice(&first[25..29]);
        m[16..20].copy_from_slice(&first[29..33]);
        m[20..24].copy_from_slice(&first[33..37]);
        let key = m.map(f32::to_bits);
        let id = *index.entry(key).or_insert_with(|| {
            materials.push(m);
            materials.len() as u32 - 1
        });
        material_of.push(id);
    }
    let bvh = Bvh::build(&corners);
    let mut triangles = Vec::with_capacity(9 * count);
    let mut ordered = Vec::with_capacity(count);
    for &t in &bvh.order {
        let t = t as usize;
        let v = |k: usize| &vertices[t * stride + k * VERTEX_FLOATS..][..VERTEX_FLOATS];
        for (k, p) in corners[t].iter().enumerate() {
            let w = if k == 0 {
                f32::from_bits(material_of[t])
            } else {
                0.
            };
            triangles.push([p.x, p.y, p.z, w]);
        }
        for k in 0..3 {
            let c = v(k);
            triangles.push([c[4], c[5], c[6], c[18]]);
            triangles.push([c[19], c[23], c[24], 0.]);
        }
        ordered.push(corners[t]);
    }
    if materials.is_empty() {
        materials.push([0.; 24]);
    }
    Uploaded {
        nodes: bvh.nodes.clone(),
        triangles,
        materials,
        corners: ordered,
        bvh,
    }
}

/// A storage buffer holding `data` (at least one element, as bindings cannot be empty).
fn storage<T: bytemuck::Pod>(device: &wgpu::Device, label: &str, data: &[T]) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    let bytes: &[u8] = bytemuck::cast_slice(data);
    let contents = if bytes.is_empty() {
        vec![0u8; 16]
    } else {
        bytes.to_vec()
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: &contents,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
    })
}

fn read_back(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>, String> {
    let (tx, rx) = mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    rx.recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let data = buffer
        .get_mapped_range(..)
        .map_err(|e| e.to_string())?
        .to_vec();
    buffer.unmap();
    Ok(data)
}

/// The path tracer on one device.
pub struct PathTracer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    trace: wgpu::ComputePipeline,
    resolve: wgpu::ComputePipeline,
    frame: wgpu::Buffer,
    sampler: wgpu::Sampler,
    nodes: wgpu::Buffer,
    triangles: wgpu::Buffer,
    materials: wgpu::Buffer,
    maps: wgpu::TextureView,
    environment: wgpu::Buffer,
    environment_cdf: wgpu::Buffer,
    accumulation: wgpu::Buffer,
    display: wgpu::Buffer,
    rejected: wgpu::Buffer,
    bind: Option<wgpu::BindGroup>,
    scene: Option<TraceScene>,
    corners: Vec<[Vec3; 3]>,
    bvh: Bvh,
    settings: TraceSettings,
    /// Width and height of the environment image on the GPU.
    environment_size: [u32; 2],
    samples: u32,
    /// Rows each dispatch covers, adapted to [`DISPATCH_BUDGET`].
    rows: u32,
    seed: u32,
}

impl PathTracer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path tracer"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let buffer = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path tracer"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                buffer(1, true),
                buffer(2, true),
                buffer(3, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                buffer(6, true),
                buffer(7, true),
                buffer(8, false),
                buffer(9, false),
                buffer(10, false),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("path tracer"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let trace = pipeline("trace");
        let resolve = pipeline("resolve");
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("path tracer frame"),
            size: FRAME_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("path tracer maps"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let empty = |label: &str| storage::<u32>(&device, label, &[]);
        let maps = layer_array(&device, &queue, &[]);
        let mut tracer = Self {
            nodes: empty("bvh"),
            triangles: empty("triangles"),
            materials: empty("materials"),
            environment: empty("environment"),
            environment_cdf: empty("environment distribution"),
            accumulation: empty("accumulation"),
            display: empty("display"),
            rejected: storage::<u32>(&device, "rejected", &[0, 0, 0, 0]),
            device,
            queue,
            layout,
            trace,
            resolve,
            frame,
            sampler,
            maps,
            bind: None,
            scene: None,
            corners: vec![],
            bvh: Bvh::default(),
            settings: TraceSettings::default(),
            environment_size: [1, 1],
            samples: 0,
            rows: 32,
            seed: 0,
        };
        tracer.set_environment(None);
        tracer.resize();
        tracer
    }

    /// Take a new scene; the image starts over.
    pub fn set_scene(&mut self, scene: TraceScene) {
        let uploaded = prepare(&scene.vertices);
        self.nodes = storage(&self.device, "bvh", &uploaded.nodes);
        self.triangles = storage(&self.device, "triangles", &uploaded.triangles);
        self.materials = storage(&self.device, "materials", &uploaded.materials);
        self.maps = layer_array(&self.device, &self.queue, &scene.layers);
        self.corners = uploaded.corners;
        self.bvh = uploaded.bvh;
        if self.settings.focus_m <= 0. {
            self.settings.focus_m = scene.camera.distance;
        }
        self.scene = Some(scene);
        self.restart();
    }

    /// Take new settings; the image starts over when they change anything it shows.
    pub fn set_settings(&mut self, settings: &TraceSettings) {
        let settings = settings.sanitized();
        if settings == self.settings {
            return;
        }
        let environment_changed = settings.environment != self.settings.environment;
        let resized =
            [settings.width, settings.height] != [self.settings.width, self.settings.height];
        self.settings = settings;
        if environment_changed {
            self.set_environment(self.settings.environment.clone());
        }
        if resized {
            self.resize();
        }
        self.restart();
    }

    pub fn settings(&self) -> &TraceSettings {
        &self.settings
    }

    fn set_environment(&mut self, environment: Option<Arc<Environment>>) {
        let environment = environment.unwrap_or_else(|| Arc::new(Environment::uniform([0.; 3])));
        let pixels: Vec<[f32; 4]> = environment
            .pixels
            .iter()
            .map(|p| [p[0], p[1], p[2], 0.])
            .collect();
        self.environment = storage(&self.device, "environment", &pixels);
        self.environment_cdf = storage(
            &self.device,
            "environment distribution",
            &environment.distribution(),
        );
        self.environment_size = [environment.width, environment.height];
        self.bind = None;
    }

    fn resize(&mut self) {
        let pixels = (self.settings.width * self.settings.height) as usize;
        self.accumulation = storage(&self.device, "accumulation", &vec![[0f32; 4]; pixels]);
        self.display = storage(&self.device, "display", &vec![0u32; pixels]);
        self.bind = None;
    }

    /// Start the image over.
    pub fn restart(&mut self) {
        self.samples = 0;
        self.seed = self.seed.wrapping_add(0x9e37_79b9);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.clear_buffer(&self.accumulation, 0, None);
        encoder.clear_buffer(&self.rejected, 0, None);
        self.queue.submit([encoder.finish()]);
        self.bind = None;
    }

    /// Samples per pixel so far.
    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// Whether there is a scene and the target sample count is not reached.
    pub fn working(&self) -> bool {
        self.scene.is_some() && self.samples < self.settings.samples
    }

    fn bind_group(&mut self) -> &wgpu::BindGroup {
        if self.bind.is_none() {
            let entries = [
                (0, self.frame.as_entire_binding()),
                (1, self.nodes.as_entire_binding()),
                (2, self.triangles.as_entire_binding()),
                (3, self.materials.as_entire_binding()),
                (4, wgpu::BindingResource::TextureView(&self.maps)),
                (5, wgpu::BindingResource::Sampler(&self.sampler)),
                (6, self.environment.as_entire_binding()),
                (7, self.environment_cdf.as_entire_binding()),
                (8, self.accumulation.as_entire_binding()),
                (9, self.display.as_entire_binding()),
                (10, self.rejected.as_entire_binding()),
            ]
            .map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource });
            self.bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("path tracer"),
                layout: &self.layout,
                entries: &entries,
            }));
        }
        self.bind.as_ref().unwrap()
    }

    /// The frame uniform for rows `first..first + rows` of sample `sample`.
    fn frame_bytes(&self, first: u32, rows: u32) -> Vec<u8> {
        let Some(scene) = &self.scene else {
            return vec![0; FRAME_BYTES as usize];
        };
        let s = &self.settings;
        let c = &scene.camera;
        let aspect = s.width as f32 / s.height as f32;
        let l = &scene.lighting;
        let sun = Vec3::new(l[0], l[1], l[2]).normalize_or(Vec3::Y);
        let key = if s.key_light {
            l[3].max(0.) * SUN_IRRADIANCE
        } else {
            0.
        };
        let ambient = l[4].max(0.);
        let look = &scene.look;
        let bottom = if look.gradient { look.bottom } else { look.top };
        let sky = |c: [u8; 3]| c.map(|v| linear(v) * SKY_RADIANCE * ambient);
        let (sky_top, sky_bottom) = (sky(look.top), sky(bottom));
        let gain = 2f32.powf(s.exposure_ev);
        let backdrop = |c: [u8; 3]| c.map(|v| untone(f32::from(v) / 255.) / gain);
        let environment = s.environment.is_some();
        let mut f: Vec<f32> = vec![];
        f.extend(c.eye.to_array());
        f.push(f32::from(c.orthographic));
        f.extend(c.forward.to_array());
        f.push(c.tan_half_fov);
        f.extend(c.right.to_array());
        f.push(aspect);
        f.extend(c.up.to_array());
        f.push(c.half_height);
        f.extend([s.aperture_mm / 1000., s.focus_m, gain, s.bounces as f32]);
        f.extend(sun.to_array());
        f.push(SUN_COS_RADIUS);
        f.extend([key; 3]);
        f.push(0.);
        f.extend(sky_top);
        f.push(s.environment_gain);
        f.extend(sky_bottom);
        f.push(s.environment_turn_degrees.to_radians());
        f.extend(look.floor_color.map(linear));
        f.push(scene.ground_y);
        f.extend([
            f32::from(s.floor && look.floor),
            f32::from(environment),
            f32::from(s.environment_background),
            0.,
        ]);
        f.extend(backdrop(look.top));
        f.push(0.);
        f.extend(backdrop(bottom));
        f.push(0.);
        let mut bytes: Vec<u8> = bytemuck::cast_slice(&f).to_vec();
        let u = [
            s.width,
            s.height,
            self.environment_size[0],
            self.environment_size[1],
            self.samples,
            first,
            rows,
            self.seed,
        ];
        bytes.extend_from_slice(bytemuck::cast_slice(&u));
        debug_assert_eq!(bytes.len() as u64, FRAME_BYTES);
        bytes
    }

    /// Add one sample to every pixel, in dispatches of a few rows each so that no one of them
    /// holds the GPU long. Does nothing once the target is reached.
    pub fn step(&mut self) {
        if !self.working() {
            return;
        }
        let (width, height) = (self.settings.width, self.settings.height);
        let mut first = 0;
        while first < height {
            let rows = self.rows.min(height - first);
            let bytes = self.frame_bytes(first, rows);
            self.queue.write_buffer(&self.frame, 0, &bytes);
            let started = Instant::now();
            let bind = self.bind_group().clone();
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("path trace"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.trace);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(width.div_ceil(8), rows.div_ceil(8), 1);
            }
            self.queue.submit([encoder.finish()]);
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            // Aim the next dispatch at the time budget.
            let spent = started.elapsed().as_secs_f32().max(1e-4);
            let ideal = rows as f32 * DISPATCH_BUDGET.as_secs_f32() / spent;
            self.rows =
                (ideal.clamp(rows as f32 * 0.5, rows as f32 * 2.) as u32).clamp(8, height.max(8));
            first += rows;
        }
        self.samples += 1;
    }

    fn resolved(&mut self) -> Result<Vec<u8>, String> {
        let bytes = self.frame_bytes(0, 0);
        self.queue.write_buffer(&self.frame, 0, &bytes);
        let bind = self.bind_group().clone();
        let pixels = self.settings.width * self.settings.height;
        let size = pixels as u64 * 4;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render image"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.resolve);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(pixels.div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.display, 0, &read, 0, size);
        self.queue.submit([encoder.finish()]);
        read_back(&self.device, &read)
    }

    /// The image so far: tone-mapped sRGB pixels, RGBA8, row by row from the top.
    pub fn image(&mut self) -> Result<Vec<u8>, String> {
        self.resolved()
    }

    /// The mean light at each pixel so far, linear (before exposure and tone mapping).
    pub fn linear(&self) -> Result<Vec<[f32; 3]>, String> {
        let size = (self.settings.width * self.settings.height) as u64 * 16;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render light"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.accumulation, 0, &read, 0, size);
        self.queue.submit([encoder.finish()]);
        let bytes = read_back(&self.device, &read)?;
        let sums: &[[f32; 4]] = bytemuck::cast_slice(&bytes);
        Ok(sums
            .iter()
            .map(|s| {
                let n = s[3].max(1.);
                [s[0] / n, s[1] / n, s[2] / n]
            })
            .collect())
    }

    /// Samples dropped because they came out not finite.
    pub fn rejected(&self) -> Result<u32, String> {
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rejected samples"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.rejected, 0, &read, 0, 16);
        self.queue.submit([encoder.finish()]);
        let bytes = read_back(&self.device, &read)?;
        Ok(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
    }

    /// Distance from the camera to what is under an image position (0..1 across, 0..1 down),
    /// to focus the lens there.
    pub fn depth_at(&self, position: [f32; 2]) -> Option<f32> {
        let c = self.scene.as_ref()?.camera;
        let aspect = self.settings.width as f32 / self.settings.height as f32;
        let ndc = [position[0] * 2. - 1., 1. - position[1] * 2.];
        let (origin, dir) = if c.orthographic {
            (
                c.eye + c.right * ndc[0] * c.half_height * aspect + c.up * ndc[1] * c.half_height,
                c.forward,
            )
        } else {
            let t = c.tan_half_fov;
            (
                c.eye,
                (c.forward + c.right * ndc[0] * t * aspect + c.up * ndc[1] * t).normalize(),
            )
        };
        let scene = self.scene.as_ref()?;
        // The floor, as the shader has it.
        let floor =
            (self.settings.floor && scene.look.floor && dir.y < 0. && origin.y > scene.ground_y)
                .then(|| (scene.ground_y - origin.y) / dir.y);
        let mesh = self
            .bvh
            .intersect(&self.corners_by_original(), origin, dir, 1e9)
            .map(|(_, d, ..)| d);
        let d = match (mesh, floor) {
            (Some(a), Some(b)) => a.min(b),
            (a, b) => a.or(b)?,
        };
        Some(d * dir.dot(c.forward))
    }

    /// Triangle corners by their original index (the tree's `order` names those).
    fn corners_by_original(&self) -> Vec<[Vec3; 3]> {
        let mut out = vec![[Vec3::ZERO; 3]; self.corners.len()];
        for (k, &t) in self.bvh.order.iter().enumerate() {
            out[t as usize] = self.corners[k];
        }
        out
    }
}

/// Bytes of the frame uniform: thirteen float and two integer vectors.
const FRAME_BYTES: u64 = 15 * 16;

/// The layers in one texture array (one white layer when there are none). Every layer has the
/// same size; the viewport's layers already do.
fn layer_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &[Arc<GraphicImage>],
) -> wgpu::TextureView {
    let white = Arc::new(GraphicImage {
        key: 0,
        size: [1, 1],
        rgba: vec![255; 4],
    });
    let side = layers.first().map_or([1, 1], |l| l.size);
    let layers: Vec<Arc<GraphicImage>> = if layers.is_empty() {
        vec![white]
    } else {
        layers
            .iter()
            .map(|l| {
                if l.size == side {
                    l.clone()
                } else {
                    let buffer = image::RgbaImage::from_raw(l.size[0], l.size[1], l.rgba.clone())
                        .unwrap_or_else(|| image::RgbaImage::new(1, 1));
                    Arc::new(GraphicImage {
                        key: l.key,
                        size: side,
                        rgba: image::imageops::resize(
                            &buffer,
                            side[0],
                            side[1],
                            image::imageops::FilterType::Triangle,
                        )
                        .into_raw(),
                    })
                }
            })
            .collect()
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("path tracer maps"),
        size: wgpu::Extent3d {
            width: side[0],
            height: side[1],
            depth_or_array_layers: layers.len() as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (k, layer) in layers.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: k as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &layer.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * side[0]),
                rows_per_image: Some(side[1]),
            },
            wgpu::Extent3d {
                width: side[0],
                height: side[1],
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// What the editor asks of the render thread.
pub enum Request {
    Scene(Box<TraceScene>),
    Settings(TraceSettings),
    /// Write the image so far to a PNG file.
    Save(std::path::PathBuf),
    /// Report the distance to what is under an image position (0..1 each way).
    Focus([f32; 2]),
}

/// What the render thread reports.
#[derive(Clone, Debug)]
pub enum Report {
    /// The image so far.
    Image {
        samples: u32,
        size: [u32; 2],
        rgba: Arc<Vec<u8>>,
        /// Time spent on this image so far.
        elapsed: Duration,
    },
    Saved(std::path::PathBuf),
    /// Distance to the surface under the asked position, m.
    Focus(f32),
    Failed(String),
}

/// The path tracer on its own thread and device: the editor sends it scenes and settings and
/// shows the images it sends back, never waiting for it.
pub struct RenderWorker {
    requests: Option<mpsc::Sender<Request>>,
    reports: mpsc::Receiver<Report>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RenderWorker {
    /// Start the thread on a new device of `adapter`; `repaint` is called after each report.
    pub fn spawn(adapter: wgpu::Adapter, repaint: impl Fn() + Send + 'static) -> Self {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, reports) = mpsc::channel::<Report>();
        let thread = std::thread::Builder::new()
            .name("photoreal render".into())
            .spawn(move || {
                let device = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("photoreal render"),
                    required_limits: adapter.limits(),
                    ..Default::default()
                }));
                let (device, queue) = match device {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = outbox.send(Report::Failed(format!("No GPU for the render: {e}")));
                        repaint();
                        return;
                    }
                };
                run(PathTracer::new(device, queue), inbox, outbox, repaint);
            })
            .expect("render thread");
        Self {
            requests: Some(requests),
            reports,
            thread: Some(thread),
        }
    }

    pub fn send(&self, request: Request) {
        if let Some(r) = &self.requests {
            let _ = r.send(request);
        }
    }

    /// Reports since the last call, oldest first.
    pub fn reports(&self) -> Vec<Report> {
        self.reports.try_iter().collect()
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        // Closing the channel ends the thread after its current pass; the editor does not wait
        // for it, and the thread's device and its memory go when it ends.
        self.requests = None;
        drop(self.thread.take());
    }
}

/// How often the thread sends the image while it refines.
const PREVIEW_EVERY: Duration = Duration::from_millis(250);

fn run(
    mut tracer: PathTracer,
    inbox: mpsc::Receiver<Request>,
    outbox: mpsc::Sender<Report>,
    repaint: impl Fn(),
) {
    let mut started = Instant::now();
    let mut sent = Instant::now();
    let mut fresh = false;
    loop {
        // Wait for work when there is none; otherwise take what came in and carry on.
        let first = if tracer.working() {
            match inbox.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match inbox.recv() {
                Ok(r) => Some(r),
                Err(_) => return,
            }
        };
        for request in first.into_iter().chain(inbox.try_iter()) {
            match request {
                Request::Scene(scene) => {
                    tracer.set_scene(*scene);
                    started = Instant::now();
                    fresh = true;
                }
                Request::Settings(settings) => {
                    let before = tracer.samples();
                    tracer.set_settings(&settings);
                    if tracer.samples() != before || before == 0 {
                        started = Instant::now();
                        fresh = true;
                    }
                }
                Request::Save(path) => {
                    let report = tracer.image().and_then(|rgba| {
                        let s = tracer.settings();
                        image::save_buffer(&path, &rgba, s.width, s.height, image::ColorType::Rgba8)
                            .map_err(|e| format!("{}: {e}", path.display()))
                    });
                    let _ = outbox.send(match report {
                        Ok(()) => Report::Saved(path),
                        Err(e) => Report::Failed(e),
                    });
                    repaint();
                }
                Request::Focus(position) => {
                    if let Some(d) = tracer.depth_at(position) {
                        let _ = outbox.send(Report::Focus(d));
                        repaint();
                    }
                }
            }
        }
        if tracer.working() {
            tracer.step();
        }
        let done = !tracer.working();
        if tracer.scene.is_some() && (fresh || sent.elapsed() >= PREVIEW_EVERY || done) {
            if !fresh && done && tracer.samples() == 0 {
                continue;
            }
            if done
                && let Ok(n) = tracer.rejected()
                && n > 0
            {
                tracing::warn!("photoreal render: {n} samples were not finite and were dropped");
            }
            fresh = false;
            sent = Instant::now();
            match tracer.image() {
                Ok(rgba) => {
                    let s = tracer.settings();
                    let _ = outbox.send(Report::Image {
                        samples: tracer.samples(),
                        size: [s.width, s.height],
                        rgba: Arc::new(rgba),
                        elapsed: started.elapsed(),
                    });
                }
                Err(e) => {
                    let _ = outbox.send(Report::Failed(e));
                }
            }
            repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backdrop_comes_back_through_the_tone_curve() {
        // The shader's curve, in Rust.
        let tone = |x: f32| {
            let m = ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0., 1.);
            if m <= 0.0031308 {
                12.92 * m
            } else {
                1.055 * m.powf(1. / 2.4) - 0.055
            }
        };
        for display in [0.05, 0.2, 0.35, 0.5, 0.8, 0.95] {
            let back = tone(untone(display));
            assert!((back - display).abs() < 2e-3, "{display} → {back}");
        }
        assert_eq!(untone(0.), 0.);
    }

    #[test]
    fn an_environment_samples_in_proportion_to_its_light() {
        let mut e = Environment::uniform([1.; 3]);
        e.pixels[5] = [9.; 3];
        let cdf = e.distribution();
        let (w, h) = (8, 4);
        assert_eq!(cdf.len(), h * (w + 1) + h + 1);
        for j in 0..h {
            let row = &cdf[j * (w + 1)..(j + 1) * (w + 1)];
            assert_eq!(row[0], 0.);
            assert!((row[w] - 1.).abs() < 1e-6);
            assert!(row.windows(2).all(|p| p[1] >= p[0]));
        }
        // The bright pixel (row 0, column 5) takes most of its row.
        let row = &cdf[..w + 1];
        assert!(row[6] - row[5] > 0.5);
        let marginal = &cdf[h * (w + 1)..];
        assert!((marginal[h] - 1.).abs() < 1e-6);
    }

    #[test]
    fn viewport_triangles_share_materials_and_keep_their_corners() {
        let mut vertices = vec![];
        for (k, color) in [[1., 0., 0.], [0., 1., 0.], [1., 0., 0.]]
            .iter()
            .enumerate()
        {
            for c in 0..3 {
                let mut v = [0f32; VERTEX_FLOATS];
                v[0] = k as f32 + (c == 1) as u8 as f32;
                v[1] = (c == 2) as u8 as f32;
                v[3] = 1.;
                v[6] = 1.;
                v[7..10].copy_from_slice(color);
                v[14] = 0.5;
                v[25..29].copy_from_slice(&[-1., 0., -1., -1.]);
                vertices.extend_from_slice(&v);
            }
        }
        let u = prepare(&vertices);
        assert_eq!(u.materials.len(), 2);
        assert_eq!(u.triangles.len(), 27);
        assert_eq!(u.corners.len(), 3);
        assert_eq!(u.materials[0][3], 0.5);
    }
}
