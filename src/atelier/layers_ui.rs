//! Simulation layers in the Piece inspector: where pieces of different layers touch, only the
//! outer (higher) one gives way, so a jacket settles over a shirt without pressing it in. As
//! in other cloth tools, layers help a layered garment settle and are then reset to 0.
use super::*;
use fashioncraft::{
    authoring::Command,
    garment::template::{MAX_LAYER, MAX_PRESSURE, SHRINK_RANGE},
};

impl Atelier {
    pub(super) fn layer_properties(&mut self, ui: &mut egui::Ui, panel: &str) {
        let ui_lang = self.prefs.language;
        let layers = &self.project.template.panel_layers;
        let layer = layers.get(panel).copied().unwrap_or(0);
        let layered: Vec<String> = layers.keys().cloned().collect();
        let mut wanted = None;
        let mut reset = false;
        let help = i18n::tr(
            ui_lang,
            "Simulation layer. Where pieces of different layers touch, only the higher \
             (outer) one gives way, so it settles over the lower one. 0: no layer. Reset to \
             0 once the garment has settled",
        );
        super::workspace::prop_row_help(ui, i18n::tr(ui_lang, "Layer"), help, |ui| {
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
                    .small_button(i18n::tr(ui_lang, "Reset all layers"))
                    .on_hover_text(format!(
                        "Put the {} layered pieces back on 0",
                        layered.len()
                    ))
                    .clicked()
            {
                reset = true;
            }
        });
        let flipped = self.project.template.flipped_panels.contains(panel);
        let mut flip = false;
        let help = i18n::tr(
            ui_lang,
            "Swap the piece's front and back faces: the fabric's back colour shows on \
             the side that showed its front. Only the look changes",
        );
        super::workspace::prop_row_help(ui, i18n::tr(ui_lang, "Face"), help, |ui| {
            ui.label(if flipped {
                i18n::tr(ui_lang, "Flipped")
            } else {
                i18n::tr(ui_lang, "Normal")
            });
            flip = ui.small_button(i18n::tr(ui_lang, "Flip")).clicked();
        });
        if flip {
            self.edit(Command::FlipNormals {
                panels: vec![panel.to_string()],
            });
        }
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
        let ui_lang = self.prefs.language;
        let t = &self.project.template;
        let pressure = t.panel_pressure.get(panel).copied().unwrap_or(0.);
        let [warp, weft] = t.panel_shrink.get(panel).copied().unwrap_or([100., 100.]);
        let mut command = None;
        if let Some(p) = committed_value(
            ui,
            ("pressure", panel),
            pressure,
            Field {
                label: i18n::tr(ui_lang, "Pressure"),
                hover: i18n::tr(
                    ui_lang,
                    "Pushes the piece along its normal, like air inside a closed garment or a \
                        cushion; negative values pull. A few pascals already outweigh the fabric",
                ),
                range: -MAX_PRESSURE..=MAX_PRESSURE,
                speed: 0.5,
                decimals: 1,
                suffix: " Pa",
                reset: Some(0.),
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
                    label: i18n::tr(ui_lang, name),
                    hover: i18n::tr(
                        ui_lang,
                        "Size the piece draws in to along this grain direction, in percent \
                            of its cut size, as a fabric that shrinks. 100 % is none",
                    ),
                    range: SHRINK_RANGE,
                    speed: 0.2,
                    decimals: 1,
                    suffix: " %",
                    reset: Some(100.),
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

impl Atelier {
    /// Whether a piece is on show in 3D (not isolated away).
    pub(super) fn panel_shown(&self, panel: &str) -> bool {
        self.authoring
            .view3d
            .isolated
            .as_ref()
            .is_none_or(|ids| ids.contains(panel))
    }
    /// Per-triangle flipped front faces for the renderer; `None` when no piece is flipped.
    pub(super) fn flipped_faces(&self) -> Option<Vec<bool>> {
        let flipped = &self.project.template.flipped_panels;
        if flipped.is_empty() {
            return None;
        }
        let g = &self.scene.garment;
        let mut faces = vec![false; g.mesh.triangles.len()];
        for r in &g.panels {
            if flipped.contains(&r.id) {
                faces[r.triangles.clone()].fill(true);
            }
        }
        Some(faces)
    }
}

pub(super) struct Field<'a> {
    pub label: &'a str,
    pub hover: &'a str,
    pub range: std::ops::RangeInclusive<f64>,
    pub speed: f64,
    pub decimals: usize,
    pub suffix: &'a str,
    /// Value of the small reset button shown next to the field, when there is one.
    pub reset: Option<f64>,
}

/// A number field whose new value is returned once, when a drag ends, a value is typed or
/// the reset button is clicked; the in-progress value lives in egui's temporary memory,
/// beside the value it started from, so a change made elsewhere (undo, a tool) shows at once.
pub(super) fn committed_value(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    current: f64,
    field: Field,
) -> Option<f64> {
    let id = egui::Id::new(key);
    let (mut draft, base) = ui.data_mut(|d| *d.get_temp_mut_or(id, (current, current)));
    if base != current {
        draft = current;
    }
    let mut commit = false;
    super::workspace::prop_row(ui, field.label, |ui| {
        let response = ui
            .add(
                egui::DragValue::new(&mut draft)
                    .range(field.range.clone())
                    .speed(field.speed)
                    .fixed_decimals(field.decimals)
                    .suffix(field.suffix),
            )
            .on_hover_text(field.hover);
        commit = response.drag_stopped() || (response.changed() && !response.dragged());
        if let Some(reset) = field.reset
            && current != reset
            && ui
                .small_button(format!("{reset}"))
                .on_hover_text("Back to none")
                .clicked()
        {
            draft = reset;
            commit = true;
        }
    });
    if commit {
        ui.data_mut(|d| d.remove::<(f64, f64)>(id));
        (draft != current).then_some(draft)
    } else {
        ui.data_mut(|d| d.insert_temp(id, (draft, current)));
        None
    }
}
