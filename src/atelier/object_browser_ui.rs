//! Object browser: everything in the scene in one tree. The avatar with its pose, animation
//! and props; the garment's pieces with their fabric and state; the fabrics with how many
//! pieces use them; and the seams, trims, graphics, holds and archived pieces. Picking a piece
//! selects it in both views; picking a fabric selects the pieces cut from it and shows the
//! fabric in the inspector.
use super::*;
use fashioncraft::garment::trims::TrimKind;

#[derive(Default)]
pub(super) struct ObjectBrowserUi {
    pub open: bool,
}

enum Pick {
    Piece(String),
    Fabric(String),
}

/// One row of the piece list: its name, fabric and anything unusual about its state.
fn piece_row(
    t: &fashioncraft::garment::GarmentTemplate,
    p: &fashioncraft::garment::template::Panel,
    ui_lang: i18n::Language,
) -> String {
    let fabric = t
        .fabrics
        .iter()
        .find(|f| f.id == p.fabric)
        .map_or(p.fabric.as_str(), |f| f.name.as_str());
    let mut row = format!("{} · {fabric}", p.name);
    let mut flag = |on: bool, text: &'static str| {
        if on {
            row.push_str(" · ");
            row.push_str(i18n::tr(ui_lang, text));
        }
    };
    flag(t.frozen_panels.iter().any(|f| f.panel == p.id), "frozen");
    flag(
        t.deactivated_panels.iter().any(|d| d.panel == p.id),
        "deactivated",
    );
    flag(
        t.solidified_panels.iter().any(|s| s.panel == p.id),
        "solidified",
    );
    flag(t.strengthened_panels.contains(&p.id), "strengthened");
    flag(t.locked_panels.contains(&p.id), "locked in 2D");
    flag(t.flipped_panels.contains(&p.id), "flipped");
    if let Some(layer) = t.panel_layers.get(&p.id).filter(|l| **l > 0) {
        row.push_str(&format!(" · {} {layer}", i18n::tr(ui_lang, "layer")));
    }
    row
}

fn trim_kind(kind: &TrimKind) -> &'static str {
    match kind {
        TrimKind::Button { .. } => "Button",
        TrimKind::Buttonhole { .. } => "Buttonhole",
        TrimKind::Zipper { .. } => "Zipper",
        TrimKind::Topstitch { .. } => "Topstitch",
        TrimKind::Piping { .. } => "Piping",
        TrimKind::Object { .. } => "Object",
    }
}

/// What the docked object browser lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum BrowserTab {
    #[default]
    Pieces,
    Fabrics,
    Trims,
    Graphics,
    Avatar,
}

/// A fabric's colour as its swatch shows it: its custom colour, else undyed cloth.
pub(super) fn fabric_swatch(
    t: &fashioncraft::garment::GarmentTemplate,
    fabric: &str,
) -> egui::Color32 {
    t.fabric_appearance
        .get(fabric)
        .map(|a| egui::Color32::from_rgb(a.color_srgb[0], a.color_srgb[1], a.color_srgb[2]))
        .unwrap_or(egui::Color32::from_rgb(0xe9, 0xe5, 0xdc))
}

impl Atelier {
    /// The object browser docked at the top of the properties panel, as desktop cloth tools
    /// have it: icon tabs for the pieces, fabrics, trims, graphics and the avatar, each a
    /// compact list. Picking a piece selects it in both views; picking a fabric selects the
    /// pieces cut from it and opens its properties.
    pub(super) fn object_browser(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let tab_id = egui::Id::new("object-browser-tab");
        let mut tab: BrowserTab = ui.data(|d| d.get_temp(tab_id)).unwrap_or_default();
        let tabs = [
            (BrowserTab::Pieces, icons::Icon::Piece, "Pieces"),
            (BrowserTab::Fabrics, icons::Icon::Fabric, "Fabrics"),
            (BrowserTab::Trims, icons::Icon::Button, "Trims"),
            (BrowserTab::Graphics, icons::Icon::Graphic, "Graphics"),
            (BrowserTab::Avatar, icons::Icon::Avatar, "Avatar"),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            let width = (ui.available_width() / tabs.len() as f32).max(28.);
            for (value, icon, label) in tabs {
                super::workspace::icon_tab(
                    ui,
                    &mut tab,
                    value,
                    icon,
                    i18n::tr(ui_lang, label),
                    width,
                );
            }
        });
        ui.data_mut(|d| d.insert_temp(tab_id, tab));
        ui.separator();
        let t = &self.project.template;
        let count = match tab {
            BrowserTab::Pieces => t.panels.len(),
            BrowserTab::Fabrics => t.fabrics.len(),
            BrowserTab::Trims => t.trims.len(),
            BrowserTab::Graphics => t.graphics.len(),
            BrowserTab::Avatar => 1 + self.project.scene_objects.len(),
        };
        let title = tabs
            .iter()
            .find(|(v, ..)| *v == tab)
            .map_or("", |(.., l)| l);
        ui.horizontal(|ui| {
            ui.add_space(4.);
            ui.label(egui::RichText::new(i18n::tr(ui_lang, title)).strong());
            ui.label(egui::RichText::new(count.to_string()).weak());
        });
        let selected = self.selected_panels();
        let piece_name = |id: &str| {
            t.panels
                .iter()
                .find(|p| p.id == id)
                .map_or(id.to_string(), |p| p.name.clone())
        };
        let mut pick = None;
        // The fabric last picked in the Fabrics tab: what Copy and Assign act on.
        let fabric_id = egui::Id::new("object-browser-fabric");
        let current_fabric: Option<String> = ui
            .data(|d| d.get_temp::<String>(fabric_id))
            .filter(|id| t.fabrics.iter().any(|f| &f.id == id));
        let mut chose_fabric = None;
        egui::ScrollArea::vertical()
            .id_salt(("object-browser-list", tab))
            .max_height(156.)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.;
                match tab {
                    BrowserTab::Pieces => {
                        for p in &t.panels {
                            let fabric = t
                                .fabrics
                                .iter()
                                .find(|f| f.id == p.fabric)
                                .map_or(p.fabric.as_str(), |f| f.name.as_str());
                            let row = super::workspace::list_row(
                                ui,
                                Some(fabric_swatch(t, &p.fabric)),
                                &p.name,
                                fabric,
                                selected.contains(&p.id),
                            )
                            .on_hover_text(piece_row(t, p, ui_lang));
                            if row.clicked() {
                                pick = Some(Pick::Piece(p.id.clone()));
                            }
                        }
                    }
                    BrowserTab::Fabrics => {
                        for f in &t.fabrics {
                            let used = t.panels.iter().filter(|p| p.fabric == f.id).count();
                            let chosen = current_fabric.as_deref() == Some(f.id.as_str())
                                || (used > 0
                                    && t.panels
                                        .iter()
                                        .filter(|p| p.fabric == f.id)
                                        .all(|p| selected.contains(&p.id)));
                            let row = super::workspace::list_row(
                                ui,
                                Some(fabric_swatch(t, &f.id)),
                                &f.name,
                                &format!("{used} {}", i18n::tr(ui_lang, "pieces")),
                                chosen,
                            )
                            .on_hover_text(i18n::tr(
                                ui_lang,
                                "Select the pieces cut from this fabric",
                            ));
                            if row.clicked() {
                                chose_fabric = Some(f.id.clone());
                                if used > 0 {
                                    pick = Some(Pick::Fabric(f.id.clone()));
                                }
                            }
                        }
                    }
                    BrowserTab::Trims => {
                        if t.trims.is_empty() {
                            ui.small(i18n::tr(
                                ui_lang,
                                "No trims yet: add them from the 2D tool row.",
                            ));
                        }
                        for trim in &t.trims {
                            let row = super::workspace::list_row(
                                ui,
                                None,
                                i18n::tr(ui_lang, trim_kind(&trim.kind)),
                                &piece_name(&trim.panel_id),
                                false,
                            );
                            if row.clicked() {
                                pick = Some(Pick::Piece(trim.panel_id.clone()));
                            }
                        }
                    }
                    BrowserTab::Graphics => {
                        if t.graphics.is_empty() {
                            ui.small(i18n::tr(ui_lang, "No graphics yet."));
                        }
                        for g in &t.graphics {
                            let row = super::workspace::list_row(
                                ui,
                                None,
                                &g.asset_id,
                                &piece_name(&g.panel_id),
                                false,
                            );
                            if row.clicked() {
                                pick = Some(Pick::Piece(g.panel_id.clone()));
                            }
                        }
                    }
                    BrowserTab::Avatar => {
                        let avatar = self
                            .project
                            .avatar_path
                            .file_stem()
                            .map_or("avatar".into(), |s| s.to_string_lossy());
                        let posed = self
                            .project
                            .avatar_pose
                            .as_ref()
                            .is_some_and(|p| !p.pose.is_empty());
                        super::workspace::list_row(
                            ui,
                            None,
                            avatar.as_ref(),
                            i18n::tr(ui_lang, if posed { "Posed" } else { "Rest pose" }),
                            false,
                        );
                        for object in &self.project.scene_objects {
                            super::workspace::list_row(
                                ui,
                                None,
                                &object.name,
                                i18n::tr(ui_lang, "Object"),
                                false,
                            );
                        }
                    }
                }
            });
        if let Some(id) = &chose_fabric {
            ui.data_mut(|d| d.insert_temp(fabric_id, id.clone()));
        }
        let current_fabric = chose_fabric.or(current_fabric);
        let shift = ui.input(|i| i.modifiers.shift);
        self.pick_object(pick, shift);
        if tab == BrowserTab::Pieces {
            self.archive_list(ui);
        }
        if tab == BrowserTab::Fabrics {
            self.fabric_actions(ui, current_fabric);
        }
    }

    /// Copy the chosen fabric, or cut the selected pieces from it.
    fn fabric_actions(&mut self, ui: &mut egui::Ui, fabric: Option<String>) {
        let ui_lang = self.prefs.language;
        let pieces = self.selected_panels();
        let mut command = None;
        ui.horizontal(|ui| {
            ui.add_space(4.);
            if ui
                .add_enabled(
                    fabric.is_some(),
                    egui::Button::new(i18n::tr(ui_lang, "Copy")),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "A new fabric with the same physics and look, cut from no piece yet",
                ))
                .on_disabled_hover_text(i18n::tr(ui_lang, "Pick a fabric in the list"))
                .clicked()
                && let Some(fabric) = &fabric
            {
                command = Some(fashioncraft::authoring::Command::DuplicateFabric {
                    fabric: fabric.clone(),
                });
            }
            let assign = ui
                .add_enabled(
                    fabric.is_some() && !pieces.is_empty(),
                    egui::Button::new(i18n::tr(ui_lang, "Assign")),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Cut the selected pieces from the chosen fabric",
                ))
                .on_disabled_hover_text(i18n::tr(
                    ui_lang,
                    "Pick a fabric in the list and select the pieces to cut from it",
                ));
            if assign.clicked()
                && let Some(fabric) = &fabric
            {
                command = Some(fashioncraft::authoring::Command::AssignFabric {
                    panels: pieces.to_vec(),
                    fabric: fabric.clone(),
                });
            }
        });
        if let Some(command) = command {
            self.edit(command);
        }
    }

    fn pick_object(&mut self, pick: Option<Pick>, shift: bool) {
        match pick {
            Some(Pick::Piece(id)) => {
                self.set_authoring_tool(Tool::Edit, false);
                self.cancel_preview();
                self.select_panel(id, shift);
                self.authoring.edge = None;
            }
            Some(Pick::Fabric(fabric)) => {
                let ids: Vec<String> = self
                    .project
                    .template
                    .panels
                    .iter()
                    .filter(|p| p.fabric == fabric)
                    .map(|p| p.id.clone())
                    .collect();
                self.set_authoring_tool(Tool::Edit, false);
                self.cancel_preview();
                for (k, id) in ids.into_iter().enumerate() {
                    self.select_panel(id, k > 0);
                }
                self.authoring.edge = None;
                self.inspector_tab = super::chrome::InspectorTab::Fabric;
            }
            None => {}
        }
    }

    pub(super) fn object_browser_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.object_browser.open {
            return;
        }
        let selected = self.selected_panels();
        let t = &self.project.template;
        let piece_name = |id: &str| {
            t.panels
                .iter()
                .find(|p| p.id == id)
                .map_or(id.to_string(), |p| p.name.clone())
        };
        let mut open = true;
        let mut pick = None;
        egui::Window::new(i18n::tr(ui_lang, "Object browser"))
            .open(&mut open)
            .default_width(300.)
            .default_height(440.)
            .show(ctx, |ui| {
                // A tree, not property groups: no bands behind its branches.
                ui.visuals_mut().collapsing_header_frame = false;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::CollapsingHeader::new(i18n::tr(ui_lang, "Avatar"))
                            .id_salt("objects.avatar")
                            .default_open(true)
                            .show(ui, |ui| {
                                let avatar = self
                                    .project
                                    .avatar_path
                                    .file_stem()
                                    .map_or("avatar".into(), |s| s.to_string_lossy());
                                ui.label(avatar.as_ref());
                                let posed = self
                                    .project
                                    .avatar_pose
                                    .as_ref()
                                    .is_some_and(|p| !p.pose.is_empty());
                                ui.small(i18n::tr(
                                    ui_lang,
                                    if posed { "Posed" } else { "Rest pose" },
                                ));
                                if let Some(animation) = &self.project.avatar_animation {
                                    ui.small(format!(
                                        "{} · {} {}",
                                        i18n::tr(ui_lang, "Animation"),
                                        animation.keys.len(),
                                        i18n::tr(ui_lang, "keys")
                                    ));
                                }
                                for object in &self.project.scene_objects {
                                    ui.label(format!(
                                        "{} · {}",
                                        i18n::tr(ui_lang, "Object"),
                                        object.name
                                    ));
                                }
                            });
                        egui::CollapsingHeader::new(format!(
                            "{} · {}",
                            i18n::tr(ui_lang, "Garment"),
                            t.name
                        ))
                        .id_salt("objects.garment")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::CollapsingHeader::new(format!(
                                "{} ({})",
                                i18n::tr(ui_lang, "Pieces"),
                                t.panels.len()
                            ))
                            .id_salt("objects.pieces")
                            .default_open(true)
                            .show(ui, |ui| {
                                for p in &t.panels {
                                    if ui
                                        .selectable_label(
                                            selected.contains(&p.id),
                                            piece_row(t, p, ui_lang),
                                        )
                                        .clicked()
                                    {
                                        pick = Some(Pick::Piece(p.id.clone()));
                                    }
                                }
                            });
                            egui::CollapsingHeader::new(format!(
                                "{} ({})",
                                i18n::tr(ui_lang, "Fabrics"),
                                t.fabrics.len()
                            ))
                            .id_salt("objects.fabrics")
                            .default_open(true)
                            .show(ui, |ui| {
                                for f in &t.fabrics {
                                    let used = t.panels.iter().filter(|p| p.fabric == f.id).count();
                                    if ui
                                        .selectable_label(
                                            false,
                                            format!(
                                                "{} · {used} {}",
                                                f.name,
                                                i18n::tr(ui_lang, "pieces")
                                            ),
                                        )
                                        .on_hover_text(i18n::tr(
                                            ui_lang,
                                            "Select the pieces cut from this fabric",
                                        ))
                                        .clicked()
                                        && used > 0
                                    {
                                        pick = Some(Pick::Fabric(f.id.clone()));
                                    }
                                }
                            });
                            ui.label(format!(
                                "{} ({})",
                                i18n::tr(ui_lang, "Seams"),
                                t.seams.len()
                            ));
                            egui::CollapsingHeader::new(format!(
                                "{} ({})",
                                i18n::tr(ui_lang, "Trims"),
                                t.trims.len()
                            ))
                            .id_salt("objects.trims")
                            .show(ui, |ui| {
                                for trim in &t.trims {
                                    ui.label(format!(
                                        "{} · {}",
                                        i18n::tr(ui_lang, trim_kind(&trim.kind)),
                                        piece_name(&trim.panel_id)
                                    ));
                                }
                            });
                            egui::CollapsingHeader::new(format!(
                                "{} ({})",
                                i18n::tr(ui_lang, "Graphics"),
                                t.graphics.len()
                            ))
                            .id_salt("objects.graphics")
                            .show(ui, |ui| {
                                for g in &t.graphics {
                                    ui.label(format!(
                                        "{} · {}",
                                        g.asset_id,
                                        piece_name(&g.panel_id)
                                    ));
                                }
                            });
                            let holds = [
                                ("Pins", t.pin_targets.len()),
                                ("Tacks", t.tacks.len()),
                                ("Internal lines with folds", t.line_folds.len()),
                                ("Elastic lines", t.line_springs.len()),
                                (
                                    "Archived pieces",
                                    t.archive.iter().map(|a| a.ids.len()).sum(),
                                ),
                            ];
                            for (name, count) in holds {
                                if count > 0 {
                                    ui.label(format!("{} ({count})", i18n::tr(ui_lang, name)));
                                }
                            }
                        });
                    });
            });
        self.authoring.object_browser.open &= open;
        let shift = ctx.input(|i| i.modifiers.shift);
        self.pick_object(pick, shift);
    }
}
