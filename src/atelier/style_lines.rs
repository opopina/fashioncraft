//! Style lines (3D Garment › Style lines): reshape the pattern from the 3D view. Edit drags a
//! point of an outline edge (near an end it moves the corner, in between it bends the edge),
//! Move drags a whole edge, Scale lengthens or shortens an edge about its middle, and Draw
//! clicks a new internal line onto a piece (Enter or a double click ends it). Each change is
//! made to the 2D pattern as one undoable edit; the drape keeps its shape.
//!
//! A drag on the cloth becomes a change of material coordinates through the cloth's local
//! stretch where the edge was picked: the pointer moves on the plane through that point facing
//! the view, and that move is taken back to the piece by the least-squares inverse of how the
//! piece's u and v directions lie on the cloth there.
use super::{authoring_ui::segment_distance, gizmo::project_matrix, *};
use fashioncraft::{
    authoring::Command,
    garment::{CompiledGarment, GarmentTemplate, seam::BoundaryRef, template::PanelRange},
};
use glam::{DMat2, DVec2, DVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StyleMode {
    Edit,
    Move,
    Scale,
    Draw,
}

/// An outline edge picked on the cloth.
#[derive(Clone, Debug)]
struct EdgePick {
    panel: String,
    edge: String,
    /// Where on the cloth it was picked, and the material point there.
    world: DVec3,
    material: DVec2,
    /// Share of the edge's length from the first end of its compiled boundary.
    t: f64,
    /// Material coordinates of that boundary's first and last points.
    ends: [DVec2; 2],
    /// How the piece's material u and v directions lie on the cloth there.
    frame: [DVec3; 2],
}

#[derive(Default)]
pub(super) struct StyleLines {
    pub mode: Option<StyleMode>,
    hover: Option<EdgePick>,
    /// The edge being dragged, and where the pointer is now on the drag plane.
    drag: Option<(EdgePick, DVec3)>,
    /// The line being drawn: its piece and its material and world points.
    draw: Option<(String, Vec<DVec2>, Vec<DVec3>)>,
}

/// How near an edge the pointer must be to pick it, screen points.
const PICK_PX: f32 = 9.;
/// Within this share of an edge's length from an end, Edit moves the corner.
const CORNER_SHARE: f64 = 0.2;
const COLOR: Color32 = Color32::from_rgb(255, 120, 200);

/// The material change whose image under `frame` comes nearest the world move `w`.
fn material_delta(frame: [DVec3; 2], w: DVec3) -> DVec2 {
    let [fu, fv] = frame;
    let gram = DMat2::from_cols(
        DVec2::new(fu.dot(fu), fv.dot(fu)),
        DVec2::new(fu.dot(fv), fv.dot(fv)),
    );
    if gram.determinant().abs() < 1e-18 {
        return DVec2::ZERO;
    }
    gram.inverse() * DVec2::new(fu.dot(w), fv.dot(w))
}

/// The scale an edge from `a` to `b` takes when a point picked at `picked` moves by `delta`
/// along it: the picked half grows or shrinks by the move.
fn edge_scale(a: DVec2, b: DVec2, picked: DVec2, delta: DVec2) -> Option<f64> {
    let length = a.distance(b);
    if length < 1e-6 {
        return None;
    }
    let along = (b - a) / length;
    let side = if (picked - (a + b) * 0.5).dot(along) >= 0. {
        1.
    } else {
        -1.
    };
    Some((1. + side * delta.dot(along) / (0.5 * length)).clamp(0.2, 5.))
}

/// The pick of `edge` of `panel`, on its compiled boundary's segment `k` at share `s` of it.
fn edge_pick(
    garment: &CompiledGarment,
    x: &[DVec3],
    range: &PanelRange,
    edge: &str,
    k: usize,
    s: f64,
) -> Option<EdgePick> {
    let ids = range.boundaries.get(edge)?;
    let (a, b) = (*ids.get(k)?, *ids.get(k + 1)?);
    let uv = &garment.mesh.uv;
    let lengths: Vec<f64> = ids
        .windows(2)
        .map(|w| uv[w[0]].distance(uv[w[1]]))
        .collect();
    let total: f64 = lengths.iter().sum();
    let before = lengths[..k].iter().sum::<f64>() + lengths[k] * s;
    let tri = garment.mesh.triangles[range.triangles.clone()]
        .iter()
        .find(|t| t.vertices.contains(&a) && t.vertices.contains(&b))?;
    let [i, j, l] = tri.vertices;
    let m = DMat2::from_cols(uv[j] - uv[i], uv[l] - uv[i]);
    if m.determinant().abs() < 1e-18 {
        return None;
    }
    let inv = m.inverse();
    let (e1, e2) = (x[j] - x[i], x[l] - x[i]);
    Some(EdgePick {
        panel: range.id.clone(),
        edge: edge.into(),
        world: x[a].lerp(x[b], s),
        material: uv[a].lerp(uv[b], s),
        t: if total > 0. { before / total } else { 0.5 },
        ends: [uv[ids[0]], uv[*ids.last()?]],
        frame: [
            e1 * inv.col(0).x + e2 * inv.col(0).y,
            e1 * inv.col(1).x + e2 * inv.col(1).y,
        ],
    })
}

/// The pick of `edge` of `panel` at share `t` of its length.
fn edge_pick_at(
    garment: &CompiledGarment,
    x: &[DVec3],
    panel: &str,
    edge: &str,
    t: f64,
) -> Option<EdgePick> {
    let range = garment.panels.iter().find(|r| r.id == panel)?;
    let ids = range.boundaries.get(edge)?;
    let uv = &garment.mesh.uv;
    let lengths: Vec<f64> = ids
        .windows(2)
        .map(|w| uv[w[0]].distance(uv[w[1]]))
        .collect();
    let target = t.clamp(0., 1.) * lengths.iter().sum::<f64>();
    let mut before = 0.;
    for (k, length) in lengths.iter().enumerate() {
        if before + length >= target || k + 1 == lengths.len() {
            let s = if *length > 0. {
                ((target - before) / length).clamp(0., 1.)
            } else {
                0.
            };
            return edge_pick(garment, x, range, edge, k, s);
        }
        before += length;
    }
    None
}

/// The edges sewn along the whole of the picked one by a one-to-one seam, picked at the
/// matching point: the same style line seen from the piece on its other side.
fn seam_partners(
    template: &GarmentTemplate,
    garment: &CompiledGarment,
    x: &[DVec3],
    pick: &EdgePick,
) -> Vec<EdgePick> {
    let whole = |r: &BoundaryRef| r.range[0] <= 1e-6 && r.range[1] >= 1. - 1e-6;
    let is_pick = |r: &BoundaryRef| r.panel == pick.panel && r.edge.as_deref() == Some(&pick.edge);
    let mut out = vec![];
    for seam in &template.seams {
        if !seam.a_tail.is_empty() || !seam.b_tail.is_empty() {
            continue;
        }
        let (this, other) = if is_pick(&seam.a) {
            (&seam.a, &seam.b)
        } else if is_pick(&seam.b) {
            (&seam.b, &seam.a)
        } else {
            continue;
        };
        let Some(edge) = &other.edge else {
            continue;
        };
        if !whole(this) || !whole(other) || other.panel == pick.panel {
            continue;
        }
        let flip = seam.reverse_b ^ this.reversed ^ other.reversed;
        let t = if flip { 1. - pick.t } else { pick.t };
        out.extend(edge_pick_at(garment, x, &other.panel, edge, t));
    }
    out
}

/// The pattern change a style-line drag makes when the cloth at `picks[0]` is pulled by `w`:
/// on the picked edge, and the same change on each partner edge across a seam, each by the
/// material amount the pull is in its own piece.
fn style_command(
    template: &GarmentTemplate,
    mode: StyleMode,
    picks: &[EdgePick],
    w: DVec3,
) -> Option<Command> {
    let primary = picks.first()?;
    let (mut moves, mut bends, mut scales) = (vec![], vec![], vec![]);
    // Scale: the factor comes from the picked edge and holds for its partners.
    let scale = if mode == StyleMode::Scale {
        let delta = material_delta(primary.frame, w);
        let poly = template
            .panels
            .iter()
            .find(|p| p.id == primary.panel)?
            .polygon
            .as_ref()?;
        let i = poly.edges.iter().position(|e| *e == primary.edge)?;
        let j = (i + 1) % poly.points.len();
        Some(edge_scale(
            poly.points[i],
            poly.points[j],
            primary.material,
            delta,
        )?)
    } else {
        None
    };
    for pick in picks {
        let Some(poly) = template
            .panels
            .iter()
            .find(|p| p.id == pick.panel)
            .and_then(|p| p.polygon.as_ref())
        else {
            continue;
        };
        let Some(i) = poly.edges.iter().position(|e| *e == pick.edge) else {
            continue;
        };
        let j = (i + 1) % poly.points.len();
        // Which outline point the compiled boundary starts from.
        let (first, last) =
            if pick.ends[0].distance(poly.points[i]) <= pick.ends[0].distance(poly.points[j]) {
                (i, j)
            } else {
                (j, i)
            };
        let delta = material_delta(pick.frame, w);
        let panel = pick.panel.clone();
        let t = pick.t;
        match mode {
            StyleMode::Move => moves.push((panel, vec![i, j], delta)),
            StyleMode::Edit if t < CORNER_SHARE => moves.push((panel, vec![first], delta)),
            StyleMode::Edit if t > 1. - CORNER_SHARE => moves.push((panel, vec![last], delta)),
            // A cubic's handles moved by d move its point at t by 3t(1-t)d.
            StyleMode::Edit => {
                bends.push((panel, pick.edge.clone(), delta / (3. * t * (1. - t))));
            }
            StyleMode::Scale => scales.push((panel, pick.edge.clone(), scale?)),
            StyleMode::Draw => return None,
        }
    }
    (!moves.is_empty() || !bends.is_empty() || !scales.is_empty()).then_some(Command::StyleLine {
        moves,
        bends,
        scales,
    })
}

impl Atelier {
    pub(super) fn start_style_lines(&mut self, mode: StyleMode) {
        let s = &mut self.authoring.style_lines;
        s.mode = Some(mode);
        s.drag = None;
        s.draw = None;
        s.hover = None;
        let hint = match mode {
            StyleMode::Edit => {
                "Drag an edge of the garment: near an end it moves the corner, in between it bends · Esc ends"
            }
            StyleMode::Move => "Drag an edge of the garment to move it · Esc ends",
            StyleMode::Scale => {
                "Drag an edge of the garment along itself to lengthen or shorten it · Esc ends"
            }
            StyleMode::Draw => {
                "Click points on a piece; Enter or a double click makes them an internal line · Esc ends"
            }
        };
        self.status = i18n::tr(self.prefs.language, hint).into();
    }

    /// The outline edge under `pointer`, unless the body hides it.
    fn pick_style_edge(&self, pointer: egui::Pos2, rect: egui::Rect) -> Option<EdgePick> {
        let matrix = self.camera.matrix(rect.width() / rect.height());
        let project = |p: DVec3| project_matrix(matrix, rect, p);
        let g = &self.scene.garment;
        let x = &self.scene.state.x;
        let mut best: Option<(f32, usize, &str, usize, f64)> = None;
        for (pi, range) in g.panels.iter().enumerate() {
            if !self.panel_shown_3d(&range.id) {
                continue;
            }
            let Some(poly) = self
                .project
                .template
                .panels
                .iter()
                .find(|p| p.id == range.id)
                .and_then(|p| p.polygon.as_ref())
            else {
                continue;
            };
            for (name, ids) in &range.boundaries {
                if !poly.edges.contains(name) {
                    continue;
                }
                for (k, seg) in ids.windows(2).enumerate() {
                    let (Some(pa), Some(pb)) = (project(x[seg[0]]), project(x[seg[1]])) else {
                        continue;
                    };
                    let d = segment_distance(pointer, pa, pb);
                    if d <= PICK_PX && best.is_none_or(|b| d < b.0) {
                        let v = pb - pa;
                        let s = ((pointer - pa).dot(v) / v.length_sq().max(1e-12)).clamp(0., 1.);
                        best = Some((d, pi, name, k, f64::from(s)));
                    }
                }
            }
        }
        let (_, pi, name, k, s) = best?;
        let pick = edge_pick(g, x, &g.panels[pi], name, k, s)?;
        let screen = project(pick.world)?;
        let (origin, dir) = self.camera.ray(screen, rect);
        if self.avatar_shown()
            && self
                .scene
                .collider
                .occludes_ray(origin, dir, pick.world.distance(origin) - 1e-4)
        {
            return None;
        }
        Some(pick)
    }

    /// Where the pointer at `pos` meets the plane through `through` that faces the view.
    fn on_view_plane(&self, pos: egui::Pos2, rect: egui::Rect, through: DVec3) -> Option<DVec3> {
        let (origin, dir) = self.camera.ray(pos, rect);
        let normal = self.camera.ray(rect.center(), rect).1;
        let denom = dir.dot(normal);
        if denom.abs() < 1e-9 {
            return None;
        }
        let s = (through - origin).dot(normal) / denom;
        (s > 0.).then(|| origin + dir * s)
    }

    /// Clicks and drags in the 3D view while a style-line tool is on.
    pub(super) fn style_line_interaction(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
    ) -> bool {
        let Some(mode) = self.authoring.style_lines.mode else {
            return false;
        };
        if response.ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let s = &mut self.authoring.style_lines;
            if s.draw.is_some() {
                s.draw = None;
            } else {
                s.mode = None;
                s.hover = None;
            }
            s.drag = None;
            return true;
        }
        if mode == StyleMode::Draw {
            self.style_draw(response, rect);
            return true;
        }
        if self.authoring.style_lines.drag.is_none() {
            self.authoring.style_lines.hover = response
                .hover_pos()
                .and_then(|pos| self.pick_style_edge(pos, rect));
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
            && let Some(pick) = self.pick_style_edge(pos, rect)
        {
            let at = pick.world;
            self.authoring.style_lines.drag = Some((pick, at));
        }
        let through = self
            .authoring
            .style_lines
            .drag
            .as_ref()
            .map(|(pick, _)| pick.world);
        if response.dragged_by(egui::PointerButton::Primary)
            && let (Some(pos), Some(through)) = (response.interact_pointer_pos(), through)
            && let Some(at) = self.on_view_plane(pos, rect, through)
            && let Some((_, now)) = &mut self.authoring.style_lines.drag
        {
            *now = at;
        }
        if response.drag_stopped_by(egui::PointerButton::Primary)
            && let Some((pick, at)) = self.authoring.style_lines.drag.take()
        {
            let w = at - pick.world;
            let mut picks = vec![pick];
            picks.extend(seam_partners(
                &self.project.template,
                &self.scene.garment,
                &self.scene.state.x,
                &picks[0],
            ));
            if w.length() > 1e-5
                && let Some(command) = style_command(&self.project.template, mode, &picks, w)
                && self.edit(command)
            {
                self.status =
                    i18n::tr(self.prefs.language, "Pattern reshaped from the 3D view").into();
            }
        }
        true
    }

    /// Draw: clicks on one piece add points; Enter or a double click makes the internal line.
    fn style_draw(&mut self, response: &egui::Response, rect: egui::Rect) {
        let ui_lang = self.prefs.language;
        if response.clicked_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            match self.picked_cloth_point(pos, rect) {
                Some((panel, uv, at)) => match &mut self.authoring.style_lines.draw {
                    Some((piece, points, worlds)) if *piece == panel => {
                        if points.last().is_none_or(|q| q.distance(uv) > 1e-4) {
                            points.push(uv);
                            worlds.push(at);
                        }
                    }
                    Some(_) => self.status = i18n::tr(ui_lang, "Keep the line on one piece").into(),
                    None => self.authoring.style_lines.draw = Some((panel, vec![uv], vec![at])),
                },
                None => self.status = i18n::tr(ui_lang, "Click on the cloth").into(),
            }
        }
        let finish = response.double_clicked_by(egui::PointerButton::Primary)
            || response.ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if finish && let Some((panel, points, _)) = self.authoring.style_lines.draw.take() {
            if points.len() < 2 {
                self.status = i18n::tr(ui_lang, "A line needs two points").into();
                return;
            }
            let tangents = vec![DVec2::ZERO; points.len()];
            if self.edit(Command::InternalLine {
                panel,
                points,
                tangents,
            }) {
                self.status = i18n::tr(ui_lang, "Internal line drawn on the piece").into();
            }
        }
    }

    /// The edge under the pointer or being dragged, with a preview of the change, and the line
    /// being drawn.
    pub(super) fn paint_style_lines(&self, ui: &egui::Ui, rect: egui::Rect) {
        let s = &self.authoring.style_lines;
        let Some(mode) = s.mode else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        let matrix = self.camera.matrix(rect.width() / rect.height());
        let project = |p: DVec3| project_matrix(matrix, rect, p);
        let line = |points: &[DVec3], stroke: Stroke| {
            let screen: Vec<egui::Pos2> = points.iter().filter_map(|p| project(*p)).collect();
            if screen.len() >= 2 {
                painter.add(egui::Shape::line(screen, stroke));
            }
        };
        // The world points of a picked edge, with their share of its length.
        let edge = |pick: &EdgePick| -> Vec<(DVec3, f64)> {
            let g = &self.scene.garment;
            let Some(ids) = g
                .panels
                .iter()
                .find(|r| r.id == pick.panel)
                .and_then(|r| r.boundaries.get(&pick.edge))
            else {
                return vec![];
            };
            let x = &self.scene.state.x;
            let mut run = 0.;
            let mut out = vec![(x[ids[0]], 0.)];
            for w in ids.windows(2) {
                run += x[w[0]].distance(x[w[1]]);
                out.push((x[w[1]], run));
            }
            let total = run.max(1e-12);
            out.into_iter().map(|(p, r)| (p, r / total)).collect()
        };
        if let Some((pick, at)) = &s.drag {
            let points = edge(pick);
            line(
                &points.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                Stroke::new(1., COLOR.gamma_multiply(0.5)),
            );
            let w = *at - pick.world;
            let (first, last) = (points[0].0, points[points.len() - 1].0);
            let moved: Vec<DVec3> = points
                .iter()
                .map(|(p, share)| match mode {
                    StyleMode::Move => *p + w,
                    StyleMode::Edit if pick.t < CORNER_SHARE => *p + w * (1. - share),
                    StyleMode::Edit if pick.t > 1. - CORNER_SHARE => *p + w * *share,
                    StyleMode::Edit => {
                        *p + w * (share * (1. - share) / (pick.t * (1. - pick.t)).max(1e-3))
                    }
                    StyleMode::Scale => {
                        let middle = (first + last) * 0.5;
                        let length = first.distance(last).max(1e-9);
                        let along = (last - first) / length;
                        let side = if (pick.world - middle).dot(along) >= 0. {
                            1.
                        } else {
                            -1.
                        };
                        let scale = (1. + side * w.dot(along) / (0.5 * length)).clamp(0.2, 5.);
                        middle + (*p - middle) * scale
                    }
                    StyleMode::Draw => *p,
                })
                .collect();
            line(&moved, Stroke::new(2.5, COLOR));
            if let (Some(a), Some(b)) = (project(pick.world), project(*at)) {
                painter.line_segment([a, b], Stroke::new(1., COLOR));
                painter.circle_filled(b, 3.5, COLOR);
            }
        } else if let Some(pick) = &s.hover {
            let points: Vec<DVec3> = edge(pick).into_iter().map(|(p, _)| p).collect();
            line(&points, Stroke::new(3., COLOR));
            if let Some(at) = project(pick.world) {
                painter.circle_stroke(at, 5., Stroke::new(1.5, COLOR));
            }
        }
        if let Some((_, _, worlds)) = &s.draw {
            line(worlds, Stroke::new(2.5, COLOR));
            for p in worlds {
                if let Some(at) = project(*p) {
                    painter.circle_filled(at, 3.5, COLOR);
                }
            }
            if let (Some(last), Some(pointer)) = (
                worlds.last().and_then(|p| project(*p)),
                ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)),
            ) {
                painter.line_segment([last, pointer], Stroke::new(1., COLOR));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fashioncraft::{authoring, garment::presets};

    /// A pick at share `t` of the first outline edge of the first piece, lying flat and
    /// unstretched in the xy plane.
    fn pick(template: &GarmentTemplate, t: f64) -> EdgePick {
        let panel = &template.panels[0];
        let poly = panel.polygon.as_ref().unwrap();
        let (a, b) = (poly.points[0], poly.points[1]);
        EdgePick {
            panel: panel.id.clone(),
            edge: poly.edges[0].clone(),
            world: DVec3::ZERO,
            material: a.lerp(b, t),
            t,
            ends: [a, b],
            frame: [DVec3::X, DVec3::Y],
        }
    }

    #[test]
    fn a_world_move_goes_back_through_the_cloth_stretch() {
        // Cloth stretched twice along u and turned so v runs along z.
        let frame = [DVec3::new(2., 0., 0.), DVec3::new(0., 0., 1.)];
        let delta = material_delta(frame, DVec3::new(0.2, 0.5, 0.3));
        assert!((delta - DVec2::new(0.1, 0.3)).length() < 1e-12, "{delta}");
    }

    #[test]
    fn each_mode_makes_its_pattern_change() {
        let base = presets::basic(presets::GarmentKind::Tshirt, 0);
        let poly = base.panels[0].polygon.clone().unwrap();
        let (a, b) = (poly.points[0], poly.points[1]);
        let delta = DVec2::new(0.01, 0.02);
        let w = DVec3::new(delta.x, delta.y, 0.);
        let after = |mode, t| {
            let command = style_command(&base, mode, &[pick(&base, t)], w).unwrap();
            let edit = authoring::apply(&base, command).unwrap();
            // Seams along changed edges take up the difference, so the garment still builds.
            edit.template.clone().compile().unwrap();
            edit.template.panels[0].polygon.clone().unwrap()
        };
        // Moving normalises the piece's material frame; compare shapes by their differences.
        let moved = after(StyleMode::Move, 0.5);
        assert!((moved.points[1] - moved.points[0] - (b - a)).length() < 1e-9);
        let rest = |p: &fashioncraft::garment::polygon::PolygonPattern| p.points[2] - p.points[0];
        assert!((rest(&moved) - (rest(&poly) - delta)).length() < 1e-9);
        // Edit near an end: that corner only.
        let edited = after(StyleMode::Edit, 0.05);
        assert!(
            (edited.points[1] - edited.points[0] - (b - a - delta)).length() < 1e-9,
            "{:?}",
            edited.points
        );
        // Edit in between: the edge bends; its middle follows the drag.
        let bent = after(StyleMode::Edit, 0.5);
        let (a2, b2) = (bent.points[0], bent.points[1]);
        assert!((b2 - a2 - (b - a)).length() < 1e-9);
        let [c, d] = bent.curves[&poly.edges[0]];
        let middle = (a2 + 3. * c + 3. * d + b2) / 8.;
        assert!(
            (middle - (a2 + b2) * 0.5 - delta).length() < 1e-9,
            "{middle}"
        );
        // Scale: the edge grows about its middle by the drag along it.
        let along = (b - a).normalize();
        let scaled = after(StyleMode::Scale, 0.9);
        let grown = scaled.points[0].distance(scaled.points[1]);
        let expected = a.distance(b) + 2. * delta.dot(along);
        assert!((grown - expected).abs() < 1e-9, "{grown} vs {expected}");
    }

    #[test]
    fn a_seam_line_moves_on_both_pieces() {
        let base = presets::basic(presets::GarmentKind::Tshirt, 0);
        let garment = base.clone().compile().unwrap();
        let x = &garment.initial_state.x;
        // A one-to-one seam between two pieces' whole edges.
        let seam = base
            .seams
            .iter()
            .find(|s| {
                s.a_tail.is_empty()
                    && s.b_tail.is_empty()
                    && s.a.panel != s.b.panel
                    && s.a.edge.is_some()
                    && s.b.edge.is_some()
                    && s.a.range == [0., 1.]
                    && s.b.range == [0., 1.]
            })
            .expect("the T-shirt has a whole-edge seam");
        let (panel, edge) = (seam.a.panel.clone(), seam.a.edge.clone().unwrap());
        let picked = edge_pick_at(&garment, x, &panel, &edge, 0.3).unwrap();
        let partners = seam_partners(&base, &garment, x, &picked);
        assert_eq!(partners.len(), 1);
        let other = &partners[0];
        assert_eq!(other.panel, seam.b.panel);
        // The matching point: where the two sides were sewn to meet, the same share along.
        let expected = if seam.reverse_b { 0.7 } else { 0.3 };
        assert!((other.t - expected).abs() < 1e-6, "{}", other.t);
        // Moving it changes both pieces in one edit, and the garment still builds.
        let w = DVec3::new(0., 0.02, 0.);
        let picks = [picked, other.clone()];
        let command = style_command(&base, StyleMode::Move, &picks, w).unwrap();
        let Command::StyleLine { moves, .. } = &command else {
            panic!("not a style line");
        };
        assert_eq!(moves.len(), 2);
        let edit = authoring::apply(&base, command).unwrap();
        edit.template.compile().unwrap();
    }
}
