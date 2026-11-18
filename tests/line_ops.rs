//! Internal line operations: carry an end on to the outline or the nearest line, and split a
//! line in two at one of its inner points.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        domain::InternalLine,
        lines2d::OUTLINE_GAP_M,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
};

/// A 300 × 500 mm rectangle with a short horizontal line and a three-point line in it.
fn sheet() -> GarmentTemplate {
    let mut t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.3, 0.5),
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
        id: "waist".into(),
        points: vec![lo + DVec2::new(0.1, 0.25), lo + DVec2::new(0.2, 0.25)],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    });
    polygon.lines.push(InternalLine {
        id: "zigzag".into(),
        points: vec![
            lo + DVec2::new(0.05, 0.05),
            lo + DVec2::new(0.1, 0.1),
            lo + DVec2::new(0.15, 0.05),
        ],
        tangents: vec![DVec2::ZERO; 3],
        closed: false,
        group: None,
    });
    t
}

fn line<'a>(t: &'a GarmentTemplate, id: &str) -> &'a InternalLine {
    t.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .find(|l| l.id == id)
        .unwrap()
}

#[test]
fn both_ends_reach_the_outline_just_short_of_it() {
    let t = sheet();
    let panel = t.panels[0].id.clone();
    let lo = t.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .points
        .iter()
        .fold(DVec2::MAX, |m, p| m.min(*p));
    let mut t2 = t.clone();
    for at_start in [true, false] {
        t2 = authoring::apply(
            &t2,
            Command::ExtendLine {
                panel: panel.clone(),
                line: "waist".into(),
                at_start,
            },
        )
        .unwrap()
        .template;
    }
    let l = line(&t2, "waist");
    let width = 0.3;
    assert!(
        (l.points[0].x - (lo.x + OUTLINE_GAP_M)).abs() < 1e-9,
        "{:?}",
        l.points
    );
    assert!((l.points[1].x - (lo.x + width - OUTLINE_GAP_M)).abs() < 1e-9);
    assert!(
        (l.points[0].y - l.points[1].y).abs() < 1e-12,
        "still straight"
    );
    // A second extension has nothing left to cover.
    assert!(
        authoring::apply(
            &t2,
            Command::ExtendLine {
                panel,
                line: "waist".into(),
                at_start: true
            }
        )
        .is_err()
    );
}

#[test]
fn a_line_is_split_at_an_inner_point() {
    let t = sheet();
    let panel = t.panels[0].id.clone();
    let before = line(&t, "zigzag").clone();
    let t2 = authoring::apply(
        &t,
        Command::SplitLine {
            panel: panel.clone(),
            line: "zigzag".into(),
            point: 1,
        },
    )
    .unwrap()
    .template;
    let first = line(&t2, "zigzag");
    let second = line(&t2, "zigzag_2");
    let half_gap = OUTLINE_GAP_M / 2.;
    assert_eq!(first.points[0], before.points[0]);
    assert!((first.points[1].distance(before.points[1]) - half_gap).abs() < 1e-12);
    assert!((second.points[0].distance(before.points[1]) - half_gap).abs() < 1e-12);
    assert_eq!(second.points[1], before.points[2]);
    assert_eq!(
        first.points.len() + second.points.len(),
        before.points.len() + 1
    );
    for bad in [0, 2, 7] {
        assert!(
            authoring::apply(
                &t,
                Command::SplitLine {
                    panel: panel.clone(),
                    line: "zigzag".into(),
                    point: bad
                }
            )
            .is_err(),
            "{bad}"
        );
    }
}
