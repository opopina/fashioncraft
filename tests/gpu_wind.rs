//! Optional GPU check of wind: the closed-form motion of a weightless sheet blown along, as
//! in tests/wind.rs, on the GPU solver. Ordinary test runs must not initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    garment::GarmentTemplate,
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver, Wind},
};
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_wind_carries_a_sheet_as_the_closed_form() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = GpuVbd::new(device, queue);
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
    p.placement.translation = DVec3::new(-0.06, 1., 0.06);
    let g = Arc::new(t.compile().unwrap());
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mean = |s: &marvelous_cloth::model::State| s.v.iter().sum::<DVec3>() / s.v.len() as f64;

    // Weightless, broadside to a 1 m/s wind: v(t) = V − V / (1 + k V t / (m/A)). The GPU
    // moves a vertex within one detection's bounds per step: small steps and a light breeze
    // keep the sheet well inside them.
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let wind = Wind {
        velocity: DVec3::new(0., 0., -1.),
        ..Default::default()
    };
    let density = g.mesh.masses.iter().sum::<f64>() / g.mesh.rest_area.iter().sum::<f64>();
    let rate = 0.5 * wind.density * wind.drag * 1. / density;
    let blow = VbdSettings {
        dt: 1. / 480.,
        damping: 0.,
        gravity: DVec3::ZERO,
        wind: Some(wind),
        ..Default::default()
    };
    for _ in 0..120 {
        gpu.step(&mut solver, &mut s, &blow, None).unwrap();
    }
    let expected = 1. - 1. / (1. + rate * 0.25);
    let v = -mean(&s).z;
    // f32 terms: a looser bound than the f64 CPU test.
    assert!(
        (v - expected).abs() / expected < 0.04,
        "wind {v} vs {expected}"
    );
}
