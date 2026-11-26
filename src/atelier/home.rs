//! The Home screen: a card over the empty workspace when FashionCraft starts without a project,
//! and again from Window › Home. It starts an empty project or one from a template, opens a
//! project, or reopens a recent one. Choosing anything closes it.
use super::*;
use fashioncraft::garment::presets;
use std::path::{Path, PathBuf};

/// Most recent projects the preferences keep.
pub(super) const RECENT_MAX: usize = 8;
/// Status when the simulation is asked to run on an empty project.
pub(super) const NOTHING_TO_SIMULATE: &str =
    "Nothing to simulate yet · draw a piece or pick a template";
/// Status when a garment export is asked of an empty project.
pub(super) const NOTHING_TO_EXPORT: &str = "Nothing to export yet: the project has no pieces";
/// Status after a new empty project.
const NEW_EMPTY: &str = "New project · draw a piece or pick a template";

/// The card's size, points.
const CARD: egui::Vec2 = egui::vec2(760., 470.);

/// What a new project starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Start {
    Empty,
    Tshirt,
    Dress,
    Skirt,
    Trousers,
}

impl Start {
    /// The templates the card offers, in its order.
    const TEMPLATES: [Self; 4] = [Self::Tshirt, Self::Dress, Self::Skirt, Self::Trousers];

    fn label(self) -> &'static str {
        match self {
            Self::Empty => "New empty project",
            Self::Tshirt => "T-shirt",
            Self::Dress => "Dress",
            Self::Skirt => "Skirt",
            Self::Trousers => "Trousers",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Empty => "The avatar alone · draw or import your first piece",
            Self::Tshirt | Self::Dress => "The two-piece example, placed on the avatar",
            Self::Skirt | Self::Trousers => "Drafted from the avatar's measurements",
        }
    }

    /// Its outline on a tile, in a unit square (y down).
    fn outline(self) -> &'static [[f32; 2]] {
        match self {
            Self::Empty => &[],
            Self::Tshirt => &[
                [0.31, 0.10],
                [0.41, 0.07],
                [0.50, 0.15],
                [0.59, 0.07],
                [0.69, 0.10],
                [0.93, 0.31],
                [0.82, 0.44],
                [0.72, 0.36],
                [0.72, 0.93],
                [0.28, 0.93],
                [0.28, 0.36],
                [0.18, 0.44],
                [0.07, 0.31],
            ],
            Self::Dress => &[
                [0.38, 0.06],
                [0.44, 0.06],
                [0.50, 0.15],
                [0.56, 0.06],
                [0.62, 0.06],
                [0.64, 0.38],
                [0.60, 0.47],
                [0.85, 0.93],
                [0.15, 0.93],
                [0.40, 0.47],
                [0.36, 0.38],
            ],
            Self::Skirt => &[
                [0.31, 0.14],
                [0.69, 0.14],
                [0.70, 0.25],
                [0.88, 0.86],
                [0.12, 0.86],
                [0.30, 0.25],
            ],
            Self::Trousers => &[
                [0.30, 0.07],
                [0.70, 0.07],
                [0.74, 0.30],
                [0.80, 0.93],
                [0.58, 0.93],
                [0.50, 0.40],
                [0.42, 0.93],
                [0.20, 0.93],
                [0.26, 0.30],
            ],
        }
    }
}

/// The project `start` begins, on the avatar of `current` with its pose and props. Skirt and
/// trousers take the default measurements here; the card drafts them from the avatar's own
/// when it can (see [`Atelier::start_from_home`]).
pub(super) fn project(start: Start, current: &FittingProject) -> Result<FittingProject, String> {
    let avatar = current.avatar_path.clone();
    let mut project = match start {
        Start::Empty => FittingProject::empty(avatar),
        Start::Tshirt => FittingProject::basic(GarmentKind::Tshirt, 2, avatar),
        Start::Dress => FittingProject::basic(GarmentKind::Dress, 2, avatar),
        Start::Skirt | Start::Trousers => {
            let template = if start == Start::Skirt {
                presets::skirt(&Default::default(), 2)?
            } else {
                presets::trousers(&Default::default(), 2)?
            };
            FittingProject {
                template,
                ..FittingProject::basic(GarmentKind::Tshirt, 2, avatar)
            }
        }
    };
    project.avatar_pose = current.avatar_pose.clone();
    project.scene_objects = current.scene_objects.clone();
    Ok(project)
}

/// `path` first in `recent`, once, keeping at most [`RECENT_MAX`].
pub(super) fn remember(recent: &mut Vec<PathBuf>, path: &Path) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    recent.retain(|p| p != &path);
    recent.insert(0, path);
    recent.truncate(RECENT_MAX);
}

/// `folder` on one short line: the home folder as `~`.
fn short_folder(folder: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| folder.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => folder.display().to_string(),
    }
}

/// What the person chose on the card.
enum Choice {
    Start(Start),
    Templates,
    Open,
    Recent(PathBuf),
}

#[derive(Default)]
pub(super) struct HomeUi {
    pub open: bool,
    logo: Option<egui::TextureHandle>,
    /// The document generation the card opened on: a document loaded some other way (a menu
    /// of the macOS menu bar, an agent) closes it.
    generation: Option<u64>,
}

impl HomeUi {
    pub(super) fn new(open: bool) -> Self {
        Self {
            open,
            ..Default::default()
        }
    }

    fn logo(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.logo.is_none() {
            let bytes = include_bytes!("../../assets/branding/fashioncraft-icon-128.png");
            let image = image::load_from_memory(bytes).ok()?.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            self.logo = Some(ctx.load_texture("home-logo", color, egui::TextureOptions::LINEAR));
        }
        self.logo.clone()
    }
}

/// A link to the template browser.
fn tile_link(ui: &mut egui::Ui, label: &str, hint: &str) -> bool {
    let response = ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{label}  ›")).color(workspace::accent(ui.visuals())),
        )
        .frame(false),
    );
    response.on_hover_text(hint).clicked()
}

/// A tile: an outline drawing over its name.
fn tile(ui: &mut egui::Ui, start: Start, label: &str, hint: &str) -> bool {
    let size = egui::vec2(84., 100.);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.visuals().clone();
    let accent = workspace::accent(&visuals);
    let hovered = response.hovered();
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        6.,
        if hovered {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            visuals.extreme_bg_color
        },
        Stroke::new(
            1.,
            if hovered {
                accent
            } else {
                visuals.widgets.noninteractive.bg_stroke.color
            },
        ),
        egui::StrokeKind::Inside,
    );
    let art = egui::Rect::from_min_size(rect.min + egui::vec2(18., 12.), egui::vec2(48., 56.));
    let ink = if hovered {
        accent
    } else {
        visuals.widgets.inactive.fg_stroke.color
    };
    let points = start
        .outline()
        .iter()
        .map(|[x, y]| art.min + egui::vec2(x * art.width(), y * art.height()))
        .collect();
    painter.add(egui::epaint::PathShape::closed_line(
        points,
        Stroke::new(1.6, ink),
    ));
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 18.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.),
        if hovered {
            visuals.strong_text_color()
        } else {
            visuals.text_color()
        },
    );
    response.on_hover_text(hint).clicked()
}

impl Atelier {
    /// The Home card, while it is open.
    pub(super) fn home_window(&mut self, ctx: &egui::Context) {
        if !self.home.open {
            self.home.generation = None;
            return;
        }
        if *self.home.generation.get_or_insert(self.generation) != self.generation {
            self.home.open = false;
            return;
        }
        let lang = self.prefs.language;
        let tr = |text| i18n::tr(lang, text);
        let logo = self.home.logo(ctx);
        let recent: Vec<PathBuf> = self
            .prefs
            .recent_projects
            .iter()
            .filter(|p| p.is_file())
            .cloned()
            .collect();
        let mut skip = !self.prefs.home_at_startup;
        let mut choice = None;
        let frame = egui::Frame::window(&ctx.global_style())
            .corner_radius(10.)
            .inner_margin(egui::Margin::same(24));
        let modal = egui::Modal::new(egui::Id::new("home"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(CARD.x);
                ui.set_min_height(CARD.y);
                let accent = workspace::accent(ui.visuals());
                // The vicuña, the name and the version.
                ui.horizontal(|ui| {
                    if let Some(logo) = &logo {
                        ui.add(
                            egui::Image::new(logo)
                                .fit_to_exact_size(egui::vec2(56., 56.))
                                .corner_radius(12.),
                        );
                    }
                    ui.add_space(6.);
                    ui.vertical(|ui| {
                        ui.add_space(4.);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("FashionCraft")
                                    .size(26.)
                                    .strong()
                                    .color(ui.visuals().strong_text_color()),
                            );
                            ui.label(
                                egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                                    .size(13.)
                                    .color(accent),
                            );
                        });
                        ui.label(
                            egui::RichText::new(tr("Design, sew and drape garments in 3D")).weak(),
                        );
                    });
                });
                ui.add_space(14.);
                ui.separator();
                ui.add_space(10.);
                ui.columns(2, |columns| {
                    let ui = &mut columns[0];
                    ui.label(workspace::section(tr("Begin")));
                    ui.add_space(6.);
                    let new = egui::Button::new(
                        egui::RichText::new(format!("+  {}", tr(Start::Empty.label())))
                            .size(15.)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(workspace::ACCENT)
                    .corner_radius(6.)
                    .min_size(egui::vec2(ui.available_width(), 40.));
                    if ui.add(new).on_hover_text(tr(Start::Empty.hint())).clicked() {
                        choice = Some(Choice::Start(Start::Empty));
                    }
                    ui.label(egui::RichText::new(tr(Start::Empty.hint())).small().weak());
                    ui.add_space(14.);
                    ui.label(workspace::section(tr("From a template")));
                    ui.add_space(6.);
                    egui::Grid::new("home.templates")
                        .spacing(egui::vec2(10., 10.))
                        .show(ui, |ui| {
                            for start in Start::TEMPLATES {
                                if tile(ui, start, tr(start.label()), tr(start.hint())) {
                                    choice = Some(Choice::Start(start));
                                }
                            }
                        });
                    ui.add_space(8.);
                    if tile_link(
                        ui,
                        tr("More templates…"),
                        tr("Browse every bundled template"),
                    ) {
                        choice = Some(Choice::Templates);
                    }
                    let ui = &mut columns[1];
                    ui.label(workspace::section(tr("Recent projects")));
                    ui.add_space(6.);
                    egui::Frame::new()
                        .fill(ui.visuals().extreme_bg_color)
                        .corner_radius(6.)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_min_height(250.);
                            if recent.is_empty() {
                                ui.add_space(96.);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new(tr("No recent projects yet")).weak(),
                                    );
                                });
                            }
                            egui::ScrollArea::vertical()
                                .max_height(250.)
                                .show(ui, |ui| {
                                    for path in &recent {
                                        let name = path
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                        let name = name
                                            .strip_suffix(".json")
                                            .and_then(|n| {
                                                n.strip_suffix(".clothcraft")
                                                    .or(n.strip_suffix(".fashioncraft"))
                                                    .or(Some(n))
                                            })
                                            .unwrap_or(&name)
                                            .to_owned();
                                        let folder =
                                            path.parent().map(short_folder).unwrap_or_default();
                                        let row = ui.add(
                                            egui::Button::new(egui::RichText::new(name).strong())
                                                .right_text(egui::RichText::new("›").weak())
                                                .frame(false)
                                                .min_size(egui::vec2(ui.available_width(), 24.)),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&folder).small().weak(),
                                            )
                                            .truncate(),
                                        );
                                        ui.add_space(4.);
                                        if row.on_hover_text(path.display().to_string()).clicked() {
                                            choice = Some(Choice::Recent(path.clone()));
                                        }
                                    }
                                });
                        });
                    ui.add_space(8.);
                    if ui
                        .add(
                            egui::Button::new(tr("Open…"))
                                .min_size(egui::vec2(ui.available_width(), 30.)),
                        )
                        .clicked()
                    {
                        choice = Some(Choice::Open);
                    }
                });
                ui.add_space(12.);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut skip, tr("Don't show at startup"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(tr("Window › Home shows this again"))
                                .small()
                                .weak(),
                        );
                    });
                });
            });
        self.prefs.home_at_startup = !skip;
        if modal.should_close() {
            self.home.open = false;
        }
        if let Some(choice) = choice {
            self.home.open = false;
            match choice {
                Choice::Start(start) => self.start_from_home(ctx, start),
                Choice::Templates => self.open_template_browser(ctx),
                Choice::Open => self.open_document(ctx, false),
                Choice::Recent(path) => {
                    if self.can_replace_document(ctx) {
                        self.open_document_path(path, false);
                    }
                }
            }
        }
    }

    /// Replaces the document with a new one started from `start`.
    pub(super) fn start_from_home(&mut self, ctx: &egui::Context, start: Start) {
        if !self.can_replace_document(ctx) {
            return;
        }
        // Skirts and trousers are drafted from the avatar's own measurements when it has them.
        let drafted = match start {
            Start::Skirt => Some("skirt"),
            Start::Trousers => Some("trousers"),
            _ => None,
        }
        .is_some_and(|kind| {
            self.agent_draft(kind, true, &Default::default(), false)
                .is_ok()
        });
        let loaded = drafted
            || match project(start, &self.project) {
                Ok(project) => self.load(project),
                Err(error) => {
                    self.status = error;
                    false
                }
            };
        if loaded {
            self.begin_new_document();
            self.status = i18n::tr(
                self.prefs.language,
                match start {
                    Start::Empty => NEW_EMPTY,
                    _ => "New arrangement · run to assemble the seams",
                },
            )
            .into();
        }
    }

    /// File › New: an empty project on the current avatar.
    pub(super) fn new_empty_document(&mut self, ctx: &egui::Context) {
        self.start_from_home(ctx, Start::Empty);
    }

    /// Puts an opened or saved project first in the recent list.
    pub(super) fn remember_recent(&mut self, path: &Path) {
        remember(&mut self.prefs.recent_projects, path);
        self.prefs_dirty = true;
    }
}

/// The card's text in Spanish.
pub(super) fn spanish(text: &str) -> Option<&'static str> {
    Some(match text {
        "Home" => "Inicio",
        "Home screen" => "Pantalla de inicio",
        "Design, sew and drape garments in 3D" => "Diseña, cose y prueba prendas en 3D",
        "Begin" => "Empezar",
        "New empty project" => "Proyecto vacío nuevo",
        "The avatar alone · draw or import your first piece" => {
            "Solo el avatar · dibuja o importa tu primera pieza"
        }
        "From a template" => "Desde una plantilla",
        "Skirt" => "Falda",
        "Trousers" => "Pantalón",
        "The two-piece example, placed on the avatar" => {
            "El ejemplo de dos piezas, colocado sobre el avatar"
        }
        "Drafted from the avatar's measurements" => "Trazada con las medidas del avatar",
        "More templates…" => "Más plantillas…",
        "Browse every bundled template" => "Ver todas las plantillas incluidas",
        "Recent projects" => "Proyectos recientes",
        "No recent projects yet" => "Aún no hay proyectos recientes",
        "Open…" => "Abrir…",
        "Don't show at startup" => "No mostrar al iniciar",
        "Window › Home shows this again" => "Ventana › Inicio la vuelve a mostrar",
        NOTHING_TO_SIMULATE => {
            "Aún no hay nada que simular · dibuja una pieza o elige una plantilla"
        }
        NEW_EMPTY => "Proyecto nuevo · dibuja una pieza o elige una plantilla",
        NOTHING_TO_EXPORT => "Aún no hay nada que exportar: el proyecto no tiene piezas",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mannequin() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/makehuman/mannequin.obj")
    }

    /// Each choice starts the project it names, on the current avatar.
    #[test]
    fn home_choices_start_the_projects_they_name() {
        let mut current = FittingProject::basic(GarmentKind::Tshirt, 2, mannequin());
        current.scene_objects = Vec::new();
        for start in [
            Start::Empty,
            Start::Tshirt,
            Start::Dress,
            Start::Skirt,
            Start::Trousers,
        ] {
            let p = project(start, &current).unwrap();
            assert_eq!(p.avatar_path, current.avatar_path, "{start:?}");
            assert!(p.state.is_none() && !p.avatar_removed, "{start:?}");
            let scene = p.compile().unwrap();
            let pieces = p.template.panels.len();
            match start {
                Start::Empty => {
                    assert_eq!(pieces, 0);
                    assert!(p.template.seams.is_empty() && scene.garment.mesh.uv.is_empty());
                }
                Start::Tshirt => {
                    assert_eq!(p.template.name, presets::basic(GarmentKind::Tshirt, 2).name)
                }
                Start::Dress => {
                    assert_eq!(p.template.name, presets::basic(GarmentKind::Dress, 2).name)
                }
                Start::Skirt | Start::Trousers => {
                    assert!(pieces >= 2 && !p.template.seams.is_empty(), "{start:?}");
                }
            }
            if start != Start::Empty {
                assert!(!scene.garment.mesh.uv.is_empty(), "{start:?}");
            }
        }
        assert_ne!(
            project(Start::Skirt, &current).unwrap().template.name,
            project(Start::Trousers, &current).unwrap().template.name
        );
    }

    /// The recent list has each project once, newest first, and keeps the last few.
    #[test]
    fn recent_projects_are_newest_first_and_bounded() {
        let mut recent = Vec::new();
        for k in 0..RECENT_MAX + 3 {
            remember(&mut recent, Path::new(&format!("/nowhere/p{k}.json")));
        }
        remember(&mut recent, Path::new("/nowhere/p5.json"));
        assert_eq!(recent.len(), RECENT_MAX);
        assert_eq!(recent[0], PathBuf::from("/nowhere/p5.json"));
        assert_eq!(recent.iter().filter(|p| p.ends_with("p5.json")).count(), 1);
        assert_eq!(
            recent[1],
            PathBuf::from(format!("/nowhere/p{}.json", RECENT_MAX + 2))
        );
    }

    /// Everything the card says has a Spanish version.
    #[test]
    fn the_card_speaks_spanish() {
        for start in [Start::Empty].into_iter().chain(Start::TEMPLATES) {
            assert!(
                i18n::tr(i18n::Language::Spanish, start.label()) != start.label(),
                "{start:?}"
            );
            assert!(spanish(start.hint()).is_some(), "{start:?}");
        }
        for text in [
            NOTHING_TO_SIMULATE,
            NOTHING_TO_EXPORT,
            NEW_EMPTY,
            "Home",
            "Don't show at startup",
            "Recent projects",
            "Open…",
        ] {
            assert!(spanish(text).is_some(), "{text}");
        }
    }
}
