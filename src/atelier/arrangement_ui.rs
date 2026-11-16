//! Arrange on body: place the active panel on an authored anchor of the current avatar
//! (EXO-AV-002 prototype). Rigid and flat; rest geometry is untouched. A placement is only
//! committed after the existing VBD initialization gate accepts the resulting state.
use super::*;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::diagnostics,
    garment::{
        arrangement::{self, Sidecar},
        template::Panel,
    },
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct ArrangementUi {
    anchor: Option<String>,
    clearance_mm: f64,
    report: Option<Result<String, String>>,
    loaded: Option<(PathBuf, Result<Option<Sidecar>, String>)>,
}

impl Atelier {
    pub(super) fn arrangement_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        let path = self.project.avatar_path.clone();
        if self.authoring.arrangement.loaded.as_ref().map(|(p, _)| p) != Some(&path) {
            self.authoring.arrangement.loaded =
                Some((path.clone(), arrangement::load_sidecar(&path)));
            self.authoring.arrangement.anchor = None;
        }
        egui::CollapsingHeader::new("Arrange on body")
            .id_salt("arrange-on-body")
            .default_open(true)
            .show(ui, |ui| {
                let sidecar = match &self.authoring.arrangement.loaded {
                    Some((_, Ok(Some(s)))) => s.clone(),
                    Some((_, Ok(None))) => {
                        ui.small("This avatar ships no arrangement anchors (arrangement.json next to its OBJ).");
                        return;
                    }
                    Some((_, Err(e))) => {
                        ui.colored_label(Color32::LIGHT_RED, format!("Anchors not loaded: {e}"));
                        return;
                    }
                    None => return,
                };
                let anchors: Vec<(String, String, f64)> = sidecar.set.anchors.iter().map(|a| (a.id.clone(), a.label.clone(), a.clearance_m)).collect();
                if self.authoring.arrangement.anchor.as_ref().is_none_or(|id| !anchors.iter().any(|a| &a.0 == id))
                    && let Some(first) = anchors.first()
                {
                    self.authoring.arrangement.anchor = Some(first.0.clone());
                    self.authoring.arrangement.clearance_mm = first.2 * 1000.0;
                }
                let current = self.authoring.arrangement.anchor.clone().unwrap_or_default();
                let label = anchors.iter().find(|a| a.0 == current).map(|a| a.1.clone()).unwrap_or_default();
                egui::ComboBox::from_label("Anchor").selected_text(&label).show_ui(ui, |ui| {
                    for (id, name, clearance) in &anchors {
                        if ui.selectable_label(id == &current, name).clicked() {
                            self.authoring.arrangement.anchor = Some(id.clone());
                            self.authoring.arrangement.clearance_mm = clearance * 1000.0;
                            self.authoring.arrangement.report = None;
                        }
                    }
                });
                ui.add(egui::DragValue::new(&mut self.authoring.arrangement.clearance_mm).range(0.0..=600.0).speed(1.0).suffix(" mm"))
                    .on_hover_text("Distance from the anchor along its outward direction");
                let placement = || -> Result<_, String> {
                    let frame = sidecar.frame(&current)?;
                    arrangement::placement_for(&arrangement::rest_outline(panel), &frame, self.authoring.arrangement.clearance_mm / 1000.0)
                };
                let placement = placement();
                ui.horizontal(|ui| {
                    if ui.button("Preview").on_hover_text("Show the panel on the anchor without changing the document").clicked() {
                        match &placement {
                            Ok(p) => {
                                self.preview_placement(p.clone());
                                self.authoring.arrangement.report = None;
                            }
                            Err(e) => self.authoring.arrangement.report = Some(Err(e.clone())),
                        }
                    }
                    if ui.button("Place").on_hover_text("Check initialization, then place (one undo step)").clicked() {
                        self.authoring.arrangement.report = Some(self.place_on_anchor(&panel.id, &label, placement.clone()));
                    }
                    if self.authoring.preview.is_some() && ui.button("Cancel").clicked() {
                        self.cancel_preview();
                    }
                });
                match &self.authoring.arrangement.report {
                    Some(Ok(msg)) => {
                        ui.small(msg);
                    }
                    Some(Err(msg)) => {
                        ui.colored_label(Color32::LIGHT_RED, msg);
                    }
                    None => {}
                }
            });
    }

    fn place_on_anchor(
        &mut self,
        panel: &str,
        label: &str,
        placement: Result<marvelous_cloth::garment::template::Placement, String>,
    ) -> Result<String, String> {
        let placement = placement?;
        self.cancel_preview();
        self.playing = false;
        let command = |placement: marvelous_cloth::garment::template::Placement| Command::Place {
            panel: panel.to_string(),
            placement,
        };
        let edit = authoring::apply(&self.project.template, command(placement.clone()))?;
        let state = edit.state_after(&self.scene.garment, &self.scene.state);
        diagnostics::check_vbd_initialization(Arc::new(edit.garment), &self.scene.collider.surface, &state)
            .map_err(|e| format!("Not placed: the panel would start intersecting ({e}). Increase the clearance or choose another anchor."))?;
        if !self.edit(command(placement)) {
            return Err(self.status.clone());
        }
        Ok(format!(
            "Placed on “{label}” at {:.0} mm. Initialization check passed.",
            self.authoring.arrangement.clearance_mm
        ))
    }
}
