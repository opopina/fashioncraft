//! Render › Photoreal render: a path-traced image of the 3D view that refines in its own window
//! while the editor keeps working, and a PNG of it at the size chosen.
//!
//! The image is drawn by [`fashioncraft::render::path_trace`] on a thread with its own GPU
//! device. The window takes the scene the 3D view draws (garment, trims, prints and avatar with
//! the same materials) and, while it follows the view, takes it again a moment after the camera,
//! the cloth or the look stop changing.
use super::*;
use fashioncraft::render::path_trace::{Environment, RenderWorker, Report, Request, TraceSettings};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How long the view must stay still before the render takes it again.
const SETTLE: Duration = Duration::from_millis(200);

/// Image sizes offered, width by height.
const SIZES: [(u32, u32, &str); 6] = [
    (1280, 720, "HD 1280 × 720"),
    (1920, 1080, "Full HD 1920 × 1080"),
    (3840, 2160, "4K 3840 × 2160"),
    (1080, 1350, "Portrait 1080 × 1350"),
    (1080, 1920, "Vertical 1080 × 1920"),
    (2048, 2048, "Square 2048 × 2048"),
];

pub(super) struct RenderUi {
    pub open: bool,
    worker: Option<RenderWorker>,
    settings: TraceSettings,
    /// Depth of field on (the aperture is kept while it is off).
    depth_of_field: bool,
    aperture_mm: f32,
    /// The next click on the image sets the focus distance.
    picking_focus: bool,
    /// Take the 3D view again whenever it changes.
    follow: bool,
    texture: Option<egui::TextureHandle>,
    samples: u32,
    size: [u32; 2],
    elapsed: Duration,
    /// What the scene last sent was made from, and when that last changed.
    sent: Option<u64>,
    changed: Option<(u64, Instant)>,
    /// A scene was asked of the 3D view and has not come yet.
    awaiting_scene: bool,
    message: Option<String>,
}

impl Default for RenderUi {
    fn default() -> Self {
        Self {
            open: false,
            worker: None,
            settings: TraceSettings::default(),
            depth_of_field: false,
            aperture_mm: 10.,
            picking_focus: false,
            follow: true,
            texture: None,
            samples: 0,
            size: [0, 0],
            elapsed: Duration::ZERO,
            sent: None,
            changed: None,
            awaiting_scene: false,
            message: None,
        }
    }
}

impl RenderUi {
    /// Changes the settings an agent gave (`render_settings`) and reports them all.
    pub(super) fn agent_settings(
        &mut self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use serde_json::{Value, json};
        let before = self.effective();
        let number = |key: &str| {
            args.get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
        };
        let flag = |key: &str| args.get(key).and_then(Value::as_bool);
        let s = &mut self.settings;
        if let Some(v) = number("width") {
            s.width = (v as u32).clamp(64, 8192);
        }
        if let Some(v) = number("height") {
            s.height = (v as u32).clamp(64, 8192);
        }
        if let Some(v) = number("samples") {
            s.samples = (v as u32).max(1);
        }
        if let Some(v) = number("bounces") {
            s.bounces = (v as u32).clamp(1, 32);
        }
        if let Some(v) = number("exposure_ev") {
            s.exposure_ev = v as f32;
        }
        if let Some(v) = number("focus_cm") {
            s.focus_m = (v / 100.) as f32;
        }
        if let Some(v) = number("environment_gain") {
            s.environment_gain = v as f32;
        }
        if let Some(v) = number("environment_turn_degrees") {
            s.environment_turn_degrees = v as f32;
        }
        if let Some(v) = flag("background") {
            s.environment_background = v;
        }
        if let Some(v) = flag("key_light") {
            s.key_light = v;
        }
        if let Some(v) = flag("floor") {
            s.floor = v;
        }
        if let Some(v) = flag("depth_of_field") {
            self.depth_of_field = v;
        }
        if let Some(v) = number("aperture_mm") {
            self.aperture_mm = (v as f32).max(0.);
        }
        if self.effective() != before
            && let Some(w) = &self.worker
        {
            w.send(Request::Settings(self.effective()));
        }
        let s = &self.settings;
        Ok(json!({
            "width": s.width, "height": s.height, "samples": s.samples, "bounces": s.bounces,
            "exposure_ev": s.exposure_ev, "depth_of_field": self.depth_of_field,
            "aperture_mm": self.aperture_mm, "focus_cm": s.focus_m * 100.,
            "environment_gain": s.environment_gain,
            "environment_turn_degrees": s.environment_turn_degrees,
            "background": s.environment_background, "key_light": s.key_light, "floor": s.floor,
        }))
    }

    /// The settings sent to the render.
    fn effective(&self) -> TraceSettings {
        TraceSettings {
            aperture_mm: if self.depth_of_field {
                self.aperture_mm
            } else {
                0.
            },
            ..self.settings.clone()
        }
    }
}

impl Atelier {
    /// Samples per pixel of the image the render window shows.
    pub(super) fn render_samples(&self) -> u32 {
        self.authoring.render.samples
    }

    /// Light the render with the HDR image at `path`, as Environment › HDR image does.
    pub(super) fn set_render_environment(&mut self, path: &std::path::Path) -> Result<(), String> {
        let r = &mut self.authoring.render;
        r.settings.environment = Some(Arc::new(Environment::load(path)?));
        if let Some(w) = &r.worker {
            w.send(Request::Settings(r.effective()));
        }
        Ok(())
    }

    /// Whether the render window's worker runs, so an image can be saved.
    pub(super) fn render_running(&self) -> bool {
        self.authoring.render.worker.is_some()
    }
    /// Write the render's image so far to `path` (a PNG), as Save image does.
    pub(super) fn save_render(&self, path: std::path::PathBuf) {
        if let Some(w) = &self.authoring.render.worker {
            w.send(Request::Save(path));
        }
    }

    pub(super) fn open_photoreal_render(&mut self) {
        let r = &mut self.authoring.render;
        r.open = true;
        r.settings.focus_m = self.camera.distance;
        r.sent = None;
        self.status = i18n::tr(
            self.prefs.language,
            "Photoreal render: the image refines while you keep working",
        )
        .into();
    }

    /// What the 3D view shows, as a number that changes when the view does.
    fn render_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let c = &self.camera;
        for v in [
            c.yaw,
            c.pitch,
            c.distance,
            c.target.x,
            c.target.y,
            c.target.z,
            c.fov_degrees,
        ] {
            v.to_bits().hash(&mut h);
        }
        c.orthographic.hash(&mut h);
        let x = &self.scene.state.x;
        x.len().hash(&mut h);
        for p in x.iter().step_by(17) {
            for v in [p.x, p.y, p.z] {
                v.to_bits().hash(&mut h);
            }
        }
        format!("{:?}", self.project.template.fabric_appearance).hash(&mut h);
        format!("{:?}", self.viewport.appearance).hash(&mut h);
        self.project.template.graphics.len().hash(&mut h);
        self.scene.collider.surface.vertices.len().hash(&mut h);
        h.finish()
    }

    /// Before the 3D view draws: ask it for its scene when the render needs one.
    pub(super) fn render_before_draw(&mut self) {
        if !self.authoring.render.open {
            return;
        }
        let print = self.render_fingerprint();
        let r = &mut self.authoring.render;
        if r.awaiting_scene {
            self.viewport.request_trace_scene();
            return;
        }
        let stale = r.sent != Some(print);
        if !stale {
            r.changed = None;
            return;
        }
        if r.sent.is_some() && !r.follow {
            return;
        }
        let settled = match r.changed {
            Some((p, since)) if p == print => since.elapsed() >= SETTLE,
            _ => {
                r.changed = Some((print, Instant::now()));
                r.sent.is_none()
            }
        };
        if settled {
            r.sent = Some(print);
            r.changed = None;
            r.awaiting_scene = true;
            self.viewport.request_trace_scene();
        }
    }

    pub(super) fn render_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.render.open {
            // Closing the window stops the render and frees its GPU memory.
            self.authoring.render.worker = None;
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        if self.authoring.render.worker.is_none() {
            let repaint = ctx.clone();
            self.authoring.render.worker =
                Some(RenderWorker::spawn(self.viewport.adapter(), move || {
                    repaint.request_repaint()
                }));
            let settings = self.authoring.render.effective();
            if let Some(w) = &self.authoring.render.worker {
                w.send(Request::Settings(settings));
            }
        }
        // Hand over a scene the 3D view kept.
        if let Some(scene) = self.viewport.take_trace_scene() {
            let r = &mut self.authoring.render;
            r.awaiting_scene = false;
            if let Some(w) = &r.worker {
                w.send(Request::Scene(Box::new(scene)));
            }
        }
        let r = &mut self.authoring.render;
        // What the render was last told; reports (a focus picked) and the controls change it.
        let before = r.effective();
        for report in r.worker.as_ref().map(|w| w.reports()).unwrap_or_default() {
            match report {
                Report::Image {
                    samples,
                    size,
                    rgba,
                    elapsed,
                } => {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [size[0] as usize, size[1] as usize],
                        &rgba,
                    );
                    match &mut r.texture {
                        Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                        None => {
                            r.texture = Some(ctx.load_texture(
                                "photoreal render",
                                image,
                                egui::TextureOptions::LINEAR,
                            ))
                        }
                    }
                    r.samples = samples;
                    r.size = size;
                    r.elapsed = elapsed;
                }
                Report::Saved(path) => {
                    self.status = format!("{} {}", t("Image saved:"), path.display());
                }
                Report::Focus(distance) => {
                    r.settings.focus_m = distance;
                    r.depth_of_field = true;
                }
                Report::Failed(e) => r.message = Some(e),
            }
        }
        let mut open = true;
        let mut save = false;
        let mut update = false;
        let mut pick_environment = false;
        let mut focus_at: Option<[f32; 2]> = None;
        let viewport_size = self.viewport.size();
        egui::Window::new(t("Photoreal render"))
            .open(&mut open)
            .default_size([720., 620.])
            .resizable(true)
            .show(ctx, |ui| {
                // The image, as large as the window allows at its own proportions.
                let available = ui.available_size() - egui::vec2(0., 250.);
                if let Some(texture) = &r.texture {
                    let [w, h] = r.size.map(|v| v.max(1) as f32);
                    let scale = (available.x / w).min(available.y.max(120.) / h);
                    let response = ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(egui::vec2(w * scale, h * scale))
                            .sense(egui::Sense::click()),
                    );
                    super::demo::mark(ui.ctx(), "render.image", response.rect);
                    if r.picking_focus {
                        response.clone().on_hover_cursor(egui::CursorIcon::Crosshair);
                    }
                    if response.clicked()
                        && r.picking_focus
                        && let Some(p) = response.interact_pointer_pos()
                    {
                        let rect = response.rect;
                        focus_at = Some([
                            ((p.x - rect.left()) / rect.width()).clamp(0., 1.),
                            ((p.y - rect.top()) / rect.height()).clamp(0., 1.),
                        ]);
                        r.picking_focus = false;
                    }
                } else {
                    ui.add_space(40.);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.label(t("Preparing the scene…"));
                    });
                    ui.add_space(40.);
                }
                let done = r.samples >= r.settings.samples;
                ui.horizontal(|ui| {
                    if !done && r.texture.is_some() {
                        ui.spinner();
                    }
                    ui.label(format!(
                        "{} / {} {} · {:.1} s",
                        r.samples,
                        r.settings.samples,
                        t("samples"),
                        r.elapsed.as_secs_f32()
                    ));
                    if let Some(m) = &r.message {
                        ui.colored_label(Color32::from_rgb(230, 120, 90), m);
                    }
                });
                ui.horizontal(|ui| {
                    save = ui
                        .button(t("Save image…"))
                        .on_hover_text(t("Write the image as a PNG at the size chosen below"))
                        .clicked();
                    update = ui
                        .button(t("Update scene"))
                        .on_hover_text(t("Take the 3D view again: camera, cloth and look"))
                        .clicked();
                    ui.checkbox(&mut r.follow, t("Follow the 3D view")).on_hover_text(t(
                        "Start over whenever the camera, the cloth or the look change",
                    ));
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("photoreal render settings")
                        .num_columns(2)
                        .spacing([12., 6.])
                        .show(ui, |ui| {
                            let s = &mut r.settings;
                            ui.label(t("Size"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut s.width)
                                        .range(16..=fashioncraft::render::path_trace::MAX_SIDE)
                                        .suffix(" px"),
                                );
                                ui.label("×");
                                ui.add(
                                    egui::DragValue::new(&mut s.height)
                                        .range(16..=fashioncraft::render::path_trace::MAX_SIDE)
                                        .suffix(" px"),
                                );
                                egui::ComboBox::from_id_salt("photoreal sizes")
                                    .selected_text(t("Presets"))
                                    .show_ui(ui, |ui| {
                                        for (w, h, label) in SIZES {
                                            if ui.selectable_label(false, label).clicked() {
                                                (s.width, s.height) = (w, h);
                                            }
                                        }
                                        if ui.selectable_label(false, t("As the 3D view")).clicked()
                                        {
                                            let [vw, vh] = viewport_size.map(|v| v.max(1));
                                            s.height = (s.width as f32 * vh as f32 / vw as f32)
                                                .round() as u32;
                                        }
                                    });
                            });
                            ui.end_row();
                            ui.label(t("Samples"));
                            ui.add(egui::DragValue::new(&mut s.samples).range(1..=65536))
                                .on_hover_text(t(
                                    "Rays per pixel to stop at: more is smoother and slower",
                                ));
                            ui.end_row();
                            ui.label(t("Bounces"));
                            ui.add(egui::DragValue::new(&mut s.bounces).range(0..=32))
                                .on_hover_text(t("How many times light may bounce between surfaces"));
                            ui.end_row();
                            ui.label(t("Exposure"));
                            ui.add(egui::Slider::new(&mut s.exposure_ev, -5.0..=5.0).suffix(" EV"));
                            ui.end_row();
                            ui.label(t("Depth of field"));
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut r.depth_of_field, "");
                                ui.add_enabled(
                                    r.depth_of_field,
                                    egui::Slider::new(&mut r.aperture_mm, 0.5..=100.0)
                                        .logarithmic(true)
                                        .suffix(" mm"),
                                )
                                .on_hover_text(t("Radius of the lens's opening: wider blurs more"));
                            });
                            ui.end_row();
                            ui.label(t("Focus distance"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut r.settings.focus_m)
                                        .range(0.01..=1000.0)
                                        .speed(0.01)
                                        .suffix(" m"),
                                );
                                let pick = ui
                                    .toggle_value(&mut r.picking_focus, t("Pick on the image"))
                                    .on_hover_text(t("Click the image where it should be sharp"));
                                super::demo::mark(ui.ctx(), "render.pick_focus", pick.rect);
                            });
                            ui.end_row();
                            let s = &mut r.settings;
                            ui.label(t("Environment"));
                            ui.horizontal(|ui| {
                                match &s.environment {
                                    Some(e) => {
                                        ui.label(&e.name);
                                        if ui.small_button(t("Remove")).clicked() {
                                            s.environment = None;
                                        }
                                    }
                                    None => {
                                        ui.label(t("Studio"));
                                    }
                                }
                                pick_environment = ui
                                    .small_button(t("HDR image…"))
                                    .on_hover_text(t(
                                        "Light the scene with an equirectangular HDR image (.hdr)",
                                    ))
                                    .clicked();
                            });
                            ui.end_row();
                            ui.label(t("Environment strength"));
                            ui.add(egui::Slider::new(&mut s.environment_gain, 0.0..=10.0).logarithmic(true));
                            ui.end_row();
                            if s.environment.is_some() {
                                ui.label(t("Environment turn"));
                                ui.add(
                                    egui::Slider::new(&mut s.environment_turn_degrees, -180.0..=180.0)
                                        .suffix("°"),
                                );
                                ui.end_row();
                                ui.label("");
                                ui.checkbox(&mut s.environment_background, t("Show it behind the scene"));
                                ui.end_row();
                            }
                            ui.label(t("Lights"));
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut s.key_light, t("Key light"))
                                    .on_hover_text(t("The 3D view's own key light, with its direction and strength"));
                                ui.checkbox(&mut s.floor, t("Floor"));
                            });
                            ui.end_row();
                        });
                });
            });
        let r = &mut self.authoring.render;
        // Wake up to take the view once it has settled, even if nothing else moves.
        if r.changed.is_some() || r.awaiting_scene {
            ctx.request_repaint_after(SETTLE / 2);
        }
        if r.effective() != before
            && let Some(w) = &r.worker
        {
            w.send(Request::Settings(r.effective()));
        }
        if let Some(position) = focus_at
            && let Some(w) = &r.worker
        {
            w.send(Request::Focus(position));
        }
        if update {
            r.sent = None;
            r.changed = None;
        }
        if pick_environment
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("HDR image", &["hdr", "png", "jpg", "jpeg"])
                .pick_file()
        {
            match Environment::load(&path) {
                Ok(e) => {
                    r.settings.environment = Some(Arc::new(e));
                    r.message = None;
                    if let Some(w) = &r.worker {
                        w.send(Request::Settings(r.effective()));
                    }
                }
                Err(e) => r.message = Some(e),
            }
        }
        if save
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name("render.png")
                .save_file()
            && let Some(w) = &r.worker
        {
            let path = if path.extension().is_none() {
                path.with_extension("png")
            } else {
                path
            };
            w.send(Request::Save(path));
        }
        if !open {
            *r = RenderUi {
                follow: r.follow,
                settings: r.settings.clone(),
                depth_of_field: r.depth_of_field,
                aperture_mm: r.aperture_mm,
                ..Default::default()
            };
        }
    }
}
