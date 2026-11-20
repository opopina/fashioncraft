//! Measuring in the 3D view: a tape along the body or the garment (the shortest way along
//! its edges between two clicked points), the height of a point of the body above the floor,
//! and the girth of the body or the garment where it is clicked (the level cut around it).
//! Read-only: nothing is added to the garment.
use super::*;
use fashioncraft::{avatar::SurfaceMesh, measure3d, trace3d};
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MeasureKind {
    BodyTape,
    BodyHeight,
    BodyGirth,
    GarmentLine,
    GarmentGirth,
}

impl MeasureKind {
    /// The measurement a command starts.
    pub(super) fn of(id: commands::CommandId) -> Option<Self> {
        use commands::CommandId as C;
        Some(match id {
            C::MeasureBodyTape => Self::BodyTape,
            C::MeasureBodyHeight => Self::BodyHeight,
            C::MeasureBodyGirth => Self::BodyGirth,
            C::MeasureGarmentLine => Self::GarmentLine,
            C::MeasureGarmentGirth => Self::GarmentGirth,
            _ => return None,
        })
    }
    fn on_garment(self) -> bool {
        matches!(self, Self::GarmentLine | Self::GarmentGirth)
    }
    /// Clicks one measurement takes.
    fn clicks(self) -> usize {
        match self {
            Self::BodyTape | Self::GarmentLine => 2,
            _ => 1,
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::BodyTape => "Tape measure",
            Self::BodyHeight => "Height measure",
            Self::BodyGirth => "Circumference measure",
            Self::GarmentLine => "Linear measure",
            Self::GarmentGirth => "Circumference measure",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::BodyTape => "Click two points on the body · Esc stops measuring",
            Self::BodyHeight => "Click a point on the body · Esc stops measuring",
            Self::BodyGirth => "Click the body where to measure around it · Esc stops measuring",
            Self::GarmentLine => "Click two points on the garment · Esc stops measuring",
            Self::GarmentGirth => {
                "Click the garment where to measure around it · Esc stops measuring"
            }
        }
    }
}

/// The measurement under way and its last result.
#[derive(Default)]
pub(super) struct Measure3d {
    pub kind: Option<MeasureKind>,
    clicks: Vec<usize>,
    /// The measured line, where it was when measured.
    path: Vec<DVec3>,
    closed: bool,
    value_m: Option<f64>,
}

impl Atelier {
    pub(super) fn start_measuring(&mut self, kind: MeasureKind) {
        self.authoring.measure3d = Measure3d {
            kind: Some(kind),
            ..Default::default()
        };
        self.authoring.trace_pen = None;
        if self.prefs.workspace.mode == layout::ViewMode::Patterns {
            self.prefs.workspace.mode = layout::ViewMode::Split;
        }
        self.status = i18n::tr(self.prefs.language, kind.hint()).into();
    }

    /// The surface a measurement is taken on: the posed body, or the cloth where it is now.
    fn measured_surface(&self, kind: MeasureKind) -> std::sync::Arc<SurfaceMesh> {
        if kind.on_garment() {
            std::sync::Arc::new(SurfaceMesh {
                vertices: self.scene.state.x.clone(),
                triangles: self
                    .scene
                    .garment
                    .mesh
                    .triangles
                    .iter()
                    .map(|t| t.vertices)
                    .collect(),
            })
        } else {
            std::sync::Arc::clone(&self.scene.collider.surface)
        }
    }

    /// Clicks in the 3D view while measuring; `false` when not measuring.
    pub(super) fn measure3d_interaction(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        let Some(kind) = self.authoring.measure3d.kind else {
            return false;
        };
        let ui_lang = self.prefs.language;
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.authoring.measure3d = Default::default();
            self.status = i18n::tr(ui_lang, "Measuring stopped").into();
            return true;
        }
        let Some(at) = response
            .clicked()
            .then(|| response.interact_pointer_pos())
            .flatten()
        else {
            return true;
        };
        let (origin, direction) = self.camera.ray(at, rect);
        let surface = self.measured_surface(kind);
        let Some(v) = trace3d::pick(&surface, origin, direction) else {
            self.status = i18n::tr(ui_lang, kind.hint()).into();
            return true;
        };
        let m = &mut self.authoring.measure3d;
        if m.clicks.len() >= kind.clicks() {
            m.clicks.clear();
        }
        m.clicks.push(v);
        m.value_m = None;
        m.closed = false;
        m.path = vec![surface.vertices[v]];
        if m.clicks.len() < kind.clicks() {
            return true;
        }
        let p = surface.vertices[v];
        let (path, closed) = match kind {
            MeasureKind::BodyTape | MeasureKind::GarmentLine => (
                trace3d::surface_chain(&surface, &m.clicks)
                    .into_iter()
                    .map(|i| surface.vertices[i])
                    .collect(),
                false,
            ),
            MeasureKind::BodyHeight => {
                let floor = surface
                    .vertices
                    .iter()
                    .map(|q| q.y)
                    .fold(f64::INFINITY, f64::min);
                (vec![DVec3::new(p.x, floor, p.z), p], false)
            }
            MeasureKind::BodyGirth | MeasureKind::GarmentGirth => {
                match measure3d::girth(&surface.vertices, &surface.triangles, p) {
                    Some(cut) => (cut.points, cut.closed),
                    None => (vec![p], false),
                }
            }
        };
        let value = measure3d::path_length(&path, closed);
        m.path = path;
        m.closed = closed;
        m.value_m = Some(value);
        let what = match (kind, closed) {
            (MeasureKind::BodyTape, _) => "Along the body",
            (MeasureKind::BodyHeight, _) => "Height above the floor",
            (MeasureKind::BodyGirth, _) => "Around the body",
            (MeasureKind::GarmentLine, _) => "Along the garment",
            (MeasureKind::GarmentGirth, true) => "Around the garment",
            (MeasureKind::GarmentGirth, false) => "Across the garment (it does not close here)",
        };
        self.status = format!("{}: {:.1} mm", i18n::tr(ui_lang, what), value * 1000.);
        true
    }

    /// The measuring bar over the 3D view.
    pub(super) fn measure3d_bar(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let Some(kind) = self.authoring.measure3d.kind else {
            return;
        };
        let mut stop = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong(i18n::tr(ui_lang, kind.title()));
            match self.authoring.measure3d.value_m {
                Some(value) => {
                    ui.label(
                        egui::RichText::new(format!("{:.1} mm", value * 1000.))
                            .strong()
                            .color(super::workspace::ACCENT),
                    );
                }
                None => {
                    ui.small(i18n::tr(ui_lang, kind.hint()));
                }
            }
            stop = ui.button(i18n::tr(ui_lang, "Stop measuring")).clicked();
        });
        if stop {
            self.authoring.measure3d = Default::default();
            self.status = i18n::tr(ui_lang, "Measuring stopped").into();
        }
    }

    /// The measured line and its value, over the 3D view.
    pub(super) fn paint_measure3d(&self, ui: &egui::Ui, rect: egui::Rect) {
        let m = &self.authoring.measure3d;
        if m.kind.is_none() || m.path.is_empty() {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        let colour = super::workspace::ACCENT;
        let mut line: Vec<egui::Pos2> = m
            .path
            .iter()
            .filter_map(|p| super::gizmo::project(&self.camera, rect, *p))
            .collect();
        if m.closed
            && let Some(first) = line.first().copied()
        {
            line.push(first);
        }
        if line.len() >= 2 {
            painter.add(egui::Shape::line(line.clone(), Stroke::new(2., colour)));
        }
        let ends = [
            m.path.first(),
            (m.path.len() > 1 && !m.closed)
                .then(|| m.path.last())
                .flatten(),
        ];
        for p in ends.into_iter().flatten() {
            if let Some(at) = super::gizmo::project(&self.camera, rect, *p) {
                painter.circle_filled(at, 4., colour);
                painter.circle_stroke(at, 4., Stroke::new(1., Color32::BLACK));
            }
        }
        if let (Some(value), Some(at)) = (m.value_m, line.last().copied()) {
            let text = format!("{:.1} mm", value * 1000.);
            let galley =
                painter.layout_no_wrap(text, egui::FontId::proportional(13.), Color32::WHITE);
            let place =
                egui::Rect::from_min_size(at + egui::vec2(8., -22.), galley.size()).expand(4.);
            painter.rect_filled(place, 3., Color32::from_black_alpha(190));
            painter.galley(place.min + egui::vec2(4., 4.), galley, Color32::WHITE);
        }
    }
}
