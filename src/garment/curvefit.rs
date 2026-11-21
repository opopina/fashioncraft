//! Fewer curve points for the same curve: smooth contour points are removed one at a time,
//! always the one whose two neighbouring spans are best replaced by a single cubic, and the
//! merged span is refitted to the original outline (least squares with the original tangent
//! directions at its ends). Corners are never removed. Material metres.
use super::{domain::InternalLine, polygon::PolygonPattern};
use glam::DVec2;
use std::collections::BTreeMap;

/// A point turning less than this between its incoming and outgoing tangents is smooth.
const SMOOTH_DEGREES: f64 = 25.;

fn bezier(c: &[DVec2; 4], t: f64) -> DVec2 {
    let u = 1. - t;
    c[0] * (u * u * u) + c[1] * (3. * u * u * t) + c[2] * (3. * u * t * t) + c[3] * (t * t * t)
}

fn bezier_d1(c: &[DVec2; 4], t: f64) -> DVec2 {
    let u = 1. - t;
    (c[1] - c[0]) * (3. * u * u) + (c[2] - c[1]) * (6. * u * t) + (c[3] - c[2]) * (3. * t * t)
}

fn bezier_d2(c: &[DVec2; 4], t: f64) -> DVec2 {
    (c[2] - c[1] * 2. + c[0]) * (6. * (1. - t)) + (c[3] - c[2] * 2. + c[1]) * (6. * t)
}

/// One cubic from `samples[0]` to the last sample, leaving along `start_dir` and arriving
/// along `end_dir` (both pointing forward along the curve). A zero direction pins that handle
/// on its end point (a sharp end). Returns the control points and the largest distance from a
/// sample to the curve.
pub fn fit_cubic(samples: &[DVec2], start_dir: DVec2, end_dir: DVec2) -> ([DVec2; 4], f64) {
    let (p0, p3) = (samples[0], *samples.last().unwrap());
    let chord = p0.distance(p3).max(1e-12);
    let t1 = start_dir.normalize_or_zero();
    let t2 = -end_dir.normalize_or_zero();
    // Chord-length parameters.
    let mut u: Vec<f64> = vec![0.];
    for w in samples.windows(2) {
        u.push(u.last().unwrap() + w[0].distance(w[1]));
    }
    let total = u.last().copied().unwrap_or(0.).max(1e-15);
    for v in &mut u {
        *v /= total;
    }
    // Start from the linear least-squares handles at chord-length parameters.
    let (mut alpha1, mut alpha2) = {
        let (mut c11, mut c12, mut c22, mut x1, mut x2) = (0., 0., 0., 0., 0.);
        for (q, &t) in samples.iter().zip(&u) {
            let s = 1. - t;
            let (b0, b1, b2, b3) = (s * s * s, 3. * s * s * t, 3. * s * t * t, t * t * t);
            let (a1, a2) = (t1 * b1, t2 * b2);
            c11 += a1.dot(a1);
            c12 += a1.dot(a2);
            c22 += a2.dot(a2);
            let rest = *q - (p0 * (b0 + b1) + p3 * (b2 + b3));
            x1 += rest.dot(a1);
            x2 += rest.dot(a2);
        }
        let det = c11 * c22 - c12 * c12;
        if c11 < 1e-18 && c22 < 1e-18 {
            (0., 0.)
        } else if c11 < 1e-18 {
            (0., (x2 / c22).max(0.))
        } else if c22 < 1e-18 {
            ((x1 / c11).max(0.), 0.)
        } else if det.abs() > 1e-18 * c11 * c22 {
            ((x1 * c22 - x2 * c12) / det, (c11 * x2 - c12 * x1) / det)
        } else {
            (chord / 3., chord / 3.)
        }
    };
    if !(alpha1 > 0. && alpha1.is_finite()) {
        alpha1 = if t1 == DVec2::ZERO { 0. } else { chord / 3. };
    }
    if !(alpha2 > 0. && alpha2.is_finite()) {
        alpha2 = if t2 == DVec2::ZERO { 0. } else { chord / 3. };
    }
    let build = |a1: f64, a2: f64| [p0, p0 + t1 * a1, p3 + t2 * a2, p3];
    let mut curve = build(alpha1, alpha2);
    // Gauss–Newton on the distances along the curve normal: moving along the curve does not
    // change a sample's distance, so only the normal part of each handle's effect counts.
    for _ in 0..12 {
        for _ in 0..3 {
            for (q, t) in samples.iter().zip(u.iter_mut()).skip(1) {
                let d = bezier(&curve, *t) - *q;
                let d1 = bezier_d1(&curve, *t);
                let d2 = bezier_d2(&curve, *t);
                let denominator = d1.dot(d1) + d.dot(d2);
                if denominator.abs() > 1e-18 {
                    *t = (*t - d.dot(d1) / denominator).clamp(0., 1.);
                }
            }
            if let Some(last) = u.last_mut() {
                *last = 1.;
            }
        }
        let (mut j11, mut j12, mut j22, mut g1, mut g2) = (0., 0., 0., 0., 0.);
        for (q, &t) in samples.iter().zip(&u) {
            let normal = bezier_d1(&curve, t).perp().normalize_or_zero();
            let e = normal.dot(*q - bezier(&curve, t));
            let s = 1. - t;
            let (ja, jb) = (
                normal.dot(t1) * 3. * s * s * t,
                normal.dot(t2) * 3. * s * t * t,
            );
            j11 += ja * ja;
            j12 += ja * jb;
            j22 += jb * jb;
            g1 += ja * e;
            g2 += jb * e;
        }
        let damping = 1e-12 * (j11 + j22 + 1e-30);
        let (j11, j22) = (j11 + damping, j22 + damping);
        let det = j11 * j22 - j12 * j12;
        if det.abs() < 1e-30 {
            break;
        }
        let (d1, d2) = ((g1 * j22 - g2 * j12) / det, (j11 * g2 - j12 * g1) / det);
        if t1 != DVec2::ZERO {
            alpha1 = (alpha1 + d1).max(0.);
        }
        if t2 != DVec2::ZERO {
            alpha2 = (alpha2 + d2).max(0.);
        }
        curve = build(alpha1, alpha2);
        if d1.abs() + d2.abs() < chord * 1e-12 {
            break;
        }
    }
    let error = samples
        .iter()
        .zip(&u)
        .map(|(q, &t)| q.distance(bezier(&curve, t)))
        .fold(0., f64::max);
    (curve, error)
}

/// A span of the reduced outline: the original outline between two kept points.
#[derive(Clone)]
struct Span {
    /// Original edge indices covered, in order.
    edges: Vec<usize>,
    samples: Vec<DVec2>,
    start_dir: DVec2,
    end_dir: DVec2,
}

fn merged(a: &Span, b: &Span) -> Span {
    let mut samples = a.samples.clone();
    samples.extend_from_slice(&b.samples[1..]);
    let mut edges = a.edges.clone();
    edges.extend_from_slice(&b.edges);
    Span {
        edges,
        samples,
        start_dir: a.start_dir,
        end_dir: b.end_dir,
    }
}

/// Outgoing and incoming tangent directions of every outline point.
fn tangents(polygon: &PolygonPattern) -> Vec<(DVec2, DVec2)> {
    let n = polygon.points.len();
    (0..n)
        .map(|k| {
            let previous = (k + n - 1) % n;
            let p = polygon.points[k];
            let outgoing = match polygon.curves.get(&polygon.edges[k]) {
                Some([c0, c1]) if c0.distance(p) > 1e-12 => *c0 - p,
                Some([_, c1]) if c1.distance(p) > 1e-12 => *c1 - p,
                _ => polygon.points[(k + 1) % n] - p,
            };
            let incoming = match polygon.curves.get(&polygon.edges[previous]) {
                Some([c0, c1]) if c1.distance(p) > 1e-12 => p - *c1,
                Some([c0, _]) if c0.distance(p) > 1e-12 => p - *c0,
                _ => p - polygon.points[previous],
            };
            (incoming, outgoing)
        })
        .collect()
}

/// Indices of the smooth points that may be removed: on a curve, turning gently, and not
/// ending a boundary that `keep` protects. An outline with no curves at all is read as a curve
/// drawn as short straight steps (as CAD files bring them, a DXF's polylines): there every
/// gently turning point is a sample of the curve.
fn removable(polygon: &PolygonPattern, keep: &impl Fn(&str) -> bool) -> Vec<usize> {
    let n = polygon.points.len();
    let dirs = tangents(polygon);
    let sampled = polygon.curves.is_empty();
    (0..n)
        .filter(|&k| {
            let previous = (k + n - 1) % n;
            let (incoming, outgoing) = dirs[k];
            (sampled
                || polygon.curves.contains_key(&polygon.edges[k])
                || polygon.curves.contains_key(&polygon.edges[previous]))
                && incoming.angle_to(outgoing).abs().to_degrees() < SMOOTH_DEGREES
                && !keep(&polygon.edges[k])
                && !keep(&polygon.edges[previous])
        })
        .collect()
}

/// The greedy reduction over a chain of spans (span i starts at node i): removes up to
/// `remove` of the `candidates` nodes, each time the one whose two spans are best replaced by
/// one fitted cubic. Returns the remaining spans with their start nodes, and the worst fit
/// error after each removal.
fn reduce(
    mut spans: Vec<Span>,
    closed: bool,
    candidates: &[usize],
    remove: usize,
    minimum: usize,
) -> (Vec<Span>, Vec<usize>, Vec<f64>) {
    let mut starts: Vec<usize> = (0..spans.len()).collect();
    let mut errors = vec![];
    let mut worst: f64 = 0.;
    for _ in 0..remove {
        let m = spans.len();
        if m <= minimum {
            break;
        }
        let first = usize::from(!closed);
        let options: Vec<(usize, f64, f64)> = (first..m)
            .filter(|&i| candidates.contains(&starts[i]))
            .map(|i| {
                let previous = (i + m - 1) % m;
                let joined = merged(&spans[previous], &spans[i]);
                let (_, error) = fit_cubic(&joined.samples, joined.start_dir, joined.end_dir);
                let length: f64 = joined.samples.windows(2).map(|w| w[0].distance(w[1])).sum();
                (i, error, length)
            })
            .collect();
        // Errors that differ only by rounding are a tie (on an even arc they all do, and the
        // rounding of sin and cos differs between platforms): the shortest joined span goes
        // first, which keeps the remaining nodes evenly spread, then the lowest index.
        let Some(least) = options.iter().map(|o| o.1).min_by(f64::total_cmp) else {
            break;
        };
        let tie = least * (1. + 1e-6) + 1e-12;
        let Some(&(i, error, _)) = options
            .iter()
            .filter(|o| o.1 <= tie)
            .min_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)))
        else {
            break;
        };
        let previous = (i + m - 1) % m;
        spans[previous] = merged(&spans[previous], &spans[i]);
        spans.remove(i);
        starts.remove(i);
        worst = worst.max(error);
        errors.push(worst);
    }
    (spans, starts, errors)
}

fn outline_spans(polygon: &PolygonPattern) -> Result<Vec<Span>, String> {
    let n = polygon.points.len();
    let dirs = tangents(polygon);
    (0..n)
        .map(|k| {
            Ok(Span {
                edges: vec![k],
                samples: polygon.edge_points(k)?,
                start_dir: dirs[k].1,
                end_dir: dirs[(k + 1) % n].0,
            })
        })
        .collect()
}

/// Keep `ratio` of the removable curve points of the outline, refitting merged spans.
pub fn optimize_outline(
    polygon: &mut PolygonPattern,
    ratio: f64,
    keep: impl Fn(&str) -> bool,
) -> Result<(), String> {
    let candidates = removable(polygon, &keep);
    let remove = candidates.len() - (candidates.len() as f64 * ratio).round() as usize;
    if remove == 0 {
        return Ok(());
    }
    let (spans, _, _) = reduce(outline_spans(polygon)?, true, &candidates, remove, 3);
    let mut points = vec![];
    let mut edges = vec![];
    let mut curves = BTreeMap::new();
    for span in &spans {
        let first = span.edges[0];
        let name = polygon.edges[first].clone();
        points.push(polygon.points[first]);
        if span.edges.len() == 1 {
            if let Some(c) = polygon.curves.get(&name) {
                curves.insert(name.clone(), *c);
            }
        } else {
            let (c, _) = fit_cubic(&span.samples, span.start_dir, span.end_dir);
            curves.insert(name.clone(), [c[1], c[2]]);
        }
        edges.push(name);
    }
    polygon.points = points;
    polygon.edges = edges;
    polygon.curves = curves;
    Ok(())
}

/// The smallest ratio whose refitted outline stays within `tolerance_m` of the original,
/// like an automatic suggestion; 1 when there is nothing to remove.
pub fn suggest_ratio(
    polygon: &PolygonPattern,
    keep: impl Fn(&str) -> bool,
    tolerance_m: f64,
) -> Result<f64, String> {
    let candidates = removable(polygon, &keep);
    if candidates.is_empty() {
        return Ok(1.);
    }
    let (_, _, errors) = reduce(
        outline_spans(polygon)?,
        true,
        &candidates,
        candidates.len(),
        3,
    );
    let removed = errors.iter().take_while(|e| **e <= tolerance_m).count();
    Ok((candidates.len() - removed) as f64 / candidates.len() as f64)
}

/// Keep `ratio` of the curve anchors (those with tangents) of an internal line. Merged spans
/// are refitted to the old line; a kept anchor takes the mean of the handle lengths its two
/// spans want, since internal line tangents are symmetric. Sharp anchors stay sharp.
pub fn optimize_line(line: &mut InternalLine, ratio: f64) -> Result<(), String> {
    let n = line.points.len();
    let candidates: Vec<usize> = (0..n)
        .filter(|&i| line.tangents[i] != DVec2::ZERO && (line.closed || (i > 0 && i < n - 1)))
        .collect();
    let remove = candidates.len() - (candidates.len() as f64 * ratio).round() as usize;
    if remove == 0 {
        return Ok(());
    }
    let spans_count = if line.closed { n } else { n - 1 };
    let spans: Vec<Span> = (0..spans_count)
        .map(|i| {
            let j = (i + 1) % n;
            let (a, b) = (line.points[i], line.points[j]);
            let control = [a, a + line.tangents[i], b - line.tangents[j], b];
            Ok(Span {
                edges: vec![i],
                samples: super::curves::flatten(control, super::curves::FLATNESS_M)?,
                start_dir: line.tangents[i],
                end_dir: line.tangents[j],
            })
        })
        .collect::<Result<_, String>>()?;
    let minimum = if line.closed { 3 } else { 1 };
    let (spans, starts, _) = reduce(spans, line.closed, &candidates, remove, minimum);
    // Handle lengths each kept anchor's spans ask for.
    let m = spans.len();
    let mut out_len = vec![0.; m];
    let mut in_len = vec![0.; m];
    for (k, span) in spans.iter().enumerate() {
        let end = (k + 1) % m;
        let (c, _) = fit_cubic(&span.samples, span.start_dir, span.end_dir);
        out_len[k] = c[1].distance(c[0]);
        in_len[end] = c[3].distance(c[2]);
    }
    let mut points: Vec<DVec2> = starts.iter().map(|&i| line.points[i]).collect();
    let mut tangents: Vec<DVec2> = starts
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let t = line.tangents[i];
            if t == DVec2::ZERO {
                return DVec2::ZERO;
            }
            let lengths = if !line.closed && k == 0 {
                out_len[k]
            } else {
                (out_len[k] + in_len[k]) * 0.5
            };
            t.normalize() * lengths
        })
        .collect();
    if !line.closed {
        points.push(line.points[n - 1]);
        tangents.push(line.tangents[n - 1]);
    }
    line.points = points;
    line.tangents = tangents;
    Ok(())
}
