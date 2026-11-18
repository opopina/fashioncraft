//! The pattern archive: pieces taken off the board and out of the simulation, kept in the
//! document with their seams, and brought back where they were.
use marvelous_cloth::{
    authoring::{self, Command},
    fitting::FittingProject,
    garment::{GarmentTemplate, presets::GarmentKind},
};

fn tshirt() -> GarmentTemplate {
    FittingProject::basic(
        GarmentKind::Tshirt,
        1,
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj"),
    )
    .template
}

fn run(t: &GarmentTemplate, command: Command) -> Result<GarmentTemplate, String> {
    authoring::apply(t, command).map(|edit| edit.template)
}

#[test]
fn archived_pieces_leave_the_board_and_come_back_where_they_were() {
    let original = tshirt();
    let front = original.panels[0].clone();
    let seams = original.seams.len();
    assert!(seams > 0 && original.panels.len() == 2);
    let archived = run(
        &original,
        Command::ArchivePanels {
            panels: vec![front.id.clone()],
        },
    )
    .unwrap();
    assert_eq!(archived.panels.len(), 1);
    // Every seam joined the front to the back: none is left on the board.
    assert!(archived.seams.is_empty());
    assert_eq!(archived.archive.len(), 1);
    assert_eq!(archived.archive[0].names, vec![front.name.clone()]);
    archived.compile().unwrap();

    let id = archived.archive[0].id.clone();
    let restored = run(&archived, Command::RestoreArchived { id }).unwrap();
    assert!(restored.archive.is_empty());
    assert_eq!(restored.panels.len(), 2);
    assert_eq!(restored.seams.len(), seams, "its seams are sewn again");
    let back = restored
        .panels
        .iter()
        .find(|p| p.id == front.id)
        .expect("the piece keeps its id");
    assert_eq!(back.name, front.name);
    assert_eq!(
        serde_json::to_value(&back.placement).unwrap(),
        serde_json::to_value(&front.placement).unwrap(),
        "same 3D arrangement"
    );
    let at = |t: &GarmentTemplate| {
        let i = t.panels.iter().position(|p| p.id == front.id).unwrap();
        authoring::layout(t, i)
    };
    assert!(at(&restored).distance(at(&original)) < 1e-9);
    restored.compile().unwrap();
}

#[test]
fn deleting_from_the_archive_keeps_the_drape_and_refusals_change_nothing() {
    let original = tshirt();
    let ids: Vec<String> = original.panels.iter().map(|p| p.id.clone()).collect();
    // The last piece cannot go.
    assert!(
        run(
            &original,
            Command::ArchivePanels {
                panels: ids.clone()
            }
        )
        .is_err()
    );
    assert!(
        run(
            &original,
            Command::ArchivePanels {
                panels: vec!["missing".into()]
            }
        )
        .is_err()
    );
    let archived = run(
        &original,
        Command::ArchivePanels {
            panels: vec![ids[1].clone()],
        },
    )
    .unwrap();
    let id = archived.archive[0].id.clone();
    let edit = authoring::apply(&archived, Command::DeleteArchived { id: id.clone() }).unwrap();
    assert!(edit.template.archive.is_empty());
    // Dropping archived pieces never touches the simulated garment.
    assert!(!edit.reset_simulation);
    assert!(run(&edit.template, Command::RestoreArchived { id }).is_err());
}

#[test]
fn the_archive_is_saved_with_the_document() {
    let original = tshirt();
    let archived = run(
        &original,
        Command::ArchivePanels {
            panels: vec![original.panels[0].id.clone()],
        },
    )
    .unwrap();
    let text = serde_json::to_string(&archived).unwrap();
    let back: GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(back.archive.len(), 1);
    assert!(
        !serde_json::to_string(&original)
            .unwrap()
            .contains("\"archive\"")
    );
}
