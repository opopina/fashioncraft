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

#[test]
fn a_take_is_read_back_from_its_point_cache_and_travels_with_the_project() {
    let take = Take {
        fps: 24.,
        frames: vec![
            vec![[0., 1., 2.], [3., 4., 5.]],
            vec![[6., 7., 8.], [9., 10., 11.]],
            vec![[1., 1., 1.], [2., 2., 2.]],
        ],
    };
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("take.pc2");
    take.write_pc2(&cache).unwrap();
    assert_eq!(Take::read_pc2(&cache, 24.).unwrap(), take);
    assert!(Take::read_pc2(&cache, 0.).is_err());
    // Truncated or foreign files are refused.
    let bytes = std::fs::read(&cache).unwrap();
    std::fs::write(dir.path().join("short.pc2"), &bytes[..bytes.len() - 4]).unwrap();
    assert!(Take::read_pc2(&dir.path().join("short.pc2"), 24.).is_err());
    std::fs::write(
        dir.path().join("other.pc2"),
        b"not a point cache at all, really",
    )
    .unwrap();
    assert!(Take::read_pc2(&dir.path().join("other.pc2"), 24.).is_err());

    // Saved, the cache is copied next to the project and found again on opening it.
    let mut p = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    p.animation_take = Some(marvelous_cloth::fitting::AnimationTake {
        path: cache.clone(),
        fps: 24.,
    });
    let saved = dir.path().join("walk.json");
    p.save(&saved).unwrap();
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(text.contains("walk.json.assets/take-"), "{text}");
    std::fs::remove_file(&cache).unwrap();
    let opened = FittingProject::load(&saved).unwrap();
    let stored = opened.animation_take.unwrap();
    assert!(stored.path.starts_with(dir.path()));
    assert_eq!(Take::read_pc2(&stored.path, stored.fps).unwrap(), take);
}

#[test]
fn wind_and_gravity_keys_drive_each_recorded_step() {
    use marvelous_cloth::avatar_animation::{VectorKey, set_vector_key};
    let mut animation = keyed(&[(0., 0.), (1., 0.)]);
    set_vector_key(&mut animation.gravity_keys, 1., DVec3::ZERO);
    set_vector_key(&mut animation.gravity_keys, 0., DVec3::new(0., -9.81, 0.));
    set_vector_key(&mut animation.wind_keys, 0.5, DVec3::new(0., 0., -4.));
    animation.validate().unwrap();
    // Held before the first key and after the last, straight in between.
    assert_eq!(animation.gravity_keys[0].time_s, 0.);
    assert!((animation.gravity_at(0.25).unwrap().y + 9.81 * 0.75).abs() < 1e-12);
    assert_eq!(animation.wind_at(0.).unwrap(), DVec3::new(0., 0., -4.));
    assert_eq!(animation.gravity_at(3.).unwrap(), DVec3::ZERO);
    assert!(keyed(&[(0., 0.)]).wind_at(0.).is_none());
    // Each planned step blows and pulls as its end time asks.
    let project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    let mut recorder = Recorder::new(&project, &animation).unwrap();
    let cloth = project.compile().unwrap().state.x.clone();
    recorder.begin(&cloth);
    let base = VbdSettings::default();
    let mut steps = 0;
    while !recorder.finished() && steps < 100_000 {
        recorder.plan(1. / 60.);
        let s = recorder.step_settings(&base);
        assert!(s.gravity.y >= -9.81 - 1e-9 && s.gravity.y <= 0.);
        assert_eq!(s.wind.as_ref().unwrap().velocity, DVec3::new(0., 0., -4.));
        recorder.commit(&cloth);
        steps += 1;
    }
    // Late steps pull less than early ones.
    recorder.plan(1. / 60.);
    assert!(recorder.step_settings(&base).gravity.y.abs() < 0.2);
    // Keys out of order or too strong are refused.
    let mut bad = animation.clone();
    bad.gravity_keys.push(VectorKey {
        time_s: 0.5,
        value: [0., -9.81, 0.],
    });
    assert!(bad.validate().is_err());
    let mut strong = animation;
    set_vector_key(&mut strong.wind_keys, 2., DVec3::new(0., 0., 80.));
    assert!(strong.validate().is_err());
}

#[test]
fn pins_attached_to_the_avatar_go_with_it_while_recording() {
    use std::collections::BTreeMap;
    let rest = SurfaceMesh::load_obj(&mannequin()).unwrap();
    let rig = AvatarPose::from_sidecar(&mannequin())
        .unwrap()
        .rig(&rest)
        .unwrap();
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 0, mannequin());
    let scene = project.compile().unwrap();
    // Pin the cloth's point nearest the left elbow, where the sleeve hangs by the arm.
    let elbow = rig.bones[rig.bone("forearm.L").unwrap()].head;
    let garment = &scene.garment;
    let vertex = (0..scene.state.x.len())
        .min_by(|a, b| {
            scene.state.x[*a]
                .distance(elbow)
                .total_cmp(&scene.state.x[*b].distance(elbow))
        })
        .unwrap();
    let panel = garment
        .panels
        .iter()
        .find(|p| p.vertices.contains(&vertex))
        .unwrap();
    let selections = BTreeMap::from([(panel.id.clone(), vec![garment.mesh.uv[vertex]])]);
    let (pinned, state) = authoring::prepare_pin_group_edit(
        &project.template,
        garment,
        &scene.state,
        &selections,
        true,
    )
    .unwrap();
    project.template = pinned.template;
    let start = state.x[vertex];
    // Lift the left arm 30° in half a second.
    let animation = AvatarAnimation {
        keys: vec![
            PoseKey {
                time_s: 0.,
                pose: Pose::default(),
            },
            PoseKey {
                time_s: 0.5,
                pose: pose(&[("upper_arm.L", [0., 0., 30.])]),
            },
        ],
        ..Default::default()
    };
    let moved = |project: &FittingProject| {
        let mut recorder = Recorder::new(project, &animation).unwrap();
        recorder.begin(&state.x);
        let mut last = start;
        let mut steps = 0;
        while !recorder.finished() && steps < 20_000 {
            recorder.plan(1. / 60.);
            for &(v, p) in recorder.pin_positions() {
                assert_eq!(v, vertex);
                assert!(p.distance(last) < 0.003, "pins move in small steps");
                last = p;
            }
            recorder.commit(&state.x);
            steps += 1;
        }
        (recorder.pin_positions().len(), last)
    };
    // Not attached: nothing holds the pin to the arm.
    assert_eq!(moved(&project).0, 0);
    // Attached: it rides up with the arm, staying as far off the skin as it was.
    project.template = authoring::apply(&project.template, Command::AttachPins { attached: true })
        .unwrap()
        .template;
    let (held, end) = moved(&project);
    assert_eq!(held, 1);
    assert!(end.y - start.y > 0.03, "{start} → {end}");
    // Attaching twice, or with no pins, is refused.
    assert!(authoring::apply(&project.template, Command::AttachPins { attached: true }).is_err());
    let mut bare = project.template.clone();
    bare.pin_targets.clear();
    bare.pins_attached = false;
    assert!(authoring::apply(&bare, Command::AttachPins { attached: true }).is_err());
}

#[test]
fn fabric_keys_reshape_and_press_pieces_as_the_take_goes_on() {
    use marvelous_cloth::{
        avatar_animation::{FabricKey, run_stepping, set_fabric_key},
        garment::FabricValues,
    };
    use std::collections::BTreeMap;
    let mut animation = keyed(&[(0., 0.), (0.2, 0.)]);
    let keys = animation.fabric_keys.entry("front".into()).or_default();
    set_fabric_key(keys, 0.2, 400., [72., 100.]);
    set_fabric_key(keys, 0., 0., [90., 100.]);
    animation.validate().unwrap();
    // Straight between keys, held outside them; pieces without keys are left out.
    let middle = animation.fabric_at(0.1);
    assert!((middle["front"].pressure - 200.).abs() < 1e-9);
    assert!((middle["front"].shrink[0] - 81.).abs() < 1e-9);
    assert!(!middle.contains_key("back"));
    assert_eq!(animation.fabric_at(5.)["front"].shrink, [72., 100.]);

    // The template shrinks the front to 90 % along the warp; keys replace that value.
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 1, mannequin());
    project
        .template
        .panel_shrink
        .insert("front".into(), [90., 100.]);
    let scene = project.compile().unwrap();
    let base = scene.garment.clone();
    let front = base
        .panels
        .iter()
        .find(|p| p.id == "front")
        .unwrap()
        .clone();
    let area = |g: &marvelous_cloth::garment::CompiledGarment| -> f64 {
        g.mesh.triangles[front.triangles.clone()]
            .iter()
            .map(|t| t.area)
            .sum()
    };
    let at = |pressure: f64, shrink: [f64; 2]| {
        let fabric = BTreeMap::from([("front".to_string(), FabricValues { pressure, shrink })]);
        base.with_fabric(&project.template, &fabric).unwrap()
    };
    let same = at(300., [90., 100.]);
    assert!((area(&same) - area(&base)).abs() < 1e-12);
    assert!(
        same.pressure[front.triangles.clone()]
            .iter()
            .all(|p| *p == 300.)
    );
    let tighter = at(0., [72., 100.]);
    assert!((area(&tighter) / area(&base) - 0.8).abs() < 1e-12);
    assert_eq!(tighter.mesh.masses, base.mesh.masses);
    // The same rest shape as a template asking for 72 % directly.
    let mut direct = project.template.clone();
    direct.panel_shrink.insert("front".into(), [72., 100.]);
    let direct = direct.compile().unwrap();
    for (a, b) in tighter.mesh.triangles.iter().zip(&direct.mesh.triangles) {
        assert!((a.area - b.area).abs() < 1e-15);
        assert!(a.inv_dm.abs_diff_eq(b.inv_dm, 1e-9));
    }
    assert!(
        base.with_fabric(
            &project.template,
            &BTreeMap::from([(
                "front".to_string(),
                FabricValues {
                    pressure: 0.,
                    shrink: [40., 100.]
                }
            )])
        )
        .is_err()
    );

    // Recording: each step's solver sees the front as keyed at the step's end.
    let recorder = Recorder::new(&project, &animation).unwrap();
    let mut seen: Vec<(f64, f64)> = vec![];
    let settings = VbdSettings {
        dt: 1. / 60.,
        iterations: 4,
        ..Default::default()
    };
    run_stepping(
        base.clone(),
        &scene.collider.surface,
        scene.state.clone(),
        recorder,
        &settings,
        |solver, state, step| {
            let g = &solver.garment;
            let p = g.pressure.get(front.triangles.start).copied().unwrap_or(0.);
            seen.push((area(g) / area(&base), p));
            solver.step(state, step, None).map(|_| ())
        },
        |_, _| true,
    )
    .unwrap();
    assert!(seen.len() >= 10);
    assert!(
        seen.windows(2)
            .all(|w| w[1].0 <= w[0].0 + 1e-12 && w[1].1 >= w[0].1)
    );
    let last = seen.last().unwrap();
    assert!((last.0 - 0.8).abs() < 1e-9 && (last.1 - 400.).abs() < 1e-9);
    // Keys out of order or out of range are refused.
    let mut bad = animation.clone();
    bad.fabric_keys.get_mut("front").unwrap().push(FabricKey {
        time_s: 0.1,
        pressure_pa: 0.,
        shrink_percent: [100., 100.],
    });
    assert!(bad.validate().is_err());
    let mut strong = animation;
    set_fabric_key(
        strong.fabric_keys.get_mut("front").unwrap(),
        1.,
        5000.,
        [100., 100.],
    );
    assert!(strong.validate().is_err());
}
