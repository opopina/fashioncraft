//! Simulation layers in the Piece inspector: where pieces of different layers touch, only the
//! outer (higher) one gives way, so a jacket settles over a shirt without pressing it in. As
//! in other cloth tools, layers help a layered garment settle and are then reset to 0.
use super::*;
use marvelous_cloth::{
    authoring::Command,
    garment::template::{MAX_LAYER, MAX_PRESSURE, SHRINK_RANGE},
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

    /// Inflating pressure and shrinkage of the piece. Each value is committed (one undo step)
    /// when a drag ends or a typed value is entered, not on every frame of a drag.
    fn pressure_property(&mut self, ui: &mut egui::Ui, panel: &str) {
        let t = &self.project.template;
        let pressure = t.panel_pressure.get(panel).copied().unwrap_or(0.);
        let [warp, weft] = t.panel_shrink.get(panel).copied().unwrap_or([100., 100.]);
        let mut command = None;
        if let Some(p) = committed_value(
            ui,
            ("pressure", panel),
            pressure,
            Field {
                label: "Pressure",
                hover: "Pushes the piece along its normal, like air inside a closed garment or a \
                        cushion; negative values pull. A few pascals already outweigh the fabric",
                range: -MAX_PRESSURE..=MAX_PRESSURE,
                speed: 0.5,
                decimals: 1,
                suffix: " Pa",
                reset: 0.,
            },
        ) {
            command = Some(Command::SetPressure {
                panels: vec![panel.to_string()],
                pressure: p,
            });
        }
        for (k, name, value) in [(0, "Shrink warp", warp), (1, "Shrink weft", weft)] {
            if let Some(v) = committed_value(
                ui,
                ("shrink", panel, k),
                value,
                Field {
                    label: name,
                    hover: "Size the piece draws in to along this grain direction, in percent \
                            of its cut size, as a fabric that shrinks. 100 % is none",
                    range: SHRINK_RANGE,
                    speed: 0.2,
                    decimals: 1,
                    suffix: " %",
                    reset: 100.,
                },
            ) {
                let [w, f] = if k == 0 { [v, weft] } else { [warp, v] };
                command = Some(Command::SetShrink {
                    panels: vec![panel.to_string()],
                    warp: w,
                    weft: f,
                });
            }
        }
        if let Some(command) = command {
            self.edit(command);
        }
    }
}

struct Field<'a> {
    label: &'a str,
    hover: &'a str,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    decimals: usize,
    suffix: &'a str,
    reset: f64,
}

/// A number field whose new value is returned once, when a drag ends, a value is typed or
/// the reset button is clicked; the in-progress value lives in egui's temporary memory.
fn committed_value(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    current: f64,
    field: Field,
) -> Option<f64> {
    let id = egui::Id::new(key);
    let mut draft = ui.data_mut(|d| *d.get_temp_mut_or(id, current));
    let mut commit = false;
    ui.horizontal(|ui| {
        ui.label(field.label).on_hover_text(field.hover);
        let response = ui.add(
            egui::DragValue::new(&mut draft)
                .range(field.range.clone())
                .speed(field.speed)
                .fixed_decimals(field.decimals)
                .suffix(field.suffix),
        );
        commit = response.drag_stopped() || (response.changed() && !response.dragged());
        if current != field.reset
            && ui
                .small_button(format!("{}", field.reset))
                .on_hover_text("Back to none")
                .clicked()
        {
            draft = field.reset;
            commit = true;
        }
    });
    if commit {
        ui.data_mut(|d| d.remove::<f64>(id));
        (draft != current).then_some(draft)
    } else {
        ui.data_mut(|d| d.insert_temp(id, draft));
        None
    }
}
