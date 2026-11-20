//! Piping: a round cord along an edge, drawn as a tube that follows the draped cloth.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        lines2d,
        presets::{self, GarmentKind},
        trims::{TRIM_LIFT_M, Trim, TrimKind, trim_triangles},
    },
};

#[test]
fn piping_is_a_tube_lying_along_its_edge() {
    let base = presets::basic(GarmentKind::Tshirt, 0);
    let panel = &base.panels[0];
    let polygon = panel.polygon.as_ref().unwrap();
    let edge = polygon.edges[0].clone();
    let diameter = 0.004;
    let path_m = lines2d::offset_lines(polygon, &edge, 1, 0.004, false, false)
        .unwrap()
        .pop()
        .unwrap();
    let t = authoring::apply(
        &base,
        Command::SetTrim {
            trim: Trim {
                id: "piping_1".into(),
                panel_id: panel.id.clone(),
                kind: TrimKind::Piping {
                    path_m,
                    diameter_m: diameter,
                },
            },
        },
    )
    .unwrap()
    .template;
    let g = t.clone().compile().unwrap();
    let triangles = trim_triangles(&t.trims, &g, &g.initial_state.x);
    // Eight sides, two triangles each, between every pair of rings.
    assert!(
        triangles.len() >= 16 && triangles.len().is_multiple_of(16),
        "{}",
        triangles.len()
    );
    // Every point of the tube within a diameter (and the lift) of the cloth it follows, give
    // or take the mesh's longest edge, since this measures to the nearest vertex.
    let x = &g.initial_state.x;
    let longest = g
        .mesh
        .triangles
        .iter()
        .flat_map(|t| {
            let [a, b, c] = t.vertices.map(|i| x[i]);
            [a.distance(b), b.distance(c), c.distance(a)]
        })
        .fold(0., f64::max);
    let reach = diameter + TRIM_LIFT_M + longest;
    for tri in &triangles {
        for p in tri.points {
            let nearest = g
                .initial_state
                .x
                .iter()
                .map(|q| q.distance(p))
                .fold(f64::INFINITY, f64::min);
            assert!(nearest < reach, "{nearest} vs {reach}");
        }
    }
    // A piping with no length or size is refused.
    let mut bad = t.trims[0].clone();
    bad.kind = TrimKind::Piping {
        path_m: vec![],
        diameter_m: diameter,
    };
    assert!(bad.validate().is_err());
}
