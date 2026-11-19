//! The 3D look of trims: coloured triangles lying on a piece's front face, a little off the
//! cloth so they draw over it. A button is a disc with four holes centred on its point; a
//! buttonhole, zipper and topstitch follow their material paths.
use fashioncraft::garment::{
    GarmentTemplate,
    trims::{TRIM_LIFT_M, Trim, TrimKind, trim_triangles},
};
use glam::{DVec2, DVec3};

/// The demo's front panel (upright, facing ±z) with `kind` on it.
fn with(kind: TrimKind) -> (fashioncraft::garment::CompiledGarment, Vec<Trim>) {
    let mut t = GarmentTemplate::two_panel_demo();
    t.panels.truncate(1);
    t.seams.clear();
    let id = t.panels[0].id.clone();
    let g = t.compile().unwrap();
    (
        g,
        vec![Trim {
            id: "t".into(),
            panel_id: id,
            kind,
        }],
    )
}

/// The front normal of the flat panel: away from its faces' winding normal.
fn front(g: &fashioncraft::garment::CompiledGarment) -> (DVec3, DVec3) {
    let x = &g.initial_state.x;
    let [a, b, c] = g.mesh.triangles[0].vertices;
    (x[a], -(x[b] - x[a]).cross(x[c] - x[a]).normalize())
}

#[test]
fn a_button_is_a_disc_with_four_holes_on_the_front_face() {
    let (g, trims) = with(TrimKind::Button {
        center_m: DVec2::new(0.1, 0.1),
        diameter_m: 0.02,
    });
    let tris = trim_triangles(&trims, &g, &g.initial_state.x);
    assert_eq!(tris.len(), 16 * 5, "a 16-sided disc and four holes");
    let (origin, n) = front(&g);
    let centre = tris[0].points[0];
    for t in &tris {
        for p in t.points {
            let height = (p - origin).dot(n);
            assert!(
                height > 0. && height < TRIM_LIFT_M + 0.002,
                "on the front: {height}"
            );
            assert!(p.distance(centre) <= 0.0101, "within the button");
        }
    }
    // The centre lies on the material point.
    let point = g
        .material_point(&trims[0].panel_id, DVec2::new(0.1, 0.1))
        .unwrap();
    let at: DVec3 = point.iter().map(|(v, w)| g.initial_state.x[*v] * *w).sum();
    let off = centre - at;
    assert!(
        (off - n * off.dot(n)).length() < 1e-9,
        "straight above its point"
    );
}

#[test]
fn paths_turn_into_bands_along_them() {
    let path = vec![DVec2::new(0.02, 0.05), DVec2::new(0.12, 0.05)];
    for kind in [
        TrimKind::Zipper {
            path_m: path.clone(),
            width_m: 0.008,
        },
        TrimKind::Topstitch {
            path_m: path.clone(),
            rows: 2,
            spacing_m: 0.003,
        },
        TrimKind::Buttonhole {
            start_m: path[0],
            end_m: path[1],
            width_m: 0.004,
        },
    ] {
        let (g, trims) = with(kind.clone());
        let tris = trim_triangles(&trims, &g, &g.initial_state.x);
        assert!(!tris.is_empty(), "{kind:?}");
        let (origin, n) = front(&g);
        for t in &tris {
            for p in t.points {
                assert!((p - origin).dot(n) > 0., "on the front: {kind:?}");
            }
        }
    }
    // Trims on a piece that is not compiled are skipped.
    let (g, mut trims) = with(TrimKind::Button {
        center_m: DVec2::new(0.1, 0.1),
        diameter_m: 0.02,
    });
    trims[0].panel_id = "gone".into();
    assert!(trim_triangles(&trims, &g, &g.initial_state.x).is_empty());
}
