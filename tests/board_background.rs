//! The 2D board's reference image travels with the project like the avatar does: saving copies
//! it into the `.assets` folder under a content hash, loading resolves it, and projects without
//! one are written exactly as before.
use marvelous_cloth::{
    fitting::{BoardBackground, FittingProject},
    garment::presets::GarmentKind,
};
use std::{fs, path::PathBuf};

fn project() -> FittingProject {
    FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
}

fn background(path: PathBuf) -> BoardBackground {
    BoardBackground {
        path,
        origin_m: [0.1, -0.2],
        width_m: 0.8,
        opacity: 0.4,
        rotation_degrees: 12.5,
    }
}

#[test]
fn the_background_travels_with_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let scan = dir.path().join("scan.PNG");
    fs::write(&scan, b"image bytes are copied, not decoded").unwrap();
    let mut source = project();
    source.board_background = Some(background(scan));
    let file = dir.path().join("coat.json");
    source.save(&file).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let stored = json["board_background"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(stored.starts_with("coat.json.assets"), "{stored}");
    assert!(
        stored.ends_with(".png"),
        "extension is kept, lowercase: {stored}"
    );
    assert_eq!(
        fs::read(dir.path().join(&stored)).unwrap(),
        b"image bytes are copied, not decoded"
    );

    let loaded = FittingProject::load(&file).unwrap();
    let restored = loaded.board_background.clone().unwrap();
    assert_eq!(restored.path, dir.path().join(&stored));
    assert_eq!(restored.origin_m, [0.1, -0.2]);
    assert_eq!(restored.width_m, 0.8);
    assert_eq!(restored.opacity, 0.4);
    assert_eq!(restored.rotation_degrees, 12.5);

    // Saving again reuses the same content-addressed asset.
    loaded.save(&file).unwrap();
    let assets = fs::read_dir(dir.path().join("coat.json.assets"))
        .unwrap()
        .count();
    assert_eq!(assets, 2, "one avatar and one image");
}

#[test]
fn projects_without_a_background_are_written_as_before() {
    let text = serde_json::to_string(&project()).unwrap();
    assert!(!text.contains("board_background"));
    let back: FittingProject = serde_json::from_str(&text).unwrap();
    assert!(back.board_background.is_none());
}

#[test]
fn impossible_backgrounds_are_refused_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let scan = dir.path().join("scan.png");
    fs::write(&scan, b"x").unwrap();
    for broken in [
        BoardBackground {
            width_m: 0.0,
            ..background(scan.clone())
        },
        BoardBackground {
            width_m: f64::NAN,
            ..background(scan.clone())
        },
        BoardBackground {
            origin_m: [f64::INFINITY, 0.],
            ..background(scan.clone())
        },
        BoardBackground {
            opacity: 1.5,
            ..background(scan.clone())
        },
    ] {
        assert!(broken.validate().is_err(), "{broken:?}");
        let mut source = project();
        source.board_background = Some(broken);
        let file = dir.path().join("broken.json");
        assert!(source.save(&file).is_err());
        assert!(!file.exists());
    }
    // A missing image is reported instead of silently dropped.
    let mut source = project();
    source.board_background = Some(background(dir.path().join("missing.png")));
    assert!(source.save(&dir.path().join("missing.json")).is_err());
}
