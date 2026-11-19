//! Split steps: a step whose motion its contacts cannot allow at once (about 0.3 m/s at
//! 1/60 s) is split into shorter whole steps, so free cloth falls as fast as gravity takes
//! it while contacts and sewing stay as safe as ever. Off unless asked for.
use fashioncraft::{
    authoring::{self, Command},
    fitting::FittingProject,
    garment::{
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::{path::PathBuf, sync::Arc};

/// A 30 cm square of cloth, flat and level, its centre at `height`.
fn square(
    height: f64,
) -> (
    Arc<fashioncraft::garment::CompiledGarment>,
    fashioncraft::model::State,
) {
    let t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.3, 0.3),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.03,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
    let mut state = g.initial_state.clone();
    let uv = &g.mesh.uv;
    let centre = uv.iter().sum::<DVec2>() / uv.len() as f64;
    for (p, q) in state.x.iter_mut().zip(uv) {
        let d = *q - centre;
        *p = DVec3::new(d.x, height, d.y);
    }
    (g, state)
}

fn mean_height(x: &[DVec3]) -> f64 {
    x.iter().map(|p| p.y).sum::<f64>() / x.len() as f64
}

#[test]
fn split_steps_let_free_cloth_fall_at_gravitys_pace() {
    let settings = |parts: usize| VbdSettings {
        dt: 1. / 60.,
        iterations: 8,
        max_substeps: parts,
        ..Default::default()
    };
    let mut drops = vec![];
    for parts in [1, 8, 16] {
        let (g, mut state) = square(2.);
        let mut solver = VbdSolver::new(g, None, &state).unwrap();
        let start = mean_height(&state.x);
        let mut split = 0;
        for _ in 0..30 {
            split = split.max(solver.parts(&state, &settings(parts)));
            let m = solver.step(&mut state, &settings(parts), None).unwrap();
            assert!((m.dt_s - 1. / 60.).abs() < 1e-15);
        }
        assert!(state.x.iter().all(|p| p.is_finite()));
        drops.push((start - mean_height(&state.x), split));
    }
    let (capped, one) = drops[0];
    let (eight, at_most_eight) = drops[1];
    let (sixteen, at_most_sixteen) = drops[2];
    // Without splitting, the step's contact bound holds the fall back to a crawl.
    assert_eq!(one, 1);
    assert!(capped < 0.3, "{capped}");
    // Split, it falls several times as far, nearly as a damped free fall would in half a
    // second (0.97 m) when allowed enough parts.
    assert!(at_most_eight > 1 && at_most_eight <= 8);
    assert!(eight > 3. * capped, "{eight} vs {capped}");
    assert!(at_most_sixteen > 8 && at_most_sixteen <= 16);
    assert!(sixteen > 0.8 && sixteen < 0.98, "{sixteen}");
}

#[test]
fn split_steps_keep_falling_cloth_out_of_the_floor() {
    let (g, mut state) = square(0.5);
    // The floor is part of the obstacle, as the editor and recordings add it.
    let floor = fashioncraft::avatar::SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    }
    .with_floor(0.);
    let mut solver = VbdSolver::new(g, Some(&floor), &state).unwrap();
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 8,
        floor: Some(0.),
        friction: 0.3,
        max_substeps: 8,
        ..Default::default()
    };
    let mut lowest = f64::INFINITY;
    for _ in 0..60 {
        solver.step(&mut state, &settings, None).unwrap();
        lowest = lowest.min(state.x.iter().map(|p| p.y).fold(f64::INFINITY, f64::min));
    }
    // It reached the floor (from 50 cm) and never went through it.
    assert!(mean_height(&state.x) < 0.02, "{}", mean_height(&state.x));
    assert!(lowest > -1e-6, "{lowest}");
}

#[test]
fn a_shirt_sews_and_settles_with_split_steps() {
    let mannequin =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin);
    let scene = project.compile().unwrap();
    let mut state = scene.state.clone();
    let mut solver =
        VbdSolver::new(scene.garment.clone(), Some(&scene.collider.surface), &state).unwrap();
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 16,
        max_substeps: 8,
        ..Default::default()
    };
    let gap = |x: &[DVec3]| scene.garment.max_seam_gap(x);
    let first = gap(&state.x);
    let mut split = 1;
    for _ in 0..40 {
        split = split.max(solver.parts(&state, &settings));
        solver.step(&mut state, &settings, None).unwrap();
    }
    assert!(state.x.iter().all(|p| p.is_finite()));
    // Sewing closes the seams as it does without splitting.
    assert!(gap(&state.x) < 0.25 * first, "{} of {first}", gap(&state.x));
    // Older settings files, which have no such field, never split.
    let old: VbdSettings = serde_json::from_value(serde_json::json!({
        "dt": 0.01, "iterations": 4, "damping": 1.5, "gravity": [0., -9.81, 0.],
        "contact": serde_json::to_value(VbdSettings::default().contact).unwrap()
    }))
    .unwrap();
    assert_eq!(old.max_substeps, 1);
    assert!(
        !serde_json::to_string(&VbdSettings::default())
            .unwrap()
            .contains("max_substeps")
    );
    assert!(
        VbdSettings {
            max_substeps: 0,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    let _ = split;
}
