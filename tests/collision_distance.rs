//! Collision distance per fabric: the gap the solver keeps between a fabric and anything else.
//! A contact keeps the mean of its two sides' distances; the avatar and the floor use the
//! scene's. Cloth resting under gravity sits just inside that gap, where the barrier carries
//! its weight, so the resting height tells which radius a contact used.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    garment::{
        CompiledGarment, GarmentTemplate,
        template::{Fabric, FabricProperties},
    },
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

const SCENE: f64 = 0.003;

fn floor() -> SurfaceMesh {
    SurfaceMesh {
        vertices: vec![
            DVec3::new(-40., 0., -20.),
            DVec3::new(40., 0., -20.),
            DVec3::new(0., 0., 40.),
        ],
        triangles: vec![[0, 1, 2]],
    }
}

/// 12 cm squares lying flat, one per entry of `layers`: (height, the fabric's own collision
/// distance). Each square has its own fabric.
fn squares(layers: &[(f64, Option<f64>)]) -> Arc<CompiledGarment> {
    let mut t = GarmentTemplate::two_panel_demo();
    t.seams.clear();
    let base = t.panels[0].clone();
    t.panels.clear();
    for (k, (height, gap)) in layers.iter().enumerate() {
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
        p.placement.translation = DVec3::new(-0.06, *height, 0.06);
        p.fabric = format!("fabric{k}");
        t.fabrics.push(Fabric {
            id: p.fabric.clone(),
            name: p.fabric.clone(),
            properties: FabricProperties {
                collision_distance: *gap,
                ..Default::default()
            },
        });
        t.panels.push(p);
    }
    Arc::new(t.compile().unwrap())
}

fn settings() -> VbdSettings {
    VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        ..Default::default()
    }
}

/// Settle for a second and return the mean height of each square.
fn settled(g: Arc<CompiledGarment>, squares: usize) -> Vec<f64> {
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&floor()), &s).unwrap();
    let settings = settings();
    for _ in 0..240 {
        solver.step(&mut s, &settings, None).unwrap();
        assert!(s.x.iter().all(|p| p.y > 0.), "never through the floor");
    }
    let per = s.x.len() / squares;
    (0..squares)
        .map(|k| s.x[k * per..(k + 1) * per].iter().map(|p| p.y).sum::<f64>() / per as f64)
        .collect()
}

#[test]
fn a_fabric_rests_at_the_mean_of_its_and_the_floors_distance() {
    assert_eq!(settings().contact.radius, SCENE);
    let plain = settled(squares(&[(0.004, None)]), 1)[0];
    let thick = settled(squares(&[(0.008, Some(0.009))]), 1)[0];
    // Just inside the radius: the scene's 3 mm, then (9 + 3) / 2 = 6 mm.
    assert!(plain > 0.8 * SCENE && plain < SCENE, "{plain}");
    let mean = 0.5 * (0.009 + SCENE);
    assert!(thick > 0.8 * mean && thick < mean, "{thick}");
}

#[test]
fn stacked_fabrics_keep_the_mean_of_their_distances() {
    // A thin-gapped square on the floor and a wide-gapped one on top: between them,
    // (2 + 10) / 2 = 6 mm.
    let h = settled(squares(&[(0.004, Some(0.002)), (0.014, Some(0.010))]), 2);
    let lower_on_floor = 0.5 * (0.002 + SCENE);
    let between = 0.5 * (0.002 + 0.010);
    assert!(
        h[0] > 0.7 * lower_on_floor && h[0] < lower_on_floor,
        "{h:?}"
    );
    let gap = h[1] - h[0];
    assert!(gap > 0.8 * between && gap < between, "{h:?}");
}

#[test]
fn without_fabric_distances_every_contact_shares_the_scene_radius() {
    let g = squares(&[(0.002, None)]);
    let s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&floor()), &s).unwrap();
    let x = solver.world.positions(&s.x);
    let contacts = solver
        .world
        .detect(&x, 2. * SCENE, &settings().contact)
        .unwrap();
    assert!(!contacts.contacts.is_empty());
    assert!(contacts.contact_radius.is_empty(), "the shared-radius path");
    assert_eq!(contacts.radius(0, SCENE), SCENE);
}

#[test]
fn collision_distances_out_of_range_are_rejected() {
    for gap in [0.0001, 0.05, f64::NAN] {
        let mut t = GarmentTemplate::two_panel_demo();
        t.fabrics[0].properties.collision_distance = Some(gap);
        assert!(t.compile().is_err(), "{gap}");
    }
    let json = serde_json::to_string(&FabricProperties::default()).unwrap();
    assert!(!json.contains("collision"), "old documents stay unchanged");
}
