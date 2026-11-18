//! Elastic and tape on the selected contour segment or internal line. An elastic gathers the
//! line to a share of its cut length; a tape keeps it from stretching. One command each.
use super::*;
use marvelous_cloth::{
    authoring::Command,
    garment::template::{LineKind, LineSpring},
};

impl Atelier {
    pub(super) fn line_spring_controls(&mut self, ui: &mut egui::Ui, panel: &str, line: &str) {
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
            if ui
                .selectable_label(elastic, "Elastic")
                .on_hover_text("Gather this line to a share of its cut length, like an elastic band sewn along it")
                .clicked()
            {
                wanted = Some((!elastic).then(|| LineSpring::elastic(panel, line)));
            }
            if ui
                .selectable_label(tape, "Tape")
                .on_hover_text("Keep this line from stretching, like a stay tape sewn along it")
                .clicked()
            {
                wanted = Some((!tape).then(|| LineSpring::tape(panel, line)));
            }
        });
        if let Some(spring) = current.as_ref().filter(|s| s.kind == LineKind::Elastic) {
            let mut edited = spring.clone();
            let mut commit = false;
            ui.horizontal(|ui| {
                ui.label("Ratio");
                let r = ui.add(
                    egui::DragValue::new(&mut edited.ratio)
                        .range(30.0..=100.0)
                        .speed(0.5)
                        .fixed_decimals(0)
                        .suffix(" %"),
                );
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                ui.label("Strength");
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
