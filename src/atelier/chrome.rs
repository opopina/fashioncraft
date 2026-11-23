//! Compact viewport headers and contextual inspector, independent of solver choice.
use super::*;
use crate::render::appearance::{AntiAliasing, Appearance};
use fashioncraft::vbd::{FinalDiagnostics, FinalDiagnosticsBackend};
use layout::ViewMode;
#[path = "avatar_diagnostics.rs"]
mod avatar_diagnostics;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InspectorTab {
    Piece,
    Fabric,
    Simulation,
}

/// Shared chrome stays usable at the minimum split width; content belongs below it.
pub(super) struct ViewHeader {
    pub expand: bool,
    pub fit: bool,
    pub settings: egui::Response,
    pub camera: Option<egui::Response>,
}
pub(super) fn view_header(
    ui: &mut egui::Ui,
    title: &str,
    detail: &str,
    expanded: bool,
    is_3d: bool,
    help: &str,
    ui_lang: i18n::Language,
) -> ViewHeader {
    let compact = ui.available_width() < 430.;
    // The title sits on a band a shade darker than the tools below it, as view headers do in
    // desktop 3D tools; the band is painted once the row's height is known.
    let band = ui.painter().add(egui::Shape::Noop);
    let pane = ui.max_rect().x_range();
    let header = ui.horizontal(|ui| {
        // The title sits in the middle of the header (on the left when the view is narrow).
        let row = ui.max_rect();
        if compact {
            ui.strong(if is_3d { "3D" } else { "2D" })
                .on_hover_text(help);
        } else {
            let colour = ui.visuals().weak_text_color();
            // The title, then what the view holds in a quieter tone when there is room for
            // it between the header's icons.
            let parts: Vec<&str> = detail.split(" · ").filter(|p| !p.is_empty()).collect();
            let line = |shown: usize| {
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    title,
                    0.,
                    egui::TextFormat::simple(egui::FontId::proportional(12.5), colour),
                );
                if shown > 0 {
                    job.append(
                        &format!("  ·  {}", parts[..shown].join(" · ")),
                        0.,
                        egui::TextFormat::simple(
                            egui::FontId::proportional(11.5),
                            colour.gamma_multiply(0.7),
                        ),
                    );
                }
                ui.painter().layout_job(job)
            };
            // As much of the detail as fits between the header's icons, last parts first out.
            // The 3D header carries one more icon (the camera menu) than the 2D one.
            let room = row.width() - 2. * if is_3d { 165. } else { 125. };
            let galley = (0..=parts.len())
                .rev()
                .map(line)
                .find(|g| g.size().x <= room)
                .unwrap_or_else(|| line(0));
            let at = egui::pos2(
                row.center().x - galley.size().x / 2.,
                row.center().y - galley.size().y / 2.,
            );
            let area = egui::Rect::from_min_size(at, galley.size());
            ui.painter().galley(at, galley, colour);
            ui.interact(area, ui.id().with("view_title"), egui::Sense::hover())
                .on_hover_text(help);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let expand = icons::icon_only(
                ui,
                icons::Icon::Expand,
                26.,
                expanded,
                i18n::tr(ui_lang, if expanded { "Split" } else { "Expand" }),
            )
            .clicked();
            let settings = icons::dropdown_button(
                ui,
                icons::Icon::Settings,
                i18n::tr(ui_lang, "Display options"),
            );
            let which = if is_3d { "3d" } else { "2d" };
            super::demo::mark(
                ui.ctx(),
                format_args!("header.{which}.settings"),
                settings.rect,
            );
            let fit = icons::icon_only(
                ui,
                icons::Icon::Fit,
                26.,
                false,
                i18n::tr(ui_lang, "Frame all"),
            );
            super::demo::mark(ui.ctx(), format_args!("header.{which}.fit"), fit.rect);
            let fit = fit.clicked();
            let camera = is_3d.then(|| {
                icons::dropdown_button(
                    ui,
                    icons::Icon::ViewIso,
                    i18n::tr(ui_lang, "Camera · standard views, projection and framing"),
                )
            });
            ViewHeader {
                expand,
                fit,
                settings,
                camera,
            }
        })
        .inner
    });
    let rect = egui::Rect::from_x_y_ranges(pane, header.response.rect.y_range().expand(4.));
    ui.painter().set(
        band,
        egui::Shape::rect_filled(rect, 0., ui.visuals().extreme_bg_color),
    );
    header.inner
}

/// Shared by the real popup and CPU-only presentation tests.
pub(super) fn appearance_controls(ui: &mut egui::Ui, value: &mut Appearance, msaa4: bool) {
    ui.set_min_width(240.);
    ui.strong("VIEWPORT DISPLAY");
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Anti-aliasing");
        ui.selectable_value(&mut value.anti_aliasing, AntiAliasing::Off, "Off");
        ui.add_enabled_ui(msaa4, |ui| {
            ui.selectable_value(&mut value.anti_aliasing, AntiAliasing::Msaa4, "4× MSAA");
        })
        .response
        .on_hover_text("Smooth silhouette edges; uses more graphics memory");
    });
    if !msaa4 {
        ui.small("4× MSAA is unavailable on this adapter.");
    }
    ui.separator();
    ui.strong("Background");
    ui.horizontal(|ui| {
        for (name, top, bottom, floor) in [
            ("Neutral", [128, 130, 135], [88, 90, 95], [98, 100, 105]),
            ("Studio", [60, 67, 78], [116, 126, 140], [98, 107, 119]),
            ("Light", [190, 199, 210], [231, 235, 240], [196, 203, 212]),
            ("Dark", [27, 30, 36], [65, 72, 83], [54, 61, 71]),
        ] {
            if ui.small_button(name).clicked() {
                value.top = top;
                value.bottom = bottom;
                value.floor_color = floor;
            }
        }
    });
    ui.checkbox(&mut value.gradient, "Gradient");
    ui.horizontal(|ui| {
        ui.color_edit_button_srgb(&mut value.top);
        ui.label(if value.gradient {
            "Top color"
        } else {
            "Background color"
        });
    });
    if value.gradient {
        ui.horizontal(|ui| {
            ui.color_edit_button_srgb(&mut value.bottom);
            ui.label("Bottom color");
        });
    }
    ui.separator();
    ui.checkbox(&mut value.floor, "Ground plane");
    ui.add_enabled_ui(value.floor, |ui| {
        ui.horizontal(|ui| {
            ui.color_edit_button_srgb(&mut value.floor_color);
            ui.label("Ground color");
        });
        ui.checkbox(&mut value.grid, "Ground grid");
        ui.checkbox(&mut value.shadow, "Soft ground shadow")
            .on_hover_text(
                "Soft contact-style preview under the model. Does not affect simulation.",
            );
        if value.shadow {
            super::workspace::slim(
                ui,
                egui::Slider::new(&mut value.shadow_opacity, 0.0..=0.8).text("Opacity"),
            );
        }
    });
    ui.separator();
    if ui.small_button("Reset display").clicked() {
        *value = Appearance::default();
    }
}

impl Atelier {
    pub(super) fn garment_header(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let detail = format!(
            "{} {:.0} mm · {} · {}",
            i18n::tr(ui_lang, "Mesh"),
            self.mesh_mm,
            super::garment_display::counted(
                self.project.template.panels.len(),
                i18n::tr(ui_lang, "piece"),
                i18n::tr(ui_lang, "pieces"),
            ),
            super::garment_display::counted(
                self.project.template.seams.len(),
                i18n::tr(ui_lang, "seam"),
                i18n::tr(ui_lang, "seams"),
            ),
        );
        let header = view_header(
            ui,
            i18n::tr(self.prefs.language, "3D garment"),
            &detail,
            self.prefs.workspace.mode == ViewMode::Garment,
            true,
            i18n::tr(ui_lang, view_controls::hint_3d(self.authoring.tool)),
            ui_lang,
        );
        if header.expand {
            self.prefs.workspace.toggle_focus(ViewMode::Garment);
        }
        if header.fit {
            let bounds = self.view_bounds();
            if let Some((lo, hi)) = bounds.all {
                self.camera
                    .request_frame((lo + hi) * 0.5, (hi - lo).length() * 0.5);
            }
        }
        egui::Popup::menu(&header.settings).show(|ui| {
            // The menu grew taller than small windows: scroll instead of cutting it off.
            let room = (ui.ctx().content_rect().height() - 120.).max(200.);
            egui::ScrollArea::vertical()
                .id_salt("viewport-display-menu")
                .max_height(room)
                .min_scrolled_height(room)
                .show(ui, |ui| {
                    appearance_controls(ui, &mut self.prefs.viewport, self.viewport.supports_msaa4());
                    ui.separator();
                    ui.strong(i18n::tr(ui_lang, "Overlays"));
                    let clean = ui.checkbox(&mut self.clean_view, i18n::tr(ui_lang, "Clean view · hide all annotations"))
                        .on_hover_text(i18n::tr(ui_lang, "Show only the shaded garment and avatar. Keeps your overlay settings for later."));
                    super::demo::mark(ui.ctx(), "view.clean", clean.rect);
                    ui.checkbox(&mut self.wire, i18n::tr(ui_lang, "Wireframe"));
                    ui.checkbox(&mut self.seam_links, i18n::tr(ui_lang, "Seam links"));
                    self.garment_display_controls(ui);
                });
        });
        egui::Popup::menu(&header.camera.unwrap()).show(|ui| {
            ui.set_max_width(330.);
            self.show_view_bar(ui);
            self.camera_controls(ui);
        });
        self.window_tools(ui, true);
    }

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let title = super::workspace::pane_title(ui, i18n::tr(ui_lang, "Object browser"));
        ui.scope_builder(egui::UiBuilder::new().max_rect(title), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("×")
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Hide the inspector · Window › Properties inspector shows it again",
                    ))
                    .clicked()
                {
                    self.prefs.workspace.inspector_open = false;
                }
            });
        });
        self.object_browser(ui);
        ui.add_space(4.);
        super::workspace::pane_title(ui, i18n::tr(ui_lang, "Property editor"));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            for (tab, label) in [
                (InspectorTab::Piece, "Piece"),
                (InspectorTab::Fabric, "Fabric"),
                (InspectorTab::Simulation, "Simulation"),
            ] {
                let button = super::workspace::tab(
                    ui,
                    &mut self.inspector_tab,
                    tab,
                    i18n::tr(ui_lang, label),
                );
                if tab == InspectorTab::Fabric {
                    self.smoke_target("fabric_tab", button.rect.center());
                }
                if tab == InspectorTab::Simulation {
                    super::demo::mark(ui.ctx(), "simulation_tab", button.rect);
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("inspector-body")
            .show(ui, |ui| match self.inspector_tab {
                InspectorTab::Piece => {
                    self.authoring_properties(ui);
                    self.drafting_tools(ui);
                }
                InspectorTab::Fabric => {
                    if let Some(panel) = self
                        .project
                        .template
                        .panels
                        .iter()
                        .find(|p| Some(&p.id) == self.authoring.selected.as_ref())
                        .cloned()
                    {
                        ui.label(&panel.name);
                        self.fabric_properties(ui, &panel);
                    } else {
                        ui.label(i18n::tr(ui_lang, "Select a piece to edit its fabric."));
                    }
                }
                InspectorTab::Simulation => {
                    use super::workspace::{group_title, prop_row, slim};
                    group_title(ui, i18n::tr(ui_lang, "Mesh resolution"));
                    prop_row(ui, i18n::tr(ui_lang, "Particle distance (mm)"), |ui| {
                        slim(ui, egui::Slider::new(&mut self.mesh_mm, 8.0..=60.0).fixed_decimals(0));
                    });
                    prop_row(ui, "", |ui| {
                        if ui.button(i18n::tr(ui_lang, "Remesh")).clicked() {
                            self.remesh();
                        }
                        ui.small(format!(
                            "{} vertices · {} triangles",
                            self.scene.state.x.len(),
                            self.scene.garment.mesh.triangles.len()
                        ));
                    });
                    group_title(ui, i18n::tr(ui_lang, "Solver settings"));
                    prop_row(ui, i18n::tr(ui_lang, "GPU compute"), |ui| {
                        ui.checkbox(&mut self.gpu, "");
                    });
                    prop_row(ui, i18n::tr(ui_lang, "Iterations"), |ui| {
                        slim(ui, egui::Slider::new(&mut self.settings.iterations, 2..=128));
                    });
                    prop_row(ui, i18n::tr(ui_lang, "Damping"), |ui| {
                        slim(ui, egui::Slider::new(&mut self.settings.damping, 0.0..=10.0))
                            .on_hover_text(i18n::tr(ui_lang, "Air damping: how much the air slows the cloth down (1/s)"));
                    });
                    self.simulation_environment(ui);
                    ui.add_space(4.);
                    ui.collapsing(super::workspace::section(i18n::tr(ui_lang, "Advanced meshing")), |ui| {
                        prop_row(ui, i18n::tr(ui_lang, "Anisotropy"), |ui| {
                            slim(ui, egui::Slider::new(&mut self.anisotropy, 1.0..=4.0));
                        });
                        prop_row(ui, i18n::tr(ui_lang, "Mesh axis"), |ui| {
                            slim(ui, egui::Slider::new(&mut self.mesh_axis, 0.0..=180.0).suffix("°"));
                        });
                    });
                    ui.collapsing(super::workspace::section(i18n::tr(ui_lang, "Avatar collision diagnostics")), |ui| {
                        avatar_diagnostics::show(ui, &self.scene, &self.settings, self.generation, self.playing, self.pending);
                    });
                    ui.collapsing(super::workspace::section(i18n::tr(ui_lang, "Diagnostics")), |ui| {
                        ui.checkbox(&mut self.seams, i18n::tr(ui_lang, "All seam samples"));
                        ui.small(if self.settings.friction > 0. {
                            i18n::tr(ui_lang, "Hybrid VBD / OGC · lagged Coulomb friction · uncalibrated materials")
                        } else {
                            i18n::tr(ui_lang, "Hybrid VBD / OGC · frictionless · uncalibrated materials")
                        });
                        if let Some(m) = &self.metrics {
                            ui.small(format!(
                                "Step {:.2} ms · detection {:.2} ms · seam gap {:.2} mm",
                                m.step_ms,
                                m.detection_ms,
                                1000. * m.max_seam_gap_m
                            ));
                            ui.small(format!(
                                "Extension: 95% of rest area ≤ {:.1}% · max {:.1}%",
                                m.extension_p95_percent, m.extension_max_percent
                            ));
                        }
                        ui.separator();
                        ui.strong(i18n::tr(ui_lang, "Final accepted state"));
                        let pending_id = egui::Id::new("clothcraft.final-diagnostics-pending");
                        let requested = ui
                            .ctx()
                            .data(|d| d.get_temp::<worker::FinalDiagnosticsRequest>(pending_id));
                        let sample = worker::final_diagnostics();
                        let error = worker::final_diagnostics_error();
                        let selection = select_final_diagnostics(
                            requested,
                            sample.as_ref(),
                            error.as_ref(),
                            self.generation,
                            self.simulation_error.is_some(),
                        );
                        if selection.requested != requested {
                            ui.ctx().data_mut(|d| {
                                d.remove::<worker::FinalDiagnosticsRequest>(pending_id)
                            });
                        }
                        let requested_now = final_diagnostics_controls(
                            ui,
                            FinalDiagnosticsView {
                                sample: selection.sample,
                                error: selection.error,
                                current_time_s: self.scene.state.time,
                                playing: self.playing,
                                pending: selection.pending,
                            },
                        );
                        if requested_now {
                            let request = worker::request_final_diagnostics(self.generation);
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(pending_id, request));
                            if !self.playing {
                                self.step(ui.ctx());
                            }
                            ui.ctx().request_repaint();
                        }
                    });
                    ui.collapsing(super::workspace::section(i18n::tr(ui_lang, "Motion / jitter")), |ui| {
                        let mut inspect = false;
                        if let Some(m) = self.metrics.as_ref().and_then(|m| m.motion.as_ref()) {
                            ui.small(format!("High-pass RMS {:.3} mm · max {:.3} mm", m.highpass_rms_m * 1000., m.highpass_max_m * 1000.));
                            ui.small(format!("{} vertices with persistent oscillation", m.oscillating_vertices));
                            if m.observed_seconds < 0.75 { ui.small(i18n::tr(ui_lang, "Collecting physical-step history…")); }
                            inspect = ui.add_enabled(!m.hotspots.is_empty(), egui::Button::new(i18n::tr(ui_lang, "Inspect jitter hotspots")))
                                .on_hover_text(i18n::tr(ui_lang, "Pause and mark faces around up to 12 strongest detected vertices. Export report includes motion measurements.")).clicked();
                        } else {
                            ui.small(i18n::tr(ui_lang, "Run simulation to measure motion."));
                        }
                        ui.small(i18n::tr(ui_lang, "Repeated reversals, not rendering FPS. Real fabric vibration can also trigger this indicator."));
                        if inspect { self.inspect_motion(); }
                    });
                }
            });
    }
}

impl Atelier {
    pub(super) fn garment_view(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        let ctx = ui.ctx().clone();
        self.garment_header(ui);
        if let Some(error) = self.simulation_error.clone() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.colored_label(
                    Color32::from_rgb(225, 120, 75),
                    i18n::tr(ui_lang, "SIMULATION PAUSED"),
                );
                ui.label(&error);
                ui.horizontal(|ui| {
                    if ui
                        .small_button(i18n::tr(ui_lang, "Inspect problem faces"))
                        .clicked()
                    {
                        self.set_authoring_tool(Tool::Inspect, false);
                    }
                    if ui.small_button(i18n::tr(ui_lang, "Copy details")).clicked() {
                        ui.ctx().copy_text(error);
                    }
                });
            });
        }
        // What the tool needs right now floats at the bottom of the 3D view instead of pushing
        // it down.
        let pane = ui.max_rect();
        let capsule_ctx = ui.ctx().clone();
        super::workspace::floating_capsule(&capsule_ctx, "garment-capsule", pane, |ui| {
            self.sewing_strip(ui, true);
            if self.authoring.tool == Tool::Grab {
                self.pin_tool_bar(ui);
            }
            if self.authoring.tool == Tool::Grab && !self.authoring.pin_tool.enabled {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(i18n::tr(ui_lang, "Grab"));
                    super::workspace::slim(ui, egui::Slider::new(&mut self.grab_radius_px, 0.0..=150.0).text(i18n::tr(ui_lang, "Radius (px)")))
                        .on_hover_text(i18n::tr(ui_lang, "Size on screen. Zero uses the original point grab. The brush follows the picked piece's surface, avoiding nearby disconnected layers."));
                    super::workspace::slim(ui, egui::Slider::new(&mut self.grab_strength, 0.25..=20.).logarithmic(true).text(i18n::tr(ui_lang, "Strength")))
                        .on_hover_text(i18n::tr(ui_lang, "1× is the original point spring strength. Higher values pull harder; collision limits still apply. Changes apply to the next drag."));
                });
            }
            if self.authoring.tool == Tool::Inspect {
                self.issue_controls(ui);
            }
            if self.authoring.trace_pen.is_some() {
                self.trace_pen_bar(ui);
            }
            self.measure3d_bar(ui);
        });
        self.viewport.appearance = self.prefs.viewport.clone();
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.)),
            egui::Sense::click_and_drag(),
        );
        self.authoring.selection.garment_rect = Some(rect);
        self.display_overlay(&ctx, rect);
        if !self.authoring.radial.blocked && !self.quick.blocked {
            self.sewing_wheel(&response);
            if response.secondary_clicked()
                && !self.authoring.board.gesture_active()
                && self.authoring.drag.is_none()
                && self.grab.is_none()
                && let Some(at) = response.interact_pointer_pos()
            {
                self.authoring.radial.open(&ctx, at);
            }
            if self.authoring.eyedropper {
                // The eyedropper takes the next click; nothing else reacts meanwhile.
                self.eyedropper_interaction(&ctx, &response, rect);
            } else if self.trace_pen_interaction(&ctx, &response, rect) {
                // The 3D pen takes clicks on the body while it is out.
            } else if self.measure3d_interaction(&ctx, &response, rect) {
                // So does measuring.
            } else if self.ik_interaction(&response, rect) {
                // A hand or foot of the pose being edited is being dragged.
            } else if self.trim_place_3d(&response, rect) {
                // A trim waiting for its place takes the click on the cloth.
            } else if self.texture_edit_3d(&response, rect) {
                // Moving a texture takes drags on the cloth.
            } else if self.style_line_interaction(&response, rect) {
                // A style-line tool takes the drags on the garment's edges.
            } else if self.mesh_select_interaction(&response, rect) {
                // A select-mesh tool takes the clicks on the cloth.
            } else if self.graphic_3d_interaction(&response, rect) {
                // A graphic waiting for its place takes the click on the cloth.
            } else if self.arrangement_point_interaction(&response, rect) {
                // A click on an arrangement point placed the selected piece.
            } else {
                if self.authoring.tool == Tool::Grab {
                    if self.authoring.pin_tool.enabled {
                        self.pin_interaction(&response, rect);
                    } else {
                        self.pick_and_drag(&response, rect);
                    }
                }
                self.place_interaction(&response, rect);
                self.edge_interaction(&response, rect);
            }
        }
        self.smoke_grabbed |= self.grab.is_some();
        if self.drag_test && self.smoke_point.is_none() {
            let aim = glam::DVec3::new(0., 1.12, 0.30);
            let front = &self.scene.garment.panels[0];
            let point = self.scene.garment.mesh.triangles[front.triangles.clone()]
                .iter()
                .map(|t| {
                    t.vertices
                        .iter()
                        .map(|i| self.scene.state.x[*i] / 3.)
                        .sum::<glam::DVec3>()
                })
                .min_by(|a, b| a.distance_squared(aim).total_cmp(&b.distance_squared(aim)))
                .unwrap();
            let clip =
                self.camera.matrix(rect.width() / rect.height()) * point.as_vec3().extend(1.);
            let ndc = clip.truncate() / clip.w;
            self.smoke_point = Some(egui::pos2(
                rect.center().x + ndc.x * rect.width() * 0.5,
                rect.center().y - ndc.y * rect.height() * 0.5,
            ));
        }
        if !self.authoring.radial.blocked && !self.quick.blocked {
            self.camera.garment_interact(&response);
        }
        let size = rect.size() * ctx.pixels_per_point();
        // The animation timeline shows its take and posed avatar in place of the live drape,
        // for drawing only.
        let animated_avatar = self.animation_avatar();
        let live_state = self
            .animation_frame_state()
            .map(|shown| std::mem::replace(&mut self.scene.state, shown));
        let mut optics =
            fashioncraft::garment::appearance::bind(&self.project.template, &self.scene.garment);
        // A fabric without a look of its own is drawn as soft off-white cloth, lit like any
        // other fabric, instead of the flat unlit grey kept for diagnostic colours.
        for look in optics.iter_mut().filter(|look| look.is_none()) {
            *look = Some(fashioncraft::garment::appearance::FabricAppearance {
                color_srgb: [0xEE, 0xEB, 0xE5],
                roughness: 0.85,
                ..Default::default()
            });
        }
        self.prefs.garment_display.style.apply(&mut optics);
        let (visible_faces, face_colors) = self.display_buffers();
        let flipped_faces = self.flipped_faces();
        // Trims of the pieces on show, lying on their front faces.
        let trims = if self.prefs.garment_display.garment && self.prefs.garment_display.trims {
            let zippers = fashioncraft::garment::trims::zipper_trims(&self.project.template);
            let shown: Vec<_> = self
                .project
                .template
                .trims
                .iter()
                .chain(&zippers)
                .filter(|t| self.panel_shown(&t.panel_id))
                .cloned()
                .collect();
            let front = self.face_positions(true);
            fashioncraft::garment::trims::trim_triangles(
                &shown,
                &self.scene.garment,
                front.as_ref().unwrap_or(&self.scene.state.x),
            )
        } else {
            vec![]
        };
        let graphic_draws =
            if self.prefs.garment_display.garment && self.prefs.garment_display.graphics {
                self.graphic_draws(&ctx)
            } else {
                vec![]
            };
        let fabric_images = self.fabric_images(&ctx, &optics);
        let fabric_images: Vec<_> = fabric_images
            .iter()
            .map(|maps| maps.each_ref().map(Option::as_deref))
            .collect();
        let graphics: Vec<fashioncraft::render::GraphicDraw> = graphic_draws
            .iter()
            .map(
                |(image, triangles, opacity)| fashioncraft::render::GraphicDraw {
                    image,
                    triangles,
                    opacity: *opacity,
                },
            )
            .collect();
        let display = self.prefs.garment_display;
        let collider = self.scene.collider.clone();
        let shown_avatar = (display.avatar && !self.project.avatar_removed)
            .then_some(animated_avatar.as_deref().unwrap_or(&collider.surface));
        // The avatar's skin and hair, moved with it as it is posed.
        let look_shown = display.avatar_look && !display.schematic;
        if let Some(body) = shown_avatar.filter(|_| look_shown) {
            self.authoring
                .avatar_look
                .prepare(&self.project.avatar_path, body);
        }
        let mut look_parts = Vec::new();
        let avatar_look = shown_avatar
            .filter(|_| look_shown)
            .and_then(|_| self.authoring.avatar_look.view(&mut look_parts));
        // Seam lines would float in the air when their pieces are hidden or isolated away.
        let seams_shown = display.garment && self.authoring.view3d.isolated.is_none();
        let texture_offsets = self.texture_offsets_per_face();
        // High quality: twice the pixels each way, averaged down when shown.
        let supersample = if display.high_quality { 2. } else { 1. };
        let texture = self.viewport.draw_scene(
            [(size.x * supersample) as u32, (size.y * supersample) as u32],
            &self.camera,
            DrawModel {
                marked_faces: if self.clean_view {
                    None
                } else {
                    Some(&self.issue.faces)
                },
                mesh: &self.scene.garment.mesh,
                state: &self.scene.state,
                materials: &self.scene.garment.materials,
                optics: Some(&optics),
                sphere: None,
                avatar: shown_avatar,
                // The contact and pressure maps ask the collider which cloth touches the avatar.
                collider:
                    ((display.contacts || (self.heat && display.pressure)) && !self.clean_view)
                        .then_some(
                            self.scene.collider.as_ref() as &dyn fashioncraft::contact::Collider
                        ),
                seams: if self.seams && !self.clean_view && seams_shown {
                    &self.scene.garment.seams
                } else {
                    &[]
                },
                trims: &trims,
                graphics: &graphics,
                fabric_images: &fabric_images,
                avatar_look,
                display: fashioncraft::render::DisplayOptions {
                    visible_faces: visible_faces.as_deref(),
                    face_colors: face_colors.as_deref(),
                    flipped_faces: flipped_faces.as_deref(),
                    avatar_color: super::garment_display::srgb_to_linear(display.avatar_color),
                    strain_max: display.strain_max_percent / 100.,
                    strain_gradient: display.strain_gradient,
                    strain_mode: self.strain_mode(),
                    schematic: self.schematic_options(),
                    // The solver keeps cloth one contact radius off the avatar; 2 mm more covers
                    // cloth resting just above that shell.
                    contact_tolerance_m: self.settings.contact.radius + 0.002,
                    translucent: display.style == super::garment_display::GarmentStyle::Translucent,
                    texture_offsets: texture_offsets.as_deref(),
                },
            },
            self.wire && !self.clean_view,
            self.heat && !self.clean_view,
            display.contacts && !self.clean_view,
        );
        ui.painter().image(
            texture,
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            Color32::WHITE,
        );
        if self.heat && !self.clean_view {
            let painter = ui.painter().with_clip_rect(rect);
            self.paint_strain_legend(&painter, rect);
            if let Some(pointer) = response.hover_pos() {
                self.paint_strain_readout(&painter, rect, pointer);
            }
        }
        if !self.clean_view {
            self.issue_brush(ui, &response, rect);
            // The technical view is a clean sketch: no coloured edge and seam annotations.
            if !display.schematic {
                self.paint_edges_3d(ui, rect);
                self.paint_seam_links(&ui.painter().with_clip_rect(rect), rect, true);
                self.paint_sewing_ranges(ui, rect, true);
            }
            self.paint_gizmo(ui, rect);
            self.paint_skeleton(ui, rect);
            self.paint_skin_weights(ui, rect);
            self.paint_arrangement_points(ui, rect);
            self.paint_selected_points_3d(ui, rect);
            self.paint_pins_3d(ui, rect);
            self.paint_mesh_select(ui, rect);
            self.paint_style_lines(ui, rect);
            self.paint_trace_pen(ui, rect);
            self.paint_measure3d(ui, rect);
            self.paint_piece_names_3d(ui, rect);
            self.paint_view_corners_3d(ui, rect);
            if self.authoring.tool == Tool::Grab
                && !self.authoring.pin_tool.enabled
                && let Some(p) = response.hover_pos()
            {
                ui.painter().with_clip_rect(rect).circle_stroke(
                    p,
                    self.grab_radius_px.max(4.),
                    Stroke::new(1.5, Color32::from_rgb(255, 194, 64)),
                );
            }
        }
        if let Some(live) = live_state {
            self.scene.state = live;
        }
        if !self.clean_view
            && let Some(g) = &self.grab
        {
            let clip =
                self.camera.matrix(rect.width() / rect.height()) * g.target.as_vec3().extend(1.);
            if clip.w > 0. {
                let ndc = clip.truncate() / clip.w;
                let p = egui::pos2(
                    rect.center().x + ndc.x * rect.width() * 0.5,
                    rect.center().y - ndc.y * rect.height() * 0.5,
                );
                ui.painter()
                    .circle_stroke(p, 8., Stroke::new(2., Color32::from_rgb(255, 194, 64)));
            }
        }
    }
}

struct FinalDiagnosticsSelection<'a> {
    requested: Option<worker::FinalDiagnosticsRequest>,
    sample: Option<&'a FinalDiagnostics>,
    error: Option<&'a str>,
    pending: bool,
}

/// Select only a result produced for the current generation and the latest
/// request. Equal physical timestamps after a reload or reset must not make an
/// older sample look current.
fn select_final_diagnostics<'a>(
    requested: Option<worker::FinalDiagnosticsRequest>,
    result: Option<&'a worker::FinalDiagnosticsResult>,
    failure: Option<&'a worker::FinalDiagnosticsFailure>,
    current_generation: u64,
    simulation_error: bool,
) -> FinalDiagnosticsSelection<'a> {
    let requested = requested.filter(|request| request.generation == current_generation);
    let sample = result
        .filter(|result| requested.is_some_and(|request| request == result.request))
        .map(|result| &result.sample);
    let error = failure
        .filter(|failure| requested.is_some_and(|request| request == failure.request))
        .map(|failure| failure.message.as_str());
    FinalDiagnosticsSelection {
        requested,
        pending: requested.is_some() && sample.is_none() && error.is_none() && !simulation_error,
        sample,
        error,
    }
}

struct FinalDiagnosticsView<'a> {
    sample: Option<&'a FinalDiagnostics>,
    error: Option<&'a str>,
    current_time_s: f64,
    playing: bool,
    pending: bool,
}

struct FinalDiagnosticsText {
    timing: String,
    contacts: String,
    residual: String,
    backend: String,
    freshness: &'static str,
    semantics: &'static str,
}

fn backend_label(backend: FinalDiagnosticsBackend) -> &'static str {
    match backend {
        FinalDiagnosticsBackend::Cpu => "CPU step / CPU audit",
        FinalDiagnosticsBackend::CpuAuditOfGpuState => {
            "GPU-accepted state / CPU audit (GPU timings unavailable)"
        }
    }
}

fn format_final_diagnostics(
    sample: &FinalDiagnostics,
    current_time_s: f64,
    playing: bool,
) -> FinalDiagnosticsText {
    let age_s = current_time_s - sample.state_time_s;
    let freshness = if age_s.abs() <= 1e-6 {
        if playing {
            "fresh at the sampled step"
        } else {
            "fresh (paused at the sampled state)"
        }
    } else if age_s < 0.0 {
        "stale (simulation state reset or reloaded)"
    } else {
        "stale; pause and sample again to inspect the current state"
    };
    FinalDiagnosticsText {
        timing: format!(
            "Accepted state t = {:.3} s · dt = {:.3} ms · sample {:.2} ms (detect {:.2} ms + residual {:.2} ms)",
            sample.state_time_s,
            1000.0 * sample.step_dt_s,
            sample.sample_ms,
            sample.detection_ms,
            sample.residual_ms
        ),
        contacts: {
            let separation = match sample.minimum_distance_m {
                Some(distance) => format!("minimum separation {:.3} mm", 1000.0 * distance),
                None => "no contact term".into(),
            };
            format!(
                "Contacts: {} active / {} retained · {separation}",
                sample.contacts, sample.contact_terms
            )
        },
        residual: match sample.residual_rms_n {
            Some(rms) => format!(
                "Free-DOF RMS residual: {:.4e} N over {} free vertices",
                rms, sample.free_vertices
            ),
            None => "Free-DOF RMS residual unavailable: no free vertices".into(),
        },
        backend: format!("Backend: {}", backend_label(sample.backend)),
        freshness,
        semantics: "Accepted post-step state with fresh final OGC contacts. Unconstrained free-DOF backward-Euler gradient, not a KKT/constrained-equilibrium or per-color mixed-gradient certificate. The in-step detection metric is an earlier query.",
    }
}

fn final_diagnostics_controls(ui: &mut egui::Ui, view: FinalDiagnosticsView<'_>) -> bool {
    let mut requested = false;
    ui.horizontal(|ui| {
        let response = ui
            .button("Sample final state")
            .on_hover_text(
                "Run a one-shot CPU audit after the next accepted step. If paused, this advances one step. It does not change contact coverage, the predictor or the rejection policy.",
            );
        #[cfg(test)]
        super::authoring_smoke::widget_harness::named(
            ui,
            &response,
            "simulation.sample_final_diagnostics",
        );
        if response.clicked() {
            requested = true;
        }
        if view.pending {
            ui.small("Sampling after the next accepted step…");
        }
    });
    if let Some(error) = view.error {
        ui.colored_label(
            Color32::from_rgb(225, 120, 75),
            format!("Final sample failed: {error}"),
        );
    }
    if let Some(sample) = view.sample {
        let text = format_final_diagnostics(sample, view.current_time_s, view.playing);
        ui.small(text.timing);
        ui.small(text.contacts);
        ui.small(text.residual);
        ui.small(format!("{} · {}", text.backend, text.freshness));
        ui.small(text.semantics);
    } else if view.pending {
        ui.small("Sampling is opt-in; no final-state sample has been accepted yet.");
    } else {
        ui.small(
            "No final-state sample yet. Sampling is opt-in and runs after the next accepted step.",
        );
    }
    requested
}

#[cfg(test)]
mod final_diagnostics_tests {
    use super::super::authoring_smoke::widget_harness::Harness;
    use super::*;

    fn sample() -> FinalDiagnostics {
        FinalDiagnostics {
            state_time_s: 1.25,
            step_dt_s: 1.0 / 60.0,
            sample_ms: 4.5,
            detection_ms: 2.0,
            residual_ms: 2.5,
            backend: FinalDiagnosticsBackend::Cpu,
            contacts: 12,
            contact_terms: 20,
            minimum_distance_m: Some(0.001_25),
            free_vertices: 96,
            residual_rms_n: Some(0.125),
            residual_kind: "free_dof_backward_euler_accepted_state_final_contacts",
        }
    }

    #[test]
    fn named_control_requests_a_sample_and_presentation_keeps_units() {
        let sample = sample();
        let mut clicked = false;
        let mut draw = |ui: &mut egui::Ui, clicked: &mut bool| {
            *clicked |= final_diagnostics_controls(
                ui,
                FinalDiagnosticsView {
                    sample: Some(&sample),
                    error: None,
                    current_time_s: sample.state_time_s,
                    playing: false,
                    pending: false,
                },
            );
        };
        let mut harness = Harness::new(egui::vec2(560., 520.));
        harness.frame(&mut clicked, &mut draw);
        harness
            .click(
                "simulation.sample_final_diagnostics",
                &mut clicked,
                &mut draw,
            )
            .unwrap();
        assert!(clicked);

        let text = format_final_diagnostics(&sample, sample.state_time_s, false);
        assert!(text.timing.contains("sample"));
        assert!(text.contacts.contains("12 active / 20 retained"));
        assert!(text.contacts.contains("1.250 mm"));
        assert!(text.residual.contains("N"));
        assert!(text.backend.contains("CPU"));
        assert!(text.semantics.contains("not a KKT"));
        assert!(text.freshness.contains("fresh"));
    }

    #[test]
    fn unavailable_residual_is_unavailable_not_zero() {
        let mut sample = sample();
        sample.free_vertices = 0;
        sample.residual_rms_n = None;
        sample.contact_terms = 0;
        sample.minimum_distance_m = None;
        let text = format_final_diagnostics(&sample, sample.state_time_s, true);
        assert!(text.residual.contains("unavailable"));
        assert!(!text.residual.contains("0.0000e0 N"));
        assert!(text.contacts.contains("no contact term"));
    }

    #[test]
    fn gpu_audit_is_labelled_and_stale_samples_are_not_fresh() {
        let mut sample = sample();
        sample.backend = FinalDiagnosticsBackend::CpuAuditOfGpuState;
        let text = format_final_diagnostics(&sample, sample.state_time_s + 0.5, true);
        assert!(text.backend.contains("GPU"));
        assert!(text.backend.contains("unavailable"));
        assert!(text.freshness.contains("stale"));
    }

    fn request(generation: u64, request_id: u64) -> worker::FinalDiagnosticsRequest {
        worker::FinalDiagnosticsRequest {
            generation,
            request_id,
        }
    }

    fn result(
        request: worker::FinalDiagnosticsRequest,
        state_time_s: f64,
    ) -> worker::FinalDiagnosticsResult {
        let mut sample = sample();
        sample.state_time_s = state_time_s;
        worker::FinalDiagnosticsResult { request, sample }
    }

    fn failure(
        request: worker::FinalDiagnosticsRequest,
        message: &str,
    ) -> worker::FinalDiagnosticsFailure {
        worker::FinalDiagnosticsFailure {
            request,
            message: message.into(),
        }
    }

    #[test]
    fn reload_with_equal_physical_time_rejects_the_old_generation() {
        let old = request(1, 1);
        let old_result = result(old, 0.0);
        let selection = select_final_diagnostics(Some(old), Some(&old_result), None, 2, false);
        assert_eq!(selection.requested, None);
        assert!(selection.sample.is_none());
        assert!(!selection.pending);

        let current = request(2, 2);
        let current_result = result(current, 0.0);
        let selection = select_final_diagnostics(Some(current), Some(&old_result), None, 2, false);
        assert!(selection.sample.is_none());
        assert!(selection.pending);

        let selection =
            select_final_diagnostics(Some(current), Some(&current_result), None, 2, false);
        assert_eq!(
            selection.sample.map(|sample| sample.state_time_s),
            Some(0.0)
        );
        assert!(!selection.pending);
    }

    #[test]
    fn late_previous_request_does_not_satisfy_a_newer_request() {
        let first = request(1, 1);
        let second = request(1, 2);
        let first_result = result(first, 0.5);
        let second_result = result(second, 0.5);

        let selection = select_final_diagnostics(Some(second), Some(&first_result), None, 1, false);
        assert_eq!(selection.requested, Some(second));
        assert!(selection.sample.is_none());
        assert!(selection.pending);

        let first_failure = failure(first, "late failure");
        let selection =
            select_final_diagnostics(Some(second), None, Some(&first_failure), 1, false);
        assert!(selection.error.is_none());
        assert!(selection.pending);

        let selection =
            select_final_diagnostics(Some(second), Some(&second_result), None, 1, false);
        assert!(selection.sample.is_some());
        assert!(!selection.pending);
    }

    #[test]
    fn current_request_error_is_shown_and_stops_pending() {
        let current = request(1, 3);
        let failure = failure(current, "sample failed");
        let selection = select_final_diagnostics(Some(current), None, Some(&failure), 1, false);
        assert_eq!(selection.error, Some("sample failed"));
        assert!(selection.sample.is_none());
        assert!(!selection.pending);
    }
}
