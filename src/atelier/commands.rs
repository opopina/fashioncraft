//! Typed descriptions of existing native workspace actions.
//!
//! A command is not an editor implementation: `Atelier::dispatch_command`
//! remains the sole live adapter. Future toolbars and automation clients can
//! inspect this registry but must use that checked adapter to perform actions.
//! This catalog intentionally omits planned tools and experimental backends.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandId {
    ToggleSimulation,
    StepSimulation,
    ResetSimulation,
    ExportSession,
    SplitView,
    GarmentView,
    PatternView,
    ToggleInspector,
    ResetLayout,
    OpenPreferences,
    LoadTshirt,
    LoadDress,
    ToolEdit,
    ToolDraw,
    ToolFreehand,
    ToolRectangle,
    ToolEllipse,
    ToolSector,
    ToolInternalLine,
    ToolSew,
    ToolMove,
    ToolRotate,
    ToolGrab,
    ToolMeasure,
    ToolInspect,
    ToolGraphic,
    LockSelectedPieces,
    LockAllPieces,
    UnlockAllPieces,
    LockInternalLines,
    UnlockInternalLines,
    InvertSelection,
    ResetSelectedPieces3d,
    NewProject,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    RecoverSnapshot,
    ImportAvatar,
    ImportDxf,
    ImportSvg,
    OpenTemplates,
    Undo,
    Redo,
    ViewFront,
    ViewBack,
    ViewAvatarRight,
    ViewAvatarLeft,
    ViewTop,
    ViewBottom,
    ViewThreeQuarterRight,
    ViewThreeQuarterLeft,
    FrameView,
    ToggleAvatar,
    ToggleGarment,
    IsolateSelection,
    StrainMap,
    TechnicalView,
    RandomPieceColours,
    SelectAllPieces,
    CopyPieces,
    PastePieces,
    DeletePieces,
    DuplicatePiece,
    DeactivatePieces,
    DeactivatePiecesWithSeams,
    ActivatePieces,
    FreezePieces,
    UnfreezePieces,
    StrengthenPieces,
    SolidifyPieces,
    FlipPieceNormals,
    SuperimposeOver,
    SuperimposeUnder,
    AddButton,
    AddButtonhole,
    FastenButtons,
    AddGraphic,
    ToggleLibrary,
    ArchivePieces,
    CutAndSew,
    MergeAlongSeam,
    UnfoldPiece,
    ExtendLineEnds,
    AddDiamondDart,
    OpenPose,
    ToggleSkeleton,
    RestPose,
    OpenSceneObjects,
    ClearSceneObjects,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandContext {
    Simulation,
    File,
    Workspace,
    Example,
    Tool,
    /// Edits the pattern document; needs one loaded.
    Pattern,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoPolicy {
    /// The operation has no authoring-history undo command.
    NotUndoable,
    /// Saved as a user preference, outside the garment authoring history.
    Preference,
    /// Restores only the previous template through authoring history, not
    /// simulated positions, velocities, solver caches or files on disk.
    TemplateHistory,
    /// Writes files; undo does not remove or restore files on disk.
    ExternalSideEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    NativeWorkspace,
    SimulationWorker,
    LocalFilesystem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub id: CommandId,
    pub key: &'static str,
    pub label: &'static str,
    pub context: CommandContext,
    /// None means no shortcut is wired; do not advertise speculative bindings.
    pub shortcut: Option<&'static str>,
    pub undo: UndoPolicy,
    pub backend: Backend,
}

macro_rules! entry {
    ($id:ident, $key:literal, $label:literal, $context:ident, $shortcut:expr, $undo:ident, $backend:ident) => {
        CommandDescriptor {
            id: CommandId::$id,
            key: $key,
            label: $label,
            context: CommandContext::$context,
            shortcut: $shortcut,
            undo: UndoPolicy::$undo,
            backend: Backend::$backend,
        }
    };
}

pub const REGISTRY: &[CommandDescriptor] = &[
    entry!(
        ToggleSimulation,
        "simulation.toggle",
        "Start or pause simulation",
        Simulation,
        Some("Space"),
        NotUndoable,
        SimulationWorker
    ),
    entry!(
        StepSimulation,
        "simulation.step",
        "Advance one simulation step",
        Simulation,
        None,
        NotUndoable,
        SimulationWorker
    ),
    entry!(
        ResetSimulation,
        "simulation.reset",
        "Reset flat arrangement",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportSession,
        "file.export",
        "Export session artifacts",
        File,
        None,
        ExternalSideEffect,
        LocalFilesystem
    ),
    entry!(
        SplitView,
        "workspace.view_split",
        "Split view",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        GarmentView,
        "workspace.view_garment",
        "3D only",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        PatternView,
        "workspace.view_patterns",
        "2D only",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ToggleInspector,
        "workspace.inspector",
        "Toggle properties inspector",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ResetLayout,
        "workspace.reset_layout",
        "Reset workspace layout",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        OpenPreferences,
        "workspace.preferences",
        "Open preferences",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        LoadTshirt,
        "example.tshirt",
        "Load T-shirt example",
        Example,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        LoadDress,
        "example.dress",
        "Load dress example",
        Example,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ToolEdit,
        "tool.edit",
        "Select and edit",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolDraw,
        "tool.draw",
        "Draw pattern",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolFreehand,
        "tool.freehand",
        "Draw freehand pattern",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolRectangle,
        "tool.rectangle",
        "Draw rectangle",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolEllipse,
        "tool.ellipse",
        "Draw ellipse",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolSector,
        "tool.sector",
        "Draw arc sector",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolInternalLine,
        "tool.internal_line",
        "Draw internal line",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolSew,
        "tool.sew",
        "Sew boundaries",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolMove,
        "tool.move_3d",
        "Move panel in 3D",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolRotate,
        "tool.rotate_3d",
        "Rotate panel in 3D",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolGrab,
        "tool.grab",
        "Grab cloth",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolMeasure,
        "tool.measure",
        "Measure on 2D board",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolInspect,
        "tool.inspect",
        "Mark simulation issue",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToolGraphic,
        "tool.graphic",
        "Transform graphic",
        Tool,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        LockSelectedPieces,
        "pattern.lock_selected",
        "Lock selected pieces in 2D",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        LockAllPieces,
        "pattern.lock_all",
        "Lock all pieces in 2D",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        UnlockAllPieces,
        "pattern.unlock_all",
        "Unlock all pieces in 2D",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        LockInternalLines,
        "pattern.lock_internal_lines",
        "Lock all internal lines in 2D",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        UnlockInternalLines,
        "pattern.unlock_internal_lines",
        "Unlock all internal lines in 2D",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        InvertSelection,
        "selection.invert",
        "Invert piece selection",
        Pattern,
        Some("Shift+Cmd+I"),
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ResetSelectedPieces3d,
        "simulation.reset_selected",
        "Reset 3D arrangement of selected pieces",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        NewProject,
        "file.new",
        "New project",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        OpenProject,
        "file.open",
        "Open project…",
        Workspace,
        Some("Cmd+O"),
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        SaveProject,
        "file.save",
        "Save project",
        File,
        Some("Cmd+S"),
        ExternalSideEffect,
        LocalFilesystem
    ),
    entry!(
        SaveProjectAs,
        "file.save_as",
        "Save project as…",
        File,
        Some("Shift+Cmd+S"),
        ExternalSideEffect,
        LocalFilesystem
    ),
    entry!(
        RecoverSnapshot,
        "file.recover",
        "Recover snapshot…",
        Workspace,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        ImportAvatar,
        "file.import_avatar",
        "Import avatar…",
        File,
        None,
        TemplateHistory,
        LocalFilesystem
    ),
    entry!(
        ImportDxf,
        "file.import_dxf",
        "Import DXF patterns…",
        File,
        None,
        TemplateHistory,
        LocalFilesystem
    ),
    entry!(
        ImportSvg,
        "file.import_svg",
        "Import SVG patterns…",
        File,
        None,
        TemplateHistory,
        LocalFilesystem
    ),
    entry!(
        OpenTemplates,
        "file.templates",
        "Garment templates…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        Undo,
        "edit.undo",
        "Undo",
        Pattern,
        Some("Cmd+Z"),
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        Redo,
        "edit.redo",
        "Redo",
        Pattern,
        Some("Shift+Cmd+Z"),
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ViewFront,
        "view.front",
        "Front view",
        Workspace,
        Some("2"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewBack,
        "view.back",
        "Back view",
        Workspace,
        Some("8"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewAvatarRight,
        "view.avatar_right",
        "View of the avatar's right side",
        Workspace,
        Some("4"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewAvatarLeft,
        "view.avatar_left",
        "View of the avatar's left side",
        Workspace,
        Some("6"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewTop,
        "view.top",
        "Top view",
        Workspace,
        Some("5"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewBottom,
        "view.bottom",
        "Bottom view",
        Workspace,
        Some("0"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewThreeQuarterRight,
        "view.three_quarter_right",
        "Three-quarter view, avatar's right",
        Workspace,
        Some("1"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ViewThreeQuarterLeft,
        "view.three_quarter_left",
        "Three-quarter view, avatar's left",
        Workspace,
        Some("3"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        FrameView,
        "view.frame",
        "Frame selection",
        Workspace,
        Some("F"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ToggleAvatar,
        "display.avatar",
        "Show avatar",
        Workspace,
        Some("Shift+A"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        ToggleGarment,
        "display.garment",
        "Show garment",
        Workspace,
        Some("Shift+C"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        IsolateSelection,
        "display.isolate",
        "Isolate selection",
        Workspace,
        Some("Shift+I"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        StrainMap,
        "display.strain_map",
        "Strain map",
        Workspace,
        Some("Alt+7"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        TechnicalView,
        "display.technical",
        "Technical view",
        Workspace,
        Some("Alt+0"),
        Preference,
        NativeWorkspace
    ),
    entry!(
        RandomPieceColours,
        "display.random_colours",
        "Random piece colours",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        SelectAllPieces,
        "edit.select_all",
        "Select all pieces",
        Pattern,
        Some("Cmd+A"),
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        CopyPieces,
        "edit.copy",
        "Copy pieces",
        Pattern,
        Some("Cmd+C"),
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        PastePieces,
        "edit.paste",
        "Paste pieces",
        Pattern,
        Some("Cmd+V"),
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        DeletePieces,
        "edit.delete",
        "Delete selected pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        DuplicatePiece,
        "pattern.duplicate",
        "Duplicate piece",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        DeactivatePieces,
        "pattern.deactivate",
        "Deactivate pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        DeactivatePiecesWithSeams,
        "pattern.deactivate_with_seams",
        "Deactivate pieces and their seams",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ActivatePieces,
        "pattern.activate",
        "Activate pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        FreezePieces,
        "pattern.freeze",
        "Freeze pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        UnfreezePieces,
        "pattern.unfreeze",
        "Unfreeze pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        StrengthenPieces,
        "pattern.strengthen",
        "Strengthen pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SolidifyPieces,
        "pattern.solidify",
        "Solidify pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        FlipPieceNormals,
        "pattern.flip_normals",
        "Flip piece normals",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SuperimposeOver,
        "pattern.superimpose_over",
        "Superimpose over",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        SuperimposeUnder,
        "pattern.superimpose_under",
        "Superimpose under",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        AddButton,
        "trim.button",
        "Add a button",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddButtonhole,
        "trim.buttonhole",
        "Add a buttonhole",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        FastenButtons,
        "trim.fasten",
        "Fasten buttons",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddGraphic,
        "graphic.add",
        "Add a graphic…",
        Pattern,
        None,
        TemplateHistory,
        LocalFilesystem
    ),
    entry!(
        ToggleLibrary,
        "window.library",
        "Library",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ArchivePieces,
        "pattern.archive",
        "Archive pieces",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        CutAndSew,
        "pattern.cut_and_sew",
        "Cut and sew along the selected line",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        MergeAlongSeam,
        "pattern.merge",
        "Merge the pieces of the selected seam",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        UnfoldPiece,
        "pattern.unfold",
        "Unfold the piece across the selected segment",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ExtendLineEnds,
        "pattern.extend_line",
        "Extend the selected line to the outline",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddDiamondDart,
        "pattern.diamond_dart",
        "Add a diamond dart to the selected piece",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        OpenPose,
        "avatar.pose",
        "Avatar pose…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToggleSkeleton,
        "display.skeleton",
        "Show the avatar's skeleton",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        RestPose,
        "avatar.rest_pose",
        "Put the avatar at rest",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        OpenSceneObjects,
        "avatar.scene_objects",
        "Scene and props…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ClearSceneObjects,
        "avatar.clear_scene_objects",
        "Delete the scene objects",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
];

impl CommandId {
    pub fn descriptor(self) -> &'static CommandDescriptor {
        REGISTRY
            .iter()
            .find(|d| d.id == self)
            .expect("registry must cover every command")
    }
}

/// Simple, side-effect-free capability snapshot; no fake command is created
/// merely because a future backend happens to be present on a computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityContext {
    pub document_loaded: bool,
    pub selected_panel: bool,
    pub worker_ready: bool,
    pub filesystem_ready: bool,
    pub playing: bool,
    pub pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    NoDocument,
    NoSelectedPanel,
    WorkerUnavailable,
    FilesystemUnavailable,
    SimulationRunning,
    SimulationBusy,
}

impl UnavailableReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoDocument => "No garment document is loaded",
            Self::NoSelectedPanel => "Select a panel first",
            Self::WorkerUnavailable => "The simulation worker is unavailable",
            Self::FilesystemUnavailable => "Local export storage is unavailable",
            Self::SimulationRunning => "Pause simulation before stepping",
            Self::SimulationBusy => "The previous simulation step is still pending",
        }
    }
}

pub fn availability(id: CommandId, ctx: CapabilityContext) -> Result<(), UnavailableReason> {
    let descriptor = id.descriptor();
    if !ctx.document_loaded
        && matches!(
            descriptor.context,
            CommandContext::Simulation | CommandContext::File | CommandContext::Pattern
        )
    {
        return Err(UnavailableReason::NoDocument);
    }
    match descriptor.backend {
        Backend::SimulationWorker if !ctx.worker_ready => {
            return Err(UnavailableReason::WorkerUnavailable);
        }
        Backend::LocalFilesystem if !ctx.filesystem_ready => {
            return Err(UnavailableReason::FilesystemUnavailable);
        }
        _ => {}
    }
    if matches!(
        id,
        CommandId::ToolMove
            | CommandId::ToolRotate
            | CommandId::LockSelectedPieces
            | CommandId::CopyPieces
            | CommandId::DeletePieces
            | CommandId::DuplicatePiece
            | CommandId::DeactivatePieces
            | CommandId::DeactivatePiecesWithSeams
            | CommandId::ActivatePieces
            | CommandId::FreezePieces
            | CommandId::UnfreezePieces
            | CommandId::StrengthenPieces
            | CommandId::SolidifyPieces
            | CommandId::FlipPieceNormals
            | CommandId::SuperimposeOver
            | CommandId::SuperimposeUnder
            | CommandId::AddButton
            | CommandId::AddButtonhole
            | CommandId::AddGraphic
            | CommandId::ArchivePieces
            | CommandId::AddDiamondDart
    ) && !ctx.selected_panel
    {
        return Err(UnavailableReason::NoSelectedPanel);
    }
    if id == CommandId::StepSimulation {
        if ctx.playing {
            return Err(UnavailableReason::SimulationRunning);
        }
        if ctx.pending {
            return Err(UnavailableReason::SimulationBusy);
        }
    }
    Ok(())
}
