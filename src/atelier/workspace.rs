//! Workspace presentation; no physical state or file-format promises live here.
use super::*;
use std::sync::OnceLock;

/// Workspace accent: a warm amber that reads on the dark chrome, on the neutral pattern
/// board and on the light variant; selected things, active tools and section titles carry it.
pub(super) const ACCENT: Color32 = Color32::from_rgb(0xF2, 0xA1, 0x3A);
/// Text drawn on the accent.
const ON_ACCENT: Color32 = Color32::from_rgb(0x1B, 0x1B, 0x1F);

/// A section title in an inspector or window: the accent, bold.
pub(super) fn section(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).strong().color(ACCENT)
}

/// Pristine egui base styles, captured before the first install. Deriving each
/// theme from its own base keeps switching Light -> Dark -> Light idempotent
/// instead of letting the light palette inherit the dark overrides.
fn bases(ctx: &egui::Context) -> &'static (egui::Style, egui::Style) {
    static BASES: OnceLock<(egui::Style, egui::Style)> = OnceLock::new();
    BASES.get_or_init(|| {
        (
            (*ctx.style_of(egui::Theme::Light)).clone(),
            (*ctx.style_of(egui::Theme::Dark)).clone(),
        )
    })
}

/// Theme-independent geometry and text sizes.
fn geometry(style: &mut egui::Style) {
    style.spacing.item_spacing = Vec2::new(8., 6.);
    style.spacing.button_padding = Vec2::new(10., 6.);
    style.spacing.interact_size.y = 28.;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.));
}

/// The reviewed light palette. These values are unchanged from the approved pass.
fn light_visuals(visuals: &mut egui::Visuals) {
    visuals.panel_fill = Color32::from_rgb(0xEC, 0xEC, 0xEC);
    visuals.window_fill = Color32::from_rgb(0xF7, 0xF7, 0xF7);
    visuals.faint_bg_color = Color32::from_rgb(0xDD, 0xDD, 0xDD);
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1., ON_ACCENT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0xDD, 0xDD, 0xDD);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xF1, 0xDF, 0xC6);
    visuals.widgets.active.bg_fill = Color32::from_rgb(0xEC, 0xC9, 0x95);
    // Section headers sit on a full-width band, as property groups do.
    visuals.collapsing_header_frame = true;
}

/// Dark counterpart of the light palette, carrying the same accent. Text and
/// stroke colours stay egui's dark defaults so contrast remains readable.
fn dark_visuals(visuals: &mut egui::Visuals) {
    visuals.panel_fill = Color32::from_rgb(0x1B, 0x1B, 0x1F);
    visuals.window_fill = Color32::from_rgb(0x23, 0x23, 0x28);
    visuals.faint_bg_color = Color32::from_rgb(0x2C, 0x2C, 0x32);
    visuals.extreme_bg_color = Color32::from_rgb(0x12, 0x12, 0x15);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1., ON_ACCENT);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x32, 0x32, 0x38);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x4A, 0x40, 0x33);
    visuals.widgets.active.bg_fill = Color32::from_rgb(0x6B, 0x52, 0x2E);
    visuals.collapsing_header_frame = true;
}

fn install(ctx: &egui::Context, dark: bool) {
    let base = bases(ctx);
    let mut style = if dark { base.1.clone() } else { base.0.clone() };
    geometry(&mut style);
    if dark {
        dark_visuals(&mut style.visuals);
    } else {
        light_visuals(&mut style.visuals);
    }
    // Both egui theme slots receive the same workspace look, so the active theme
    // preference cannot pick a half-styled variant.
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Install the reviewed light workspace chrome.
pub(super) fn style(ctx: &egui::Context) {
    install(ctx, false);
}

/// Install the dark workspace chrome. `settings::apply` chooses between the two.
// The only caller is `settings::apply`, which lands with the module declaration.
#[allow(dead_code)]
pub(super) fn dark_style(ctx: &egui::Context) {
    install(ctx, true);
}
impl Atelier {
    pub(super) fn panel_browser(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let mut select = None;
        egui::CollapsingHeader::new(section(format!(
            "{} ({})",
            i18n::tr(ui_lang, "Pieces"),
            self.project.template.panels.len()
        )))
        .id_salt("panel_browser")
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("panel_list")
                .max_height(115.)
                .show(ui, |ui| {
                    for panel in &self.project.template.panels {
                        let name = if self.project.template.locked_panels.contains(&panel.id) {
                            format!("{} · locked in 2D", panel.name)
                        } else {
                            panel.name.clone()
                        };
                        if ui
                            .selectable_label(
                                self.authoring.selected.as_ref() == Some(&panel.id),
                                name,
                            )
                            .on_hover_text(format!("{} · select in both views", panel.id))
                            .clicked()
                        {
                            select = Some(panel.id.clone());
                        }
                    }
                });
        });
        if let Some(id) = select {
            self.set_authoring_tool(Tool::Edit, false);
            self.cancel_preview();
            self.select_panel(id, ui.input(|i| i.modifiers.shift));
            self.authoring.edge = None;
        }
        self.archive_list(ui);
        ui.separator();
    }

    /// Archived pieces, each with Restore and Delete; hidden while the archive is empty.
    fn archive_list(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        if self.project.template.archive.is_empty() {
            return;
        }
        let mut command = None;
        egui::CollapsingHeader::new(section(format!("{} ({})", i18n::tr(ui_lang, "Archived"),
            self.project.template.archive.len()
        )))
        .id_salt("archive_list")
        .show(ui, |ui| {
            for entry in &self.project.template.archive {
                ui.horizontal(|ui| {
                    ui.label(entry.names.join(", "))
                        .on_hover_text(i18n::tr(ui_lang, "Off the board and out of the simulation, kept with the project"));
                    if ui
                        .small_button(i18n::tr(ui_lang, "Restore"))
                        .on_hover_text(i18n::tr(ui_lang, "Bring them back where they were, sewn to the pieces still on the board"))
                        .clicked()
                    {
                        command = Some(marvelous_cloth::authoring::Command::RestoreArchived {
                            id: entry.id.clone(),
                        });
                    }
                    if ui.small_button(i18n::tr(ui_lang, "Delete")).clicked() {
                        command = Some(marvelous_cloth::authoring::Command::DeleteArchived {
                            id: entry.id.clone(),
                        });
                    }
                });
            }
        });
        if let Some(command) = command {
            self.edit(command);
        }
    }
    pub(super) fn export_information(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        ui.collapsing(section(i18n::tr(ui_lang, "Files & formats")), |ui| {
            ui.label(i18n::tr(ui_lang, "Import"));
            ui.small(i18n::tr(ui_lang, "ClothCraft project and template JSON"));
            ui.small(i18n::tr(ui_lang, "Static avatars: OBJ · glTF · GLB · VRM · STL · PLY · COLLADA · FBX"));
            ui.small(i18n::tr(ui_lang, "Pattern outlines: generic ASCII DXF · physical SVG subsets"));
            ui.label(i18n::tr(ui_lang, "Export"));
            ui.small(i18n::tr(ui_lang, "Project and template JSON · garment OBJ (welded, quads, retopology, baked look) · glTF · GLB · FBX · USD · USDZ · Alembic · MDD and PC2 caches · SVG patterns · fiber guides"));
            ui.small(i18n::tr(ui_lang, "Industrial DXF and PDF are not yet supported."));
            ui.separator();
            ui.small(i18n::tr(ui_lang, "Export folder (files replaced on every export):"));
            ui.label(egui::RichText::new(self.output.display().to_string()).small());
        });
    }
}
