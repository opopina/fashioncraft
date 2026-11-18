//! Arrange on body: place the active panel on an authored anchor of the current avatar
//! (EXO-AV-002 prototype). The panel sits a chosen distance outside the skin and can wrap around
//! the anchor axis. Rest geometry is untouched. A placement is only committed after the existing
//! VBD initialization gate accepts the resulting state.
use super::*;
use glam::DVec3;
use marvelous_cloth::{
    authoring::{self, Command},
    avatar::diagnostics,
    garment::{
        arrangement::{self, Arrangement, Sidecar},
        template::{Panel, PanelWrap, Placement},
    },
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct ArrangementUi {
    anchor: Option<String>,
    clearance_mm: f64,
    wrap: bool,
    report: Option<Result<String, String>>,
    loaded: Option<(PathBuf, Result<Option<Sidecar>, String>)>,
    /// Show the anchors as points on the avatar's skin in the 3D view.
    pub points: bool,
    /// Those points for the avatar surface they were found on.
    skin_points: Option<(usize, Vec<SkinPoint>)>,
}

/// An anchor where its outward direction meets the skin.
#[derive(Clone)]
struct SkinPoint {
    id: String,
    label: String,
    at: DVec3,
    outward: DVec3,
}

/// How near (points) a click must land to an arrangement point.
const POINT_PX: f32 = 9.;

struct AnchorChoice {
    id: String,
    label: String,
    clearance_m: f64,
    wrap: bool,
    max_wrap_degrees: f64,
}

/// The arrangements to try on an anchor, closest fit first: on an ellipse fitted to the
/// body, then on a cylinder.
fn arrangements(
    sidecar: &Sidecar,
    anchor: &str,
    panel: &Panel,
    clearance_m: f64,
    max_wrap_degrees: Option<f64>,
    surface: &marvelous_cloth::avatar::SurfaceMesh,
) -> Vec<Result<Arrangement, String>> {
    let frame = match sidecar.frame(anchor) {
        Ok(frame) => frame,
        Err(e) => return vec![Err(e)],
    };
    let outline = arrangement::rest_outline(panel);
    let mut out = Vec::new();
    if max_wrap_degrees.is_some() {
        out.push(arrangement::arrange_elliptical(
            &outline,
            &frame,
            clearance_m,
            max_wrap_degrees,
            Some(surface),
        ));
    }
    out.push(arrangement::arrange(
        &outline,
        &frame,
        clearance_m,
        max_wrap_degrees,
        Some(surface),
    ));
    out
}

impl Atelier {
    /// The current avatar's anchors, loaded once per avatar file.
    fn arrangement_sidecar(&mut self) -> Option<Sidecar> {
        let path = self.project.avatar_path.clone();
        if self.authoring.arrangement.loaded.as_ref().map(|(p, _)| p) != Some(&path) {
            self.authoring.arrangement.loaded =
                Some((path.clone(), arrangement::load_sidecar(&path)));
            self.authoring.arrangement.anchor = None;
            self.authoring.arrangement.skin_points = None;
        }
        match &self.authoring.arrangement.loaded {
            Some((_, Ok(Some(s)))) => Some(s.clone()),
            _ => None,
        }
    }

    /// Where each anchor meets the skin of the avatar as it is now.
    fn skin_points(&mut self) -> Vec<SkinPoint> {
        let surface = self.scene.collider.surface.clone();
        let key = Arc::as_ptr(&surface) as usize;
        if let Some((k, points)) = &self.authoring.arrangement.skin_points
            && *k == key
        {
            return points.clone();
        }
        let Some(sidecar) = self.arrangement_sidecar() else {
            return Vec::new();
        };
        let points: Vec<SkinPoint> = sidecar
            .set
            .anchors
            .iter()
            .filter_map(|a| {
                let frame = sidecar.frame(&a.id).ok()?;
                let depth = arrangement::surface_distance(&surface, frame.origin, frame.outward)
                    .unwrap_or(0.);
                Some(SkinPoint {
                    id: a.id.clone(),
                    label: a.label.clone(),
                    at: frame.origin + frame.outward * depth,
                    outward: frame.outward,
                })
            })
            .collect();
        self.authoring.arrangement.skin_points = Some((key, points.clone()));
        points
    }

    /// The arrangement point under `pointer`, among those facing the camera.
    fn point_under(
        &mut self,
        pointer: egui::Pos2,
        rect: egui::Rect,
    ) -> Option<(SkinPoint, egui::Pos2)> {
        let eye = self.camera.eye().as_dvec3();
        self.skin_points()
            .into_iter()
            .filter(|p| p.outward.dot(eye - p.at) > 0.)
            .filter_map(|p| Some((self.camera.project(p.at.as_vec3(), rect)?, p)))
            .filter(|(at, _)| at.distance(pointer) <= POINT_PX)
            .min_by(|(a, _), (b, _)| a.distance(pointer).total_cmp(&b.distance(pointer)))
            .map(|(at, p)| (p, at))
    }

    /// Draw the arrangement points facing the camera, naming the one under the pointer.
    pub(super) fn paint_arrangement_points(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if !self.authoring.arrangement.points {
            return;
        }
        let eye = self.camera.eye().as_dvec3();
        let painter = ui.painter().with_clip_rect(rect);
        let blue = Color32::from_rgb(0x3D, 0x8B, 0xFF);
        for p in self.skin_points() {
            if p.outward.dot(eye - p.at) <= 0. {
                continue;
            }
            if let Some(at) = self.camera.project(p.at.as_vec3(), rect) {
                painter.circle_filled(at, 5., blue);
                painter.circle_stroke(at, 5., Stroke::new(1.5, Color32::WHITE));
            }
        }
        if let Some(pointer) = ui.ctx().pointer_hover_pos()
            && rect.contains(pointer)
            && let Some((p, at)) = self.point_under(pointer, rect)
        {
            painter.circle_stroke(at, 8., Stroke::new(2., Color32::WHITE));
            painter.text(
                at + egui::vec2(10., -10.),
                egui::Align2::LEFT_BOTTOM,
                &p.label,
                egui::FontId::proportional(12.),
                Color32::WHITE,
            );
        }
    }

    /// A click on an arrangement point places the selected piece there. Returns whether the
    /// click was taken.
    pub(super) fn arrangement_point_interaction(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        if !self.authoring.arrangement.points || !response.clicked() {
            return false;
        }
        let Some(pointer) = response.interact_pointer_pos() else {
            return false;
        };
        let Some((point, _)) = self.point_under(pointer, rect) else {
            return false;
        };
        let Some(panel) = self.authoring.selected.clone().and_then(|id| {
            self.project
                .template
                .panels
                .iter()
                .find(|p| p.id == id)
                .cloned()
        }) else {
            self.status = format!(
                "{}: select a piece, then click the point to place it",
                point.label
            );
            return true;
        };
        let Some(sidecar) = self.arrangement_sidecar() else {
            return true;
        };
        let Some(anchor) = sidecar
            .set
            .anchors
            .iter()
            .find(|a| a.id == point.id)
            .cloned()
        else {
            return true;
        };
        let state = &mut self.authoring.arrangement;
        state.anchor = Some(anchor.id.clone());
        state.clearance_mm = anchor.clearance_m * 1000.;
        state.wrap = anchor.wrap;
        let candidates = arrangements(
            &sidecar,
            &anchor.id,
            &panel,
            anchor.clearance_m,
            anchor.wrap.then_some(anchor.max_wrap_degrees),
            &self.scene.collider.surface,
        );
        let report = self.place_on_anchor(&panel.id, &anchor.label, candidates);
        self.status = match &report {
            Ok(message) => format!("{}: {message}", panel.id),
            Err(e) => e.clone(),
        };
        self.authoring.arrangement.report = Some(report);
        true
    }

    pub(super) fn arrangement_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        self.arrangement_sidecar();
        egui::CollapsingHeader::new("Arrange on body")
            .id_salt("arrange-on-body")
            .default_open(true)
            .show(ui, |ui| {
                let sidecar = match &self.authoring.arrangement.loaded {
                    Some((_, Ok(Some(s)))) => s.clone(),
                    Some((_, Ok(None))) => {
                        ui.small("This avatar ships no arrangement anchors (arrangement.json next to its OBJ), and its shape does not allow estimating them: it should stand Y up, facing +Z, arms away from the body.");
                        return;
                    }
                    Some((_, Err(e))) => {
                        ui.colored_label(Color32::LIGHT_RED, format!("Anchors not loaded: {e}"));
                        return;
                    }
                    None => return,
                };
                if sidecar.estimated {
                    ui.small("Anchors on joints estimated from the avatar's shape.");
                }
                let anchors: Vec<AnchorChoice> = sidecar
                    .set
                    .anchors
                    .iter()
                    .map(|a| AnchorChoice {
                        id: a.id.clone(),
                        label: a.label.clone(),
                        clearance_m: a.clearance_m,
                        wrap: a.wrap,
                        max_wrap_degrees: a.max_wrap_degrees,
                    })
                    .collect();
                let state = &mut self.authoring.arrangement;
                if state.anchor.as_ref().is_none_or(|id| !anchors.iter().any(|a| &a.id == id))
                    && let Some(first) = anchors.first()
                {
                    state.anchor = Some(first.id.clone());
                    state.clearance_mm = first.clearance_m * 1000.0;
                    state.wrap = first.wrap;
                }
                let current = state.anchor.clone().unwrap_or_default();
                let chosen = anchors.iter().find(|a| a.id == current);
                let label = chosen.map(|a| a.label.clone()).unwrap_or_default();
                let max_wrap = chosen.map_or(80.0, |a| a.max_wrap_degrees);
                egui::ComboBox::from_label("Anchor").selected_text(&label).height(320.0).show_ui(ui, |ui| {
                    for a in &anchors {
                        if ui.selectable_label(a.id == current, &a.label).clicked() {
                            state.anchor = Some(a.id.clone());
                            state.clearance_mm = a.clearance_m * 1000.0;
                            state.wrap = a.wrap;
                            state.report = None;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut state.clearance_mm).range(0.0..=600.0).speed(1.0).suffix(" mm"))
                        .on_hover_text("Distance from the skin, measured along the anchor's outward direction");
                    ui.label("from skin");
                });
                ui.checkbox(&mut state.wrap, "Wrap around body")
                    .on_hover_text("Curve the panel around the anchor axis, clearing the body across its width and height");
                let (clearance, wrap) = (state.clearance_mm / 1000.0, state.wrap);
                let candidates = arrangements(
                    &sidecar,
                    &current,
                    panel,
                    clearance,
                    wrap.then_some(max_wrap),
                    &self.scene.collider.surface,
                );
                ui.horizontal(|ui| {
                    if ui.button("Preview").on_hover_text("Show the panel on the anchor without changing the document").clicked() {
                        let first = candidates.iter().find(|c| c.is_ok()).unwrap_or(&candidates[0]);
                        match first {
                            Ok(a) => {
                                self.preview_arrangement(&panel.id, a.placement.clone(), a.wrap);
                                self.authoring.arrangement.report = None;
                            }
                            Err(e) => self.authoring.arrangement.report = Some(Err(e.clone())),
                        }
                    }
                    if ui.button("Place").on_hover_text("Check initialization, then place (one undo step)").clicked() {
                        self.authoring.arrangement.report = Some(self.place_on_anchor(&panel.id, &label, candidates.clone()));
                    }
                    if self.authoring.preview.is_some() && ui.button("Cancel").clicked() {
                        self.cancel_preview();
                    }
                });
                match &self.authoring.arrangement.report {
                    Some(Ok(msg)) => {
                        ui.small(msg);
                    }
                    Some(Err(msg)) => {
                        ui.colored_label(Color32::LIGHT_RED, msg);
                    }
                    None => {}
                }
            });
    }

    /// Show the arranged panel (wrap included) without changing the document; Cancel restores it.
    fn preview_arrangement(&mut self, panel: &str, placement: Placement, wrap: Option<PanelWrap>) {
        let Some(range) = self
            .scene
            .garment
            .panels
            .iter()
            .find(|p| p.id == panel)
            .cloned()
        else {
            return;
        };
        if self.authoring.preview.is_none() {
            self.authoring.preview_state = Some(self.scene.state.clone());
            self.generation += 1;
        }
        self.playing = false;
        for i in range.vertices.clone() {
            let uv = self.scene.garment.mesh.uv[i];
            let local = DVec3::new(uv.x, -uv.y, 0.);
            let local = wrap.map_or(local, |w| w.bend(local));
            self.scene.state.x[i] = placement.transform(local);
            self.scene.state.v[i] = DVec3::ZERO;
        }
        self.authoring.preview = Some(placement);
    }

    /// Place on the first of `candidates` (closest fit first) that starts clear of the body
    /// and the other pieces.
    fn place_on_anchor(
        &mut self,
        panel: &str,
        label: &str,
        candidates: Vec<Result<Arrangement, String>>,
    ) -> Result<String, String> {
        self.cancel_preview();
        self.playing = false;
        let mut last = "No arrangement".to_string();
        let mut chosen = None;
        for candidate in candidates {
            let arranged = match candidate {
                Ok(a) => a,
                Err(e) => {
                    last = e;
                    continue;
                }
            };
            let command = Command::Arrange {
                panel: panel.to_string(),
                placement: arranged.placement.clone(),
                wrap: arranged.wrap,
            };
            let edit = match authoring::apply(&self.project.template, command) {
                Ok(edit) => edit,
                Err(e) => {
                    last = e;
                    continue;
                }
            };
            let state = edit.state_after(&self.scene.garment, &self.scene.state);
            match diagnostics::check_vbd_initialization(
                Arc::new(edit.garment),
                &self.scene.collider.surface,
                &state,
            ) {
                Ok(()) => {
                    chosen = Some(arranged);
                    break;
                }
                Err(e) => {
                    last = format!(
                        "Not placed: the panel would start intersecting ({e}). Increase the clearance or choose another anchor."
                    )
                }
            }
        }
        let Some(arranged) = chosen else {
            return Err(last);
        };
        let wrap = arranged.wrap;
        let command = |placement: Placement| Command::Arrange {
            panel: panel.to_string(),
            placement,
            wrap,
        };
        if !self.edit(command(arranged.placement)) {
            return Err(self.status.clone());
        }
        let shape = match wrap {
            Some(w) => match w.half_width_m {
                Some(a) => format!(
                    "wrapped on an ellipse {:.0} mm across, {:.0} mm deep",
                    a * 1000.0,
                    w.radius_m.abs() * 1000.0
                ),
                None => format!("wrapped, radius {:.0} mm", w.radius_m.abs() * 1000.0),
            },
            None => "flat".to_string(),
        };
        Ok(format!(
            "Placed on “{label}”: {:.0} mm from the skin, {shape}. Initialization check passed.",
            self.authoring.arrangement.clearance_mm
        ))
    }
}
