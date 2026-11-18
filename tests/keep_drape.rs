//! A 2D edit of a draped panel keeps its 3D shape instead of flattening it back to the
//! arrangement; flat panels and re-placed panels behave as before.
use glam::{DVec2, DVec3};
use marvelous_cloth::{
    authoring::{self, Command, arrange2d::SegmentEnd},
    garment::{
        CompiledGarment, GarmentTemplate,
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
    model::State,
};

fn rectangle() -> GarmentTemplate {
    let mut t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.4, 0.3),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    for p in &mut t.panels {
        p.pins.clear();
        p.pin_points.clear();
    }
    t
}

/// A smooth stand-in for a drape: the flat arrangement rolled onto a vertical cylinder.
fn roll(p: DVec3, x0: f64) -> DVec3 {
    let r = 0.25;
    let a = (p.x - x0) / r;
    DVec3::new(x0 + r * a.sin(), p.y, p.z + r * (1. - a.cos()))
}

fn draped(g: &CompiledGarment) -> (State, f64) {
    let x0 = g.arrangement_positions.iter().map(|p| p.x).sum::<f64>()
        / g.arrangement_positions.len() as f64;
    let mut state = g.initial_state.clone();
    for (x, rest) in state.x.iter_mut().zip(&g.arrangement_positions) {
        *x = roll(*rest, x0);
    }
    for v in &mut state.v {
        *v = DVec3::new(0., -0.1, 0.);
    }
    (state, x0)
}

fn top_edge(t: &GarmentTemplate) -> String {
    let p = t.panels[0].polygon.as_ref().unwrap();
    let n = p.points.len();
    (0..n)
        .find(|&k| p.points[k].y.abs() < 1e-12 && p.points[(k + 1) % n].y.abs() < 1e-12)
        .map(|k| p.edges[k].clone())
        .unwrap()
}

#[test]
fn a_reshaped_panel_keeps_its_drape() {
    let t = rectangle();
    let g = t.compile().unwrap();
    let (state, x0) = draped(&g);
    // Lengthen the top edge by 60 mm, moving its end: the panel becomes a trapezoid.
    let edit = authoring::apply(
        &t,
        Command::SetLength {
            panel: t.panels[0].id.clone(),
            edge: top_edge(&t),
            length_m: 0.46,
            moving: SegmentEnd::End,
        },
    )
    .unwrap();
    assert!(edit.reset_simulation);
    let next = edit.state_keeping_drape(&g, &state);
    let new = &edit.garment;
    assert_eq!(next.x.len(), new.mesh.uv.len());
    let (mut inside_worst, mut outside_worst): (f64, f64) = (0., 0.);
    let (lo, hi) = g.arrangement_positions.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    for (x, rest) in next.x.iter().zip(&new.arrangement_positions) {
        assert!(x.is_finite());
        let error = x.distance(roll(*rest, x0));
        let inside = rest.x > lo.x && rest.x < hi.x && rest.y > lo.y && rest.y < hi.y;
        if inside {
            inside_worst = inside_worst.max(error);
        } else {
            outside_worst = outside_worst.max(error);
        }
    }
    // Piecewise-linear interpolation of the roll on a 20 mm mesh.
    assert!(
        inside_worst < 2e-3,
        "inside the old outline: {inside_worst}"
    );
    // The added strip follows the nearest triangle's slope.
    assert!(outside_worst < 1.5e-2, "outside: {outside_worst}");
    assert!(
        next.v.iter().all(|v| *v == DVec3::ZERO),
        "no momentum is invented"
    );
    // The old behaviour flattened it.
    let flat = edit.state_after(&g, &state);
    assert_eq!(flat.x, new.initial_state.x);
}

#[test]
fn flat_and_replaced_panels_behave_as_before() {
    let t = rectangle();
    let g = t.compile().unwrap();
    let edit = authoring::apply(
        &t,
        Command::SetLength {
            panel: t.panels[0].id.clone(),
            edge: top_edge(&t),
            length_m: 0.46,
            moving: SegmentEnd::End,
        },
    )
    .unwrap();
    // Never simulated: the new arrangement, exactly as before.
    let flat = edit.state_keeping_drape(&g, &g.initial_state);
    assert_eq!(flat.x, edit.state_after(&g, &g.initial_state).x);

    // Re-placing a panel in 3D lays it out again (same mesh, new arrangement).
    let (state, _) = draped(&g);
    let mut placement = t.panels[0].placement.clone();
    placement.translation += DVec3::X * 0.1;
    let moved = authoring::apply(
        &t,
        Command::Place {
            panel: t.panels[0].id.clone(),
            placement,
        },
    )
    .unwrap();
    assert_eq!(
        moved.state_keeping_drape(&g, &state).x,
        moved.state_after(&g, &state).x
    );
}

#[test]
fn untouched_panels_are_copied_and_pins_hold() {
    let mut t = GarmentTemplate::two_panel_demo();
    for p in &mut t.panels {
        p.pins.clear();
    }
    let g = t.compile().unwrap();
    let (state, _) = draped(&g);
    // Duplicate a panel: the original panels keep their exact state.
    let edit = authoring::apply(
        &t,
        Command::Duplicate {
            panel: t.panels[0].id.clone(),
        },
    )
    .unwrap();
    let next = edit.state_keeping_drape(&g, &state);
    for old in &g.panels {
        let new = edit.garment.panels.iter().find(|p| p.id == old.id).unwrap();
        assert_eq!(next.x[new.vertices.clone()], state.x[old.vertices.clone()]);
    }
}

#[test]
fn a_piece_moved_in_3d_carries_its_drape() {
    let t = rectangle();
    let g = t.compile().unwrap();
    let (state, _) = draped(&g);
    let id = t.panels[0].id.clone();
    let mut placement = t.panels[0].placement.clone();
    placement.translation += DVec3::new(0.1, -0.05, 0.2);
    placement.rotation_degrees += DVec3::new(10., 35., -5.);
    let moved = authoring::apply(
        &t,
        Command::Place {
            panel: id.clone(),
            placement: placement.clone(),
        },
    )
    .unwrap();
    let next = moved.state_moving_drape(&t, &g, &state);
    let (q, shift) = t.panels[0].placement.motion_to(&placement);
    let range = moved.garment.panels[0].vertices.clone();
    for i in range.clone() {
        assert!((next.x[i] - (q * state.x[i] + shift)).length() < 1e-12);
        assert_eq!(next.v[i], DVec3::ZERO);
    }
    // The shape is the drape's: distances between points are kept, not the flat layout's.
    let d = |s: &State, a: usize, b: usize| s.x[a].distance(s.x[b]);
    let (a, b) = (range.start, range.end - 1);
    assert!((d(&next, a, b) - d(&state, a, b)).abs() < 1e-12);
    assert!((d(&next, a, b) - d(&moved.garment.initial_state, a, b)).abs() > 1e-4);

    // Never simulated: the new arrangement, as before.
    assert_eq!(
        moved.state_moving_drape(&t, &g, &g.initial_state).x,
        moved.state_after(&g, &g.initial_state).x
    );
    // An edit that is not a move behaves as keeping the drape does.
    let reshaped = authoring::apply(
        &t,
        Command::SetLength {
            panel: id,
            edge: top_edge(&t),
            length_m: 0.46,
            moving: SegmentEnd::End,
        },
    )
    .unwrap();
    assert_eq!(
        reshaped.state_moving_drape(&t, &g, &state).x,
        reshaped.state_keeping_drape(&g, &state).x
    );
}
