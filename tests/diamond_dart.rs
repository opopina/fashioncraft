//! Diamond dart: a diamond hole whose upper legs and lower legs are sewn together closes
//! into a dart; its side corners come together when the cloth settles.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        GarmentTemplate,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

fn sheet() -> GarmentTemplate {
    authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.3, 0.4),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template
}

fn centre(t: &GarmentTemplate) -> DVec2 {
    let p = t.panels[0].polygon.as_ref().unwrap();
    let lo = p.points.iter().fold(DVec2::MAX, |m, q| m.min(*q));
    let hi = p.points.iter().fold(DVec2::MIN, |m, q| m.max(*q));
    (lo + hi) / 2.
}

#[test]
fn a_diamond_hole_is_added_with_two_seams_and_closes_when_settled() {
    let t = sheet();
    let c = centre(&t);
    let width = 0.05;
    let edit = authoring::apply(
        &t,
        Command::DiamondDart {
            panel: t.panels[0].id.clone(),
            center: c,
            width,
            height: 0.2,
        },
    )
    .unwrap();
    let after = edit.template;
    assert_eq!(after.panels[0].polygon.as_ref().unwrap().holes.len(), 1);
    assert_eq!(after.seams.len(), t.seams.len() + 2);

    // Settle without gravity, far from any body.
    let g = Arc::new(after.compile().unwrap());
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    for (p, uv) in s.x.iter_mut().zip(&g.mesh.uv) {
        p.z += 0.002 * ((uv.x - c.x).abs() + (uv.y - c.y).abs());
    }
    let corner = |at: DVec2| {
        (0..g.mesh.uv.len())
            .min_by(|&a, &b| {
                g.mesh.uv[a]
                    .distance(at)
                    .total_cmp(&g.mesh.uv[b].distance(at))
            })
            .unwrap()
    };
    let (left, right) = (
        corner(c - DVec2::new(width / 2., 0.)),
        corner(c + DVec2::new(width / 2., 0.)),
    );
    let gap_before = s.x[left].distance(s.x[right]);
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 16,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..240 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    let gap_after = s.x[left].distance(s.x[right]);
    assert!(
        gap_after < 0.25 * gap_before,
        "the dart closed from {gap_before} to {gap_after}"
    );
}

#[test]
fn diamond_darts_that_do_not_fit_are_refused() {
    let t = sheet();
    let c = centre(&t);
    let panel = t.panels[0].id.clone();
    for (center, width, height) in [
        (c, 0.05, 0.06),           // not at least twice as tall as wide
        (c, 0.001, 0.1),           // too narrow
        (c + DVec2::X, 0.03, 0.1), // outside the piece
        (c, 0.03, 0.6),            // taller than the piece
    ] {
        assert!(
            authoring::apply(
                &t,
                Command::DiamondDart {
                    panel: panel.clone(),
                    center,
                    width,
                    height
                }
            )
            .is_err(),
            "{center:?} {width} {height}"
        );
    }
}
