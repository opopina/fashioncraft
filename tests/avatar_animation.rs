//! Avatar animation: keyed poses run on smooth curves that start and stop at rest, ready-made
//! motions return to where they began, and recording moves the body in short steps that push
//! the cloth along, keeping one frame per tick of the frame rate.
use glam::{DVec2, DVec3};
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::SurfaceMesh,
    avatar_animation::{AvatarAnimation, MOTIONS, Marker, PoseKey, Recorder, Take, motion, run},
    avatar_rig::Pose,
    fitting::{AvatarPose, FittingProject},
    garment::{
        meshing::MeshingSettings,
        presets::{self, GarmentKind},
        shapes::Primitive,
    },
    vbd::VbdSettings,
};
use std::{path::PathBuf, sync::Arc};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

fn pose(entries: &[(&str, [f64; 3])]) -> Pose {
    Pose(entries.iter().map(|(b, a)| (b.to_string(), *a)).collect())
}

fn keyed(keys: &[(f64, f64)]) -> AvatarAnimation {
    AvatarAnimation {
        keys: keys
            .iter()
            .map(|&(time_s, angle)| PoseKey {
                time_s,
                pose: pose(&[("spine", [0., angle, 0.])]),
            })
            .collect(),
        ..Default::default()
    }
}

fn spine(a: &AvatarAnimation, t: f64) -> f64 {
    a.pose_at(t).0.get("spine").map_or(0., |v| v[1])
}

#[test]
fn keyed_angles_pass_through_their_keys_on_curves_that_never_swing_past_them() {
    let a = keyed(&[(0., 0.), (1., 20.), (2., 30.), (3., -10.)]);
    a.validate().unwrap();
    for (t, v) in [(0., 0.), (1., 20.), (2., 30.), (3., -10.)] {
        assert!((spine(&a, t) - v).abs() < 1e-9, "at {t}");
    }
    // Held before the first key and after the last.
    assert_eq!(spine(&a, -1.), 0.);
    assert_eq!(spine(&a, 9.), -10.);
    // Flat at the ends and at the turning point (2 s): tiny steps barely change the angle.
    for t in [0., 2., 3.] {
        let before = spine(&a, t - 1e-4);
        let after = spine(&a, t + 1e-4);
        assert!((after - before).abs() < 1e-5, "slope at {t}");
    }
    // Rising through the middle key (1 s) without stopping.
    assert!(spine(&a, 1.01) - spine(&a, 0.99) > 0.05);
    // Never past the keys on either side.
    for k in 0..=300 {
        let t = k as f64 * 0.01;
        let v = spine(&a, t);
        assert!((-10. - 1e-9..=30. + 1e-9).contains(&v), "{v} at {t}");
    }
    // Bones a key leaves out are at rest in it.
    let mut b = a.clone();
    b.keys[1].pose.0.insert("neck".into(), [10., 0., 0.]);
    assert_eq!(b.pose_at(0.).0["neck"], [0., 0., 0.]);
    assert!((b.pose_at(1.).0["neck"][0] - 10.).abs() < 1e-9);
}

#[test]
fn keys_and_markers_are_kept_in_order_and_nonsense_is_refused() {
    let mut a = keyed(&[(0., 0.), (1., 10.)]);
    a.set_key(0.5, pose(&[("spine", [0., 5., 0.])]));
    a.set_key(1.01, pose(&[("spine", [0., 12., 0.])]));
    let times: Vec<f64> = a.keys.iter().map(|k| k.time_s).collect();
    assert_eq!(times, vec![0., 0.5, 1.]);
    assert_eq!(a.keys[2].pose.0["spine"][1], 12.);
    assert_eq!(a.key_near(0.49), Some(1));
    assert_eq!(a.key_near(0.7), None);
    a.add_marker(0.8, None);
    a.add_marker(0.2, Some("Arms up".into()));
    let names: Vec<&str> = a.markers.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["Arms up", "Marker 1"]);
    a.validate().unwrap();
    assert_eq!(a.duration(), 1.);
    assert_eq!(a.frame_count(), 31);

    let refused = |f: &dyn Fn(&mut AvatarAnimation)| {
        let mut b = a.clone();
        f(&mut b);
        assert!(b.validate().is_err());
    };
    refused(&|b| b.keys.clear());
    refused(&|b| b.keys.swap(0, 1));
    refused(&|b| b.keys[1].time_s = f64::NAN);
    refused(&|b| b.keys[0].time_s = -1.);
    refused(&|b| b.fps = 0.);
    refused(&|b| b.fps = 500.);
    refused(&|b| {
        b.keys[0].pose.0.insert("tail".into(), [0.; 3]);
    });
    refused(&|b| {
        b.markers.push(Marker {
            time_s: 1.,
            name: " ".into(),
        })
    });

    // Stored with the project, absent when there is none.
    let mut p = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let plain = serde_json::to_string(&p).unwrap();
    assert!(!plain.contains("avatar_animation"));
    p.avatar_animation = Some(a.clone());
    let back: FittingProject = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back.avatar_animation, Some(a));
}

#[test]
fn ready_made_motions_start_and_end_where_the_avatar_stands() {
    let start = pose(&[
        ("upper_arm.L", [0., 0., -30.]),
        ("upper_arm.R", [0., 0., 30.]),
    ]);
    for name in MOTIONS {
        let a = motion(name, &start).unwrap();
        a.validate().unwrap();
        assert!(a.duration() > 1., "{name}");
        assert_eq!(a.keys[0].pose, start, "{name}");
        let end = a.pose_at(a.duration());
        for bone in end.0.keys() {
            let s = start.0.get(bone).copied().unwrap_or([0.; 3]);
            assert_eq!(end.0[bone], s, "{name} {bone}");
        }
        // Somewhere in between the avatar really moves.
        let middle = a.pose_at(a.duration() * 0.3);
        assert_ne!(middle, a.pose_at(0.), "{name}");
    }
    assert!(motion("Moonwalk", &start).is_none());
}

#[test]
fn a_take_is_written_as_a_pc2_point_cache() {
    let take = Take {
        fps: 30.,
        frames: vec![
            vec![[0., 1., 2.], [3., 4., 5.]],
            vec![[6., 7., 8.], [9., 10., 11.]],
        ],
    };
    assert!((take.duration() - 1. / 30.).abs() < 1e-12);
    assert_eq!(take.frame_index(0.02), 1);
    assert_eq!(take.frame_index(5.), 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cloth.pc2");
    take.write_pc2(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 32 + 2 * 2 * 12);
    assert_eq!(&bytes[..12], b"POINTCACHE2\0");
    let int = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let float = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!((int(12), int(16), int(28)), (1, 2, 2));
    assert_eq!((float(20), float(24)), (0., 1.));
    assert_eq!(float(32 + 12 * 3 + 8), 11.);
    assert!(Take::default().write_pc2(&path).is_err());
}

#[test]
fn recording_raises_the_arm_into_a_cloth_and_carries_it_up() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let rig = AvatarPose::from_sidecar(&mannequin())
        .unwrap()
        .rig(&rest)
        .unwrap();
    // A point on the right forearm towards the wrist, and the highest point of the body
    // under a 10 cm square centred over it.
    let forearm = &rig.bones[rig.bone("forearm.R").unwrap()];
    let near = forearm.head + (forearm.tail - forearm.head) * 0.65;
    let top = rest
        .vertices
        .iter()
        .filter(|p| (p.x - near.x).abs() < 0.06 && (p.z - near.z).abs() < 0.06)
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        top.is_finite() && top < near.y + 0.1,
        "top {top} near {near}"
    );

    // The square of cloth floating flat 1 cm above it, with no gravity.
    let t = authoring::apply(
        &presets::basic(GarmentKind::Tshirt, 0),
        Command::Primitive {
            kind: Primitive::Rectangle,
            size: DVec2::new(0.1, 0.1),
            sweep_degrees: 180.,
            origin: DVec2::ZERO,
            meshing: MeshingSettings {
                target_edge_m: 0.02,
                ..Default::default()
            },
            fabric: "woven-reference".into(),
            replace: true,
        },
    )
    .unwrap()
    .template;
    let g = Arc::new(t.compile().unwrap());
    let mut state = g.initial_state.clone();
    let [a, b, c] = g.mesh.triangles[0].vertices.map(|i| state.x[i]);
    let u = (b - a).normalize();
    let v = (b - a).cross(c - a).cross(u).normalize();
    let centre = state.x.iter().sum::<DVec3>() / state.x.len() as f64;
    let target = DVec3::new(near.x, top + 0.01, near.z);
    for p in &mut state.x {
        let d = *p - centre;
        *p = target + DVec3::X * d.dot(u) + DVec3::Z * d.dot(v);
    }
    let placed = state.clone();

    // Raise the right arm 25° in half a second, recorded at 20 frames per second.
    let animation = AvatarAnimation {
        keys: vec![
            PoseKey {
                time_s: 0.,
                pose: Pose::default(),
            },
            PoseKey {
                time_s: 0.5,
                pose: pose(&[("upper_arm.R", [0., 0., -25.])]),
            },
        ],
        fps: 20.,
        ..Default::default()
    };
    let recorder = Recorder::new(&project, &animation).unwrap();
    assert!(!recorder.moving_into_first_pose());
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 16,
        gravity: DVec3::ZERO,
        ..Default::default()
    };
    let start = state.x.clone();
    let mut steps = 0;
    let (recorder, state) = run(g.clone(), &rest, state, recorder, &settings, |r, _| {
        steps += 1;
        assert!(steps < 5000, "the recording does not end at {}", r.time());
        true
    })
    .unwrap();
    assert!((recorder.time() - 0.5).abs() < 1e-9);
    // More steps than frames: the arm moves faster than 1.5 mm per frame-long step.
    assert!(steps > 20, "{steps} steps");
    let take = recorder.into_take();
    assert_eq!(take.frames.len(), 11);
    assert_eq!(take.positions(0)[0].as_vec3(), start[0].as_vec3());
    let last = take.positions(10);
    for (p, q) in last.iter().zip(&state.x) {
        assert!(p.distance(*q) < 1e-5);
    }
    let rise = (last.iter().map(|p| p.y).sum::<f64>() - start.iter().map(|p| p.y).sum::<f64>())
        / start.len() as f64;
    assert!(rise > 0.02, "the cloth rose {rise} m");

    // Stopped early, the take keeps the frames recorded so far.
    let recorder = Recorder::new(&project, &animation).unwrap();
    let (recorder, _) = run(g, &rest, placed, recorder, &settings, |r, _| r.time() < 0.2).unwrap();
    assert!(!recorder.finished());
    let frames = recorder.take().frames.len();
    assert!((4..11).contains(&frames), "{frames} frames");
}

#[test]
fn takes_and_the_moving_avatar_are_written_as_obj_sequences() {
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let scene = project.compile().unwrap();
    let x = &scene.state.x;
    let frame = |dy: f32| -> Vec<[f32; 3]> {
        x.iter()
            .map(|p| [p.x as f32, p.y as f32 + dy, p.z as f32])
            .collect()
    };
    let take = Take {
        fps: 10.,
        frames: vec![frame(0.), frame(0.1)],
    };
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        take.write_obj_sequence(&scene.garment, dir.path(), "cloth")
            .unwrap(),
        2
    );
    let second = std::fs::read_to_string(dir.path().join("cloth_0001.obj")).unwrap();
    let vertices: Vec<f64> = second
        .lines()
        .filter_map(|l| l.strip_prefix("v "))
        .map(|l| l.split_whitespace().nth(1).unwrap().parse().unwrap())
        .collect();
    assert_eq!(vertices.len(), x.len());
    assert!((vertices[0] - (x[0].y + 0.1)).abs() < 1e-6);
    assert!(second.lines().any(|l| l.starts_with("vt ")));
    // A take from another mesh is refused.
    let mut other = take.clone();
    other.frames[0].pop();
    other.frames[1].pop();
    assert!(
        other
            .write_obj_sequence(&scene.garment, dir.path(), "x")
            .is_err()
    );

    // The avatar along a motion, one file per frame, moving between them.
    let animation = motion("Raise arms", &Pose::default()).unwrap();
    let n = marvelous_cloth::avatar_animation::write_body_sequence(
        &project,
        &animation,
        3,
        dir.path(),
        "avatar",
    )
    .unwrap();
    assert_eq!(n, 3);
    let body =
        |k: usize| std::fs::read_to_string(dir.path().join(format!("avatar_{k:04}.obj"))).unwrap();
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let first = SurfaceMesh::parse_obj(&body(0)).unwrap();
    assert_eq!(first.vertices.len(), rest.vertices.len());
    assert_eq!(first.triangles, rest.triangles);
    assert_ne!(body(0), body(2));
}
