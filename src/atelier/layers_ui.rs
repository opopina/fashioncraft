//! Simulation layers in the Piece inspector: where pieces of different layers touch, only the
//! outer (higher) one gives way, so a jacket settles over a shirt without pressing it in. As
//! in other cloth tools, layers help a layered garment settle and are then reset to 0.
use super::*;
use marvelous_cloth::{authoring::Command, garment::template::MAX_LAYER};

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
}
