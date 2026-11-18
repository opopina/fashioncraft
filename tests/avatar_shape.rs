//! Body measurements: the mannequin measures like a body, and reshaping it lands on the
//! asked height and girths while keeping its mesh, moving its joints and staying riggable.
use glam::DVec3;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    avatar_rig::Rig,
    avatar_shape::{BodyMeasurements, measure, reshape},
    garment::arrangement::load_sidecar,
};
use std::{collections::BTreeMap, path::PathBuf};

fn mannequin() -> (SurfaceMesh, BTreeMap<String, DVec3>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let joints = load_sidecar(&path)
        .unwrap()
        .unwrap()
        .landmarks
        .into_iter()
        .map(|l| (l.id, l.position_m))
        .collect();
    (SurfaceMesh::load_obj(&path).unwrap(), joints)
}

#[test]
fn the_mannequin_measures_like_a_body() {
    let (body, joints) = mannequin();
    let m = measure(&body, &joints).unwrap();
    assert!((m.height_m - 1.75).abs() < 1e-9);
    assert!((0.8..1.0).contains(&m.bust_m), "{m:?}");
    assert!((0.6..0.85).contains(&m.waist_m), "{m:?}");
    assert!((0.9..1.15).contains(&m.hip_m), "{m:?}");
    assert!(m.waist_m < m.bust_m && m.waist_m < m.hip_m);
    assert!((0.45..0.7).contains(&m.arm_m), "{m:?}");
    assert!((0.7..0.9).contains(&m.inseam_m), "{m:?}");
}

#[test]
fn reshaping_lands_on_the_asked_measurements() {
    let (body, joints) = mannequin();
    let now = measure(&body, &joints).unwrap();
    for target in [
        BodyMeasurements {
            height_m: 1.82,
            bust_m: now.bust_m + 0.10,
            waist_m: now.waist_m - 0.05,
            hip_m: now.hip_m + 0.08,
            arm_m: now.arm_m + 0.04,
            inseam_m: now.inseam_m + 0.05,
        },
        BodyMeasurements {
            height_m: 1.60,
            bust_m: now.bust_m - 0.06,
            waist_m: now.waist_m + 0.06,
            hip_m: now.hip_m - 0.05,
            arm_m: now.arm_m - 0.03,
            inseam_m: now.inseam_m - 0.04,
        },
    ] {
        let (shaped, moved) = reshape(&body, &joints, &target).unwrap();
        assert_eq!(shaped.triangles, body.triangles);
        let got = measure(&shaped, &moved).unwrap();
        for (a, b) in [
            (got.height_m, target.height_m),
            (got.bust_m, target.bust_m),
            (got.waist_m, target.waist_m),
            (got.hip_m, target.hip_m),
            (got.arm_m, target.arm_m),
            (got.inseam_m, target.inseam_m),
        ] {
            assert!((a - b).abs() < 0.01, "{got:?} for {target:?}");
        }
        // Still a body with a skeleton.
        Rig::new(&moved, &shaped).unwrap();
        assert!(moved["joint-head-2"].y > moved["joint-neck"].y);
    }
    // The same measurements barely move it.
    let (same, _) = reshape(&body, &joints, &now).unwrap();
    let most = same
        .vertices
        .iter()
        .zip(&body.vertices)
        .map(|(a, b)| a.distance(*b))
        .fold(0., f64::max);
    assert!(most < 0.003, "{most}");
    let bad = BodyMeasurements {
        height_m: 4.,
        ..now
    };
    assert!(reshape(&body, &joints, &bad).is_err());
}
