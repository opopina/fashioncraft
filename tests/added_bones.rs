//! Bones added to the avatar's skeleton and skin weights painted by hand.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    avatar_rig::{AddedBone, Pose, paint_weights},
    fitting::AvatarPose,
};
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

fn pose(bones: &[(&str, [f64; 3])]) -> Pose {
    Pose(bones.iter().map(|(b, a)| (b.to_string(), *a)).collect())
}

#[test]
fn an_added_bone_takes_the_skin_nearest_it_and_turns_it() {
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let mut avatar = AvatarPose::from_sidecar(&mannequin()).unwrap();
    let plain = avatar.rig(&rest).unwrap();
    // A short bone at the back of the head, hanging down from it like a ponytail's root.
    let head = plain.bones[plain.bone("head").unwrap()].clone();
    let back = rest
        .vertices
        .iter()
        .filter(|p| p.y > head.head.y)
        .min_by(|a, b| a.z.total_cmp(&b.z))
        .copied()
        .unwrap();
    let root = DVec3::new(head.head.x, back.y, back.z + 0.02);
    avatar.added_bones.push(AddedBone {
        name: "ponytail".into(),
        parent: "head".into(),
        head: root.to_array(),
        tail: (root - DVec3::Y * 0.08).to_array(),
    });
    let rig = avatar.rig(&rest).unwrap();
    assert_eq!(rig.bones.len(), plain.bones.len() + 1);
    let tail = rig.bone("ponytail").unwrap();
    assert_eq!(rig.bones[tail].parent, rig.bone("head"));
    // Skin at the back of the head follows it now; the feet do not.
    let near: Vec<usize> = (0..rest.vertices.len())
        .filter(|v| rest.vertices[*v].distance(root) < 0.03)
        .collect();
    assert!(!near.is_empty());
    assert!(
        near.iter()
            .all(|v| rig.weights[*v].iter().any(|(b, _)| *b == tail))
    );
    let turned = rig.posed(&rest, &pose(&[("ponytail", [40., 0., 0.])]));
    let moved = near
        .iter()
        .map(|v| turned.vertices[*v].distance(rest.vertices[*v]))
        .fold(0., f64::max);
    assert!(moved > 0.005, "{moved}");
    let low = (0..rest.vertices.len())
        .min_by(|a, b| rest.vertices[*a].y.total_cmp(&rest.vertices[*b].y))
        .unwrap();
    assert!(turned.vertices[low].distance(rest.vertices[low]) < 1e-9);
    // A pose for this avatar may name it; a pose on its own names built-in bones only.
    pose(&[("ponytail", [10., 0., 0.])])
        .validate_angles()
        .unwrap();
    assert!(pose(&[("ponytail", [10., 0., 0.])]).validate().is_err());
    assert!(
        pose(&[("no bone!", [0., 0., 0.])])
            .validate_angles()
            .is_err()
    );

    // Bad bones are refused: a taken name, a missing parent, ends too close.
    for bad in [
        AddedBone {
            name: "head".into(),
            ..avatar.added_bones[0].clone()
        },
        AddedBone {
            name: "tail2".into(),
            parent: "tail".into(),
            ..avatar.added_bones[0].clone()
        },
        AddedBone {
            name: "tail3".into(),
            tail: avatar.added_bones[0].head,
            ..avatar.added_bones[0].clone()
        },
    ] {
        let mut wrong = avatar.clone();
        wrong.added_bones.push(bad);
        assert!(wrong.rig(&rest).is_err());
    }
}

#[test]
fn painted_weights_make_skin_follow_another_bone() {
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let mut avatar = AvatarPose::from_sidecar(&mannequin()).unwrap();
    let rig = avatar.rig(&rest).unwrap();
    let forearm = rig.bone("forearm.L").unwrap();
    // A spot on the chest, far from the left forearm.
    let chest = rest
        .vertices
        .iter()
        .filter(|p| (p.y - 1.3).abs() < 0.02 && p.x.abs() < 0.02)
        .max_by(|a, b| a.z.total_cmp(&b.z))
        .copied()
        .unwrap();
    let spot: Vec<usize> = (0..rest.vertices.len())
        .filter(|v| rest.vertices[*v].distance(chest) < 0.02)
        .collect();
    let bend = pose(&[("forearm.L", [0., 0., 60.])]);
    let before = rig.posed(&rest, &bend);
    assert!(
        spot.iter()
            .all(|v| before.vertices[*v].distance(rest.vertices[*v]) < 1e-9)
    );
    // Paint the forearm onto it at full strength.
    let changed = paint_weights(
        &rig,
        &rest,
        &mut avatar.painted_weights,
        "forearm.L",
        chest,
        0.05,
        1.,
    )
    .unwrap();
    assert!(changed >= spot.len());
    let painted = avatar.rig(&rest).unwrap();
    for v in &spot {
        let w = painted.weights[*v]
            .iter()
            .find(|(b, _)| *b == forearm)
            .map_or(0., |(_, w)| *w);
        // The brush's falloff: full at the centre, (1 - d/r)² away from it.
        let falloff = (1. - rest.vertices[*v].distance(chest) / 0.05).powi(2);
        assert!(w >= falloff - 1e-9, "{w} {falloff}");
        let sum: f64 = painted.weights[*v].iter().map(|(_, w)| w).sum();
        assert!((sum - 1.).abs() < 1e-12);
    }
    let after = painted.posed(&rest, &bend);
    assert!(
        spot.iter()
            .any(|v| after.vertices[*v].distance(rest.vertices[*v]) > 0.01)
    );
    // Painting it away again takes it back off.
    let again = avatar.rig(&rest).unwrap();
    for _ in 0..8 {
        paint_weights(
            &again,
            &rest,
            &mut avatar.painted_weights,
            "forearm.L",
            chest,
            0.05,
            -1.,
        )
        .unwrap();
    }
    let cleared = avatar.rig(&rest).unwrap();
    let w = cleared.weights[spot[0]]
        .iter()
        .find(|(b, _)| *b == forearm)
        .map_or(0., |(_, w)| *w);
    assert!(w < 0.05, "{w}");
    // Bones only (no skin): the painted weights do not get in the way.
    let empty = SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    };
    assert_eq!(avatar.rig(&empty).unwrap().bones.len(), rig.bones.len());
    // Kept with the document; older documents without them still load.
    let json = serde_json::to_string(&avatar).unwrap();
    let back: AvatarPose = serde_json::from_str(&json).unwrap();
    assert_eq!(back, avatar);
    let old = serde_json::json!({"pose": {}, "joints": avatar.joints});
    let old: AvatarPose = serde_json::from_value(old).unwrap();
    assert!(old.added_bones.is_empty() && old.painted_weights.is_empty());
    // Out-of-range strokes are refused.
    assert!(
        paint_weights(
            &rig,
            &rest,
            &mut avatar.painted_weights,
            "nope",
            chest,
            0.05,
            1.
        )
        .is_err()
    );
    assert!(
        paint_weights(
            &rig,
            &rest,
            &mut avatar.painted_weights,
            "forearm.L",
            chest,
            0.,
            1.
        )
        .is_err()
    );
}

#[test]
fn poses_and_recordings_may_turn_only_bones_the_skeleton_has() {
    use marvelous_cloth::{
        avatar_animation::{AvatarAnimation, PoseKey, Recorder},
        fitting::FittingProject,
        garment::presets::GarmentKind,
    };
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let mut avatar = AvatarPose::from_sidecar(&mannequin()).unwrap();
    avatar.pose = pose(&[("tail", [10., 0., 0.])]);
    assert!(avatar.rig(&rest).is_err());
    let head = avatar.joints["joint-head"];
    avatar.added_bones.push(AddedBone {
        name: "tail".into(),
        parent: "pelvis".into(),
        head,
        tail: [head[0], head[1] - 0.1, head[2] - 0.05],
    });
    avatar.rig(&rest).unwrap();
    // Recording keys of the added bone needs the avatar to have it.
    let animation = AvatarAnimation {
        keys: vec![
            PoseKey {
                time_s: 0.,
                pose: Pose::default(),
            },
            PoseKey {
                time_s: 0.5,
                pose: pose(&[("tail", [20., 0., 0.])]),
            },
        ],
        ..Default::default()
    };
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    assert!(Recorder::new(&project, &animation).is_err());
    avatar.pose = Pose::default();
    project.avatar_pose = Some(avatar);
    Recorder::new(&project, &animation).unwrap();
}
