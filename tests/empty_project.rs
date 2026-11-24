//! A project with no pieces: the avatar alone. It compiles, saves and loads, and has nothing
//! to simulate or export; the first piece added to it simulates as any other, and deleting
//! the last piece leaves it empty again.
use fashioncraft::{
    authoring::{self, Command},
    fitting::FittingProject,
    garment::{export, presets, shapes::Primitive},
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec2;
use std::path::PathBuf;

fn mannequin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
}

/// A 40 × 60 cm rectangle, as the Rectangle tool draws it on the board.
fn rectangle(fabric: &str) -> Command {
    Command::Primitive {
        kind: Primitive::Rectangle,
        size: DVec2::new(0.4, 0.6),
        sweep_degrees: 180.,
        origin: DVec2::ZERO,
        meshing: Default::default(),
        fabric: fabric.into(),
        replace: false,
    }
}

#[test]
fn an_empty_project_compiles_saves_and_loads() {
    let project = FittingProject::empty(mannequin());
    assert!(project.template.panels.is_empty() && project.template.seams.is_empty());
    assert!(!project.template.fabrics.is_empty(), "a fabric to cut from");
    let scene = project.compile().unwrap();
    assert!(scene.garment.mesh.uv.is_empty() && scene.garment.mesh.triangles.is_empty());
    assert!(scene.state.x.is_empty());
    // The avatar is there: it is what the 3D view shows.
    assert!(!scene.collider.surface.vertices.is_empty());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.json");
    project.save(&path).unwrap();
    let back = FittingProject::load(&path).unwrap();
    assert!(back.template.panels.is_empty());
    back.compile().unwrap();
    // A garment export has nothing to write: it says so, and the window says it first.
    assert!(
        export::write_artifacts(
            &project.template,
            &scene.garment,
            &scene.state,
            &dir.path().join("session"),
        )
        .is_err()
    );
    assert!(export::pattern_svg(&project.template).contains("<svg"));
}

#[test]
fn an_empty_garment_has_nothing_to_step() {
    let scene = FittingProject::empty(mannequin()).compile().unwrap();
    let mut state = scene.state.clone();
    let mut solver =
        VbdSolver::new(scene.garment.clone(), Some(&scene.collider.surface), &state).unwrap();
    solver
        .step(&mut state, &VbdSettings::default(), None)
        .unwrap();
    assert!(state.x.is_empty());
}

#[test]
fn the_first_piece_drawn_simulates() {
    let mut project = FittingProject::empty(mannequin());
    let fabric = project.template.fabrics[0].id.clone();
    let edit = authoring::apply(&project.template, rectangle(&fabric)).unwrap();
    assert_eq!(edit.template.panels.len(), 1);
    assert!(edit.selected.is_some());
    project.template = edit.template;
    let scene = project.compile().unwrap();
    assert!(!scene.garment.mesh.uv.is_empty());
    let mut state = scene.state.clone();
    let start: f64 = state.x.iter().map(|p| p.y).sum::<f64>() / state.x.len() as f64;
    let mut solver =
        VbdSolver::new(scene.garment.clone(), Some(&scene.collider.surface), &state).unwrap();
    let settings = VbdSettings {
        dt: 1. / 60.,
        ..Default::default()
    };
    for _ in 0..20 {
        solver.step(&mut state, &settings, None).unwrap();
    }
    assert!(state.x.iter().all(|p| p.is_finite()));
    let now: f64 = state.x.iter().map(|p| p.y).sum::<f64>() / state.x.len() as f64;
    assert!(now < start - 0.01, "the piece falls: {start} → {now}");
}

#[test]
fn deleting_the_last_piece_leaves_an_empty_project() {
    let mut project = FittingProject::empty(mannequin());
    let fabric = project.template.fabrics[0].id.clone();
    let one = authoring::apply(&project.template, rectangle(&fabric))
        .unwrap()
        .template;
    let panel = one.panels[0].id.clone();
    let gone = authoring::apply(&one, Command::RemovePanel { panel }).unwrap();
    assert!(gone.template.panels.is_empty());
    assert!(gone.garment.mesh.uv.is_empty());
    project.template = gone.template;
    project.compile().unwrap();
    // A whole garment goes at once too, with its seams.
    let tshirt = presets::basic(presets::GarmentKind::Tshirt, 0);
    let all = authoring::apply(
        &tshirt,
        Command::RemovePanels {
            panels: tshirt.panels.iter().map(|p| p.id.clone()).collect(),
        },
    )
    .unwrap();
    assert!(all.template.panels.is_empty() && all.template.seams.is_empty());
    // And a template brings pieces back into it.
    let mut back = project.clone();
    back.template = tshirt;
    assert!(!back.compile().unwrap().garment.mesh.uv.is_empty());
}
