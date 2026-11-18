//! The selected contour segment: change its length from either end or both, match its length
//! with another segment, match another piece up against it, or move it along its normal.
//! "Match" actions wait for a click on the second segment; Esc cancels. One command each.
use super::*;
use glam::DVec2;
use marvelous_cloth::{
    authoring::{
        self, Command,
        arrange2d::{self, SegmentEnd},
    },
    garment::{polygon::PolygonPattern, template::Panel},
};

/// What the next click on a contour segment does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PickAction {
    /// Match the two lengths (the shorter or the longer one is copied).
    MatchLength,
    /// Bring the clicked segment's piece up against the first segment.
    MatchUp,
    /// Group the clicked internal line with the first one.
    GroupLine,
}

/// A segment action waiting for its second segment.
pub(super) struct SegmentPick {
    pub action: PickAction,
    pub panel: String,
    pub edge: String,
}

pub(super) struct SegmentsUi {
    /// The segment the length field was loaded from; another segment reloads it.
    length_for: Option<(String, String)>,
    length_mm: f64,
    moving: SegmentEnd,
    to_longer: bool,
    anchor: SegmentEnd,
    offset_mm: f64,
    pub pick: Option<SegmentPick>,
    /// Piece whose outline is copied as an internal shape.
    pub clone_source: Option<String>,
    /// Offset copies of the segment or outline.
    copies: u32,
    copy_mm: f64,
    extend: bool,
    reverse: bool,
    /// Line leaving the segment: position from its start (`None`: the middle), angle, and
    /// length (`None`: up to the outline).
    branch_for: Option<(String, String)>,
    branch_at_mm: f64,
    branch_angle: f64,
    branch_length_mm: Option<f64>,
    /// Clean-up: overlap distance and the straightness limit for straight-through points.
    overlap_mm: f64,
    straight: bool,
    straight_degrees: f64,
    /// Share of curve points kept by "Optimize", in percent, and the piece it was suggested for.
    curve_percent: f64,
    curve_for: Option<String>,
}

impl Default for SegmentsUi {
    fn default() -> Self {
        Self {
            length_for: None,
            length_mm: 0.,
            moving: SegmentEnd::End,
            to_longer: false,
            anchor: SegmentEnd::Start,
            offset_mm: 10.,
            pick: None,
            clone_source: None,
            copies: 1,
            copy_mm: 10.,
            extend: true,
            reverse: false,
            branch_for: None,
            branch_at_mm: 0.,
            branch_angle: 90.,
            branch_length_mm: None,
            overlap_mm: 0.1,
            straight: false,
            straight_degrees: 2.,
            curve_percent: 50.,
            curve_for: None,
        }
    }
}

/// How far an optimized outline may move by default, like an automatic suggestion.
const CURVE_TOLERANCE_M: f64 = 0.0005;

fn pick_prompt(action: PickAction) -> &'static str {
    match action {
        PickAction::MatchLength => "Click the segment to match lengths with · Esc cancels",
        PickAction::MatchUp => "Click a segment of the piece to bring here · Esc cancels",
        PickAction::GroupLine => "Click the internal line to group with · Esc cancels",
    }
}

fn end_label(end: SegmentEnd) -> &'static str {
    match end {
        SegmentEnd::Start => "Start",
        SegmentEnd::Center => "Both",
        SegmentEnd::End => "End",
    }
}

impl Atelier {
    /// Segment section of the inspector for the selected contour boundary.
    pub(super) fn segment_properties(
        &mut self,
        ui: &mut egui::Ui,
        panel: &Panel,
        polygon: &PolygonPattern,
    ) {
        let Some(name) = self
            .authoring
            .edge
            .as_ref()
            .filter(|edge| edge.panel == panel.id)
            .and_then(|edge| edge.edge.clone())
        else {
            return;
        };
        let Some(k) = polygon.edges.iter().position(|e| e == &name) else {
            return;
        };
        let Ok(measured) = authoring::edge_length(polygon, &name) else {
            return;
        };
        let state = &mut self.authoring.segments;
        let key = (panel.id.clone(), name.clone());
        if state.length_for.as_ref() != Some(&key) {
            state.length_for = Some(key);
            state.length_mm = measured * 1000.;
        }
        let mut command = None;
        let mut elastic_ui = false;
        egui::CollapsingHeader::new("Segment")
            .id_salt("segment-tools")
            .default_open(true)
            .show(ui, |ui| {
                let n = polygon.points.len();
                ui.small(format!(
                    "From point {} to point {} · {:.1} mm",
                    k + 1,
                    (k + 1) % n + 1,
                    measured * 1000.
                ));
                elastic_ui = true;
                ui.horizontal(|ui| {
                    ui.label("Length");
                    ui.add(
                        egui::DragValue::new(&mut state.length_mm)
                            .range(1.0..=50_000.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Moves");
                    for end in SegmentEnd::ALL {
                        ui.selectable_value(&mut state.moving, end, end_label(end))
                            .on_hover_text(match end {
                                SegmentEnd::Start => "The start point moves; the end stays",
                                SegmentEnd::Center => "Both ends move by the same amount",
                                SegmentEnd::End => "The end point moves; the start stays",
                            });
                    }
                });
                if ui.button("Change length").clicked() {
                    command = Some(Command::SetLength {
                        panel: panel.id.clone(),
                        edge: name.clone(),
                        length_m: state.length_mm / 1000.,
                        moving: state.moving,
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Match length to the");
                    ui.selectable_value(&mut state.to_longer, false, "shorter");
                    ui.selectable_value(&mut state.to_longer, true, "longer");
                });
                if ui
                    .button("Pick the other segment…")
                    .on_hover_text(
                        "Click a second segment: the one that differs takes the other's length, \
                         moving the end chosen above",
                    )
                    .clicked()
                {
                    state.pick = Some(SegmentPick {
                        action: PickAction::MatchLength,
                        panel: panel.id.clone(),
                        edge: name.clone(),
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Match up at");
                    for end in SegmentEnd::ALL {
                        let label = match end {
                            SegmentEnd::Start => "start",
                            SegmentEnd::Center => "centre",
                            SegmentEnd::End => "end",
                        };
                        ui.selectable_value(&mut state.anchor, end, label);
                    }
                });
                if ui
                    .button("Pick a segment on another piece…")
                    .on_hover_text(
                        "That piece turns and moves so its segment lies along this one, the other \
                         way round, as if sewn edge to edge. Start: its start meets this segment's \
                         end. End: its end meets this segment's start.",
                    )
                    .clicked()
                {
                    state.pick = Some(SegmentPick {
                        action: PickAction::MatchUp,
                        panel: panel.id.clone(),
                        edge: name.clone(),
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Move out");
                    ui.add(
                        egui::DragValue::new(&mut state.offset_mm)
                            .range(-1000.0..=1000.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .on_hover_text("Negative values move the segment into the piece");
                    if ui.button("Move").clicked() {
                        command = Some(Command::MovePoints {
                            panel: panel.id.clone(),
                            points: vec![k, (k + 1) % n],
                            delta: arrange2d::outward_normal(polygon, k) * state.offset_mm / 1000.,
                        });
                    }
                });
                ui.separator();
                ui.strong("Internal lines from this segment");
                ui.horizontal(|ui| {
                    ui.label("Copies");
                    ui.add(egui::DragValue::new(&mut state.copies).range(1..=50));
                    ui.label("every");
                    ui.add(
                        egui::DragValue::new(&mut state.copy_mm)
                            .range(1.0..=1000.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.extend, "Extend to outline")
                        .on_hover_text("Run each copy up to the outline instead of stopping at the segment's ends");
                    if ui
                        .button("Offset")
                        .on_hover_text("Parallel copies of this segment inside the piece")
                        .clicked()
                    {
                        command = Some(Command::OffsetAsLines {
                            panel: panel.id.clone(),
                            path: Some(name.clone()),
                            count: state.copies,
                            distance_m: state.copy_mm / 1000.,
                            reverse: false,
                            extend: state.extend,
                        });
                    }
                });
                let key = (panel.id.clone(), name.clone());
                if state.branch_for.as_ref() != Some(&key) {
                    state.branch_for = Some(key);
                    state.branch_at_mm = measured * 500.;
                }
                ui.horizontal(|ui| {
                    ui.label("Line at");
                    ui.add(
                        egui::DragValue::new(&mut state.branch_at_mm)
                            .range(0.0..=measured * 1000.)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .on_hover_text("Distance from the segment's start");
                    ui.label("angle");
                    ui.add(
                        egui::DragValue::new(&mut state.branch_angle)
                            .range(1.0..=179.0)
                            .fixed_decimals(1)
                            .suffix("°"),
                    )
                    .on_hover_text("90° is perpendicular; smaller angles lean towards the end");
                });
                ui.horizontal(|ui| {
                    let mut to_outline = state.branch_length_mm.is_none();
                    ui.checkbox(&mut to_outline, "Up to outline");
                    if to_outline {
                        state.branch_length_mm = None;
                    } else {
                        let length = state.branch_length_mm.get_or_insert(100.);
                        ui.add(
                            egui::DragValue::new(length)
                                .range(2.0..=10_000.0)
                                .fixed_decimals(1)
                                .suffix(" mm"),
                        );
                    }
                    if ui.button("Add line").clicked() {
                        command = Some(Command::PerpendicularLine {
                            panel: panel.id.clone(),
                            path: name.clone(),
                            at_m: state.branch_at_mm / 1000.,
                            angle_degrees: state.branch_angle,
                            length_m: state.branch_length_mm.map(|l| l / 1000.),
                            reverse: false,
                        });
                    }
                });
                if state.pick.is_some() {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(0x15, 0x65, 0xC0), "Click a segment");
                        if ui.small_button("Cancel").clicked() {
                            state.pick = None;
                        }
                    });
                }
            });
        if elastic_ui {
            ui.indent("segment-elastic", |ui| {
                self.line_spring_controls(ui, &panel.id, &name);
            });
        }
        if let Some(pick) = &self.authoring.segments.pick {
            self.status = pick_prompt(pick.action).into();
        }
        if let Some(command) = command {
            self.edit(command);
        }
    }

    /// Finish a waiting segment action with the clicked contour segment (`contour`) or
    /// internal line. Returns true when the click was used.
    pub(super) fn finish_segment_pick(
        &mut self,
        panel: String,
        edge: String,
        contour: bool,
    ) -> bool {
        let Some(action) = self.authoring.segments.pick.as_ref().map(|p| p.action) else {
            return false;
        };
        if contour != (action != PickAction::GroupLine) {
            self.status = match action {
                PickAction::GroupLine => "Click another internal line of this piece · Esc cancels",
                _ => "Click a segment of a piece outline · Esc cancels",
            }
            .into();
            return true;
        }
        let pick = self.authoring.segments.pick.take().unwrap();
        let state = &self.authoring.segments;
        let command = match pick.action {
            PickAction::GroupLine => {
                if panel != pick.panel {
                    self.status = "Only lines of the same piece can be grouped".into();
                    return true;
                }
                Command::GroupLines {
                    panel,
                    lines: vec![pick.edge, edge],
                }
            }
            PickAction::MatchUp => Command::MatchUp {
                reference: (pick.panel, pick.edge),
                moved: (panel, edge),
                anchor: state.anchor,
            },
            PickAction::MatchLength => {
                let length = |id: &str, name: &str| {
                    self.project
                        .template
                        .panels
                        .iter()
                        .find(|p| p.id == id)
                        .and_then(|p| p.polygon.as_ref())
                        .and_then(|p| authoring::edge_length(p, name).ok())
                };
                let (Some(a), Some(b)) = (length(&pick.panel, &pick.edge), length(&panel, &edge))
                else {
                    return true;
                };
                let Some((which, target)) = arrange2d::matched_length([a, b], state.to_longer)
                else {
                    self.status = "The two segments already have the same length".into();
                    return true;
                };
                let (panel, edge) = if which == 0 {
                    (pick.panel, pick.edge)
                } else {
                    (panel, edge)
                };
                Command::SetLength {
                    panel,
                    edge,
                    length_m: target,
                    moving: state.moving,
                }
            }
        };
        if self.edit(command) {
            self.status = match pick.action {
                PickAction::MatchLength => "Lengths matched".into(),
                PickAction::MatchUp => "Piece matched up".into(),
                PickAction::GroupLine => "Lines grouped · they move together".into(),
            };
        }
        true
    }

    /// Group membership of the selected internal line, and the group actions.
    pub(super) fn line_group_controls(&mut self, ui: &mut egui::Ui, panel: &str, line: &str) {
        let Some(polygon) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| p.id == panel)
            .and_then(|p| p.polygon.as_ref())
        else {
            return;
        };
        let group = polygon
            .lines
            .iter()
            .find(|l| l.id == line)
            .and_then(|l| l.group.clone());
        let members = group.as_ref().map_or(0, |g| {
            polygon
                .lines
                .iter()
                .filter(|l| l.group.as_ref() == Some(g))
                .count()
        });
        let mut command = None;
        ui.horizontal(|ui| {
            match &group {
                Some(name) => {
                    ui.small(format!("Group {name} · {members} lines"));
                    if ui.small_button("Ungroup").clicked() {
                        command = Some(Command::UngroupLines {
                            panel: panel.into(),
                            line: line.into(),
                        });
                    }
                }
                None => {
                    ui.small("Not grouped");
                }
            }
            if ui
                .small_button("Group with…")
                .on_hover_text(
                    "Click another internal line of this piece; grouped lines move together",
                )
                .clicked()
            {
                self.authoring.segments.pick = Some(SegmentPick {
                    action: PickAction::GroupLine,
                    panel: panel.into(),
                    edge: line.into(),
                });
                self.status = pick_prompt(PickAction::GroupLine).into();
            }
        });
        ui.small("Drag a line on the board to move it with its group.");
        if let Some(command) = command {
            self.edit(command);
        }
    }

    /// Where a closed internal shape's copy goes: next to its piece, on the right.
    pub(super) fn clone_origin(&self, panel: &str, line: &str) -> Option<DVec2> {
        let template = &self.project.template;
        let index = template.panels.iter().position(|p| p.id == panel)?;
        let shape = template.panels[index]
            .polygon
            .as_ref()?
            .lines
            .iter()
            .find(|l| l.id == line)?;
        let left = shape
            .samples()
            .ok()?
            .into_iter()
            .map(|uv| arrange2d::board_point(template, index, uv).x)
            .fold(f64::INFINITY, f64::min);
        let (_, hi) = arrange2d::board_bounds(template, index).ok()?;
        Some(authoring::layout(template, index) + DVec2::new(hi.x - left + 0.05, 0.))
    }

    /// "Clean up points" for the active piece or every piece, and inset copies of the outline.
    pub(super) fn piece_line_tools(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        if panel.polygon.is_none() {
            return;
        }
        let all: Vec<String> = self
            .project
            .template
            .panels
            .iter()
            .filter(|p| p.polygon.is_some())
            .map(|p| p.id.clone())
            .collect();
        let mut command = None;
        egui::CollapsingHeader::new("Outline offsets and clean-up")
            .id_salt("outline-cleanup")
            .show(ui, |ui| {
                let state = &mut self.authoring.segments;
                ui.horizontal(|ui| {
                    ui.label("Copies");
                    ui.add(egui::DragValue::new(&mut state.copies).range(1..=50));
                    ui.label("every");
                    ui.add(
                        egui::DragValue::new(&mut state.copy_mm)
                            .range(1.0..=1000.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    );
                });
                if ui
                    .button("Offset outline inwards")
                    .on_hover_text("Closed internal shapes following the whole outline")
                    .clicked()
                {
                    command = Some(Command::OffsetAsLines {
                        panel: panel.id.clone(),
                        path: None,
                        count: state.copies,
                        distance_m: state.copy_mm / 1000.,
                        reverse: false,
                        extend: false,
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Overlapping within");
                    ui.add(
                        egui::DragValue::new(&mut state.overlap_mm)
                            .range(0.0..=10.0)
                            .speed(0.01)
                            .fixed_decimals(2)
                            .suffix(" mm"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.straight, "Also points on straight lines, under");
                    ui.add_enabled(
                        state.straight,
                        egui::DragValue::new(&mut state.straight_degrees)
                            .range(0.0..=10.0)
                            .fixed_decimals(1)
                            .suffix("°"),
                    );
                });
                let clean = |panels: Vec<String>| Command::CleanUpPoints {
                    panels,
                    overlap_m: state.overlap_mm / 1000.,
                    straight_degrees: state.straight.then_some(state.straight_degrees),
                };
                ui.horizontal(|ui| {
                    if ui.button("Clean up this piece").clicked() {
                        command = Some(clean(vec![panel.id.clone()]));
                    }
                    if ui.button("All pieces").clicked() {
                        command = Some(clean(all.clone()));
                    }
                });
                ui.small("Sewn boundaries and those with targets, allowances or notches keep their points.");
                ui.separator();
                if state.curve_for.as_deref() != Some(panel.id.as_str()) {
                    state.curve_for = Some(panel.id.clone());
                    // Like a fresh dialog: start from the smallest share that keeps the shape.
                    state.curve_percent = authoring::suggested_curve_ratio(
                        &self.project.template,
                        &panel.id,
                        CURVE_TOLERANCE_M,
                    )
                    .map_or(50., |r| (r * 100.).round());
                }
                ui.horizontal(|ui| {
                    ui.label("Curve points");
                    ui.add(
                        egui::Slider::new(&mut state.curve_percent, 0.0..=100.0)
                            .suffix(" %")
                            .fixed_decimals(0),
                    )
                    .on_hover_text(
                        "Share of the curve points kept; the start value keeps this piece within 0.5 mm",
                    );
                });
                let optimize = |panels: Vec<String>| Command::OptimizeCurvePoints {
                    panels,
                    ratio: state.curve_percent / 100.,
                };
                ui.horizontal(|ui| {
                    if ui.button("Optimize this piece").clicked() {
                        command = Some(optimize(vec![panel.id.clone()]));
                    }
                    if ui.button("All pieces").clicked() {
                        command = Some(optimize(all.clone()));
                    }
                });
            });
        if let Some(command) = command {
            let cleaning = matches!(
                command,
                Command::CleanUpPoints { .. } | Command::OptimizeCurvePoints { .. }
            );
            let before = self.point_count();
            if self.edit(command) && cleaning {
                let removed = before.saturating_sub(self.point_count());
                self.status = format!(
                    "Removed {}",
                    super::garment_display::counted(removed, "point", "points")
                );
            }
        }
    }

    fn point_count(&self) -> usize {
        self.project
            .template
            .panels
            .iter()
            .filter_map(|p| p.polygon.as_ref())
            .map(|p| p.points.len() + p.lines.iter().map(|l| l.points.len()).sum::<usize>())
            .sum()
    }

    /// Offset copies of the selected open internal line, on either side.
    pub(super) fn internal_line_offsets(&mut self, ui: &mut egui::Ui, panel: &str, line: &str) {
        let state = &mut self.authoring.segments;
        let mut command = None;
        ui.horizontal(|ui| {
            ui.label("Copies");
            ui.add(egui::DragValue::new(&mut state.copies).range(1..=50));
            ui.label("every");
            ui.add(
                egui::DragValue::new(&mut state.copy_mm)
                    .range(1.0..=1000.0)
                    .fixed_decimals(1)
                    .suffix(" mm"),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.reverse, "Other side");
            ui.checkbox(&mut state.extend, "Extend to outline");
            if ui.button("Offset").clicked() {
                command = Some(Command::OffsetAsLines {
                    panel: panel.into(),
                    path: Some(line.into()),
                    count: state.copies,
                    distance_m: state.copy_mm / 1000.,
                    reverse: state.reverse,
                    extend: state.extend,
                });
            }
        });
        if let Some(command) = command {
            self.edit(command);
        }
    }
}
