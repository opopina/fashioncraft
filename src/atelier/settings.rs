//! User preferences: theme, board and simulation defaults.
//!
//! Preferences are persisted as JSON in the session output directory. A missing,
//! truncated or corrupt file never panics: the loader falls back to
//! [`Preferences::default`] and clamps hand-edited values into the ranges that
//! the board and `VbdSettings::validate` actually accept.
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Name of the preferences file inside the session output directory.
pub(super) const FILE_NAME: &str = "preferences.json";

/// The two workspace themes [`apply`] can install.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ThemeChoice {
    Light,
    Dark,
}

/// Board magnet step in millimetres.
pub(super) const SNAP_MM_MIN: f64 = 0.1;
pub(super) const SNAP_MM_MAX: f64 = 100.;
/// Physical time step in seconds, inside `VbdSettings::validate` (1e-6..=0.025).
pub(super) const DT_MIN: f64 = 1. / 240.;
pub(super) const DT_MAX: f64 = 0.025;
/// VBD iterations per step; the inspector slider offers the same range.
pub(super) const ITERATIONS_MIN: usize = 2;
pub(super) const ITERATIONS_MAX: usize = 128;

const DEFAULT_SNAP_MM: f64 = 10.;
const DEFAULT_DT: f64 = 1. / 60.;
const DEFAULT_ITERATIONS: usize = 16;

/// User preferences. Every field is read by the live atelier; nothing is stored
/// for show only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub(super) workspace: super::layout::WorkspaceLayout,
    pub(super) viewport: crate::render::appearance::Appearance,
    /// Workspace theme installed at startup and whenever the switch changes.
    pub(super) theme: ThemeChoice,
    /// Board magnet step in millimetres; the 2D board snaps to this grid.
    pub(super) snap_mm: f64,
    /// Whether the 2D + 3D seam-link overlay starts enabled.
    pub(super) seam_links: bool,
    /// Whether the wireframe mesh overlay starts enabled.
    pub(super) wireframe: bool,
    /// What the 2D pattern board draws besides the pieces.
    pub(super) pattern_display: super::board_display::PatternDisplay,
    /// What the 3D view draws: avatar, garment, colours and the strain-map range.
    pub(super) garment_display: super::garment_display::GarmentDisplay,
    /// Vertical field of view of the 3D camera, in degrees.
    pub(super) camera_fov_degrees: f32,
    /// Physical time step in seconds for the fixed-step worker.
    pub(super) dt: f64,
    /// VBD iterations per physical step.
    pub(super) iterations: usize,
    /// Which keys run tools and piece actions.
    pub(super) keymap: super::keymap::KeymapName,
    /// Folder the library window shows; none uses `library` in the session folder.
    pub(super) library_dir: Option<std::path::PathBuf>,
    /// Language of the menus and tool rows.
    pub(super) language: super::i18n::Language,
    /// Poses the user saved, by name, offered in the pose window on every project.
    pub(super) user_poses: std::collections::BTreeMap<String, fashioncraft::avatar_rig::Pose>,
    /// Which axes the placement gizmo follows (Settings › Gizmo).
    pub(super) gizmo_axes: super::gizmo::GizmoAxes,
    /// AI agents and the Assistant panel (Settings › AI agents).
    pub(super) ai: AiPrefs,
}

/// AI agents and the Assistant panel. API keys are not kept here: they go to the system's
/// keychain (see `assistant`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct AiPrefs {
    /// Whether MCP clients may work in this window through `fashioncraft-cli mcp`. Off until
    /// the person turns it on.
    pub(super) agents_allowed: bool,
    /// The Assistant panel's provider: anthropic, openai, gemini or local.
    pub(super) provider: String,
    /// The model it asks; empty uses the provider's default where there is one.
    pub(super) model: String,
    /// The provider's address; empty uses the provider's default.
    pub(super) base_url: String,
}

impl Default for AiPrefs {
    fn default() -> Self {
        Self {
            agents_allowed: false,
            provider: "anthropic".into(),
            model: String::new(),
            base_url: String::new(),
        }
    }
}

/// Most poses the user can keep, and the longest name one may have.
pub(super) const USER_POSES_MAX: usize = 200;
pub(super) const POSE_NAME_MAX: usize = 60;

impl Default for Preferences {
    fn default() -> Self {
        Self {
            workspace: Default::default(),
            viewport: Default::default(),
            theme: ThemeChoice::Dark,
            snap_mm: DEFAULT_SNAP_MM,
            seam_links: true,
            wireframe: false,
            pattern_display: Default::default(),
            garment_display: Default::default(),
            camera_fov_degrees: fashioncraft::render::DEFAULT_FOV_DEGREES,
            dt: DEFAULT_DT,
            iterations: DEFAULT_ITERATIONS,
            keymap: Default::default(),
            library_dir: None,
            language: Default::default(),
            user_poses: Default::default(),
            gizmo_axes: Default::default(),
            ai: Default::default(),
        }
    }
}

impl Preferences {
    /// Clamp values that a hand-edited or corrupt file could otherwise push
    /// outside the ranges the board and `VbdSettings::validate` accept.
    fn sanitize(&mut self) {
        self.workspace.sanitize();
        self.viewport.sanitize();
        self.garment_display.sanitize();
        let fov = fashioncraft::render::FOV_RANGE_DEGREES;
        self.camera_fov_degrees = if self.camera_fov_degrees.is_finite() {
            self.camera_fov_degrees.clamp(*fov.start(), *fov.end())
        } else {
            fashioncraft::render::DEFAULT_FOV_DEGREES
        };
        self.snap_mm = finite_or(self.snap_mm, DEFAULT_SNAP_MM).clamp(SNAP_MM_MIN, SNAP_MM_MAX);
        self.dt = finite_or(self.dt, DEFAULT_DT).clamp(DT_MIN, DT_MAX);
        self.iterations = self.iterations.clamp(ITERATIONS_MIN, ITERATIONS_MAX);
        // Poses a hand-edited file broke, or nameless ones, are dropped.
        self.user_poses.retain(|name, pose| {
            !name.trim().is_empty()
                && name.chars().count() <= POSE_NAME_MAX
                && pose.validate().is_ok()
        });
        while self.user_poses.len() > USER_POSES_MAX {
            let last = self.user_poses.keys().next_back().cloned();
            if let Some(last) = last {
                self.user_poses.remove(&last);
            }
        }
    }
}

/// Replace a non-finite value before clamping; `f64::clamp` propagates NaN.
fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

/// Read preferences from `path`. A missing, unreadable, truncated or corrupt
/// file yields the defaults; no error is propagated and nothing panics.
pub(super) fn load(path: &Path) -> Preferences {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Preferences::default();
    };
    let mut prefs: Preferences = serde_json::from_str(&text).unwrap_or_default();
    prefs.sanitize();
    prefs
}

/// Write preferences to `path`, creating the parent directory when needed.
pub(super) fn save(path: &Path, prefs: &Preferences) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(prefs).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

/// Draw the Preferences window. `changed` is set when any value was edited so
/// the caller can re-apply the theme and re-save the file.
///
/// The window is opened from the menu bar by the caller; it stays resizable and
/// shows the current values, including a Restore defaults button.
pub(super) fn window(
    ctx: &egui::Context,
    open: &mut bool,
    prefs: &mut Preferences,
    changed: &mut bool,
) {
    if !*open {
        return;
    }
    let mut edited = false;
    egui::Window::new("Preferences")
        .open(open)
        .resizable(true)
        .default_width(340.)
        .show(ctx, |ui| {
            super::workspace::group_title(ui, "Appearance");
            ui.horizontal(|ui| {
                ui.label("Theme");
                edited |= ui
                    .selectable_value(&mut prefs.theme, ThemeChoice::Light, "Light")
                    .changed();
                edited |= ui
                    .selectable_value(&mut prefs.theme, ThemeChoice::Dark, "Dark")
                    .changed();
            });
            ui.small("Viewport appearance: use the gear above the 3D view. Pane sizes: drag a divider; double-click to reset.");
            ui.horizontal(|ui| {
                ui.label("Language");
                for language in super::i18n::Language::ALL {
                    edited |= ui
                        .selectable_value(&mut prefs.language, language, language.name())
                        .changed();
                }
            })
            .response
            .on_hover_text("Language of the menus and the tool rows of the views");
            ui.horizontal(|ui| {
                ui.label("Keyboard map");
                for map in super::keymap::KeymapName::ALL {
                    edited |= ui
                        .selectable_value(&mut prefs.keymap, map, map.label())
                        .changed();
                }
            })
            .response
            .on_hover_text(
                "Compatible: the single keys garment designers know (S rectangle, H polygon, \
                 Cmd+K freeze…). FashionCraft classic: this editor's earlier keys (G, R, M, K, D)",
            );

            ui.separator();
            super::workspace::group_title(ui, "Board");
            ui.horizontal(|ui| {
                ui.label("Snap step (mm)");
                edited |= ui
                    .add(
                        egui::DragValue::new(&mut prefs.snap_mm)
                            .speed(0.1)
                            .range(SNAP_MM_MIN..=SNAP_MM_MAX),
                    )
                    .changed();
            });

            ui.separator();
            super::workspace::group_title(ui, "Overlays");
            edited |= ui
                .checkbox(&mut prefs.seam_links, "Seam links enabled at startup")
                .changed();
            edited |= ui
                .checkbox(&mut prefs.wireframe, "Wireframe enabled at startup")
                .changed();

            ui.separator();
            super::workspace::group_title(ui, "Simulation defaults for next launch");
            ui.horizontal(|ui| {
                ui.label("Time step (s)");
                edited |= ui
                    .add(
                        egui::DragValue::new(&mut prefs.dt)
                            .speed(0.0005)
                            .range(DT_MIN..=DT_MAX),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Iterations");
                edited |= ui
                    .add(
                        egui::DragValue::new(&mut prefs.iterations)
                            .speed(1.)
                            .range(ITERATIONS_MIN..=ITERATIONS_MAX),
                    )
                    .changed();
            });
            ui.separator();
            let tr = |text| super::i18n::tr(prefs.language, text);
            super::workspace::group_title(ui, tr("AI agents"));
            let allow = ui.horizontal(|ui| {
                let allow = ui.checkbox(
                    &mut prefs.ai.agents_allowed,
                    tr("Let AI agents use FashionCraft"),
                );
                super::workspace::info_tip(
                    ui,
                    tr("MCP clients such as Claude Desktop, Claude Code, Cursor or ArtCraft's \
                        gateway work in this window through fashioncraft-cli mcp. Off, nothing \
                        listens."),
                );
                allow
            });
            edited |= allow.inner.changed();
            ui.label(
                egui::RichText::new(tr(
                    "They work in the open window, with undo, and save and export only into \
                     Documents/FashionCraft.",
                ))
                .small()
                .weak(),
            );
            let entry = mcp_entry();
            // The client entry as a code card with its copy button, as developer docs show it.
            egui::Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(8.)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr("MCP client entry"))
                                .small()
                                .strong()
                                .color(super::workspace::accent(ui.visuals())),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(tr("Copy")).clicked() {
                                ui.ctx().copy_text(entry.clone());
                            }
                        });
                    });
                    ui.add(
                        egui::Label::new(egui::RichText::new(&entry).monospace().small()).wrap(),
                    );
                });
            ui.label(
                egui::RichText::new(tr(
                    "The Assistant (Window › Assistant) works in this window with Claude, ChatGPT, \
                     Gemini or a local model; set its connection in its own window.",
                ))
                .small()
                .weak(),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Restore defaults").clicked() {
                    *prefs = Preferences::default();
                    edited = true;
                }
                ui.small(format!("Stored as {FILE_NAME} in the session folder"));
            });
        });
    *changed |= edited;
}

/// The `mcpServers` entry that starts this installation's `fashioncraft-cli mcp`, for Claude
/// Desktop, Cursor, LM Studio and most other MCP clients.
fn mcp_entry() -> String {
    // An AppImage runs its fashioncraft-cli as `<AppImage> cli` (see scripts/package-linux.sh).
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {"FashionCraft": {
                "command": appimage.to_string_lossy(), "args": ["cli", "mcp"]
            }}
        }))
        .unwrap_or_default();
    }
    let name = if cfg!(windows) {
        "fashioncraft-cli.exe"
    } else {
        "fashioncraft-cli"
    };
    let cli = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(name)))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| name.into());
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {"FashionCraft": {"command": cli, "args": ["mcp"]}}
    }))
    .unwrap_or_default()
}

/// Install the workspace chrome for the selected theme, and select that theme so
/// widgets that branch on `dark_mode` agree with the installed palette.
pub(super) fn apply(ctx: &egui::Context, prefs: &Preferences) {
    ctx.set_theme(match prefs.theme {
        ThemeChoice::Light => egui::Theme::Light,
        ThemeChoice::Dark => egui::Theme::Dark,
    });
    match prefs.theme {
        ThemeChoice::Light => super::workspace::style(ctx),
        ThemeChoice::Dark => super::workspace::dark_style(ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Preferences {
        Preferences {
            theme: ThemeChoice::Dark,
            snap_mm: 2.5,
            seam_links: false,
            wireframe: true,
            pattern_display: super::super::board_display::PatternDisplay {
                names: false,
                lengths: true,
                grid: false,
                silhouette: false,
                rulers: true,
                guides: false,
                ruler_unit: super::super::board_guides::RulerUnit::Inch,
                measurements: false,
                white_sheet: true,
            },
            dt: 1.0 / 120.0,
            iterations: 24,
            user_poses: [(
                "Waving".to_string(),
                fashioncraft::avatar_rig::preset("T-pose").unwrap(),
            )]
            .into(),
            ..Default::default()
        }
    }

    #[test]
    fn broken_or_nameless_user_poses_are_dropped_on_load() {
        let path = scratch_path("user-poses");
        let mut prefs = sample();
        let mut bad = fashioncraft::avatar_rig::Pose::default();
        bad.0.insert("no-such-bone".into(), [10., 0., 0.]);
        prefs.user_poses.insert("Broken".into(), bad);
        prefs.user_poses.insert("  ".into(), Default::default());
        save(&path, &prefs).unwrap();
        let loaded = load(&path);
        let names: Vec<_> = loaded.user_poses.keys().cloned().collect();
        assert_eq!(names, ["Waving"]);
        assert_eq!(loaded.user_poses["Waving"], sample().user_poses["Waving"]);
        let _ = std::fs::remove_file(path);
    }

    /// Scratch file under `target/`, which the test sandbox can always write.
    fn scratch_path(name: &str) -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/preferences-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{name}-{}.json", std::process::id()))
    }

    #[test]
    fn serde_round_trip_preserves_every_field() {
        let prefs = sample();
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<Preferences>(&json).unwrap(), prefs);
    }

    #[test]
    fn every_field_survives_a_save_load_cycle() {
        let path = scratch_path("cycle");
        let _ = std::fs::remove_file(&path);
        let prefs = sample();
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path), prefs);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults_without_panicking() {
        let path = scratch_path("corrupt");
        std::fs::write(&path, "{ this is not json").unwrap();
        assert_eq!(load(&path), Preferences::default());
        std::fs::write(&path, "[]").unwrap();
        assert_eq!(load(&path), Preferences::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let path = scratch_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), Preferences::default());
    }

    #[test]
    fn partial_file_keeps_stated_fields_and_defaults_the_rest() {
        let path = scratch_path("partial");
        std::fs::write(&path, "{\"theme\":\"dark\",\"iterations\":24}").unwrap();
        let prefs = load(&path);
        assert_eq!(prefs.theme, ThemeChoice::Dark);
        assert_eq!(prefs.iterations, 24);
        assert_eq!(prefs.snap_mm, Preferences::default().snap_mm);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn out_of_range_values_are_clamped_to_solver_and_board_ranges() {
        let path = scratch_path("clamped");
        std::fs::write(
            &path,
            "{\"snap_mm\":-5.0,\"dt\":9.0,\"iterations\":9999,\"mesh_mm\":1.0e9}",
        )
        .unwrap();
        let prefs = load(&path);
        assert_eq!(prefs.snap_mm, SNAP_MM_MIN);
        assert_eq!(prefs.dt, DT_MAX);
        assert_eq!(prefs.iterations, ITERATIONS_MAX);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn apply_installs_a_real_dark_style_and_the_reviewed_light_style() {
        let ctx = egui::Context::default();
        let light = Preferences {
            theme: ThemeChoice::Light,
            ..Preferences::default()
        };
        let dark = Preferences {
            theme: ThemeChoice::Dark,
            ..Preferences::default()
        };
        let reviewed = egui::Color32::from_rgb(0xF2, 0xF1, 0xEE);

        apply(&ctx, &light);
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            reviewed
        );
        assert_eq!(ctx.style_of(egui::Theme::Dark).visuals.panel_fill, reviewed);
        assert!(!ctx.style_of(egui::Theme::Light).visuals.dark_mode);

        apply(&ctx, &dark);
        let dark_fill = ctx.style_of(egui::Theme::Dark).visuals.panel_fill;
        assert_ne!(dark_fill, reviewed, "dark must not reuse the light fill");
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            dark_fill
        );
        assert!(ctx.style_of(egui::Theme::Dark).visuals.dark_mode);

        // Switching back must restore the reviewed light palette exactly.
        apply(&ctx, &light);
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            reviewed
        );
        assert!(!ctx.style_of(egui::Theme::Light).visuals.dark_mode);
    }
}
