//! Optional GPU check of friction: the same inclined-plane acceptance as tests/friction.rs on
//! the GPU solver, with its per-vertex forces checked against the f64 CPU terms every step.
//! Ordinary test runs must not initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    garment::GarmentTemplate,
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_friction_matches_the_inclined_plane() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut gpu = GpuVbd::new(device, queue);
    gpu.check_gradients = true;
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pattern = marvelous_cloth::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 3,
        ny: 3,
    };
    p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
    p.placement.translation = DVec3::new(-0.06, 0.00305, 0.06);
    let floor = SurfaceMesh {
        vertices: vec![
            DVec3::new(-40., 0., -20.),
            DVec3::new(40., 0., -20.),
            DVec3::new(0., 0., 40.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let g = Arc::new(t.compile().unwrap());
    let (g_acc, dt) = (9.81, 1. / 240.);
    for (mu, sticks) in [(0.3, false), (0.6, true)] {
        let mut s = g.initial_state.clone();
        let mut solver = VbdSolver::new(g.clone(), Some(&floor), &s).unwrap();
        let rest = VbdSettings {
            dt,
            iterations: 64,
            damping: 0.,
            friction: mu,
            ..Default::default()
        };
        for _ in 0..60 {
            gpu.step(&mut solver, &mut s, &rest, None).unwrap();
        }
        s.v.fill(DVec3::ZERO);
        let a = 20f64.to_radians();
        let tilted = VbdSettings {
            gravity: DVec3::new(g_acc * a.sin(), -g_acc * a.cos(), 0.),
            ..rest.clone()
        };
        let mean_vx = |s: &marvelous_cloth::model::State| {
            s.v.iter().map(|v| v.x).sum::<f64>() / s.v.len() as f64
        };
        let mut samples = vec![];
        for i in 0..48 {
            gpu.step(&mut solver, &mut s, &tilted, None).unwrap();
            if i % 12 == 11 {
                samples.push(mean_vx(&s));
            }
        }
        let acc = (samples[3] - samples[0]) / (36. * dt);
        let expected = (g_acc * (a.sin() - mu * a.cos())).max(0.);
        if sticks {
            assert!(acc.abs() < 0.02, "μ {mu}: {acc}");
        } else {
            // f32 terms: a looser bound than the f64 CPU test.
            assert!(
                (acc - expected).abs() / expected < 0.08,
                "μ {mu}: {acc} vs {expected}"
            );
        }
    }
}
