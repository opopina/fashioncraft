//! 3D display choices that never change the scene: avatar and garment visibility, avatar
//! colour, isolating the selected pieces, per-piece colours, state tints and the strain-map
//! range with its legend. Hidden or isolated-away cloth is also left out of 3D picking.
use super::*;
use marvelous_cloth::{garment::template::PanelRange, render};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Strain-map maximum limits, in percent.
pub(super) const STRAIN_MAX_PERCENT: std::ops::RangeInclusive<f64> = 1.0..=200.0;
/// Optional base colour per cloth triangle, linear RGB.
pub(super) type FaceColors = Vec<Option<[f32; 3]>>;
/// Frozen pieces are tinted icy blue (linear RGB).
const FROZEN_TINT: [f32; 3] = [0.42, 0.66, 0.92];

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
    /// One distinct colour per piece instead of the fabric colour.
    pub(super) piece_colors: bool,
    /// Tint frozen pieces so their state is visible at a glance.
    pub(super) state_colors: bool,
    /// Strain that reaches the strain map's last colour, in percent.
    pub(super) strain_max_percent: f64,
    /// Blend the strain map's eight colours instead of showing discrete bands.
    pub(super) strain_gradient: bool,
}

impl Default for GarmentDisplay {
    fn default() -> Self {
        Self {
            avatar: true,
            garment: true,
            avatar_color: render::AVATAR_COLOR.map(linear_to_srgb),
            piece_colors: false,
            state_colors: true,
            strain_max_percent: render::STRAIN_MAX * 100.,
            strain_gradient: true,
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
        self.prefs.garment_display.avatar
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
        (
            self.display_mask(),
            face_colors(
                &self.scene.garment.panels,
                self.scene.garment.mesh.triangles.len(),
                display.piece_colors,
                Some(&frozen),
            ),
        )
    }

    /// Start or stop isolating the current selection (Shift+I).
    pub(super) fn toggle_isolation(&mut self) {
        if self.authoring.view3d.isolated.take().is_some() {
            self.status = "Isolation ended: every piece is shown again".into();
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

    /// Shift+A, Shift+C, Shift+I and Alt+7, unless a text field has the keyboard.
    pub(super) fn garment_display_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (avatar, garment, isolate, strain) = ctx.input(|i| {
            let shift = i.modifiers.shift && !i.modifiers.command && !i.modifiers.alt;
            (
                shift && i.key_pressed(egui::Key::A),
                shift && i.key_pressed(egui::Key::C),
                shift && i.key_pressed(egui::Key::I),
                i.modifiers.alt && !i.modifiers.command && i.key_pressed(egui::Key::Num7),
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
        if isolate {
            self.toggle_isolation();
        }
    }

    /// The "Display" part of the 3D view's settings menu.
    pub(super) fn garment_display_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("Display");
        let display = &mut self.prefs.garment_display;
        ui.horizontal(|ui| {
            ui.checkbox(&mut display.avatar, "Avatar · Shift+A");
            ui.color_edit_button_srgb(&mut display.avatar_color)
                .on_hover_text("Avatar colour");
        });
        ui.checkbox(&mut display.garment, "Garment · Shift+C");
        ui.checkbox(&mut display.piece_colors, "Random piece colours")
            .on_hover_text("One distinct colour per piece instead of the fabric colour");
        ui.checkbox(&mut display.state_colors, "Tint frozen pieces");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.heat, "Strain map · Alt+7");
            ui.add(
                egui::DragValue::new(&mut display.strain_max_percent)
                    .range(STRAIN_MAX_PERCENT)
                    .speed(0.5)
                    .fixed_decimals(1)
                    .prefix("max ")
                    .suffix(" %"),
            )
            .on_hover_text("Strain that reaches the last of the eight colours");
        });
        ui.horizontal(|ui| {
            ui.add_space(22.);
            ui.selectable_value(&mut display.strain_gradient, true, "Gradient");
            ui.selectable_value(&mut display.strain_gradient, false, "Bands");
        });
        let isolating = self.authoring.view3d.isolated.is_some();
        if ui
            .selectable_label(isolating, "Isolate selection · Shift+I")
            .on_hover_text("Show only the pieces selected now; press again to show every piece")
            .clicked()
        {
            self.toggle_isolation();
        }
    }

    /// Strain-map legend in the 3D view's lower-left corner: the colour bar as drawn (gradient
    /// or eight bands) with its 0 %, middle and maximum marks.
    pub(super) fn paint_strain_legend(&self, painter: &egui::Painter, rect: egui::Rect) {
        let display = &self.prefs.garment_display;
        let max = display.strain_max_percent;
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
            "Strain |stretch − 1|",
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
                format!("{:.0} %", max * fraction as f64),
                font.clone(),
                text,
            );
        }
        painter.text(
            origin + egui::vec2(bar.x + 4., bar.y * 0.5),
            egui::Align2::LEFT_CENTER,
            format!("≥ {max:.0} %"),
            font,
            text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
