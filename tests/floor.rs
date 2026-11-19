//! A physical floor: dropped cloth comes to rest on it, inside the contact shell, without
//! passing through; without a floor it keeps falling. Gravity and the floor are settings.
use fashioncraft::{
    avatar::SurfaceMesh,
    garment::GarmentTemplate,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::sync::Arc;

/// A 12 cm square of cloth lying flat, `height` above y = 0.
fn patch(height: f64) -> Arc<fashioncraft::garment::CompiledGarment> {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pattern = fashioncraft::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 3,
        ny: 3,
    };
    p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
    p.placement.translation = DVec3::new(-0.06, height, 0.06);
    Arc::new(t.compile().unwrap())
}

/// A small obstacle away from the cloth, standing in for an avatar; the floor is centred
/// under it and reaches well past the cloth.
fn far_away() -> SurfaceMesh {
    SurfaceMesh {
        vertices: vec![
            DVec3::new(5., 0.5, 5.),
            DVec3::new(5.1, 0.5, 5.),
            DVec3::new(5., 0.6, 5.),
        ],
        triangles: vec![[0, 1, 2]],
    }
}

#[test]
fn the_floor_is_one_large_triangle_under_the_surface() {
    let floored = far_away().with_floor(0.02);
    assert_eq!(floored.vertices.len(), 6);
    assert_eq!(floored.triangles.len(), 2);
    floored.validate().unwrap();
    let centre = DVec3::new(5.05, 0.02, 5.);
    let corners: Vec<DVec3> = floored.vertices[3..].to_vec();
    for v in &corners {
        assert_eq!(v.y, 0.02);
    }
    // Every point within 20 m of the centre is inside: its distance to each side is ≥ 20 m.
    for k in 0..3 {
        let (a, b) = (corners[k], corners[(k + 1) % 3]);
        let side = (b - a).normalize();
        let to_centre = centre - a;
        let distance = (to_centre - side * to_centre.dot(side)).length();
        assert!(distance >= 20. - 1e-9, "{distance}");
    }
}

#[test]
fn dropped_cloth_rests_on_the_floor_and_falls_without_it() {
    let settings = VbdSettings {
        dt: 1. / 120.,
        floor: Some(0.),
        ..Default::default()
    };
    settings.validate().unwrap();
    let g = patch(0.03);
    let floored = far_away().with_floor(0.);
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g.clone(), Some(&floored), &s).unwrap();
    for _ in 0..120 {
        solver.step(&mut s, &settings, None).unwrap();
        assert!(s.x.iter().all(|p| p.y > 0.), "never through the floor");
    }
    // One second later: lying on the floor inside the contact shell, at rest.
    for p in &s.x {
        assert!(
            p.y > 0. && p.y < settings.contact.radius + 1e-3,
            "rests on the floor: {}",
            p.y
        );
    }
    // Vertically at rest. Sideways it may still glide: the solver has no friction yet.
    let vertical = s.v.iter().map(|v| v.y.abs()).fold(0., f64::max);
    let sideways =
        s.v.iter()
            .map(|v| DVec3::new(v.x, 0., v.z).length())
            .fold(0., f64::max);
    assert!(
        vertical < 0.02,
        "settled on the floor: {vertical} m/s, sideways {sideways}"
    );

    // The same drop with no floor keeps going.
    let mut free = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&far_away()), &free).unwrap();
    for _ in 0..120 {
        solver.step(&mut free, &settings, None).unwrap();
    }
    assert!(free.x.iter().all(|p| p.y < -0.5), "falls past y = 0");
}

#[test]
fn zero_gravity_leaves_still_cloth_still() {
    let settings = VbdSettings {
        dt: 1. / 120.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    let g = patch(0.5);
    let mut s = g.initial_state.clone();
    let start = s.x.clone();
    let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
    for _ in 0..30 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    for (a, b) in s.x.iter().zip(&start) {
        assert!(a.distance(*b) < 1e-9, "rest state is preserved");
    }
}
