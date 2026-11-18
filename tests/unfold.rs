//! Unfold: a half piece mirrored across a straight boundary into the whole piece, with its
//! curves, markings and internal lines mirrored and the fold boundary gone.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        domain::InternalLine,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
};

/// A lone half piece: a trapezoid with a curved hem, a seam allowance and an internal line.
fn half() -> GarmentTemplate {
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
    // Narrow one corner so the piece is not symmetric on its own.
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    let far = polygon
        .points
        .iter()
        .position(|p| p.distance(lo) > 0.55)
        .unwrap();
    polygon.points[far].x -= 0.08;
    polygon.lines.push(InternalLine {
        id: "dart_line".into(),
        points: vec![lo + DVec2::new(0.05, 0.1), lo + DVec2::new(0.12, 0.2)],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    });
    t
}

fn area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| points[k].perp_dot(points[(k + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.
}

/// The boundary of the half that lies on its left side (smallest x at both ends).
fn left_edge(t: &GarmentTemplate) -> String {
    let polygon = t.panels[0].polygon.as_ref().unwrap();
    let n = polygon.points.len();
    let lo = polygon.points.iter().fold(f64::MAX, |m, p| m.min(p.x));
    (0..n)
        .find(|&k| {
            (polygon.points[k].x - lo).abs() < 1e-9
                && (polygon.points[(k + 1) % n].x - lo).abs() < 1e-9
        })
        .map(|k| polygon.edges[k].clone())
        .expect("a straight left boundary")
}

#[test]
fn a_half_piece_unfolds_into_its_mirror_image() {
    let t = half();
    let before = t.panels[0].polygon.clone().unwrap();
    let edge = left_edge(&t);
    let n = before.points.len();
    let after = authoring::apply(
        &t,
        Command::Unfold {
            panel: t.panels[0].id.clone(),
            edge: edge.clone(),
        },
    )
    .unwrap()
    .template;
    let whole = after.panels[0].polygon.as_ref().unwrap();
    assert_eq!(whole.points.len(), 2 * n - 2);
    assert_eq!(whole.edges.len(), 2 * n - 2);
    assert!(
        !whole.edges.contains(&edge),
        "the fold is inside the piece now"
    );
    assert!(
        (area(&whole.points) - 2. * area(&before.points)).abs() < 1e-9,
        "{} vs twice {}",
        area(&whole.points),
        area(&before.points)
    );
    // The whole piece is symmetric about the old fold line.
    let (a, b) = (whole.points[n - 1], whole.points[0]);
    let d = (b - a).normalize();
    let reflect = |x: DVec2| a + 2. * (x - a).dot(d) * d - (x - a);
    for p in &whole.points {
        assert!(
            whole.points.iter().any(|q| q.distance(reflect(*p)) < 1e-9),
            "{p:?} has no mirror image"
        );
    }
    // Internal lines come in mirrored pairs.
    let line = whole.lines.iter().find(|l| l.id == "dart_line").unwrap();
    let mirrored = whole
        .lines
        .iter()
        .find(|l| l.id == "dart_line_mirror")
        .unwrap();
    for (p, q) in line.points.iter().zip(&mirrored.points) {
        assert!(q.distance(reflect(*p)) < 1e-9);
    }
}

#[test]
fn a_sewn_or_missing_fold_is_refused() {
    let t = half();
    let panel = t.panels[0].id.clone();
    assert!(
        authoring::apply(
            &t,
            Command::Unfold {
                panel: panel.clone(),
                edge: "missing".into()
            }
        )
        .is_err()
    );
    // The tee's front is sewn along its sides: unfolding across a sewn boundary is refused.
    let tee = presets::basic(GarmentKind::Tshirt, 0);
    let sewn = tee.seams[0].a.clone();
    assert!(
        authoring::apply(
            &tee,
            Command::Unfold {
                panel: sewn.panel,
                edge: sewn.edge.unwrap()
            }
        )
        .is_err()
    );
}
