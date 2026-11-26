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

/// The avatar's arms, shoulder to elbow, as draft_garment's shirt wraps its sleeves round them.
fn arms() -> [[DVec3; 2]; 2] {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let joints = fashioncraft::fitting::AvatarPose::from_sidecar(&file)
        .unwrap()
        .joints;
    let joint = |id: &str| DVec3::from_array(joints[id]);
    let mut arms = [
        [joint("joint-l-shoulder"), joint("joint-l-elbow")],
        [joint("joint-r-shoulder"), joint("joint-r-elbow")],
    ];
    if arms[0][0].x < arms[1][0].x {
        arms.swap(0, 1);
    }
    arms
}

/// The farthest a seam's side `a` is from its side `b`: every vertex of one side's boundary to
/// the nearest point of the other's, in the simulated state.
fn side_distance(
    template: &fashioncraft::garment::GarmentTemplate,
    garment: &fashioncraft::garment::CompiledGarment,
    x: &[DVec3],
    seam: &str,
) -> f64 {
    let spec = template.seams.iter().find(|s| s.id == seam).unwrap();
    let boundary = |r: &fashioncraft::garment::seam::BoundaryRef| -> Vec<DVec3> {
        let panel = garment.panels.iter().find(|p| p.id == r.panel).unwrap();
        panel.boundaries[r.edge.as_deref().unwrap()]
            .iter()
            .map(|v| x[*v])
            .collect()
    };
    let (a, b) = (boundary(&spec.a), boundary(&spec.b));
    let to_b = |p: DVec3| {
        b.windows(2)
            .map(|s| {
                let d = s[1] - s[0];
                let t = ((p - s[0]).dot(d) / d.length_squared().max(1e-12)).clamp(0., 1.);
                p.distance(s[0] + d * t)
            })
            .fold(f64::INFINITY, f64::min)
    };
    a.iter().map(|p| to_b(*p)).fold(0., f64::max)
}

#[test]
fn a_shirt_with_set_in_sleeves_added_over_a_skirt_closes_like_the_shirt_alone() {
    // The user's case: a draped skirt, a shirt added over it. Its eight seams that meet at the
    // armpits read 60-70 mm open and stayed so, because a paste dropped the sleeves' wrap round
    // the arm: they started flat and the armpit corner never closed.
    let body = mannequin();
    let axis = body.vertices.iter().copied().sum::<DVec3>() / body.vertices.len() as f64;
    let settings = VbdSettings::default();
    let skirt = skirt(&SkirtMeasurements::default(), 1).unwrap();
    let draped = Arc::new(skirt.compile().unwrap());
    let mut state = draped.initial_state.clone();
    let mut solver = VbdSolver::new(draped.clone(), Some(&body), &state).unwrap();
    for _ in 0..150 {
        solver.step(&mut state, &settings, None).unwrap();
    }
    let shirt = fashioncraft::garment::presets::shirt_around(
        &fashioncraft::garment::presets::ShirtMeasurements {
            arms: arms(),
            ..Default::default()
        },
        1,
        Some(&body.vertices),
    )
    .unwrap();
    let contact = settings.contact.clone();
    let (build, _) = agent_build::wear_over(&skirt, &shirt, "shirt", axis, true, &mut |e, a| {
        agent_build::starts_clear(e, a, &draped, &state, &body, &contact)
    })
    .unwrap();
    let both = agent_build::applied(&skirt, &build).unwrap();
    assert_eq!(
        both.panel_wraps.len(),
        shirt.panel_wraps.len(),
        "the sleeves keep their wrap"
    );

    let drape = |garment: Arc<fashioncraft::garment::CompiledGarment>,
                 mut state: fashioncraft::model::State| {
        let mut solver = VbdSolver::new(garment, Some(&body), &state).unwrap();
        for _ in 0..300 {
            solver.step(&mut state, &settings, None).unwrap();
        }
        state
    };
    let edit = agent_build::compose(&skirt, &build.commands).unwrap();
    let garment = Arc::new(edit.garment.clone());
    let worn = drape(garment.clone(), edit.state_keeping_drape(&draped, &state));
    let alone_garment = Arc::new(shirt.compile().unwrap());
    let alone = drape(alone_garment.clone(), alone_garment.initial_state.clone());

    // Every seam of the added shirt closes, and what the agent is told agrees with the
    // distance between the seam's two sides.
    let live = fashioncraft::agent_ops::Live {
        garment: &garment,
        x: &worn.x,
        simulated: true,
    };
    let told = fashioncraft::agent_ops::describe_seams(&both, Some(&live));
    let added: Vec<&serde_json::Value> = told
        .iter()
        .filter(|s| s["id"].as_str().unwrap().starts_with("seam_"))
        .collect();
    assert_eq!(added.len(), shirt.seams.len());
    for seam in added {
        let id = seam["id"].as_str().unwrap();
        let gap = seam["gap_mm"].as_f64().unwrap();
        let apart = side_distance(&both, &garment, &worn.x, id) * 1000.;
        assert!(gap < 8., "{id} reads {gap} mm open");
        assert!(
            (gap - apart).abs() < 5.,
            "{id}: told {gap} mm, sides {apart:.1} mm apart"
        );
    }
    // Strain on each piece is as on the shirt worn alone.
    let worst = |template: &fashioncraft::garment::GarmentTemplate,
                 garment: &fashioncraft::garment::CompiledGarment,
                 x: &[DVec3],
                 v: &[DVec3]| {
        let report = fashioncraft::agent_check::check(&fashioncraft::agent_check::Input {
            template,
            live: fashioncraft::agent_ops::Live {
                garment,
                x,
                simulated: true,
            },
            v,
            body: None,
            pressure: None,
            body_cm: None,
        });
        report["strain_over_limit"].as_array().map_or(0., |s| {
            s.iter()
                .filter_map(|e| e["worst"].as_f64())
                .fold(0., f64::max)
        })
    };
    let worn_strain = worst(&both, &garment, &worn.x, &worn.v);
    let alone_strain = worst(&shirt, &alone_garment, &alone.x, &alone.v);
    assert!(
        worn_strain <= alone_strain + 15.,
        "strain {worn_strain} % worn over the skirt, {alone_strain} % alone"
    );
}

#[test]
fn sleeves_wrapped_round_the_arms_are_not_pushed_out_with_the_body() {
    // Pushed out to start clear of the skirt, the shirt's front and back move out; its sleeves
    // stay round the arms: moved sideways they sat off the arm and caught on it as they fell
    // (the app: underarm seams 75 mm open, the arm between their sides).
    let skirt = skirt(&SkirtMeasurements::default(), 1).unwrap();
    let shirt = fashioncraft::garment::presets::shirt_around(
        &fashioncraft::garment::presets::ShirtMeasurements {
            arms: arms(),
            ..Default::default()
        },
        1,
        None,
    )
    .unwrap();
    let mut tries = 0;
    let (build, push) =
        agent_build::wear_over(&skirt, &shirt, "shirt", DVec3::ZERO, true, &mut |_, _| {
            tries += 1;
            tries > 1
        })
        .unwrap();
    assert!(push > 0.);
    let both = agent_build::applied(&skirt, &build).unwrap();
    for (p, drafted) in both.panels[skirt.panels.len()..].iter().zip(&shirt.panels) {
        let moved = p
            .placement
            .translation
            .distance(drafted.placement.translation);
        if shirt.panel_wraps.contains_key(&drafted.id) {
            assert!(moved < 1e-9, "{} moved {moved}", drafted.id);
        } else {
            assert!((moved - push).abs() < 1e-6, "{} moved {moved}", drafted.id);
        }
    }
}
