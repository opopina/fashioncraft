//! Create avatar (MakeHuman): a new body from MakeHuman's macro settings (gender, age, muscle,
//! weight, height, proportions and ancestry), measured as the sliders move. The body is written
//! next to the session's other files with its joints and anchors, then goes through the usual
//! avatar import confirmation, as the avatar editor's does.
use super::*;
use fashioncraft::{
    avatar_maker::{MacroBody, MacroTargets},
    avatar_shape::{self, BodyMeasurements},
};

#[derive(Default)]
pub(super) struct AvatarMakerUi {
    pub open: bool,
    body: MacroBody,
    /// The settings last measured, and what they measured.
    measured: Option<(MacroBody, Result<BodyMeasurements, String>)>,
    error: Option<String>,
}

fn measure(body: &MacroBody) -> Result<BodyMeasurements, String> {
    let made = MacroTargets::bundled()?.make(body)?;
    avatar_shape::measure(&made.surface, &made.joints)
}

impl Atelier {
    pub(super) fn avatar_maker_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.avatar_maker.open {
            return;
        }
        let mut open = true;
        let mut make = false;
        let state = &mut self.authoring.avatar_maker;
        if state.measured.as_ref().map(|(b, _)| b) != Some(&state.body) {
            state.measured = Some((state.body, measure(&state.body)));
        }
        egui::Window::new(i18n::tr(ui_lang, "Create avatar (MakeHuman)"))
            .open(&mut open)
            .resizable(false)
            .min_width(380.)
            .show(ctx, |ui| {
                let body = &mut state.body;
                ui.small(i18n::tr(
                    ui_lang,
                    "A body from MakeHuman's CC0 shapes. 50 % is average; the measurements follow the sliders.",
                ));
                let percent = |v: f64, _: std::ops::RangeInclusive<usize>| format!("{:.0} %", v * 100.);
                egui::Grid::new("avatar.maker")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label(i18n::tr(ui_lang, "Gender"));
                        ui.horizontal(|ui| {
                            ui.small(i18n::tr(ui_lang, "Female"));
                            ui.add(egui::Slider::new(&mut body.gender, 0.0..=1.0).show_value(false));
                            ui.small(i18n::tr(ui_lang, "Male"));
                        });
                        ui.end_row();
                        ui.label(i18n::tr(ui_lang, "Age"));
                        ui.add(
                            egui::Slider::new(&mut body.age_years, 1.0..=90.0)
                                .step_by(1.)
                                .suffix(i18n::tr(ui_lang, " years")),
                        );
                        ui.end_row();
                        for (name, value) in [
                            ("Muscle", &mut body.muscle),
                            ("Weight", &mut body.weight),
                            ("Height", &mut body.height),
                            ("Proportions", &mut body.proportions),
                        ] {
                            ui.label(i18n::tr(ui_lang, name));
                            ui.add(egui::Slider::new(value, 0.0..=1.0).custom_formatter(percent));
                            ui.end_row();
                        }
                    });
                ui.add_space(4.);
                ui.label(i18n::tr(ui_lang, "Ancestry"));
                egui::Grid::new("avatar.maker.ancestry")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (name, value) in [
                            ("African", &mut body.african),
                            ("Asian", &mut body.asian),
                            ("European", &mut body.european),
                        ] {
                            ui.label(i18n::tr(ui_lang, name));
                            ui.add(egui::Slider::new(value, 0.0..=1.0).custom_formatter(percent));
                            ui.end_row();
                        }
                    });
                ui.separator();
                match &state.measured {
                    Some((_, Ok(m))) => {
                        ui.label(format!(
                            "{} {:.0} cm · {} {:.0} · {} {:.0} · {} {:.0} cm",
                            i18n::tr(ui_lang, "Height"),
                            m.height_m * 100.,
                            i18n::tr(ui_lang, "Bust"),
                            m.bust_m * 100.,
                            i18n::tr(ui_lang, "Waist"),
                            m.waist_m * 100.,
                            i18n::tr(ui_lang, "Hip"),
                            m.hip_m * 100.,
                        ));
                    }
                    Some((_, Err(e))) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
                    }
                    None => {}
                }
                ui.horizontal(|ui| {
                    if ui.button(i18n::tr(ui_lang, "Reset")).clicked() {
                        *body = MacroBody::default();
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Make avatar…"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Make this body, then confirm it like an imported avatar",
                        ))
                        .clicked()
                    {
                        make = true;
                    }
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
            });
        self.authoring.avatar_maker.open &= open;
        if make && let Err(e) = self.make_macro_avatar() {
            self.authoring.avatar_maker.error = Some(e);
        }
    }

    /// Make the body, write it with its joints and anchors, and offer it for import.
    fn make_macro_avatar(&mut self) -> Result<(), String> {
        let b = self.authoring.avatar_maker.body;
        let made = MacroTargets::bundled()?.make(&b)?;
        let percent = |v: f64| (v * 100.).round();
        let dir = self.output.join(format!(
            "avatar-makehuman-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}",
            percent(b.gender),
            b.age_years,
            percent(b.muscle),
            percent(b.weight),
            percent(b.height),
            percent(b.proportions),
            percent(b.african),
            percent(b.asian),
            percent(b.european),
        ));
        let obj = super::avatar_shape_ui::write_avatar(&dir, &made.surface, &made.joints)?;
        self.authoring.avatar_maker.error = None;
        self.offer_avatar(&obj);
        self.status = i18n::tr(
            self.prefs.language,
            "Avatar made from MakeHuman's shapes · confirm to replace the current one",
        )
        .into();
        Ok(())
    }
}
