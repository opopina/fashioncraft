//! Local demo for the EXO-AV-002 prototype: writes a fitting project whose T-shirt panels are placed
//! with landmark-referenced anchors, ready to open with `cloth-fit --project <file> --steps 0 --view`.
use glam::DVec3;
use marvelous_cloth::{
    avatar::{SurfaceMesh, diagnostics},
    avatar_measurements::{AvatarFrame, Landmark},
    fitting::FittingProject,
    garment::{
        arrangement::{ArrangementSet, placement_for},
        presets::GarmentKind,
    },
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn main() -> Result<(), String> {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "output/arrangement-demo".into()));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    #[derive(serde::Deserialize)]
    struct Prepared {
        landmarks: BTreeMap<String, [f64; 3]>,
    }
    let prepared: Prepared = serde_json::from_str(include_str!("../assets/makehuman/measurements.json")).map_err(|e| e.to_string())?;
    let landmarks: Vec<Landmark> = prepared.landmarks.into_iter().map(|(id, v)| Landmark { id, position_m: DVec3::from_array(v) }).collect();
    let frame = AvatarFrame { avatar_id: "makehuman-mannequin".into(), revision: 1 };
    let set = ArrangementSet::from_json(include_str!("../tests/fixtures/arrangement-makehuman.json"))?;
    let avatar_path = root.join("assets/makehuman/mannequin.obj");
    let avatar = SurfaceMesh::load_obj(&avatar_path)?;
    for (name, front_clearance) in [("anclas-torso", 0.25), ("anclas-dentro-del-cuerpo", 0.02)] {
        let mut project = FittingProject::basic(GarmentKind::Tshirt, 2, avatar_path.clone());
        for (panel, anchor, c) in [("front", "torso-front", front_clearance), ("back", "torso-back", 0.2)] {
            let f = set.evaluate(&frame, &landmarks, anchor)?;
            let p = project.template.panels.iter_mut().find(|p| p.id == panel).ok_or("panel not found")?;
            let pts = p.polygon.as_ref().ok_or("panel without polygon")?.points.clone();
            p.placement = placement_for(&pts, &f, c)?;
        }
        let garment = Arc::new(project.template.compile()?);
        let state = garment.initial_state.clone();
        let gate = diagnostics::check_vbd_initialization(garment, &avatar, &state);
        let file = out.join(format!("{name}.json"));
        project.save(&file)?;
        println!("{} → {} (comprobación de inicio: {})", name, file.display(), match gate { Ok(()) => "OK, la tela empieza fuera del cuerpo".to_string(), Err(e) => format!("RECHAZADA: {e}") });
    }
    Ok(())
}
