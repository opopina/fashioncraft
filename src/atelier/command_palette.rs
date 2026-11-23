//! Search and input adapter; execution belongs to Atelier::dispatch_command.
use super::{
    authoring_ui::Tool,
    commands::{self, CapabilityContext, CommandId},
};
use eframe::egui;

#[derive(Default)]
pub(super) struct Palette {
    pub open: bool,
    /// Interface language: labels show in it and searches also match them in it.
    pub language: super::i18n::Language,
    query: String,
    selected: usize,
    focus_search: bool,
}

fn aliases(id: CommandId) -> &'static str {
    use CommandId::*;
    match id {
        ToggleSimulation => "play pause run simulate",
        StepSimulation => "step frame advance",
        ResetSimulation => "reset flat arrange restart",
        ExportSession => "export write artifacts",
        SplitView => "split both 2d 3d",
        GarmentView => "garment cloth 3d viewport",
        PatternView => "pattern flat 2d viewport",
        ToggleInspector => "properties inspector sidebar",
        ResetLayout => "layout workspace reset",
        OpenPreferences => "settings preferences options theme",
        LoadTshirt => "tshirt t-shirt tee example preset",
        LoadDress => "dress example preset",
        ToolEdit => "select edit point piece",
        ToolDraw => "pen bezier pattern cloth",
        ToolFreehand => "freehand sketch stroke outline pattern",
        ToolRectangle => "square shape primitive",
        ToolEllipse => "circle shape primitive",
        ToolSector => "arc shape primitive",
        ToolInternalLine => "internal path curve frill",
        ToolSew => "seam stitch boundary",
        ToolMove => "place arrange translate gizmo",
        ToolRotate => "place arrange rotation gizmo",
        ToolGrab => "pull drag cloth",
        ToolMeasure => "ruler tape distance",
        ToolInspect => "debug report paint faces",
        ToolGraphic => "print logo image move scale rotate graphic",
        NewProject => "new blank empty document",
        OpenProject => "open load document json",
        SaveProject => "save write document",
        SaveProjectAs => "save as copy document",
        RecoverSnapshot => "recovery autosave crash",
        ImportAvatar => "avatar body obj mannequin import",
        ImportDxf => "dxf aama astm cad import",
        ImportDxfAama => "dxf aama astm d6673 cad plotter cutter workshop import pattern",
        ImportSvg => "svg vector import",
        OpenTemplates => "template library catalog block",
        Undo => "undo back revert",
        Redo => "redo again forward",
        ViewFront => "camera front",
        ViewBack => "camera back rear",
        ViewAvatarRight => "camera side right profile",
        ViewAvatarLeft => "camera side left profile",
        ViewTop => "camera top above",
        ViewBottom => "camera bottom below",
        ViewThreeQuarterRight => "camera three quarter 3/4",
        ViewThreeQuarterLeft => "camera three quarter 3/4",
        FrameView => "zoom fit focus frame all",
        ToggleAvatar => "hide show avatar body",
        ToggleGarment => "hide show garment cloth",
        IsolateSelection => "isolate solo hide others",
        StrainMap => "fit map stretch heat strain",
        TechnicalView => "schematic flat sketch lines",
        RandomPieceColours => "colour color pieces distinct",
        SelectAllPieces => "select all every piece",
        CopyPieces => "copy clipboard",
        PastePieces => "paste clipboard",
        DeletePieces => "delete remove erase",
        DuplicatePiece => "duplicate clone copy",
        DeactivatePieces => "deactivate hold pass through",
        DeactivatePiecesWithSeams => "deactivate seams off",
        ActivatePieces => "activate enable simulate",
        FreezePieces => "freeze hold fixed",
        UnfreezePieces => "unfreeze thaw release",
        StrengthenPieces => "strengthen stiffer harden",
        SolidifyPieces => "solidify rigid shape keep",
        FlipPieceNormals => "flip normal face front back",
        SuperimposeOver => "superimpose outside layer over",
        SuperimposeUnder => "superimpose inside layer under",
        AddButton => "button trim",
        AddButtonhole => "buttonhole trim hole",
        FastenButtons => "fasten button close",
        AddGraphic => "graphic print logo image png",
        ToggleLibrary => "library assets folder browse files",
        ArchivePieces => "archive shelve put away hide pieces",
        CutAndSew => "cut sew split slash divide piece line",
        ReplaceOutline => "replace outline contour shape boundary reemplazar contorno",
        SlashSpread => "slash spread open wedge pivot dart abrir separar cuna",
        MergeAlongSeam => "merge join combine pieces seam",
        UnfoldPiece => "unfold mirror half symmetric fold open",
        ExtendLineEnds => "extend prolong internal line outline",
        AddDiamondDart => "diamond dart fish eye waist shaping",
        OpenPose => "pose avatar body arms legs posture",
        ToggleSkeleton => "skeleton bones joints x-ray",
        RestPose => "rest pose reset avatar a-pose",
        OpenSceneObjects => "scene props objects obj chair table",
        ClearSceneObjects => "delete clear scene props objects",
        OpenAnimation => "animation timeline keys keyframes motion markers",
        PlayAnimation => "play pause animation timeline preview",
        RecordAnimation => "record animation cloth cache take",
        DeleteAnimation => "delete clear animation keys",
        ExportPointCache => "export pc2 point cache take animation",
        ExportObjSequence => "export obj sequence frames take animation",
        CaptureView3d => "snapshot capture png screenshot 3d view",
        CaptureVideo => "video capture mp4 movie take animation record",
        ExportGltf => "export gltf glb 3d model web blender unity unreal",
        ExportDxfAama => "export dxf aama astm d6673 cad plotter cutter workshop pattern",
        ExportObj => "export obj mtl mesh wavefront thick uv units",
        ExportObjSelected => "export obj selected pieces mesh",
        ExportMdd => "export mdd point cache take animation lightwave",
        ExportMddZUp => "export mdd point cache maya 3ds max z up",
        ExportUsd => "export usd usdz usda pixar apple ar quick look",
        ExportFbx => "export fbx autodesk maya 3ds max blender unity unreal",
        ExportAlembic => "export alembic abc cache take animation houdini maya blender",
        TracePen3d => "3d pen trace draw body avatar flatten pattern piece",
        SnapshotPatterns => {
            "snapshot print pdf svg patterns 1:1 actual size real scale tile pages a4 letter a3 plotter"
        }
        ToggleArrangementPoints => "arrangement points anchors place avatar show",
        OpenAvatarEditor => "avatar editor measurements height bust waist hip size body",
        CreateMakeHumanAvatar => {
            "makehuman create new avatar body mannequin gender age muscle weight"
        }
        OpenAvatarLibrary => {
            "avatar library presets ready made bodies skin hair eyes woman man child makehuman"
        }
        DraftTshirt => "tshirt t-shirt draft pattern measurements bust hip sleeve size",
        ToggleHistory => "history undo redo edits list go back",
        ToggleAssistant => "assistant ai agent chat claude chatgpt gemini model help ask",
        ToggleObjectBrowser => "object browser scene tree outliner pieces fabrics avatar list",
        DraftSkirt => "skirt draft pattern measurements waist hip flare length size",
        DraftTrousers => {
            "trousers pants draft pattern measurements waist hip rise inseam length size"
        }
        ImportBvh => "bvh motion capture mocap import animation walk dance",
        AttachPins => "attach pins avatar follow body animation hold",
        DetachPins => "detach release pins avatar",
        ImportPointCache => "import point cache pc2 take animation playback",
        AddSpiral => "spiral ruffle cascade strip shape create",
        AddInternalRectangle => "internal rectangle shape hole pocket create",
        AddInternalEllipse => "internal ellipse circle shape hole create",
        CheckSewingLength => "check sewing length seam mismatch audit difference",
        AddZipper => "zipper zip trim segment closure",
        AddTopstitch => "topstitch stitch trim segment decorative",
        ToggleSmartGuides => "smart guides align snapping board",
        ToggleSnapToGrid => "snap grid magnet board",
        ToggleSnapToPattern => "snap pattern points lines edges magnet board",
        SimulationProperties => "simulation properties settings solver inspector",
        StressMap => "stress map tension fit map fabric limit",
        SaveGarment => "save garment template pattern pieces file",
        AddGarment => "add garment merge pieces another file template project",
        SavePoseFile => "save pose file avatar posture",
        LoadPoseFile => "open load pose file avatar posture",
        PinBox => "pin box marquee hold cloth",
        PinLasso => "pin lasso freehand hold cloth",
        ShowPins => "show hide pins heads display",
        ShowTrims => "show hide trims zippers topstitch display",
        ShowGraphics => "show hide graphics prints logos display",
        HighResolution => "high resolution fine mesh particle distance remesh final",
        LowResolution => "low resolution coarse mesh particle distance remesh draft fast",
        StyleTexturedThick => "garment style textured thickness thick render look",
        StyleTextured => "garment style textured thin render look",
        StylePlain => "garment style plain colour color flat untextured look",
        MeasureBodyTape => "measure tape avatar body length distance surface",
        MeasureBodyHeight => "measure height avatar body floor",
        MeasureBodyGirth => "measure circumference girth around avatar body chest waist hip",
        MeasureGarmentLine => "measure linear length garment cloth distance",
        MeasureGarmentGirth => "measure circumference girth around garment cloth",
        ShowInternalLines => "show hide internal lines 3d display",
        RefreshTextures => "refresh reload textures graphics images files",
        SaveAvatar => "save export avatar body posed obj mesh",
        OpenUserManual => "help user manual documentation guide readme",
        OpenWhatsNew => "help whats new changes changelog commits",
        ShowPieceNames => "show piece names labels 3d display",
        SmoothCurve => "smooth curve round points bezier handles",
        SplitSegment => "add point split line segment middle insert",
        AddTape => "tape stay stretch line segment reinforce",
        SeamTaping => "seam taping tape both sides seam",
        SeamlineTopstitch => "seamline topstitch stitch both sides seam",
        FreeTopstitch => "free topstitch stitch internal line path",
        AutoSewing => "auto sewing automatic seams match segments sew all",
        StyleTranslucent => "garment style translucent transparent see through",
        GizmoWorldAxes => "gizmo world axes global move rotate",
        GizmoLocalAxes => "gizmo local axes piece own move rotate",
        GizmoScreenAxes => "gizmo screen axes view camera move rotate",
        CameraProperties => "camera properties field of view fov distance orthographic perspective",
        FreeSewing => "free sewing partial range part segment sew",
        ChainSegmentSewing => "m:n mn segment sewing chain several multiple edges sew",
        ChainFreeSewing => "m:n mn free sewing chain partial several multiple sew",
        Pleats => "pleats pleat knife box inverted fold skirt waist",
        SinglePleat => "single one pleat fold",
        FoldArrangement => "fold arrangement arrange folded line crease pre fold",
        SelectAllSeams => "select all seams sewing every",
        ActivateAllSeams => "activate all seams sewing switch on inactive",
        PressureMap => "pressure map fit map tight contact kpa avatar",
        Reset2dArrangement => "reset 2d arrangement lay out flat board pieces",
        GraphicOn3dGarment => "graphic print logo image png place click garment",
        PleatSewing => "pleat sewing tack close pleats",
        Puckering => "puckering pucker seam ripple wrinkle tight",
        Steam => "steam brush shrink stretch iron",
        Piping => "piping cord binding edge seam trim",
        FrayEdge => "fray frayed edge threads loose raw cut distressed fringe",
        TearSeam => "tear rip seam open ripped distressed",
        EditTears => "edit tears rip seam mend",
        TraceShape => "trace shape pattern piece new",
        CutAndExtend => "cut extend prolong line outline add point",
        SelectMeshBrush => "select mesh brush paint points cloth freeze",
        SelectMeshBox => "select mesh box rectangle points cloth freeze",
        SelectMeshLasso => "select mesh lasso points cloth freeze",
        SubdivideMesh => "subdivide mesh refine finer triangles selection detail resolution",
        ConvertToQuads => "convert quads quad mesh grid squares",
        ConvertToTriangles => "convert triangles triangle mesh",
        HighQualityView => "high quality view render supersample smooth antialias",
        SuperimposeSide => "superimpose side beside lay flat seam",
        DeleteAvatar => "delete remove avatar body mannequin",
        EditTexture2d => "edit texture shift offset position grain repeat 2d board",
        EditTexture3d => "edit texture shift offset position cloth 3d",
        OpenTrim => "open trim object buckle badge button mesh obj fbx gltf",
        AddAvatarObject => "add avatar second body multiple avatars armour collide",
        StyleLineEdit => "style line styleline edit corner bend edge reshape pattern drape",
        StyleLineMove => "style line styleline shift edge reshape pattern drape",
        StyleLineScale => "style line styleline scale resize lengthen shorten edge drape",
        StyleLineDraw => "style line styleline draw internal line on cloth drape",
        ReDrape => "redrape re-drape retarget carry avatar new body keep size",
        AutoFit => "auto fit fitting grade resize avatar new body size",
        RunPythonScript => "python script py automate api run code",
        LockSelectedPieces => "lock freeze protect selected piece pattern 2d",
        LockAllPieces => "lock all pieces patterns 2d",
        UnlockAllPieces => "unlock all pieces patterns 2d",
        LockInternalLines => "lock internal lines shapes 2d",
        UnlockInternalLines => "unlock internal lines shapes 2d",
        InvertSelection => "invert reverse other pieces selection",
        ResetSelectedPieces3d => "reset arrangement flat selected pieces 3d drape",
    }
}

/// Map only implemented selections to the existing editor transition.
pub(super) fn tool_for_command(id: CommandId) -> Option<(Tool, bool)> {
    use CommandId::*;
    Some(match id {
        ToolEdit => (Tool::Edit, false),
        ToolDraw => (Tool::Draw, false),
        ToolFreehand => (Tool::Draw, false),
        ToolRectangle => (Tool::Rectangle, false),
        ToolEllipse => (Tool::Ellipse, false),
        ToolSector => (Tool::Sector, false),
        ToolInternalLine => (Tool::InternalLine, false),
        ToolSew => (Tool::Sew, false),
        ToolMove => (Tool::Place, false),
        ToolRotate => (Tool::Place, true),
        ToolGrab => (Tool::Grab, false),
        ToolMeasure => (Tool::Measure, false),
        ToolInspect => (Tool::Inspect, false),
        ToolGraphic => (Tool::Graphic, false),
        _ => return None,
    })
}

const MAX_QUERY_CHARS: usize = 128;

fn is_subsequence(needle: &str, word: &str) -> bool {
    let mut chars = word.chars();
    needle
        .chars()
        .all(|c| chars.any(|candidate| candidate == c))
}

#[cfg(test)]
fn matches(query: &str) -> Vec<CommandId> {
    matches_in(query, super::i18n::Language::English)
}

fn matches_in(query: &str, language: super::i18n::Language) -> Vec<CommandId> {
    if query.chars().count() > MAX_QUERY_CHARS {
        return Vec::new();
    }
    let query = query.to_lowercase();
    let mut results: Vec<_> = commands::REGISTRY
        .iter()
        .enumerate()
        .filter_map(|(index, d)| {
            let label = d.label.to_lowercase();
            let shown = super::i18n::tr(language, d.label).to_lowercase();
            let key = d.key.to_lowercase();
            let shortcut = d.shortcut.unwrap_or_default().to_lowercase();
            let text = format!("{} {} {} {} {}", label, shown, key, shortcut, aliases(d.id));
            let mut fuzzy_words = 0usize;
            for word in query.split_whitespace() {
                if text.contains(word) {
                    continue;
                }
                if word.chars().count() < 3
                    || !text
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|candidate| is_subsequence(word, candidate))
                {
                    return None;
                }
                fuzzy_words += 1;
            }
            let term = query.trim();
            // Favor an actual command name over a match in a broad alias,
            // while leaving an empty search in the original registry order.
            let relevance = if term.is_empty()
                || label == term
                || shown == term
                || key == term
                || shortcut == term
            {
                0
            } else if label.starts_with(term)
                || shown.starts_with(term)
                || key.starts_with(term)
                || shortcut.starts_with(term)
            {
                1
            } else if label
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w == term)
                || key.split(|c: char| !c.is_alphanumeric()).any(|w| w == term)
            {
                2
            } else if label.contains(term) || key.contains(term) {
                3
            } else {
                4
            };
            Some((fuzzy_words, relevance, index, d.id))
        })
        .collect();
    if results
        .iter()
        .any(|(fuzzy_words, _, _, _)| *fuzzy_words == 0)
    {
        results.retain(|(fuzzy_words, _, _, _)| *fuzzy_words == 0);
    }
    results.sort_by_key(|(fuzzy_words, relevance, index, _)| (*fuzzy_words, *relevance, *index));
    results.into_iter().map(|(_, _, _, id)| id).collect()
}

fn named(_ui: &egui::Ui, _r: &egui::Response, _name: &str) {
    #[cfg(test)]
    super::authoring_smoke::widget_harness::named(_ui, _r, _name);
}

// Shared production boundary for the launcher button and keyboard entrypoint.
// The caller supplies editing/modal state owned by Atelier; egui supplies actual
// text focus and modal ownership. Blocked shortcuts remain with their owner.
pub(super) fn can_launch(ctx: &egui::Context, editor_idle: bool) -> bool {
    editor_idle && !ctx.egui_wants_keyboard_input() && ctx.memory(|m| m.top_modal_layer().is_none())
}

pub(super) fn launch_shortcut(ctx: &egui::Context, enabled: bool) -> bool {
    enabled
        && !ctx.input(|i| i.pointer.any_down())
        && ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers {
                    command: true,
                    shift: true,
                    ..Default::default()
                },
                egui::Key::P,
            )
        })
}

/// The command search of the top bar: a field that opens the command palette, with `label`
/// as its placeholder and the palette's keys on its right.
pub(super) fn launch_button(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    language: super::i18n::Language,
) -> bool {
    let width = ui.available_width().clamp(120., 340.);
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, 26.), egui::Sense::click());
            if ui.is_rect_visible(rect) {
                let visuals = ui.visuals();
                let stroke = if response.hovered() {
                    visuals.widgets.hovered.bg_stroke
                } else {
                    visuals.widgets.inactive.bg_stroke
                };
                ui.painter().rect(
                    rect,
                    5.,
                    visuals.extreme_bg_color,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                let weak = visuals.weak_text_color();
                super::icons::glyph(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        egui::pos2(rect.left() + 16., rect.center().y),
                        egui::Vec2::splat(14.),
                    ),
                    super::icons::Icon::Search,
                    weak,
                    1.5,
                );
                ui.painter().text(
                    egui::pos2(rect.left() + 30., rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(12.5),
                    weak,
                );
                let keys = if cfg!(target_os = "macos") {
                    "⇧⌘P"
                } else {
                    "Ctrl+Shift+P"
                };
                ui.painter().text(
                    egui::pos2(rect.right() - 10., rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    keys,
                    egui::FontId::proportional(11.5),
                    weak,
                );
            }
            response
        })
        .inner
        .on_hover_text(super::i18n::tr(
            language,
            "Search actions · Ctrl/Cmd+Shift+P",
        ));
    named(ui, &response, "palette.open");
    response.clicked()
}

impl Palette {
    pub fn begin(&mut self) {
        *self = Self {
            open: true,
            focus_search: true,
            ..Default::default()
        };
    }

    /// Returns at most one requested command; the caller must revalidate it.
    pub fn show(&mut self, ctx: &egui::Context, context: CapabilityContext) -> Option<CommandId> {
        if !self.open {
            return None;
        }
        if !ctx.input(|i| i.focused) || ctx.input(|i| i.viewport().close_requested()) {
            self.open = false;
            return None;
        }
        let mut command = None;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("command-palette")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.).clamp(200., 520.));
            let lang = self.language;
            ui.heading(super::i18n::tr(lang, "Commands"));
            ui.small(super::i18n::tr(
                lang,
                "Search actions · arrow keys / Page Up Down navigate · Enter run · Esc close",
            ));
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .id(egui::Id::new("palette.query"))
                    .char_limit(MAX_QUERY_CHARS)
                    .hint_text(super::i18n::tr(lang, "Search commands…"))
                    .desired_width(f32::INFINITY),
            );
            named(ui, &response, "palette.query");
            if self.focus_search {
                response.request_focus();
                self.focus_search = false;
            }
            if response.changed() {
                self.selected = 0;
            }
            let results = matches_in(&self.query, lang);
            self.selected = self.selected.min(results.len().saturating_sub(1));
            let available: Vec<usize> = results
                .iter()
                .enumerate()
                .filter_map(|(index, &id)| {
                    commands::availability(id, context).is_ok().then_some(index)
                })
                .collect();
            // When a selected action becomes unavailable, move the highlight
            // but do not let the same Enter press activate a different action.
            let selection_changed = !available.contains(&self.selected) && !available.is_empty();
            if selection_changed {
                self.selected = available[0];
            }
            let (up, down, page_up, page_down, enter, escape) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            });
            if !available.is_empty() {
                let position = available
                    .iter()
                    .position(|&index| index == self.selected)
                    .expect("selected command must be available");
                if down {
                    self.selected = available[(position + 1) % available.len()];
                } else if up {
                    self.selected = available[(position + available.len() - 1) % available.len()];
                } else if page_down {
                    self.selected = available[(position + 5).min(available.len() - 1)];
                } else if page_up {
                    self.selected = available[position.saturating_sub(5)];
                }
                if enter && !selection_changed {
                    command = Some(results[self.selected]);
                }
            }
            // TextEdit relinquishes focus on Enter even if activation was
            // rejected. Keep the palette usable without a new mouse click.
            if enter && command.is_none() {
                response.request_focus();
            }
            cancel |= escape;
            let results_height = (ctx.content_rect().height() - 230.).clamp(80., 380.);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(results_height)
                .min_scrolled_height(results_height)
                .show(ui, |ui| {
                    if results.is_empty() {
                        ui.label(super::i18n::tr(lang, "No matching commands"));
                    }
                    for (index, &id) in results.iter().enumerate() {
                        let descriptor = id.descriptor();
                        let availability = commands::availability(id, context);
                        let name = super::i18n::tr(lang, descriptor.label);
                        let label = match descriptor.shortcut {
                            Some(key) => format!("{name}   {}", super::keymap::shown(key)),
                            None => name.into(),
                        };
                        let row = ui.add_enabled(
                            availability.is_ok(),
                            egui::Button::new(label)
                                .selected(index == self.selected && availability.is_ok()),
                        );
                        named(ui, &row, &format!("palette.command.{}", descriptor.key));
                        if (up
                            || down
                            || page_up
                            || page_down
                            || response.changed()
                            || selection_changed)
                            && index == self.selected
                            && availability.is_ok()
                        {
                            row.scroll_to_me(Some(egui::Align::Center));
                        }
                        if row.clicked() {
                            command = Some(id);
                        }
                        if let Err(reason) = availability {
                            ui.small(super::i18n::tr(lang, reason.message()));
                        }
                    }
                });
            let close = ui.button(super::i18n::tr(lang, "Close"));
            named(ui, &close, "palette.close");
            cancel |= close.clicked();
        });
        cancel |= modal.should_close();
        if cancel {
            command = None;
        }
        if cancel || command.is_some() {
            self.open = false;
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::super::authoring_smoke::widget_harness::Harness;
    use super::*;
    struct Case {
        palette: Palette,
        context: CapabilityContext,
        commands: Vec<CommandId>,
    }
    fn draw(ui: &mut egui::Ui, c: &mut Case) {
        if let Some(id) = c.palette.show(ui.ctx(), c.context) {
            c.commands.push(id);
        }
    }
    fn case() -> Case {
        let mut palette = Palette::default();
        palette.begin();
        Case {
            palette,
            context: CapabilityContext {
                document_loaded: true,
                selected_panel: true,
                worker_ready: true,
                filesystem_ready: true,
                playing: false,
                pending: false,
            },
            commands: vec![],
        }
    }
    fn frames(h: &mut Harness, c: &mut Case) {
        for _ in 0..3 {
            h.frame(c, &mut draw);
        }
    }
    #[test]
    fn launch_boundary_preserves_text_modal_gesture_and_pointer_ownership() {
        let shortcut = egui::Modifiers {
            command: true,
            shift: true,
            ..Default::default()
        };
        // Each case uses actual egui focus/modal/pointer state. The final case
        // is the normal idle workspace and must consume exactly one request.
        for (idle, text, modal, pointer, expected) in [
            (false, false, false, false, false),
            (true, true, false, false, false),
            (true, false, true, false, false),
            (true, false, false, true, false),
            (true, false, false, false, true),
        ] {
            let ctx = egui::Context::default();
            let mut value = String::new();
            let mut observation = None;
            for frame in 0..3 {
                let mut events = Vec::new();
                if frame == 2 {
                    events.push(egui::Event::Key {
                        key: egui::Key::P,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: shortcut,
                    });
                    if pointer {
                        events.push(egui::Event::PointerButton {
                            pos: egui::pos2(500., 500.),
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: shortcut,
                        });
                    }
                }
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900., 900.),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        // The production caller checks input before drawing its
                        // tools, so use the focus established by earlier frames.
                        if frame == 2 {
                            let enabled = can_launch(&ctx, idle);
                            let launched = launch_shortcut(&ctx, enabled);
                            let second = launch_shortcut(&ctx, enabled);
                            let retained = ctx.input(|i| i.key_pressed(egui::Key::P));
                            observation = Some((launched, second, retained));
                        }
                        if text {
                            ui.text_edit_singleline(&mut value).request_focus();
                        }
                        if modal {
                            egui::Modal::new(egui::Id::new("launch-test-modal")).show(&ctx, |ui| {
                                ui.label("Modal owner");
                            });
                        }
                    },
                );
                output.textures_delta.clear();
            }
            assert_eq!(
                observation,
                Some((expected, false, !expected)),
                "idle={idle}, text={text}, modal={modal}, pointer={pointer}"
            );
        }
    }

    #[test]
    fn plain_p_and_unshifted_command_p_do_not_launch_palette() {
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::COMMAND] {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![egui::Event::Key {
                        key: egui::Key::P,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |_| {
                    assert!(!launch_shortcut(&ctx, true));
                    assert!(ctx.input(|i| i.key_pressed(egui::Key::P)));
                },
            );
            output.textures_delta.clear();
        }
    }

    #[test]
    fn every_command_has_a_spanish_name_and_spanish_searches_find_it() {
        use super::super::i18n::{Language, tr};
        for d in commands::REGISTRY {
            assert_ne!(tr(Language::Spanish, d.label), d.label, "{:?}", d.id);
        }
        assert_eq!(
            matches_in("pantalón", Language::Spanish).first(),
            Some(&CommandId::DraftTrousers)
        );
        assert!(matches_in("historial", Language::Spanish).contains(&CommandId::ToggleHistory));
        // English names keep working with the interface in Spanish.
        assert!(matches_in("history", Language::Spanish).contains(&CommandId::ToggleHistory));
    }
    #[test]
    fn search_keys_labels_and_aliases() {
        assert_eq!(matches("TEE"), vec![CommandId::LoadTshirt]);
        assert_eq!(matches("simulation.step"), vec![CommandId::StepSimulation]);
        assert_eq!(matches("garment 3d"), vec![CommandId::GarmentView]);
        assert_eq!(matches("ruler"), vec![CommandId::ToolMeasure]);
        assert_eq!(matches("frill"), vec![CommandId::ToolInternalLine]);
        assert!(matches("not-a-command").is_empty());
        assert_eq!(matches(" ").len(), commands::REGISTRY.len());
    }

    #[test]
    fn abbreviations_preserve_exact_priority_and_bound_input() {
        assert_eq!(matches("rctngl").first(), Some(&CommandId::ToolRectangle));
        assert_eq!(matches("FRHND").first(), Some(&CommandId::ToolFreehand));
        assert_eq!(matches("ruler").first(), Some(&CommandId::ToolMeasure));
        assert!(matches("qz").is_empty());
        assert!(matches(&"a".repeat(MAX_QUERY_CHARS + 1)).is_empty());
        assert!(is_subsequence("fhn", "freehand"));
        assert!(!is_subsequence("hfn", "freehand"));
    }

    #[test]
    fn changed_search_reveals_new_selection_after_keyboard_scroll() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(700., 480.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "tool", &mut c, &mut draw)
            .unwrap();
        for _ in 0..10 {
            h.key(
                "palette.query",
                egui::Key::ArrowDown,
                egui::Modifiers::NONE,
                &mut c,
                &mut draw,
            )
            .unwrap();
        }
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "tool.", &mut c, &mut draw)
            .unwrap();
        // The list scrolls back to the top with egui's scroll animation.
        frames(&mut h, &mut c);
        frames(&mut h, &mut c);
        h.click("palette.command.tool.edit", &mut c, &mut draw)
            .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToolEdit]);
    }

    #[test]
    fn named_shortcut_search_finds_and_dispatches_the_associated_command() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "Space", &mut c, &mut draw)
            .unwrap();
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToggleSimulation]);
    }

    #[test]
    fn command_names_rank_before_broad_aliases_without_reordering_empty_search() {
        assert_eq!(matches("cloth").first(), Some(&CommandId::ToolGrab));
        assert_eq!(matches("TOOL.GRAB").first(), Some(&CommandId::ToolGrab));
        assert_eq!(
            matches("   "),
            commands::REGISTRY.iter().map(|d| d.id).collect::<Vec<_>>()
        );
        assert!(matches("cloth").contains(&CommandId::GarmentView));
    }

    #[test]
    fn named_ambiguous_search_selects_the_relevant_tool() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "cloth", &mut c, &mut draw)
            .unwrap();
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToolGrab]);
    }

    #[test]
    fn named_abbreviated_search_dispatches_the_real_tool_and_bounds_paste() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "rctngl", &mut c, &mut draw)
            .unwrap();
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToolRectangle]);
        c.palette.begin();
        frames(&mut h, &mut c);
        h.replace_text(
            "palette.query",
            &"x".repeat(MAX_QUERY_CHARS * 4),
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert!(c.palette.query.chars().count() <= MAX_QUERY_CHARS);
        assert!(c.commands.len() == 1 && c.palette.open);
    }
    #[test]
    fn launcher_accepts_a_complete_pointer_gesture() {
        let mut count = 0;
        let mut h = Harness::new(egui::vec2(900., 900.));
        let mut draw = |ui: &mut egui::Ui, count: &mut usize| {
            if launch_button(ui, true, "Commands…", Default::default()) {
                *count += 1;
            }
        };
        h.frame(&mut count, &mut draw);
        h.click("palette.open", &mut count, &mut draw).unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn named_search_single_dispatch_and_disabled_context() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "step", &mut c, &mut draw)
            .unwrap();
        c.context.playing = true;
        frames(&mut h, &mut c);
        assert!(
            h.click("palette.command.simulation.step", &mut c, &mut draw)
                .is_err()
        );
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert!(c.commands.is_empty());
        c.context.playing = false;
        frames(&mut h, &mut c);
        h.click("palette.command.simulation.step", &mut c, &mut draw)
            .unwrap();
        frames(&mut h, &mut c);
        assert_eq!(c.commands, vec![CommandId::StepSimulation]);
        assert!(!c.palette.open);
    }
    #[test]
    fn backdrop_click_cancels_without_dispatch_or_click_through() {
        let mut c = case();
        let ctx = egui::Context::default();
        let mut underlying_clicks = 0;
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 900.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        if ui
                            .put(
                                egui::Rect::from_min_size(
                                    egui::pos2(10., 10.),
                                    egui::vec2(80., 30.),
                                ),
                                egui::Button::new("Behind"),
                            )
                            .clicked()
                        {
                            underlying_clicks += 1;
                        }
                        draw(ui, &mut c);
                    });
                },
            );
            output.textures_delta.clear(); // CPU-only input test has no renderer.
        };
        for _ in 0..3 {
            frame(vec![]);
        }
        let pos = egui::pos2(30., 25.);
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        frame(vec![]);
        assert!(!c.palette.open);
        assert!(c.commands.is_empty());
        assert_eq!(underlying_clicks, 0);
    }
    #[test]
    fn escape_cancels_without_request() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.key(
            "palette.query",
            egui::Key::Escape,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert!(!c.palette.open);
        assert!(c.commands.is_empty());
    }

    #[test]
    fn keyboard_navigation_and_context_changes_are_checked_at_activation() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "example", &mut c, &mut draw)
            .unwrap();
        h.key(
            "palette.query",
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.palette.selected, 1);
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::LoadDress]);
        c.palette.begin();
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "export", &mut c, &mut draw)
            .unwrap();
        c.context.filesystem_ready = false;
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands.len(), 1);
        assert!(c.palette.open);
    }

    #[test]
    fn keyboard_skips_disabled_actions_and_wraps_without_hiding_them() {
        let mut c = case();
        c.context.document_loaded = false;
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        assert_eq!(
            commands::REGISTRY[c.palette.selected].id,
            CommandId::SplitView
        );
        assert!(
            h.click("palette.command.simulation.toggle", &mut c, &mut draw)
                .is_err()
        );
        h.key(
            "palette.query",
            egui::Key::ArrowUp,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(
            commands::REGISTRY[c.palette.selected].id,
            CommandId::AddAvatarObject
        );
        h.key(
            "palette.query",
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(
            commands::REGISTRY[c.palette.selected].id,
            CommandId::SplitView
        );
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::SplitView]);
    }

    #[test]
    fn page_navigation_skips_disabled_commands_and_clamps() {
        let mut c = case();
        c.context.document_loaded = false;
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        let available: Vec<_> = commands::REGISTRY
            .iter()
            .enumerate()
            .filter_map(|(i, d)| commands::availability(d.id, c.context).is_ok().then_some(i))
            .collect();
        assert!(available.len() > 5);
        h.key(
            "palette.query",
            egui::Key::PageDown,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.palette.selected, available[5]);
        h.key(
            "palette.query",
            egui::Key::PageUp,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.palette.selected, available[0]);
        h.key(
            "palette.query",
            egui::Key::PageUp,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.palette.selected, available[0]);
        for _ in 0..available.len() {
            h.key(
                "palette.query",
                egui::Key::PageDown,
                egui::Modifiers::NONE,
                &mut c,
                &mut draw,
            )
            .unwrap();
        }
        assert_eq!(c.palette.selected, *available.last().unwrap());
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(
            c.commands,
            vec![commands::REGISTRY[*available.last().unwrap()].id]
        );
    }

    #[test]
    fn losing_availability_never_dispatches_a_different_action_on_same_enter() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        assert_eq!(
            commands::REGISTRY[c.palette.selected].id,
            CommandId::ToggleSimulation
        );
        c.context.document_loaded = false;
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert!(c.commands.is_empty());
        assert!(c.palette.open);
        assert_eq!(
            commands::REGISTRY[c.palette.selected].id,
            CommandId::SplitView
        );
        h.frame(&mut c, &mut draw);
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::SplitView]);
    }

    #[test]
    fn all_disabled_matches_leave_palette_open_without_dispatch() {
        let mut c = case();
        c.context.document_loaded = false;
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "simulation.step", &mut c, &mut draw)
            .unwrap();
        for key in [egui::Key::ArrowDown, egui::Key::ArrowUp, egui::Key::Enter] {
            h.key(
                "palette.query",
                key,
                egui::Modifiers::NONE,
                &mut c,
                &mut draw,
            )
            .unwrap();
        }
        assert!(c.commands.is_empty());
        assert!(c.palette.open);
    }

    #[test]
    fn every_tool_descriptor_maps_to_the_existing_transition() {
        let expected = [
            (CommandId::ToolEdit, Tool::Edit, false),
            (CommandId::ToolDraw, Tool::Draw, false),
            (CommandId::ToolFreehand, Tool::Draw, false),
            (CommandId::ToolRectangle, Tool::Rectangle, false),
            (CommandId::ToolEllipse, Tool::Ellipse, false),
            (CommandId::ToolSector, Tool::Sector, false),
            (CommandId::ToolInternalLine, Tool::InternalLine, false),
            (CommandId::ToolSew, Tool::Sew, false),
            (CommandId::ToolMove, Tool::Place, false),
            (CommandId::ToolRotate, Tool::Place, true),
            (CommandId::ToolGrab, Tool::Grab, false),
            (CommandId::ToolMeasure, Tool::Measure, false),
            (CommandId::ToolInspect, Tool::Inspect, false),
            (CommandId::ToolGraphic, Tool::Graphic, false),
        ];
        for (id, tool, rotate) in expected {
            assert!(tool_for_command(id).is_some_and(|actual| actual == (tool, rotate)));
            assert_eq!(id.descriptor().context, commands::CommandContext::Tool);
        }
        assert!(tool_for_command(CommandId::ToggleSimulation).is_none());
        assert_eq!(
            commands::REGISTRY
                .iter()
                .filter(|d| d.context == commands::CommandContext::Tool)
                .count(),
            expected.len()
        );
    }

    #[test]
    fn palette_search_selects_real_tools_and_disables_unselected_placement() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "ruler", &mut c, &mut draw)
            .unwrap();
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToolMeasure]);
        c.palette.begin();
        c.context.selected_panel = false;
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "move 3d", &mut c, &mut draw)
            .unwrap();
        assert!(
            h.click("palette.command.tool.move_3d", &mut c, &mut draw)
                .is_err()
        );
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToolMeasure]);
        assert!(c.palette.open);
    }

    #[test]
    fn palette_search_activates_freehand_by_keyboard() {
        let mut c = case();
        let mut h = Harness::new(egui::vec2(900., 900.));
        frames(&mut h, &mut c);
        h.replace_text("palette.query", "sketch", &mut c, &mut draw)
            .unwrap();
        h.key(
            "palette.query",
            egui::Key::Enter,
            egui::Modifiers::NONE,
            &mut c,
            &mut draw,
        )
        .unwrap();
        assert_eq!(c.commands, vec![CommandId::ToolFreehand]);
        assert!(!c.palette.open);
    }
}
