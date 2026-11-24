//! Avatar editor: the avatar's height and bust, waist and hip girths, and a new avatar made to
//! other ones. The reshaped body is written next to the session's other files with its joints
//! and anchors, then goes through the usual avatar import confirmation.
use super::*;
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_shape::{self, BodyMeasurements},
    garment::arrangement,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct AvatarShapeUi {
    pub open: bool,
    /// The avatar measured, by its file: its body, joints and measurements.
    measured: Option<(PathBuf, Result<Measured, String>)>,
    /// The measurements asked for, centimetres.
    target: Option<[f64; 6]>,
    error: Option<String>,
}

struct Measured {
    body: SurfaceMesh,
    joints: BTreeMap<String, glam::DVec3>,
    now: BodyMeasurements,
}

fn measure(path: &std::path::Path) -> Result<Measured, String> {
    let body = SurfaceMesh::load_obj(path)?;
    let sidecar = arrangement::load_sidecar(path)?
        .ok_or("This avatar's joints are unknown and cannot be estimated from its shape")?;
    let joints: BTreeMap<String, glam::DVec3> = sidecar
        .landmarks
        .into_iter()
        .map(|l| (l.id, l.position_m))
        .collect();
    let now = avatar_shape::measure(&body, &joints)?;
    Ok(Measured { body, joints, now })
}

/// Write a made body into `dir` as `avatar.obj`, with its joints (`measurements.json`) and the
/// mannequin's anchors (`arrangement.json`), as an avatar import reads them. Returns the OBJ.
pub(super) fn write_avatar(
    dir: &std::path::Path,
    body: &SurfaceMesh,
    joints: &BTreeMap<String, glam::DVec3>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let obj = dir.join("avatar.obj");
    std::fs::write(&obj, fashioncraft::avatar_animation::surface_obj(body))
        .map_err(|e| e.to_string())?;
    let landmarks: BTreeMap<&String, [f64; 3]> =
        joints.iter().map(|(id, p)| (id, p.to_array())).collect();
    std::fs::write(
        dir.join("measurements.json"),
        serde_json::to_string_pretty(&serde_json::json!({ "landmarks": landmarks }))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("arrangement.json"),
        include_str!("../../assets/makehuman/arrangement.json"),
    )
    .map_err(|e| e.to_string())?;
    Ok(obj)
}

fn centimetres(m: &BodyMeasurements) -> [f64; 6] {
    [
        m.height_m, m.bust_m, m.waist_m, m.hip_m, m.arm_m, m.inseam_m,
    ]
    .map(|v| (v * 1000.).round() / 10.)
}

impl Atelier {
    pub(super) fn avatar_shape_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.avatar_shape.open {
            return;
        }
        let path = self.project.avatar_path.clone();
        let state = &mut self.authoring.avatar_shape;
        if state.measured.as_ref().map(|(p, _)| p) != Some(&path) {
            let measured = measure(&path);
            state.target = measured.as_ref().ok().map(|m| centimetres(&m.now));
            state.measured = Some((path, measured));
            state.error = None;
        }
        let mut open = true;
        let mut make = false;
        egui::Window::new(i18n::tr(ui_lang, "Avatar editor"))
            .open(&mut open)
            .resizable(false)
            .default_width(300.)
            .show(ctx, |ui| {
                let Some((_, measured)) = &state.measured else {
                    return;
                };
                let measured = match measured {
                    Ok(m) => m,
                    Err(e) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
                        return;
                    }
                };
                let now = centimetres(&measured.now);
                let target = state.target.get_or_insert(now);
                ui.small(i18n::tr(ui_lang, "Girths are taken around the torso: bust at the chest, the narrowest waist, the widest hip."));
                egui::Grid::new("avatar.measurements")
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.label("");
                        ui.strong(i18n::tr(ui_lang, "Now"));
                        ui.strong(i18n::tr(ui_lang, "New"));
                        ui.end_row();
                        for (k, name) in ["Height", "Bust", "Waist", "Hip", "Arm length", "Inseam"]
                            .iter()
                            .enumerate()
                        {
                            ui.label(i18n::tr(ui_lang, name));
                            ui.label(format!("{:.1} cm", now[k]));
                            ui.add(
                                egui::DragValue::new(&mut target[k])
                                    .range(match k {
                                        0 => 50.0..=250.0,
                                        4 => 15.0..=120.0,
                                        5 => 20.0..=140.0,
                                        _ => 30.0..=220.0,
                                    })
                                    .speed(0.5)
                                    .suffix(" cm"),
                            );
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button(i18n::tr(ui_lang, "Reset")).clicked() {
                        *target = now;
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Make avatar…"))
                        .on_hover_text(i18n::tr(ui_lang, "Reshape the avatar to these measurements, then confirm it like an imported avatar"))
                        .clicked()
                    {
                        make = true;
                    }
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
            });
        self.authoring.avatar_shape.open &= open;
        if make && let Err(e) = self.make_shaped_avatar() {
            self.authoring.avatar_shape.error = Some(e);
        }
    }

    /// Reshape the avatar, write it with its joints and anchors, and offer it for import.
    fn make_shaped_avatar(&mut self) -> Result<(), String> {
        let state = &self.authoring.avatar_shape;
        let Some((_, Ok(measured))) = &state.measured else {
            return Err("The avatar is not measured".into());
        };
        let t = state.target.ok_or("No measurements")?;
        let target = BodyMeasurements {
            height_m: t[0] / 100.,
            bust_m: t[1] / 100.,
            waist_m: t[2] / 100.,
            hip_m: t[3] / 100.,
            arm_m: t[4] / 100.,
            inseam_m: t[5] / 100.,
        };
        let (body, joints) = avatar_shape::reshape(&measured.body, &measured.joints, &target)?;
        let dir = self.output.join(format!(
            "avatar-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}-{:.0}",
            t[0], t[1], t[2], t[3], t[4], t[5]
        ));
        let obj = write_avatar(&dir, &body, &joints)?;
        self.offer_avatar(&obj);
        self.status = format!(
            "Avatar made to {:.1} / {:.1} / {:.1} / {:.1} / {:.1} / {:.1} cm · confirm to replace the current one",
            t[0], t[1], t[2], t[3], t[4], t[5]
        );
        Ok(())
    }
}

/// The T-shirt drafting window: measurements in centimetres, filled from the avatar on asking.
pub(super) struct DraftUi {
    pub open: bool,
    values: [f64; 8],
    error: Option<String>,
}

impl Default for DraftUi {
    fn default() -> Self {
        let m = fashioncraft::garment::presets::TshirtMeasurements::default();
        Self {
            open: false,
            values: [
                m.bust_m,
                m.hip_m,
                m.ease_m,
                m.length_m,
                m.shoulder_m,
                m.sleeve_m,
                m.cuff_m,
                m.top_m,
            ]
            .map(|v| (v * 1000.).round() / 10.),
            error: None,
        }
    }
}

impl Atelier {
    /// The measurements of the current avatar a shirt should fit, centimetres.
    fn avatar_shirt_values(&self) -> Result<[f64; 8], String> {
        let path = &self.project.avatar_path;
        let measured = measure(path)?;
        let joint = |id: &str| {
            measured
                .joints
                .get(id)
                .copied()
                .ok_or_else(|| format!("The avatar lacks the joint {id}"))
        };
        let (l, r) = (joint("joint-l-shoulder")?, joint("joint-r-shoulder")?);
        let now = measured.now;
        let mut v = self.authoring.shirt_draft.values;
        v[0] = now.bust_m * 100.;
        v[1] = now.hip_m * 100.;
        v[3] = 66. * now.height_m / 1.75;
        v[4] = (l.distance(r) + 0.08) * 100.;
        v[7] = (l.y.max(r.y) + 0.09) * 100.;
        Ok(v.map(|x| (x * 10.).round() / 10.))
    }

    pub(super) fn draft_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.shirt_draft.open {
            return;
        }
        let mut open = true;
        let (mut from_avatar, mut create) = (false, false);
        let state = &mut self.authoring.shirt_draft;
        egui::Window::new(i18n::tr(ui_lang, "T-shirt from measurements"))
            .open(&mut open)
            .resizable(false)
            .default_width(300.)
            .show(ctx, |ui| {
                egui::Grid::new("draft.tshirt")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (k, name) in [
                            "Bust",
                            "Hip",
                            "Ease",
                            "Length",
                            "Shoulders",
                            "Sleeve",
                            "Sleeve opening",
                            "Shoulder height",
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.label(i18n::tr(ui_lang, name));
                            ui.add(
                                egui::DragValue::new(&mut state.values[k])
                                    .range(0.0..=250.0)
                                    .speed(0.5)
                                    .suffix(" cm"),
                            );
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui
                        .button(i18n::tr(ui_lang, "From the avatar"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Bust, hip, shoulders, length and placement from the current avatar",
                        ))
                        .clicked()
                    {
                        from_avatar = true;
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Create"))
                        .on_hover_text(i18n::tr(ui_lang, "Replace the garment with this T-shirt"))
                        .clicked()
                    {
                        create = true;
                    }
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
            });
        self.authoring.shirt_draft.open &= open;
        if from_avatar {
            match self.avatar_shirt_values() {
                Ok(v) => {
                    self.authoring.shirt_draft.values = v;
                    self.authoring.shirt_draft.error = None;
                }
                Err(e) => self.authoring.shirt_draft.error = Some(e),
            }
        }
        if create {
            let v = self.authoring.shirt_draft.values.map(|x| x / 100.);
            let m = fashioncraft::garment::presets::TshirtMeasurements {
                bust_m: v[0],
                hip_m: v[1],
                ease_m: v[2],
                length_m: v[3],
                shoulder_m: v[4],
                sleeve_m: v[5],
                cuff_m: v[6],
                top_m: v[7],
            };
            match fashioncraft::garment::presets::tshirt(&m, 2) {
                Ok(template) => {
                    let mut project = FittingProject::basic(
                        GarmentKind::Tshirt,
                        2,
                        self.project.avatar_path.clone(),
                    );
                    project.template = template;
                    project.avatar_pose = self.project.avatar_pose.clone();
                    project.scene_objects = self.project.scene_objects.clone();
                    if self.load(project) {
                        self.authoring.shirt_draft.error = None;
                        self.status =
                            i18n::tr(ui_lang, "T-shirt drafted from the measurements").into();
                    } else {
                        self.authoring.shirt_draft.error = Some(self.status.clone());
                    }
                }
                Err(e) => self.authoring.shirt_draft.error = Some(e),
            }
        }
    }
}

/// The skirt drafting window: measurements in centimetres, filled from the avatar on asking.
pub(super) struct SkirtDraftUi {
    pub open: bool,
    values: [f64; 7],
    error: Option<String>,
}

impl Default for SkirtDraftUi {
    fn default() -> Self {
        let m = fashioncraft::garment::presets::SkirtMeasurements::default();
        Self {
            open: false,
            values: [
                m.waist_m,
                m.hip_m,
                m.ease_m,
                m.hip_depth_m,
                m.length_m,
                m.flare_m,
                m.waist_height_m,
            ]
            .map(|v| (v * 1000.).round() / 10.),
            error: None,
        }
    }
}

impl Atelier {
    /// The measurements of the current avatar a skirt should fit, centimetres.
    fn avatar_skirt_values(&self) -> Result<[f64; 7], String> {
        let measured = measure(&self.project.avatar_path)?;
        let joint = |id: &str| {
            measured
                .joints
                .get(id)
                .copied()
                .ok_or_else(|| format!("The avatar lacks the joint {id}"))
        };
        let (waist, leg) = (joint("joint-spine-3")?, joint("joint-l-upper-leg")?);
        let now = measured.now;
        let mut v = self.authoring.skirt_draft.values;
        v[0] = now.waist_m * 100.;
        v[1] = now.hip_m * 100.;
        v[3] = (waist.y - leg.y + 0.03) * 100.;
        v[4] = 55. * now.height_m / 1.75;
        v[6] = (waist.y + 0.01) * 100.;
        Ok(v.map(|x| (x * 10.).round() / 10.))
    }

    pub(super) fn skirt_draft_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.skirt_draft.open {
            return;
        }
        let mut open = true;
        let (mut from_avatar, mut create) = (false, false);
        let state = &mut self.authoring.skirt_draft;
        egui::Window::new(i18n::tr(ui_lang, "Skirt from measurements"))
            .open(&mut open)
            .resizable(false)
            .default_width(300.)
            .show(ctx, |ui| {
                egui::Grid::new("draft.skirt")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (k, name) in [
                            "Waist",
                            "Hip",
                            "Ease",
                            "Hip depth",
                            "Length",
                            "Flare",
                            "Waist height",
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.label(i18n::tr(ui_lang, name));
                            ui.add(
                                egui::DragValue::new(&mut state.values[k])
                                    .range(0.0..=250.0)
                                    .speed(0.5)
                                    .suffix(" cm"),
                            );
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui
                        .button(i18n::tr(ui_lang, "From the avatar"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Waist, hip, depths, length and placement from the current avatar",
                        ))
                        .clicked()
                    {
                        from_avatar = true;
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Create"))
                        .on_hover_text(i18n::tr(ui_lang, "Replace the garment with this skirt"))
                        .clicked()
                    {
                        create = true;
                    }
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
            });
        self.authoring.skirt_draft.open &= open;
        if from_avatar {
            match self.avatar_skirt_values() {
                Ok(v) => {
                    self.authoring.skirt_draft.values = v;
                    self.authoring.skirt_draft.error = None;
                }
                Err(e) => self.authoring.skirt_draft.error = Some(e),
            }
        }
        if create {
            let v = self.authoring.skirt_draft.values.map(|x| x / 100.);
            let m = fashioncraft::garment::presets::SkirtMeasurements {
                waist_m: v[0],
                hip_m: v[1],
                ease_m: v[2],
                hip_depth_m: v[3],
                length_m: v[4],
                flare_m: v[5],
                waist_height_m: v[6],
            };
            match fashioncraft::garment::presets::skirt(&m, 2) {
                Ok(template) => {
                    let mut project = FittingProject::basic(
                        GarmentKind::Tshirt,
                        2,
                        self.project.avatar_path.clone(),
                    );
                    project.template = template;
                    project.avatar_pose = self.project.avatar_pose.clone();
                    project.scene_objects = self.project.scene_objects.clone();
                    if self.load(project) {
                        self.authoring.skirt_draft.error = None;
                        self.status =
                            i18n::tr(ui_lang, "Skirt drafted from the measurements").into();
                    } else {
                        self.authoring.skirt_draft.error = Some(self.status.clone());
                    }
                }
                Err(e) => self.authoring.skirt_draft.error = Some(e),
            }
        }
    }
}

/// The trousers drafting window: measurements in centimetres, filled from the avatar on asking.
pub(super) struct TrousersDraftUi {
    pub open: bool,
    values: [f64; 8],
    error: Option<String>,
}

impl Default for TrousersDraftUi {
    fn default() -> Self {
        let m = fashioncraft::garment::presets::TrousersMeasurements::default();
        Self {
            open: false,
            values: [
                m.waist_m,
                m.hip_m,
                m.ease_m,
                m.hip_depth_m,
                m.rise_m,
                m.length_m,
                m.hem_m,
                m.waist_height_m,
            ]
            .map(|v| (v * 1000.).round() / 10.),
            error: None,
        }
    }
}

impl Atelier {
    /// The measurements of the current avatar trousers should fit, centimetres.
    fn avatar_trouser_values(&self) -> Result<[f64; 8], String> {
        let measured = measure(&self.project.avatar_path)?;
        let joint = |id: &str| {
            measured
                .joints
                .get(id)
                .copied()
                .ok_or_else(|| format!("The avatar lacks the joint {id}"))
        };
        let waist = joint("joint-spine-3")?;
        let leg = joint("joint-l-upper-leg")?;
        let ankle = joint("joint-l-ankle").or_else(|_| joint("joint-l-foot"))?;
        let now = measured.now;
        let mut v = self.authoring.trousers_draft.values;
        v[0] = now.waist_m * 100.;
        v[1] = now.hip_m * 100.;
        v[3] = (waist.y - leg.y + 0.03) * 100.;
        // The crotch sits a little below the hip joints.
        v[4] = (waist.y - leg.y + 0.12) * 100.;
        v[5] = (waist.y - ankle.y - 0.02) * 100.;
        v[7] = (waist.y + 0.01) * 100.;
        Ok(v.map(|x| (x * 10.).round() / 10.))
    }

    pub(super) fn trousers_draft_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.trousers_draft.open {
            return;
        }
        let mut open = true;
        let (mut from_avatar, mut create) = (false, false);
        let state = &mut self.authoring.trousers_draft;
        egui::Window::new(i18n::tr(ui_lang, "Trousers from measurements"))
            .open(&mut open)
            .resizable(false)
            .default_width(300.)
            .show(ctx, |ui| {
                egui::Grid::new("draft.trousers").num_columns(2).show(ui, |ui| {
                    for (k, name) in [
                        "Waist",
                        "Hip",
                        "Ease",
                        "Hip depth",
                        "Rise",
                        "Length",
                        "Leg opening",
                        "Waist height",
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.label(i18n::tr(ui_lang, name));
                        let field = ui.add(
                            egui::DragValue::new(&mut state.values[k])
                                .range(0.0..=250.0)
                                .speed(0.5)
                                .suffix(" cm"),
                        );
                        super::demo::mark(ui.ctx(), format_args!("draft.trousers.{k}"), field.rect);
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    let button = ui
                        .button(i18n::tr(ui_lang, "From the avatar"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Waist, hip, depths, rise, length and placement from the current avatar",
                        ));
                    super::demo::mark(ui.ctx(), "draft.trousers.from_avatar", button.rect);
                    if button.clicked() {
                        from_avatar = true;
                    }
                    let button = ui
                        .button(i18n::tr(ui_lang, "Create"))
                        .on_hover_text(i18n::tr(ui_lang, "Replace the garment with these trousers"));
                    super::demo::mark(ui.ctx(), "draft.trousers.create", button.rect);
                    if button.clicked() {
                        create = true;
                    }
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
            });
        self.authoring.trousers_draft.open &= open;
        if from_avatar {
            match self.avatar_trouser_values() {
                Ok(v) => {
                    self.authoring.trousers_draft.values = v;
                    self.authoring.trousers_draft.error = None;
                }
                Err(e) => self.authoring.trousers_draft.error = Some(e),
            }
        }
        if create {
            let v = self.authoring.trousers_draft.values.map(|x| x / 100.);
            let m = fashioncraft::garment::presets::TrousersMeasurements {
                waist_m: v[0],
                hip_m: v[1],
                ease_m: v[2],
                hip_depth_m: v[3],
                rise_m: v[4],
                length_m: v[5],
                hem_m: v[6],
                waist_height_m: v[7],
            };
            match fashioncraft::garment::presets::trousers(&m, 2) {
                Ok(template) => {
                    let mut project = FittingProject::basic(
                        GarmentKind::Tshirt,
                        2,
                        self.project.avatar_path.clone(),
                    );
                    project.template = template;
                    project.avatar_pose = self.project.avatar_pose.clone();
                    project.scene_objects = self.project.scene_objects.clone();
                    if self.load(project) {
                        self.authoring.trousers_draft.error = None;
                        self.status =
                            i18n::tr(ui_lang, "Trousers drafted from the measurements").into();
                    } else {
                        self.authoring.trousers_draft.error = Some(self.status.clone());
                    }
                }
                Err(e) => self.authoring.trousers_draft.error = Some(e),
            }
        }
    }
}

/// The measurements each drafted garment takes, by name, in the drafting windows' order
/// (centimetres). Agents name them in `draft_garment`.
const SHIRT_FIELDS: [&str; 8] = [
    "bust", "hip", "ease", "length", "shoulder", "sleeve", "cuff", "top",
];
const DRESS_FIELDS: [&str; 8] = [
    "bust",
    "hip",
    "ease",
    "length",
    "shoulder",
    "hip_depth",
    "flare",
    "top",
];
const BLOUSE_FIELDS: [&str; 9] = [
    "bust",
    "hip",
    "ease",
    "length",
    "shoulder",
    "upper_arm",
    "sleeve",
    "cuff",
    "top",
];
const SKIRT_FIELDS: [&str; 7] = [
    "waist",
    "hip",
    "ease",
    "hip_depth",
    "length",
    "flare",
    "waist_height",
];
const TROUSERS_FIELDS: [&str; 8] = [
    "waist",
    "hip",
    "ease",
    "hip_depth",
    "rise",
    "length",
    "hem",
    "waist_height",
];

/// `values` with the named `overrides` (centimetres) set; an unknown name is an error.
fn with_overrides<const N: usize>(
    mut values: [f64; N],
    names: &[&str; N],
    overrides: &BTreeMap<String, f64>,
) -> Result<[f64; N], String> {
    for (name, value) in overrides {
        let Some(i) = names.iter().position(|n| n == name) else {
            return Err(format!(
                "unknown measurement '{name}'; this garment takes {}",
                names.join(", ")
            ));
        };
        if !value.is_finite() || *value < 0. || *value > 400. {
            return Err(format!("{name} = {value} cm is out of range"));
        }
        values[i] = *value;
    }
    Ok(values)
}

fn listed<const N: usize>(names: &[&str; N], values: &[f64; N]) -> String {
    names
        .iter()
        .zip(values)
        .map(|(n, v)| format!("{n} {v:.1}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Atelier {
    /// Drafts a garment for an agent, as the drafting windows' Create does: the window's
    /// values, or the avatar's when `from_avatar`, with `overrides` (centimetres) on top.
    /// Reports the measurements used.
    pub(super) fn agent_draft(
        &mut self,
        kind: &str,
        from_avatar: bool,
        overrides: &BTreeMap<String, f64>,
    ) -> Result<String, String> {
        use fashioncraft::garment::presets;
        let (template, used) = match kind {
            "tshirt" => {
                let base = if from_avatar {
                    self.avatar_shirt_values()?
                } else {
                    self.authoring.shirt_draft.values
                };
                let v = with_overrides(base, &SHIRT_FIELDS, overrides)?;
                let m = v.map(|x| x / 100.);
                let template = presets::tshirt(
                    &presets::TshirtMeasurements {
                        bust_m: m[0],
                        hip_m: m[1],
                        ease_m: m[2],
                        length_m: m[3],
                        shoulder_m: m[4],
                        sleeve_m: m[5],
                        cuff_m: m[6],
                        top_m: m[7],
                    },
                    2,
                )
                .map_err(|e| format!("{e} (cm: {})", listed(&SHIRT_FIELDS, &v)))?;
                // Kept for the drafting window only once the pattern is made.
                self.authoring.shirt_draft.values = v;
                (template, listed(&SHIRT_FIELDS, &v))
            }
            "skirt" => {
                let base = if from_avatar {
                    self.avatar_skirt_values()?
                } else {
                    self.authoring.skirt_draft.values
                };
                let v = with_overrides(base, &SKIRT_FIELDS, overrides)?;
                let m = v.map(|x| x / 100.);
                let template = presets::skirt(
                    &presets::SkirtMeasurements {
                        waist_m: m[0],
                        hip_m: m[1],
                        ease_m: m[2],
                        hip_depth_m: m[3],
                        length_m: m[4],
                        flare_m: m[5],
                        waist_height_m: m[6],
                    },
                    2,
                )
                .map_err(|e| format!("{e} (cm: {})", listed(&SKIRT_FIELDS, &v)))?;
                // Kept for the drafting window only once the pattern is made.
                self.authoring.skirt_draft.values = v;
                (template, listed(&SKIRT_FIELDS, &v))
            }
            "trousers" => {
                let base = if from_avatar {
                    self.avatar_trouser_values()?
                } else {
                    self.authoring.trousers_draft.values
                };
                let v = with_overrides(base, &TROUSERS_FIELDS, overrides)?;
                let m = v.map(|x| x / 100.);
                let template = presets::trousers(
                    &presets::TrousersMeasurements {
                        waist_m: m[0],
                        hip_m: m[1],
                        ease_m: m[2],
                        hip_depth_m: m[3],
                        rise_m: m[4],
                        length_m: m[5],
                        hem_m: m[6],
                        waist_height_m: m[7],
                    },
                    2,
                )
                .map_err(|e| format!("{e} (cm: {})", listed(&TROUSERS_FIELDS, &v)))?;
                // Kept for the drafting window only once the pattern is made.
                self.authoring.trousers_draft.values = v;
                (template, listed(&TROUSERS_FIELDS, &v))
            }
            "dress" => {
                let base = if from_avatar {
                    self.avatar_dress_values()?
                } else {
                    let m = presets::DressMeasurements::default();
                    [
                        m.bust_m,
                        m.hip_m,
                        m.ease_m,
                        m.length_m,
                        m.shoulder_m,
                        m.hip_depth_m,
                        m.flare_m,
                        m.top_m,
                    ]
                    .map(|x| (x * 1000.).round() / 10.)
                };
                let v = with_overrides(base, &DRESS_FIELDS, overrides)?;
                let m = v.map(|x| x / 100.);
                let template = presets::dress(
                    &presets::DressMeasurements {
                        bust_m: m[0],
                        hip_m: m[1],
                        ease_m: m[2],
                        length_m: m[3],
                        shoulder_m: m[4],
                        hip_depth_m: m[5],
                        flare_m: m[6],
                        top_m: m[7],
                    },
                    2,
                )
                .map_err(|e| format!("{e} (cm: {})", listed(&DRESS_FIELDS, &v)))?;
                (template, listed(&DRESS_FIELDS, &v))
            }
            "shirt" => {
                let defaults = presets::ShirtMeasurements::default();
                let (base, arms) = if from_avatar {
                    self.avatar_blouse_values()?
                } else {
                    let m = defaults;
                    (
                        [
                            m.bust_m,
                            m.hip_m,
                            m.ease_m,
                            m.length_m,
                            m.shoulder_m,
                            m.upper_arm_m,
                            m.sleeve_m,
                            m.cuff_m,
                            m.top_m,
                        ]
                        .map(|x| (x * 1000.).round() / 10.),
                        m.arms,
                    )
                };
                let v = with_overrides(base, &BLOUSE_FIELDS, overrides)?;
                let m = v.map(|x| x / 100.);
                // Each sleeve starts clear of the avatar's arm, as it stands now.
                let body = &self.scene.collider.surface.vertices;
                let template = presets::shirt_around(
                    &presets::ShirtMeasurements {
                        bust_m: m[0],
                        hip_m: m[1],
                        ease_m: m[2],
                        length_m: m[3],
                        shoulder_m: m[4],
                        upper_arm_m: m[5],
                        sleeve_m: m[6],
                        cuff_m: m[7],
                        top_m: m[8],
                        arms,
                    },
                    2,
                    (!self.project.avatar_removed).then_some(body.as_slice()),
                )
                .map_err(|e| format!("{e} (cm: {})", listed(&BLOUSE_FIELDS, &v)))?;
                (template, listed(&BLOUSE_FIELDS, &v))
            }
            other => return Err(format!("unknown garment kind '{other}'")),
        };
        let mut project =
            FittingProject::basic(GarmentKind::Tshirt, 2, self.project.avatar_path.clone());
        project.template = template;
        project.avatar_pose = self.project.avatar_pose.clone();
        project.scene_objects = self.project.scene_objects.clone();
        if !self.load(project) {
            return Err(self.status.clone());
        }
        let garment = match kind {
            "tshirt" => "a t-shirt",
            "shirt" => "a shirt with set-in sleeves",
            "dress" => "a dress",
            "skirt" => "a skirt",
            _ => "trousers",
        };
        let mut report = format!("Drafted {garment} (cm): {used}");
        // A piece that starts inside the body would be pushed out violently: say so.
        if let Err(e) = fashioncraft::avatar::diagnostics::check_vbd_initialization(
            self.scene.garment.clone(),
            &self.scene.collider.surface,
            &self.scene.state,
        ) {
            report.push_str(&format!(". Warning: it starts touching the body ({e})"));
        }
        Ok(report)
    }
}

/// The avatar's own body measurements by name, centimetres, as the avatar editor lists them.
pub(super) const BODY_FIELDS: [&str; 6] = ["height", "bust", "waist", "hip", "arm", "inseam"];

impl Atelier {
    /// The avatar's height, bust, waist, hip, arm length and inseam, centimetres, measured
    /// once per avatar file.
    pub(super) fn avatar_body_cm(&mut self) -> Result<[f64; 6], String> {
        let path = self.project.avatar_path.clone();
        let state = &mut self.authoring.avatar_shape;
        if state.measured.as_ref().map(|(p, _)| p) != Some(&path) {
            let measured = measure(&path);
            state.target = measured.as_ref().ok().map(|m| centimetres(&m.now));
            state.measured = Some((path, measured));
        }
        match &state.measured {
            Some((_, Ok(m))) => Ok(centimetres(&m.now)),
            Some((_, Err(e))) => Err(e.clone()),
            None => Err("The avatar is not measured".into()),
        }
    }

    /// Reshape the avatar to `overrides` (centimetres, by [`BODY_FIELDS`] name; the rest stay)
    /// and put it in place of the current one, as the avatar editor and its confirmation do.
    pub(super) fn agent_reshape_avatar(
        &mut self,
        overrides: &BTreeMap<String, f64>,
    ) -> Result<String, String> {
        let now = self.avatar_body_cm()?;
        let target = with_overrides(now, &BODY_FIELDS, overrides)?;
        self.authoring.avatar_shape.target = Some(target);
        self.make_shaped_avatar()?;
        self.install_avatar_candidate()?;
        Ok(format!(
            "Avatar reshaped (cm): {}. The drape was reset: simulate again.",
            listed(&BODY_FIELDS, &target)
        ))
    }

    /// The avatar's joints by name, metres.
    fn avatar_joints(&self) -> Result<BTreeMap<String, glam::DVec3>, String> {
        Ok(measure(&self.project.avatar_path)?.joints)
    }

    /// The measurements of the current avatar a dress should fit, centimetres.
    fn avatar_dress_values(&self) -> Result<[f64; 8], String> {
        let measured = measure(&self.project.avatar_path)?;
        let joint = |id: &str| {
            measured
                .joints
                .get(id)
                .copied()
                .ok_or_else(|| format!("The avatar lacks the joint {id}"))
        };
        let (l, r) = (joint("joint-l-shoulder")?, joint("joint-r-shoulder")?);
        let now = measured.now;
        let scale = now.height_m / 1.75;
        let d = fashioncraft::garment::presets::DressMeasurements::default();
        let v = [
            now.bust_m,
            now.hip_m,
            d.ease_m,
            d.length_m * scale,
            l.distance(r) + 0.03,
            d.hip_depth_m * scale,
            d.flare_m,
            l.y.max(r.y) + 0.09,
        ];
        Ok(v.map(|x| (x * 1000.).round() / 10.))
    }

    /// The measurements of the current avatar a shirt with set-in sleeves should fit,
    /// centimetres, and its arms' shoulder and elbow joints (+x side first), metres.
    fn avatar_blouse_values(&self) -> Result<([f64; 9], [[glam::DVec3; 2]; 2]), String> {
        let measured = measure(&self.project.avatar_path)?;
        let joints = self.avatar_joints()?;
        let joint = |id: &str| {
            joints
                .get(id)
                .copied()
                .ok_or_else(|| format!("The avatar lacks the joint {id}"))
        };
        let mut arms = [
            [joint("joint-l-shoulder")?, joint("joint-l-elbow")?],
            [joint("joint-r-shoulder")?, joint("joint-r-elbow")?],
        ];
        if arms[0][0].x < arms[1][0].x {
            arms.swap(0, 1);
        }
        let now = measured.now;
        let s = fashioncraft::garment::presets::ShirtMeasurements::default();
        let scale = now.height_m / 1.75;
        let v = [
            now.bust_m,
            now.hip_m,
            s.ease_m,
            s.length_m * scale,
            arms[0][0].distance(arms[1][0]) + 0.05,
            s.upper_arm_m * now.bust_m / 0.88,
            s.sleeve_m * scale,
            s.cuff_m * now.bust_m / 0.88,
            arms[0][0].y.max(arms[1][0].y) + 0.09,
        ];
        Ok((v.map(|x| (x * 1000.).round() / 10.), arms))
    }
}
