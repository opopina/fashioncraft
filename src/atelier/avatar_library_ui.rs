//! Avatar looks in the atelier: the current avatar's skin and parts (eyes, eyebrows,
//! eyelashes, hair) for the 3D view, following its pose; and the avatar library, ready-made
//! MakeHuman bodies with their looks, each offered like an imported avatar.
use super::*;
use fashioncraft::{
    avatar::SurfaceMesh,
    avatar_look::{self, BoundPart, DecodedImage, LookChoice, PRESETS},
    avatar_maker::{self, MacroBody, MacroTargets},
    render::{AvatarLookDraw, AvatarPartDraw, GraphicImage},
};
use glam::{DVec3, Vec3};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

/// Side of the library's thumbnails, in pixels (drawn at half that in points).
const THUMB: [u32; 2] = [180, 270];

/// The current avatar's look, read once per avatar file and look file.
#[derive(Default)]
pub(super) struct LookCache {
    /// The avatar file, when its look file was written, and when that was last checked.
    source: Option<(PathBuf, Option<SystemTime>, Instant)>,
    loaded: Option<LoadedLook>,
    /// Why the look could not be shown.
    pub error: Option<String>,
}

/// Texture coordinates of the avatar's triangles' corners: MakeHuman's, or its file's.
type BodyUvs = Cow<'static, [[[f32; 2]; 3]]>;

struct LoadedLook {
    /// The avatar at rest, as its file has it.
    rest: Vec<DVec3>,
    skin: Option<(GraphicImage, BodyUvs)>,
    parts: Vec<LoadedPart>,
    /// The parts where the avatar last drawn has them, and a key of that avatar.
    posed: Option<(u64, Vec<Vec<Vec3>>)>,
    generation: u64,
}

struct LoadedPart {
    image: GraphicImage,
    bound: BoundPart,
    triangles: Vec<[u32; 3]>,
    uvs: Vec<[[f32; 2]; 3]>,
    cutout: bool,
}

fn hash_of(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, c| {
        (h ^ u64::from(c)).wrapping_mul(0x100_0000_01b3)
    })
}

/// An image a look names, decoded.
fn decoded(reference: &str, look_dir: &Path) -> Result<DecodedImage, String> {
    let bytes = avatar_look::image_bytes(reference, look_dir)?;
    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("{reference}: {e}"))?
        .to_rgba8();
    Ok(DecodedImage {
        size: [image.width(), image.height()],
        rgba: image.into_raw(),
    })
}

/// Side the viewport keeps its textures at; larger images are reduced to it once here.
const TEXTURE_SIDE: u32 = 1024;

/// `image` for the viewport under `key`.
fn graphic(image: DecodedImage, key: u64) -> GraphicImage {
    let [w, h] = image.size;
    let reduced = (w > TEXTURE_SIDE || h > TEXTURE_SIDE)
        .then(|| image::RgbaImage::from_raw(w, h, image.rgba.clone()))
        .flatten()
        .map(|buffer| {
            image::imageops::resize(
                &buffer,
                TEXTURE_SIDE,
                TEXTURE_SIDE,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
        });
    match reduced {
        Some(rgba) => GraphicImage {
            key,
            size: [TEXTURE_SIDE; 2],
            rgba,
        },
        None => GraphicImage {
            key,
            size: image.size,
            rgba: image.rgba,
        },
    }
}

fn modified(path: Option<&Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|m| m.modified()).ok()
}

impl LookCache {
    /// Keep the look of the avatar at `avatar` current: read again when the avatar or its
    /// look file changed (checked once a second).
    fn refresh(&mut self, avatar: &Path) {
        let now = Instant::now();
        if let Some((path, _, checked)) = &self.source
            && path == avatar
            && now.duration_since(*checked) < Duration::from_secs(1)
        {
            return;
        }
        let file = avatar_look::look_file(avatar);
        let written = modified(file.as_deref());
        let same = self
            .source
            .as_ref()
            .is_some_and(|(path, w, _)| path == avatar && *w == written);
        self.source = Some((avatar.to_owned(), written, now));
        if same {
            return;
        }
        let generation = self.loaded.as_ref().map_or(1, |l| l.generation + 1);
        self.loaded = None;
        self.error = None;
        match load(avatar, generation) {
            Ok(loaded) => self.loaded = loaded,
            Err(e) => self.error = Some(e),
        }
    }

    /// Make the look of the avatar at `avatar` ready to draw on `body`, the avatar as shown
    /// (posed or not): its parts moved with it.
    pub(super) fn prepare(&mut self, avatar: &Path, body: &SurfaceMesh) {
        self.refresh(avatar);
        let Some(loaded) = self.loaded.as_mut() else {
            return;
        };
        if body.vertices.len() < loaded.rest.len() {
            loaded.posed = None;
            return;
        }
        // A key of the shown avatar: where the vertices the parts follow are.
        let mut key = 0xcbf2_9ce4_8422_2325u64 ^ body.vertices.len() as u64;
        for part in &loaded.parts {
            for i in &part.bound.anchors {
                for v in body.vertices[*i].to_array() {
                    key = (key ^ v.to_bits()).wrapping_mul(0x100_0000_01b3);
                }
            }
        }
        if loaded.posed.as_ref().is_none_or(|(k, _)| *k != key) {
            let posed = loaded
                .parts
                .iter()
                .map(|p| {
                    p.bound
                        .posed(&loaded.rest, &body.vertices)
                        .unwrap_or_else(|| p.bound.rest.clone())
                        .iter()
                        .map(|v| v.as_vec3())
                        .collect()
                })
                .collect();
            loaded.posed = Some((key, posed));
        }
    }

    /// The look as [`LookCache::prepare`] left it, its parts' draws gathered in `parts`.
    pub(super) fn view<'a>(
        &'a self,
        parts: &'a mut Vec<AvatarPartDraw<'a>>,
    ) -> Option<AvatarLookDraw<'a>> {
        let loaded = self.loaded.as_ref()?;
        let (key, posed) = loaded.posed.as_ref()?;
        parts.extend(
            loaded
                .parts
                .iter()
                .zip(posed)
                .map(|(p, vertices)| AvatarPartDraw {
                    image: &p.image,
                    vertices,
                    triangles: &p.triangles,
                    uvs: &p.uvs,
                    cutout: p.cutout,
                }),
        );
        Some(AvatarLookDraw {
            skin: loaded
                .skin
                .as_ref()
                .map(|(image, uvs)| (image, uvs.as_ref())),
            parts,
            key: key ^ loaded.generation.wrapping_mul(0x9e37_79b9_7f4a_7c15),
        })
    }
}

/// The look of the avatar at `avatar`, ready to draw; none when it has none.
fn load(avatar: &Path, generation: u64) -> Result<Option<LoadedLook>, String> {
    let Some(look) = avatar_look::load(avatar)? else {
        return Ok(None);
    };
    let dir = avatar_look::look_file(avatar)
        .and_then(|f| f.parent().map(Path::to_owned))
        .unwrap_or_default();
    let rest = SurfaceMesh::load_obj(avatar)?;
    // Images of this avatar's look are its own: keyed by the look file and the reference.
    let place = avatar_look::look_file(avatar)
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key = |reference: &str| hash_of(&format!("{place}|{generation}|{reference}"));
    let skin = match &look.skin {
        Some(reference) => {
            let uvs: Option<BodyUvs> = if avatar_maker::is_makehuman_body(&rest) {
                Some(Cow::Borrowed(avatar_look::body_uvs()?))
            } else {
                avatar_look::avatar_uvs(avatar)?
                    .filter(|u| u.len() == rest.triangles.len())
                    .map(Cow::Owned)
            };
            match uvs {
                Some(uvs) => {
                    let mut image = decoded(reference, &dir)?;
                    if let Some(underwear) = &look.underwear {
                        avatar_look::paint_underwear(&mut image, &rest.triangles, &uvs, underwear);
                    }
                    Some((graphic(image, key(reference)), uvs))
                }
                None => None,
            }
        }
        None => None,
    };
    let parts = look
        .parts
        .iter()
        .map(|p| {
            let mut image = decoded(&p.image, &dir)?;
            if let Some(tint) = p.tint {
                image = avatar_look::tinted(&image, tint);
            }
            Ok(LoadedPart {
                image: graphic(image, key(&format!("{}|{:?}", p.image, p.tint))),
                bound: BoundPart::new(&rest.vertices, p),
                triangles: p.triangles.clone(),
                uvs: p.uvs.clone(),
                cutout: p.cutout,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Some(LoadedLook {
        rest: rest.vertices,
        skin,
        parts,
        posed: None,
        generation,
    }))
}

/// The avatar library window.
#[derive(Default)]
pub(super) struct AvatarLibraryUi {
    pub open: bool,
    /// Thumbnails drawn in the background, by preset, as they are ready.
    pictures: Option<Arc<Mutex<Vec<Option<egui::ColorImage>>>>>,
    textures: Vec<Option<egui::TextureHandle>>,
    error: Option<String>,
}

/// Draw every preset's thumbnail, one after another, into `out`.
fn draw_thumbnails(out: Arc<Mutex<Vec<Option<egui::ColorImage>>>>, ctx: egui::Context) {
    let Ok(targets) = MacroTargets::bundled() else {
        return;
    };
    let Ok(body_uvs) = avatar_look::body_uvs() else {
        return;
    };
    let mut images: BTreeMap<String, DecodedImage> = BTreeMap::new();
    for (k, preset) in PRESETS.iter().enumerate() {
        let Ok(made) = targets.make(&(preset.body)()) else {
            continue;
        };
        let Ok(look) = avatar_look::make_look(&made, &preset.look) else {
            continue;
        };
        for reference in look.skin.iter().chain(look.parts.iter().map(|p| &p.image)) {
            if images.contains_key(reference) {
                continue;
            }
            let Ok(bytes) = avatar_look::image_bytes(reference, Path::new(".")) else {
                continue;
            };
            if let Ok(image) = image::load_from_memory(&bytes) {
                let image = image.to_rgba8();
                images.insert(
                    reference.clone(),
                    DecodedImage {
                        size: [image.width(), image.height()],
                        rgba: image.into_raw(),
                    },
                );
            }
        }
        let rgba = avatar_look::thumbnail(&made.surface, Some(body_uvs), &look, &images, THUMB);
        let picture =
            egui::ColorImage::from_rgba_unmultiplied([THUMB[0] as usize, THUMB[1] as usize], &rgba);
        if let Ok(mut pictures) = out.lock() {
            pictures[k] = Some(picture);
        }
        ctx.request_repaint();
    }
}

/// Write the body and look made from `body` and `choice` in a folder of `output` named after
/// them, with the body's joints and anchors: the avatar file to offer.
pub(super) fn write_made_avatar(
    output: &Path,
    body: &MacroBody,
    choice: &LookChoice,
) -> Result<PathBuf, String> {
    let made = MacroTargets::bundled()?.make(body)?;
    let hash = hash_of(&format!("{body:?}{choice:?}"));
    let dir = output.join(format!("avatar-makehuman-{hash:016x}"));
    let obj = super::avatar_shape_ui::write_avatar(&dir, &made.surface, &made.joints)?;
    let look_path = dir.join("look.json");
    if choice.is_empty() {
        if look_path.exists() {
            std::fs::remove_file(&look_path).map_err(|e| e.to_string())?;
        }
    } else {
        let look = avatar_look::make_look(&made, choice)?;
        let text = serde_json::to_string(&look).map_err(|e| e.to_string())?;
        std::fs::write(&look_path, text).map_err(|e| e.to_string())?;
    }
    Ok(obj)
}

impl Atelier {
    pub(super) fn avatar_library_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.avatar_library.open {
            return;
        }
        let tr = |text: &'static str| i18n::tr(ui_lang, text);
        let state = &mut self.authoring.avatar_library;
        let pictures = state
            .pictures
            .get_or_insert_with(|| {
                let out = Arc::new(Mutex::new(vec![None; PRESETS.len()]));
                let (into, ctx) = (out.clone(), ctx.clone());
                std::thread::spawn(move || draw_thumbnails(into, ctx));
                out
            })
            .clone();
        state.textures.resize(PRESETS.len(), None);
        if let Ok(mut pictures) = pictures.lock() {
            for (k, picture) in pictures.iter_mut().enumerate() {
                if let Some(picture) = picture.take() {
                    state.textures[k] = Some(ctx.load_texture(
                        format!("avatar.library.{k}"),
                        picture,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }
        let mut open = true;
        let mut chosen = None;
        let mut customise = None;
        egui::Window::new(tr("Avatar library"))
            .open(&mut open)
            .default_width(640.)
            .show(ctx, |ui| {
                ui.small(tr(
                    "Ready-made avatars with skin, eyes and hair. Use one as it is, or open it in Create avatar to change it.",
                ));
                egui::ScrollArea::vertical().max_height(560.).show(ui, |ui| {
                    egui::Grid::new("avatar.library")
                        .spacing([10., 10.])
                        .show(ui, |ui| {
                            for (k, preset) in PRESETS.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let size = egui::vec2(THUMB[0] as f32, THUMB[1] as f32) / 2.;
                                    match &state.textures[k] {
                                        Some(texture) => {
                                            let image = egui::Image::new((texture.id(), size))
                                                .sense(egui::Sense::click());
                                            if ui
                                                .add(image)
                                                .on_hover_text(tr("Use this avatar"))
                                                .clicked()
                                            {
                                                chosen = Some(k);
                                            }
                                        }
                                        None => {
                                            ui.allocate_ui(size, |ui| {
                                                ui.centered_and_justified(|ui| ui.spinner());
                                            });
                                        }
                                    }
                                    ui.label(tr(preset.name));
                                    ui.horizontal(|ui| {
                                        let use_button = ui.small_button(tr("Use"));
                                        super::demo::mark(ctx, format_args!("library.use.{k}"), use_button.rect);
                                        if use_button.clicked() {
                                            chosen = Some(k);
                                        }
                                        if ui
                                            .small_button(tr("Customise…"))
                                            .on_hover_text(tr("Open it in Create avatar"))
                                            .clicked()
                                        {
                                            customise = Some(k);
                                        }
                                    });
                                });
                                if k % 4 == 3 {
                                    ui.end_row();
                                }
                            }
                        });
                });
                if let Some(e) = &state.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                ui.small(tr(
                    "Bodies, skins, eyes and hair: MakeHuman (CC0). Settings by age: Anny, NAVER LABS Europe (Apache 2.0).",
                ));
            });
        self.authoring.avatar_library.open &= open;
        if let Some(k) = customise {
            let preset = &PRESETS[k];
            self.authoring
                .avatar_maker
                .set((preset.body)(), preset.look);
            self.authoring.avatar_maker.open = true;
        }
        if let Some(k) = chosen {
            let preset = &PRESETS[k];
            match write_made_avatar(&self.output, &(preset.body)(), &preset.look) {
                Ok(obj) => {
                    self.authoring.avatar_library.error = None;
                    // The choice is made: the library gives way to the confirmation.
                    self.authoring.avatar_library.open = false;
                    self.offer_avatar(&obj);
                    self.status = i18n::tr(
                        self.prefs.language,
                        "Avatar from the library · confirm to replace the current one",
                    )
                    .into();
                }
                Err(e) => self.authoring.avatar_library.error = Some(e),
            }
        }
    }
}
