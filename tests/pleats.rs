//! Pleats laid along a segment: grouped, folded internal lines across the piece, tacks closing
//! each pleat at the segment, one undo step; and the piece draws in when it settles.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        meshing::MeshingSettings,
        pleats::{INNER_FOLD_DEGREES, OUTER_FOLD_DEGREES, PleatKind},
        presets,
    },
};
use glam::{DVec2, DVec3};

/// A 60 × 30 cm rectangle meshed at `edge_m`; its first segment runs along the top, 60 cm.
fn panel_meshed(edge_m: f64) -> (GarmentTemplate, String, String) {
    let points = [[0., 0.], [0.6, 0.], [0.6, 0.3], [0., 0.3]];
    let mut t = authoring::apply(
        &presets::basic(presets::GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.iter().map(|p| DVec2::from_array(*p)).collect(),
            tangents: vec![DVec2::ZERO; 4],
            meshing: MeshingSettings {
                target_edge_m: edge_m,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    t.panels[0].placement.translation = DVec3::new(-0.3, 1., 0.);
    let id = t.panels[0].id.clone();
    let edge = t.panels[0].polygon.as_ref().unwrap().edges[0].clone();
    (t, id, edge)
}

fn panel() -> (GarmentTemplate, String, String) {
    panel_meshed(0.02)
}

fn pleats(panel: &str, edge: &str, count: usize, depth_m: f64, kind: PleatKind) -> Command {
    Command::AddPleats {
        panel: panel.into(),
        edge: edge.into(),
        count,
        depth_m,
        kind,
        tack: true,
    }
}

#[test]
fn pleats_add_grouped_folded_lines_and_tacks_in_one_edit() {
    let (base, id, edge) = panel();
    let edit = authoring::apply(&base, pleats(&id, &edge, 3, 0.02, PleatKind::Knife)).unwrap();
    let t = &edit.template;
    let lines = &t.panels[0].polygon.as_ref().unwrap().lines;
    assert_eq!(lines.len(), 6);
    let group = lines[0].group.clone().unwrap();
    assert!(lines.iter().all(|l| l.group.as_ref() == Some(&group)));
    // Straight across the piece, perpendicular to the top, from near it to near the bottom.
    for line in lines {
        let [a, b] = [line.points[0], line.points[1]];
        assert!((a.x - b.x).abs() < 1e-9);
        assert!(a.y.min(b.y) < 0.002 && a.y.max(b.y) > 0.298);
    }
    let angles: Vec<f64> = t.line_folds.iter().map(|f| f.angle).collect();
    assert_eq!(angles.len(), 6);
    assert_eq!(angles[0], OUTER_FOLD_DEGREES);
    assert_eq!(angles[1], INNER_FOLD_DEGREES);
    assert_eq!(t.tacks.len(), 3);
    // Each tack joins the outer crease to where it lands, 2 × depth further on, just inside.
    for tack in &t.tacks {
        assert!((tack.b.uv.x - tack.a.uv.x - 0.04).abs() < 1e-9);
        assert!(tack.a.uv.y > 0. && tack.a.uv.y < 0.01);
    }
    t.clone().compile().unwrap();
    // A second set gets its own names and group.
    let again = authoring::apply(t, pleats(&id, &edge, 1, 0.01, PleatKind::Box)).unwrap();
    let more = &again.template.panels[0].polygon.as_ref().unwrap().lines;
    assert_eq!(more.len(), 10);
    assert!(more[6..].iter().all(|l| l.group.as_ref() != Some(&group)));
    again.template.clone().compile().unwrap();
}

#[test]
fn pleats_that_do_not_fit_or_have_no_segment_are_refused() {
    let (base, id, edge) = panel();
    assert!(authoring::apply(&base, pleats(&id, &edge, 20, 0.02, PleatKind::Box)).is_err());
    assert!(authoring::apply(&base, pleats(&id, "nowhere", 2, 0.02, PleatKind::Knife)).is_err());
    assert!(authoring::apply(&base, pleats("nobody", &edge, 2, 0.02, PleatKind::Knife)).is_err());
}

#[test]
fn a_pleated_piece_draws_in_across_its_pleats() {
    use fashioncraft::{
        avatar::SurfaceMesh,
        vbd::{VbdSettings, VbdSolver},
    };
    let (base, id, edge) = panel_meshed(0.04);
    let t = authoring::apply(&base, pleats(&id, &edge, 3, 0.02, PleatKind::Knife))
        .unwrap()
        .template;
    let g = std::sync::Arc::new(t.compile().unwrap());
    // Weightless, far from any obstacle: only the creases move the cloth.
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
        p.z += 0.002 * (uv.x * 40.).sin();
    }
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
    // Across the pleats, along the top: from the left side to the right side, flat 60 cm.
    let side = |x: f64| {
        let ids: Vec<usize> = (0..s.x.len())
            .filter(|&i| (g.mesh.uv[i].x - x).abs() < 1e-9)
            .collect();
        ids.iter().map(|&i| s.x[i]).sum::<DVec3>() / ids.len() as f64
    };
    let width = side(0.).distance(side(0.6));
    assert!(width < 0.57, "pleated width {width} m, flat 0.6 m");
}
