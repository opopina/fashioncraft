//! Object browser: everything in the scene in one tree. The avatar with its pose, animation
//! and props; the garment's pieces with their fabric and state; the fabrics with how many
//! pieces use them; and the seams, trims, graphics, holds and archived pieces. Picking a piece
//! selects it in both views; picking a fabric selects the pieces cut from it and shows the
//! fabric in the inspector.
use super::*;
use marvelous_cloth::garment::trims::TrimKind;

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
    t: &marvelous_cloth::garment::GarmentTemplate,
    p: &marvelous_cloth::garment::template::Panel,
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
    }
}

impl Atelier {
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
        match pick {
            Some(Pick::Piece(id)) => {
                self.set_authoring_tool(Tool::Edit, false);
                self.cancel_preview();
                self.select_panel(id, ctx.input(|i| i.modifiers.shift));
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
}
