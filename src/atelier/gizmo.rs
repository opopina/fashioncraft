//! Placement handles along the world's, the piece's or the screen's axes. Preview is rigid;
//! only release recompiles.
use super::{
    authoring_ui::{Tool, segment_distance},
    *,
};
use fashioncraft::{authoring, garment::template::Placement};
use glam::{DQuat, DVec3, EulerRot};

pub(super) struct Drag {
    /// Which of the gizmo's three handles is held.
    index: usize,
    axis: DVec3,
    pivot: DVec3,
    center: DVec3,
    start: Placement,
    pointer: egui::Pos2,
    screen_axis: Vec2,
    radius: f64,
    rotate: bool,
    vector: DVec3,
    /// Screen tangent fallback for a ring viewed edge-on.
    screen_rotation: Option<Vec2>,
    group: Vec<String>,
    group_translation: DVec3,
    group_rotation: DQuat,
}
fn orientation(p: &Placement) -> DQuat {
    let r = p.rotation_degrees * (std::f64::consts::PI / 180.);
    DQuat::from_euler(EulerRot::XYZ, r.x, r.y, r.z)
}
fn world_motion(start: &Placement, next: &Placement, center: DVec3) -> (DVec3, DQuat) {
    let rotation = (orientation(next) * orientation(start).inverse()).normalize();
    let translation = next.translation - center - rotation * (start.translation - center);
    (translation, rotation)
}
const AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

/// Which axes the placement gizmo's arrows and rings follow (Settings › Gizmo).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GizmoAxes {
    /// The scene's X, Y and Z.
    #[default]
    World,
    /// The selected piece's own axes, turned with it.
    Local,
    /// The view's right, up and toward-the-viewer directions.
    Screen,
}

/// The gizmo's three unit axes for a piece at `placement` seen by `camera`.
pub(super) fn gizmo_axes(mode: GizmoAxes, placement: &Placement, camera: &Camera) -> [DVec3; 3] {
    match mode {
        GizmoAxes::World => AXES,
        GizmoAxes::Local => AXES.map(|a| (orientation(placement) * a).normalize()),
        GizmoAxes::Screen => {
            let forward = camera.forward().as_dvec3().normalize();
            let up = camera.up().as_dvec3().normalize();
            [forward.cross(up).normalize(), up, -forward]
        }
    }
}
const COLORS: [Color32; 3] = [
    Color32::from_rgb(248, 95, 94),
    Color32::from_rgb(104, 220, 134),
    Color32::from_rgb(98, 160, 255),
];
pub(super) fn project(camera: &Camera, rect: egui::Rect, p: DVec3) -> Option<egui::Pos2> {
    project_matrix(camera.matrix(rect.width() / rect.height()), rect, p)
}
pub(super) fn project_matrix(matrix: glam::Mat4, rect: egui::Rect, p: DVec3) -> Option<egui::Pos2> {
    let clip = matrix * p.as_vec3().extend(1.);
    if clip.w <= 0. {
        return None;
    }
    let p = clip.truncate() / clip.w;
    Some(egui::pos2(
        rect.center().x + p.x * rect.width() * 0.5,
        rect.center().y - p.y * rect.height() * 0.5,
    ))
}
fn ring(
    camera: &Camera,
    rect: egui::Rect,
    center: DVec3,
    axes: &[DVec3; 3],
    axis: usize,
    radius: f64,
) -> Vec<egui::Pos2> {
    let a = axes[(axis + 1) % 3];
    let b = axes[(axis + 2) % 3];
    (0..=64)
        .filter_map(|i| {
            let angle = i as f64 * std::f64::consts::TAU / 64.;
            project(
                camera,
                rect,
                center + radius * (angle.cos() * a + angle.sin() * b),
            )
        })
        .collect()
}
impl Atelier {
    fn gizmo_basis(&self) -> Option<(Placement, DVec3, DVec3, f64)> {
        let panel = self
            .project
            .template
            .panels
            .iter()
            .find(|p| Some(&p.id) == self.authoring.selected.as_ref())?;
        let placement = self
            .authoring
            .preview
            .clone()
            .unwrap_or_else(|| panel.placement.clone());
        let mut pivot = DVec3::new(panel.pattern.width * 0.5, -panel.pattern.height * 0.5, 0.);
        let mut center = placement.transform(pivot);
        let ids = self.selected_panels();
        if ids.len() > 1 {
            // Equal panel weighting: the mean of arranged material-box centres.
            center = self
                .project
                .template
                .panels
                .iter()
                .filter(|p| ids.contains(&p.id))
                .map(|p| {
                    p.placement.transform(DVec3::new(
                        p.pattern.width * 0.5,
                        -p.pattern.height * 0.5,
                        0.,
                    ))
                })
                .sum::<DVec3>()
                / ids.len() as f64;
            if let Some(drag) = &self.authoring.drag {
                center = drag.center + drag.group_translation;
            }
            pivot = orientation(&placement).inverse() * (center - placement.translation);
        }
        Some((
            placement,
            pivot,
            center,
            self.camera.distance as f64 * 0.055,
        ))
    }
    pub(super) fn place_interaction(&mut self, response: &egui::Response, rect: egui::Rect) {
        if self.authoring.tool != Tool::Place {
            self.authoring.selection.placement_marquee = None;
            return;
        }
        if response.ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_preview();
            super::selection::marquee_input(
                &mut self.authoring.selection.placement_marquee,
                response,
                false,
            );
            return;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && !response.ctx.input(|i| i.key_down(egui::Key::L))
            && let Some(pointer) = response.ctx.input(|i| i.pointer.press_origin())
            && let Some((start, pivot, center, radius)) = self.gizmo_basis()
            && let Some(screen_center) = project(&self.camera, rect, center)
        {
            let mut best = 11.;
            let mut hit = None;
            let axes = gizmo_axes(self.prefs.gizmo_axes, &start, &self.camera);
            for (i, axis) in axes.iter().enumerate() {
                let Some(tip) = project(&self.camera, rect, center + radius * axis) else {
                    continue;
                };
                let screen_axis = tip - screen_center;
                let rotate = self.authoring.rotate;
                let (origin, dir) = self.camera.ray(pointer, rect);
                let vector = picking::ray_plane(origin, dir, center, *axis)
                    .map(|p| p - center)
                    .unwrap_or_default();
                let distance = if rotate {
                    ring(&self.camera, rect, center, &axes, i, radius)
                        .windows(2)
                        .map(|p| segment_distance(pointer, p[0], p[1]))
                        .fold(f32::INFINITY, f32::min)
                } else {
                    if screen_axis.length() < 16. {
                        continue;
                    }
                    segment_distance(pointer, screen_center + screen_axis * 0.15, tip)
                };
                if distance < best {
                    best = distance;
                    hit = Some(Drag {
                        index: i,
                        axis: *axis,
                        pivot,
                        center,
                        start: start.clone(),
                        pointer,
                        screen_axis,
                        radius,
                        rotate,
                        vector,
                        group: self.selected_panels(),
                        group_translation: DVec3::ZERO,
                        group_rotation: DQuat::IDENTITY,
                        screen_rotation: (rotate
                            && (dir.dot(*axis).abs() < 0.08 || vector.length_squared() < 1e-8))
                            .then_some(Vec2::new(-screen_axis.y, screen_axis.x)),
                    });
                }
            }
            self.authoring.drag = hit;
        }
        let completed_marquee = super::selection::marquee_input(
            &mut self.authoring.selection.placement_marquee,
            response,
            self.authoring.drag.is_none(),
        );
        if response.dragged_by(egui::PointerButton::Primary)
            && let (Some(drag), Some(pointer)) =
                (&self.authoring.drag, response.interact_pointer_pos())
        {
            let placement = if drag.rotate {
                let angle = if let Some(tangent) = drag.screen_rotation {
                    Some(
                        (pointer - drag.pointer).dot(tangent) as f64
                            / tangent.length_sq().max(1.) as f64,
                    )
                } else {
                    let (origin, dir) = self.camera.ray(pointer, rect);
                    picking::ray_plane(origin, dir, drag.center, drag.axis).and_then(|p| {
                        let v = p - drag.center;
                        (v.length_squared() > 1e-10).then(|| {
                            drag.axis
                                .dot(drag.vector.cross(v))
                                .atan2(drag.vector.dot(v))
                        })
                    })
                };
                angle.map(|mut angle| {
                    if response.ctx.input(|i| i.modifiers.shift) {
                        let snap = 15_f64.to_radians();
                        angle = (angle / snap).round() * snap;
                    }
                    authoring::rotate_about(&drag.start, drag.pivot, drag.axis, angle)
                })
            } else {
                let mut p = drag.start.clone();
                let delta =
                    (pointer - drag.pointer).dot(drag.screen_axis) / drag.screen_axis.length_sq();
                p.translation += drag.axis * (delta as f64 * drag.radius);
                Some(p)
            };
            if let Some(p) = placement {
                if drag.group.len() > 1 {
                    let (translation, rotation) = world_motion(&drag.start, &p, drag.center);
                    let pivot = drag.center;
                    let ids = drag.group.clone();
                    let placements: Result<Vec<_>, _> = self
                        .project
                        .template
                        .panels
                        .iter()
                        .filter(|panel| ids.contains(&panel.id))
                        .map(|panel| {
                            authoring::transform_placement(
                                &panel.placement,
                                pivot,
                                translation,
                                rotation,
                            )
                            .map(|placement| (panel.id.clone(), placement))
                        })
                        .collect();
                    match placements {
                        Ok(placements) => {
                            self.preview_placement(p);
                            for (id, placement) in placements {
                                self.show_panel_at(&id, &placement);
                            }
                            if let Some(drag) = self.authoring.drag.as_mut() {
                                drag.group_translation = translation;
                                drag.group_rotation = rotation;
                            }
                        }
                        Err(error) => {
                            self.cancel_preview();
                            self.status = error;
                        }
                    }
                } else {
                    self.preview_placement(p);
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) && self.authoring.drag.is_some() {
            let group = self
                .authoring
                .drag
                .as_ref()
                .filter(|drag| drag.group.len() > 1)
                .map(|drag| authoring::Command::PlaceGroup {
                    panels: drag.group.clone(),
                    pivot: drag.center,
                    translation: drag.group_translation,
                    rotation: drag.group_rotation,
                });
            if let Some(command) = group {
                self.authoring.carry_drape = true;
                if self.authoring.preview.is_some() && !self.edit(command) {
                    self.cancel_preview();
                }
                self.authoring.carry_drape = false;
            } else {
                self.commit_placement();
            }
            self.authoring.drag = None;
        }
        if let Some(gesture) = completed_marquee {
            self.finish_placement_marquee(rect, gesture);
        }
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let (origin, dir) = self.camera.ray(pointer, rect);
            let mut best = f64::INFINITY;
            let mut selected = None;
            for panel in &self.scene.garment.panels {
                for t in &self.scene.garment.mesh.triangles[panel.triangles.clone()] {
                    if let Some((d, _)) = picking::ray_triangle(
                        origin,
                        dir,
                        t.vertices.map(|i| self.scene.state.x[i]),
                    ) && d < best
                    {
                        best = d;
                        selected = Some(panel.id.clone());
                    }
                }
            }
            if self.scene.collider.occludes_ray(origin, dir, best - 1e-5) {
                selected = None;
            }
            if let Some(id) = selected {
                self.cancel_preview();
                self.select_panel(id, response.ctx.input(|i| i.modifiers.shift));
            }
        }
    }
    pub(super) fn paint_gizmo(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if self.authoring.tool != Tool::Place {
            return;
        }
        let count = self.selected_panels().len();
        if let Some(gesture) = &self.authoring.selection.placement_marquee {
            let box_rect = egui::Rect::from_two_pos(gesture.start, gesture.end).intersect(rect);
            let color = if gesture.subtract {
                Color32::LIGHT_RED
            } else if gesture.through {
                Color32::GOLD
            } else {
                Color32::LIGHT_BLUE
            };
            let painter = ui.painter().with_clip_rect(rect);
            if gesture.lasso {
                if gesture.path.len() >= 2 {
                    let mut path = gesture.path.clone();
                    path.push(gesture.path[0]);
                    painter.add(egui::Shape::line(path, Stroke::new(1.5, color)));
                }
            } else {
                painter.rect_filled(box_rect, 0., color.gamma_multiply(0.12));
                painter.rect_stroke(
                    box_rect,
                    0.,
                    Stroke::new(1.5, color),
                    egui::StrokeKind::Inside,
                );
            }
            painter.text(
                box_rect.left_top(),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "{} · {} · {}{}",
                    if gesture.subtract {
                        "Subtract"
                    } else if gesture.add {
                        "Add"
                    } else {
                        "Replace"
                    },
                    if gesture.through {
                        "through"
                    } else {
                        "visible"
                    },
                    if gesture.lasso { "lasso" } else { "box" },
                    if gesture.overflow {
                        " · too complex"
                    } else {
                        ""
                    }
                ),
                egui::FontId::proportional(12.),
                color,
            );
        }
        ui.painter().with_clip_rect(rect).text(
            rect.left_bottom() + Vec2::new(12., -12.),
            egui::Align2::LEFT_BOTTOM,
            format!("{count} {}", super::i18n::tr(self.prefs.language, "selected · Shift-click: toggle · Shift-drag: add\nHold L + drag: lasso · Ctrl/Cmd: subtract · Alt: through\nMoved pieces keep their drape")),
            egui::FontId::proportional(12.), Color32::LIGHT_GRAY,
        );
        let Some((placement, _, center, radius)) = self.gizmo_basis() else {
            return;
        };
        let axes = gizmo_axes(self.prefs.gizmo_axes, &placement, &self.camera);
        let Some(screen_center) = project(&self.camera, rect, center) else {
            return;
        };
        if let Some(tip) = project(&self.camera, rect, center + radius * axes[0]) {
            self.smoke_target("gizmo_x", screen_center + (tip - screen_center) * 0.70);
        }
        if self.authoring.rotate {
            let target = center + radius * (axes[0] + axes[1]) * std::f64::consts::FRAC_1_SQRT_2;
            if let Some(p) = project(&self.camera, rect, target) {
                self.smoke_target("ring_z", p);
            }
            for (i, key) in [(0, "ring_x"), (1, "ring_y")] {
                let target = center
                    + radius * axes[(i + 1) % 3] * 0.65
                    + radius * axes[(i + 2) % 3] * (1. - 0.65_f64.powi(2)).sqrt();
                if let Some(p) = project(&self.camera, rect, target) {
                    self.smoke_target(key, p);
                }
            }
        }
        let painter = ui.painter().with_clip_rect(rect);
        if let Some(panel) = self
            .project
            .template
            .panels
            .iter()
            .find(|p| Some(&p.id) == self.authoring.selected.as_ref())
        {
            painter.text(
                screen_center + Vec2::new(0., -12.),
                egui::Align2::CENTER_BOTTOM,
                &panel.name,
                egui::FontId::proportional(13.),
                Color32::GOLD,
            );
        }
        for (i, axis) in axes.iter().enumerate() {
            let color = if self.authoring.drag.as_ref().is_some_and(|d| d.index == i) {
                Color32::WHITE
            } else {
                COLORS[i]
            };
            if self.authoring.rotate {
                let points = ring(&self.camera, rect, center, &axes, i, radius);
                painter.add(egui::Shape::line(points, Stroke::new(2.5, color)));
                if let Some(label) =
                    project(&self.camera, rect, center + radius * axes[(i + 1) % 3])
                {
                    painter.text(
                        label,
                        egui::Align2::LEFT_BOTTOM,
                        ["X", "Y", "Z"][i],
                        egui::FontId::proportional(13.),
                        color,
                    );
                }
            } else if let Some(tip) = project(&self.camera, rect, center + radius * axis) {
                painter.arrow(screen_center, tip - screen_center, Stroke::new(3., color));
                painter.text(
                    tip,
                    egui::Align2::CENTER_BOTTOM,
                    ["X", "Y", "Z"][i],
                    egui::FontId::proportional(14.),
                    color,
                );
            }
        }
        painter.circle_filled(screen_center, 4., Color32::GOLD);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gizmo_axes_follow_the_world_the_piece_or_the_screen() {
        let placement = Placement {
            translation: DVec3::new(0.1, 1.0, 0.2),
            rotation_degrees: DVec3::new(0., 90., 0.),
        };
        let camera = Camera::new(0.7, 0.3, 2.5, glam::Vec3::new(0., 1., 0.));
        assert_eq!(gizmo_axes(GizmoAxes::World, &placement, &camera), AXES);
        // The piece turned a quarter about Y: its X points along the world's -Z.
        let local = gizmo_axes(GizmoAxes::Local, &placement, &camera);
        assert!(local[0].distance(-DVec3::Z) < 1e-12);
        assert!(local[1].distance(DVec3::Y) < 1e-12);
        assert!(local[2].distance(DVec3::X) < 1e-12);
        let screen = gizmo_axes(GizmoAxes::Screen, &placement, &camera);
        for (i, axis) in screen.iter().enumerate() {
            assert!((axis.length() - 1.).abs() < 1e-6);
            assert!(axis.dot(screen[(i + 1) % 3]).abs() < 1e-6);
        }
        // Right-handed, with Z toward the viewer and X to the right on screen.
        assert!(screen[0].cross(screen[1]).distance(screen[2]) < 1e-6);
        assert!(screen[2].distance(-camera.forward().as_dvec3()) < 1e-6);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        let center = camera.target.as_dvec3();
        let (c, r) = (
            project(&camera, rect, center).unwrap(),
            project(&camera, rect, center + 0.1 * screen[0]).unwrap(),
        );
        assert!(r.x > c.x + 1. && (r.y - c.y).abs() < 0.5);
    }

    #[test]
    fn rotated_primary_handle_defines_one_world_motion_for_the_whole_group() {
        let primary = Placement {
            translation: DVec3::new(0.3, 1.2, -0.2),
            rotation_degrees: DVec3::new(20., 70., -15.),
        };
        let other = Placement {
            translation: DVec3::new(-0.2, 0.7, 0.4),
            rotation_degrees: DVec3::new(-60., 10., 120.),
        };
        let center = DVec3::new(0.1, 0.9, 0.2);
        let local_pivot = orientation(&primary).inverse() * (center - primary.translation);
        let next = authoring::rotate_about(&primary, local_pivot, DVec3::Z, 0.6);
        let (translation, rotation) = world_motion(&primary, &next, center);
        assert!(translation.length() < 1e-12);
        let transformed =
            authoring::transform_placement(&other, center, translation, rotation).unwrap();
        for p in [DVec3::ZERO, DVec3::new(0.2, -0.4, 0.)] {
            let expected = center + DQuat::from_rotation_z(0.6) * (other.transform(p) - center);
            assert!(transformed.transform(p).distance(expected) < 1e-12);
        }
        let mut moved = primary.clone();
        moved.translation += DVec3::new(0.4, 0., 0.);
        let (delta, rotation) = world_motion(&primary, &moved, center);
        assert!(delta.distance(DVec3::new(0.4, 0., 0.)) < 1e-12);
        assert!(rotation.abs_diff_eq(DQuat::IDENTITY, 1e-12));
    }
}
