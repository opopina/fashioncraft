//! Quad meshes: inside, an even grid of squares along the mesh axis, each two triangles;
//! free triangles only along the outline.
use fashioncraft::garment::meshing::{MeshingSettings, triangulate};
use glam::DVec2;

/// Share of the triangles that are half of a grid square `h` wide, turned `axis` degrees.
fn grid_share(faces: &[[usize; 3]], uv: &[DVec2], h: f64, axis: f64) -> f64 {
    let (s, c) = axis.to_radians().sin_cos();
    let along = |d: DVec2| (c * d.x + s * d.y, -s * d.x + c * d.y);
    let on_grid = faces
        .iter()
        .filter(|t| {
            let [a, b, c3] = t.map(|i| uv[i]);
            let area = (b - a).perp_dot(c3 - a).abs() * 0.5;
            if (area - 0.5 * h * h).abs() > 0.01 * h * h {
                return false;
            }
            // Two of its sides run along the grid, one unit long.
            [(a, b), (b, c3), (c3, a)]
                .iter()
                .filter(|(p, q)| {
                    let (x, y) = along(*q - *p);
                    let aligned = x.abs() < 1e-6 * h || y.abs() < 1e-6 * h;
                    aligned && ((q - p).length() - h).abs() < 1e-6 * h
                })
                .count()
                == 2
        })
        .count();
    on_grid as f64 / faces.len() as f64
}

fn rectangle() -> (Vec<DVec2>, Vec<String>) {
    let points = vec![
        DVec2::new(0., 0.),
        DVec2::new(0.3, 0.),
        DVec2::new(0.3, 0.2),
        DVec2::new(0., 0.2),
    ];
    let names = (1..=4).map(|i| format!("edge_{i}")).collect();
    (points, names)
}

#[test]
fn a_quad_mesh_is_mostly_half_squares_along_its_axis() {
    let (points, names) = rectangle();
    for axis in [0., 30.] {
        let quads = MeshingSettings {
            target_edge_m: 0.02,
            axis_degrees: axis,
            quads: true,
            ..Default::default()
        };
        let mesh = triangulate(&points, &names, &quads).unwrap();
        let share = grid_share(&mesh.faces, &mesh.uv, 0.02, axis);
        // The rest are the free triangles along the outline, more of them when the grid is
        // turned against it.
        assert!(share > 0.5, "axis {axis}: {share}");
        // Triangles stay triangles otherwise.
        let free = triangulate(
            &points,
            &names,
            &MeshingSettings {
                quads: false,
                ..quads.clone()
            },
        )
        .unwrap();
        let free_share = grid_share(&free.faces, &free.uv, 0.02, axis);
        assert!(free_share < 0.2, "axis {axis}: {free_share}");
    }
}

#[test]
fn the_quad_setting_round_trips_and_defaults_to_triangles() {
    let quads = MeshingSettings {
        quads: true,
        ..Default::default()
    };
    let text = serde_json::to_string(&quads).unwrap();
    assert!(text.contains("\"quads\":true"));
    let plain = serde_json::to_string(&MeshingSettings::default()).unwrap();
    assert!(!plain.contains("quads"));
    let back: MeshingSettings = serde_json::from_str(&text).unwrap();
    assert!(back.quads);
}

#[test]
fn pieces_convert_to_quads_and_back_even_with_internal_lines() {
    use fashioncraft::{
        authoring::{self, Command},
        garment::{pleats::PleatKind, presets},
    };
    let base = presets::basic(presets::GarmentKind::Tshirt, 1);
    let ids: Vec<String> = base.panels.iter().map(|p| p.id.clone()).collect();
    let quads = authoring::apply(
        &base,
        Command::SetMeshQuads {
            panels: ids.clone(),
            quads: true,
        },
    )
    .unwrap()
    .template;
    for p in &quads.panels {
        let m = p.polygon.as_ref().unwrap().meshing_settings();
        assert!(m.quads && m.axis_degrees == p.grain_degrees);
    }
    let g = quads.clone().compile().unwrap();
    assert!(!g.mesh.triangles.is_empty());
    // Already quads: refused; back to triangles works.
    assert!(
        authoring::apply(
            &quads,
            Command::SetMeshQuads {
                panels: ids.clone(),
                quads: true
            }
        )
        .is_err()
    );
    let back = authoring::apply(
        &quads,
        Command::SetMeshQuads {
            panels: ids.clone(),
            quads: false,
        },
    )
    .unwrap()
    .template;
    back.compile().unwrap();
    // Pleat lines are constraints the grid keeps clear of.
    let pleated = authoring::apply(
        &quads,
        Command::AddPleats {
            panel: ids[0].clone(),
            edge: "hem".into(),
            count: 2,
            depth_m: 0.01,
            kind: PleatKind::Knife,
            tack: false,
        },
    )
    .unwrap()
    .template;
    pleated.compile().unwrap();
}
