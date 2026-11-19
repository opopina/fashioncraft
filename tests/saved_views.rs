//! Named 3D views travel with the project, are validated on load and save, and projects
//! without any are written exactly as before.
use fashioncraft::{
    fitting::{FittingProject, SavedView},
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

fn view(name: &str) -> SavedView {
    SavedView {
        name: name.into(),
        yaw_degrees: 30.,
        pitch_degrees: 10.,
        distance_m: 2.4,
        target_m: [0., 1.1, 0.05],
        orthographic: false,
        fov_degrees: 35.,
    }
}

#[test]
fn views_round_trip_through_a_saved_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = project();
    source.saved_views = vec![view("Front detail"), view("Back")];
    let file = dir.path().join("coat.json");
    source.save(&file).unwrap();
    let loaded = FittingProject::load(&file).unwrap();
    assert_eq!(loaded.saved_views, source.saved_views);
}

#[test]
fn projects_without_views_are_written_as_before() {
    let text = serde_json::to_string(&project()).unwrap();
    assert!(!text.contains("saved_views"));
    assert!(
        serde_json::from_str::<FittingProject>(&text)
            .unwrap()
            .saved_views
            .is_empty()
    );
}

#[test]
fn impossible_views_are_refused_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let broken = [
        SavedView {
            name: "  ".into(),
            ..view("x")
        },
        SavedView {
            fov_degrees: 200.,
            ..view("wide")
        },
        SavedView {
            pitch_degrees: 120.,
            ..view("upside")
        },
        SavedView {
            distance_m: 0.,
            ..view("inside")
        },
        SavedView {
            target_m: [f64::NAN, 0., 0.],
            ..view("lost")
        },
    ];
    for view in broken {
        assert!(view.validate().is_err(), "{view:?}");
        let mut source = project();
        source.saved_views = vec![view];
        let file = dir.path().join("broken.json");
        assert!(source.save(&file).is_err());
        assert!(!file.exists());
    }
    // Two views with the same name (ignoring case and spaces) are ambiguous.
    let mut source = project();
    source.saved_views = vec![view("Side"), view(" side ")];
    assert!(source.save(&dir.path().join("twins.json")).is_err());
    // A hand-edited file with a bad view is refused on load as well.
    let mut good = project();
    good.saved_views = vec![view("Side")];
    let file = dir.path().join("edited.json");
    good.save(&file).unwrap();
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace("\"fov_degrees\": 35.0", "\"fov_degrees\": 1.0");
    fs::write(&file, text).unwrap();
    assert!(FittingProject::load(&file).is_err());
}
