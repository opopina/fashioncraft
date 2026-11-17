//! Fewer curve points for the same curve, and groups of internal lines.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate, curvefit,
        domain::{self, InternalLine},
        lines2d,
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        presets::{self, GarmentKind},
    },
};
use std::f64::consts::TAU;

/// A drafted piece through `points` with Catmull–Rom pen tangents where `smooth` says so.
fn piece(points: &[DVec2], smooth: impl Fn(usize) -> bool) -> GarmentTemplate {
    let n = points.len();
    let tangents = (0..n)
        .map(|i| {
            if smooth(i) {
                (points[(i + 1) % n] - points[(i + n - 1) % n]) / 6.
            } else {
                DVec2::ZERO
            }
        })
        .collect();
    authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.to_vec(),
            tangents,
            meshing: MeshingSettings {
                target_edge_m: 0.03,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template
}

fn polygon(t: &GarmentTemplate) -> &PolygonPattern {
    t.panels[0].polygon.as_ref().unwrap()
}

/// Largest distance from the flattened outline of `b`'s first piece to `a`'s, on the board
/// (editing re-bases material coordinates, the board position stays).
fn board_deviation(a: &GarmentTemplate, b: &GarmentTemplate) -> f64 {
    let shift = |t: &GarmentTemplate| t.panels[0].layout_2d.unwrap_or_default();
    let mut moved = polygon(b).clone();
    moved.translate_material(shift(b) - shift(a));
    deviation(polygon(a), &moved)
}

/// Largest distance from the flattened outline `b` to the flattened outline `a`.
fn deviation(a: &PolygonPattern, b: &PolygonPattern) -> f64 {
    let ring = a.outer_points().unwrap();
    let distance = |q: DVec2| {
        (0..ring.len())
            .map(|i| {
                let (p, r) = (ring[i], ring[(i + 1) % ring.len()]);
                let t = ((q - p).dot(r - p) / (r - p).length_squared()).clamp(0., 1.);
                q.distance(p + (r - p) * t)
            })
            .fold(f64::INFINITY, f64::min)
    };
    b.outer_points()
        .unwrap()
        .into_iter()
        .map(distance)
        .fold(0., f64::max)
}

fn optimize(t: &GarmentTemplate, ratio: f64) -> Result<GarmentTemplate, String> {
    authoring::apply(
        t,
        Command::OptimizeCurvePoints {
            panels: vec![t.panels[0].id.clone()],
            ratio,
        },
    )
    .map(|e| e.template)
}

#[test]
fn a_single_cubic_is_recovered() {
    let c = [
        DVec2::ZERO,
        DVec2::new(0.1, 0.2),
        DVec2::new(0.3, 0.25),
        DVec2::new(0.4, 0.),
    ];
    let samples: Vec<DVec2> = (0..=60)
        .map(|k| {
            let t = k as f64 / 60.;
            let u = 1. - t;
            c[0] * (u * u * u)
                + c[1] * (3. * u * u * t)
                + c[2] * (3. * u * t * t)
                + c[3] * (t * t * t)
        })
        .collect();
    let (fit, error) = curvefit::fit_cubic(&samples, c[1] - c[0], c[3] - c[2]);
    assert!(error < 1e-5, "{error}");
    assert!(
        fit[1].distance(c[1]) < 1e-3 && fit[2].distance(c[2]) < 1e-3,
        "{fit:?}"
    );
}

#[test]
fn a_dense_circle_keeps_its_shape_with_a_quarter_of_its_points() {
    let points: Vec<DVec2> = (0..48)
        .map(|k| {
            let a = TAU * k as f64 / 48.;
            DVec2::new(0.3 + 0.2 * a.cos(), 0.3 + 0.2 * a.sin())
        })
        .collect();
    let t = piece(&points, |_| true);
    let after = optimize(&t, 0.25).unwrap();
    let (a, b) = (polygon(&t), polygon(&after));
    assert_eq!(b.points.len(), 12);
    let worst = board_deviation(&t, &after).max(board_deviation(&after, &t));
    assert!(worst < 2e-4, "within 0.2 mm: {worst}");
    let area = |p: &PolygonPattern| domain::area(&p.outer_points().unwrap()).abs();
    assert!((area(a) - area(b)).abs() / area(a) < 1e-3);
}

#[test]
fn corners_stay_and_the_suggestion_keeps_the_shape() {
    // A 300 x 300 piece whose right side bulges 40 mm through 11 curve points.
    let mut points = vec![DVec2::new(0., 0.), DVec2::new(0.3, 0.)];
    for k in 1..12 {
        let v = 0.025 * k as f64;
        points.push(DVec2::new(
            0.3 + 0.04 * (std::f64::consts::PI * k as f64 / 12.).sin(),
            v,
        ));
    }
    points.push(DVec2::new(0.3, 0.3));
    points.push(DVec2::new(0., 0.3));
    let t = piece(&points, |i| (2..=12).contains(&i));
    let p = polygon(&t);
    let ratio = curvefit::suggest_ratio(p, |_| false, 5e-4).unwrap();
    assert!(ratio < 0.5, "most curve points can go: {ratio}");
    let after = optimize(&t, ratio).unwrap();
    let q = polygon(&after);
    assert!(board_deviation(&t, &after).max(board_deviation(&after, &t)) < 5e-4);
    for corner in [[0., 0.], [0.3, 0.], [0.3, 0.3], [0., 0.3]] {
        assert!(
            q.points
                .iter()
                .any(|x| x.distance(DVec2::from_array(corner)) < 1e-9),
            "corner {corner:?} stays"
        );
    }
    assert_eq!(q.points.len(), 4 + (11. * ratio).round() as usize);
    // 0 % leaves only the corners; nothing more to do afterwards.
    let bare = optimize(&t, 0.).unwrap();
    assert_eq!(polygon(&bare).points.len(), 4);
    assert!(optimize(&bare, 0.).is_err());
}

#[test]
fn protected_boundaries_keep_their_points() {
    let points: Vec<DVec2> = (0..24)
        .map(|k| {
            let a = TAU * k as f64 / 24.;
            DVec2::new(0.3 + 0.2 * a.cos(), 0.3 + 0.2 * a.sin())
        })
        .collect();
    let mut t = piece(&points, |_| true);
    let p = t.panels[0].polygon.as_mut().unwrap();
    let edge = p.edges[5].clone();
    let length = authoring::edge_length(p, &edge).unwrap();
    p.target_lengths.insert(edge.clone(), length);
    let after = optimize(&t, 0.).unwrap();
    let q = polygon(&after);
    assert!(q.points.len() < 24, "the rest is optimized");
    // Its two points and its curve stay, so it keeps its length exactly.
    let kept = authoring::edge_length(q, &edge).expect("the boundary survives");
    assert!((kept - length).abs() < 1e-12);
}

#[test]
fn curved_internal_lines_lose_anchors_but_not_their_shape() {
    let arc: Vec<DVec2> = (0..=40)
        .map(|k| {
            let a = std::f64::consts::PI * k as f64 / 40.;
            DVec2::new(0.5 + 0.2 * a.cos(), 0.5 - 0.2 * a.sin())
        })
        .collect();
    let mut line = lines2d::line_from_polyline("a".into(), &arc).unwrap();
    let before = line.points.len();
    curvefit::optimize_line(&mut line, 0.2).unwrap();
    assert!(
        line.points.len() < before / 3,
        "{} of {before}",
        line.points.len()
    );
    let worst = line
        .samples()
        .unwrap()
        .iter()
        .map(|p| (p.distance(DVec2::splat(0.5)) - 0.2).abs())
        .fold(0., f64::max);
    assert!(worst < 2e-3, "{worst}");
    // A straight polyline has no curve anchors and is left alone.
    let mut straight = InternalLine {
        id: "s".into(),
        points: vec![DVec2::ZERO, DVec2::X * 0.1, DVec2::new(0.1, 0.1)],
        tangents: vec![DVec2::ZERO; 3],
        closed: false,
        group: None,
    };
    curvefit::optimize_line(&mut straight, 0.).unwrap();
    assert_eq!(straight.points.len(), 3);
}

#[test]
fn internal_lines_group_and_ungroup() {
    let t = piece(
        &[
            DVec2::ZERO,
            DVec2::new(0.6, 0.),
            DVec2::new(0.6, 0.6),
            DVec2::new(0., 0.6),
        ],
        |_| false,
    );
    let id = t.panels[0].id.clone();
    let mut t = t;
    for y in [0.1, 0.2, 0.3] {
        t = authoring::apply(
            &t,
            Command::InternalLine {
                panel: id.clone(),
                points: vec![DVec2::new(0.1, y), DVec2::new(0.5, y)],
                tangents: vec![DVec2::ZERO; 2],
            },
        )
        .unwrap()
        .template;
    }
    let ids: Vec<String> = polygon(&t).lines.iter().map(|l| l.id.clone()).collect();
    let group = |t: &GarmentTemplate| -> Vec<Option<String>> {
        polygon(t).lines.iter().map(|l| l.group.clone()).collect()
    };
    let two = authoring::apply(
        &t,
        Command::GroupLines {
            panel: id.clone(),
            lines: ids[..2].to_vec(),
        },
    )
    .unwrap()
    .template;
    let g = group(&two);
    assert!(g[0].is_some() && g[0] == g[1] && g[2].is_none());
    // Grouping the third with one member joins the existing group.
    let three = authoring::apply(
        &two,
        Command::GroupLines {
            panel: id.clone(),
            lines: vec![ids[1].clone(), ids[2].clone()],
        },
    )
    .unwrap()
    .template;
    let g = group(&three);
    assert!(g.iter().all(|x| *x == g[0] && x.is_some()));
    let text = serde_json::to_string(&polygon(&three).lines[0]).unwrap();
    assert!(text.contains("\"group\""));
    let none = authoring::apply(
        &three,
        Command::UngroupLines {
            panel: id.clone(),
            line: ids[0].clone(),
        },
    )
    .unwrap()
    .template;
    assert!(group(&none).iter().all(Option::is_none));
    assert!(
        !serde_json::to_string(&polygon(&none).lines[0])
            .unwrap()
            .contains("group")
    );
    assert!(
        authoring::apply(
            &t,
            Command::GroupLines {
                panel: id,
                lines: vec![ids[0].clone()],
            },
        )
        .is_err()
    );
}
