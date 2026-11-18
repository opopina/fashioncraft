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
    pub draft: Option<AvatarPose>,
    /// Bones of the current avatar, for the X-ray view, keyed by the joints they came from.
    bones: Option<(String, Arc<Rig>)>,
    error: Option<String>,
    /// A pose change in progress while the cloth stays on the body.
    transition: Option<Transition>,
    /// The limb whose end is being dragged, and the camera-facing plane it moves in.
    reaching: Option<(usize, glam::DVec3, glam::DVec3)>,
}

impl PoseUi {
    pub(super) fn transition_running(&self) -> bool {
        self.transition.is_some()
    }
}

/// How near (points) a press must land to a hand or foot to drag it.
const GRAB_PX: f32 = 14.;

/// Largest distance any avatar vertex moves in one simulation step of a pose change: below
/// the contact radius, so the body pushes the cloth instead of passing through it.
const POSE_STEP_M: f64 = 0.0015;

/// The avatar moving from one pose to another, one simulation step at a time.
pub(super) struct Transition {
    rig: Arc<Rig>,
    rest: Arc<marvelous_cloth::avatar::SurfaceMesh>,
    /// Scene objects, added after the body as part of the same obstacle.
    props: Option<Arc<marvelous_cloth::avatar::SurfaceMesh>>,
    from: avatar_rig::Pose,
    to: AvatarPose,
    step: usize,
    steps: usize,
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

    /// Move the avatar into `pose` over many simulation steps, the cloth following it.
    pub(super) fn begin_pose_transition(&mut self, pose: AvatarPose) -> bool {
        let start = || -> Result<Transition, String> {
            let rest = marvelous_cloth::avatar::SurfaceMesh::load_obj(&self.project.avatar_path)?;
            let rig = pose.rig(&rest)?;
            let from = self
                .project
                .avatar_pose
                .as_ref()
                .map(|p| p.pose.clone())
                .unwrap_or_default();
            let a = rig.posed(&rest, &from);
            let b = rig.posed(&rest, &pose.pose);
            let farthest = a
                .vertices
                .iter()
                .zip(&b.vertices)
                .map(|(p, q)| p.distance(*q))
                .fold(0., f64::max);
            let steps = ((farthest / POSE_STEP_M).ceil() as usize).clamp(5, 900);
            let props = marvelous_cloth::fitting::scene_objects_mesh(&self.project.scene_objects)?
                .map(Arc::new);
            Ok(Transition {
                rig: Arc::new(rig),
                rest: Arc::new(rest),
                props,
                from,
                to: pose.clone(),
                step: 0,
                steps,
            })
        };
        match start() {
            Ok(t) => {
                self.status = format!("Moving into the pose over {} steps…", t.steps);
                self.authoring.pose.transition = Some(t);
                self.playing = true;
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    /// After each accepted simulation step, move the avatar one step further into its new pose.
    pub(super) fn advance_pose_transition(&mut self) {
        let Some(t) = self.authoring.pose.transition.as_mut() else {
            return;
        };
        t.step += 1;
        let fraction = t.step as f64 / t.steps as f64;
        let blend = t.from.blend(&t.to.pose, fraction.min(1.));
        let transforms = t
            .rig
            .transforms(|bone| blend.get(bone).copied().unwrap_or_default());
        let mut body = t.rig.skin(&t.rest, &transforms);
        if let Some(props) = &t.props {
            body = body.merged(props);
        }
        let surface = Arc::new(body);
        match marvelous_cloth::avatar::MeshCollider::with_clearance(
            surface,
            self.project.contact_stiffness,
            self.project.contact_clearance,
        ) {
            Ok(collider) => self.scene.collider = Arc::new(collider),
            Err(e) => {
                self.status = e;
                self.authoring.pose.transition = None;
                return;
            }
        }
        if t.step >= t.steps {
            let done = self.authoring.pose.transition.take().unwrap();
            self.project.avatar_pose = Some(done.to);
            self.document.dirty = true;
            self.status = "In the new pose · the drape followed the body".into();
        } else {
            self.status = format!("Moving into the pose… {:.0} %", fraction * 100.);
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
                let moving = state.transition.as_ref().map(|t| t.step * 100 / t.steps);
                if let Some(percent) = moving {
                    ui.label(format!("Moving into the pose… {percent} %"));
                }
                ui.add_enabled_ui(moving.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .button("Move into pose")
                            .on_hover_text(
                                "Move the avatar into this pose while simulating: the drape \
                                 follows the body",
                            )
                            .clicked()
                        {
                            apply = Some((draft.clone(), true));
                        }
                        if ui
                            .button("Apply and restart")
                            .on_hover_text(
                                "Rebuild the avatar in this pose at once; the drape starts again",
                            )
                            .clicked()
                        {
                            apply = Some((draft.clone(), false));
                        }
                        if ui.button("Rest").clicked() {
                            draft.pose = Default::default();
                        }
                    });
                });
                ui.checkbox(&mut state.skeleton, "Show skeleton (X-ray)");
            });
        self.authoring.pose.open &= open;
        if !self.authoring.pose.open {
            self.authoring.pose.draft = None;
            self.authoring.pose.error = None;
        }
        match apply {
            Some((pose, true)) => {
                self.begin_pose_transition(pose);
            }
            Some((pose, false)) => {
                self.apply_pose(pose);
            }
            None => {}
        }
    }

    /// The skeleton of the current avatar in its current pose, drawn over the 3D view.
    pub(super) fn paint_skeleton(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if !self.authoring.pose.skeleton {
            return;
        }
        // The pose being edited shows while the window is open.
        let Ok(pose) = self
            .authoring
            .pose
            .draft
            .clone()
            .map_or_else(|| self.current_pose(), Ok)
        else {
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
        // Hands and feet can be dragged while the pose window is open.
        if self.authoring.pose.draft.is_some() {
            for (_, _, lower) in avatar_rig::LIMBS {
                if let Some(end) = rig.limb_end(&pose.pose, lower)
                    && let Some(at) = self.camera.project(end.as_vec3(), rect)
                {
                    painter.circle_stroke(at, GRAB_PX * 0.6, Stroke::new(2., Color32::WHITE));
                }
            }
        }
    }

    /// Drag a hand or foot of the pose being edited; the limb follows by inverse kinematics.
    /// Returns whether the 3D view's pointer belongs to this drag.
    pub(super) fn ik_interaction(&mut self, response: &egui::Response, rect: egui::Rect) -> bool {
        let state = &self.authoring.pose;
        if !state.open || !state.skeleton || state.transition.is_some() {
            return false;
        }
        let (Some(draft), Some((_, rig))) = (state.draft.clone(), state.bones.clone()) else {
            return false;
        };
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.ctx.input(|i| i.pointer.press_origin())
        {
            let hit = avatar_rig::LIMBS
                .iter()
                .enumerate()
                .find_map(|(k, (_, _, lower))| {
                    let end = rig.limb_end(&draft.pose, lower)?;
                    let at = self.camera.project(end.as_vec3(), rect)?;
                    (at.distance(pos) <= GRAB_PX).then_some((k, end))
                });
            if let Some((k, end)) = hit {
                self.authoring.pose.reaching = Some((k, end, self.camera.forward().as_dvec3()));
                self.status = format!(
                    "Reaching with the {}",
                    avatar_rig::LIMBS[k].0.to_lowercase()
                );
            }
        }
        let Some((k, point, normal)) = self.authoring.pose.reaching else {
            return false;
        };
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (origin, dir) = self.camera.ray(pos, rect);
            if let Some(target) = picking::ray_plane(origin, dir, point, normal) {
                let (_, upper, lower) = avatar_rig::LIMBS[k];
                if let Ok(next) = rig.reach(&draft.pose, upper, lower, target)
                    && let Some(d) = self.authoring.pose.draft.as_mut()
                {
                    d.pose = next;
                }
            }
        }
        if response.drag_stopped() {
            self.authoring.pose.reaching = None;
            self.status = "Limb placed · Move into pose to bring the avatar there".into();
        }
        true
    }
}
