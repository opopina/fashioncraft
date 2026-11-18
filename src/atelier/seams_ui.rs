//! Existing seam selection/deletion is available independently of sewing mode.
use super::{authoring_ui::Tool, board::same_edge, *};
use marvelous_cloth::{authoring::Command, garment::seam::BoundaryRef};

impl Atelier {
    pub(super) fn edge_seams(&self) -> Vec<String> {
        self.project
            .template
            .seams
            .iter()
            .filter(|s| {
                self.authoring
                    .edge
                    .as_ref()
                    .is_some_and(|e| s.references().any(|r| same_edge(e, r)))
            })
            .map(|s| s.id.clone())
            .collect()
    }
    pub(super) fn seam_edge_highlighted(&self, edge: &BoundaryRef) -> bool {
        if let Some(id) = &self.authoring.selection.seam
            && let Some(seam) = self.project.template.seams.iter().find(|s| &s.id == id)
        {
            return seam.references().any(|r| same_edge(edge, r));
        }
        self.authoring.edge.as_ref().is_some_and(|selected| {
            same_edge(selected, edge)
                || self.project.template.seams.iter().any(|s| {
                    s.references().any(|r| same_edge(selected, r))
                        && s.references().any(|r| same_edge(edge, r))
                })
        })
    }
    pub(super) fn remove_seam(&mut self, seam: String) {
        if self.edit(Command::RemoveSeam { seam }) {
            self.authoring.selection.seam = None;
            self.authoring.selection.seam_card = None;
            self.status = "Seam removed · drape preserved · Ctrl+Z to undo".into();
        }
    }
    pub(super) fn existing_seams(&mut self, ui: &mut egui::Ui) {
        let selected = self.edge_seams();
        let mut remove = None;
        let mut edit = None;
        for id in &selected {
            ui.colored_label(super::seam_links::seam_color(id), format!("Seam: {id}"));
            let button = ui.button("Edit ranges / chain…");
            self.smoke_target_ui(ui, "edit_seam", button.rect);
            if button.clicked() {
                edit = Some(id.clone());
            }
            let button = ui.button("Delete the seam").on_hover_text(
                "Free both boundaries without deleting the fabric. Shortcut: Del. Undo: Ctrl+Z.",
            );
            self.smoke_target("remove_seam", button.rect.center());
            if button.clicked() {
                remove = Some(id.clone());
            }
            self.seam_fold_controls(ui, id);
        }
        let mut select = None;
        egui::CollapsingHeader::new(format!(
            "Existing seams ({})",
            self.project.template.seams.len()
        ))
        .id_salt("existing_seams")
        .default_open(false)
        .show(ui, |ui| {
            if self.project.template.seams.is_empty() {
                ui.small("No seams. Sewing links two boundaries.");
            }
            for seam in &self.project.template.seams {
                ui.horizontal(|ui| {
                    let (swatch, _) =
                        ui.allocate_exact_size(Vec2::splat(10.), egui::Sense::hover());
                    ui.painter().circle_filled(
                        swatch.center(),
                        3.5,
                        super::seam_links::seam_color(&seam.id),
                    );
                    if ui.small_button("Edit").clicked() {
                        edit = Some(seam.id.clone());
                    }
                    if ui
                        .selectable_label(
                            self.authoring.selection.seam.as_ref() == Some(&seam.id),
                            &seam.id,
                        )
                        .clicked()
                    {
                        select = Some((seam.a.clone(), seam.id.clone()));
                    }
                    if ui
                        .small_button("Delete")
                        .on_hover_text("Delete this seam only")
                        .clicked()
                    {
                        remove = Some(seam.id.clone());
                    }
                });
                ui.small(format!(
                    "{} / {} ↔ {} / {} · {}:{} edges",
                    seam.a.panel,
                    seam.a.edge.as_deref().unwrap_or("boundary"),
                    seam.b.panel,
                    seam.b.edge.as_deref().unwrap_or("boundary"),
                    1 + seam.a_tail.len(),
                    1 + seam.b_tail.len()
                ));
            }
        });
        if let Some((edge, id)) = select {
            self.playing = false;
            self.authoring.tool = Tool::Edit;
            self.authoring.ends.clear();
            self.select_boundary(edge);
            self.authoring.selection.seam = Some(id);
            self.authoring.selection.seam_card = None;
        }
        if let Some(seam) = remove {
            self.remove_seam(seam);
        }
        if let Some(id) = edit {
            self.begin_seam_edit(&id);
        }
        ui.separator();
    }
}

impl Atelier {
    /// The angle at which the selected seam's pieces meet (180° flat) and its strength.
    fn seam_fold_controls(&mut self, ui: &mut egui::Ui, seam: &str) {
        use marvelous_cloth::garment::template::SeamFold;
        let current = self.project.template.seam_folds.get(seam).cloned();
        let mut wanted: Option<Option<SeamFold>> = None;
        ui.horizontal(|ui| {
            if ui
                .selectable_label(current.is_some(), "Fold angle")
                .on_hover_text("Make the two pieces meet at an angle across this seam, like a pressed collar or a turned facing")
                .clicked()
            {
                wanted = Some(current.is_none().then_some(SeamFold {
                    angle: 90.,
                    strength: 10.,
                }));
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
                    .on_hover_text("180° is flat; less folds toward the pieces' front, more away from it");
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                let r = ui
                    .add(
                        egui::DragValue::new(&mut edited.strength)
                            .range(1.0..=1000.0)
                            .speed(0.5)
                            .fixed_decimals(0)
                            .prefix("×"),
                    )
                    .on_hover_text("Bending stiffness across the seam, times the fabric's");
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                if commit && &edited != fold {
                    wanted = Some(Some(edited));
                }
            }
        });
        if let Some(fold) = wanted {
            self.edit(marvelous_cloth::authoring::Command::SetSeamFold {
                seam: seam.into(),
                fold,
            });
        }
    }
}
