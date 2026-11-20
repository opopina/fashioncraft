//! Python scripts: a script reads the open garment, changes it and saves it back.
use fashioncraft::{
    authoring::{self, Command},
    garment::presets::{self, GarmentKind},
    scripting,
};
use std::time::Duration;

fn python_present() -> bool {
    std::process::Command::new(scripting::python())
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn width(p: &fashioncraft::garment::template::Panel) -> f64 {
    let xs = p.polygon.as_ref().unwrap().points.iter().map(|q| q.x);
    xs.clone().fold(f64::NEG_INFINITY, f64::max) - xs.fold(f64::INFINITY, f64::min)
}

#[test]
fn a_script_changes_the_garment_through_the_helper() {
    if !python_present() {
        eprintln!("skipped: no Python 3");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("grow front.py");
    std::fs::write(
        &script,
        "import fashioncraft\n\
         g = fashioncraft.garment()\n\
         first = fashioncraft.pieces(g)[0]\n\
         print('pieces:', len(fashioncraft.pieces(g)))\n\
         fashioncraft.rename_piece(g, first['id'], 'Grown')\n\
         fashioncraft.scale_piece(g, 'Grown', 1.1, 1.0)\n\
         fashioncraft.save(g)\n",
    )
    .unwrap();
    let template = presets::basic(GarmentKind::Tshirt, 0);
    let run = scripting::run(
        &script,
        &template,
        &dir.path().join("work"),
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(run.code, Some(0), "{}", run.output);
    assert!(
        run.output
            .contains(&format!("pieces: {}", template.panels.len())),
        "{}",
        run.output
    );
    let saved = run.garment.expect("the script saved");
    let edit = authoring::apply(
        &template,
        Command::RunScript {
            template: Box::new(saved),
        },
    )
    .unwrap();
    let grown = &edit.template.panels[0];
    assert_eq!(grown.name, "Grown");
    assert!((width(grown) / width(&template.panels[0]) - 1.1).abs() < 1e-6);
    edit.template.compile().unwrap();
}

#[test]
fn a_failing_or_silent_script_changes_nothing() {
    if !python_present() {
        eprintln!("skipped: no Python 3");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let template = presets::basic(GarmentKind::Tshirt, 0);
    let work = dir.path().join("work");
    // An error: its message is kept, nothing is saved.
    let broken = dir.path().join("broken.py");
    std::fs::write(&broken, "raise ValueError('no sleeves here')\n").unwrap();
    let run = scripting::run(&broken, &template, &work, Duration::from_secs(60)).unwrap();
    assert_ne!(run.code, Some(0));
    assert!(run.output.contains("no sleeves here"), "{}", run.output);
    assert!(run.garment.is_none());
    // A script that never saves leaves no garment, even after an earlier run saved one.
    let quiet = dir.path().join("quiet.py");
    std::fs::write(&quiet, "print('looked only')\n").unwrap();
    let run = scripting::run(&quiet, &template, &work, Duration::from_secs(60)).unwrap();
    assert_eq!(run.code, Some(0));
    assert!(run.garment.is_none());
    // One that runs too long is stopped.
    let slow = dir.path().join("slow.py");
    std::fs::write(&slow, "import time\ntime.sleep(30)\n").unwrap();
    let run = scripting::run(&slow, &template, &work, Duration::from_millis(500)).unwrap();
    assert_eq!(run.code, None);
    assert!(run.output.contains("Stopped"), "{}", run.output);
}
