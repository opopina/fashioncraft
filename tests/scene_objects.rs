//! Scene objects: static OBJ props the cloth collides with, placed in the scene, merged with
//! the avatar into one obstacle and kept with the project when it is saved.
use glam::DVec3;
use marvelous_cloth::{
    fitting::{AvatarPose, FittingProject, SceneObject},
    garment::presets::GarmentKind,
};
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
    pose.pose = marvelous_cloth::avatar_rig::preset("T-pose").unwrap();
    posed.avatar_pose = Some(pose);
    let scene = posed.compile().unwrap();
    assert_eq!(scene.collider.surface.vertices.len(), body + 8);
    // Nonsense placements are refused.
    let mut bad = p.clone();
    bad.scene_objects[0].scale = 0.;
    assert!(bad.compile().is_err());
}
