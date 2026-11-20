//! Steam tool (Materials › Steam): brush steam onto the pieces on the board to shrink or
//! stretch the cloth there, or take it off; the simulation then pulls the cloth to its new
//! size. Steamed areas show blue where they shrink and orange where they stretch.
use super::*;
use fashioncraft::{
    authoring::{Command, arrange2d},
    garment::steam::{self, PERCENT_RANGE, RADIUS_RANGE_M},
};
use glam::DVec2;

pub(super) struct SteamUi {
    pub open: bool,
    /// Take steam off instead of adding it.
    pub erase: bool,
    /// Size the cloth takes under new steam, percent.
    pub percent: f64,
    pub radius_mm: f64,
    /// Share of the radius at full strength, percent.
    pub hardness_percent: f64,
    /// The stroke being brushed: the piece (index in the template) and its material points.
    stroke: Option<(usize, Vec<DVec2>)>,
}

impl Default for SteamUi {
    fn default() -> Self {
        Self {
            open: false,
            erase: false,
            percent: 85.,
            radius_mm: 30.,
            hardness_percent: 50.,
            stroke: None,
        }
    }
}

const SHRINK_COLOR: Color32 = Color32::from_rgb(70, 150, 255);
const STRETCH_COLOR: Color32 = Color32::from_rgb(255, 150, 60);

impl Atelier {
    pub(super) fn open_steam(&mut self) {
        self.authoring.steam.open = true;
        self.status = i18n::tr(
            self.prefs.language,
            "Brush steam onto a piece on the board, then simulate · right-drag pans",
        )
        .into();
    }

    pub(super) fn steam_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.steam.open {
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        let mut open = true;
        let mut remove: Option<Vec<String>> = None;
        egui::Window::new(t("Steam"))
            .open(&mut open)
            .default_width(260.)
            .show(ctx, |ui| {
                let s = &mut self.authoring.steam;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut s.erase, false, t("Add"));
                    ui.selectable_value(&mut s.erase, true, t("Remove"));
                });
                egui::Grid::new("steam").num_columns(2).show(ui, |ui| {
                    ui.label(t("Size"));
                    ui.add(
                        egui::DragValue::new(&mut s.radius_mm)
                            .range(RADIUS_RANGE_M.start() * 1000.0..=RADIUS_RANGE_M.end() * 1000.0)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                    .on_hover_text(t("Brush radius"));
                    ui.end_row();
                    ui.label(t("Hardness"));
                    ui.add(
                        egui::DragValue::new(&mut s.hardness_percent)
                            .range(0.0..=100.0)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .on_hover_text(t("Share of the brush at full strength; the rest eases off"));
                    ui.end_row();
                    ui.label(t("Shrinkage"));
                    ui.add(
                        egui::DragValue::new(&mut s.percent)
                            .range(PERCENT_RANGE)
                            .speed(0.25)
                            .suffix(" %"),
                    )
                    .on_hover_text(t(
                        "Size the cloth takes under the steam: below 100 % it shrinks, above it stretches",
                    ));
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if let Some(panel) = self.authoring.selected.clone()
                        && ui.button(t("Remove steam on the selected piece")).clicked()
                    {
                        remove = Some(vec![panel]);
                    }
                    if ui.button(t("Remove all steam")).clicked() {
                        remove = Some(vec![]);
                    }
                });
            });
        if let Some(panels) = remove
            && self.edit(Command::RemoveSteam {
                panels,
                eraser: None,
            })
        {
            self.status = i18n::tr(ui_lang, "Steam removed · simulate to settle").into();
        }
        if !open {
            self.authoring.steam.open = false;
            self.authoring.steam.stroke = None;
        }
    }

    /// The piece under board point `at`, as its index in the template.
    fn steam_piece_at(&self, at: DVec2) -> Option<usize> {
        (0..self.project.template.panels.len()).rev().find(|&i| {
            self.board_outline(i)
                .is_some_and(|outline| super::board_measure::contains(&outline, at))
        })
    }

    /// Brushing on the board while the steam tool is open. Takes the left button.
    pub(super) fn steam_interaction(
        &mut self,
        rect: egui::Rect,
        response: &egui::Response,
    ) -> bool {
        if !self.authoring.steam.open {
            return false;
        }
        let template = &self.project.template;
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let at = self.authoring.board.world(rect, pos);
            self.authoring.steam.stroke = self
                .steam_piece_at(at)
                .map(|i| (i, vec![arrange2d::material_point(template, i, at)]));
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some((i, points)) = &mut self.authoring.steam.stroke
        {
            let at = self.authoring.board.world(rect, pos);
            points.push(arrange2d::material_point(template, *i, at));
        }
        let clicked = response.clicked_by(egui::PointerButton::Primary);
        if clicked && let Some(pos) = response.interact_pointer_pos() {
            let at = self.authoring.board.world(rect, pos);
            self.authoring.steam.stroke = self
                .steam_piece_at(at)
                .map(|i| (i, vec![arrange2d::material_point(template, i, at)]));
        }
        if (clicked || response.drag_stopped_by(egui::PointerButton::Primary))
            && let Some((i, points)) = self.authoring.steam.stroke.take()
        {
            let s = &self.authoring.steam;
            let panel = self.project.template.panels[i].id.clone();
            let radius = s.radius_mm / 1000.;
            let command = if s.erase {
                Command::RemoveSteam {
                    panels: vec![],
                    eraser: Some((panel, points, radius)),
                }
            } else {
                Command::AddSteam {
                    stamps: steam::stroke(
                        &panel,
                        &points,
                        radius,
                        s.hardness_percent / 100.,
                        s.percent,
                    ),
                }
            };
            if self.edit(command) {
                self.status = i18n::tr(
                    self.prefs.language,
                    "Steamed · simulate to see the cloth take its new size",
                )
                .into();
            }
        }
        true
    }

    /// The steamed areas and the brush, while the steam tool is open.
    pub(super) fn paint_steam(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        hover: Option<egui::Pos2>,
    ) {
        if !self.authoring.steam.open {
            return;
        }
        let template = &self.project.template;
        let px = self.authoring.board.pixels_per_metre();
        for stamp in &template.steam {
            let Some(i) = template.panels.iter().position(|p| p.id == stamp.panel) else {
                continue;
            };
            let at = arrange2d::board_point(template, i, stamp.center);
            let color = if stamp.percent < 100. {
                SHRINK_COLOR
            } else {
                STRETCH_COLOR
            };
            painter.circle_filled(
                self.authoring.board.screen(rect, at),
                (stamp.radius_m * px) as f32,
                color.gamma_multiply(0.12),
            );
        }
        if let Some(pos) = hover {
            let s = &self.authoring.steam;
            let color = if s.erase {
                Color32::WHITE
            } else if s.percent < 100. {
                SHRINK_COLOR
            } else {
                STRETCH_COLOR
            };
            painter.circle_stroke(
                pos,
                (s.radius_mm / 1000. * px) as f32,
                Stroke::new(1.5, color),
            );
        }
    }
}
