//! Prototype checks for landmark-referenced arrangement anchors (EXO-AV-002 scope review).
use glam::{DVec2, DVec3};
use marvelous_cloth::{
    avatar::{SurfaceMesh, diagnostics},
    avatar_measurements::{AvatarFrame, Landmark},
    garment::{
        GarmentTemplate,
        arrangement::{ArrangementSet, Direction, placement_for},
        presets::{self, GarmentKind},
    },
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn frame() -> AvatarFrame {
    AvatarFrame {
        avatar_id: "makehuman-mannequin".into(),
        revision: 1,
    }
}

fn landmarks() -> Vec<Landmark> {
    #[derive(serde::Deserialize)]
    struct Prepared {
        landmarks: BTreeMap<String, [f64; 3]>,
    }
    let p: Prepared =
        serde_json::from_str(include_str!("../assets/makehuman/measurements.json")).unwrap();
    p.landmarks
        .into_iter()
        .map(|(id, v)| Landmark {
            id,
            position_m: DVec3::from_array(v),
        })
        .collect()
}

fn set() -> ArrangementSet {
    ArrangementSet::from_json(include_str!("../assets/makehuman/arrangement.json")).unwrap()
}

fn landmark(id: &str) -> DVec3 {
    landmarks()
        .into_iter()
        .find(|l| l.id == id)
        .unwrap()
        .position_m
}

fn rest_points(t: &GarmentTemplate, panel: &str) -> Vec<DVec2> {
    t.panels
        .iter()
        .find(|p| p.id == panel)
        .unwrap()
        .polygon
        .as_ref()
        .unwrap()
        .points
        .clone()
}

#[test]
fn weighted_origin_and_orthonormal_axes() {
    let s = set();
    s.validate(&frame(), &landmarks()).unwrap();
    let f = s.evaluate(&frame(), &landmarks(), "torso-front").unwrap();
    let expected = (landmark("joint-spine-1") + landmark("joint-spine-2")) / 2.0;
    assert!((f.origin - expected).length() < 1e-12);
    let arm = s
        .evaluate(&frame(), &landmarks(), "upper-arm-left-front")
        .unwrap();
    for g in [f, arm] {
        assert!((g.right.length() - 1.0).abs() < 1e-12 && (g.up.length() - 1.0).abs() < 1e-12);
        assert!(g.right.dot(g.up).abs() < 1e-12);
        assert!(g.right.cross(g.up).dot(g.outward).abs() > 0.99);
    }
    // the arm's up axis follows the bone from elbow to shoulder
    let bone = (landmark("joint-l-shoulder") - landmark("joint-l-elbow")).normalize();
    assert!((arm.up - bone).length() < 1e-12);
}

#[test]
fn invalid_sets_are_rejected_as_a_whole() {
    let lm = landmarks();
    let mut bad = set();
    bad.anchors[2].origin[0].landmark = "joint-missing".into();
    assert!(
        bad.validate(&frame(), &lm)
            .unwrap_err()
            .contains("unknown landmark")
    );
    assert!(
        bad.evaluate(&frame(), &lm, "torso-front").is_err(),
        "a bad anchor must reject the whole set"
    );

    let mut bad = set();
    bad.anchors[0].origin[0].weight = 0.7;
    assert!(
        bad.validate(&frame(), &lm)
            .unwrap_err()
            .contains("sum to 1")
    );

    let mut bad = set();
    bad.anchors[1].id = "torso-front".into();
    assert!(
        bad.validate(&frame(), &lm)
            .unwrap_err()
            .contains("duplicate")
    );

    let mut bad = set();
    bad.anchors[0].up = Direction::Fixed {
        vector: DVec3::ZERO,
    };
    assert!(bad.validate(&frame(), &lm).unwrap_err().contains("zero"));

    let mut bad = set();
    bad.anchors[0].up = Direction::Fixed { vector: DVec3::Z };
    assert!(
        bad.validate(&frame(), &lm).is_err(),
        "right × up must face outward"
    );

    let mut bad = set();
    bad.anchors[0].clearance_m = -0.01;
    assert!(
        bad.validate(&frame(), &lm)
            .unwrap_err()
            .contains("clearance")
    );

    let other = AvatarFrame {
        revision: 2,
        ..frame()
    };
    assert!(
        set()
            .validate(&other, &lm)
            .unwrap_err()
            .contains("reconfirm")
    );
}

#[test]
fn placement_puts_the_panel_centre_on_the_anchor_with_its_axes() {
    let t = presets::basic(GarmentKind::Tshirt, 2);
    let pts = rest_points(&t, "front");
    for id in [
        "torso-front",
        "torso-back",
        "torso-left",
        "torso-right",
        "upper-arm-left-front",
    ] {
        let f = set().evaluate(&frame(), &landmarks(), id).unwrap();
        let p = placement_for(&pts, &f, 0.2).unwrap();
        let (lo, hi) = pts
            .iter()
            .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(a, b), q| {
                (a.min(*q), b.max(*q))
            });
        let c = (lo + hi) / 2.0;
        let local = DVec3::new(c.x, -c.y, 0.0);
        let at = p.transform(local);
        assert!(
            (at - (f.origin + f.outward * 0.2)).length() < 1e-9,
            "{id}: centre off by {}",
            (at - (f.origin + f.outward * 0.2)).length()
        );
        assert!(
            (p.transform(local + DVec3::X) - at - f.right).length() < 1e-9,
            "{id}: u is not along right"
        );
        assert!(
            (p.transform(local + DVec3::Y) - at - f.up).length() < 1e-9,
            "{id}: −v is not along up"
        );
    }
}

#[test]
fn placement_never_changes_rest_geometry() {
    let mut t = presets::basic(GarmentKind::Tshirt, 2);
    let before = t.compile().unwrap();
    let f = set()
        .evaluate(&frame(), &landmarks(), "torso-front")
        .unwrap();
    let pts = rest_points(&t, "front");
    t.panels
        .iter_mut()
        .find(|p| p.id == "front")
        .unwrap()
        .placement = placement_for(&pts, &f, 0.25).unwrap();
    let after = t.compile().unwrap();
    assert_eq!(
        serde_json::to_string(&before.mesh.uv).unwrap(),
        serde_json::to_string(&after.mesh.uv).unwrap()
    );
    let tris = |g: &marvelous_cloth::garment::CompiledGarment| {
        g.mesh
            .triangles
            .iter()
            .map(|t| t.vertices)
            .collect::<Vec<_>>()
    };
    assert_eq!(tris(&before), tris(&after));
    assert_eq!(before.mesh.rest_area, after.mesh.rest_area);
    assert_eq!(before.mesh.masses, after.mesh.masses);
    assert_ne!(
        serde_json::to_string(&before.initial_state.x).unwrap(),
        serde_json::to_string(&after.initial_state.x).unwrap()
    );
}

fn placed_tshirt(front_clearance: f64, back_clearance: f64) -> GarmentTemplate {
    let mut t = presets::basic(GarmentKind::Tshirt, 2);
    let s = set();
    for (panel, anchor, c) in [
        ("front", "torso-front", front_clearance),
        ("back", "torso-back", back_clearance),
    ] {
        let f = s.evaluate(&frame(), &landmarks(), anchor).unwrap();
        let pts = rest_points(&t, panel);
        t.panels
            .iter_mut()
            .find(|p| p.id == panel)
            .unwrap()
            .placement = placement_for(&pts, &f, c).unwrap();
    }
    t
}

#[test]
fn initialization_gate_accepts_clear_and_rejects_intersecting_anchored_placement() {
    let avatar = SurfaceMesh::load_obj(Path::new("assets/makehuman/mannequin.obj")).unwrap();
    let ok = Arc::new(placed_tshirt(0.25, 0.2).compile().unwrap());
    let state = ok.initial_state.clone();
    diagnostics::check_vbd_initialization(ok, &avatar, &state)
        .expect("anchored default clearances should start separated");
    let inside = Arc::new(placed_tshirt(0.02, 0.2).compile().unwrap());
    let state = inside.initial_state.clone();
    assert!(
        diagnostics::check_vbd_initialization(inside, &avatar, &state).is_err(),
        "a front panel 2 cm from the spine starts inside the body"
    );
}
