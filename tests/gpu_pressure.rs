//! Optional GPU check of pressure: a free weightless sheet accelerates at p/ρ_A along its
//! normal on the GPU solver, as in tests/pressure.rs. Ordinary test runs must not initialize
//! a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::GarmentTemplate,
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_pressure_pushes_a_sheet_at_p_over_areal_density() {
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
        nx: 4,
        ny: 4,
    };
    p.placement.translation = DVec3::new(-0.06, 1., 0.);
    let id = t.panels[0].id.clone();
    let t = authoring::apply(
        &t,
        Command::SetPressure {
            panels: vec![id],
            pressure: 1.,
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
    let [a, b, c] = g.mesh.triangles[0].vertices;
    let x = &g.initial_state.x;
    let n = (x[b] - x[a]).cross(x[c] - x[a]).normalize();
    let density = g.mesh.masses.iter().sum::<f64>() / g.mesh.rest_area.iter().sum::<f64>();
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 480.,
        iterations: 32,
        damping: 0.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..48 {
        gpu.step(&mut solver, &mut s, &settings, None).unwrap();
    }
    let v = s.v.iter().sum::<DVec3>() / s.v.len() as f64;
    let expected = 0.1 / density;
    assert!(
        (v.dot(n) - expected).abs() / expected < 0.03,
        "{v} vs {expected}"
    );
}
