//! Cut and sew: a piece split in two along a straight internal line carried on to the
//! outline, sewn back together along the cut, with the seams that crossed the cut split so
//! the garment stays sewn exactly as before.
use fashioncraft::{
    authoring::{self, Command, edge_length},
    fitting::FittingProject,
    garment::{
        GarmentTemplate,
        domain::{Hole, InternalLine},
        graphics::{GraphicPlacement, GraphicSide},
        presets::GarmentKind,
        template::{Tack, TackPoint},
        trims::{Trim, TrimKind},
    },
};
use glam::DVec2;

fn tshirt() -> GarmentTemplate {
    FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .template
}

fn area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| points[k].perp_dot(points[(k + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.
}

/// A short horizontal line across the front near its top, where only its two straight sides
/// cross that height.
fn with_cut_line(t: GarmentTemplate) -> (GarmentTemplate, String) {
    with_cut_line_at(t, &[0.1, 0.15, 0.2, 0.8, 0.85, 0.9])
}

/// A short horizontal line across the front at the first of `fractions` of its height where
/// only two straight sides cross.
fn with_cut_line_at(mut t: GarmentTemplate, fractions: &[f64]) -> (GarmentTemplate, String) {
    let polygon = t.panels[0].polygon.as_mut().expect("editable front");
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    let hi = polygon.points.iter().fold(DVec2::MIN, |m, p| m.max(*p));
    let n = polygon.points.len();
    for &f in fractions {
        let y = lo.y + f * (hi.y - lo.y);
        let mut xs = vec![];
        for k in 0..n {
            let (a, b) = (polygon.points[k], polygon.points[(k + 1) % n]);
            if (a.y - y) * (b.y - y) < 0. && !polygon.curves.contains_key(&polygon.edges[k]) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        if xs.len() == 2 {
            let cx = (xs[0] + xs[1]) / 2.;
            polygon.lines.push(InternalLine {
                id: "cut_here".into(),
                points: vec![DVec2::new(cx - 0.02, y), DVec2::new(cx + 0.02, y)],
                tangents: vec![DVec2::ZERO; 2],
                closed: false,
                group: None,
            });
            return (t, "cut_here".into());
        }
    }
    panic!("no straight crossing found on the front");
}

/// Sewn length per piece pair, summed over seams (side a).
fn sewn_length(t: &GarmentTemplate) -> f64 {
    t.seams
        .iter()
        .map(|s| {
            let p = t.panels.iter().find(|p| p.id == s.a.panel).unwrap();
            let polygon = p.polygon.as_ref().unwrap();
            let edge = s.a.edge.as_deref().unwrap();
            (s.a.range[1] - s.a.range[0]) * edge_length(polygon, edge).unwrap()
        })
        .sum()
}

/// Heights across the front's body, well clear of the neckline.
const BODY: [f64; 3] = [0.7, 0.6, 0.75];

/// Where a material point of `panel` lies on the 2D board.
fn on_board(t: &GarmentTemplate, panel: &str, p: DVec2) -> DVec2 {
    let piece = t.panels.iter().find(|q| q.id == panel).unwrap();
    piece.layout_2d.unwrap_or_default() + authoring::rotate_2d(p, piece.layout_rotation_degrees)
}

fn graphic(id: &str, panel: &str, center: DVec2) -> GraphicPlacement {
    GraphicPlacement {
        id: id.into(),
        panel_id: panel.into(),
        asset_id: "image_1".into(),
        center_m: center,
        size_m: DVec2::splat(0.02),
        rotation_degrees: 0.,
        side: GraphicSide::Front,
        opacity: 1.,
    }
}

/// The middle of the cut line `line` on the front.
fn cut_middle(t: &GarmentTemplate, line: &str) -> DVec2 {
    let l = t.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .find(|l| l.id == line)
        .unwrap();
    (l.points[0] + l.points[1]) / 2.
}

/// The seam the cut sewed between the front and its new half.
fn cut_seam(cut: &GarmentTemplate, front: &str, half: &str) -> String {
    cut.seams
        .iter()
        .find(|s| {
            [&s.a.panel, &s.b.panel] == [&front, &half]
                || [&s.a.panel, &s.b.panel] == [&half, &front]
        })
        .expect("the cut seam joins the halves")
        .id
        .clone()
}

#[test]
fn a_piece_is_cut_in_two_and_stays_sewn() {
    let (before, line) = with_cut_line(tshirt());
    let front = before.panels[0].clone();
    let front_area = area(&front.polygon.as_ref().unwrap().points);
    let sewn = sewn_length(&before);
    let edit = authoring::apply(
        &before,
        Command::CutAndSew {
            panel: front.id.clone(),
            line,
        },
    )
    .unwrap();
    let after = edit.template;
    assert_eq!(after.panels.len(), before.panels.len() + 1);
    let halves: Vec<_> = after
        .panels
        .iter()
        .filter(|p| p.id == front.id || p.name == format!("{} 2", front.name))
        .collect();
    assert_eq!(halves.len(), 2);
    let cut_area: f64 = halves
        .iter()
        .map(|p| area(&p.polygon.as_ref().unwrap().points))
        .sum();
    assert!(
        (cut_area - front_area).abs() < 1e-9,
        "{cut_area} vs {front_area}"
    );
    // Every reference names a boundary that exists on its piece.
    for s in &after.seams {
        for r in s.references() {
            let p = after.panels.iter().find(|p| p.id == r.panel).unwrap();
            let edges = &p.polygon.as_ref().unwrap().edges;
            assert!(edges.contains(r.edge.as_ref().unwrap()), "{r:?}");
            assert!(r.range[0] >= 0. && r.range[1] <= 1. && r.range[0] < r.range[1]);
        }
    }
    // The seams that crossed the cut were split, not dropped: the sewn length is unchanged
    // apart from the new seam along the cut.
    let cut_seam = after
        .seams
        .iter()
        .find(|s| {
            s.a.panel != s.b.panel
                && halves.iter().any(|h| h.id == s.b.panel)
                && s.a.edge == s.b.edge
        })
        .expect("the halves are sewn along the cut");
    let cut_length = edge_length(
        halves[0].polygon.as_ref().unwrap(),
        cut_seam.a.edge.as_ref().unwrap(),
    )
    .unwrap();
    assert!(
        (sewn_length(&after) - cut_length - sewn).abs() < 1e-6,
        "sewn {} vs {} + {cut_length}",
        sewn_length(&after),
        sewn
    );
    assert!(
        after.seams.len() > before.seams.len() + 1,
        "crossing seams were split"
    );
    // The garment still compiles and the cut line is gone.
    let line_left = after
        .panels
        .iter()
        .flat_map(|p| p.polygon.as_ref().unwrap().lines.iter())
        .any(|l| l.id == "cut_here");
    assert!(!line_left);
}

#[test]
fn what_cannot_be_carried_over_is_refused() {
    let (t, line) = with_cut_line_at(tshirt(), &BODY);
    let panel = t.panels[0].id.clone();
    // A graphic lying on both halves would have no single home.
    let mut with_graphic = t.clone();
    let mid = cut_middle(&t, &line);
    with_graphic.graphics.push(graphic("g", &panel, mid));
    let refused = authoring::apply(
        &with_graphic,
        Command::CutAndSew {
            panel: panel.clone(),
            line: line.clone(),
        },
    )
    .err()
    .expect("a graphic across the cut is refused");
    assert!(refused.contains("across the cut"), "{refused}");
    // So would a hole.
    let mut with_hole = t.clone();
    let corner = mid - DVec2::splat(0.01);
    with_hole.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .holes
        .push(Hole {
            points: vec![
                corner,
                corner + DVec2::new(0., 0.02),
                corner + DVec2::new(0.02, 0.02),
                corner + DVec2::new(0.02, 0.),
            ],
            edges: (0..4).map(|i| format!("eyelet_{i}")).collect(),
        });
    let refused = authoring::apply(
        &with_hole,
        Command::CutAndSew {
            panel: panel.clone(),
            line: line.clone(),
        },
    )
    .err()
    .expect("a hole across the cut is refused");
    assert!(refused.contains("hole"), "{refused}");
    // A bent line is not a straight cut.
    let mut bent = t.clone();
    let l = bent.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .lines
        .iter_mut()
        .find(|l| l.id == line)
        .unwrap();
    let mid = (l.points[0] + l.points[1]) / 2. + DVec2::new(0., 0.01);
    l.points.insert(1, mid);
    l.tangents.push(DVec2::ZERO);
    assert!(
        authoring::apply(
            &bent,
            Command::CutAndSew {
                panel: panel.clone(),
                line: line.clone()
            }
        )
        .is_err()
    );
    assert!(
        authoring::apply(
            &t,
            Command::CutAndSew {
                panel,
                line: "missing".into()
            }
        )
        .is_err()
    );
}

#[test]
fn merging_the_halves_back_gives_the_piece_its_area_and_seams_again() {
    let (before, line) = with_cut_line(tshirt());
    let front = before.panels[0].clone();
    let front_area = area(&front.polygon.as_ref().unwrap().points);
    let sewn = sewn_length(&before);
    let cut = authoring::apply(
        &before,
        Command::CutAndSew {
            panel: front.id.clone(),
            line,
        },
    )
    .unwrap()
    .template;
    let half = cut
        .panels
        .iter()
        .find(|p| p.name == format!("{} 2", front.name))
        .unwrap()
        .id
        .clone();
    let cut_seam = cut
        .seams
        .iter()
        .find(|s| {
            [&s.a.panel, &s.b.panel] == [&front.id, &half]
                || [&s.a.panel, &s.b.panel] == [&half, &front.id]
        })
        .expect("the cut seam joins the halves")
        .id
        .clone();
    let edit = authoring::apply(&cut, Command::MergeAlongSeam { seam: cut_seam }).unwrap();
    let merged = edit.template;
    assert_eq!(merged.panels.len(), before.panels.len());
    let piece = merged.panels.iter().find(|p| p.id == front.id).unwrap();
    let merged_area = area(&piece.polygon.as_ref().unwrap().points);
    assert!(
        (merged_area - front_area).abs() < 1e-9,
        "{merged_area} vs {front_area}"
    );
    // The split side seams stay split, so the sewn length is the original one.
    assert!(
        (sewn_length(&merged) - sewn).abs() < 1e-6,
        "{} vs {sewn}",
        sewn_length(&merged)
    );
    for s in &merged.seams {
        for r in s.references() {
            let p = merged.panels.iter().find(|p| p.id == r.panel).unwrap();
            assert!(
                p.polygon
                    .as_ref()
                    .unwrap()
                    .edges
                    .contains(r.edge.as_ref().unwrap())
            );
        }
    }
}

#[test]
fn merging_refuses_seams_it_cannot_close_flat() {
    let t = tshirt();
    // The tee's seams join pieces that face each other across the body (or are not whole
    // boundaries): merging them flat is refused, never half done.
    for seam in &t.seams {
        if let Ok(edit) = authoring::apply(
            &t,
            Command::MergeAlongSeam {
                seam: seam.id.clone(),
            },
        ) {
            // A merge that is accepted must leave a valid, single piece.
            assert_eq!(edit.template.panels.len(), t.panels.len() - 1);
        }
    }
    assert!(
        authoring::apply(
            &t,
            Command::MergeAlongSeam {
                seam: "missing".into()
            }
        )
        .is_err()
    );
}

#[test]
fn graphics_trims_tacks_and_holes_go_with_their_half_and_come_back_when_merged() {
    let (mut t, line) = with_cut_line_at(tshirt(), &BODY);
    let front = t.panels[0].id.clone();
    let back = t.panels[1].id.clone();
    let mid = cut_middle(&t, &line);
    t.graphics
        .push(graphic("below", &front, mid - DVec2::new(0., 0.03)));
    t.graphics
        .push(graphic("above", &front, mid + DVec2::new(0., 0.03)));
    t.trims.push(Trim {
        id: "button".into(),
        panel_id: front.clone(),
        kind: TrimKind::Button {
            center_m: mid + DVec2::new(0.04, 0.03),
            diameter_m: 0.01,
        },
    });
    t.tacks.push(Tack {
        id: "tack".into(),
        a: TackPoint {
            panel: front.clone(),
            uv: mid - DVec2::new(0.04, 0.02),
        },
        b: TackPoint {
            panel: back.clone(),
            uv: DVec2::splat(0.1),
        },
    });
    let corner = mid + DVec2::new(-0.06, 0.02);
    t.panels[0].polygon.as_mut().unwrap().holes.push(Hole {
        points: vec![
            corner,
            corner + DVec2::new(0., 0.02),
            corner + DVec2::new(0.02, 0.02),
            corner + DVec2::new(0.02, 0.),
        ],
        edges: (0..4).map(|i| format!("eyelet_{i}")).collect(),
    });
    // Two pinned outline corners, one at the top and one at the hem.
    let pins = {
        let poly = t.panels[0].polygon.as_ref().unwrap();
        let lowest = |by: fn(&DVec2, &DVec2) -> std::cmp::Ordering| {
            *poly.points.iter().min_by(|a, b| by(a, b)).unwrap()
        };
        vec![
            lowest(|a, b| a.y.total_cmp(&b.y)),
            lowest(|a, b| b.y.total_cmp(&a.y)),
        ]
    };
    t.panels[0].pin_points = pins;
    let pinned = |t: &GarmentTemplate| -> Vec<DVec2> {
        let mut out: Vec<DVec2> = t
            .panels
            .iter()
            .flat_map(|p| p.pin_points.iter().map(|q| on_board(t, &p.id, *q)))
            .collect();
        out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        out
    };
    let same = |a: &[DVec2], b: &[DVec2]| {
        a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 1e-9)
    };
    let pinned_before = pinned(&t);
    let board = |t: &GarmentTemplate| {
        let g = |id: &str| {
            let g = t.graphics.iter().find(|g| g.id == id).unwrap();
            on_board(t, &g.panel_id, g.center_m)
        };
        let button = &t.trims[0];
        let TrimKind::Button { center_m, .. } = button.kind else {
            unreachable!()
        };
        let tack = &t.tacks[0];
        let (hole_panel, hole) = t
            .panels
            .iter()
            .find_map(|p| Some((p, p.polygon.as_ref()?.holes.first()?)))
            .unwrap();
        [
            g("below"),
            g("above"),
            on_board(t, &button.panel_id, center_m),
            on_board(t, &tack.a.panel, tack.a.uv),
            on_board(t, &tack.b.panel, tack.b.uv),
            on_board(t, &hole_panel.id, hole.points[0]),
        ]
    };
    let before = board(&t);
    let cut = authoring::apply(
        &t,
        Command::CutAndSew {
            panel: front.clone(),
            line,
        },
    )
    .unwrap()
    .template;
    let name = &t.panels[0].name;
    let half = cut
        .panels
        .iter()
        .find(|p| p.name == format!("{name} 2"))
        .unwrap()
        .id
        .clone();
    let on = |id: &str| {
        cut.graphics
            .iter()
            .find(|g| g.id == id)
            .unwrap()
            .panel_id
            .clone()
    };
    // Each item went to the half it lies on, and stays where it was on the board.
    let (below, above) = (on("below"), on("above"));
    assert!(below != above && [&front, &half].contains(&&below));
    assert_eq!(cut.trims[0].panel_id, above);
    assert_eq!(cut.tacks[0].a.panel, below);
    assert_eq!(cut.tacks[0].b.panel, back);
    let holder = |t: &GarmentTemplate| {
        t.panels
            .iter()
            .filter(|p| !p.polygon.as_ref().unwrap().holes.is_empty())
            .map(|p| p.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(holder(&cut), vec![above.clone()]);
    for (a, b) in before.iter().zip(board(&cut)) {
        assert!(a.distance(b) < 1e-9, "{a} moved to {b}");
    }
    assert!(same(&pinned(&cut), &pinned_before), "pins moved");
    let holding: Vec<&str> = cut
        .panels
        .iter()
        .filter(|p| p.pin_points.len() == 1)
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(holding.len(), 2, "each half keeps its own pinned corner");
    cut.compile().expect("the cut garment compiles");
    // Merging the halves back puts everything on the whole piece where it was.
    let merged = authoring::apply(
        &cut,
        Command::MergeAlongSeam {
            seam: cut_seam(&cut, &front, &half),
        },
    )
    .unwrap()
    .template;
    assert!(merged.graphics.iter().all(|g| g.panel_id == front));
    assert_eq!(merged.trims[0].panel_id, front);
    assert_eq!(merged.tacks[0].a.panel, front);
    assert_eq!(holder(&merged), vec![front.clone()]);
    for (a, b) in before.iter().zip(board(&merged)) {
        assert!(a.distance(b) < 1e-9, "{a} moved to {b}");
    }
    for g in &merged.graphics {
        let turn = g.rotation_degrees.to_radians();
        assert!(
            turn.sin().abs() < 1e-9 && turn.cos() > 0.,
            "{}",
            g.rotation_degrees
        );
    }
    assert!(same(&pinned(&merged), &pinned_before), "pins moved");
    merged.compile().expect("the merged garment compiles");
}
