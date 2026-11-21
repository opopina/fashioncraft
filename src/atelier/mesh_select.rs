//! Select mesh (3D Garment › Select mesh: brush, box, lasso): pick points of the draped cloth
//! in the 3D view, then freeze them where they are (they are pinned in place), release them,
//! or select every frozen point. Shift adds to the selection, Cmd/Ctrl takes away; the brush
//! paints while dragged. Only cloth that can be seen is taken. The selection lasts until the
//! garment is rebuilt.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectShape {
    Brush,
    Box,
    Lasso,
}

pub(super) struct MeshSelect {
    /// The tool in use; `None` when off.
    pub shape: Option<SelectShape>,
    pub vertices: BTreeSet<usize>,
    /// The garment build the selection belongs to.
    generation: u64,
    /// Brush radius, screen points.
    pub brush_px: f32,
    drag: Option<(egui::Pos2, egui::Pos2)>,
    path: Vec<egui::Pos2>,
}

impl Default for MeshSelect {
    fn default() -> Self {
        Self {
            shape: None,
            vertices: BTreeSet::new(),
            generation: 0,
            brush_px: 24.,
            drag: None,
            path: vec![],
        }
    }
}

/// Brush radii, screen points.
const BRUSH_PX: std::ops::RangeInclusive<f32> = 4.0..=200.0;
/// Least distance between two recorded lasso samples, in points.
const LASSO_STEP_PX: f32 = 2.;
const SELECTED: Color32 = Color32::from_rgb(64, 210, 255);
/// Most selected points drawn.
const MAX_DRAWN: usize = 20_000;

/// How a stroke changes the selection.
#[derive(Clone, Copy)]
enum Combine {
    Replace,
    Add,
    Remove,
}

impl Atelier {
    pub(super) fn start_mesh_select(&mut self, shape: SelectShape) {
        let tool = &mut self.authoring.mesh_select;
        tool.shape = Some(shape);
        tool.drag = None;
        tool.path.clear();
        self.status = i18n::tr(
            self.prefs.language,
            "Select cloth in the 3D view · Shift adds, Cmd/Ctrl takes away · Esc ends",
        )
        .into();
    }

    /// The selection, emptied when the garment was rebuilt since it was made.
    fn current_selection(&mut self) -> Vec<usize> {
        let tool = &mut self.authoring.mesh_select;
        if tool.generation != self.generation {
            tool.vertices.clear();
            tool.generation = self.generation;
        }
        let count = self.scene.state.x.len();
        tool.vertices.retain(|v| *v < count);
        tool.vertices.iter().copied().collect()
    }

    fn combine(&mut self, picked: Vec<usize>, how: Combine) {
        self.current_selection();
        let tool = &mut self.authoring.mesh_select;
        match how {
            Combine::Replace => tool.vertices = picked.into_iter().collect(),
            Combine::Add => tool.vertices.extend(picked),
            Combine::Remove => {
                for v in picked {
                    tool.vertices.remove(&v);
                }
            }
        }
        tool.generation = self.generation;
    }

    /// Clicks and drags in the 3D view while a select-mesh tool is on.
    pub(super) fn mesh_select_interaction(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        let Some(shape) = self.authoring.mesh_select.shape else {
            return false;
        };
        if response.ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.authoring.mesh_select.shape = None;
            return true;
        }
        let (shift, command) = response
            .ctx
            .input(|i| (i.modifiers.shift, i.modifiers.command));
        let how = if command {
            Combine::Remove
        } else if shift {
            Combine::Add
        } else {
            Combine::Replace
        };
        match shape {
            SelectShape::Brush => {
                let (smaller, larger) = response.ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::OpenBracket),
                        i.key_pressed(egui::Key::CloseBracket),
                    )
                });
                let tool = &mut self.authoring.mesh_select;
                if smaller {
                    tool.brush_px =
                        (tool.brush_px / 1.25).clamp(*BRUSH_PX.start(), *BRUSH_PX.end());
                }
                if larger {
                    tool.brush_px =
                        (tool.brush_px * 1.25).clamp(*BRUSH_PX.start(), *BRUSH_PX.end());
                }
                // A new plain stroke starts a new selection; the brush then adds as it goes.
                if response.drag_started_by(egui::PointerButton::Primary)
                    && matches!(how, Combine::Replace)
                {
                    self.combine(vec![], Combine::Replace);
                }
                let painting = response.dragged_by(egui::PointerButton::Primary)
                    || response.clicked_by(egui::PointerButton::Primary);
                if painting && let Some(pos) = response.interact_pointer_pos() {
                    let radius = self.authoring.mesh_select.brush_px;
                    let panels = self.pinnable_panels();
                    let picked: Vec<usize> = self
                        .visible_vertices(rect, &panels, |_, at| at.distance(pos) <= radius)
                        .into_iter()
                        .map(|(v, _)| v)
                        .collect();
                    let how = match how {
                        Combine::Remove => Combine::Remove,
                        _ => Combine::Add,
                    };
                    self.combine(picked, how);
                }
            }
            SelectShape::Box => {
                let tool = &mut self.authoring.mesh_select;
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    tool.drag = Some((pos, pos));
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && let (Some((_, end)), Some(pos)) =
                        (tool.drag.as_mut(), response.interact_pointer_pos())
                {
                    *end = pos;
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && let Some((start, end)) = self.authoring.mesh_select.drag.take()
                {
                    let region =
                        super::selection::Region::Box(egui::Rect::from_two_pos(start, end));
                    let picked = self.visible_vertices_in(rect, &region);
                    self.combine(picked, how);
                }
            }
            SelectShape::Lasso => {
                let tool = &mut self.authoring.mesh_select;
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    tool.path = vec![pos];
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(pos) = response.interact_pointer_pos()
                    && tool
                        .path
                        .last()
                        .is_some_and(|last| last.distance(pos) >= LASSO_STEP_PX)
                    && tool.path.len() < super::selection::MAX_LASSO_RAW_POINTS
                {
                    tool.path.push(pos);
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    let path = std::mem::take(&mut self.authoring.mesh_select.path);
                    if path.len() >= 3 {
                        match super::pin_tool::lasso_region(&path) {
                            Ok(region) => {
                                let picked = self.visible_vertices_in(rect, &region);
                                self.combine(picked, how);
                            }
                            Err(e) => self.status = e,
                        }
                    }
                }
            }
        }
        if response.clicked_by(egui::PointerButton::Primary)
            && shape != SelectShape::Brush
            && matches!(how, Combine::Replace)
        {
            // A plain click outside a drag clears the selection.
            self.combine(vec![], Combine::Replace);
        }
        true
    }

    /// The selected points, the brush and the box or lasso being drawn.
    pub(super) fn paint_mesh_select(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if self.authoring.mesh_select.shape.is_none()
            && self.authoring.mesh_select.vertices.is_empty()
        {
            return;
        }
        let selected = self.current_selection();
        let painter = ui.painter().with_clip_rect(rect);
        let matrix = self.camera.matrix(rect.width() / rect.height());
        for v in selected.iter().take(MAX_DRAWN) {
            if let Some(at) = super::gizmo::project_matrix(matrix, rect, self.scene.state.x[*v]) {
                painter.circle_filled(at, 2., SELECTED);
            }
        }
        let tool = &self.authoring.mesh_select;
        let stroke = Stroke::new(1.5, SELECTED);
        match tool.shape {
            Some(SelectShape::Brush) => {
                if let Some(pos) = ui.ctx().pointer_hover_pos()
                    && rect.contains(pos)
                {
                    painter.circle_stroke(pos, tool.brush_px, stroke);
                }
            }
            Some(SelectShape::Box) => {
                if let Some((a, b)) = tool.drag {
                    let r = egui::Rect::from_two_pos(a, b);
                    painter.rect_filled(r, 0., SELECTED.gamma_multiply(0.1));
                    painter.rect_stroke(r, 0., stroke, egui::StrokeKind::Inside);
                }
            }
            Some(SelectShape::Lasso) if tool.path.len() >= 2 => {
                painter.add(egui::Shape::line(tool.path.clone(), stroke));
            }
            _ => {}
        }
    }

    /// The window of the select-mesh tool: how much is selected and what to do with it.
    pub(super) fn mesh_select_window(&mut self, ctx: &egui::Context) {
        if self.authoring.mesh_select.shape.is_none() {
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        let selected = self.current_selection();
        let mut open = true;
        let mut action: Option<&str> = None;
        egui::Window::new(t("Select mesh"))
            .open(&mut open)
            .default_width(260.)
            .show(ctx, |ui| {
                let tool = &mut self.authoring.mesh_select;
                ui.horizontal(|ui| {
                    for (shape, label) in [
                        (SelectShape::Brush, "Brush"),
                        (SelectShape::Box, "Box"),
                        (SelectShape::Lasso, "Lasso"),
                    ] {
                        ui.selectable_value(&mut tool.shape, Some(shape), t(label));
                    }
                });
                if tool.shape == Some(SelectShape::Brush) {
                    ui.horizontal(|ui| {
                        ui.label(t("Brush size"));
                        super::workspace::slim(
                            ui,
                            egui::Slider::new(&mut tool.brush_px, BRUSH_PX).suffix(" pt"),
                        )
                        .on_hover_text(t("[ and ] make it smaller and larger"));
                    });
                }
                ui.label(format!("{} {}", selected.len(), t("points selected")));
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !selected.is_empty(),
                            egui::Button::new(t("Freeze in place")),
                        )
                        .on_hover_text(t(
                            "Hold the selected points where they are: they are pinned",
                        ))
                        .clicked()
                    {
                        action = Some("freeze");
                    }
                    if ui
                        .add_enabled(!selected.is_empty(), egui::Button::new(t("Release")))
                        .on_hover_text(t("Let the selected points move again"))
                        .clicked()
                    {
                        action = Some("release");
                    }
                    if ui.button(t("Select frozen")).clicked() {
                        action = Some("frozen");
                    }
                    if ui
                        .add_enabled(
                            !selected.is_empty(),
                            egui::Button::new(t("Clear selection")),
                        )
                        .clicked()
                    {
                        action = Some("clear");
                    }
                });
            });
        match action {
            Some("freeze") => self.apply_pins(&selected, true),
            Some("release") => self.apply_pins(&selected, false),
            Some("frozen") => {
                let holds = self.scene.garment.authored_holds.clone();
                let frozen: Vec<usize> = (0..holds.len()).filter(|v| holds[*v]).collect();
                self.combine(frozen, Combine::Replace);
            }
            Some("clear") => self.combine(vec![], Combine::Replace),
            _ => {}
        }
        if !open {
            self.authoring.mesh_select.shape = None;
        }
    }
}
