//! Colours for fabrics: the project palette (saved swatches) and the eyedropper, which takes
//! the colour a piece shows at a point of the 3D view, its texture included.
use super::*;
use marvelous_cloth::{garment::appearance::FabricAppearance, picking};

/// Side of a palette swatch, points.
const SWATCH: f32 = 18.;
/// Most colours a palette keeps.
const MAX_SWATCHES: usize = 48;

/// A row of swatches: a click takes the colour, a right click removes the swatch, "+" saves
/// the current colour.
pub(super) fn palette_row(ui: &mut egui::Ui, palette: &mut Vec<[u8; 3]>, color: &mut [u8; 3]) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Palette");
        let mut remove = None;
        for (k, swatch) in palette.iter().enumerate() {
            let [r, g, b] = *swatch;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 3., egui::Color32::from_rgb(r, g, b));
            if swatch == color {
                ui.painter().rect_stroke(
                    rect,
                    3.,
                    egui::Stroke::new(2., ui.visuals().selection.bg_fill),
                    egui::StrokeKind::Outside,
                );
            }
            let response = response.on_hover_text(format!(
                "#{r:02X}{g:02X}{b:02X} · click to use, right click to remove"
            ));
            if response.clicked() {
                *color = *swatch;
            }
            if response.secondary_clicked() {
                remove = Some(k);
            }
        }
        if let Some(k) = remove {
            palette.remove(k);
        }
        if ui
            .add_enabled(
                palette.len() < MAX_SWATCHES && !palette.contains(color),
                egui::Button::new("+").small(),
            )
            .on_hover_text("Save the current colour in the project palette")
            .clicked()
        {
            palette.push(*color);
        }
    });
}

fn linear(v: u8) -> f32 {
    let s = f32::from(v) / 255.;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(v: f32) -> u8 {
    let v = v.clamp(0., 1.);
    let s = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (s * 255.).round() as u8
}

/// The colour a face shows: its face colour, times the texture's texel when there is one.
pub(super) fn shown_color(face: [u8; 3], texel: Option<[u8; 3]>) -> [u8; 3] {
    match texel {
        None => face,
        Some(t) => [0, 1, 2].map(|k| srgb(linear(face[k]) * linear(t[k]))),
    }
}

impl Atelier {
    /// The colour of the cloth at `pos` in the 3D view and the fabric it belongs to.
    fn colour_at(
        &mut self,
        ctx: &egui::Context,
        pos: egui::Pos2,
        rect: egui::Rect,
    ) -> Option<[u8; 3]> {
        let (origin, dir) = self.camera.ray(pos, rect);
        let x = &self.scene.state.x;
        let mut best = (f64::INFINITY, None);
        for (face, t) in self.scene.garment.mesh.triangles.iter().enumerate() {
            if !self.cloth_face_shown(face) {
                continue;
            }
            if let Some((d, w)) = picking::ray_triangle(origin, dir, t.vertices.map(|i| x[i]))
                && d < best.0
            {
                best = (d, Some((face, w)));
            }
        }
        let (distance, Some((face, weights))) = best else {
            return None;
        };
        if self.avatar_shown()
            && self
                .scene
                .collider
                .occludes_ray(origin, dir, distance - 1e-5)
        {
            return None;
        }
        let garment = &self.scene.garment;
        let t = &garment.mesh.triangles[face];
        let panel = garment
            .panels
            .iter()
            .find(|p| p.triangles.contains(&face))?;
        let template = &self.project.template;
        let fabric = &template.panels.iter().find(|p| p.id == panel.id)?.fabric;
        let appearance = template
            .fabric_appearance
            .get(fabric)
            .cloned()
            .unwrap_or_else(FabricAppearance::default);
        // The side the ray meets: the front faces away from the winding normal (towards it
        // on a flipped piece).
        let [a, b, c] = t.vertices.map(|i| x[i]);
        let winding = (b - a).cross(c - a);
        let flipped = template.flipped_panels.contains(&panel.id);
        let front = (winding.dot(dir) > 0.) != flipped;
        let face_color = if front {
            appearance.color_srgb
        } else {
            appearance.back_color_srgb.unwrap_or(appearance.color_srgb)
        };
        let grain = garment.materials[t.material].grain_degrees;
        let uv: glam::DVec2 = t
            .vertices
            .iter()
            .zip(weights)
            .map(|(&v, w)| garment.mesh.uv[v] * w)
            .sum();
        let texel = appearance.texture.clone().and_then(|texture| {
            let image = self.image_of(ctx, &texture.asset_id)?;
            let (sin, cos) = grain.to_radians().sin_cos();
            let warp_weft = glam::DVec2::new(uv.x * cos + uv.y * sin, -uv.x * sin + uv.y * cos);
            let p = texture.image_uv(warp_weft);
            let [w, h] = image.size;
            let px = ((p.x.rem_euclid(1.) * f64::from(w)) as u32).min(w - 1);
            let py = ((p.y.rem_euclid(1.) * f64::from(h)) as u32).min(h - 1);
            let i = ((py * w + px) * 4) as usize;
            Some([image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]])
        });
        Some(shown_color(face_color, texel))
    }

    /// While the eyedropper is on, a click on the cloth puts its colour in the fabric draft.
    pub(super) fn eyedropper_interaction(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        rect: egui::Rect,
    ) {
        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.authoring.eyedropper = false;
            self.status = "Eyedropper cancelled".into();
            return;
        }
        if !response.clicked() {
            return;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        let Some(color) = self.colour_at(ctx, pos, rect) else {
            self.status = "Eyedropper: no cloth there".into();
            return;
        };
        let [r, g, b] = color;
        if let Some(appearance) = self
            .authoring
            .fabric_draft
            .as_mut()
            .map(|d| d.appearance.get_or_insert_with(FabricAppearance::default))
        {
            appearance.color_srgb = color;
            self.status = format!("Picked #{r:02X}{g:02X}{b:02X} · Apply appearance to use it");
        }
        self.authoring.eyedropper = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_tints_the_face_colour_in_linear_light() {
        assert_eq!(shown_color([200, 100, 50], None), [200, 100, 50]);
        // White face: the texel as it is; white texel: the face colour as it is.
        assert_eq!(shown_color([255; 3], Some([10, 128, 240])), [10, 128, 240]);
        assert_eq!(shown_color([10, 128, 240], Some([255; 3])), [10, 128, 240]);
        // Mid grey on mid grey multiplies in linear light: 0.216² is about 0.047, byte 61.
        let [v, ..] = shown_color([128; 3], Some([128; 3]));
        assert_eq!(v, 61);
    }
}
