//! Cut and sew: a piece split in two along a straight internal line carried on to the
//! outline, sewn back together along the cut, with the seams that crossed the cut split so
//! the garment stays sewn exactly as before.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{self, Command, edge_length},
    fitting::FittingProject,
    garment::{GarmentTemplate, domain::InternalLine, presets::GarmentKind},
};

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

/// A short horizontal line across the front's lower body, where only its two straight sides
/// cross that height.
fn with_cut_line(mut t: GarmentTemplate) -> (GarmentTemplate, String) {
    let polygon = t.panels[0].polygon.as_mut().expect("editable front");
    let lo = polygon.points.iter().fold(DVec2::MAX, |m, p| m.min(*p));
    let hi = polygon.points.iter().fold(DVec2::MIN, |m, p| m.max(*p));
    let n = polygon.points.len();
    for f in [0.1, 0.15, 0.2, 0.8, 0.85, 0.9] {
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
    let (t, line) = with_cut_line(tshirt());
    let panel = t.panels[0].id.clone();
    // A graphic on the piece would need a new home.
    let mut with_graphic = t.clone();
    with_graphic
        .graphics
        .push(marvelous_cloth::garment::graphics::GraphicPlacement {
            id: "g".into(),
            panel_id: panel.clone(),
            asset_id: "image_1".into(),
            center_m: DVec2::new(0.1, 0.1),
            size_m: DVec2::splat(0.05),
            rotation_degrees: 0.,
            side: marvelous_cloth::garment::graphics::GraphicSide::Front,
            opacity: 1.,
        });
    assert!(
        authoring::apply(
            &with_graphic,
            Command::CutAndSew {
                panel: panel.clone(),
                line: line.clone()
            }
        )
        .is_err()
    );
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
