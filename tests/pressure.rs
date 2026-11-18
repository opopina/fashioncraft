//! Pressure per piece: each triangle is pushed with p·A along its winding normal. A free,
//! weightless sheet accelerates at exactly p/ρ_A (ρ_A its mass per area) along its normal; a
//! sheet held by its corners bulges along its normal, the other way under negative pressure.
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate},
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

/// An upright 12 cm square (corners pinned when `held`) with `pressure` Pa.
fn sheet(held: bool, pressure: f64) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    if held {
        use marvelous_cloth::garment::template::Corner;
        p.pins = vec![
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomLeft,
            Corner::BottomRight,
        ];
    } else {
        p.pins.clear();
    }
    p.pattern = marvelous_cloth::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 4,
        ny: 4,
    };
    p.placement.translation = DVec3::new(-0.06, 1., 0.);
    if pressure == 0. {
        return t;
    }
    let id = t.panels[0].id.clone();
    authoring::apply(
        &t,
        Command::SetPressure {
            panels: vec![id],
            pressure,
        },
    )
    .unwrap()
    .template
}

/// The sheet's winding normal (all its triangles face the same way).
fn normal(g: &CompiledGarment) -> DVec3 {
    let t = &g.mesh.triangles[0];
    let x = &g.initial_state.x;
    let [a, b, c] = t.vertices;
    (x[b] - x[a]).cross(x[c] - x[a]).normalize()
}

fn settings(dt: f64) -> VbdSettings {
    VbdSettings {
        dt,
        iterations: 32,
        damping: 0.,
        gravity: DVec3::ZERO,
        ..Default::default()
    }
}

#[test]
fn a_free_sheet_accelerates_at_pressure_over_areal_density() {
    let g = Arc::new(sheet(false, 1.).compile().unwrap());
    let n = normal(&g);
    let density = g.mesh.masses.iter().sum::<f64>() / g.mesh.rest_area.iter().sum::<f64>();
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
    for _ in 0..48 {
        solver.step(&mut s, &settings(1. / 480.), None).unwrap();
    }
    let v = s.v.iter().sum::<DVec3>() / s.v.len() as f64;
    let expected = 1. * 0.1 / density;
    assert!(
        (v.dot(n) - expected).abs() / expected < 0.01,
        "{v} vs {expected}"
    );
    assert!(
        (v - n * v.dot(n)).length() < 1e-3 * expected,
        "only along the normal"
    );
}

#[test]
fn a_held_sheet_bulges_along_its_normal_and_back_under_suction() {
    let bulge = |pressure: f64| {
        let g = Arc::new(sheet(true, pressure).compile().unwrap());
        let n = normal(&g);
        let start = g.initial_state.x.clone();
        let mut s = g.initial_state.clone();
        let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
        let settings = VbdSettings {
            damping: 2.,
            ..settings(1. / 240.)
        };
        for _ in 0..240 {
            solver.step(&mut s, &settings, None).unwrap();
        }
        s.x.iter()
            .zip(&start)
            .map(|(p, q)| (*p - *q).dot(n))
            .fold(0., |m: f64, d| if d.abs() > m.abs() { d } else { m })
    };
    let (out, flat, inward) = (bulge(20.), bulge(0.), bulge(-20.));
    assert!(out > 0.005, "{out}");
    assert!(inward < -0.005, "{inward}");
    assert!(
        (out + inward).abs() < 0.1 * out,
        "mirror images: {out} {inward}"
    );
    assert!(flat.abs() < 1e-9, "no pressure, no bulge: {flat}");
}

#[test]
fn pressure_is_validated_and_cleared_with_zero() {
    let t = sheet(false, 5.);
    let id = t.panels[0].id.clone();
    assert_eq!(t.panel_pressure.get(&id), Some(&5.));
    let cleared = authoring::apply(
        &t,
        Command::SetPressure {
            panels: vec![id.clone()],
            pressure: 0.,
        },
    )
    .unwrap()
    .template;
    assert!(cleared.panel_pressure.is_empty());
    assert!(cleared.compile().unwrap().pressure.is_empty());
    for pressure in [5000., f64::NAN] {
        assert!(
            authoring::apply(
                &t,
                Command::SetPressure {
                    panels: vec![id.clone()],
                    pressure
                }
            )
            .is_err()
        );
    }
}
