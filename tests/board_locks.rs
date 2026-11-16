//! 2D board locks are a selection aid: one undo step, no physical effect, older documents
//! unchanged, and stale ids never survive.
use marvelous_cloth::{
    authoring::{self, Command, History},
    garment::{
        GarmentTemplate,
        presets::{self, GarmentKind},
    },
};
use std::collections::BTreeSet;

fn tshirt() -> GarmentTemplate {
    presets::basic(GarmentKind::Tshirt, 1)
}

fn first_panel(template: &GarmentTemplate) -> BTreeSet<String> {
    template
        .panels
        .iter()
        .take(1)
        .map(|p| p.id.clone())
        .collect()
}

fn lock(source: &GarmentTemplate, panels: BTreeSet<String>, lines: bool) -> authoring::Edit {
    authoring::apply(
        source,
        Command::SetBoardLocks {
            panels,
            internal_lines: lines,
        },
    )
    .unwrap()
}

#[test]
fn locking_is_one_nonphysical_step_that_keeps_the_drape() {
    let source = tshirt();
    let locked = first_panel(&source);
    let edit = lock(&source, locked.clone(), true);
    assert!(!edit.reset_simulation);
    assert!(authoring::same_physics(&source, &edit.template));
    assert!(authoring::only_appearance_changed(&source, &edit.template));
    assert_eq!(edit.template.locked_panels, locked);
    assert!(edit.template.internal_lines_locked);
    assert_eq!(edit.selected, None, "a freshly locked piece is deselected");
    let before = source.compile().unwrap();
    assert_eq!(edit.garment.mesh.uv, before.mesh.uv);
    assert_eq!(edit.garment.initial_state.x, before.initial_state.x);
    // Setting the same locks again is refused instead of recording an empty undo step.
    assert!(
        authoring::apply(
            &edit.template,
            Command::SetBoardLocks {
                panels: locked,
                internal_lines: true,
            },
        )
        .is_err()
    );
}

#[test]
fn undo_and_redo_restore_the_locks() {
    let source = tshirt();
    let mut history = History::default();
    let edit = lock(&source, first_panel(&source), false);
    history.record(source.clone());
    let undone = history.restore(&edit.template, false).unwrap();
    assert!(undone.template.locked_panels.is_empty());
    let redone = history.restore(&undone.template, true).unwrap();
    assert_eq!(redone.template.locked_panels, first_panel(&source));
}

#[test]
fn unknown_or_stale_ids_never_survive() {
    let source = tshirt();
    let unknown: BTreeSet<String> = ["no-such-panel".to_string()].into();
    assert!(
        authoring::apply(
            &source,
            Command::SetBoardLocks {
                panels: unknown.clone(),
                internal_lines: false,
            },
        )
        .is_err()
    );
    let mut hand_edited = source.clone();
    hand_edited.locked_panels = unknown;
    assert!(hand_edited.compile().is_err());
    // Removing a locked piece (the 3D view, scripts and the piece list still reach it)
    // drops its lock with it.
    let locked = lock(&source, first_panel(&source), false).template;
    let id = locked.panels[0].id.clone();
    let removed = authoring::apply(&locked, Command::RemovePanel { panel: id }).unwrap();
    assert!(removed.template.locked_panels.is_empty());
}

#[test]
fn documents_without_locks_are_unchanged_and_locks_round_trip() {
    let source = tshirt();
    let plain = serde_json::to_string(&source).unwrap();
    assert!(!plain.contains("locked_panels") && !plain.contains("internal_lines_locked"));
    let locked = lock(&source, first_panel(&source), true).template;
    let text = serde_json::to_string(&locked).unwrap();
    let back: GarmentTemplate = serde_json::from_str(&text).unwrap();
    assert_eq!(back.locked_panels, locked.locked_panels);
    assert!(back.internal_lines_locked);
    assert_eq!(serde_json::to_string(&back).unwrap(), text);
}
