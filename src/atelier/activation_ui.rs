//! Taking pieces out of the simulation and back, and laying a piece over or under the piece it
//! is sewn to. Deactivation is a document edit (one undo step); superimposing only moves the
//! current 3D state, like resetting a piece.
use super::*;
use glam::DVec3;
use marvelous_cloth::{authoring, garment::transfer};
use std::collections::BTreeSet;

impl Atelier {
    /// "Simulation state" in the piece inspector, for the selected pieces.
    pub(super) fn activation_properties(
        &mut self,
        ui: &mut egui::Ui,
        panel: &marvelous_cloth::garment::template::Panel,
    ) {
        let state = self
            .project
            .template
            .deactivated_panels
            .iter()
            .find(|d| d.panel == panel.id)
            .map(|d| d.sewing);
        let mut action = None;
        egui::CollapsingHeader::new("Simulation state")
            .id_salt("activation")
            .show(ui, |ui| {
                ui.small(match state {
                    None => "Active: simulated and colliding",
                    Some(false) => "Deactivated: held in place, other cloth passes through it",
                    Some(true) => {
                        "Deactivated with its seams: held in place, not sewn, other cloth passes through it"
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button("Deactivate")
                        .on_hover_text("Hold the selected pieces where they are and let other cloth pass through them; seams stay")
                        .clicked()
                    {
                        action = Some(Action::Deactivate(false));
                    }
                    if ui
                        .button("Deactivate with seams")
                        .on_hover_text("Also leave their seams out of the simulation")
                        .clicked()
                    {
                        action = Some(Action::Deactivate(true));
                    }
                    if ui
                        .add_enabled(state.is_some(), egui::Button::new("Activate"))
                        .clicked()
                    {
                        action = Some(Action::Activate);
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label("Superimpose");
                    if ui
                        .button("Over")
                        .on_hover_text("Lay the selected pieces on the outside of the pieces they are sewn to")
                        .clicked()
                    {
                        action = Some(Action::Superimpose(true));
                    }
                    if ui
                        .button("Under")
                        .on_hover_text("Lay the selected pieces on the inside of the pieces they are sewn to")
                        .clicked()
                    {
                        action = Some(Action::Superimpose(false));
                    }
                });
            });
        let ids = self.selected_panels();
        match action {
            Some(Action::Deactivate(sewing)) => self.set_active(&ids, true, sewing),
            Some(Action::Activate) => self.set_active(&ids, false, false),
            Some(Action::Superimpose(over)) => self.superimpose_panels(&ids, over),
            None => {}
        }
    }

    /// Deactivate (`deactivate`) or activate pieces as one validated undo step.
    pub(super) fn set_active(&mut self, ids: &[String], deactivate: bool, sewing: bool) {
        if ids.is_empty() {
            self.status = "Select the pieces first".into();
            return;
        }
        if self.pending {
            self.status = "Wait for the current step to finish, then try again".into();
            return;
        }
        self.playing = false;
        let result = authoring::prepare_deactivate_edit(
            &self.project.template,
            &self.scene.garment,
            &self.scene.state,
            ids,
            deactivate,
            sewing,
        )
        .and_then(|(edit, next)| {
            super::pins_ui::validate(&edit, &next, Some(&self.scene.collider.surface))?;
            Ok(edit)
        });
        match result {
            Ok(edit) => {
                self.authoring.history.record(self.project.template.clone());
                self.install_edit(edit);
                let what = super::garment_display::counted(ids.len(), "piece", "pieces");
                self.status = match (deactivate, sewing) {
                    (true, false) => format!("{what} deactivated · other cloth passes through"),
                    (true, true) => format!("{what} deactivated with their seams"),
                    (false, _) => format!("{what} active again"),
                };
            }
            Err(error) => {
                self.status = error.clone();
                self.authoring.error = Some(error);
            }
        }
    }

    /// Lay each piece on the draped surface of the pieces it is sewn to, a little outside
    /// (`over`) or inside. Only the current 3D state changes, at rest; the drape of every other
    /// piece is kept.
    pub(super) fn superimpose_panels(&mut self, ids: &[String], over: bool) {
        if ids.is_empty() {
            self.status = "Select the pieces first".into();
            return;
        }
        if self.pending {
            self.status = "Wait for the current step to finish, then try again".into();
            return;
        }
        self.playing = false;
        let garment = self.scene.garment.clone();
        let surface = &self.scene.collider.surface;
        let axis = if surface.vertices.is_empty() {
            self.scene.state.x.iter().sum::<DVec3>() / self.scene.state.x.len().max(1) as f64
        } else {
            surface.vertices.iter().sum::<DVec3>() / surface.vertices.len() as f64
        };
        let mut candidate = self.scene.state.clone();
        let mut moved = 0;
        for id in ids {
            let Some(range) = garment.panels.iter().find(|p| &p.id == id) else {
                continue;
            };
            if range
                .vertices
                .clone()
                .any(|v| garment.mesh.pins[v].is_some())
            {
                self.status = "Release the pins of that piece before superimposing it".into();
                return;
            }
            let partners: BTreeSet<&str> = self
                .project
                .template
                .seams
                .iter()
                .filter(|s| s.references().any(|r| &r.panel == id))
                .flat_map(|s| s.references().map(|r| r.panel.as_str()))
                .filter(|p| p != id && !ids.iter().any(|i| i == p))
                .collect();
            let triangles: Vec<[usize; 3]> = garment
                .panels
                .iter()
                .filter(|p| partners.contains(p.id.as_str()))
                .flat_map(|p| garment.mesh.triangles[p.triangles.clone()].iter())
                .map(|t| t.vertices)
                .collect();
            if triangles.is_empty() {
                continue;
            }
            let laid = transfer::superimpose_positions(
                &self.scene.state.x[range.vertices.clone()],
                &self.scene.state.x,
                &triangles,
                axis,
                over,
                self.settings.contact.radius * 2. + 0.002,
            );
            for (v, p) in range.vertices.clone().zip(laid) {
                candidate.x[v] = p;
                candidate.v[v] = DVec3::ZERO;
            }
            moved += 1;
        }
        if moved == 0 {
            self.status = "Superimpose works on pieces sewn to another piece".into();
            return;
        }
        if let Err(error) =
            marvelous_cloth::vbd::VbdSolver::new(garment.clone(), Some(surface), &candidate)
        {
            self.status =
                format!("Not superimposed: the pieces would start intersecting ({error})");
            return;
        }
        self.scene.state = candidate;
        self.issue.invalidate();
        self.generation += 1;
        self.metrics = None;
        self.grab = None;
        let what = super::garment_display::counted(moved, "piece", "pieces");
        self.status = format!(
            "{what} laid {} the pieces they are sewn to · simulate to settle",
            if over { "over" } else { "under" }
        );
    }
}

#[derive(Clone, Copy)]
enum Action {
    Deactivate(bool),
    Activate,
    Superimpose(bool),
}
