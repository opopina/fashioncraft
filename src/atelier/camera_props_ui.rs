//! Camera properties window (Settings › Camera properties…): the 3D view's projection, field
//! of view, orbit and target as numbers.
use super::*;
use fashioncraft::render::{DISTANCE_RANGE, FOV_RANGE_DEGREES};

#[derive(Default)]
pub(super) struct CameraPropsUi {
    pub open: bool,
}

impl Atelier {
    pub(super) fn camera_props_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.camera_props.open {
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        let mut open = true;
        egui::Window::new(t("Camera properties"))
            .open(&mut open)
            .default_width(280.)
            .show(ctx, |ui| {
                egui::Grid::new("camera_properties")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label(t("Projection"));
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut self.camera.orthographic,
                                false,
                                t("Perspective"),
                            );
                            ui.selectable_value(
                                &mut self.camera.orthographic,
                                true,
                                t("Orthographic"),
                            );
                        });
                        ui.end_row();
                        ui.label(t("Field of view"));
                        let mut fov = self.camera.fov_radians().to_degrees();
                        if super::workspace::slim(
                            ui,
                            egui::Slider::new(&mut fov, FOV_RANGE_DEGREES).suffix("°"),
                        )
                        .changed()
                        {
                            self.camera.fov_degrees = fov;
                            self.prefs.camera_fov_degrees = fov;
                        }
                        ui.end_row();
                        ui.label(t("Distance"));
                        ui.add(
                            egui::DragValue::new(&mut self.camera.distance)
                                .range(DISTANCE_RANGE)
                                .speed(0.01)
                                .suffix(" m"),
                        );
                        ui.end_row();
                        ui.label(t("Turn"));
                        let mut yaw = self.camera.yaw.to_degrees();
                        if ui
                            .add(egui::DragValue::new(&mut yaw).speed(0.5).suffix("°"))
                            .changed()
                        {
                            self.camera.yaw = yaw.rem_euclid(360.).to_radians();
                        }
                        ui.end_row();
                        ui.label(t("Tilt"));
                        let mut pitch = self.camera.pitch.to_degrees();
                        if ui
                            .add(
                                egui::DragValue::new(&mut pitch)
                                    .range(-90.0..=90.0)
                                    .speed(0.5)
                                    .suffix("°"),
                            )
                            .changed()
                        {
                            self.camera.pitch = pitch.to_radians();
                        }
                        ui.end_row();
                        ui.label(t("Target"));
                        ui.horizontal(|ui| {
                            let target = &mut self.camera.target;
                            for (axis, value) in [
                                ("X", &mut target.x),
                                ("Y", &mut target.y),
                                ("Z", &mut target.z),
                            ] {
                                ui.label(axis);
                                ui.add(egui::DragValue::new(value).speed(0.005).suffix(" m"));
                            }
                        });
                        ui.end_row();
                    });
            });
        if !open {
            self.authoring.camera_props.open = false;
        }
    }
}
