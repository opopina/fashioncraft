//! Slash and spread: a piece slashed along a straight internal line and one part (or both)
//! turned about the far end of the slash, opening a wedge; what lies on each part turns with it.
use fashioncraft::{
    authoring::{self, Command, SpreadSide},
    fitting::FittingProject,
    garment::{
        GarmentTemplate,
        domain::InternalLine,
        graphics::{GraphicPlacement, GraphicSide},
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
};
use glam::DVec2;

/// A lone 40 × 60 cm piece with a vertical slash line up its middle, from 20 cm to 40 cm.
fn piece() -> GarmentTemplate {
    let mut t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.4, 0.6),
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
        id: "slash".into(),
        points: vec![lo + DVec2::new(0.2, 0.2), lo + DVec2::new(0.2, 0.4)],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    });
    t
}

fn lo(t: &GarmentTemplate) -> DVec2 {
    let polygon = t.panels[0].polygon.as_ref().unwrap();
    polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p))
}

fn area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| points[k].perp_dot(points[(k + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.
}

/// Where a material point of the piece lies on the 2D board.
fn on_board(t: &GarmentTemplate, p: DVec2) -> DVec2 {
    let piece = &t.panels[0];
    piece.layout_2d.unwrap_or_default() + authoring::rotate_2d(p, piece.layout_rotation_degrees)
}

fn spread(t: &GarmentTemplate, degrees: f64, side: SpreadSide) -> Result<GarmentTemplate, String> {
    authoring::apply(
        t,
        Command::SlashSpread {
            panel: t.panels[0].id.clone(),
            line: "slash".into(),
            degrees,
            side,
        },
    )
    .map(|e| e.template)
}

#[test]
fn the_wedge_opens_by_the_angle_and_adds_its_area() {
    let t = piece();
    let before = area(&t.panels[0].polygon.as_ref().unwrap().points);
    for side in [SpreadSide::Left, SpreadSide::Right, SpreadSide::Both] {
        let after = spread(&t, 10., side).unwrap();
        let outline = after.panels[0].polygon.as_ref().unwrap();
        // The slash runs 60 cm from edge to edge; the wedge of 10° adds ½·0.6²·sin 10°.
        let wedge = 0.5 * 0.6 * 0.6 * 10f64.to_radians().sin();
        assert!(
            (area(&outline.points) - (before + wedge)).abs() < 1e-9,
            "{side:?}: {} vs {}",
            area(&outline.points),
            before + wedge
        );
        // The slash line is gone and the wedge's mouth is a new boundary.
        assert!(outline.lines.iter().all(|l| l.id != "slash"));
        assert_eq!(outline.points.len(), 4 + 3);
        after.compile().expect("the spread piece compiles");
    }
}

#[test]
fn what_lies_on_a_part_turns_with_it() {
    let mut t = piece();
    let id = t.panels[0].id.clone();
    let lo = lo(&t);
    // A graphic on the left part and one on the right part, low down near the wedge.
    for (name, x) in [("left", 0.1), ("right", 0.3)] {
        t.graphics.push(GraphicPlacement {
            id: name.into(),
            panel_id: id.clone(),
            asset_id: "image_1".into(),
            center_m: lo + DVec2::new(x, 0.1),
            size_m: DVec2::splat(0.04),
            rotation_degrees: 0.,
            side: GraphicSide::Front,
            opacity: 1.,
        });
    }
    let before: Vec<DVec2> = t
        .graphics
        .iter()
        .map(|g| on_board(&t, g.center_m))
        .collect();
    let after = spread(&t, 20., SpreadSide::Right).unwrap();
    let moved: Vec<f64> = after
        .graphics
        .iter()
        .zip(&before)
        .map(|(g, b)| on_board(&after, g.center_m).distance(*b))
        .collect();
    let turned: Vec<f64> = after.graphics.iter().map(|g| g.rotation_degrees).collect();
    // The board shows material y downwards: walking the slash from its first point (higher
    // up) to its second, the right-hand part is the one at smaller x. Only its graphic moves,
    // and it turns by the angle.
    let (still, moving) = (1, 0);
    assert!(moved[still] < 1e-9 && moved[moving] > 0.01, "{moved:?}");
    assert_eq!(turned[still], 0.);
    let turn = turned[moving].rem_euclid(360.);
    assert!(
        (turn - 20.).abs() < 1e-9 || (turn - 340.).abs() < 1e-9,
        "{turn}"
    );
    after.compile().expect("the spread piece compiles");
}

#[test]
fn bad_angles_and_lines_are_refused() {
    let t = piece();
    for degrees in [0., f64::NAN, 120.] {
        assert!(spread(&t, degrees, SpreadSide::Right).is_err(), "{degrees}");
    }
    let mut missing = t.clone();
    missing.panels[0].polygon.as_mut().unwrap().lines.clear();
    assert!(spread(&missing, 10., SpreadSide::Right).is_err());
}

#[test]
fn seams_on_the_slashed_boundaries_follow_their_parts() {
    // On the tee's front, a slash from the hem up: the side seams stay whole and closed.
    let mut t = FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .template;
    let polygon = t.panels[0].polygon.as_mut().unwrap();
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    let hi = polygon.points.iter().fold(DVec2::MIN, |m, p| m.max(*p));
    let x = (lo.x + hi.x) / 2. + 0.05;
    // From near the hem towards the chest: the wedge opens at the hem.
    polygon.lines.push(InternalLine {
        id: "slash".into(),
        points: vec![DVec2::new(x, hi.y - 0.05), DVec2::new(x, hi.y - 0.25)],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    });
    let seams = t.seams.len();
    let after = authoring::apply(
        &t,
        Command::SlashSpread {
            panel: t.panels[0].id.clone(),
            line: "slash".into(),
            degrees: 8.,
            side: SpreadSide::Both,
        },
    )
    .map(|e| e.template);
    let after = after.expect("the front opens at the hem");
    assert!(after.seams.len() >= seams);
    for s in &after.seams {
        for r in s.references() {
            let p = after.panels.iter().find(|p| p.id == r.panel).unwrap();
            assert!(
                p.polygon
                    .as_ref()
                    .unwrap()
                    .edges
                    .contains(r.edge.as_ref().unwrap()),
                "{:?}",
                r.edge
            );
        }
    }
}
