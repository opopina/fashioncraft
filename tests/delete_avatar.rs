//! A project whose avatar was deleted: the scene has no body within reach and the cloth hangs
//! and falls on its own; the setting is saved with the project.
use fashioncraft::{
    fitting::FittingProject,
    garment::presets::GarmentKind,
    vbd::{VbdSettings, VbdSolver},
};
use std::path::PathBuf;

#[test]
fn without_an_avatar_the_cloth_falls_freely() {
    let avatar = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj");
    let mut project = FittingProject::basic(GarmentKind::Tshirt, 0, avatar);
    project.avatar_removed = true;
    let scene = project.compile().unwrap();
    assert!(scene.collider.surface.vertices.iter().all(|p| p.y < -999.));
    let mut state = scene.state.clone();
    let start = state.x.iter().map(|p| p.y).sum::<f64>() / state.x.len() as f64;
    let mut solver =
        VbdSolver::new(scene.garment.clone(), Some(&scene.collider.surface), &state).unwrap();
    let settings = VbdSettings::default();
    for _ in 0..30 {
        solver.step(&mut state, &settings, None).unwrap();
    }
    let end = state.x.iter().map(|p| p.y).sum::<f64>() / state.x.len() as f64;
    assert!(end < start - 0.001, "falls: {start} → {end}");
    // Saved with the project, and absent while there is an avatar.
    let text = serde_json::to_string(&project).unwrap();
    assert!(text.contains("\"avatar_removed\":true"));
    project.avatar_removed = false;
    assert!(
        !serde_json::to_string(&project)
            .unwrap()
            .contains("avatar_removed")
    );
}
