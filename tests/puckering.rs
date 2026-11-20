//! Puckering: a line drawn in a little, so the cloth beside it ripples instead of lying flat.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{GarmentTemplate, meshing::MeshingSettings, presets, template::LineSpring},
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

/// A 40 × 30 cm rectangle; its first segment runs along the top.
fn rectangle() -> (GarmentTemplate, String, String) {
    let points = [[0., 0.], [0.4, 0.], [0.4, 0.3], [0., 0.3]];
    let t = authoring::apply(
        &presets::basic(presets::GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.iter().map(|p| DVec2::from_array(*p)).collect(),
            tangents: vec![DVec2::ZERO; 4],
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let id = t.panels[0].id.clone();
    let edge = t.panels[0].polygon.as_ref().unwrap().edges[0].clone();
    (t, id, edge)
}

#[test]
fn a_puckered_edge_draws_in_and_the_cloth_beside_it_ripples() {
    let (base, id, edge) = rectangle();
    let t = authoring::apply(
        &base,
        Command::SetLineSpring {
            panel: id.clone(),
            line: edge.clone(),
            spring: Some(LineSpring::puckering(&id, &edge)),
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
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
        p.z += 0.001 * (uv.x * 60.).sin();
    }
    let flat_z: f64 = s.x.iter().map(|p| p.z).sum::<f64>() / s.x.len() as f64;
    let mut solver = VbdSolver::new(g.clone(), Some(&far), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 240.,
        iterations: 16,
        damping: 3.,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    for _ in 0..160 {
        solver.step(&mut s, &settings, None).unwrap();
    }
    // The top edge, end to end along its vertices.
    let mut top: Vec<usize> = (0..s.x.len())
        .filter(|&i| g.mesh.uv[i].y.abs() < 1e-9)
        .collect();
    top.sort_by(|a, b| g.mesh.uv[*a].x.total_cmp(&g.mesh.uv[*b].x));
    let length: f64 = top.windows(2).map(|w| s.x[w[0]].distance(s.x[w[1]])).sum();
    assert!(length < 0.4 * 0.97, "top edge {length} m of 0.4 m");
    // The cloth along it no longer lies in one plane.
    let near: Vec<f64> = (0..s.x.len())
        .filter(|&i| g.mesh.uv[i].y < 0.05)
        .map(|i| s.x[i].z - flat_z)
        .collect();
    let spread = near.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - near.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(spread > 0.004, "ripples {spread} m");
    // Puckering is a line spring of its own kind, and validates.
    assert!(LineSpring::puckering(&id, &edge).validate().is_ok());
}
