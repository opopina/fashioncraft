//! Explicit hardware regression: an animation recorded with the GPU solver carries the cloth
//! like the CPU recording does. Never run as part of ordinary CPU checks.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use glam::{DVec2, DVec3};
use marvelous_cloth::{
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
    assert!(gpu_rise > 0.02, "the cloth rose {gpu_rise} m on the GPU");
    // The GPU solver resolves contacts without the CPU's mid-step refresh, so it carries the
    // cloth somewhat further (about 30 % here): the same motion, not the same numbers.
    assert!(
        (gpu_rise - cpu_rise).abs() < 0.5 * cpu_rise,
        "GPU {gpu_rise} vs CPU {cpu_rise}"
    );
}
