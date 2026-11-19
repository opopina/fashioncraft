//! A moving obstacle: the avatar can be moved while the cloth rests on it. Moved in steps
//! smaller than the contact radius, it pushes the cloth along instead of passing through it.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use std::sync::Arc;

#[test]
fn a_wall_moved_in_small_steps_pushes_the_cloth_without_crossing_it() {
    let t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.2, 0.2),
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
    let mut s = g.initial_state.clone();
    let [a, b, c] = g.mesh.triangles[0].vertices.map(|i| s.x[i]);
    let n = (b - a).cross(c - a).normalize();
    let centre = s.x.iter().sum::<DVec3>() / s.x.len() as f64;
    // A 2 m square wall 1 cm behind the cloth, facing it.
    let u = (b - a).normalize();
    let v = n.cross(u);
    let wall_at = |offset: f64| {
        let o = centre + n * offset;
        SurfaceMesh {
            vertices: vec![o - u - v, o + u - v, o + u + v, o - u + v],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        }
    };
    let mut solver = VbdSolver::new(g.clone(), Some(&wall_at(-0.01)), &s).unwrap();
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 16,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    let start = s.x.iter().map(|p| (p - centre).dot(n)).sum::<f64>() / s.x.len() as f64;
    let mut offset = -0.01;
    for _ in 0..40 {
        offset += 0.001;
        solver.set_obstacle(&s.x, &wall_at(offset)).unwrap();
        solver.step(&mut s, &settings, None).unwrap();
        for p in &s.x {
            assert!(
                (p - centre).dot(n) > offset,
                "cloth crossed the wall at offset {offset}"
            );
        }
    }
    let end = s.x.iter().map(|p| (p - centre).dot(n)).sum::<f64>() / s.x.len() as f64;
    assert!(end - start > 0.02, "the cloth moved {} m", end - start);
    // A mesh with another vertex count is refused.
    let mut wrong = wall_at(offset);
    wrong.vertices.push(DVec3::ZERO);
    assert!(solver.set_obstacle(&s.x, &wrong).is_err());
}
