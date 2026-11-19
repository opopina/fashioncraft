//! Optional GPU check of folds: the folded-rest bending term's analytic GPU gradient against
//! the f64 autodiff every step, and the 90° crease of tests/folds.rs on the GPU solver.
//! Ordinary test runs must not initialize a GPU.
#![cfg(feature = "desktop")]
#[path = "support/folds.rs"]
#[allow(dead_code)]
mod folds;
use eframe::wgpu;
use fashioncraft::{
    avatar::SurfaceMesh,
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_crease_folds_square_with_checked_gradients() {
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
    let g = Arc::new(folds::folded(&folds::creased(), 90.).compile().unwrap());
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    for (p, uv) in s.x.iter_mut().zip(&g.mesh.uv) {
        p.z += 0.002 * (uv.x - 0.2).abs();
    }
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for step in 0..480 {
        // Check the folded term's forces and energies against f64 for a while, then run free.
        gpu.check_gradients = step < 60;
        gpu.step(&mut solver, &mut s, &settings, None).unwrap();
    }
    let (l, r) = folds::halves(&g, &s);
    let angle = l.angle_between(r).to_degrees();
    assert!((angle - 90.).abs() < 12., "{angle}°");
}
