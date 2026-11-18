//! Avatar editor: the avatar's height and bust, waist and hip girths, and a new avatar made to
//! other ones. The reshaped body is written next to the session's other files with its joints
//! and anchors, then goes through the usual avatar import confirmation.
use super::*;
use marvelous_cloth::{
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
    target: Option<[f64; 4]>,
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

fn centimetres(m: &BodyMeasurements) -> [f64; 4] {
    [m.height_m, m.bust_m, m.waist_m, m.hip_m].map(|v| (v * 1000.).round() / 10.)
}

impl Atelier {
    pub(super) fn avatar_shape_window(&mut self, ctx: &egui::Context) {
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
        egui::Window::new("Avatar editor")
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
                ui.small("Girths are taken around the torso: bust at the chest, the narrowest waist, the widest hip.");
                egui::Grid::new("avatar.measurements")
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.label("");
                        ui.strong("Now");
                        ui.strong("New");
                        ui.end_row();
                        for (k, name) in ["Height", "Bust", "Waist", "Hip"].iter().enumerate() {
                            ui.label(*name);
                            ui.label(format!("{:.1} cm", now[k]));
                            ui.add(
                                egui::DragValue::new(&mut target[k])
                                    .range(if k == 0 { 50.0..=250.0 } else { 30.0..=220.0 })
                                    .speed(0.5)
                                    .suffix(" cm"),
                            );
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Reset").clicked() {
                        *target = now;
                    }
                    if ui
                        .button("Make avatar…")
                        .on_hover_text("Reshape the avatar to these measurements, then confirm it like an imported avatar")
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
        };
        let (body, joints) = avatar_shape::reshape(&measured.body, &measured.joints, &target)?;
        let dir = self.output.join(format!(
            "avatar-{:.0}-{:.0}-{:.0}-{:.0}",
            t[0], t[1], t[2], t[3]
        ));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let obj = dir.join("avatar.obj");
        std::fs::write(&obj, marvelous_cloth::avatar_animation::surface_obj(&body))
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
        self.offer_avatar(&obj);
        self.status = format!(
            "Avatar made to {:.1} / {:.1} / {:.1} / {:.1} cm · confirm to replace the current one",
            t[0], t[1], t[2], t[3]
        );
        Ok(())
    }
}
