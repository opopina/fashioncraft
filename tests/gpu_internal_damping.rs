//! Optional GPU check of internal damping: a stretched weightless square settles on the GPU
//! solver as in tests/internal_damping.rs, and a falling one is not slowed. Ordinary test
//! runs must not initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use fashioncraft::{
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate, template::FabricProperties},
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

fn square(damping: f64) -> Arc<CompiledGarment> {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    t.fabrics[0].properties = FabricProperties {
        internal_damping: damping,
        ..Default::default()
    };
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pattern = fashioncraft::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 3,
        ny: 3,
    };
    p.placement.translation = DVec3::new(-0.06, 1., 0.);
    Arc::new(t.compile().unwrap())
}

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_internal_damping_settles_stretching_but_not_a_fall() {
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
    // Energies and forces of every vertex against the f64 terms, each step.
    gpu.check_gradients = true;
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let ring = |damping: f64| {
        let g = square(damping);
        let mut s = g.initial_state.clone();
        let centre = s.x.iter().sum::<DVec3>() / s.x.len() as f64;
        for p in &mut s.x {
            let d = *p - centre;
            *p = centre + DVec3::new(d.x * 1.08, d.y * 1.08, d.z);
        }
        let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
        let settings = VbdSettings {
            dt: 1. / 240.,
            iterations: 32,
            damping: 0.,
            gravity: DVec3::ZERO,
            ..Default::default()
        };
        for _ in 0..72 {
            gpu.step(&mut solver, &mut s, &settings, None).unwrap();
        }
        s.v.iter()
            .zip(&g.mesh.masses)
            .map(|(v, m)| 0.5 * m * v.length_squared())
            .sum::<f64>()
    };
    let (free, damped) = (ring(0.), ring(0.01));
    assert!(free > 1e-7 && damped < 0.1 * free, "{damped} vs {free}");
    let fall = |damping: f64| {
        let g = square(damping);
        let mut s = g.initial_state.clone();
        let mut solver = VbdSolver::new(g, Some(&far), &s).unwrap();
        let settings = VbdSettings {
            dt: 1. / 480.,
            iterations: 16,
            damping: 0.,
            ..Default::default()
        };
        for _ in 0..60 {
            gpu.step(&mut solver, &mut s, &settings, None).unwrap();
        }
        s.v.iter().map(|v| v.y).sum::<f64>() / s.v.len() as f64
    };
    let (free, damped) = (fall(0.), fall(0.02));
    assert!((damped - free).abs() < 1e-3, "{damped} vs {free}");
}
