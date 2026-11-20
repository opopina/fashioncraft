//! The menu bar, laid out the way garment designers expect from desktop cloth tools: File,
//! Edit, 3D Garment, 2D Pattern, Sewing, Materials, Avatar, Render, Display, Settings,
//! Window and Help. Every item runs a registry command; items not built yet stay visible
//! but disabled, and [`coverage`] counts how many of them work.
use super::commands::CommandId;
use super::*;

/// UI-only entries that are not commands: they keep their existing controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Custom {
    OnlinePatterns,
    ExportInformation,
    ToolRail,
    Presentation,
    Profiles,
    Coverage,
    Keymap,
    Language,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Item {
    /// Runs a registry command.
    Run(&'static str, CommandId),
    /// Planned: shown disabled and counted as missing.
    Todo(&'static str),
    Sub(&'static str, &'static [Item]),
    Custom(Custom),
    Separator,
}

use CommandId as C;
use Item::{Custom as X, Run, Separator as S, Sub, Todo};

const FILE: &[Item] = &[
    Run("New project", C::NewProject),
    Run("Open project…", C::OpenProject),
    Sub(
        "Open",
        &[
            Run("Garment…", C::OpenTemplates),
            Todo("Trim…"),
            Run("Avatar…", C::ImportAvatar),
            Run("Pose…", C::LoadPoseFile),
            Run("Scene and props…", C::OpenSceneObjects),
        ],
    ),
    Sub(
        "Add",
        &[
            Run("Project…", C::AddGarment),
            Run("Garment…", C::AddGarment),
            Todo("Avatar…"),
        ],
    ),
    Run("Save project", C::SaveProject),
    Sub(
        "Save as",
        &[
            Run("Project…", C::SaveProjectAs),
            Run("Garment…", C::SaveGarment),
            Run("Avatar…", C::SaveAvatar),
            Run("Pose…", C::SavePoseFile),
        ],
    ),
    S,
    Sub(
        "Import",
        &[
            Run("SVG patterns…", C::ImportSvg),
            Run("DXF patterns…", C::ImportDxf),
            Run("OBJ…", C::ImportAvatar),
            Run("FBX…", C::ImportAvatar),
            Run("glTF…", C::ImportAvatar),
            Run("Alembic…", C::ImportAvatar),
            Run("USD…", C::ImportAvatar),
            Run("Point cache…", C::ImportPointCache),
            Run("Motion (BVH)…", C::ImportBvh),
            Todo("Python script…"),
        ],
    ),
    Sub(
        "Export",
        &[
            Run("Session files (OBJ, patterns, previews)", C::ExportSession),
            Run("OBJ…", C::ExportObj),
            Run("OBJ (selected)…", C::ExportObjSelected),
            Run("FBX…", C::ExportFbx),
            Run("glTF / GLB…", C::ExportGltf),
            Run("Alembic…", C::ExportAlembic),
            Run("USD…", C::ExportUsd),
            Run("Point cache…", C::ExportPointCache),
            Run("MDD cache…", C::ExportMdd),
            Run("MDD cache (Maya/3ds Max)…", C::ExportMddZUp),
            Run("OBJ sequence…", C::ExportObjSequence),
        ],
    ),
    S,
    Sub(
        "Snapshot",
        &[
            Run("2D patterns at 1:1…", C::SnapshotPatterns),
            Run("3D view…", C::CaptureView3d),
        ],
    ),
    S,
    Run("Recover snapshot…", C::RecoverSnapshot),
    Run("Garment templates…", C::OpenTemplates),
    Run("T-shirt from measurements…", C::DraftTshirt),
    Run("Skirt from measurements…", C::DraftSkirt),
    Run("Trousers from measurements…", C::DraftTrousers),
    Sub(
        "Examples",
        &[Run("T-shirt", C::LoadTshirt), Run("Dress", C::LoadDress)],
    ),
    X(Custom::OnlinePatterns),
    S,
    X(Custom::ExportInformation),
];

const EDIT: &[Item] = &[
    Run("Undo", C::Undo),
    Run("Redo", C::Redo),
    S,
    Run("Delete", C::DeletePieces),
    Run("Copy", C::CopyPieces),
    Run("Paste", C::PastePieces),
    Run("Select all", C::SelectAllPieces),
    Run("Select all seams", C::SelectAllSeams),
    Run("Invert selection", C::InvertSelection),
    S,
    Sub(
        "Lock",
        &[
            Run("Lock selected pieces", C::LockSelectedPieces),
            Run("Lock all pieces", C::LockAllPieces),
            Run("Unlock all pieces", C::UnlockAllPieces),
            Run("Lock internal lines", C::LockInternalLines),
            Run("Unlock internal lines", C::UnlockInternalLines),
        ],
    ),
];

const GARMENT_3D: &[Item] = &[
    Run("Simulate", C::ToggleSimulation),
    Run("Advance one step", C::StepSimulation),
    S,
    Run("High-resolution garment", C::HighResolution),
    Run("Low-resolution garment", C::LowResolution),
    Run("Custom resolution…", C::SimulationProperties),
    S,
    Sub(
        "Pieces",
        &[
            Run("Archive", C::ArchivePieces),
            Run("Deactivate", C::DeactivatePieces),
            Run("Deactivate with seams", C::DeactivatePiecesWithSeams),
            Run("Activate", C::ActivatePieces),
            Run("Freeze", C::FreezePieces),
            Run("Unfreeze", C::UnfreezePieces),
            Run("Strengthen", C::StrengthenPieces),
            Run("Solidify", C::SolidifyPieces),
            Run("Flip normals", C::FlipPieceNormals),
            Run("Convert to quads", C::ConvertToQuads),
            Run("Convert to triangles", C::ConvertToTriangles),
            S,
            Run("Superimpose over", C::SuperimposeOver),
            Run("Superimpose under", C::SuperimposeUnder),
            Run("Superimpose side", C::SuperimposeSide),
        ],
    ),
    S,
    Run("Move in 3D", C::ToolMove),
    Run("Rotate in 3D", C::ToolRotate),
    Run("Grab cloth", C::ToolGrab),
    Run("Select mesh (brush)", C::SelectMeshBrush),
    Run("Select mesh (box)", C::SelectMeshBox),
    Run("Select mesh (lasso)", C::SelectMeshLasso),
    S,
    Sub(
        "Pins",
        &[
            Run("Pin (box)", C::PinBox),
            Run("Pin (lasso)", C::PinLasso),
            Run("Attach all pins to avatar", C::AttachPins),
            Run("Detach all pins from avatar", C::DetachPins),
        ],
    ),
    Todo("Style lines"),
    Run("3D pen", C::TracePen3d),
    S,
    Run("Fold arrangement", C::FoldArrangement),
    Run("Reset 2D arrangement (all)", C::Reset2dArrangement),
    Run("Reset 3D arrangement (all)", C::ResetSimulation),
    Run("Reset 3D arrangement (selected)", C::ResetSelectedPieces3d),
    Todo("Re-drape 3D arrangement"),
    S,
    Sub(
        "Garment measurements",
        &[
            Run("Linear measure", C::MeasureGarmentLine),
            Run("Circumference measure", C::MeasureGarmentGirth),
        ],
    ),
    Sub(
        "Tear",
        &[
            Run("Edit tears", C::EditTears),
            Run("Tear seam", C::TearSeam),
        ],
    ),
    S,
    Run("Mark simulation issue", C::ToolInspect),
];

const PATTERN_2D: &[Item] = &[
    Sub(
        "Edit",
        &[
            Run("Transform pattern", C::ToolEdit),
            Run("Duplicate piece", C::DuplicatePiece),
            Run("Cut and sew along line", C::CutAndSew),
            Run("Replace outline with shape", C::ReplaceOutline),
            Run("Slash and spread along line", C::SlashSpread),
            Run("Merge pieces along seam", C::MergeAlongSeam),
            Run("Unfold across segment", C::UnfoldPiece),
            Run("Extend line to outline", C::ExtendLineEnds),
            Run("Add point / split line", C::SplitSegment),
            Run("Smooth curve", C::SmoothCurve),
            Run("Cut and extend", C::CutAndExtend),
        ],
    ),
    Sub(
        "Create",
        &[
            Run("Polygon", C::ToolDraw),
            Run("Freehand", C::ToolFreehand),
            Run("Rectangle", C::ToolRectangle),
            Run("Ellipse", C::ToolEllipse),
            Run("Arc sector", C::ToolSector),
            Run("Spiral", C::AddSpiral),
            Run("Internal polygon or line", C::ToolInternalLine),
            Run("Internal rectangle", C::AddInternalRectangle),
            Run("Internal ellipse", C::AddInternalEllipse),
            Run("Diamond dart", C::AddDiamondDart),
        ],
    ),
    Run("Trace", C::TraceShape),
    Run("Measure on the board", C::ToolMeasure),
    Sub(
        "Pleats",
        &[
            Run("Pleats", C::Pleats),
            Run("Single pleat", C::SinglePleat),
            Run("Pleat sewing", C::PleatSewing),
        ],
    ),
    Todo("Auto fit"),
];

const SEWING: &[Item] = &[
    Run("Segment sewing", C::ToolSew),
    Run("Free sewing", C::FreeSewing),
    Run("M:N segment sewing", C::ChainSegmentSewing),
    Run("M:N free sewing", C::ChainFreeSewing),
    Run("Auto sewing", C::AutoSewing),
    Run("Check sewing length", C::CheckSewingLength),
    Run("Activate all seams", C::ActivateAllSeams),
];

const MATERIALS: &[Item] = &[
    Sub(
        "Texture",
        &[
            Run("Edit texture (2D)", C::EditTexture2d),
            Run("Edit texture (3D)", C::EditTexture3d),
            Run("Refresh textures", C::RefreshTextures),
        ],
    ),
    Sub(
        "Graphics",
        &[
            Run("Transform graphic", C::ToolGraphic),
            Run("Graphic on the selected piece…", C::AddGraphic),
            Run("Graphic on the 3D garment", C::GraphicOn3dGarment),
        ],
    ),
    Sub(
        "Buttons",
        &[
            Run("Button", C::AddButton),
            Run("Buttonhole", C::AddButtonhole),
            Run("Fasten buttons", C::FastenButtons),
        ],
    ),
    Run("Zipper", C::AddZipper),
    Run("Piping", C::Piping),
    Run("Tape", C::AddTape),
    Sub(
        "Topstitch",
        &[
            Run("Segment topstitch", C::AddTopstitch),
            Run("Free topstitch", C::FreeTopstitch),
            Run("Seamline topstitch", C::SeamlineTopstitch),
        ],
    ),
    Run("Puckering", C::Puckering),
    Run("Steam", C::Steam),
    Run("Seam taping", C::SeamTaping),
];

const AVATAR: &[Item] = &[
    Run("Delete avatar", C::DeleteAvatar),
    Run("Delete scene and props", C::ClearSceneObjects),
    S,
    Sub(
        "Avatar measurements",
        &[
            Run("Tape measure", C::MeasureBodyTape),
            Run("Height measure", C::MeasureBodyHeight),
            Run("Circumference measure", C::MeasureBodyGirth),
        ],
    ),
    Run("Pose…", C::OpenPose),
    Run("Rest pose", C::RestPose),
    Run("Play animation", C::PlayAnimation),
    Run("Avatar editor…", C::OpenAvatarEditor),
];

const ANIMATION: &[Item] = &[
    Run("Animation editor", C::OpenAnimation),
    Run("Record", C::RecordAnimation),
    Run("Delete animation", C::DeleteAnimation),
    S,
    Run("Video capture…", C::CaptureVideo),
];

const RENDER: &[Item] = &[
    Run("High-quality 3D view", C::HighQualityView),
    Run("Technical view", C::TechnicalView),
];

const DISPLAY: &[Item] = &[
    Sub(
        "Viewpoint",
        &[
            Run("Front", C::ViewFront),
            Run("Three-quarter, avatar's left", C::ViewThreeQuarterLeft),
            Run("Back", C::ViewBack),
            Run("Avatar's right side", C::ViewAvatarRight),
            Run("Avatar's left side", C::ViewAvatarLeft),
            Run("Top", C::ViewTop),
            Run("Bottom", C::ViewBottom),
            Run("Three-quarter, avatar's right", C::ViewThreeQuarterRight),
            S,
            Run("Frame selection", C::FrameView),
        ],
    ),
    Sub(
        "3D garment",
        &[
            Run("Show garment", C::ToggleGarment),
            Run("Show pins", C::ShowPins),
            Run("Show piece names", C::ShowPieceNames),
            Run("Show internal lines", C::ShowInternalLines),
            Run("Show graphics", C::ShowGraphics),
            Run("Show trims", C::ShowTrims),
        ],
    ),
    Sub(
        "Avatar",
        &[
            Run("Show avatar", C::ToggleAvatar),
            Run("Show arrangement points", C::ToggleArrangementPoints),
            Run("Show joints", C::ToggleSkeleton),
        ],
    ),
    Sub(
        "Garment style",
        &[
            Run("Textured with thickness", C::StyleTexturedThick),
            Run("Textured", C::StyleTextured),
            Run("Plain colour", C::StylePlain),
            Run("Translucent", C::StyleTranslucent),
            Run("Random piece colours", C::RandomPieceColours),
        ],
    ),
    Sub(
        "Fit maps",
        &[
            Run("Strain map", C::StrainMap),
            Run("Stress map", C::StressMap),
            Run("Pressure map", C::PressureMap),
        ],
    ),
    Run("Isolate selection", C::IsolateSelection),
];

const SETTINGS: &[Item] = &[
    Sub(
        "Gizmo",
        &[
            Run("Screen axes", C::GizmoScreenAxes),
            Run("Local axes", C::GizmoLocalAxes),
            Run("World axes", C::GizmoWorldAxes),
        ],
    ),
    Sub(
        "2D snapping",
        &[
            Run("Smart guides", C::ToggleSmartGuides),
            Run("Snap to pattern", C::ToggleSnapToPattern),
            Run("Snap to grid", C::ToggleSnapToGrid),
        ],
    ),
    Run("Grid properties…", C::OpenPreferences),
    Run("Camera properties…", C::CameraProperties),
    Run("Simulation properties…", C::SimulationProperties),
    X(Custom::Language),
    X(Custom::Keymap),
    Run("Preferences…", C::OpenPreferences),
];

const WINDOW: &[Item] = &[
    Run("Split view", C::SplitView),
    Run("3D only", C::GarmentView),
    Run("2D only", C::PatternView),
    Run("Properties inspector", C::ToggleInspector),
    Run("Library", C::ToggleLibrary),
    Run("Object browser", C::ToggleObjectBrowser),
    Run("History", C::ToggleHistory),
    S,
    Run("Reset layout", C::ResetLayout),
    X(Custom::ToolRail),
    X(Custom::Presentation),
    X(Custom::Profiles),
];

const HELP: &[Item] = &[
    X(Custom::Coverage),
    S,
    Run("User manual", C::OpenUserManual),
    Run("What's new", C::OpenWhatsNew),
];

/// The menu bar, left to right.
pub(super) const MENUS: &[(&str, &[Item])] = &[
    ("File", FILE),
    ("Edit", EDIT),
    ("3D Garment", GARMENT_3D),
    ("2D Pattern", PATTERN_2D),
    ("Sewing", SEWING),
    ("Materials", MATERIALS),
    ("Avatar", AVATAR),
    ("Animation", ANIMATION),
    ("Render", RENDER),
    ("Display", DISPLAY),
    ("Settings", SETTINGS),
    ("Window", WINDOW),
    ("Help", HELP),
];

fn count(items: &[Item], done: &mut usize, all: &mut usize) {
    for item in items {
        match item {
            Item::Run(..) => {
                *done += 1;
                *all += 1;
            }
            Item::Todo(_) => *all += 1,
            Item::Sub(_, inner) => count(inner, done, all),
            Item::Custom(_) | Item::Separator => {}
        }
    }
}

/// Mesh size of 3D Garment › High-resolution garment, in millimetres.
pub(super) const HIGH_RESOLUTION_MM: f64 = 10.;
/// Mesh size of 3D Garment › Low-resolution garment, in millimetres.
pub(super) const LOW_RESOLUTION_MM: f64 = 30.;

/// Menu items that work, out of all the menu items the bar lays out.
pub(super) fn coverage() -> (usize, usize) {
    let (mut done, mut all) = (0, 0);
    for (_, items) in MENUS {
        count(items, &mut done, &mut all);
    }
    (done, all)
}

use super::garment_display::GarmentStyle;

impl Atelier {
    /// Whether a toggle command is on, for its menu item's highlight.
    fn command_checked(&self, id: CommandId) -> bool {
        use CommandId as C;
        let display = &self.prefs.garment_display;
        match id {
            C::ToggleAvatar => display.avatar,
            C::ShowPins => display.pins,
            C::ShowInternalLines => display.internal_lines,
            C::ShowPieceNames => display.piece_names,
            C::MeasureBodyTape
            | C::MeasureBodyHeight
            | C::MeasureBodyGirth
            | C::MeasureGarmentLine
            | C::MeasureGarmentGirth => {
                self.authoring.measure3d.kind == super::measure3d_ui::MeasureKind::of(id)
            }
            C::StyleTexturedThick => display.style == GarmentStyle::TexturedThick,
            C::StyleTextured => display.style == GarmentStyle::Textured,
            C::StylePlain => display.style == GarmentStyle::Plain,
            C::StyleTranslucent => display.style == GarmentStyle::Translucent,
            C::GizmoWorldAxes => self.prefs.gizmo_axes == super::gizmo::GizmoAxes::World,
            C::GizmoLocalAxes => self.prefs.gizmo_axes == super::gizmo::GizmoAxes::Local,
            C::GizmoScreenAxes => self.prefs.gizmo_axes == super::gizmo::GizmoAxes::Screen,
            C::CameraProperties => self.authoring.camera_props.open,
            C::FreeSewing => self.in_sewing_mode(false, true),
            C::ChainSegmentSewing => self.in_sewing_mode(true, false),
            C::ChainFreeSewing => self.in_sewing_mode(true, true),
            C::Pleats | C::SinglePleat => self.authoring.pleats.open,
            C::SelectAllSeams => self.authoring.selection.all_seams,
            C::Steam => self.authoring.steam.open,
            C::HighQualityView => display.high_quality,
            C::EditTexture2d | C::EditTexture3d => self.authoring.texture_edit.open,
            C::SelectMeshBrush => {
                self.authoring.mesh_select.shape == Some(super::mesh_select::SelectShape::Brush)
            }
            C::SelectMeshBox => {
                self.authoring.mesh_select.shape == Some(super::mesh_select::SelectShape::Box)
            }
            C::SelectMeshLasso => {
                self.authoring.mesh_select.shape == Some(super::mesh_select::SelectShape::Lasso)
            }
            C::ShowTrims => display.trims,
            C::ShowGraphics => display.graphics,
            C::HighResolution => (self.mesh_mm - HIGH_RESOLUTION_MM).abs() < 1e-9,
            C::LowResolution => (self.mesh_mm - LOW_RESOLUTION_MM).abs() < 1e-9,
            C::ToggleGarment => display.garment,
            C::StrainMap => self.heat && !display.pressure && !display.strain_fabric,
            C::StressMap => self.heat && !display.pressure && display.strain_fabric,
            C::PressureMap => self.heat && display.pressure,
            C::ToggleSmartGuides => self.authoring.board.smart_guides,
            C::ToggleSnapToGrid => self.authoring.board.snap,
            C::ToggleSnapToPattern => self.authoring.board.snap_pattern,
            C::PinBox | C::PinLasso => {
                let pins = &self.authoring.pin_tool;
                self.authoring.tool == super::authoring_ui::Tool::Grab
                    && pins.enabled
                    && !pins.tack
                    && pins.lasso == (id == C::PinLasso)
            }
            C::TechnicalView => display.schematic,
            C::RandomPieceColours => display.piece_colors,
            C::IsolateSelection => self.authoring.view3d.isolated.is_some(),
            C::SplitView => self.prefs.workspace.mode == layout::ViewMode::Split,
            C::GarmentView => self.prefs.workspace.mode == layout::ViewMode::Garment,
            C::PatternView => self.prefs.workspace.mode == layout::ViewMode::Patterns,
            C::ToggleInspector => self.prefs.workspace.inspector_open,
            C::ToggleLibrary => self.authoring.library.open,
            C::ToggleSkeleton => self.authoring.pose.skeleton,
            C::ToggleArrangementPoints => self.authoring.arrangement.points,
            C::OpenAvatarEditor => self.authoring.avatar_shape.open,
            C::DraftTshirt => self.authoring.shirt_draft.open,
            C::ToggleHistory => self.authoring.history_ui.open,
            C::ToggleObjectBrowser => self.authoring.object_browser.open,
            C::DraftSkirt => self.authoring.skirt_draft.open,
            C::AttachPins => self.project.template.pins_attached,
            C::DraftTrousers => self.authoring.trousers_draft.open,
            C::OpenPose => self.authoring.pose.open,
            C::OpenAnimation => self.authoring.animation.open,
            C::RecordAnimation => self.authoring.animation.recording(),
            _ => false,
        }
    }

    fn menu_items(
        &mut self,
        ui: &mut egui::Ui,
        items: &[Item],
        ctx: &egui::Context,
        can_open_palette: bool,
    ) {
        for item in items {
            match *item {
                Item::Run(label, id) => {
                    let available = commands::availability(id, self.command_context());
                    let label = i18n::tr(self.prefs.language, label);
                    let mut button = egui::Button::new(label).selected(self.command_checked(id));
                    let keys = keymap::label(self.prefs.keymap, id)
                        .or_else(|| id.descriptor().shortcut.map(String::from));
                    if let Some(keys) = keys {
                        button = button.shortcut_text(keys);
                    }
                    let response = ui
                        .add_enabled(available.is_ok(), button)
                        .on_disabled_hover_text(
                            available.err().map_or("", |reason| reason.message()),
                        );
                    if response.clicked() {
                        self.invoke_command(id, ctx);
                        ui.close();
                    }
                }
                Item::Todo(label) => {
                    let language = self.prefs.language;
                    ui.add_enabled(false, egui::Button::new(i18n::tr(language, label)))
                        .on_disabled_hover_text(i18n::tr(language, "Not built yet"));
                }
                Item::Sub(label, inner) => {
                    ui.menu_button(i18n::tr(self.prefs.language, label), |ui| {
                        self.menu_items(ui, inner, ctx, can_open_palette)
                    });
                }
                Item::Separator => {
                    ui.separator();
                }
                Item::Custom(custom) => self.custom_menu_item(ui, custom, can_open_palette),
            }
        }
    }

    fn custom_menu_item(&mut self, ui: &mut egui::Ui, custom: Custom, can_open_palette: bool) {
        match custom {
            Custom::OnlinePatterns => self.online_patterns(ui),
            Custom::ExportInformation => self.export_information(ui),
            Custom::ToolRail => {
                ui.checkbox(
                    &mut self.tools_visible,
                    i18n::tr(self.prefs.language, "Tool rail"),
                );
            }
            Custom::Presentation => {
                stage_settings::menu_button().ui(ui, |ui| {
                    if let Some(action) =
                        stage_settings::controls(ui, can_open_palette, &mut self.prefs.viewport)
                    {
                        let path = self.output.join(stage_settings::FILE_NAME);
                        self.status = stage_settings::execute(
                            action,
                            &path,
                            &mut self.camera,
                            &mut self.prefs.viewport,
                        )
                        .unwrap_or_else(|e| format!("Presentation: {e}"));
                        ui.close();
                    }
                });
            }
            Custom::Profiles => {
                if ui
                    .add_enabled(
                        can_open_palette,
                        egui::Button::new(i18n::tr(self.prefs.language, "Workspace profiles…")),
                    )
                    .clicked()
                {
                    self.profiles
                        .begin(&self.output.join(workspace_profiles::FILE_NAME));
                    ui.close();
                }
            }
            Custom::Keymap => {
                let language = self.prefs.language;
                ui.menu_button(i18n::tr(language, "Keyboard map"), |ui| {
                    for map in keymap::KeymapName::ALL {
                        if ui
                            .selectable_label(
                                self.prefs.keymap == map,
                                i18n::tr(language, map.label()),
                            )
                            .clicked()
                        {
                            self.prefs.keymap = map;
                            self.prefs_dirty = true;
                            ui.close();
                        }
                    }
                });
            }
            Custom::Language => {
                let language = self.prefs.language;
                ui.menu_button(i18n::tr(language, "Language"), |ui| {
                    for choice in i18n::Language::ALL {
                        if ui
                            .selectable_label(language == choice, choice.name())
                            .clicked()
                        {
                            self.prefs.language = choice;
                            self.prefs_dirty = true;
                            ui.close();
                        }
                    }
                });
            }
            Custom::Coverage => {
                let (done, all) = coverage();
                ui.label(i18n::coverage_line(self.prefs.language, done, all));
            }
        }
    }

    /// The menus of the top bar.
    pub(super) fn menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        can_open_palette: bool,
    ) {
        for (title, items) in MENUS {
            let title = i18n::tr(self.prefs.language, title);
            ui.menu_button(title, |ui| {
                self.menu_items(ui, items, ctx, can_open_palette)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(items: &[Item], out: &mut Vec<CommandId>) {
        for item in items {
            match item {
                Item::Run(_, id) => out.push(*id),
                Item::Sub(_, inner) => runs(inner, out),
                _ => {}
            }
        }
    }

    #[test]
    fn coverage_only_grows() {
        let (done, all) = coverage();
        assert!(done <= all);
        // Raise these as items get built; a drop means a menu item lost its command.
        assert!(done >= 207, "{done} of {all}");
        assert!(all >= 150, "{done} of {all}");
    }

    #[test]
    fn every_menu_command_is_in_the_registry_and_labels_are_unique_per_menu() {
        for (title, items) in MENUS {
            let mut ids = vec![];
            runs(items, &mut ids);
            for id in &ids {
                // descriptor() panics for a command missing from the registry.
                let _ = id.descriptor();
            }
            let mut labels = std::collections::BTreeSet::new();
            for item in *items {
                let label = match item {
                    Item::Run(l, _) | Item::Todo(l) | Item::Sub(l, _) => *l,
                    _ => continue,
                };
                assert!(labels.insert(label), "{title}: {label} twice");
            }
        }
    }
}
