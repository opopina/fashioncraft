//! Board arrangement of points and panels: transforms keep curves attached, alignment reads
//! board coordinates (also on rotated panels), and panel moves never touch the material.
use glam::DVec2;
use marvelous_cloth::{
    authoring::{
        self, Command,
        arrange2d::{self, AlignMode, Axis},
    },
    garment::{
        GarmentTemplate,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
};

fn square(size: f64) -> GarmentTemplate {
    authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::splat(size),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.06,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template
}

fn board_points(t: &GarmentTemplate, index: usize) -> Vec<DVec2> {
    t.panels[index]
        .polygon
        .as_ref()
        .unwrap()
        .points
        .iter()
        .map(|uv| arrange2d::board_point(t, index, *uv))
        .collect()
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn pure_helpers_align_and_distribute() {
    let points = [
        DVec2::new(0.1, 0.5),
        DVec2::new(0.4, 0.2),
        DVec2::new(0.3, 0.9),
    ];
    let left = arrange2d::align_points(&points, AlignMode::Left, None).unwrap();
    assert!(left.iter().all(|p| p.x == 0.1));
    assert_eq!(left[2].y, 0.9, "only the mode's axis changes");
    let middle = arrange2d::align_points(&points, AlignMode::Middle, None).unwrap();
    assert!(middle.iter().all(|p| (p.y - 0.55).abs() < 1e-12));
    let to_x =
        arrange2d::align_points(&points, AlignMode::ToPointX, Some(DVec2::new(0.7, 0.))).unwrap();
    assert!(to_x.iter().all(|p| p.x == 0.7));
    assert!(arrange2d::align_points(&points, AlignMode::ToPointY, None).is_err());

    let boxes = [
        (DVec2::new(1.0, 0.), DVec2::new(1.5, 1.)),
        (DVec2::new(0.0, 0.), DVec2::new(0.4, 1.)),
        (DVec2::new(3.0, 0.), DVec2::new(3.2, 1.)),
    ];
    let offsets = arrange2d::distribute_offsets(&boxes, Axis::Horizontal, 0.1);
    assert_eq!(
        offsets[1],
        DVec2::ZERO,
        "the first box along the axis stays"
    );
    assert!((offsets[0].x - (0.5 - 1.0)).abs() < 1e-12);
    assert!((offsets[2].x - (1.1 - 3.0)).abs() < 1e-12);
    // Even spacing keeps the overall extent: the gaps here are 0.4 and 0.55, so 0.475 each.
    let spread = [
        (DVec2::new(-0.1, 0.), DVec2::new(0.1, 0.3)),
        (DVec2::new(0.5, 0.), DVec2::new(0.9, 0.3)),
        (DVec2::new(1.45, 0.), DVec2::new(1.75, 0.3)),
    ];
    let even = arrange2d::average_gap(&spread, Axis::Horizontal).unwrap();
    assert!((even - 0.475).abs() < 1e-12);
    let offsets = arrange2d::distribute_offsets(&spread, Axis::Horizontal, even);
    assert!((offsets[1].x - 0.075).abs() < 1e-12 && offsets[2].x.abs() < 1e-12);
    assert_eq!(arrange2d::average_gap(&spread[..1], Axis::Horizontal), None);
    let right = arrange2d::align_box_offsets(&boxes, boxes[0], AlignMode::Right).unwrap();
    assert!((right[1].x - 1.1).abs() < 1e-12);
    assert!(arrange2d::align_box_offsets(&boxes, boxes[0], AlignMode::ToPointX).is_err());
}

#[test]
fn rotating_points_turns_their_curve_handles_too() {
    let mut t = square(0.4);
    let id = t.panels[0].id.clone();
    // Curve the bottom edge (point 2 → point 3) so a handle hangs off the moved points.
    let poly = t.panels[0].polygon.as_mut().unwrap();
    let edge = poly.edges[2].clone();
    poly.curves
        .insert(edge.clone(), [DVec2::new(0.3, 0.5), DVec2::new(0.1, 0.5)]);
    let before = board_points(&t, 0);
    let pivot = (before[2] + before[3]) * 0.5;
    let edit = authoring::apply(
        &t,
        Command::TransformPointGroups {
            groups: vec![(id.clone(), vec![2, 3])],
            pivot,
            rotation_degrees: 20.,
            scale: DVec2::ONE,
        },
    )
    .unwrap();
    let after = board_points(&edit.template, 0);
    // The two bottom corners turn 20° about their midpoint; the top ones stay.
    for k in [2, 3] {
        let expected = arrange2d::affine(before[k], pivot, 20., DVec2::ONE);
        assert!(
            close(after[k], expected),
            "{k}: {:?} vs {expected:?}",
            after[k]
        );
    }
    assert!(close(after[0], before[0]) && close(after[1], before[1]));
    // The bulge turns with them: its handles are rotated about the same pivot.
    let poly = edit.template.panels[0].polygon.as_ref().unwrap();
    let c = poly.curves[&edge];
    for (handle, original) in c.iter().zip([DVec2::new(0.3, 0.5), DVec2::new(0.1, 0.5)]) {
        let got = arrange2d::board_point(&edit.template, 0, *handle);
        let expected = arrange2d::affine(
            arrange2d::board_point(&t, 0, original),
            pivot,
            20.,
            DVec2::ONE,
        );
        assert!(close(got, expected), "{got:?} vs {expected:?}");
    }
    // A half turn would swap the corners and cross the outline: refused, nothing changes.
    assert!(
        authoring::apply(
            &t,
            Command::TransformPointGroups {
                groups: vec![(t.panels[0].id.clone(), vec![2, 3])],
                pivot,
                rotation_degrees: 180.,
                scale: DVec2::ONE,
            },
        )
        .is_err()
    );
    assert!(edit.template.compile().is_ok());
}

#[test]
fn scaling_points_about_their_centre() {
    let t = square(0.4);
    let id = t.panels[0].id.clone();
    let before = board_points(&t, 0);
    let pivot = (before[1] + before[2]) * 0.5;
    let edit = authoring::apply(
        &t,
        Command::TransformPointGroups {
            groups: vec![(id.clone(), vec![1, 2])],
            pivot,
            rotation_degrees: 0.,
            scale: DVec2::new(1., 2.),
        },
    )
    .unwrap();
    let after = board_points(&edit.template, 0);
    // The right edge doubles its height about its middle; the left edge stays.
    assert!(((after[2] - after[1]).length() - 2. * (before[2] - before[1]).length()).abs() < 1e-9);
    assert!(close(after[0], before[0]) && close(after[3], before[3]));
    for bad in [DVec2::ZERO, DVec2::new(1., f64::NAN), DVec2::splat(1000.)] {
        assert!(
            authoring::apply(
                &t,
                Command::TransformPointGroups {
                    groups: vec![(id.clone(), vec![1])],
                    pivot,
                    rotation_degrees: 0.,
                    scale: bad,
                },
            )
            .is_err()
        );
    }
}

#[test]
fn aligning_points_reads_the_board_on_rotated_panels() {
    let t = square(0.4);
    let id = t.panels[0].id.clone();
    // Turn the panel on the board first.
    let origin = authoring::layout(&t, 0);
    let t = authoring::apply(
        &t,
        Command::BoardTransform {
            panel: id.clone(),
            origin,
            rotation_degrees: 30.,
        },
    )
    .unwrap()
    .template;
    let before = board_points(&t, 0);
    let edit = authoring::apply(
        &t,
        Command::AlignPointGroups {
            groups: vec![(id.clone(), vec![0, 1])],
            mode: AlignMode::Top,
            reference: None,
        },
    )
    .unwrap();
    let after = board_points(&edit.template, 0);
    let top = before[0].y.min(before[1].y);
    assert!((after[0].y - top).abs() < 1e-9 && (after[1].y - top).abs() < 1e-9);
    assert!(
        (after[0].x - before[0].x).abs() < 1e-9,
        "x is kept on the board"
    );
    // To another point's x.
    let edit = authoring::apply(
        &t,
        Command::AlignPointGroups {
            groups: vec![(id.clone(), vec![1, 2])],
            mode: AlignMode::ToPointX,
            reference: Some((id.clone(), 0)),
        },
    )
    .unwrap();
    let after = board_points(&edit.template, 0);
    assert!((after[1].x - before[0].x).abs() < 1e-9 && (after[2].x - before[0].x).abs() < 1e-9);
}

#[test]
fn scaling_a_panel_keeps_its_centre_and_scales_its_contents() {
    let t = square(0.4);
    let id = t.panels[0].id.clone();
    let centre = |t: &GarmentTemplate| {
        let (lo, hi) = arrange2d::board_bounds(t, 0).unwrap();
        (lo + hi) * 0.5
    };
    let edit = authoring::apply(
        &t,
        Command::ScalePanel {
            panel: id.clone(),
            scale: DVec2::new(1.5, 0.5),
        },
    )
    .unwrap();
    let p = &edit.template.panels[0];
    assert!((p.pattern.width - 0.6).abs() < 1e-9 && (p.pattern.height - 0.2).abs() < 1e-9);
    assert!(close(centre(&edit.template), centre(&t)));
    assert!(edit.reset_simulation, "a new rest shape restarts the drape");
}

#[test]
fn panels_align_and_distribute_without_touching_the_material() {
    let t = presets::basic(GarmentKind::Tshirt, 0);
    let ids: Vec<String> = t.panels.iter().map(|p| p.id.clone()).collect();
    assert!(ids.len() >= 2);
    let aligned = authoring::apply(
        &t,
        Command::AlignPanels {
            panels: ids.clone(),
            reference: ids[0].clone(),
            mode: AlignMode::Bottom,
        },
    )
    .unwrap();
    let bottoms: Vec<f64> = (0..ids.len())
        .map(|i| arrange2d::board_bounds(&aligned.template, i).unwrap().1.y)
        .collect();
    assert!(bottoms.iter().all(|b| (b - bottoms[0]).abs() < 1e-9));
    assert!(!aligned.reset_simulation, "board layout is not physics");
    for (a, b) in t.panels.iter().zip(&aligned.template.panels) {
        assert_eq!(
            serde_json::to_string(&a.polygon).unwrap(),
            serde_json::to_string(&b.polygon).unwrap()
        );
    }
    let spread = authoring::apply(
        &t,
        Command::DistributePanels {
            panels: ids.clone(),
            axis: Axis::Horizontal,
            gap_m: 0.05,
        },
    )
    .unwrap();
    let mut boxes: Vec<_> = (0..ids.len())
        .map(|i| arrange2d::board_bounds(&spread.template, i).unwrap())
        .collect();
    boxes.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
    for pair in boxes.windows(2) {
        assert!((pair[1].0.x - pair[0].1.x - 0.05).abs() < 1e-9);
    }
    assert!(
        authoring::apply(
            &t,
            Command::DistributePanels {
                panels: vec![ids[0].clone()],
                axis: Axis::Vertical,
                gap_m: 0.05,
            },
        )
        .is_err()
    );
}
