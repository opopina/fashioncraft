//! The tool row under the header of each view, as in desktop cloth tools: the 3D view holds
//! simulation and 3D handling, the 2D view drafting, sewing and graphics. Related tools share
//! one button, which shows the tool in use or the last one chosen; the triangle in its corner
//! (or a secondary click) lists the rest. Every entry runs a registry command, and its tooltip
//! and its row name the active keyboard map's key.
use super::commands::CommandId;
use super::icons::Icon;
use super::selection::{BoxTarget, SelectionShape};
use super::*;

/// Size of a tool button, points.
const TOOL_PX: f32 = 25.;

/// Memory key of the open Shape by size window.
const SHAPE_BY_SIZE: &str = "shape-by-size-window";

/// One entry of a tool group's list.
#[derive(Clone, Copy)]
enum Entry {
    /// A tool or mode: when chosen it becomes the one the group's button shows.
    Run(CommandId, Icon, &'static str),
    /// A one-off action: listed, never shown on the button.
    Act(CommandId, Icon, &'static str),
    /// How the 2D selection tool picks: dragging a box or drawing a lasso.
    Shape(SelectionShape, &'static str),
    /// What the 2D selection tool picks: pieces or their points.
    Target(BoxTarget, &'static str),
    /// Pieces of an exact size, in a submenu.
    ShapeBySize,
    /// A new garment from its first piece.
    NewSheet,
    /// A small title over the entries after it.
    Heading(&'static str),
    Separator,
}

/// Tools that share one button.
struct Group {
    /// Names the group for the tool it remembers and for smoke targets (`flyout_<id>`).
    id: &'static str,
    entries: &'static [Entry],
}

enum Slot {
    Group(Group),
    /// Shapes, mirrors and booleans near the cursor.
    Quick,
    /// What the current tool does, on hover.
    Hint,
    Separator,
}

use CommandId as C;
use Entry::{Act, Heading, Run, Separator as Sep};

// Both rows follow the order desktop cloth tools use (Simulate, Select, Select mesh, Style
// line, Pin, Tear, arrangement, Sew, Topology, 3D pen in 3D; Transform, Edit pattern, Shapes,
// Internal, Sew, Steam, Taping, Graphic, Pleats, Topstitch, Puckering, Piping in 2D), then the
// tools FashionCraft adds.
const TOOLS_3D: &[Slot] = &[
    Slot::Group(Group {
        id: "simulate",
        entries: &[
            Run(C::ToggleSimulation, Icon::Simulate, "Simulate"),
            Act(C::StepSimulation, Icon::Step, "Advance one step"),
        ],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "select_3d",
        entries: &[
            Run(C::ToolEdit, Icon::Select, "Select and move"),
            Run(C::ToolMove, Icon::Move3d, "Move in 3D"),
            Run(C::ToolRotate, Icon::Rotate3d, "Rotate in 3D"),
        ],
    }),
    Slot::Group(Group {
        id: "select_mesh",
        entries: &[
            Run(C::SelectMeshBrush, Icon::Brush, "Select mesh (brush)"),
            Run(C::SelectMeshBox, Icon::SelectArea, "Select mesh (box)"),
            Run(C::SelectMeshLasso, Icon::Lasso, "Select mesh (lasso)"),
            Sep,
            Run(C::PinBox, Icon::PinBox, "Pin (box)"),
            Run(C::PinLasso, Icon::PinLasso, "Pin (lasso)"),
        ],
    }),
    Slot::Group(Group {
        id: "style_line",
        entries: &[
            Run(C::StyleLineEdit, Icon::StyleEdit, "Edit style line"),
            Run(C::StyleLineMove, Icon::StyleMove, "Move style line"),
            Run(C::StyleLineScale, Icon::StyleScale, "Scale style line"),
            Run(C::StyleLineDraw, Icon::StyleDraw, "Draw style line"),
        ],
    }),
    Slot::Group(Group {
        id: "pins",
        entries: &[
            Run(C::ToolGrab, Icon::Grab, "Grab cloth"),
            Sep,
            Act(C::AttachPins, Icon::PinAttach, "Attach all pins to avatar"),
            Act(
                C::DetachPins,
                Icon::PinDetach,
                "Detach all pins from avatar",
            ),
        ],
    }),
    Slot::Group(Group {
        id: "tear",
        entries: &[
            Act(C::TearSeam, Icon::Tear, "Tear seam"),
            Act(C::EditTears, Icon::TearEdit, "Edit tears"),
        ],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "reset_arrangement",
        entries: &[
            Act(
                C::Reset2dArrangement,
                Icon::ResetSheet,
                "Reset 2D arrangement (all)",
            ),
            Act(
                C::ResetSelectedPieces3d,
                Icon::ResetSelected,
                "Reset 3D arrangement (selected)",
            ),
            Act(
                C::ResetSimulation,
                Icon::Reset,
                "Reset 3D arrangement (all)",
            ),
        ],
    }),
    Slot::Group(Group {
        id: "fold",
        entries: &[Act(C::FoldArrangement, Icon::Fold, "Fold arrangement")],
    }),
    Slot::Group(Group {
        id: "redrape",
        entries: &[
            Act(C::ReDrape, Icon::Drape, "Re-drape 3D arrangement"),
            Act(C::AutoFit, Icon::FitBody, "Auto fit"),
        ],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "sewing_3d",
        entries: &[
            Run(C::ToolSew, Icon::Sew, "Segment sewing"),
            Run(C::FreeSewing, Icon::SewFree, "Free sewing"),
            Run(C::ChainSegmentSewing, Icon::SewChain, "M:N segment sewing"),
            Run(C::ChainFreeSewing, Icon::SewChainFree, "M:N free sewing"),
        ],
    }),
    Slot::Group(Group {
        id: "topology",
        entries: &[
            Act(C::ConvertToQuads, Icon::Quads, "Convert to quads"),
            Act(
                C::ConvertToTriangles,
                Icon::Triangles,
                "Convert to triangles",
            ),
            Sep,
            Run(
                C::HighResolution,
                Icon::MeshFiner,
                "High-resolution garment",
            ),
            Run(
                C::LowResolution,
                Icon::MeshCoarser,
                "Low-resolution garment",
            ),
        ],
    }),
    Slot::Group(Group {
        id: "pen_3d",
        entries: &[Run(C::TracePen3d, Icon::Pen, "3D pen")],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "superimpose",
        entries: &[
            Act(C::SuperimposeOver, Icon::LayerOver, "Superimpose over"),
            Act(C::SuperimposeUnder, Icon::LayerUnder, "Superimpose under"),
            Act(C::SuperimposeSide, Icon::LayerSide, "Superimpose side"),
        ],
    }),
    Slot::Group(Group {
        id: "piece_state",
        entries: &[
            Act(C::FreezePieces, Icon::Snowflake, "Freeze"),
            Act(C::UnfreezePieces, Icon::Drop, "Unfreeze"),
            Sep,
            Act(C::StrengthenPieces, Icon::Shield, "Strengthen"),
            Act(C::SolidifyPieces, Icon::Block, "Solidify"),
            Sep,
            Act(C::DeactivatePieces, Icon::PieceOff, "Deactivate"),
            Act(
                C::DeactivatePiecesWithSeams,
                Icon::PieceSeamOff,
                "Deactivate with seams",
            ),
            Act(C::ActivatePieces, Icon::PieceOn, "Activate"),
        ],
    }),
    Slot::Group(Group {
        id: "measure_3d",
        entries: &[
            Heading("Avatar"),
            Run(C::MeasureBodyTape, Icon::Tape, "Tape measure"),
            Run(C::MeasureBodyHeight, Icon::Height, "Height measure"),
            Run(C::MeasureBodyGirth, Icon::Girth, "Circumference measure"),
            Heading("Garment"),
            Run(C::MeasureGarmentLine, Icon::Dimension, "Linear measure"),
            Run(C::MeasureGarmentGirth, Icon::Loop, "Circumference measure"),
        ],
    }),
    Slot::Group(Group {
        id: "avatar",
        entries: &[
            Run(C::OpenPose, Icon::Pose, "Pose…"),
            Run(C::OpenAvatarEditor, Icon::AvatarEdit, "Avatar editor…"),
            Run(C::OpenAvatarLibrary, Icon::AvatarLibrary, "Avatar library…"),
        ],
    }),
    Slot::Group(Group {
        id: "inspect",
        entries: &[Run(C::ToolInspect, Icon::Warning, "Mark simulation issue")],
    }),
];

const TOOLS_2D: &[Slot] = &[
    Slot::Group(Group {
        id: "select_2d",
        entries: &[
            Run(C::ToolEdit, Icon::Select, "Transform pattern"),
            Heading("Select with"),
            Entry::Shape(SelectionShape::Box, "Box"),
            Entry::Shape(SelectionShape::Lasso, "Lasso"),
            Heading("Select"),
            Entry::Target(BoxTarget::Panels, "Panels"),
            Entry::Target(BoxTarget::Points, "Points"),
            Sep,
            Act(C::SelectAllPieces, Icon::SelectAll, "Select all"),
            Act(C::CopyPieces, Icon::Duplicate, "Copy"),
            Act(C::PastePieces, Icon::Paste, "Paste"),
        ],
    }),
    Slot::Group(Group {
        id: "edit_lines",
        entries: &[
            Act(C::SplitSegment, Icon::AddPoint, "Add point / split line"),
            Act(C::SmoothCurve, Icon::Smooth, "Smooth curve"),
            Act(C::ExtendLineEnds, Icon::Extend, "Extend line to outline"),
            Act(C::CutAndExtend, Icon::Trim, "Cut and extend"),
            Act(
                C::ReplaceOutline,
                Icon::Replace,
                "Replace outline with shape",
            ),
        ],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "shapes",
        entries: &[
            Run(C::ToolDraw, Icon::Polygon, "Polygon"),
            Run(C::ToolRectangle, Icon::Rectangle, "Rectangle"),
            Run(C::ToolEllipse, Icon::Ellipse, "Ellipse"),
            Run(C::ToolSector, Icon::Arc, "Arc sector"),
            Run(C::ToolFreehand, Icon::Curve, "Freehand"),
            Act(C::AddSpiral, Icon::Spiral, "Spiral"),
            Sep,
            Entry::ShapeBySize,
            Entry::NewSheet,
        ],
    }),
    Slot::Group(Group {
        id: "internal",
        entries: &[
            Run(
                C::ToolInternalLine,
                Icon::InternalLine,
                "Internal polygon or line",
            ),
            Act(
                C::AddInternalRectangle,
                Icon::InternalRect,
                "Internal rectangle",
            ),
            Act(
                C::AddInternalEllipse,
                Icon::InternalCircle,
                "Internal ellipse",
            ),
            Act(C::AddDiamondDart, Icon::InternalDart, "Diamond dart"),
            Sep,
            Act(C::TraceShape, Icon::Trace, "Trace"),
        ],
    }),
    Slot::Group(Group {
        id: "cut",
        entries: &[
            Act(C::CutAndSew, Icon::Scissors, "Cut and sew along line"),
            Act(C::SlashSpread, Icon::Spread, "Slash and spread along line"),
            Act(C::UnfoldPiece, Icon::Mirror, "Unfold across segment"),
            Act(C::MergeAlongSeam, Icon::Union, "Merge pieces along seam"),
            Sep,
            Act(C::DuplicatePiece, Icon::PiecePlus, "Duplicate piece"),
        ],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "sewing",
        entries: &[
            Run(C::ToolSew, Icon::Sew, "Segment sewing"),
            Run(C::FreeSewing, Icon::SewFree, "Free sewing"),
            Run(C::ChainSegmentSewing, Icon::SewChain, "M:N segment sewing"),
            Run(C::ChainFreeSewing, Icon::SewChainFree, "M:N free sewing"),
            Sep,
            Act(C::AutoSewing, Icon::AutoSew, "Auto sewing"),
            Act(
                C::CheckSewingLength,
                Icon::LengthCheck,
                "Check sewing length",
            ),
        ],
    }),
    Slot::Group(Group {
        id: "steam",
        entries: &[Run(C::Steam, Icon::Iron, "Steam")],
    }),
    Slot::Group(Group {
        id: "taping",
        entries: &[
            Act(C::SeamTaping, Icon::TapeBand, "Seam taping"),
            Act(C::AddTape, Icon::TapeRoll, "Tape"),
        ],
    }),
    Slot::Group(Group {
        id: "graphics",
        entries: &[
            Run(C::ToolGraphic, Icon::GraphicMove, "Transform graphic"),
            Act(
                C::AddGraphic,
                Icon::Graphic,
                "Graphic on the selected piece…",
            ),
            Run(C::EditTexture2d, Icon::Texture, "Edit texture (2D)"),
        ],
    }),
    Slot::Group(Group {
        id: "pleats",
        entries: &[
            Run(C::Pleats, Icon::Pleat, "Pleats"),
            Run(C::SinglePleat, Icon::PleatSingle, "Single pleat"),
            Act(C::PleatSewing, Icon::PleatSew, "Pleat sewing"),
        ],
    }),
    Slot::Group(Group {
        id: "topstitch",
        entries: &[
            Run(C::AddTopstitch, Icon::Stitch, "Segment topstitch"),
            Act(C::SeamlineTopstitch, Icon::StitchSeam, "Seamline topstitch"),
            Act(C::FreeTopstitch, Icon::StitchFree, "Free topstitch"),
        ],
    }),
    Slot::Group(Group {
        id: "puckering",
        entries: &[Act(C::Puckering, Icon::Wave, "Puckering")],
    }),
    Slot::Group(Group {
        id: "piping",
        entries: &[Act(C::Piping, Icon::Piping, "Piping")],
    }),
    Slot::Separator,
    Slot::Group(Group {
        id: "trims",
        entries: &[
            Run(C::AddButton, Icon::Button, "Button"),
            Run(C::AddButtonhole, Icon::Buttonhole, "Buttonhole"),
            Act(C::FastenButtons, Icon::Fasten, "Fasten buttons"),
            Sep,
            Run(C::AddZipper, Icon::Zipper, "Zipper"),
        ],
    }),
    Slot::Group(Group {
        id: "measure_2d",
        entries: &[Run(C::ToolMeasure, Icon::Measure, "Measure on the board")],
    }),
    Slot::Separator,
    Slot::Quick,
    Slot::Hint,
];

/// The smoke targets that live on rows of a group's list.
fn list_targets(group: &Group) -> Vec<&'static str> {
    group
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Shape(SelectionShape::Box, _) => Some("selection_box"),
            Entry::Shape(SelectionShape::Lasso, _) => Some("selection_lasso"),
            Entry::Target(BoxTarget::Panels, _) => Some("selection_panels"),
            Entry::Target(BoxTarget::Points, _) => Some("selection_points"),
            Entry::NewSheet => Some("replace"),
            _ => None,
        })
        .collect()
}

/// How wide a slot of the tool row is, points.
fn slot_width(slot: &Slot) -> f32 {
    match slot {
        Slot::Separator => 14.,
        _ => TOOL_PX + 1.,
    }
}

/// How many of `slots` fit in `width`, leaving room for the » button when not all do, and not
/// ending on a separator.
fn fitting_slots(slots: &[Slot], width: f32) -> usize {
    let total: f32 = slots.iter().map(slot_width).sum();
    if total <= width {
        return slots.len();
    }
    let room = width - (TOOL_PX + 4.);
    let mut used = 0.;
    let mut count = 0;
    while count < slots.len() && used + slot_width(&slots[count]) <= room {
        used += slot_width(&slots[count]);
        count += 1;
    }
    while count > 0 && matches!(slots[count - 1], Slot::Separator) {
        count -= 1;
    }
    count
}

impl Atelier {
    fn tool_row_active(&self, id: CommandId) -> bool {
        if id == CommandId::ToggleSimulation {
            return self.playing;
        }
        // Freehand is the pen tool drawing strokes: tell the two apart.
        let freehand = self.authoring.board.freehand.enabled;
        match id {
            CommandId::ToolDraw if freehand => return false,
            CommandId::ToolFreehand if !freehand => return false,
            _ => {}
        }
        command_palette::tool_for_command(id).is_some_and(|(tool, rotate)| {
            self.authoring.tool == tool && (tool != Tool::Place || self.authoring.rotate == rotate)
        })
    }

    /// Whether `id` is in use: its tool is held, or its mode or view is on.
    fn tool_in_use(&self, id: CommandId) -> bool {
        self.tool_row_active(id) || self.command_checked(id)
    }

    /// `id`'s keys in the active keyboard map, as this platform writes them.
    pub(super) fn tool_keys(&self, id: CommandId) -> String {
        keymap::label(self.prefs.keymap, id)
            .or_else(|| id.descriptor().shortcut.map(String::from))
            .map(|k| keymap::shown(&k))
            .unwrap_or_default()
    }

    /// 2D Pattern › Shapes › Shape by size, as a small window.
    fn shape_by_size_window(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(SHAPE_BY_SIZE);
        let mut open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return;
        }
        egui::Window::new(i18n::tr(self.prefs.language, "Shape by size"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.primitive_menu_body(ui));
        ctx.data_mut(|d| d.insert_temp(id, open));
    }

    /// The tool row of the 3D view (`garment`) or of the 2D view. What does not fit in the
    /// view's width goes under a » button at the end of the row, as desktop tools fold their
    /// toolbars, instead of wrapping onto a second line.
    pub(super) fn window_tools(&mut self, ui: &mut egui::Ui, garment: bool) {
        let ctx = ui.ctx().clone();
        if !garment {
            self.shape_by_size_window(&ctx);
        }
        let slots = if garment { TOOLS_3D } else { TOOLS_2D };
        // The tools sit on a band of their own, a shade lighter than the header above, with a
        // dark rule between them and the view.
        let band = ui.painter().add(egui::Shape::Noop);
        let pane = ui.max_rect().x_range();
        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.;
            let cut = fitting_slots(slots, ui.available_width());
            for slot in &slots[..cut] {
                self.tool_slot(ui, &ctx, slot);
            }
            if cut < slots.len() {
                self.tool_overflow(ui, &ctx, &slots[cut..], garment);
            }
        });
        let rect = egui::Rect::from_x_y_ranges(pane, row.response.rect.y_range().expand(4.));
        ui.painter().set(
            band,
            egui::Shape::rect_filled(rect, 0., ui.visuals().window_fill),
        );
        ui.painter().hline(
            pane,
            rect.bottom(),
            Stroke::new(1., Color32::from_black_alpha(90)),
        );
    }

    fn tool_slot(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, slot: &Slot) {
        match slot {
            Slot::Group(group) => self.tool_group(ui, ctx, group),
            Slot::Separator => {
                ui.add_space(3.);
                ui.separator();
                ui.add_space(3.);
            }
            Slot::Quick => {
                let tip = format!(
                    "{} · Q",
                    i18n::tr(
                        self.prefs.language,
                        "Shapes, mirrors and booleans near the cursor"
                    )
                );
                let quick = icons::icon_only(ui, Icon::Bolt, TOOL_PX, false, &tip);
                self.smoke_target("quick_open", quick.rect.center());
                if quick.clicked() {
                    self.quick.open(ui.ctx(), quick.rect.left_bottom());
                }
            }
            Slot::Hint => {
                let hint = i18n::tr(self.prefs.language, self.board_hint());
                icons::icon_only(ui, Icon::Info, TOOL_PX, false, hint);
            }
        }
    }

    /// The » button holding the tools that did not fit, each group's list one after another.
    fn tool_overflow(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        rest: &[Slot],
        garment: bool,
    ) {
        let language = self.prefs.language;
        let more = ui
            .add(
                egui::Button::new(egui::RichText::new("»").size(15.))
                    .frame(false)
                    .min_size(Vec2::splat(TOOL_PX)),
            )
            .on_hover_text(i18n::tr(language, "More tools"));
        self.smoke_target(
            if garment {
                "tools_overflow_3d"
            } else {
                "tools_overflow_2d"
            },
            more.rect.center(),
        );
        let width = rest
            .iter()
            .map(|slot| match slot {
                Slot::Group(group) => self.tool_list_width(ui, group),
                _ => 210.,
            })
            .fold(210., f32::max);
        let shown = egui::Popup::menu(&more).show(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 1.;
            let mut first = true;
            for slot in rest {
                match slot {
                    Slot::Group(group) => {
                        if !first {
                            ui.separator();
                        }
                        first = false;
                        let remembered_id = egui::Id::new(("tool-group", group.id));
                        self.tool_group_list(ui, ctx, group, remembered_id);
                    }
                    Slot::Quick => {
                        if !first {
                            ui.separator();
                        }
                        first = false;
                        let row = icons::menu_row(
                            ui,
                            Some(Icon::Bolt),
                            i18n::tr(language, "Shapes, mirrors and booleans near the cursor"),
                            "Q",
                            false,
                        );
                        self.smoke_target("quick_open", row.rect.center());
                        if row.clicked() {
                            self.quick.open(ctx, more.rect.left_bottom());
                            ui.close();
                        }
                    }
                    Slot::Hint => {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(i18n::tr(language, self.board_hint()))
                                .small()
                                .weak(),
                        );
                    }
                    Slot::Separator => {}
                }
            }
        });
        // Rows of a closed list are not on screen: drop their smoke targets so a test opens
        // the list again instead of clicking where a row used to be.
        if shown.is_none() {
            let mut inside: Vec<&str> = rest
                .iter()
                .flat_map(|slot| match slot {
                    Slot::Group(group) => list_targets(group),
                    _ => Vec::new(),
                })
                .collect();
            if rest.iter().any(|slot| matches!(slot, Slot::Quick)) {
                inside.push("quick_open");
            }
            self.forget_smoke_targets(&inside);
        }
    }

    /// One group's button and, when asked for, its list.
    fn tool_group(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, group: &Group) {
        let language = self.prefs.language;
        let remembered_id = egui::Id::new(("tool-group", group.id));
        // The tool in use wins (the most specific one: a sewing mode over the sewing tool),
        // then the one chosen last, then the first.
        let mut shown_entries: Vec<(usize, CommandId, Icon, &'static str)> = group
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| match *entry {
                Run(id, icon, label) => Some((i, id, icon, label)),
                _ => None,
            })
            .collect();
        // A group of one-off actions shows its first one.
        if shown_entries.is_empty()
            && let Some((i, Act(id, icon, label))) = group
                .entries
                .iter()
                .enumerate()
                .find(|(_, e)| matches!(e, Act(..)))
        {
            shown_entries.push((i, *id, *icon, *label));
        }
        let Some(&first) = shown_entries.first() else {
            return;
        };
        let in_use = shown_entries
            .iter()
            .rev()
            .find(|entry| self.tool_in_use(entry.1))
            .copied();
        let remembered: Option<usize> = ctx.data(|d| d.get_temp(remembered_id));
        let (_, id, icon, label) = in_use
            .or_else(|| {
                remembered.and_then(|i| shown_entries.iter().find(|entry| entry.0 == i).copied())
            })
            .unwrap_or(first);
        let available = commands::availability(id, self.command_context());
        let keys = self.tool_keys(id);
        let mut tip = i18n::tr(language, label).to_owned();
        if !keys.is_empty() {
            tip.push_str(" · ");
            tip.push_str(&keys);
        }
        if let Err(reason) = &available {
            tip.push('\n');
            tip.push_str(reason.message());
        }
        let single = group.entries.len() == 1;
        let (response, open) = if single {
            (
                icons::icon_only(ui, icon, TOOL_PX, in_use.is_some(), &tip),
                false,
            )
        } else {
            tip.push('\n');
            tip.push_str(i18n::tr(
                language,
                "More tools: the corner triangle or a right-click",
            ));
            icons::flyout_button(ui, icon, TOOL_PX, in_use.is_some(), &tip)
        };
        if !single {
            self.smoke_target(
                &format!("flyout_{}", group.id),
                response.rect.right_bottom() - Vec2::splat(4.),
            );
        }
        if response.clicked() && !open {
            match available {
                Ok(()) => self.invoke_command(id, ctx),
                Err(reason) => self.status = reason.message().to_owned(),
            }
        }
        if single {
            return;
        }
        let shown = egui::Popup::menu(&response)
            .open_memory(open.then_some(egui::SetOpenCommand::Toggle))
            .show(|ui| {
                ui.set_width(self.tool_list_width(ui, group));
                ui.spacing_mut().item_spacing.y = 1.;
                self.tool_group_list(ui, ctx, group, remembered_id);
            });
        if shown.is_none() {
            self.forget_smoke_targets(&list_targets(group));
        }
    }

    /// The width of `group`'s list: its longest row, and no narrower than a comfortable minimum.
    fn tool_list_width(&self, ui: &egui::Ui, group: &Group) -> f32 {
        let language = self.prefs.language;
        group
            .entries
            .iter()
            .map(|entry| match *entry {
                Run(id, _, label) | Act(id, _, label) => {
                    icons::menu_row_width(ui, i18n::tr(language, label), &self.tool_keys(id))
                }
                Entry::Shape(_, label) | Entry::Target(_, label) => {
                    icons::menu_row_width(ui, i18n::tr(language, label), "")
                }
                Entry::ShapeBySize => {
                    icons::menu_row_width(ui, i18n::tr(language, "Shape by size…"), "")
                }
                Entry::NewSheet => icons::menu_row_width(ui, i18n::tr(language, "New sheet"), ""),
                Heading(_) | Sep => 0.,
            })
            .fold(210., f32::max)
    }

    fn tool_group_list(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        group: &Group,
        remembered_id: egui::Id,
    ) {
        let language = self.prefs.language;
        for (i, entry) in group.entries.iter().enumerate() {
            match *entry {
                Run(id, icon, label) | Act(id, icon, label) => {
                    let available = commands::availability(id, self.command_context());
                    let keys = self.tool_keys(id);
                    let row = ui
                        .add_enabled_ui(available.is_ok(), |ui| {
                            icons::menu_row(
                                ui,
                                Some(icon),
                                i18n::tr(language, label),
                                &keys,
                                self.tool_in_use(id),
                            )
                        })
                        .inner;
                    let row = match available {
                        Ok(()) => row,
                        Err(reason) => row.on_disabled_hover_text(reason.message()),
                    };
                    if row.clicked() {
                        if matches!(entry, Run(..)) {
                            ctx.data_mut(|d| d.insert_temp(remembered_id, i));
                        }
                        self.invoke_command(id, ctx);
                        ui.close();
                    }
                }
                Entry::Shape(shape, label) => {
                    let row = icons::menu_row(
                        ui,
                        Some(match shape {
                            SelectionShape::Box => Icon::SelectArea,
                            SelectionShape::Lasso => Icon::Lasso,
                        }),
                        i18n::tr(language, label),
                        "",
                        self.authoring.selection.shape == shape,
                    );
                    let target = match shape {
                        SelectionShape::Box => "selection_box",
                        SelectionShape::Lasso => "selection_lasso",
                    };
                    self.smoke_target(target, row.rect.center());
                    if row.clicked() {
                        self.authoring.selection.shape = shape;
                        self.use_selection_tool(ctx);
                        ui.close();
                    }
                }
                Entry::Target(target, label) => {
                    let row = icons::menu_row(
                        ui,
                        Some(match target {
                            BoxTarget::Panels => Icon::Piece,
                            BoxTarget::Points => Icon::Points,
                        }),
                        i18n::tr(language, label),
                        "",
                        self.authoring.selection.target == target,
                    );
                    let name = match target {
                        BoxTarget::Panels => "selection_panels",
                        BoxTarget::Points => "selection_points",
                    };
                    self.smoke_target(name, row.rect.center());
                    if row.clicked() {
                        self.authoring.selection.target = target;
                        self.use_selection_tool(ctx);
                        ui.close();
                    }
                }
                Entry::ShapeBySize => {
                    let row = icons::menu_row(
                        ui,
                        Some(Icon::ShapeSize),
                        i18n::tr(language, "Shape by size…"),
                        "",
                        false,
                    );
                    if row.clicked() {
                        ctx.data_mut(|d| d.insert_temp(egui::Id::new(SHAPE_BY_SIZE), true));
                        ui.close();
                    }
                }
                Entry::NewSheet => {
                    let row = icons::menu_row(
                        ui,
                        Some(Icon::PagePlus),
                        i18n::tr(language, "New sheet"),
                        "",
                        self.authoring.replace,
                    )
                    .on_hover_text(i18n::tr(
                        language,
                        "Draw the first panel of a new garment; replaces the current garment when the contour is closed",
                    ));
                    self.smoke_target("replace", row.rect.center());
                    if row.clicked() {
                        self.start_new_sheet();
                        ui.close();
                    }
                }
                Heading(text) => {
                    ui.add_space(4.);
                    ui.label(egui::RichText::new(i18n::tr(language, text)).small().weak());
                }
                Sep => {
                    ui.separator();
                }
            }
        }
        if group.id == "select_2d" {
            ui.separator();
            let count = if self.authoring.selection.target == BoxTarget::Points {
                format!(
                    "{} {}",
                    self.authoring.board.point_count(),
                    i18n::tr(language, "points")
                )
            } else {
                format!(
                    "{} {}",
                    self.selected_panels().len(),
                    i18n::tr(language, "panels")
                )
            };
            ui.label(egui::RichText::new(count).small().weak());
        }
    }

    /// Box, lasso, pieces and points belong to the 2D selection tool: choosing one picks it.
    fn use_selection_tool(&mut self, ctx: &egui::Context) {
        if self.authoring.tool != Tool::Edit {
            self.invoke_command(C::ToolEdit, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_that_do_not_fit_fold_under_the_more_button_without_a_trailing_separator() {
        let all: f32 = TOOLS_2D.iter().map(slot_width).sum();
        assert_eq!(fitting_slots(TOOLS_2D, all + 1.), TOOLS_2D.len());
        for width in [120., 200., 300., all - 1.] {
            let cut = fitting_slots(TOOLS_2D, width);
            assert!(cut < TOOLS_2D.len(), "{width}");
            let used: f32 = TOOLS_2D[..cut].iter().map(slot_width).sum();
            assert!(used + TOOL_PX + 4. <= width, "{width}: {used}");
            assert!(cut == 0 || !matches!(TOOLS_2D[cut - 1], Slot::Separator));
        }
    }
}
