//! Optional GPU check of simulation layers: the outer sheet thrown at the inner one on the
//! GPU solver, as in tests/layers.rs. Ordinary test runs must not initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::GarmentTemplate,
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_inner_layer_keeps_still_when_the_outer_one_hits_it() {
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
    t.seams.clear();
    let base = t.panels[0].clone();
    t.panels.clear();
    for k in 0..2 {
        let mut p = base.clone();
        p.id = format!("square{k}");
        p.name = p.id.clone();
        p.pins.clear();
        p.pattern = fashioncraft::model::Pattern {
            width: 0.12,
            height: 0.12,
            nx: 3,
            ny: 3,
        };
        p.placement.rotation_degrees = DVec3::ZERO;
        p.placement.translation = DVec3::new(-0.06, 0.5, 0.01 * k as f64);
        t.panels.push(p);
    }
    let t = authoring::apply(
        &t,
        Command::SetLayer {
            panels: vec!["square1".into()],
            layer: 1,
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
    let mut s = g.initial_state.clone();
    let per = s.x.len() / 2;
    for v in &mut s.v[per..] {
        v.z = -0.5;
    }
    let start = s.x.clone();
    let floor = SurfaceMesh {
        vertices: vec![
            DVec3::new(-40., 0., -20.),
            DVec3::new(40., 0., -20.),
            DVec3::new(0., 0., 40.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut solver = VbdSolver::new(g, Some(&floor), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        gravity: DVec3::ZERO,
        damping: 0.,
        ..Default::default()
    };
    for _ in 0..48 {
        gpu.step(&mut solver, &mut s, &settings, None).unwrap();
    }
    for (i, (p, q)) in s.x[..per].iter().zip(&start).enumerate() {
        assert!(p.distance(*q) < 1e-6, "inner vertex {i} moved");
        assert!(p.z < s.x[i + per].z, "vertex {i} passed through");
    }
    let outer = s.v[per..].iter().map(|v| v.z).sum::<f64>() / per as f64;
    assert!(outer > -0.05, "the outer sheet is stopped: {outer}");
}
