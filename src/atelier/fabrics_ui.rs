//! Fabric controls convert user units to the existing SI material model.
use super::*;
use marvelous_cloth::{
    authoring::Command,
    fabrics::{self, PRESETS},
    garment::template::{FabricProperties, Panel},
};

pub(super) struct FabricDraft {
    panel: String,
    name: String,
    properties: FabricProperties,
    extension: [f64; 2],
    preset: Option<usize>,
    all_panels: bool,
    appearance: Option<marvelous_cloth::garment::appearance::FabricAppearance>,
}
impl Atelier {
    pub(super) fn fabric_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        let Some(fabric) = self
            .project
            .template
            .fabrics
            .iter()
            .find(|f| f.id == panel.fabric)
        else {
            return;
        };
        if self
            .authoring
            .fabric_draft
            .as_ref()
            .is_none_or(|d| d.panel != panel.id)
        {
            self.authoring.fabric_draft = Some(FabricDraft {
                panel: panel.id.clone(),
                name: fabric.name.clone(),
                extension: [
                    fabrics::extension_percent(fabric.properties.warp, &fabric.properties),
                    fabrics::extension_percent(fabric.properties.weft, &fabric.properties),
                ],
                properties: fabric.properties.clone(),
                preset: PRESETS
                    .iter()
                    .position(|p| p.properties() == fabric.properties),
                all_panels: false,
                appearance: self
                    .project
                    .template
                    .fabric_appearance
                    .get(&fabric.id)
                    .cloned(),
            });
        }
        let mut appearance_targets = Vec::new();
        let appearance_action = appearance_controls(
            ui,
            &mut self.authoring.fabric_draft.as_mut().unwrap().appearance,
            &mut appearance_targets,
        );
        for (name, rect) in appearance_targets {
            if let Some(smoke) = &mut self.authoring_smoke {
                smoke.target_rect(ui, name, rect);
            }
        }
        if let Some(appearance) = appearance_action {
            let fabric = fabric.id.clone();
            if self.project.template.fabric_appearance.get(&fabric) == appearance.as_ref() {
                self.status = "Fabric appearance is unchanged".into();
                return;
            }
            self.edit(Command::FabricAppearance { fabric, appearance });
            return;
        }
        let panel_mass: f64 = self
            .scene
            .garment
            .panels
            .iter()
            .find(|p| p.id == panel.id)
            .map(|p| {
                self.scene.garment.mesh.masses[p.vertices.clone()]
                    .iter()
                    .sum()
            })
            .unwrap_or(0.);
        ui.small(format!(
            "{} · {:.0} g/m²",
            fabric.name,
            fabric.properties.density * 1000.
        ));
        ui.small(format!(
            "Mass: panel {:.1} g · garment {:.1} g",
            panel_mass * 1000.,
            self.scene.garment.mesh.masses.iter().sum::<f64>() * 1000.
        ));
        let mut apply = false;
        let mut discard = false;
        let mut targets = vec![];
        let scene_gap = self.settings.contact.radius;
        let header = egui::CollapsingHeader::new("Fabric · weight and stretch")
            .id_salt("fabric_controls").default_open(true).show(ui, |ui| {
                let d = self.authoring.fabric_draft.as_mut().unwrap();
                let old = d.preset;
                let combo = egui::ComboBox::from_id_salt("fabric_preset")
                    .selected_text(d.preset.map(|i| PRESETS[i].name).unwrap_or("Choose a fabric type"))
                    .show_ui(ui, |ui| {
                        for (i, p) in PRESETS.iter().enumerate() {
                            let response = ui.selectable_value(&mut d.preset, Some(i), p.name);
                            targets.push((format!("fabric_preset_{}", p.id), response.rect.center()));
                        }
                    });
                targets.push(("fabric_presets".into(), combo.response.rect.center()));
                if d.preset != old && let Some(i) = d.preset {
                    let preset = &PRESETS[i];
                    d.name = format!("{} (approx.)", preset.name);
                    d.properties = preset.properties();
                    d.extension = preset.extension;
                }
                ui.small("Indicative presets. Elasticity and bending must be calibrated on your fabric.");
                if let Some(i) = d.preset {
                    ui.small(PRESETS[i].evidence);
                    ui.hyperlink_to("Measurement source", PRESETS[i].source);
                }
                let name = ui.text_edit_singleline(&mut d.name);
                if let Some(smoke) = &mut self.authoring_smoke {
                    smoke.appearance_text_target(ui, &name);
                }
                let p = &mut d.properties;
                let before_properties = p.clone();
                let mut gsm = p.density * 1000.;
                if value(ui, "Area weight", &mut gsm, 1., 10., 2000., " g/m²") { p.density = gsm / 1000.; }
                let mut thickness = p.thickness * 1000.;
                if value(ui, "Thickness", &mut thickness, 0.01, 0.02, 5., " mm") { p.thickness = thickness / 1000.; }
                let mut own = p.collision_distance.is_some();
                if ui
                    .checkbox(&mut own, "Own collision distance")
                    .on_hover_text(
                        "Gap the simulator keeps between this fabric and the avatar, the floor or \
                         other cloth; two fabrics keep the mean of theirs. Off: the scene's \
                         Collision distance in Simulation",
                    )
                    .changed()
                {
                    p.collision_distance = own.then_some(scene_gap);
                }
                if let Some(gap) = p.collision_distance.as_mut() {
                    let mut mm = *gap * 1000.;
                    if value(ui, "Collision distance", &mut mm, 0.05, 0.5, 20., " mm") { *gap = mm / 1000.; }
                }
                let mut damping_ms = p.internal_damping * 1000.;
                if value(ui, "Internal damping", &mut damping_ms, 0.1, 0., 100., " ms") {
                    p.internal_damping = damping_ms / 1000.;
                }
                ui.small("Internal damping settles stretching and bending; the piece as a whole still moves freely.");
                ui.label("Stretch under 5 N on a 5 cm strip");
                ui.small("The larger the %, the more the fabric stretches. This is not the strain at failure.");
                for (axis, label) in ["Warp", "Weft"].iter().enumerate() {
                    if value(ui, label, &mut d.extension[axis], 0.1, 0.1, 100., " %")
                        && let Ok(k) = fabrics::stiffness_for_extension(d.extension[axis], p) {
                            if axis == 0 { p.warp = k; } else { p.weft = k; }
                        }
                }
                ui.collapsing("Advanced stiffness", |ui| {
                    value(ui, "Shear", &mut p.shear, 1., 0.1, 10000., " N/m");
                    for (label, b) in [("Bending warp", &mut p.bend_warp), ("Bending weft", &mut p.bend_weft)] {
                        let mut micro = *b * 1e6;
                        if value(ui, label, &mut micro, 1., 0., 100000., " µN·m") { *b = micro * 1e-6; }
                    }
                    ui.small(format!("Warp/weft membrane: {:.0} / {:.0} N/m", p.warp, p.weft));
                    ui.small("Without its own collision distance a fabric uses the scene's, in Simulation.");
                });
                if d.properties != before_properties { d.preset = None; }
                ui.checkbox(&mut d.all_panels, "Apply to all panels");
                let button = ui.button("Apply fabric");
                targets.push(("fabric_apply".into(), button.rect.center()));
                apply = button.clicked();
                discard = ui.button("Discard settings").clicked();
            });
        self.smoke_target("fabric_panel", header.header_response.rect.center());
        for (name, p) in targets {
            self.smoke_target(&name, p);
        }
        if apply {
            let d = self.authoring.fabric_draft.as_ref().unwrap();
            self.edit(Command::Fabric {
                panel: d.panel.clone(),
                name: d.name.clone(),
                properties: d.properties.clone(),
                all_panels: d.all_panels,
            });
        } else if discard {
            self.authoring.fabric_draft = None;
        }
    }
}
fn value(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f64,
    speed: f64,
    min: f64,
    max: f64,
    suffix: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(v)
                .speed(speed)
                .range(min..=max)
                .suffix(suffix),
        )
        .changed()
    })
    .inner
}

fn appearance_controls(
    ui: &mut egui::Ui,
    draft: &mut Option<marvelous_cloth::garment::appearance::FabricAppearance>,
    targets: &mut Vec<(&'static str, egui::Rect)>,
) -> Option<Option<marvelous_cloth::garment::appearance::FabricAppearance>> {
    ui.strong("Fabric appearance");
    ui.small("Shared by panels using this fabric. Does not change its mechanics.");
    let mut custom = draft.is_some();
    let enable = ui.checkbox(&mut custom, "Custom color and roughness");
    #[cfg(test)]
    super::authoring_smoke::widget_harness::named(ui, &enable, "appearance.enabled");
    targets.push(("appearance.enabled", enable.rect));
    if enable.changed() {
        *draft = custom.then(Default::default);
    }
    if let Some(value) = draft {
        ui.horizontal(|ui| {
            ui.label("Color (sRGB)");
            let swatch = ui.color_edit_button_srgb(&mut value.color_srgb);
            #[cfg(test)]
            super::authoring_smoke::widget_harness::named(ui, &swatch, "appearance.color");
            targets.push(("appearance.color", swatch.rect));
        });
        ui.horizontal(|ui| {
            for (channel, label, key) in [
                (0, "R", "appearance.color.red"),
                (1, "G", "appearance.color.green"),
                (2, "B", "appearance.color.blue"),
            ] {
                let response = ui
                    .add(
                        egui::DragValue::new(&mut value.color_srgb[channel])
                            .range(0..=255)
                            .speed(1.0)
                            .prefix(format!("{label} ")),
                    )
                    .on_hover_text("sRGB byte (0-255). Apply appearance to commit.");
                #[cfg(test)]
                super::authoring_smoke::widget_harness::named(ui, &response, key);
                targets.push((key, response.rect));
            }
        });
        let roughness =
            ui.add(egui::Slider::new(&mut value.roughness, 0.0..=1.0).text("Roughness"));
        #[cfg(test)]
        super::authoring_smoke::widget_harness::named(ui, &roughness, "appearance.roughness");
        targets.push((
            "appearance.roughness.low",
            egui::Rect::from_center_size(
                egui::pos2(roughness.rect.left() + 8., roughness.rect.center().y),
                egui::vec2(4., 4.),
            ),
        ));
        ui.horizontal(|ui| {
            let mut back = value.back_color_srgb.is_some();
            if ui
                .checkbox(&mut back, "Back color")
                .on_hover_text("A different colour on the fabric's back face, as on a lining or a two-tone fabric")
                .changed()
            {
                value.back_color_srgb = back.then_some(value.color_srgb);
            }
            if let Some(color) = value.back_color_srgb.as_mut() {
                ui.color_edit_button_srgb(color);
            }
        });
        use marvelous_cloth::garment::appearance::{Weave, WeaveKind};
        ui.horizontal(|ui| {
            ui.label("Weave");
            let label = value.weave.as_ref().map_or("None", |w| w.kind.label());
            egui::ComboBox::from_id_salt("appearance_weave")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(value.weave.is_none(), "None").clicked() {
                        value.weave = None;
                    }
                    for kind in WeaveKind::ALL {
                        let on = value.weave.as_ref().is_some_and(|w| w.kind == kind);
                        if ui.selectable_label(on, kind.label()).clicked() {
                            let mut w = value.weave.clone().unwrap_or_default();
                            w.kind = kind;
                            value.weave = Some(w);
                        }
                    }
                });
        });
        if let Some(w) = value.weave.as_mut() {
            ui.horizontal(|ui| {
                ui.label("Thread spacing");
                ui.add(
                    egui::DragValue::new(&mut w.spacing_mm)
                        .range(0.05..=20.0)
                        .speed(0.02)
                        .fixed_decimals(2)
                        .suffix(" mm"),
                );
            });
            ui.add(egui::Slider::new(&mut w.contrast, 0.0..=1.0).text("Weave contrast"));
            let _: &Weave = w;
        }
    }
    let apply = ui.button("Apply appearance");
    #[cfg(test)]
    super::authoring_smoke::widget_harness::named(ui, &apply, "appearance.apply");
    targets.push(("appearance.apply", apply.rect));
    ui.separator();
    apply.clicked().then(|| draft.clone())
}

#[cfg(test)]
mod appearance_tests {
    use super::super::authoring_smoke::widget_harness::Harness;
    use super::*;
    #[test]
    fn color_channel_drag_stages_only_that_channel_until_apply() {
        let initial = marvelous_cloth::garment::appearance::FabricAppearance::default();
        let mut state = (Some(initial.clone()), Vec::new());
        let mut draw = |ui: &mut egui::Ui,
                        state: &mut (
            Option<marvelous_cloth::garment::appearance::FabricAppearance>,
            Vec<Option<marvelous_cloth::garment::appearance::FabricAppearance>>,
        )| {
            if let Some(value) = appearance_controls(ui, &mut state.0, &mut Vec::new()) {
                state.1.push(value);
            }
        };
        let mut h = Harness::new(egui::vec2(800., 600.));
        h.frame(&mut state, &mut draw);
        h.drag(
            "appearance.color.red",
            egui::vec2(-40., 0.),
            5,
            &mut state,
            &mut draw,
        )
        .unwrap();
        let draft = state.0.as_ref().unwrap();
        assert!(draft.color_srgb[0] < initial.color_srgb[0]);
        assert_eq!(&draft.color_srgb[1..], &initial.color_srgb[1..]);
        assert_eq!(draft.roughness, initial.roughness);
        assert!(state.1.is_empty(), "editing the draft must not commit it");
        h.click("appearance.apply", &mut state, &mut draw).unwrap();
        assert_eq!(state.1, vec![state.0.clone()]);
    }
    #[test]
    fn named_appearance_controls_stage_before_explicit_apply() {
        let mut state = (None, Vec::new());
        let mut draw = |ui: &mut egui::Ui,
                        state: &mut (
            Option<marvelous_cloth::garment::appearance::FabricAppearance>,
            Vec<Option<marvelous_cloth::garment::appearance::FabricAppearance>>,
        )| {
            if let Some(value) = appearance_controls(ui, &mut state.0, &mut Vec::new()) {
                state.1.push(value);
            }
        };
        let mut h = Harness::new(egui::vec2(800., 600.));
        h.frame(&mut state, &mut draw);
        h.click("appearance.enabled", &mut state, &mut draw)
            .unwrap();
        assert!(state.0.is_some());
        assert!(state.1.is_empty());
        h.click("appearance.apply", &mut state, &mut draw).unwrap();
        assert_eq!(state.1, vec![state.0.clone()]);
        h.click("appearance.enabled", &mut state, &mut draw)
            .unwrap();
        h.click("appearance.apply", &mut state, &mut draw).unwrap();
        assert_eq!(state.1.last(), Some(&None));
    }
}
