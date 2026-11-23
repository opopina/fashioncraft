//! Graphics on a piece: PNG prints and logos placed in the piece's material space, shown on
//! the 2D board and on the 3D cloth. Only the look changes; each edit is one command. The
//! image files themselves live in the project's asset table.
use super::layers_ui::{Field, committed_value};
use super::*;
use fashioncraft::{
    authoring::Command,
    fitting::ImageAsset,
    garment::graphics::{GraphicPlacement, GraphicSide, GraphicTriangle, clip_to_image},
    render::GraphicImage,
};
use glam::DVec2;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Largest side of a graphic's texture; bigger images are scaled down on load.
const MAX_GRAPHIC_SIDE: u32 = 2048;
/// Width of a new graphic, in metres; its height follows the image's aspect ratio.
const NEW_GRAPHIC_WIDTH_M: f64 = 0.12;

/// A decoded image and its 2D board texture.
pub(super) struct LoadedGraphic {
    pub image: Arc<GraphicImage>,
    texture: egui::TextureHandle,
}

struct CachedGraphic {
    path: PathBuf,
    result: Result<LoadedGraphic, String>,
}

/// Decoded graphic images by asset ID, reloaded when an asset's file changes.
#[derive(Default)]
pub(super) struct GraphicImages {
    cache: HashMap<String, CachedGraphic>,
}

/// Each load gets a fresh key, so the 3D viewport never reuses stale pixels.
fn next_key() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn load(ctx: &egui::Context, path: &Path) -> Result<LoadedGraphic, String> {
    let image = image::open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .to_rgba8();
    let (w, h) =
        super::board_media::fit_texture_size(image.width(), image.height(), MAX_GRAPHIC_SIDE);
    let image = if (w, h) == image.dimensions() {
        image
    } else {
        image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle)
    };
    let size = [image.width() as usize, image.height() as usize];
    let texture = ctx.load_texture(
        format!("graphic-{}", path.display()),
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    Ok(LoadedGraphic {
        image: Arc::new(GraphicImage {
            key: next_key(),
            size: [image.width(), image.height()],
            rgba: image.into_raw(),
        }),
        texture,
    })
}

/// The cut-out of `placement` on its piece, on the 2D board: clipped triangles carrying their
/// image coordinates, in board screen points.
fn board_triangles(
    placement: &GraphicPlacement,
    garment: &fashioncraft::garment::CompiledGarment,
    point: &dyn Fn(DVec2) -> egui::Pos2,
) -> Vec<[(egui::Pos2, egui::Pos2); 3]> {
    let Ok(triangles) = placement.triangle_uvs(garment) else {
        return vec![];
    };
    let mut out = vec![];
    for t in triangles {
        let rest = t.vertices.map(|v| garment.mesh.uv[v]);
        for weights in clip_to_image(t.image_uv) {
            out.push(weights.map(|w| {
                let p = rest[0] * w.x + rest[1] * w.y + rest[2] * w.z;
                let uv = t.image_uv[0] * w.x + t.image_uv[1] * w.y + t.image_uv[2] * w.z;
                (point(p), egui::pos2(uv.x as f32, uv.y as f32))
            }));
        }
    }
    out
}

/// Paint the graphics of one piece on the 2D board, over its fill.
pub(super) fn paint_board_graphics(
    painter: &egui::Painter,
    placements: &[&GraphicPlacement],
    textures: &HashMap<String, egui::TextureId>,
    garment: &fashioncraft::garment::CompiledGarment,
    point: &dyn Fn(DVec2) -> egui::Pos2,
) {
    for placement in placements {
        let Some(&texture) = textures.get(&placement.asset_id) else {
            continue;
        };
        let mut mesh = egui::Mesh::with_texture(texture);
        // A back print shows through the cloth on the board, faintly.
        let alpha = match placement.side {
            GraphicSide::Front => 255.,
            GraphicSide::Back => 90.,
        } * placement.opacity.clamp(0., 1.);
        let tint = egui::Color32::from_white_alpha(alpha.round() as u8);
        for triangle in board_triangles(placement, garment, point) {
            let base = mesh.vertices.len() as u32;
            for (pos, uv) in triangle {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv,
                    color: tint,
                });
            }
            mesh.add_triangle(base, base + 1, base + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Screen size of the corner and turn handles, points.
const HANDLE_PX: f32 = 7.;
/// How far the turn handle stands off the graphic's top edge, points.
const TURN_OFFSET_PX: f32 = 22.;

/// Which part of a graphic a drag holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Body,
    Corner,
    Turn,
}

struct GraphicDrag {
    original: GraphicPlacement,
    handle: Handle,
    /// Where the press landed on the piece, metres.
    start_m: DVec2,
    preview: GraphicPlacement,
}

/// The Transform graphic tool's selection and drag in progress.
#[derive(Default)]
pub(super) struct GraphicTool {
    selected: Option<String>,
    drag: Option<GraphicDrag>,
}

/// Screen corners of a graphic and its turn handle, given the piece-to-screen map.
fn handle_points(
    placement: &GraphicPlacement,
    point: &dyn Fn(DVec2) -> egui::Pos2,
) -> Option<([egui::Pos2; 4], egui::Pos2, egui::Pos2)> {
    let corners = placement.corners()?.map(point);
    let centre = point(placement.center_m);
    let top = corners[0] + (corners[1] - corners[0]) * 0.5;
    let out = (top - centre).normalized();
    Some((corners, top, top + out * TURN_OFFSET_PX))
}

impl Atelier {
    /// The piece-to-screen map of the board for piece `panel`, and its inverse.
    fn piece_maps(
        &self,
        panel: &str,
        rect: egui::Rect,
    ) -> Option<(
        impl Fn(DVec2) -> egui::Pos2 + '_,
        impl Fn(egui::Pos2) -> DVec2 + '_,
    )> {
        let i = self
            .project
            .template
            .panels
            .iter()
            .position(|p| p.id == panel)?;
        let (origin, angle) = (self.board_origin(i), self.board_angle(i));
        let board = &self.authoring.board;
        Some((
            move |uv: DVec2| {
                board.screen(rect, origin + fashioncraft::authoring::rotate_2d(uv, angle))
            },
            move |pos: egui::Pos2| {
                fashioncraft::authoring::rotate_2d(board.world(rect, pos) - origin, -angle)
            },
        ))
    }

    /// The graphics of a piece as the board shows them, with a drag's preview in place.
    pub(super) fn shown_graphics(&self, panel: &str) -> Vec<GraphicPlacement> {
        let preview = self
            .authoring
            .graphic_tool
            .drag
            .as_ref()
            .map(|d| &d.preview);
        self.project
            .template
            .graphics
            .iter()
            .filter(|g| g.panel_id == panel)
            .map(|g| match preview {
                Some(p) if p.id == g.id => p.clone(),
                _ => g.clone(),
            })
            .collect()
    }

    /// The graphic and handle under `pos`: the selected graphic's handles first, then the
    /// topmost graphic whose image covers the point.
    fn graphic_hit(&self, rect: egui::Rect, pos: egui::Pos2) -> Option<(String, Handle)> {
        let graphics = &self.project.template.graphics;
        if let Some(selected) = &self.authoring.graphic_tool.selected
            && let Some(g) = graphics.iter().find(|g| &g.id == selected)
            && let Some((to_screen, _)) = self.piece_maps(&g.panel_id, rect)
            && let Some((corners, _, turn)) = handle_points(g, &to_screen)
        {
            let near = |p: egui::Pos2| p.distance(pos) <= HANDLE_PX + 3.;
            if near(turn) {
                return Some((g.id.clone(), Handle::Turn));
            }
            if corners.into_iter().any(near) {
                return Some((g.id.clone(), Handle::Corner));
            }
        }
        graphics.iter().rev().find_map(|g| {
            let (_, to_piece) = self.piece_maps(&g.panel_id, rect)?;
            g.contains(to_piece(pos))
                .then(|| (g.id.clone(), Handle::Body))
        })
    }

    /// Press, drag and release of the Transform graphic tool on the 2D board. A release
    /// commits the change as one command; Escape cancels a drag; Delete removes the selection.
    pub(super) fn graphic_tool_events(&mut self, response: &egui::Response, rect: egui::Rect) {
        let ctx = response.ctx.clone();
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(start) = ctx.input(|i| i.pointer.press_origin())
        {
            let hit = self.graphic_hit(rect, start);
            self.authoring.graphic_tool.selected = hit.as_ref().map(|(id, _)| id.clone());
            self.authoring.graphic_tool.drag = hit.and_then(|(id, handle)| {
                let g = self
                    .project
                    .template
                    .graphics
                    .iter()
                    .find(|g| g.id == id)?
                    .clone();
                let (_, to_piece) = self.piece_maps(&g.panel_id, rect)?;
                let start_m = to_piece(start);
                Some(GraphicDrag {
                    preview: g.clone(),
                    original: g,
                    handle,
                    start_m,
                })
            });
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(drag) = self.authoring.graphic_tool.drag.as_ref()
        {
            let snap = ctx.input(|i| i.modifiers.shift).then_some(15.);
            let preview = self
                .piece_maps(&drag.original.panel_id, rect)
                .map(|(_, to_piece)| {
                    let now = to_piece(pos);
                    match drag.handle {
                        Handle::Body => drag.original.moved(now - drag.start_m),
                        Handle::Corner => drag.original.scaled_towards(drag.start_m, now),
                        Handle::Turn => drag.original.turned_towards(drag.start_m, now, snap),
                    }
                });
            if let (Some(preview), Some(drag)) =
                (preview, self.authoring.graphic_tool.drag.as_mut())
                && preview.validate().is_ok()
            {
                drag.preview = preview;
            }
        }
        if response.drag_stopped()
            && let Some(drag) = self.authoring.graphic_tool.drag.take()
            && drag.preview != drag.original
        {
            self.edit(Command::SetGraphic {
                placement: drag.preview,
            });
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.authoring.graphic_tool.selected = self.graphic_hit(rect, pos).map(|(id, _)| id);
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.authoring.graphic_tool.drag = None;
        }
        if self.authoring.graphic_tool.drag.is_none()
            && ctx
                .input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && let Some(id) = self.authoring.graphic_tool.selected.take()
            && self.project.template.graphics.iter().any(|g| g.id == id)
        {
            self.edit(Command::RemoveGraphic { id });
        }
    }

    /// Outline, corner handles and turn handle of the selected graphic on the board.
    pub(super) fn paint_graphic_handles(&self, ui: &egui::Ui, rect: egui::Rect) {
        let Some(id) = &self.authoring.graphic_tool.selected else {
            return;
        };
        let Some(g) = self.authoring.graphic_tool.drag.as_ref().map_or_else(
            || {
                self.project
                    .template
                    .graphics
                    .iter()
                    .find(|g| &g.id == id)
                    .cloned()
            },
            |d| Some(d.preview.clone()),
        ) else {
            return;
        };
        let Some((to_screen, _)) = self.piece_maps(&g.panel_id, rect) else {
            return;
        };
        let Some((corners, top, turn)) = handle_points(&g, &to_screen) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        let color = super::workspace::accent(ui.visuals());
        let stroke = egui::Stroke::new(1.5, color);
        painter.add(egui::Shape::closed_line(corners.to_vec(), stroke));
        for c in corners {
            painter.rect(
                egui::Rect::from_center_size(c, egui::vec2(HANDLE_PX, HANDLE_PX)),
                1.,
                egui::Color32::WHITE,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        painter.line_segment([top, turn], stroke);
        painter.circle(turn, HANDLE_PX * 0.6, egui::Color32::WHITE, stroke);
    }

    /// The cached image of an asset, loading it on first use or after its file changed.
    pub(super) fn graphic(
        &mut self,
        ctx: &egui::Context,
        asset: &str,
    ) -> Option<&Result<LoadedGraphic, String>> {
        let path = self
            .project
            .image_assets
            .iter()
            .find(|a| a.id == asset)?
            .path
            .clone();
        let cache = &mut self.authoring.graphic_images.cache;
        if cache.get(asset).is_none_or(|c| c.path != path) {
            let result = load(ctx, &path);
            cache.insert(asset.into(), CachedGraphic { path, result });
        }
        cache.get(asset).map(|c| &c.result)
    }

    /// Board textures of every image a graphic uses; images that fail to load are left out.
    pub(super) fn graphic_textures(
        &mut self,
        ctx: &egui::Context,
    ) -> HashMap<String, egui::TextureId> {
        let assets: Vec<String> = self
            .project
            .template
            .graphics
            .iter()
            .map(|g| g.asset_id.clone())
            .collect();
        let mut textures = HashMap::new();
        for asset in assets {
            if let Some(Ok(loaded)) = self.graphic(ctx, &asset) {
                textures.insert(asset, loaded.texture.id());
            }
        }
        textures
    }

    /// Cloth positions moved onto one face of the fabric: half its look thickness towards the
    /// front (or the back). `None` when no fabric looks thick, so the simulated surface is it.
    pub(super) fn face_positions(&self, front: bool) -> Option<Vec<glam::DVec3>> {
        let template = &self.project.template;
        let garment = &self.scene.garment;
        let half = |panel: &str| {
            let fabric = &template.panels.iter().find(|p| p.id == panel)?.fabric;
            let mm = template.fabric_appearance.get(fabric)?.render_thickness_mm;
            (mm > 0.).then_some(mm / 2000.)
        };
        if !garment.panels.iter().any(|p| half(&p.id).is_some()) {
            return None;
        }
        let x = &self.scene.state.x;
        let mut normals = vec![glam::DVec3::ZERO; x.len()];
        for t in &garment.mesh.triangles {
            let [a, b, c] = t.vertices.map(|v| x[v]);
            let n = (b - a).cross(c - a);
            for v in t.vertices {
                normals[v] += n;
            }
        }
        let mut moved = x.clone();
        for panel in &garment.panels {
            let Some(half) = half(&panel.id) else {
                continue;
            };
            // The front faces away from the winding normal, towards it on a flipped piece.
            let flipped = template.flipped_panels.contains(&panel.id);
            let sign = if flipped == front { 1. } else { -1. };
            for v in panel.vertices.clone() {
                moved[v] += normals[v].normalize_or_zero() * (sign * half);
            }
        }
        Some(moved)
    }

    /// Images and lifted cloth triangles of the graphics on the pieces on show.
    pub(super) fn graphic_draws(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<(Arc<GraphicImage>, Vec<GraphicTriangle>, f32)> {
        // In a size other than the base, graphics follow their piece into it.
        let placements: Vec<GraphicPlacement> =
            fashioncraft::garment::grading::apply::graphics_shown(&self.project.template)
                .into_iter()
                .filter(|g| self.panel_shown(&g.panel_id))
                .collect();
        let front = self.face_positions(true);
        let back = self.face_positions(false);
        let mut draws = vec![];
        for placement in placements {
            let Some(Ok(loaded)) = self.graphic(ctx, &placement.asset_id) else {
                continue;
            };
            let image = loaded.image.clone();
            let flipped = self
                .project
                .template
                .flipped_panels
                .contains(&placement.panel_id);
            let x = match placement.side {
                GraphicSide::Front => front.as_ref(),
                GraphicSide::Back => back.as_ref(),
            }
            .unwrap_or(&self.scene.state.x);
            if let Ok(triangles) = placement.graphic_triangles(&self.scene.garment, x, flipped) {
                draws.push((image, triangles, placement.opacity as f32));
            }
        }
        draws
    }

    /// The decoded image of an asset, when it loads.
    pub(super) fn image_of(
        &mut self,
        ctx: &egui::Context,
        asset: &str,
    ) -> Option<Arc<GraphicImage>> {
        match self.graphic(ctx, asset)? {
            Ok(loaded) => Some(loaded.image.clone()),
            Err(_) => None,
        }
    }

    /// The images of each material's fabric texture, by material index: its colours, then its
    /// maps in `MapSlot::ALL` order.
    pub(super) fn fabric_images(
        &mut self,
        ctx: &egui::Context,
        optics: &[Option<fashioncraft::garment::appearance::FabricAppearance>],
    ) -> Vec<[Option<Arc<GraphicImage>>; 7]> {
        use fashioncraft::garment::appearance::MapSlot;
        optics
            .iter()
            .map(|appearance| {
                let Some(texture) = appearance.as_ref().and_then(|a| a.texture.as_ref()) else {
                    return Default::default();
                };
                let mut load = |asset: Option<&str>| -> Option<Arc<GraphicImage>> {
                    match self.graphic(ctx, asset?)? {
                        Ok(loaded) => Some(loaded.image.clone()),
                        Err(_) => None,
                    }
                };
                let mut images: [Option<Arc<GraphicImage>>; 7] = Default::default();
                images[0] = load(Some(&texture.asset_id));
                for slot in MapSlot::ALL {
                    images[fashioncraft::render::image_index(slot)] = load(texture.map(slot));
                }
                images
            })
            .collect()
    }

    fn new_graphic_id(&self) -> String {
        let used = |id: &str| self.project.template.graphics.iter().any(|g| g.id == id);
        (1..)
            .map(|k| format!("graphic_{k}"))
            .find(|id| !used(id))
            .unwrap()
    }

    /// The asset of `path`, adding it to the project's table when it is new.
    pub(super) fn asset_for(&mut self, path: PathBuf) -> String {
        if let Some(asset) = self.project.image_assets.iter().find(|a| a.path == path) {
            return asset.id.clone();
        }
        let used = |id: &str| self.project.image_assets.iter().any(|a| a.id == id);
        let id = (1..)
            .map(|k| format!("image_{k}"))
            .find(|id| !used(id))
            .unwrap();
        self.project.image_assets.push(ImageAsset {
            id: id.clone(),
            path,
        });
        id
    }

    /// Place a PNG at the middle of a piece, 12 cm wide.
    pub(super) fn add_graphic(&mut self, panel: &str) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .pick_file()
        else {
            return;
        };
        self.add_graphic_from(panel, path);
    }

    /// Place the PNG at `path` at the middle of a piece, 12 cm wide.
    pub(super) fn add_graphic_from(&mut self, panel: &str, path: PathBuf) {
        self.add_graphic_at(panel, path, None, GraphicSide::Front);
    }

    /// Graphic on the 3D garment: choose a PNG, then click the cloth where it goes.
    pub(super) fn start_graphic_3d(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .pick_file()
        else {
            return;
        };
        self.authoring.graphic_3d = Some(path);
        self.status = i18n::tr(
            self.prefs.language,
            "Click the garment where the graphic goes · Esc cancels",
        )
        .into();
    }

    /// While a graphic waits for its place, a click on the cloth puts it there, on the side of
    /// the cloth that faces the view. Takes every 3D click until then.
    pub(super) fn graphic_3d_interaction(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        if self.authoring.graphic_3d.is_none() {
            return false;
        }
        let ui_lang = self.prefs.language;
        if response.ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.authoring.graphic_3d = None;
            self.status = i18n::tr(ui_lang, "Graphic cancelled").into();
            return true;
        }
        if !response.clicked_by(egui::PointerButton::Primary) {
            return true;
        }
        let Some(pos) = response.interact_pointer_pos() else {
            return true;
        };
        let Some(v) = self.picked_vertex(pos, rect) else {
            self.status = i18n::tr(ui_lang, "Click on the cloth").into();
            return true;
        };
        let g = self.scene.garment.clone();
        let Some(range) = g.panels.iter().find(|r| r.vertices.contains(&v)) else {
            return true;
        };
        let x = &self.scene.state.x;
        let normal: glam::DVec3 = g.mesh.triangles[range.triangles.clone()]
            .iter()
            .filter(|t| t.vertices.contains(&v))
            .map(|t| {
                let [a, b, c] = t.vertices.map(|i| x[i]);
                (b - a).cross(c - a)
            })
            .sum();
        let facing = normal.dot(self.camera.eye().as_dvec3() - x[v]) > 0.;
        let flipped = self.project.template.flipped_panels.contains(&range.id);
        let side = if facing != flipped {
            GraphicSide::Front
        } else {
            GraphicSide::Back
        };
        let (panel, uv) = (range.id.clone(), g.mesh.uv[v]);
        let path = self.authoring.graphic_3d.take().expect("checked above");
        self.add_graphic_at(&panel, path, Some(uv), side);
        true
    }

    /// Place the PNG at `path` on a piece, 12 cm wide, centred at `centre` (material metres) or
    /// at the middle of the piece, on `side` of the cloth.
    pub(super) fn add_graphic_at(
        &mut self,
        panel: &str,
        path: PathBuf,
        centre: Option<DVec2>,
        side: GraphicSide,
    ) {
        let (w, h) = match image::image_dimensions(&path) {
            Ok(size) => size,
            Err(e) => {
                self.status = format!("Cannot open {}: {e}", path.display());
                return;
            }
        };
        let Some(centre) = centre.or_else(|| self.piece_centre(panel)) else {
            return;
        };
        let asset_id = self.asset_for(path);
        let width = NEW_GRAPHIC_WIDTH_M;
        let height = width * f64::from(h) / f64::from(w.max(1));
        self.edit(Command::SetGraphic {
            placement: GraphicPlacement {
                id: self.new_graphic_id(),
                panel_id: panel.into(),
                asset_id,
                center_m: centre,
                size_m: DVec2::new(width, height),
                rotation_degrees: 0.,
                side,
                opacity: 1.,
            },
        });
    }

    /// "Graphics" in the Piece inspector: add a PNG, then size, turn, move, flip to the back
    /// or remove each graphic.
    pub(super) fn piece_graphics(&mut self, ui: &mut egui::Ui, panel: &str) {
        let ui_lang = self.prefs.language;
        let mine: Vec<GraphicPlacement> = self
            .project
            .template
            .graphics
            .iter()
            .filter(|g| g.panel_id == panel)
            .cloned()
            .collect();
        let names: HashMap<String, String> = self
            .project
            .image_assets
            .iter()
            .map(|a| {
                let name = a
                    .path
                    .file_stem()
                    .map_or_else(|| a.id.clone(), |s| s.to_string_lossy().into_owned());
                (a.id.clone(), name)
            })
            .collect();
        let errors: HashMap<String, String> = mine
            .iter()
            .filter_map(|g| {
                let entry = self.authoring.graphic_images.cache.get(&g.asset_id)?;
                entry
                    .result
                    .as_ref()
                    .err()
                    .map(|e| (g.asset_id.clone(), e.clone()))
            })
            .collect();
        let mut command = None;
        let mut add = false;
        egui::CollapsingHeader::new(super::workspace::section(format!(
            "{} ({})",
            i18n::tr(ui_lang, "Graphics"),
            mine.len()
        )))
        .id_salt("piece_graphics")
        .show(ui, |ui| {
            add = ui
                .button(i18n::tr(ui_lang, "Add graphic…"))
                .on_hover_text(i18n::tr(
                    ui_lang,
                    "Print a PNG image on this piece, 12 cm wide at its middle",
                ))
                .clicked();
            for g in &mine {
                ui.separator();
                ui.horizontal(|ui| {
                    let name = names.get(&g.asset_id).cloned().unwrap_or_default();
                    ui.label(format!("{name} · {}", g.id));
                    if ui.small_button(i18n::tr(ui_lang, "Remove")).clicked() {
                        command = Some(Command::RemoveGraphic { id: g.id.clone() });
                    }
                });
                if let Some(error) = errors.get(&g.asset_id) {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let mut edited = g.clone();
                let mm = |label, hover, range| Field {
                    label,
                    hover,
                    range,
                    speed: 1.,
                    decimals: 1,
                    suffix: " mm",
                    reset: None,
                };
                ui.horizontal(|ui| {
                    if let Some(w) = committed_value(
                        ui,
                        ("graphic_w", &g.id),
                        g.size_m.x * 1000.,
                        mm("W", "Width of the image on the piece", 1.0..=2000.),
                    ) {
                        // Width and height keep the image's proportions.
                        let ratio = g.size_m.y / g.size_m.x;
                        edited.size_m = DVec2::new(w, w * ratio) / 1000.;
                    }
                    if let Some(h) = committed_value(
                        ui,
                        ("graphic_h", &g.id),
                        g.size_m.y * 1000.,
                        mm("H", "Height of the image on the piece", 1.0..=2000.),
                    ) {
                        let ratio = g.size_m.x / g.size_m.y;
                        edited.size_m = DVec2::new(h * ratio, h) / 1000.;
                    }
                });
                ui.horizontal(|ui| {
                    if let Some(x) = committed_value(
                        ui,
                        ("graphic_x", &g.id),
                        g.center_m.x * 1000.,
                        mm("X", "Centre across the piece", -5000.0..=5000.),
                    ) {
                        edited.center_m.x = x / 1000.;
                    }
                    if let Some(y) = committed_value(
                        ui,
                        ("graphic_y", &g.id),
                        g.center_m.y * 1000.,
                        mm("Y", "Centre along the piece", -5000.0..=5000.),
                    ) {
                        edited.center_m.y = y / 1000.;
                    }
                });
                ui.horizontal(|ui| {
                    if let Some(angle) = committed_value(
                        ui,
                        ("graphic_angle", &g.id),
                        g.rotation_degrees,
                        Field {
                            label: i18n::tr(ui_lang, "Angle"),
                            hover: i18n::tr(ui_lang, "Turn the image on the piece"),
                            range: -360.0..=360.,
                            speed: 0.5,
                            decimals: 1,
                            suffix: "°",
                            reset: Some(0.),
                        },
                    ) {
                        edited.rotation_degrees = angle;
                    }
                    let back = g.side == GraphicSide::Back;
                    if ui
                        .selectable_label(back, i18n::tr(ui_lang, "Back"))
                        .on_hover_text(i18n::tr(
                            ui_lang,
                            "Print on the back face of the fabric instead",
                        ))
                        .clicked()
                    {
                        edited.side = if back {
                            GraphicSide::Front
                        } else {
                            GraphicSide::Back
                        };
                    }
                });
                if let Some(percent) = committed_value(
                    ui,
                    ("graphic_opacity", &g.id),
                    g.opacity * 100.,
                    Field {
                        label: i18n::tr(ui_lang, "Opacity"),
                        hover: i18n::tr(
                            ui_lang,
                            "How solid the print looks; the image's own transparency adds to it",
                        ),
                        range: 0.0..=100.,
                        speed: 0.5,
                        decimals: 0,
                        suffix: " %",
                        reset: Some(100.),
                    },
                ) {
                    edited.opacity = percent / 100.;
                }
                if edited != *g && edited.validate().is_ok() {
                    command = Some(Command::SetGraphic { placement: edited });
                }
            }
        });
        if add {
            self.add_graphic(panel);
        }
        if let Some(command) = command {
            self.edit(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_cut_out_stays_inside_the_image_and_the_piece() {
        let template = fashioncraft::garment::GarmentTemplate::two_panel_demo();
        let garment = template.compile().unwrap();
        let panel = &garment.panels[0];
        let uv = &garment.mesh.uv[panel.vertices.clone()];
        let centre = uv.iter().sum::<DVec2>() / uv.len() as f64;
        let placement = GraphicPlacement {
            id: "graphic_1".into(),
            panel_id: panel.id.clone(),
            asset_id: "image_1".into(),
            center_m: centre,
            size_m: DVec2::new(0.1, 0.05),
            rotation_degrees: 30.,
            side: GraphicSide::Front,
            opacity: 1.,
        };
        let triangles = board_triangles(&placement, &garment, &|p| {
            egui::pos2(p.x as f32, p.y as f32)
        });
        assert!(!triangles.is_empty());
        let mut area = 0.;
        for t in &triangles {
            for (_, uv) in t {
                assert!((-1e-6..=1. + 1e-6).contains(&uv.x) && (-1e-6..=1. + 1e-6).contains(&uv.y));
            }
            let [a, b, c] = t.map(|(p, _)| p);
            area += 0.5 * ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs();
        }
        // The whole rectangle lies inside the piece, so its cut-out keeps all of its area.
        assert!((area - 0.1 * 0.05).abs() < 1e-6, "area {area}");
    }
}
