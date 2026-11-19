//! Existing seam selection/deletion is available independently of sewing mode.
use super::{authoring_ui::Tool, board::same_edge, *};
use fashioncraft::{authoring::Command, garment::seam::BoundaryRef};

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
        let ui_lang = self.prefs.language;
        if self.edit(Command::RemoveSeam { seam }) {
            self.authoring.selection.seam = None;
            self.authoring.selection.seam_card = None;
            self.status =
                i18n::tr(ui_lang, "Seam removed · drape preserved · Ctrl+Z to undo").into();
        }
    }
    pub(super) fn existing_seams(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let selected = self.edge_seams();
        let mut remove = None;
        let mut edit = None;
        let mut merge = None;
        for id in &selected {
            ui.colored_label(super::seam_links::seam_color(id), format!("Seam: {id}"));
            let button = ui.button(i18n::tr(ui_lang, "Edit ranges / chain…"));
            self.smoke_target_ui(ui, "edit_seam", button.rect);
            if button.clicked() {
                edit = Some(id.clone());
            }
            let button = ui.button(i18n::tr(ui_lang, "Delete the seam")).on_hover_text(
                i18n::tr(ui_lang, "Free both boundaries without deleting the fabric. Shortcut: Del. Undo: Ctrl+Z."),
            );
            self.smoke_target("remove_seam", button.rect.center());
            if button.clicked() {
                remove = Some(id.clone());
            }
            if ui
                .button(i18n::tr(ui_lang, "Merge the two pieces here"))
                .on_hover_text(
                    i18n::tr(ui_lang, "Join the pieces this seam sews edge to edge into one piece; the seam goes away"),
                )
                .clicked()
            {
                merge = Some(id.clone());
            }
            self.seam_fold_controls(ui, id);
            self.seam_zipper_controls(ui, id);
        }
        let mut select = None;
        egui::CollapsingHeader::new(super::workspace::section(format!(
            "{} ({})",
            i18n::tr(ui_lang, "Existing seams"),
            self.project.template.seams.len()
        )))
        .id_salt("existing_seams")
        .default_open(false)
        .show(ui, |ui| {
            if self.project.template.seams.is_empty() {
                ui.small(i18n::tr(ui_lang, "No seams. Sewing links two boundaries."));
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
                    if ui.small_button(i18n::tr(ui_lang, "Edit")).clicked() {
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
                        .small_button(i18n::tr(ui_lang, "Delete"))
                        .on_hover_text(i18n::tr(ui_lang, "Delete this seam only"))
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
        if let Some(seam) = merge {
            self.merge_along_seam(seam);
        }
        if let Some(id) = edit {
            self.begin_seam_edit(&id);
        }
        ui.separator();
    }
}

impl Atelier {
    /// Merge the pieces a seam joins into one, as one undo step.
    pub(super) fn merge_along_seam(&mut self, seam: String) -> bool {
        let ui_lang = self.prefs.language;
        if self.edit(fashioncraft::authoring::Command::MergeAlongSeam { seam }) {
            self.authoring.edge = None;
            self.authoring.selection.seam = None;
            self.status = i18n::tr(ui_lang, "Pieces merged along the seam").into();
            return true;
        }
        false
    }

    /// A zipper along the selected seam: put it on or take it off, slide it closed or open (only
    /// the closed share is sewn) and set its tape width.
    fn seam_zipper_controls(&mut self, ui: &mut egui::Ui, seam: &str) {
        let ui_lang = self.prefs.language;
        use fashioncraft::garment::template::SeamZipper;
        let current = self.project.template.seam_zippers.get(seam).cloned();
        let mut wanted: Option<Option<SeamZipper>> = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(current.is_some(), i18n::tr(ui_lang, "Zipper"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "A zipper along this seam: only the share its slider has closed is sewn",
                ))
                .clicked()
            {
                wanted = Some(current.is_none().then(SeamZipper::default));
            }
            if let Some(zipper) = &current {
                let mut edited = zipper.clone();
                let mut closed = edited.closed * 100.;
                let r = ui
                    .add(
                        egui::Slider::new(&mut closed, 0.0..=100.0)
                            .fixed_decimals(0)
                            .suffix(" %"),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "How far the slider has closed it, from the seam's start: 0 % open and \
                         apart, 100 % closed",
                    ));
                let mut commit = r.drag_stopped() || (r.changed() && !r.dragged());
                edited.closed = (closed / 100.).clamp(0., 1.);
                let mut width = edited.width_m * 1000.;
                let r = ui
                    .add(
                        egui::DragValue::new(&mut width)
                            .range(2.0..=50.0)
                            .speed(0.2)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .on_hover_text(i18n::tr(ui_lang, "Tape width"));
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                edited.width_m = width / 1000.;
                if commit && &edited != zipper {
                    wanted = Some(Some(edited));
                }
            }
        });
        if let Some(zipper) = wanted {
            self.edit(fashioncraft::authoring::Command::SetSeamZipper {
                seam: seam.into(),
                zipper,
            });
        }
    }

    /// The angle at which the selected seam's pieces meet (180° flat) and its strength.
    fn seam_fold_controls(&mut self, ui: &mut egui::Ui, seam: &str) {
        let ui_lang = self.prefs.language;
        use fashioncraft::garment::template::SeamFold;
        let current = self.project.template.seam_folds.get(seam).cloned();
        let mut wanted: Option<Option<SeamFold>> = None;
        ui.horizontal(|ui| {
            if ui
                .selectable_label(current.is_some(), i18n::tr(ui_lang, "Fold angle"))
                .on_hover_text(i18n::tr(ui_lang, "Make the two pieces meet at an angle across this seam, like a pressed collar or a turned facing"))
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
                    .on_hover_text(i18n::tr(ui_lang, "180° is flat; less folds toward the pieces' front, more away from it"));
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                let r = ui
                    .add(
                        egui::DragValue::new(&mut edited.strength)
                            .range(1.0..=1000.0)
                            .speed(0.5)
                            .fixed_decimals(0)
                            .prefix("×"),
                    )
                    .on_hover_text(i18n::tr(ui_lang, "Bending stiffness across the seam, times the fabric's"));
                commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                if commit && &edited != fold {
                    wanted = Some(Some(edited));
                }
            }
        });
        if let Some(fold) = wanted {
            self.edit(fashioncraft::authoring::Command::SetSeamFold {
                seam: seam.into(),
                fold,
            });
        }
    }
}
