//! Shared fixture for the fold tests: a creased rectangle, its fold, its settling and the
//! mean normals of its halves.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        CompiledGarment, GarmentTemplate, domain::InternalLine, meshing::MeshingSettings, presets,
        template::LineFold,
    },
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

/// A 40 × 30 cm rectangle with an internal line `crease` down the middle.
pub fn creased() -> GarmentTemplate {
    let points = [[0., 0.], [0.4, 0.], [0.4, 0.3], [0., 0.3]];
    let mut t = authoring::apply(
        &presets::basic(presets::GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.iter().map(|p| DVec2::from_array(*p)).collect(),
            tangents: vec![DVec2::ZERO; 4],
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
    t.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .lines
        .push(InternalLine {
            id: "crease".into(),
            points: vec![DVec2::new(0.2, 0.002), DVec2::new(0.2, 0.298)],
            tangents: vec![DVec2::ZERO; 2],
            closed: false,
            group: None,
        });
    t.panels[0].placement.translation = DVec3::new(-0.2, 1., 0.);
    t
}

pub fn folded(t: &GarmentTemplate, angle: f64) -> GarmentTemplate {
    let id = t.panels[0].id.clone();
    authoring::apply(
        t,
        Command::SetLineFold {
            panel: id.clone(),
            line: "crease".into(),
            fold: Some(LineFold {
                angle,
                strength: 50.,
                ..LineFold::new(&id, "crease")
            }),
        },
    )
    .unwrap()
    .template
}

/// Mean unit normals of the triangles left and right of the crease.
pub fn halves(g: &CompiledGarment, s: &State) -> (DVec3, DVec3) {
    let (mut left, mut right) = (DVec3::ZERO, DVec3::ZERO);
    for t in &g.mesh.triangles {
        let [a, b, c] = t.vertices;
        let n = (s.x[b] - s.x[a]).cross(s.x[c] - s.x[a]).normalize();
        let u = (g.mesh.uv[a].x + g.mesh.uv[b].x + g.mesh.uv[c].x) / 3.;
        if u < 0.18 {
            left += n;
        } else if u > 0.22 {
            right += n;
        }
    }
    (left.normalize(), right.normalize())
}

pub fn settle(t: &GarmentTemplate) -> (Arc<CompiledGarment>, State) {
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
    // A nudge out of the plane, so the fold has a way to start.
    for (p, uv) in s.x.iter_mut().zip(&g.mesh.uv) {
        p.z += 0.002 * (uv.x - 0.2).abs();
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
    (g, s)
}
