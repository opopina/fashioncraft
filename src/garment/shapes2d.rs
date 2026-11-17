//! Drafting shapes beyond the basic primitives: closed internal shapes (rectangle, ellipse)
//! with replication, conversion between internal shapes and holes, and the spiral pattern.
//! Everything is in material metres with v growing downward.
use super::{
    domain::{self, Hole, InternalLine},
    meshing::MeshingSettings,
    polygon::PolygonPattern,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Kind of closed internal shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternalShapeKind {
    Rectangle,
    Ellipse,
}

/// Cubic Bézier handle length for a quarter ellipse, as a fraction of the radius.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// A closed internal rectangle or ellipse of `size` centred on `center`.
pub fn internal_shape(
    id: String,
    kind: InternalShapeKind,
    center: DVec2,
    size: DVec2,
) -> Result<InternalLine, String> {
    if !center.is_finite()
        || !size.is_finite()
        || size.min_element() < 0.002
        || size.max_element() > 10.
    {
        return Err("Internal shapes must be between 2 mm and 10 m".into());
    }
    let h = size * 0.5;
    let (points, tangents) = match kind {
        InternalShapeKind::Rectangle => (
            vec![
                center + DVec2::new(-h.x, -h.y),
                center + DVec2::new(h.x, -h.y),
                center + DVec2::new(h.x, h.y),
                center + DVec2::new(-h.x, h.y),
            ],
            vec![DVec2::ZERO; 4],
        ),
        InternalShapeKind::Ellipse => (
            vec![
                center + DVec2::new(h.x, 0.),
                center + DVec2::new(0., h.y),
                center + DVec2::new(-h.x, 0.),
                center + DVec2::new(0., -h.y),
            ],
            vec![
                DVec2::new(0., KAPPA * h.y),
                DVec2::new(-KAPPA * h.x, 0.),
                DVec2::new(0., -KAPPA * h.y),
                DVec2::new(KAPPA * h.x, 0.),
            ],
        ),
    };
    Ok(InternalLine {
        id,
        points,
        tangents,
        closed: true,
    })
}

/// Centres of `count` copies placed `interval_m` apart (centre to centre) from `first`, along
/// `angle_degrees`: 0° points up the board (−v), 90° to the right (+u), as garment tools
/// measure it.
pub fn replicate(first: DVec2, count: u32, interval_m: f64, angle_degrees: f64) -> Vec<DVec2> {
    let a = angle_degrees.to_radians();
    let step = DVec2::new(a.sin(), -a.cos()) * interval_m;
    (0..count).map(|k| first + step * f64::from(k)).collect()
}

/// The hole a closed internal shape cuts: its flattened outline, clockwise, one named edge per
/// segment (`{line id}_{k}`).
pub fn shape_to_hole(line: &InternalLine) -> Result<Hole, String> {
    if !line.closed {
        return Err("Only closed internal shapes can become holes".into());
    }
    let mut points = line.samples()?;
    points.pop();
    if domain::area(&points) > 0. {
        points.reverse();
    }
    if points.len() > 4096 {
        return Err("The shape is too detailed to become a hole".into());
    }
    let edges = (0..points.len())
        .map(|k| format!("{}_{}", line.id, k + 1))
        .collect();
    Ok(Hole { points, edges })
}

/// The closed internal shape a hole leaves when it is filled back in. Holes cut from curves are
/// dense polylines, so the rim is first thinned to at most 64 corners (dropping the corners
/// that change the outline least), then gently turning corners get smooth Catmull–Rom handles
/// while sharp ones (a rectangle's) stay sharp.
pub fn hole_to_shape(id: String, hole: &Hole) -> Result<InternalLine, String> {
    let points = thin_closed(&hole.points, 64);
    if points.len() < 3 {
        return Err("The hole is too small to become an internal shape".into());
    }
    let n = points.len();
    let tangents = (0..n)
        .map(|i| {
            let (a, p, b) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            let turn = (p - a).angle_to(b - p).abs().to_degrees();
            if turn < SMOOTH_TURN_DEGREES {
                (b - a) / 6.
            } else {
                DVec2::ZERO
            }
        })
        .collect();
    Ok(InternalLine {
        id,
        points,
        tangents,
        closed: true,
    })
}

/// Corners turning less than this are treated as samples of a curve.
const SMOOTH_TURN_DEGREES: f64 = 25.;

/// Visvalingam–Whyatt thinning of a closed outline to at most `max` points.
fn thin_closed(points: &[DVec2], max: usize) -> Vec<DVec2> {
    let mut out = points.to_vec();
    while out.len() > max.max(3) {
        let n = out.len();
        let weight = |i: usize| {
            let (a, p, b) = (out[(i + n - 1) % n], out[i], out[(i + 1) % n]);
            ((p - a).perp_dot(b - a)).abs()
        };
        let least = (0..n)
            .min_by(|x, y| weight(*x).total_cmp(&weight(*y)))
            .unwrap_or(0);
        out.remove(least);
    }
    out
}

/// The spiral strip pattern: an Archimedean spiral whose centreline starts `radius_m` from the
/// centre and moves out by `offset_m` per turn, with a width that changes linearly from
/// `inner_width_m` to `outer_width_m`. `inner_length_m` is the length of the inner edge and
/// sets how far the strip winds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spiral {
    pub radius_m: f64,
    pub inner_length_m: f64,
    pub inner_width_m: f64,
    pub outer_width_m: f64,
    pub offset_m: f64,
    pub clockwise: bool,
}

impl Default for Spiral {
    /// A medium ruffle strip.
    fn default() -> Self {
        Self {
            radius_m: 0.16,
            inner_length_m: 2.278,
            inner_width_m: 0.08,
            outer_width_m: 0.08,
            offset_m: 0.2,
            clockwise: true,
        }
    }
}

/// Samples per turn when the spiral is traced with cubic segments.
const SPIRAL_SEGMENTS_PER_TURN: f64 = 16.;

impl Spiral {
    fn validate(&self) -> Result<(), String> {
        let values = [
            self.radius_m,
            self.inner_length_m,
            self.inner_width_m,
            self.outer_width_m,
            self.offset_m,
        ];
        if values.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("Spiral sizes must be positive".into());
        }
        if self.inner_width_m * 0.5 >= self.radius_m {
            return Err("The radius must exceed half the inner width".into());
        }
        if self.offset_m <= self.inner_width_m.max(self.outer_width_m) {
            return Err(
                "The offset must exceed the strip width, or the turns would overlap".into(),
            );
        }
        if self.inner_length_m > 100. || self.radius_m > 10. || self.offset_m > 10. {
            return Err("The spiral is too large".into());
        }
        Ok(())
    }

    fn width(&self, theta: f64, end: f64) -> f64 {
        self.inner_width_m + (self.outer_width_m - self.inner_width_m) * (theta / end)
    }

    /// Radius of the inner (`side = −1`) or outer (`side = 1`) edge at `theta`.
    fn edge_radius(&self, theta: f64, end: f64, side: f64) -> f64 {
        self.radius_m
            + self.offset_m / std::f64::consts::TAU * theta
            + side * self.width(theta, end) * 0.5
    }

    fn edge_length(&self, end: f64, side: f64) -> f64 {
        let n = ((end / std::f64::consts::TAU) * 2000.).ceil().max(200.) as usize;
        let point = |t: f64| {
            let r = self.edge_radius(t, end, side);
            DVec2::new(r * t.cos(), r * t.sin())
        };
        (0..n)
            .map(|i| {
                point(end * i as f64 / n as f64).distance(point(end * (i + 1) as f64 / n as f64))
            })
            .sum()
    }

    /// Winding angle at which the inner edge reaches `inner_length_m`.
    pub fn end_angle(&self) -> Result<f64, String> {
        self.validate()?;
        let (mut lo, mut hi) = (0., 1.);
        while self.edge_length(hi, -1.) < self.inner_length_m {
            hi *= 2.;
            if hi > 2000. {
                return Err("The spiral is too long".into());
            }
        }
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if self.edge_length(mid, -1.) < self.inner_length_m {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Ok(0.5 * (lo + hi))
    }

    /// Length of the outer edge for the current settings.
    pub fn outer_length(&self) -> Result<f64, String> {
        let end = self.end_angle()?;
        Ok(self.edge_length(end, 1.))
    }

    /// The strip as a counterclockwise pattern outline with cubic edges, translated so its
    /// bounds start at the origin.
    pub fn polygon(&self, meshing: MeshingSettings) -> Result<PolygonPattern, String> {
        let end = self.end_angle()?;
        let segments = ((end / std::f64::consts::TAU) * SPIRAL_SEGMENTS_PER_TURN)
            .ceil()
            .max(4.) as usize;
        let mirror = if self.clockwise { -1. } else { 1. };
        let b = self.offset_m / std::f64::consts::TAU;
        let slope = |side: f64| b + side * (self.outer_width_m - self.inner_width_m) / end * 0.5;
        // Position and derivative with respect to theta on one edge.
        let at = |t: f64, side: f64| {
            let r = self.edge_radius(t, end, side);
            let dr = slope(side);
            let (s, c) = t.sin_cos();
            (
                DVec2::new(r * c, mirror * r * s),
                DVec2::new(dr * c - r * s, mirror * (dr * s + r * c)),
            )
        };
        let step = end / segments as f64;
        // Each entry: the point and, when the edge leaving it is curved, its two handles.
        let mut outline: Vec<(DVec2, Option<[DVec2; 2]>)> = Vec::new();
        for (side, forward) in [(1., true), (-1., false)] {
            for k in 0..segments {
                let (t0, t1) = if forward {
                    (step * k as f64, step * (k + 1) as f64)
                } else {
                    (end - step * k as f64, end - step * (k + 1) as f64)
                };
                let (p0, d0) = at(t0, side);
                let (p1, d1) = at(t1, side);
                let h = (t1 - t0) / 3.;
                outline.push((p0, Some([p0 + d0 * h, p1 - d1 * h])));
            }
            // The straight end cap leaves the last point of this edge.
            let (cap, _) = at(if forward { end } else { 0. }, side);
            outline.push((cap, None));
        }
        let mut points: Vec<DVec2> = outline.iter().map(|(p, _)| *p).collect();
        let mut curves: Vec<Option<[DVec2; 2]>> = outline.iter().map(|(_, c)| *c).collect();
        if domain::area(&points) < 0. {
            // Reverse the loop: each edge now runs the other way, so its handles swap.
            let n = points.len();
            let reversed_points: Vec<DVec2> = (0..n).map(|i| points[(n - i) % n]).collect();
            let reversed_curves: Vec<Option<[DVec2; 2]>> = (0..n)
                .map(|i| curves[(2 * n - 1 - i) % n].map(|[a, b]| [b, a]))
                .collect();
            points = reversed_points;
            curves = reversed_curves;
        }
        let lo = points
            .iter()
            .chain(curves.iter().flatten().flatten())
            .fold(DVec2::splat(f64::INFINITY), |m, p| m.min(*p));
        let points: Vec<DVec2> = points.into_iter().map(|p| p - lo).collect();
        let edges: Vec<String> = (0..points.len())
            .map(|i| format!("spiral_{}", i + 1))
            .collect();
        let curves: BTreeMap<String, [DVec2; 2]> = curves
            .into_iter()
            .enumerate()
            .filter_map(|(i, c)| c.map(|[a, b]| (edges[i].clone(), [a - lo, b - lo])))
            .collect();
        Ok(PolygonPattern {
            points,
            edges,
            subdivisions: 0,
            meshing: Some(meshing),
            curves,
            holes: vec![],
            lines: vec![],
            target_lengths: Default::default(),
            seam_allowances: Default::default(),
            notches: Default::default(),
        })
    }
}
