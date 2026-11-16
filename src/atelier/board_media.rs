//! Board media: a reference image under the 2D board for tracing, and a PNG capture of the
//! board as shown. Presentation only: neither touches the garment, the mesh or the history.
use super::*;
use glam::DVec2;
use marvelous_cloth::fitting::BoardBackground;
use rfd::FileDialog;
use std::path::PathBuf;

/// Largest texture side uploaded for a background; bigger images are scaled down first.
const MAX_TEXTURE_SIDE: u32 = 4096;
/// Frames to wait after the file dialog so menus and popups are gone from the capture.
const CAPTURE_WAIT_FRAMES: u8 = 3;
/// Default real width of a new background image on the board, in metres.
const DEFAULT_WIDTH_M: f64 = 1.0;

/// Decoded background, kept as a texture until the image file changes.
struct BackgroundTexture {
    path: PathBuf,
    result: Result<(egui::TextureHandle, [usize; 2]), String>,
}

/// A board capture waiting for its screenshot.
struct PendingCapture {
    path: PathBuf,
    wait_frames: u8,
    requested: bool,
}

/// Tag carried by our screenshot request, so other screenshot users are left alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoardCaptureTag;

#[derive(Default)]
pub(super) struct BoardMedia {
    texture: Option<BackgroundTexture>,
    capture: Option<PendingCapture>,
    /// Board canvas of the last frame, in points.
    pub(super) last_rect: Option<egui::Rect>,
}

impl BoardMedia {
    /// Whether a capture is in flight; transient overlays are hidden meanwhile.
    pub(super) fn capturing(&self) -> bool {
        self.capture.is_some()
    }
}

/// Corners of the background on the board (top-left, top-right, bottom-right, bottom-left),
/// in metres with v down. The height follows the image's aspect ratio.
pub(super) fn background_corners(background: &BoardBackground, size: [usize; 2]) -> [DVec2; 4] {
    let [x, y] = background.origin_m;
    let w = background.width_m;
    let h = w * size[1] as f64 / size[0].max(1) as f64;
    [
        DVec2::new(x, y),
        DVec2::new(x + w, y),
        DVec2::new(x + w, y + h),
        DVec2::new(x, y + h),
    ]
}

/// A new background: one metre wide, half transparent, its top-left corner at `at`.
pub(super) fn new_background(path: PathBuf, at: DVec2) -> BoardBackground {
    BoardBackground {
        path,
        origin_m: [at.x, at.y],
        width_m: DEFAULT_WIDTH_M,
        opacity: 0.5,
    }
}

/// Scale `[w, h]` down so neither side exceeds `max`, keeping the aspect ratio.
pub(super) fn fit_texture_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w <= max && h <= max {
        return (w, h);
    }
    let scale = f64::from(max) / f64::from(w.max(h));
    (
        ((f64::from(w) * scale).round() as u32).max(1),
        ((f64::from(h) * scale).round() as u32).max(1),
    )
}

/// The part of `rect` (points) inside a screenshot of `image_size` pixels at
/// `pixels_per_point`, or `None` when nothing of it is on screen.
pub(super) fn capture_rect(
    rect: egui::Rect,
    image_size: [usize; 2],
    pixels_per_point: f32,
) -> Option<egui::Rect> {
    let screen = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            image_size[0] as f32 / pixels_per_point,
            image_size[1] as f32 / pixels_per_point,
        ),
    );
    let clipped = rect.intersect(screen);
    (clipped.width() >= 1. && clipped.height() >= 1.).then_some(clipped)
}

fn load_texture(
    ctx: &egui::Context,
    path: &std::path::Path,
) -> Result<(egui::TextureHandle, [usize; 2]), String> {
    let image = image::open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .to_rgba8();
    let (w, h) = fit_texture_size(image.width(), image.height(), MAX_TEXTURE_SIDE);
    let image = if (w, h) == image.dimensions() {
        image
    } else {
        image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle)
    };
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Ok((
        ctx.load_texture("board-background", color, egui::TextureOptions::LINEAR),
        size,
    ))
}

impl Atelier {
    /// Draw the background image under the grid and the pieces, following the board view.
    pub(super) fn paint_background(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        ctx: &egui::Context,
    ) {
        let Some(background) = self.project.board_background.clone() else {
            self.authoring.board.media.texture = None;
            return;
        };
        let media = &mut self.authoring.board.media;
        if media.texture.as_ref().map(|t| &t.path) != Some(&background.path) {
            media.texture = Some(BackgroundTexture {
                path: background.path.clone(),
                result: load_texture(ctx, &background.path),
            });
        }
        let Some(Ok((texture, size))) = media.texture.as_ref().map(|t| &t.result) else {
            return;
        };
        let (texture, size) = (texture.id(), *size);
        let corners = background_corners(&background, size);
        let board = &self.authoring.board;
        let tint = Color32::WHITE.gamma_multiply(background.opacity as f32);
        let mut mesh = egui::Mesh::with_texture(texture);
        for (corner, uv) in corners.iter().zip([
            egui::pos2(0., 0.),
            egui::pos2(1., 0.),
            egui::pos2(1., 1.),
            egui::pos2(0., 1.),
        ]) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: board.screen(rect, *corner),
                uv,
                color: tint,
            });
        }
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        painter.add(egui::Shape::mesh(mesh));
    }

    /// "Background image" and "Save board as PNG" in the board's display menu.
    pub(super) fn media_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("BACKGROUND IMAGE");
        let pick = || {
            FileDialog::new()
                .add_filter("Image", &["png", "tif", "tiff"])
                .pick_file()
        };
        let mut changed = false;
        if let Some(background) = self.project.board_background.as_mut() {
            if let Some(Err(error)) = self
                .authoring
                .board
                .media
                .texture
                .as_ref()
                .map(|t| &t.result)
            {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            let mut width_mm = background.width_m * 1000.;
            let mut x_mm = background.origin_m[0] * 1000.;
            let mut y_mm = background.origin_m[1] * 1000.;
            let mut opacity = background.opacity * 100.;
            egui::Grid::new("board-background").num_columns(2).show(ui, |ui| {
                ui.label("Width");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut width_mm)
                            .range(10.0..=20000.0)
                            .speed(1.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .on_hover_text("Real width the image spans; set it from a known measurement to trace at scale")
                    .changed();
                ui.end_row();
                ui.label("Left");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut x_mm)
                            .speed(1.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .changed();
                ui.end_row();
                ui.label("Top");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut y_mm)
                            .speed(1.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .changed();
                ui.end_row();
                ui.label("Opacity");
                changed |= ui
                    .add(egui::Slider::new(&mut opacity, 0.0..=100.0).suffix(" %"))
                    .changed();
                ui.end_row();
            });
            if changed {
                background.width_m = (width_mm / 1000.).clamp(0.01, 20.);
                background.origin_m = [x_mm / 1000., y_mm / 1000.];
                background.opacity = (opacity / 100.).clamp(0., 1.);
            }
            let mut remove = false;
            ui.horizontal(|ui| {
                if ui.button("Replace…").clicked()
                    && let Some(path) = pick()
                {
                    background.path = path;
                    changed = true;
                }
                remove = ui.button("Remove").clicked();
            });
            if remove {
                self.project.board_background = None;
                changed = true;
            }
        } else if ui
            .button("Choose image…")
            .on_hover_text("Trace over a scanned or photographed pattern (PNG or TIFF)")
            .clicked()
            && let Some(path) = pick()
        {
            let at = self
                .authoring
                .board
                .media
                .last_rect
                .map_or(DVec2::ZERO, |r| {
                    self.authoring.board.world(r, r.min + egui::vec2(24., 24.))
                });
            self.project.board_background = Some(new_background(path, at));
            changed = true;
        }
        if changed {
            self.document.dirty = true;
        }
        ui.separator();
        if ui
            .button("Save board as PNG…")
            .on_hover_text("Save the 2D board exactly as shown, at screen resolution")
            .clicked()
            && let Some(path) = FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name("board.png")
                .save_file()
        {
            self.authoring.board.media.capture = Some(PendingCapture {
                path,
                wait_frames: CAPTURE_WAIT_FRAMES,
                requested: false,
            });
            ui.close();
        }
    }

    /// Run a pending board capture: wait a few frames, request a screenshot, then crop it to
    /// the board and save it. Call once per frame after the board is painted.
    pub(super) fn drive_board_capture(&mut self, ctx: &egui::Context) {
        let Some(capture) = self.authoring.board.media.capture.as_mut() else {
            return;
        };
        if capture.wait_frames > 0 {
            capture.wait_frames -= 1;
            ctx.request_repaint();
            return;
        }
        if !capture.requested {
            capture.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                BoardCaptureTag,
            )));
            ctx.request_repaint();
            return;
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot {
                    user_data, image, ..
                } if user_data
                    .data
                    .as_ref()
                    .is_some_and(|d| d.downcast_ref::<BoardCaptureTag>().is_some()) =>
                {
                    Some(image.clone())
                }
                _ => None,
            })
        });
        let Some(image) = shot else {
            ctx.request_repaint();
            return;
        };
        let Some(capture) = self.authoring.board.media.capture.take() else {
            return;
        };
        let pixels_per_point = ctx.pixels_per_point();
        let Some(rect) = self
            .authoring
            .board
            .media
            .last_rect
            .and_then(|r| capture_rect(r, image.size, pixels_per_point))
        else {
            self.status = "Board capture failed: the board is not on screen".into();
            return;
        };
        let board = image.region(&rect, Some(pixels_per_point));
        let pixels: Vec<u8> = board.pixels.iter().flat_map(|p| p.to_array()).collect();
        self.status = match image::save_buffer(
            &capture.path,
            &pixels,
            board.size[0] as u32,
            board.size[1] as u32,
            image::ColorType::Rgba8,
        ) {
            Ok(()) => format!(
                "Board saved as {} ({} × {} px)",
                capture.path.display(),
                board.size[0],
                board.size[1]
            ),
            Err(error) => format!("Board capture could not be saved: {error}"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_corners_follow_width_and_aspect() {
        let background = new_background(PathBuf::from("scan.png"), DVec2::new(0.1, -0.2));
        let corners = background_corners(&background, [2000, 1000]);
        assert_eq!(corners[0], DVec2::new(0.1, -0.2));
        assert_eq!(corners[1], DVec2::new(1.1, -0.2));
        assert_eq!(corners[2], DVec2::new(1.1, 0.3));
        assert_eq!(corners[3], DVec2::new(0.1, 0.3));
        assert!(background.validate().is_ok());
    }

    #[test]
    fn oversized_images_are_scaled_down_keeping_their_aspect() {
        assert_eq!(fit_texture_size(800, 600, 4096), (800, 600));
        assert_eq!(fit_texture_size(8192, 4096, 4096), (4096, 2048));
        assert_eq!(fit_texture_size(1000, 9000, 4096), (455, 4096));
        assert_eq!(fit_texture_size(10_000, 1, 4096), (4096, 1));
    }

    #[test]
    fn captures_are_clipped_to_the_screenshot() {
        let rect = egui::Rect::from_min_max(egui::pos2(100., 50.), egui::pos2(900., 700.));
        // A 1600 × 1000 px screenshot at 2 px/pt covers 800 × 500 pt.
        let clipped = capture_rect(rect, [1600, 1000], 2.).unwrap();
        assert_eq!(
            clipped,
            egui::Rect::from_min_max(egui::pos2(100., 50.), egui::pos2(800., 500.))
        );
        let off = egui::Rect::from_min_max(egui::pos2(900., 600.), egui::pos2(950., 650.));
        assert_eq!(capture_rect(off, [1600, 1000], 2.), None);
    }
}
