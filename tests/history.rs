//! Undo history: every step keeps the name of its edit, the steps can be listed, and going
//! back or forward several at once lands where single steps would, or changes nothing.
use marvelous_cloth::{
    authoring::{self, Command, History},
    garment::GarmentTemplate,
};

fn json(t: &GarmentTemplate) -> String {
    serde_json::to_string(t).unwrap()
}

/// The demo garment after `edits`, with the template before each one recorded.
fn edited(edits: Vec<Command>) -> (Vec<GarmentTemplate>, History) {
    let mut history = History::default();
    let mut states = vec![GarmentTemplate::two_panel_demo()];
    for command in edits {
        let name = command.name();
        let now = states.last().unwrap().clone();
        let next = authoring::apply(&now, command).unwrap().template;
        history.record_named(now, name);
        states.push(next);
    }
    (states, history)
}

fn panel() -> Vec<String> {
    vec![GarmentTemplate::two_panel_demo().panels[0].id.clone()]
}

#[test]
fn steps_are_listed_by_name_and_jumped_to() {
    let (states, mut history) = edited(vec![
        Command::SetPressure {
            panels: panel(),
            pressure: 20.,
        },
        Command::SetLayer {
            panels: panel(),
            layer: 2,
        },
        Command::SetPressure {
            panels: panel(),
            pressure: -5.,
        },
    ]);
    assert_eq!(
        history.done().collect::<Vec<_>>(),
        ["Pressure", "Layer", "Pressure"]
    );
    assert_eq!(history.undone().count(), 0);

    // Two back at once is the template after the first edit.
    let back = history
        .restore_steps_checked(&states[3], false, 2, |_| Ok(()))
        .unwrap();
    assert_eq!(json(&back.template), json(&states[1]));
    assert_eq!(history.done().collect::<Vec<_>>(), ["Pressure"]);
    assert_eq!(history.undone().collect::<Vec<_>>(), ["Layer", "Pressure"]);

    // One forward redoes the layer, then one more the last pressure.
    let redo = history.restore(&back.template, true).unwrap();
    assert_eq!(json(&redo.template), json(&states[2]));
    let redo = history.restore(&redo.template, true).unwrap();
    assert_eq!(json(&redo.template), json(&states[3]));
    assert!(!history.can_redo());

    // All the way back, then all the way forward again.
    let start = history
        .restore_steps_checked(&states[3], false, 3, |_| Ok(()))
        .unwrap();
    assert_eq!(json(&start.template), json(&states[0]));
    let end = history
        .restore_steps_checked(&start.template, true, 3, |_| Ok(()))
        .unwrap();
    assert_eq!(json(&end.template), json(&states[3]));
    assert_eq!(
        history.done().collect::<Vec<_>>(),
        ["Pressure", "Layer", "Pressure"]
    );
}

#[test]
fn a_refused_or_missing_step_changes_nothing() {
    let (states, mut history) = edited(vec![
        Command::SetLayer {
            panels: panel(),
            layer: 1,
        },
        Command::SetLayer {
            panels: panel(),
            layer: 3,
        },
    ]);
    assert!(
        history
            .restore_steps_checked(&states[2], false, 2, |_| Err("no".into()))
            .is_err()
    );
    assert!(
        history
            .restore_steps_checked(&states[2], false, 3, |_| Ok(()))
            .is_err()
    );
    assert!(
        history
            .restore_steps_checked(&states[2], false, 0, |_| Ok(()))
            .is_err()
    );
    assert_eq!(history.done().count(), 2);
    assert_eq!(history.undone().count(), 0);
    // A new edit after undoing drops what was undone.
    history.restore(&states[2], false).unwrap();
    assert_eq!(history.undone().count(), 1);
    history.record_named(states[1].clone(), "Layer");
    assert_eq!(history.undone().count(), 0);
}

#[test]
fn only_the_latest_steps_are_kept() {
    let mut history = History::default();
    let t = GarmentTemplate::two_panel_demo();
    for k in 0..authoring::HISTORY_STEPS + 5 {
        history.record_named(t.clone(), format!("edit {k}"));
    }
    let names: Vec<_> = history.done().collect();
    assert_eq!(names.len(), authoring::HISTORY_STEPS);
    assert_eq!(names[0], "edit 5");
}
