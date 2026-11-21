//! Taking the garment and its animation out of the editor: OBJ with its options, glTF, the
//! take as PC2 or MDD point caches or as OBJ sequences of the cloth and the avatar, and
//! captures of the 3D view: one PNG, or every frame of the take encoded as a video with
//! ffmpeg when it is installed (otherwise the PNG frames stay).
use super::*;
use fashioncraft::avatar_animation;
use rfd::FileDialog;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Frames to wait after moving the timeline so the 3D view shows the new frame.
const SETTLE_FRAMES: u8 = 2;
const NO_TAKE: &str = "Record the animation first (Animation › Record)";

struct CaptureTag;

/// A capture of the 3D view in progress: one PNG, or each frame of the take.
struct Capture {
    /// The PNG of a single capture, or the folder of a video's frames.
    target: PathBuf,
    /// Where the video goes once every frame is captured.
    video: Option<PathBuf>,
    frame: usize,
    frames: usize,
    fps: f64,
    wait: u8,
    requested: bool,
    /// The clean view setting to restore afterwards.
    clean_view: bool,
}

#[derive(Default)]
pub(super) struct Media3d {
    capture: Option<Capture>,
    encoding: Option<std::thread::JoinHandle<Result<String, String>>>,
    obj: ObjDialog,
    print: PrintDialog,
}

/// The 1:1 pattern print options, kept between saves.
#[derive(Default)]
struct PrintDialog {
    open: bool,
    paper: fashioncraft::garment::print_pdf::Paper,
}

/// The OBJ export options, kept between exports.
#[derive(Default)]
struct ObjDialog {
    open: bool,
    selected_only: bool,
    avatar: bool,
    options: fashioncraft::obj_export::ObjOptions,
    /// Bake colours, fabric textures and prints into one image of this many pixels a side.
    bake: Option<u32>,
}

/// The ffmpeg program, looked for on the path and where Homebrew installs it.
pub(super) fn ffmpeg() -> Option<PathBuf> {
    [
        "ffmpeg",
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
    ]
    .into_iter()
    .find(|candidate| {
        Command::new(candidate)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
    .map(PathBuf::from)
}

/// Encode `frames/frame_0000.png`… as an H.264 video.
fn encode(ffmpeg: &Path, frames: &Path, fps: f64, video: &Path) -> Result<String, String> {
    let output = Command::new(ffmpeg)
        .args([
            "-y",
            "-loglevel",
            "error",
            "-framerate",
            &format!("{fps}"),
            "-i",
        ])
        .arg(frames.join("frame_%04d.png"))
        .args([
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(video)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(format!("Video saved as {}", video.display()))
    } else {
        Err(format!(
            "ffmpeg could not encode the video: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

impl Atelier {
    pub(super) fn export_point_cache(&mut self) -> Result<(), String> {
        let take = self.current_take().ok_or(NO_TAKE)?;
        let Some(path) = FileDialog::new()
            .add_filter("PC2 point cache", &["pc2"])
            .set_file_name("cloth.pc2")
            .save_file()
        else {
            return Ok(());
        };
        take.write_pc2(&path)?;
        self.status = format!(
            "Point cache saved: {} frames in {}",
            take.frames.len(),
            path.display()
        );
        Ok(())
    }

    /// File › Import › Point cache: a PC2 cache of this garment's mesh becomes the take on the
    /// timeline, played back and exported like a recording.
    pub(super) fn import_point_cache(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .set_title("Import a point cache")
            .add_filter("PC2 point cache", &["pc2"])
            .pick_file()
        else {
            return Ok(());
        };
        self.load_point_cache(&path)
    }

    pub(super) fn export_obj_sequence(&mut self) -> Result<(), String> {
        let take = self.current_take().ok_or(NO_TAKE)?;
        let Some(dir) = FileDialog::new()
            .set_title("Folder for the OBJ sequence")
            .pick_folder()
        else {
            return Ok(());
        };
        let frames = take.write_obj_sequence(&self.scene.garment, &dir, "cloth")?;
        if let Some(animation) = &self.project.avatar_animation {
            avatar_animation::write_body_sequence(
                &self.project,
                animation,
                frames,
                &dir,
                "avatar",
            )?;
        }
        self.status = format!(
            "OBJ sequence saved: {frames} frames of the cloth and the avatar in {}",
            dir.display()
        );
        Ok(())
    }

    /// The garment, the avatar and the take when there is one, as glTF or GLB.
    pub(super) fn export_gltf(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .add_filter("glTF binary", &["glb"])
            .add_filter("glTF", &["gltf"])
            .set_file_name("garment.glb")
            .save_file()
        else {
            return Ok(());
        };
        self.export_gltf_to(&path)
    }

    /// The glTF export to `path`, without a dialog (agents use it too).
    pub(super) fn export_gltf_to(&mut self, path: &std::path::Path) -> Result<(), String> {
        let take = self.current_take();
        let bodies = match (take, &self.project.avatar_animation) {
            (Some(take), Some(animation)) => Some(
                avatar_animation::body_frames(&self.project, animation, take.frames.len())?
                    .into_iter()
                    .map(|b| b.vertices)
                    .collect(),
            ),
            _ => None,
        };
        // With a take, the avatar keeps these triangles and moves with the frames.
        let avatar = self.scene.collider.surface.as_ref().clone();
        let images: Vec<(String, PathBuf)> = self
            .project
            .image_assets
            .iter()
            .map(|a| (a.id.clone(), a.path.clone()))
            .collect();
        let scene = fashioncraft::gltf_export::GltfScene {
            template: &self.project.template,
            garment: &self.scene.garment,
            state: &self.scene.state,
            images: &images,
            avatar: self.prefs.garment_display.avatar.then_some(&avatar),
            take: take.map(|t| (t, bodies)),
        };
        fashioncraft::gltf_export::write(&scene, path)?;
        self.status = format!(
            "Exported {}{}",
            path.display(),
            if take.is_some() {
                " with the take as an animation"
            } else {
                ""
            }
        );
        Ok(())
    }

    /// The garment, the avatar when shown, and the take when there is one, as a USDZ package
    /// or a USDA layer.
    pub(super) fn export_usd(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .add_filter("USDZ package", &["usdz"])
            .add_filter("USD text layer", &["usda"])
            .set_file_name("garment.usdz")
            .save_file()
        else {
            return Ok(());
        };
        let take = self.current_take();
        let bodies = match (take, &self.project.avatar_animation) {
            (Some(take), Some(animation)) => Some(
                avatar_animation::body_frames(&self.project, animation, take.frames.len())?
                    .into_iter()
                    .map(|b| b.vertices)
                    .collect(),
            ),
            _ => None,
        };
        let avatar = self.scene.collider.surface.as_ref().clone();
        let images: Vec<(String, PathBuf)> = self
            .project
            .image_assets
            .iter()
            .map(|a| (a.id.clone(), a.path.clone()))
            .collect();
        let scene = fashioncraft::usd_export::UsdScene {
            template: &self.project.template,
            garment: &self.scene.garment,
            state: &self.scene.state,
            images: &images,
            avatar: self.prefs.garment_display.avatar.then_some(&avatar),
            take: take.map(|t| (t, bodies)),
        };
        fashioncraft::usd_export::write(&scene, &path)?;
        self.status = format!(
            "Exported {}{}",
            path.display(),
            if take.is_some() {
                " with the take as an animation"
            } else {
                ""
            }
        );
        Ok(())
    }

    /// The garment, the avatar when shown, and the take when there is one (as blend shapes),
    /// as a binary FBX.
    pub(super) fn export_fbx(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .add_filter("FBX", &["fbx"])
            .set_file_name("garment.fbx")
            .save_file()
        else {
            return Ok(());
        };
        let take = self.current_take();
        let bodies = match (take, &self.project.avatar_animation) {
            (Some(take), Some(animation)) => Some(
                avatar_animation::body_frames(&self.project, animation, take.frames.len())?
                    .into_iter()
                    .map(|b| b.vertices)
                    .collect(),
            ),
            _ => None,
        };
        let images: Vec<(String, PathBuf)> = self
            .project
            .image_assets
            .iter()
            .map(|a| (a.id.clone(), a.path.clone()))
            .collect();
        let scene = fashioncraft::fbx_export::FbxScene {
            template: &self.project.template,
            garment: &self.scene.garment,
            state: &self.scene.state,
            images: &images,
            avatar: self
                .prefs
                .garment_display
                .avatar
                .then_some(self.scene.collider.surface.as_ref()),
            take: take.map(|t| (t, bodies)),
        };
        fashioncraft::fbx_export::write(&scene, &path)?;
        self.status = format!(
            "Exported {}{}",
            path.display(),
            if take.is_some() {
                " with the take as an animation"
            } else {
                ""
            }
        );
        Ok(())
    }

    /// Save the garment as Alembic: animated over the take when one is recorded, with the
    /// avatar when it is shown.
    pub(super) fn export_alembic(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .add_filter("Alembic", &["abc"])
            .set_file_name("garment.abc")
            .save_file()
        else {
            return Ok(());
        };
        let take = self.current_take();
        let bodies = match (take, &self.project.avatar_animation) {
            (Some(take), Some(animation)) => Some(
                avatar_animation::body_frames(&self.project, animation, take.frames.len())?
                    .into_iter()
                    .map(|b| b.vertices)
                    .collect(),
            ),
            _ => None,
        };
        let scene = fashioncraft::abc_export::AbcScene {
            template: &self.project.template,
            garment: &self.scene.garment,
            state: &self.scene.state,
            avatar: self
                .prefs
                .garment_display
                .avatar
                .then_some(self.scene.collider.surface.as_ref()),
            take: take.map(|t| (t, bodies)),
        };
        let summary = fashioncraft::abc_export::write(&scene, &path)?;
        self.status = format!(
            "Alembic saved: {} frames in {}",
            summary.frames,
            path.display()
        );
        Ok(())
    }

    pub(super) fn open_print(&mut self) {
        self.authoring.media3d.print.open = true;
    }

    /// The 1:1 pattern print options: the paper to tile over, and PDF or SVG.
    pub(super) fn print_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.media3d.print.open {
            return;
        }
        let ui_lang = self.prefs.language;
        let mut open = true;
        let (mut pdf, mut svg) = (false, false);
        let paper = &mut self.authoring.media3d.print.paper;
        egui::Window::new(i18n::tr(ui_lang, "2D patterns at 1:1"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                super::workspace::prop_row(ui, i18n::tr(ui_lang, "Paper"), |ui| {
                    egui::ComboBox::from_id_salt("print.paper")
                        .selected_text(i18n::tr(ui_lang, paper.label()))
                        .show_ui(ui, |ui| {
                            for p in fashioncraft::garment::print_pdf::Paper::ALL {
                                ui.selectable_value(paper, p, i18n::tr(ui_lang, p.label()));
                            }
                        });
                });
                ui.small(i18n::tr(ui_lang, "Pieces at actual size, tiled over framed pages to trim and tape edge to edge. Print at 100% and check the 100 mm ruler."));
                ui.horizontal(|ui| {
                    pdf = ui.button(i18n::tr(ui_lang, "Save PDF…")).clicked();
                    svg = ui.button(i18n::tr(ui_lang, "Save SVG (one sheet)…")).clicked();
                });
            });
        self.authoring.media3d.print.open &= open;
        let saved = if pdf {
            self.save_print(true)
        } else if svg {
            self.save_print(false)
        } else {
            return;
        };
        match saved {
            Ok(true) => self.authoring.media3d.print.open = false,
            Ok(false) => {}
            Err(e) => self.status = e,
        }
    }

    /// Save the patterns at 1:1 as a tiled PDF or a one-sheet SVG; whether a file was saved.
    fn save_print(&mut self, pdf: bool) -> Result<bool, String> {
        let (filter, extension, name) = if pdf {
            ("PDF", "pdf", "patterns-1to1.pdf")
        } else {
            ("SVG", "svg", "patterns-1to1.svg")
        };
        let Some(path) = FileDialog::new()
            .add_filter(filter, &[extension])
            .set_file_name(name)
            .save_file()
        else {
            return Ok(false);
        };
        let template = &self.project.template;
        let (bytes, pages) = if pdf {
            let paper = self.authoring.media3d.print.paper;
            let (bytes, summary) =
                fashioncraft::garment::print_pdf::print_pattern_pdf(template, paper)?;
            (bytes, summary.pages)
        } else {
            let svg = fashioncraft::garment::export::print_pattern_svg(template)?;
            (svg.into_bytes(), 1)
        };
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.status = format!(
            "Patterns at 1:1 saved: {} pages in {}",
            pages,
            path.display()
        );
        Ok(true)
    }

    /// Open the OBJ export options, for every piece or only the selected ones.
    pub(super) fn open_obj_export(&mut self, selected_only: bool) -> Result<(), String> {
        if selected_only && self.selected_panels().is_empty() {
            return Err("Select the pieces to export first".into());
        }
        let dialog = &mut self.authoring.media3d.obj;
        dialog.open = true;
        dialog.selected_only = selected_only;
        Ok(())
    }

    fn export_obj(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .add_filter("OBJ mesh", &["obj"])
            .set_file_name("garment.obj")
            .save_file()
        else {
            return Ok(());
        };
        self.export_obj_to(ctx, &path)
    }

    /// The OBJ export to `path` with the export window's options, without a dialog (agents use
    /// it too).
    pub(super) fn export_obj_to(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let dialog = &self.authoring.media3d.obj;
        let mut options = dialog.options.clone();
        options.pieces = dialog
            .selected_only
            .then(|| self.selected_panels().into_iter().collect());
        let (bake, avatar) = (dialog.bake, dialog.avatar);
        let images: Vec<(String, PathBuf)> = self
            .project
            .image_assets
            .iter()
            .map(|a| (a.id.clone(), a.path.clone()))
            .collect();
        // The fabric textures and the prints, decoded, for baking.
        let atlas = match bake {
            Some(size) => {
                let template = &self.project.template;
                let assets: std::collections::BTreeSet<String> = template
                    .fabric_appearance
                    .values()
                    .filter_map(|a| a.texture.as_ref().map(|t| t.asset_id.clone()))
                    .chain(template.graphics.iter().map(|g| g.asset_id.clone()))
                    .collect();
                let mut decoded = vec![];
                for asset in assets {
                    if let Some(Ok(loaded)) = self.graphic(ctx, &asset) {
                        decoded.push((asset, loaded.image.clone()));
                    }
                }
                let pixels: std::collections::HashMap<String, fashioncraft::bake::Image> = decoded
                    .iter()
                    .map(|(id, image)| {
                        (
                            id.clone(),
                            fashioncraft::bake::Image {
                                size: image.size,
                                rgba: &image.rgba,
                            },
                        )
                    })
                    .collect();
                Some(fashioncraft::bake::bake(
                    &self.project.template,
                    &self.scene.garment,
                    &pixels,
                    size,
                )?)
            }
            None => None,
        };
        let scene = fashioncraft::obj_export::ObjScene {
            template: &self.project.template,
            garment: &self.scene.garment,
            state: &self.scene.state,
            images: &images,
            avatar: avatar.then_some(self.scene.collider.surface.as_ref()),
            atlas: atlas.as_ref(),
        };
        let summary = fashioncraft::obj_export::write(&scene, &options, path)?;
        self.status = format!(
            "Exported {}: {} vertices, {} faces, {} textures",
            path.display(),
            summary.vertices,
            summary.faces,
            summary.textures
        );
        Ok(())
    }

    pub(super) fn obj_export_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.media3d.obj.open {
            return;
        }
        let mut open = true;
        let mut export = false;
        let dialog = &mut self.authoring.media3d.obj;
        egui::Window::new(if dialog.selected_only {
            i18n::tr(ui_lang, "Export OBJ (selected)")
        } else {
            i18n::tr(ui_lang, "Export OBJ")
        })
        .open(&mut open)
        .resizable(false)
        .default_width(280.)
        .show(ctx, |ui| {
            let o = &mut dialog.options;
            ui.checkbox(&mut o.thick, i18n::tr(ui_lang, "Thick"))
                .on_hover_text(i18n::tr(ui_lang, "Both faces of the cloth, apart by the fabric's look thickness (1 mm without one), joined by a rim"));
            ui.checkbox(&mut o.unified_uv, i18n::tr(ui_lang, "Unified UV"))
                .on_hover_text(i18n::tr(ui_lang, "Pack every piece into the unit square of texture space; fabric textures are left out"));
            ui.horizontal(|ui| {
                let mut baking = dialog.bake.is_some();
                if ui
                    .checkbox(&mut baking, i18n::tr(ui_lang, "Bake the look"))
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Colours, fabric textures and prints painted into one image, with every piece packed into it",
                    ))
                    .changed()
                {
                    dialog.bake = baking.then_some(2048);
                }
                if let Some(size) = &mut dialog.bake {
                    egui::ComboBox::from_id_salt("obj.bake")
                        .selected_text(format!("{size} px"))
                        .show_ui(ui, |ui| {
                            for s in [1024u32, 2048, 4096] {
                                ui.selectable_value(size, s, format!("{s} px"));
                            }
                        });
                }
            });
            ui.horizontal(|ui| {
                let mut on = o.retopology.is_some();
                ui.checkbox(&mut on, i18n::tr(ui_lang, "Retopology"))
                    .on_hover_text(i18n::tr(ui_lang, "Each piece remeshed as quads of this size along its grain, lying on the drape; pieces stay apart"));
                let mut mm = o.retopology.unwrap_or(0.02) * 1000.;
                ui.add_enabled(
                    on,
                    egui::DragValue::new(&mut mm).range(2.0..=500.0).speed(0.5).suffix(" mm"),
                );
                o.retopology = on.then_some(mm / 1000.);
            });
            let remeshed = o.retopology.is_some();
            ui.add_enabled(!remeshed, egui::Checkbox::new(&mut o.weld, i18n::tr(ui_lang, "Weld seams")))
                .on_hover_text(i18n::tr(ui_lang, "Each seam's two edges joined vertex to vertex, so the pieces make one connected mesh; vertices are added where the two sides' meshes differ"));
            // Welded pieces share vertices: they stay in one object.
            let welded = o.weld && !remeshed;
            if welded {
                o.per_piece = false;
            }
            ui.add_enabled(!welded, egui::Checkbox::new(&mut o.per_piece, i18n::tr(ui_lang, "One object per piece")));
            ui.add_enabled(!remeshed, egui::Checkbox::new(&mut o.quads, i18n::tr(ui_lang, "Quads")))
                .on_hover_text(i18n::tr(ui_lang, "Neighbouring triangles joined into quads where they make a flat, convex, nearly rectangular one"));
            ui.checkbox(&mut dialog.avatar, i18n::tr(ui_lang, "Include the avatar and scene objects"));
            super::workspace::prop_row(ui, i18n::tr(ui_lang, "Units"), |ui| {
                egui::ComboBox::from_id_salt("obj.units")
                    .selected_text(o.units.label())
                    .show_ui(ui, |ui| {
                        for u in fashioncraft::obj_export::Units::ALL {
                            ui.selectable_value(&mut o.units, u, u.label());
                        }
                    });
                ui.checkbox(&mut o.z_up, i18n::tr(ui_lang, "Z up"));
            });
            ui.small(i18n::tr(
                ui_lang,
                if o.weld && o.retopology.is_none() {
                    "Pieces share their vertices along welded seams."
                } else {
                    "Pieces stay split along seams."
                },
            ));
            if ui.button(i18n::tr(ui_lang, "Export…")).clicked() {
                export = true;
            }
        });
        self.authoring.media3d.obj.open &= open;
        if export {
            match self.export_obj(ctx) {
                Ok(()) => self.authoring.media3d.obj.open = false,
                Err(e) => self.status = e,
            }
        }
    }

    pub(super) fn export_mdd(&mut self, z_up: bool) -> Result<(), String> {
        let take = self.current_take().ok_or(NO_TAKE)?;
        let Some(path) = FileDialog::new()
            .add_filter("MDD point cache", &["mdd"])
            .set_file_name("cloth.mdd")
            .save_file()
        else {
            return Ok(());
        };
        take.write_mdd(&path, z_up)?;
        self.status = format!(
            "MDD cache saved: {} frames in {}",
            take.frames.len(),
            path.display()
        );
        Ok(())
    }

    /// Save the 3D view as a PNG, as it is on screen.
    pub(super) fn capture_view3d(&mut self) -> Result<(), String> {
        let Some(path) = FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name("view.png")
            .save_file()
        else {
            return Ok(());
        };
        self.begin_capture(path, None, 1, 30.);
        Ok(())
    }

    /// Capture every frame of the take from the 3D view and encode them as a video.
    pub(super) fn capture_video(&mut self) -> Result<(), String> {
        let take = self.current_take().ok_or(NO_TAKE)?;
        let (frames, fps) = (take.frames.len(), take.fps);
        let Some(video) = FileDialog::new()
            .add_filter("MP4 video", &["mp4"])
            .set_file_name("take.mp4")
            .save_file()
        else {
            return Ok(());
        };
        let stem = video
            .file_stem()
            .map_or("take".into(), |s| s.to_string_lossy().into_owned());
        let dir = video.with_file_name(format!("{stem}_frames"));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        self.authoring.animation.open = true;
        self.playing = false;
        self.stop_animation_preview();
        self.seek_animation(0.);
        self.begin_capture(dir, Some(video), frames, fps);
        Ok(())
    }

    fn begin_capture(&mut self, target: PathBuf, video: Option<PathBuf>, frames: usize, fps: f64) {
        let ui_lang = self.prefs.language;
        self.authoring.media3d.capture = Some(Capture {
            target,
            video,
            frame: 0,
            frames,
            fps,
            wait: SETTLE_FRAMES,
            requested: false,
            clean_view: self.clean_view,
        });
        // Captures show the garment and avatar without selection and inspection marks.
        self.clean_view = true;
        self.status = i18n::tr(ui_lang, "Capturing the 3D view…").into();
    }

    fn end_capture(&mut self, status: String) {
        if let Some(capture) = self.authoring.media3d.capture.take() {
            self.clean_view = capture.clean_view;
        }
        self.status = status;
    }

    /// Run a capture in progress and collect a finished video. Call once per frame after the
    /// views are painted.
    pub(super) fn drive_media3d(&mut self, ctx: &egui::Context) {
        if self
            .authoring
            .media3d
            .encoding
            .as_ref()
            .is_some_and(|h| h.is_finished())
        {
            let done = self.authoring.media3d.encoding.take().unwrap().join();
            self.status = match done {
                Ok(Ok(message)) | Ok(Err(message)) => message,
                Err(_) => "The video encoder stopped unexpectedly".into(),
            };
        }
        let Some(capture) = self.authoring.media3d.capture.as_mut() else {
            return;
        };
        ctx.request_repaint();
        if capture.wait > 0 {
            capture.wait -= 1;
            return;
        }
        if !capture.requested {
            capture.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                CaptureTag,
            )));
            return;
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot {
                    user_data, image, ..
                } if user_data
                    .data
                    .as_ref()
                    .is_some_and(|d| d.downcast_ref::<CaptureTag>().is_some()) =>
                {
                    Some(image.clone())
                }
                _ => None,
            })
        });
        let Some(image) = shot else {
            return;
        };
        let pixels_per_point = ctx.pixels_per_point();
        let Some(rect) = self
            .authoring
            .selection
            .garment_rect
            .and_then(|r| super::board_media::capture_rect(r, image.size, pixels_per_point))
        else {
            self.end_capture("Capture stopped: the 3D view is not on screen".into());
            return;
        };
        let view = image.region(&rect, Some(pixels_per_point));
        let pixels: Vec<u8> = view.pixels.iter().flat_map(|p| p.to_array()).collect();
        let capture = self.authoring.media3d.capture.as_mut().unwrap();
        let path = match capture.video {
            Some(_) => capture
                .target
                .join(format!("frame_{:04}.png", capture.frame)),
            None => capture.target.clone(),
        };
        if let Err(e) = image::save_buffer(
            &path,
            &pixels,
            view.size[0] as u32,
            view.size[1] as u32,
            image::ColorType::Rgba8,
        ) {
            self.end_capture(format!("Capture could not be saved: {e}"));
            return;
        }
        capture.frame += 1;
        if capture.frame < capture.frames {
            let time = capture.frame as f64 / capture.fps;
            capture.wait = SETTLE_FRAMES;
            capture.requested = false;
            self.status = format!(
                "Capturing the take… frame {} of {}",
                capture.frame + 1,
                capture.frames
            );
            self.seek_animation(time);
            return;
        }
        let (dir, video, fps) = (capture.target.clone(), capture.video.clone(), capture.fps);
        let Some(video) = video else {
            self.end_capture(format!(
                "3D view saved as {} ({} × {} px)",
                dir.display(),
                view.size[0],
                view.size[1]
            ));
            return;
        };
        match ffmpeg() {
            Some(program) => {
                self.authoring.media3d.encoding = Some(std::thread::spawn(move || {
                    encode(&program, &dir, fps, &video)
                }));
                self.end_capture("Encoding the video…".into());
            }
            None => self.end_capture(format!(
                "Frames saved in {}; install ffmpeg to have them encoded as a video",
                dir.display()
            )),
        }
    }
}
