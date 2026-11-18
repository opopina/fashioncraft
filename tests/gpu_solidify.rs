//! Optional GPU check of Solidify: a square held at its centre stays flat on the GPU solver,
//! as in tests/strengthen_solidify.rs. Ordinary test runs must not initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use glam::{DVec2, DVec3};
use marvelous_cloth::{
    authoring,
    avatar::SurfaceMesh,
    garment::{GarmentTemplate, pins::MaterialPinTarget},
    gpu_vbd::GpuVbd,
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_solidified_square_stays_flat() {
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
        width: 0.24,
        height: 0.24,
        nx: 6,
        ny: 6,
    };
    p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
    p.placement.translation = DVec3::new(-0.12, 1., 0.12);
    let g = t.clone().compile().unwrap();
    let centre = g.mesh.uv.iter().sum::<DVec2>() / g.mesh.uv.len() as f64;
    let i = (0..g.mesh.uv.len())
        .min_by(|a, b| {
            g.mesh.uv[*a]
                .distance(centre)
                .total_cmp(&g.mesh.uv[*b].distance(centre))
        })
        .unwrap();
    t.pin_targets = vec![MaterialPinTarget {
        panel: t.panels[0].id.clone(),
        uv: g.mesh.uv[i],
        world: g.initial_state.x[i],
    }];
    t.version = 2;
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 2.,
        ..Default::default()
    };
    let drop = |t: &GarmentTemplate, state: Option<State>| {
        let g = Arc::new(t.clone().compile().unwrap());
        let mut s = state.unwrap_or_else(|| g.initial_state.clone());
        let mut solver = VbdSolver::new(g, Some(&far), &s).unwrap();
        for _ in 0..240 {
            gpu.step(&mut solver, &mut s, &settings, None).unwrap();
        }
        1. - s.x.iter().map(|p| p.y).fold(f64::INFINITY, f64::min)
    };
    let free = drop(&t, None);
    let g = t.clone().compile().unwrap();
    let (edit, next) =
        authoring::prepare_solidify_edit(&t, &g, &g.initial_state, &[t.panels[0].id.clone()], true)
            .unwrap();
    let solid = drop(&edit.template, Some(next));
    assert!(free > 0.05 && solid < 0.3 * free, "{solid} vs {free}");
}
