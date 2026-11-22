//! Avatar pose: a window with ready-made poses and per-bone rotations, and an X-ray view of
//! the skeleton drawn over the 3D view. Applying a pose rebuilds the avatar collider; the
//! drape starts again from the flat arrangement.
use super::*;
use fashioncraft::{
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
    /// Name for the pose being saved to the user's poses.
    new_name: String,
    /// Drag joints instead of hands and feet: the skeleton's fit, edited at rest.
    pub move_joints: bool,
    /// The joint being dragged and the camera-facing plane it moves in.
    joint_drag: Option<(String, glam::DVec3, glam::DVec3)>,
    /// Adding a bone: none until started, then its head once picked on the body.
    pub adding_bone: Option<Option<glam::DVec3>>,
    /// Painting skin weights onto the selected bone, with this brush.
    pub painting: bool,
    brush_cm: f64,
    brush_strength: f64,
    brush_remove: bool,
    /// The rest avatar and its rig with skin weights, keyed by the draft's skeleton.
    skin: Option<(String, Arc<fashioncraft::avatar::SurfaceMesh>, Arc<Rig>)>,
}

/// Brush defaults: radius in centimetres and strength per dab.
const BRUSH_CM: f64 = 4.;
const BRUSH_STRENGTH: f64 = 0.3;

/// The draft's skeleton as a cache key: joints and added bones.
fn skeleton_key(pose: &AvatarPose) -> String {
    format!("{:?}{:?}", pose.joints, pose.added_bones)
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
    rest: Arc<fashioncraft::avatar::SurfaceMesh>,
    /// Scene objects, added after the body as part of the same obstacle.
    props: Option<Arc<fashioncraft::avatar::SurfaceMesh>>,
    from: avatar_rig::Pose,
    to: AvatarPose,
    step: usize,
    steps: usize,
}

impl Atelier {
    /// The avatar's pose as stored, or its joints at rest when it has none yet.
    pub(super) fn current_pose(&self) -> Result<AvatarPose, String> {
        match &self.project.avatar_pose {
            Some(p) => Ok(p.clone()),
            None => AvatarPose::from_sidecar(&self.project.avatar_path),
        }
    }

    /// Rebuild the avatar in `pose`; the drape restarts from the flat arrangement.
    pub(super) fn apply_pose(&mut self, pose: AvatarPose) -> bool {
        let ui_lang = self.prefs.language;
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
                self.status = i18n::tr(
                    ui_lang,
                    "Pose applied · the drape starts again from the arrangement",
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

    /// Move the avatar into `pose` over many simulation steps, the cloth following it.
    pub(super) fn begin_pose_transition(&mut self, pose: AvatarPose) -> bool {
        let start = || -> Result<Transition, String> {
            let rest = fashioncraft::avatar::SurfaceMesh::load_obj(&self.project.avatar_path)?;
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
            let props = fashioncraft::fitting::scene_objects_mesh(&self.project.scene_objects)?
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
        let ui_lang = self.prefs.language;
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
        match fashioncraft::avatar::MeshCollider::with_clearance(
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
            self.status = i18n::tr(ui_lang, "In the new pose · the drape followed the body").into();
        } else {
            self.status = format!("Moving into the pose… {:.0} %", fraction * 100.);
        }
    }

    pub(super) fn pose_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
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
        egui::Window::new(i18n::tr(ui_lang, "Avatar pose"))
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
                super::workspace::prop_row(ui, i18n::tr(ui_lang, "Preset"), |ui| {
                    for name in avatar_rig::PRESETS {
                        if ui.small_button(i18n::tr(ui_lang, name)).clicked()
                            && let Some(p) = avatar_rig::preset(name)
                        {
                            draft.pose = p;
                        }
                    }
                });
                // The user's own poses: click to use, right-click to forget.
                let user_poses = &mut self.prefs.user_poses;
                if !user_poses.is_empty() {
                    let mut forget = None;
                    super::workspace::prop_row(ui, i18n::tr(ui_lang, "My poses"), |ui| {
                        for (name, pose) in user_poses.iter() {
                            let button = ui.small_button(name).on_hover_text(i18n::tr(
                                ui_lang,
                                "Use this pose · right-click to delete it",
                            ));
                            if button.clicked() {
                                draft.pose = pose.clone();
                            }
                            if button.secondary_clicked() {
                                forget = Some(name.clone());
                            }
                        }
                    });
                    if let Some(name) = forget {
                        user_poses.remove(&name);
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_name)
                            .hint_text(i18n::tr(ui_lang, "Pose name"))
                            .char_limit(super::settings::POSE_NAME_MAX)
                            .desired_width(150.),
                    );
                    let name = state.new_name.trim().to_string();
                    let room = user_poses.len() < super::settings::USER_POSES_MAX
                        || user_poses.contains_key(&name);
                    if ui
                        .add_enabled(
                            !name.is_empty() && room,
                            egui::Button::new(i18n::tr(ui_lang, "Save pose")),
                        )
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Keep this pose under its name for every project",
                        ))
                        .clicked()
                    {
                        user_poses.insert(name, draft.pose.clone());
                        state.new_name.clear();
                    }
                });
                ui.separator();
                let names: Vec<String> = avatar_rig::bone_names()
                    .map(String::from)
                    .chain(draft.added_bones.iter().map(|b| b.name.clone()))
                    .collect();
                state.bone = state.bone.min(names.len() - 1);
                egui::ComboBox::from_label(i18n::tr(ui_lang, "Bone"))
                    .selected_text(names[state.bone].as_str())
                    .show_ui(ui, |ui| {
                        for (k, name) in names.iter().enumerate() {
                            ui.selectable_value(&mut state.bone, k, name.as_str());
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
                ui.small(i18n::tr(
                    ui_lang,
                    "Turns about the bone's joint, around the world axes at rest; children follow.",
                ));
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
                            .button(i18n::tr(ui_lang, "Move into pose"))
                            .on_hover_text(i18n::tr(
                                ui_lang,
                                "Move the avatar into this pose while simulating: the drape \
                                 follows the body",
                            ))
                            .clicked()
                        {
                            apply = Some((draft.clone(), true));
                        }
                        if ui
                            .button(i18n::tr(ui_lang, "Apply and restart"))
                            .on_hover_text(i18n::tr(
                                ui_lang,
                                "Rebuild the avatar in this pose at once; the drape starts again",
                            ))
                            .clicked()
                        {
                            apply = Some((draft.clone(), false));
                        }
                        if ui.button(i18n::tr(ui_lang, "Rest")).clicked() {
                            draft.pose = Default::default();
                        }
                    });
                });
                ui.checkbox(
                    &mut state.skeleton,
                    i18n::tr(ui_lang, "Show skeleton (X-ray)"),
                );
                if state.skeleton {
                    let at_rest = draft.pose.is_empty();
                    ui.add_enabled_ui(at_rest, |ui| {
                        ui.checkbox(&mut state.move_joints, i18n::tr(ui_lang, "Move joints"))
                            .on_hover_text(i18n::tr(
                                ui_lang,
                                "Drag the skeleton's joints to fit the body; Apply and restart uses them",
                            ))
                            .on_disabled_hover_text(i18n::tr(
                                ui_lang,
                                "Put the avatar at rest to move its joints",
                            ));
                    });
                    if !at_rest {
                        state.move_joints = false;
                    }
                }
                ui.separator();
                ui.collapsing(super::workspace::section(i18n::tr(ui_lang, "Bones and skin weights")), |ui| {
                    let mut remove = None;
                    for (k, b) in draft.added_bones.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ({})", b.name, b.parent));
                            if ui
                                .small_button("×")
                                .on_hover_text(i18n::tr(ui_lang, "Remove this bone and the bones hanging from it"))
                                .clicked()
                            {
                                remove = Some(k);
                            }
                        });
                    }
                    if let Some(k) = remove {
                        remove_bone(draft, k);
                        state.bones = None;
                        state.skin = None;
                    }
                    match state.adding_bone {
                        None => {
                            if ui
                                .button(i18n::tr(ui_lang, "Add bone"))
                                .on_hover_text(i18n::tr(
                                    ui_lang,
                                    "Click its two ends on the body; it hangs from the nearest bone",
                                ))
                                .clicked()
                            {
                                state.adding_bone = Some(None);
                                state.painting = false;
                                state.skeleton = true;
                            }
                        }
                        Some(head) => {
                            ui.horizontal(|ui| {
                                ui.label(i18n::tr(
                                    ui_lang,
                                    if head.is_none() {
                                        "Click the bone's first end on the body"
                                    } else {
                                        "Click the bone's other end"
                                    },
                                ));
                                if ui.small_button(i18n::tr(ui_lang, "Cancel")).clicked() {
                                    state.adding_bone = None;
                                }
                            });
                        }
                    }
                    if ui
                        .checkbox(&mut state.painting, i18n::tr(ui_lang, "Paint weights"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Drag on the body to make the skin there follow the selected bone",
                        ))
                        .changed()
                        && state.painting
                    {
                        state.adding_bone = None;
                    }
                    if state.painting {
                        if state.brush_cm <= 0. {
                            state.brush_cm = BRUSH_CM;
                            state.brush_strength = BRUSH_STRENGTH;
                        }
                        ui.label(format!(
                            "{}: {}",
                            i18n::tr(ui_lang, "Painting"),
                            names[state.bone]
                        ));
                        super::workspace::slim(ui, egui::Slider::new(&mut state.brush_cm, 1.0..=20.)
                                .suffix(" cm")
                                .text(i18n::tr(ui_lang, "Radius")),
                        );
                        super::workspace::slim(ui, egui::Slider::new(&mut state.brush_strength, 0.05..=1.)
                                .text(i18n::tr(ui_lang, "Strength")),
                        );
                        ui.checkbox(&mut state.brush_remove, i18n::tr(ui_lang, "Remove"));
                    }
                    if ui
                        .add_enabled(
                            !draft.painted_weights.is_empty(),
                            egui::Button::new(i18n::tr(ui_lang, "Clear painted weights")),
                        )
                        .clicked()
                    {
                        draft.painted_weights.clear();
                    }
                    ui.small(i18n::tr(
                        ui_lang,
                        "Apply and restart uses the new bones and weights.",
                    ));
                });
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
        let key = skeleton_key(&pose);
        let fresh = self
            .authoring
            .pose
            .bones
            .as_ref()
            .is_none_or(|(k, _)| k != &key);
        if fresh {
            // Bones only: an empty surface needs no weights.
            let empty = fashioncraft::avatar::SurfaceMesh {
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
        // Joints can be dragged while fitting the skeleton.
        if self.authoring.pose.move_joints {
            for (id, p) in &pose.joints {
                if let Some(at) = self
                    .camera
                    .project(glam::DVec3::from_array(*p).as_vec3(), rect)
                {
                    let dragged = self
                        .authoring
                        .pose
                        .joint_drag
                        .as_ref()
                        .is_some_and(|(d, _, _)| d == id);
                    let side = if dragged { 5. } else { 3.5 };
                    painter.rect_filled(
                        egui::Rect::from_center_size(at, egui::vec2(side, side) * 2.),
                        1.,
                        if dragged {
                            Color32::YELLOW
                        } else {
                            Color32::WHITE
                        },
                    );
                }
            }
            return;
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

    /// Drag a joint of the skeleton (at rest) in the plane facing the camera.
    fn joint_interaction(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
        draft: AvatarPose,
    ) -> bool {
        let ui_lang = self.prefs.language;
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.ctx.input(|i| i.pointer.press_origin())
        {
            let hit = draft
                .joints
                .iter()
                .filter_map(|(id, p)| {
                    let p = glam::DVec3::from_array(*p);
                    let at = self.camera.project(p.as_vec3(), rect)?;
                    Some((id.clone(), p, at.distance(pos)))
                })
                .filter(|(_, _, d)| *d <= GRAB_PX)
                .min_by(|a, b| a.2.total_cmp(&b.2));
            if let Some((id, p, _)) = hit {
                self.authoring.pose.joint_drag = Some((id, p, self.camera.forward().as_dvec3()));
            }
        }
        let Some((id, point, normal)) = self.authoring.pose.joint_drag.clone() else {
            return false;
        };
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (origin, dir) = self.camera.ray(pos, rect);
            if let Some(target) = picking::ray_plane(origin, dir, point, normal) {
                match draft.with_joint(&id, target) {
                    Ok(moved) => {
                        self.authoring.pose.draft = Some(moved);
                        self.authoring.pose.bones = None;
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if response.drag_stopped() {
            self.authoring.pose.joint_drag = None;
            self.status = format!(
                "{} · {}",
                id,
                i18n::tr(
                    ui_lang,
                    "joint moved · Apply and restart to use the new skeleton"
                )
            );
        }
        true
    }

    /// Drag a hand or foot of the pose being edited; the limb follows by inverse kinematics.
    /// Returns whether the 3D view's pointer belongs to this drag.
    pub(super) fn ik_interaction(&mut self, response: &egui::Response, rect: egui::Rect) -> bool {
        let ui_lang = self.prefs.language;
        let state = &self.authoring.pose;
        if state.open
            && state.transition.is_none()
            && (state.adding_bone.is_some() || state.painting)
        {
            return self.skin_interaction(response, rect);
        }
        if !state.open || !state.skeleton || state.transition.is_some() {
            return false;
        }
        let (Some(draft), Some((_, rig))) = (state.draft.clone(), state.bones.clone()) else {
            return false;
        };
        if state.move_joints {
            return self.joint_interaction(response, rect, draft);
        }
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
            self.status = i18n::tr(
                ui_lang,
                "Limb placed · Move into pose to bring the avatar there",
            )
            .into();
        }
        true
    }
}

/// Remove added bone `k` from `pose`, with the bones hanging from it, their painted weights
/// and their rotations.
fn remove_bone(pose: &mut AvatarPose, k: usize) {
    let mut gone = std::collections::BTreeSet::from([pose.added_bones[k].name.clone()]);
    loop {
        let more: Vec<String> = pose
            .added_bones
            .iter()
            .filter(|b| gone.contains(&b.parent) && !gone.contains(&b.name))
            .map(|b| b.name.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        gone.extend(more);
    }
    pose.added_bones.retain(|b| !gone.contains(&b.name));
    for list in pose.painted_weights.values_mut() {
        list.retain(|(b, _)| !gone.contains(b));
    }
    pose.painted_weights.retain(|_, list| !list.is_empty());
    pose.pose.0.retain(|b, _| !gone.contains(b));
}

impl Atelier {
    /// The rest avatar and the draft's rig with skin weights, built once per skeleton.
    fn skin(&mut self) -> Option<(Arc<fashioncraft::avatar::SurfaceMesh>, Arc<Rig>)> {
        let draft = self.authoring.pose.draft.as_ref()?;
        let key = skeleton_key(draft);
        if self
            .authoring
            .pose
            .skin
            .as_ref()
            .is_none_or(|(k, _, _)| *k != key)
        {
            let rest =
                fashioncraft::avatar::SurfaceMesh::load_obj(&self.project.avatar_path).ok()?;
            let plain = AvatarPose {
                painted_weights: Default::default(),
                ..draft.clone()
            };
            let rig = plain.rig(&rest).ok()?;
            self.authoring.pose.skin = Some((key, Arc::new(rest), Arc::new(rig)));
        }
        let (_, rest, rig) = self.authoring.pose.skin.as_ref()?;
        Some((rest.clone(), rig.clone()))
    }

    /// The selected bone's name in the pose window.
    fn selected_bone(&self) -> Option<String> {
        let draft = self.authoring.pose.draft.as_ref()?;
        avatar_rig::bone_names()
            .map(str::to_string)
            .chain(draft.added_bones.iter().map(|b| b.name.clone()))
            .nth(self.authoring.pose.bone)
    }

    /// Clicks and drags on the body while adding a bone or painting weights.
    fn skin_interaction(&mut self, response: &egui::Response, rect: egui::Rect) -> bool {
        let ui_lang = self.prefs.language;
        let Some((rest, rig)) = self.skin() else {
            return false;
        };
        let picked = |at: egui::Pos2, this: &Self| {
            let (origin, dir) = this.camera.ray(at, rect);
            fashioncraft::trace3d::pick(&this.scene.collider.surface, origin, dir)
                .filter(|v| *v < rest.vertices.len())
        };
        if self.authoring.pose.adding_bone.is_some() {
            if response.clicked()
                && let Some(at) = response.interact_pointer_pos()
            {
                let Some(v) = picked(at, self) else {
                    self.status = i18n::tr(ui_lang, "Click on the body").into();
                    return true;
                };
                let point = rest.vertices[v];
                match self.authoring.pose.adding_bone {
                    Some(None) => self.authoring.pose.adding_bone = Some(Some(point)),
                    Some(Some(head)) => {
                        self.authoring.pose.adding_bone = None;
                        self.add_bone(&rig, head, point);
                    }
                    None => {}
                }
            }
            return true;
        }
        // Painting: one dab per frame while the button is down over the body.
        let down = response.is_pointer_button_down_on() || response.clicked();
        if down
            && let Some(at) = response.interact_pointer_pos().or(response.hover_pos())
            && let Some(v) = picked(at, self)
            && let Some(bone) = self.selected_bone()
        {
            let state = &mut self.authoring.pose;
            let strength = if state.brush_remove {
                -state.brush_strength
            } else {
                state.brush_strength
            };
            let radius = state.brush_cm / 100.;
            if let Some(draft) = state.draft.as_mut()
                && let Err(e) = avatar_rig::paint_weights(
                    &rig,
                    &rest,
                    &mut draft.painted_weights,
                    &bone,
                    rest.vertices[v],
                    radius,
                    strength,
                )
            {
                self.status = e;
            }
        }
        true
    }

    /// Add a bone from `head` to `tail` (rest positions), hanging from the bone nearest its head.
    fn add_bone(&mut self, rig: &Rig, head: glam::DVec3, tail: glam::DVec3) {
        let ui_lang = self.prefs.language;
        let Some(draft) = self.authoring.pose.draft.as_mut() else {
            return;
        };
        let parent = rig
            .bones
            .iter()
            .min_by(|a, b| {
                avatar_rig::segment_distance_to(head, a.head, a.tail)
                    .total_cmp(&avatar_rig::segment_distance_to(head, b.head, b.tail))
            })
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "pelvis".into());
        let name = (1..)
            .map(|k| format!("bone_{k}"))
            .find(|n| rig.bone(n).is_none() && !draft.added_bones.iter().any(|b| &b.name == n))
            .unwrap();
        let mut next = draft.clone();
        next.added_bones.push(avatar_rig::AddedBone {
            name: name.clone(),
            parent: parent.clone(),
            head: head.to_array(),
            tail: tail.to_array(),
        });
        // The bone must make a valid skeleton.
        let empty = fashioncraft::avatar::SurfaceMesh {
            vertices: vec![],
            triangles: vec![],
        };
        match next.rig(&empty) {
            Ok(_) => {
                *draft = next;
                self.authoring.pose.bones = None;
                self.authoring.pose.bone =
                    avatar_rig::bone_names().count() + draft.added_bones.len() - 1;
                self.status = format!(
                    "{name} ({parent}) · {}",
                    i18n::tr(ui_lang, "Apply and restart uses the new bones and weights.")
                );
            }
            Err(e) => self.status = e,
        }
    }

    /// The selected bone's skin weights as coloured dots while painting.
    pub(super) fn paint_skin_weights(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        let state = &self.authoring.pose;
        if !state.open || !state.painting {
            return;
        }
        let Some(bone) = self.selected_bone() else {
            return;
        };
        let Some((rest, rig)) = self.skin() else {
            return;
        };
        let Some(index) = rig.bone(&bone) else {
            return;
        };
        let Some(draft) = self.authoring.pose.draft.as_ref() else {
            return;
        };
        let body = &self.scene.collider.surface;
        let n = rest.vertices.len().min(body.vertices.len());
        let mut normals = vec![glam::DVec3::ZERO; n];
        for t in &body.triangles {
            if t.iter().any(|v| *v >= n) {
                continue;
            }
            let [a, b, c] = t.map(|v| body.vertices[v]);
            let face = (b - a).cross(c - a);
            for v in t {
                normals[*v] += face;
            }
        }
        let forward = self.camera.forward().as_dvec3();
        let painter = ui.painter().with_clip_rect(rect);
        for (v, normal) in normals.iter().enumerate() {
            if normal.dot(forward) >= 0. {
                continue;
            }
            let weight = match draft.painted_weights.get(&v) {
                Some(list) => list
                    .iter()
                    .find(|(b, _)| *b == bone)
                    .map_or(0., |(_, w)| *w),
                None => rig.weights[v]
                    .iter()
                    .find(|(b, _)| *b == index)
                    .map_or(0., |(_, w)| *w),
            };
            if weight < 0.02 {
                continue;
            }
            if let Some(at) = self.camera.project(body.vertices[v].as_vec3(), rect) {
                let w = weight.clamp(0., 1.) as f32;
                let colour = Color32::from_rgb(
                    (40. + 215. * w) as u8,
                    (90. * (1. - w)) as u8,
                    (255. * (1. - w)) as u8,
                );
                painter.circle_filled(at, 1.6, colour);
            }
        }
    }
}
