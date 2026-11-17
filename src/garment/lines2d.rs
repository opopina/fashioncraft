//! Internal lines built from a piece's geometry: copies of a boundary or of the whole outline
//! set off by a distance, lines leaving a boundary at an angle, and the clean-up of points that
//! sit on top of each other or on a straight line. Material metres, v growing downward.
//!
//! Internal lines may not touch the outline, so a line that "reaches" the outline stops
//! [`OUTLINE_GAP_M`] short of it.
use super::{
    domain::{self, InternalLine},
    polygon::PolygonPattern,
    shapes2d::{self, SMOOTH_TURN_DEGREES},
};
use glam::DVec2;

/// How far a line that reaches the outline stops short of it.
pub const OUTLINE_GAP_M: f64 = 0.0005;

/// A named path of a piece as a polyline: a contour boundary or an internal line. `inward`
/// is +1 when the piece's material lies on the left of the path's direction (`d.perp()`),
/// −1 when it lies on the right; for internal lines both sides are material and it is +1.
pub struct Path {
    pub points: Vec<DVec2>,
    pub inward: f64,
    pub boundary: bool,
}

/// The polyline of boundary or internal line `name`.
pub fn path(polygon: &PolygonPattern, name: &str) -> Result<Path, String> {
    if let Some(k) = polygon.edges.iter().position(|e| e == name) {
        let inward = if domain::area(&polygon.points) >= 0. {
            1.
        } else {
            -1.
        };
        return Ok(Path {
            points: polygon.edge_points(k)?,
            inward,
            boundary: true,
        });
    }
    let line = polygon
        .lines
        .iter()
        .find(|l| l.id == name)
        .ok_or("Boundary or internal line not found")?;
    if line.closed {
        return Err("Pick a boundary or an open internal line".into());
    }
    Ok(Path {
        points: line.samples()?,
        inward: 1.,
        boundary: false,
    })
}

/// Every segment a new line may not cross: the outline, the hole rims and the existing
/// internal lines.
fn obstacles(polygon: &PolygonPattern) -> Result<Vec<[DVec2; 2]>, String> {
    let mut out = vec![];
    let mut chain = |points: &[DVec2], closed: bool| {
        let n = points.len();
        let spans = if closed { n } else { n.saturating_sub(1) };
        for i in 0..spans {
            out.push([points[i], points[(i + 1) % n]]);
        }
    };
    chain(&polygon.outer_points()?, true);
    for hole in &polygon.holes {
        chain(&hole.points, true);
    }
    for line in &polygon.lines {
        // Closed shapes repeat their first sample at the end, so both kinds are open chains.
        chain(&line.samples()?, false);
    }
    Ok(out)
}

/// Parameter `t` along `a → b` where it crosses `c → d`, if it does.
fn crossing(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<f64> {
    let r = b - a;
    let s = d - c;
    let denominator = r.perp_dot(s);
    if denominator.abs() < 1e-15 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denominator;
    let u = (c - a).perp_dot(r) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Whether `p` is on material: inside the outline and outside every hole.
fn on_material(polygon: &PolygonPattern, p: DVec2) -> Result<bool, String> {
    Ok(domain::inside(p, &polygon.outer_points()?)
        && !polygon.holes.iter().any(|h| domain::inside(p, &h.points)))
}

fn cumulative(points: &[DVec2]) -> Vec<f64> {
    let mut out = vec![0.];
    for w in points.windows(2) {
        out.push(out.last().unwrap() + w[0].distance(w[1]));
    }
    out
}

/// The point at arclength `s` along a polyline and the direction of the segment it lies on.
pub fn point_at(points: &[DVec2], s: f64) -> (DVec2, DVec2) {
    let lengths = cumulative(points);
    let total = *lengths.last().unwrap();
    let s = s.clamp(0., total);
    let i = (0..points.len() - 1)
        .find(|&i| lengths[i + 1] >= s && lengths[i + 1] > lengths[i])
        .unwrap_or(points.len() - 2);
    let span = (lengths[i + 1] - lengths[i]).max(1e-15);
    let t = ((s - lengths[i]) / span).clamp(0., 1.);
    (
        points[i].lerp(points[i + 1], t),
        (points[i + 1] - points[i]).normalize_or_zero(),
    )
}

/// The part of `points` between arclengths `s0 < s1`.
fn sub_polyline(points: &[DVec2], s0: f64, s1: f64) -> Vec<DVec2> {
    let lengths = cumulative(points);
    let mut out = vec![point_at(points, s0).0];
    for (p, s) in points.iter().zip(&lengths) {
        if *s > s0 + 1e-12 && *s < s1 - 1e-12 {
            out.push(*p);
        }
    }
    out.push(point_at(points, s1).0);
    out.dedup_by(|a, b| a.distance(*b) < 1e-9);
    out
}

/// Keep the stretch of `points` that is on material around arclength `keep`, cut where it
/// meets the outline, a hole or another internal line, [`OUTLINE_GAP_M`] short of it. With `extend`, both ends are
/// first prolonged along their end directions so the line runs up to the outline.
fn fit_to_material(
    polygon: &PolygonPattern,
    points: &[DVec2],
    keep: f64,
    extend: bool,
) -> Result<Vec<DVec2>, String> {
    let mut line = points.to_vec();
    let mut keep = keep;
    if extend {
        let outline = polygon.outer_points()?;
        let (lo, hi) = outline.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        let reach = (hi - lo).length() * 2. + 1.;
        let n = line.len();
        let head = (line[0] - line[1]).normalize_or_zero();
        let tail = (line[n - 1] - line[n - 2]).normalize_or_zero();
        line.insert(0, line[0] + head * reach);
        line.push(line[n] + tail * reach);
        keep += reach;
    }
    if !on_material(polygon, point_at(&line, keep).0)? {
        return Err("The line would leave the fabric".into());
    }
    let lengths = cumulative(&line);
    let total = *lengths.last().unwrap();
    let mut before = 0.;
    let mut after = total;
    let (mut cut_before, mut cut_after) = (false, false);
    for (i, w) in line.windows(2).enumerate() {
        for [c, d] in obstacles(polygon)? {
            if let Some(t) = crossing(w[0], w[1], c, d) {
                let s = lengths[i] + t * (lengths[i + 1] - lengths[i]);
                if s < keep && s >= before {
                    before = s;
                    cut_before = true;
                } else if s > keep && s <= after {
                    after = s;
                    cut_after = true;
                }
            }
        }
    }
    if cut_before {
        before += OUTLINE_GAP_M;
    }
    if cut_after {
        after -= OUTLINE_GAP_M;
    }
    if after - before < 0.002 {
        return Err("The line would be shorter than 2 mm".into());
    }
    Ok(sub_polyline(&line, before, after))
}

/// Unit normal of each polyline vertex, mitred at corners, on the side `side` (+1 = left).
fn vertex_normals(points: &[DVec2], closed: bool, side: f64) -> Vec<DVec2> {
    let n = points.len();
    let left = |i: usize| {
        let j = (i + 1) % n;
        (points[j] - points[i]).normalize_or_zero().perp() * side
    };
    (0..n)
        .map(|i| {
            let (incoming, outgoing) = match (closed, i) {
                (false, 0) => (left(0), left(0)),
                (false, _) if i == n - 1 => (left(n - 2), left(n - 2)),
                _ => (left((i + n - 1) % n), left(i)),
            };
            let sum = incoming + outgoing;
            // Mitre: the offset corner stays at the same distance from both segments.
            sum / (1. + incoming.dot(outgoing)).max(0.25)
        })
        .collect()
}

/// Copies of boundary or open internal line `name`, `distance_m`, 2·`distance_m`, … into the
/// piece (`reverse`: the other side), each cut where it meets the outline and, with `extend`,
/// prolonged up to it.
pub fn offset_lines(
    polygon: &PolygonPattern,
    name: &str,
    count: u32,
    distance_m: f64,
    reverse: bool,
    extend: bool,
) -> Result<Vec<Vec<DVec2>>, String> {
    if !(1..=50).contains(&count) || !distance_m.is_finite() || distance_m < 0.001 {
        return Err("Use 1 to 50 copies at least 1 mm apart".into());
    }
    let path = path(polygon, name)?;
    let side = if reverse { -path.inward } else { path.inward };
    let normals = vertex_normals(&path.points, false, side);
    let middle = cumulative(&path.points).last().unwrap() * 0.5;
    (1..=count)
        .map(|k| {
            let d = distance_m * k as f64;
            let moved: Vec<DVec2> = path
                .points
                .iter()
                .zip(&normals)
                .map(|(p, n)| *p + *n * d)
                .collect();
            fit_to_material(polygon, &moved, middle, extend)
        })
        .collect()
}

/// Closed rings `distance_m`, 2·`distance_m`, … inside the whole outline.
pub fn offset_outline(
    polygon: &PolygonPattern,
    count: u32,
    distance_m: f64,
) -> Result<Vec<Vec<DVec2>>, String> {
    if !(1..=50).contains(&count) || !distance_m.is_finite() || distance_m < 0.001 {
        return Err("Use 1 to 50 copies at least 1 mm apart".into());
    }
    let ring = polygon.outer_points()?;
    let side = if domain::area(&ring) >= 0. { 1. } else { -1. };
    let normals = vertex_normals(&ring, true, side);
    (1..=count)
        .map(|k| {
            let d = distance_m * k as f64;
            let inset: Vec<DVec2> = ring
                .iter()
                .zip(&normals)
                .map(|(p, n)| *p + *n * d)
                .collect();
            if domain::area(&inset) * domain::area(&ring) <= 0. || domain::area(&inset).abs() < 1e-6
            {
                return Err("The piece is too small for that distance".into());
            }
            Ok(inset)
        })
        .collect()
}

/// A straight line leaving boundary or open internal line `name` at arclength `at_m` from its
/// start, turned `angle_degrees` from the path direction towards the material (90° is
/// perpendicular). It is `length_m` long, or runs up to the outline when `length_m` is `None`.
pub fn perpendicular_line(
    polygon: &PolygonPattern,
    name: &str,
    at_m: f64,
    angle_degrees: f64,
    length_m: Option<f64>,
    reverse: bool,
) -> Result<[DVec2; 2], String> {
    if !(1.0..=179.0).contains(&angle_degrees) {
        return Err("The angle must be between 1° and 179°".into());
    }
    if length_m.is_some_and(|l| !l.is_finite() || l < 0.002) {
        return Err("The line must be at least 2 mm long".into());
    }
    let path = path(polygon, name)?;
    let total = *cumulative(&path.points).last().unwrap();
    if !at_m.is_finite() || !(0.0..=total).contains(&at_m) {
        return Err("The position must lie on the segment".into());
    }
    let (p, t) = point_at(&path.points, at_m);
    let side = if reverse { -path.inward } else { path.inward };
    let a = angle_degrees.to_radians();
    let direction = t * a.cos() + t.perp() * side * a.sin();
    // Leave the boundary (or the line, which the new one may not touch either) by the gap
    // measured across it, whatever the angle.
    let start = p + direction * (OUTLINE_GAP_M / a.sin());
    if !on_material(polygon, start)? {
        return Err("The line would leave the fabric".into());
    }
    let full = fit_to_material(polygon, &[start, start + direction * 1e-3], 0., true)?;
    let room = full.last().unwrap().distance(start);
    let length = match length_m {
        None => room,
        Some(l) if l <= room + 1e-9 => l,
        Some(_) => return Err("The line would leave the fabric; shorten it or extend it".into()),
    };
    Ok([start, start + direction * length])
}

/// An open internal line through a polyline: at most 64 anchors, smooth where the polyline
/// bends gently, sharp at corners.
pub fn line_from_polyline(id: String, points: &[DVec2]) -> Result<InternalLine, String> {
    let points = thin_open(points, 64);
    if points.len() < 2 {
        return Err("The line is too short".into());
    }
    let n = points.len();
    let tangents = (0..n)
        .map(|i| {
            if i == 0 || i == n - 1 {
                return DVec2::ZERO;
            }
            let (a, p, b) = (points[i - 1], points[i], points[i + 1]);
            if (p - a).angle_to(b - p).abs().to_degrees() < SMOOTH_TURN_DEGREES {
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
        closed: false,
        group: None,
    })
}

/// A closed internal shape through a ring (see [`shapes2d`]).
pub fn shape_from_ring(id: String, ring: &[DVec2]) -> Result<InternalLine, String> {
    shapes2d::shape_from_ring(id, ring)
}

/// Visvalingam–Whyatt thinning of an open polyline to at most `max` points; the ends stay,
/// and points that change the line less than 1 µm² are always dropped.
fn thin_open(points: &[DVec2], max: usize) -> Vec<DVec2> {
    let mut out = points.to_vec();
    loop {
        let n = out.len();
        if n <= 2 {
            break;
        }
        let weight = |i: usize| ((out[i] - out[i - 1]).perp_dot(out[i + 1] - out[i - 1])).abs();
        let least = (1..n - 1)
            .min_by(|x, y| weight(*x).total_cmp(&weight(*y)))
            .unwrap();
        if n <= max.max(2) && weight(least) > 1e-12 {
            break;
        }
        out.remove(least);
    }
    out
}

/// Contour points to remove from an outline: each point that starts a boundary shorter than
/// `overlap_m` (the boundary goes with it and its neighbour runs on to the next point), and,
/// when `straight_degrees` is set, each point between two straight boundaries that turn
/// less than that. `keep` says which boundaries must keep their name and length (sewn ones).
/// Indices are returned in decreasing order, never leaving fewer than three points.
pub fn removable_points(
    polygon: &PolygonPattern,
    overlap_m: f64,
    straight_degrees: Option<f64>,
    keep: impl Fn(&str) -> bool,
) -> Vec<usize> {
    let mut points = polygon.points.clone();
    let mut edges = polygon.edges.clone();
    let mut removed = vec![];
    let mut index: Vec<usize> = (0..points.len()).collect();
    let mut k = 0;
    while k < points.len() && points.len() > 3 {
        let n = points.len();
        let next = (k + 1) % n;
        let previous = (k + n - 1) % n;
        let curved = |e: &String| polygon.curves.contains_key(e);
        let overlapping = points[k].distance(points[next]) < overlap_m && !keep(&edges[k]);
        let straight = straight_degrees.is_some_and(|limit| {
            !curved(&edges[k])
                && !curved(&edges[previous])
                && !keep(&edges[k])
                && !keep(&edges[previous])
                && (points[k] - points[previous])
                    .angle_to(points[next] - points[k])
                    .abs()
                    .to_degrees()
                    < limit
        });
        if overlapping || straight {
            removed.push(index[k]);
            points.remove(k);
            edges.remove(k);
            index.remove(k);
        } else {
            k += 1;
        }
    }
    removed.sort_unstable_by(|a, b| b.cmp(a));
    removed
}

/// Anchors of an internal line closer than `overlap_m` to the previous kept anchor, in
/// decreasing order; an open line keeps at least two anchors, a closed one three.
pub fn removable_line_points(line: &InternalLine, overlap_m: f64) -> Vec<usize> {
    let minimum = if line.closed { 3 } else { 2 };
    let mut kept = vec![0];
    let mut removed = vec![];
    for i in 1..line.points.len() {
        let last = line.points[*kept.last().unwrap()];
        let is_end = !line.closed && i == line.points.len() - 1;
        if line.points[i].distance(last) < overlap_m && line.points.len() - removed.len() > minimum
        {
            if is_end {
                // Keep the end anchor; drop the one before it instead.
                let previous = kept.pop().unwrap();
                if previous == 0 {
                    kept.push(previous);
                    kept.push(i);
                    continue;
                }
                removed.push(previous);
                kept.push(i);
            } else {
                removed.push(i);
            }
        } else {
            kept.push(i);
        }
    }
    if line.closed
        && line.points.len() - removed.len() > minimum
        && let Some(&last) = kept.last()
        && last != 0
        && line.points[last].distance(line.points[0]) < overlap_m
    {
        removed.push(last);
    }
    removed.sort_unstable_by(|a, b| b.cmp(a));
    removed.dedup();
    removed
}
