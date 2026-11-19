//! Pleats window (2D Pattern › Pleats): lay knife, box or inverted box pleats across a piece
//! along its selected segment, as folded internal lines, optionally tacked shut at the segment.
use super::*;
use fashioncraft::garment::pleats::{DEPTH_RANGE_M, MAX_PLEATS, PleatKind};

pub(super) struct PleatsUi {
    pub open: bool,
    pub count: usize,
    pub depth_mm: f64,
    pub kind: PleatKind,
    pub tack: bool,
}

impl Default for PleatsUi {
    fn default() -> Self {
        Self {
            open: false,
            count: 4,
            depth_mm: 20.,
            kind: PleatKind::Knife,
            tack: true,
        }
    }
}

impl Atelier {
    /// Open the pleats window, for one pleat when `single`.
    pub(super) fn open_pleats(&mut self, single: bool) {
        let ui = &mut self.authoring.pleats;
        ui.open = true;
        if single {
            ui.count = 1;
        }
    }

    pub(super) fn pleats_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.pleats.open {
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        let target = self
            .authoring
            .edge
            .as_ref()
            .and_then(|e| Some((e.panel.clone(), e.edge.clone()?)));
        let mut open = true;
        let mut lay = false;
        egui::Window::new(t("Pleats"))
            .open(&mut open)
            .default_width(280.)
            .show(ctx, |ui| {
                match &target {
                    Some((panel, edge)) => {
                        let name = self
                            .project
                            .template
                            .panels
                            .iter()
                            .find(|p| &p.id == panel)
                            .map_or(panel.as_str(), |p| p.name.as_str());
                        ui.label(format!("{} · {edge}", name));
                    }
                    None => {
                        ui.label(t("Select the segment the pleats start from (a waist, a yoke) on the board"));
                    }
                }
                let pleats = &mut self.authoring.pleats;
                egui::Grid::new("pleats").num_columns(2).show(ui, |ui| {
                    ui.label(t("Number of pleats"));
                    ui.add(egui::DragValue::new(&mut pleats.count).range(1..=MAX_PLEATS));
                    ui.end_row();
                    ui.label(t("Depth"));
                    ui.add(
                        egui::DragValue::new(&mut pleats.depth_mm)
                            .range(DEPTH_RANGE_M.start() * 1000.0..=DEPTH_RANGE_M.end() * 1000.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                    .on_hover_text(t("Each pleat takes up twice its depth of fabric"));
                    ui.end_row();
                    ui.label(t("Kind"));
                    ui.horizontal(|ui| {
                        for (kind, label) in [
                            (PleatKind::Knife, "Knife"),
                            (PleatKind::Box, "Box pleat"),
                            (PleatKind::InvertedBox, "Inverted box"),
                        ] {
                            ui.selectable_value(&mut pleats.kind, kind, t(label));
                        }
                    });
                    ui.end_row();
                });
                ui.checkbox(&mut pleats.tack, t("Close each pleat at the segment"));
                ui.separator();
                lay = ui
                    .add_enabled(target.is_some(), egui::Button::new(t("Lay pleats")))
                    .clicked();
            });
        if lay && let Some((panel, edge)) = target {
            let pleats = &self.authoring.pleats;
            let count = pleats.count;
            let command = fashioncraft::authoring::Command::AddPleats {
                panel,
                edge,
                count,
                depth_m: pleats.depth_mm / 1000.,
                kind: pleats.kind,
                tack: pleats.tack,
            };
            if self.edit(command) {
                self.status = format!("{count} {}", t("pleats laid · simulate to fold them"));
            }
        }
        if !open {
            self.authoring.pleats.open = false;
        }
    }
}
