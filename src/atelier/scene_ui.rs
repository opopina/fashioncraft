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

/// Mesh files a scene object or an added avatar can come from.
const OBJECT_FILES: &[&str] = &[
    "obj", "gltf", "glb", "vrm", "stl", "ply", "dae", "fbx", "abc", "usda", "usd", "usdz",
];

/// The corners of the box around `points`.
fn bounds<'a>(points: impl Iterator<Item = &'a glam::DVec3>) -> (glam::DVec3, glam::DVec3) {
    points.fold(
        (
            glam::DVec3::splat(f64::INFINITY),
            glam::DVec3::splat(f64::NEG_INFINITY),
        ),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

/// A new object's scale from its size: files larger than 500 units are taken to be in
/// millimetres, larger than 10 in centimetres, others in metres.
fn scale_of(mesh: &fashioncraft::avatar::SurfaceMesh) -> f64 {
    let (lo, hi) = bounds(mesh.vertices.iter());
    let size = (hi - lo).max_element();
    if size > 500. {
        0.001
    } else if size > 10. {
        0.01
    } else {
        1.
    }
}

fn guess_scale(path: &std::path::Path) -> Result<f64, String> {
    Ok(scale_of(&fashioncraft::mesh_import::load(path)?))
}

/// An object id not used yet among `objects`.
fn new_object_id(objects: &[SceneObject]) -> String {
    (1..)
        .map(|k| format!("object_{k}"))
        .find(|id| !objects.iter().any(|o| &o.id == id))
        .unwrap()
}

impl Atelier {
    /// File › Add › Avatar: another avatar, or a hard object such as armour, that the cloth
    /// collides with, kept with the project's scene objects.
    pub(super) fn add_avatar_object(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Add avatar")
            .add_filter("Avatar", OBJECT_FILES)
            .pick_file()
        {
            self.add_scene_avatar(path);
        }
    }

    /// Add the avatar at `path` to the scene, standing 15 cm to the right of the current one on
    /// the same floor, and open Scene and props to place it.
    pub(super) fn add_scene_avatar(&mut self, path: PathBuf) -> bool {
        let mesh = match fashioncraft::mesh_import::load(&path) {
            Ok(mesh) => mesh,
            Err(e) => {
                self.status = e;
                return false;
            }
        };
        let scale = scale_of(&mesh);
        let (lo, hi) = bounds(mesh.vertices.iter());
        let (lo, hi) = (lo * scale, hi * scale);
        let (body_lo, body_hi) = if self.project.avatar_removed {
            (glam::DVec3::ZERO, glam::DVec3::ZERO)
        } else {
            bounds(self.scene.collider.surface.vertices.iter())
        };
        let translation = [
            body_hi.x + 0.15 - lo.x,
            body_lo.y - lo.y,
            0.5 * (body_lo.z + body_hi.z) - 0.5 * (lo.z + hi.z),
        ];
        let mut objects = self.project.scene_objects.clone();
        let id = new_object_id(&objects);
        let name = path
            .file_stem()
            .map_or_else(|| id.clone(), |s| s.to_string_lossy().into_owned());
        objects.push(SceneObject {
            id,
            name,
            path,
            translation,
            rotation_degrees: [0.; 3],
            scale,
            keys: vec![],
        });
        if !self.apply_scene_objects(objects) {
            return false;
        }
        self.authoring.scene.open = true;
        self.status = i18n::tr(
            self.prefs.language,
            "Avatar added beside the current one · place it in Scene and props",
        )
        .into();
        true
    }

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
                    "Objects and other avatars the cloth rests on or drapes over (OBJ, glTF, FBX, STL, PLY, COLLADA, Alembic or USD); with keys they move while the animation records.",
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
                            .add_filter("Mesh", OBJECT_FILES)
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
                    let id = new_object_id(draft);
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
