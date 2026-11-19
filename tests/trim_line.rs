//! Trim a line: one segment of an internal line removed. An open line keeps the parts on either
//! side; a closed shape opens there into one line.
use fashioncraft::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        domain::InternalLine,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
};
use glam::DVec2;

/// A lone 40 × 60 cm piece with a three-segment zigzag "zig" and a closed square "box".
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
    let at = |x: f64, y: f64| lo + DVec2::new(x, y);
    polygon.lines.push(InternalLine {
        id: "zig".into(),
        points: vec![at(0.05, 0.1), at(0.15, 0.15), at(0.25, 0.1), at(0.35, 0.15)],
        tangents: vec![DVec2::ZERO; 4],
        closed: false,
        group: None,
    });
    polygon.lines.push(InternalLine {
        id: "box".into(),
        points: vec![at(0.1, 0.3), at(0.3, 0.3), at(0.3, 0.5), at(0.1, 0.5)],
        tangents: vec![DVec2::ZERO; 4],
        closed: true,
        group: None,
    });
    t
}

fn trim(t: &GarmentTemplate, line: &str, segment: usize) -> Result<GarmentTemplate, String> {
    authoring::apply(
        t,
        Command::TrimLine {
            panel: t.panels[0].id.clone(),
            line: line.into(),
            segment,
        },
    )
    .map(|e| e.template)
}

fn line<'a>(t: &'a GarmentTemplate, id: &str) -> Option<&'a InternalLine> {
    t.panels[0]
        .polygon
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .find(|l| l.id == id)
}

#[test]
fn trimming_an_open_line_keeps_the_parts_on_either_side() {
    let t = piece();
    let zig = line(&t, "zig").unwrap().points.clone();
    // The middle segment: two lines, the second under a fresh name.
    let after = trim(&t, "zig", 1).unwrap();
    assert_eq!(line(&after, "zig").unwrap().points, zig[..2]);
    assert_eq!(line(&after, "zig_2").unwrap().points, zig[2..]);
    // An end segment: one shorter line under the same name.
    let after = trim(&t, "zig", 0).unwrap();
    assert_eq!(line(&after, "zig").unwrap().points, zig[1..]);
    assert!(line(&after, "zig_2").is_none());
    let after = trim(&t, "zig", 2).unwrap();
    assert_eq!(line(&after, "zig").unwrap().points, zig[..3]);
    after.compile().expect("the trimmed piece compiles");
}

#[test]
fn a_closed_shape_opens_at_the_segment() {
    let t = piece();
    let square = line(&t, "box").unwrap().points.clone();
    let after = trim(&t, "box", 1).unwrap();
    let opened = line(&after, "box").unwrap();
    assert!(!opened.closed);
    // It runs from the removed segment's end round to its start.
    assert_eq!(
        opened.points,
        vec![square[2], square[3], square[0], square[1]]
    );
    after.compile().expect("the opened shape compiles");
}

#[test]
fn single_segments_and_missing_segments_are_refused() {
    let mut t = piece();
    assert!(trim(&t, "zig", 3).is_err());
    assert!(trim(&t, "box", 4).is_err());
    assert!(trim(&t, "missing", 0).is_err());
    let zig = t.panels[0]
        .polygon
        .as_mut()
        .unwrap()
        .lines
        .iter_mut()
        .find(|l| l.id == "zig")
        .unwrap();
    zig.points.truncate(2);
    zig.tangents.truncate(2);
    assert!(trim(&t, "zig", 0).is_err());
}
