//! Rulers and guidelines for the 2D board. Rulers run along the top and left edges and read from
//! a movable origin; dragging out of a ruler creates a guideline (Ctrl: the other axis). Guides
//! are dragged to move them, dropped back on a ruler to delete them, clicked to select them, and
//! can be locked, offset into evenly spaced copies or deleted. Pattern points snap to guides and
//! a guide snaps to pattern points. Stored with the project; nothing here touches the garment.
use super::*;
use glam::DVec2;
use marvelous_cloth::fitting::{GuideAxis, GuideLine};
use serde::{Deserialize, Serialize};

/// Guide colour on the white board.
pub(super) const GUIDE_COLOR: Color32 = Color32::from_rgb(0x00, 0x97, 0xA7);
/// Ruler thickness, in points.
const RULER_PX: f32 = 18.;
/// How close the pointer must be to a guide to grab or select it, in points.
const GUIDE_PICK_PX: f32 = 5.;

/// Unit the rulers and guide readouts use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RulerUnit {
    #[default]
    Millimetre,
    Centimetre,
    Inch,
}

impl RulerUnit {
    pub(super) const ALL: [Self; 3] = [Self::Millimetre, Self::Centimetre, Self::Inch];
    /// Units per metre.
    pub(super) fn per_metre(self) -> f64 {
        match self {
            Self::Millimetre => 1000.,
            Self::Centimetre => 100.,
            Self::Inch => 1000. / 25.4,
        }
    }
    pub(super) fn symbol(self) -> &'static str {
        match self {
            Self::Millimetre => "mm",
            Self::Centimetre => "cm",
            Self::Inch => "in",
        }
    }
    /// A board distance in metres, written in this unit.
    pub(super) fn format(self, metres: f64) -> String {
        let value = metres * self.per_metre();
        match self {
            Self::Millimetre => format!("{value:.1} mm"),
            Self::Centimetre => format!("{value:.2} cm"),
            Self::Inch => format!("{value:.3} in"),
        }
    }
}

/// Ruler tick spacing in ruler units: the major step is the smallest 1, 2 or 5 × 10ⁿ that leaves
/// at least `min_major_px` between labels; minor ticks subdivide it while they stay readable.
pub(super) fn ruler_steps(pixels_per_unit: f64, min_major_px: f64) -> (f64, f64) {
    if !pixels_per_unit.is_finite() || pixels_per_unit <= 0. {
        return (1., 1.);
    }
    let mut decade = 10f64.powf((min_major_px / pixels_per_unit).log10().floor() - 1.);
    let major = loop {
        if let Some(step) = [1., 2., 5.]
            .iter()
            .map(|m| m * decade)
            .find(|step| step * pixels_per_unit >= min_major_px)
        {
            break step;
        }
        decade *= 10.;
    };
    let minor = [10., 5., 2.]
        .iter()
        .map(|parts| major / parts)
        .find(|step| step * pixels_per_unit >= 6.)
        .unwrap_or(major);
    (major, minor)
}

/// Guide positions `count` steps of `distance_m` away from `base` (and on the other side too
/// when `both_sides`). Copies are unlocked.
pub(super) fn offset_guides(
    base: GuideLine,
    count: u32,
    distance_m: f64,
    both_sides: bool,
) -> Vec<GuideLine> {
    let mut lines = Vec::new();
    for k in 1..=count {
        for sign in if both_sides {
            [1., -1.].as_slice()
        } else {
            [1.].as_slice()
        } {
            lines.push(GuideLine {
                axis: base.axis,
                position_m: base.position_m + sign * distance_m * f64::from(k),
                locked: false,
            });
        }
    }
    lines
}

/// The board coordinate a guide under the pointer would take: the pointer's own, or that of the
/// nearest pattern point within `reach` (compared in screen points).
pub(super) fn snapped_guide_position(
    axis: GuideAxis,
    pointer: DVec2,
    points: &[DVec2],
    distance_px: impl Fn(DVec2) -> f32,
    reach: f32,
) -> f64 {
    let along = |p: DVec2| match axis {
        GuideAxis::Horizontal => DVec2::new(pointer.x, p.y),
        GuideAxis::Vertical => DVec2::new(p.x, pointer.y),
    };
    let coordinate = |p: DVec2| match axis {
        GuideAxis::Horizontal => p.y,
        GuideAxis::Vertical => p.x,
    };
    points
        .iter()
        .map(|p| (distance_px(along(*p)), coordinate(*p)))
        .filter(|(d, _)| *d <= reach)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(coordinate(pointer), |(_, c)| c)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum GuideDrag {
    /// A new guide being pulled out of a ruler.
    New { axis: GuideAxis, at: f64 },
    /// An existing guide being moved.
    Move { index: usize, at: f64 },
    /// The ruler origin being moved.
    Origin { at: DVec2 },
}

/// Transient guide state on the board: the drag in progress, the selected guide and the offset
/// form.
pub(super) struct GuidesUi {
    drag: Option<GuideDrag>,
    pub(super) selected: Option<usize>,
    offset_count: u32,
    offset_mm: f64,
    offset_both: bool,
}

impl Default for GuidesUi {
    fn default() -> Self {
        Self {
            drag: None,
            selected: None,
            offset_count: 1,
            offset_mm: 10.,
            offset_both: false,
        }
    }
}

struct RulerRects {
    top: egui::Rect,
    left: egui::Rect,
    corner: egui::Rect,
}

fn ruler_rects(rect: egui::Rect) -> RulerRects {
    RulerRects {
        corner: egui::Rect::from_min_size(rect.min, egui::Vec2::splat(RULER_PX)),
        top: egui::Rect::from_min_max(
            rect.min + egui::vec2(RULER_PX, 0.),
            egui::pos2(rect.max.x, rect.min.y + RULER_PX),
        ),
        left: egui::Rect::from_min_max(
            rect.min + egui::vec2(0., RULER_PX),
            egui::pos2(rect.min.x + RULER_PX, rect.max.y),
        ),
    }
}

impl Atelier {
    /// Rulers are drawn and used only while the board is not rotated: they measure along the
    /// screen axes, which match the board axes only then.
    fn rulers_active(&self) -> bool {
        self.prefs.pattern_display.rulers && self.authoring.board.view_rotation.abs() < 1e-9
    }

    fn guide_line_screen(&self, rect: egui::Rect, axis: GuideAxis, at: f64) -> [egui::Pos2; 2] {
        let board = &self.authoring.board;
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ]
        .map(|c| board.world(rect, c));
        let (lo, hi) = corners.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), c| (lo.min(*c), hi.max(*c)),
        );
        let (a, b) = match axis {
            GuideAxis::Horizontal => (DVec2::new(lo.x, at), DVec2::new(hi.x, at)),
            GuideAxis::Vertical => (DVec2::new(at, lo.y), DVec2::new(at, hi.y)),
        };
        [board.screen(rect, a), board.screen(rect, b)]
    }

    /// Index of the guide within reach of `pos`, nearest first.
    fn guide_at(&self, rect: egui::Rect, pos: egui::Pos2) -> Option<usize> {
        if !self.prefs.pattern_display.guides {
            return None;
        }
        self.project
            .board_guides
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let [a, b] = self.guide_line_screen(rect, line.axis, line.position_m);
                (i, super::authoring_ui::segment_distance(pos, a, b))
            })
            .filter(|(_, d)| *d <= GUIDE_PICK_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn guide_position(&self, rect: egui::Rect, axis: GuideAxis, pos: egui::Pos2) -> f64 {
        let board = &self.authoring.board;
        let world = board.world(rect, pos);
        let points = self.snap_geometry_for_guides();
        snapped_guide_position(
            axis,
            world,
            &points,
            |p| board.screen(rect, p).distance(pos),
            super::board::SNAP_POINT_PX,
        )
    }

    /// Ruler and guide input. Returns true when it handled the pointer this frame, so the
    /// board's own tools must not see it.
    pub(super) fn guide_interaction(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        response: &egui::Response,
    ) -> bool {
        let rulers = self.rulers_active().then(|| ruler_rects(rect));
        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        // A drag in progress owns the pointer until release.
        if let Some(drag) = self.authoring.board.guides_ui.drag {
            if let Some(pos) = pointer {
                let next = match drag {
                    GuideDrag::New { axis, .. } => GuideDrag::New {
                        axis,
                        at: self.guide_position(rect, axis, pos),
                    },
                    GuideDrag::Move { index, .. } => {
                        let axis = self.project.board_guides.lines[index].axis;
                        GuideDrag::Move {
                            index,
                            at: self.guide_position(rect, axis, pos),
                        }
                    }
                    GuideDrag::Origin { .. } => GuideDrag::Origin {
                        at: self.authoring.board.world(rect, pos),
                    },
                };
                self.authoring.board.guides_ui.drag = Some(next);
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                // Released on the ruler strip the guide came from, or past it: delete.
                let axis = match drag {
                    GuideDrag::New { axis, .. } => Some(axis),
                    GuideDrag::Move { index, .. } => {
                        Some(self.project.board_guides.lines[index].axis)
                    }
                    GuideDrag::Origin { .. } => None,
                };
                let on_ruler = pointer
                    .zip(axis)
                    .is_some_and(|(p, axis)| dropped_on_ruler(rect, axis, p));
                self.finish_guide_drag(on_ruler);
            }
            return true;
        }
        let press = ui.input(|i| i.pointer.press_origin());
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(start) = press
        {
            let other_axis = ui.input(|i| i.modifiers.command);
            if let Some(r) = &rulers {
                let axis = if r.top.contains(start) {
                    Some(GuideAxis::Horizontal)
                } else if r.left.contains(start) {
                    Some(GuideAxis::Vertical)
                } else {
                    None
                };
                if r.corner.contains(start) {
                    self.authoring.board.guides_ui.drag = Some(GuideDrag::Origin {
                        at: self.authoring.board.world(rect, start),
                    });
                    return true;
                }
                if let Some(axis) = axis {
                    let axis = match (axis, other_axis) {
                        (GuideAxis::Horizontal, true) => GuideAxis::Vertical,
                        (GuideAxis::Vertical, true) => GuideAxis::Horizontal,
                        (axis, false) => axis,
                    };
                    self.prefs.pattern_display.guides = true;
                    self.authoring.board.guides_ui.drag = Some(GuideDrag::New {
                        axis,
                        at: self.guide_position(rect, axis, start),
                    });
                    return true;
                }
            }
            // Grab a guide in empty space or over a piece body; points and edges win.
            if let Some(index) = self.guide_at(rect, start)
                && !self.project.board_guides.lines[index].locked
                && self
                    .board_hit(rect, start)
                    .is_none_or(|hit| matches!(hit.part, super::board::Part::Body))
            {
                // Selection waits for the drop: showing the guide bar now would shift the board
                // under the pointer mid-drag.
                self.authoring.board.guides_ui.drag = Some(GuideDrag::Move {
                    index,
                    at: self.project.board_guides.lines[index].position_m,
                });
                return true;
            }
        }
        if let (Some(r), Some(pos)) = (&rulers, response.hover_pos())
            && (r.top.contains(pos) || r.left.contains(pos) || r.corner.contains(pos))
        {
            if response.double_clicked() && r.corner.contains(pos) {
                self.project.board_guides.origin_m = [0., 0.];
                self.document.dirty = true;
                self.status = "Ruler origin reset".into();
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            // Clicks on the rulers never reach the board tools.
            return response.clicked() || response.double_clicked();
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            match self.guide_at(rect, pos) {
                Some(index) if self.board_hit(rect, pos).is_none() => {
                    self.authoring.board.guides_ui.selected = Some(index);
                    return true;
                }
                _ => self.authoring.board.guides_ui.selected = None,
            }
        }
        if let Some(pos) = response.hover_pos()
            && let Some(index) = self.guide_at(rect, pos)
            && self.board_hit(rect, pos).is_none()
        {
            ui.ctx()
                .set_cursor_icon(match self.project.board_guides.lines[index].axis {
                    GuideAxis::Horizontal => egui::CursorIcon::ResizeVertical,
                    GuideAxis::Vertical => egui::CursorIcon::ResizeHorizontal,
                });
        }
        if let Some(index) = self.authoring.board.guides_ui.selected
            && response.hovered()
            && ui.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
            })
        {
            self.delete_guide(index);
            return true;
        }
        false
    }

    fn finish_guide_drag(&mut self, dropped_on_ruler: bool) {
        let Some(drag) = self.authoring.board.guides_ui.drag.take() else {
            return;
        };
        let unit = self.prefs.pattern_display.ruler_unit;
        let origin = self.project.board_guides.origin_m;
        match drag {
            GuideDrag::New { axis, at } => {
                if dropped_on_ruler {
                    return;
                }
                if self.project.board_guides.lines.len()
                    >= marvelous_cloth::fitting::BoardGuides::MAX_LINES
                {
                    self.status = "Guide limit reached; delete some guides first".into();
                    return;
                }
                self.project.board_guides.lines.push(GuideLine {
                    axis,
                    position_m: at,
                    locked: false,
                });
                self.authoring.board.guides_ui.selected =
                    Some(self.project.board_guides.lines.len() - 1);
                self.status = format!(
                    "Guide at {}",
                    unit.format(at - origin[guide_axis_index(axis)])
                );
            }
            GuideDrag::Move { index, at } => {
                if dropped_on_ruler {
                    self.delete_guide(index);
                    return;
                }
                self.authoring.board.guides_ui.selected = Some(index);
                let line = &mut self.project.board_guides.lines[index];
                line.position_m = at;
                let axis = line.axis;
                self.status = format!(
                    "Guide moved to {}",
                    unit.format(at - origin[guide_axis_index(axis)])
                );
            }
            GuideDrag::Origin { at } => {
                self.project.board_guides.origin_m = [at.x, at.y];
                self.status = "Ruler origin moved · double-click the corner to reset it".into();
            }
        }
        self.document.dirty = true;
    }

    fn delete_guide(&mut self, index: usize) {
        if index < self.project.board_guides.lines.len() {
            self.project.board_guides.lines.remove(index);
            self.authoring.board.guides_ui.selected = None;
            self.document.dirty = true;
            self.status = "Guide deleted".into();
        }
    }

    /// Board points a guide snaps to: every displayed contour point.
    fn snap_geometry_for_guides(&self) -> Vec<DVec2> {
        self.snap_geometry_for(None).points
    }

    /// Draw the guides, the guide being dragged and, when shown, the rulers on top.
    pub(super) fn paint_guides_and_rulers(&self, painter: &egui::Painter, rect: egui::Rect) {
        let display = self.prefs.pattern_display;
        let ui_state = &self.authoring.board.guides_ui;
        let unit = display.ruler_unit;
        let origin = self.project.board_guides.origin_m;
        if display.guides {
            for (i, line) in self.project.board_guides.lines.iter().enumerate() {
                let at = match ui_state.drag {
                    Some(GuideDrag::Move { index, at }) if index == i => at,
                    _ => line.position_m,
                };
                let [a, b] = self.guide_line_screen(rect, line.axis, at);
                let width = if ui_state.selected == Some(i) { 2. } else { 1. };
                let stroke = Stroke::new(width, GUIDE_COLOR);
                if line.locked {
                    painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 6., 4.));
                } else {
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        if let Some(GuideDrag::New { axis, at }) = ui_state.drag {
            let [a, b] = self.guide_line_screen(rect, axis, at);
            painter.line_segment([a, b], Stroke::new(1.5, GUIDE_COLOR));
        }
        // Readout beside the guide being dragged or selected.
        let readout = match ui_state.drag {
            Some(GuideDrag::New { axis, at }) => Some((axis, at)),
            Some(GuideDrag::Move { index, at }) => {
                Some((self.project.board_guides.lines[index].axis, at))
            }
            _ => ui_state
                .selected
                .and_then(|i| self.project.board_guides.lines.get(i))
                .map(|l| (l.axis, l.position_m)),
        };
        if let Some((axis, at)) = readout {
            let [a, _] = self.guide_line_screen(rect, axis, at);
            let anchor = match axis {
                GuideAxis::Horizontal => egui::pos2(rect.min.x + RULER_PX + 6., a.y - 3.),
                GuideAxis::Vertical => egui::pos2(a.x + 4., rect.min.y + RULER_PX + 4.),
            };
            painter.text(
                anchor,
                match axis {
                    GuideAxis::Horizontal => egui::Align2::LEFT_BOTTOM,
                    GuideAxis::Vertical => egui::Align2::LEFT_TOP,
                },
                unit.format(at - origin[guide_axis_index(axis)]),
                egui::FontId::proportional(11.),
                GUIDE_COLOR,
            );
        }
        if !display.rulers {
            return;
        }
        if !self.rulers_active() {
            painter.text(
                rect.right_top() + egui::vec2(-8., 8.),
                egui::Align2::RIGHT_TOP,
                "Rulers hidden while the board is rotated",
                egui::FontId::proportional(11.),
                Color32::from_gray(0x77),
            );
            return;
        }
        let board = &self.authoring.board;
        let r = ruler_rects(rect);
        let fill = Color32::from_rgb(0xEE, 0xEF, 0xF2);
        let ink = Color32::from_gray(0x55);
        for strip in [r.top, r.left, r.corner] {
            painter.rect_filled(strip, 0., fill);
        }
        painter.line_segment(
            [r.top.left_bottom(), r.top.right_bottom()],
            Stroke::new(1., Color32::from_gray(0xC8)),
        );
        painter.line_segment(
            [r.left.right_top(), r.left.right_bottom()],
            Stroke::new(1., Color32::from_gray(0xC8)),
        );
        let per_metre = unit.per_metre();
        let pixels_per_unit = f64::from(board.scale) / per_metre;
        let (major, minor) = ruler_steps(pixels_per_unit, 60.);
        let font = egui::FontId::proportional(9.);
        let lo = board.world(rect, r.top.left_top());
        let hi = board.world(rect, rect.max);
        for (axis_index, start, end) in [(0usize, lo.x, hi.x), (1usize, lo.y, hi.y)] {
            let from = ((start - origin[axis_index]) * per_metre / minor).floor() as i64;
            let to = ((end - origin[axis_index]) * per_metre / minor).ceil() as i64;
            if to - from > 4000 {
                continue;
            }
            for k in from..=to {
                let value = k as f64 * minor;
                let world = origin[axis_index] + value / per_metre;
                let is_major = ((value / major) - (value / major).round()).abs() < 1e-6;
                let length = if is_major {
                    RULER_PX * 0.6
                } else {
                    RULER_PX * 0.25
                };
                if axis_index == 0 {
                    let x = board.screen(rect, DVec2::new(world, lo.y)).x;
                    if x < r.top.min.x {
                        continue;
                    }
                    painter.line_segment(
                        [
                            egui::pos2(x, r.top.max.y - length),
                            egui::pos2(x, r.top.max.y),
                        ],
                        Stroke::new(1., ink),
                    );
                    if is_major {
                        painter.text(
                            egui::pos2(x + 2., r.top.min.y + 1.),
                            egui::Align2::LEFT_TOP,
                            ruler_label(value, major),
                            font.clone(),
                            ink,
                        );
                    }
                } else {
                    let y = board.screen(rect, DVec2::new(lo.x, world)).y;
                    if y < r.left.min.y {
                        continue;
                    }
                    painter.line_segment(
                        [
                            egui::pos2(r.left.max.x - length, y),
                            egui::pos2(r.left.max.x, y),
                        ],
                        Stroke::new(1., ink),
                    );
                    if is_major {
                        painter.text(
                            egui::pos2(r.left.min.x + 1., y + 2.),
                            egui::Align2::LEFT_TOP,
                            ruler_label(value, major),
                            font.clone(),
                            ink,
                        );
                    }
                }
            }
        }
        painter.text(
            r.corner.center(),
            egui::Align2::CENTER_CENTER,
            unit.symbol(),
            font,
            ink,
        );
        // The origin the rulers read from.
        let o = board.screen(rect, DVec2::new(origin[0], origin[1]));
        if rect.contains(o) {
            let stroke = Stroke::new(1., GUIDE_COLOR);
            painter.line_segment([o - egui::vec2(6., 0.), o + egui::vec2(6., 0.)], stroke);
            painter.line_segment([o - egui::vec2(0., 6.), o + egui::vec2(0., 6.)], stroke);
        }
        if let Some(GuideDrag::Origin { at }) = ui_state.drag {
            // The crosshair the origin will move to.
            let p = board.screen(rect, at);
            let stroke = Stroke::new(1., GUIDE_COLOR.gamma_multiply(0.7));
            painter.line_segment(
                [egui::pos2(rect.min.x, p.y), egui::pos2(rect.max.x, p.y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(p.x, rect.min.y), egui::pos2(p.x, rect.max.y)],
                stroke,
            );
        }
    }

    /// "Rulers & guides" in the board's display menu.
    pub(super) fn guide_display_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("RULERS & GUIDES");
        let display = &mut self.prefs.pattern_display;
        ui.horizontal(|ui| {
            ui.checkbox(&mut display.rulers, "Rulers");
            for unit in RulerUnit::ALL {
                ui.selectable_value(&mut display.ruler_unit, unit, unit.symbol());
            }
        });
        ui.checkbox(&mut display.guides, "Guides")
            .on_hover_text("Drag out of a ruler to add one (Ctrl: the other axis); drop it back on a ruler to delete it");
        let lines = &mut self.project.board_guides.lines;
        let any = !lines.is_empty();
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui.add_enabled(any, egui::Button::new("Lock all")).clicked() {
                lines.iter_mut().for_each(|l| l.locked = true);
                changed = true;
            }
            if ui
                .add_enabled(any, egui::Button::new("Unlock all"))
                .clicked()
            {
                lines.iter_mut().for_each(|l| l.locked = false);
                changed = true;
            }
            if ui
                .add_enabled(any, egui::Button::new("Delete all"))
                .clicked()
            {
                lines.clear();
                self.authoring.board.guides_ui.selected = None;
                changed = true;
            }
        });
        if changed {
            self.document.dirty = true;
        }
    }

    /// The selected guide's actions, shown above the board while a guide is selected.
    pub(super) fn guide_toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.authoring.board.guides_ui.selected else {
            return;
        };
        let Some(line) = self.project.board_guides.lines.get(index).copied() else {
            self.authoring.board.guides_ui.selected = None;
            return;
        };
        let unit = self.prefs.pattern_display.ruler_unit;
        let origin = self.project.board_guides.origin_m[guide_axis_index(line.axis)];
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} guide · {}",
                    match line.axis {
                        GuideAxis::Horizontal => "Horizontal",
                        GuideAxis::Vertical => "Vertical",
                    },
                    unit.format(line.position_m - origin)
                ))
                .color(GUIDE_COLOR),
            );
            if ui
                .button(if line.locked { "Unlock" } else { "Lock" })
                .clicked()
            {
                self.project.board_guides.lines[index].locked = !line.locked;
                self.document.dirty = true;
            }
            ui.separator();
            let state = &mut self.authoring.board.guides_ui;
            ui.label("Offset");
            ui.add(
                egui::DragValue::new(&mut state.offset_count)
                    .range(1..=50)
                    .suffix(" ×"),
            );
            ui.add(
                egui::DragValue::new(&mut state.offset_mm)
                    .range(0.1..=5000.0)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .suffix(" mm"),
            );
            ui.checkbox(&mut state.offset_both, "Both sides");
            if ui
                .button("Create")
                .on_hover_text("Add evenly spaced guides from this one")
                .clicked()
            {
                let copies = offset_guides(
                    line,
                    state.offset_count,
                    state.offset_mm / 1000.,
                    state.offset_both,
                );
                let room = marvelous_cloth::fitting::BoardGuides::MAX_LINES
                    .saturating_sub(self.project.board_guides.lines.len());
                let added = copies.len().min(room);
                self.project
                    .board_guides
                    .lines
                    .extend(copies.into_iter().take(room));
                self.document.dirty = true;
                self.status = format!(
                    "{} added",
                    super::garment_display::counted(added, "guide", "guides")
                );
            }
            ui.separator();
            if ui.button("Delete").clicked() {
                self.delete_guide(index);
            }
        });
    }
}

/// Whether a guide released at `pointer` goes back into its ruler: a horizontal guide at or
/// above the bottom of the top ruler strip, a vertical one at or left of the left strip.
pub(super) fn dropped_on_ruler(rect: egui::Rect, axis: GuideAxis, pointer: egui::Pos2) -> bool {
    match axis {
        GuideAxis::Horizontal => pointer.y <= rect.min.y + RULER_PX,
        GuideAxis::Vertical => pointer.x <= rect.min.x + RULER_PX,
    }
}

fn guide_axis_index(axis: GuideAxis) -> usize {
    match axis {
        GuideAxis::Horizontal => 1,
        GuideAxis::Vertical => 0,
    }
}

fn ruler_label(value: f64, major: f64) -> String {
    if major >= 1. {
        format!("{value:.0}")
    } else if major >= 0.1 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_steps_keep_labels_apart_at_any_zoom() {
        // 300 px/m in millimetres: 0.3 px per mm, so a 60 px gap needs 200 mm.
        assert_eq!(ruler_steps(0.3, 60.), (200., 20.));
        // 3000 px/m: 3 px per mm, 20 mm majors and 2 mm minors.
        assert_eq!(ruler_steps(3.0, 60.), (20., 2.));
        // Centimetres at 300 px/m: 3 px per cm.
        assert_eq!(ruler_steps(3.0, 60.).0, 20.);
        let (major, minor) = ruler_steps(0.012, 60.);
        assert!(major * 0.012 >= 60. && minor <= major);
        assert_eq!(ruler_steps(f64::NAN, 60.), (1., 1.));
    }

    #[test]
    fn dropping_on_or_past_the_ruler_deletes() {
        let rect = egui::Rect::from_min_size(egui::pos2(100., 200.), egui::vec2(600., 400.));
        assert!(dropped_on_ruler(
            rect,
            GuideAxis::Horizontal,
            egui::pos2(300., 210.)
        ));
        assert!(
            dropped_on_ruler(rect, GuideAxis::Horizontal, egui::pos2(300., 150.)),
            "past it"
        );
        assert!(!dropped_on_ruler(
            rect,
            GuideAxis::Horizontal,
            egui::pos2(300., 230.)
        ));
        assert!(dropped_on_ruler(
            rect,
            GuideAxis::Vertical,
            egui::pos2(90., 400.)
        ));
        assert!(!dropped_on_ruler(
            rect,
            GuideAxis::Vertical,
            egui::pos2(150., 210.)
        ));
    }

    #[test]
    fn units_convert_and_format() {
        assert_eq!(RulerUnit::Millimetre.format(0.1234), "123.4 mm");
        assert_eq!(RulerUnit::Centimetre.format(0.1234), "12.34 cm");
        assert_eq!(RulerUnit::Inch.format(0.0254), "1.000 in");
    }

    #[test]
    fn offsets_space_copies_evenly_on_one_or_both_sides() {
        let base = GuideLine {
            axis: GuideAxis::Vertical,
            position_m: 0.1,
            locked: true,
        };
        let one_side = offset_guides(base, 3, 0.05, false);
        let positions: Vec<f64> = one_side.iter().map(|l| l.position_m).collect();
        assert_eq!(positions.len(), 3);
        for (p, expected) in positions.iter().zip([0.15, 0.2, 0.25]) {
            assert!((p - expected).abs() < 1e-12);
        }
        assert!(
            one_side
                .iter()
                .all(|l| !l.locked && l.axis == GuideAxis::Vertical)
        );
        let both = offset_guides(base, 2, 0.05, true);
        assert_eq!(both.len(), 4);
        assert!(both.iter().any(|l| (l.position_m - 0.0).abs() < 1e-12));
    }

    #[test]
    fn guides_snap_to_nearby_points_only() {
        let points = [DVec2::new(0.3, 0.2), DVec2::new(0.5, 0.21)];
        // Screen distance = 1000 px per metre here.
        let px = |p: DVec2| (p.distance(DVec2::new(0.4, 0.205)) * 1000.) as f32;
        let snapped = snapped_guide_position(
            GuideAxis::Horizontal,
            DVec2::new(0.4, 0.205),
            &points,
            px,
            10.,
        );
        assert!((snapped - 0.2).abs() < 1e-12 || (snapped - 0.21).abs() < 1e-12);
        let free = snapped_guide_position(
            GuideAxis::Horizontal,
            DVec2::new(0.4, 0.5),
            &points,
            |p: DVec2| (p.distance(DVec2::new(0.4, 0.5)) * 1000.) as f32,
            10.,
        );
        assert_eq!(free, 0.5);
    }
}
