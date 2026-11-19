//! Arrange bar above the 2D board (Edit tool): align, rotate and scale the selected contour
//! points; scale a piece; align pieces to the active one and distribute them with a fixed gap.
//! Every action is one `Command`, so one undo step.
use super::*;
use fashioncraft::authoring::{
    Command,
    arrange2d::{self, AlignMode, Axis},
};
use glam::DVec2;

/// Values typed into the arrange bar.
pub(super) struct ArrangeUi {
    rotation_degrees: f64,
    scale_percent: [f64; 2],
    uniform: bool,
    gap_mm: f64,
    /// Height the bar took on the last frame without a board gesture. While a gesture runs
    /// the bar keeps that height, so a selection made by the press cannot move the board
    /// under the pointer.
    height: f32,
}

impl Default for ArrangeUi {
    fn default() -> Self {
        Self {
            rotation_degrees: 0.,
            scale_percent: [100., 100.],
            uniform: true,
            gap_mm: 50.,
            height: 0.,
        }
    }
}

/// Short button labels for the align modes (plain text: the default fonts lack arrow glyphs),
/// with the full name as the tooltip.
fn align_button(mode: AlignMode) -> &'static str {
    match mode {
        AlignMode::Left => "Left",
        AlignMode::Center => "Centre",
        AlignMode::Right => "Right",
        AlignMode::Top => "Top",
        AlignMode::Middle => "Middle",
        AlignMode::Bottom => "Bottom",
        AlignMode::ToPointX => "To X",
        AlignMode::ToPointY => "To Y",
    }
}

impl Atelier {
    /// The selected contour points on the board, grouped by panel.
    fn selected_point_groups(&self) -> Vec<(String, Vec<usize>)> {
        self.authoring
            .board
            .point_selections()
            .into_iter()
            .filter(|(_, points)| !points.is_empty())
            .collect()
    }

    /// Centre of the selected points' board bounds.
    fn point_selection_centre(&self, groups: &[(String, Vec<usize>)]) -> Option<DVec2> {
        let template = &self.project.template;
        let mut points = Vec::new();
        for (id, indices) in groups {
            let index = template.panels.iter().position(|p| &p.id == id)?;
            let poly = template.panels[index].polygon.as_ref()?;
            for k in indices {
                points.push(arrange2d::board_point(
                    template,
                    index,
                    *poly.points.get(*k)?,
                ));
            }
        }
        arrange2d::bounds(&points).map(|(lo, hi)| (lo + hi) * 0.5)
    }

    fn scale_inputs(&mut self, ui: &mut egui::Ui) -> DVec2 {
        let ui_lang = self.prefs.language;
        let state = &mut self.authoring.arrange;
        ui.label(i18n::tr(ui_lang, "Scale"));
        let changed = ui
            .add(
                egui::DragValue::new(&mut state.scale_percent[0])
                    .range(1.0..=10_000.0)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .suffix(" %"),
            )
            .on_hover_text(i18n::tr(ui_lang, "Horizontal scale"))
            .changed();
        if state.uniform {
            if changed {
                state.scale_percent[1] = state.scale_percent[0];
            }
        } else {
            ui.add(
                egui::DragValue::new(&mut state.scale_percent[1])
                    .range(1.0..=10_000.0)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .suffix(" %"),
            )
            .on_hover_text(i18n::tr(ui_lang, "Vertical scale"));
        }
        if ui
            .checkbox(&mut state.uniform, i18n::tr(ui_lang, "Uniform"))
            .on_hover_text(i18n::tr(ui_lang, "Same scale both ways"))
            .changed()
            && state.uniform
        {
            state.scale_percent[1] = state.scale_percent[0];
        }
        DVec2::from_array(state.scale_percent) / 100.
    }

    /// The arrange bar; shown only with the Edit tool and something selected.
    pub(super) fn arrange_toolbar(&mut self, ui: &mut egui::Ui) {
        if self.authoring.tool != Tool::Edit {
            return;
        }
        if self.authoring.board.dragging() {
            // Mid-gesture the bar's height must not change, or the board shifts under the
            // pointer and the dragged points jump. The bar for the new selection appears
            // once the gesture ends.
            let height = self.authoring.arrange.height;
            ui.scope(|ui| ui.allocate_space(egui::vec2(ui.available_width(), height)));
            return;
        }
        let rect = ui
            .scope(|ui| {
                let groups = self.selected_point_groups();
                let count: usize = groups.iter().map(|(_, p)| p.len()).sum();
                if count > 0 {
                    self.point_arrange_bar(ui, groups, count);
                    return;
                }
                let panels = self.selected_panels();
                if !panels.is_empty() {
                    self.panel_arrange_bar(ui, panels);
                }
            })
            .response
            .rect;
        self.authoring.arrange.height = rect.height();
    }

    fn point_arrange_bar(
        &mut self,
        ui: &mut egui::Ui,
        groups: Vec<(String, Vec<usize>)>,
        count: usize,
    ) {
        let ui_lang = self.prefs.language;
        let mut command = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(super::garment_display::counted(count, "point", "points"));
            ui.separator();
            ui.label(i18n::tr(ui_lang, "Align"));
            let reference = self.authoring.anchor.clone();
            for mode in AlignMode::ALL {
                let needs_reference = matches!(mode, AlignMode::ToPointX | AlignMode::ToPointY);
                let enabled = if needs_reference {
                    reference.is_some()
                } else {
                    count >= 2
                };
                let tip = if needs_reference {
                    format!("{} of the focused point", mode.label())
                } else {
                    format!("{} of the selected points", mode.label())
                };
                if ui
                    .add_enabled(enabled, egui::Button::new(align_button(mode)).small())
                    .on_hover_text(tip)
                    .clicked()
                {
                    command = Some(Command::AlignPointGroups {
                        groups: groups.clone(),
                        mode,
                        reference: needs_reference.then(|| reference.clone()).flatten(),
                    });
                }
            }
            ui.separator();
            ui.label(i18n::tr(ui_lang, "Rotate"));
            ui.add(
                egui::DragValue::new(&mut self.authoring.arrange.rotation_degrees)
                    .range(-360.0..=360.0)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .suffix("°"),
            );
            let scale = self.scale_inputs(ui);
            if ui
                .add_enabled(
                    count >= 2,
                    egui::Button::new(i18n::tr(ui_lang, "Transform")),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Rotate and scale the selected points about their centre",
                ))
                .clicked()
                && let Some(pivot) = self.point_selection_centre(&groups)
            {
                command = Some(Command::TransformPointGroups {
                    groups: groups.clone(),
                    pivot,
                    rotation_degrees: self.authoring.arrange.rotation_degrees,
                    scale,
                });
            }
        });
        if let Some(command) = command
            && self.edit(command)
        {
            self.status = i18n::tr(ui_lang, "Points arranged · Ctrl+Z undoes it").into();
        }
    }

    fn panel_arrange_bar(&mut self, ui: &mut egui::Ui, panels: Vec<String>) {
        let ui_lang = self.prefs.language;
        let mut command = None;
        let reference = self
            .authoring
            .selected
            .clone()
            .filter(|id| panels.contains(id));
        ui.horizontal_wrapped(|ui| {
            ui.label(super::garment_display::pieces(panels.len()));
            ui.separator();
            let scale = self.scale_inputs(ui);
            if ui
                .add_enabled(panels.len() == 1, egui::Button::new(i18n::tr(ui_lang, "Scale piece")))
                .on_hover_text(i18n::tr(ui_lang, "Scale the piece, its holes and internal lines about its centre"))
                .clicked()
            {
                command = Some(Command::ScalePanel {
                    panel: panels[0].clone(),
                    scale,
                });
            }
            if panels.len() >= 2 {
                ui.separator();
                let name = reference
                    .as_ref()
                    .and_then(|id| self.project.template.panels.iter().find(|p| &p.id == id))
                    .map_or("the active piece".to_string(), |p| p.name.clone());
                ui.label(i18n::tr(ui_lang, "Align to"))
                    .on_hover_text(format!("Line pieces up with {name}"));
                for mode in AlignMode::ALL.into_iter().take(6) {
                    if ui
                        .add_enabled(
                            reference.is_some(),
                            egui::Button::new(align_button(mode)).small(),
                        )
                        .on_hover_text(format!("{} edge or centre of {name}", mode.label()))
                        .clicked()
                        && let Some(reference) = reference.clone()
                    {
                        command = Some(Command::AlignPanels {
                            panels: panels.clone(),
                            reference,
                            mode,
                        });
                    }
                }
                ui.separator();
                ui.label(i18n::tr(ui_lang, "Distribute"));
                ui.add(
                    egui::DragValue::new(&mut self.authoring.arrange.gap_mm)
                        .range(-1000.0..=1000.0)
                        .speed(1.0)
                        .fixed_decimals(1)
                        .suffix(" mm"),
                )
                .on_hover_text(i18n::tr(ui_lang, "Gap between neighbouring pieces"));
                if ui
                    .small_button(i18n::tr(ui_lang, "Even"))
                    .on_hover_text(i18n::tr(ui_lang, "Use the current average gap, left to right: equal gaps within the same overall width"))
                    .clicked()
                {
                    let template = &self.project.template;
                    let boxes: Option<Vec<_>> = panels
                        .iter()
                        .map(|id| {
                            let i = template.panels.iter().position(|p| &p.id == id)?;
                            arrange2d::board_bounds(template, i).ok()
                        })
                        .collect();
                    if let Some(gap) = boxes.and_then(|b| arrange2d::average_gap(&b, Axis::Horizontal)) {
                        self.authoring.arrange.gap_mm = gap * 1000.;
                    }
                }
                for (axis, label) in [(Axis::Horizontal, "Across"), (Axis::Vertical, "Down")] {
                    if ui
                        .small_button(label)
                        .on_hover_text(match axis {
                            Axis::Horizontal => i18n::tr(ui_lang, "Space the pieces left to right"),
                            Axis::Vertical => i18n::tr(ui_lang, "Space the pieces top to bottom"),
                        })
                        .clicked()
                    {
                        command = Some(Command::DistributePanels {
                            panels: panels.clone(),
                            axis,
                            gap_m: self.authoring.arrange.gap_mm / 1000.,
                        });
                    }
                }
            }
        });
        if let Some(command) = command
            && self.edit(command)
        {
            self.status = i18n::tr(ui_lang, "Pieces arranged · Ctrl+Z undoes it").into();
        }
    }
}
