//! Scene and props: static mesh objects the cloth collides with (a stool, a table…), listed
//! with their placement. Applying rebuilds the obstacle; the drape starts again.
use super::*;
use fashioncraft::fitting::SceneObject;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct SceneUi {
    pub open: bool,
    draft: Option<Vec<SceneObject>>,
}

/// A new object's scale from its size: files larger than 500 units are taken to be in
/// millimetres, larger than 10 in centimetres, others in metres.
fn guess_scale(path: &std::path::Path) -> Result<f64, String> {
    let mesh = fashioncraft::mesh_import::load(path)?;
    let (lo, hi) = mesh.vertices.iter().fold(
        (
            glam::DVec3::splat(f64::INFINITY),
            glam::DVec3::splat(f64::NEG_INFINITY),
        ),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let size = (hi - lo).max_element();
    Ok(if size > 500. {
        0.001
    } else if size > 10. {
        0.01
    } else {
        1.
    })
}

impl Atelier {
    /// Rebuild the scene with `objects`; the drape starts again from the arrangement.
    pub(super) fn apply_scene_objects(&mut self, objects: Vec<SceneObject>) -> bool {
        let ui_lang = self.prefs.language;
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
                self.status = i18n::tr(
                    ui_lang,
                    "Scene updated · the drape starts again from the arrangement",
                )
                .into();
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    pub(super) fn scene_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.scene.open {
            return;
        }
        let now = self.authoring.animation.time();
        let draft = self
            .authoring
            .scene
            .draft
            .get_or_insert_with(|| self.project.scene_objects.clone());
        let mut open = true;
        let mut add: Option<PathBuf> = None;
        let mut apply = false;
        egui::Window::new(i18n::tr(ui_lang, "Scene and props"))
            .open(&mut open)
            .default_width(320.)
            .resizable(false)
            .show(ctx, |ui| {
                ui.small(i18n::tr(
                    ui_lang,
                    "OBJ, STL, PLY, COLLADA or FBX objects the cloth rests on or drapes over; with keys they move while the animation records.",
                ));
                if draft.is_empty() {
                    ui.label(i18n::tr(ui_lang, "No objects yet."));
                }
                let mut remove = None;
                for (k, object) in draft.iter_mut().enumerate() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong(&object.name);
                        if ui.small_button(i18n::tr(ui_lang, "Remove")).clicked() {
                            remove = Some(k);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(i18n::tr(ui_lang, "Position"));
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
                        ui.label(i18n::tr(ui_lang, "Turn"));
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
                        ui.label(i18n::tr(ui_lang, "Scale"));
                        ui.add(
                            egui::DragValue::new(&mut object.scale)
                                .range(0.0001..=1000.)
                                .speed(0.01),
                        )
                        .on_hover_text(i18n::tr(ui_lang, "From the file's units to metres"));
                    });
                    // Keys: where the object is at times of the avatar's animation.
                    ui.horizontal(|ui| {
                        if ui
                            .small_button(format!("{} {now:.2} s", i18n::tr(ui_lang, "Key at")))
                            .on_hover_text(i18n::tr(
                                ui_lang,
                                "Keep this position and turn at the animation's current time; while recording, the object moves from key to key",
                            ))
                            .clicked()
                        {
                            let key = fashioncraft::fitting::ObjectKey {
                                time_s: now,
                                translation: object.translation,
                                rotation_degrees: object.rotation_degrees,
                            };
                            object.keys.retain(|k| (k.time_s - now).abs() > 1e-6);
                            let at = object.keys.partition_point(|k| k.time_s < now);
                            object.keys.insert(at, key);
                        }
                        if !object.keys.is_empty() {
                            ui.small(format!(
                                "{}: {}",
                                i18n::tr(ui_lang, "Keys"),
                                object.keys.len()
                            ));
                            if ui.small_button(i18n::tr(ui_lang, "Clear keys")).clicked() {
                                object.keys.clear();
                            }
                        }
                    });
                }
                if let Some(k) = remove {
                    draft.remove(k);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(i18n::tr(ui_lang, "Add object…")).clicked() {
                        add = rfd::FileDialog::new()
                            .add_filter(
                                "Mesh",
                                &["obj", "stl", "ply", "dae", "fbx", "abc", "usda", "usd", "usdz"],
                            )
                            .pick_file();
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Apply"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Rebuild the obstacle with these objects; the drape starts again",
                        ))
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
                        keys: vec![],
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
