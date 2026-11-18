//! Camera conveniences familiar from other garment tools: number keys for the standard views,
//! F to frame, an adjustable field of view, named views saved with the project, and inverting
//! the piece selection. Presentation only, apart from the views stored in the document.
use super::*;
use marvelous_cloth::{
    fitting::SavedView,
    render::{Camera, FOV_RANGE_DEGREES, PresetView},
};

/// Number keys (top row or keypad), the preset each one shows and what that shows of the
/// avatar. The keys name the avatar's sides, as garment tools do: 4 shows the avatar's right
/// side, which is the view-bar preset "Left" (its camera sits on the viewer's left).
pub(super) const NUMBER_VIEWS: [(egui::Key, PresetView, &str); 8] = [
    (egui::Key::Num2, PresetView::Front, "Front"),
    (egui::Key::Num8, PresetView::Back, "Back"),
    (egui::Key::Num4, PresetView::Left, "Avatar's right side"),
    (egui::Key::Num6, PresetView::Right, "Avatar's left side"),
    (egui::Key::Num5, PresetView::Top, "Top"),
    (egui::Key::Num0, PresetView::Bottom, "Bottom"),
    (
        egui::Key::Num1,
        PresetView::ThreeQuarterLeft,
        "Three-quarter, avatar's right",
    ),
    (
        egui::Key::Num3,
        PresetView::ThreeQuarterRight,
        "Three-quarter, avatar's left",
    ),
];

/// The current camera as a named view.
pub(super) fn view_from_camera(name: &str, camera: &Camera) -> SavedView {
    SavedView {
        name: name.trim().to_string(),
        yaw_degrees: f64::from(camera.yaw.to_degrees()),
        pitch_degrees: f64::from(camera.pitch.to_degrees()).clamp(-90., 90.),
        distance_m: f64::from(camera.distance),
        target_m: camera.target.as_dvec3().to_array(),
        orthographic: camera.orthographic,
        fov_degrees: f64::from(camera.fov_radians().to_degrees()),
    }
}

/// Move the camera to a saved view. The previous pose stays available to Restore.
pub(super) fn apply_view(view: &SavedView, camera: &mut Camera) {
    camera.save_pose();
    camera.yaw = (view.yaw_degrees as f32).to_radians();
    camera.pitch = (view.pitch_degrees as f32).to_radians();
    camera.distance = view.distance_m as f32;
    camera.target = glam::DVec3::from_array(view.target_m).as_vec3();
    camera.orthographic = view.orthographic;
    camera.fov_degrees = view.fov_degrees as f32;
}

/// "View 1", "View 2", … : the first name not taken (names compare case-insensitively).
pub(super) fn next_view_name(views: &[SavedView]) -> String {
    (1..)
        .map(|n| format!("View {n}"))
        .find(|name| {
            !views
                .iter()
                .any(|v| v.name.trim().eq_ignore_ascii_case(name))
        })
        .expect("an unused name exists")
}

/// Pieces selected after inverting: every piece not selected now, except those locked in 2D.
pub(super) fn inverted(
    all: &[String],
    selected: &BTreeSet<String>,
    locked: &BTreeSet<String>,
) -> BTreeSet<String> {
    all.iter()
        .filter(|id| !selected.contains(*id) && !locked.contains(*id))
        .cloned()
        .collect()
}

use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct CameraViewsUi {
    /// Name typed for the next saved view.
    draft_name: String,
}

impl Atelier {
    /// Number keys, F and Shift+Cmd+I, unless a text field has the keyboard or a menu is open.
    pub(super) fn camera_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (view, frame, invert) = ctx.input(|i| {
            let plain = !i.modifiers.any();
            (
                NUMBER_VIEWS
                    .iter()
                    .find(|(key, _, _)| plain && i.key_pressed(*key))
                    .map(|(_, view, label)| (*view, *label)),
                plain && i.key_pressed(egui::Key::F),
                i.modifiers.command
                    && i.modifiers.shift
                    && !i.modifiers.alt
                    && i.key_pressed(egui::Key::I),
            )
        });
        if let Some((view, label)) = view {
            self.camera.apply_preset(view);
            self.status = format!("{label} view");
        }
        // In the Sew tool F belongs to the sewing controls.
        if frame && self.authoring.tool != Tool::Sew {
            self.frame_selection_or_all();
        }
        if invert {
            self.invert_selection();
        }
    }

    /// Frame the selected pieces, or everything when nothing is selected.
    pub(super) fn frame_selection_or_all(&mut self) {
        let bounds = self.view_bounds();
        if let Some((lo, hi)) = bounds.selection.or(bounds.all) {
            self.camera
                .request_frame((lo + hi) * 0.5, (hi - lo).length() * 0.5);
        }
    }

    /// Select every piece that is not selected now, leaving out pieces locked in 2D.
    pub(super) fn invert_selection(&mut self) {
        let all: Vec<String> = self
            .project
            .template
            .panels
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let current: BTreeSet<String> = self.selected_panels().into_iter().collect();
        let next = inverted(&all, &current, &self.project.template.locked_panels);
        self.cancel_preview();
        self.authoring.selected = next.iter().next().cloned();
        self.authoring.selection.panels = next;
        self.authoring.board.selection.clear();
        self.authoring.board.selection_panel = None;
        self.authoring.board.point_groups.clear();
        self.authoring.anchor = None;
        self.authoring.edge = None;
        self.status = format!(
            "Selection inverted: {} selected",
            super::garment_display::pieces(self.selected_panels().len())
        );
    }

    /// Select every piece, leaving out pieces locked in 2D.
    pub(super) fn select_all_pieces(&mut self) {
        let locked = &self.project.template.locked_panels;
        let next: BTreeSet<String> = self
            .project
            .template
            .panels
            .iter()
            .filter(|p| !locked.contains(&p.id))
            .map(|p| p.id.clone())
            .collect();
        self.cancel_preview();
        self.authoring.selected = next.iter().next().cloned();
        self.authoring.selection.panels = next;
        self.authoring.board.selection.clear();
        self.authoring.board.selection_panel = None;
        self.authoring.board.point_groups.clear();
        self.authoring.anchor = None;
        self.authoring.edge = None;
        self.status = format!(
            "{} selected",
            super::garment_display::pieces(self.selected_panels().len())
        );
    }

    /// Field of view and saved views, under the 3D view's camera menu.
    pub(super) fn camera_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Field of view");
            let mut fov = self.camera.fov_radians().to_degrees();
            if ui
                .add(egui::Slider::new(&mut fov, FOV_RANGE_DEGREES).suffix("°"))
                .on_hover_text("Vertical field of view; also sets the orthographic view's size")
                .changed()
            {
                self.camera.fov_degrees = fov;
                self.prefs.camera_fov_degrees = fov;
            }
        });
        ui.label(egui::RichText::new("Number keys: 2 front · 8 back · 4/6 avatar's right/left side · 5 top · 0 bottom · 1/3 three-quarter right/left · F frame").small());
        ui.separator();
        ui.strong("Saved views");
        let mut go = None;
        let mut remove = None;
        for (index, view) in self.project.saved_views.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .button(&view.name)
                    .on_hover_text("Go to this view; Restore comes back")
                    .clicked()
                {
                    go = Some(index);
                }
                if ui
                    .small_button("×")
                    .on_hover_text("Delete this view")
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        }
        if self.project.saved_views.is_empty() {
            ui.small("No saved views yet. They are stored with the project.");
        }
        if let Some(index) = go {
            apply_view(&self.project.saved_views[index], &mut self.camera);
            // The view brings its own field of view; keep the preference in step with it.
            self.prefs.camera_fov_degrees = self.camera.fov_degrees;
            self.status = format!("View “{}”", self.project.saved_views[index].name);
        }
        if let Some(index) = remove {
            let removed = self.project.saved_views.remove(index);
            self.document.dirty = true;
            self.status = format!("View “{}” deleted", removed.name);
        }
        ui.horizontal(|ui| {
            if self.authoring.camera_views.draft_name.is_empty() {
                self.authoring.camera_views.draft_name = next_view_name(&self.project.saved_views);
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.authoring.camera_views.draft_name)
                    .desired_width(120.)
                    .char_limit(SavedView::MAX_NAME_CHARS),
            );
            if ui
                .add_enabled(
                    self.project.saved_views.len() < SavedView::MAX_VIEWS,
                    egui::Button::new("Save current view"),
                )
                .clicked()
            {
                let name = self.authoring.camera_views.draft_name.trim().to_string();
                let view = view_from_camera(&name, &self.camera);
                if let Err(error) = view.validate() {
                    self.status = error;
                } else if let Some(existing) = self
                    .project
                    .saved_views
                    .iter_mut()
                    .find(|v| v.name.trim().eq_ignore_ascii_case(&name))
                {
                    *existing = view;
                    self.document.dirty = true;
                    self.status = format!("View “{name}” updated");
                } else {
                    self.project.saved_views.push(view);
                    self.document.dirty = true;
                    self.status = format!("View “{name}” saved with the project");
                    self.authoring.camera_views.draft_name.clear();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn number_keys_follow_the_keypad_layout() {
        let view = |key| NUMBER_VIEWS.iter().find(|(k, _, _)| *k == key).unwrap().1;
        assert_eq!(view(egui::Key::Num2), PresetView::Front);
        assert_eq!(view(egui::Key::Num8), PresetView::Back);
        assert_eq!(view(egui::Key::Num5), PresetView::Top);
        assert_eq!(view(egui::Key::Num0), PresetView::Bottom);
        // 4 shows the avatar's right side: the avatar faces +z, so its right is −x and the
        // camera sits there (the viewer-named "Left" preset).
        assert!(view(egui::Key::Num4).direction().x < -0.99);
        assert!(view(egui::Key::Num6).direction().x > 0.99);
        assert!(
            view(egui::Key::Num1).direction().x < 0. && view(egui::Key::Num1).direction().z > 0.
        );
        assert!(
            view(egui::Key::Num3).direction().x > 0. && view(egui::Key::Num3).direction().z > 0.
        );
        let keys: BTreeSet<_> = NUMBER_VIEWS
            .iter()
            .map(|(k, _, _)| format!("{k:?}"))
            .collect();
        assert_eq!(keys.len(), 8, "each key shows one view");
    }

    #[test]
    fn saved_views_round_trip_the_camera() {
        let mut camera = Camera::new(0.7, -0.3, 3.2, Vec3::new(0.1, 1.2, -0.05));
        camera.orthographic = true;
        camera.fov_degrees = 30.;
        let view = view_from_camera("  Side detail ", &camera);
        assert_eq!(view.name, "Side detail");
        assert!(view.validate().is_ok());
        let mut other = Camera::default();
        apply_view(&view, &mut other);
        assert!((other.yaw - 0.7).abs() < 1e-5 && (other.pitch + 0.3).abs() < 1e-5);
        assert!((other.distance - 3.2).abs() < 1e-5);
        assert!(other.target.distance(Vec3::new(0.1, 1.2, -0.05)) < 1e-5);
        assert!(other.orthographic);
        assert!((other.fov_degrees - 30.).abs() < 1e-4);
        assert!(
            other.restore_pose(),
            "the previous pose is kept for Restore"
        );
        assert!((other.fov_degrees - Camera::default().fov_degrees).abs() < 1e-6);
    }

    #[test]
    fn view_names_and_inversion() {
        let named = |name: &str| view_from_camera(name, &Camera::default());
        assert_eq!(next_view_name(&[]), "View 1");
        assert_eq!(
            next_view_name(&[named("view 1"), named("View 3")]),
            "View 2"
        );
        let all: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let selected: BTreeSet<String> = ["a".to_string()].into();
        let locked: BTreeSet<String> = ["c".to_string()].into();
        let next = inverted(&all, &selected, &locked);
        assert_eq!(next, ["b".to_string(), "d".to_string()].into());
        assert_eq!(inverted(&all, &next, &locked), ["a".to_string()].into());
    }
}
