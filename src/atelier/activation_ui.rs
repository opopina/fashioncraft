//! Taking pieces out of the simulation and back, and laying a piece over or under the piece it
//! is sewn to. Deactivation is a document edit (one undo step); superimposing only moves the
//! current 3D state, like resetting a piece.
use super::*;
use fashioncraft::{authoring, garment::transfer};
use glam::DVec3;
use std::collections::BTreeSet;

impl Atelier {
    /// "Simulation state" in the piece inspector, for the selected pieces.
    pub(super) fn activation_properties(
        &mut self,
        ui: &mut egui::Ui,
        panel: &fashioncraft::garment::template::Panel,
    ) {
        let ui_lang = self.prefs.language;
        let state = self
            .project
            .template
            .deactivated_panels
            .iter()
            .find(|d| d.panel == panel.id)
            .map(|d| d.sewing);
        let strong = self
            .project
            .template
            .strengthened_panels
            .contains(&panel.id);
        let solid = self
            .project
            .template
            .solidified_panels
            .iter()
            .any(|s| s.panel == panel.id);
        let mut action = None;
        egui::CollapsingHeader::new(super::workspace::section(i18n::tr(ui_lang, "Simulation state")))
            .id_salt("activation")
            .show(ui, |ui| {
                ui.small(match state {
                    None => i18n::tr(ui_lang, "Active: simulated and colliding"),
                    Some(false) => i18n::tr(ui_lang, "Deactivated: held in place, other cloth passes through it"),
                    Some(true) => {
                        i18n::tr(ui_lang, "Deactivated with its seams: held in place, not sewn, other cloth passes through it")
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(i18n::tr(ui_lang, "Deactivate"))
                        .on_hover_text(i18n::tr(ui_lang, "Hold the selected pieces where they are and let other cloth pass through them; seams stay"))
                        .clicked()
                    {
                        action = Some(Action::Deactivate(false));
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Deactivate with seams"))
                        .on_hover_text(i18n::tr(ui_lang, "Also leave their seams out of the simulation"))
                        .clicked()
                    {
                        action = Some(Action::Deactivate(true));
                    }
                    if ui
                        .add_enabled(state.is_some(), egui::Button::new(i18n::tr(ui_lang, "Activate")))
                        .clicked()
                    {
                        action = Some(Action::Activate);
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(strong, i18n::tr(ui_lang, "Strengthen"))
                        .on_hover_text(format!(
                            "Simulate the selected pieces {}× stiffer so they hold their shape while the garment settles; click again to return them to normal",
                            fashioncraft::garment::template::STRENGTHEN_FACTOR
                        ))
                        .clicked()
                    {
                        action = Some(Action::Strengthen(!strong));
                    }
                    if ui
                        .selectable_label(solid, i18n::tr(ui_lang, "Solidify"))
                        .on_hover_text(i18n::tr(ui_lang, "Keep the selected pieces in their current 3D shape: they still move and collide as a whole. Click again to release them"))
                        .clicked()
                    {
                        action = Some(Action::Solidify(!solid));
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(i18n::tr(ui_lang, "Superimpose"));
                    if ui
                        .button(i18n::tr(ui_lang, "Over"))
                        .on_hover_text(i18n::tr(ui_lang, "Lay the selected pieces on the outside of the pieces they are sewn to"))
                        .clicked()
                    {
                        action = Some(Action::Superimpose(true));
                    }
                    if ui
                        .button(i18n::tr(ui_lang, "Under"))
                        .on_hover_text(i18n::tr(ui_lang, "Lay the selected pieces on the inside of the pieces they are sewn to"))
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
            Some(Action::Strengthen(on)) => {
                if self.edit(authoring::Command::Strengthen { panels: ids, on }) {
                    self.status = if on {
                        "Strengthened · simulated stiffer until released".into()
                    } else {
                        "Back to the fabric's own stiffness".into()
                    };
                }
            }
            Some(Action::Solidify(on)) => self.set_solid(&ids, on),
            None => {}
        }
    }

    /// Solidify pieces in their current shape, or release them, as one validated undo step.
    pub(super) fn set_solid(&mut self, ids: &[String], solidify: bool) {
        let ui_lang = self.prefs.language;
        if ids.is_empty() {
            self.status = i18n::tr(ui_lang, "Select the pieces first").into();
            return;
        }
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
            return;
        }
        self.playing = false;
        let result = authoring::prepare_solidify_edit(
            &self.project.template,
            &self.scene.garment,
            &self.scene.state,
            ids,
            solidify,
        )
        .and_then(|(edit, next)| {
            super::pins_ui::validate(&edit, &next, Some(&self.scene.collider.surface))?;
            Ok(edit)
        });
        match result {
            Ok(edit) => {
                self.authoring.history.record_named(
                    self.project.template.clone(),
                    if solidify { "Solidify" } else { "Unsolidify" },
                );
                self.install_edit(edit);
                let what = super::garment_display::counted(ids.len(), "piece", "pieces");
                self.status = if solidify {
                    format!("{what} solidified · they keep this shape")
                } else {
                    format!("{what} released")
                };
            }
            Err(error) => {
                self.status = error.clone();
                self.authoring.error = Some(error);
            }
        }
    }

    /// Deactivate (`deactivate`) or activate pieces as one validated undo step.
    pub(super) fn set_active(&mut self, ids: &[String], deactivate: bool, sewing: bool) {
        let ui_lang = self.prefs.language;
        if ids.is_empty() {
            self.status = i18n::tr(ui_lang, "Select the pieces first").into();
            return;
        }
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
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
                self.authoring.history.record_named(
                    self.project.template.clone(),
                    if deactivate { "Deactivate" } else { "Activate" },
                );
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
        let ui_lang = self.prefs.language;
        if ids.is_empty() {
            self.status = i18n::tr(ui_lang, "Select the pieces first").into();
            return;
        }
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
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
                self.status = i18n::tr(
                    ui_lang,
                    "Release the pins of that piece before superimposing it",
                )
                .into();
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
            self.status =
                i18n::tr(ui_lang, "Superimpose works on pieces sewn to another piece").into();
            return;
        }
        if let Err(error) =
            fashioncraft::vbd::VbdSolver::new(garment.clone(), Some(surface), &candidate)
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

impl Atelier {
    /// Fold arrangement (3D Garment › Fold arrangement): turn one side of internal line `line`
    /// of `panel` about it as the piece lies, to the line's fold angle (90° without a fold), so
    /// the simulation starts folded. Refused while a step runs, over pins, or if the cloth would
    /// start intersecting.
    pub(super) fn fold_arrange(&mut self, panel: &str, line: &str, other_side: bool) {
        let ui_lang = self.prefs.language;
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
            return;
        }
        let Some(points) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|poly| poly.lines.iter().find(|l| l.id == line))
            .and_then(|l| Some([*l.points.first()?, *l.points.last()?]))
        else {
            self.status = i18n::tr(ui_lang, "Select an internal line on the board first").into();
            return;
        };
        let angle = self
            .project
            .template
            .line_folds
            .iter()
            .find(|f| f.panel == panel && f.line == line)
            .map_or(90., |f| f.angle);
        self.playing = false;
        let garment = self.scene.garment.clone();
        let moved = match transfer::fold_about_line(
            &garment,
            &self.scene.state.x,
            panel,
            points,
            angle,
            other_side,
        ) {
            Ok(moved) => moved,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if moved.iter().any(|(v, _)| garment.mesh.pins[*v].is_some()) {
            self.status =
                i18n::tr(ui_lang, "Release the pins on that side before folding it").into();
            return;
        }
        let mut candidate = self.scene.state.clone();
        for (v, p) in moved {
            candidate.x[v] = p;
            candidate.v[v] = DVec3::ZERO;
        }
        if let Err(error) = fashioncraft::vbd::VbdSolver::new(
            garment.clone(),
            Some(&self.scene.collider.surface),
            &candidate,
        ) {
            self.status = format!("Not folded: the cloth would start intersecting ({error})");
            return;
        }
        self.scene.state = candidate;
        self.issue.invalidate();
        self.generation += 1;
        self.metrics = None;
        self.grab = None;
        self.status = format!("Folded at {angle:.0}° · simulate to settle");
    }

    /// Reset 2D arrangement (3D Garment › Reset 2D arrangement): every piece laid flat in 3D as
    /// it lies on the board, on an upright plane in front of the avatar, centred on it. Only the
    /// draped positions change (Reset 3D arrangement brings the pieces back to their places);
    /// refused while a step runs, or if the cloth would start intersecting.
    pub(super) fn reset_2d_arrangement(&mut self) {
        let ui_lang = self.prefs.language;
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
            return;
        }
        self.playing = false;
        let garment = self.scene.garment.clone();
        let laid = transfer::lay_out_as_board(
            &self.project.template,
            &garment,
            &self.scene.collider.surface.vertices,
        );
        if laid.is_empty() {
            self.status = i18n::tr(ui_lang, "There are no pieces to lay out").into();
            return;
        }
        let mut candidate = self.scene.state.clone();
        for (v, p) in laid {
            candidate.x[v] = p;
            candidate.v[v] = DVec3::ZERO;
        }
        if let Err(error) = fashioncraft::vbd::VbdSolver::new(
            garment.clone(),
            Some(&self.scene.collider.surface),
            &candidate,
        ) {
            self.status = format!("Not laid out: the cloth would start intersecting ({error})");
            return;
        }
        self.scene.state = candidate;
        self.issue.invalidate();
        self.generation += 1;
        self.metrics = None;
        self.grab = None;
        self.status = i18n::tr(
            ui_lang,
            "Pieces laid out as on the board, in front of the avatar · Reset 3D arrangement puts them back",
        )
        .into();
    }

    /// Superimpose (side): the selected piece laid flat beside the piece it is sewn to (its first
    /// seam with another piece), the two seam edges meeting, on the side away from that piece.
    /// Only the draped positions change; refused while a step runs, over pins, or if the cloth
    /// would start intersecting.
    pub(super) fn superimpose_side(&mut self) -> Result<(), String> {
        let ui_lang = self.prefs.language;
        if self.pending {
            return Err("Wait for the current step to finish, then try again".into());
        }
        let ids = self.selected_panels();
        let [panel] = ids.as_slice() else {
            return Err("Select one piece sewn to another".into());
        };
        let garment = self.scene.garment.clone();
        let placed = transfer::superimpose_side(
            &self.project.template,
            &garment,
            &self.scene.state.x,
            panel,
            self.settings.contact.radius * 2. + 0.002,
        )?;
        if placed.iter().any(|(v, _)| garment.mesh.pins[*v].is_some()) {
            return Err(i18n::tr(
                ui_lang,
                "Release the pins of that piece before superimposing it",
            )
            .into());
        }
        self.playing = false;
        let mut candidate = self.scene.state.clone();
        for (v, p) in placed {
            candidate.x[v] = p;
            candidate.v[v] = DVec3::ZERO;
        }
        fashioncraft::vbd::VbdSolver::new(
            garment.clone(),
            Some(&self.scene.collider.surface),
            &candidate,
        )
        .map_err(|e| format!("Not laid beside: the cloth would start intersecting ({e})"))?;
        self.scene.state = candidate;
        self.issue.invalidate();
        self.generation += 1;
        self.metrics = None;
        self.grab = None;
        self.status = "Piece laid beside the piece it is sewn to · simulate to settle".into();
        Ok(())
    }

    /// The internal line selected on the board, as (piece, line).
    pub(super) fn selected_internal_line(&self) -> Option<(String, String)> {
        let edge = self.authoring.edge.as_ref()?;
        let name = edge.edge.clone()?;
        let panel = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == edge.panel)?;
        panel
            .polygon
            .as_ref()?
            .lines
            .iter()
            .any(|l| l.id == name)
            .then(|| (edge.panel.clone(), name))
    }
}

#[derive(Clone, Copy)]
enum Action {
    Deactivate(bool),
    Activate,
    Superimpose(bool),
    Strengthen(bool),
    Solidify(bool),
}
