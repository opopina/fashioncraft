//! Scene objects: static OBJ props the cloth collides with, placed in the scene, merged with
//! the avatar into one obstacle and kept with the project when it is saved.
use fashioncraft::{
    fitting::{AvatarPose, FittingProject, SceneObject},
    garment::presets::GarmentKind,
};
use glam::DVec3;
use std::path::PathBuf;

/// A 1 m cube OBJ centred on its origin.
fn cube(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("cube.obj");
    let mut text = String::new();
    for z in [-0.5, 0.5] {
        for y in [-0.5, 0.5] {
            for x in [-0.5, 0.5] {
                text += &format!("v {x} {y} {z}\n");
            }
        }
    }
    for f in [
        [1, 3, 4, 2],
        [5, 6, 8, 7],
        [1, 2, 6, 5],
        [3, 7, 8, 4],
        [1, 5, 7, 3],
        [2, 4, 8, 6],
    ] {
        text += &format!(
            "f {} {} {}\nf {} {} {}\n",
            f[0], f[1], f[2], f[0], f[2], f[3]
        );
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn project() -> FittingProject {
    FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
}

#[test]
fn a_prop_joins_the_obstacle_where_it_is_placed_and_travels_with_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    let body = p.compile().unwrap().collider.surface.vertices.len();
    p.scene_objects.push(SceneObject {
        id: "object_1".into(),
        name: "Stool".into(),
        path: cube(dir.path()),
        translation: [0.6, 0.25, 0.],
        rotation_degrees: [0., 45., 0.],
        scale: 0.5,
        keys: vec![],
    });
    let scene = p.compile().unwrap();
    let surface = &scene.collider.surface;
    assert_eq!(surface.vertices.len(), body + 8);
    // A half-metre cube resting on the floor beside the avatar, turned 45°.
    let prop = &surface.vertices[body..];
    let lo = prop.iter().fold(DVec3::MAX, |m, v| m.min(*v));
    let hi = prop.iter().fold(DVec3::MIN, |m, v| m.max(*v));
    assert!((lo.y - 0.0).abs() < 1e-9 && (hi.y - 0.5).abs() < 1e-9);
    let half_diagonal = 0.25 * 2f64.sqrt();
    assert!((hi.x - (0.6 + half_diagonal)).abs() < 1e-9, "{hi:?}");
    // Saved, the OBJ is copied into the project's assets and the placement kept.
    let file = dir.path().join("saved").join("scene.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    p.save(&file).unwrap();
    let reopened = FittingProject::load(&file).unwrap();
    assert!(
        reopened.scene_objects[0]
            .path
            .starts_with(file.parent().unwrap())
    );
    assert_eq!(reopened.scene_objects[0].translation, [0.6, 0.25, 0.]);
    assert_eq!(
        reopened.compile().unwrap().collider.surface.vertices.len(),
        body + 8
    );
    // With a posed avatar the prop still comes after the body.
    let mut posed = reopened.clone();
    let mut pose = AvatarPose::from_sidecar(&p.avatar_path).unwrap();
    pose.pose = fashioncraft::avatar_rig::preset("T-pose").unwrap();
    posed.avatar_pose = Some(pose);
    let scene = posed.compile().unwrap();
    assert_eq!(scene.collider.surface.vertices.len(), body + 8);
    // Nonsense placements are refused.
    let mut bad = p.clone();
    bad.scene_objects[0].scale = 0.;
    assert!(bad.compile().is_err());
}

#[test]
fn keyed_objects_move_between_their_keys() {
    use fashioncraft::fitting::ObjectKey;
    let dir = tempfile::tempdir().unwrap();
    let object = SceneObject {
        id: "object_1".into(),
        name: "Box".into(),
        path: cube(dir.path()),
        translation: [0.6, 0.25, 0.],
        rotation_degrees: [0.; 3],
        scale: 0.2,
        keys: vec![
            ObjectKey {
                time_s: 0.,
                translation: [0.6, 0.25, 0.],
                rotation_degrees: [0.; 3],
            },
            ObjectKey {
                time_s: 1.,
                translation: [0.6, 0.45, 0.],
                rotation_degrees: [0., 90., 0.],
            },
        ],
    };
    object.validate().unwrap();
    let (q, p) = object.transform_at(0.5);
    assert!((p - DVec3::new(0.6, 0.35, 0.)).length() < 1e-12);
    let (angle_axis, angle) = q.to_axis_angle();
    assert!((angle - 45f64.to_radians()).abs() < 1e-9 && angle_axis.y > 0.99);
    // Before the first key and after the last it holds them.
    assert_eq!(object.transform_at(-1.).1, DVec3::new(0.6, 0.25, 0.));
    assert_eq!(object.transform_at(5.).1, DVec3::new(0.6, 0.45, 0.));
    // Keys out of order are refused.
    let mut bad = object.clone();
    bad.keys.swap(0, 1);
    assert!(bad.validate().is_err());
}

#[test]
fn recording_carries_keyed_objects_in_small_steps() {
    use fashioncraft::{
        avatar_animation::{AvatarAnimation, PoseKey, Recorder},
        fitting::ObjectKey,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.scene_objects.push(SceneObject {
        id: "object_1".into(),
        name: "Box".into(),
        path: cube(dir.path()),
        translation: [0.6, 0.25, 0.],
        rotation_degrees: [0.; 3],
        scale: 0.2,
        keys: vec![
            ObjectKey {
                time_s: 0.,
                translation: [0.6, 0.25, 0.],
                rotation_degrees: [0.; 3],
            },
            ObjectKey {
                time_s: 0.5,
                translation: [0.6, 0.30, 0.],
                rotation_degrees: [0.; 3],
            },
        ],
    });
    // The avatar stands still: only the box moves.
    let rest = AvatarPose::from_sidecar(&p.avatar_path).unwrap();
    let animation = AvatarAnimation {
        keys: vec![
            PoseKey {
                time_s: 0.,
                pose: rest.pose.clone(),
            },
            PoseKey {
                time_s: 0.5,
                pose: rest.pose.clone(),
            },
        ],
        ..Default::default()
    };
    let mut recorder = Recorder::new(&p, &animation).unwrap();
    let cloth = p.compile().unwrap().state.x.clone();
    recorder.begin(&cloth);
    let top = |mesh: &fashioncraft::avatar::SurfaceMesh| {
        mesh.vertices[mesh.vertices.len() - 8..]
            .iter()
            .map(|v| v.y)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut last = top(&recorder.obstacle_now());
    let mut steps = 0;
    while !recorder.finished() && steps < 10_000 {
        let (_, obstacle) = recorder.plan(1. / 60.);
        let now = top(&obstacle);
        assert!(
            (now - last).abs() <= 0.0015 + 1e-9,
            "{steps}: {last} → {now}"
        );
        last = now;
        recorder.commit(&cloth);
        steps += 1;
    }
    assert!(recorder.finished());
    assert!(
        (last - (0.30 + 0.1)).abs() < 1e-6,
        "the box reached its last key: {last}"
    );
    assert!(steps >= 33, "5 cm in steps of at most 1.5 mm: {steps}");
}
