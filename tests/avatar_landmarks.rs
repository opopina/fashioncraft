//! Joints estimated from an avatar's shape land near the real ones on the bundled mannequin,
//! scaled, moved or in a T pose; anything that is not a standing body is refused; and an
//! avatar without anchors becomes arrangeable and poseable.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_landmarks::{JOINTS, estimate},
    avatar_rig::{Rig, preset},
    fitting::AvatarPose,
    garment::arrangement::load_sidecar,
};
use glam::DVec3;
use std::{collections::BTreeMap, path::PathBuf};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

fn real() -> BTreeMap<String, DVec3> {
    load_sidecar(&mannequin())
        .unwrap()
        .unwrap()
        .landmarks
        .into_iter()
        .map(|l| (l.id, l.position_m))
        .collect()
}

/// Largest error of the estimate, fingertips (the hand bones' far ends) apart.
fn worst(estimate: &BTreeMap<String, DVec3>, real: &BTreeMap<String, DVec3>) -> (f64, f64) {
    let mut joints = 0f64;
    let mut tips = 0f64;
    for id in JOINTS {
        let e = estimate[id].distance(real[id]);
        if id.contains("finger") {
            tips = tips.max(e);
        } else {
            joints = joints.max(e);
        }
    }
    (joints, tips)
}

#[test]
fn the_mannequins_joints_are_found_from_its_shape_alone() {
    let surface = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let real = real();
    let (joints, tips) = worst(&estimate(&surface).unwrap(), &real);
    assert!(joints < 0.02 && tips < 0.05, "{joints} {tips}");

    // Taller and elsewhere: the joints follow.
    let moved = SurfaceMesh {
        vertices: surface
            .vertices
            .iter()
            .map(|p| *p * 1.1 + DVec3::new(0.3, 0., -0.2))
            .collect(),
        triangles: surface.triangles.clone(),
    };
    let scaled: BTreeMap<String, DVec3> = real
        .iter()
        .map(|(k, p)| (k.clone(), *p * 1.1 + DVec3::new(0.3, 0., -0.2)))
        .collect();
    let (joints, tips) = worst(&estimate(&moved).unwrap(), &scaled);
    assert!(joints < 0.025 && tips < 0.055, "{joints} {tips}");
}

#[test]
fn arms_raised_to_a_t_pose_are_followed() {
    let surface = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let real = real();
    let rig = Rig::new(&real, &surface).unwrap();
    let pose = preset("T-pose").unwrap();
    let posed = rig.posed(&surface, &pose);
    let bones = rig.posed_bones(&pose);
    let head_of = |name: &str| bones[rig.bone(name).unwrap()].0;
    let estimate = estimate(&posed).unwrap();
    for (joint, bone) in [
        ("joint-l-elbow", "forearm.L"),
        ("joint-r-elbow", "forearm.R"),
        ("joint-l-hand", "hand.L"),
        ("joint-r-hand", "hand.R"),
    ] {
        let e = estimate[joint].distance(head_of(bone));
        assert!(e < 0.04, "{joint} is {e} m off");
    }
}

#[test]
fn what_is_not_a_standing_body_is_refused() {
    let cube = |size: f64| SurfaceMesh {
        vertices: (0..8)
            .map(|k| {
                DVec3::new((k & 1) as f64, ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64) * size
            })
            .collect(),
        triangles: vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ],
    };
    assert!(estimate(&cube(1.7)).is_err());
    assert!(estimate(&cube(10.)).is_err());
}

#[test]
fn an_avatar_without_anchors_can_be_arranged_and_posed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("body.obj");
    std::fs::copy(mannequin(), &path).unwrap();
    let sidecar = load_sidecar(&path).unwrap().expect("estimated anchors");
    assert!(sidecar.estimated);
    let front = sidecar
        .set
        .anchors
        .iter()
        .find(|a| a.id.contains("front"))
        .unwrap()
        .id
        .clone();
    let frame = sidecar.frame(&front).unwrap();
    assert!(frame.outward.z > 0.5, "the front anchor faces forward");
    // Its skeleton builds from the estimated joints.
    let avatar = AvatarPose::from_sidecar(&path).unwrap();
    let surface = SurfaceMesh::load_obj(&path).unwrap();
    avatar.rig(&surface).unwrap();
    // A shipped sidecar is still preferred.
    assert!(!load_sidecar(&mannequin()).unwrap().unwrap().estimated);
}

#[test]
fn anchors_follow_the_pose() {
    let path = mannequin();
    let surface = SurfaceMesh::load_obj(&path).unwrap();
    let sidecar = load_sidecar(&path).unwrap().unwrap();
    let mut avatar = AvatarPose::from_sidecar(&path).unwrap();
    avatar.pose = preset("T-pose").unwrap();
    let posed = avatar.posed_sidecar(&path).unwrap().unwrap();
    let rig = avatar.rig(&surface).unwrap();
    let bones = rig.posed_bones(&avatar.pose);
    let rest_bones = rig.posed_bones(&Default::default());
    let distance = |p: DVec3, (a, b): (DVec3, DVec3)| {
        let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0., 1.);
        p.distance(a + (b - a) * t)
    };
    for (anchor, bone) in [
        ("upper-arm-left-outer", "upper_arm.L"),
        ("forearm-right-front", "forearm.R"),
    ] {
        let k = rig.bone(bone).unwrap();
        let before = sidecar.frame(anchor).unwrap().origin;
        let after = posed.frame(anchor).unwrap().origin;
        assert!(after.distance(before) > 0.05, "{anchor} moved with the arm");
        // As far from its bone as it was at rest.
        let (d0, d1) = (distance(before, rest_bones[k]), distance(after, bones[k]));
        assert!((d0 - d1).abs() < 0.01, "{anchor}: {d0} then {d1}");
    }
    // The torso does not move in a T pose.
    let front = "torso-middle-front";
    let moved = sidecar
        .frame(front)
        .unwrap()
        .origin
        .distance(posed.frame(front).unwrap().origin);
    assert!(moved < 1e-9, "{moved}");
    // No pose, no change.
    avatar.pose = Default::default();
    let rest = avatar.posed_sidecar(&path).unwrap().unwrap();
    assert_eq!(
        rest.frame(front).unwrap().origin,
        sidecar.frame(front).unwrap().origin
    );
}

#[test]
fn a_joint_moved_reshapes_the_skeleton_and_bad_moves_are_refused() {
    let avatar = AvatarPose::from_sidecar(&mannequin()).unwrap();
    let elbow = DVec3::from_array(avatar.joints["joint-l-elbow"]);
    let moved = avatar
        .with_joint("joint-l-elbow", elbow + DVec3::new(0.02, 0.03, 0.))
        .unwrap();
    let surface = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let rig = moved.rig(&surface).unwrap();
    let (_, upper_end) = rig.bone_span("upper_arm.L").unwrap();
    let upper = &rig.bones[upper_end];
    assert!((upper.tail - (elbow + DVec3::new(0.02, 0.03, 0.))).length() < 1e-12);
    // Only that joint moves.
    for (id, p) in &avatar.joints {
        if id != "joint-l-elbow" {
            assert_eq!(moved.joints[id], *p);
        }
    }
    // Onto the shoulder the upper arm would vanish; unknown joints and NaN are refused.
    let shoulder = DVec3::from_array(avatar.joints["joint-l-shoulder"]);
    assert!(avatar.with_joint("joint-l-elbow", shoulder).is_err());
    assert!(avatar.with_joint("joint-tail", elbow).is_err());
    assert!(
        avatar
            .with_joint("joint-l-elbow", DVec3::new(f64::NAN, 0., 0.))
            .is_err()
    );
}
