//! Segment editing on the 2D board: changing a boundary's length from either end or both,
//! matching two lengths, laying one piece's boundary along another's, and copying closed
//! internal shapes as pieces and pieces as internal shapes.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{
        self, Command,
        arrange2d::{self, SegmentEnd},
    },
    garment::{
        GarmentTemplate,
        domain::{self, InternalLine},
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        presets::{self, GarmentKind},
        shapes::Primitive,
        shapes2d::{self, InternalShapeKind},
    },
};

fn meshing() -> MeshingSettings {
    MeshingSettings {
        target_edge_m: 0.05,
        ..Default::default()
    }
}

fn rectangle(t: &GarmentTemplate, size: DVec2, origin: DVec2, replace: bool) -> GarmentTemplate {
    authoring::apply(
        t,
        Command::Primitive {
            kind: Primitive::Rectangle,
            size,
            sweep_degrees: 180.,
            origin,
            meshing: meshing(),
            fabric: "woven-reference".into(),
            replace,
        },
    )
    .unwrap()
    .template
}

fn base(size: DVec2) -> GarmentTemplate {
    rectangle(
        &presets::basic(GarmentKind::Tshirt, 0),
        size,
        DVec2::ZERO,
        true,
    )
}

fn polygon(t: &GarmentTemplate, index: usize) -> &PolygonPattern {
    t.panels[index].polygon.as_ref().unwrap()
}

/// Board positions of boundary `k`'s start and end.
fn ends(t: &GarmentTemplate, index: usize, k: usize) -> [DVec2; 2] {
    let p = polygon(t, index);
    let n = p.points.len();
    [
        arrange2d::board_point(t, index, p.points[k]),
        arrange2d::board_point(t, index, p.points[(k + 1) % n]),
    ]
}

/// The boundary along the top of a rectangle (smallest v).
fn top_edge(t: &GarmentTemplate, index: usize) -> usize {
    let p = polygon(t, index);
    let n = p.points.len();
    (0..n)
        .find(|&k| p.points[k].y.abs() < 1e-12 && p.points[(k + 1) % n].y.abs() < 1e-12)
        .unwrap()
}

fn ring_area(p: &PolygonPattern) -> f64 {
    let ring: Vec<DVec2> = (0..p.points.len())
        .flat_map(|k| {
            let s = p.edge_points(k).unwrap();
            s[..s.len() - 1].to_vec()
        })
        .collect();
    domain::area(&ring).abs()
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn set_length_moves_the_chosen_end_and_keeps_curves() {
    let t = base(DVec2::new(0.4, 0.3));
    let k = top_edge(&t, 0);
    let edge = polygon(&t, 0).edges[k].clone();
    let [a, b] = ends(&t, 0, k);
    for (moving, fixed) in [
        (SegmentEnd::End, a),
        (SegmentEnd::Start, b),
        (SegmentEnd::Center, (a + b) * 0.5),
    ] {
        let edit = authoring::apply(
            &t,
            Command::SetLength {
                panel: t.panels[0].id.clone(),
                edge: edge.clone(),
                length_m: 0.5,
                moving,
            },
        )
        .unwrap();
        let after = &edit.template;
        let length = authoring::edge_length(polygon(after, 0), &edge).unwrap();
        assert!((length - 0.5).abs() < 1e-9, "{moving:?}: {length}");
        let [a2, b2] = ends(after, 0, k);
        let still = match moving {
            SegmentEnd::End => a2,
            SegmentEnd::Start => b2,
            SegmentEnd::Center => (a2 + b2) * 0.5,
        };
        assert!(
            close(still, fixed),
            "{moving:?} keeps the other end where it was"
        );
        assert!(((b2 - a2).normalize() - (b - a).normalize()).length() < 1e-9);
    }

    // A curved boundary keeps its shape: the same bulge, scaled.
    let mut curved = polygon(&t, 0).clone();
    let n = curved.points.len();
    let (p0, p1) = (curved.points[k], curved.points[(k + 1) % n]);
    let out = DVec2::new(0., -0.08);
    curved.curves.insert(
        edge.clone(),
        [p0 + (p1 - p0) / 3. + out, p0 + (p1 - p0) * (2. / 3.) + out],
    );
    let t = authoring::apply(
        &t,
        Command::Shape {
            panel: t.panels[0].id.clone(),
            polygon: curved,
        },
    )
    .unwrap()
    .template;
    let before = authoring::edge_length(polygon(&t, 0), &edge).unwrap();
    let edit = authoring::apply(
        &t,
        Command::SetLength {
            panel: t.panels[0].id.clone(),
            edge: edge.clone(),
            length_m: before * 0.8,
            moving: SegmentEnd::Center,
        },
    )
    .unwrap();
    let p = polygon(&edit.template, 0);
    let after = authoring::edge_length(p, &edge).unwrap();
    assert!(
        (after - before * 0.8).abs() < 1e-6,
        "{after} vs {}",
        before * 0.8
    );
    let [c0, c1] = p.curves[&edge];
    let (q0, q1) = (p.points[k], p.points[(k + 1) % n]);
    let sag = |c: DVec2, a: DVec2, b: DVec2| (c - a).perp_dot(b - a).abs() / (b - a).length();
    // Same proportions: the bulge over the chord is unchanged (the chord was 0.4 m).
    let chord = q0.distance(q1);
    let (s0, s1) = (sag(c0, q0, q1) / chord, sag(c1, q0, q1) / chord);
    assert!(
        (s0 - 0.2).abs() < 1e-9 && (s1 - 0.2).abs() < 1e-9,
        "the bulge scales with the boundary: {s0} {s1}"
    );
    assert!((chord / 0.4 - 0.8).abs() < 1e-3);

    for bad in [0., f64::NAN, 60.] {
        assert!(
            authoring::apply(
                &t,
                Command::SetLength {
                    panel: t.panels[0].id.clone(),
                    edge: edge.clone(),
                    length_m: bad,
                    moving: SegmentEnd::End,
                },
            )
            .is_err()
        );
    }
}

#[test]
fn matched_length_copies_the_shorter_or_the_longer() {
    assert_eq!(arrange2d::matched_length([0.3, 0.26], true), Some((1, 0.3)));
    assert_eq!(
        arrange2d::matched_length([0.3, 0.26], false),
        Some((0, 0.26))
    );
    assert_eq!(arrange2d::matched_length([0.2, 0.2], true), None);
}

#[test]
fn match_up_lays_one_boundary_along_the_other_the_other_way_round() {
    // A 400 x 300 rectangle and, to its right, a quadrilateral with a slanted side.
    let t = base(DVec2::new(0.4, 0.3));
    let t = authoring::apply(
        &t,
        Command::Polygon {
            points: vec![
                DVec2::new(0.70, 0.0),
                DVec2::new(0.95, 0.0),
                DVec2::new(0.95, 0.25),
                DVec2::new(0.73, 0.26),
            ],
            tangents: vec![DVec2::ZERO; 4],
            meshing: meshing(),
            fabric: "woven-reference".into(),
            replace: false,
        },
    )
    .unwrap()
    .template;
    let (r, m) = (0, t.panels.len() - 1);
    let right = (0..4)
        .find(|&k| {
            let [a, b] = ends(&t, r, k);
            (a.x - b.x).abs() < 1e-12 && a.x > 0.3
        })
        .unwrap();
    let slanted = (0..4)
        .max_by(|&x, &y| {
            let s = |k| {
                let [a, b] = ends(&t, m, k);
                ((a.x - b.x).abs() > 1e-6 && (a.y - b.y).abs() > 1e-6) as u8
            };
            s(x).cmp(&s(y))
        })
        .unwrap();
    let reference = ends(&t, r, right);
    let centroid = |t: &GarmentTemplate, i: usize| {
        let p = polygon(t, i);
        p.points
            .iter()
            .map(|uv| arrange2d::board_point(t, i, *uv))
            .sum::<DVec2>()
            / p.points.len() as f64
    };
    let side = |p: DVec2| (reference[1] - reference[0]).perp_dot(p - reference[0]);
    for anchor in SegmentEnd::ALL {
        let edit = authoring::apply(
            &t,
            Command::MatchUp {
                reference: (t.panels[r].id.clone(), polygon(&t, r).edges[right].clone()),
                moved: (
                    t.panels[m].id.clone(),
                    polygon(&t, m).edges[slanted].clone(),
                ),
                anchor,
            },
        )
        .unwrap();
        let after = &edit.template;
        assert_eq!(ends(after, r, right), reference, "the reference stays");
        let [b0, b1] = ends(after, m, slanted);
        let along = (reference[1] - reference[0]).normalize();
        assert!(
            ((b1 - b0).normalize() + along).length() < 1e-9,
            "{anchor:?}: runs the other way"
        );
        match anchor {
            SegmentEnd::Start => assert!(close(b0, reference[1])),
            SegmentEnd::End => assert!(close(b1, reference[0])),
            SegmentEnd::Center => assert!(close(b0 + b1, reference[0] + reference[1])),
        }
        assert!(
            side(centroid(after, m)) * side(centroid(after, r)) < 0.,
            "{anchor:?}: the moved piece ends up on the other side"
        );
        assert!(
            authoring::same_physics(&t, after),
            "a board-only move never touches the cloth"
        );
    }
    assert!(
        authoring::apply(
            &t,
            Command::MatchUp {
                reference: (t.panels[r].id.clone(), polygon(&t, r).edges[0].clone()),
                moved: (t.panels[r].id.clone(), polygon(&t, r).edges[1].clone()),
                anchor: SegmentEnd::Start,
            },
        )
        .is_err(),
        "two boundaries of one piece cannot be matched up"
    );
}

#[test]
fn outward_normals_point_out_of_the_piece() {
    let t = base(DVec2::new(0.4, 0.3));
    let p = polygon(&t, 0);
    let centre = p.points.iter().sum::<DVec2>() / p.points.len() as f64;
    for k in 0..p.points.len() {
        let mid = (p.points[k] + p.points[(k + 1) % p.points.len()]) * 0.5;
        let normal = arrange2d::outward_normal(p, k);
        assert!((normal.length() - 1.).abs() < 1e-12);
        assert!(normal.dot(mid - centre) > 0., "edge {k}");
    }
}

#[test]
fn closed_shapes_clone_as_pieces_exactly() {
    let t = base(DVec2::splat(1.0));
    let id = t.panels[0].id.clone();
    let t = authoring::apply(
        &t,
        Command::InternalShape {
            panel: id.clone(),
            kind: InternalShapeKind::Ellipse,
            center: DVec2::new(0.5, 0.4),
            size: DVec2::new(0.3, 0.2),
            count: 1,
            interval_m: 0.,
            angle_degrees: 0.,
        },
    )
    .unwrap()
    .template;
    let mut t = t;
    t.panels[0].layout_rotation_degrees = 30.;
    let shape: InternalLine = polygon(&t, 0).lines[0].clone();
    let origin = arrange2d::board_point(&t, 0, DVec2::ZERO);
    let edit = authoring::apply(
        &t,
        Command::CloneShapeAsPanel {
            panel: id.clone(),
            line: shape.id.clone(),
            origin,
        },
    )
    .unwrap();
    let after = &edit.template;
    let clone = after.panels.len() - 1;
    assert_eq!(after.panels[clone].fabric, t.panels[0].fabric);
    // The new piece sits exactly over the shape, rotation included.
    let shape_board: Vec<DVec2> = shape
        .points
        .iter()
        .map(|uv| arrange2d::board_point(after, 0, *uv))
        .collect();
    let clone_board: Vec<DVec2> = polygon(after, clone)
        .points
        .iter()
        .map(|uv| arrange2d::board_point(after, clone, *uv))
        .collect();
    for p in &shape_board {
        assert!(
            clone_board.iter().any(|q| q.distance(*p) < 1e-9),
            "corner {p} of the shape is a corner of the piece"
        );
    }
    let samples = shape.samples().unwrap();
    let perimeter: f64 = samples.windows(2).map(|w| w[0].distance(w[1])).sum();
    let outline: f64 = polygon(after, clone)
        .edges
        .iter()
        .map(|e| authoring::edge_length(polygon(after, clone), e).unwrap())
        .sum();
    assert!(
        (perimeter - outline).abs() < 1e-6,
        "{perimeter} vs {outline}"
    );
    assert!(domain::area(&polygon(after, clone).points) > 0.);
    // An open line is not a shape.
    let open = authoring::apply(
        &t,
        Command::InternalLine {
            panel: id.clone(),
            points: vec![DVec2::splat(0.1), DVec2::splat(0.2)],
            tangents: vec![DVec2::ZERO; 2],
        },
    )
    .unwrap()
    .template;
    let line = polygon(&open, 0).lines.last().unwrap().id.clone();
    assert!(
        authoring::apply(
            &open,
            Command::CloneShapeAsPanel {
                panel: id,
                line,
                origin,
            },
        )
        .is_err()
    );
}

#[test]
fn pieces_clone_as_internal_shapes() {
    let t = base(DVec2::splat(2.0));
    let target = t.panels[0].id.clone();
    // A small rectangle: corners and straight sides copy exactly.
    let t = rectangle(&t, DVec2::new(0.2, 0.1), DVec2::new(2.5, 0.), false);
    let small = t.panels.last().unwrap().id.clone();
    let edit = authoring::apply(
        &t,
        Command::ClonePanelAsShape {
            source: small,
            target: target.clone(),
            center: DVec2::splat(0.5),
        },
    )
    .unwrap();
    let line = polygon(&edit.template, 0).lines.last().unwrap().clone();
    assert!(line.closed && line.points.len() == 4);
    assert!(line.tangents.iter().all(|t| *t == DVec2::ZERO));
    let samples = line.samples().unwrap();
    assert!((domain::area(&samples).abs() - 0.02).abs() < 1e-12);
    let (lo, hi) = arrange2d::bounds(&samples).unwrap();
    assert!(close((lo + hi) * 0.5, DVec2::splat(0.5)), "centred");

    // A curved piece (the T-shirt front) is followed within a fraction of its area.
    let tshirt = presets::basic(GarmentKind::Tshirt, 0);
    let front = &tshirt.panels[0];
    let shape = shapes2d::outline_to_shape("s".into(), front.polygon.as_ref().unwrap()).unwrap();
    let area = domain::area(&shape.samples().unwrap()).abs();
    let reference = ring_area(front.polygon.as_ref().unwrap());
    assert!(
        (area - reference).abs() / reference < 1e-2,
        "{area} vs {reference}"
    );
}

#[test]
fn held_drags_follow_the_nearest_straight_line() {
    let d = DVec2::new(0.10, 0.02);
    assert_eq!(arrange2d::constrain_move(d, &[]), DVec2::new(0.10, 0.));
    let diagonal = arrange2d::constrain_move(DVec2::new(0.1, 0.09), &[]);
    assert!((diagonal.x - diagonal.y).abs() < 1e-12, "45°");
    // A slanted segment's own direction wins when the drag runs along it.
    let axis = DVec2::new(1., 3.);
    let along = arrange2d::constrain_move(axis * 0.05 + DVec2::new(0.001, 0.), &[axis]);
    assert!(along.perp_dot(axis).abs() < 1e-12);
}
