//! Optional GPU check of per-fabric collision distances: stacked squares settle with the
//! gaps of tests/collision_distance.rs on the GPU solver. Ordinary test runs must not
//! initialize a GPU.
#![cfg(feature = "desktop")]
use eframe::wgpu;
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    garment::{
        GarmentTemplate,
        template::{Fabric, FabricProperties},
    },
    gpu_vbd::GpuVbd,
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

#[test]
#[ignore = "requires an explicitly authorized hardware GPU"]
fn gpu_stacked_fabrics_keep_the_mean_of_their_distances() {
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
    for (k, (height, gap)) in [(0.004, 0.002), (0.014, 0.010)].into_iter().enumerate() {
        let mut p = base.clone();
        p.id = format!("square{k}");
        p.name = p.id.clone();
        p.pins.clear();
        p.pattern = marvelous_cloth::model::Pattern {
            width: 0.12,
            height: 0.12,
            nx: 3,
            ny: 3,
        };
        p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
        p.placement.translation = DVec3::new(-0.06, height, 0.06);
        p.fabric = format!("fabric{k}");
        t.fabrics.push(Fabric {
            id: p.fabric.clone(),
            name: p.fabric.clone(),
            properties: FabricProperties {
                collision_distance: Some(gap),
                ..Default::default()
            },
        });
        t.panels.push(p);
    }
    let g = Arc::new(t.compile().unwrap());
    let floor = SurfaceMesh {
        vertices: vec![
            DVec3::new(-40., 0., -20.),
            DVec3::new(40., 0., -20.),
            DVec3::new(0., 0., 40.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&floor), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        ..Default::default()
    };
    for _ in 0..240 {
        gpu.step(&mut solver, &mut s, &settings, None).unwrap();
    }
    let per = s.x.len() / 2;
    let h: Vec<f64> = (0..2)
        .map(|k| s.x[k * per..(k + 1) * per].iter().map(|p| p.y).sum::<f64>() / per as f64)
        .collect();
    let lower_on_floor = 0.5 * (0.002 + settings.contact.radius);
    let between = 0.5 * (0.002 + 0.010);
    assert!(
        h[0] > 0.7 * lower_on_floor && h[0] < lower_on_floor,
        "{h:?}"
    );
    let gap = h[1] - h[0];
    assert!(gap > 0.8 * between && gap < between, "{h:?}");
}
