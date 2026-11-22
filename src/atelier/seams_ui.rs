//! Existing seam selection/deletion is available independently of sewing mode.
use super::{authoring_ui::Tool, board::same_edge, *};
use fashioncraft::{authoring::Command, garment::seam::BoundaryRef};

/// Where a new tear opens a seam: its middle fifth.
const NEW_TEAR: [f64; 2] = [0.4, 0.6];

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
        if self.authoring.selection.all_seams {
            return self
                .project
                .template
                .seams
                .iter()
                .any(|s| s.references().any(|r| same_edge(edge, r)));
        }
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
    /// Properties › Piece › Seams: every seam as one compact row (its colour, name, the
    /// pieces it joins and its kind), and the chosen seam's settings below in two columns,
    /// names on the left and values on the right, as desktop cloth tools lay them out.
    pub(super) fn existing_seams(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let mut todo = SeamActions::default();
        // With a piece chosen, its own seams; otherwise all of them.
        let piece = self.authoring.selected.clone();
        let shown = self
            .project
            .template
            .seams
            .iter()
            .filter(|s| {
                piece
                    .as_ref()
                    .is_none_or(|p| &s.a.panel == p || &s.b.panel == p)
            })
            .count();
        egui::CollapsingHeader::new(super::workspace::section(format!(
            "{} ({shown})",
            i18n::tr(
                ui_lang,
                if piece.is_some() {
                    "Seams of this piece"
                } else {
                    "Seams"
                }
            ),
        )))
        .id_salt("existing_seams")
        .default_open(true)
        .open(self.authoring.selection.all_seams.then_some(true))
        .show(ui, |ui| {
            if self.project.template.seams.is_empty() {
                ui.small(i18n::tr(ui_lang, "No seams. Sewing links two boundaries."));
                return;
            }
            if self.authoring.selection.all_seams {
                self.all_seams_actions(ui, &mut todo);
            }
            self.seam_table(ui, &mut todo);
        });
        for id in self.edge_seams() {
            self.seam_details(ui, &id, &mut todo);
        }
        self.apply_seam_actions(todo);
        ui.separator();
    }

    /// What to do with seams once the panel is drawn.
    fn apply_seam_actions(&mut self, todo: SeamActions) {
        let ui_lang = self.prefs.language;
        if let Some((edge, id)) = todo.select {
            self.playing = false;
            self.authoring.tool = Tool::Edit;
            self.authoring.ends.clear();
            self.select_boundary(edge);
            self.authoring.selection.seam = Some(id);
            self.authoring.selection.all_seams = false;
            self.authoring.selection.seam_card = None;
        }
        if let Some((seams, active)) = todo.switch {
            let count = seams.len();
            if self.edit(Command::SetSeamsActive { seams, active }) {
                self.status = format!(
                    "{} {}",
                    super::garment_display::counted(count, "seam", "seams"),
                    if active { "on" } else { "off" }
                );
            }
        }
        if todo.remove_all {
            let seams: Vec<String> = self
                .project
                .template
                .seams
                .iter()
                .map(|s| s.id.clone())
                .collect();
            if self.edit(Command::RemoveSeams { seams }) {
                self.authoring.selection.all_seams = false;
                self.authoring.selection.seam = None;
                self.status = i18n::tr(ui_lang, "Seams deleted · Ctrl+Z to undo").into();
            }
        }
        if let Some(seam) = todo.remove {
            self.remove_seam(seam);
        }
        if let Some(seam) = todo.merge {
            self.merge_along_seam(seam);
        }
        if let Some(id) = todo.edit {
            self.begin_seam_edit(&id);
        }
    }

    /// Edit › Select all seams: switch them all on or off, or delete them.
    fn all_seams_actions(&mut self, ui: &mut egui::Ui, todo: &mut SeamActions) {
        let ui_lang = self.prefs.language;
        let all: Vec<String> = self
            .project
            .template
            .seams
            .iter()
            .map(|s| s.id.clone())
            .collect();
        ui.label(format!(
            "{} {}",
            all.len(),
            i18n::tr(ui_lang, "seams selected")
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.button(i18n::tr(ui_lang, "Switch off")).clicked() {
                todo.switch = Some((all.clone(), false));
            }
            if ui.button(i18n::tr(ui_lang, "Switch on")).clicked() {
                todo.switch = Some((all.clone(), true));
            }
            if ui
                .button(i18n::tr(ui_lang, "Delete them"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Delete every seam in one step; Ctrl+Z brings them back",
                ))
                .clicked()
            {
                todo.remove_all = true;
            }
            if ui.button(i18n::tr(ui_lang, "Clear selection")).clicked() {
                self.authoring.selection.all_seams = false;
            }
        });
        ui.separator();
    }

    /// The seams, one row each: a click chooses one; its pencil edits its ranges and its bin
    /// deletes it, both shown on the chosen row and under the pointer.
    fn seam_table(&mut self, ui: &mut egui::Ui, todo: &mut SeamActions) {
        let ui_lang = self.prefs.language;
        let names: std::collections::HashMap<&str, &str> = self
            .project
            .template
            .panels
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        let piece = |id: &str| names.get(id).copied().unwrap_or(id).to_owned();
        let chosen = self.edge_seams();
        let width = ui.available_width().max(220.);
        let columns = SeamColumns::new(width);
        let weak = ui.visuals().weak_text_color();
        let ink = ui.visuals().text_color();
        let accent = super::workspace::accent(ui.visuals());
        // Column titles.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 20.), egui::Sense::hover());
        for (x, title) in [
            (columns.name, "Name"),
            (
                columns.joins,
                if columns.joins < columns.kind {
                    "Joins"
                } else {
                    ""
                },
            ),
            (columns.kind, "Type"),
        ] {
            ui.painter().text(
                egui::pos2(rect.left() + x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                i18n::tr(ui_lang, title),
                egui::FontId::proportional(11.),
                weak,
            );
        }
        let chosen_piece = self.authoring.selected.clone();
        let seams: Vec<_> = self
            .project
            .template
            .seams
            .iter()
            .filter(|s| {
                chosen_piece
                    .as_ref()
                    .is_none_or(|p| &s.a.panel == p || &s.b.panel == p)
            })
            .cloned()
            .collect();
        for seam in &seams {
            let (rect, row) = ui.allocate_exact_size(Vec2::new(width, 26.), egui::Sense::click());
            let picked = self.authoring.selection.all_seams
                || self.authoring.selection.seam.as_ref() == Some(&seam.id)
                || chosen.contains(&seam.id);
            let off = self.project.template.inactive_seams.contains(&seam.id);
            let hovered = row.hovered() || ui.rect_contains_pointer(rect);
            if ui.is_rect_visible(rect) {
                let fill = if picked {
                    accent.gamma_multiply(0.16)
                } else if hovered {
                    ui.visuals().widgets.hovered.bg_fill
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, 3., fill);
                ui.painter().circle_filled(
                    egui::pos2(rect.left() + columns.dot, rect.center().y),
                    3.5,
                    super::seam_links::seam_color(&seam.id),
                );
                let text = |ui: &egui::Ui, text: String, color: Color32, x: f32, until: f32| {
                    let galley = egui::WidgetText::from(egui::RichText::new(text).color(color))
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Truncate),
                            (until - x - 6.).max(10.),
                            egui::TextStyle::Body,
                        );
                    ui.painter().galley(
                        egui::pos2(rect.left() + x, rect.center().y - galley.size().y * 0.5),
                        galley,
                        color,
                    );
                };
                let name = if off {
                    format!("{} ({})", seam.id, i18n::tr(ui_lang, "off"))
                } else {
                    seam.id.clone()
                };
                text(
                    ui,
                    name,
                    if off { weak } else { ink },
                    columns.name,
                    columns.joins,
                );
                if columns.joins < columns.kind {
                    text(
                        ui,
                        joined(&piece(&seam.a.panel), &piece(&seam.b.panel)),
                        weak,
                        columns.joins,
                        columns.kind,
                    );
                }
                text(
                    ui,
                    format!("{}:{}", 1 + seam.a_tail.len(), 1 + seam.b_tail.len()),
                    weak,
                    columns.kind,
                    columns.actions,
                );
            }
            let mut acted = false;
            if picked || hovered {
                let size = Vec2::splat(22.);
                let edit_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + columns.actions, rect.center().y - 11.),
                    size,
                );
                let bin_rect = edit_rect.translate(Vec2::new(24., 0.));
                for (r, icon, tip, key) in [
                    (edit_rect, icons::Icon::Edit, "Edit ranges / chain…", "edit"),
                    (
                        bin_rect,
                        icons::Icon::Trash,
                        "Delete this seam only",
                        "delete",
                    ),
                ] {
                    let response = ui
                        .interact(
                            r,
                            ui.id().with(("seam-row", &seam.id, key)),
                            egui::Sense::click(),
                        )
                        .on_hover_text(i18n::tr(ui_lang, tip));
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(r, 3., ui.visuals().widgets.hovered.bg_fill);
                    }
                    icons::glyph(ui.painter(), r.shrink(4.), icon, ink, 1.4);
                    if response.clicked() {
                        acted = true;
                        if key == "edit" {
                            todo.edit = Some(seam.id.clone());
                        } else {
                            todo.remove = Some(seam.id.clone());
                        }
                    }
                }
            }
            let row = row.on_hover_text(format!(
                "{} / {} ↔ {} / {} · {}:{}",
                piece(&seam.a.panel),
                seam.a.edge.as_deref().unwrap_or("boundary"),
                piece(&seam.b.panel),
                seam.b.edge.as_deref().unwrap_or("boundary"),
                1 + seam.a_tail.len(),
                1 + seam.b_tail.len()
            ));
            if row.clicked() && !acted {
                todo.select = Some((seam.a.clone(), seam.id.clone()));
            }
        }
    }

    /// The chosen seam's settings, names on the left and values on the right.
    fn seam_details(&mut self, ui: &mut egui::Ui, id: &str, todo: &mut SeamActions) {
        let ui_lang = self.prefs.language;
        let Some(seam) = self
            .project
            .template
            .seams
            .iter()
            .find(|s| s.id == id)
            .cloned()
        else {
            return;
        };
        let piece = |panel: &str| {
            self.project
                .template
                .panels
                .iter()
                .find(|p| p.id == panel)
                .map_or(panel.to_owned(), |p| p.name.clone())
        };
        let side_a = format!(
            "{} · {}",
            piece(&seam.a.panel),
            seam.a.edge.as_deref().unwrap_or("boundary")
        );
        let side_b = format!(
            "{} · {}",
            piece(&seam.b.panel),
            seam.b.edge.as_deref().unwrap_or("boundary")
        );
        let accent = super::workspace::accent(ui.visuals());
        ui.add_space(6.);
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 3.5, super::seam_links::seam_color(id));
            ui.label(
                egui::RichText::new(i18n::tr(ui_lang, "Seam"))
                    .strong()
                    .color(accent),
            );
            ui.label(egui::RichText::new(id).weak());
        });
        let label = |ui: &mut egui::Ui, text: &'static str| {
            ui.label(egui::RichText::new(i18n::tr(ui_lang, text)).weak());
        };
        egui::Grid::new(("seam-details", id))
            .num_columns(2)
            .spacing([14., 6.])
            .min_col_width(96.)
            .show(ui, |ui| {
                label(ui, "Side A");
                ui.label(&side_a);
                ui.end_row();
                label(ui, "Side B");
                ui.label(&side_b);
                ui.end_row();
                label(ui, "Type");
                ui.label(format!(
                    "{}:{} {}",
                    1 + seam.a_tail.len(),
                    1 + seam.b_tail.len(),
                    i18n::tr(ui_lang, "segments")
                ));
                ui.end_row();
                label(ui, "Active");
                let mut active = !self.project.template.inactive_seams.contains(id);
                if ui
                    .checkbox(&mut active, "")
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "An inactive seam stays in the document but is not sewn",
                    ))
                    .changed()
                {
                    todo.switch = Some((vec![id.to_owned()], active));
                }
                ui.end_row();
                label(ui, "Fold angle");
                self.seam_fold_controls(ui, id);
                ui.end_row();
                label(ui, "Zipper");
                self.seam_zipper_controls(ui, id);
                ui.end_row();
            });
        self.seam_tear_controls(ui, id);
        ui.horizontal_wrapped(|ui| {
            let button = ui.button(i18n::tr(ui_lang, "Edit ranges / chain…"));
            self.smoke_target_ui(ui, "edit_seam", button.rect);
            if button.clicked() {
                todo.edit = Some(id.to_owned());
            }
            if ui
                .button(i18n::tr(ui_lang, "Merge the two pieces here"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Join the pieces this seam sews edge to edge into one piece; the seam goes away",
                ))
                .clicked()
            {
                todo.merge = Some(id.to_owned());
            }
            let button = ui.button(i18n::tr(ui_lang, "Delete the seam")).on_hover_text(
                i18n::tr(ui_lang, "Free both boundaries without deleting the fabric. Shortcut: Del. Undo: Ctrl+Z."),
            );
            self.smoke_target("remove_seam", button.rect.center());
            if button.clicked() {
                todo.remove = Some(id.to_owned());
            }
        });
    }
}

/// "front ↔ back" for pieces named "tshirt front" and "tshirt back": a first word both names
/// share says nothing in a narrow column.
fn joined(a: &str, b: &str) -> String {
    let shared = match (a.split_once(' '), b.split_once(' ')) {
        (Some((first_a, rest_a)), Some((first_b, rest_b))) if first_a == first_b => {
            Some((rest_a, rest_b))
        }
        _ => None,
    };
    let (a, b) = shared.unwrap_or((a, b));
    format!("{a} ↔ {b}")
}

/// What the seams panel was asked to do this frame, done once it is drawn.
#[derive(Default)]
struct SeamActions {
    select: Option<(BoundaryRef, String)>,
    switch: Option<(Vec<String>, bool)>,
    remove: Option<String>,
    remove_all: bool,
    merge: Option<String>,
    edit: Option<String>,
}

/// Where the columns of the seams table start, from the row's left edge.
struct SeamColumns {
    dot: f32,
    name: f32,
    joins: f32,
    kind: f32,
    actions: f32,
}

impl SeamColumns {
    /// Below this width the Joins column gives its room to the names (it stays in the row's
    /// tooltip).
    const JOINS_FROM: f32 = 330.;

    fn new(width: f32) -> Self {
        let actions = width - 50.;
        let kind = actions - 34.;
        let name = 22.;
        let joins = if width < Self::JOINS_FROM {
            kind
        } else {
            name + (kind - name) * 0.48
        };
        Self {
            dot: 10.,
            name,
            joins,
            kind,
            actions,
        }
    }
}

impl Atelier {
    /// Tears of a seam (3D Garment › Tear): stretches left unsewn, as a rip, each from and to
    /// a share of the seam's length; tear another stretch, change one, or mend them.
    pub(super) fn seam_tear_controls(&mut self, ui: &mut egui::Ui, id: &str) {
        let ui_lang = self.prefs.language;
        let tears = self
            .project
            .template
            .seam_tears
            .get(id)
            .cloned()
            .unwrap_or_default();
        let mut wanted: Option<Vec<[f64; 2]>> = None;
        super::workspace::prop_row(ui, i18n::tr(ui_lang, "Tears"), |ui| {
            if ui
                .small_button(i18n::tr(ui_lang, "Tear"))
                .on_hover_text(i18n::tr(ui_lang, "Open a stretch of this seam, as a rip"))
                .clicked()
            {
                let mut next = tears.clone();
                next.push(NEW_TEAR);
                wanted = Some(next);
            }
            if !tears.is_empty() && ui.small_button(i18n::tr(ui_lang, "Mend all")).clicked() {
                wanted = Some(vec![]);
            }
        });
        for (k, tear) in tears.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(12.);
                let mut edited = tear.map(|s| s * 100.);
                let mut commit = false;
                for (n, value) in edited.iter_mut().enumerate() {
                    if n == 1 {
                        ui.label("–");
                    }
                    let r = ui.add(
                        egui::DragValue::new(value)
                            .range(0.0..=100.0)
                            .speed(0.5)
                            .fixed_decimals(0)
                            .suffix(" %"),
                    );
                    commit |= r.drag_stopped() || (r.changed() && !r.dragged());
                }
                if commit && edited[1] > edited[0] {
                    let mut next = tears.clone();
                    next[k] = edited.map(|s| s / 100.);
                    wanted = Some(next);
                }
                if ui
                    .small_button(i18n::tr(ui_lang, "Mend"))
                    .on_hover_text(i18n::tr(ui_lang, "Sew this stretch again"))
                    .clicked()
                {
                    let mut next = tears.clone();
                    next.remove(k);
                    wanted = Some(next);
                }
            });
        }
        if let Some(tears) = wanted {
            self.edit(Command::SetSeamTears {
                seam: id.into(),
                tears,
            });
        }
    }

    /// 3D Garment › Tear › Tear seam: open the middle fifth of the chosen seam.
    pub(super) fn tear_chosen_seam(&mut self) -> Result<(), String> {
        let seam = self
            .chosen_seam()
            .ok_or("Select a seam (or a sewn segment) first")?;
        let mut tears = self
            .project
            .template
            .seam_tears
            .get(&seam.id)
            .cloned()
            .unwrap_or_default();
        tears.push(NEW_TEAR);
        if !self.edit(Command::SetSeamTears {
            seam: seam.id.clone(),
            tears,
        }) {
            return Err(self.status.clone());
        }
        self.authoring.selection.seam = Some(seam.id.clone());
        self.prefs.workspace.inspector_open = true;
        self.status = format!(
            "Seam {} torn from 40 to 60 % · change it under Tears · simulate to see it open",
            seam.id
        );
        Ok(())
    }

    /// 3D Garment › Tear › Edit tears: show the chosen seam's tears in the inspector.
    pub(super) fn edit_tears(&mut self) -> Result<(), String> {
        let seam = self
            .chosen_seam()
            .ok_or("Select a seam (or a sewn segment) first")?;
        self.authoring.selection.seam = Some(seam.id.clone());
        self.prefs.workspace.inspector_open = true;
        self.status = format!("Tears of seam {} are under Tears in the inspector", seam.id);
        Ok(())
    }

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
            let mut on = current.is_some();
            if ui
                .checkbox(&mut on, "")
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
                let r = super::workspace::slim(
                    ui,
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
            let mut on = current.is_some();
            if ui
                .checkbox(&mut on, "")
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_shared_first_word_is_left_out_of_the_joins_column() {
        assert_eq!(super::joined("tshirt front", "tshirt back"), "front ↔ back");
        assert_eq!(super::joined("Delantero", "Espalda"), "Delantero ↔ Espalda");
        assert_eq!(
            super::joined("sleeve left", "cuff left"),
            "sleeve left ↔ cuff left"
        );
    }
}
