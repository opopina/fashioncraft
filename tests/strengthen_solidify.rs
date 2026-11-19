//! Strengthen and Solidify. A strengthened piece is simulated ten times stiffer, so it
//! stretches far less under its own weight. A solidified piece is drawn to its captured shape
//! fitted (rotated and moved) to where it is heading: a square held at its centre stays flat
//! instead of drooping, while a solidified square still falls exactly as fast as a free one.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{GarmentTemplate, pins::MaterialPinTarget},
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
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

fn settings(dt: f64) -> VbdSettings {
    VbdSettings {
        dt,
        iterations: 32,
        damping: 0.,
        ..Default::default()
    }
}

fn run(t: &GarmentTemplate, state: Option<State>, dt: f64, steps: usize) -> (State, f64) {
    let g = Arc::new(t.clone().compile().unwrap());
    let mut s = state.unwrap_or_else(|| g.initial_state.clone());
    let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
    let mut stretch = 0.;
    let settings = VbdSettings {
        damping: 2.,
        ..settings(dt)
    };
    for _ in 0..steps {
        stretch = solver
            .step(&mut s, &settings, None)
            .unwrap()
            .extension_p95_percent;
    }
    (s, stretch)
}

#[test]
fn a_strengthened_piece_stretches_far_less_under_its_weight() {
    // The demo front panel hanging from its top corners.
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let (_, plain) = run(&t, None, 1. / 240., 240);
    let strong = authoring::apply(
        &t,
        Command::Strengthen {
            panels: vec![t.panels[0].id.clone()],
            on: true,
        },
    )
    .unwrap();
    assert!(strong.reset_simulation);
    let (_, stiff) = run(&strong.template, None, 1. / 240., 240);
    assert!(plain > 0.05, "the plain piece stretches: {plain} %");
    assert!(stiff < 0.3 * plain, "{stiff} % vs {plain} %");
    // And back.
    let back = authoring::apply(
        &strong.template,
        Command::Strengthen {
            panels: vec![t.panels[0].id.clone()],
            on: false,
        },
    )
    .unwrap();
    assert!(back.template.strengthened_panels.is_empty());
}

/// A flat 24 cm square, held at its centre when `held`.
fn umbrella(held: bool) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let p = &mut t.panels[0];
    p.pins.clear();
    p.pattern = fashioncraft::model::Pattern {
        width: 0.24,
        height: 0.24,
        nx: 6,
        ny: 6,
    };
    p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
    p.placement.translation = DVec3::new(-0.12, 1., 0.12);
    if held {
        let g = t.clone().compile().unwrap();
        let centre = g.mesh.uv.iter().sum::<glam::DVec2>() / g.mesh.uv.len() as f64;
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
    }
    t
}

#[test]
fn a_solidified_piece_keeps_its_shape() {
    // Held at its centre, a free square droops like an umbrella; solidified flat, it stays
    // nearly flat (balanced on the pin, a rigid plate has no reason to tip).
    let t = umbrella(true);
    let (free, _) = run(&t, None, 1. / 240., 240);
    let g = t.clone().compile().unwrap();
    let start = g.initial_state.clone();
    let id = t.panels[0].id.clone();
    let (edit, next) =
        authoring::prepare_solidify_edit(&t, &g, &start, std::slice::from_ref(&id), true).unwrap();
    assert!(edit.reset_simulation);
    let (solid, _) = run(&edit.template, Some(next), 1. / 240., 240);
    let drop = |s: &State| 1. - s.x.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    assert!(
        drop(&free) > 0.05,
        "the free square droops: {}",
        drop(&free)
    );
    assert!(
        drop(&solid) < 0.3 * drop(&free),
        "{} vs {}",
        drop(&solid),
        drop(&free)
    );
    // Released, it is plain cloth again.
    let (released, _) =
        authoring::prepare_solidify_edit(&edit.template, &edit.garment, &start, &[id], false)
            .unwrap();
    assert!(released.template.solidified_panels.is_empty());
    assert!(released.garment.solid.is_empty());
}

#[test]
fn a_solidified_piece_falls_as_fast_as_a_free_one() {
    let t = umbrella(false);
    let fall = |t: &GarmentTemplate, state: Option<State>| {
        let g = Arc::new(t.clone().compile().unwrap());
        let mut s = state.unwrap_or_else(|| g.initial_state.clone());
        let mut solver = VbdSolver::new(g, Some(&far_away()), &s).unwrap();
        for _ in 0..60 {
            solver.step(&mut s, &settings(1. / 480.), None).unwrap();
        }
        s.v.iter().sum::<DVec3>() / s.v.len() as f64
    };
    let free = fall(&t, None);
    let g = t.clone().compile().unwrap();
    let (edit, next) =
        authoring::prepare_solidify_edit(&t, &g, &g.initial_state, &[t.panels[0].id.clone()], true)
            .unwrap();
    let solid = fall(&edit.template, Some(next));
    assert!((free.y + 9.81 * 0.125).abs() < 0.02, "{free}");
    assert!((solid - free).length() < 1e-3, "{solid} vs {free}");
}

#[test]
fn strengthen_and_solidify_are_validated() {
    let t = umbrella(false);
    let id = t.panels[0].id.clone();
    assert!(
        authoring::apply(
            &t,
            Command::Strengthen {
                panels: vec!["nope".into()],
                on: true
            }
        )
        .is_err()
    );
    assert!(
        authoring::apply(
            &t,
            Command::Strengthen {
                panels: vec![id.clone()],
                on: false
            }
        )
        .is_err(),
        "no empty undo steps"
    );
    let g = t.clone().compile().unwrap();
    let (frozen, state) =
        authoring::prepare_freeze_edit(&t, &g, &g.initial_state, &id, true).unwrap();
    assert!(
        authoring::prepare_solidify_edit(&frozen.template, &frozen.garment, &state, &[id], true)
            .is_err(),
        "a frozen piece cannot also be solidified"
    );
    let json = serde_json::to_string(&t).unwrap();
    assert!(!json.contains("strengthened") && !json.contains("solidified"));
}
