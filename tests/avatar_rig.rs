//! The avatar's skeleton and skin: built from its joint landmarks, it leaves the body as it
//! is at rest and bends each limb about its joint without dragging the rest of the body.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    avatar_rig::{Pose, Rig, preset},
    garment::arrangement::load_sidecar,
};
use std::{collections::BTreeMap, path::PathBuf};

fn mannequin() -> (SurfaceMesh, BTreeMap<String, DVec3>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let surface = SurfaceMesh::load_obj(&path).unwrap();
    let landmarks = load_sidecar(&path)
        .unwrap()
        .unwrap()
        .landmarks
        .into_iter()
        .map(|l| (l.id, l.position_m))
        .collect();
    (surface, landmarks)
}

#[test]
fn the_rest_pose_leaves_the_body_untouched_and_weights_are_normalised() {
    let (surface, landmarks) = mannequin();
    let rig = Rig::new(&landmarks, &surface).unwrap();
    assert_eq!(rig.weights.len(), surface.vertices.len());
    for w in &rig.weights {
        assert!(!w.is_empty() && w.len() <= 4);
        let sum: f64 = w.iter().map(|p| p.1).sum();
        assert!((sum - 1.).abs() < 1e-9);
    }
    let rest = rig.posed(&surface, &Pose::default());
    for (a, b) in rest.vertices.iter().zip(&surface.vertices) {
        assert!(a.distance(*b) < 1e-12);
    }
}

#[test]
fn bending_the_left_elbow_moves_the_left_hand_only() {
    let (surface, landmarks) = mannequin();
    let rig = Rig::new(&landmarks, &surface).unwrap();
    let near = |id: &str| {
        let at = landmarks[id];
        (0..surface.vertices.len())
            .min_by(|&a, &b| {
                surface.vertices[a]
                    .distance(at)
                    .total_cmp(&surface.vertices[b].distance(at))
            })
            .unwrap()
    };
    let (left_hand, right_hand, head, elbow) = (
        near("joint-l-hand"),
        near("joint-r-hand"),
        near("joint-head-2"),
        landmarks["joint-l-elbow"],
    );
    let mut pose = Pose::default();
    pose.0.insert("forearm.L".into(), [0., 0., 60.]);
    let posed = rig.posed(&surface, &pose);
    let moved = |v: usize| posed.vertices[v].distance(surface.vertices[v]);
    assert!(
        moved(left_hand) > 0.1,
        "left hand moved {}",
        moved(left_hand)
    );
    assert!(moved(right_hand) < 1e-9);
    assert!(moved(head) < 1e-9);
    // The hand turns about the elbow: its distance to the elbow is kept.
    let before = surface.vertices[left_hand].distance(elbow);
    let after = posed.vertices[left_hand].distance(elbow);
    assert!((before - after).abs() < 0.005, "{before} vs {after}");
}

#[test]
fn children_follow_their_parents_and_presets_are_valid() {
    let (surface, landmarks) = mannequin();
    let rig = Rig::new(&landmarks, &surface).unwrap();
    let pose = preset("T-pose").unwrap();
    pose.validate().unwrap();
    let bones = rig.posed_bones(&pose);
    for (i, bone) in rig.bones.iter().enumerate() {
        if let Some(p) = bone.parent {
            // A child that starts where its parent ends still does once posed.
            if rig.bones[p].tail.distance(bone.head) < 1e-12 {
                assert!(bones[p].1.distance(bones[i].0) < 1e-9, "{}", bone.name);
            }
        }
    }
    // In the T-pose the hands are higher and farther out than at rest.
    let hand = rig.bone("hand.L").unwrap();
    assert!(bones[hand].0.y > landmarks["joint-l-hand"].y + 0.15);
    assert!(bones[hand].0.x > landmarks["joint-l-hand"].x);
    // Arms down brings the hand closer to the body.
    let down = rig.posed_bones(&preset("Arms down").unwrap());
    assert!(down[hand].0.x < landmarks["joint-l-hand"].x - 0.05);
    assert!(
        Pose([("tail".to_string(), [0.; 3])].into())
            .validate()
            .is_err()
    );
}

#[test]
fn a_posed_project_compiles_with_the_posed_body_and_keeps_it_when_saved() {
    use marvelous_cloth::{
        fitting::{AvatarPose, FittingProject},
        garment::presets::GarmentKind,
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, path.clone());
    let rest = project.compile().unwrap();
    let mut pose = AvatarPose::from_sidecar(&path).unwrap();
    pose.pose = preset("T-pose").unwrap();
    project.avatar_pose = Some(pose);
    let posed = project.compile().unwrap();
    let highest = |s: &marvelous_cloth::avatar::SurfaceMesh| {
        s.vertices
            .iter()
            .filter(|v| v.x > 0.3)
            .map(|v| v.y)
            .fold(f64::MIN, f64::max)
    };
    // In the T-pose the left arm's outer part is raised.
    assert!(highest(&posed.collider.surface) > highest(&rest.collider.surface) + 0.1);
    // Saved, the avatar moves into the project's assets, away from its landmark files: the
    // joints travel with the document, so it still opens posed.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("posed.json");
    project.save(&file).unwrap();
    let reopened = FittingProject::load(&file).unwrap();
    assert_eq!(reopened.avatar_pose, project.avatar_pose);
    let again = reopened.compile().unwrap();
    assert!((highest(&again.collider.surface) - highest(&posed.collider.surface)).abs() < 1e-9);
}
