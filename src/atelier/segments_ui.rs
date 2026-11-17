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
        }
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
                if state.pick.is_some() {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(0x15, 0x65, 0xC0), "Click a segment");
                        if ui.small_button("Cancel").clicked() {
                            state.pick = None;
                        }
                    });
                }
            });
        if let Some(pick) = &self.authoring.segments.pick {
            self.status = match pick.action {
                PickAction::MatchLength => "Click the segment to match lengths with · Esc cancels",
                PickAction::MatchUp => "Click a segment of the piece to bring here · Esc cancels",
            }
            .into();
        }
        if let Some(command) = command {
            self.edit(command);
        }
    }

    /// Finish a waiting segment action with the clicked contour segment. Returns true when
    /// the click was used.
    pub(super) fn finish_segment_pick(&mut self, panel: String, edge: String) -> bool {
        let Some(pick) = self.authoring.segments.pick.take() else {
            return false;
        };
        let state = &self.authoring.segments;
        let command = match pick.action {
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
            };
        }
        true
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
}
