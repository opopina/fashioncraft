//! Replace outline: a closed internal shape becomes the piece's outline, keeping in place what
//! lies inside it and refusing whatever would be lost.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{self, Command},
    fitting::FittingProject,
    garment::{
        GarmentTemplate,
        domain::{Hole, InternalLine},
        graphics::{GraphicPlacement, GraphicSide},
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
        template::{Tack, TackPoint},
    },
};

/// A lone 40 × 50 cm piece with a closed square shape in it, 10–30 cm across and 10–40 cm up.
fn piece() -> GarmentTemplate {
    let mut t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.4, 0.5),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.04,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let polygon = t.panels[0].polygon.as_mut().unwrap();
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    polygon.lines.push(InternalLine {
        id: "window".into(),
        points: [(0.1, 0.1), (0.3, 0.1), (0.3, 0.4), (0.1, 0.4)]
            .map(|(x, y)| lo + DVec2::new(x, y))
            .to_vec(),
        tangents: vec![DVec2::ZERO; 4],
        closed: true,
        group: None,
    });
    t
}

fn lo(t: &GarmentTemplate) -> DVec2 {
    let polygon = t.panels[0].polygon.as_ref().unwrap();
    polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p))
}

/// Where a material point of the piece lies on the 2D board.
fn on_board(t: &GarmentTemplate, p: DVec2) -> DVec2 {
    let piece = &t.panels[0];
    piece.layout_2d.unwrap_or_default() + authoring::rotate_2d(p, piece.layout_rotation_degrees)
}

fn area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| points[k].perp_dot(points[(k + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.
}

fn replace(t: &GarmentTemplate) -> Result<GarmentTemplate, String> {
    authoring::apply(
        t,
        Command::ReplaceOutline {
            panel: t.panels[0].id.clone(),
            line: "window".into(),
        },
    )
    .map(|e| e.template)
}

#[test]
fn the_shape_becomes_the_outline_and_what_is_inside_keeps_its_place() {
    let mut t = piece();
    let id = t.panels[0].id.clone();
    let lo = lo(&t);
    let polygon = t.panels[0].polygon.as_mut().unwrap();
    polygon.lines.push(InternalLine {
        id: "dart".into(),
        points: vec![lo + DVec2::new(0.15, 0.15), lo + DVec2::new(0.2, 0.25)],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    });
    let corner = lo + DVec2::new(0.22, 0.3);
    polygon.holes.push(Hole {
        points: vec![
            corner,
            corner + DVec2::new(0., 0.02),
            corner + DVec2::new(0.02, 0.02),
            corner + DVec2::new(0.02, 0.),
        ],
        edges: (0..4).map(|i| format!("eyelet_{i}")).collect(),
    });
    t.graphics.push(GraphicPlacement {
        id: "logo".into(),
        panel_id: id.clone(),
        asset_id: "image_1".into(),
        center_m: lo + DVec2::new(0.2, 0.35),
        size_m: DVec2::splat(0.04),
        rotation_degrees: 0.,
        side: GraphicSide::Front,
        opacity: 1.,
    });
    t.tacks.push(Tack {
        id: "tack".into(),
        a: TackPoint {
            panel: id.clone(),
            uv: lo + DVec2::new(0.12, 0.2),
        },
        b: TackPoint {
            panel: id.clone(),
            uv: lo + DVec2::new(0.28, 0.2),
        },
    });
    let board = |t: &GarmentTemplate| {
        let polygon = t.panels[0].polygon.as_ref().unwrap();
        let dart = polygon.lines.iter().find(|l| l.id == "dart").unwrap();
        [
            on_board(t, dart.points[0]),
            on_board(t, polygon.holes[0].points[0]),
            on_board(t, t.graphics[0].center_m),
            on_board(t, t.tacks[0].a.uv),
            on_board(t, t.tacks[0].b.uv),
        ]
    };
    let shape_on_board: Vec<DVec2> = t.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .find(|l| l.id == "window")
        .unwrap()
        .points
        .iter()
        .map(|p| on_board(&t, *p))
        .collect();
    let before = board(&t);
    let after = replace(&t).unwrap();
    let outline = after.panels[0].polygon.as_ref().unwrap();
    assert_eq!(outline.points.len(), 4);
    assert!((area(&outline.points) - 0.2 * 0.3).abs() < 1e-12);
    // The new outline sits where the shape was, and the shape itself is gone.
    for p in &outline.points {
        let p = on_board(&after, *p);
        assert!(shape_on_board.iter().any(|q| q.distance(p) < 1e-9), "{p}");
    }
    assert!(outline.lines.iter().all(|l| l.id != "window"));
    assert_eq!(outline.holes.len(), 1);
    for (a, b) in before.iter().zip(board(&after)) {
        assert!(a.distance(b) < 1e-9, "{a} moved to {b}");
    }
    after.compile().expect("the reshaped piece compiles");
}

#[test]
fn what_would_end_up_outside_is_refused() {
    let base = piece();
    let lo = lo(&base);
    let id = base.panels[0].id.clone();
    // A line outside the shape.
    let mut t = base.clone();
    t.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .lines
        .push(InternalLine {
            id: "hem_line".into(),
            points: vec![lo + DVec2::new(0.05, 0.45), lo + DVec2::new(0.35, 0.45)],
            tangents: vec![DVec2::ZERO; 2],
            closed: false,
            group: None,
        });
    let refused = replace(&t).unwrap_err();
    assert!(refused.contains("hem_line"), "{refused}");
    // A graphic outside the shape.
    let mut t = base.clone();
    t.graphics.push(GraphicPlacement {
        id: "logo".into(),
        panel_id: id,
        asset_id: "image_1".into(),
        center_m: lo + DVec2::new(0.35, 0.45),
        size_m: DVec2::splat(0.02),
        rotation_degrees: 0.,
        side: GraphicSide::Front,
        opacity: 1.,
    });
    let refused = replace(&t).unwrap_err();
    assert!(refused.contains("logo"), "{refused}");
    // An open line is not a shape.
    let mut t = base.clone();
    t.panels[0].polygon.as_mut().unwrap().lines[0].closed = false;
    assert!(replace(&t).is_err());
}

#[test]
fn a_sewn_outline_is_refused() {
    let mut t = FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .template;
    let polygon = t.panels[0].polygon.as_mut().unwrap();
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    let hi = polygon.points.iter().fold(DVec2::MIN, |m, p| m.max(*p));
    let c = (lo + hi) / 2.;
    polygon.lines.push(InternalLine {
        id: "window".into(),
        points: [(-0.05, -0.05), (0.05, -0.05), (0.05, 0.05), (-0.05, 0.05)]
            .map(|(x, y)| c + DVec2::new(x, y))
            .to_vec(),
        tangents: vec![DVec2::ZERO; 4],
        closed: true,
        group: None,
    });
    let refused = replace(&t).unwrap_err();
    assert!(refused.contains("seams"), "{refused}");
}
