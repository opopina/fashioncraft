//! Scene environment and quality presets in the Simulation tab: a physical floor, gravity,
//! and time-step / iteration presets like a CAD cloth tool's Normal / Animation / Fitting.
use super::*;
use glam::DVec3;

/// Time step (s) and solver iterations of each quality preset.
pub(super) const PRESETS: [(&str, f64, usize, &str); 3] = [
    (
        "Fast",
        1. / 60.,
        8,
        "Quick previews; may need a final Accurate pass",
    ),
    (
        "Normal",
        1. / 60.,
        16,
        "The default balance of speed and accuracy",
    ),
    (
        "Accurate",
        1. / 120.,
        32,
        "Smaller steps and more iterations: slower, for fitting",
    ),
];

impl Atelier {
    pub(super) fn simulation_environment(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Quality");
            for (name, dt, iterations, hint) in PRESETS {
                let current =
                    (self.settings.dt - dt).abs() < 1e-12 && self.settings.iterations == iterations;
                if ui
                    .selectable_label(current, name)
                    .on_hover_text(format!(
                        "{hint} · step {:.1} ms · {iterations} iterations",
                        dt * 1000.
                    ))
                    .clicked()
                {
                    self.settings.dt = dt;
                    self.settings.iterations = iterations;
                }
            }
        });
        ui.small(format!(
            "Step {:.1} ms · {} iterations",
            self.settings.dt * 1000.,
            self.settings.iterations
        ));
        ui.separator();
        ui.strong("Environment");
        let feet = self
            .scene
            .collider
            .surface
            .vertices
            .iter()
            .map(|v| v.y)
            .fold(f64::INFINITY, f64::min);
        let feet = if feet.is_finite() { feet } else { 0. };
        let mut floor = self.settings.floor.is_some();
        if ui
            .checkbox(&mut floor, "Floor")
            .on_hover_text("A physical floor the cloth can rest on; starts under the avatar's feet")
            .changed()
        {
            self.settings.floor = floor.then_some(feet);
        }
        if let Some(height) = self.settings.floor.as_mut() {
            let mut mm = *height * 1000.;
            ui.horizontal(|ui| {
                ui.label("Floor height");
                if ui
                    .add(
                        egui::DragValue::new(&mut mm)
                            .speed(1.0)
                            .fixed_decimals(0)
                            .suffix(" mm"),
                    )
                    .changed()
                {
                    *height = mm / 1000.;
                }
                if ui.small_button("At the feet").clicked() {
                    *height = feet;
                }
            });
        }
        let mut g = -self.settings.gravity.y;
        ui.horizontal(|ui| {
            ui.label("Gravity");
            if ui
                .add(
                    egui::DragValue::new(&mut g)
                        .range(0.0..=30.0)
                        .speed(0.05)
                        .fixed_decimals(2)
                        .suffix(" m/s²"),
                )
                .on_hover_text("Downward acceleration; 9.81 on Earth, 0 for weightless cloth")
                .changed()
            {
                self.settings.gravity = DVec3::new(0., -g, 0.);
            }
            if ui.small_button("Earth").clicked() {
                self.settings.gravity = DVec3::new(0., -9.81, 0.);
            }
        });
    }
}
