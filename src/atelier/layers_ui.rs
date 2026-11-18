//! Simulation layers in the Piece inspector: where pieces of different layers touch, only the
//! outer (higher) one gives way, so a jacket settles over a shirt without pressing it in. As
//! in other cloth tools, layers help a layered garment settle and are then reset to 0.
use super::*;
use marvelous_cloth::{
    authoring::Command,
    garment::template::{MAX_LAYER, MAX_PRESSURE},
};

impl Atelier {
    pub(super) fn layer_properties(&mut self, ui: &mut egui::Ui, panel: &str) {
        let layers = &self.project.template.panel_layers;
        let layer = layers.get(panel).copied().unwrap_or(0);
        let layered: Vec<String> = layers.keys().cloned().collect();
        let mut wanted = None;
        let mut reset = false;
        ui.horizontal(|ui| {
            ui.label("Layer").on_hover_text(
                "Simulation layer. Where pieces of different layers touch, only the higher \
                 (outer) one gives way, so it settles over the lower one. 0: no layer. Reset to \
                 0 once the garment has settled",
            );
            if ui
                .add_enabled(layer > 0, egui::Button::new("−").small())
                .clicked()
            {
                wanted = Some(layer - 1);
            }
            ui.monospace(layer.to_string());
            if ui
                .add_enabled(layer < MAX_LAYER, egui::Button::new("+").small())
                .clicked()
            {
                wanted = Some(layer + 1);
            }
            if !layered.is_empty()
                && ui
                    .small_button("Reset all layers")
                    .on_hover_text(format!(
                        "Put the {} layered pieces back on 0",
                        layered.len()
                    ))
                    .clicked()
            {
                reset = true;
            }
        });
        self.pressure_property(ui, panel);
        if let Some(layer) = wanted {
            self.edit(Command::SetLayer {
                panels: vec![panel.to_string()],
                layer,
            });
        } else if reset {
            self.edit(Command::SetLayer {
                panels: layered,
                layer: 0,
            });
        }
    }

    /// Inflating pressure of the piece, Pa. The value is committed (one undo step) when a drag
    /// ends or a typed value is entered, not on every frame of a drag.
    fn pressure_property(&mut self, ui: &mut egui::Ui, panel: &str) {
        let current = self
            .project
            .template
            .panel_pressure
            .get(panel)
            .copied()
            .unwrap_or(0.);
        let id = egui::Id::new(("pressure_draft", panel));
        let mut draft = ui.data_mut(|d| *d.get_temp_mut_or(id, current));
        let mut commit = false;
        ui.horizontal(|ui| {
            ui.label("Pressure").on_hover_text(
                "Pushes the piece along its normal, like air inside a closed garment or a \
                 cushion; negative values pull. A few pascals already outweigh the fabric",
            );
            let response = ui.add(
                egui::DragValue::new(&mut draft)
                    .range(-MAX_PRESSURE..=MAX_PRESSURE)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .suffix(" Pa"),
            );
            commit = response.drag_stopped() || (response.changed() && !response.dragged());
            if current != 0. && ui.small_button("0").on_hover_text("No pressure").clicked() {
                draft = 0.;
                commit = true;
            }
        });
        if commit && draft != current {
            self.edit(Command::SetPressure {
                panels: vec![panel.to_string()],
                pressure: draft,
            });
            ui.data_mut(|d| d.remove::<f64>(id));
        } else if !commit {
            ui.data_mut(|d| d.insert_temp(id, draft));
        }
        if commit {
            ui.data_mut(|d| d.remove::<f64>(id));
        }
    }
}
