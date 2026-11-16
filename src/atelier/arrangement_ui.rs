//! Arrange on body: place the active panel on an authored anchor of the current avatar
//! (EXO-AV-002 prototype). The panel sits a chosen distance outside the skin and can wrap around
//! the anchor axis. Rest geometry is untouched. A placement is only committed after the existing
//! VBD initialization gate accepts the resulting state.
use super::*;
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::diagnostics,
    garment::{
        arrangement::{self, Arrangement, Sidecar},
        template::{Panel, PanelWrap, Placement},
    },
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct ArrangementUi {
    anchor: Option<String>,
    clearance_mm: f64,
    wrap: bool,
    report: Option<Result<String, String>>,
    loaded: Option<(PathBuf, Result<Option<Sidecar>, String>)>,
}

struct AnchorChoice {
    id: String,
    label: String,
    clearance_m: f64,
    wrap: bool,
    max_wrap_degrees: f64,
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
                let anchors: Vec<AnchorChoice> = sidecar
                    .set
                    .anchors
                    .iter()
                    .map(|a| AnchorChoice {
                        id: a.id.clone(),
                        label: a.label.clone(),
                        clearance_m: a.clearance_m,
                        wrap: a.wrap,
                        max_wrap_degrees: a.max_wrap_degrees,
                    })
                    .collect();
                let state = &mut self.authoring.arrangement;
                if state.anchor.as_ref().is_none_or(|id| !anchors.iter().any(|a| &a.id == id))
                    && let Some(first) = anchors.first()
                {
                    state.anchor = Some(first.id.clone());
                    state.clearance_mm = first.clearance_m * 1000.0;
                    state.wrap = first.wrap;
                }
                let current = state.anchor.clone().unwrap_or_default();
                let chosen = anchors.iter().find(|a| a.id == current);
                let label = chosen.map(|a| a.label.clone()).unwrap_or_default();
                let max_wrap = chosen.map_or(80.0, |a| a.max_wrap_degrees);
                egui::ComboBox::from_label("Anchor").selected_text(&label).height(320.0).show_ui(ui, |ui| {
                    for a in &anchors {
                        if ui.selectable_label(a.id == current, &a.label).clicked() {
                            state.anchor = Some(a.id.clone());
                            state.clearance_mm = a.clearance_m * 1000.0;
                            state.wrap = a.wrap;
                            state.report = None;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut state.clearance_mm).range(0.0..=600.0).speed(1.0).suffix(" mm"))
                        .on_hover_text("Distance from the skin, measured along the anchor's outward direction");
                    ui.label("from skin");
                });
                ui.checkbox(&mut state.wrap, "Wrap around body")
                    .on_hover_text("Curve the panel around the anchor axis, clearing the body across its width and height");
                let (clearance, wrap) = (state.clearance_mm / 1000.0, state.wrap);
                let arranged = sidecar.frame(&current).and_then(|frame| {
                    arrangement::arrange(
                        &arrangement::rest_outline(panel),
                        &frame,
                        clearance,
                        wrap.then_some(max_wrap),
                        Some(&self.scene.collider.surface),
                    )
                });
                ui.horizontal(|ui| {
                    if ui.button("Preview").on_hover_text("Show the panel on the anchor without changing the document").clicked() {
                        match &arranged {
                            Ok(a) => {
                                self.preview_arrangement(&panel.id, a.placement.clone(), a.wrap);
                                self.authoring.arrangement.report = None;
                            }
                            Err(e) => self.authoring.arrangement.report = Some(Err(e.clone())),
                        }
                    }
                    if ui.button("Place").on_hover_text("Check initialization, then place (one undo step)").clicked() {
                        self.authoring.arrangement.report = Some(self.place_on_anchor(&panel.id, &label, arranged.clone()));
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

    /// Show the arranged panel (wrap included) without changing the document; Cancel restores it.
    fn preview_arrangement(&mut self, panel: &str, placement: Placement, wrap: Option<PanelWrap>) {
        let Some(range) = self
            .scene
            .garment
            .panels
            .iter()
            .find(|p| p.id == panel)
            .cloned()
        else {
            return;
        };
        if self.authoring.preview.is_none() {
            self.authoring.preview_state = Some(self.scene.state.clone());
            self.generation += 1;
        }
        self.playing = false;
        for i in range.vertices.clone() {
            let uv = self.scene.garment.mesh.uv[i];
            let local = DVec3::new(uv.x, -uv.y, 0.);
            let local = wrap.map_or(local, |w| w.bend(local));
            self.scene.state.x[i] = placement.transform(local);
            self.scene.state.v[i] = DVec3::ZERO;
        }
        self.authoring.preview = Some(placement);
    }

    fn place_on_anchor(
        &mut self,
        panel: &str,
        label: &str,
        arranged: Result<Arrangement, String>,
    ) -> Result<String, String> {
        let arranged = arranged?;
        let wrap = arranged.wrap;
        self.cancel_preview();
        self.playing = false;
        let command = |placement: Placement| Command::Arrange {
            panel: panel.to_string(),
            placement,
            wrap,
        };
        let edit = authoring::apply(&self.project.template, command(arranged.placement.clone()))?;
        let state = edit.state_after(&self.scene.garment, &self.scene.state);
        diagnostics::check_vbd_initialization(Arc::new(edit.garment), &self.scene.collider.surface, &state).map_err(|e| {
            format!("Not placed: the panel would start intersecting ({e}). Increase the clearance or choose another anchor.")
        })?;
        if !self.edit(command(arranged.placement)) {
            return Err(self.status.clone());
        }
        let shape = match wrap {
            Some(w) => format!("wrapped, radius {:.0} mm", w.radius_m.abs() * 1000.0),
            None => "flat".to_string(),
        };
        Ok(format!(
            "Placed on “{label}”: {:.0} mm from the skin, {shape}. Initialization check passed.",
            self.authoring.arrangement.clearance_mm
        ))
    }
}
