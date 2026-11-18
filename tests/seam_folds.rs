//! Seam fold angles: hinges across the seam, from each edge's triangle on one side to the
//! triangle at the matching place on the other, with a rest angle. Two weightless squares
//! sewn edge to edge meet at 90° when the seam is folded at 90°, and stay flat at 180°.
//! Deleting a piece or a seam takes its simulation settings with it.
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        GarmentTemplate,
        seam::{BoundaryRef, SeamSpec, Side},
        template::SeamFold,
    },
    vbd::{VbdSettings, VbdSolver},
};
use std::sync::Arc;

/// Two upright 10 cm squares side by side, the right side of the first sewn to the left side
/// of the second.
fn sewn() -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.seams.clear();
    for (k, p) in t.panels.iter_mut().enumerate() {
        p.pins.clear();
        p.placement.rotation_degrees = DVec3::ZERO;
        p.pattern = marvelous_cloth::model::Pattern {
            width: 0.1,
            height: 0.1,
            nx: 5,
            ny: 5,
        };
        p.placement.translation = DVec3::new(0.1 * k as f64, 1., 0.);
    }
    let reference = |panel: &str, side| BoundaryRef {
        reversed: false,
        panel: panel.into(),
        side,
        edge: None,
        range: [0., 1.],
    };
    t.seams.push(SeamSpec {
        id: "middle".into(),
        a: reference(&t.panels[0].id, Side::Right),
        b: reference(&t.panels[1].id, Side::Left),
        a_tail: vec![],
        b_tail: vec![],
        reverse_b: true,
        notches: vec![],
        stiffness: 50_000.,
        allow_ease: false,
    });
    t
}

fn folded(t: &GarmentTemplate, angle: f64) -> GarmentTemplate {
    authoring::apply(
        t,
        Command::SetSeamFold {
            seam: "middle".into(),
            fold: Some(SeamFold {
                angle,
                strength: 50.,
            }),
        },
    )
    .unwrap()
    .template
}

/// The angle between the two squares' mean normals after settling weightless, degrees.
fn settled_angle(t: &GarmentTemplate) -> f64 {
    let g = Arc::new(t.clone().compile().unwrap());
    let far = SurfaceMesh {
        vertices: vec![
            DVec3::new(50., 0.5, 50.),
            DVec3::new(50.1, 0.5, 50.),
            DVec3::new(50., 0.6, 50.),
        ],
        triangles: vec![[0, 1, 2]],
    };
    let mut s = g.initial_state.clone();
    // A nudge out of the plane, so a fold has a way to start.
    for p in &mut s.x {
        p.z += 0.002 * (p.x - 0.1).abs();
    }
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 32,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..480 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    let normal = |k: usize| {
        let r = &g.panels[k];
        g.mesh.triangles[r.triangles.clone()]
            .iter()
            .map(|t| {
                let [a, b, c] = t.vertices;
                (s.x[b] - s.x[a]).cross(s.x[c] - s.x[a]).normalize()
            })
            .sum::<DVec3>()
            .normalize()
    };
    normal(0).angle_between(normal(1)).to_degrees()
}

#[test]
fn a_seam_folded_at_ninety_degrees_meets_square() {
    let angle = settled_angle(&folded(&sewn(), 90.));
    assert!((angle - 90.).abs() < 12., "{angle}°");
}

#[test]
fn a_seam_at_180_stays_flat() {
    let angle = settled_angle(&folded(&sewn(), 180.));
    assert!(angle < 3., "{angle}°");
}

#[test]
fn deleting_pieces_and_seams_takes_their_settings_along() {
    let t = folded(&sewn(), 90.);
    let second = t.panels[1].id.clone();
    let t = authoring::apply(
        &t,
        Command::SetLayer {
            panels: vec![second.clone()],
            layer: 2,
        },
    )
    .unwrap()
    .template;
    let t = authoring::apply(
        &t,
        Command::SetPressure {
            panels: vec![second.clone()],
            pressure: 5.,
        },
    )
    .unwrap()
    .template;
    let unsewn = authoring::apply(
        &t,
        Command::RemoveSeam {
            seam: "middle".into(),
        },
    )
    .unwrap()
    .template;
    assert!(unsewn.seam_folds.is_empty());
    let gone = authoring::apply(&t, Command::RemovePanel { panel: second }).unwrap();
    assert!(gone.template.panel_layers.is_empty());
    assert!(gone.template.panel_pressure.is_empty());
    assert!(gone.template.seam_folds.is_empty());
}
