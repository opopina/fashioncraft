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
    SlashSpread,
    ReplaceOutline,
    MergeAlongSeam,
    UnfoldPiece,
    ExtendLineEnds,
    AddDiamondDart,
    OpenPose,
    ToggleSkeleton,
    RestPose,
    OpenSceneObjects,
    ClearSceneObjects,
    OpenAnimation,
    PlayAnimation,
    RecordAnimation,
    DeleteAnimation,
    ExportPointCache,
    ExportObjSequence,
    CaptureView3d,
    CaptureVideo,
    ExportGltf,
    ExportObj,
    ExportObjSelected,
    ExportMdd,
    ExportMddZUp,
    ExportUsd,
    ExportFbx,
    ExportAlembic,
    SnapshotPatterns,
    TracePen3d,
    ToggleArrangementPoints,
    OpenAvatarEditor,
    DraftTshirt,
    ToggleHistory,
    ToggleObjectBrowser,
    DraftSkirt,
    DraftTrousers,
    ImportBvh,
    AttachPins,
    DetachPins,
    ImportPointCache,
    AddSpiral,
    AddInternalRectangle,
    AddInternalEllipse,
    CheckSewingLength,
    AddZipper,
    AddTopstitch,
    ToggleSmartGuides,
    ToggleSnapToGrid,
    ToggleSnapToPattern,
    SimulationProperties,
    StressMap,
    SaveGarment,
    AddGarment,
    SavePoseFile,
    LoadPoseFile,
    PinBox,
    PinLasso,
    ShowPins,
    ShowTrims,
    ShowGraphics,
    HighResolution,
    LowResolution,
    StyleTexturedThick,
    StyleTextured,
    StylePlain,
    MeasureBodyTape,
    MeasureBodyHeight,
    MeasureBodyGirth,
    MeasureGarmentLine,
    MeasureGarmentGirth,
    ShowInternalLines,
    RefreshTextures,
    SaveAvatar,
    OpenUserManual,
    OpenWhatsNew,
    ShowPieceNames,
    SmoothCurve,
    SplitSegment,
    AddTape,
    SeamTaping,
    SeamlineTopstitch,
    FreeTopstitch,
    AutoSewing,
    StyleTranslucent,
    GizmoWorldAxes,
    GizmoLocalAxes,
    GizmoScreenAxes,
    CameraProperties,
    FreeSewing,
    ChainSegmentSewing,
    ChainFreeSewing,
    Pleats,
    SinglePleat,
    FoldArrangement,
    SelectAllSeams,
    ActivateAllSeams,
    PressureMap,
    Reset2dArrangement,
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
        SlashSpread,
        "pattern.slash_spread",
        "Slash and spread along the selected line",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ReplaceOutline,
        "pattern.replace_outline",
        "Replace the outline with the selected shape",
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
    entry!(
        OpenAnimation,
        "animation.editor",
        "Animation editor",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        PlayAnimation,
        "animation.play",
        "Play or pause the animation",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        RecordAnimation,
        "animation.record",
        "Record the cloth along the animation",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        DeleteAnimation,
        "animation.delete",
        "Delete the animation",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportPointCache,
        "animation.export_pc2",
        "Export the take as a point cache…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportObjSequence,
        "animation.export_obj_sequence",
        "Export the take as an OBJ sequence…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        CaptureView3d,
        "capture.view3d",
        "Save the 3D view as PNG…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        CaptureVideo,
        "animation.video",
        "Capture the take as a video…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportGltf,
        "export.gltf",
        "Export glTF / GLB…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportObj,
        "export.obj",
        "Export OBJ…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportObjSelected,
        "export.obj_selected",
        "Export the selected pieces as OBJ…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportMdd,
        "animation.export_mdd",
        "Export the take as an MDD cache…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportMddZUp,
        "animation.export_mdd_z_up",
        "Export the take as an MDD cache for Maya or 3ds Max…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportUsd,
        "export.usd",
        "Export USD / USDZ…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportFbx,
        "export.fbx",
        "Export FBX…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ExportAlembic,
        "export.alembic",
        "Export Alembic…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        TracePen3d,
        "3d.trace_pen",
        "Trace a piece on the body (3D pen)",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        SnapshotPatterns,
        "capture.patterns_1to1",
        "Save the 2D patterns at 1:1…",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToggleArrangementPoints,
        "display.arrangement_points",
        "Show arrangement points",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        OpenAvatarEditor,
        "avatar.editor",
        "Avatar editor…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        DraftTshirt,
        "pattern.draft_tshirt",
        "T-shirt from measurements…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ToggleHistory,
        "window.history",
        "History",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ToggleObjectBrowser,
        "window.object_browser",
        "Object browser",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        DraftSkirt,
        "pattern.draft_skirt",
        "Skirt from measurements…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        DraftTrousers,
        "pattern.draft_trousers",
        "Trousers from measurements…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ImportBvh,
        "animation.import_bvh",
        "Import a BVH motion…",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        AttachPins,
        "pins.attach_to_avatar",
        "Attach all pins to the avatar",
        Workspace,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        DetachPins,
        "pins.detach_from_avatar",
        "Detach all pins from the avatar",
        Workspace,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ImportPointCache,
        "file.import_point_cache",
        "Import a point cache…",
        Simulation,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        AddSpiral,
        "pattern.add_spiral",
        "Add a spiral at the board centre",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddInternalRectangle,
        "pattern.internal_rectangle",
        "Add an internal rectangle to the selected piece",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddInternalEllipse,
        "pattern.internal_ellipse",
        "Add an internal ellipse to the selected piece",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        CheckSewingLength,
        "sewing.check_length",
        "Check sewing lengths",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        AddZipper,
        "trims.zipper",
        "Zipper along the selected segment",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddTopstitch,
        "trims.topstitch",
        "Topstitch along the selected segment",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        ToggleSmartGuides,
        "board.smart_guides",
        "Smart guides on the board",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ToggleSnapToGrid,
        "board.snap_grid",
        "Snap to the board grid",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ToggleSnapToPattern,
        "board.snap_pattern",
        "Snap to pattern points and lines",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        SimulationProperties,
        "workspace.simulation_properties",
        "Simulation properties",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        StressMap,
        "display.stress_map",
        "Stress map",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        SaveGarment,
        "file.save_garment",
        "Save the garment as a template…",
        File,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        AddGarment,
        "file.add_garment",
        "Add a garment from a file…",
        File,
        None,
        TemplateHistory,
        LocalFilesystem
    ),
    entry!(
        SavePoseFile,
        "file.save_pose",
        "Save the pose to a file…",
        File,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        LoadPoseFile,
        "file.open_pose",
        "Open a pose file…",
        Simulation,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        PinBox,
        "pins.box",
        "Pin the cloth inside a box",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        PinLasso,
        "pins.lasso",
        "Pin the cloth inside a lasso",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ShowPins,
        "display.pins",
        "Show pins",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ShowTrims,
        "display.trims",
        "Show trims",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        ShowGraphics,
        "display.graphics",
        "Show graphics",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        HighResolution,
        "simulation.high_resolution",
        "High-resolution garment (10 mm mesh)",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        LowResolution,
        "simulation.low_resolution",
        "Low-resolution garment (30 mm mesh)",
        Simulation,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        StyleTexturedThick,
        "display.style_textured_thick",
        "Garment style: textured with thickness",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        StyleTextured,
        "display.style_textured",
        "Garment style: textured",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        StylePlain,
        "display.style_plain",
        "Garment style: plain colour",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        MeasureBodyTape,
        "measure.body_tape",
        "Measure along the body",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        MeasureBodyHeight,
        "measure.body_height",
        "Measure a height on the body",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        MeasureBodyGirth,
        "measure.body_girth",
        "Measure around the body",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        MeasureGarmentLine,
        "measure.garment_line",
        "Measure along the garment",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        MeasureGarmentGirth,
        "measure.garment_girth",
        "Measure around the garment",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ShowInternalLines,
        "display.internal_lines",
        "Show internal lines",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        RefreshTextures,
        "materials.refresh_textures",
        "Reload textures and graphics from their files",
        Workspace,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        SaveAvatar,
        "file.save_avatar",
        "Save the posed avatar as OBJ…",
        Simulation,
        None,
        NotUndoable,
        LocalFilesystem
    ),
    entry!(
        OpenUserManual,
        "help.user_manual",
        "User manual (web)",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        OpenWhatsNew,
        "help.whats_new",
        "What's new (web)",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ShowPieceNames,
        "display.piece_names_3d",
        "Show piece names in 3D",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        SmoothCurve,
        "pattern.smooth_curve",
        "Round the selected points into a smooth curve",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SplitSegment,
        "pattern.split_segment",
        "Add a point at the middle of the selected segment",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AddTape,
        "materials.tape",
        "Tape along the selected line",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SeamTaping,
        "materials.seam_taping",
        "Tape along both sides of the selected seam",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SeamlineTopstitch,
        "materials.seamline_topstitch",
        "Topstitch along both sides of the selected seam",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        FreeTopstitch,
        "materials.free_topstitch",
        "Topstitch along the selected internal line",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        AutoSewing,
        "sewing.auto",
        "Sew matching segments automatically",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        StyleTranslucent,
        "display.style_translucent",
        "Garment style: translucent",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        GizmoWorldAxes,
        "settings.gizmo_world",
        "Gizmo: world axes",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        GizmoLocalAxes,
        "settings.gizmo_local",
        "Gizmo: the piece's local axes",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        GizmoScreenAxes,
        "settings.gizmo_screen",
        "Gizmo: screen axes",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        CameraProperties,
        "view.camera_properties",
        "Camera properties",
        Workspace,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        FreeSewing,
        "sewing.free",
        "Free sewing: sew parts of segments",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ChainSegmentSewing,
        "sewing.chain_segments",
        "M:N segment sewing: several segments per side",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ChainFreeSewing,
        "sewing.chain_free",
        "M:N free sewing: several parts of segments per side",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        Pleats,
        "pattern.pleats",
        "Pleats along the selected segment",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SinglePleat,
        "pattern.single_pleat",
        "One pleat along the selected segment",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        SelectAllSeams,
        "edit.select_all_seams",
        "Select all seams",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        ActivateAllSeams,
        "sewing.activate_all",
        "Switch every seam back on",
        Pattern,
        None,
        TemplateHistory,
        NativeWorkspace
    ),
    entry!(
        PressureMap,
        "display.pressure_map",
        "Pressure map: what the garment presses on the avatar",
        Workspace,
        None,
        Preference,
        NativeWorkspace
    ),
    entry!(
        Reset2dArrangement,
        "garment.reset_2d",
        "Reset 2D arrangement: lay every piece out flat as on the board",
        Pattern,
        None,
        NotUndoable,
        NativeWorkspace
    ),
    entry!(
        FoldArrangement,
        "garment.fold_arrangement",
        "Fold arrangement: start folded along the selected internal line",
        Pattern,
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
            | CommandId::AddInternalRectangle
            | CommandId::AddInternalEllipse
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
