//! Fabric controls convert user units to the existing SI material model.
use super::*;
use fashioncraft::{
    authoring::Command,
    fabrics::{self, PRESETS},
    garment::{
        template::{FabricProperties, Panel},
        texture_set,
    },
};
use std::path::PathBuf;

pub(super) struct FabricDraft {
    panel: String,
    name: String,
    properties: FabricProperties,
    extension: [f64; 2],
    preset: Option<usize>,
    all_panels: bool,
    pub(super) appearance: Option<fashioncraft::garment::appearance::FabricAppearance>,
    /// Texture sets found in the last folder or files picked, when there was more than one to
    /// choose from.
    texture_sets: Vec<fashioncraft::garment::texture_set::TextureSet>,
}
impl Atelier {
    pub(super) fn fabric_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        let ui_lang = self.prefs.language;
        let Some(fabric) = self
            .project
            .template
            .fabrics
            .iter()
            .find(|f| f.id == panel.fabric)
            .cloned()
        else {
            return;
        };
        let fabric = &fabric;
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
                texture_sets: vec![],
            });
        }
        let mut appearance_targets = Vec::new();
        let texture_name = self
            .authoring
            .fabric_draft
            .as_ref()
            .and_then(|d| d.appearance.as_ref()?.texture.as_ref())
            .and_then(|t| {
                self.project
                    .image_assets
                    .iter()
                    .find(|a| a.id == t.asset_id)
            })
            .and_then(|a| a.path.file_stem())
            .map(|s| s.to_string_lossy().into_owned());
        let set_names: Vec<String> = self
            .authoring
            .fabric_draft
            .as_ref()
            .map(|d| {
                d.texture_sets
                    .iter()
                    .map(|s| {
                        let name = if s.name.is_empty() { "—" } else { &s.name };
                        format!("{name} ({})", s.len())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut extras = AppearanceExtras {
            texture_name,
            pick_texture: false,
            pick_map: None,
            pick_set: None,
            set_names,
            chosen_set: None,
            palette: Some(&mut self.project.color_palette),
            eyedropper: self.authoring.eyedropper,
            language: ui_lang,
        };
        let appearance_action = appearance_controls(
            ui,
            &mut self.authoring.fabric_draft.as_mut().unwrap().appearance,
            &mut appearance_targets,
            &mut extras,
        );
        let (pick_texture, pick_map, eyedropper) =
            (extras.pick_texture, extras.pick_map, extras.eyedropper);
        let (pick_set, chosen_set) = (extras.pick_set, extras.chosen_set);
        if eyedropper != self.authoring.eyedropper {
            self.authoring.eyedropper = eyedropper;
            self.status = if eyedropper {
                "Eyedropper: click a piece in the 3D view".into()
            } else {
                String::new()
            };
        }
        if pick_texture {
            self.pick_fabric_texture();
        }
        if let Some(slot) = pick_map {
            self.pick_fabric_map(slot);
        }
        if let Some(folder) = pick_set {
            self.pick_texture_set(folder);
        }
        if let Some(k) = chosen_set
            && let Some(d) = self.authoring.fabric_draft.as_mut()
            && k < d.texture_sets.len()
        {
            let set = d.texture_sets.remove(k);
            d.texture_sets.clear();
            self.stage_texture_set(set);
        }
        for (name, rect) in appearance_targets {
            super::demo::mark(ui.ctx(), name, rect);
            if let Some(smoke) = &mut self.authoring_smoke {
                smoke.target_rect(ui, name, rect);
            }
        }
        if let Some(appearance) = appearance_action {
            let fabric = fabric.id.clone();
            if self.project.template.fabric_appearance.get(&fabric) == appearance.as_ref() {
                self.status = i18n::tr(ui_lang, "Fabric appearance is unchanged").into();
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
            "{} {:.1} g · {} {:.1} g",
            i18n::tr(ui_lang, "Mass: panel"),
            panel_mass * 1000.,
            i18n::tr(ui_lang, "garment"),
            self.scene.garment.mesh.masses.iter().sum::<f64>() * 1000.
        ));
        let mut apply = false;
        let mut discard = false;
        let mut targets = vec![];
        let scene_gap = self.settings.contact.radius;
        let header = egui::CollapsingHeader::new(super::workspace::section(i18n::tr(ui_lang, "Fabric · weight and stretch")))
            .id_salt("fabric_controls").default_open(true).show(ui, |ui| {
                let d = self.authoring.fabric_draft.as_mut().unwrap();
                let old = d.preset;
                let combo = super::workspace::prop_row(ui, i18n::tr(ui_lang, "Fabric type"), |ui| {
                    egui::ComboBox::from_id_salt("fabric_preset")
                        .selected_text(d.preset.map(|i| PRESETS[i].name).unwrap_or(i18n::tr(ui_lang, "Choose a fabric type")))
                        .show_ui(ui, |ui| {
                            for (i, p) in PRESETS.iter().enumerate() {
                                let response = ui.selectable_value(&mut d.preset, Some(i), p.name);
                                targets.push((format!("fabric_preset_{}", p.id), response.rect.center()));
                            }
                        })
                })
                .inner;
                targets.push(("fabric_presets".into(), combo.response.rect.center()));
                if d.preset != old && let Some(i) = d.preset {
                    let preset = &PRESETS[i];
                    d.name = format!("{} (approx.)", preset.name);
                    d.properties = preset.properties();
                    d.extension = preset.extension;
                }
                ui.small(i18n::tr(ui_lang, "Indicative presets. Elasticity and bending must be calibrated on your fabric."));
                if let Some(i) = d.preset {
                    ui.small(PRESETS[i].evidence);
                    ui.hyperlink_to(i18n::tr(ui_lang, "Measurement source"), PRESETS[i].source);
                }
                let name = super::workspace::prop_row(ui, i18n::tr(ui_lang, "Name"), |ui| {
                    ui.text_edit_singleline(&mut d.name)
                })
                .inner;
                if let Some(smoke) = &mut self.authoring_smoke {
                    smoke.appearance_text_target(ui, &name);
                }
                let p = &mut d.properties;
                let before_properties = p.clone();
                let mut gsm = p.density * 1000.;
                if value(ui, i18n::tr(ui_lang, "Area weight"), &mut gsm, 1., 10., 2000., " g/m²") { p.density = gsm / 1000.; }
                let mut thickness = p.thickness * 1000.;
                if value(ui, i18n::tr(ui_lang, "Thickness"), &mut thickness, 0.01, 0.02, 5., " mm") { p.thickness = thickness / 1000.; }
                let mut own = p.collision_distance.is_some();
                let own_changed = super::workspace::prop_row(ui, i18n::tr(ui_lang, "Own collision distance"), |ui| {
                    ui.checkbox(&mut own, "")
                        .on_hover_text(
                            i18n::tr(ui_lang, "Gap the simulator keeps between this fabric and the avatar, the floor or \
                             other cloth; two fabrics keep the mean of theirs. Off: the scene's \
                             Collision distance in Simulation"),
                        )
                        .changed()
                })
                .inner;
                if own_changed {
                    p.collision_distance = own.then_some(scene_gap);
                }
                if let Some(gap) = p.collision_distance.as_mut() {
                    let mut mm = *gap * 1000.;
                    if value(ui, i18n::tr(ui_lang, "Collision distance"), &mut mm, 0.05, 0.5, 20., " mm") { *gap = mm / 1000.; }
                }
                let mut damping_ms = p.internal_damping * 1000.;
                if value(ui, i18n::tr(ui_lang, "Internal damping"), &mut damping_ms, 0.1, 0., 100., " ms") {
                    p.internal_damping = damping_ms / 1000.;
                }
                ui.small(i18n::tr(ui_lang, "Internal damping settles stretching and bending; the piece as a whole still moves freely."));
                super::workspace::group_title(ui, i18n::tr(ui_lang, "Stretch under 5 N on a 5 cm strip"));
                ui.small(i18n::tr(ui_lang, "The larger the %, the more the fabric stretches. This is not the strain at failure."));
                for (axis, label) in ["Warp", "Weft"].iter().enumerate() {
                    if value(ui, i18n::tr(ui_lang, label), &mut d.extension[axis], 0.1, 0.1, 100., " %")
                        && let Ok(k) = fabrics::stiffness_for_extension(d.extension[axis], p) {
                            if axis == 0 { p.warp = k; } else { p.weft = k; }
                        }
                }
                ui.collapsing(super::workspace::section(i18n::tr(ui_lang, "Advanced stiffness")), |ui| {
                    value(ui, i18n::tr(ui_lang, "Shear"), &mut p.shear, 1., 0.1, 10000., " N/m");
                    for (label, b) in [("Bending warp", &mut p.bend_warp), ("Bending weft", &mut p.bend_weft)] {
                        let mut micro = *b * 1e6;
                        if value(ui, i18n::tr(ui_lang, label), &mut micro, 1., 0., 100000., " µN·m") { *b = micro * 1e-6; }
                    }
                    ui.small(format!("{}: {:.0} / {:.0} N/m", i18n::tr(ui_lang, "Warp/weft membrane"), p.warp, p.weft));
                    ui.small(i18n::tr(ui_lang, "Without its own collision distance a fabric uses the scene's, in Simulation."));
                });
                if d.properties != before_properties { d.preset = None; }
                let all = ui.checkbox(&mut d.all_panels, i18n::tr(ui_lang, "Apply to all panels"));
                targets.push(("fabric_all".into(), all.rect.center()));
                let button = ui.button(i18n::tr(ui_lang, "Apply fabric"));
                targets.push(("fabric_apply".into(), button.rect.center()));
                apply = button.clicked();
                discard = ui.button(i18n::tr(ui_lang, "Discard settings")).clicked();
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
impl Atelier {
    /// Stage an image as one of the drafted fabric texture's maps.
    fn pick_fabric_map(&mut self, slot: fashioncraft::garment::appearance::MapSlot) {
        let ui_lang = self.prefs.language;
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &texture_set::IMAGE_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        if let Err(e) = image::image_dimensions(&path) {
            self.status = format!("Cannot open {}: {e}", path.display());
            return;
        }
        let asset_id = self.asset_for(path);
        let Some(texture) = self
            .authoring
            .fabric_draft
            .as_mut()
            .and_then(|d| d.appearance.as_mut())
            .and_then(|a| a.texture.as_mut())
        else {
            return;
        };
        *texture.map_mut(slot) = Some(asset_id);
        if slot == fashioncraft::garment::appearance::MapSlot::Displacement
            && texture.displacement_mm == 0.
        {
            texture.displacement_mm = DEFAULT_DISPLACEMENT_MM;
        }
        self.status = i18n::tr(ui_lang, "Map staged · Apply appearance to show it").into();
    }

    /// Stage an image as the drafted fabric's texture; its height follows the image's aspect.
    fn pick_fabric_texture(&mut self) {
        let ui_lang = self.prefs.language;
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &texture_set::IMAGE_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        if self.stage_fabric_image(path).is_some() {
            self.status = i18n::tr(ui_lang, "Texture staged · Apply appearance to show it").into();
        }
    }

    /// Make `path` the drafted fabric's texture image, keeping the repeat's width, its angle,
    /// relief and maps; its height follows the image's aspect. None when it cannot be read.
    fn stage_fabric_image(&mut self, path: PathBuf) -> Option<()> {
        let (w, h) = match image::image_dimensions(&path) {
            Ok(size) => size,
            Err(e) => {
                self.status = format!("Cannot open {}: {e}", path.display());
                return None;
            }
        };
        let asset_id = self.asset_for(path);
        let appearance = self
            .authoring
            .fabric_draft
            .as_mut()
            .and_then(|d| d.appearance.as_mut())?;
        let previous = appearance.texture.take();
        let width_mm = previous.as_ref().map_or(100., |t| t.width_mm);
        appearance.texture = Some(fashioncraft::garment::appearance::FabricTexture {
            asset_id,
            width_mm,
            height_mm: (width_mm * f64::from(h) / f64::from(w.max(1))).clamp(1., 5000.),
            ..previous.unwrap_or_default()
        });
        Some(())
    }

    /// Pick a folder (or files) holding a texture set exported from a texture tool, and stage
    /// it.
    fn pick_texture_set(&mut self, folder: bool) {
        let paths = if folder {
            let Some(folder) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            match texture_set::folder_files(&folder) {
                Ok(files) => files,
                Err(e) => {
                    self.status = format!("Cannot read {}: {e}", folder.display());
                    return;
                }
            }
        } else {
            let mut extensions = texture_set::IMAGE_EXTENSIONS.to_vec();
            extensions.push("sbsar");
            let Some(files) = rfd::FileDialog::new()
                .add_filter("Texture maps", &extensions)
                .pick_files()
            else {
                return;
            };
            files
        };
        self.texture_set_from_files(&paths);
    }

    /// Stage the texture set among `paths` (a folder's files or the files picked); when they
    /// hold several materials' sets, list them to choose from.
    pub(super) fn texture_set_from_files(&mut self, paths: &[PathBuf]) {
        let ui_lang = self.prefs.language;
        let found = texture_set::group(paths);
        if found.sets.is_empty() {
            self.status = if paths.iter().any(|p| texture_set::is_substance_archive(p)) {
                i18n::tr(ui_lang, SBSAR_NOTE).into()
            } else {
                i18n::tr(ui_lang, "No texture maps recognised: their names need a map suffix such as _BaseColor or _Normal").into()
            };
            return;
        }
        if found.sets.len() == 1 {
            let set = found.sets.into_iter().next().unwrap();
            self.stage_texture_set(set);
            return;
        }
        if let Some(d) = self.authoring.fabric_draft.as_mut() {
            d.texture_sets = found.sets;
        }
        self.status = i18n::tr(
            ui_lang,
            "Several materials found · choose one under Texture set",
        )
        .into();
    }

    /// Stage every map of `set` on the drafted fabric's texture.
    fn stage_texture_set(&mut self, set: texture_set::TextureSet) {
        use fashioncraft::garment::appearance::MapSlot;
        let ui_lang = self.prefs.language;
        let has_texture = self
            .authoring
            .fabric_draft
            .as_ref()
            .and_then(|d| d.appearance.as_ref())
            .is_some_and(|a| a.texture.is_some());
        match set.base_color.clone() {
            Some(path) => {
                if self.stage_fabric_image(path).is_none() {
                    return;
                }
            }
            None if has_texture => {}
            None => {
                self.status = i18n::tr(
                    ui_lang,
                    "The texture set has no base colour map: add a texture image first",
                )
                .into();
                return;
            }
        }
        let maps = [
            (MapSlot::Normal, set.normal.clone()),
            (MapSlot::Roughness, set.roughness.clone()),
            (MapSlot::Metalness, set.metalness.clone()),
            (MapSlot::Opacity, set.opacity.clone()),
            (MapSlot::Displacement, set.height.clone()),
            (MapSlot::Occlusion, set.occlusion.clone()),
        ];
        let assets: Vec<(MapSlot, String)> = maps
            .into_iter()
            .filter_map(|(slot, path)| Some((slot, self.asset_for(path?))))
            .collect();
        let Some(texture) = self
            .authoring
            .fabric_draft
            .as_mut()
            .and_then(|d| d.appearance.as_mut())
            .and_then(|a| a.texture.as_mut())
        else {
            return;
        };
        // The set replaces the maps the texture had.
        for slot in MapSlot::ALL {
            *texture.map_mut(slot) = None;
        }
        for (slot, asset) in assets {
            *texture.map_mut(slot) = Some(asset);
        }
        texture.normal_directx = set.normal_directx();
        texture.roughness_is_glossiness = set.roughness_is_glossiness;
        if set.height.is_some() && texture.displacement_mm == 0. {
            texture.displacement_mm = DEFAULT_DISPLACEMENT_MM;
        }
        self.status = format!(
            "{} · {} {} · {}",
            i18n::tr(ui_lang, "Texture set staged"),
            set.len(),
            i18n::tr(ui_lang, "maps"),
            i18n::tr(ui_lang, "Apply appearance to show it"),
        );
    }
}

/// Height a new displacement map draws, mm.
const DEFAULT_DISPLACEMENT_MM: f64 = 1.;

/// Why `.sbsar` files cannot be opened, and what to do instead.
const SBSAR_NOTE: &str = "Substance .sbsar files cannot be opened: they need Adobe's proprietary engine. Export the maps as images from Substance and pick those";

fn value(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f64,
    speed: f64,
    min: f64,
    max: f64,
    suffix: &str,
) -> bool {
    super::workspace::prop_row(ui, label, |ui| {
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

/// What the appearance controls need beyond the draft: the texture's image name, the
/// project palette and whether the eyedropper is on; and what they ask for in return.
#[derive(Default)]
struct AppearanceExtras<'a> {
    texture_name: Option<String>,
    /// Set when a new texture image was asked for.
    pick_texture: bool,
    /// Set when an image for one of the texture's maps was asked for.
    pick_map: Option<fashioncraft::garment::appearance::MapSlot>,
    /// Set when a texture set was asked for: from a folder (true) or from files (false).
    pick_set: Option<bool>,
    /// Texture sets to choose from (names and map counts), and the one chosen.
    set_names: Vec<String>,
    chosen_set: Option<usize>,
    palette: Option<&'a mut Vec<[u8; 3]>>,
    /// Whether the eyedropper is waiting for a click on the 3D view; the controls toggle it.
    eyedropper: bool,
    /// Interface language of the labels.
    language: super::i18n::Language,
}

/// What the Texture set button does, and why `.sbsar` files are not on the list.
const TEXTURE_SET_HOVER: &str = "Import the maps a texture tool exported (Substance Painter, Designer or Sampler, and others): pick a folder or the files. Names ending in BaseColor, Normal, Roughness or Glossiness, Metallic, Height, Opacity and AO are recognised. Substance .sbsar files are not supported: they need Adobe's proprietary engine, so export the maps as images instead";

fn appearance_controls(
    ui: &mut egui::Ui,
    draft: &mut Option<fashioncraft::garment::appearance::FabricAppearance>,
    targets: &mut Vec<(&'static str, egui::Rect)>,
    extras: &mut AppearanceExtras,
) -> Option<Option<fashioncraft::garment::appearance::FabricAppearance>> {
    let lang = extras.language;
    let tr = |text: &'static str| super::i18n::tr(lang, text);
    super::workspace::group_title(ui, tr("Fabric appearance"));
    ui.small(tr(
        "Shared by panels using this fabric. Does not change its mechanics.",
    ));
    let mut custom = draft.is_some();
    let enable = ui.checkbox(&mut custom, tr("Custom color and roughness"));
    #[cfg(test)]
    super::authoring_smoke::widget_harness::named(ui, &enable, "appearance.enabled");
    targets.push(("appearance.enabled", enable.rect));
    if enable.changed() {
        *draft = custom.then(Default::default);
    }
    if let Some(value) = draft {
        super::workspace::prop_row(ui, tr("Color (sRGB)"), |ui| {
            let swatch = ui.color_edit_button_srgb(&mut value.color_srgb);
            #[cfg(test)]
            super::authoring_smoke::widget_harness::named(ui, &swatch, "appearance.color");
            targets.push(("appearance.color", swatch.rect));
            if ui
                .selectable_label(extras.eyedropper, tr("Pick"))
                .on_hover_text(tr(
                    "Eyedropper: click a piece in the 3D view to take its colour",
                ))
                .clicked()
            {
                extras.eyedropper = !extras.eyedropper;
            }
        });
        if let Some(palette) = extras.palette.as_deref_mut() {
            super::palette_ui::palette_row(ui, palette, &mut value.color_srgb);
        }
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
                    .on_hover_text(tr("sRGB byte (0-255). Apply appearance to commit."));
                #[cfg(test)]
                super::authoring_smoke::widget_harness::named(ui, &response, key);
                targets.push((key, response.rect));
            }
        });
        let roughness = super::workspace::prop_row(ui, tr("Roughness"), |ui| {
            super::workspace::slim(ui, egui::Slider::new(&mut value.roughness, 0.0..=1.0))
        })
        .inner;
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
                .checkbox(&mut back, tr("Back color"))
                .on_hover_text(tr("A different colour on the fabric's back face, as on a lining or a two-tone fabric"))
                .changed()
            {
                value.back_color_srgb = back.then_some(value.color_srgb);
            }
            if let Some(color) = value.back_color_srgb.as_mut() {
                ui.color_edit_button_srgb(color);
            }
        });
        use fashioncraft::garment::appearance::{Weave, WeaveKind};
        super::workspace::prop_row(ui, tr("Weave"), |ui| {
            let label = value
                .weave
                .as_ref()
                .map_or(tr("None"), |w| tr(w.kind.label()));
            egui::ComboBox::from_id_salt("appearance_weave")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(value.weave.is_none(), tr("None"))
                        .clicked()
                    {
                        value.weave = None;
                    }
                    for kind in WeaveKind::ALL {
                        let on = value.weave.as_ref().is_some_and(|w| w.kind == kind);
                        if ui.selectable_label(on, tr(kind.label())).clicked() {
                            let mut w = value.weave.clone().unwrap_or_default();
                            w.kind = kind;
                            value.weave = Some(w);
                        }
                    }
                });
        });
        if let Some(w) = value.weave.as_mut() {
            super::workspace::prop_row(ui, tr("Thread spacing"), |ui| {
                ui.add(
                    egui::DragValue::new(&mut w.spacing_mm)
                        .range(0.05..=20.0)
                        .speed(0.02)
                        .fixed_decimals(2)
                        .suffix(" mm"),
                );
            });
            super::workspace::prop_row(ui, tr("Weave contrast"), |ui| {
                super::workspace::slim(ui, egui::Slider::new(&mut w.contrast, 0.0..=1.0));
            });
            let _: &Weave = w;
        }
        super::workspace::prop_row(ui, tr("Look thickness"), |ui| {
            ui.add(
                egui::DragValue::new(&mut value.render_thickness_mm)
                    .range(0.0..=fashioncraft::garment::appearance::MAX_RENDER_THICKNESS_MM)
                    .speed(0.05)
                    .fixed_decimals(2)
                    .suffix(" mm"),
            )
            .on_hover_text(
                tr("Draw the cloth this thick, with a rim along its outline; the simulation is unchanged"),
            );
        });
        super::workspace::prop_row(ui, tr("Metalness"), |ui| {
            super::workspace::slim(ui, egui::Slider::new(&mut value.metalness, 0.0..=1.0))
                .on_hover_text(tr(
                    "0 for cloth, 1 for bare metal (foil, lamé); a metalness map replaces it",
                ));
        });
        super::workspace::prop_row(ui, tr("Texture"), |ui| {
            if value.texture.is_some() {
                ui.label(extras.texture_name.as_deref().unwrap_or(tr("image")));
            }
            let pick = if value.texture.is_some() {
                tr("Replace…")
            } else {
                tr("Image…")
            };
            extras.pick_texture = ui
                .button(pick)
                .on_hover_text(tr("Repeat an image over the fabric along its grain"))
                .clicked();
            if value.texture.is_some() && ui.small_button(tr("Remove")).clicked() {
                value.texture = None;
            }
        });
        super::workspace::prop_row(ui, tr("Texture set"), |ui| {
            let hover = tr(TEXTURE_SET_HOVER);
            let menu = ui.menu_button(tr("Import…"), |ui| {
                if ui.button(tr("From a folder…")).clicked() {
                    extras.pick_set = Some(true);
                    ui.close();
                }
                if ui.button(tr("From files…")).clicked() {
                    extras.pick_set = Some(false);
                    ui.close();
                }
            });
            menu.response.on_hover_text(hover);
        });
        if !extras.set_names.is_empty() {
            ui.small(tr("Choose the material to use:"));
            ui.horizontal_wrapped(|ui| {
                for (k, name) in extras.set_names.iter().enumerate() {
                    if ui.small_button(name).clicked() {
                        extras.chosen_set = Some(k);
                    }
                }
            });
        }
        if let Some(t) = value.texture.as_mut() {
            super::workspace::prop_row(ui, tr("Repeat"), |ui| {
                ui.add(
                    egui::DragValue::new(&mut t.width_mm)
                        .range(1.0..=5000.)
                        .speed(1.)
                        .fixed_decimals(1)
                        .prefix("W ")
                        .suffix(" mm"),
                );
                ui.add(
                    egui::DragValue::new(&mut t.height_mm)
                        .range(1.0..=5000.)
                        .speed(1.)
                        .fixed_decimals(1)
                        .prefix("H ")
                        .suffix(" mm"),
                );
            });
            super::workspace::prop_row(ui, tr("Angle"), |ui| {
                ui.add(
                    egui::DragValue::new(&mut t.rotation_degrees)
                        .range(-360.0..=360.)
                        .speed(0.5)
                        .suffix("°"),
                );
            });
            super::workspace::prop_row(ui, tr("Relief (mm)"), |ui| {
                super::workspace::slim(
                    ui,
                    egui::Slider::new(
                        &mut t.relief_mm,
                        0.0..=fashioncraft::garment::appearance::MAX_TEXTURE_RELIEF_MM,
                    ),
                )
                .on_hover_text(tr(
                    "Bright parts of the image stand out by this much; 0 is flat",
                ));
            });
            ui.small(tr("The colours tint the image: white shows it as it is."));
            use fashioncraft::garment::appearance::MapSlot;
            for slot in MapSlot::ALL {
                let hover = match slot {
                    MapSlot::Normal => {
                        "An image of the surface's normals over one repeat: red along it, green up it, blue out of the cloth"
                    }
                    MapSlot::Roughness => {
                        "A grey image of the roughness over one repeat: black glossy, white matt"
                    }
                    MapSlot::Metalness => {
                        "A grey image of the metalness over one repeat: black cloth, white metal"
                    }
                    MapSlot::Opacity => {
                        "A grey image of the opacity over one repeat, for lace and mesh: the cloth is cut away where it is dark"
                    }
                    MapSlot::Displacement => {
                        "A grey height image over one repeat, black low and white high, drawn as relief seen in depth"
                    }
                    MapSlot::Occlusion => {
                        "A grey ambient-occlusion image over one repeat: darkens the light from all around in its creases"
                    }
                };
                super::workspace::prop_row(ui, tr(slot.label()), |ui| {
                    let set = t.map(slot).is_some();
                    let pick = if set {
                        tr("Replace…")
                    } else {
                        tr("Image…")
                    };
                    if ui.small_button(pick).on_hover_text(tr(hover)).clicked() {
                        extras.pick_map = Some(slot);
                    }
                    if set && ui.small_button(tr("Remove")).clicked() {
                        *t.map_mut(slot) = None;
                    }
                });
                if t.map(slot).is_none() {
                    continue;
                }
                match slot {
                    MapSlot::Normal => {
                        ui.checkbox(&mut t.normal_directx, tr("DirectX normal map (flip green)"))
                            .on_hover_text(tr(
                                "Tick for maps made the DirectX way (Unreal, some Substance presets); OpenGL maps (Blender, glTF) stay unticked",
                            ));
                    }
                    MapSlot::Roughness => {
                        ui.checkbox(
                            &mut t.roughness_is_glossiness,
                            tr("Glossiness map (inverted)"),
                        )
                        .on_hover_text(tr(
                            "Tick when the map is glossiness: white glossy, black matt",
                        ));
                    }
                    MapSlot::Displacement => {
                        super::workspace::prop_row(ui, tr("Height"), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut t.displacement_mm)
                                    .range(
                                        0.0..=fashioncraft::garment::appearance::MAX_DISPLACEMENT_MM,
                                    )
                                    .speed(0.05)
                                    .fixed_decimals(2)
                                    .suffix(" mm"),
                            )
                            .on_hover_text(tr(
                                "How far the map's white stands above its black; drawn only, the simulation is unchanged",
                            ));
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    let apply = ui.button(tr("Apply appearance"));
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
        let initial = fashioncraft::garment::appearance::FabricAppearance::default();
        let mut state = (Some(initial.clone()), Vec::new());
        let mut draw = |ui: &mut egui::Ui,
                        state: &mut (
            Option<fashioncraft::garment::appearance::FabricAppearance>,
            Vec<Option<fashioncraft::garment::appearance::FabricAppearance>>,
        )| {
            if let Some(value) =
                appearance_controls(ui, &mut state.0, &mut Vec::new(), &mut Default::default())
            {
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
            Option<fashioncraft::garment::appearance::FabricAppearance>,
            Vec<Option<fashioncraft::garment::appearance::FabricAppearance>>,
        )| {
            if let Some(value) =
                appearance_controls(ui, &mut state.0, &mut Vec::new(), &mut Default::default())
            {
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
