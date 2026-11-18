//! A tool row under the header of each view, as in desktop cloth tools: the 3D view holds
//! simulation and 3D handling, the 2D view drafting, sewing and graphics. Each button runs a
//! registry command and its tooltip names the active keyboard map's key.
use super::commands::CommandId;
use super::icons::Icon;
use super::*;

/// Size of a tool button, points.
const TOOL_PX: f32 = 24.;

const TOOLS_3D: &[(CommandId, Icon, &str)] = &[
    (CommandId::ToggleSimulation, Icon::Simulate, "Simulate"),
    (CommandId::ToolEdit, Icon::Select, "Select and move"),
    (CommandId::ToolMove, Icon::Move3d, "Move in 3D"),
    (CommandId::ToolRotate, Icon::Rotate3d, "Rotate in 3D"),
    (
        CommandId::ToolGrab,
        Icon::Grab,
        "Grab cloth · pins and tacks",
    ),
    (
        CommandId::ToolInspect,
        Icon::Warning,
        "Mark simulation issue",
    ),
];

const TOOLS_2D: &[(CommandId, Icon, &str)] = &[
    (CommandId::ToolEdit, Icon::Select, "Transform pattern"),
    (CommandId::ToolDraw, Icon::Pen, "Polygon"),
    (CommandId::ToolRectangle, Icon::Rectangle, "Rectangle"),
    (CommandId::ToolEllipse, Icon::Ellipse, "Ellipse"),
    (CommandId::ToolSector, Icon::Arc, "Arc sector"),
    (
        CommandId::ToolInternalLine,
        Icon::Edit,
        "Internal polygon or line",
    ),
    (CommandId::ToolSew, Icon::Sew, "Segment sewing"),
    (
        CommandId::ToolMeasure,
        Icon::Measure,
        "Measure on the board",
    ),
    (CommandId::ToolGraphic, Icon::Graphic, "Transform graphic"),
];

impl Atelier {
    fn tool_row_active(&self, id: CommandId) -> bool {
        if id == CommandId::ToggleSimulation {
            return self.playing;
        }
        command_palette::tool_for_command(id).is_some_and(|(tool, rotate)| {
            self.authoring.tool == tool && (tool != Tool::Place || self.authoring.rotate == rotate)
        })
    }

    /// The tool row of the 3D view (`garment`) or of the 2D view.
    pub(super) fn window_tools(&mut self, ui: &mut egui::Ui, garment: bool) {
        let ctx = ui.ctx().clone();
        let tools = if garment { TOOLS_3D } else { TOOLS_2D };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            for &(id, icon, label) in tools {
                let available = commands::availability(id, self.command_context());
                let key = keymap::label(self.prefs.keymap, id)
                    .or_else(|| id.descriptor().shortcut.map(String::from))
                    .map_or_else(String::new, |k| format!(" · {k}"));
                let label = i18n::tr(self.prefs.language, label);
                let tip = match &available {
                    Ok(()) => format!("{label}{key}"),
                    Err(reason) => format!("{label}{key}\n{}", reason.message()),
                };
                let active = self.tool_row_active(id);
                let clicked = ui
                    .add_enabled_ui(available.is_ok(), |ui| {
                        icons::icon_only(ui, icon, TOOL_PX, active, &tip)
                    })
                    .inner
                    .clicked();
                if clicked {
                    self.invoke_command(id, &ctx);
                }
            }
        });
    }
}
