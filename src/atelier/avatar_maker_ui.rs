//! Create avatar (MakeHuman · Anny): a new body from MakeHuman's shapes with Anny's settings
//! (gender, age in years, muscle, weight, height, proportions, bust and ancestry) and local
//! measurements, measured as the sliders move. "Typical" and "Random realistic" follow Anny's
//! WHO-calibrated distributions for the age and gender. Its look (skin, eyes, eyebrows,
//! eyelashes and hair, MakeHuman's CC0 assets) is fitted to it. The body is written next to the
//! session's other files with its joints, anchors and look, then goes through the usual avatar
//! import confirmation, as the avatar editor's does.
use super::*;
use fashioncraft::{
    avatar_look::{self, LookChoice},
    avatar_maker::{MEASURES, MacroBody, MacroTargets},
    avatar_shape::{self, BodyMeasurements},
};

pub(super) struct AvatarMakerUi {
    pub open: bool,
    body: MacroBody,
    look: LookChoice,
    /// The settings last measured, and what they measured.
    measured: Option<(MacroBody, Result<BodyMeasurements, String>)>,
    error: Option<String>,
}

impl Default for AvatarMakerUi {
    fn default() -> Self {
        Self {
            open: false,
            body: MacroBody::default(),
            look: LookChoice::of(
                "young_caucasian_female",
                "brown",
                "eyebrow001",
                "eyelashes01",
                Some("short02"),
            ),
            measured: None,
            error: None,
        }
    }
}

impl AvatarMakerUi {
    /// The settings and look the maker holds now.
    pub(super) fn current(&self) -> (MacroBody, LookChoice) {
        (self.body, self.look.clone())
    }

    /// Start from `body` and `look` (a preset of the library).
    pub(super) fn set(&mut self, body: MacroBody, look: LookChoice) {
        self.body = body;
        self.look = look;
        self.error = None;
    }
}

/// A choice among `options` (name, English label), or none.
fn pick(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<&'static str>,
    options: &[(&'static str, &'static str)],
    none: Option<&str>,
    language: i18n::Language,
) {
    let label = |v: Option<&'static str>| match v {
        Some(name) => options
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(name, |(_, l)| i18n::tr(language, l)),
        None => none.unwrap_or("—"),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*value))
        .show_ui(ui, |ui| {
            if none.is_some() {
                ui.selectable_value(value, None, label(None));
            }
            for (name, _) in options {
                ui.selectable_value(value, Some(*name), label(Some(*name)));
            }
        });
}

fn measure(body: &MacroBody) -> Result<BodyMeasurements, String> {
    let made = MacroTargets::bundled()?.make(body)?;
    avatar_shape::measure(&made.surface, &made.joints)
}

/// A slider between two words, the value not shown.
fn between(ui: &mut egui::Ui, value: &mut f64, low: &str, high: &str) {
    ui.horizontal(|ui| {
        ui.small(low);
        super::workspace::slim(ui, egui::Slider::new(value, 0.0..=1.0).show_value(false));
        ui.small(high);
    });
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
        let tr = |text: &'static str| i18n::tr(ui_lang, text);
        egui::Window::new(tr("Create avatar (MakeHuman)"))
            .open(&mut open)
            .resizable(false)
            .min_width(400.)
            .show(ctx, |ui| {
                let body = &mut state.body;
                ui.small(tr(
                    "A body from MakeHuman's CC0 shapes, with Anny's settings. The measurements follow the sliders.",
                ));
                let percent = |v: f64, _: std::ops::RangeInclusive<usize>| format!("{:.0} %", v * 100.);
                egui::Grid::new("avatar.maker")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label(tr("Gender"));
                        between(ui, &mut body.gender, tr("Female"), tr("Male"));
                        ui.end_row();
                        ui.label(tr("Age"));
                        super::workspace::slim(ui, egui::Slider::new(&mut body.age_years, 1.0..=90.0)
                                .step_by(1.)
                                .suffix(tr(" years")),
                        );
                        ui.end_row();
                        for (name, value) in [
                            ("Muscle", &mut body.muscle),
                            ("Weight", &mut body.weight),
                            ("Height", &mut body.height),
                        ] {
                            ui.label(tr(name));
                            super::workspace::slim(ui, egui::Slider::new(value, 0.0..=1.0).custom_formatter(percent));
                            ui.end_row();
                        }
                        ui.label(tr("Proportions"));
                        between(ui, &mut body.proportions, tr("Ideal"), tr("Uncommon"));
                        ui.end_row();
                        ui.label(tr("Bust size"));
                        super::workspace::slim(ui, egui::Slider::new(&mut body.cupsize, 0.0..=1.0).custom_formatter(percent));
                        ui.end_row();
                        ui.label(tr("Bust firmness"));
                        super::workspace::slim(ui, egui::Slider::new(&mut body.firmness, 0.0..=1.0).custom_formatter(percent));
                        ui.end_row();
                    });
                ui.add_space(4.);
                ui.label(tr("Ancestry"));
                egui::Grid::new("avatar.maker.ancestry")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (name, value) in [
                            ("African", &mut body.african),
                            ("Asian", &mut body.asian),
                            ("European", &mut body.european),
                        ] {
                            ui.label(tr(name));
                            super::workspace::slim(ui, egui::Slider::new(value, 0.0..=1.0).custom_formatter(percent));
                            ui.end_row();
                        }
                    });
                ui.add_space(4.);
                ui.label(tr("Look"));
                let look = &mut state.look;
                egui::Grid::new("avatar.maker.look")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let none = Some(tr("None"));
                        ui.label(tr("Skin"));
                        pick(ui, "avatar.maker.skin", &mut look.skin, &avatar_look::SKINS, Some(tr("Plain colour")), ui_lang);
                        ui.end_row();
                        ui.label(tr("Eyes"));
                        pick(ui, "avatar.maker.eyes", &mut look.eyes, &avatar_look::EYE_COLOURS, none, ui_lang);
                        ui.end_row();
                        ui.label(tr("Eyebrows"));
                        pick(ui, "avatar.maker.eyebrows", &mut look.eyebrows, &avatar_look::EYEBROWS, none, ui_lang);
                        ui.end_row();
                        ui.label(tr("Eyelashes"));
                        pick(ui, "avatar.maker.eyelashes", &mut look.eyelashes, &avatar_look::EYELASHES, none, ui_lang);
                        ui.end_row();
                        ui.label(tr("Hair"));
                        pick(ui, "avatar.maker.hair", &mut look.hair, &avatar_look::HAIR, none, ui_lang);
                        ui.end_row();
                        ui.label(tr("Hair colour"));
                        let colours: Vec<(&'static str, &'static str)> =
                            avatar_look::HAIR_COLOURS.iter().map(|(n, l, _)| (*n, *l)).collect();
                        pick(ui, "avatar.maker.hair.colour", &mut look.hair_colour, &colours, Some(tr("Natural")), ui_lang);
                        ui.end_row();
                        // Children always wear it; adults may go without.
                        let adult = body.age_years >= avatar_look::ADULT_AGE;
                        let mut on = !look.without_underwear || !adult;
                        ui.label(tr("Underwear"));
                        let response = ui
                            .add_enabled(adult && look.skin.is_some(), egui::Checkbox::without_text(&mut on))
                            .on_hover_text(tr("Painted on the skin; it does not change the simulation"))
                            .on_disabled_hover_text(tr("Always on for avatars under 18 and with a skin"));
                        if response.changed() {
                            look.without_underwear = !on;
                        }
                        ui.end_row();
                    });
                egui::CollapsingHeader::new(tr("Measurements"))
                    .id_salt("avatar.maker.measures")
                    .show(ui, |ui| {
                        ui.small(tr("Each from −100 % smaller to +100 % larger than the settings make it."));
                        egui::Grid::new("avatar.maker.measures.grid")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (k, (_, label)) in MEASURES.iter().enumerate() {
                                    ui.label(tr(label));
                                    super::workspace::slim(ui, egui::Slider::new(&mut body.measures[k], -1.0..=1.0)
                                            .custom_formatter(|v, _| format!("{:+.0} %", v * 100.)),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                match &state.measured {
                    Some((_, Ok(m))) => {
                        ui.label(format!(
                            "{} {:.0} cm · {} {:.0} · {} {:.0} · {} {:.0} cm",
                            tr("Height"),
                            m.height_m * 100.,
                            tr("Bust"),
                            m.bust_m * 100.,
                            tr("Waist"),
                            m.waist_m * 100.,
                            tr("Hip"),
                            m.hip_m * 100.,
                        ));
                    }
                    Some((_, Err(e))) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
                    }
                    None => {}
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(tr("Typical"))
                        .on_hover_text(tr("The typical build for this age and gender"))
                        .clicked()
                    {
                        let typical = MacroBody::typical(body.gender, body.age_years);
                        *body = MacroBody {
                            african: body.african,
                            asian: body.asian,
                            european: body.european,
                            ..typical
                        };
                    }
                    if ui
                        .button(tr("Random realistic"))
                        .on_hover_text(tr(
                            "A build drawn as a real person of this age and gender might have",
                        ))
                        .clicked()
                    {
                        let seed = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_nanos() as u64);
                        match body.realistic(seed) {
                            Ok(drawn) => *body = drawn,
                            Err(e) => state.error = Some(e),
                        }
                    }
                    if ui.button(tr("Reset")).clicked() {
                        *body = MacroBody::default();
                    }
                    if ui
                        .button(tr("Make avatar…"))
                        .on_hover_text(tr("Make this body, then confirm it like an imported avatar"))
                        .clicked()
                    {
                        make = true;
                    }
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                ui.small(tr(
                    "Shapes, skins, eyes and hair: MakeHuman (CC0). Settings and calibration by age: Anny, NAVER LABS Europe (Apache 2.0).",
                ));
            });
        self.authoring.avatar_maker.open &= open;
        if make && let Err(e) = self.make_macro_avatar() {
            self.authoring.avatar_maker.error = Some(e);
        }
    }

    /// Make the body, write it with its joints, anchors and look, and offer it for import.
    fn make_macro_avatar(&mut self) -> Result<(), String> {
        let state = &self.authoring.avatar_maker;
        let obj =
            super::avatar_library_ui::write_made_avatar(&self.output, &state.body, &state.look)?;
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
