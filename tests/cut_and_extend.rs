//! Cut and extend: an internal line prolonged to the outline, and a point added to the
//! outline where it arrives.
use fashioncraft::{
    authoring::{self, Command},
    garment::{domain::InternalLine, lines2d, meshing::MeshingSettings, presets},
};
use glam::DVec2;

#[test]
fn a_line_prolonged_to_the_outline_leaves_a_point_there() {
    let points = [[0., 0.], [0.4, 0.], [0.4, 0.3], [0., 0.3]];
    let mut t = authoring::apply(
        &presets::basic(presets::GarmentKind::Tshirt, 0),
        Command::Polygon {
            points: points.iter().map(|p| DVec2::from_array(*p)).collect(),
            tangents: vec![DVec2::ZERO; 4],
            meshing: MeshingSettings {
                target_edge_m: 0.03,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let id = t.panels[0].id.clone();
    let polygon = t.panels[0].polygon.as_mut().unwrap();
    polygon.lines.push(InternalLine {
        id: "line_1".into(),
        points: vec![DVec2::new(0.15, 0.05), DVec2::new(0.15, 0.25)],
        tangents: vec![DVec2::ZERO; 2],
        closed: false,
        group: None,
    });
    let edges_before = polygon.edges.len();
    let polygon = polygon.clone();
    // The start is 5 cm from the top edge, the end 5 cm from the bottom: either works.
    let end = lines2d::extended_end(&polygon, "line_1", true).unwrap();
    assert!((end.y - lines2d::OUTLINE_GAP_M).abs() < 1e-9 && (end.x - 0.15).abs() < 1e-9);
    let t = authoring::apply(
        &t,
        Command::ExtendLine {
            panel: id.clone(),
            line: "line_1".into(),
            at_start: true,
        },
    )
    .unwrap()
    .template;
    let top = &polygon.edges[0];
    let t = authoring::apply(
        &t,
        Command::SplitBoundary {
            panel: id.clone(),
            edge: top.clone(),
            at_m: 0.15,
        },
    )
    .unwrap()
    .template;
    let polygon = t.panels[0].polygon.as_ref().unwrap();
    assert_eq!(polygon.edges.len(), edges_before + 1);
    assert!(
        polygon
            .points
            .iter()
            .any(|p| p.distance(DVec2::new(0.15, 0.)) < 1e-9)
    );
    t.compile().unwrap();
}
