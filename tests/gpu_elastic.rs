//! Optional GPU check of elastic: the new tension-only term (kind 4) draws a free edge in on
//! the GPU solver as on the CPU, with its forces checked against the f64 terms every step.
//! Ordinary test runs must not initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{GarmentTemplate, seam::Side, template::LineSpring},
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_elastic_draws_a_free_edge_in() {
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
    p.pattern = fashioncraft::model::Pattern {
        width: 0.2,
        height: 0.2,
        nx: 8,
        ny: 8,
    };
    p.placement.translation = DVec3::new(-0.1, 1., 0.);
    let id = t.panels[0].id.clone();
    let spring = LineSpring {
        ratio: 70.,
        strength: 200.,
        ..LineSpring::elastic(&id, "top")
    };
    let t = authoring::apply(
        &t,
        Command::SetLineSpring {
            panel: id,
            line: "top".into(),
            spring: Some(spring),
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..360 {
        gpu.step(&mut solver, &mut s, &settings, None).unwrap();
    }
    let chain = t.panels[0].boundary_vertices(Side::Top, 0);
    let ends = |x: &[DVec3]| x[chain[0]].distance(x[*chain.last().unwrap()]);
    let top = ends(&s.x) / ends(&g.initial_state.x);
    assert!(top < 0.8, "the elastic edge is drawn in: {top}");
}
