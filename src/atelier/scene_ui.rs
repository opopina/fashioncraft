//! Scene and props: static OBJ objects the cloth collides with (a stool, a table…), listed
//! with their placement. Applying rebuilds the obstacle; the drape starts again.
use super::*;
use marvelous_cloth::fitting::SceneObject;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct SceneUi {
    pub open: bool,
    draft: Option<Vec<SceneObject>>,
}

/// A new object's scale from its size: files whose height exceeds 10 units are taken to be
/// in centimetres, others in metres.
fn guess_scale(path: &std::path::Path) -> Result<f64, String> {
    let mesh = marvelous_cloth::avatar::SurfaceMesh::load_obj(path)?;
    let (lo, hi) = mesh.vertices.iter().fold(
        (
            glam::DVec3::splat(f64::INFINITY),
            glam::DVec3::splat(f64::NEG_INFINITY),
        ),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    Ok(if (hi - lo).max_element() > 10. {
        0.01
    } else {
        1.
    })
}

impl Atelier {
    /// Rebuild the scene with `objects`; the drape starts again from the arrangement.
    pub(super) fn apply_scene_objects(&mut self, objects: Vec<SceneObject>) -> bool {
        let mut project = self.project.clone();
        project.scene_objects = objects;
        project.state = None;
        project.mesh_signature = None;
        match project.compile() {
            Ok(scene) => {
                self.project = project;
                self.scene = scene;
                self.generation += 1;
                self.playing = false;
                self.pending = false;
                self.grab = None;
                self.drag_plane = None;
                self.issue.invalidate();
                self.metrics = None;
                self.simulation_error = None;
                self.document.dirty = true;
                self.authoring.scene.draft = None;
                self.status = "Scene updated · the drape starts again from the arrangement".into();
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    pub(super) fn scene_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.scene.open {
            return;
        }
        let draft = self
            .authoring
            .scene
            .draft
            .get_or_insert_with(|| self.project.scene_objects.clone());
        let mut open = true;
        let mut add: Option<PathBuf> = None;
        let mut apply = false;
        egui::Window::new("Scene and props")
            .open(&mut open)
            .default_width(320.)
            .resizable(false)
            .show(ctx, |ui| {
                ui.small("Static OBJ objects the cloth rests on or drapes over.");
                if draft.is_empty() {
                    ui.label("No objects yet.");
                }
                let mut remove = None;
                for (k, object) in draft.iter_mut().enumerate() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(&object.name);
                        if ui.small_button("Remove").clicked() {
                            remove = Some(k);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Position");
                        for (axis, v) in ["X", "Y", "Z"].iter().zip(object.translation.iter_mut()) {
                            let mut mm = *v * 1000.;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut mm)
                                        .speed(5.)
                                        .prefix(format!("{axis} "))
                                        .suffix(" mm"),
                                )
                                .changed()
                            {
                                *v = mm / 1000.;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Turn");
                        for (axis, v) in ["X", "Y", "Z"]
                            .iter()
                            .zip(object.rotation_degrees.iter_mut())
                        {
                            ui.add(
                                egui::DragValue::new(v)
                                    .range(-360.0..=360.)
                                    .speed(1.)
                                    .prefix(format!("{axis} "))
                                    .suffix("°"),
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Scale");
                        ui.add(
                            egui::DragValue::new(&mut object.scale)
                                .range(0.0001..=1000.)
                                .speed(0.01),
                        )
                        .on_hover_text("From the file's units to metres");
                    });
                }
                if let Some(k) = remove {
                    draft.remove(k);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Add OBJ…").clicked() {
                        add = rfd::FileDialog::new()
                            .add_filter("OBJ mesh", &["obj"])
                            .pick_file();
                    }
                    if ui
                        .button("Apply")
                        .on_hover_text(
                            "Rebuild the obstacle with these objects; the drape starts again",
                        )
                        .clicked()
                    {
                        apply = true;
                    }
                });
            });
        if let Some(path) = add {
            match guess_scale(&path) {
                Ok(scale) => {
                    let draft = self.authoring.scene.draft.get_or_insert_with(Vec::new);
                    let id = (1..)
                        .map(|k| format!("object_{k}"))
                        .find(|id| !draft.iter().any(|o| &o.id == id))
                        .unwrap();
                    let name = path
                        .file_stem()
                        .map_or_else(|| id.clone(), |s| s.to_string_lossy().into_owned());
                    draft.push(SceneObject {
                        id,
                        name,
                        path,
                        translation: [0.6, 0., 0.],
                        rotation_degrees: [0.; 3],
                        scale,
                    });
                }
                Err(e) => self.status = e,
            }
        }
        if apply && let Some(objects) = self.authoring.scene.draft.clone() {
            self.apply_scene_objects(objects);
        }
        self.authoring.scene.open &= open;
        if !self.authoring.scene.open {
            self.authoring.scene.draft = None;
        }
    }
}
