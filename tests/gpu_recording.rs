//! Explicit hardware regression: an animation recorded with the GPU solver carries the cloth
//! like the CPU recording does. Never run as part of ordinary CPU checks.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    avatar_animation::{AvatarAnimation, PoseKey, Recorder, run, run_stepping},
    avatar_rig::Pose,
    fitting::{AvatarPose, FittingProject},
    garment::{
        CompiledGarment,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
    gpu_vbd::GpuVbd,
    model::State,
    vbd::VbdSettings,
};
use glam::{DVec2, DVec3};
use std::{path::PathBuf, sync::Arc};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// A 10 cm square of cloth floating 1 cm above the right forearm, and the arm raised 25° in
/// half a second at 20 frames per second.
fn scene() -> (
    FittingProject,
    SurfaceMesh,
    Arc<CompiledGarment>,
    State,
    AvatarAnimation,
) {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let rig = AvatarPose::from_sidecar(&mannequin())
        .unwrap()
        .rig(&rest)
        .unwrap();
    let forearm = &rig.bones[rig.bone("forearm.R").unwrap()];
    let near = forearm.head + (forearm.tail - forearm.head) * 0.65;
    let top = rest
        .vertices
        .iter()
        .filter(|p| (p.x - near.x).abs() < 0.06 && (p.z - near.z).abs() < 0.06)
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.1, 0.1),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
    let mut state = g.initial_state.clone();
    let [a, b, c] = g.mesh.triangles[0].vertices.map(|i| state.x[i]);
    let u = (b - a).normalize();
    let v = (b - a).cross(c - a).cross(u).normalize();
    let centre = state.x.iter().sum::<DVec3>() / state.x.len() as f64;
    let target = DVec3::new(near.x, top + 0.01, near.z);
    for p in &mut state.x {
        let d = *p - centre;
        *p = target + DVec3::X * d.dot(u) + DVec3::Z * d.dot(v);
    }
    let raised = Pose([("upper_arm.R".to_string(), [0., 0., -25.])].into());
    let animation = AvatarAnimation {
        keys: vec![
            PoseKey {
                time_s: 0.,
                pose: Pose::default(),
            },
            PoseKey {
                time_s: 0.5,
                pose: raised,
            },
        ],
        fps: 20.,
        ..Default::default()
    };
    (project, rest, g, state, animation)
}

/// Whether `p` is inside the closed `body`: a ray from it crosses the surface an odd number of
/// times.
fn inside(body: &SurfaceMesh, p: DVec3) -> bool {
    let dir = DVec3::new(1., 0.123, 0.0457).normalize();
    let crossings = body
        .triangles
        .iter()
        .filter(|t| {
            let [a, b, c] = t.map(|i| body.vertices[i]);
            let (e1, e2) = (b - a, c - a);
            let h = dir.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-14 {
                return false;
            }
            let s = p - a;
            let q = s.cross(e1);
            let (u, v, t) = (s.dot(h) / det, dir.dot(q) / det, e2.dot(q) / det);
            u >= 0. && v >= 0. && u + v <= 1. && t > 0.
        })
        .count();
    crossings % 2 == 1
}

fn rise(start: &[DVec3], end: &[DVec3]) -> f64 {
    (end.iter().map(|p| p.y).sum::<f64>() - start.iter().map(|p| p.y).sum::<f64>())
        / start.len() as f64
}

#[test]
#[ignore = "uses a hardware GPU; run explicitly with --ignored"]
fn a_recording_on_the_gpu_carries_the_cloth_like_the_cpu_one() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = GpuVbd::new(device, queue);
    let (project, rest, g, state, animation) = scene();
    // The editor's friction.
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 16,
        gravity: DVec3::ZERO,
        friction: 0.3,
        ..Default::default()
    };
    let start = state.x.clone();
    let cpu = run(
        g.clone(),
        &rest,
        state.clone(),
        Recorder::new(&project, &animation).unwrap(),
        &settings,
        |_, _| true,
    )
    .unwrap();
    let on_gpu = run_stepping(
        g,
        &rest,
        state,
        Recorder::new(&project, &animation).unwrap(),
        &settings,
        |solver, state, step| gpu.step(solver, state, step, None).map(|_| ()),
        |_, _| true,
    )
    .unwrap();
    assert!(on_gpu.0.finished());
    let take = on_gpu.0.into_take();
    assert_eq!(take.frames.len(), 11);
    assert!(on_gpu.1.x.iter().all(|p| p.is_finite()));
    let (cpu_rise, gpu_rise) = (rise(&start, &cpu.1.x), rise(&start, &on_gpu.1.x));
    eprintln!("the cloth rose {cpu_rise:.4} m on the CPU, {gpu_rise:.4} m on the GPU");
    assert!(gpu_rise > 0.02, "the cloth rose {gpu_rise} m on the GPU");
    // The arm rises 11 cm under the cloth, which slides off it on both backends. Their local
    // solves differ (f32 and its own line search on the GPU), and so does how long the cloth
    // stays on: 3.4 cm on the CPU, 3.0 cm on the GPU now that steps split (3.1 and 5.1 cm
    // when they did not). The same motion, not the same numbers, and never into the arm.
    assert!(
        (0.5..2.).contains(&(gpu_rise / cpu_rise)),
        "GPU {gpu_rise} vs CPU {cpu_rise}"
    );
    let body = cpu.0.obstacle_now();
    for (label, x) in [("CPU", &cpu.1.x), ("GPU", &on_gpu.1.x)] {
        let count = x.iter().filter(|p| inside(&body, **p)).count();
        assert_eq!(
            count, 0,
            "{count} cloth vertices inside the arm on the {label}"
        );
    }
}

/// The largest distance between two points of the cloth.
fn span(x: &[DVec3]) -> f64 {
    x.iter()
        .flat_map(|a| x.iter().map(move |b| a.distance(*b)))
        .fold(0., f64::max)
}

#[test]
#[ignore = "uses a hardware GPU; run explicitly with --ignored"]
fn fabric_keys_shrink_the_cloth_on_the_gpu_as_on_the_cpu() {
    use fashioncraft::avatar_animation::set_fabric_key;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = GpuVbd::new(device, queue);
    let (project, rest, g, state, mut animation) = scene();
    // The square, drawn in to 70 % both ways over the half second.
    let piece = g.panels[0].id.clone();
    let keys = animation.fabric_keys.entry(piece).or_default();
    set_fabric_key(keys, 0., 0., [100., 100.]);
    set_fabric_key(keys, 0.5, 0., [70., 70.]);
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 16,
        gravity: DVec3::ZERO,
        friction: 0.3,
        ..Default::default()
    };
    let before = span(&state.x);
    let cpu = run(
        g.clone(),
        &rest,
        state.clone(),
        Recorder::new(&project, &animation).unwrap(),
        &settings,
        |_, _| true,
    )
    .unwrap();
    let on_gpu = run_stepping(
        g,
        &rest,
        state,
        Recorder::new(&project, &animation).unwrap(),
        &settings,
        |solver, state, step| gpu.step(solver, state, step, None).map(|_| ()),
        |_, _| true,
    )
    .unwrap();
    let (on_cpu, on_gpu) = (span(&cpu.1.x) / before, span(&on_gpu.1.x) / before);
    assert!(on_cpu < 0.85 && on_gpu < 0.85, "CPU {on_cpu}, GPU {on_gpu}");
    assert!((on_cpu - on_gpu).abs() < 0.1, "CPU {on_cpu}, GPU {on_gpu}");
}
