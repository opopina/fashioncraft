//! Drafting tools only issue CPU transactions; they never touch the solver.
use super::{authoring_ui::Tool, *};
use fashioncraft::{
    authoring::{self, Command, SpreadSide},
    garment::{
        meshing::MeshingSettings,
        shapes::{BooleanOp, MirrorAxis, Primitive},
        template::Panel,
    },
};
use glam::DVec2;

pub(super) struct DraftingUi {
    kind: Primitive,
    size_mm: [f64; 2],
    pub sweep: f64,
    operand: String,
    keep_tool: bool,
    width_mm: f64,
    ratio: f64,
    pub line_panel: Option<String>,
    /// Slash and spread: the wedge's angle and which part turns.
    spread_degrees: f64,
    spread_side: SpreadSide,
}
impl Default for DraftingUi {
    fn default() -> Self {
        Self {
            kind: Primitive::Ellipse,
            size_mm: [300., 300.],
            sweep: 180.,
            operand: String::new(),
            keep_tool: false,
            width_mm: 80.,
            ratio: 1.5,
            line_panel: None,
            spread_degrees: 10.,
            spread_side: SpreadSide::Right,
        }
    }
}
impl Atelier {
    pub(super) fn primitive_menu(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        ui.menu_button(i18n::tr(ui_lang, "Shapes"), |ui| {
            let d = &mut self.authoring.drafting;
            ui.horizontal(|ui| {
                for (kind, label) in [
                    (Primitive::Rectangle, "Rectangle"),
                    (Primitive::Ellipse, "Circle / ellipse"),
                    (Primitive::Sector, "Arc sector"),
                ] {
                    ui.selectable_value(&mut d.kind, kind, i18n::tr(ui_lang, label));
                }
            });
            for (label, value) in ["Width", "Height"].into_iter().zip(d.size_mm.iter_mut()) {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(value)
                            .range(5. ..=10_000.)
                            .suffix(" mm"),
                    );
                });
            }
            if d.kind == Primitive::Ellipse
                && ui
                    .small_button(i18n::tr(ui_lang, "Circle: H = W"))
                    .clicked()
            {
                d.size_mm[1] = d.size_mm[0];
            }
            if d.kind == Primitive::Sector {
                ui.add(
                    egui::Slider::new(&mut d.sweep, 5. ..=355.).text(i18n::tr(ui_lang, "Angle")),
                );
                ui.small(i18n::tr(
                    ui_lang,
                    "Size of the supporting ellipse. The sector is closed by two radii.",
                ));
            }
            if ui
                .button(i18n::tr(ui_lang, "Add at board center"))
                .clicked()
            {
                let size = DVec2::from_array(d.size_mm) / 1000.;
                let command = Command::Primitive {
                    kind: d.kind,
                    size,
                    sweep_degrees: d.sweep,
                    origin: self.authoring.board.center - size * 0.5,
                    meshing: MeshingSettings {
                        target_edge_m: self.mesh_mm * 0.001,
                        anisotropy: self.anisotropy,
                        axis_degrees: self.mesh_axis,
                        ..Default::default()
                    },
                    fabric: self.project.template.fabrics[0].id.clone(),
                    replace: false,
                };
                if self.edit(command) {
                    self.set_authoring_tool(Tool::Edit, false);
                    ui.close();
                }
            }
            self.spiral_menu(ui);
        });
    }
    pub(super) fn drafting_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        let ui_lang = self.prefs.language;
        self.linked_mirror_properties(ui, &panel.id);
        ui.collapsing(
            super::workspace::section(i18n::tr(ui_lang, "Mirrors and booleans")),
            |ui| {
                ui.small(i18n::tr(
                    ui_lang,
                    "X: left / right · Y: up / down in local pattern coordinates.",
                ));
                ui.horizontal_wrapped(|ui| {
                    for (axis, label) in [(MirrorAxis::X, "Mirror X"), (MirrorAxis::Y, "Mirror Y")]
                    {
                        if ui.button(label).clicked() {
                            self.edit(Command::Mirror {
                                panel: panel.id.clone(),
                                axis,
                                copy_side: None,
                            });
                        }
                    }
                });
                ui.label(i18n::tr(ui_lang, "Create an independent mirrored copy"));
                ui.horizontal(|ui| {
                    for (axis, side, label) in [
                        (MirrorAxis::X, -1, "−X"),
                        (MirrorAxis::X, 1, "+X"),
                        (MirrorAxis::Y, -1, "−Y"),
                        (MirrorAxis::Y, 1, "+Y"),
                    ] {
                        if ui.button(label).clicked() {
                            self.edit(Command::Mirror {
                                panel: panel.id.clone(),
                                axis,
                                copy_side: Some(side),
                            });
                        }
                    }
                });
                ui.separator();
                self.boolean_properties(ui, panel);
            },
        );
        self.internal_shape_properties(ui, panel);
        self.piece_line_tools(ui, panel);
    }
    pub(super) fn linked_mirror_properties(&mut self, ui: &mut egui::Ui, panel: &str) {
        let ui_lang = self.prefs.language;
        let link = self
            .project
            .template
            .mirror_links
            .iter()
            .find(|l| l.a == panel || l.b == panel)
            .cloned();
        if let Some(link) = link {
            let partner = if link.a == panel { link.b } else { link.a };
            ui.strong(format!("Linked mirror {:?} · {partner}", link.axis));
            ui.small(i18n::tr(ui_lang, "Edit either piece. Shape, curves, internal lines and grain update together on release. Placement and fabric stay independent."));
            if ui.button(i18n::tr(ui_lang, "Unlink mirror")).clicked() {
                self.edit(Command::UnlinkMirror {
                    panel: panel.into(),
                });
            }
        } else {
            ui.horizontal(|ui| {
                ui.label(i18n::tr(ui_lang, "Linked copy"));
                for (axis, name) in [(MirrorAxis::X, "X"), (MirrorAxis::Y, "Y")] {
                    let button = ui.button(name).on_hover_text(i18n::tr(ui_lang, "Create a mirrored partner. Later shape edits update both pieces; undo restores both."));
                    self.smoke_target_ui(ui, &format!("linked_mirror_{name}"), button.rect);
                    if button.clicked() {
                        self.edit(Command::LinkedMirror { panel: panel.into(), axis });
                    }
                }
            });
        }
    }
    fn boolean_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        let ui_lang = self.prefs.language;
        ui.label(i18n::tr(ui_lang, "Boolean · A = active panel"));
        let choices: Vec<_> = self
            .project
            .template
            .panels
            .iter()
            .filter(|p| p.id != panel.id)
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        let d = &mut self.authoring.drafting;
        if !choices.iter().any(|(id, _)| id == &d.operand) {
            d.operand = choices.first().map(|p| p.0.clone()).unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("boolean_b")
            .selected_text(
                choices
                    .iter()
                    .find(|p| p.0 == d.operand)
                    .map_or("Choose B", |p| p.1.as_str()),
            )
            .show_ui(ui, |ui| {
                for (id, name) in choices {
                    ui.selectable_value(&mut d.operand, id, name);
                }
            });
        ui.checkbox(&mut d.keep_tool, i18n::tr(ui_lang, "Keep B"));
        ui.small(i18n::tr(
            ui_lang,
            "Overlap the pieces on the 2D board. A supplies fabric and placement.",
        ));
        ui.small(i18n::tr(
            ui_lang,
            "Remove seams, pins, internal lines and target lengths first.",
        ));
        ui.horizontal_wrapped(|ui| {
            for (op, label) in [
                (BooleanOp::Union, "Union"),
                (BooleanOp::Difference, "A − B"),
                (BooleanOp::Intersection, "Intersection"),
            ] {
                if ui
                    .add_enabled(
                        !self.authoring.drafting.operand.is_empty(),
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    self.edit(Command::Boolean {
                        a: panel.id.clone(),
                        b: self.authoring.drafting.operand.clone(),
                        op,
                        keep_tool: self.authoring.drafting.keep_tool,
                    });
                }
            }
        });
    }
    pub(super) fn finish_internal_line(&mut self) {
        let Some(id) = self.authoring.drafting.line_panel.clone() else {
            self.authoring.error = Some("Select a panel before using Internal line".into());
            return;
        };
        let Some(i) = self.project.template.panels.iter().position(|p| p.id == id) else {
            self.authoring.error = Some("The target panel no longer exists; select a piece and activate Internal line again".into());
            return;
        };
        let origin = authoring::layout(&self.project.template, i);
        let angle = self.project.template.panels[i].layout_rotation_degrees;
        let command = Command::InternalLine {
            panel: id,
            points: self
                .authoring
                .draft
                .iter()
                .map(|p| authoring::rotate_2d(*p - origin, -angle))
                .collect(),
            tangents: self
                .authoring
                .tangents
                .iter()
                .map(|p| authoring::rotate_2d(*p, -angle))
                .collect(),
        };
        if self.edit(command) {
            let panel = self.authoring.selected.clone().unwrap();
            let line = self
                .project
                .template
                .panels
                .iter()
                .find(|p| p.id == panel)
                .unwrap()
                .polygon
                .as_ref()
                .unwrap()
                .lines
                .last()
                .unwrap()
                .id
                .clone();
            self.set_authoring_tool(Tool::Edit, false);
            self.authoring.edge = Some(fashioncraft::garment::seam::BoundaryRef {
                reversed: false,
                panel,
                edge: Some(line),
                side: fashioncraft::garment::seam::Side::Top,
                range: [0., 1.],
            });
            self.status =
                "Internal line created · Sew to a boundary or add a ruffle in the inspector".into();
        }
    }
    /// Slash and spread along the selected straight internal line, with the inspector's angle
    /// and side, as one undo step.
    pub(super) fn slash_spread_selected_line(&mut self) -> bool {
        let ui_lang = self.prefs.language;
        let Some((panel, line)) = self
            .authoring
            .edge
            .as_ref()
            .and_then(|e| Some((e.panel.clone(), e.edge.clone()?)))
        else {
            self.status = i18n::tr(ui_lang, "Select a straight internal line first").into();
            return false;
        };
        let d = &self.authoring.drafting;
        let command = Command::SlashSpread {
            panel,
            line,
            degrees: d.spread_degrees,
            side: d.spread_side,
        };
        if self.edit(command) {
            self.authoring.edge = None;
            self.status = i18n::tr(ui_lang, "Wedge opened along the line").into();
            return true;
        }
        false
    }

    /// Make the selected closed internal shape its piece's outline, as one undo step.
    pub(super) fn replace_outline_with_selected_shape(&mut self) -> bool {
        let ui_lang = self.prefs.language;
        let Some((panel, line)) = self
            .authoring
            .edge
            .as_ref()
            .and_then(|e| Some((e.panel.clone(), e.edge.clone()?)))
        else {
            self.status = i18n::tr(ui_lang, "Select a closed internal shape first").into();
            return false;
        };
        if self.edit(Command::ReplaceOutline { panel, line }) {
            self.authoring.edge = None;
            self.status = i18n::tr(ui_lang, "Outline replaced by the shape").into();
            return true;
        }
        false
    }

    /// Cut and sew along the selected internal line, as one undo step.
    pub(super) fn cut_and_sew_selected_line(&mut self) -> bool {
        let ui_lang = self.prefs.language;
        let Some((panel, line)) = self
            .authoring
            .edge
            .as_ref()
            .and_then(|e| Some((e.panel.clone(), e.edge.clone()?)))
        else {
            self.status = i18n::tr(ui_lang, "Select a straight internal line first").into();
            return false;
        };
        if self.edit(Command::CutAndSew { panel, line }) {
            self.authoring.edge = None;
            self.status = i18n::tr(ui_lang, "Cut in two and sewn along the cut").into();
            return true;
        }
        false
    }

    pub(super) fn internal_line_properties(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let Some(edge) = self.authoring.edge.clone() else {
            return;
        };
        let Some(name) = edge.edge.clone() else {
            return;
        };
        let Some(line) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == edge.panel)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|p| p.lines.iter().find(|l| l.id == name))
            .cloned()
        else {
            return;
        };
        ui.strong(format!("Internal line · {name}"));
        let length: f64 = line
            .samples()
            .unwrap_or_default()
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .sum();
        ui.small(format!(
            "{:.1} mm · sew in 2D / 3D · draggable points and tangents",
            length * 1000.
        ));
        if line.closed
            && ui
                .button(i18n::tr(ui_lang, "Cut out as hole"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Remove the cloth inside this closed shape",
                ))
                .clicked()
            && self.edit(Command::ConvertToHole {
                panel: edge.panel.clone(),
                line: name.clone(),
            })
        {
            self.authoring.edge = None;
            self.status = i18n::tr(
                ui_lang,
                "Hole cut · fill it back in from the piece inspector",
            )
            .into();
            return;
        }
        if line.closed && line.points.len() >= 3 {
            let id = egui::Id::new(("open_shape_segment", &edge.panel, &name));
            let segments = line.points.len();
            let mut at: usize = ui.data_mut(|d| *d.get_temp_mut_or(id, 1));
            at = at.clamp(1, segments);
            let mut open = false;
            ui.horizontal(|ui| {
                ui.label(i18n::tr(ui_lang, "Open at segment"));
                ui.add(egui::DragValue::new(&mut at).range(1..=segments));
                open = ui
                    .small_button(i18n::tr(ui_lang, "Open"))
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Remove this segment of the shape, leaving it an open line",
                    ))
                    .clicked();
            });
            ui.data_mut(|d| d.insert_temp(id, at));
            if open
                && self.edit(Command::TrimLine {
                    panel: edge.panel.clone(),
                    line: name.clone(),
                    segment: at - 1,
                })
            {
                return;
            }
        }
        if line.closed
            && ui
                .button(i18n::tr(ui_lang, "Make this the outline"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Replace the piece's outline with this shape; what lies inside keeps its place",
                ))
                .clicked()
        {
            self.replace_outline_with_selected_shape();
            return;
        }
        if line.closed
            && ui
                .button(i18n::tr(ui_lang, "Clone as piece"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "A new piece of the same fabric with this outline, next to this one",
                ))
                .clicked()
            && let Some(origin) = self.clone_origin(&edge.panel, &name)
        {
            if self.edit(Command::CloneShapeAsPanel {
                panel: edge.panel.clone(),
                line: name.clone(),
                origin,
            }) {
                // Show the new piece rather than the shape it was copied from.
                self.authoring.edge = None;
                self.status =
                    i18n::tr(ui_lang, "Shape cloned as a piece of the same fabric").into();
            }
            return;
        }
        let straight = !line.closed
            && line.points.len() == 2
            && line.tangents.iter().all(|t| t.length() < 1e-12);
        if straight
            && ui
                .button(i18n::tr(ui_lang, "Cut and sew along this line"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Split the piece in two along this line, carried on to the outline, and sew \
                     the halves together; seams crossing the cut are split, not lost",
                ))
                .clicked()
        {
            self.cut_and_sew_selected_line();
            return;
        }
        if straight {
            let d = &mut self.authoring.drafting;
            let mut open = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(i18n::tr(ui_lang, "Slash and spread"));
                ui.add(
                    egui::DragValue::new(&mut d.spread_degrees)
                        .range(0.1..=90.)
                        .fixed_decimals(1)
                        .suffix("°"),
                );
                for (label, side) in [
                    ("Left", SpreadSide::Left),
                    ("Right", SpreadSide::Right),
                    ("Both", SpreadSide::Both),
                ] {
                    ui.selectable_value(&mut d.spread_side, side, i18n::tr(ui_lang, label));
                }
                open = ui
                    .button(i18n::tr(ui_lang, "Open"))
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Slash the piece along this line and turn the chosen part about where the \
                         line leaves the outline past its end, opening a wedge at its start",
                    ))
                    .clicked();
            });
            if open && self.slash_spread_selected_line() {
                return;
            }
        }
        if !line.closed {
            let mut command = None;
            ui.horizontal_wrapped(|ui| {
                ui.label(i18n::tr(ui_lang, "Extend to outline"));
                for (label, at_start) in [("Start", true), ("End", false)] {
                    if ui
                        .small_button(label)
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Carry this end on straight up to the outline or the nearest line",
                        ))
                        .clicked()
                    {
                        command = Some(Command::ExtendLine {
                            panel: edge.panel.clone(),
                            line: name.clone(),
                            at_start,
                        });
                    }
                }
            });
            if line.points.len() >= 3 {
                let id = egui::Id::new(("split_line_at", &edge.panel, &name));
                let mut at: usize = ui.data_mut(|d| *d.get_temp_mut_or(id, 1));
                at = at.clamp(1, line.points.len() - 2);
                ui.horizontal(|ui| {
                    ui.label(i18n::tr(ui_lang, "Split at point"));
                    ui.add(egui::DragValue::new(&mut at).range(1..=line.points.len() - 2));
                    if ui
                        .small_button(i18n::tr(ui_lang, "Split"))
                        .on_hover_text(i18n::tr(ui_lang, "Cut the line in two at this point"))
                        .clicked()
                    {
                        command = Some(Command::SplitLine {
                            panel: edge.panel.clone(),
                            line: name.clone(),
                            point: at,
                        });
                    }
                });
                ui.data_mut(|d| d.insert_temp(id, at));
                let id = egui::Id::new(("trim_line_segment", &edge.panel, &name));
                let segments = line.points.len() - 1;
                let mut at: usize = ui.data_mut(|d| *d.get_temp_mut_or(id, 1));
                at = at.clamp(1, segments);
                ui.horizontal(|ui| {
                    ui.label(i18n::tr(ui_lang, "Trim segment"));
                    ui.add(egui::DragValue::new(&mut at).range(1..=segments));
                    if ui
                        .small_button(i18n::tr(ui_lang, "Cut away"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Remove this segment of the line; the parts on either side stay",
                        ))
                        .clicked()
                    {
                        command = Some(Command::TrimLine {
                            panel: edge.panel.clone(),
                            line: name.clone(),
                            segment: at - 1,
                        });
                    }
                });
                ui.data_mut(|d| d.insert_temp(id, at));
            }
            if let Some(command) = command {
                self.edit(command);
                return;
            }
        }
        self.line_group_controls(ui, &edge.panel, &name);
        self.line_spring_controls(ui, &edge.panel, &name);
        self.line_fold_controls(ui, &edge.panel, &name);
        if !line.closed {
            self.internal_line_offsets(ui, &edge.panel, &name);
        }
        if ui
            .small_button(i18n::tr(ui_lang, "Smooth tangents"))
            .clicked()
        {
            let mut poly = self
                .project
                .template
                .panels
                .iter()
                .find(|p| p.id == edge.panel)
                .unwrap()
                .polygon
                .clone()
                .unwrap();
            let l = poly.lines.iter_mut().find(|l| l.id == name).unwrap();
            for k in 0..l.points.len() {
                l.tangents[k] = (l.points[(k + 1).min(l.points.len() - 1)]
                    - l.points[k.saturating_sub(1)])
                    / 6.;
            }
            self.edit(Command::Shape {
                panel: edge.panel.clone(),
                polygon: poly,
            });
        }
        let d = &mut self.authoring.drafting;
        ui.horizontal(|ui| {
            ui.label(i18n::tr(ui_lang, "Ruffle width"));
            ui.add(
                egui::DragValue::new(&mut d.width_mm)
                    .range(5. ..=1000.)
                    .suffix(" mm"),
            );
        });
        ui.add(egui::Slider::new(&mut d.ratio, 1. ..=4.).text(i18n::tr(ui_lang, "Length ×")));
        if ui
            .button(i18n::tr(ui_lang, "Create and sew a ruffle"))
            .clicked()
        {
            let command = Command::Ruffle {
                panel: edge.panel.clone(),
                line: name.clone(),
                width: d.width_mm * 0.001,
                ratio: d.ratio,
            };
            if self.edit(command) {
                self.authoring.edge = None;
                self.set_authoring_tool(Tool::Place, false);
                self.status =
                    "Ruffle created with ease · adjust its 3D placement before simulation".into();
            }
        }
        ui.small(i18n::tr(
            ui_lang,
            "A ratio > 1 adds ease. Adjust the initial placement, especially along a curve.",
        ));
        if ui.small_button(i18n::tr(ui_lang, "Delete line")).clicked() {
            self.edit(Command::RemoveLine {
                panel: edge.panel,
                line: name,
            });
        }
        ui.separator();
    }
    pub(super) fn online_patterns(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        ui.menu_button(i18n::tr(ui_lang, "Free patterns · Web"), |ui| {
            ui.label(i18n::tr(ui_lang, "FreeSewing · parametric patterns"));
            for (label,url) in [("Teagan · T-shirt","https://freesewing.eu/designs/teagan/"),("Titan · trouser block","https://freesewing.eu/designs/titan/"),("Jett · bomber","https://freesewing.eu/designs/jett/")] {
                ui.hyperlink_to(label,url);
            }
            ui.separator();
            ui.small(i18n::tr(ui_lang, "Open the website to generate a pattern from your measurements. FreeSewing / SVG import is planned; these links do not add pieces to the scene."));
            ui.hyperlink_to("FreeSewing terms of use","https://freesewing.eu/docs/about/faq/commercial-use/");
        });
    }
}

impl Atelier {
    /// 2D Pattern › Trace: the selected closed internal shape traced as a new piece of the
    /// same fabric, next to its piece.
    pub(super) fn trace_selected_shape(&mut self) -> Result<(), String> {
        let (panel, line) = self
            .selected_internal_line()
            .ok_or("Select a closed internal shape on the board first")?;
        let closed = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.as_ref())
            .and_then(|poly| poly.lines.iter().find(|l| l.id == line))
            .is_some_and(|l| l.closed);
        if !closed {
            return Err("Trace makes a piece from a closed internal shape; select one".into());
        }
        let origin = self
            .clone_origin(&panel, &line)
            .ok_or("There is no room for the traced piece")?;
        if !self.edit(Command::CloneShapeAsPanel {
            panel,
            line,
            origin,
        }) {
            return Err(self.status.clone());
        }
        self.authoring.edge = None;
        self.status = "Shape traced as a new piece of the same fabric".into();
        Ok(())
    }

    /// 2D Pattern › Edit › Cut and extend: the selected open internal line prolonged from the
    /// end nearer the outline until it reaches it, and a point added to the outline there, so
    /// the piece can later be cut or sewn at that point.
    pub(super) fn cut_and_extend_selected(&mut self) -> Result<(), String> {
        use fashioncraft::garment::lines2d;
        let (panel, line) = self
            .selected_internal_line()
            .ok_or("Select an open internal line on the board first")?;
        let polygon = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.clone())
            .ok_or("The piece has no outline")?;
        // The end that reaches the outline soonest.
        let current = |at_start: bool| {
            let l = polygon.lines.iter().find(|l| l.id == line)?;
            if at_start {
                l.points.first().copied()
            } else {
                l.points.last().copied()
            }
        };
        let reach = |at_start: bool| {
            let to = lines2d::extended_end(&polygon, &line, at_start).ok()?;
            Some((to.distance(current(at_start)?), to, at_start))
        };
        let (_, end, at_start) = [reach(true), reach(false)]
            .into_iter()
            .flatten()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .ok_or("Neither end of this line can be prolonged")?;
        // The outline segment the prolonged end meets, and how far along it.
        let mut nearest: Option<(f64, String, f64)> = None;
        for edge in &polygon.edges {
            let Ok(path) = lines2d::path(&polygon, edge) else {
                continue;
            };
            let mut along = 0.;
            for w in path.points.windows(2) {
                let d = w[1] - w[0];
                let length = d.length();
                if length < 1e-12 {
                    continue;
                }
                let t = ((end - w[0]).dot(d) / (length * length)).clamp(0., 1.);
                let distance = (w[0] + d * t).distance(end);
                if nearest.as_ref().is_none_or(|n| distance < n.0) {
                    nearest = Some((distance, edge.clone(), along + t * length));
                }
                along += length;
            }
        }
        let (distance, edge, at_m) = nearest.ok_or("The piece has no outline")?;
        if distance > 0.002 {
            return Err("This line reaches another internal line, not the outline; use Extend line to outline".into());
        }
        if !self.edit(Command::ExtendLine {
            panel: panel.clone(),
            line: line.clone(),
            at_start,
        }) {
            return Err(self.status.clone());
        }
        if !self.edit(Command::SplitBoundary {
            panel,
            edge: edge.clone(),
            at_m,
        }) {
            return Err(format!(
                "The line reaches the outline, but no point could be added on {edge}: {}",
                self.status
            ));
        }
        self.status = format!("Line prolonged to {edge}, with a new point there");
        Ok(())
    }
}
