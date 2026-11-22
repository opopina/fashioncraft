//! Pins on the 3D cloth: click the cloth to pin a point where it is, drag a box or draw a lasso
//! to pin every visible point inside it, click a pin to remove it (Cmd+drag removes the pins
//! inside the box or lasso).
//! Pins are authored targets in material coordinates (`pin_targets`), so each action is one
//! validated undo step; frozen pieces keep their own hold and are skipped.
use super::*;
use fashioncraft::{authoring, picking};
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, BTreeSet};

/// The Grab tool's pin mode and its box gesture.
#[derive(Default)]
pub(super) struct PinTool {
    pub enabled: bool,
    /// Tack mode (within the pin mode): two clicks hold two points of the cloth together.
    pub tack: bool,
    tack_first: Option<usize>,
    drag: Option<(egui::Pos2, egui::Pos2)>,
    /// Draw a lasso instead of dragging a box.
    pub lasso: bool,
    /// The lasso being drawn, in screen points.
    path: Vec<egui::Pos2>,
    /// A pinned area being dragged to a new place.
    moving: Option<PinMove>,
}

/// A pinned area being dragged: its points, where they were, and the plane they move in.
struct PinMove {
    vertices: Vec<usize>,
    start: Vec<DVec3>,
    anchor: DVec3,
    normal: DVec3,
    translation: DVec3,
}

/// Least distance between two recorded lasso samples, in points.
const LASSO_STEP_PX: f32 = 2.;

/// The region a drawn lasso encloses, its stroke simplified and closed.
pub(super) fn lasso_region(path: &[egui::Pos2]) -> Result<super::selection::Region, String> {
    Ok(super::selection::Region::Lasso(
        super::selection::simplify_lasso(path)?,
    ))
}

/// How close to a pin a click must land to remove it, in points.
const PIN_PICK_PX: f32 = 8.;
/// Most points one box may pin, like the authoring limit.
const MAX_BOX_PINS: usize = 65_536;

impl Atelier {
    /// Mode switch and pin count above the 3D view while the Grab tool is active.
    pub(super) fn pin_tool_bar(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let count = self.project.template.pin_targets.len();
        ui.horizontal_wrapped(|ui| {
            let tool = &mut self.authoring.pin_tool;
            if ui
                .selectable_label(!tool.enabled, i18n::tr(ui_lang, "Grab"))
                .on_hover_text(i18n::tr(ui_lang, "Drag the cloth while it simulates"))
                .clicked()
            {
                tool.enabled = false;
                tool.tack = false;
            }
            if ui
                .selectable_label(tool.enabled && !tool.tack, i18n::tr(ui_lang, "Pin"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Click the cloth to pin a point where it is · drag a box to pin an area · \
                 click a pin to remove it · Cmd+drag removes the pins in a box · \
                 drag a pin to move its pinned area",
                ))
                .clicked()
            {
                tool.enabled = true;
                tool.tack = false;
            }
            if ui
                .selectable_label(tool.enabled && tool.tack, i18n::tr(ui_lang, "Tack"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Click two points of the cloth to hold them together, like a basting stitch",
                ))
                .clicked()
            {
                tool.enabled = true;
                tool.tack = true;
                tool.tack_first = None;
            }
            if tool.enabled && !tool.tack {
                ui.separator();
                if ui
                    .selectable_label(!tool.lasso, i18n::tr(ui_lang, "Box"))
                    .on_hover_text(i18n::tr(ui_lang, "Drag a box to pin every visible point inside it"))
                    .clicked()
                {
                    tool.lasso = false;
                }
                if ui
                    .selectable_label(tool.lasso, i18n::tr(ui_lang, "Lasso"))
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Draw around the cloth to pin every visible point inside · Cmd+drag removes the pins inside",
                    ))
                    .clicked()
                {
                    tool.lasso = true;
                }
            }
            ui.separator();
            // Each count stays with its button when the capsule wraps.
            let mut delete_pins = false;
            let mut remove_tacks = false;
            let tacks = self.project.template.tacks.len();
            ui.horizontal(|ui| {
                ui.label(super::garment_display::counted(
                    count,
                    i18n::tr(ui_lang, "pin"),
                    i18n::tr(ui_lang, "pins"),
                ));
                delete_pins = ui
                    .add_enabled(
                        count > 0,
                        egui::Button::new(i18n::tr(ui_lang, "Delete all pins")),
                    )
                    .clicked();
            });
            if delete_pins {
                self.delete_all_pins();
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(super::garment_display::counted(
                    tacks,
                    i18n::tr(ui_lang, "tack"),
                    i18n::tr(ui_lang, "tacks"),
                ));
                remove_tacks = ui
                    .add_enabled(
                        tacks > 0,
                        egui::Button::new(i18n::tr(ui_lang, "Remove tacks")),
                    )
                    .clicked();
            });
            if remove_tacks {
                let ids = self
                    .project
                    .template
                    .tacks
                    .iter()
                    .map(|t| t.id.clone())
                    .collect();
                if self.edit(authoring::Command::RemoveTacks { ids }) {
                    self.status = i18n::tr(ui_lang, "Tacks removed").into();
                }
            }
            if ui
                .button(i18n::tr(ui_lang, "Fasten buttons"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Fasten each button to the nearest buttonhole on another piece",
                ))
                .clicked()
            {
                self.fasten(true);
            }
            if ui.button(i18n::tr(ui_lang, "Unfasten")).clicked() {
                self.fasten(false);
            }
        });
    }

    pub(super) fn fasten(&mut self, fasten: bool) {
        match authoring::prepare_fasten_edit(
            &self.project.template,
            &self.scene.garment,
            &self.scene.state,
            fasten,
        ) {
            Ok(edit) => {
                self.authoring.history.record_named(
                    self.project.template.clone(),
                    if fasten {
                        "Fasten buttons"
                    } else {
                        "Unfasten buttons"
                    },
                );
                self.install_edit(edit);
                self.status = if fasten {
                    "Buttons fastened to their buttonholes".into()
                } else {
                    "Buttons unfastened".into()
                };
            }
            Err(error) => self.status = error,
        }
    }

    /// Tack-mode click: the first point, then the second, which makes the tack.
    fn tack_click(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        let ui_lang = self.prefs.language;
        let Some(v) = self.picked_vertex(pos, rect) else {
            self.status = i18n::tr(ui_lang, "Click on the cloth to tack it").into();
            return;
        };
        let Some(first) = self.authoring.pin_tool.tack_first.take() else {
            self.authoring.pin_tool.tack_first = Some(v);
            self.status = i18n::tr(
                ui_lang,
                "First point chosen · click the point to tack it to",
            )
            .into();
            return;
        };
        let g = &self.scene.garment;
        let point = |v: usize| {
            g.panels.iter().find(|r| r.vertices.contains(&v)).map(|r| {
                fashioncraft::garment::template::TackPoint {
                    panel: r.id.clone(),
                    uv: g.mesh.uv[v],
                }
            })
        };
        let (Some(a), Some(b)) = (point(first), point(v)) else {
            return;
        };
        if first == v {
            self.status = i18n::tr(ui_lang, "Pick two different points").into();
            return;
        }
        let n = (1..)
            .find(|k| {
                !self
                    .project
                    .template
                    .tacks
                    .iter()
                    .any(|t| t.id == format!("tack_{k}"))
            })
            .unwrap_or(1);
        let tack = fashioncraft::garment::template::Tack {
            id: format!("tack_{n}"),
            a,
            b,
        };
        if self.edit(authoring::Command::AddTack { tack }) {
            self.status = i18n::tr(ui_lang, "Tacked · the two points are drawn together").into();
        }
    }

    /// Pin-mode clicks and box drags in the 3D view.
    pub(super) fn pin_interaction(&mut self, response: &egui::Response, rect: egui::Rect) {
        let ui_lang = self.prefs.language;
        if self.authoring.pin_tool.tack {
            if response.clicked_by(egui::PointerButton::Primary)
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.tack_click(pos, rect);
            }
            return;
        }
        let removing = response.ctx.input(|i| i.modifiers.command);
        // Dragging a pin moves the whole pinned area it belongs to.
        if response.drag_started_by(egui::PointerButton::Primary)
            && !removing
            && let Some(origin) = response.ctx.input(|i| i.pointer.press_origin())
        {
            let nearest = self
                .visible_pins(rect)
                .into_iter()
                .map(|(v, at)| (v, at.distance(origin)))
                .filter(|(_, d)| *d <= PIN_PICK_PX)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((v, _)) = nearest {
                self.start_pin_move(v, origin, rect);
            }
        }
        if let Some(moving) = self.authoring.pin_tool.moving.as_mut() {
            if response.dragged_by(egui::PointerButton::Primary)
                && let Some(pos) = response.interact_pointer_pos()
            {
                let (origin, direction) = self.camera.ray(pos, rect);
                if let Some(hit) =
                    picking::ray_plane(origin, direction, moving.anchor, moving.normal)
                {
                    moving.translation = hit - moving.anchor;
                    for (k, v) in moving.vertices.iter().enumerate() {
                        self.scene.state.x[*v] = moving.start[k] + moving.translation;
                        self.scene.state.v[*v] = DVec3::ZERO;
                    }
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary)
                && let Some(moving) = self.authoring.pin_tool.moving.take()
            {
                for (k, v) in moving.vertices.iter().enumerate() {
                    self.scene.state.x[*v] = moving.start[k];
                }
                if moving.translation.length() > 1e-4 {
                    self.apply_pin_move(&moving.vertices, moving.translation);
                }
            }
            return;
        }
        if self.authoring.pin_tool.lasso {
            let tool = &mut self.authoring.pin_tool;
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
                let path = std::mem::take(&mut tool.path);
                if path.len() >= 3 {
                    match lasso_region(&path) {
                        Ok(region) => {
                            let vertices = self.visible_vertices_in(rect, &region);
                            if vertices.is_empty() {
                                self.status =
                                    i18n::tr(ui_lang, "No visible cloth inside the lasso").into();
                            } else {
                                self.apply_pins(&vertices, !removing);
                            }
                        }
                        Err(e) => self.status = e,
                    }
                    return;
                }
            }
        }
        if !self.authoring.pin_tool.lasso
            && response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.authoring.pin_tool.drag = Some((pos, pos));
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let (Some((_, end)), Some(pos)) = (
                self.authoring.pin_tool.drag.as_mut(),
                response.interact_pointer_pos(),
            )
        {
            *end = pos;
        }
        if response.drag_stopped_by(egui::PointerButton::Primary)
            && let Some((start, end)) = self.authoring.pin_tool.drag.take()
        {
            let region = egui::Rect::from_two_pos(start, end);
            if region.width() >= 4. || region.height() >= 4. {
                let vertices =
                    self.visible_vertices_in(rect, &super::selection::Region::Box(region));
                if vertices.is_empty() {
                    self.status = i18n::tr(ui_lang, "No visible cloth inside the box").into();
                } else {
                    self.apply_pins(&vertices, !removing);
                }
                return;
            }
        }
        if response.clicked_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let pins = self.visible_pins(rect);
            let nearest = pins
                .iter()
                .map(|(v, at)| (*v, at.distance(pos)))
                .filter(|(_, d)| *d <= PIN_PICK_PX)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((v, _)) = nearest {
                self.apply_pins(&[v], false);
            } else if let Some(v) = self.picked_vertex(pos, rect) {
                self.apply_pins(&[v], true);
            } else {
                self.status = i18n::tr(ui_lang, "Click on the cloth to pin it").into();
            }
        }
    }

    /// The cloth vertex nearest to where a ray through `pos` first meets visible cloth.
    pub(super) fn picked_vertex(&self, pos: egui::Pos2, rect: egui::Rect) -> Option<usize> {
        let (origin, dir) = self.camera.ray(pos, rect);
        let mut best = f64::INFINITY;
        let mut picked = None;
        for (face, t) in self.scene.garment.mesh.triangles.iter().enumerate() {
            if !self.cloth_face_shown(face) {
                continue;
            }
            if let Some((d, w)) =
                picking::ray_triangle(origin, dir, t.vertices.map(|i| self.scene.state.x[i]))
                && d < best
            {
                best = d;
                let k = (0..3).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
                picked = Some(t.vertices[k]);
            }
        }
        if picked.is_some()
            && self.avatar_shown()
            && self.scene.collider.occludes_ray(origin, dir, best - 1e-5)
        {
            return None;
        }
        picked
    }

    /// The point of the cloth under screen point `pos`, unless the body hides it: its piece,
    /// its material coordinates and where it is.
    pub(super) fn picked_cloth_point(
        &self,
        pos: egui::Pos2,
        rect: egui::Rect,
    ) -> Option<(String, DVec2, DVec3)> {
        let (origin, dir) = self.camera.ray(pos, rect);
        let g = &self.scene.garment;
        let x = &self.scene.state.x;
        let mut best: Option<(f64, usize, [f64; 3])> = None;
        for (face, t) in g.mesh.triangles.iter().enumerate() {
            if !self.cloth_face_shown(face) {
                continue;
            }
            if let Some((d, w)) = picking::ray_triangle(origin, dir, t.vertices.map(|i| x[i]))
                && best.is_none_or(|(b, _, _)| d < b)
            {
                best = Some((d, face, w));
            }
        }
        let (d, face, w) = best?;
        if self.avatar_shown() && self.scene.collider.occludes_ray(origin, dir, d - 1e-5) {
            return None;
        }
        let t = &g.mesh.triangles[face];
        let range = g.panels.iter().find(|r| r.triangles.contains(&face))?;
        let uv = (0..3).map(|k| g.mesh.uv[t.vertices[k]] * w[k]).sum();
        let at = (0..3).map(|k| x[t.vertices[k]] * w[k]).sum();
        Some((range.id.clone(), uv, at))
    }

    /// Whether each compiled panel is on screen (not hidden, not isolated away) and not frozen.
    pub(super) fn pinnable_panels(&self) -> Vec<bool> {
        let frozen: BTreeSet<&str> = self
            .project
            .template
            .frozen_panels
            .iter()
            .map(|p| p.panel.as_str())
            .collect();
        self.scene
            .garment
            .panels
            .iter()
            .map(|p| {
                !frozen.contains(p.id.as_str())
                    && !p.triangles.is_empty()
                    && self.cloth_face_shown(p.triangles.start)
            })
            .collect()
    }

    /// Vertices of `panels` (by index, `true` = included) that project into `region` and are
    /// not hidden behind the avatar or other cloth, with their screen positions.
    pub(super) fn visible_vertices(
        &mut self,
        rect: egui::Rect,
        panels: &[bool],
        keep: impl Fn(usize, egui::Pos2) -> bool,
    ) -> Vec<(usize, egui::Pos2)> {
        let avatar = self.avatar_shown();
        let matrix = self.camera.matrix(rect.width() / rect.height());
        let garment = self.scene.garment.clone();
        let mut candidates = vec![];
        for (range, include) in garment.panels.iter().zip(panels) {
            if !*include {
                continue;
            }
            for v in range.vertices.clone() {
                if let Some(at) = super::gizmo::project_matrix(matrix, rect, self.scene.state.x[v])
                    && keep(v, at)
                {
                    candidates.push((v, at));
                }
            }
        }
        let mut visibility = super::selection::ClothVisibilityQueries::new(
            &self.scene.state.x,
            &garment.mesh.triangles,
            candidates.len(),
            &mut self.authoring.selection.visibility_cache,
        );
        candidates
            .into_iter()
            .filter(|(v, at)| {
                let p: DVec3 = self.scene.state.x[*v];
                let (origin, direction) = self.camera.ray(*at, rect);
                // Stop short of the point: the triangles around it would hide it otherwise.
                let limit = origin.distance(p) - 1e-3;
                !(avatar && self.scene.collider.occludes_ray(origin, direction, limit))
                    && !visibility.occludes(origin, direction, limit)
            })
            .collect()
    }

    pub(super) fn visible_vertices_in(
        &mut self,
        rect: egui::Rect,
        region: &super::selection::Region,
    ) -> Vec<usize> {
        let panels = self.pinnable_panels();
        self.visible_vertices(rect, &panels, |_, at| region.contains(at))
            .into_iter()
            .map(|(v, _)| v)
            .take(MAX_BOX_PINS)
            .collect()
    }

    /// Authored pins (not frozen pieces) that can be seen, with their screen positions.
    pub(super) fn visible_pins(&mut self, rect: egui::Rect) -> Vec<(usize, egui::Pos2)> {
        if self.project.template.pin_targets.is_empty() {
            return vec![];
        }
        let panels = self.pinnable_panels();
        let holds = self.scene.garment.authored_holds.clone();
        self.visible_vertices(rect, &panels, |v, _| holds.get(v).copied().unwrap_or(false))
    }

    /// Begin dragging the pinned area around pin `v`: the pinned points joined to it through
    /// the mesh, moving in the plane through it that faces the camera.
    fn start_pin_move(&mut self, v: usize, origin: egui::Pos2, rect: egui::Rect) {
        let garment = &self.scene.garment;
        let held = |i: usize| garment.authored_holds.get(i).copied().unwrap_or(false);
        let mut next: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for t in &garment.mesh.triangles {
            let [a, b, c] = t.vertices;
            for (p, q) in [(a, b), (b, c), (c, a)] {
                if held(p) && held(q) {
                    next.entry(p).or_default().push(q);
                    next.entry(q).or_default().push(p);
                }
            }
        }
        let mut area = BTreeSet::from([v]);
        let mut stack = vec![v];
        while let Some(i) = stack.pop() {
            for j in next.get(&i).into_iter().flatten() {
                if area.insert(*j) {
                    stack.push(*j);
                }
            }
        }
        let vertices: Vec<usize> = area.into_iter().collect();
        let (_, direction) = self.camera.ray(origin, rect);
        self.playing = false;
        self.authoring.pin_tool.moving = Some(PinMove {
            start: vertices.iter().map(|i| self.scene.state.x[*i]).collect(),
            vertices,
            anchor: self.scene.state.x[v],
            normal: direction.normalize_or_zero(),
            translation: DVec3::ZERO,
        });
    }

    /// Move the pins at `vertices` by `translation` as one validated undo step.
    fn apply_pin_move(&mut self, vertices: &[usize], translation: DVec3) {
        let ui_lang = self.prefs.language;
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
            return;
        }
        let garment = self.scene.garment.clone();
        let mut selections: BTreeMap<String, Vec<DVec2>> = BTreeMap::new();
        for &v in vertices {
            if !garment.authored_holds.get(v).copied().unwrap_or(false) {
                continue;
            }
            if let Some(panel) = garment.panels.iter().find(|p| p.vertices.contains(&v)) {
                selections
                    .entry(panel.id.clone())
                    .or_default()
                    .push(garment.mesh.uv[v]);
            }
        }
        let count: usize = selections.values().map(Vec::len).sum();
        let result = authoring::prepare_pin_move_edit(
            &self.project.template,
            &garment,
            &self.scene.state,
            &selections,
            translation,
        )
        .and_then(|(edit, next)| {
            super::pins_ui::validate(&edit, &next, Some(&self.scene.collider.surface))?;
            Ok(edit)
        });
        match result {
            Ok(edit) => {
                self.authoring
                    .history
                    .record_named(self.project.template.clone(), "Move pins");
                self.install_edit(edit);
                self.status = format!(
                    "{} {}",
                    super::garment_display::counted(count, "pin", "pins"),
                    i18n::tr(ui_lang, "moved · the cloth follows them when simulated")
                );
            }
            Err(error) => {
                self.status = error.clone();
                self.authoring.error = Some(error);
            }
        }
    }

    /// Pin (`hold`) or release the given cloth vertices as one validated undo step.
    pub(super) fn apply_pins(&mut self, vertices: &[usize], hold: bool) {
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
        let (selections, skipped) =
            pin_selections(&garment, &self.project.template, vertices, hold);
        let count: usize = selections.values().map(Vec::len).sum();
        if count == 0 {
            self.status = if skipped > 0 {
                "Frozen pieces are held already; unfreeze them to pin single points".into()
            } else if hold {
                "Those points are pinned already".into()
            } else {
                "No pins there".into()
            };
            return;
        }
        let result = authoring::prepare_pin_group_edit(
            &self.project.template,
            &garment,
            &self.scene.state,
            &selections,
            hold,
        )
        .and_then(|(edit, next)| {
            super::pins_ui::validate(&edit, &next, Some(&self.scene.collider.surface))?;
            Ok(edit)
        });
        match result {
            Ok(edit) => {
                self.authoring.history.record_named(
                    self.project.template.clone(),
                    if hold { "Pin" } else { "Unpin" },
                );
                self.install_edit(edit);
                let what = super::garment_display::counted(count, "pin", "pins");
                self.status = if hold {
                    format!("{what} added · the points stay where they are")
                } else {
                    format!("{what} removed")
                };
            }
            Err(error) => {
                self.status = error.clone();
                self.authoring.error = Some(error);
            }
        }
    }

    /// Remove every authored pin except those of frozen pieces, as one undo step.
    pub(super) fn delete_all_pins(&mut self) {
        let garment = self.scene.garment.clone();
        let vertices: Vec<usize> = (0..garment.authored_holds.len())
            .filter(|v| garment.authored_holds[*v])
            .collect();
        self.apply_pins(&vertices, false);
    }

    /// Pins as small heads on the cloth, and the box being dragged in pin mode.
    pub(super) fn paint_pins_3d(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        let painter = ui.painter().with_clip_rect(rect);
        let heads = if self.prefs.garment_display.pins || self.authoring.pin_tool.enabled {
            self.visible_pins(rect)
        } else {
            vec![]
        };
        for (_, at) in heads {
            painter.circle_filled(at, 3.5, Color32::from_rgb(255, 214, 64));
            painter.circle_stroke(at, 3.5, Stroke::new(1., Color32::from_rgb(90, 60, 0)));
        }
        let path = &self.authoring.pin_tool.path;
        if path.len() >= 2 {
            let stroke = Stroke::new(1.5, Color32::from_rgb(255, 194, 64));
            painter.add(egui::Shape::line(path.clone(), stroke));
            painter.line_segment(
                [path[path.len() - 1], path[0]],
                Stroke::new(1., stroke.color.gamma_multiply(0.5)),
            );
        }
        if let Some((start, end)) = self.authoring.pin_tool.drag {
            let region = egui::Rect::from_two_pos(start, end);
            painter.rect_filled(
                region,
                0.,
                Color32::from_rgba_unmultiplied(255, 214, 64, 28),
            );
            painter.rect_stroke(
                region,
                0.,
                Stroke::new(1., Color32::from_rgb(255, 194, 64)),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// Group cloth vertices into per-panel material points for a pin edit. Frozen panels are
/// skipped (their count is returned); pinning leaves out points already pinned and releasing
/// leaves out points that are not.
fn pin_selections(
    garment: &fashioncraft::garment::CompiledGarment,
    template: &fashioncraft::garment::GarmentTemplate,
    vertices: &[usize],
    hold: bool,
) -> (BTreeMap<String, Vec<DVec2>>, usize) {
    let frozen: BTreeSet<&str> = template
        .frozen_panels
        .iter()
        .map(|p| p.panel.as_str())
        .collect();
    let mut selections: BTreeMap<String, Vec<DVec2>> = BTreeMap::new();
    let mut skipped = 0;
    for &v in vertices {
        let Some(panel) = garment.panels.iter().find(|p| p.vertices.contains(&v)) else {
            continue;
        };
        if frozen.contains(panel.id.as_str()) {
            skipped += 1;
            continue;
        }
        if garment.authored_holds.get(v).copied().unwrap_or(false) == hold {
            continue;
        }
        selections
            .entry(panel.id.clone())
            .or_default()
            .push(garment.mesh.uv[v]);
    }
    (selections, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fashioncraft::garment::GarmentTemplate;

    #[test]
    fn interior_points_pin_where_they_are_and_release() {
        let mut template = GarmentTemplate::two_panel_demo();
        template.seams.clear();
        for p in &mut template.panels {
            p.pins.clear();
            p.pattern.nx = 4;
            p.pattern.ny = 4;
        }
        let garment = template.compile().unwrap();
        let mut state = garment.initial_state.clone();
        for (i, x) in state.x.iter_mut().enumerate() {
            x.z += 0.01 * (i as f64).sin();
        }
        state.v.fill(DVec3::ZERO);
        // An interior vertex of the first panel (not on its outline) and one of the second.
        let first = garment.panels[0].vertices.clone();
        let interior = first
            .clone()
            .find(|v| {
                let uv = garment.mesh.uv[*v];
                uv.x > 0.05 && uv.y > 0.05 && uv.x < 0.3 && uv.y < 0.3
            })
            .unwrap();
        let other = garment.panels[1].vertices.start;
        let (selections, skipped) = pin_selections(&garment, &template, &[interior, other], true);
        assert_eq!((selections.len(), skipped), (2, 0));
        let (edit, next) =
            authoring::prepare_pin_group_edit(&template, &garment, &state, &selections, true)
                .unwrap();
        assert_eq!(edit.template.pin_targets.len(), 2);
        assert_eq!(next.x, state.x, "pinning does not move the cloth");
        assert!(edit.garment.authored_holds[interior]);
        // Already pinned points are left out; releasing takes only pinned ones.
        let (again, _) = pin_selections(&edit.garment, &edit.template, &[interior], true);
        assert!(again.is_empty());
        let (release, _) = pin_selections(
            &edit.garment,
            &edit.template,
            &[interior, first.start],
            false,
        );
        assert_eq!(release.values().map(Vec::len).sum::<usize>(), 1);
        let (released, _) = authoring::prepare_pin_group_edit(
            &edit.template,
            &edit.garment,
            &next,
            &release,
            false,
        )
        .unwrap();
        assert_eq!(released.template.pin_targets.len(), 1);
    }

    #[test]
    fn a_lasso_takes_what_it_encloses_and_not_its_bay() {
        // A U drawn as a stroke: the two arms and the base are inside, the bay between the
        // arms is not, though a box around the stroke would take it.
        let mut path = vec![];
        let mut stroke = |a: egui::Pos2, b: egui::Pos2| {
            for k in 0..20 {
                path.push(a + (b - a) * (k as f32 / 20.));
            }
        };
        let p = egui::pos2;
        stroke(p(0., 0.), p(0., 100.));
        stroke(p(0., 100.), p(100., 100.));
        stroke(p(100., 100.), p(100., 0.));
        stroke(p(100., 0.), p(70., 0.));
        stroke(p(70., 0.), p(70., 70.));
        stroke(p(70., 70.), p(30., 70.));
        stroke(p(30., 70.), p(30., 0.));
        let region = lasso_region(&path).unwrap();
        for inside in [p(15., 20.), p(85., 20.), p(50., 85.)] {
            assert!(region.contains(inside), "{inside:?}");
        }
        for outside in [p(50., 30.), p(150., 50.), p(-5., 50.)] {
            assert!(!region.contains(outside), "{outside:?}");
        }
        // Too few points is not a lasso.
        assert!(lasso_region(&[p(0., 0.), p(10., 0.)]).is_err());
    }

    #[test]
    fn frozen_panels_are_skipped() {
        let mut template = GarmentTemplate::two_panel_demo();
        for p in &mut template.panels {
            p.pins.clear();
        }
        let garment = template.compile().unwrap();
        let state = garment.initial_state.clone();
        let (edit, _) = authoring::prepare_freeze_edit(
            &template,
            &garment,
            &state,
            &template.panels[0].id,
            true,
        )
        .unwrap();
        let v = edit.garment.panels[0].vertices.start;
        let (selections, skipped) = pin_selections(&edit.garment, &edit.template, &[v], true);
        assert!(selections.is_empty());
        assert_eq!(skipped, 1);
    }
}
