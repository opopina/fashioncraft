//! 3D display choices that never change the scene: avatar and garment visibility, avatar
//! colour, isolating the selected pieces, per-piece colours, state tints and the strain-map
//! range with its legend. Hidden or isolated-away cloth is also left out of 3D picking.
use super::*;
use fashioncraft::{garment::template::PanelRange, render};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Strain-map maximum limits, in percent.
pub(super) const STRAIN_MAX_PERCENT: std::ops::RangeInclusive<f64> = 1.0..=200.0;
/// Optional base colour per cloth triangle, linear RGB.
pub(super) type FaceColors = Vec<Option<[f32; 3]>>;
/// Frozen pieces are tinted icy blue (linear RGB).
const FROZEN_TINT: [f32; 3] = [0.42, 0.66, 0.92];
/// Deactivated pieces look washed out, as they no longer take part in the simulation.
const DEACTIVATED_TINT: [f32; 3] = [0.62, 0.62, 0.62];

/// How the garment's fabrics look in the 3D view (Display › Garment style).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum GarmentStyle {
    /// Textures, weave and each fabric's render thickness, as authored.
    #[default]
    TexturedThick,
    /// Textures and weave on a thin surface.
    Textured,
    /// Each fabric's plain colour: no texture, weave or thickness.
    Plain,
    /// Textures and weave on a thin surface, drawn see-through over the avatar.
    Translucent,
}

impl GarmentStyle {
    /// The fabrics' looks as drawn in this style.
    pub(super) fn apply(
        self,
        optics: &mut [Option<fashioncraft::garment::appearance::FabricAppearance>],
    ) {
        for look in optics.iter_mut().flatten() {
            if self != Self::TexturedThick {
                look.render_thickness_mm = 0.;
            }
            if self == Self::Plain {
                look.texture = None;
                look.weave = None;
            }
        }
    }
}

/// What the 3D view draws. Stored in the preferences file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct GarmentDisplay {
    /// Show the avatar (Shift+A).
    pub(super) avatar: bool,
    /// Show the garment (Shift+C).
    pub(super) garment: bool,
    /// Avatar colour, sRGB.
    pub(super) avatar_color: [u8; 3],
    /// Show the avatar's skin, eyes and hair where it has them (avatars from the library or
    /// made with MakeHuman).
    pub(super) avatar_look: bool,
    /// One distinct colour per piece instead of the fabric colour.
    pub(super) piece_colors: bool,
    /// Tint frozen pieces so their state is visible at a glance.
    pub(super) state_colors: bool,
    /// Strain that reaches the strain map's last colour, in percent.
    pub(super) strain_max_percent: f64,
    /// Blend the strain map's eight colours instead of showing discrete bands.
    pub(super) strain_gradient: bool,
    /// Strain map against each fabric's limit instead of one global range.
    pub(super) strain_fabric: bool,
    /// Reference tension of the fabric mode, in N/m: the tension at which a fabric counts as
    /// stretched to its wearable limit.
    pub(super) reference_tension: f64,
    /// The fit map shows the pressure on the avatar instead of strain or tension.
    pub(super) pressure: bool,
    /// Pressure that reaches the pressure map's last colour, kPa.
    pub(super) pressure_max_kpa: f64,
    /// Show where the garment touches the avatar.
    pub(super) contacts: bool,
    /// Show the pins' heads on the cloth (always shown while pinning).
    pub(super) pins: bool,
    /// Show zippers, topstitching and other trims on the garment.
    pub(super) trims: bool,
    /// Show the graphics printed on the garment.
    pub(super) graphics: bool,
    /// How the fabrics look.
    pub(super) style: GarmentStyle,
    /// Show the pieces' internal lines on the 3D garment.
    pub(super) internal_lines: bool,
    /// Label each piece with its name in the 3D view.
    pub(super) piece_names: bool,
    /// High-quality 3D view: the scene drawn at twice the resolution and averaged down, for
    /// smoother edges and fine wrinkles (four times the pixels to draw).
    pub(super) high_quality: bool,
    /// Technical-flat rendering (Alt+0).
    pub(super) schematic: bool,
    pub(super) schematic_line_px: f32,
    pub(super) schematic_line_color: [u8; 3],
    pub(super) schematic_garment_color: [u8; 3],
    pub(super) schematic_silhouette: bool,
    pub(super) schematic_outlines: bool,
}

/// Reference-tension limits of the fabric strain mode, in N/m.
pub(super) const REFERENCE_TENSION: std::ops::RangeInclusive<f64> = 1.0..=500.0;
/// Pressure-map maxima accepted, kPa.
pub(super) const PRESSURE_MAX_KPA: std::ops::RangeInclusive<f64> = 0.1..=50.0;

impl Default for GarmentDisplay {
    fn default() -> Self {
        Self {
            avatar: true,
            garment: true,
            avatar_color: render::AVATAR_COLOR.map(linear_to_srgb),
            avatar_look: true,
            piece_colors: false,
            state_colors: true,
            strain_max_percent: render::STRAIN_MAX * 100.,
            strain_gradient: true,
            strain_fabric: false,
            reference_tension: 20.,
            pressure: false,
            pressure_max_kpa: 2.,
            contacts: false,
            pins: true,
            trims: true,
            graphics: true,
            style: GarmentStyle::TexturedThick,
            internal_lines: true,
            piece_names: false,
            high_quality: false,
            schematic: false,
            schematic_line_px: 1.5,
            schematic_line_color: [0x1E, 0x1E, 0x22],
            schematic_garment_color: [0xF4, 0xF4, 0xF2],
            schematic_silhouette: true,
            schematic_outlines: true,
        }
    }
}

impl GarmentDisplay {
    pub(super) fn sanitize(&mut self) {
        if !self.strain_max_percent.is_finite() {
            self.strain_max_percent = render::STRAIN_MAX * 100.;
        }
        self.strain_max_percent = self
            .strain_max_percent
            .clamp(*STRAIN_MAX_PERCENT.start(), *STRAIN_MAX_PERCENT.end());
        if !self.reference_tension.is_finite() {
            self.reference_tension = 20.;
        }
        self.reference_tension = self
            .reference_tension
            .clamp(*REFERENCE_TENSION.start(), *REFERENCE_TENSION.end());
        if !self.pressure_max_kpa.is_finite() {
            self.pressure_max_kpa = 2.;
        }
        self.pressure_max_kpa = self
            .pressure_max_kpa
            .clamp(*PRESSURE_MAX_KPA.start(), *PRESSURE_MAX_KPA.end());
        if !self.schematic_line_px.is_finite() {
            self.schematic_line_px = 1.5;
        }
        self.schematic_line_px = self.schematic_line_px.clamp(0.5, 8.);
    }
}

/// Transient 3D view state: the isolated pieces, captured when isolation starts.
#[derive(Default)]
pub(super) struct View3d {
    /// `Some` while isolating; the set is the selection at the moment isolation started and does
    /// not follow later selection changes. An empty set leaves the 3D view empty.
    pub(super) isolated: Option<BTreeSet<String>>,
}

fn linear_to_srgb(v: f32) -> u8 {
    let s = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (s.clamp(0., 1.) * 255.).round() as u8
}

pub(super) fn srgb_to_linear(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| {
        let s = f32::from(v) / 255.;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    })
}

/// "1 piece", "3 pieces".
pub(super) fn pieces(count: usize) -> String {
    counted(count, "piece", "pieces")
}

/// "1 guide", "3 guides": a count with the right noun form.
pub(super) fn counted(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// Per-triangle visibility, or `None` when every triangle is shown.
pub(super) fn face_mask(
    panels: &[PanelRange],
    triangle_count: usize,
    garment: bool,
    isolated: Option<&BTreeSet<String>>,
) -> Option<Vec<bool>> {
    if garment && isolated.is_none() {
        return None;
    }
    let mut shown = vec![false; triangle_count];
    if garment {
        for panel in panels {
            if isolated.is_none_or(|set| set.contains(&panel.id)) {
                shown[panel.triangles.clone()].fill(true);
            }
        }
    }
    Some(shown)
}

/// A distinct, stable colour for the piece at `index` (golden-ratio hue steps), linear RGB.
pub(super) fn piece_color(index: usize) -> [f32; 3] {
    let hue = (index as f32 * 0.618_034).fract();
    let (s, v) = (0.45, 0.85);
    let h = hue * 6.;
    let c = v * s;
    let x = c * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = v - c;
    srgb_to_linear([r + m, g + m, b + m].map(|u| (u.clamp(0., 1.) * 255.).round() as u8))
}

/// Per-triangle base colours: piece colours and the frozen tint, or `None` when neither applies.
pub(super) fn face_colors(
    panels: &[PanelRange],
    triangle_count: usize,
    piece_colors: bool,
    frozen: Option<&BTreeSet<String>>,
) -> Option<FaceColors> {
    if !piece_colors && frozen.is_none_or(BTreeSet::is_empty) {
        return None;
    }
    let mut colors = vec![None; triangle_count];
    for (index, panel) in panels.iter().enumerate() {
        let color = if frozen.is_some_and(|set| set.contains(&panel.id)) {
            Some(FROZEN_TINT)
        } else if piece_colors {
            Some(piece_color(index))
        } else {
            None
        };
        colors[panel.triangles.clone()].fill(color);
    }
    Some(colors)
}

impl Atelier {
    fn display_mask(&self) -> Option<Vec<bool>> {
        face_mask(
            &self.scene.garment.panels,
            self.scene.garment.mesh.triangles.len(),
            self.prefs.garment_display.garment,
            self.authoring.view3d.isolated.as_ref(),
        )
    }

    /// Whether a cloth triangle is drawn, and therefore pickable, in the 3D view.
    pub(super) fn cloth_face_shown(&self, face: usize) -> bool {
        let display = &self.prefs.garment_display;
        if !display.garment {
            return false;
        }
        let Some(isolated) = &self.authoring.view3d.isolated else {
            return true;
        };
        self.scene
            .garment
            .panels
            .iter()
            .find(|p| p.triangles.contains(&face))
            .is_some_and(|p| isolated.contains(&p.id))
    }

    /// Whether a piece is drawn, and therefore pickable, in the 3D view.
    pub(super) fn panel_shown_3d(&self, panel: &str) -> bool {
        self.prefs.garment_display.garment
            && self
                .authoring
                .view3d
                .isolated
                .as_ref()
                .is_none_or(|set| set.contains(panel))
    }

    /// Whether the avatar is drawn and blocks 3D picking.
    pub(super) fn avatar_shown(&self) -> bool {
        self.prefs.garment_display.avatar && !self.project.avatar_removed
    }

    /// The per-triangle arrays the renderer borrows for one frame.
    pub(super) fn display_buffers(&self) -> (Option<Vec<bool>>, Option<FaceColors>) {
        let display = &self.prefs.garment_display;
        let frozen: BTreeSet<String> = if display.state_colors {
            self.project
                .template
                .frozen_panels
                .iter()
                .map(|f| f.panel.clone())
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut colors = face_colors(
            &self.scene.garment.panels,
            self.scene.garment.mesh.triangles.len(),
            display.piece_colors,
            Some(&frozen),
        );
        let deactivated: BTreeSet<&str> = self
            .project
            .template
            .deactivated_panels
            .iter()
            .map(|d| d.panel.as_str())
            .collect();
        if display.state_colors && !deactivated.is_empty() {
            let colors =
                colors.get_or_insert_with(|| vec![None; self.scene.garment.mesh.triangles.len()]);
            for panel in &self.scene.garment.panels {
                if deactivated.contains(panel.id.as_str()) {
                    colors[panel.triangles.clone()].fill(Some(DEACTIVATED_TINT));
                }
            }
        }
        (self.display_mask(), colors)
    }

    /// Start or stop isolating the current selection (Shift+I).
    pub(super) fn toggle_isolation(&mut self) {
        let ui_lang = self.prefs.language;
        if self.authoring.view3d.isolated.take().is_some() {
            self.status = i18n::tr(ui_lang, "Isolation ended: every piece is shown again").into();
            return;
        }
        let selected: BTreeSet<String> = self.selected_panels().into_iter().collect();
        self.status = if selected.is_empty() {
            "Isolating an empty selection: the 3D view stays empty until Shift+I".into()
        } else {
            format!(
                "Isolated {} in 3D · Shift+I shows every piece again",
                pieces(selected.len())
            )
        };
        self.authoring.view3d.isolated = Some(selected);
    }

    /// The renderer options these display choices amount to.
    pub(super) fn strain_mode(&self) -> render::StrainMode {
        let display = &self.prefs.garment_display;
        if display.pressure {
            render::StrainMode::Pressure {
                max_pa: display.pressure_max_kpa * 1000.,
            }
        } else if display.strain_fabric {
            render::StrainMode::Fabric {
                reference_n_per_m: display.reference_tension,
            }
        } else {
            render::StrainMode::Global
        }
    }

    pub(super) fn schematic_options(&self) -> Option<render::Schematic> {
        let display = &self.prefs.garment_display;
        display.schematic.then(|| render::Schematic {
            garment_color: srgb_to_linear(display.schematic_garment_color),
            line_color: srgb_to_linear(display.schematic_line_color),
            line_px: display.schematic_line_px,
            silhouette: display.schematic_silhouette,
            outlines: display.schematic_outlines,
        })
    }

    /// Shift+A, Shift+C, Shift+I, Alt+7 and Alt+0, unless a text field has the keyboard.
    pub(super) fn garment_display_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (avatar, garment, isolate, strain, schematic) = ctx.input(|i| {
            let shift = i.modifiers.shift && !i.modifiers.command && !i.modifiers.alt;
            let alt = i.modifiers.alt && !i.modifiers.command;
            (
                shift && i.key_pressed(egui::Key::A),
                shift && i.key_pressed(egui::Key::C),
                shift && i.key_pressed(egui::Key::I),
                alt && i.key_pressed(egui::Key::Num7),
                alt && i.key_pressed(egui::Key::Num0),
            )
        });
        let display = &mut self.prefs.garment_display;
        if avatar {
            display.avatar = !display.avatar;
        }
        if garment {
            display.garment = !display.garment;
        }
        if strain {
            self.heat = !self.heat;
        }
        if schematic {
            display.schematic = !display.schematic;
        }
        if isolate {
            self.toggle_isolation();
        }
    }

    /// The "Display" part of the 3D view's settings menu.
    pub(super) fn garment_display_controls(&mut self, ui: &mut egui::Ui) {
        let ui_lang = self.prefs.language;
        ui.separator();
        ui.strong(i18n::tr(ui_lang, "Display"));
        let display = &mut self.prefs.garment_display;
        ui.horizontal(|ui| {
            ui.checkbox(&mut display.avatar, i18n::tr(ui_lang, "Avatar · Shift+A"));
            ui.color_edit_button_srgb(&mut display.avatar_color)
                .on_hover_text(i18n::tr(ui_lang, "Avatar colour"));
            for (name, linear) in [
                ("Skin", render::AVATAR_COLOR),
                ("Grey", render::AVATAR_GREY),
            ] {
                if ui.small_button(i18n::tr(ui_lang, name)).clicked() {
                    display.avatar_color = linear.map(linear_to_srgb);
                }
            }
        });
        ui.checkbox(
            &mut display.avatar_look,
            i18n::tr(ui_lang, "Skin, eyes and hair"),
        )
        .on_hover_text(i18n::tr(
            ui_lang,
            "The avatar's skin, eyes and hair, where it has them (library and MakeHuman avatars)",
        ));
        ui.checkbox(&mut display.garment, i18n::tr(ui_lang, "Garment · Shift+C"));
        ui.checkbox(
            &mut display.piece_colors,
            i18n::tr(ui_lang, "Random piece colours"),
        )
        .on_hover_text(i18n::tr(
            ui_lang,
            "One distinct colour per piece instead of the fabric colour",
        ));
        ui.checkbox(
            &mut display.state_colors,
            i18n::tr(ui_lang, "Tint frozen and deactivated pieces"),
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.heat, i18n::tr(ui_lang, "Strain map · Alt+7"));
            ui.add(
                egui::DragValue::new(&mut display.strain_max_percent)
                    .range(STRAIN_MAX_PERCENT)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .prefix("max ")
                    .suffix(" %"),
            )
            .on_hover_text(i18n::tr(
                ui_lang,
                "Strain that reaches the last of the eight colours",
            ));
        });
        ui.horizontal(|ui| {
            ui.add_space(22.);
            ui.selectable_value(
                &mut display.strain_gradient,
                true,
                i18n::tr(ui_lang, "Gradient"),
            );
            ui.selectable_value(
                &mut display.strain_gradient,
                false,
                i18n::tr(ui_lang, "Bands"),
            );
            ui.separator();
            let (global, fabric) = (
                !display.pressure && !display.strain_fabric,
                !display.pressure && display.strain_fabric,
            );
            if ui
                .selectable_label(global, i18n::tr(ui_lang, "Global"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "|stretch − 1| against the same range for every fabric",
                ))
                .clicked()
            {
                display.pressure = false;
                display.strain_fabric = false;
            }
            if ui
                .selectable_label(fabric, i18n::tr(ui_lang, "Fabric"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Estimated tension against each fabric's limit: stiff fabrics reach it sooner",
                ))
                .clicked()
            {
                display.pressure = false;
                display.strain_fabric = true;
            }
            if ui
                .selectable_label(display.pressure, i18n::tr(ui_lang, "Pressure"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Pressure on the avatar where the garment touches it: each fibre's tension times the curvature along it",
                ))
                .clicked()
            {
                display.pressure = true;
            }
        });
        if display.pressure {
            ui.horizontal(|ui| {
                ui.add_space(22.);
                ui.label(i18n::tr(ui_lang, "Last colour at"));
                ui.add(
                    egui::DragValue::new(&mut display.pressure_max_kpa)
                        .range(PRESSURE_MAX_KPA)
                        .speed(0.05)
                        .fixed_decimals(2)
                        .suffix(" kPa"),
                );
            });
        }
        if display.strain_fabric && !display.pressure {
            ui.horizontal(|ui| {
                ui.add_space(22.);
                ui.label(i18n::tr(ui_lang, "Limit tension"));
                ui.add(
                    egui::DragValue::new(&mut display.reference_tension)
                        .range(REFERENCE_TENSION)
                        .speed(0.5)
                        .fixed_decimals(1)
                        .suffix(" N/m"),
                )
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Tension at which a fabric counts as stretched to its wearable limit (100 %)",
                ));
            });
        }
        ui.checkbox(&mut display.contacts, i18n::tr(ui_lang, "Contact points"))
            .on_hover_text(i18n::tr(ui_lang, "Cloth touching the avatar, in red"));
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut display.schematic,
                i18n::tr(ui_lang, "Technical view · Alt+0"),
            )
            .on_hover_text(i18n::tr(
                ui_lang,
                "A flat sketch: plain garment colour with outline and silhouette lines",
            ));
            if display.schematic {
                ui.color_edit_button_srgb(&mut display.schematic_garment_color)
                    .on_hover_text(i18n::tr(ui_lang, "Garment colour"));
                ui.color_edit_button_srgb(&mut display.schematic_line_color)
                    .on_hover_text(i18n::tr(ui_lang, "Line colour"));
            }
        });
        if display.schematic {
            ui.horizontal(|ui| {
                ui.add_space(22.);
                ui.add(
                    egui::DragValue::new(&mut display.schematic_line_px)
                        .range(0.5..=8.0)
                        .speed(0.05)
                        .fixed_decimals(1)
                        .suffix(" px"),
                )
                .on_hover_text(i18n::tr(ui_lang, "Line width on screen"));
                ui.checkbox(&mut display.schematic_outlines, i18n::tr(ui_lang, "Edges"));
                ui.checkbox(
                    &mut display.schematic_silhouette,
                    i18n::tr(ui_lang, "Silhouette"),
                );
            });
        }
        let isolating = self.authoring.view3d.isolated.is_some();
        if ui
            .selectable_label(isolating, i18n::tr(ui_lang, "Isolate selection · Shift+I"))
            .on_hover_text(i18n::tr(
                ui_lang,
                "Show only the pieces selected now; press again to show every piece",
            ))
            .clicked()
        {
            self.toggle_isolation();
        }
    }

    /// Strain-map legend in the 3D view's lower-left corner: the colour bar as drawn (gradient
    /// or eight bands) with its 0 %, middle and maximum marks.
    pub(super) fn paint_strain_legend(&self, painter: &egui::Painter, rect: egui::Rect) {
        let display = &self.prefs.garment_display;
        let (max, title, suffix) = if display.pressure {
            (
                display.pressure_max_kpa,
                "Pressure on the body · tension × curvature",
                " kPa",
            )
        } else if display.strain_fabric {
            (
                100.,
                "Fabric limit · tension ÷ limit tension",
                " % of limit",
            )
        } else {
            (display.strain_max_percent, "Strain |stretch − 1|", " %")
        };
        // Pressures are fractions of a kPa; strains whole percent.
        let number = |v: f64| {
            if display.pressure {
                format!("{v:.2}")
            } else {
                format!("{v:.0}")
            }
        };
        let tick = |v: f64| {
            format!(
                "{} {}",
                number(v),
                if display.pressure { "kPa" } else { "%" }
            )
        };
        let bar = egui::vec2(176., 12.);
        let origin = rect.left_bottom() + egui::vec2(12., -12. - bar.y);
        let font = egui::FontId::proportional(10.);
        let text = Color32::from_gray(0xE8);
        let color = |c: [f32; 3]| {
            let s = c.map(linear_to_srgb);
            Color32::from_rgb(s[0], s[1], s[2])
        };
        painter.text(
            origin + egui::vec2(0., -16.),
            egui::Align2::LEFT_BOTTOM,
            title,
            egui::FontId::proportional(11.),
            text,
        );
        let bands = render::STRAIN_BANDS.len();
        if display.strain_gradient {
            // Colour k sits at k/7 of the bar; neighbouring stops are blended per vertex.
            let mut mesh = egui::Mesh::default();
            for (k, c) in render::STRAIN_BANDS.iter().enumerate() {
                let x = origin.x + bar.x * k as f32 / (bands - 1) as f32;
                mesh.colored_vertex(egui::pos2(x, origin.y), color(*c));
                mesh.colored_vertex(egui::pos2(x, origin.y + bar.y), color(*c));
            }
            for k in 0..bands as u32 - 1 {
                let i = 2 * k;
                mesh.add_triangle(i, i + 1, i + 2);
                mesh.add_triangle(i + 1, i + 3, i + 2);
            }
            painter.add(egui::Shape::mesh(mesh));
        } else {
            let swatch = egui::vec2(bar.x / bands as f32, bar.y);
            for (band, c) in render::STRAIN_BANDS.iter().enumerate() {
                let min = origin + egui::vec2(band as f32 * swatch.x, 0.);
                painter.rect_filled(egui::Rect::from_min_size(min, swatch), 0., color(*c));
            }
        }
        for (fraction, align) in [
            (0., egui::Align2::LEFT_BOTTOM),
            (0.5, egui::Align2::CENTER_BOTTOM),
        ] {
            painter.text(
                origin + egui::vec2(bar.x * fraction, -2.),
                align,
                tick(max * fraction as f64),
                font.clone(),
                text,
            );
        }
        painter.text(
            origin + egui::vec2(bar.x + 4., bar.y * 0.5),
            egui::Align2::LEFT_CENTER,
            format!("≥ {}{suffix}", number(max)),
            font,
            text,
        );
    }

    /// Strain readout for the cloth under the pointer while the strain map is shown: the most
    /// stretched direction, its stretch and, in fabric mode, the share of the limit.
    pub(super) fn paint_strain_readout(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        pointer: egui::Pos2,
    ) {
        let (origin, dir) = self.camera.ray(pointer, rect);
        let garment = &self.scene.garment;
        let mut best: Option<(f64, usize)> = None;
        for (face, t) in garment.mesh.triangles.iter().enumerate() {
            if !self.cloth_face_shown(face) {
                continue;
            }
            if let Some((d, _)) = fashioncraft::picking::ray_triangle(
                origin,
                dir,
                t.vertices.map(|i| self.scene.state.x[i]),
            ) && best.is_none_or(|(b, _)| d < b)
            {
                best = Some((d, face));
            }
        }
        let Some((distance, face)) = best else {
            return;
        };
        if self.avatar_shown()
            && self
                .scene
                .collider
                .occludes_ray(origin, dir, distance - 1e-5)
        {
            return;
        }
        let t = &garment.mesh.triangles[face];
        let material = &garment.materials[t.material];
        let text = strain_readout(
            fashioncraft::energy::stretches(&self.scene.state.x, t, material),
            material,
            self.strain_mode(),
        );
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.), Color32::WHITE);
        let at = pointer + egui::vec2(14., 14.);
        let frame = egui::Rect::from_min_size(at, galley.size()).expand(4.);
        painter.rect_filled(frame, 3., Color32::from_black_alpha(190));
        painter.galley(at, galley, Color32::WHITE);
    }
}

/// "Weft · stretch 4.2 % · 61 % of limit": the most stretched direction and its values.
pub(super) fn strain_readout(
    stretch: [f64; 2],
    material: &fashioncraft::model::Material,
    mode: render::StrainMode,
) -> String {
    let tension = render::fibre_tension(stretch, material);
    let (name, index) = match mode {
        render::StrainMode::Fabric { .. } if tension[1] > tension[0] => ("Weft", 1),
        render::StrainMode::Fabric { .. } => ("Warp", 0),
        render::StrainMode::Pressure { .. } if tension[1] > tension[0] => ("Weft", 1),
        render::StrainMode::Pressure { .. } => ("Warp", 0),
        render::StrainMode::Global if (stretch[1] - 1.).abs() > (stretch[0] - 1.).abs() => {
            ("Weft", 1)
        }
        render::StrainMode::Global => ("Warp", 0),
    };
    let elongation = (stretch[index] - 1.) * 100.;
    match mode {
        render::StrainMode::Fabric { reference_n_per_m } => format!(
            "{name} · stretch {elongation:+.1} % · {:.0} % of limit",
            tension[index] / reference_n_per_m.max(1e-9) * 100.
        ),
        render::StrainMode::Global => format!("{name} · stretch {elongation:+.1} %"),
        render::StrainMode::Pressure { .. } => format!(
            "{name} · stretch {elongation:+.1} % · tension {:.0} N/m",
            tension[index]
        ),
    }
}

impl Atelier {
    /// The display capsules floating at the top-left corner of the 3D view, as desktop cloth
    /// tools have them: each icon opens a menu of what it governs, with a check on what is
    /// shown. Render (quality and garment style), what is shown (garment, trims, avatar), the
    /// maps, and isolating the selection.
    pub(super) fn display_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        self.display_capsules(
            ctx,
            "display_overlay",
            rect.min + egui::vec2(10., 10.),
            &DISPLAY_GROUPS,
        );
    }

    /// The corners of the 3D view: the world axes at the bottom left, as a camera reads them,
    /// and a quiet label at the bottom right naming what is chosen (a seam and its type, or
    /// the pieces).
    pub(super) fn paint_view_corners_3d(&self, ui: &egui::Ui, rect: egui::Rect) {
        let painter = ui.painter().with_clip_rect(rect);
        let matrix = self.camera.matrix(rect.width() / rect.height());
        let target = self.camera.target.as_dvec3();
        let reach = f64::from(self.camera.distance.max(0.01)) * 0.1;
        let origin = rect.left_bottom() + egui::vec2(30., -30.);
        let center = super::gizmo::project_matrix(matrix, rect, target);
        let axes = [
            (glam::DVec3::X, "X", Color32::from_rgb(0xE5, 0x53, 0x4B)),
            (glam::DVec3::Y, "Y", Color32::from_rgb(0x57, 0xAB, 0x5A)),
            (glam::DVec3::Z, "Z", Color32::from_rgb(0x53, 0x9B, 0xF5)),
        ];
        for (axis, name, color) in axes {
            let Some(direction) = center
                .zip(super::gizmo::project_matrix(
                    matrix,
                    rect,
                    target + axis * reach,
                ))
                .map(|(a, b)| b - a)
            else {
                continue;
            };
            // An axis pointing straight at the camera shrinks to a dot, as it should.
            let length = direction.length();
            let end = if length > 1e-3 {
                origin + direction / length * 20. * (length / (length + 4.))
            } else {
                origin
            };
            painter.line_segment([origin, end], egui::Stroke::new(2., color));
            painter.text(
                end + (end - origin).normalized() * 7.,
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::proportional(9.5),
                color,
            );
        }
        let chosen = if let Some(seam) = self
            .authoring
            .selection
            .seam
            .as_ref()
            .and_then(|id| self.project.template.seams.iter().find(|s| &s.id == id))
        {
            Some(format!(
                "{} · {}:{}",
                seam.id,
                1 + seam.a_tail.len(),
                1 + seam.b_tail.len()
            ))
        } else if self.authoring.selection.panels.len() > 1 {
            Some(pieces(self.authoring.selection.panels.len()))
        } else {
            self.authoring.selected.as_ref().and_then(|id| {
                self.project
                    .template
                    .panels
                    .iter()
                    .find(|p| &p.id == id)
                    .map(|p| p.name.clone())
            })
        };
        if let Some(text) = chosen {
            let galley = painter.layout_no_wrap(
                text,
                egui::FontId::proportional(11.),
                Color32::from_white_alpha(220),
            );
            let size = galley.size() + egui::vec2(16., 6.);
            let place = super::workspace::clear_of_capsule(
                ui.ctx(),
                "garment-capsule",
                egui::Rect::from_min_size(rect.right_bottom() - size - egui::vec2(12., 10.), size),
            );
            painter.rect_filled(
                place,
                size.y * 0.5,
                Color32::from_rgba_unmultiplied(28, 29, 32, 150),
            );
            painter.galley(place.min + egui::vec2(8., 3.), galley, Color32::WHITE);
        }
    }

    /// The same capsules for the 2D pattern view: pattern information, the board, rulers and
    /// guides, snapping and locking. `rect` is the board canvas; the capsules clear the rulers
    /// when those are shown.
    pub(super) fn board_display_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let inset = if self.rulers_active() { 18. + 8. } else { 10. };
        self.display_capsules(
            ctx,
            "board_display_overlay",
            rect.min + egui::vec2(inset, inset),
            &BOARD_DISPLAY_GROUPS,
        );
    }

    fn display_capsules(
        &mut self,
        ctx: &egui::Context,
        id: &str,
        at: egui::Pos2,
        groups: &[&[DisplayMenu]],
    ) {
        let ui_lang = self.prefs.language;
        egui::Area::new(egui::Id::new(id))
            .fixed_pos(at)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.;
                    for group in groups {
                        let visuals = ui.visuals();
                        egui::Frame::new()
                            .fill(visuals.window_fill.gamma_multiply(0.88))
                            .stroke(egui::Stroke::new(
                                1.,
                                visuals.widgets.noninteractive.bg_stroke.color,
                            ))
                            .corner_radius(9.)
                            .inner_margin(3.)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = 2.;
                                for menu in group.iter() {
                                    self.display_menu(ui, ctx, menu, ui_lang);
                                }
                            });
                    }
                });
            });
    }

    /// Whether a display setting kept outside the command registry is on.
    fn view_checked(&self, kind: ViewToggle) -> bool {
        let viewport = &self.prefs.viewport;
        let pattern = &self.prefs.pattern_display;
        let board = &self.authoring.board;
        match kind {
            ViewToggle::Wireframe => self.wire,
            ViewToggle::SeamLinks => self.seam_links,
            ViewToggle::Floor => viewport.floor,
            ViewToggle::Grid => viewport.grid,
            ViewToggle::Shadow => viewport.shadow,
            ViewToggle::Smooth => {
                viewport.anti_aliasing == crate::render::appearance::AntiAliasing::Msaa4
            }
            ViewToggle::PieceNames2d => pattern.names,
            ViewToggle::Lengths2d => pattern.lengths,
            ViewToggle::Measurements2d => pattern.measurements,
            ViewToggle::BoardGrid => pattern.grid,
            ViewToggle::WhiteSheet => pattern.white_sheet,
            ViewToggle::Silhouette => pattern.silhouette,
            ViewToggle::Rulers => pattern.rulers,
            ViewToggle::Guides => pattern.guides,
            ViewToggle::SnapGrid => board.snap,
            ViewToggle::SmartGuides => board.smart_guides,
        }
    }

    fn toggle_view(&mut self, kind: ViewToggle) {
        let viewport = &mut self.prefs.viewport;
        let pattern = &mut self.prefs.pattern_display;
        let board = &mut self.authoring.board;
        match kind {
            ViewToggle::Wireframe => self.wire = !self.wire,
            ViewToggle::SeamLinks => self.seam_links = !self.seam_links,
            ViewToggle::Floor => viewport.floor = !viewport.floor,
            ViewToggle::Grid => viewport.grid = !viewport.grid,
            ViewToggle::Shadow => viewport.shadow = !viewport.shadow,
            ViewToggle::Smooth => {
                use crate::render::appearance::AntiAliasing;
                viewport.anti_aliasing = if viewport.anti_aliasing == AntiAliasing::Msaa4 {
                    AntiAliasing::Off
                } else {
                    AntiAliasing::Msaa4
                }
            }
            ViewToggle::PieceNames2d => pattern.names = !pattern.names,
            ViewToggle::Lengths2d => pattern.lengths = !pattern.lengths,
            ViewToggle::Measurements2d => pattern.measurements = !pattern.measurements,
            ViewToggle::BoardGrid => pattern.grid = !pattern.grid,
            ViewToggle::WhiteSheet => pattern.white_sheet = !pattern.white_sheet,
            ViewToggle::Silhouette => pattern.silhouette = !pattern.silhouette,
            ViewToggle::Rulers => pattern.rulers = !pattern.rulers,
            ViewToggle::Guides => pattern.guides = !pattern.guides,
            ViewToggle::SnapGrid => board.snap = !board.snap,
            ViewToggle::SmartGuides => board.smart_guides = !board.smart_guides,
        }
        self.prefs_dirty = true;
    }

    /// One icon of the display capsules and the menu it opens (or, for a single toggle, the
    /// toggle itself).
    fn display_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        menu: &DisplayMenu,
        ui_lang: i18n::Language,
    ) {
        let on = menu.items.iter().any(|item| match *item {
            DisplayItem::Cmd(id, ..) => menu.lit_by.contains(&id) && self.command_checked(id),
            DisplayItem::View(kind, ..) => {
                menu.lit_views.contains(&kind) && self.view_checked(kind)
            }
            DisplayItem::Sep => false,
        });
        let single = menu.items.len() == 1;
        let response =
            super::icons::icon_only(ui, menu.icon, 26., on, i18n::tr(ui_lang, menu.title));
        if single {
            if response.clicked()
                && let DisplayItem::Cmd(id, ..) = menu.items[0]
            {
                self.invoke_command(id, ctx);
            }
            return;
        }
        let width = menu
            .items
            .iter()
            .map(|item| match *item {
                DisplayItem::Cmd(id, _, label) => {
                    super::icons::menu_row_width(ui, i18n::tr(ui_lang, label), &self.tool_keys(id))
                        + 22.
                }
                DisplayItem::View(_, _, label) => {
                    super::icons::menu_row_width(ui, i18n::tr(ui_lang, label), "") + 22.
                }
                DisplayItem::Sep => 0.,
            })
            .fold(230., f32::max);
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 1.;
            ui.add_space(2.);
            ui.label(egui::RichText::new(i18n::tr(ui_lang, menu.title)).strong());
            ui.separator();
            for item in menu.items {
                match *item {
                    DisplayItem::Cmd(id, icon, label) => {
                        let keys = self.tool_keys(id);
                        if super::icons::check_row(
                            ui,
                            self.command_checked(id),
                            icon,
                            i18n::tr(ui_lang, label),
                            &keys,
                        )
                        .clicked()
                        {
                            self.invoke_command(id, ctx);
                        }
                    }
                    DisplayItem::View(kind, icon, label) => {
                        if super::icons::check_row(
                            ui,
                            self.view_checked(kind),
                            icon,
                            i18n::tr(ui_lang, label),
                            "",
                        )
                        .clicked()
                        {
                            self.toggle_view(kind);
                        }
                    }
                    DisplayItem::Sep => {
                        ui.separator();
                    }
                }
            }
        });
    }
}

/// A display setting kept outside the command registry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewToggle {
    Wireframe,
    SeamLinks,
    Floor,
    Grid,
    Shadow,
    Smooth,
    PieceNames2d,
    Lengths2d,
    Measurements2d,
    BoardGrid,
    WhiteSheet,
    Silhouette,
    Rulers,
    Guides,
    SnapGrid,
    SmartGuides,
}

#[derive(Clone, Copy)]
enum DisplayItem {
    Cmd(super::commands::CommandId, super::icons::Icon, &'static str),
    View(ViewToggle, super::icons::Icon, &'static str),
    Sep,
}

/// One icon of the display capsules: its menu, and which of its items light the icon up.
struct DisplayMenu {
    icon: super::icons::Icon,
    title: &'static str,
    items: &'static [DisplayItem],
    lit_by: &'static [super::commands::CommandId],
    lit_views: &'static [ViewToggle],
}

mod display_groups {
    use super::super::commands::CommandId as C;
    use super::super::icons::Icon;
    use super::DisplayItem::{Cmd, Sep, View};
    use super::{DisplayMenu, ViewToggle};

    pub(super) const RENDER: &[DisplayMenu] = &[
        DisplayMenu {
            icon: Icon::Gem,
            title: "Render quality (3D view)",
            items: &[
                Cmd(C::HighQualityView, Icon::Gem, "High-quality 3D view"),
                View(ViewToggle::Smooth, Icon::Curve, "Smooth edges"),
                Sep,
                View(ViewToggle::Shadow, Icon::Shadow, "Soft ground shadow"),
                View(ViewToggle::Floor, Icon::Floor, "Floor"),
                View(ViewToggle::Grid, Icon::Grid, "Floor grid"),
            ],
            lit_by: &[C::HighQualityView],
            lit_views: &[],
        },
        DisplayMenu {
            icon: Icon::Fabric,
            title: "Garment style",
            items: &[
                Cmd(
                    C::StyleTexturedThick,
                    Icon::Fabric,
                    "Textured with thickness",
                ),
                Cmd(C::StyleTextured, Icon::Texture, "Textured"),
                Cmd(C::StylePlain, Icon::Swatch, "Plain colour"),
                Cmd(C::StyleTranslucent, Icon::Layers, "Translucent"),
                Sep,
                Cmd(C::TechnicalView, Icon::Technical, "Technical view"),
                View(ViewToggle::Wireframe, Icon::Triangles, "Wireframe"),
            ],
            lit_by: &[],
            lit_views: &[],
        },
    ];

    pub(super) const SHOW: &[DisplayMenu] = &[
        DisplayMenu {
            icon: Icon::Garment,
            title: "Show garment",
            items: &[
                Cmd(C::ToggleGarment, Icon::Garment, "Show garment"),
                Sep,
                Cmd(
                    C::ShowInternalLines,
                    Icon::InternalLine,
                    "Show internal lines",
                ),
                Cmd(C::ShowGraphics, Icon::Graphic, "Show graphics"),
                Cmd(C::ShowPieceNames, Icon::Tag, "Show piece names"),
                View(ViewToggle::SeamLinks, Icon::Seam, "Seam links"),
                Cmd(C::RandomPieceColours, Icon::Colours, "Random piece colours"),
            ],
            lit_by: &[C::ToggleGarment],
            lit_views: &[],
        },
        DisplayMenu {
            icon: Icon::Button,
            title: "Show trims and pins",
            items: &[
                Cmd(C::ShowTrims, Icon::Button, "Show trims"),
                Cmd(C::ShowPins, Icon::Pin, "Show pins"),
            ],
            lit_by: &[C::ShowTrims],
            lit_views: &[],
        },
        DisplayMenu {
            icon: Icon::Avatar,
            title: "Show avatar",
            items: &[
                Cmd(C::ToggleAvatar, Icon::Avatar, "Show avatar"),
                Sep,
                Cmd(C::ToggleSkeleton, Icon::Skeleton, "Show joints"),
                Cmd(
                    C::ToggleArrangementPoints,
                    Icon::Points,
                    "Show arrangement points",
                ),
            ],
            lit_by: &[C::ToggleAvatar],
            lit_views: &[],
        },
    ];

    pub(super) const MAPS: &[DisplayMenu] = &[DisplayMenu {
        icon: Icon::Gradient,
        title: "Maps",
        items: &[
            Cmd(C::StrainMap, Icon::Gradient, "Strain map"),
            Cmd(C::StressMap, Icon::Force, "Stress map"),
            Cmd(C::PressureMap, Icon::Pressure, "Pressure map"),
        ],
        lit_by: &[C::StrainMap, C::StressMap, C::PressureMap],
        lit_views: &[],
    }];

    pub(super) const PATTERN_INFO: &[DisplayMenu] = &[
        DisplayMenu {
            icon: Icon::Tag,
            title: "Pattern information",
            items: &[
                View(ViewToggle::PieceNames2d, Icon::Tag, "Piece names"),
                View(ViewToggle::Lengths2d, Icon::Dimension, "Segment lengths"),
                View(ViewToggle::Measurements2d, Icon::Measure, "2D measurements"),
            ],
            lit_by: &[],
            lit_views: &[ViewToggle::Lengths2d],
        },
        DisplayMenu {
            icon: Icon::Page,
            title: "Pattern board",
            items: &[
                View(ViewToggle::WhiteSheet, Icon::Page, "White sheet"),
                View(ViewToggle::BoardGrid, Icon::Grid, "Grid"),
                View(ViewToggle::Silhouette, Icon::Avatar, "Avatar silhouette"),
            ],
            lit_by: &[],
            lit_views: &[ViewToggle::WhiteSheet],
        },
        DisplayMenu {
            icon: Icon::RulerCorner,
            title: "Rulers and guides",
            items: &[
                View(ViewToggle::Rulers, Icon::RulerCorner, "Rulers"),
                View(ViewToggle::Guides, Icon::Guide, "Guides"),
            ],
            lit_by: &[],
            lit_views: &[ViewToggle::Rulers],
        },
    ];

    pub(super) const BOARD_TOOLS: &[DisplayMenu] = &[
        DisplayMenu {
            icon: Icon::Snap,
            title: "Snap to",
            items: &[
                View(ViewToggle::SnapGrid, Icon::Grid, "Snap to grid"),
                Cmd(C::ToggleSnapToPattern, Icon::Points, "Snap to pattern"),
                View(ViewToggle::SmartGuides, Icon::Align, "Smart guides"),
            ],
            lit_by: &[C::ToggleSnapToPattern],
            lit_views: &[ViewToggle::SnapGrid, ViewToggle::SmartGuides],
        },
        DisplayMenu {
            icon: Icon::Lock,
            title: "Lock in 2D",
            items: &[
                Cmd(
                    C::LockSelectedPieces,
                    Icon::Lock,
                    "Lock selected pieces in 2D",
                ),
                Cmd(C::LockAllPieces, Icon::Lock, "Lock all pieces in 2D"),
                Cmd(C::UnlockAllPieces, Icon::Unlock, "Unlock all pieces in 2D"),
                Sep,
                Cmd(
                    C::LockInternalLines,
                    Icon::Lock,
                    "Lock all internal lines in 2D",
                ),
                Cmd(
                    C::UnlockInternalLines,
                    Icon::Unlock,
                    "Unlock all internal lines in 2D",
                ),
            ],
            lit_by: &[],
            lit_views: &[],
        },
    ];

    pub(super) const ISOLATE: &[DisplayMenu] = &[DisplayMenu {
        icon: Icon::Eye,
        title: "Isolate selection",
        items: &[Cmd(C::IsolateSelection, Icon::Eye, "Isolate selection")],
        lit_by: &[C::IsolateSelection],
        lit_views: &[],
    }];
}

/// The capsules, left to right.
const DISPLAY_GROUPS: [&[DisplayMenu]; 4] = [
    display_groups::RENDER,
    display_groups::SHOW,
    display_groups::MAPS,
    display_groups::ISOLATE,
];

/// The 2D view's capsules, left to right.
const BOARD_DISPLAY_GROUPS: [&[DisplayMenu]; 2] =
    [display_groups::PATTERN_INFO, display_groups::BOARD_TOOLS];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garment_styles_strip_thickness_then_textures_and_weave() {
        use fashioncraft::garment::appearance::{FabricAppearance, Weave, WeaveKind};
        let rich = FabricAppearance {
            color_srgb: [200, 40, 40],
            render_thickness_mm: 2.,
            weave: Some(Weave {
                kind: WeaveKind::Plain,
                spacing_mm: 0.5,
                contrast: 0.3,
            }),
            ..Default::default()
        };
        let look = |style: GarmentStyle| {
            let mut optics = vec![Some(rich.clone()), None];
            style.apply(&mut optics);
            optics
        };
        assert_eq!(look(GarmentStyle::TexturedThick)[0], Some(rich.clone()));
        let thin = look(GarmentStyle::Textured);
        let thin = thin[0].as_ref().unwrap();
        assert_eq!(thin.render_thickness_mm, 0.);
        assert_eq!(thin.weave, rich.weave);
        // Translucent keeps the textured look on a thin surface; the renderer blends it.
        assert_eq!(look(GarmentStyle::Translucent)[0].as_ref(), Some(thin));
        let plain = look(GarmentStyle::Plain);
        let plain = plain[0].as_ref().unwrap();
        assert_eq!(
            (
                plain.render_thickness_mm,
                plain.weave.clone(),
                plain.color_srgb
            ),
            (0., None, rich.color_srgb)
        );
        assert!(plain.texture.is_none());
        // Fabrics without a look of their own stay without one.
        assert!(look(GarmentStyle::Plain)[1].is_none());
        assert_eq!(GarmentDisplay::default().style, GarmentStyle::TexturedThick);
    }
    use std::collections::BTreeMap;

    fn panels() -> Vec<PanelRange> {
        ["front", "back"]
            .iter()
            .enumerate()
            .map(|(i, id)| PanelRange {
                id: id.to_string(),
                vertices: i * 10..(i + 1) * 10,
                triangles: i * 4..(i + 1) * 4,
                material: 0,
                boundaries: BTreeMap::new(),
            })
            .collect()
    }

    #[test]
    fn everything_shown_needs_no_mask() {
        assert_eq!(face_mask(&panels(), 8, true, None), None);
    }

    #[test]
    fn hiding_the_garment_hides_every_triangle() {
        assert_eq!(face_mask(&panels(), 8, false, None), Some(vec![false; 8]));
    }

    #[test]
    fn isolation_keeps_only_the_captured_pieces() {
        let only_back: BTreeSet<String> = ["back".to_string()].into();
        let mask = face_mask(&panels(), 8, true, Some(&only_back)).unwrap();
        assert_eq!(mask, [vec![false; 4], vec![true; 4]].concat());
        // Isolating nothing leaves the view empty, as documented.
        let empty = BTreeSet::new();
        assert_eq!(
            face_mask(&panels(), 8, true, Some(&empty)),
            Some(vec![false; 8])
        );
    }

    #[test]
    fn piece_colours_are_stable_and_distinct_and_frozen_wins() {
        assert_eq!(piece_color(3), piece_color(3));
        assert_ne!(piece_color(0), piece_color(1));
        assert_eq!(face_colors(&panels(), 8, false, None), None);
        let frozen: BTreeSet<String> = ["front".to_string()].into();
        let colors = face_colors(&panels(), 8, true, Some(&frozen)).unwrap();
        assert_eq!(colors[0], Some(FROZEN_TINT));
        assert_eq!(colors[4], Some(piece_color(1)));
        let tint_only = face_colors(&panels(), 8, false, Some(&frozen)).unwrap();
        assert_eq!(
            tint_only[5], None,
            "unfrozen pieces keep their fabric colour"
        );
    }

    #[test]
    fn readout_names_the_most_stretched_direction() {
        let material = fashioncraft::model::Material {
            warp: 400.,
            weft: 100.,
            ..Default::default()
        };
        // Weft stretches more, but warp is four times stiffer: by tension the warp leads.
        let fabric = render::StrainMode::Fabric {
            reference_n_per_m: 20.,
        };
        assert_eq!(
            strain_readout([1.04, 1.1], &material, fabric),
            "Warp · stretch +4.0 % · 80 % of limit"
        );
        assert_eq!(
            strain_readout([1.04, 1.1], &material, render::StrainMode::Global),
            "Weft · stretch +10.0 %"
        );
        let (value, max) = render::fit_value([1.04, 1.1], &material, fabric, 0.2);
        assert!((value - 0.8).abs() < 1e-9 && max == 1.);
        let (value, max) =
            render::fit_value([0.97, 1.0], &material, render::StrainMode::Global, 0.2);
        assert!((value - 0.03).abs() < 1e-9 && max == 0.2);
        // Compression is not tension.
        assert_eq!(render::fibre_tension([0.9, 0.95], &material), [0., 0.]);
    }

    #[test]
    fn srgb_round_trips_and_display_defaults_are_sane() {
        for v in [0u8, 1, 10, 128, 200, 255] {
            assert_eq!(linear_to_srgb(srgb_to_linear([v; 3])[0]), v);
        }
        let mut display = GarmentDisplay {
            strain_max_percent: f64::NAN,
            ..Default::default()
        };
        display.sanitize();
        assert_eq!(display.strain_max_percent, 20.);
        display.strain_max_percent = 1e6;
        display.sanitize();
        assert_eq!(display.strain_max_percent, 200.);
    }
}
