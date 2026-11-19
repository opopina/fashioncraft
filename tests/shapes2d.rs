//! Spiral strips, closed internal shapes with replication, and holes cut from or filled back
//! into internal shapes.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        domain::{self, InternalLine},
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
        shapes2d::{self, InternalShapeKind, Spiral},
    },
};
use glam::DVec2;
use std::f64::consts::TAU;

fn square(size: f64) -> GarmentTemplate {
    authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::splat(size),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.05,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template
}

/// Arclength of r(θ) = r0 + bθ + side·w(θ)/2 between 0 and `end`, by dense sampling.
fn analytic_edge(s: &Spiral, end: f64, side: f64) -> f64 {
    let b = s.offset_m / TAU;
    let r = |t: f64| {
        let w = s.inner_width_m + (s.outer_width_m - s.inner_width_m) * t / end;
        s.radius_m + b * t + side * w * 0.5
    };
    let n = 200_000;
    let p = |t: f64| DVec2::new(r(t) * t.cos(), r(t) * t.sin());
    (0..n)
        .map(|i| p(end * i as f64 / n as f64).distance(p(end * (i + 1) as f64 / n as f64)))
        .sum()
}

fn perimeter(poly: &fashioncraft::garment::polygon::PolygonPattern) -> f64 {
    poly.edges
        .iter()
        .map(|e| authoring::edge_length(poly, e).unwrap())
        .sum()
}

#[test]
fn spiral_follows_the_archimedean_strip() {
    for spiral in [
        Spiral::default(),
        Spiral {
            outer_width_m: 0.14,
            clockwise: false,
            ..Spiral::default()
        },
    ] {
        let end = spiral.end_angle().unwrap();
        let inner = analytic_edge(&spiral, end, -1.);
        let outer = analytic_edge(&spiral, end, 1.);
        assert!((inner - spiral.inner_length_m).abs() < 1e-4, "{inner}");
        assert!((spiral.outer_length().unwrap() - outer).abs() < 1e-4);
        let poly = spiral.polygon(MeshingSettings::default()).unwrap();
        // Two spiral edges plus the two straight end caps (the strip's end widths).
        let expected = inner + outer + spiral.inner_width_m + spiral.outer_width_m;
        let got = perimeter(&poly);
        assert!((got - expected).abs() < 2e-3, "{got} vs {expected}");
        assert!(domain::area(&poly.points) > 0., "counterclockwise outline");
        let lo = poly
            .points
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |m, p| m.min(*p));
        assert!(lo.min_element() > -1e-12, "bounds start at the origin");
    }
}

#[test]
fn spiral_direction_mirrors_the_strip_and_bad_sizes_are_refused() {
    let cw = Spiral::default()
        .polygon(MeshingSettings::default())
        .unwrap();
    let ccw = Spiral {
        clockwise: false,
        ..Spiral::default()
    }
    .polygon(MeshingSettings::default())
    .unwrap();
    assert!((perimeter(&cw) - perimeter(&ccw)).abs() < 1e-9);
    assert_ne!(cw.points, ccw.points);
    for bad in [
        Spiral {
            offset_m: 0.05,
            ..Spiral::default()
        },
        Spiral {
            radius_m: 0.03,
            ..Spiral::default()
        },
        Spiral {
            inner_length_m: f64::NAN,
            ..Spiral::default()
        },
    ] {
        assert!(bad.polygon(MeshingSettings::default()).is_err(), "{bad:?}");
    }
}

#[test]
fn spiral_command_creates_a_panel_that_compiles() {
    let t = square(0.6);
    let edit = authoring::apply(
        &t,
        Command::Spiral {
            spiral: Spiral::default(),
            origin: DVec2::new(1.0, 0.),
            meshing: MeshingSettings {
                target_edge_m: 0.04,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
        },
    )
    .unwrap();
    assert_eq!(edit.template.panels.len(), t.panels.len() + 1);
    assert!(edit.garment.mesh.triangles.len() > t.compile().unwrap().mesh.triangles.len());
}

#[test]
fn internal_shapes_replicate_centre_to_centre() {
    let centres = shapes2d::replicate(DVec2::new(0.2, 0.3), 3, 0.15, 90.);
    assert_eq!(centres.len(), 3);
    assert!(
        (centres[1] - DVec2::new(0.35, 0.3)).length() < 1e-12,
        "90° is to the right"
    );
    let up = shapes2d::replicate(DVec2::new(0.2, 0.3), 2, 0.1, 0.);
    assert!(
        (up[1] - DVec2::new(0.2, 0.2)).length() < 1e-12,
        "0° is up the board"
    );

    let t = square(1.0);
    let id = t.panels[0].id.clone();
    let edit = authoring::apply(
        &t,
        Command::InternalShape {
            panel: id.clone(),
            kind: InternalShapeKind::Rectangle,
            center: DVec2::new(0.237, 0.182),
            size: DVec2::splat(0.1),
            count: 3,
            interval_m: 0.15,
            angle_degrees: 90.,
        },
    )
    .unwrap();
    let lines = &edit.template.panels[0].polygon.as_ref().unwrap().lines;
    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|l| l.closed));
    let left = |l: &InternalLine| l.points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    assert!((left(&lines[1]) - left(&lines[0]) - 0.15).abs() < 1e-9);
    // Overlapping copies cross each other and are refused as a whole.
    assert!(
        authoring::apply(
            &t,
            Command::InternalShape {
                panel: id,
                kind: InternalShapeKind::Rectangle,
                center: DVec2::new(0.3, 0.3),
                size: DVec2::splat(0.1),
                count: 2,
                interval_m: 0.05,
                angle_degrees: 90.,
            },
        )
        .is_err()
    );
}

#[test]
fn internal_ellipse_has_the_ellipse_perimeter() {
    let line = shapes2d::internal_shape(
        "e".into(),
        InternalShapeKind::Ellipse,
        DVec2::new(0.5, 0.5),
        DVec2::new(0.3, 0.2),
    )
    .unwrap();
    let samples = line.samples().unwrap();
    assert_eq!(samples.first(), samples.last(), "a closed loop");
    let length: f64 = samples.windows(2).map(|p| p[0].distance(p[1])).sum();
    let (a, b) = (0.15_f64, 0.1_f64);
    let h = ((a - b) / (a + b)).powi(2);
    let ramanujan = std::f64::consts::PI * (a + b) * (1. + 3. * h / (10. + (4. - 3. * h).sqrt()));
    assert!(
        (length - ramanujan).abs() / ramanujan < 2e-3,
        "{length} vs {ramanujan}"
    );
}

#[test]
fn shapes_become_holes_and_holes_become_shapes() {
    let t = square(1.0);
    let id = t.panels[0].id.clone();
    let with_shape = authoring::apply(
        &t,
        Command::InternalShape {
            panel: id.clone(),
            kind: InternalShapeKind::Ellipse,
            center: DVec2::splat(0.5),
            size: DVec2::new(0.3, 0.2),
            count: 1,
            interval_m: 0.,
            angle_degrees: 0.,
        },
    )
    .unwrap();
    let line = with_shape.template.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .lines[0]
        .id
        .clone();
    let holed = authoring::apply(
        &with_shape.template,
        Command::ConvertToHole {
            panel: id.clone(),
            line,
        },
    )
    .unwrap();
    let poly = holed.template.panels[0].polygon.as_ref().unwrap();
    assert!(poly.lines.is_empty());
    assert_eq!(poly.holes.len(), 1);
    assert!(
        domain::area(&poly.holes[0].points) < 0.,
        "holes run clockwise"
    );
    // The hole removes material: less cloth area than the solid square.
    let area = |g: &fashioncraft::garment::CompiledGarment| g.mesh.rest_area.iter().sum::<f64>();
    assert!(area(&holed.garment) < area(&with_shape.garment) - 0.04);
    let filled = authoring::apply(
        &holed.template,
        Command::ConvertToInternalShape {
            panel: id.clone(),
            hole: 0,
        },
    )
    .unwrap();
    let poly = filled.template.panels[0].polygon.as_ref().unwrap();
    assert!(poly.holes.is_empty() && poly.lines.len() == 1 && poly.lines[0].closed);
    // The refilled rim is a smooth copy of the ellipse: same cloth area within 0.5 %.
    let (a, b) = (area(&filled.garment), area(&with_shape.garment));
    assert!((a - b).abs() / b < 5e-3, "{a} vs {b}");
    // An open line cannot become a hole.
    let open = authoring::apply(
        &t,
        Command::InternalLine {
            panel: id.clone(),
            points: vec![DVec2::splat(0.2), DVec2::splat(0.4)],
            tangents: vec![DVec2::ZERO; 2],
        },
    )
    .unwrap();
    let open_id = open.template.panels[0].polygon.as_ref().unwrap().lines[0]
        .id
        .clone();
    assert!(
        authoring::apply(
            &open.template,
            Command::ConvertToHole {
                panel: id,
                line: open_id,
            },
        )
        .is_err()
    );
}

#[test]
fn rectangular_holes_keep_their_corners_when_filled() {
    let hole = fashioncraft::garment::domain::Hole {
        points: vec![
            DVec2::new(0.2, 0.2),
            DVec2::new(0.2, 0.4),
            DVec2::new(0.5, 0.4),
            DVec2::new(0.5, 0.2),
        ],
        edges: (1..=4).map(|k| format!("h{k}")).collect(),
    };
    let line = shapes2d::hole_to_shape("s".into(), &hole).unwrap();
    assert_eq!(line.points, hole.points);
    assert!(
        line.tangents.iter().all(|t| *t == DVec2::ZERO),
        "sharp corners stay sharp"
    );
    // A dense round rim is thinned to 64 corners and smoothed.
    let round = fashioncraft::garment::domain::Hole {
        points: (0..200)
            .map(|k| {
                let a = -(k as f64) / 200. * std::f64::consts::TAU;
                DVec2::new(0.5 + 0.1 * a.cos(), 0.5 + 0.1 * a.sin())
            })
            .collect(),
        edges: (1..=200).map(|k| format!("r{k}")).collect(),
    };
    let line = shapes2d::hole_to_shape("r".into(), &round).unwrap();
    assert_eq!(line.points.len(), 64);
    assert!(line.tangents.iter().all(|t| *t != DVec2::ZERO));
    let samples = line.samples().unwrap();
    let worst = samples
        .iter()
        .map(|p| (p.distance(DVec2::splat(0.5)) - 0.1).abs())
        .fold(0., f64::max);
    assert!(worst < 5e-4, "stays within 0.5 mm of the circle: {worst}");
}

#[test]
fn closed_flag_is_omitted_for_open_lines() {
    let open = InternalLine {
        id: "l".into(),
        points: vec![DVec2::ZERO, DVec2::X],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    };
    let text = serde_json::to_string(&open).unwrap();
    assert!(!text.contains("closed"));
    let back: InternalLine = serde_json::from_str(&text).unwrap();
    assert!(!back.closed);
}
