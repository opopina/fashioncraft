//! An avatar's own anchors and joints travel with it: through the import confirmation and
//! into a saved project's assets, so they are not estimated again from its shape.
use fashioncraft::{
    avatar_import::Candidate,
    document::portable_avatar,
    fitting::FittingProject,
    garment::{arrangement::load_sidecar, presets::GarmentKind},
    vbd::VbdSettings,
};
use std::path::{Path, PathBuf};

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

fn shaped_copy(dir: &Path) -> PathBuf {
    let source = mannequin().parent().unwrap().to_path_buf();
    let avatar = dir.join("avatar.obj");
    std::fs::copy(mannequin(), &avatar).unwrap();
    for name in ["arrangement.json", "measurements.json"] {
        std::fs::copy(source.join(name), dir.join(name)).unwrap();
    }
    avatar
}

#[test]
fn imported_avatars_keep_their_joints() {
    let dir = tempfile::tempdir().unwrap();
    let avatar = shaped_copy(dir.path());
    let candidate = Candidate::load(&avatar).unwrap();
    let project = FittingProject::basic(GarmentKind::Tshirt, 0, mannequin());
    let (staged, _, _keep) = candidate
        .prepare(&project, VbdSettings::default())
        .unwrap()
        .into_parts();
    let sidecar = load_sidecar(&staged.avatar_path).unwrap().unwrap();
    assert!(!sidecar.estimated, "the shipped joints are used");
    let original = load_sidecar(&avatar).unwrap().unwrap();
    assert_eq!(sidecar.landmarks.len(), original.landmarks.len());
    // Without a sidecar the joints are estimated, as before.
    let bare = tempfile::tempdir().unwrap();
    let lone = bare.path().join("body.obj");
    std::fs::copy(mannequin(), &lone).unwrap();
    let candidate = Candidate::load(&lone).unwrap();
    let (staged, _, _keep) = candidate
        .prepare(&project, VbdSettings::default())
        .unwrap()
        .into_parts();
    assert!(
        load_sidecar(&staged.avatar_path)
            .unwrap()
            .unwrap()
            .estimated
    );
}

#[test]
fn saved_avatars_keep_their_joints() {
    let dir = tempfile::tempdir().unwrap();
    let avatar = shaped_copy(dir.path());
    let out = tempfile::tempdir().unwrap();
    let project = out.path().join("look.json");
    let relative = portable_avatar(&avatar, &project).unwrap();
    let saved = out.path().join(&relative);
    let sidecar = load_sidecar(&saved).unwrap().unwrap();
    assert!(!sidecar.estimated);
    let original = load_sidecar(&avatar).unwrap().unwrap();
    for (a, b) in sidecar.landmarks.iter().zip(&original.landmarks) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.position_m, b.position_m);
    }
    // A second avatar saved into the same assets keeps its own files.
    let other = tempfile::tempdir().unwrap();
    let lone = other.path().join("body.obj");
    let mut bytes = std::fs::read(mannequin()).unwrap();
    bytes.extend_from_slice(b"\n# another\n");
    std::fs::write(&lone, bytes).unwrap();
    let second = out.path().join(portable_avatar(&lone, &project).unwrap());
    assert!(load_sidecar(&second).unwrap().unwrap().estimated);
    assert!(!load_sidecar(&saved).unwrap().unwrap().estimated);
}
