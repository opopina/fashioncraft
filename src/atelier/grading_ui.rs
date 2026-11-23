//! Grading in the workspace: the grading window (size groups, sizes and the grade table,
//! sizes × points), the size on show in the 2D bar, the 2D review with every size laid over
//! the base, and the points of measure window. The rules live in
//! `fashioncraft::garment::grading`; this file only shows them and commits edits, each one
//! undoable. Picking the size on show is a view: it is not an edit and undo keeps it.
use super::*;
use fashioncraft::{
    authoring::{self, Edit},
    garment::GarmentTemplate,
    garment::grading::{
        self, Grading, NotchMode, NotchRule, Target, apply, eval_mm,
        pom::{self, MeasureKind, Segment},
    },
};
use glam::DVec2;
use std::collections::BTreeMap;

/// Grading window, POM window and 2D review state.
pub(super) struct GradingUi {
    pub open: bool,
    pub pom_open: bool,
    /// Every shown size laid over the base on the board.
    pub review: bool,
    /// The group the window shows.
    group: Option<String>,
    /// Text being typed in the table's fields, by field.
    drafts: BTreeMap<String, String>,
    new_sizes: String,
    new_base: String,
    new_size: String,
    new_group: String,
    /// Arrow step, millimetres.
    nudge_mm: f64,
    pom_name: String,
    pom_tolerance_mm: f64,
    pom_from: Option<String>,
    pom_to: Option<String>,
    /// Boundaries collected for a length measurement.
    pom_segments: Vec<Segment>,
    /// The point whose row the table last scrolled to.
    scrolled_to: Option<(String, usize)>,
}

impl Default for GradingUi {
    fn default() -> Self {
        Self {
            open: false,
            pom_open: false,
            review: false,
            group: None,
            drafts: BTreeMap::new(),
            new_sizes: "S M L XL".into(),
            new_base: "M".into(),
            new_size: String::new(),
            new_group: String::new(),
            nudge_mm: 1.,
            pom_name: String::new(),
            pom_tolerance_mm: 0.,
            pom_from: None,
            pom_to: None,
            pom_segments: vec![],
            scrolled_to: None,
        }
    }
}

/// An edit of the grading, given the template it is made in.
pub(super) type GradingEdit = Box<dyn FnOnce(&mut Grading, &GarmentTemplate) -> Result<(), String>>;

/// A change asked for in a window, applied once the window is drawn.
pub(super) enum Change {
    /// An undoable edit of the grading, named for the history.
    Edit(&'static str, GradingEdit),
    /// Start grading the garment.
    Create(Vec<String>, usize),
    /// Stop grading it.
    Remove,
    /// Show a size (not an edit).
    Show(String, String),
    /// Select a contour point of a piece on the board.
    Point(String, usize),
}

pub(super) fn edit(
    name: &'static str,
    f: impl FnOnce(&mut Grading, &GarmentTemplate) -> Result<(), String> + 'static,
) -> Change {
    Change::Edit(name, Box::new(f))
}

/// Millimetres as typed: up to two decimals, no trailing zeros.
fn mm_text(metres: f64) -> String {
    let s = format!("{:.2}", metres * 1000.);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// A millimetre field: shows `value`, takes sums such as `10+2`, and gives the new value in
/// metres when Enter is pressed or the field is left with a different value. Esc discards.
fn mm_field(
    ui: &mut egui::Ui,
    drafts: &mut BTreeMap<String, String>,
    key: String,
    value: f64,
    width: f32,
) -> Option<f64> {
    let mut text = drafts.get(&key).cloned().unwrap_or_else(|| mm_text(value));
    // A fixed size: in a grid, a desired width would be held to the column's last width.
    let r = ui.add_sized(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::TextEdit::singleline(&mut text)
            .id(egui::Id::new(("grading-field", &key)))
            .horizontal_align(egui::Align::RIGHT),
    );
    mark_shown(ui, format_args!("field:{key}"), r.rect);
    if r.changed() {
        drafts.insert(key.clone(), text.clone());
    }
    if r.lost_focus() {
        drafts.remove(&key);
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            return None;
        }
        let v = eval_mm(&text)? * 0.001;
        if (v - value).abs() > 1e-9 {
            return Some(v);
        }
    }
    None
}

/// Report a widget to the demo only while it is wholly in view, so a demo never aims at a row
/// scrolled out of its window.
fn mark_shown(ui: &egui::Ui, key: std::fmt::Arguments<'_>, rect: egui::Rect) {
    if ui.clip_rect().contains_rect(rect) {
        super::demo::mark(ui.ctx(), key, rect);
    }
}

/// A colour per size: smaller sizes blue, larger orange.
fn size_color(rank: usize, count: usize) -> Color32 {
    let t = if count > 1 {
        rank as f32 / (count - 1) as f32
    } else {
        0.5
    };
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(lerp(60, 235), lerp(125, 120), lerp(235, 40))
}

impl Atelier {
    fn grading_group_id(&self) -> Option<String> {
        let g = self.project.template.grading.as_ref()?;
        if let Some(id) = &self.authoring.grading.group
            && g.group(id).is_some()
        {
            return Some(id.clone());
        }
        self.authoring
            .selected
            .as_deref()
            .and_then(|p| g.group_of(p))
            .or(g.groups.first())
            .map(|g| g.id.clone())
    }

    /// Apply a change from the grading windows.
    pub(super) fn apply_grading_change(&mut self, change: Change) {
        let ui_lang = self.prefs.language;
        let before = &self.project.template;
        let (template, name) = match change {
            Change::Point(panel, k) => {
                self.authoring.selected = Some(panel.clone());
                self.authoring.anchor = Some((panel, k));
                return;
            }
            Change::Create(sizes, base) => {
                let names: Vec<&str> = sizes.iter().map(String::as_str).collect();
                match Grading::new(i18n::tr(ui_lang, "Sizes"), &names, base) {
                    Ok(g) => {
                        let mut t = before.clone();
                        t.grading = Some(g);
                        (t, Some("Create sizes"))
                    }
                    Err(e) => {
                        self.status = e;
                        return;
                    }
                }
            }
            Change::Remove => {
                let mut t = before.clone();
                t.grading = None;
                (t, Some("Remove grading"))
            }
            Change::Show(group, size) => {
                let mut t = before.clone();
                let Some(g) = t.grading.as_mut() else { return };
                if let Err(e) = g.set_active(&group, &size) {
                    self.status = e;
                    return;
                }
                (t, None)
            }
            Change::Edit(name, f) => {
                let mut t = before.clone();
                let Some(mut g) = t.grading.take() else {
                    return;
                };
                if let Err(e) = f(&mut g, before).and_then(|_| {
                    g.sanitize(before);
                    g.validate()
                }) {
                    self.status = e;
                    return;
                }
                t.grading = Some(g);
                (t, Some(name))
            }
        };
        // A change that leaves what is simulated alone keeps the compiled garment.
        let same = authoring::same_physics(before, &template);
        let garment = if same {
            Ok((*self.scene.garment).clone())
        } else {
            template.compile()
        };
        let garment = match garment {
            Ok(g) => g,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let shown = template.grading.as_ref().map(|g| {
            g.groups
                .iter()
                .map(|x| format!("{} {}", x.name, x.name_of(x.active())))
                .collect::<Vec<_>>()
                .join(", ")
        });
        if let Some(name) = name {
            self.authoring
                .history
                .record_named(self.project.template.clone(), name);
        }
        let showing = name.is_none();
        self.install_edit(Edit {
            reset_simulation: !same,
            template,
            garment,
            selected: self.authoring.selected.clone(),
            new_point: None,
        });
        if showing && let Some(shown) = shown {
            self.status = format!(
                "{}: {shown} · {}",
                i18n::tr(ui_lang, "In 3D"),
                i18n::tr(ui_lang, "the board and edits stay in the base size")
            );
        }
    }

    // --- Commands ---------------------------------------------------------------------

    pub(super) fn open_grading(&mut self) {
        self.authoring.grading.open = true;
    }
    pub(super) fn open_points_of_measure(&mut self) {
        self.authoring.grading.pom_open = true;
    }
    pub(super) fn toggle_grading_review(&mut self) {
        let ui_lang = self.prefs.language;
        if self.project.template.grading.is_none() {
            self.status = i18n::tr(
                ui_lang,
                "Create sizes first: 2D Pattern › Grading › Sizes and grade rules",
            )
            .into();
            return;
        }
        self.authoring.grading.review = !self.authoring.grading.review;
    }
    /// Show the next (or previous) shown size of the selected piece's group.
    pub(super) fn step_shown_size(&mut self, forward: bool) {
        let ui_lang = self.prefs.language;
        let Some(group) = self
            .grading_group_id()
            .and_then(|id| self.project.template.grading.as_ref()?.group(&id).cloned())
        else {
            self.status = i18n::tr(
                ui_lang,
                "Create sizes first: 2D Pattern › Grading › Sizes and grade rules",
            )
            .into();
            return;
        };
        let visible: Vec<&str> = group.visible().map(|s| s.id.as_str()).collect();
        let at = visible
            .iter()
            .position(|s| *s == group.active())
            .unwrap_or(0);
        let next = if forward {
            (at + 1).min(visible.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        if next != at {
            self.apply_grading_change(Change::Show(group.id.clone(), visible[next].into()));
        }
    }
    /// Make the selected contour point a grade point, or a proportional one.
    pub(super) fn toggle_grade_point(&mut self) {
        let ui_lang = self.prefs.language;
        let Some((panel, k)) = self.authoring.anchor.clone() else {
            self.status = i18n::tr(ui_lang, "Select a contour point on the board first").into();
            return;
        };
        let Some(edge) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|p| p.edges.get(k).cloned())
        else {
            return;
        };
        if self.project.template.grading.is_none() {
            self.status = i18n::tr(
                ui_lang,
                "Create sizes first: 2D Pattern › Grading › Sizes and grade rules",
            )
            .into();
            return;
        }
        let grade = !self
            .project
            .template
            .grading
            .as_ref()
            .is_some_and(|g| g.is_grade_point(&panel, &edge));
        self.apply_grading_change(edit("Grade point", move |g, _| {
            g.set_grade_point(&panel, &edge, grade);
            Ok(())
        }));
        self.status = i18n::tr(
            ui_lang,
            if grade {
                "Grade point: it grows by its own rule"
            } else {
                "Proportional point: it follows the grade points around it"
            },
        )
        .into();
    }

    // --- The size on show, in the 2D bar ---------------------------------------------

    pub(super) fn grading_size_bar(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let Some(group) = self
            .grading_group_id()
            .and_then(|id| self.project.template.grading.as_ref()?.group(&id).cloned())
        else {
            return;
        };
        let mut change = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(i18n::tr(ui_lang, "Size")).on_hover_text(i18n::tr(
                ui_lang,
                "The size shown and simulated in 3D; the board edits the base size",
            ));
            let visible: Vec<_> = group.visible().collect();
            let count = group.sizes.len();
            if visible.len() <= 8 {
                for size in visible {
                    let rank = group.rank(&size.id).unwrap_or(0);
                    let mut text = egui::RichText::new(&size.name);
                    if group.is_base(&size.id) {
                        text = text.strong();
                    }
                    let r = ui
                        .selectable_label(
                            group.active() == size.id,
                            text.color(size_color(rank, count)),
                        )
                        .on_hover_text(if group.is_base(&size.id) {
                            i18n::tr(ui_lang, "Base size")
                        } else {
                            i18n::tr(ui_lang, "Show this size in 3D")
                        });
                    super::demo::mark(ui.ctx(), format_args!("grading.size.{}", size.name), r.rect);
                    if r.clicked() && group.active() != size.id {
                        change = Some(Change::Show(group.id.clone(), size.id.clone()));
                    }
                }
            } else {
                egui::ComboBox::from_id_salt("grading-size")
                    .selected_text(group.name_of(group.active()))
                    .show_ui(ui, |ui| {
                        for size in visible {
                            if ui
                                .selectable_label(group.active() == size.id, &size.name)
                                .clicked()
                            {
                                change = Some(Change::Show(group.id.clone(), size.id.clone()));
                            }
                        }
                    });
            }
            ui.separator();
            let r = ui
                .toggle_value(
                    &mut self.authoring.grading.review,
                    i18n::tr(ui_lang, "All sizes"),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Lay every shown size over the base on the board",
                ));
            super::demo::mark(ui.ctx(), "grading.all_sizes", r.rect);
        });
        if let Some(change) = change {
            self.apply_grading_change(change);
        }
    }

    // --- The 2D review ------------------------------------------------------------------

    /// Outlines of other sizes over the board: every shown size in review (the base
    /// highlighted), else the size on show when it is not the base. With the grading window
    /// open, the selected piece's grade points (squares) and proportional points (rings).
    pub(super) fn paint_grading_2d(&self, painter: &egui::Painter, rect: egui::Rect) {
        let t = &self.project.template;
        let Some(grading) = &t.grading else { return };
        let board = &self.authoring.board;
        let review = self.authoring.grading.review;
        for (i, panel) in t.panels.iter().enumerate() {
            let Some(group) = grading.group_of(&panel.id) else {
                continue;
            };
            let Some(base) = &panel.polygon else { continue };
            let to_screen = |p: DVec2| {
                board.screen(
                    rect,
                    self.board_origin(i) + authoring::rotate_2d(p, self.board_angle(i)),
                )
            };
            let ring = |poly: &fashioncraft::garment::polygon::PolygonPattern| {
                poly.outer_points().ok().map(|pts| {
                    let mut v: Vec<egui::Pos2> = pts.iter().map(|p| to_screen(*p)).collect();
                    if let Some(first) = v.first().copied() {
                        v.push(first);
                    }
                    v
                })
            };
            let count = group.sizes.len();
            let sizes: Vec<&str> = if review {
                group
                    .visible()
                    .filter(|s| !group.is_base(&s.id))
                    .map(|s| s.id.as_str())
                    .collect()
            } else if !group.is_base(group.active()) {
                vec![group.active()]
            } else {
                vec![]
            };
            let mut graded_points: Vec<(usize, Vec<DVec2>)> = vec![];
            for size in &sizes {
                let Ok(Some(graded)) = apply::panel_in_size(t, &panel.id, size) else {
                    continue;
                };
                let rank = group.rank(size).unwrap_or(0);
                let color = size_color(rank, count);
                if let Some(points) = ring(&graded.polygon) {
                    let stroke = egui::Stroke::new(if review { 1.2 } else { 1.6 }, color);
                    if review {
                        painter.add(egui::Shape::line(points.clone(), stroke));
                    } else {
                        painter.extend(egui::Shape::dashed_line(&points, stroke, 6., 4.));
                    }
                    let label_at = points
                        .iter()
                        .copied()
                        .max_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)))
                        .unwrap_or(rect.center());
                    painter.text(
                        label_at + egui::vec2(4., 2.),
                        egui::Align2::LEFT_TOP,
                        group.name_of(size),
                        egui::FontId::proportional(11.),
                        color,
                    );
                }
                if graded.problem.is_some() {
                    painter.text(
                        to_screen(base.points[0]) + egui::vec2(0., -14.),
                        egui::Align2::LEFT_BOTTOM,
                        format!("! {}", group.name_of(size)),
                        egui::FontId::proportional(11.),
                        Color32::from_rgb(230, 70, 60),
                    );
                }
                graded_points.push((rank, graded.polygon.points.clone()));
            }
            if review {
                if let Some(points) = ring(base) {
                    painter.add(egui::Shape::line(
                        points,
                        egui::Stroke::new(2.4, workspace::accent(&egui::Visuals::default())),
                    ));
                }
                // Grade lines: each point through its sizes, smallest to largest.
                graded_points.push((group.rank(&group.base).unwrap_or(0), base.points.clone()));
                graded_points.sort_by_key(|(r, _)| *r);
                for k in 0..base.points.len() {
                    let line: Vec<egui::Pos2> = graded_points
                        .iter()
                        .filter_map(|(_, p)| p.get(k).map(|q| to_screen(*q)))
                        .collect();
                    if line.len() > 1 {
                        painter.add(egui::Shape::line(
                            line,
                            egui::Stroke::new(0.8, Color32::from_gray(140)),
                        ));
                    }
                }
            }
            let selected = self.authoring.selected.as_deref() == Some(panel.id.as_str());
            if self.authoring.grading.open && selected {
                for (k, (p, edge)) in base.points.iter().zip(&base.edges).enumerate() {
                    let at = to_screen(*p);
                    let picked = self.authoring.anchor.as_ref() == Some(&(panel.id.clone(), k));
                    let color = if picked {
                        Color32::from_rgb(230, 160, 20)
                    } else {
                        Color32::from_rgb(40, 120, 220)
                    };
                    if grading.is_grade_point(&panel.id, edge) {
                        painter.rect_stroke(
                            egui::Rect::from_center_size(at, egui::vec2(9., 9.)),
                            0.,
                            egui::Stroke::new(1.6, color),
                            egui::StrokeKind::Middle,
                        );
                    } else {
                        painter.circle_stroke(at, 4., egui::Stroke::new(1.4, color));
                    }
                    painter.text(
                        at + egui::vec2(6., -6.),
                        egui::Align2::LEFT_BOTTOM,
                        format!("P{}", k + 1),
                        egui::FontId::proportional(10.),
                        color,
                    );
                }
            }
        }
    }

    // --- The grading window -------------------------------------------------------------

    pub(super) fn grading_window(&mut self, ctx: &egui::Context) {
        if self.authoring.grading.open {
            let mut open = true;
            let mut changes: Vec<Change> = vec![];
            let ui_lang = self.prefs.language;
            // Below the menu bar and the view toolbars, so it never covers the menus.
            let top = ctx.content_rect().top() + 130.;
            egui::Window::new(i18n::tr(ui_lang, "Grading"))
                .id(egui::Id::new("grading-window"))
                .default_pos(egui::pos2(ctx.content_rect().left() + 360., top))
                .open(&mut open)
                .default_width(560.)
                .default_height(520.)
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::both()
                        .id_salt("grading-scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.grading_controls(ui, &mut changes));
                });
            self.authoring.grading.open &= open;
            for change in changes {
                self.apply_grading_change(change);
            }
        }
        if self.authoring.grading.pom_open {
            let mut open = true;
            let mut changes: Vec<Change> = vec![];
            let ui_lang = self.prefs.language;
            let top = ctx.content_rect().top() + 150.;
            egui::Window::new(i18n::tr(ui_lang, "Points of measure"))
                .id(egui::Id::new("pom-window"))
                .default_pos(egui::pos2(ctx.content_rect().left() + 400., top))
                .open(&mut open)
                .default_width(720.)
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::both()
                        .id_salt("pom-scroll")
                        .auto_shrink([false, true])
                        .show(ui, |ui| self.pom_controls(ui, &mut changes));
                });
            self.authoring.grading.pom_open &= open;
            for change in changes {
                self.apply_grading_change(change);
            }
        }
    }

    fn grading_controls(&mut self, ui: &mut egui::Ui, changes: &mut Vec<Change>) {
        let ui_lang = self.prefs.language;
        let tr = |text| i18n::tr(ui_lang, text);
        let template = &self.project.template;
        let Some(grading) = &template.grading else {
            ui.label(tr("The garment is drawn in one size. List its sizes, smallest first, and choose the base: the size the pieces are drawn in."));
            let s = &mut self.authoring.grading;
            egui::Grid::new("grading-create")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(tr("Sizes"));
                    ui.text_edit_singleline(&mut s.new_sizes);
                    ui.end_row();
                    ui.label(tr("Base size"));
                    ui.text_edit_singleline(&mut s.new_base);
                    ui.end_row();
                });
            let sizes: Vec<String> = s.new_sizes.split_whitespace().map(String::from).collect();
            let base = sizes
                .iter()
                .position(|n| n.eq_ignore_ascii_case(s.new_base.trim()));
            if base.is_none() {
                ui.colored_label(
                    Color32::YELLOW,
                    tr("The base size must be one of the sizes"),
                );
            }
            let r = ui.add_enabled(base.is_some(), egui::Button::new(tr("Create sizes")));
            super::demo::mark(ui.ctx(), "grading.create", r.rect);
            if r.clicked()
                && let Some(base) = base
            {
                changes.push(Change::Create(sizes, base));
            }
            return;
        };
        let Some(group_id) = self.grading_group_id() else {
            return;
        };
        let group = grading.group(&group_id).cloned().expect("listed");
        // Groups.
        ui.horizontal_wrapped(|ui| {
            ui.label(workspace::section(tr("Size group")));
            egui::ComboBox::from_id_salt("grading-group")
                .selected_text(&group.name)
                .show_ui(ui, |ui| {
                    for g in &grading.groups {
                        if ui.selectable_label(g.id == group.id, &g.name).clicked() {
                            self.authoring.grading.group = Some(g.id.clone());
                        }
                    }
                });
            let s = &mut self.authoring.grading;
            ui.add(
                egui::TextEdit::singleline(&mut s.new_group)
                    .hint_text(tr("name"))
                    .desired_width(90.),
            );
            let name = s.new_group.trim().to_string();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new(tr("Rename")))
                .clicked()
            {
                let id = group.id.clone();
                let n = name.clone();
                changes.push(edit("Rename size group", move |g, _| {
                    g.rename_group(&id, &n)
                }));
            }
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new(tr("New group")))
                .on_hover_text(tr(
                    "A new group with the same sizes; assign pieces to it below",
                ))
                .clicked()
            {
                let names: Vec<String> = group.sizes.iter().map(|s| s.name.clone()).collect();
                let base = group.rank(&group.base).unwrap_or(0);
                changes.push(edit("New size group", move |g, _| {
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    g.add_group(&name, &names, base).map(|_| ())
                }));
            }
            if grading.groups.len() > 1 && ui.button(tr("Remove group")).clicked() {
                let id = group.id.clone();
                changes.push(edit("Remove size group", move |g, _| g.remove_group(&id)));
            }
        });
        // Sizes.
        let count = group.sizes.len();
        egui::Grid::new("grading-sizes")
            .num_columns(6)
            .striped(true)
            .show(ui, |ui| {
                ui.strong(tr("Size"));
                ui.strong(tr("Base"));
                ui.strong(tr("Shown"));
                ui.strong(tr("In 3D"));
                ui.label("");
                ui.label("");
                ui.end_row();
                for size in &group.sizes {
                    let key = format!("size-name-{}-{}", group.id, size.id);
                    let s = &mut self.authoring.grading;
                    let mut text = s.drafts.get(&key).cloned().unwrap_or(size.name.clone());
                    let r = ui.add_sized(
                        egui::vec2(70., ui.spacing().interact_size.y),
                        egui::TextEdit::singleline(&mut text)
                            .text_color(size_color(group.rank(&size.id).unwrap_or(0), count)),
                    );
                    if r.changed() {
                        s.drafts.insert(key.clone(), text.clone());
                    }
                    if r.lost_focus() {
                        s.drafts.remove(&key);
                        if text.trim() != size.name {
                            let (g_id, s_id) = (group.id.clone(), size.id.clone());
                            changes.push(edit("Rename size", move |g, _| {
                                g.rename_size(&g_id, &s_id, &text)
                            }));
                        }
                    }
                    let is_base = group.is_base(&size.id);
                    ui.add_enabled(false, egui::RadioButton::new(is_base, ""))
                        .on_disabled_hover_text(tr("The size the pieces are drawn in"));
                    let mut shown = !size.hidden;
                    if ui
                        .add_enabled(!is_base, egui::Checkbox::without_text(&mut shown))
                        .changed()
                    {
                        let (g_id, s_id) = (group.id.clone(), size.id.clone());
                        changes.push(edit(
                            if shown { "Show size" } else { "Hide size" },
                            move |g, _| g.set_hidden(&g_id, &s_id, !shown),
                        ));
                    }
                    if ui
                        .add_enabled(
                            !size.hidden,
                            egui::RadioButton::new(group.active() == size.id, ""),
                        )
                        .clicked()
                        && group.active() != size.id
                    {
                        changes.push(Change::Show(group.id.clone(), size.id.clone()));
                    }
                    if ui
                        .small_button(tr("Insert after"))
                        .on_hover_text(tr(
                            "A size between this one and the next, its growths halfway",
                        ))
                        .clicked()
                    {
                        let (g_id, s_id) = (group.id.clone(), size.id.clone());
                        let name = format!("{}+", size.name);
                        changes.push(edit("Insert size", move |g, _| {
                            g.add_size(&g_id, &name, Some(&s_id)).map(|_| ())
                        }));
                    }
                    if ui
                        .add_enabled(!is_base, egui::Button::new(tr("Remove")).small())
                        .clicked()
                    {
                        let (g_id, s_id) = (group.id.clone(), size.id.clone());
                        changes.push(edit("Remove size", move |g, _| g.remove_size(&g_id, &s_id)));
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            let s = &mut self.authoring.grading;
            ui.add(
                egui::TextEdit::singleline(&mut s.new_size)
                    .hint_text(tr("new size"))
                    .desired_width(70.),
            );
            let name = s.new_size.trim().to_string();
            let g_id = group.id.clone();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new(tr("Add larger size")))
                .clicked()
            {
                let n = name.clone();
                let id = g_id.clone();
                changes.push(edit("Add size", move |g, _| {
                    g.add_size(&id, &n, None).map(|_| ())
                }));
                s.new_size.clear();
            }
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new(tr("Add smaller size")))
                .clicked()
            {
                changes.push(edit("Add size", move |g, _| {
                    g.insert_size(&g_id, &name, 0).map(|_| ())
                }));
                s.new_size.clear();
            }
        });
        // Pieces in this group.
        let members: Vec<&str> = template
            .panels
            .iter()
            .filter(|p| grading.group_of(&p.id).is_some_and(|g| g.id == group.id))
            .map(|p| p.name.as_str())
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{}: {}", tr("Pieces"), members.join(", ")));
            let selection = self.selected_panels();
            if grading.groups.len() > 1
                && !selection.is_empty()
                && ui
                    .button(tr("Grade the selected pieces in this group"))
                    .clicked()
            {
                let id = group.id.clone();
                changes.push(edit("Assign size group", move |g, _| {
                    g.assign(&selection, &id)
                }));
            }
        });
        ui.separator();
        self.grade_table(ui, &group, changes);
        let template = &self.project.template;
        ui.separator();
        // Seams whose two sides grow apart.
        egui::CollapsingHeader::new(workspace::section(tr("Seams in each size")))
            .id_salt("grading-seams")
            .default_open(false)
            .show(ui, |ui| {
                let mut any = false;
                for size in group.visible().filter(|s| !group.is_base(&s.id)) {
                    match apply::seam_differences(template, &group, &size.id, 0.5e-3) {
                        Ok(list) => {
                            for (seam, a, b) in list {
                                any = true;
                                ui.label(format!(
                                    "{} · {seam}: {:.1} / {:.1} mm ({:+.1})",
                                    size.name,
                                    a * 1000.,
                                    b * 1000.,
                                    (a - b) * 1000.
                                ));
                            }
                        }
                        Err(e) => {
                            any = true;
                            ui.colored_label(
                                Color32::from_rgb(230, 70, 60),
                                format!("{}: {e}", size.name),
                            );
                        }
                    }
                }
                if !any {
                    ui.label(tr(
                        "Both sides of every seam keep the same length in every size.",
                    ));
                }
                ui.small(tr(
                    "A seam of unequal sides eases in that size when simulated.",
                ));
            });
        ui.separator();
        if ui
            .button(tr("Remove grading"))
            .on_hover_text(tr("Back to one size; Undo brings the sizes and rules back"))
            .clicked()
        {
            changes.push(Change::Remove);
        }
    }

    /// The grade table of the selected piece: its points and curves × the shown sizes.
    fn grade_table(
        &mut self,
        ui: &mut egui::Ui,
        group: &grading::SizeGroup,
        changes: &mut Vec<Change>,
    ) {
        let ui_lang = self.prefs.language;
        let tr = |text| i18n::tr(ui_lang, text);
        let template = &self.project.template;
        let grading = template.grading.as_ref().expect("checked");
        let Some(panel) = self
            .authoring
            .selected
            .as_deref()
            .and_then(|id| template.panels.iter().find(|p| p.id == id))
        else {
            ui.label(tr("Select a piece to see its grade rules."));
            return;
        };
        let Some(poly) = &panel.polygon else {
            ui.label(tr("This piece has no editable outline to grade."));
            return;
        };
        if grading.group_of(&panel.id).is_none_or(|g| g.id != group.id) {
            ui.label(tr("The selected piece is graded in another size group."));
            return;
        }
        ui.label(workspace::section(format!(
            "{} · {}",
            tr("Grade rules"),
            panel.name
        )));
        ui.small(tr("Growth from the base size, mm: X across, Y down the board. Squares on the board are grade points; rings follow the grade points around them. Fields take sums such as 10+2."));
        let sizes: Vec<_> = group
            .visible()
            .filter(|s| !group.is_base(&s.id))
            .cloned()
            .collect();
        let count = group.sizes.len();
        let shown = group.active().to_string();
        // Arrows for the selected point in the size on show.
        let anchor = self
            .authoring
            .anchor
            .as_ref()
            .filter(|(id, k)| id == &panel.id && *k < poly.points.len())
            .map(|(_, k)| *k);
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Step"));
            ui.add(
                egui::DragValue::new(&mut self.authoring.grading.nudge_mm)
                    .range(0.01..=100.)
                    .speed(0.1)
                    .suffix(" mm"),
            );
            let enabled = anchor.is_some() && !group.is_base(&shown);
            let step = self.authoring.grading.nudge_mm * 0.001;
            for (label, d) in [
                ("←", DVec2::new(-step, 0.)),
                ("→", DVec2::new(step, 0.)),
                ("↑", DVec2::new(0., -step)),
                ("↓", DVec2::new(0., step)),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_hover_text(tr("Move the selected point in the size shown in 3D"))
                    .clicked()
                    && let Some(k) = anchor
                {
                    let target = Target::Point {
                        panel: panel.id.clone(),
                        edge: poly.edges[k].clone(),
                    };
                    let now = grading
                        .rule(&target)
                        .map_or(DVec2::ZERO, |r| r.delta(group, &shown));
                    let size = shown.clone();
                    changes.push(edit("Grade rule", move |g, _| {
                        g.set_delta(&target, &size, now + d)
                    }));
                }
            }
            if !enabled {
                ui.small(tr(
                    "Select a point and show a size other than the base to use the arrows.",
                ));
            }
        });
        // The table.
        let rules = grading.panels.get(&panel.id);
        let deltas: BTreeMap<String, Vec<DVec2>> = sizes
            .iter()
            .filter_map(|s| {
                Some((
                    s.id.clone(),
                    apply::point_deltas(poly, rules, group, &s.id).ok()?,
                ))
            })
            .collect();
        egui::Grid::new(("grade-table", &panel.id))
            .striped(true)
            .min_col_width(20.)
            .show(ui, |ui| {
                ui.strong(tr("Point"));
                ui.strong(tr("Grade"));
                for s in &sizes {
                    ui.label(
                        egui::RichText::new(format!("{} X", s.name))
                            .strong()
                            .color(size_color(group.rank(&s.id).unwrap_or(0), count)),
                    );
                    ui.label(
                        egui::RichText::new("Y")
                            .strong()
                            .color(size_color(group.rank(&s.id).unwrap_or(0), count)),
                    );
                }
                ui.strong(tr("Equal step"));
                ui.end_row();
                let mut rows: Vec<(Target, String, Option<usize>)> = vec![];
                for (k, edge) in poly.edges.iter().enumerate() {
                    rows.push((
                        Target::Point {
                            panel: panel.id.clone(),
                            edge: edge.clone(),
                        },
                        format!("P{}", k + 1),
                        Some(k),
                    ));
                    if poly.curves.contains_key(edge) {
                        rows.push((
                            Target::Curve {
                                panel: panel.id.clone(),
                                edge: edge.clone(),
                            },
                            format!("{} {edge}", tr("Curve")),
                            None,
                        ));
                    }
                }
                for (target, label, k) in rows {
                    let picked = k.is_some() && k == anchor;
                    let r = ui
                        .selectable_label(picked, &label)
                        .on_hover_text(target.edge());
                    mark_shown(ui, format_args!("grading.row.{label}"), r.rect);
                    // A point picked on the board brings its row into view.
                    if picked
                        && let Some(k) = k
                    {
                        let at = Some((panel.id.clone(), k));
                        if self.authoring.grading.scrolled_to != at {
                            r.scroll_to_me(Some(egui::Align::Center));
                            self.authoring.grading.scrolled_to = at;
                        }
                    }
                    if r.clicked()
                        && let Some(k) = k
                    {
                        changes.push(Change::Point(panel.id.clone(), k));
                    }
                    let grade = match &target {
                        Target::Point { edge, .. } => {
                            let mut on = grading.is_grade_point(&panel.id, edge);
                            if ui.checkbox(&mut on, "").on_hover_text(tr("A grade point grows by its own rule; a proportional point follows the grade points around it")).changed() {
                                let (p, e) = (panel.id.clone(), edge.clone());
                                changes.push(edit("Grade point", move |g, _| {
                                    g.set_grade_point(&p, &e, on);
                                    Ok(())
                                }));
                            }
                            on
                        }
                        Target::Curve { .. } => {
                            ui.label("");
                            true
                        }
                    };
                    let rule = grading.rule(&target).cloned().unwrap_or_default();
                    for s in &sizes {
                        let value = match (&target, k, grade) {
                            // A proportional point shows where it goes, read-only.
                            (Target::Point { .. }, Some(k), false) => {
                                let d = deltas.get(&s.id).map_or(DVec2::ZERO, |d| d[k]);
                                ui.weak(mm_text(d.x));
                                ui.weak(mm_text(d.y));
                                continue;
                            }
                            _ => rule.delta(group, &s.id),
                        };
                        for axis in 0..2 {
                            let key = format!("{}-{}-{}-{}-{axis}", panel.id, label, target.edge(), s.id);
                            let v = if axis == 0 { value.x } else { value.y };
                            let width = 54.;
                            if let Some(new) = mm_field(ui, &mut self.authoring.grading.drafts, key, v, width) {
                                let mut d = value;
                                if axis == 0 {
                                    d.x = new;
                                } else {
                                    d.y = new;
                                }
                                let (t, size) = (target.clone(), s.id.clone());
                                changes.push(edit("Grade rule", move |g, _| g.set_delta(&t, &size, d)));
                            }
                        }
                    }
                    if grade {
                        // The growth of the size above the base, repeated between every size.
                        let next = group
                            .rank(&group.base)
                            .and_then(|r| group.sizes.get(r + 1))
                            .map(|s| rule.delta(group, &s.id));
                        let stepped = rule.step.is_some();
                        let r = ui
                            .add_enabled(next.is_some(), egui::Button::new("=").small().selected(stepped))
                            .on_hover_text(tr("Equal step: every size grows from the one before by what the size above the base grows"));
                        mark_shown(ui, format_args!("grading.step.{label}"), r.rect);
                        if r.clicked()
                            && let Some(step) = next
                        {
                            let t = target.clone();
                            changes.push(edit("Equal step", move |g, _| g.set_step(&t, step)));
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        // Problems of each size.
        for s in &sizes {
            if let Ok(graded) = apply::grade_polygon(poly, rules, group, &s.id)
                && let Some(problem) = graded.problem
            {
                ui.colored_label(
                    Color32::from_rgb(230, 70, 60),
                    format!("{} {}: {problem}", tr("Size"), s.name),
                );
            }
        }
        // Notches.
        let notches: Vec<(String, f64)> = poly
            .notches
            .iter()
            .flat_map(|(e, list)| list.iter().map(move |n| (e.clone(), n.at)))
            .collect();
        if !notches.is_empty() {
            egui::CollapsingHeader::new(workspace::section(tr("Notches")))
                .id_salt(("grading-notches", &panel.id))
                .default_open(false)
                .show(ui, |ui| {
                    ui.small(tr("How each notch follows its boundary; the extra distance moves it away from the end it keeps (from the start for proportional ones)."));
                    egui::Grid::new(("notch-rules", &panel.id)).striped(true).show(ui, |ui| {
                        ui.strong(tr("Boundary"));
                        ui.strong(tr("Place"));
                        ui.strong(tr("Follows"));
                        for s in &sizes {
                            ui.strong(format!("{} +mm", s.name));
                        }
                        ui.end_row();
                        for (edge, at) in notches {
                            let current = rules
                                .and_then(|r| r.notch(&edge, at))
                                .cloned()
                                .unwrap_or(NotchRule {
                                    edge: edge.clone(),
                                    at,
                                    mode: NotchMode::Proportional,
                                    deltas: BTreeMap::new(),
                                });
                            ui.label(&edge);
                            ui.label(format!("{:.0} %", at * 100.));
                            let mut mode = current.mode;
                            egui::ComboBox::from_id_salt(("notch-mode", &panel.id, &edge, (at * 1e6) as i64))
                                .selected_text(tr(mode.label()))
                                .show_ui(ui, |ui| {
                                    for m in NotchMode::ALL {
                                        ui.selectable_value(&mut mode, m, tr(m.label()));
                                    }
                                });
                            if mode != current.mode {
                                let mut rule = current.clone();
                                rule.mode = mode;
                                let p = panel.id.clone();
                                changes.push(edit("Notch grading", move |g, _| g.set_notch_rule(&p, rule)));
                            }
                            for s in &sizes {
                                let v = current.deltas.get(&s.id).copied().unwrap_or(0.);
                                let key = format!("notch-{}-{edge}-{}-{}", panel.id, (at * 1e6) as i64, s.id);
                                if let Some(new) = mm_field(ui, &mut self.authoring.grading.drafts, key, v, 46.) {
                                    let mut rule = current.clone();
                                    if new == 0. {
                                        rule.deltas.remove(&s.id);
                                    } else {
                                        rule.deltas.insert(s.id.clone(), new);
                                    }
                                    let p = panel.id.clone();
                                    changes.push(edit("Notch grading", move |g, _| g.set_notch_rule(&p, rule)));
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
        }
        // Graphics on the piece.
        let graphics: Vec<_> = template
            .graphics
            .iter()
            .filter(|g| g.panel_id == panel.id)
            .map(|g| g.id.clone())
            .collect();
        if !graphics.is_empty() {
            egui::CollapsingHeader::new(workspace::section(tr("Graphics")))
                .id_salt(("grading-graphics", &panel.id))
                .default_open(false)
                .show(ui, |ui| {
                    ui.small(tr("Graphics follow the piece. Add an offset (mm) or a size (% of the base, proportions locked) per size."));
                    egui::Grid::new(("graphic-rules", &panel.id)).striped(true).show(ui, |ui| {
                        ui.strong(tr("Graphic"));
                        for s in &sizes {
                            ui.strong(format!("{} X", s.name));
                            ui.strong("Y");
                            ui.strong("%");
                        }
                        ui.end_row();
                        for id in graphics {
                            ui.label(&id);
                            let current = grading.graphics.get(&id).cloned().unwrap_or_default();
                            for s in &sizes {
                                let off = current.offsets.get(&s.id).copied().unwrap_or(DVec2::ZERO);
                                for axis in 0..2 {
                                    let key = format!("graphic-{id}-{}-{axis}", s.id);
                                    let v = if axis == 0 { off.x } else { off.y };
                                    if let Some(new) = mm_field(ui, &mut self.authoring.grading.drafts, key, v, 40.) {
                                        let mut rule = current.clone();
                                        let mut o = off;
                                        if axis == 0 { o.x = new } else { o.y = new }
                                        if o == DVec2::ZERO {
                                            rule.offsets.remove(&s.id);
                                        } else {
                                            rule.offsets.insert(s.id.clone(), o);
                                        }
                                        let gid = id.clone();
                                        changes.push(edit("Graphic grading", move |g, _| g.set_graphic_rule(&gid, rule)));
                                    }
                                }
                                let scale = current.scales.get(&s.id).copied().unwrap_or(1.);
                                let key = format!("graphic-{id}-{}-scale", s.id);
                                // Percent, typed like millimetres: 105 is 105 %.
                                if let Some(new) = mm_field(ui, &mut self.authoring.grading.drafts, key, scale * 0.1, 40.) {
                                    let mut rule = current.clone();
                                    let pct = new * 10.;
                                    if (pct - 1.).abs() < 1e-9 {
                                        rule.scales.remove(&s.id);
                                    } else {
                                        rule.scales.insert(s.id.clone(), pct);
                                    }
                                    let gid = id.clone();
                                    changes.push(edit("Graphic grading", move |g, _| g.set_graphic_rule(&gid, rule)));
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
        }
    }

    // --- Points of measure --------------------------------------------------------------

    fn pom_controls(&mut self, ui: &mut egui::Ui, changes: &mut Vec<Change>) {
        let ui_lang = self.prefs.language;
        let tr = |text| i18n::tr(ui_lang, text);
        let template = &self.project.template;
        let Some(grading) = &template.grading else {
            ui.label(tr("Points of measure are read in every size: create sizes first (2D Pattern › Grading › Sizes and grade rules)."));
            if ui.button(tr("Sizes and grade rules…")).clicked() {
                self.authoring.grading.open = true;
            }
            return;
        };
        let rows = pom::table(template);
        let mut by_group: BTreeMap<String, Vec<&pom::Row>> = BTreeMap::new();
        for row in &rows {
            by_group.entry(row.group.clone()).or_default().push(row);
        }
        if rows.is_empty() {
            ui.label(tr(
                "No points of measure yet. Add one from the selection below.",
            ));
        }
        for (group_id, rows) in by_group {
            let Some(group) = grading.group(&group_id) else {
                continue;
            };
            ui.label(workspace::section(&group.name));
            let sizes: Vec<_> = group.visible().cloned().collect();
            egui::Grid::new(("pom-table", &group.id))
                .striped(true)
                .show(ui, |ui| {
                    ui.strong(tr("Measurement"));
                    for s in &sizes {
                        let mut text = egui::RichText::new(&s.name).strong();
                        if group.is_base(&s.id) {
                            text = text.underline();
                        }
                        ui.label(text);
                    }
                    ui.strong(tr("Tolerance ±"));
                    ui.label("");
                    ui.end_row();
                    for row in rows {
                        let key = format!("pom-name-{}", row.id);
                        let s = &mut self.authoring.grading;
                        let mut name = s.drafts.get(&key).cloned().unwrap_or(row.name.clone());
                        let r = ui.add_sized(
                            egui::vec2(150., ui.spacing().interact_size.y),
                            egui::TextEdit::singleline(&mut name),
                        );
                        if r.changed() {
                            s.drafts.insert(key.clone(), name.clone());
                        }
                        if r.lost_focus() {
                            s.drafts.remove(&key);
                            if name.trim() != row.name {
                                let id = row.id.clone();
                                changes.push(edit("Rename measurement", move |g, _| {
                                    let m =
                                        g.measurement_mut(&id).ok_or("No such point of measure")?;
                                    m.name = name.trim().into();
                                    m.validate()
                                }));
                            }
                        }
                        for s in &sizes {
                            let value = row
                                .values
                                .iter()
                                .find(|(id, _)| id == &s.id)
                                .map(|(_, v)| v);
                            match value {
                                Some(Ok(v)) => {
                                    let text = if group.is_base(&s.id) {
                                        format!("{:.1}", v * 1000.)
                                    } else {
                                        let d = row.difference(&s.id).unwrap_or(0.);
                                        format!("{:.1} ({:+.1})", v * 1000., d * 1000.)
                                    };
                                    ui.label(text);
                                }
                                Some(Err(e)) => {
                                    ui.colored_label(Color32::from_rgb(230, 70, 60), "—")
                                        .on_hover_text(e);
                                }
                                None => {
                                    ui.label("");
                                }
                            }
                        }
                        let key = format!("pom-tol-{}", row.id);
                        if let Some(new) = mm_field(
                            ui,
                            &mut self.authoring.grading.drafts,
                            key,
                            row.tolerance_m,
                            46.,
                        ) {
                            let id = row.id.clone();
                            changes.push(edit("Measurement tolerance", move |g, _| {
                                let m = g.measurement_mut(&id).ok_or("No such point of measure")?;
                                m.tolerance_m = new.abs();
                                m.validate()
                            }));
                        }
                        if ui.small_button(tr("Remove")).clicked() {
                            let id = row.id.clone();
                            changes.push(edit("Remove measurement", move |g, _| {
                                g.remove_measurement(&id)
                            }));
                        }
                        ui.end_row();
                    }
                });
            ui.small(tr("mm; in brackets the difference from the base size."));
        }
        ui.separator();
        // New point of measure from the selection.
        ui.label(workspace::section(tr("New point of measure")));
        let s = &mut self.authoring.grading;
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Name"));
            ui.add(egui::TextEdit::singleline(&mut s.pom_name).desired_width(140.));
            ui.label(tr("Tolerance ±"));
            ui.add(
                egui::DragValue::new(&mut s.pom_tolerance_mm)
                    .range(0.0..=100.)
                    .speed(0.1)
                    .suffix(" mm"),
            );
        });
        let name = s.pom_name.trim().to_string();
        let tolerance = s.pom_tolerance_mm * 0.001;
        // A length: the boundaries collected, or else the selected one.
        let selected_edge = self.authoring.edge.as_ref().and_then(|e| {
            Some(Segment {
                panel: e.panel.clone(),
                edge: e.edge.clone()?,
            })
        });
        let s = &mut self.authoring.grading;
        ui.horizontal_wrapped(|ui| {
            let names: Vec<String> = s
                .pom_segments
                .iter()
                .map(|seg| {
                    let piece = template
                        .panels
                        .iter()
                        .find(|p| p.id == seg.panel)
                        .map_or(seg.panel.as_str(), |p| p.name.as_str());
                    format!("{piece} · {}", seg.edge)
                })
                .collect();
            ui.label(format!(
                "{}: {}",
                tr("Boundaries"),
                if names.is_empty() {
                    "—".to_string()
                } else {
                    names.join(" + ")
                }
            ));
            if ui
                .add_enabled(
                    selected_edge
                        .as_ref()
                        .is_some_and(|e| !s.pom_segments.contains(e)),
                    egui::Button::new(tr("Add the selected boundary")),
                )
                .clicked()
                && let Some(e) = selected_edge.clone()
            {
                s.pom_segments.push(e);
            }
            if !s.pom_segments.is_empty() && ui.button(tr("Clear")).clicked() {
                s.pom_segments.clear();
            }
        });
        let segments: Vec<Segment> = if s.pom_segments.is_empty() {
            selected_edge.into_iter().collect()
        } else {
            s.pom_segments.clone()
        };
        ui.horizontal_wrapped(|ui| {
            let label = format!("{} ({})", tr("Add length"), segments.len());
            if ui
                .add_enabled(!name.is_empty() && !segments.is_empty(), egui::Button::new(label))
                .on_hover_text(tr("Length along the boundaries, added up; the gaps between pieces are not counted"))
                .clicked()
            {
                let n = name.clone();
                changes.push(edit("Add measurement", move |g, _| {
                    g.add_measurement(&n, MeasureKind::Length { segments }, tolerance).map(|_| ())
                }));
                s.pom_name.clear();
                s.pom_segments.clear();
            }
        });
        // A distance between two points of the selected piece.
        let panel = self
            .authoring
            .selected
            .as_deref()
            .and_then(|id| template.panels.iter().find(|p| p.id == id));
        if let Some((panel, poly)) = panel.and_then(|p| Some((p, p.polygon.as_ref()?))) {
            let s = &mut self.authoring.grading;
            if let Some((id, k)) = &self.authoring.anchor
                && id == &panel.id
                && s.pom_from.is_none()
            {
                s.pom_from = poly.edges.get(*k).cloned();
            }
            let label = |edge: &Option<String>| {
                edge.as_ref()
                    .and_then(|e| poly.edges.iter().position(|x| x == e))
                    .map_or("—".to_string(), |k| format!("P{}", k + 1))
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{} · {}",
                    tr("Distance between two points of"),
                    panel.name
                ));
                for (which, salt) in [(&mut s.pom_from, "pom-from"), (&mut s.pom_to, "pom-to")] {
                    egui::ComboBox::from_id_salt(salt)
                        .selected_text(label(which))
                        .width(60.)
                        .show_ui(ui, |ui| {
                            for (k, e) in poly.edges.iter().enumerate() {
                                ui.selectable_value(which, Some(e.clone()), format!("P{}", k + 1));
                            }
                        });
                }
                let ready = !name.is_empty()
                    && s.pom_from.is_some()
                    && s.pom_to.is_some()
                    && s.pom_from != s.pom_to;
                if ui
                    .add_enabled(ready, egui::Button::new(tr("Add distance")))
                    .clicked()
                {
                    let kind = pom::distance(
                        &panel.id,
                        s.pom_from.as_deref().unwrap_or_default(),
                        s.pom_to.as_deref().unwrap_or_default(),
                    );
                    let n = name.clone();
                    changes.push(edit("Add measurement", move |g, _| {
                        g.add_measurement(&n, kind, tolerance).map(|_| ())
                    }));
                    s.pom_name.clear();
                    s.pom_from = None;
                    s.pom_to = None;
                }
            });
        } else {
            ui.small(tr("Select a piece (and a point) for a distance; select one or more boundaries for a length."));
        }
        ui.separator();
        if ui
            .add_enabled(
                !rows.is_empty(),
                egui::Button::new(tr("Save the table as CSV…")),
            )
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(format!(
                    "{}-pom.csv",
                    fashioncraft::garment::dxf_aama::template::file_stem(&template.name)
                ))
                .save_file()
        {
            self.status = match std::fs::write(&path, pom::csv(template)) {
                Ok(()) => format!("{} {}", tr("Saved"), path.display()),
                Err(e) => e.to_string(),
            };
        }
    }
}

impl Atelier {
    /// The size chart for agents: groups, sizes, the size on show and the points of measure.
    pub(super) fn size_chart(&self) -> serde_json::Value {
        let t = &self.project.template;
        let Some(grading) = &t.grading else {
            return serde_json::json!({"graded": false,
                "note": "The garment is drawn in one size; 2D Pattern › Grading › Sizes and grade rules adds sizes"});
        };
        let mm = |v: f64| (v * 1e4).round() / 10.;
        let groups: Vec<_> = grading
            .groups
            .iter()
            .map(|g| {
                let pieces: Vec<&str> = t
                    .panels
                    .iter()
                    .filter(|p| grading.group_of(&p.id).is_some_and(|x| x.id == g.id))
                    .map(|p| p.name.as_str())
                    .collect();
                serde_json::json!({
                    "name": g.name,
                    "sizes": g.sizes.iter().map(|s| serde_json::json!({"name": s.name, "hidden": s.hidden})).collect::<Vec<_>>(),
                    "base": g.base_name(),
                    "shown_in_3d": g.name_of(g.active()),
                    "pieces": pieces,
                })
            })
            .collect();
        let measurements: Vec<_> = pom::table(t)
            .into_iter()
            .map(|row| {
                let group = grading.group(&row.group);
                let values: serde_json::Map<String, serde_json::Value> = row
                    .values
                    .iter()
                    .map(|(s, v)| {
                        let name = group.map_or(s.as_str(), |g| g.name_of(s)).to_string();
                        let value = match v {
                            Ok(v) => serde_json::json!({"mm": mm(*v), "from_base_mm": row.difference(s).map(mm)}),
                            Err(e) => serde_json::json!({"error": e}),
                        };
                        (name, value)
                    })
                    .collect();
                serde_json::json!({"name": row.name, "tolerance_mm": mm(row.tolerance_m), "sizes": values})
            })
            .collect();
        serde_json::json!({"graded": true, "groups": groups, "points_of_measure": measurements})
    }
}

/// Spanish for the grading interface; `None` for text that is not its own.
pub(super) fn spanish(text: &str) -> Option<&'static str> {
    Some(match text {
        "Grading" => "Escalado",
        "Sizes and grade rules…" => "Tallas y reglas de escalado…",
        "Points of measure (POM)…" => "Medidas del patrón (POM)…",
        "Points of measure" => "Medidas del patrón",
        "Grade point on / off" => "Punto de escalado sí / no",
        "Review all sizes on the board" => "Revisar todas las tallas en el tablero",
        "Next size" => "Talla siguiente",
        "Previous size" => "Talla anterior",
        "Grading: sizes and grade rules…" => "Escalado: tallas y reglas…",
        "Points of measure (POM): measurements in every size…" => {
            "Medidas del patrón (POM): medidas en cada talla…"
        }
        "Grade point: make the selected point a grade point or a proportional one" => {
            "Punto de escalado: convertir el punto seleccionado en punto de escalado o en proporcional"
        }
        "Grading review: every size laid over the base on the board" => {
            "Revisión de escalado: todas las tallas sobre la base en el tablero"
        }
        "Show the next size in 3D" => "Mostrar en 3D la talla siguiente",
        "Show the previous size in 3D" => "Mostrar en 3D la talla anterior",
        "Sizes" => "Tallas",
        "Size" => "Talla",
        "Base size" => "Talla base",
        "Base" => "Base",
        "Shown" => "Visible",
        "In 3D" => "En 3D",
        "Size group" => "Grupo de tallas",
        "name" => "nombre",
        "Rename" => "Renombrar",
        "New group" => "Grupo nuevo",
        "Remove group" => "Quitar grupo",
        "Remove" => "Quitar",
        "Insert after" => "Insertar detrás",
        "new size" => "talla nueva",
        "Add larger size" => "Añadir talla mayor",
        "Add smaller size" => "Añadir talla menor",
        "Pieces" => "Piezas",
        "Create sizes" => "Crear tallas",
        "Remove grading" => "Quitar el escalado",
        "Grade rules" => "Reglas de escalado",
        "Point" => "Punto",
        "Grade" => "Escalar",
        "Equal step" => "Salto igual",
        "Curve" => "Curva",
        "Step" => "Paso",
        "Notches" => "Piquetes",
        "Boundary" => "Tramo",
        "Place" => "Lugar",
        "Follows" => "Sigue",
        "Graphics" => "Gráficos",
        "Graphic" => "Gráfico",
        "Proportional" => "Proporcional",
        "Fixed from the start" => "Fijo desde el inicio",
        "Fixed distance from the end" => "A distancia fija del final",
        "Measurement" => "Medida",
        "Tolerance ±" => "Tolerancia ±",
        "Name" => "Nombre",
        "Add distance" => "Añadir distancia",
        "Saved" => "Guardado",
        "All sizes" => "Todas las tallas",
        "New point of measure" => "Medida nueva",
        "Seams in each size" => "Costuras en cada talla",
        "Save the table as CSV…" => "Guardar la tabla en CSV…",
        "Length of the selected boundaries" => "Largo de los tramos seleccionados",
        "Distance between two points of" => "Distancia entre dos puntos de",
        "mm; in brackets the difference from the base size." => {
            "mm; entre paréntesis, la diferencia con la talla base."
        }
        "The garment is drawn in one size. List its sizes, smallest first, and choose the base: the size the pieces are drawn in." => {
            "La prenda está dibujada en una talla. Escribe sus tallas, de menor a mayor, y elige la base: la talla en la que están dibujadas las piezas."
        }
        "The base size must be one of the sizes" => "La talla base tiene que ser una de las tallas",
        "A new group with the same sizes; assign pieces to it below" => {
            "Un grupo nuevo con las mismas tallas; asígnale piezas más abajo"
        }
        "The size the pieces are drawn in" => "La talla en la que están dibujadas las piezas",
        "A size between this one and the next, its growths halfway" => {
            "Una talla entre esta y la siguiente, con crecimientos a medio camino"
        }
        "Grade the selected pieces in this group" => {
            "Escalar las piezas seleccionadas en este grupo"
        }
        "Both sides of every seam keep the same length in every size." => {
            "Los dos lados de cada costura miden lo mismo en todas las tallas."
        }
        "A seam of unequal sides eases in that size when simulated." => {
            "Una costura de lados desiguales se embebe en esa talla al simular."
        }
        "Back to one size; Undo brings the sizes and rules back" => {
            "Vuelve a una sola talla; Deshacer recupera las tallas y las reglas"
        }
        "Select a piece to see its grade rules." => {
            "Selecciona una pieza para ver sus reglas de escalado."
        }
        "This piece has no editable outline to grade." => {
            "Esta pieza no tiene un contorno editable que escalar."
        }
        "The selected piece is graded in another size group." => {
            "La pieza seleccionada se escala en otro grupo de tallas."
        }
        "Growth from the base size, mm: X across, Y down the board. Squares on the board are grade points; rings follow the grade points around them. Fields take sums such as 10+2." => {
            "Crecimiento desde la talla base, en mm: X a lo ancho, Y hacia abajo en el tablero. Los cuadrados del tablero son puntos de escalado; los círculos siguen a los puntos de escalado de alrededor. Los campos aceptan cálculos como 10+2."
        }
        "Move the selected point in the size shown in 3D" => {
            "Mover el punto seleccionado en la talla que se ve en 3D"
        }
        "Select a point and show a size other than the base to use the arrows." => {
            "Selecciona un punto y muestra una talla distinta de la base para usar las flechas."
        }
        "A grade point grows by its own rule; a proportional point follows the grade points around it" => {
            "Un punto de escalado crece según su propia regla; un punto proporcional sigue a los puntos de escalado de alrededor"
        }
        "Equal step: every size grows from the one before by what the size above the base grows" => {
            "Salto igual: cada talla crece desde la anterior lo que crece la talla siguiente a la base"
        }
        "How each notch follows its boundary; the extra distance moves it away from the end it keeps (from the start for proportional ones)." => {
            "Cómo sigue cada piquete a su tramo; la distancia extra lo aleja del extremo que conserva (del inicio en los proporcionales)."
        }
        "Graphics follow the piece. Add an offset (mm) or a size (% of the base, proportions locked) per size." => {
            "Los gráficos siguen a la pieza. Añade un desplazamiento (mm) o un tamaño (% de la base, con la proporción bloqueada) por talla."
        }
        "Points of measure are read in every size: create sizes first (2D Pattern › Grading › Sizes and grade rules)." => {
            "Las medidas del patrón se leen en cada talla: crea antes las tallas (Patrón 2D › Escalado › Tallas y reglas de escalado)."
        }
        "No points of measure yet. Add one from the selection below." => {
            "Aún no hay medidas. Añade una desde la selección, abajo."
        }
        "Select a piece (and a point) for a distance; select one or more boundaries for a length." => {
            "Selecciona una pieza (y un punto) para una distancia; selecciona uno o más tramos para un largo."
        }
        "Create sizes first: 2D Pattern › Grading › Sizes and grade rules" => {
            "Crea antes las tallas: Patrón 2D › Escalado › Tallas y reglas de escalado"
        }
        "Select a contour point on the board first" => {
            "Selecciona antes un punto del contorno en el tablero"
        }
        "Grade point: it grows by its own rule" => "Punto de escalado: crece según su propia regla",
        "Proportional point: it follows the grade points around it" => {
            "Punto proporcional: sigue a los puntos de escalado de alrededor"
        }
        "The size shown and simulated in 3D; the board edits the base size" => {
            "La talla que se ve y se simula en 3D; el tablero edita la talla base"
        }
        "Show this size in 3D" => "Ver esta talla en 3D",
        "Lay every shown size over the base on the board" => {
            "Superponer en el tablero todas las tallas visibles sobre la base"
        }
        "the board and edits stay in the base size" => {
            "el tablero y las ediciones siguen en la talla base"
        }
        "Grade rules out" => "Reglas de escalado",
        "Grade rules: " => "Reglas de escalado: ",
        "rules" => "reglas",
        "The pieces carry grade rule numbers, but no .rul file is beside the DXF: they come in ungraded." => {
            "Las piezas llevan números de regla, pero no hay un archivo .rul junto al DXF: entran sin escalar."
        }
        "Import the grade rules" => "Importar las reglas de escalado",
        "Rule table" => "Tabla de reglas",
        "Each piece is a block with its cut and sew lines, notches, grain line, name, size and quantity. Curves are written as points within 0.25 mm. A graded garment goes out in its base size, with its grade rules in a .rul file beside the DXF." => {
            "Cada pieza es un bloque con sus líneas de corte y de costura, piquetes, hilo, nombre, talla y cantidad. Las curvas se escriben como puntos a menos de 0,25 mm. Una prenda escalada sale en su talla base, con sus reglas de escalado en un archivo .rul junto al DXF."
        }
        "Boundaries" => "Tramos",
        "Add the selected boundary" => "Añadir el tramo seleccionado",
        "Clear" => "Vaciar",
        "Add length" => "Añadir largo",
        "Length along the boundaries, added up; the gaps between pieces are not counted" => {
            "Largo a lo largo de los tramos, sumados; no cuenta el hueco entre piezas"
        }
        // The grading demo's captions.
        "Grading: 2D Pattern › Grading › Sizes and grade rules" => {
            "Escalado: Patrón 2D › Escalado › Tallas y reglas de escalado"
        }
        "Sizes S M L XL, base M" => "Tallas S M L XL, base M",
        "Type a growth as a sum: 10+2 mm across for the hem's corner in L" => {
            "Escribe un crecimiento como cálculo: 10+2 mm a lo ancho para la esquina del bajo en L"
        }
        "An equal step between sizes" => "Un salto igual entre tallas",
        "Show XL in 3D from the 2D bar" => "Ver la XL en 3D desde la barra 2D",
        "Drape XL" => "Drapear la XL",
        "Every size over the base" => "Todas las tallas sobre la base",
        "Undo takes the rules back; the size on show stays" => {
            "Deshacer quita las reglas; la talla visible se queda"
        }
        "Points of measure in every size" => "Medidas del patrón en cada talla",
        // Edit names, as the history lists them.
        "Grade rule" => "Regla de escalado",
        "Grade point" => "Punto de escalado",
        "Rename size" => "Renombrar talla",
        "Rename size group" => "Renombrar grupo de tallas",
        "New size group" => "Grupo de tallas nuevo",
        "Remove size group" => "Quitar grupo de tallas",
        "Show size" => "Mostrar talla",
        "Hide size" => "Ocultar talla",
        "Insert size" => "Insertar talla",
        "Add size" => "Añadir talla",
        "Remove size" => "Quitar talla",
        "Assign size group" => "Asignar grupo de tallas",
        "Notch grading" => "Escalado de piquete",
        "Graphic grading" => "Escalado de gráfico",
        "Add measurement" => "Añadir medida",
        "Rename measurement" => "Renombrar medida",
        "Measurement tolerance" => "Tolerancia de la medida",
        "Remove measurement" => "Quitar medida",
        _ => return None,
    })
}
