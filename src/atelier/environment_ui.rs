//! Scene environment and quality presets in the Simulation tab: a physical floor, gravity,
//! wind, the collision distance, and time-step / iteration presets like a CAD cloth tool's
//! Normal / Animation / Fitting.
use super::*;
use fashioncraft::vbd::Wind;
use glam::DVec3;

/// Friction of new documents: a typical value for cotton on skin and on cotton.
pub(super) const DEFAULT_FRICTION: f64 = 0.3;

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
        let ui_lang = self.prefs.language;
        ui.horizontal_wrapped(|ui| {
            ui.label(i18n::tr(ui_lang, "Quality"));
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
        ui.horizontal_wrapped(|ui| {
            ui.label(i18n::tr(ui_lang, "Friction"));
            ui.add(
                egui::DragValue::new(&mut self.settings.friction)
                    .range(0.0..=1.5)
                    .speed(0.01)
                    .fixed_decimals(2),
            )
            .on_hover_text(i18n::tr(
                ui_lang,
                "Coulomb friction coefficient of every contact: cloth on the avatar, on itself and \
                 on the floor. 0 lets cloth slide freely",
            ));
            for (label, value) in [("None", 0.0), ("Cotton", DEFAULT_FRICTION), ("Grippy", 0.6)] {
                if ui.small_button(label).clicked() {
                    self.settings.friction = value;
                }
            }
        });
        let mut gap = self.settings.contact.radius * 1000.;
        ui.horizontal(|ui| {
            ui.label(i18n::tr(ui_lang, "Collision distance"));
            if ui
                .add(
                    egui::DragValue::new(&mut gap)
                        .range(1.0..=10.0)
                        .speed(0.05)
                        .fixed_decimals(1)
                        .suffix(" mm"),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Gap the solver keeps between cloth and the avatar, the floor and other \
                     cloth. Larger is more forgiving with thick layers; smaller fits closer. A \
                     fabric can set its own in the Fabric tab; two sides keep the mean",
                ))
                .changed()
            {
                self.settings.contact.radius = gap / 1000.;
            }
        });
        let mut fast = self.settings.max_substeps > 1;
        if ui
            .checkbox(&mut fast, i18n::tr(ui_lang, "Full-speed falls"))
            .on_hover_text(i18n::tr(
                ui_lang,
                "Split a step into up to 16 shorter ones when the cloth moves faster than one step's \
                 contacts allow (about 0.3 m/s), so falling cloth keeps up with gravity. Slower while \
                 cloth moves fast; settled cloth is unaffected",
            ))
            .changed()
        {
            self.settings.max_substeps = if fast { 16 } else { 1 };
        }
        ui.separator();
        ui.strong(i18n::tr(ui_lang, "Environment"));
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
            .checkbox(&mut floor, i18n::tr(ui_lang, "Floor"))
            .on_hover_text(i18n::tr(
                ui_lang,
                "A physical floor the cloth can rest on; starts under the avatar's feet",
            ))
            .changed()
        {
            self.settings.floor = floor.then_some(feet);
        }
        if let Some(height) = self.settings.floor.as_mut() {
            let mut mm = *height * 1000.;
            ui.horizontal(|ui| {
                ui.label(i18n::tr(ui_lang, "Floor height"));
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
                if ui.small_button(i18n::tr(ui_lang, "At the feet")).clicked() {
                    *height = feet;
                }
            });
        }
        let mut g = -self.settings.gravity.y;
        ui.horizontal(|ui| {
            ui.label(i18n::tr(ui_lang, "Gravity"));
            if ui
                .add(
                    egui::DragValue::new(&mut g)
                        .range(0.0..=30.0)
                        .speed(0.05)
                        .fixed_decimals(2)
                        .suffix(" m/s²"),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Downward acceleration; 9.81 on Earth, 0 for weightless cloth",
                ))
                .changed()
            {
                self.settings.gravity = DVec3::new(0., -g, 0.);
            }
            if ui.small_button(i18n::tr(ui_lang, "Earth")).clicked() {
                self.settings.gravity = DVec3::new(0., -9.81, 0.);
            }
        });
        self.wind_controls(ui);
    }

    fn wind_controls(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let mut on = self.settings.wind.is_some();
        if ui
            .checkbox(&mut on, i18n::tr(ui_lang, "Wind"))
            .on_hover_text(i18n::tr(
                ui_lang,
                "A steady breeze. Cloth feels pressure drag where it faces the flow, and the same \
                 drag slows it as it falls",
            ))
            .changed()
        {
            self.settings.wind = on.then(Wind::default);
        }
        let Some(wind) = self.settings.wind.as_mut() else {
            return;
        };
        let flat = DVec3::new(wind.velocity.x, 0., wind.velocity.z);
        let mut speed = flat.length();
        // The direction the wind comes from, clockwise seen from above, 0° from +Z.
        let mut from = (-flat.x).atan2(-flat.z).to_degrees().rem_euclid(360.);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(i18n::tr(ui_lang, "Wind speed"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut speed)
                        .range(0.0..=20.0)
                        .speed(0.05)
                        .fixed_decimals(1)
                        .suffix(" m/s"),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "2 m/s is a light breeze, 8 m/s a strong wind",
                ))
                .changed();
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(i18n::tr(ui_lang, "From"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut from)
                        .range(0.0..=359.0)
                        .speed(1.0)
                        .fixed_decimals(0)
                        .suffix("°"),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Where the wind comes from: 0° is +Z, 90° is +X, seen from above",
                ))
                .changed();
            for (label, angle) in [("+Z", 0.), ("+X", 90.), ("−Z", 180.), ("−X", 270.)] {
                if ui.small_button(label).clicked() {
                    from = angle;
                    changed = true;
                }
            }
        });
        if changed {
            let a = f64::to_radians(from);
            wind.velocity = DVec3::new(-a.sin() * speed, wind.velocity.y, -a.cos() * speed);
        }
    }
}
