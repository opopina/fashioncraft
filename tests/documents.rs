use fashioncraft::{document::atomic_write, fitting::FittingProject, garment::GarmentTemplate};
use std::{fs, path::PathBuf};

fn project() -> FittingProject {
    let mut p = FittingProject::basic(
        fashioncraft::garment::presets::GarmentKind::Tshirt,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    );
    p.template = GarmentTemplate::two_panel_demo();
    p
}

#[test]
fn portable_document_and_cached_state_survive_relocation() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.obj");
    fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let mut p = project();
    p.avatar_path = source.clone();
    let state = p.compile().unwrap().state;
    p.state = Some(state.clone());
    p.simulation = Some(fashioncraft::vbd::VbdSettings {
        iterations: 27,
        dt: 1. / 120.,
        ..Default::default()
    });
    let bundle = dir.path().join("original");
    fs::create_dir(&bundle).unwrap();
    let file = bundle.join("test.json");
    p.save(&file).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert!(PathBuf::from(json["avatar_path"].as_str().unwrap()).is_relative());
    let moved = dir.path().join("moved");
    fs::rename(&bundle, &moved).unwrap();
    fs::remove_file(source).unwrap();
    let loaded = FittingProject::load(&moved.join("test.json")).unwrap();
    assert_eq!(loaded.simulation, p.simulation);
    let reopened = loaded.compile().unwrap();
    assert_eq!(reopened.state.x, state.x);
    assert_eq!(reopened.state.v, state.v);
    assert_eq!(reopened.state.time, state.time);
}

#[test]
fn failed_save_preserves_previous_document() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.json");
    atomic_write(&file, b"previous valid bytes").unwrap();
    let mut p = project();
    p.avatar_path = dir.path().join("missing.obj");
    assert!(p.save(&file).is_err());
    assert_eq!(fs::read(&file).unwrap(), b"previous valid bytes");
    assert!(atomic_write(dir.path(), b"cannot replace directory").is_err());
    atomic_write(&file, b"replacement").unwrap();
    assert_eq!(fs::read(file).unwrap(), b"replacement");
}

#[test]
fn templates_saved_with_the_earlier_tag_still_open() {
    let current = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/templates/two-panel.json"),
    )
    .unwrap();
    assert!(current.contains("\"clothcraft-template\""));
    let legacy = current.replace("\"clothcraft-template\"", "\"marvelous-cloth-template\"");
    let template: GarmentTemplate = serde_json::from_str(&legacy).unwrap();
    assert_eq!(
        template.format,
        fashioncraft::garment::template::TEMPLATE_FORMAT
    );
    template.compile().unwrap();
    let saved = serde_json::to_string(&template).unwrap();
    assert!(saved.contains("\"clothcraft-template\"") && !saved.contains("marvelous"));
    let unknown = current.replace("\"clothcraft-template\"", "\"other-template\"");
    let template: GarmentTemplate = serde_json::from_str(&unknown).unwrap();
    assert!(template.compile().is_err());
}

#[test]
fn assistant_notes_are_kept_with_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.json");
    let mut p = project();
    p.save(&file).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert!(json.get("assistant_notes").is_none(), "absent when there are none");
    p.assistant_notes = vec!["School uniform; sizes 8 to 14.".into()];
    p.save(&file).unwrap();
    let loaded = FittingProject::load(&file).unwrap();
    assert_eq!(loaded.assistant_notes, p.assistant_notes);
}
