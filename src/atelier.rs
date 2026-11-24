//! Garment workspace: library | 3D | 2D patterns | properties.
use crate::render::{Camera, DrawModel, Viewport};
use eframe::egui::{self, Color32, Stroke, Vec2};
use fashioncraft::{
    fitting::{FittingProject, FittingScene},
    garment::{export, presets::GarmentKind},
    picking,
    vbd::{Grab, VbdMetrics, VbdSettings},
};
use std::{path::PathBuf, sync::mpsc, time::Duration};
#[path = "atelier/activation_ui.rs"]
mod activation_ui;
#[path = "atelier/agent_base_tools.rs"]
mod agent_base_tools;
#[path = "atelier/agent_hub.rs"]
mod agent_hub;
#[path = "atelier/agent_tools.rs"]
mod agent_tools;
#[path = "atelier/animation_export.rs"]
mod animation_export;
#[path = "atelier/animation_ui.rs"]
mod animation_ui;
#[path = "atelier/arrange_ui.rs"]
mod arrange_ui;
#[path = "atelier/arrangement_ui.rs"]
mod arrangement_ui;
#[path = "atelier/assistant_eval.rs"]
mod assistant_eval;
#[path = "atelier/assistant_keys.rs"]
mod assistant_keys;
#[path = "atelier/assistant_ui.rs"]
mod assistant_ui;
#[path = "atelier/authoring_smoke.rs"]
mod authoring_smoke;
#[path = "atelier/authoring_ui.rs"]
mod authoring_ui;
#[path = "atelier/avatar_library_ui.rs"]
mod avatar_library_ui;
#[path = "atelier/avatar_maker_ui.rs"]
mod avatar_maker_ui;
#[path = "atelier/avatar_shape_ui.rs"]
mod avatar_shape_ui;
#[path = "atelier/board.rs"]
mod board;
#[path = "atelier/board_display.rs"]
mod board_display;
#[path = "atelier/board_guides.rs"]
mod board_guides;
#[path = "atelier/board_locks.rs"]
mod board_locks;
#[path = "atelier/board_measure.rs"]
mod board_measure;
#[path = "atelier/board_media.rs"]
mod board_media;
#[path = "atelier/camera_props_ui.rs"]
mod camera_props_ui;
#[path = "atelier/camera_views.rs"]
mod camera_views;
#[path = "atelier/chrome.rs"]
mod chrome;
#[path = "atelier/command_palette.rs"]
mod command_palette;
#[path = "atelier/commands.rs"]
mod commands;
#[path = "atelier/demo.rs"]
mod demo;
#[path = "atelier/demo_cargo.rs"]
mod demo_cargo;
#[path = "atelier/demo_effects.rs"]
mod demo_effects;
#[path = "atelier/demo_grading.rs"]
mod demo_grading;
#[path = "atelier/demo_looks.rs"]
mod demo_looks;
#[path = "atelier/demo_outfit.rs"]
mod demo_outfit;
#[path = "atelier/demo_vest.rs"]
mod demo_vest;
#[path = "atelier/documents_ui.rs"]
mod documents_ui;
#[path = "atelier/drafting_ui.rs"]
mod drafting_ui;
#[path = "atelier/edges_3d.rs"]
mod edges_3d;
#[path = "atelier/environment_ui.rs"]
mod environment_ui;
#[path = "atelier/fabric_kit_ui.rs"]
mod fabric_kit_ui;
#[path = "atelier/fabrics_ui.rs"]
mod fabrics_ui;
#[path = "atelier/garment_display.rs"]
mod garment_display;
#[path = "atelier/garment_files.rs"]
mod garment_files;
#[path = "atelier/gizmo.rs"]
mod gizmo;
#[path = "atelier/grading_ui.rs"]
mod grading_ui;
#[path = "atelier/graphics_ui.rs"]
mod graphics_ui;
#[path = "atelier/history_ui.rs"]
mod history_ui;
#[path = "atelier/i18n.rs"]
mod i18n;
#[path = "atelier/icons.rs"]
mod icons;
#[path = "atelier/issue_ui.rs"]
mod issue_ui;
#[path = "atelier/keymap.rs"]
mod keymap;
#[path = "atelier/layers_ui.rs"]
mod layers_ui;
#[path = "atelier/layout.rs"]
mod layout;
#[path = "atelier/library_ui.rs"]
mod library_ui;
#[path = "atelier/line_springs_ui.rs"]
mod line_springs_ui;
#[path = "atelier/measure.rs"]
mod measure;
#[path = "atelier/measure3d_ui.rs"]
mod measure3d_ui;
#[path = "atelier/menus.rs"]
mod menus;
#[path = "atelier/mesh_select.rs"]
mod mesh_select;
#[path = "atelier/native_menu.rs"]
mod native_menu;
#[path = "atelier/object_browser_ui.rs"]
mod object_browser_ui;
#[path = "atelier/palette_ui.rs"]
mod palette_ui;
#[path = "atelier/perf.rs"]
mod perf;
#[path = "atelier/pin_tool.rs"]
mod pin_tool;
#[path = "atelier/pins_ui.rs"]
mod pins_ui;
#[path = "atelier/pleats_ui.rs"]
mod pleats_ui;
#[path = "atelier/pose_ui.rs"]
mod pose_ui;
#[path = "atelier/quick_actions.rs"]
mod quick_actions;
#[path = "atelier/radial.rs"]
mod radial;
#[path = "atelier/redrape_ui.rs"]
mod redrape_ui;
#[path = "atelier/render_ui.rs"]
mod render_ui;
#[path = "atelier/scene_ui.rs"]
mod scene_ui;
#[path = "atelier/script_ui.rs"]
mod script_ui;
#[path = "atelier/seam_break_ui.rs"]
mod seam_break_ui;
#[path = "atelier/seam_links.rs"]
mod seam_links;
#[path = "atelier/seams_ui.rs"]
mod seams_ui;
#[path = "atelier/segments_ui.rs"]
mod segments_ui;
#[path = "atelier/selection.rs"]
mod selection;
#[path = "atelier/settings.rs"]
mod settings;
#[path = "atelier/sewing_ui.rs"]
mod sewing_ui;
#[path = "atelier/shapes_ui.rs"]
mod shapes_ui;
#[path = "atelier/stage_settings.rs"]
mod stage_settings;
#[path = "atelier/steam_ui.rs"]
mod steam_ui;
#[path = "atelier/style_lines.rs"]
mod style_lines;
#[path = "atelier/texture_ui.rs"]
mod texture_ui;
#[path = "atelier/trace_pen.rs"]
mod trace_pen;
#[path = "atelier/trims_ui.rs"]
mod trims_ui;
#[cfg(test)]
#[path = "atelier/ui_preview.rs"]
mod ui_preview;
#[path = "atelier/uv_editor_ui.rs"]
mod uv_editor_ui;
#[path = "atelier/view_controls.rs"]
mod view_controls;
#[path = "atelier/window_tools.rs"]
mod window_tools;
#[path = "atelier/worker.rs"]
mod worker;
#[path = "atelier/workspace.rs"]
mod workspace;
#[path = "atelier/workspace_profiles.rs"]
mod workspace_profiles;
use authoring_ui::{AuthoringUi, Tool};
use worker::Work;

/// Export is not atomic across artifacts: an error may leave earlier files.
/// Keep the fallible operation separate from status text so dispatch can report
/// the real outcome and CPU tests can exercise disk failures without a renderer.
fn export_session(
    project: &FittingProject,
    scene: &FittingScene,
    settings: &VbdSettings,
    output: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output)?;
    export::write_artifacts(&project.template, &scene.garment, &scene.state, output)?;
    let mut p = project.clone();
    p.state = Some(scene.state.clone());
    p.simulation = Some(settings.clone());
    p.save(&output.join("fitting-project.json"))?;
    Ok(())
}

fn queue_work(sender: &mpsc::Sender<Work>, work: Work) -> Result<(), String> {
    sender
        .send(work)
        .map_err(|_| "The simulation worker is unavailable".to_owned())
}

fn camera() -> Camera {
    Camera {
        yaw: 0.10,
        pitch: 0.05,
        distance: 2.75,
        target: glam::Vec3::new(0., 0.95, 0.),
        ..Default::default()
    }
}
pub struct DocumentFiles {
    pub output: PathBuf,
    pub source: Option<PathBuf>,
    pub modified: bool,
}
pub fn run(
    project: FittingProject,
    scene: FittingScene,
    files: DocumentFiles,
    smoke: bool,
    drag_test: bool,
    perf_test: bool,
    authoring_test: bool,
) -> Result<(), String> {
    let output = files.output;
    let smoke_report = output.join("interaction.json");
    let authoring_report = output.join("authoring.json");
    let perf_report = output.join("ui-perf.json");
    let smoke_size = (smoke || drag_test || perf_test || authoring_test)
        .then(|| std::env::var("CLOTHCRAFT_SMOKE_SIZE").ok())
        .flatten()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
            ((1100..=3840).contains(&w) && (700..=2160).contains(&h))
                .then_some([w as f32, h as f32])
        });
    // macOS shows this as the running app's Dock icon, so it carries Apple's grid margin there;
    // elsewhere the tile fills the icon, as other apps' do.
    #[cfg(target_os = "macos")]
    let icon_png: &[u8] = include_bytes!("../assets/branding/fashioncraft-icon-macos.png");
    #[cfg(not(target_os = "macos"))]
    let icon_png: &[u8] = include_bytes!("../assets/branding/fashioncraft-icon.png");
    let icon = eframe::icon_data::from_png_bytes(icon_png)
        .map_err(|error| format!("Cannot decode the bundled app icon: {error}"))?;
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        let base = setup.device_descriptor.clone();
        setup.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut descriptor = base(adapter);
            descriptor.required_features |= adapter.features()
                & (eframe::wgpu::Features::TIMESTAMP_QUERY | eframe::wgpu::Features::SUBGROUP);
            descriptor
        });
    }
    let options = eframe::NativeOptions {
        wgpu_options,
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            // Wayland's app_id (X11's WM_CLASS): the name of the installed .desktop entry, so
            // launchers and docks match the window to its icon. It defaults to the title.
            .with_app_id("io.github.opopina.fashioncraft")
            // A size that fits a 13-inch laptop's screen (1440 × 900 points) with its menu bar
            // and title bar; eframe also clamps it to the monitor;
            // a demo opens at the size it records at, 16:9.
            .with_inner_size(
                smoke_size
                    .or_else(demo::window_size)
                    .unwrap_or([1440., 840.]),
            )
            // A demo stays in front: macOS stops drawing a covered window, which would stall it.
            .with_window_level(if demo::window_size().is_some() {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            })
            .with_min_inner_size([1100., 700.]),
        ..Default::default()
    };
    eframe::run_native(
        "FashionCraft",
        options,
        Box::new(move |cc| {
            let render = cc.wgpu_render_state.clone().ok_or("wgpu unavailable")?;
            let gpu_device = (render.device.clone(), render.queue.clone());
            // A software adapter (WARP on a Windows without a graphics driver, llvmpipe on Linux)
            // runs "GPU" compute on the CPU anyway, slowly, and WARP crashed during a demo on a
            // Windows VM: simulate on the CPU there. GPU compute stays a choice in the settings.
            // FASHIONCRAFT_GPU_COMPUTE=0 or 1 chooses it outright, to test or to work round a driver.
            let gpu_compute = match std::env::var("FASHIONCRAFT_GPU_COMPUTE").as_deref() {
                Ok("0") => false,
                Ok("1") => true,
                _ => render.adapter.get_info().device_type != eframe::wgpu::DeviceType::Cpu,
            };
            // Preferences choose the theme and the startup display defaults, so
            // they are loaded and applied before the first frame is drawn.
            // Applying them afterwards would show a flash of the wrong theme.
            let prefs = settings::load(&output.join(settings::FILE_NAME));
            settings::apply(&cc.egui_ctx, &prefs);
            // Copy the startup values out before `prefs` moves into the struct.
            let (wire, seam_links) = (prefs.wireframe, prefs.seam_links);
            let (dt, iterations) = (prefs.dt, prefs.iterations);
            let ui_lang = prefs.language;
            let simulation = project.simulation.clone().unwrap_or(VbdSettings {
                dt,
                iterations,
                friction: environment_ui::DEFAULT_FRICTION,
                ..Default::default()
            });
            let mut authoring = AuthoringUi {
                tool: if drag_test || perf_test {
                    Tool::Grab
                } else {
                    Tool::Edit
                },
                ..Default::default()
            };
            authoring.board.snap_mm = prefs.snap_mm;
            let camera_fov_degrees = prefs.camera_fov_degrees;
            let (tx, receive) = worker::start(render.device.clone(), render.queue.clone())?;
            let m = project
                .template
                .panels
                .iter()
                .find_map(|p| p.polygon.as_ref())
                .map(|p| p.meshing_settings())
                .unwrap_or_default();
            Ok(Box::new(Atelier {
                document: documents_ui::DocumentUi::new(&output, files.source, files.modified),
                issue: Default::default(),
                quick: Default::default(),
                authoring,
                prefs,
                settings_open: false,
                prefs_dirty: false,
                prefs_kept: demo::window_size().is_some(),
                inspector_tab: chrome::InspectorTab::Piece,
                authoring_smoke: authoring_test.then(authoring_smoke::Smoke::new),
                demo: demo::Demo::from_env(&output),
                // A demo and the smoke tests click the in-window menus.
                native_menu: (!(smoke || drag_test || perf_test || authoring_test)
                    && demo::window_size().is_none())
                .then(|| native_menu::NativeMenu::install(&cc.egui_ctx))
                .flatten(),
                menu_windows: Default::default(),
                demo_points: Default::default(),
                demo_points_before: Default::default(),
                window_checked: false,
                agent: agent_hub::AgentHub::new(output.clone()),
                assistant: Default::default(),
                mesh_mm: m.target_edge_m * 1000.,
                anisotropy: m.anisotropy,
                mesh_axis: m.axis_degrees,
                project,
                smoke_initial: if drag_test {
                    scene.state.x.clone()
                } else {
                    Vec::new()
                },
                scene,
                output,
                viewport: Viewport::new(render),
                palette: Default::default(),
                profiles: Default::default(),
                // The tools live in each view's row, as in desktop cloth tools; the rail stays
                // for the smoke tests and demos that click it, and in Window › Tool rail.
                tools_visible: smoke
                    || drag_test
                    || perf_test
                    || authoring_test
                    || demo::window_size().is_some(),
                camera: Camera {
                    fov_degrees: camera_fov_degrees,
                    ..camera()
                },
                send: tx,
                receive,
                generation: 0,
                pending: false,
                playing: perf_test,
                wire,
                heat: false,
                seams: false,
                seam_links,
                clean_view: false,
                gpu: gpu_compute,
                gpu_device,
                settings: simulation,
                metrics: None,
                cadence: Default::default(),
                simulation_error: None,
                status: i18n::tr(ui_lang, "Ready · Space to start the simulation").into(),
                grab: None,
                grab_radius_px: 30.,
                grab_strength: 1.,
                drag_plane: None,
                smoke: smoke || drag_test || perf_test || authoring_test,
                perf: perf_test.then(perf::PerfRun::new),
                drag_test,
                smoke_point: None,
                smoke_tick: 0,
                smoke_started: std::time::Instant::now(),
                smoke_grabbed: false,
                smoke_applied: false,
                smoke_captured: false,
                frames: 0,
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    if drag_test {
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(smoke_report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if report["passed"] != true {
            return Err(format!("UI drag smoke failed: {report}"));
        }
    }
    if authoring_test {
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(authoring_report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if report["passed"] != true {
            return Err(format!("Authoring smoke failed: {report}"));
        }
    }
    if perf_test {
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(perf_report).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if report["passed"] != true {
            return Err(format!("UI realtime check failed: {report}"));
        }
    }
    Ok(())
}
struct Atelier {
    profiles: workspace_profiles::ProfileUi,
    tools_visible: bool,
    /// The menus in macOS's menu bar; None keeps them in the window (other systems, demos
    /// and smoke tests).
    native_menu: Option<native_menu::NativeMenu>,
    /// Windows the macOS menu bar opens for entries the in-window menus show inline.
    menu_windows: native_menu::MenuWindows,
    palette: command_palette::Palette,
    cadence: perf::Cadence,
    simulation_error: Option<String>,
    issue: issue_ui::IssueUi,
    quick: quick_actions::QuickActions,
    document: documents_ui::DocumentUi,
    authoring: AuthoringUi,
    /// Loaded from `output/preferences.json` before the first frame and saved
    /// after a preference edit or workspace drag is complete.
    prefs: settings::Preferences,
    /// Whether the preferences window is currently open.
    settings_open: bool,
    prefs_dirty: bool,
    /// A demo window (`FASHIONCRAFT_DEMO`) changes the language, the layout and the display
    /// for its script, and never saves them: the person's own preferences stay as they left
    /// them, also after the demo ends in this window.
    prefs_kept: bool,
    inspector_tab: chrome::InspectorTab,
    authoring_smoke: Option<authoring_smoke::Smoke>,
    /// A scripted demo driving the interface (`FASHIONCRAFT_DEMO`).
    demo: Option<demo::Demo>,
    /// Controls that report themselves for the interface tests, this frame and the last, so
    /// a demo can aim at them too.
    demo_points: std::collections::HashMap<String, egui::Pos2>,
    demo_points_before: std::collections::HashMap<String, egui::Pos2>,
    /// Whether the window was checked against its screen at start (see `ui`).
    window_checked: bool,
    /// AI agents' tool calls and the loopback server for `fashioncraft-cli mcp`.
    agent: agent_hub::AgentHub,
    /// Window › Assistant.
    assistant: assistant_ui::AssistantUi,
    perf: Option<perf::PerfRun>,
    mesh_mm: f64,
    anisotropy: f64,
    mesh_axis: f64,
    project: FittingProject,
    scene: FittingScene,
    output: PathBuf,
    viewport: Viewport,
    camera: Camera,
    send: mpsc::Sender<Work>,
    receive: worker::Latest,
    generation: u64,
    pending: bool,
    playing: bool,
    wire: bool,
    heat: bool,
    seams: bool,
    seam_links: bool,
    clean_view: bool,
    gpu: bool,
    /// The window's wgpu device and queue, for GPU work outside the simulation worker (the
    /// animation recording thread).
    gpu_device: (eframe::wgpu::Device, eframe::wgpu::Queue),
    settings: VbdSettings,
    metrics: Option<VbdMetrics>,
    status: String,
    grab: Option<Grab>,
    grab_radius_px: f32,
    grab_strength: f64,
    drag_plane: Option<(glam::DVec3, glam::DVec3)>,
    smoke: bool,
    frames: u32,
    drag_test: bool,
    smoke_point: Option<egui::Pos2>,
    smoke_tick: u32,
    smoke_started: std::time::Instant,
    smoke_grabbed: bool,
    smoke_applied: bool,
    smoke_captured: bool,
    smoke_initial: Vec<glam::DVec3>,
}
/// Retired worker generations cannot overwrite an edit or a restored preview.
fn current_worker_result(done: Option<worker::Done>, generation: u64) -> Option<worker::Done> {
    done.filter(|d| d.generation == generation)
}
impl Atelier {
    /// The only live dispatch route for registry actions. All external
    /// adapters must recheck this snapshot rather than trusting cached UI
    /// availability from an earlier frame.
    fn command_context(&self) -> commands::CapabilityContext {
        commands::CapabilityContext {
            document_loaded: !self.project.template.panels.is_empty(),
            selected_panel: self.authoring.selected.as_ref().is_some_and(|id| {
                self.project
                    .template
                    .panels
                    .iter()
                    .any(|panel| &panel.id == id)
            }),
            // Optimistic availability only: a channel can disconnect and
            // storage can fail after this snapshot. Dispatch checks both.
            worker_ready: true,
            filesystem_ready: true,
            playing: self.playing,
            pending: self.pending,
        }
    }

    /// Success for simulation commands means a request was queued; worker
    /// completion and solver errors arrive asynchronously. Synchronous actions
    /// report their actual result, including compile and filesystem failures.
    fn dispatch_command(
        &mut self,
        id: commands::CommandId,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        use commands::CommandId as Id;
        commands::availability(id, self.command_context())
            .map_err(|reason| reason.message().to_owned())?;
        if self.project.avatar_removed
            && matches!(
                id,
                Id::MeasureBodyTape
                    | Id::MeasureBodyHeight
                    | Id::MeasureBodyGirth
                    | Id::OpenPose
                    | Id::RestPose
                    | Id::LoadPoseFile
                    | Id::SavePoseFile
                    | Id::PlayAnimation
                    | Id::RecordAnimation
                    | Id::OpenAnimation
                    | Id::ImportBvh
                    | Id::OpenAvatarEditor
                    | Id::ToggleArrangementPoints
                    | Id::ToggleSkeleton
                    | Id::TracePen3d
                    | Id::SaveAvatar
                    | Id::AttachPins
            )
        {
            return Err("There is no avatar: import one first (File › Import)".into());
        }
        match id {
            Id::DeleteAvatar => {
                if self.project.avatar_removed {
                    return Err("There is no avatar to delete".into());
                }
                if self.pending {
                    return Err("Wait for the current step to finish, then try again".into());
                }
                self.playing = false;
                let mut project = self.project.clone();
                project.avatar_removed = true;
                project.avatar_pose = None;
                project.avatar_animation = None;
                // Only the body changes: build the bodiless collider from a fresh compile.
                let mut probe = project.clone();
                probe.state = None;
                probe.mesh_signature = None;
                let scene = probe.compile()?;
                self.scene.collider = scene.collider;
                self.project = project;
                self.generation += 1;
                self.document.dirty = true;
                self.issue.invalidate();
                self.metrics = None;
                self.grab = None;
                self.status =
                    "Avatar deleted · the cloth hangs on its own; import an avatar to bring one back"
                        .into();
            }
            Id::ToggleSimulation => {
                if self.authoring.animation.recording() {
                    return Err("Recording · stop it from the timeline first".into());
                }
                if self.playing {
                    self.playing = false;
                } else if self.prepare_simulation() {
                    self.playing = true;
                } else {
                    return Err(self.status.clone());
                }
                if !self.submit(ctx, false) {
                    return Err(self.status.clone());
                }
            }
            Id::StepSimulation => {
                if !self.prepare_simulation() {
                    return Err(self.status.clone());
                }
                if !self.step(ctx) {
                    return Err(self.status.clone());
                }
            }
            Id::ResetSimulation => {
                if !self.reset() {
                    return Err(self.status.clone());
                }
            }
            Id::ExportSession => {
                if !self.export() {
                    return Err(self.status.clone());
                }
            }
            Id::SplitView => self.prefs.workspace.mode = layout::ViewMode::Split,
            Id::GarmentView => self.prefs.workspace.mode = layout::ViewMode::Garment,
            Id::PatternView => self.prefs.workspace.mode = layout::ViewMode::Patterns,
            Id::ToggleInspector => {
                self.prefs.workspace.inspector_open = !self.prefs.workspace.inspector_open;
            }
            Id::ResetLayout => self.prefs.workspace = Default::default(),
            Id::OpenPreferences => self.settings_open = true,
            Id::LoadTshirt => {
                if !self.preset(GarmentKind::Tshirt) {
                    return Err(self.status.clone());
                }
            }
            Id::LoadDress => {
                if !self.preset(GarmentKind::Dress) {
                    return Err(self.status.clone());
                }
            }
            Id::ToolFreehand => self.activate_freehand(),
            Id::ToolEdit
            | Id::ToolDraw
            | Id::ToolRectangle
            | Id::ToolEllipse
            | Id::ToolSector
            | Id::ToolInternalLine
            | Id::ToolSew
            | Id::ToolMove
            | Id::ToolRotate
            | Id::ToolGrab
            | Id::ToolMeasure
            | Id::ToolInspect
            | Id::ToolGraphic => {
                let (tool, rotate) =
                    command_palette::tool_for_command(id).ok_or("Unknown authoring tool")?;
                self.set_authoring_tool(tool, rotate);
            }
            Id::NewProject => self.new_document(ctx),
            Id::OpenProject => self.open_document(ctx, false),
            Id::SaveProject => {
                if !self.save_document(ctx, false) {
                    return Err(self.status.clone());
                }
            }
            Id::SaveProjectAs => {
                if !self.save_document(ctx, true) {
                    return Err(self.status.clone());
                }
            }
            Id::RecoverSnapshot => self.open_document(ctx, true),
            Id::ImportAvatar => self.open_avatar_import(ctx),
            Id::ImportDxf => self.open_pattern_import(),
            Id::ImportDxfAama => self.open_dxf_aama_import(),
            Id::ImportSvg => self.open_svg_import(),
            Id::OpenTemplates => self.open_template_browser(ctx),
            Id::Undo => self.undo_edit(false),
            Id::Redo => self.undo_edit(true),
            Id::ViewFront
            | Id::ViewBack
            | Id::ViewAvatarRight
            | Id::ViewAvatarLeft
            | Id::ViewTop
            | Id::ViewBottom
            | Id::ViewThreeQuarterRight
            | Id::ViewThreeQuarterLeft => {
                let view = match id {
                    Id::ViewFront => crate::render::PresetView::Front,
                    Id::ViewBack => crate::render::PresetView::Back,
                    Id::ViewAvatarRight => crate::render::PresetView::Left,
                    Id::ViewAvatarLeft => crate::render::PresetView::Right,
                    Id::ViewTop => crate::render::PresetView::Top,
                    Id::ViewBottom => crate::render::PresetView::Bottom,
                    Id::ViewThreeQuarterRight => crate::render::PresetView::ThreeQuarterLeft,
                    _ => crate::render::PresetView::ThreeQuarterRight,
                };
                self.camera.apply_preset(view);
            }
            Id::FrameView => self.frame_selection_or_all(),
            Id::ToggleAvatar => {
                self.prefs.garment_display.avatar = !self.prefs.garment_display.avatar;
            }
            Id::ToggleGarment => {
                self.prefs.garment_display.garment = !self.prefs.garment_display.garment;
            }
            Id::IsolateSelection => self.toggle_isolation(),
            Id::StrainMap | Id::StressMap | Id::PressureMap => {
                // One fit map at a time: picking another one switches to it.
                let display = &mut self.prefs.garment_display;
                let shown = self.heat.then_some(if display.pressure {
                    Id::PressureMap
                } else if display.strain_fabric {
                    Id::StressMap
                } else {
                    Id::StrainMap
                });
                if shown == Some(id) {
                    self.heat = false;
                } else {
                    self.heat = true;
                    display.pressure = id == Id::PressureMap;
                    if id != Id::PressureMap {
                        display.strain_fabric = id == Id::StressMap;
                    }
                }
            }
            Id::ImportPointCache => self.import_point_cache()?,
            Id::AddSpiral => {
                if !self.add_spiral() {
                    return Err(self.status.clone());
                }
            }
            Id::AddInternalRectangle => {
                self.add_internal_shape(
                    fashioncraft::garment::shapes2d::InternalShapeKind::Rectangle,
                )?;
            }
            Id::AddInternalEllipse => {
                self.add_internal_shape(
                    fashioncraft::garment::shapes2d::InternalShapeKind::Ellipse,
                )?;
            }
            Id::CheckSewingLength => self.check_sewing_lengths()?,
            Id::AddZipper | Id::AddTopstitch => {
                let (panel, edge) = self
                    .selected_segment()
                    .ok_or("Select an outline segment first")?;
                if !self.add_segment_trim(&panel, &edge, id == Id::AddZipper) {
                    return Err(self.status.clone());
                }
            }
            Id::ToggleSmartGuides => {
                self.authoring.board.smart_guides = !self.authoring.board.smart_guides;
            }
            Id::ToggleSnapToGrid => self.authoring.board.snap = !self.authoring.board.snap,
            Id::ToggleSnapToPattern => {
                self.authoring.board.snap_pattern = !self.authoring.board.snap_pattern;
            }
            Id::SaveGarment => self.save_garment()?,
            Id::AddGarment => self.add_garment()?,
            Id::SavePoseFile => self.save_pose_file()?,
            Id::LoadPoseFile => self.load_pose_file()?,
            Id::MeasureBodyTape
            | Id::MeasureBodyHeight
            | Id::MeasureBodyGirth
            | Id::MeasureGarmentLine
            | Id::MeasureGarmentGirth => {
                let kind = measure3d_ui::MeasureKind::of(id).ok_or("Unknown measurement")?;
                self.start_measuring(kind);
            }
            Id::StyleTexturedThick | Id::StyleTextured | Id::StylePlain | Id::StyleTranslucent => {
                use garment_display::GarmentStyle;
                self.prefs.garment_display.style = match id {
                    Id::StyleTexturedThick => GarmentStyle::TexturedThick,
                    Id::StyleTextured => GarmentStyle::Textured,
                    Id::StyleTranslucent => GarmentStyle::Translucent,
                    _ => GarmentStyle::Plain,
                };
            }
            Id::GizmoWorldAxes | Id::GizmoLocalAxes | Id::GizmoScreenAxes => {
                use gizmo::GizmoAxes;
                self.prefs.gizmo_axes = match id {
                    Id::GizmoLocalAxes => GizmoAxes::Local,
                    Id::GizmoScreenAxes => GizmoAxes::Screen,
                    _ => GizmoAxes::World,
                };
            }
            Id::CameraProperties => {
                self.authoring.camera_props.open = !self.authoring.camera_props.open;
            }
            Id::Pleats => self.open_pleats(false),
            Id::SinglePleat => self.open_pleats(true),
            Id::SelectAllSeams => {
                let count = self.project.template.seams.len();
                if count == 0 {
                    return Err("There are no seams".into());
                }
                self.authoring.selection.all_seams = true;
                self.authoring.selection.seam = None;
                self.prefs.workspace.inspector_open = true;
                self.status = format!(
                    "{} selected · switch them off, on or delete them under Existing seams",
                    garment_display::counted(count, "seam", "seams")
                );
            }
            Id::ActivateAllSeams => {
                let seams: Vec<String> = self
                    .project
                    .template
                    .inactive_seams
                    .iter()
                    .cloned()
                    .collect();
                if seams.is_empty() {
                    return Err("Every seam is on already".into());
                }
                self.edit(fashioncraft::authoring::Command::SetSeamsActive {
                    seams,
                    active: true,
                });
            }
            Id::Reset2dArrangement => self.reset_2d_arrangement(),
            Id::GraphicOn3dGarment => self.start_graphic_3d(),
            Id::Puckering => self.pucker_selected()?,
            Id::Steam => self.open_steam(),
            Id::MeasureFabric => self.open_fabric_kit(),
            Id::Piping => self.pipe_selected()?,
            Id::FrayEdge => self.fray_selected()?,
            Id::SubdivideMesh => self.subdivide_selection()?,
            Id::SuperimposeSide => self.superimpose_side()?,
            Id::EditTexture2d | Id::EditTexture3d => self.open_texture_edit(),
            Id::UvEditor => self.open_uv_editor(),
            Id::PackUvIslands => self.pack_uv_islands()?,
            Id::AutomaticUvLayout => self.automatic_uv_layout(),
            Id::OpenTrim => self.open_trim(),
            Id::AddAvatarObject => self.add_avatar_object(),
            Id::StyleLineEdit => self.start_style_lines(style_lines::StyleMode::Edit),
            Id::StyleLineMove => self.start_style_lines(style_lines::StyleMode::Move),
            Id::StyleLineScale => self.start_style_lines(style_lines::StyleMode::Scale),
            Id::StyleLineDraw => self.start_style_lines(style_lines::StyleMode::Draw),
            Id::ReDrape => self.redrape()?,
            Id::AutoFit => self.auto_fit()?,
            Id::RunPythonScript => self.open_python_script(),
            Id::PhotorealRender => self.open_photoreal_render(),
            Id::GradingTable => self.open_grading(),
            Id::PointsOfMeasure => self.open_points_of_measure(),
            Id::ToggleGradePoint => self.toggle_grade_point(),
            Id::GradingReview => self.toggle_grading_review(),
            Id::NextSize => self.step_shown_size(true),
            Id::PreviousSize => self.step_shown_size(false),
            Id::HighQualityView => {
                let display = &mut self.prefs.garment_display;
                display.high_quality = !display.high_quality;
                self.status = if display.high_quality {
                    "High-quality 3D view: drawn at twice the resolution".into()
                } else {
                    "Standard 3D view".into()
                };
            }
            Id::ConvertToQuads | Id::ConvertToTriangles => {
                let panels = self.selected_panels();
                let quads = id == Id::ConvertToQuads;
                if self.edit(fashioncraft::authoring::Command::SetMeshQuads { panels, quads }) {
                    self.status = if quads {
                        "Meshed as quads along the grain".into()
                    } else {
                        "Meshed as triangles".into()
                    };
                }
            }
            Id::SelectMeshBrush => self.start_mesh_select(mesh_select::SelectShape::Brush),
            Id::SelectMeshBox => self.start_mesh_select(mesh_select::SelectShape::Box),
            Id::SelectMeshLasso => self.start_mesh_select(mesh_select::SelectShape::Lasso),
            Id::TearSeam => self.tear_chosen_seam()?,
            Id::EditTears => self.edit_tears()?,
            Id::TraceShape => self.trace_selected_shape()?,
            Id::CutAndExtend => self.cut_and_extend_selected()?,
            Id::PleatSewing => {
                let (panel, line) = self
                    .selected_internal_line()
                    .ok_or("Select a line of a set of pleats on the board first")?;
                let group = self
                    .project
                    .template
                    .panels
                    .iter()
                    .find(|p| p.id == panel)
                    .and_then(|p| p.polygon.as_ref())
                    .and_then(|poly| poly.lines.iter().find(|l| l.id == line))
                    .and_then(|l| l.group.clone())
                    .ok_or("Select a line of a set of pleats on the board first")?;
                if self.edit(fashioncraft::authoring::Command::TackPleats { panel, group }) {
                    self.status = "Pleats tacked shut at their segment".into();
                }
            }
            Id::FoldArrangement => {
                let (panel, line) = self
                    .selected_internal_line()
                    .ok_or("Select an internal line on the board first")?;
                self.fold_arrange(&panel, &line, false);
            }
            Id::FreeSewing => self.sewing_mode(false, true)?,
            Id::ChainSegmentSewing => self.sewing_mode(true, false)?,
            Id::ChainFreeSewing => self.sewing_mode(true, true)?,
            Id::ShowPieceNames => {
                let display = &mut self.prefs.garment_display;
                display.piece_names = !display.piece_names;
            }
            Id::SmoothCurve => self.smooth_selected_points()?,
            Id::AddTape => self.tape_selected_line()?,
            Id::AutoSewing => self.auto_sew()?,
            Id::SeamTaping => self.finish_selected_seam(true)?,
            Id::SeamlineTopstitch => self.finish_selected_seam(false)?,
            Id::FreeTopstitch => self.topstitch_selected_line()?,
            Id::SplitSegment => self.split_selected_segment()?,
            Id::ShowInternalLines => {
                let display = &mut self.prefs.garment_display;
                display.internal_lines = !display.internal_lines;
            }
            Id::RefreshTextures => {
                self.authoring.graphic_images = Default::default();
                self.status = "Textures and graphics reloaded from their files".into();
            }
            Id::SaveAvatar => self.save_avatar()?,
            Id::OpenUserManual | Id::OpenWhatsNew => {
                let page = if id == Id::OpenUserManual {
                    "#readme"
                } else {
                    "/commits/main"
                };
                ctx.open_url(egui::OpenUrl::new_tab(format!(
                    "{}{page}",
                    env!("CARGO_PKG_REPOSITORY")
                )));
            }
            Id::ShowPins => {
                let display = &mut self.prefs.garment_display;
                display.pins = !display.pins;
            }
            Id::ShowTrims => {
                let display = &mut self.prefs.garment_display;
                display.trims = !display.trims;
            }
            Id::ShowGraphics => {
                let display = &mut self.prefs.garment_display;
                display.graphics = !display.graphics;
            }
            Id::HighResolution | Id::LowResolution => {
                self.mesh_mm = if id == Id::HighResolution {
                    menus::HIGH_RESOLUTION_MM
                } else {
                    menus::LOW_RESOLUTION_MM
                };
                self.remesh();
                self.status = format!(
                    "Garment remeshed at {:.0} mm: {} vertices · the drape starts again from the arrangement",
                    self.mesh_mm,
                    self.scene.state.x.len()
                );
            }
            Id::PinBox | Id::PinLasso => {
                self.set_authoring_tool(authoring_ui::Tool::Grab, false);
                let pins = &mut self.authoring.pin_tool;
                pins.enabled = true;
                pins.tack = false;
                pins.lasso = id == Id::PinLasso;
                self.status = if pins.lasso {
                    "Pin: draw a lasso around the cloth to pin · Cmd+lasso removes pins"
                } else {
                    "Pin: drag a box over the cloth to pin · Cmd+drag removes pins"
                }
                .into();
            }
            Id::SimulationProperties => {
                self.prefs.workspace.inspector_open = true;
                self.inspector_tab = chrome::InspectorTab::Simulation;
            }
            Id::TechnicalView => {
                self.prefs.garment_display.schematic = !self.prefs.garment_display.schematic;
            }
            Id::ToggleLibrary => self.authoring.library.open = !self.authoring.library.open,
            Id::OpenPose => self.authoring.pose.open = !self.authoring.pose.open,
            Id::OpenSceneObjects => self.authoring.scene.open = !self.authoring.scene.open,
            Id::OpenAnimation => {
                self.authoring.animation.open = !self.authoring.animation.open;
            }
            Id::PlayAnimation => self.toggle_animation_preview()?,
            Id::RecordAnimation => {
                if self.authoring.animation.recording() {
                    self.stop_animation_recording();
                } else {
                    self.authoring.animation.open = true;
                    self.start_animation_recording(ctx)?;
                }
            }
            Id::DeleteAnimation => self.delete_animation()?,
            Id::ExportPointCache => self.export_point_cache()?,
            Id::ExportObjSequence => self.export_obj_sequence()?,
            Id::CaptureView3d => self.capture_view3d()?,
            Id::SnapshotPatterns => self.open_print(),
            Id::TracePen3d => self.open_trace_pen(),
            Id::CaptureVideo => self.capture_video()?,
            Id::ExportGltf => self.export_gltf()?,
            Id::ExportDxfAama => self.open_dxf_aama_export(),
            Id::ExportObj => self.open_obj_export(false)?,
            Id::ExportObjSelected => self.open_obj_export(true)?,
            Id::ExportMdd => self.export_mdd(false)?,
            Id::ExportMddZUp => self.export_mdd(true)?,
            Id::ExportUsd => self.export_usd()?,
            Id::ExportFbx => self.export_fbx()?,
            Id::ExportAlembic => self.export_alembic()?,
            Id::DraftTshirt => self.authoring.shirt_draft.open = !self.authoring.shirt_draft.open,
            Id::ToggleHistory => self.authoring.history_ui.open = !self.authoring.history_ui.open,
            Id::ToggleAssistant => self.assistant.open = !self.assistant.open,
            Id::DraftSkirt => self.authoring.skirt_draft.open = !self.authoring.skirt_draft.open,
            Id::ImportBvh => self.import_bvh()?,
            Id::AttachPins | Id::DetachPins => {
                let attached = matches!(id, Id::AttachPins);
                if self.edit(fashioncraft::authoring::Command::AttachPins { attached }) {
                    self.status = if attached {
                        "Pins attached to the avatar · they follow it while an animation records"
                    } else {
                        "Pins detached from the avatar"
                    }
                    .into();
                }
            }
            Id::DraftTrousers => {
                self.authoring.trousers_draft.open = !self.authoring.trousers_draft.open;
            }
            Id::ToggleObjectBrowser => {
                self.authoring.object_browser.open = !self.authoring.object_browser.open;
            }
            Id::OpenAvatarEditor => {
                self.authoring.avatar_shape.open = !self.authoring.avatar_shape.open;
            }
            Id::CreateMakeHumanAvatar => {
                self.authoring.avatar_maker.open = !self.authoring.avatar_maker.open;
            }
            Id::OpenAvatarLibrary => {
                self.authoring.avatar_library.open = !self.authoring.avatar_library.open;
            }
            Id::ToggleArrangementPoints => {
                self.authoring.arrangement.points = !self.authoring.arrangement.points;
            }
            Id::ClearSceneObjects => {
                if self.project.scene_objects.is_empty() {
                    return Err("The scene has no objects".into());
                }
                if !self.apply_scene_objects(vec![]) {
                    return Err(self.status.clone());
                }
            }
            Id::ToggleSkeleton => {
                self.authoring.pose.skeleton = !self.authoring.pose.skeleton;
            }
            Id::RestPose => {
                let mut pose = self
                    .project
                    .avatar_pose
                    .clone()
                    .ok_or("The avatar is already at rest")?;
                pose.pose = Default::default();
                if !self.apply_pose(pose) {
                    return Err(self.status.clone());
                }
            }
            Id::AddDiamondDart => {
                let panel = self
                    .authoring
                    .selected
                    .clone()
                    .ok_or("Select a panel first")?;
                let center = self
                    .piece_centre(&panel)
                    .ok_or("The piece has no mesh yet")?;
                if !self.edit(fashioncraft::authoring::Command::DiamondDart {
                    panel,
                    center,
                    width: 0.03,
                    height: 0.15,
                }) {
                    return Err(self.status.clone());
                }
                self.status =
                    "Diamond dart (30 × 150 mm) added at the middle of the piece and sewn".into();
            }
            Id::ExtendLineEnds => {
                let (panel, line) = self
                    .authoring
                    .edge
                    .as_ref()
                    .and_then(|e| Some((e.panel.clone(), e.edge.clone()?)))
                    .ok_or("Select an internal line first")?;
                let mut extended = false;
                for at_start in [true, false] {
                    extended |= self.edit(fashioncraft::authoring::Command::ExtendLine {
                        panel: panel.clone(),
                        line: line.clone(),
                        at_start,
                    });
                }
                if !extended {
                    return Err(self.status.clone());
                }
            }
            Id::UnfoldPiece => {
                if !self.unfold_selected_segment() {
                    return Err(self.status.clone());
                }
            }
            Id::MergeAlongSeam => {
                let seam = self
                    .edge_seams()
                    .into_iter()
                    .next()
                    .ok_or("Select a sewn boundary first")?;
                if !self.merge_along_seam(seam) {
                    return Err(self.status.clone());
                }
            }
            Id::CutAndSew => {
                if !self.cut_and_sew_selected_line() {
                    return Err(self.status.clone());
                }
            }
            Id::SlashSpread => {
                if !self.slash_spread_selected_line() {
                    return Err(self.status.clone());
                }
            }
            Id::ReplaceOutline => {
                if !self.replace_outline_with_selected_shape() {
                    return Err(self.status.clone());
                }
            }
            Id::ArchivePieces => {
                let panels = self.selected_panels();
                self.edit(fashioncraft::authoring::Command::ArchivePanels { panels });
            }
            Id::RandomPieceColours => {
                self.prefs.garment_display.piece_colors = !self.prefs.garment_display.piece_colors;
            }
            Id::SelectAllPieces => self.select_all_pieces(),
            Id::CopyPieces => self.copy_panels(ctx),
            Id::PastePieces => {
                let clip = self
                    .authoring
                    .selection
                    .clipboard
                    .clone()
                    .ok_or("Nothing copied yet")?;
                self.paste_panels(clip, ctx);
            }
            Id::DeletePieces => {
                let panels = self.selected_panels();
                self.edit(fashioncraft::authoring::Command::RemovePanels { panels });
            }
            Id::DuplicatePiece => {
                let panel = self
                    .authoring
                    .selected
                    .clone()
                    .ok_or("Select a panel first")?;
                self.edit(fashioncraft::authoring::Command::Duplicate { panel });
            }
            Id::DeactivatePieces | Id::DeactivatePiecesWithSeams | Id::ActivatePieces => {
                let ids = self.selected_panels();
                self.set_active(
                    &ids,
                    id != Id::ActivatePieces,
                    id == Id::DeactivatePiecesWithSeams,
                );
            }
            Id::FreezePieces => self.freeze_selected(true),
            Id::UnfreezePieces => self.freeze_selected(false),
            Id::StrengthenPieces => {
                let panels = self.selected_panels();
                // Toggle as a group: strengthen unless every selected piece already is.
                let on = !panels
                    .iter()
                    .all(|p| self.project.template.strengthened_panels.contains(p));
                self.edit(fashioncraft::authoring::Command::Strengthen { panels, on });
            }
            Id::SolidifyPieces => {
                let ids = self.selected_panels();
                let solid = ids.iter().all(|p| {
                    self.project
                        .template
                        .solidified_panels
                        .iter()
                        .any(|s| &s.panel == p)
                });
                self.set_solid(&ids, !solid);
            }
            Id::FlipPieceNormals => {
                let panels = self.selected_panels();
                self.edit(fashioncraft::authoring::Command::FlipNormals { panels });
            }
            Id::SuperimposeOver | Id::SuperimposeUnder => {
                let ids = self.selected_panels();
                self.superimpose_panels(&ids, id == Id::SuperimposeOver);
            }
            Id::AddButton | Id::AddButtonhole => {
                // As in other garment tools: choose it, then click where each one goes.
                let buttonhole = id == Id::AddButtonhole;
                if self.prefs.workspace.mode == layout::ViewMode::Garment {
                    // No pattern view to click in: one at the middle of the selected piece.
                    let panel = self
                        .authoring
                        .selected
                        .clone()
                        .ok_or("Select a panel first")?;
                    let centre = self
                        .piece_centre(&panel)
                        .ok_or("The piece has no mesh yet")?;
                    let command = self.trim_at(&panel, centre, buttonhole);
                    self.edit(command);
                    return Ok(());
                }
                self.set_authoring_tool(Tool::Edit, false);
                self.authoring.trim_placement = Some(buttonhole);
                self.status = i18n::tr(
                    self.prefs.language,
                    if buttonhole {
                        "Click on a piece in 2D to place a buttonhole · Esc when done"
                    } else {
                        "Click on a piece in 2D to place a button · Esc when done"
                    },
                )
                .into();
            }
            Id::FastenButtons => self.fasten(true),
            Id::AddGraphic => {
                let panel = self
                    .authoring
                    .selected
                    .clone()
                    .ok_or("Select a panel first")?;
                self.add_graphic(&panel);
            }
            Id::LockSelectedPieces => {
                self.apply_lock_action(board_locks::LockAction::LockSelected)?
            }
            Id::LockAllPieces => self.apply_lock_action(board_locks::LockAction::LockAll)?,
            Id::UnlockAllPieces => self.apply_lock_action(board_locks::LockAction::UnlockAll)?,
            Id::LockInternalLines => {
                self.apply_lock_action(board_locks::LockAction::LockInternalLines)?
            }
            Id::UnlockInternalLines => {
                self.apply_lock_action(board_locks::LockAction::UnlockInternalLines)?
            }
            Id::InvertSelection => self.invert_selection(),
            Id::ResetSelectedPieces3d => {
                let ids = self.selected_panels();
                self.reset_panels_3d(&ids);
            }
        }
        Ok(())
    }

    fn invoke_command(&mut self, id: commands::CommandId, ctx: &egui::Context) {
        if let Err(error) = self.dispatch_command(id, ctx) {
            self.status = error;
        }
    }

    fn step(&mut self, ctx: &egui::Context) -> bool {
        self.submit(ctx, true)
    }
    /// Takes the worker's latest step, if it finished since the last frame.
    fn take_simulation_result(&mut self) {
        let done = self.receive.lock().unwrap().take();
        if let Some(done) = current_worker_result(done, self.generation) {
            self.pending = false;
            match done.result {
                Ok((state, m)) => {
                    self.simulation_error = None;
                    self.document.dirty = true;
                    self.smoke_applied |= done.had_grab;
                    self.scene.state = state;
                    self.status = "Dynamic simulation · OGC contacts".into();
                    self.metrics = Some(m);
                    self.advance_pose_transition();
                    self.observe_seam_breaks();
                }
                Err(e) => {
                    self.playing = false;
                    self.simulation_error = Some(e.clone());
                    self.status = e;
                }
            }
        }
    }
    fn submit(&mut self, ctx: &egui::Context, once: bool) -> bool {
        let scene = FittingScene {
            garment: self.scene.garment.clone(),
            collider: self.scene.collider.clone(),
            state: self.scene.state.clone(),
            quasistatic: false,
            next_dt: self.settings.dt,
        };
        let sent = queue_work(
            &self.send,
            Work {
                generation: self.generation,
                playing: self.playing,
                once,
                scene,
                settings: self.settings.clone(),
                grab: self.grab.clone(),
                gpu: self.gpu,
                ctx: ctx.clone(),
            },
        );
        self.pending = sent.is_ok() && (self.playing || once);
        if let Err(error) = sent {
            self.playing = false;
            self.status = error;
            return false;
        }
        true
    }
    fn load(&mut self, p: FittingProject) -> bool {
        let ui_lang = self.prefs.language;
        match p.compile() {
            Ok(scene) => {
                self.document.dirty = true;
                self.authoring.sewing = Default::default();
                if let Some(poly) = p.template.panels.iter().find_map(|p| p.polygon.as_ref()) {
                    let m = poly.meshing_settings();
                    self.mesh_mm = m.target_edge_m * 1000.;
                    self.anisotropy = m.anisotropy;
                    self.mesh_axis = m.axis_degrees;
                }
                self.authoring
                    .history
                    .record_named(self.project.template.clone(), "Load");
                self.authoring.board.fitted = false;
                self.authoring.replace = false;
                self.authoring.draft.clear();
                self.authoring.tangents.clear();
                self.authoring.edge = None;
                self.authoring.board.clear_preview();
                self.authoring.preview = None;
                self.authoring.preview_state = None;
                self.authoring.error = None;
                self.authoring.fabric_draft = None;
                self.authoring.drag = None;
                self.authoring.ends.clear();
                self.authoring.selected = p.template.panels.first().map(|p| p.id.clone());
                self.authoring.drafting.line_panel = if self.authoring.tool == Tool::InternalLine {
                    self.authoring.selected.clone()
                } else {
                    None
                };
                self.scene = scene;
                self.issue.invalidate();
                let saved = p.animation_take.clone();
                self.project = p;
                self.restore_take(saved);
                self.generation += 1;
                self.playing = false;
                self.pending = false;
                self.metrics = None;
                self.grab = None;
                self.drag_plane = None;
                self.status =
                    i18n::tr(ui_lang, "New arrangement · run to assemble the seams").into();
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }
    fn preset(&mut self, kind: GarmentKind) -> bool {
        self.load(FittingProject::basic(
            kind,
            2,
            self.project.avatar_path.clone(),
        ))
    }
    fn reset(&mut self) -> bool {
        let mut p = self.project.clone();
        p.state = None;
        p.mesh_signature = None;
        p.prearrange = false;
        self.load(p)
    }
    fn remesh(&mut self) {
        let mut p = self.project.clone();
        p.state = None;
        p.mesh_signature = None;
        p.prearrange = false;
        for panel in &mut p.template.panels {
            if let Some(poly) = &mut panel.polygon {
                let mut m = poly.meshing_settings();
                m.target_edge_m = self.mesh_mm * 0.001;
                m.anisotropy = self.anisotropy;
                m.axis_degrees = self.mesh_axis;
                poly.meshing = Some(m);
            }
        }
        self.load(p);
    }
    fn export(&mut self) -> bool {
        if !self.commit_placement() {
            return false;
        }
        let result = export_session(&self.project, &self.scene, &self.settings, &self.output);
        let succeeded = result.is_ok();
        self.status = result.map_or_else(
            |e| e.to_string(),
            |_| format!("Export: {}", self.output.display()),
        );
        succeeded
    }
    fn pick_and_drag(&mut self, response: &egui::Response, rect: egui::Rect) {
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (origin, dir) = self.camera.ray(pos, rect);
            let mut best = f64::INFINITY;
            let mut picked = None;
            for (face, t) in self.scene.garment.mesh.triangles.iter().enumerate() {
                // Hidden or isolated-away cloth cannot be grabbed.
                if !self.cloth_face_shown(face) {
                    continue;
                }
                if let Some((d, w)) =
                    picking::ray_triangle(origin, dir, t.vertices.map(|i| self.scene.state.x[i]))
                    && d < best
                {
                    best = d;
                    picked = Some((t.vertices, w));
                }
            }
            if picked.is_some()
                && self.avatar_shown()
                && self.scene.collider.occludes_ray(origin, dir, best - 1e-5)
            {
                picked = None;
            }
            if let Some((vertices, weights)) = picked {
                let target = origin + best * dir;
                let (edge_origin, edge_dir) = self
                    .camera
                    .ray(pos + egui::vec2(self.grab_radius_px, 0.), rect);
                let radius = picking::ray_plane(edge_origin, edge_dir, target, dir)
                    .map_or(0., |edge| edge.distance(target));
                let patch = self
                    .scene
                    .garment
                    .panels
                    .iter()
                    .find(|panel| panel.vertices.contains(&vertices[0]))
                    .map_or_else(Vec::new, |panel| {
                        fashioncraft::vbd::grab::surface_patch(
                            &self.scene.garment.mesh,
                            &self.scene.state.x,
                            panel.triangles.clone(),
                            vertices,
                            target,
                            radius,
                        )
                    });
                self.grab = Some(Grab {
                    patch,
                    vertices,
                    weights,
                    target,
                    stiffness: 150. * self.grab_strength,
                });
                self.drag_plane = Some((target, dir));
                self.playing = true;
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let (Some(g), Some((point, normal)), Some(pos)) = (
                &mut self.grab,
                self.drag_plane,
                response.interact_pointer_pos(),
            )
        {
            let (origin, dir) = self.camera.ray(pos, rect);
            if let Some(target) = picking::ray_plane(origin, dir, point, normal) {
                g.target = target;
            }
        }
        if !response.ctx.input(|i| i.pointer.primary_down()) {
            self.grab = None;
            self.drag_plane = None;
        }
    }
}
impl eframe::App for Atelier {
    /// Runs before every `ui`, and also while the window is minimised or covered (macOS draws
    /// no frames then), so an AI agent's calls are answered either way.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drive_agent(ctx);
        self.drive_assistant_eval(ctx);
        // `ui` steps the simulation; a hidden window still steps one an agent asked for.
        let hidden = ctx.input(|i| i.viewport().visible() == Some(false));
        if hidden
            && self.playing
            && matches!(
                self.agent.running,
                Some((_, agent_hub::Pending::Simulate { .. }))
            )
        {
            self.take_simulation_result();
            self.submit(ctx, false);
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    /// A clean quit takes the agents' connection file away at once.
    fn on_exit(&mut self) {
        self.agent.stop();
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(demo) = &mut self.demo {
            demo.input(input);
        }
        if let Some(smoke) = &mut self.authoring_smoke {
            smoke.input(input);
        }
        if !self.drag_test {
            return;
        }
        let Some(origin) = self.smoke_point else {
            return;
        };
        // Startup shader compilation may outlast a short render-frame script.
        // Keep the actual pointer held until the worker accepts a grabbed step;
        // wall-clock deadlines below still turn a stalled worker into a failure.
        if self.smoke_tick != 14 || self.smoke_applied || self.simulation_error.is_some() {
            self.smoke_tick += 1;
        }
        let tick = self.smoke_tick;
        let pos = origin + Vec2::new((tick.saturating_sub(2).min(12) * 5) as f32, 0.);
        if tick <= 15 {
            input.events.push(egui::Event::PointerMoved(pos));
        }
        if tick == 2 || tick == 15 {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: tick == 2,
                modifiers: Default::default(),
            });
        }
        if tick >= 15 {
            self.playing = false;
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.begin_demo_frame(&ctx);
        // A window larger than its screen (a 1280 × 800 laptop, say) starts maximized, so nothing
        // is off screen or under the taskbar: eframe only clamps it to the monitor's full size. A
        // demo or a smoke run keeps the size it asked for.
        if !self.window_checked {
            let (monitor, outer) =
                ctx.input(|i| (i.viewport().monitor_size, i.viewport().outer_rect));
            if let (Some(monitor), Some(outer)) = (monitor, outer) {
                self.window_checked = true;
                let fixed_size = self.demo.is_some()
                    || self.smoke
                    || self.drag_test
                    || self.perf.is_some()
                    || self.authoring_smoke.is_some();
                if !fixed_size
                    && (outer.width() > monitor.x - 16. || outer.height() > monitor.y - 64.)
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                }
            }
        }
        // Placing buttons ends with Esc, another tool, or leaving the pattern view, however
        // the tool changed (several places set it directly).
        if self.authoring.trim_placement.is_some()
            && (self.authoring.tool != Tool::Edit
                || self.prefs.workspace.mode == layout::ViewMode::Garment
                || ctx.input(|i| i.key_pressed(egui::Key::Escape)))
        {
            self.authoring.trim_placement = None;
        }
        if !ctx.input(|i| i.focused) {
            // A hidden/blurred viewport must not retain an unfinished placement.
            // Existing cancellation restores the captured drape and invalidates
            // stale worker responses; it does not record an authoring edit.
            self.cancel_preview();
            self.authoring.selection.placement_marquee = None;
        }
        if self.authoring.tool != Tool::Place {
            self.authoring.selection.placement_marquee = None;
        }
        let previous_preferences = self.prefs.clone();
        let previous_simulation = self.settings.clone();
        let profiles_blocked = self.profiles.open;
        let profile_path = self.output.join(workspace_profiles::FILE_NAME);
        let presentation = workspace_profiles::Snapshot::capture(
            &self.prefs,
            self.tools_visible,
            self.wire,
            self.seam_links,
        );
        if let Some(snapshot) = self.profiles.show(&ctx, &profile_path, &presentation) {
            snapshot.apply(
                &mut self.prefs,
                &mut self.tools_visible,
                &mut self.wire,
                &mut self.seam_links,
            );
        }
        let palette_idle = !profiles_blocked
            && !self.settings_open
            && !self.authoring.radial.blocked
            && !self.quick.blocked
            && self.authoring.preview.is_none()
            && !self.authoring.board.gesture_active()
            && self.grab.is_none()
            && self.authoring.drag.is_none()
            && self.authoring.dart.is_none()
            && self.authoring.draft.is_empty()
            && self.authoring.ends.is_empty()
            && self.authoring.sewing.start.is_none()
            && self.authoring.sewing.sides.iter().all(Vec::is_empty);
        let can_open_palette = command_palette::can_launch(&ctx, palette_idle);
        if command_palette::launch_shortcut(&ctx, can_open_palette) {
            self.palette.begin();
        }
        // Retain the barrier on the closing frame so Enter/click cannot reach a tool.
        let palette_blocked = self.palette.open || profiles_blocked;
        let context = self.command_context();
        self.palette.language = self.prefs.language;
        if let Some(command) = self.palette.show(&ctx, context) {
            self.invoke_command(command, &ctx);
        }
        if palette_blocked {
            ui.disable();
            self.authoring.radial.blocked = true;
            self.quick.blocked = true;
        } else {
            if let Some((tool, rotate)) = self.authoring.radial.show(
                &ctx,
                self.authoring.tool,
                self.authoring.rotate,
                self.authoring.selected.is_some(),
            ) {
                self.set_authoring_tool(tool, rotate);
            }
            self.quick_actions(&ctx);
            self.sewing_context(&ctx);
            self.clipboard_shortcuts(&ctx);
        }
        self.frames += 1;
        self.authoring.hover_edge = None;
        self.take_simulation_result();
        if !self.authoring.radial.blocked
            && !self.quick.blocked
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
            && !ctx.egui_wants_keyboard_input()
        {
            self.invoke_command(commands::CommandId::ToggleSimulation, &ctx);
        }
        if !self.authoring.radial.blocked && !self.quick.blocked {
            self.garment_display_shortcuts(&ctx);
            self.camera_shortcuts(&ctx);
        }
        self.run_native_menu(&ctx, palette_blocked);
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("FASHIONCRAFT")
                        .size(15.)
                        .family(workspace::display_or_body(ui))
                        .extra_letter_spacing(0.5)
                        .color(workspace::app_ink(ui.visuals())),
                );
                ui.label(
                    egui::RichText::new("ALPHA")
                        .small()
                        .extra_letter_spacing(1.)
                        .color(ui.visuals().warn_fg_color),
                )
                .on_hover_text("Early alpha · built on ClothCraft by erythrofr and contributors");
                let language = self.prefs.language;
                // In a narrow window (a laptop screen) these buttons shrink to their symbols,
                // so that every menu of the bar still fits. With the menus in macOS's menu bar
                // they have the room.
                let compact = self.native_menu.is_none() && ctx.content_rect().width() < 1560.;
                ui.add_space(14.);
                // Design or Animation, as cloth tools switch between the two ways of working.
                let animation = self.authoring.animation.open;
                if workspace::mode_switch(
                    ui,
                    animation,
                    [i18n::tr(language, "Design"), i18n::tr(language, "Animation")],
                )
                .is_some()
                {
                    self.invoke_command(commands::CommandId::OpenAnimation, &ctx);
                }
                if self.native_menu.is_none() {
                    ui.separator();
                    ui.scope(|ui| {
                        workspace::menu_bar_style(ui);
                        self.menu_bar(ui, &ctx, can_open_palette);
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let play_label = i18n::tr(
                        language,
                        if self.playing {
                            "⏸ Pause"
                        } else if self.authoring.ends.len() == 2 && !self.authoring.sewing.chain_mode {
                            "▶ Sew and simulate"
                        } else {
                            "▶ Simulate"
                        },
                    );
                    // The one primary action of the bar: filled with FashionCraft's colour.
                    let play_text = if compact {
                        if self.playing { workspace::symbols::PAUSE } else { workspace::symbols::PLAY }
                    } else {
                        play_label
                    };
                    let play = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(play_text)
                                    .strong()
                                    .color(Color32::from_rgb(0x1b, 0x1b, 0x1f)),
                            )
                            .fill(workspace::accent(ui.visuals()))
                            .stroke(Stroke::NONE)
                            .min_size(Vec2::new(0., 26.)),
                        )
                        .on_hover_text(format!("{play_label} · {}", i18n::tr(language, "Space")));
                    demo::mark(&ctx, "toolbar.simulate", play.rect);
                    self.smoke_target("simulate", play.rect.center());
                    if play.clicked() {
                        self.invoke_command(commands::CommandId::ToggleSimulation, &ctx);
                    }
                    let export_label = i18n::tr(language, "Export");
                    let export_button = ui
                        .add(
                            egui::Button::new(if compact { workspace::symbols::EXPORT } else { export_label })
                                .min_size(Vec2::new(0., 26.)),
                        )
                        .on_hover_text(format!(
                            "{export_label}: {}",
                            i18n::tr(language, "Saves the JSON project, the patterns, the OBJ garment and the previews into the session folder.")
                        ));
                    self.smoke_target("export", export_button.rect.center());
                    if export_button.clicked() {
                        self.invoke_command(commands::CommandId::ExportSession, &ctx);
                    }
                    for (id, icon, tip) in [
                        (commands::CommandId::Redo, icons::Icon::Redo, "Redo"),
                        (commands::CommandId::Undo, icons::Icon::Undo, "Undo"),
                    ] {
                        let available = commands::availability(id, self.command_context()).is_ok();
                        let keys = keymap::label(self.prefs.keymap, id)
                            .or_else(|| id.descriptor().shortcut.map(String::from))
                            .map(|k| format!(" · {}", keymap::shown(&k)))
                            .unwrap_or_default();
                        let clicked = ui
                            .add_enabled_ui(available, |ui| {
                                icons::icon_only(
                                    ui,
                                    icon,
                                    26.,
                                    false,
                                    &format!("{}{keys}", i18n::tr(language, tip)),
                                )
                            })
                            .inner
                            .clicked();
                        if clicked {
                            self.invoke_command(id, &ctx);
                        }
                    }
                    // The command search sits in the middle of the room left between them, and
                    // only when there is room: in a narrow window it would cover the menus.
                    let search = ui.available_width().clamp(120., 340.);
                    ui.add_space(((ui.available_width() - search) / 2.).max(8.));
                    if ui.available_width() >= 128.
                        && command_palette::launch_button(
                        ui,
                        can_open_palette,
                        i18n::tr(language, "Search commands"),
                        language,
                    ) {
                        self.palette.begin();
                    }
                });
            });
        });
        self.animation_tick(&ctx);
        let mut changed = false;
        if !palette_blocked {
            settings::window(&ctx, &mut self.settings_open, &mut self.prefs, &mut changed);
            self.library_window(&ctx);
            self.pose_window(&ctx);
            self.scene_window(&ctx);
            self.obj_export_window(&ctx);
            self.print_window(&ctx);
            self.avatar_shape_window(&ctx);
            self.avatar_maker_window(&ctx);
            self.avatar_library_window(&ctx);
            self.draft_window(&ctx);
            self.skirt_draft_window(&ctx);
            self.trousers_draft_window(&ctx);
            self.history_window(&ctx);
            self.assistant_window(&ctx);
            self.camera_props_window(&ctx);
            self.pleats_window(&ctx);
            self.mesh_select_window(&ctx);
            self.texture_edit_window(&ctx);
            self.uv_editor_window(&ctx);
            self.script_window(&ctx);
            self.steam_window(&ctx);
            self.fabric_kit_window(&ctx);
            self.render_window(&ctx);
            self.grading_window(&ctx);
            self.object_browser_window(&ctx);
            self.native_menu_windows(&ctx, palette_idle);
        }
        if changed {
            // Apply immediately rather than on the next start: the window edits
            // the live `Preferences`, so anything less would leave the theme
            // switch looking broken until a restart. `apply` is idempotent.
            settings::apply(&ctx, &self.prefs);
            // The board reads its step at startup, so push the edited value
            // through here too; otherwise the control would persist without
            // taking effect, which is a stored value pretending to be live.
            self.authoring.board.snap_mm = self.prefs.snap_mm;
        }
        egui::Panel::bottom("status").show(ui, |ui| {
            self.cadence.update(std::time::Instant::now(), self.scene.state.time, self.generation, self.playing);
            ui.horizontal(|ui| {
                let ui_lang = self.prefs.language;
                // The state as a dot (green at rest, fuchsia while simulating) and a word.
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.), egui::Sense::hover());
                ui.painter().circle_filled(
                    dot_rect.center(),
                    3.5,
                    if self.playing {
                        workspace::accent(ui.visuals())
                    } else {
                        Color32::from_rgb(0x57, 0xab, 0x5a)
                    },
                );
                ui.add_space(2.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("FashionCraft {}", env!("CARGO_PKG_VERSION")))
                            .small()
                            .weak(),
                    );
                    ui.add_space(6.);
                    // The views: split, 3D only, 2D only, as icons.
                    for (icon, hint, mode) in [
                        (icons::Icon::Sheet, "2D only", layout::ViewMode::Patterns),
                        (icons::Icon::ViewIso, "3D only", layout::ViewMode::Garment),
                        (icons::Icon::SplitView, "Split view", layout::ViewMode::Split),
                    ] {
                        if icons::icon_only(
                            ui,
                            icon,
                            22.,
                            self.prefs.workspace.mode == mode,
                            i18n::tr(ui_lang, hint),
                        )
                        .clicked()
                        {
                            self.prefs.workspace.mode = mode;
                        }
                    }
                    if let Some(speed) = self.cadence.speed {
                        ui.add_space(8.);
                        ui.label(
                            egui::RichText::new(format!(
                                "{:.0} steps/s · {:.2}×",
                                speed / self.settings.dt,
                                speed
                            ))
                            .small()
                            .color(if speed >= 0.95 {
                                ui.visuals().weak_text_color()
                            } else {
                                Color32::GOLD
                            }),
                        )
                        .on_hover_text("Actual physical time advanced per wall-clock second. Independent of render FPS.");
                    }
                    // Simulated time and where the solver runs, at rest as while simulating.
                    ui.add_space(8.);
                    let time = format!("{:.2}", self.scene.state.time);
                    let time = if ui_lang == i18n::Language::Spanish {
                        time.replace('.', ",")
                    } else {
                        time
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {time} s · {}",
                            i18n::tr(ui_lang, "Time"),
                            if self.gpu { "GPU" } else { "CPU" }
                        ))
                        .small()
                        .weak(),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Simulated time, and whether the solver runs on the graphics card or the processor",
                    ));
                    let status = i18n::status(ui_lang, &self.status);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(status.as_ref()).truncate())
                        .on_hover_text(status.as_ref());
                    });
                });
            });
        });
        if self.authoring.animation.open {
            egui::Panel::bottom("animation").show(ui, |ui| self.animation_timeline(ui));
        }
        if self.tools_visible {
            egui::Panel::left("authoring-tools")
                .exact_size(52.)
                .resizable(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.authoring_toolbar(ui));
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().panel_fill))
            .show(ui, |ui| {
                let area = ui.available_rect_before_wrap();
                let panes = layout::arrange(ui, area, &mut self.prefs.workspace);
                self.authoring.selection.board_rect = None;
                self.authoring.selection.garment_rect = None;
                for (id, rect) in [
                    ("garment", panes.garment),
                    ("patterns", panes.patterns),
                    ("inspector", panes.inspector),
                ] {
                    let Some(rect) = rect else { continue };
                    ui.painter().rect_filled(rect, 0., ui.visuals().panel_fill);
                    let mut pane = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(id)
                            .max_rect(rect.shrink(7.))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    pane.set_clip_rect(rect);
                    match id {
                        "garment" => self.garment_view(&mut pane),
                        "patterns" => self.patterns(&mut pane),
                        _ => self.inspector(&mut pane),
                    }
                }
            });
        self.drive_media3d(&ctx);
        self.prefs_dirty |= self.prefs != previous_preferences;
        if self.prefs_dirty && !ctx.input(|i| i.pointer.any_down()) {
            if !self.prefs_kept
                && let Err(error) =
                    settings::save(&self.output.join(settings::FILE_NAME), &self.prefs)
            {
                self.status = format!("Preferences could not be saved: {error}");
            }
            self.prefs_dirty = false;
        }
        if let Some(perf) = &mut self.perf
            && let Some(mut report) = perf.tick(
                self.scene.state.time,
                ctx.cumulative_frame_nr(),
                self.settings.dt,
                self.metrics.is_some(),
            )
        {
            report["last_delivered_step"] = serde_json::to_value(&self.metrics).unwrap();
            self.playing = false;
            if let Err(e) = std::fs::write(
                self.output.join("ui-perf.json"),
                serde_json::to_string_pretty(&report).unwrap(),
            ) {
                eprintln!("UI performance report: {e}");
            }
        }
        self.document.dirty |= self.settings != previous_simulation;
        self.document_tick(&ctx);
        self.submit(&ctx, false);
        if self.playing {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if self.pending {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.authoring_smoke_tick();
        if self.smoke {
            let drag_finished = self.drag_test && self.smoke_tick >= 20 && !self.pending;
            if !self.smoke_captured
                && ((self.authoring_smoke.is_none()
                    && !self.drag_test
                    && self.perf.is_none()
                    && self.frames == 8)
                    || self.authoring_smoke.as_ref().is_some_and(|s| s.finished)
                    || drag_finished
                    || (self.authoring_smoke.is_none()
                        && self.perf.is_none()
                        && ((!self.drag_test && self.frames > 110)
                            || (self.drag_test
                                && self.smoke_started.elapsed() > Duration::from_secs(15))))
                    || self.perf.as_ref().is_some_and(|p| p.finished))
            {
                self.smoke_captured = true;
                if self.drag_test {
                    let displacement = self
                        .scene
                        .state
                        .x
                        .iter()
                        .zip(&self.smoke_initial)
                        .map(|(a, b)| a.distance(*b))
                        .fold(0., f64::max);
                    let report = serde_json::json!({"passed":self.smoke_grabbed&&self.smoke_applied&&displacement>1e-5,"pointer_picked_cloth":self.smoke_grabbed,"accepted_step_with_grab":self.smoke_applied,"max_vertex_displacement_m":displacement,"status":self.status,"state_time_s":self.scene.state.time});
                    if let Err(e) = std::fs::write(
                        self.output.join("interaction.json"),
                        serde_json::to_string_pretty(&report).unwrap(),
                    ) {
                        eprintln!("Interaction report: {e}");
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        self.output.join("ui.png"),
                        &pixels,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("UI capture: {e}");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            if (self.authoring_smoke.is_none()
                && self.perf.is_none()
                && ((!self.drag_test && self.frames > 120)
                    || (self.drag_test && self.smoke_started.elapsed() > Duration::from_secs(20))))
                || self.perf.as_ref().is_some_and(|p| p.expired())
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        self.sync_native_menu(&ctx, palette_blocked, can_open_palette);
        // Last, once every window and dialog of the frame has reported where it is.
        self.drive_demo(&ctx);
    }
}

#[cfg(test)]
mod command_action_tests {
    use super::*;

    #[test]
    fn simulation_submission_reports_disconnection_without_running_a_solver() {
        let dir = tempfile::tempdir().unwrap();
        let avatar = dir.path().join("avatar.obj");
        std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").unwrap();
        let project = FittingProject::basic(GarmentKind::Dress, 1, avatar);
        let scene = project.compile().unwrap();
        let initial = serde_json::to_vec(&scene.state).unwrap();
        let work = Work {
            generation: 42,
            scene,
            settings: VbdSettings::default(),
            grab: None,
            gpu: false,
            ctx: egui::Context::default(),
            playing: false,
            once: true,
        };
        let (send, receive) = mpsc::channel();
        queue_work(&send, work).unwrap();
        let received = receive.try_recv().unwrap();
        assert!(received.once);
        assert!(!received.playing);
        assert_eq!(received.generation, 42);
        assert_eq!(serde_json::to_vec(&received.scene.state).unwrap(), initial);
        drop(receive);
        assert_eq!(
            queue_work(&send, received),
            Err("The simulation worker is unavailable".into())
        );
    }

    #[test]
    fn export_reports_early_and_late_io_failures_and_preserves_live_state() {
        let dir = tempfile::tempdir().unwrap();
        let avatar = dir.path().join("avatar.obj");
        std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").unwrap();
        let project = FittingProject::basic(GarmentKind::Dress, 1, avatar);
        let mut scene = project.compile().unwrap();
        scene.state.time = 1.25;
        scene.state.v[0] = glam::DVec3::new(0.1, 0.2, 0.3);
        let state_before = serde_json::to_vec(&scene.state).unwrap();
        let project_before = serde_json::to_vec(&project).unwrap();
        let settings = VbdSettings::default();

        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "keep").unwrap();
        assert!(export_session(&project, &scene, &settings, &blocked).is_err());
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "keep");

        let late = dir.path().join("late-failure");
        std::fs::create_dir_all(late.join("fitting-project.json")).unwrap();
        assert!(export_session(&project, &scene, &settings, &late).is_err());

        let valid = dir.path().join("valid");
        export_session(&project, &scene, &settings, &valid).unwrap();
        let restored = FittingProject::load(&valid.join("fitting-project.json")).unwrap();
        assert_eq!(
            serde_json::to_vec(restored.state.as_ref().unwrap()).unwrap(),
            state_before
        );
        assert_eq!(serde_json::to_vec(&scene.state).unwrap(), state_before);
        assert_eq!(serde_json::to_vec(&project).unwrap(), project_before);
    }
}
