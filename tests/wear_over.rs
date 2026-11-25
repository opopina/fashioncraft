//! `wear_over`: a second garment drafted for the avatar is added over the draped one (a t-shirt
//! with a skirt), keeping its own arrangement around the avatar, on an outer layer, starting
//! clear of the skirt as it hangs; the skirt stays.
use fashioncraft::{
    agent_build,
    avatar::SurfaceMesh,
    garment::presets::{SkirtMeasurements, TshirtMeasurements, skirt, tshirt},
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::{path::PathBuf, sync::Arc};

fn mannequin() -> SurfaceMesh {
    SurfaceMesh::load_obj(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .unwrap()
}

#[test]
fn a_tshirt_goes_over_a_draped_skirt_and_the_skirt_stays() {
    let body = mannequin();
    let axis = body.vertices.iter().copied().sum::<DVec3>() / body.vertices.len() as f64;
    let skirt = skirt(&SkirtMeasurements::default(), 1).unwrap();
    let draped = Arc::new(skirt.compile().unwrap());
    let mut state = draped.initial_state.clone();
    let mut solver = VbdSolver::new(draped.clone(), Some(&body), &state).unwrap();
    for _ in 0..150 {
        solver
            .step(&mut state, &VbdSettings::default(), None)
            .unwrap();
    }

    let shirt = tshirt(&TshirtMeasurements::default(), 1).unwrap();
    let contact = VbdSettings::default().contact;
    let mut tries = 0;
    let (build, push) =
        agent_build::wear_over(&skirt, &shirt, "t-shirt", axis, true, &mut |edit, added| {
            tries += 1;
            agent_build::starts_clear(edit, added, &draped, &state, &body, &contact)
        })
        .unwrap();
    assert!(tries >= 1 && push <= 0.08);
    assert_eq!(build.history_name, "Add garment");
    let both = agent_build::applied(&skirt, &build).unwrap();
    assert_eq!(both.panels.len(), skirt.panels.len() + shirt.panels.len());
    assert_eq!(both.seams.len(), skirt.seams.len() + shirt.seams.len());
    // The skirt's pieces are untouched; the t-shirt's are named after it, on the outer layer.
    for p in &skirt.panels {
        let kept = both.panels.iter().find(|q| q.id == p.id).unwrap();
        assert_eq!(kept.placement.translation, p.placement.translation);
        assert_eq!(kept.name, p.name);
    }
    for (p, drafted) in both.panels[skirt.panels.len()..].iter().zip(&shirt.panels) {
        assert!(
            p.name.starts_with("t-shirt ") && !p.name.contains("tshirt"),
            "{}",
            p.name
        );
        assert_eq!(both.panel_layers.get(&p.id), Some(&1));
        // Where it was drafted around the avatar, at most `push` out from it: not moved aside
        // as a paste would.
        let moved = p.placement.translation - drafted.placement.translation;
        assert!(moved.y.abs() < 1e-9, "{}: {moved:?}", p.name);
        assert!(
            DVec3::new(moved.x, 0., moved.z).length() <= push + 1e-9,
            "{}: {moved:?}",
            p.name
        );
    }

    // It starts as the window starts it, the skirt keeping its drape, and drapes from there.
    let edit = agent_build::compose(&skirt, &build.commands).unwrap();
    let garment = Arc::new(edit.garment.clone());
    let mut state = edit.state_keeping_drape(&draped, &state);
    let mut solver = VbdSolver::new(garment, Some(&body), &state).unwrap();
    for _ in 0..120 {
        solver
            .step(&mut state, &VbdSettings::default(), None)
            .unwrap();
    }
    assert!(state.x.iter().all(|x| x.is_finite()));
}

#[test]
fn a_garment_still_laid_out_flat_is_draped_before_another_goes_over_it() {
    // Laid out flat together, a skirt and a t-shirt close in on the body at once and catch on
    // each other within 20 steps (each alone runs on): the skirt is draped first.
    let skirt = skirt(&SkirtMeasurements::default(), 1).unwrap();
    let shirt = tshirt(&TshirtMeasurements::default(), 1).unwrap();
    let mut asked = false;
    let refused = agent_build::wear_over(
        &skirt,
        &shirt,
        "t-shirt",
        DVec3::ZERO,
        false,
        &mut |_, _| {
            asked = true;
            true
        },
    )
    .err()
    .unwrap();
    assert!(refused.contains("simulate it first"), "{refused}");
    assert!(!asked);
}
