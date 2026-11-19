//! Internal lines from a piece's geometry: boundary and outline offsets, lines leaving a
//! boundary at an angle, and the clean-up of overlapping and straight-through points.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate, domain,
        lines2d::{self, OUTLINE_GAP_M},
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        presets::{self, GarmentKind},
    },
};
use glam::DVec2;

fn piece(points: &[[f64; 2]]) -> GarmentTemplate {
    authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.iter().map(|p| DVec2::from_array(*p)).collect(),
            tangents: vec![DVec2::ZERO; points.len()],
            meshing: MeshingSettings {
                target_edge_m: 0.04,
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

/// Boundary from `a` to `b` (as drawn), by its end points.
fn edge(t: &GarmentTemplate, a: [f64; 2], b: [f64; 2]) -> String {
    let p = polygon(t);
    let n = p.points.len();
    let (a, b) = (DVec2::from_array(a), DVec2::from_array(b));
    (0..n)
        .find(|&k| p.points[k].distance(a) < 1e-9 && p.points[(k + 1) % n].distance(b) < 1e-9)
        .map(|k| p.edges[k].clone())
        .expect("boundary")
}

fn rectangle() -> GarmentTemplate {
    // Counterclockwise in (u, v): along the top (v = 0), down the right side, back along the
    // bottom (v = 0.3) and up the left side.
    piece(&[[0., 0.], [0.4, 0.], [0.4, 0.3], [0., 0.3]])
}

#[test]
fn boundary_offsets_run_parallel_into_the_piece() {
    let t = rectangle();
    let top = edge(&t, [0., 0.], [0.4, 0.]);
    let lines = lines2d::offset_lines(polygon(&t), &top, 2, 0.05, false, false).unwrap();
    assert_eq!(lines.len(), 2);
    for (k, line) in lines.iter().enumerate() {
        let v = 0.05 * (k + 1) as f64;
        assert_eq!(line.len(), 2, "a straight boundary gives a straight line");
        assert!(
            line.iter().all(|p| (p.y - v).abs() < 1e-12),
            "{v} into the piece"
        );
        // Same extent as the boundary, stopping just short of the sides.
        assert!((line[0].x - OUTLINE_GAP_M).abs() < 1e-9);
        assert!((line[1].x - (0.4 - OUTLINE_GAP_M)).abs() < 1e-9);
    }
    // The other side is outside the piece.
    assert!(lines2d::offset_lines(polygon(&t), &top, 1, 0.05, true, false).is_err());
    // Too far: past the opposite side.
    assert!(lines2d::offset_lines(polygon(&t), &top, 1, 0.35, false, false).is_err());

    // Through the command: valid internal lines that compile.
    let edit = authoring::apply(
        &t,
        Command::OffsetAsLines {
            panel: t.panels[0].id.clone(),
            path: Some(top),
            count: 3,
            distance_m: 0.05,
            reverse: false,
            extend: true,
        },
    )
    .unwrap();
    assert_eq!(polygon(&edit.template).lines.len(), 3);
}

#[test]
fn slanted_offsets_are_cut_or_extended_at_the_outline() {
    // A quadrilateral with a slanted left side, like the one measured by hand.
    let t = piece(&[[0.03, 0.], [0.25, 0.01], [0.25, 0.26], [0., 0.26]]);
    let p = polygon(&t);
    let left = edge(&t, [0., 0.26], [0.03, 0.]);
    let outline = p.outer_points().unwrap();
    let distance_to_outline = |q: DVec2| {
        (0..outline.len())
            .map(|i| {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
                let t = ((q - a).dot(b - a) / (b - a).length_squared()).clamp(0., 1.);
                q.distance(a + (b - a) * t)
            })
            .fold(f64::INFINITY, f64::min)
    };
    let extended = lines2d::offset_lines(p, &left, 1, 0.04, false, true).unwrap();
    let line = &extended[0];
    for end in [line[0], *line.last().unwrap()] {
        let d = distance_to_outline(end);
        assert!((d - OUTLINE_GAP_M).abs() < 2e-4, "ends at the outline: {d}");
    }
    // Every point is 40 mm from the slanted side.
    let (a, b) = (DVec2::new(0., 0.26), DVec2::new(0.03, 0.));
    for q in line {
        let d = (*q - a).perp_dot(b - a).abs() / (b - a).length();
        assert!((d - 0.04).abs() < 1e-9, "{d}");
    }
    // Without extending, the copy keeps the boundary's extent but is cut where it would leave.
    let plain = lines2d::offset_lines(p, &left, 1, 0.04, false, false).unwrap();
    let span = |l: &Vec<DVec2>| l[0].distance(*l.last().unwrap());
    assert!(span(&plain[0]) <= a.distance(b) + 1e-9);
    assert!(span(&plain[0]) < span(line));
}

#[test]
fn outline_offsets_are_inset_shapes() {
    let t = rectangle();
    let rings = lines2d::offset_outline(polygon(&t), 2, 0.02).unwrap();
    for (k, ring) in rings.iter().enumerate() {
        let d = 0.02 * (k + 1) as f64;
        let area = domain::area(ring).abs();
        assert!(
            (area - (0.4 - 2. * d) * (0.3 - 2. * d)).abs() < 1e-12,
            "{area}"
        );
    }
    let edit = authoring::apply(
        &t,
        Command::OffsetAsLines {
            panel: t.panels[0].id.clone(),
            path: None,
            count: 1,
            distance_m: 0.02,
            reverse: false,
            extend: false,
        },
    )
    .unwrap();
    let shape = &polygon(&edit.template).lines[0];
    assert!(
        shape.closed && shape.points.len() == 4,
        "an exact inset rectangle"
    );
    assert!(
        lines2d::offset_outline(polygon(&t), 1, 0.2).is_err(),
        "too deep"
    );
}

#[test]
fn perpendicular_lines_leave_the_boundary_at_the_angle() {
    let t = rectangle();
    let top = edge(&t, [0., 0.], [0.4, 0.]);
    let p = polygon(&t);
    // From the middle, straight across to the bottom.
    let [a, b] = lines2d::perpendicular_line(p, &top, 0.2, 90., None, false).unwrap();
    assert!((a - DVec2::new(0.2, OUTLINE_GAP_M)).length() < 1e-9);
    assert!((b - DVec2::new(0.2, 0.3 - OUTLINE_GAP_M)).length() < 1e-9);
    // A fixed length.
    let [_, b] = lines2d::perpendicular_line(p, &top, 0.1, 90., Some(0.1), false).unwrap();
    assert!((b.y - (OUTLINE_GAP_M + 0.1)).abs() < 1e-9);
    // 45° leans towards the boundary's end and stops at the first side it meets.
    let [a, b] = lines2d::perpendicular_line(p, &top, 0.3, 45., None, false).unwrap();
    let d = (b - a).normalize();
    assert!((d - DVec2::new(1., 1.).normalize()).length() < 1e-9);
    // The gap is measured along the line, so across the side it is gap·cos 45°.
    let across = 0.4 - b.x;
    assert!(
        (across - OUTLINE_GAP_M * 0.5_f64.sqrt()).abs() < 1e-9,
        "{b}"
    );
    // Too long, outside the boundary or a flat angle are refused.
    assert!(lines2d::perpendicular_line(p, &top, 0.2, 90., Some(0.5), false).is_err());
    assert!(lines2d::perpendicular_line(p, &top, 0.5, 90., None, false).is_err());
    assert!(lines2d::perpendicular_line(p, &top, 0.2, 0., None, false).is_err());
    let edit = authoring::apply(
        &t,
        Command::PerpendicularLine {
            panel: t.panels[0].id.clone(),
            path: top,
            at_m: 0.2,
            angle_degrees: 90.,
            length_m: None,
            reverse: false,
        },
    )
    .unwrap();
    assert_eq!(polygon(&edit.template).lines.len(), 1);

    // An internal line in the way stops it, just short, since lines may not cross.
    let bottom = edge(&t, [0.4, 0.3], [0., 0.3]);
    let with_line = authoring::apply(
        &t,
        Command::OffsetAsLines {
            panel: t.panels[0].id.clone(),
            path: Some(bottom),
            count: 1,
            distance_m: 0.1,
            reverse: false,
            extend: true,
        },
    )
    .unwrap()
    .template;
    let top = edge(&with_line, [0., 0.], [0.4, 0.]);
    let [_, b] =
        lines2d::perpendicular_line(polygon(&with_line), &top, 0.2, 90., None, false).unwrap();
    assert!((b.y - (0.2 - OUTLINE_GAP_M)).abs() < 1e-9, "{b}");
    let edit = authoring::apply(
        &with_line,
        Command::PerpendicularLine {
            panel: t.panels[0].id.clone(),
            path: top,
            at_m: 0.2,
            angle_degrees: 90.,
            length_m: None,
            reverse: false,
        },
    );
    assert!(edit.is_ok(), "{:?}", edit.err());
}

#[test]
fn clean_up_removes_overlapping_and_straight_through_points() {
    // A 0.05 mm boundary at the top-right corner and a point halfway along the bottom.
    let t = piece(&[
        [0., 0.],
        [0.4, 0.],
        [0.4, 0.00005],
        [0.4, 0.3],
        [0.2, 0.3],
        [0., 0.3],
    ]);
    let id = t.panels[0].id.clone();
    let clean = |t: &GarmentTemplate, straight: Option<f64>| {
        authoring::apply(
            t,
            Command::CleanUpPoints {
                panels: vec![id.clone()],
                overlap_m: 0.0001,
                straight_degrees: straight,
            },
        )
    };
    let overlap_only = clean(&t, None).unwrap();
    assert_eq!(polygon(&overlap_only.template).points.len(), 5);
    let both = clean(&t, Some(2.)).unwrap();
    let p = polygon(&both.template);
    assert_eq!(p.points.len(), 4, "a plain rectangle is left");
    // The corner moves by less than the 0.1 mm overlap distance.
    assert!((domain::area(&p.points).abs() - 0.12).abs() < 0.4 * 0.0001);
    // Nothing left to clean is reported.
    assert!(clean(&both.template, Some(2.)).is_err());

    // A sewn boundary keeps its points.
    let mut sewn = t.clone();
    let tiny = edge(&t, [0.4, 0.], [0.4, 0.00005]);
    sewn.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .target_lengths
        .insert(tiny, 0.00005);
    let kept = clean(&sewn, None);
    assert!(kept.is_err(), "the only overlap is protected");

    // Internal line anchors on top of each other go too.
    let mut line = lines2d::line_from_polyline(
        "l".into(),
        &[
            DVec2::new(0.1, 0.1),
            DVec2::new(0.2, 0.1),
            DVec2::new(0.2, 0.15),
        ],
    )
    .unwrap();
    line.points.insert(2, DVec2::new(0.20004, 0.1));
    line.tangents.insert(2, DVec2::ZERO);
    assert_eq!(lines2d::removable_line_points(&line, 0.0001), vec![2]);
}

#[test]
fn open_lines_are_thinned_and_smoothed() {
    let arc: Vec<DVec2> = (0..=200)
        .map(|k| {
            let a = std::f64::consts::PI * k as f64 / 200.;
            DVec2::new(0.5 + 0.2 * a.cos(), 0.5 - 0.2 * a.sin())
        })
        .collect();
    let line = lines2d::line_from_polyline("a".into(), &arc).unwrap();
    assert_eq!(line.points.len(), 64);
    assert_eq!(line.points[0], arc[0]);
    assert_eq!(*line.points.last().unwrap(), arc[200]);
    let worst = line
        .samples()
        .unwrap()
        .iter()
        .map(|p| (p.distance(DVec2::splat(0.5)) - 0.2).abs())
        .fold(0., f64::max);
    assert!(worst < 5e-4, "{worst}");
    let straight = lines2d::line_from_polyline(
        "s".into(),
        &[DVec2::ZERO, DVec2::new(0.1, 0.), DVec2::new(0.2, 0.)],
    )
    .unwrap();
    assert_eq!(straight.points.len(), 2, "collinear points are dropped");
}
