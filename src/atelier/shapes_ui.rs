//! Spiral patterns, closed internal shapes with replication, and holes: the Shapes menu entry
//! and the piece inspector section. Every action is one `Command`, so one undo step.
use super::{authoring_ui::Tool, *};
use glam::DVec2;
use marvelous_cloth::{
    authoring::Command,
    garment::{
        meshing::MeshingSettings,
        shapes2d::{InternalShapeKind, Spiral},
        template::Panel,
    },
};

/// Values typed into the spiral and internal-shape forms, in millimetres and degrees.
pub(super) struct ShapesUi {
    spiral: Spiral,
    kind: InternalShapeKind,
    size_mm: [f64; 2],
    /// Centre of the first shape in a piece's material coordinates; `None` (or another
    /// piece) uses the middle of that piece.
    center_mm: Option<(String, [f64; 2])>,
    count: u32,
    interval_mm: f64,
    angle_degrees: f64,
}

impl Default for ShapesUi {
    fn default() -> Self {
        Self {
            spiral: Spiral::default(),
            kind: InternalShapeKind::Rectangle,
            size_mm: [100., 100.],
            center_mm: None,
            count: 1,
            interval_mm: 150.,
            angle_degrees: 90.,
        }
    }
}

fn mm_value(
    ui: &mut egui::Ui,
    label: &str,
    metres: &mut f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    let mut mm = *metres * 1000.;
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(&mut mm)
                    .range(range)
                    .speed(1.0)
                    .fixed_decimals(1)
                    .suffix(" mm"),
            )
            .changed()
        })
        .inner;
    if changed {
        *metres = mm / 1000.;
    }
    changed
}

impl Atelier {
    /// The spiral block of the Shapes menu.
    pub(super) fn spiral_menu(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("Spiral");
        let s = &mut self.authoring.shapes.spiral;
        mm_value(ui, "Radius", &mut s.radius_m, 5.0..=5000.0);
        mm_value(ui, "Inner length", &mut s.inner_length_m, 10.0..=50_000.0);
        mm_value(ui, "Inner width", &mut s.inner_width_m, 2.0..=2000.0);
        mm_value(ui, "Outer width", &mut s.outer_width_m, 2.0..=2000.0);
        mm_value(ui, "Offset per turn", &mut s.offset_m, 5.0..=5000.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.clockwise, true, "Clockwise");
            ui.selectable_value(&mut s.clockwise, false, "Counterclockwise");
        });
        let spiral = *s;
        match spiral.outer_length() {
            Ok(outer) => {
                ui.small(format!(
                    "Outer edge {:.1} mm · {:.2} turns",
                    outer * 1000.,
                    spiral.end_angle().unwrap_or_default() / std::f64::consts::TAU
                ));
            }
            Err(error) => {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
        }
        if ui
            .button("Add spiral at board center")
            .on_hover_text("A spiral strip, e.g. for a ruffle or a cascade")
            .clicked()
        {
            let size = spiral
                .polygon(MeshingSettings::default())
                .ok()
                .and_then(|p| p.points.iter().copied().reduce(DVec2::max))
                .unwrap_or(DVec2::ZERO);
            let command = Command::Spiral {
                spiral,
                origin: self.authoring.board.center - size * 0.5,
                meshing: MeshingSettings {
                    target_edge_m: self.mesh_mm * 0.001,
                    anisotropy: self.anisotropy,
                    axis_degrees: self.mesh_axis,
                    ..Default::default()
                },
                fabric: self.project.template.fabrics[0].id.clone(),
            };
            if self.edit(command) {
                self.set_authoring_tool(Tool::Edit, false);
                ui.close();
            }
        }
    }

    /// "Internal shapes & holes" in the piece inspector.
    pub(super) fn internal_shape_properties(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        let Some(poly) = panel.polygon.clone() else {
            return;
        };
        egui::CollapsingHeader::new("Internal shapes & holes")
            .id_salt("internal-shapes")
            .show(ui, |ui| {
                let state = &mut self.authoring.shapes;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.kind, InternalShapeKind::Rectangle, "Rectangle");
                    ui.selectable_value(&mut state.kind, InternalShapeKind::Ellipse, "Ellipse");
                });
                for (label, value) in ["Width", "Height"].into_iter().zip(state.size_mm.iter_mut()) {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.add(
                            egui::DragValue::new(value)
                                .range(2.0..=10_000.0)
                                .fixed_decimals(1)
                                .suffix(" mm"),
                        );
                    });
                }
                let (lo, hi) = poly.points.iter().fold(
                    (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                    |(lo, hi), p| (lo.min(*p), hi.max(*p)),
                );
                let middle = ((lo + hi) * 500.).to_array();
                let mut centre = match &state.center_mm {
                    Some((id, at)) if *id == panel.id => *at,
                    _ => middle,
                };
                let mut moved = false;
                ui.horizontal(|ui| {
                    ui.label("Centre");
                    for value in centre.iter_mut() {
                        moved |= ui
                            .add(egui::DragValue::new(value).fixed_decimals(1).suffix(" mm"))
                            .on_hover_text("Position in the piece, from its top-left corner")
                            .changed();
                    }
                });
                if ui
                    .small_button("Centre of the piece")
                    .on_hover_text("Put the first shape in the middle of the piece")
                    .clicked()
                {
                    state.center_mm = None;
                    moved = false;
                }
                if moved {
                    state.center_mm = Some((panel.id.clone(), centre));
                }
                ui.label("Replicate");
                ui.horizontal(|ui| {
                    ui.label("Copies");
                    ui.add(egui::DragValue::new(&mut state.count).range(1..=50))
                        .on_hover_text("Shapes created in one go, the first one included");
                });
                ui.horizontal(|ui| {
                    ui.label("Interval");
                    ui.add(
                        egui::DragValue::new(&mut state.interval_mm)
                            .range(-10_000.0..=10_000.0)
                            .fixed_decimals(1)
                            .suffix(" mm"),
                    )
                    .on_hover_text("Distance between the centres of neighbouring copies");
                });
                ui.horizontal(|ui| {
                    ui.label("Angle");
                    ui.add(
                        egui::DragValue::new(&mut state.angle_degrees)
                            .range(-360.0..=360.0)
                            .fixed_decimals(1)
                            .suffix("°"),
                    )
                    .on_hover_text("Direction of the copies: 0° up, 90° right");
                });
                let command = Command::InternalShape {
                    panel: panel.id.clone(),
                    kind: state.kind,
                    center: DVec2::from_array(centre) / 1000.,
                    size: DVec2::from_array(state.size_mm) / 1000.,
                    count: state.count,
                    interval_m: state.interval_mm / 1000.,
                    angle_degrees: state.angle_degrees,
                };
                if ui.button("Add internal shape").clicked() && self.edit(command) {
                    self.status = "Internal shape added · select it to cut it out as a hole".into();
                }
                if !poly.holes.is_empty() {
                    ui.separator();
                    for (index, hole) in poly.holes.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.small(format!("Hole {} · {} points", index + 1, hole.points.len()));
                            if ui
                                .small_button("Fill")
                                .on_hover_text("Fill the hole back in, leaving an internal shape along its rim")
                                .clicked()
                                && self.edit(Command::ConvertToInternalShape {
                                    panel: panel.id.clone(),
                                    hole: index,
                                })
                            {
                                self.status = "Hole filled · an internal shape keeps its outline".into();
                            }
                        });
                    }
                }
            });
    }
}
