//! Wind as pressure drag on the cloth's triangles, checked against closed-form motion of a
//! flat sheet: F = ½ ρ C_d A (r·n)|r·n| n with r the air speed relative to the cloth. Air
//! damping is off so the drag is the only force besides gravity. The solver moves a vertex at
//! most a fraction of its contact query radius per detection, so the steps are small enough
//! for the speeds here to stay well inside that bound.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    garment::{CompiledGarment, GarmentTemplate},
    model::State,
    vbd::{VbdSettings, VbdSolver, Wind},
};
use std::sync::Arc;

const G: f64 = 9.81;
const DT: f64 = 1. / 480.;

/// A free 12 cm square, rotated by `rotation` degrees (0: upright, facing ±Z; 90 about X:
/// lying flat) and placed well away from anything.
fn patch(rotation: DVec3) -> Arc<CompiledGarment> {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pattern = marvelous_cloth::model::Pattern {
        width: 0.12,
        height: 0.12,
        nx: 3,
        ny: 3,
    };
    p.placement.rotation_degrees = rotation;
    p.placement.translation = DVec3::new(-0.06, 1., 0.06);
    Arc::new(t.compile().unwrap())
}

/// A small obstacle far from the cloth.
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

/// Mass per unit area of the cloth, kg/m².
fn areal_density(g: &CompiledGarment) -> f64 {
    g.mesh.masses.iter().sum::<f64>() / g.mesh.rest_area.iter().sum::<f64>()
}

fn still_air() -> Wind {
    Wind {
        velocity: DVec3::ZERO,
        ..Default::default()
    }
}

fn settings(wind: Option<Wind>, gravity: DVec3) -> VbdSettings {
    VbdSettings {
        dt: DT,
        iterations: 16,
        damping: 0.,
        gravity,
        wind,
        ..Default::default()
    }
}

fn mean_v(s: &State) -> DVec3 {
    s.v.iter().sum::<DVec3>() / s.v.len() as f64
}

/// Run until each of `times` (s) and return the mean velocity there.
fn velocities(g: Arc<CompiledGarment>, settings: &VbdSettings, times: &[f64]) -> Vec<DVec3> {
    let mut s = g.initial_state.clone();
    let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
    let mut out = vec![];
    let mut step = 0;
    for t in times {
        while (step as f64) * DT < t - 1e-9 {
            solver.step(&mut s, settings, None).unwrap();
            step += 1;
        }
        out.push(mean_v(&s));
    }
    out
}

#[test]
fn flat_falling_cloth_reaches_its_terminal_velocity() {
    // m dv/dt = m g − ½ ρ C_d A v²: v(t) = v_t tanh(g t / v_t), v_t = √(2 (m/A) g / (ρ C_d)).
    let g = patch(DVec3::new(90., 0., 0.));
    let air = still_air();
    let terminal = (2. * areal_density(&g) * G / (air.density * air.drag)).sqrt();
    let times = [0.1, 0.25, 1.0];
    let got = velocities(g, &settings(Some(air), DVec3::new(0., -G, 0.)), &times);
    for (t, v) in times.iter().zip(&got) {
        let expected = terminal * (G * t / terminal).tanh();
        assert!(
            (-v.y - expected).abs() / expected < 0.02,
            "t {t}: {} vs {expected}",
            -v.y
        );
        assert!(
            v.x.abs() + v.z.abs() < 1e-3 * v.y.abs(),
            "falls straight down: {v}"
        );
    }
    // A long way below free fall (9.81 m/s after a second).
    assert!(-got[2].y < 0.3 * G);
}

#[test]
fn edge_on_cloth_falls_freely_through_still_air() {
    // Upright: the fall is along the sheet, so no air pushes on it, while the same sheet
    // lying flat is held back to its terminal velocity.
    let air = Some(still_air());
    let gravity = DVec3::new(0., -G, 0.);
    let upright = velocities(patch(DVec3::ZERO), &settings(air.clone(), gravity), &[0.25]);
    let flat = velocities(
        patch(DVec3::new(90., 0., 0.)),
        &settings(air, gravity),
        &[0.25],
    );
    let free = G * 0.25;
    assert!(
        (-upright[0].y - free).abs() / free < 2e-3,
        "{}",
        upright[0].y
    );
    assert!(-flat[0].y < 0.7 * free, "{}", flat[0].y);
}

#[test]
fn a_free_sheet_is_carried_toward_the_wind_speed() {
    // Weightless, broadside to a 3 m/s wind along −Z: (m/A) dv/dt = k (V − v)², k = ½ ρ C_d,
    // so v(t) = V − V / (1 + k V t / (m/A)).
    let g = patch(DVec3::ZERO);
    let wind = Wind {
        velocity: DVec3::new(0., 0., -3.),
        ..Default::default()
    };
    let rate = 0.5 * wind.density * wind.drag * 3. / areal_density(&g);
    let times = [0.1, 0.5, 1.0];
    let got = velocities(g, &settings(Some(wind), DVec3::ZERO), &times);
    for (t, v) in times.iter().zip(&got) {
        let expected = 3. - 3. / (1. + rate * t);
        assert!(
            (-v.z - expected).abs() / expected < 0.02,
            "t {t}: {} vs {expected}",
            -v.z
        );
    }
    // Never faster than the wind.
    assert!(-got[2].z < 3.);
}

#[test]
fn hanging_cloth_blows_downwind_and_hangs_straight_without_wind() {
    // The demo front panel, held by its two top corners.
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let g = Arc::new(t.compile().unwrap());
    let drift = |wind: Option<Wind>| {
        let mut s = g.initial_state.clone();
        let start = s.x.clone();
        let mut solver = VbdSolver::new(g.clone(), Some(&far_away()), &s).unwrap();
        let settings = VbdSettings {
            wind,
            ..Default::default()
        };
        for _ in 0..60 {
            solver.step(&mut s, &settings, None).unwrap();
        }
        s.x.iter().zip(&start).map(|(a, b)| a.z - b.z).sum::<f64>() / s.x.len() as f64
    };
    let breeze = |z| Wind {
        velocity: DVec3::new(0., 0., z),
        ..Default::default()
    };
    let calm = drift(None);
    let back = drift(Some(breeze(-4.)));
    let front = drift(Some(breeze(4.)));
    assert!(calm.abs() < 1e-4, "{calm}");
    assert!(back < -0.01 && front > 0.01, "{back} {front}");
    assert!(
        (back + front).abs() < 0.1 * back.abs(),
        "mirror winds, mirror drift: {back} {front}"
    );
}

#[test]
fn wind_settings_round_trip_and_are_left_out_when_off() {
    let calm = VbdSettings::default();
    let json = serde_json::to_string(&calm).unwrap();
    assert!(!json.contains("wind"), "old documents stay unchanged");
    let windy = VbdSettings {
        wind: Some(Wind::default()),
        ..Default::default()
    };
    windy.validate().unwrap();
    let back: VbdSettings = serde_json::from_str(&serde_json::to_string(&windy).unwrap()).unwrap();
    assert_eq!(back.wind, windy.wind);
    let gale = VbdSettings {
        wind: Some(Wind {
            velocity: DVec3::new(60., 0., 0.),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(gale.validate().is_err());
}
