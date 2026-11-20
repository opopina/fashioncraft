//! Elastic and tape on the selected contour segment or internal line. An elastic gathers the
//! line to a share of its cut length; a tape keeps it from stretching. One command each.
use super::*;
use fashioncraft::{
    authoring::Command,
    garment::template::{LineKind, LineSpring},
};

impl Atelier {
    pub(super) fn line_spring_controls(&mut self, ui: &mut egui::Ui, panel: &str, line: &str) {
        let ui_lang = self.prefs.language;
        let current = self
            .project
            .template
            .line_springs
            .iter()
            .find(|s| s.panel == panel && s.line == line)
            .cloned();
        let mut wanted: Option<Option<LineSpring>> = None;
        ui.horizontal_wrapped(|ui| {
            let elastic = current.as_ref().is_some_and(|s| s.kind == LineKind::Elastic);
            let tape = current.as_ref().is_some_and(|s| s.kind == LineKind::Tape);
            let pucker = current.as_ref().is_some_and(|s| s.kind == LineKind::Puckering);
            if ui
                .selectable_label(elastic, i18n::tr(ui_lang, "Elastic"))
                .on_hover_text(i18n::tr(ui_lang, "Gather this line to a share of its cut length, like an elastic band sewn along it"))
                .clicked()
            {
                wanted = Some((!elastic).then(|| LineSpring::elastic(panel, line)));
            }
            if ui
                .selectable_label(tape, i18n::tr(ui_lang, "Tape"))
                .on_hover_text(i18n::tr(ui_lang, "Keep this line from stretching, like a stay tape sewn along it"))
                .clicked()
            {
                wanted = Some((!tape).then(|| LineSpring::tape(panel, line)));
            }
            if ui
                .selectable_label(pucker, i18n::tr(ui_lang, "Puckering"))
                .on_hover_text(i18n::tr(ui_lang, "Draw this line in a little, as a tight seam does, so the cloth beside it puckers"))
                .clicked()
            {
                wanted = Some((!pucker).then(|| LineSpring::puckering(panel, line)));
            }
        });
        if let Some(spring) = current
            .as_ref()
            .filter(|s| matches!(s.kind, LineKind::Elastic | LineKind::Puckering))
        {
            let mut edited = spring.clone();
            let mut commit = false;
            ui.horizontal(|ui| {
                ui.label(i18n::tr(ui_lang, "Ratio"));
                let r = ui.add(
                    egui::DragValue::new(&mut edited.ratio)
                        .range(30.0..=100.0)
                        .speed(0.5)
                        .fixed_decimals(0)
                        .suffix(" %"),
                );
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                ui.label(i18n::tr(ui_lang, "Strength"));
                let r = ui.add(
                    egui::DragValue::new(&mut edited.strength)
                        .range(0.1..=10_000.0)
                        .speed(0.5)
                        .fixed_decimals(1)
                        .suffix(" N"),
                );
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
            });
            if commit && &edited != spring {
                wanted = Some(Some(edited));
            }
        }
        if let Some(spring) = wanted {
            self.edit(Command::SetLineSpring {
                panel: panel.into(),
                line: line.into(),
                spring,
            });
        }
    }
}

impl Atelier {
    /// Fold of the selected internal line: on/off, its angle (180° flat) and strength.
    pub(super) fn line_fold_controls(&mut self, ui: &mut egui::Ui, panel: &str, line: &str) {
        let ui_lang = self.prefs.language;
        use fashioncraft::garment::template::LineFold;
        let current = self
            .project
            .template
            .line_folds
            .iter()
            .find(|f| f.panel == panel && f.line == line)
            .cloned();
        let mut wanted: Option<Option<LineFold>> = None;
        ui.horizontal(|ui| {
            let on = current.is_some();
            if ui
                .selectable_label(on, i18n::tr(ui_lang, "Fold"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Crease the piece along this line at a set angle, like a pressed pleat",
                ))
                .clicked()
            {
                wanted = Some((!on).then(|| LineFold::new(panel, line)));
            }
            if let Some(fold) = &current {
                let mut edited = fold.clone();
                let mut commit = false;
                let r = ui
                    .add(
                        egui::DragValue::new(&mut edited.angle)
                            .range(0.0..=360.0)
                            .speed(1.0)
                            .fixed_decimals(0)
                            .suffix("°"),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "180° is flat; less folds toward the piece's front, more away from it",
                    ));
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                let r = ui
                    .add(
                        egui::DragValue::new(&mut edited.strength)
                            .range(1.0..=1000.0)
                            .speed(0.5)
                            .fixed_decimals(0)
                            .prefix("×"),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Bending stiffness of the crease, times the fabric's",
                    ));
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                if commit && &edited != fold {
                    wanted = Some(Some(edited));
                }
            }
        });
        if let Some(fold) = wanted {
            self.edit(Command::SetLineFold {
                panel: panel.into(),
                line: line.into(),
                fold,
            });
        }
        let mut arrange = None;
        ui.horizontal(|ui| {
            if ui
                .small_button(i18n::tr(ui_lang, "Arrange folded"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Turn one side of the line in 3D to the fold angle (90° without a fold), so the simulation starts folded",
                ))
                .clicked()
            {
                arrange = Some(false);
            }
            if ui
                .small_button(i18n::tr(ui_lang, "Other side"))
                .on_hover_text(i18n::tr(ui_lang, "Fold the other side of the line instead"))
                .clicked()
            {
                arrange = Some(true);
            }
        });
        if let Some(other_side) = arrange {
            self.fold_arrange(panel, line, other_side);
        }
    }
}
