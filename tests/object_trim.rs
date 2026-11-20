//! Object trims (File › Open › Trim): a rigid 3D object from a mesh file, fixed on the cloth.
use fashioncraft::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    garment::{
        presets::{self, GarmentKind},
        trims::{
            MAX_OBJECT_TRIANGLES, TRIM_LIFT_M, Trim, TrimKind, object_from_mesh, trim_triangles,
        },
    },
};
use glam::{DVec2, DVec3};

/// A closed box of `size`, from the origin, as a mesh file would hold it.
fn cuboid(size: DVec3) -> SurfaceMesh {
    let vertices = (0..8)
        .map(|k| {
            DVec3::new(
                if k & 1 == 0 { 0. } else { size.x },
                if k & 2 == 0 { 0. } else { size.y },
                if k & 4 == 0 { 0. } else { size.z },
            )
        })
        .collect();
    let triangles = vec![
        [0, 2, 1],
        [1, 2, 3],
        [4, 5, 6],
        [5, 7, 6],
        [0, 1, 4],
        [1, 5, 4],
        [2, 6, 3],
        [3, 6, 7],
        [0, 4, 2],
        [2, 4, 6],
        [1, 3, 5],
        [3, 7, 5],
    ];
    SurfaceMesh {
        vertices,
        triangles,
    }
}

fn extent(kind: &TrimKind) -> (DVec3, DVec3) {
    let TrimKind::Object { vertices, .. } = kind else {
        panic!("not an object trim");
    };
    vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), v| {
            let v = glam::Vec3::from_array(*v).as_dvec3();
            (lo.min(v), hi.max(v))
        },
    )
}

#[test]
fn a_mesh_becomes_an_object_lying_flat_in_metres() {
    // A 40 x 4 x 30 mm buckle, thin along Y, saved in millimetres.
    let kind = object_from_mesh("buckle", &cuboid(DVec3::new(40., 4., 30.)), DVec2::ZERO).unwrap();
    let (lo, hi) = extent(&kind);
    // The thin axis now points out of the cloth; the others turned along without mirroring.
    assert!(
        (hi - lo - DVec3::new(0.030, 0.040, 0.004)).length() < 1e-6,
        "{lo} {hi}"
    );
    // Centred across, its back at zero.
    assert!((lo.x + hi.x).abs() < 1e-6 && (lo.y + hi.y).abs() < 1e-6 && lo.z.abs() < 1e-6);
    // Centimetres and metres are told by size.
    let (lo, hi) =
        extent(&object_from_mesh("badge", &cuboid(DVec3::new(5., 3., 0.2)), DVec2::ZERO).unwrap());
    assert!(((hi - lo).x - 0.05).abs() < 1e-6);
    let (lo, hi) = extent(
        &object_from_mesh("stud", &cuboid(DVec3::new(0.01, 0.01, 0.002)), DVec2::ZERO).unwrap(),
    );
    assert!(((hi - lo).x - 0.01).abs() < 1e-6);
}

#[test]
fn an_object_trim_sits_on_the_front_of_the_cloth_and_changes_the_look_only() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let panel = &base.panels[0];
    let g0 = base.clone().compile().unwrap();
    let range = g0.panels.iter().find(|r| r.id == panel.id).unwrap();
    let uv = &g0.mesh.uv[range.vertices.clone()];
    let centre = uv.iter().sum::<DVec2>() / uv.len() as f64;
    let kind = object_from_mesh("buckle", &cuboid(DVec3::new(40., 4., 30.)), centre).unwrap();
    let edit = authoring::apply(
        &base,
        Command::SetTrim {
            trim: Trim {
                id: "trim_1".into(),
                panel_id: panel.id.clone(),
                kind,
            },
        },
    )
    .unwrap();
    let t = edit.template;
    assert!(authoring::same_physics(&base, &t));
    // It survives saving and reading back.
    let back: fashioncraft::garment::GarmentTemplate =
        serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
    assert_eq!(back.trims, t.trims);

    let g = t.clone().compile().unwrap();
    let x = &g.initial_state.x;
    let triangles = trim_triangles(&t.trims, &g, x);
    assert_eq!(triangles.len(), 12);
    // Every corner within the buckle's half-diagonal (and the lift) of the point it is fixed
    // to, plus the mesh's longest edge since this measures to the nearest vertex.
    let longest = g
        .mesh
        .triangles
        .iter()
        .flat_map(|t| {
            let [a, b, c] = t.vertices.map(|i| x[i]);
            [a.distance(b), b.distance(c), c.distance(a)]
        })
        .fold(0., f64::max);
    let reach = DVec3::new(0.03, 0.04, 0.004).length() + TRIM_LIFT_M + longest;
    let points: Vec<DVec3> = triangles.iter().flat_map(|t| t.points).collect();
    let middle = points.iter().sum::<DVec3>() / points.len() as f64;
    for p in &points {
        let nearest = x
            .iter()
            .map(|q| q.distance(*p))
            .fold(f64::INFINITY, f64::min);
        assert!(nearest < reach, "{nearest} vs {reach}");
        assert!(p.distance(middle) < 0.03, "{}", p.distance(middle));
    }

    // Turning and sizing it are edits of the same trim.
    let mut bigger = t.trims[0].clone();
    if let TrimKind::Object {
        scale,
        turn_degrees,
        ..
    } = &mut bigger.kind
    {
        *scale = 2.;
        *turn_degrees = 90.;
    }
    let t2 = authoring::apply(&t, Command::SetTrim { trim: bigger })
        .unwrap()
        .template;
    assert_eq!(t2.trims.len(), 1);
    let spread = |template: &fashioncraft::garment::GarmentTemplate| {
        let g = template.clone().compile().unwrap();
        let points: Vec<DVec3> = trim_triangles(&template.trims, &g, &g.initial_state.x)
            .iter()
            .flat_map(|t| t.points)
            .collect();
        let middle = points.iter().sum::<DVec3>() / points.len() as f64;
        points.iter().map(|p| p.distance(middle)).fold(0., f64::max)
    };
    assert!((spread(&t2) / spread(&t) - 2.).abs() < 0.05);
}

#[test]
fn unusable_objects_are_refused() {
    let empty = SurfaceMesh {
        vertices: vec![DVec3::ZERO; 3],
        triangles: vec![],
    };
    assert!(object_from_mesh("none", &empty, DVec2::ZERO).is_err());
    let mut huge = cuboid(DVec3::ONE);
    huge.triangles = vec![[0, 1, 2]; MAX_OBJECT_TRIANGLES + 1];
    assert!(object_from_mesh("huge", &huge, DVec2::ZERO).is_err());
    let mut trim = Trim {
        id: "trim_1".into(),
        panel_id: "front".into(),
        kind: object_from_mesh("badge", &cuboid(DVec3::new(0.05, 0.03, 0.002)), DVec2::ZERO)
            .unwrap(),
    };
    assert!(trim.validate().is_ok());
    if let TrimKind::Object { scale, .. } = &mut trim.kind {
        // Over two metres across.
        *scale = 100.;
    }
    assert!(trim.validate().is_err());
}
