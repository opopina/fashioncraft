//! Outline edits on a piece laid out automatically keep it where it is on the board.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{self, Command},
    garment::{
        GarmentTemplate,
        presets::{self, GarmentKind},
    },
};

/// Where a piece's outline points are on the board.
fn on_board(t: &GarmentTemplate, index: usize) -> Vec<DVec2> {
    let panel = &t.panels[index];
    let origin = authoring::layout(t, index);
    panel
        .polygon
        .as_ref()
        .unwrap()
        .points
        .iter()
        .map(|p| origin + authoring::rotate_2d(*p, panel.layout_rotation_degrees))
        .collect()
}

#[test]
fn splitting_a_segment_leaves_an_auto_laid_out_piece_in_place() {
    let t = presets::basic(GarmentKind::Tshirt, 0);
    assert!(t.panels.iter().all(|p| p.layout_2d.is_none()));
    let before = on_board(&t, 1);
    let edit = authoring::apply(
        &t,
        Command::SplitBoundary {
            panel: t.panels[1].id.clone(),
            edge: "hem".into(),
            at_m: 0.28,
        },
    )
    .unwrap()
    .template;
    // Every old point of the back is where it was (one new point sits on the hem), so the
    // back does not jump onto the front.
    let after = on_board(&edit, 1);
    assert_eq!(after.len(), before.len() + 1);
    for p in &before {
        assert!(
            after.iter().any(|q| q.distance(*p) < 1e-9),
            "{p} moved: {after:?}"
        );
    }
    // The front stays where it was too.
    assert_eq!(on_board(&edit, 0), on_board(&t, 0));
    // An edit that moves nothing on the board leaves every piece automatic.
    let pressed = authoring::apply(
        &t,
        Command::SetPressure {
            panels: vec![t.panels[0].id.clone()],
            pressure: 10.,
        },
    )
    .unwrap()
    .template;
    assert!(pressed.panels.iter().all(|p| p.layout_2d.is_none()));
}
