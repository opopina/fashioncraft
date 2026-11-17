//! Deactivated pieces stay put and take part in no contact; superimposed pieces start on
//! the piece they are sewn to, outside or inside it.
use glam::DVec3;
use marvelous_cloth::{
    authoring,
    garment::{GarmentTemplate, transfer},
    ogc::{OgcSettings, OgcWorld},
    vbd::VbdSolver,
};
use std::sync::Arc;

/// Two flat panels lying on top of each other, `gap` apart, both facing +y.
fn stacked(gap: f64) -> GarmentTemplate {
    let mut t = GarmentTemplate::two_panel_demo();
    t.seams.clear();
    for (i, p) in t.panels.iter_mut().enumerate() {
        p.pins.clear();
        p.pattern.nx = 2;
        p.pattern.ny = 2;
        p.placement.rotation_degrees = DVec3::new(90., 0., 0.);
        p.placement.translation = DVec3::new(0., 1. + i as f64 * gap, 0.);
    }
    t
}

#[test]
fn deactivated_pieces_take_part_in_no_contact() {
    let t = stacked(0.002);
    let g = t.compile().unwrap();
    let faces: Vec<[usize; 3]> = g.mesh.triangles.iter().map(|tr| tr.vertices).collect();
    let n = g.mesh.uv.len();
    let settings = OgcSettings::default();
    let contacts = |inert: Vec<bool>| {
        let mut world = OgcWorld::new(&g.initial_state.x, faces.clone(), n, []).unwrap();
        world.set_inert(inert).unwrap();
        world
            .detect(&g.initial_state.x, 0.01, &settings)
            .unwrap()
            .contacts
            .len()
    };
    assert!(contacts(vec![]) > 0, "2 mm apart: in contact");
    let mut inert = vec![false; n];
    inert[g.panels[0].vertices.clone()].fill(true);
    assert_eq!(contacts(inert), 0, "nothing touches a deactivated piece");
}

#[test]
fn a_deactivated_piece_may_start_inside_another() {
    // Two panels crossing each other: no simulation may start from this.
    let mut t = stacked(0.0);
    t.panels[1].placement.rotation_degrees = DVec3::ZERO;
    let bounds = |g: &marvelous_cloth::garment::CompiledGarment, i: usize| {
        g.initial_state.x[g.panels[i].vertices.clone()].iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        )
    };
    let probe = t.compile().unwrap();
    let ((lo0, hi0), (lo1, hi1)) = (bounds(&probe, 0), bounds(&probe, 1));
    // Centre the vertical panel on the horizontal one, straddling its plane.
    let centre0 = (lo0 + hi0) * 0.5;
    let centre1 = (lo1 + hi1) * 0.5;
    t.panels[1].placement.translation += DVec3::new(
        centre0.x - centre1.x,
        centre0.y - centre1.y,
        centre0.z - centre1.z,
    );
    let g = t.compile().unwrap();
    let state = g.initial_state.clone();
    assert!(VbdSolver::new(Arc::new(g.clone()), None, &state).is_err());
    let first = t.panels[0].id.clone();
    let (edit, next) = authoring::prepare_deactivate_edit(
        &t,
        &g,
        &state,
        std::slice::from_ref(&first),
        true,
        false,
    )
    .unwrap();
    assert!(
        edit.garment.inert[edit.garment.panels[0].vertices.clone()]
            .iter()
            .all(|i| *i)
    );
    assert_eq!(next.x, state.x, "it stays where it is");
    VbdSolver::new(Arc::new(edit.garment.clone()), None, &next)
        .expect("the other piece passes through it");
    // Activating it again restores the old behaviour, where it is.
    let (back, after) = authoring::prepare_deactivate_edit(
        &edit.template,
        &edit.garment,
        &next,
        &[first],
        false,
        false,
    )
    .unwrap();
    assert!(back.template.deactivated_panels.is_empty());
    assert_eq!(after.x, next.x);
    assert!(back.garment.inert.iter().all(|i| !*i));
}

#[test]
fn deactivating_with_sewing_leaves_its_seams_out() {
    let t = GarmentTemplate::two_panel_demo();
    let g = t.compile().unwrap();
    let state = g.initial_state.clone();
    assert!(!g.seams.is_empty());
    let first = t.panels[0].id.clone();
    let (only, _) = authoring::prepare_deactivate_edit(
        &t,
        &g,
        &state,
        std::slice::from_ref(&first),
        true,
        false,
    )
    .unwrap();
    assert_eq!(
        only.garment.seams.len(),
        g.seams.len(),
        "pattern only: seams stay"
    );
    let (both, _) =
        authoring::prepare_deactivate_edit(&t, &g, &state, &[first], true, true).unwrap();
    assert!(
        both.garment.seams.len() < g.seams.len(),
        "pattern and sewing"
    );
    assert_eq!(
        both.template.seams.len(),
        t.seams.len(),
        "the document keeps them"
    );
    let text = serde_json::to_string(&both.template).unwrap();
    let back: GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(back.deactivated_panels, both.template.deactivated_panels);
}

#[test]
fn superimposed_pieces_lie_on_the_target_outside_or_inside() {
    // A cylinder of radius 0.2 around the y axis, and a few points far from it.
    let (r, rings, around) = (0.2, 6, 24);
    let mut x = vec![];
    for k in 0..rings {
        for j in 0..around {
            let a = std::f64::consts::TAU * j as f64 / around as f64;
            x.push(DVec3::new(r * a.cos(), 0.8 + 0.05 * k as f64, r * a.sin()));
        }
    }
    let mut triangles = vec![];
    for k in 0..rings - 1 {
        for j in 0..around {
            let (a, b) = (k * around + j, k * around + (j + 1) % around);
            let (c, d) = (a + around, b + around);
            triangles.push([a, b, c]);
            triangles.push([b, d, c]);
        }
    }
    // A piece lying near the cylinder, one point outside and one inside, both facing the
    // middle of a facet.
    let facet = |radius: f64, degrees: f64| {
        let a = degrees.to_radians();
        DVec3::new(radius * a.cos(), 0.9, radius * a.sin())
    };
    let moved = [facet(0.23, 7.5), facet(0.17, 187.5)];
    // Facet planes sit r·cos 7.5° from the axis.
    let plane = r * (std::f64::consts::PI / 24.).cos();
    for (over, expected) in [(true, plane + 0.005), (false, plane - 0.005)] {
        let laid =
            transfer::superimpose_positions(&moved, &x, &triangles, DVec3::ZERO, over, 0.005);
        for p in laid {
            let radial = DVec3::new(p.x, 0., p.z).length();
            assert!((radial - expected).abs() < 1e-9, "{over}: {radial}");
        }
    }
}
