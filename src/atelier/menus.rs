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
            Todo("Garment…"),
            Todo("Trim…"),
            Run("Avatar…", C::ImportAvatar),
            Todo("Pose…"),
            Run("Scene and props…", C::OpenSceneObjects),
        ],
    ),
    Sub(
        "Add",
        &[Todo("Project…"), Todo("Garment…"), Todo("Avatar…")],
    ),
    Run("Save project", C::SaveProject),
    Sub(
        "Save as",
        &[
            Run("Project…", C::SaveProjectAs),
            Todo("Garment…"),
            Todo("Avatar…"),
            Todo("Pose…"),
        ],
    ),
    S,
    Sub(
        "Import",
        &[
            Run("SVG patterns…", C::ImportSvg),
            Run("DXF patterns…", C::ImportDxf),
            Todo("OBJ…"),
            Todo("FBX…"),
            Todo("glTF…"),
            Todo("Alembic…"),
            Todo("USD…"),
            Todo("Point cache…"),
            Todo("Python script…"),
        ],
    ),
    Sub(
        "Export",
        &[
            Run("Session files (OBJ, patterns, previews)", C::ExportSession),
            Todo("OBJ (selected)…"),
            Todo("FBX…"),
            Run("glTF / GLB…", C::ExportGltf),
            Todo("Alembic…"),
            Todo("USD…"),
            Run("Point cache…", C::ExportPointCache),
            Run("OBJ sequence…", C::ExportObjSequence),
        ],
    ),
    S,
    Sub(
        "Snapshot",
        &[
            Todo("2D patterns at 1:1…"),
            Run("3D view…", C::CaptureView3d),
        ],
    ),
    S,
    Run("Recover snapshot…", C::RecoverSnapshot),
    Run("Garment templates…", C::OpenTemplates),
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
    Todo("Select all seams"),
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
    Todo("High-resolution garment"),
    Todo("Low-resolution garment"),
    Todo("Custom resolution…"),
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
            Todo("Convert to quads"),
            Todo("Convert to triangles"),
            S,
            Run("Superimpose over", C::SuperimposeOver),
            Run("Superimpose under", C::SuperimposeUnder),
            Todo("Superimpose side"),
        ],
    ),
    S,
    Run("Move in 3D", C::ToolMove),
    Run("Rotate in 3D", C::ToolRotate),
    Run("Grab cloth", C::ToolGrab),
    Todo("Select mesh (brush)"),
    Todo("Select mesh (box)"),
    Todo("Select mesh (lasso)"),
    S,
    Sub(
        "Pins",
        &[
            Todo("Pin (box)"),
            Todo("Pin (lasso)"),
            Todo("Attach all pins to avatar"),
            Todo("Detach all pins from avatar"),
        ],
    ),
    Todo("Style lines"),
    Todo("3D pen"),
    S,
    Todo("Fold arrangement"),
    Todo("Reset 2D arrangement (all)"),
    Run("Reset 3D arrangement (all)", C::ResetSimulation),
    Run("Reset 3D arrangement (selected)", C::ResetSelectedPieces3d),
    Todo("Re-drape 3D arrangement"),
    S,
    Sub(
        "Garment measurements",
        &[Todo("Linear measure"), Todo("Circumference measure")],
    ),
    Sub("Tear", &[Todo("Edit tears"), Todo("Tear seam")]),
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
            Run("Merge pieces along seam", C::MergeAlongSeam),
            Run("Unfold across segment", C::UnfoldPiece),
            Run("Extend line to outline", C::ExtendLineEnds),
            Todo("Add point / split line"),
            Todo("Smooth curve"),
            Todo("Cut and extend"),
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
            Todo("Spiral"),
            Run("Internal polygon or line", C::ToolInternalLine),
            Todo("Internal rectangle"),
            Todo("Internal ellipse"),
            Run("Diamond dart", C::AddDiamondDart),
        ],
    ),
    Todo("Trace"),
    Run("Measure on the board", C::ToolMeasure),
    Sub(
        "Pleats",
        &[Todo("Pleats"), Todo("Single pleat"), Todo("Pleat sewing")],
    ),
    Todo("Auto fit"),
];

const SEWING: &[Item] = &[
    Run("Segment sewing", C::ToolSew),
    Todo("Free sewing"),
    Todo("M:N segment sewing"),
    Todo("M:N free sewing"),
    Todo("Auto sewing"),
    Todo("Check sewing length"),
    Todo("Activate all seams"),
];

const MATERIALS: &[Item] = &[
    Sub(
        "Texture",
        &[
            Todo("Edit texture (2D)"),
            Todo("Edit texture (3D)"),
            Todo("Refresh textures"),
        ],
    ),
    Sub(
        "Graphics",
        &[
            Run("Transform graphic", C::ToolGraphic),
            Run("Graphic on the selected piece…", C::AddGraphic),
            Todo("Graphic on the 3D garment"),
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
    Todo("Zipper"),
    Todo("Piping"),
    Todo("Tape"),
    Sub(
        "Topstitch",
        &[
            Todo("Segment topstitch"),
            Todo("Free topstitch"),
            Todo("Seamline topstitch"),
        ],
    ),
    Todo("Puckering"),
    Todo("Steam"),
    Todo("Seam taping"),
];

const AVATAR: &[Item] = &[
    Todo("Delete avatar"),
    Run("Delete scene and props", C::ClearSceneObjects),
    S,
    Sub(
        "Avatar measurements",
        &[
            Todo("Tape measure"),
            Todo("Height measure"),
            Todo("Circumference measure"),
        ],
    ),
    Run("Pose…", C::OpenPose),
    Run("Rest pose", C::RestPose),
    Run("Play animation", C::PlayAnimation),
    Todo("Avatar editor…"),
];

const ANIMATION: &[Item] = &[
    Run("Animation editor", C::OpenAnimation),
    Run("Record", C::RecordAnimation),
    Run("Delete animation", C::DeleteAnimation),
    S,
    Run("Video capture…", C::CaptureVideo),
];

const RENDER: &[Item] = &[
    Todo("High-quality 3D view"),
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
            Todo("Show pins"),
            Todo("Show piece names"),
            Todo("Show internal lines"),
            Todo("Show graphics"),
            Todo("Show trims"),
        ],
    ),
    Sub(
        "Avatar",
        &[
            Run("Show avatar", C::ToggleAvatar),
            Todo("Show arrangement points"),
            Run("Show joints", C::ToggleSkeleton),
        ],
    ),
    Sub(
        "Garment style",
        &[
            Todo("Textured with thickness"),
            Todo("Textured"),
            Todo("Plain colour"),
            Todo("Translucent"),
            Run("Random piece colours", C::RandomPieceColours),
        ],
    ),
    Sub(
        "Fit maps",
        &[
            Run("Strain map", C::StrainMap),
            Todo("Stress map"),
            Todo("Pressure map"),
        ],
    ),
    Run("Isolate selection", C::IsolateSelection),
];

const SETTINGS: &[Item] = &[
    Sub(
        "Gizmo",
        &[Todo("Screen axes"), Todo("Local axes"), Todo("World axes")],
    ),
    Sub(
        "2D snapping",
        &[
            Todo("Smart guides"),
            Todo("Snap to pattern"),
            Todo("Snap to grid"),
        ],
    ),
    Todo("Grid properties…"),
    Todo("Camera properties…"),
    Todo("Simulation properties…"),
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
    Todo("Object browser"),
    Todo("History"),
    S,
    Run("Reset layout", C::ResetLayout),
    X(Custom::ToolRail),
    X(Custom::Presentation),
    X(Custom::Profiles),
];

const HELP: &[Item] = &[
    X(Custom::Coverage),
    S,
    Todo("User manual"),
    Todo("What's new"),
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

/// Menu items that work, out of all the menu items the bar lays out.
pub(super) fn coverage() -> (usize, usize) {
    let (mut done, mut all) = (0, 0);
    for (_, items) in MENUS {
        count(items, &mut done, &mut all);
    }
    (done, all)
}

impl Atelier {
    /// Whether a toggle command is on, for its menu item's highlight.
    fn command_checked(&self, id: CommandId) -> bool {
        use CommandId as C;
        let display = &self.prefs.garment_display;
        match id {
            C::ToggleAvatar => display.avatar,
            C::ToggleGarment => display.garment,
            C::StrainMap => self.heat,
            C::TechnicalView => display.schematic,
            C::RandomPieceColours => display.piece_colors,
            C::IsolateSelection => self.authoring.view3d.isolated.is_some(),
            C::SplitView => self.prefs.workspace.mode == layout::ViewMode::Split,
            C::GarmentView => self.prefs.workspace.mode == layout::ViewMode::Garment,
            C::PatternView => self.prefs.workspace.mode == layout::ViewMode::Patterns,
            C::ToggleInspector => self.prefs.workspace.inspector_open,
            C::ToggleLibrary => self.authoring.library.open,
            C::ToggleSkeleton => self.authoring.pose.skeleton,
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
        assert!(done >= 99, "{done} of {all}");
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
