//! Anny compact 104-bone rig: vertex weights, joint placement on the mannequin and extreme
//! generated bodies, compatibility of all legacy bone names (including swing-twist splitting and
//! two-bone IK), shoulder/axilla volume preservation against the legacy automatic rig, and
//! non-MakeHuman fallback.
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_maker::{BODY_VERTICES, MacroBody, MacroTargets},
    avatar_rig::{
        AddedBone, INFLUENCES, LIMBS, PaintedWeights, Pose, Rig, euler_degrees, joint_ids,
        legacy_bone_names, paint_weights,
    },
    fitting::AvatarPose,
    garment::arrangement::load_sidecar,
};
use glam::{DQuat, DVec3};
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
fn anny_rig_has_104_bones_and_every_body_vertex_weights_sum_to_one() {
    let (surface, landmarks) = mannequin();
    let rig = Rig::new(&landmarks, &surface).unwrap();
    assert!(rig.is_anny());
    assert_eq!(rig.bones.len(), 104);
    assert_eq!(rig.weights.len(), BODY_VERTICES);

    for (i, bone) in rig.bones.iter().enumerate() {
        if let Some(p) = bone.parent {
            assert!(p < i, "bone {} precedes its parent", bone.name);
        }
        assert!(bone.head.is_finite() && bone.tail.is_finite());
        assert!(
            bone.head.distance(bone.tail) > 0.005,
            "bone {} is shorter than 5 mm: {:.4} m",
            bone.name,
            bone.head.distance(bone.tail)
        );
    }

    let mut max_inf = 0usize;
    for (v, weights) in rig.weights.iter().enumerate() {
        assert!(!weights.is_empty(), "vertex {v} has no skin weights");
        assert!(
            weights.len() <= INFLUENCES,
            "vertex {v} has {} > {INFLUENCES} influences",
            weights.len()
        );
        max_inf = max_inf.max(weights.len());
        let mut sum = 0.0;
        for &(b, w) in weights {
            assert!(b < rig.bones.len(), "vertex {v} references bone {b}");
            assert!(
                w.is_finite() && w > 0.0,
                "vertex {v} has invalid weight {w}"
            );
            sum += w;
        }
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "vertex {v} weights sum to {sum}, expected 1"
        );
    }
    // Anny's hand-painted weights genuinely use more than 4 influences around joints.
    assert!(max_inf > 4 && max_inf <= INFLUENCES);
}

#[test]
fn bones_land_near_joint_landmarks_on_mannequin_and_extreme_bodies() {
    let targets = MacroTargets::bundled().unwrap();
    let (mannequin_surface, mannequin_joints) = mannequin();

    let child = targets.make(&MacroBody::typical(0.5, 4.0)).unwrap();
    let tall = targets
        .make(&MacroBody {
            height: 1.0,
            ..MacroBody::typical(1.0, 25.0)
        })
        .unwrap();
    let heavy = targets
        .make(&MacroBody {
            weight: 1.0,
            muscle: 0.3,
            ..MacroBody::typical(0.0, 35.0)
        })
        .unwrap();

    // Direct joint-helper bone endpoints that must land exactly on the body's own landmarks.
    let direct_heads = [
        ("pelvis.L", "joint-pelvis"),
        ("pelvis.R", "joint-pelvis"),
        ("spine05", "joint-pelvis"),
        ("spine04", "joint-spine-4"),
        ("spine03", "joint-spine-3"),
        ("spine02", "joint-spine-2"),
        ("spine01", "joint-spine-1"),
        ("neck01", "joint-neck"),
        ("head", "joint-head"),
        ("clavicle.L", "joint-l-clavicle"),
        ("shoulder01.L", "joint-l-scapula"),
        ("upperarm01.L", "joint-l-shoulder"),
        ("lowerarm01.L", "joint-l-elbow"),
        ("wrist.L", "joint-l-hand"),
        ("clavicle.R", "joint-r-clavicle"),
        ("shoulder01.R", "joint-r-scapula"),
        ("upperarm01.R", "joint-r-shoulder"),
        ("lowerarm01.R", "joint-r-elbow"),
        ("wrist.R", "joint-r-hand"),
        ("upperleg01.L", "joint-l-upper-leg"),
        ("lowerleg01.L", "joint-l-knee"),
        ("foot.L", "joint-l-ankle"),
        ("upperleg01.R", "joint-r-upper-leg"),
        ("lowerleg01.R", "joint-r-knee"),
        ("foot.R", "joint-r-ankle"),
        ("finger3-1.L", "joint-l-finger-3-1"),
        ("finger3-3.L", "joint-l-finger-3-3"),
        ("toe1-1.L", "joint-l-toe-1-1"),
        ("toe1-1.R", "joint-r-toe-1-1"),
    ];
    let direct_tails = [
        ("root", "joint-pelvis"),
        ("head", "joint-head-2"),
        ("shoulder01.L", "joint-l-shoulder"),
        ("upperarm02.L", "joint-l-elbow"),
        ("lowerarm02.L", "joint-l-hand"),
        ("upperarm02.R", "joint-r-elbow"),
        ("lowerarm02.R", "joint-r-hand"),
        ("upperleg02.L", "joint-l-knee"),
        ("lowerleg02.L", "joint-l-ankle"),
        ("upperleg02.R", "joint-r-knee"),
        ("lowerleg02.R", "joint-r-ankle"),
        ("finger3-3.L", "joint-l-finger-3-4"),
        ("finger3-3.R", "joint-r-finger-3-4"),
    ];

    let empty = SurfaceMesh {
        vertices: vec![],
        triangles: vec![],
    };

    for (label, surface, joints) in [
        ("mannequin", &mannequin_surface, &mannequin_joints),
        ("child (4 y)", &child.surface, &child.joints),
        ("very tall", &tall.surface, &tall.joints),
        ("very heavy", &heavy.surface, &heavy.joints),
    ] {
        let rig = Rig::new(joints, surface).unwrap();
        assert!(rig.is_anny(), "{label}");
        assert_eq!(rig.bones.len(), 104, "{label}");

        // Every landmark-defined bone endpoint lands directly on its joint helper.
        for (bone_name, landmark_id) in direct_heads {
            let b = &rig.bones[rig.bone(bone_name).unwrap()];
            let err = b.head.distance(joints[landmark_id]);
            assert!(
                err < 1e-9,
                "{label}: {bone_name}.head is {:.4} mm from {landmark_id}",
                err * 1e3
            );
        }
        for (bone_name, landmark_id) in direct_tails {
            let b = &rig.bones[rig.bone(bone_name).unwrap()];
            let err = b.tail.distance(joints[landmark_id]);
            assert!(
                err < 1e-9,
                "{label}: {bone_name}.tail is {:.4} mm from {landmark_id}",
                err * 1e3
            );
        }

        // Split arm and forearm midpoints remain contiguous and close to the limb's axis
        // (within a few millimetres of the segment between shoulder/elbow/wrist).
        for (first, second, start_lm, end_lm, max_perp_m) in [
            (
                "upperarm01.L",
                "upperarm02.L",
                "joint-l-shoulder",
                "joint-l-elbow",
                0.015,
            ),
            (
                "upperarm01.R",
                "upperarm02.R",
                "joint-r-shoulder",
                "joint-r-elbow",
                0.015,
            ),
            (
                "lowerarm01.L",
                "lowerarm02.L",
                "joint-l-elbow",
                "joint-l-hand",
                0.015,
            ),
            (
                "lowerarm01.R",
                "lowerarm02.R",
                "joint-r-elbow",
                "joint-r-hand",
                0.015,
            ),
        ] {
            let b0 = &rig.bones[rig.bone(first).unwrap()];
            let b1 = &rig.bones[rig.bone(second).unwrap()];
            assert!(
                b0.tail.distance(b1.head) < 1e-12,
                "{label}: {first}/{second}"
            );
            let a = joints[start_lm];
            let b = joints[end_lm];
            let ab = b - a;
            let t = (b0.tail - a).dot(ab) / ab.length_squared();
            assert!((0.15..=0.75).contains(&t), "{label}: {first} t={t}");
            let perp = b0.tail.distance(a + ab * t);
            assert!(
                perp < max_perp_m,
                "{label}: {first} midpoint is {:.2} mm off the {start_lm}->{end_lm} axis",
                perp * 1e3
            );
        }

        // Bones-only mode (no surface mesh) still builds all 104 bones and agrees closely on
        // the arm joints.
        let bones_only = Rig::new(joints, &empty).unwrap();
        assert_eq!(bones_only.bones.len(), 104, "{label}");
        for bone_name in [
            "upperarm01.L",
            "lowerarm01.L",
            "upperarm01.R",
            "lowerarm01.R",
        ] {
            let i = rig.bone(bone_name).unwrap();
            let err = rig.bones[i].tail.distance(bones_only.bones[i].tail);
            assert!(
                err < 0.015,
                "{label}: bones-only {bone_name}.tail differs by {:.2} mm",
                err * 1e3
            );
        }
    }
}

#[test]
fn legacy_bone_names_move_the_same_parts_and_split_axial_twist() {
    let (surface, landmarks) = mannequin();
    let anny = Rig::new(&landmarks, &surface).unwrap();
    let legacy = Rig::build_automatic(&landmarks, &surface, &[], &PaintedWeights::new()).unwrap();
    assert!(anny.is_anny() && !legacy.is_anny());

    // Every legacy bone name moves its target landmark in the same direction as the legacy rig.
    let checks: &[(&str, [f64; 3], &str)] = &[
        ("pelvis", [0., 30., 0.], "joint-l-hand"),
        ("spine", [0., 25., 0.], "joint-l-shoulder"),
        ("chest", [20., 0., 0.], "joint-head-2"),
        ("neck", [25., 0., 0.], "joint-head-2"),
        ("head", [0., 35., 0.], "joint-head-2"),
        ("clavicle.L", [0., 0., 20.], "joint-l-shoulder"),
        ("upper_arm.L", [0., 0., 50.], "joint-l-elbow"),
        ("forearm.L", [0., 0., 60.], "joint-l-hand"),
        ("hand.L", [0., 0., 45.], "joint-l-finger-3-4"),
        ("clavicle.R", [0., 0., -20.], "joint-r-shoulder"),
        ("upper_arm.R", [0., 0., -50.], "joint-r-elbow"),
        ("forearm.R", [0., 0., -60.], "joint-r-hand"),
        ("hand.R", [0., 0., -45.], "joint-r-finger-3-4"),
        ("thigh.L", [-30., 0., 0.], "joint-l-knee"),
        ("shin.L", [40., 0., 0.], "joint-l-ankle"),
        ("foot.L", [25., 0., 0.], "joint-l-foot-2"),
        ("thigh.R", [-30., 0., 0.], "joint-r-knee"),
        ("shin.R", [40., 0., 0.], "joint-r-ankle"),
        ("foot.R", [25., 0., 0.], "joint-r-foot-2"),
    ];
    assert_eq!(checks.len(), legacy_bone_names().count());

    let nearest_vertex = |lm: &str| {
        let p = landmarks[lm];
        (0..surface.vertices.len())
            .min_by(|&a, &b| {
                surface.vertices[a]
                    .distance(p)
                    .total_cmp(&surface.vertices[b].distance(p))
            })
            .unwrap()
    };

    for &(bone_name, angles, probe_lm) in checks {
        let pose = Pose([(bone_name.to_string(), angles)].into());
        pose.validate().unwrap();
        let v = nearest_vertex(probe_lm);
        let d_leg = legacy.posed(&surface, &pose).vertices[v] - surface.vertices[v];
        let d_ann = anny.posed(&surface, &pose).vertices[v] - surface.vertices[v];
        assert!(
            d_ann.length() > 0.005,
            "{bone_name} barely moved {probe_lm}: {:.4} m",
            d_ann.length()
        );
        let align = d_ann.normalize().dot(d_leg.normalize());
        assert!(
            align > 0.9,
            "{bone_name} moved {probe_lm} with alignment {align:.3} (legacy {d_leg:?} vs anny {d_ann:?})"
        );
    }

    // Axial twist on every split long bone is divided evenly (50/50) between segment 01 and 02,
    // while the distal bone still receives the exact full twist rotation.
    for (legacy_name, seg0, seg1) in [
        ("upper_arm.L", "upperarm01.L", "upperarm02.L"),
        ("forearm.L", "lowerarm01.L", "lowerarm02.L"),
        ("thigh.L", "upperleg01.L", "upperleg02.L"),
        ("shin.L", "lowerleg01.L", "lowerleg02.L"),
    ] {
        let (i0, i1) = anny.bone_span(legacy_name).unwrap();
        assert_eq!(anny.bones[i0].name, seg0);
        assert_eq!(anny.bones[i1].name, seg1);
        let axis = (anny.bones[i1].tail - anny.bones[i0].head).normalize();
        let q_twist = DQuat::from_axis_angle(axis, 90f64.to_radians());
        let pose = Pose([(legacy_name.to_string(), euler_degrees(q_twist))].into());
        let t = anny.transforms(|b| pose.rotation(b));
        let r0 = DQuat::from_mat3(&t[i0].matrix3).normalize();
        let r1 = DQuat::from_mat3(&t[i1].matrix3).normalize();
        let half = DQuat::from_axis_angle(axis, 45f64.to_radians());
        assert!(
            r0.angle_between(half) < 1e-4,
            "{legacy_name}: {seg0} angle off 45° by {} rad",
            r0.angle_between(half)
        );
        assert!(
            r1.angle_between(q_twist) < 1e-4,
            "{legacy_name}: {seg1} angle off 90° by {} rad",
            r1.angle_between(q_twist)
        );
    }

    // Two-bone IK works from a pose that already uses legacy keys on split limbs.
    let seeded = Pose(
        [
            ("upper_arm.L".into(), [0., 0., 25.]),
            ("forearm.L".into(), [0., 0., 15.]),
        ]
        .into(),
    );
    for (_, upper, lower) in LIMBS {
        let (u_start, _) = anny.bone_span(upper).unwrap();
        let root = anny.bones[u_start].head;
        let start = anny.limb_end(&seeded, lower).unwrap();
        // Pull 15 % toward the root and lift/shift forward so the target is strictly in reach.
        let target = start.lerp(root, 0.15) + DVec3::new(0., 0.05, 0.08);
        let reached = anny.reach(&seeded, upper, lower, target).unwrap();
        reached.validate().unwrap();
        let end = anny.limb_end(&reached, lower).unwrap();
        assert!(
            end.distance(target) < 1e-3,
            "IK from legacy pose failed on {lower}: {end:?} vs {target:?}"
        );
    }
}

/// Cross-sectional area (m²) of `surface` intersected with the plane `(origin, normal)`,
/// keeping only segments whose midpoint lies within `radius` of `origin`.
fn local_slice_area(surface: &SurfaceMesh, origin: DVec3, normal: DVec3, radius: f64) -> f64 {
    let normal = normal.normalize();
    let mut segs: Vec<(DVec3, DVec3)> = Vec::new();
    for &[a, b, c] in &surface.triangles {
        let pts = [
            surface.vertices[a],
            surface.vertices[b],
            surface.vertices[c],
        ];
        let ds = pts.map(|p| (p - origin).dot(normal));
        let mut hits = Vec::with_capacity(2);
        for k in 0..3 {
            let k2 = (k + 1) % 3;
            if (ds[k] >= 0.) != (ds[k2] >= 0.) {
                let t = ds[k] / (ds[k] - ds[k2]);
                hits.push(pts[k].lerp(pts[k2], t));
            }
        }
        if hits.len() == 2 {
            let mid = (hits[0] + hits[1]) * 0.5;
            if mid.distance(origin) < radius {
                segs.push((hits[0], hits[1]));
            }
        }
    }
    if segs.is_empty() {
        return 0.;
    }
    let ref_v = if normal.z.abs() < 0.9 {
        DVec3::Z
    } else {
        DVec3::X
    };
    let u = normal.cross(ref_v).normalize();
    let v = normal.cross(u);
    let centroid: DVec3 =
        segs.iter().map(|(a, b)| *a + *b).sum::<DVec3>() / (2. * segs.len() as f64);
    segs.iter()
        .map(|(p0, p1)| {
            let (a0, a1) = (*p0 - centroid, *p1 - centroid);
            let (x0, y0) = (a0.dot(u), a0.dot(v));
            let (x1, y1) = (a1.dot(u), a1.dot(v));
            0.5 * (x0 * y1 - y0 * x1).abs()
        })
        .sum()
}

#[test]
fn raising_the_arm_preserves_more_shoulder_and_axilla_volume_than_legacy_rig() {
    let (surface, landmarks) = mannequin();
    let anny = Rig::new(&landmarks, &surface).unwrap();
    let legacy = Rig::build_automatic(&landmarks, &surface, &[], &PaintedWeights::new()).unwrap();

    let shoulder = landmarks["joint-l-shoulder"];
    let elbow = landmarks["joint-l-elbow"];
    let rest_dir = (elbow - shoulder).normalize();
    let rest_slice_origin = shoulder.lerp(elbow, 0.20);
    let rest_area = local_slice_area(&surface, rest_slice_origin, rest_dir, 0.12);
    assert!(rest_area > 0.005, "rest upper-arm area {rest_area}");

    // Triangles within 12 cm of the left shoulder (deltoid + axilla cap).
    let shoulder_tris: Vec<[usize; 3]> = surface
        .triangles
        .iter()
        .copied()
        .filter(|t| surface.vertices[t[0]].distance(shoulder) < 0.12)
        .collect();
    let cap_area = |mesh: &SurfaceMesh| -> f64 {
        shoulder_tris
            .iter()
            .map(|&[a, b, c]| {
                0.5 * (mesh.vertices[b] - mesh.vertices[a])
                    .cross(mesh.vertices[c] - mesh.vertices[a])
                    .length()
            })
            .sum()
    };
    let rest_cap = cap_area(&surface);

    for deg in [75.0_f64, 90.0_f64] {
        let raise = Pose([("upper_arm.L".into(), [0., 0., deg])].into());
        let posed_legacy = legacy.posed(&surface, &raise);
        let posed_anny = anny.posed(&surface, &raise);

        let rot = DQuat::from_rotation_z(deg.to_radians());
        let posed_elbow = shoulder + rot * (elbow - shoulder);
        let arm_dir = (posed_elbow - shoulder).normalize();
        let slice_origin = shoulder.lerp(posed_elbow, 0.20);

        let area_legacy = local_slice_area(&posed_legacy, slice_origin, arm_dir, 0.12);
        let area_anny = local_slice_area(&posed_anny, slice_origin, arm_dir, 0.12);
        assert!(
            area_anny > area_legacy * 1.15,
            "at +{deg}°: axilla-side arm cross-section anny {:.2} cm² vs legacy {:.2} cm² (rest {:.2} cm²)",
            area_anny * 1e4,
            area_legacy * 1e4,
            rest_area * 1e4
        );

        let cap_legacy = cap_area(&posed_legacy);
        let cap_anny = cap_area(&posed_anny);
        assert!(
            cap_anny > cap_legacy * 1.05
                && (cap_anny - rest_cap).abs() < (cap_legacy - rest_cap).abs(),
            "at +{deg}°: shoulder cap area anny {:.2} cm² vs legacy {:.2} cm² (rest {:.2} cm²)",
            cap_anny * 1e4,
            cap_legacy * 1e4,
            rest_cap * 1e4
        );
    }
}

#[test]
fn non_makehuman_avatars_keep_legacy_rig_and_saved_projects_still_open() {
    let (surface, landmarks) = mannequin();

    // 1. A non-MakeHuman mesh (different vertex count/topology) keeps the 19-bone automatic rig.
    let mut non_mh = surface.clone();
    non_mh.vertices.push(DVec3::new(0., 0., 0.));
    let rig_non_mh = Rig::new(&landmarks, &non_mh).unwrap();
    assert!(!rig_non_mh.is_anny());
    assert_eq!(rig_non_mh.bones.len(), 19);
    for w in &rig_non_mh.weights {
        let sum: f64 = w.iter().map(|p| p.1).sum();
        assert!((sum - 1.).abs() < 1e-9);
    }

    // 2. An older saved project carrying only the 24 legacy joints on the mannequin still opens
    //    cleanly on the 19-bone rig with legacy pose, painted weights and added bones.
    let legacy_ids = joint_ids();
    let legacy_joints: BTreeMap<String, [f64; 3]> = landmarks
        .iter()
        .filter(|(k, _)| legacy_ids.contains(k.as_str()))
        .map(|(k, v)| (k.clone(), v.to_array()))
        .collect();
    let mut old_pose = AvatarPose {
        pose: Pose([("upper_arm.L".into(), [0., 0., 40.])].into()),
        joints: legacy_joints,
        added_bones: vec![AddedBone {
            name: "belt_charm".into(),
            parent: "pelvis".into(),
            head: [0., 1.0, 0.12],
            tail: [0., 0.9, 0.14],
        }],
        painted_weights: PaintedWeights::new(),
    };
    let old_rig = old_pose.rig(&surface).unwrap();
    assert!(!old_rig.is_anny());
    paint_weights(
        &old_rig,
        &surface,
        &mut old_pose.painted_weights,
        "forearm.L",
        landmarks["joint-l-hand"],
        0.04,
        0.5,
    )
    .unwrap();
    old_pose.rig(&surface).unwrap();

    // 3. A new MakeHuman AvatarPose (full Anny joints) with legacy bone names in pose, added_bones
    //    and painted_weights opens on the 104-bone Anny rig without error.
    let mut new_pose = AvatarPose {
        pose: Pose(
            [
                ("upper_arm.L".into(), [0., 0., 40.]),
                ("forearm.L".into(), [0., 0., 30.]),
            ]
            .into(),
        ),
        joints: landmarks
            .iter()
            .map(|(k, v)| (k.clone(), v.to_array()))
            .collect(),
        added_bones: vec![AddedBone {
            name: "belt_charm".into(),
            parent: "pelvis".into(),
            head: [0., 1.0, 0.12],
            tail: [0., 0.9, 0.14],
        }],
        painted_weights: BTreeMap::from([(0, vec![("forearm.L".into(), 1.0)])]),
    };
    let new_rig = new_pose.rig(&surface).unwrap();
    assert!(new_rig.is_anny());
    assert_eq!(new_rig.bones.len(), 105);
    let posed = new_rig.posed(&surface, &new_pose.pose);
    assert!(posed.vertices[0].distance(surface.vertices[0]) > 0.05);
    new_pose.painted_weights.clear();
    assert_eq!(new_pose.rig(&surface).unwrap().bones.len(), 105);
}
