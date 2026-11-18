//! Avatar pose: a window with ready-made poses and per-bone rotations, and an X-ray view of
//! the skeleton drawn over the 3D view. Applying a pose rebuilds the avatar collider; the
//! drape starts again from the flat arrangement.
use super::*;
use marvelous_cloth::{
    avatar_rig::{self, Rig},
    fitting::AvatarPose,
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct PoseUi {
    pub open: bool,
    /// Draw the skeleton over the 3D view.
    pub skeleton: bool,
    bone: usize,
    /// The pose being edited, until applied.
    draft: Option<AvatarPose>,
    /// Bones of the current avatar, for the X-ray view, keyed by the joints they came from.
    bones: Option<(String, Arc<Rig>)>,
    error: Option<String>,
}

impl Atelier {
    /// The avatar's pose as stored, or its joints at rest when it has none yet.
    fn current_pose(&self) -> Result<AvatarPose, String> {
        match &self.project.avatar_pose {
            Some(p) => Ok(p.clone()),
            None => AvatarPose::from_sidecar(&self.project.avatar_path),
        }
    }

    /// Rebuild the avatar in `pose`; the drape restarts from the flat arrangement.
    pub(super) fn apply_pose(&mut self, pose: AvatarPose) -> bool {
        let mut project = self.project.clone();
        project.avatar_pose = Some(pose);
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
                self.status = "Pose applied · the drape starts again from the arrangement".into();
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    pub(super) fn pose_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.pose.open {
            return;
        }
        if self.authoring.pose.draft.is_none() {
            match self.current_pose() {
                Ok(p) => self.authoring.pose.draft = Some(p),
                Err(e) => self.authoring.pose.error = Some(e),
            }
        }
        let mut open = true;
        let mut apply = None;
        egui::Window::new("Avatar pose")
            .open(&mut open)
            .default_width(300.)
            .resizable(false)
            .show(ctx, |ui| {
                let state = &mut self.authoring.pose;
                if let Some(e) = &state.error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                    return;
                }
                let Some(draft) = state.draft.as_mut() else {
                    return;
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label("Preset");
                    for name in avatar_rig::PRESETS {
                        if ui.small_button(name).clicked()
                            && let Some(p) = avatar_rig::preset(name)
                        {
                            draft.pose = p;
                        }
                    }
                });
                ui.separator();
                let names: Vec<&str> = avatar_rig::bone_names().collect();
                state.bone = state.bone.min(names.len() - 1);
                egui::ComboBox::from_label("Bone")
                    .selected_text(names[state.bone])
                    .show_ui(ui, |ui| {
                        for (k, name) in names.iter().enumerate() {
                            ui.selectable_value(&mut state.bone, k, *name);
                        }
                    });
                let bone = names[state.bone].to_string();
                let mut angles = draft.pose.0.get(&bone).copied().unwrap_or([0.; 3]);
                let mut changed = false;
                ui.horizontal(|ui| {
                    for (axis, value) in ["X", "Y", "Z"].iter().zip(angles.iter_mut()) {
                        changed |= ui
                            .add(
                                egui::DragValue::new(value)
                                    .range(-180.0..=180.)
                                    .speed(0.5)
                                    .prefix(format!("{axis} "))
                                    .suffix("°"),
                            )
                            .changed();
                    }
                });
                ui.small(
                    "Turns about the bone's joint, around the world axes at rest; children follow.",
                );
                if changed {
                    if angles == [0.; 3] {
                        draft.pose.0.remove(&bone);
                    } else {
                        draft.pose.0.insert(bone, angles);
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Apply pose")
                        .on_hover_text("Rebuild the avatar in this pose; the drape starts again")
                        .clicked()
                    {
                        apply = Some(draft.clone());
                    }
                    if ui.button("Rest").clicked() {
                        draft.pose = Default::default();
                    }
                });
                ui.checkbox(&mut state.skeleton, "Show skeleton (X-ray)");
            });
        self.authoring.pose.open &= open;
        if !self.authoring.pose.open {
            self.authoring.pose.draft = None;
            self.authoring.pose.error = None;
        }
        if let Some(pose) = apply {
            self.apply_pose(pose);
        }
    }

    /// The skeleton of the current avatar in its current pose, drawn over the 3D view.
    pub(super) fn paint_skeleton(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if !self.authoring.pose.skeleton {
            return;
        }
        let Ok(pose) = self.current_pose() else {
            return;
        };
        let key = format!("{:?}", pose.joints);
        let fresh = self
            .authoring
            .pose
            .bones
            .as_ref()
            .is_none_or(|(k, _)| k != &key);
        if fresh {
            // Bones only: an empty surface needs no weights.
            let empty = marvelous_cloth::avatar::SurfaceMesh {
                vertices: vec![],
                triangles: vec![],
            };
            match pose.rig(&empty) {
                Ok(rig) => self.authoring.pose.bones = Some((key, Arc::new(rig))),
                Err(_) => return,
            }
        }
        let rig = self.authoring.pose.bones.as_ref().unwrap().1.clone();
        let painter = ui.painter().with_clip_rect(rect);
        let colour = Color32::from_rgb(0x46, 0xD1, 0xFF);
        for (head, tail) in rig.posed_bones(&pose.pose) {
            let (Some(a), Some(b)) = (
                self.camera.project(head.as_vec3(), rect),
                self.camera.project(tail.as_vec3(), rect),
            ) else {
                continue;
            };
            painter.line_segment([a, b], Stroke::new(3., colour));
            painter.circle_filled(a, 4., colour);
            painter.circle_stroke(b, 3., Stroke::new(1.5, colour));
        }
    }
}
