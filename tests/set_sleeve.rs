//! `set_sleeve`: a sleeve the agent unsewed and moved away by hand is set back into its armhole
//! by the engine, and the armholes close when it drapes on the stock mannequin.
use fashioncraft::{
    agent_build,
    authoring::{self, Command},
    avatar::SurfaceMesh,
    fitting::AvatarPose,
    garment::presets::{ShirtMeasurements, shirt_around},
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use serde_json::json;
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

fn arms() -> [[DVec3; 2]; 2] {
    let joints = AvatarPose::from_sidecar(&mannequin()).unwrap().joints;
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

#[test]
fn sleeves_moved_by_hand_are_set_back_and_their_armholes_close() {
    let body = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let arms = arms();
    let shirt = shirt_around(
        &ShirtMeasurements {
            arms,
            ..Default::default()
        },
        1,
        Some(&body.vertices),
    )
    .unwrap();
    // As an agent left them: unsewn, unwrapped and moved off to the side.
    let sleeve_seams: Vec<String> = shirt
        .seams
        .iter()
        .filter(|s| s.references().any(|r| r.panel.starts_with("sleeve-")))
        .map(|s| s.id.clone())
        .collect();
    let mut t = authoring::apply(
        &shirt,
        Command::RemoveSeams {
            seams: sleeve_seams,
        },
    )
    .unwrap()
    .template;
    for (id, dx) in [("sleeve-right", 0.6), ("sleeve-left", -0.6)] {
        let mut placement = t
            .panels
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .placement
            .clone();
        placement.translation += DVec3::new(dx, -0.3, 0.2);
        t = authoring::apply(
            &t,
            Command::Arrange {
                panel: id.into(),
                placement,
                wrap: None,
            },
        )
        .unwrap()
        .template;
    }
    // The engine sets each back: the shirt's "right" pieces are on +x, the avatar's left.
    for (id, side, armhole) in [
        ("sleeve-right", "left", "armhole-right"),
        ("sleeve-left", "right", "armhole-left"),
    ] {
        let build = agent_build::set_sleeve(
            &t,
            &json!({
                "sleeve": id, "side": side,
                "cap": ["cap-right", "cap-left"],
                "underarm": ["underarm-right", "underarm-left"],
                // Back first: the order does not matter.
                "armhole": [{"piece": "back", "segment": armhole}, {"piece": "front", "segment": armhole}],
            }),
            arms,
            Some(&body.vertices),
        )
        .unwrap_or_else(|e| panic!("{id}: {e}"));
        t = agent_build::applied(&t, &build).unwrap();
    }
    assert_eq!(t.seams.len(), shirt.seams.len());
    for id in ["sleeve-right", "sleeve-left"] {
        let set = &t.panels.iter().find(|p| p.id == id).unwrap().placement;
        let drafted = &shirt.panels.iter().find(|p| p.id == id).unwrap().placement;
        assert!(
            set.translation.distance(drafted.translation) < 0.02,
            "{id} starts where the shirt draft puts it"
        );
    }

    let garment = std::sync::Arc::new(t.compile().unwrap());
    let mut state = garment.initial_state.clone();
    let mut solver = VbdSolver::new(garment.clone(), Some(&body), &state).unwrap();
    let settings = VbdSettings::default();
    for _ in 0..300 {
        solver.step(&mut state, &settings, None).unwrap();
    }
    for seam in garment.seams.iter().filter(|s| {
        t.seams
            .iter()
            .find(|spec| spec.id == s.id)
            .is_some_and(|spec| spec.references().any(|r| r.panel.starts_with("sleeve-")))
    }) {
        let gap = seam.max_gap(&state.x);
        assert!(
            gap < 0.008,
            "seam {} still {:.1} mm open",
            seam.id,
            gap * 1000.
        );
    }
}

#[test]
fn a_puffed_sleeve_is_gathered_into_the_armhole() {
    let body = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let arms = arms();
    let shirt = shirt_around(
        &ShirtMeasurements {
            arms,
            ..Default::default()
        },
        1,
        Some(&body.vertices),
    )
    .unwrap();
    // A wider sleeve: its cap is much longer than the armhole.
    let wide = agent_build::applied(
        &shirt,
        &agent_build::Build {
            commands: vec![Command::ScalePanel {
                panel: "sleeve-right".into(),
                scale: glam::DVec2::new(1.5, 1.),
            }],
            names: vec![],
            done: String::new(),
            history_name: "",
            lay: None,
        },
    )
    .unwrap();
    let build = agent_build::set_sleeve(
        &wide,
        &json!({
            "sleeve": "sleeve-right", "side": "left",
            "cap": ["cap-right", "cap-left"], "underarm": ["underarm-right", "underarm-left"],
            "armhole": [{"piece": "front", "segment": "armhole-right"}, {"piece": "back", "segment": "armhole-right"}],
        }),
        arms,
        Some(&body.vertices),
    )
    .unwrap();
    assert!(build.done.contains("replacing seams"));
    let t = agent_build::applied(&wide, &build).unwrap();
    let cap_seams: Vec<_> = t
        .seams
        .iter()
        .filter(|s| {
            s.references().any(|r| {
                r.panel == "sleeve-right" && r.edge.as_deref().is_some_and(|e| e.starts_with("cap"))
            })
        })
        .collect();
    assert_eq!(cap_seams.len(), 2);
    assert!(
        cap_seams.iter().all(|s| s.allow_ease),
        "gathered: ease allowed"
    );
    t.compile().unwrap();
}
