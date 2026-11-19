//! Coulomb friction, checked against the analytic block on an inclined plane. The plane is
//! the horizontal floor and gravity is tilted by θ instead, which is the same problem: a
//! patch resting on it accelerates at g (sin θ − μ cos θ) when μ < tan θ and stays put
//! otherwise. Air damping is off so the analytic motion applies unchanged, and the step is
//! small with enough iterations for the local solves to converge: the comparison is with
//! the physics, not with an unconverged iterate.
use fashioncraft::{
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate},
    model::State,
    vbd::{VbdSettings, VbdSolver, friction},
};
use glam::DVec3;
use std::sync::Arc;

const G: f64 = 9.81;
const DT: f64 = 1. / 240.;

/// A 12 cm patch lying flat just above a large one-triangle floor at y = 0.
fn resting_patch() -> (Arc<CompiledGarment>, SurfaceMesh) {
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
    p.placement.translation = DVec3::new(-0.06, 0.00305, 0.06);
    let floor = SurfaceMesh {
        vertices: vec![
            DVec3::new(-40., 0., -20.),
            DVec3::new(40., 0., -20.),
            DVec3::new(0., 0., 40.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    (Arc::new(t.compile().unwrap()), floor)
}

fn settings(mu: f64, gravity: DVec3) -> VbdSettings {
    VbdSettings {
        dt: DT,
        iterations: 64,
        damping: 0.,
        friction: mu,
        gravity,
        ..Default::default()
    }
}

fn mean_vx(s: &State) -> f64 {
    s.v.iter().map(|v| v.x).sum::<f64>() / s.v.len() as f64
}

/// Settle on the floor under vertical gravity, then tilt gravity by 20° and measure the
/// patch's mean acceleration along the slope between 0.05 s and 0.2 s.
fn slope_acceleration(mu: f64) -> (f64, State) {
    let (g, floor) = resting_patch();
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&floor), &s).unwrap();
    let rest = settings(mu, DVec3::new(0., -G, 0.));
    for _ in 0..60 {
        solver.step(&mut s, &rest, None).unwrap();
    }
    s.v.fill(DVec3::ZERO);
    let a = 20f64.to_radians();
    let tilted = settings(mu, DVec3::new(G * a.sin(), -G * a.cos(), 0.));
    let mut samples = vec![];
    for i in 0..48 {
        solver.step(&mut s, &tilted, None).unwrap();
        assert!(s.x.iter().all(|p| p.y > 0.), "never through the floor");
        if i % 12 == 11 {
            samples.push(mean_vx(&s));
        }
    }
    ((samples[3] - samples[0]) / (36. * DT), s)
}

fn expected(mu: f64) -> f64 {
    let a = 20f64.to_radians();
    (G * (a.sin() - mu * a.cos())).max(0.)
}

#[test]
fn frictionless_patches_slide_with_the_full_slope_acceleration() {
    let (acc, _) = slope_acceleration(0.);
    assert!((acc - expected(0.)).abs() / expected(0.) < 0.03, "{acc}");
}

#[test]
fn sliding_friction_takes_off_mu_g_cos_theta() {
    for mu in [0.15, 0.3] {
        let (acc, _) = slope_acceleration(mu);
        assert!(
            (acc - expected(mu)).abs() / expected(mu) < 0.03,
            "μ {mu}: {acc} vs {}",
            expected(mu)
        );
    }
}

#[test]
fn static_friction_holds_the_patch_on_a_gentle_slope() {
    // tan 20° ≈ 0.36 < μ = 0.6: it sticks, creeping at most at the stick speed.
    let (acc, s) = slope_acceleration(0.6);
    assert!(acc.abs() < 0.01, "{acc}");
    assert!(
        s.v.iter()
            .all(|v| v.length() < 2. * friction::STICK_SPEED_M_S),
        "sticks"
    );
}

#[test]
fn friction_terms_carry_the_normal_force() {
    let (g, floor) = resting_patch();
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&floor), &s).unwrap();
    let rest = settings(0.5, DVec3::new(0., -G, 0.));
    for _ in 0..240 {
        solver.step(&mut s, &rest, None).unwrap();
    }
    let x = solver.world.positions(&s.x);
    let contacts = solver
        .world
        .detect(&x, 2. * rest.contact.radius, &rest.contact)
        .unwrap();
    let terms = friction::friction_terms(&contacts, &x, &x, &rest);
    let active: Vec<_> = terms.iter().flatten().collect();
    assert!(!active.is_empty());
    // At rest the normal forces carry the weight: Σ μ λ = μ m g.
    let total: f64 = active.iter().map(|t| t.mu_lambda).sum();
    let weight: f64 = solver.garment.mesh.masses.iter().sum::<f64>() * G;
    assert!(
        (total - 0.5 * weight).abs() / (0.5 * weight) < 0.02,
        "{total} vs {}",
        0.5 * weight
    );
    for t in &active {
        assert!((t.normal - DVec3::Y).length() < 1e-6, "pushes straight up");
        // No slip yet: f0(0) = ε/3 and no force.
        let e = t.energy(|i| x[i].to_array());
        assert!((e - t.mu_lambda * t.eps / 3.).abs() < 1e-15);
    }
    assert!(
        friction::friction_terms(&contacts, &x, &x, &settings(0., DVec3::ZERO)).is_empty(),
        "no friction, no terms"
    );
}

#[test]
fn the_friction_force_is_coulomb_once_sliding() {
    // Gradient of μ λ f0(|P u|) by autodiff, against the closed form μ λ P u / |P u|.
    use fashioncraft::math::Jet;
    let t = friction::FrictionTerm {
        kind: fashioncraft::ogc::potential::Kind::PointFace,
        vertices: [0, 1, 2, 3],
        count: 1,
        weights: [1., 0., 0., 0.],
        normal: DVec3::Y,
        mu_lambda: 0.2,
        anchor: DVec3::ZERO,
        eps: 1e-5,
    };
    for p in [
        DVec3::new(0.003, 0.001, -0.002),
        DVec3::new(2e-6, 0.004, 1e-6),
    ] {
        let j = t.energy(|_| std::array::from_fn(|k| Jet::<3>::variable(p[k], k)));
        let slip = DVec3::new(p.x, 0., p.z);
        let expected = if slip.length() >= t.eps {
            slip / slip.length() * 0.2
        } else {
            let y = slip.length();
            slip * (0.2 * (2. / t.eps - y / (t.eps * t.eps)))
        };
        let got = DVec3::from_array(j.g);
        assert!((got - expected).length() < 1e-9, "{got} vs {expected}");
        assert!(got.y.abs() < 1e-12, "no normal force from friction");
        assert!(j.v > 0.);
    }
}
