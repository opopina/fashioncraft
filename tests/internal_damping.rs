//! Internal damping per fabric: the force −(k_d/dt)(∇E(x) − ∇E(xᵗ)) resists stretching and
//! bending motion but no rigid motion. A stretched, weightless square released in still air
//! rings on without it and settles with it; a falling square falls exactly as fast either way.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate, template::FabricProperties},
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

fn far_away() -> SurfaceMesh {
    SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    }
}

/// An upright 12 cm square of a fabric with the given internal damping (s).
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
    p.pattern = marvelous_cloth::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 3,
        ny: 3,
    };
    p.placement.translation = DVec3::new(-0.06, 1., 0.);
    Arc::new(t.compile().unwrap())
}

fn kinetic(g: &CompiledGarment, s: &State) -> f64 {
    s.v.iter()
        .zip(&g.mesh.masses)
        .map(|(v, m)| 0.5 * m * v.length_squared())
        .sum()
}

fn mean_v(s: &State) -> DVec3 {
    s.v.iter().sum::<DVec3>() / s.v.len() as f64
}

/// Release the square stretched by 8 % in its plane, weightless, for `steps` of 1/240 s.
/// Returns the kinetic energy at the end and the largest mean (whole-piece) speed seen.
fn ring(damping: f64, steps: usize) -> (f64, f64) {
    let g = square(damping);
    let mut s = g.initial_state.clone();
    let centre = s.x.iter().sum::<DVec3>() / s.x.len() as f64;
    for p in &mut s.x {
        let d = *p - centre;
        *p = centre + DVec3::new(d.x * 1.08, d.y * 1.08, d.z);
    }
    let mut solver = VbdSolver::new(g.clone(), Some(&far_away()), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 0.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    let mut drift: f64 = 0.;
    for _ in 0..steps {
        solver.step(&mut s, &settings, None).unwrap();
        drift = drift.max(mean_v(&s).length());
    }
    (kinetic(&g, &s), drift)
}

#[test]
fn internal_damping_settles_a_stretched_square() {
    // Over 0.3 s: the undamped square still rings; 10 ms of internal damping settles it.
    let (free, free_drift) = ring(0., 72);
    let (damped, damped_drift) = ring(0.01, 72);
    assert!(free > 1e-7, "rings on without damping: {free}");
    assert!(damped < 0.05 * free, "{damped} vs {free}");
    // Block descent does not conserve momentum exactly, so both drift a little as a whole;
    // damping does not add to it.
    assert!(free_drift < 0.02, "{free_drift}");
    assert!(
        damped_drift < 2. * free_drift,
        "{damped_drift} vs {free_drift}"
    );
}

#[test]
fn internal_damping_leaves_a_fall_alone() {
    let fall = |damping: f64| {
        let g = square(damping);
        let mut s = g.initial_state.clone();
        let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
        let settings = VbdSettings {
            dt: 1. / 480.,
            iterations: 16,
            damping: 0.,
            ..Default::default()
        };
        for _ in 0..60 {
            solver.step(&mut s, &settings, None).unwrap();
        }
        mean_v(&s)
    };
    let (free, damped) = (fall(0.), fall(0.02));
    assert!((free.y + 9.81 * 0.125).abs() < 0.01, "{free}");
    assert!((damped - free).length() < 1e-6, "{damped} vs {free}");
}

#[test]
fn internal_damping_is_validated_and_left_out_when_zero() {
    let mut t = GarmentTemplate::two_panel_demo();
    t.fabrics[0].properties.internal_damping = 0.2;
    assert!(t.compile().is_err(), "at most 100 ms");
    t.fabrics[0].properties.internal_damping = -0.001;
    assert!(t.compile().is_err());
    let json = serde_json::to_string(&FabricProperties::default()).unwrap();
    assert!(!json.contains("internal_damping"));
}
