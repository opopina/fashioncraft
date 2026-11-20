//! 2D measurements: distances from a point inside a piece to the piece outline along the four
//! board directions (left, right, up, down). Shown from the cursor while the internal-line tool
//! is active, and from the centre of the selected internal line. Presentation only.
use super::*;
use glam::DVec2;

/// Purple, as dimension lines usually are, apart from guides and smart guides.
const MEASURE_COLOR: Color32 = Color32::from_rgb(0x7B, 0x3F, 0xC4);

/// The four board directions, in the order [`axis_distances`] reports them: left, right, up
/// (towards smaller v) and down (towards larger v; v grows downward on the board).
pub(super) const DIRECTIONS: [DVec2; 4] = [
    DVec2::new(-1., 0.),
    DVec2::new(1., 0.),
    DVec2::new(0., -1.),
    DVec2::new(0., 1.),
];

/// For each of [`DIRECTIONS`], the distance from `p` to the closed `outline` along that
/// direction and the point where the ray meets it; `None` when the ray misses.
pub(super) fn axis_distances(p: DVec2, outline: &[DVec2]) -> [Option<(f64, DVec2)>; 4] {
    DIRECTIONS.map(|d| {
        let mut best: Option<f64> = None;
        for k in 0..outline.len() {
            let a = outline[k];
            let b = outline[(k + 1) % outline.len()];
            let e = b - a;
            let denom = d.perp_dot(e);
            if denom.abs() < 1e-15 {
                continue;
            }
            let w = a - p;
            let t = w.perp_dot(e) / denom;
            let s = w.perp_dot(d) / denom;
            if t > 1e-12 && (-1e-12..=1. + 1e-12).contains(&s) && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
        best.map(|t| (t, p + d * t))
    })
}

/// Even-odd point-in-polygon test.
pub(super) fn contains(outline: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    for k in 0..outline.len() {
        let a = outline[k];
        let b = outline[(k + 1) % outline.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            inside = !inside;
        }
    }
    inside
}

impl Atelier {
    /// The outer contour of piece `i` on the board, with curves flattened.
    pub(super) fn board_outline(&self, i: usize) -> Option<Vec<DVec2>> {
        let poly = self.board_polygon(i)?;
        let origin = self.board_origin(i);
        let angle = self.board_angle(i);
        let mut outline = Vec::new();
        for k in 0..poly.edges.len() {
            let points = poly.edge_points(k).ok()?;
            outline.extend(
                points[..points.len() - 1]
                    .iter()
                    .map(|uv| origin + fashioncraft::authoring::rotate_2d(*uv, angle)),
            );
        }
        (outline.len() >= 3).then_some(outline)
    }

    /// Where 2D measurements start this frame: the cursor inside a piece while drawing an
    /// internal line, or the centre of the selected internal line.
    fn measurement_origin(
        &self,
        rect: egui::Rect,
        hover: Option<egui::Pos2>,
    ) -> Option<(DVec2, usize)> {
        let panels = &self.project.template.panels;
        if self.authoring.tool == Tool::InternalLine
            && let Some(pos) = hover
        {
            let p = self.authoring.board.world(rect, pos);
            return (0..panels.len()).rev().find_map(|i| {
                self.board_outline(i)
                    .filter(|outline| contains(outline, p))
                    .map(|_| (p, i))
            });
        }
        let edge = self.authoring.edge.as_ref()?;
        let i = panels.iter().position(|p| p.id == edge.panel)?;
        let poly = self.board_polygon(i)?;
        let line = poly
            .lines
            .iter()
            .find(|line| edge.edge.as_ref() == Some(&line.id))?;
        let (lo, hi) = line.points.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        let centre = (lo + hi) * 0.5;
        Some((
            self.board_origin(i) + fashioncraft::authoring::rotate_2d(centre, self.board_angle(i)),
            i,
        ))
    }

    /// Dashed purple distance lines from the measurement origin to the piece outline.
    pub(super) fn paint_measurements_2d(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        hover: Option<egui::Pos2>,
    ) {
        let Some((p, i)) = self.measurement_origin(rect, hover) else {
            return;
        };
        let Some(outline) = self.board_outline(i) else {
            return;
        };
        let board = &self.authoring.board;
        let unit = self.prefs.pattern_display.ruler_unit;
        let from = board.screen(rect, p);
        painter.circle_filled(from, 3., MEASURE_COLOR);
        for (distance, hit) in axis_distances(p, &outline).into_iter().flatten() {
            let to = board.screen(rect, hit);
            painter.extend(egui::Shape::dashed_line(
                &[from, to],
                Stroke::new(1.2, MEASURE_COLOR),
                5.,
                3.,
            ));
            painter.circle_stroke(to, 2.5, Stroke::new(1., MEASURE_COLOR));
            if from.distance(to) > 24. {
                painter.text(
                    from + (to - from) * 0.5 + egui::vec2(3., -3.),
                    egui::Align2::LEFT_BOTTOM,
                    unit.format(distance),
                    egui::FontId::proportional(11.),
                    MEASURE_COLOR,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<DVec2> {
        vec![
            DVec2::new(0., 0.),
            DVec2::new(0.4, 0.),
            DVec2::new(0.4, 0.6),
            DVec2::new(0., 0.6),
        ]
    }

    #[test]
    fn distances_reach_each_side_of_a_rectangle() {
        let [left, right, up, down] = axis_distances(DVec2::new(0.1, 0.2), &square());
        let close = |v: Option<(f64, DVec2)>, d: f64| (v.unwrap().0 - d).abs() < 1e-12;
        assert!(close(left, 0.1));
        assert!(close(right, 0.3));
        assert!(close(up, 0.2));
        assert!(close(down, 0.4));
        assert_eq!(left.unwrap().1, DVec2::new(0., 0.2));
    }

    #[test]
    fn slanted_edges_and_outside_points() {
        // A right triangle with its hypotenuse from (0.4, 0) to (0, 0.4).
        let triangle = [DVec2::ZERO, DVec2::new(0.4, 0.), DVec2::new(0., 0.4)];
        let p = DVec2::new(0.1, 0.1);
        assert!(contains(&triangle, p));
        let [_, right, _, down] = axis_distances(p, &triangle);
        assert!((right.unwrap().0 - 0.2).abs() < 1e-12);
        assert!((down.unwrap().0 - 0.2).abs() < 1e-12);
        let outside = DVec2::new(1., 1.);
        assert!(!contains(&triangle, outside));
        let [_, right, _, down] = axis_distances(outside, &triangle);
        assert!(
            right.is_none() && down.is_none(),
            "rays pointing away miss the piece"
        );
    }

    #[test]
    fn concave_outlines_report_the_nearest_wall() {
        // A U shape: the ray to the right from inside the left arm stops at the inner wall.
        let u = [
            DVec2::new(0., 0.),
            DVec2::new(0.1, 0.),
            DVec2::new(0.1, 0.3),
            DVec2::new(0.3, 0.3),
            DVec2::new(0.3, 0.),
            DVec2::new(0.4, 0.),
            DVec2::new(0.4, 0.4),
            DVec2::new(0., 0.4),
        ];
        let p = DVec2::new(0.05, 0.1);
        assert!(contains(&u, p));
        let [_, right, _, _] = axis_distances(p, &u);
        assert!((right.unwrap().0 - 0.05).abs() < 1e-12);
    }
}
