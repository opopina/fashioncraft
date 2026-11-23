//! Photoreal render on a hardware GPU: headless renders of simple scenes converge, with no
//! sample ever coming out not finite. Never run as part of ordinary CPU checks.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use fashioncraft::render::{
    GraphicImage,
    path_trace::{Environment, PathTracer, TraceCamera, TraceMaterial, TraceScene, TraceSettings},
};
use glam::Vec3;
use std::sync::Arc;

fn tracer() -> PathTracer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    PathTracer::new(device, queue)
}

fn camera(eye: Vec3, target: Vec3) -> TraceCamera {
    let forward = (target - eye).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    let tan_half_fov = 20f32.to_radians().tan();
    TraceCamera {
        eye,
        forward,
        right,
        up,
        tan_half_fov,
        orthographic: false,
        half_height: eye.distance(target) * tan_half_fov,
        distance: eye.distance(target),
    }
}

/// A sphere of latitude and longitude bands, wound so its outside is the material's front.
fn sphere(scene: &mut TraceScene, centre: Vec3, radius: f32, m: &TraceMaterial) {
    let (rings, segments) = (24, 48);
    let point = |i: usize, j: usize| {
        let theta = std::f32::consts::PI * j as f32 / rings as f32;
        let phi = std::f32::consts::TAU * i as f32 / segments as f32;
        Vec3::new(
            theta.sin() * phi.cos(),
            theta.cos(),
            theta.sin() * phi.sin(),
        )
    };
    for j in 0..rings {
        for i in 0..segments {
            let [a, b, c, d] = [
                point(i, j),
                point(i + 1, j),
                point(i, j + 1),
                point(i + 1, j + 1),
            ];
            let uv = |i: usize, j: usize| {
                [
                    i as f32 / segments as f32 * 4.,
                    j as f32 / rings as f32 * 2.,
                ]
            };
            for (tri, uvs) in [
                ([a, c, b], [uv(i, j), uv(i, j + 1), uv(i + 1, j)]),
                ([b, c, d], [uv(i + 1, j), uv(i, j + 1), uv(i + 1, j + 1)]),
            ] {
                // The viewport's normals point along the winding normal, which faces inwards
                // here, so the outside shows the front colour.
                scene.add_triangle(tri.map(|n| centre + n * radius), tri.map(|n| -n), uvs, m);
            }
        }
    }
}

fn spread(pixels: &[[f32; 3]], mask: &[bool]) -> (f32, f32) {
    let values: Vec<f32> = pixels
        .iter()
        .zip(mask)
        .filter(|(_, m)| **m)
        .map(|(p, _)| (p[0] + p[1] + p[2]) / 3.)
        .collect();
    let n = values.len() as f32;
    let mean = values.iter().sum::<f32>() / n;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
    (mean, var.sqrt())
}

fn finite(pixels: &[[f32; 3]]) -> bool {
    pixels.iter().flatten().all(|v| v.is_finite() && *v >= 0.)
}

#[test]
#[ignore = "uses a hardware GPU; run explicitly with --ignored"]
fn a_grey_sphere_in_a_uniform_sky_converges_to_its_albedo_without_nans() {
    let mut t = tracer();
    let mut scene = TraceScene::new(camera(Vec3::new(0., 0., 3.), Vec3::ZERO));
    let m = TraceMaterial {
        color: [0.5; 3],
        back: [0.5; 3],
        roughness: 1.,
        ..Default::default()
    };
    sphere(&mut scene, Vec3::ZERO, 0.5, &m);
    t.set_scene(scene);
    let settings = TraceSettings {
        width: 64,
        height: 64,
        samples: 1024,
        environment: Some(Arc::new(Environment::uniform([1.; 3]))),
        key_light: false,
        floor: false,
        ..Default::default()
    };
    t.set_settings(&settings);
    // The sphere (about 15 pixels in radius) covers the middle of the image: its face towards
    // the camera, where Fresnel reflects least.
    let mask: Vec<bool> = (0..64 * 64)
        .map(|k| {
            let (x, y) = ((k % 64) as f32 - 31.5, (k / 64) as f32 - 31.5);
            x * x + y * y < 8. * 8.
        })
        .collect();
    let mut checkpoints: Vec<Vec<[f32; 3]>> = vec![];
    while t.samples() < 1024 {
        t.step();
        if [16, 64, 256, 1024].contains(&t.samples()) {
            let pixels = t.linear().unwrap();
            assert!(finite(&pixels), "at {} samples", t.samples());
            checkpoints.push(pixels);
        }
    }
    assert_eq!(t.rejected().unwrap(), 0, "samples that were not finite");
    // A convex object in a uniform sky sees only the sky: it sends back its albedo (the
    // diffuse half less what Fresnel reflects, plus that reflection).
    let (mean, spread) = spread(&checkpoints[3], &mask);
    eprintln!("mean {mean}, spread {spread} at 1024 samples");
    assert!((0.47..0.56).contains(&mean), "mean {mean}");
    // The background is the sky itself.
    assert!((checkpoints[3][0][0] - 1.).abs() < 1e-3);
    // Noise falls as one over the square root of the samples: each fourfold step halves the
    // change the image still makes.
    let changes: Vec<f32> = checkpoints
        .windows(2)
        .map(|w| {
            w[1].iter()
                .zip(&w[0])
                .map(|(a, b)| (a[0] - b[0]).abs())
                .sum::<f32>()
                / w[0].len() as f32
        })
        .collect();
    eprintln!("mean change between fourfold steps: {changes:?}");
    assert!(changes.windows(2).all(|w| w[1] < 0.7 * w[0]), "{changes:?}");
    // The tone-mapped image is opaque and lit.
    let rgba = t.image().unwrap();
    assert_eq!(rgba.len(), 64 * 64 * 4);
    assert!(rgba.chunks(4).all(|p| p[3] == 255));
}

#[test]
#[ignore = "uses a hardware GPU; run explicitly with --ignored"]
fn a_textured_lace_scene_with_every_light_converges_without_nans() {
    let mut t = tracer();
    let mut scene = TraceScene::new(camera(Vec3::new(0.6, 0.9, 2.2), Vec3::new(0., 0.4, 0.)));
    // Layers: a check colour image, a bumpy normal map and a holed opacity map.
    let side = 64u32;
    let layer = |f: &dyn Fn(u32, u32) -> [u8; 4]| {
        Arc::new(GraphicImage {
            key: 0,
            size: [side; 2],
            rgba: (0..side * side)
                .flat_map(|k| f(k % side, k / side))
                .collect(),
        })
    };
    scene.layers.push(layer(&|x, y| {
        if (x / 8 + y / 8) % 2 == 0 {
            [220, 60, 60, 255]
        } else {
            [240, 240, 240, 255]
        }
    }));
    scene
        .layers
        .push(layer(&|x, _| [128 + ((x % 8) as u8) * 12, 128, 230, 255]));
    scene.layers.push(layer(&|x, y| {
        let (dx, dy) = ((x % 16) as i32 - 8, (y % 16) as i32 - 8);
        if dx * dx + dy * dy < 25 {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    }));
    let lace = TraceMaterial {
        color: [0.9, 0.85, 0.8],
        back: [0.3, 0.3, 0.6],
        roughness: 0.6,
        color_layer: Some(0),
        normal_layer: Some(1),
        opacity_layer: Some(2),
        weave: [2., 0.002, 0.4],
        ..Default::default()
    };
    // A draped panel: a wavy sheet over a metal sphere on the floor.
    let (nx, ny) = (24, 24);
    let at = |i: usize, j: usize| {
        let (u, v) = (i as f32 / nx as f32, j as f32 / ny as f32);
        Vec3::new(u - 0.5, 0.2 + v * 0.8, 0.1 * (u * 12.).sin())
    };
    for j in 0..ny {
        for i in 0..nx {
            let [a, b, c, d] = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
            let uv = |i: usize, j: usize| [i as f32 / 6., j as f32 / 6.];
            let n = (b - a).cross(c - a).normalize();
            scene.add_triangle(
                [a, b, c],
                [n; 3],
                [uv(i, j), uv(i + 1, j), uv(i, j + 1)],
                &lace,
            );
            scene.add_triangle(
                [b, d, c],
                [n; 3],
                [uv(i + 1, j), uv(i + 1, j + 1), uv(i, j + 1)],
                &lace,
            );
        }
    }
    let gold = TraceMaterial {
        color: [1.0, 0.78, 0.34],
        back: [1.0, 0.78, 0.34],
        roughness: 0.25,
        metalness: 1.,
        ..Default::default()
    };
    sphere(&mut scene, Vec3::new(0.35, 0.15, -0.3), 0.15, &gold);
    t.set_scene(scene);
    // A sky with a bright patch, so the environment's sampling matters.
    let mut sky = Environment::uniform([0.3, 0.35, 0.45]);
    sky.pixels[2] = [40., 38., 30.];
    let settings = TraceSettings {
        width: 96,
        height: 64,
        samples: 256,
        environment: Some(Arc::new(sky)),
        aperture_mm: 8.,
        focus_m: 2.,
        ..Default::default()
    };
    t.set_settings(&settings);
    let mut previous: Option<Vec<[f32; 3]>> = None;
    let mut changes = vec![];
    while t.samples() < 256 {
        t.step();
        if [32, 64, 128, 256].contains(&t.samples()) {
            let pixels = t.linear().unwrap();
            assert!(finite(&pixels), "at {} samples", t.samples());
            if let Some(p) = &previous {
                let change = pixels
                    .iter()
                    .zip(p)
                    .map(|(a, b)| (0..3).map(|c| (a[c] - b[c]).abs()).sum::<f32>())
                    .sum::<f32>()
                    / pixels.len() as f32;
                changes.push(change);
            }
            previous = Some(pixels);
        }
    }
    eprintln!("mean change between doublings: {changes:?}");
    assert_eq!(t.rejected().unwrap(), 0, "samples that were not finite");
    // Each doubling of the samples moves the image less (by about 1/√2 each time).
    assert!(changes.windows(2).all(|w| w[1] < w[0]), "{changes:?}");
    // Starting over (a new setting) clears the image.
    t.set_settings(&TraceSettings {
        exposure_ev: 1.,
        ..settings
    });
    assert_eq!(t.samples(), 0);
    t.step();
    assert!(finite(&t.linear().unwrap()));
}
