//! The 3D pen: points clicked on the body join along its surface into a line; closing it
//! flattens the part of the body it encloses into a new piece (see `trace3d`), which starts
//! on the body where it was drawn.
use super::*;
use glam::{DVec2, DVec3};
use marvelous_cloth::{authoring::Command, garment::meshing::MeshingSettings, trace3d};
use std::collections::BTreeSet;

/// Points picked so far on the body and the surface path through them, for drawing.
#[derive(Default)]
pub(super) struct TracePen {
    points: Vec<usize>,
    chain: Vec<usize>,
}

/// How far a traced outline may stray from the traced loop when its points are thinned, m.
const OUTLINE_TOLERANCE_M: f64 = 0.0015;

impl Atelier {
    pub(super) fn open_trace_pen(&mut self) {
        let ui_lang = self.prefs.language;
        if !self.avatar_shown() || self.scene.collider.surface.vertices.is_empty() {
            self.status = i18n::tr(ui_lang, "Show the avatar to trace on it").into();
            return;
        }
        self.authoring.trace_pen = Some(TracePen::default());
        self.status = i18n::tr(
            ui_lang,
            "Click on the body to trace a piece · Enter makes it · Backspace removes the last point · Esc cancels",
        )
        .into();
    }

    /// The pen's points and actions above the 3D view.
    pub(super) fn trace_pen_bar(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let Some(pen) = &self.authoring.trace_pen else {
            return;
        };
        let count = pen.points.len();
        let (mut make, mut undo, mut cancel) = (false, false, false);
        ui.horizontal_wrapped(|ui| {
            ui.strong(i18n::tr(ui_lang, "3D pen"));
            ui.label(format!("{}: {count}", i18n::tr(ui_lang, "Points")));
            make = ui
                .add_enabled(
                    count >= 3,
                    egui::Button::new(i18n::tr(ui_lang, "Make piece")),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Close the line and flatten the part of the body it encloses into a new piece",
                ))
                .clicked();
            undo = ui
                .add_enabled(
                    count > 0,
                    egui::Button::new(i18n::tr(ui_lang, "Remove last point")),
                )
                .clicked();
            cancel = ui.button(i18n::tr(ui_lang, "Cancel")).clicked();
        });
        if make {
            self.finish_trace_pen();
        } else if undo {
            self.remove_trace_point();
        } else if cancel {
            self.authoring.trace_pen = None;
            self.status = i18n::tr(ui_lang, "3D pen cancelled").into();
        }
    }

    fn remove_trace_point(&mut self) {
        if let Some(pen) = &mut self.authoring.trace_pen {
            pen.points.pop();
            pen.chain = trace3d::surface_chain(&self.scene.collider.surface, &pen.points);
        }
    }

    /// Clicks and keys while the pen is out; whether it took them.
    pub(super) fn trace_pen_interaction(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        if self.authoring.trace_pen.is_none() {
            return false;
        }
        if !ctx.egui_wants_keyboard_input() {
            let (enter, back, escape) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Backspace),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if escape {
                self.authoring.trace_pen = None;
                self.status = i18n::tr(self.prefs.language, "3D pen cancelled").into();
                return true;
            }
            if back {
                self.remove_trace_point();
            }
            if enter {
                self.finish_trace_pen();
                return true;
            }
        }
        if response.clicked()
            && let Some(at) = response.interact_pointer_pos()
        {
            let (origin, direction) = self.camera.ray(at, rect);
            let body = &self.scene.collider.surface;
            match trace3d::pick(body, origin, direction) {
                Some(v) => {
                    let pen = self.authoring.trace_pen.as_mut().unwrap();
                    if pen.points.last() != Some(&v) {
                        pen.points.push(v);
                        pen.chain = trace3d::surface_chain(body, &pen.points);
                    }
                }
                None => {
                    self.status =
                        i18n::tr(self.prefs.language, "Click on the body to add a point").into();
                }
            }
        }
        true
    }

    /// The line being traced, over the 3D view.
    pub(super) fn paint_trace_pen(&self, ui: &egui::Ui, rect: egui::Rect) {
        let Some(pen) = &self.authoring.trace_pen else {
            return;
        };
        let body = &self.scene.collider.surface;
        let screen = |v: usize| super::gizmo::project(&self.camera, rect, body.vertices[v]);
        let painter = ui.painter().with_clip_rect(rect);
        let colour = Color32::from_rgb(64, 200, 255);
        let line: Vec<egui::Pos2> = pen.chain.iter().filter_map(|v| screen(*v)).collect();
        if line.len() >= 2 {
            painter.add(egui::Shape::line(line, Stroke::new(2., colour)));
        }
        if pen.points.len() >= 3
            && let (Some(a), Some(b)) = (
                pen.points.last().and_then(|v| screen(*v)),
                screen(pen.points[0]),
            )
        {
            painter.line_segment([a, b], Stroke::new(1., colour.gamma_multiply(0.5)));
        }
        for p in pen.points.iter().filter_map(|v| screen(*v)) {
            painter.circle_filled(p, 3.5, colour);
            painter.circle_stroke(p, 3.5, Stroke::new(1., Color32::from_rgb(0, 60, 90)));
        }
    }

    /// Close the line: the enclosed part of the body becomes a new piece, laid on the body.
    fn finish_trace_pen(&mut self) {
        let ui_lang = self.prefs.language;
        if self.pending {
            self.status = i18n::tr(
                ui_lang,
                "Wait for the current step to finish, then try again",
            )
            .into();
            return;
        }
        let Some(pen) = &self.authoring.trace_pen else {
            return;
        };
        let body = self.scene.collider.surface.clone();
        let traced = match trace3d::trace(&body, &pen.points) {
            Ok(traced) => traced,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let outline = trace3d::simplify(&traced.pattern_points(), OUTLINE_TOLERANCE_M);
        let shift = outline
            .iter()
            .copied()
            .fold(DVec2::splat(f64::INFINITY), DVec2::min);
        let before: BTreeSet<String> = self
            .project
            .template
            .panels
            .iter()
            .map(|p| p.id.clone())
            .collect();
        // On the board, to the right of the pieces already there.
        let template = &self.project.template;
        let (mut right, mut top) = (0f64, f64::INFINITY);
        for (i, panel) in template.panels.iter().enumerate() {
            let origin = marvelous_cloth::authoring::layout(template, i);
            right = right.max(origin.x + panel.pattern.width);
            top = top.min(origin.y);
        }
        let at = DVec2::new(
            if template.panels.is_empty() {
                0.
            } else {
                right + 0.1
            },
            if top.is_finite() { top } else { 0. },
        );
        let points: Vec<DVec2> = outline.iter().map(|p| *p - shift + at).collect();
        self.playing = false;
        let command = Command::Polygon {
            tangents: vec![DVec2::ZERO; points.len()],
            points,
            meshing: MeshingSettings {
                target_edge_m: self.mesh_mm * 0.001,
                anisotropy: self.anisotropy,
                axis_degrees: self.mesh_axis,
                ..Default::default()
            },
            fabric: self.project.template.fabrics[0].id.clone(),
            replace: false,
        };
        if !self.edit(command) {
            return;
        }
        self.authoring.trace_pen = None;
        let Some(range) = self
            .scene
            .garment
            .panels
            .iter()
            .find(|p| !before.contains(&p.id))
            .cloned()
        else {
            return;
        };
        // The new piece starts on the body where it was traced, just outside its contact
        // distance.
        let uv = self.scene.garment.mesh.uv[range.vertices.clone()].to_vec();
        let placed = traced.body_positions(&body, &uv, shift, self.settings.contact.radius + 0.001);
        let mut candidate = self.scene.state.clone();
        for (v, p) in range.vertices.clone().zip(placed) {
            candidate.x[v] = p;
            candidate.v[v] = DVec3::ZERO;
        }
        let stretch = traced.stretch_rms * 100.;
        match marvelous_cloth::vbd::VbdSolver::new(
            self.scene.garment.clone(),
            Some(&body),
            &candidate,
        ) {
            Ok(_) => {
                self.scene.state = candidate;
                self.issue.invalidate();
                self.generation += 1;
                self.metrics = None;
                self.grab = None;
                self.status = format!(
                    "Piece traced on the body ({stretch:.1} % stretch on average) · simulate to settle"
                );
            }
            Err(error) => {
                self.status = format!(
                    "Piece made, but it cannot start on the body ({error}); it starts in front"
                );
            }
        }
    }
}
