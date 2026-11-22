//! Garment panels to apparel-DXF pieces and back.
//!
//! Material coordinates are (u, v) with v drawn downward; DXF is +Y up, so a piece is
//! written at (u, -v) and read back the same way. The material outline is the sew line;
//! the cut line is the outline grown by the stored seam allowances. Every outline point of
//! a panel is written as a turn point, so reading the file splits the sew line into the same
//! boundaries again; a run of curve points between two turn points comes back as one cubic
//! when a cubic fits it, and as its straight segments otherwise.
use super::{Line, NotchMark, NotchShape, PatternFile, Piece, Quantity, Ring};
use crate::{
    authoring::{self, Edit},
    garment::{
        GarmentTemplate, curvefit, curves,
        domain::{Hole, InternalLine},
        markings::{self, MAX_ALLOWANCE_M, MAX_NOTCH_LENGTH_M, Notch, NotchKind},
        meshing::MeshingSettings,
        polygon::PolygonPattern,
        template::{Panel, Placement},
    },
    model::Pattern,
};
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, BTreeSet};

/// Per-piece facts the garment does not hold, chosen when exporting.
#[derive(Clone, Debug, Default)]
pub struct PieceOptions {
    pub size: String,
    /// How many of each piece to cut, by panel id; one as drawn when absent.
    pub quantities: BTreeMap<String, Quantity>,
    /// Only these panels, by id; every panel when `None`.
    pub panels: Option<BTreeSet<String>>,
}

/// Material point to DXF point.
fn out(p: DVec2) -> DVec2 {
    DVec2::new(p.x, -p.y)
}

/// A material ring written as a DXF ring: Y flipped, then reversed so it keeps its turn.
fn out_ring(points: &[DVec2], turns: &[bool]) -> Ring {
    let mut points: Vec<DVec2> = points.iter().map(|p| out(*p)).collect();
    let mut turns = turns.to_vec();
    points.reverse();
    turns.reverse();
    Ring { points, turns }
}

/// A panel's sampled outline: its samples, which of them are outline points, and the
/// allowance of the boundary each sample segment belongs to.
struct Outline {
    points: Vec<DVec2>,
    turns: Vec<bool>,
    widths: Vec<f64>,
}

fn outline(polygon: &PolygonPattern) -> Result<Outline, String> {
    let mut points = vec![];
    let mut turns = vec![];
    let mut widths = vec![];
    for i in 0..polygon.points.len() {
        let samples = polygon.edge_points(i)?;
        let width = polygon.allowance_of(&polygon.edges[i]);
        for (j, p) in samples[..samples.len() - 1].iter().enumerate() {
            points.push(*p);
            turns.push(j == 0);
            widths.push(width);
        }
    }
    Ok(Outline {
        points,
        turns,
        widths,
    })
}

/// Which cut-line points stand for an outline point: the cut line has one point per outline
/// sample (a mitre) or two (a bevel), in order. When the cut line cannot be matched that way,
/// its corners (turning over 30 degrees) are its turn points.
fn cut_turns(sew: &[DVec2], turns: &[bool], widths: &[f64], cut: &[DVec2]) -> Vec<bool> {
    let n = sew.len();
    let offset = |i: usize| {
        let (a, b) = (sew[i], sew[(i + 1) % n]);
        let d = (b - a).normalize_or_zero();
        let normal = DVec2::new(d.y, -d.x) * widths[i];
        (a + normal, b + normal)
    };
    let mut flags = Vec::with_capacity(cut.len());
    let mut k = 0;
    for (i, turn) in turns.iter().enumerate().take(n) {
        let (_, pb) = offset((i + n - 1) % n);
        let (ca, _) = offset(i);
        let bevel = k + 1 < cut.len()
            && cut[k].distance(pb) < 1e-9
            && cut[k + 1].distance(ca) < 1e-9
            && pb.distance(ca) > 1e-9;
        let count = if bevel { 2 } else { 1 };
        flags.extend(std::iter::repeat_n(*turn, count));
        k += count;
    }
    if k == cut.len() {
        return flags;
    }
    let m = cut.len();
    (0..m)
        .map(|i| {
            let u = (cut[i] - cut[(i + m - 1) % m]).normalize_or_zero();
            let v = (cut[(i + 1) % m] - cut[i]).normalize_or_zero();
            u.dot(v) < 30f64.to_radians().cos()
        })
        .collect()
}

/// The point at `fraction` of a polyline's length and the unit direction there.
fn along(points: &[DVec2], fraction: f64) -> Option<(DVec2, DVec2)> {
    let total = markings::polyline_length(points);
    let mut left = total * fraction;
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if length <= 1e-12 {
            continue;
        }
        if left <= length {
            let d = (pair[1] - pair[0]) / length;
            return Some((pair[0] + d * left, d));
        }
        left -= length;
    }
    None
}

/// One panel as a piece, metres, DXF orientation.
pub fn piece(
    template: &GarmentTemplate,
    panel: &Panel,
    options: &PieceOptions,
) -> Result<Piece, String> {
    let fail = |e: String| format!("Piece {}: {e}", panel.name);
    let fabric = template
        .fabrics
        .iter()
        .find(|f| f.id == panel.fabric)
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let mut piece = Piece {
        name: panel.name.clone(),
        size: options.size.clone(),
        quantity: options
            .quantities
            .get(&panel.id)
            .copied()
            .unwrap_or_default(),
        material: fabric,
        ..Default::default()
    };
    let Some(polygon) = &panel.polygon else {
        let (w, h) = (panel.pattern.width, panel.pattern.height);
        let points = [
            DVec2::ZERO,
            DVec2::new(w, 0.),
            DVec2::new(w, h),
            DVec2::new(0., h),
        ];
        piece.cut = out_ring(&points, &[true; 4]);
        piece.grain = Some(grain_line(&points, panel.grain_degrees));
        return Ok(piece);
    };
    let Outline {
        points: sew,
        turns,
        widths,
    } = outline(polygon).map_err(fail)?;
    let contour = polygon.cutting_contour().map_err(fail)?;
    let mut cutouts = vec![];
    let mut cutout_sews = vec![];
    match &contour {
        Some(contour) => {
            let flags = cut_turns(&sew, &turns, &widths, &contour.outer);
            piece.cut = out_ring(&contour.outer, &flags);
            piece.sew = Some(out_ring(&sew, &turns));
            for (hole, cut) in polygon.holes.iter().zip(&contour.holes) {
                let all = vec![true; hole.points.len()];
                if cut == &hole.points {
                    cutouts.push(out_ring(&hole.points, &all));
                } else {
                    let widths: Vec<f64> =
                        hole.edges.iter().map(|e| polygon.allowance_of(e)).collect();
                    let flags = cut_turns(&hole.points, &all, &widths, cut);
                    cutouts.push(out_ring(cut, &flags));
                    cutout_sews.push(out_ring(&hole.points, &all));
                }
            }
        }
        None => {
            piece.cut = out_ring(&sew, &turns);
            for hole in &polygon.holes {
                cutouts.push(out_ring(&hole.points, &vec![true; hole.points.len()]));
            }
        }
    }
    piece.cutouts = cutouts;
    piece.cutout_sews = cutout_sews;
    for line in &polygon.lines {
        let mut samples = line.samples().map_err(fail)?;
        if line.closed {
            samples.pop();
        }
        let turns = samples
            .iter()
            .map(|p| line.points.iter().any(|a| a.distance(*p) < 1e-12))
            .collect();
        piece.internal.push(Line {
            points: samples.into_iter().map(out).collect(),
            turns,
            closed: line.closed,
        });
    }
    for (edge, list) in &polygon.notches {
        let Some(boundary) = polygon.closed_boundary(edge) else {
            continue;
        };
        let width = polygon.allowance_of(edge);
        for notch in list {
            let (at, direction) = along(&boundary, notch.at)
                .ok_or_else(|| fail(format!("a notch on {edge} cannot be placed")))?;
            let outward = DVec2::new(direction.y, -direction.x);
            let base = at + outward * width;
            // A V notch opens as wide as it is deep, like FashionCraft draws it.
            let (shape, width) = match notch.kind {
                NotchKind::Slit => (NotchShape::Slit, 0.),
                NotchKind::V => (NotchShape::V, notch.length_m),
            };
            piece.notches.push(NotchMark {
                base: out(base),
                tip: out(base - outward * notch.length_m),
                width,
                shape,
            });
        }
    }
    let material = polygon.outer_points().map_err(fail)?;
    piece.grain = Some(grain_line(&material, panel.grain_degrees));
    Ok(piece)
}

/// A grain line through the middle of the outline, 60 % of its extent along the grain.
fn grain_line(outline: &[DVec2], degrees: f64) -> [DVec2; 2] {
    let angle = degrees.to_radians();
    let d = DVec2::new(angle.cos(), angle.sin());
    let (lo, hi) = outline.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let centre = (lo + hi) * 0.5;
    let (near, far) = outline
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            let s = (*p - centre).dot(d);
            (a.min(s), b.max(s))
        });
    let half = (far - near) * 0.3;
    [out(centre - d * half), out(centre + d * half)]
}

/// Every panel (or the chosen ones) as pieces.
pub fn pieces(template: &GarmentTemplate, options: &PieceOptions) -> Result<Vec<Piece>, String> {
    template.compile()?;
    let mut list: Vec<Piece> = template
        .panels
        .iter()
        .filter(|p| {
            options
                .panels
                .as_ref()
                .is_none_or(|ids| ids.contains(&p.id))
        })
        .map(|panel| piece(template, panel, options))
        .collect::<Result<_, _>>()?;
    if list.is_empty() {
        return Err("There are no pieces to export".into());
    }
    // A piece name is the piece's unique name in the file.
    let mut taken = BTreeSet::new();
    for piece in &mut list {
        let base = if piece.name.trim().is_empty() {
            "Piece".to_string()
        } else {
            piece.name.trim().to_string()
        };
        let mut name = base.clone();
        let mut serial = 2;
        while !taken.insert(super::write::ascii(&name).to_uppercase()) {
            name = format!("{base} {serial}");
            serial += 1;
        }
        piece.name = name;
    }
    Ok(list)
}

/// A file name stem from a style name: ASCII letters and digits joined by hyphens.
pub fn file_stem(name: &str) -> String {
    let words: Vec<String> = super::write::ascii(name)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        "patterns".into()
    } else {
        words.join("-").chars().take(60).collect()
    }
}

/// `Author:` for files FashionCraft writes: vendor, application and release.
pub fn author() -> String {
    format!("FashionCraft;FashionCraft;{}", env!("CARGO_PKG_VERSION"))
}

/// The date and time now, UTC, as the Style System Text writes them: `dd-mm-yyyy`, `hh:mm`.
pub fn now_utc() -> (String, String) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Civil date from a day count (Howard Hinnant's days_from_civil, inverted).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        format!("{day:02}-{month:02}-{year:04}"),
        format!("{:02}:{:02}", rest / 3600, rest % 3600 / 60),
    )
}

/// A piece as panel geometry, with what did not come across.
#[derive(Clone, Debug)]
pub struct Converted {
    pub polygon: PolygonPattern,
    pub grain_degrees: f64,
    pub size: DVec2,
    pub notes: Vec<String>,
}

/// Material point from a DXF point, with the piece's top-left at the origin.
struct Frame {
    lo: DVec2,
    hi: DVec2,
}
impl Frame {
    fn map(&self, p: DVec2) -> DVec2 {
        DVec2::new(p.x - self.lo.x, self.hi.y - p.y)
    }
}

fn area(points: &[DVec2]) -> f64 {
    crate::garment::domain::area(points)
}

/// The ring in material coordinates, turning the way asked (counter-clockwise for an
/// outline, clockwise for a hole), with its turn flags kept in step.
fn material_ring(ring: &Ring, frame: &Frame, ccw: bool) -> (Vec<DVec2>, Vec<bool>) {
    let mut points: Vec<DVec2> = ring.points.iter().map(|p| frame.map(*p)).collect();
    let mut turns = ring.turns.clone();
    if (area(&points) > 0.) != ccw {
        points.reverse();
        turns.reverse();
    }
    // Start on a turn point so no boundary wraps round the start.
    if let Some(first) = turns.iter().position(|t| *t) {
        points.rotate_left(first);
        turns.rotate_left(first);
    }
    (points, turns)
}

/// Turn points to split on: the ring's own, or with fewer than three, four points a quarter
/// of the way round each from the other.
fn split_points(points: &[DVec2], turns: &[bool]) -> Vec<usize> {
    let marked: Vec<usize> = (0..points.len()).filter(|i| turns[*i]).collect();
    if marked.len() >= 3 {
        return marked;
    }
    let n = points.len();
    let mut lengths = vec![0.];
    for i in 0..n {
        lengths.push(lengths[i] + points[i].distance(points[(i + 1) % n]));
    }
    let total = lengths[n];
    let mut out: Vec<usize> = (0..4)
        .map(|k| {
            let target = total * k as f64 / 4.;
            (0..n)
                .min_by(|a, b| {
                    (lengths[*a] - target)
                        .abs()
                        .total_cmp(&(lengths[*b] - target).abs())
                })
                .unwrap()
        })
        .collect();
    out.dedup();
    if out.len() < 3 {
        return (0..n).collect();
    }
    out
}

/// The tangent at the start of a run of samples: from the parabola through its first three.
fn start_tangent(s: &[DVec2]) -> DVec2 {
    if s.len() < 3 {
        return s[1] - s[0];
    }
    let (t1, t2) = (
        s[0].distance(s[1]),
        s[0].distance(s[1]) + s[1].distance(s[2]),
    );
    if t1 <= 1e-12 || t2 - t1 <= 1e-12 {
        return s[1] - s[0];
    }
    s[0] * (-(t1 + t2) / (t1 * t2)) + s[1] * (t2 / (t1 * (t2 - t1)))
        - s[2] * (t1 / (t2 * (t2 - t1)))
}

/// Largest sample deviation and length difference a refitted cubic may have, metres.
const FIT_ERROR_M: f64 = 0.0001;
const FIT_LENGTH_M: f64 = 0.00002;

/// One boundary rebuilt from the file: where it starts, its cubic handles if it is a curve,
/// and the file's own points along it.
struct Boundary {
    start: DVec2,
    curve: Option<[DVec2; 2]>,
    run: Vec<DVec2>,
}

/// The outline as boundaries: each run between split points is a straight boundary, one
/// cubic, or, when no cubic fits it closely, its own segments.
fn boundaries(points: &[DVec2], splits: &[usize]) -> Vec<Boundary> {
    let n = points.len();
    let mut out = vec![];
    for (k, &start) in splits.iter().enumerate() {
        let end = splits[(k + 1) % splits.len()];
        let count = (end + n - start) % n;
        let count = if count == 0 { n } else { count };
        let run: Vec<DVec2> = (0..=count).map(|j| points[(start + j) % n]).collect();
        if run.len() == 2 {
            out.push(Boundary {
                start: run[0],
                curve: None,
                run,
            });
            continue;
        }
        let mut back: Vec<DVec2> = run.clone();
        back.reverse();
        let (c, error) = curvefit::fit_cubic(&run, start_tangent(&run), -start_tangent(&back));
        let fitted = curves::flatten(c, curves::FLATNESS_M)
            .map(|f| markings::polyline_length(&f))
            .unwrap_or(f64::INFINITY);
        if error <= FIT_ERROR_M && (fitted - markings::polyline_length(&run)).abs() <= FIT_LENGTH_M
        {
            out.push(Boundary {
                start: run[0],
                curve: Some([c[1], c[2]]),
                run,
            });
        } else {
            for w in run.windows(2) {
                out.push(Boundary {
                    start: w[0],
                    curve: None,
                    run: w.to_vec(),
                });
            }
        }
    }
    out
}

/// First crossing of the ray `from + t·direction` (t ≥ 0) with any ring, its distance.
fn ray_hit(from: DVec2, direction: DVec2, rings: &[&[DVec2]]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for ring in rings {
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let e = b - a;
            let det = direction.perp_dot(e);
            if det.abs() < 1e-15 {
                continue;
            }
            let t = (a - from).perp_dot(e) / det;
            let u = (a - from).perp_dot(direction) / det;
            if t >= -1e-9 && (-1e-9..=1. + 1e-9).contains(&u) {
                best = Some(best.map_or(t.max(0.), |b: f64| b.min(t.max(0.))));
            }
        }
    }
    best
}

/// The allowance of each boundary, read off the cut rings: the median distance from the
/// midpoints of the file's segments along it, along their outward normals. The file's cut
/// line was drawn round those very segments, so a refitted curve does not shift it.
fn allowances(runs: &[&[DVec2]], cut: &[&[DVec2]]) -> Vec<f64> {
    runs.iter()
        .map(|samples| {
            let mut found: Vec<f64> = samples
                .windows(2)
                .filter_map(|w| {
                    let d = (w[1] - w[0]).normalize_or_zero();
                    let outward = DVec2::new(d.y, -d.x);
                    ray_hit((w[0] + w[1]) * 0.5, outward, cut)
                })
                .collect();
            if found.is_empty() {
                return 0.;
            }
            found.sort_by(f64::total_cmp);
            let median = found[found.len() / 2];
            // Whole hundredths of a millimetre: what a pattern specifies.
            (median * 1e5).round() / 1e5
        })
        .collect()
}

fn distance_to(p: DVec2, ring: &[DVec2]) -> f64 {
    super::read::ring_distance(p, ring)
}

/// One piece as panel geometry: the sew line as outline, allowances from the cut line,
/// notches placed back on their boundaries, the grain from its line.
pub fn convert(piece: &Piece) -> Result<Converted, String> {
    let mut notes = vec![];
    let sew = piece.sew_line();
    let (lo, hi) = sew.points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let frame = Frame { lo, hi };
    let (outline, turns) = material_ring(sew, &frame, true);
    if outline.len() < 3 {
        return Err("The outline has fewer than three points".into());
    }
    let splits = split_points(&outline, &turns);
    let rebuilt = boundaries(&outline, &splits);
    let points: Vec<DVec2> = rebuilt.iter().map(|b| b.start).collect();
    if points.len() > 4096 {
        return Err(format!(
            "The outline has {} points; a piece holds at most 4096",
            points.len()
        ));
    }
    let edges: Vec<String> = (0..points.len())
        .map(|i| format!("edge_{}", i + 1))
        .collect();
    let mut polygon = PolygonPattern {
        edges: edges.clone(),
        points: points.clone(),
        subdivisions: 0,
        meshing: None,
        curves: BTreeMap::new(),
        holes: vec![],
        lines: vec![],
        target_lengths: BTreeMap::new(),
        seam_allowances: BTreeMap::new(),
        notches: BTreeMap::new(),
    };
    for (name, boundary) in edges.iter().zip(&rebuilt) {
        if let Some(c) = boundary.curve {
            polygon.curves.insert(name.clone(), c);
        }
    }

    // Holes: a cutout's sew line when it has one, else its cut line.
    let cut_outer: Vec<DVec2> = piece.cut.points.iter().map(|p| frame.map(*p)).collect();
    let cutouts: Vec<Vec<DVec2>> = piece
        .cutouts
        .iter()
        .map(|r| r.points.iter().map(|p| frame.map(*p)).collect())
        .collect();
    let mut hole_cuts: Vec<Option<usize>> = vec![];
    let mut serial = 0;
    for (i, cutout) in piece.cutouts.iter().enumerate() {
        let sew_hole = piece.cutout_sews.iter().find(|s| {
            super::read::ring_distance(cutout.points[0], &s.points) < 1e-9
                || crate::garment::domain::inside(cutout.points[0], &s.points)
        });
        let ring = sew_hole.unwrap_or(cutout);
        let (hole_points, _) = material_ring(ring, &frame, false);
        let hole_edges = (0..hole_points.len())
            .map(|_| {
                serial += 1;
                format!("hole_edge_{serial}")
            })
            .collect();
        polygon.holes.push(Hole {
            points: hole_points,
            edges: hole_edges,
        });
        hole_cuts.push(sew_hole.map(|_| i));
    }

    // Seam allowances, when there is a sew line apart from the cut line.
    if piece.sew.is_some() {
        let runs: Vec<&[DVec2]> = rebuilt.iter().map(|b| &b.run[..]).collect();
        let widths = allowances(&runs, &[&cut_outer]);
        for (name, width) in edges.iter().zip(widths) {
            if width > MAX_ALLOWANCE_M {
                notes.push(format!(
                    "Seam allowance of {:.1} mm on {name} is over the {:.0} mm limit; left out",
                    width * 1000.,
                    MAX_ALLOWANCE_M * 1000.
                ));
            } else if width >= 0.00001 {
                polygon.seam_allowances.insert(name.clone(), width);
            }
        }
    }
    for (hole, cut) in polygon.holes.clone().iter().zip(&hole_cuts) {
        let Some(i) = cut else { continue };
        let m = hole.points.len();
        let segments: Vec<[DVec2; 2]> = (0..m)
            .map(|k| [hole.points[k], hole.points[(k + 1) % m]])
            .collect();
        let runs: Vec<&[DVec2]> = segments.iter().map(|s| &s[..]).collect();
        let widths = allowances(&runs, &[&cutouts[*i]]);
        for (name, width) in hole.edges.iter().zip(widths) {
            if (0.00001..=MAX_ALLOWANCE_M).contains(&width) {
                polygon.seam_allowances.insert(name.clone(), width);
            }
        }
    }
    if !polygon.seam_allowances.is_empty() {
        match polygon.cutting_contour() {
            Ok(Some(rebuilt)) => {
                let worst = rebuilt
                    .outer
                    .iter()
                    .map(|p| distance_to(*p, &cut_outer))
                    .chain(cut_outer.iter().map(|p| distance_to(*p, &rebuilt.outer)))
                    .fold(0., f64::max);
                if worst > 0.0005 {
                    notes.push(format!(
                        "The cut line is not an even allowance round the sew line: the rebuilt one is up to {:.1} mm from it",
                        worst * 1000.
                    ));
                }
            }
            Ok(None) => {}
            Err(e) => {
                notes.push(format!("Seam allowance left out: {e}"));
                polygon.seam_allowances.clear();
            }
        }
    }

    // Notches: back along the notch line onto the sew line, then onto its boundary.
    let rings: Vec<(String, Vec<DVec2>)> = polygon
        .paths()?
        .into_iter()
        .filter(|(name, _)| polygon.closed_boundary(name).is_some())
        .collect();
    let material_rings: Vec<&[DVec2]> = std::iter::once(&outline[..])
        .chain(polygon.holes.iter().map(|h| &h.points[..]))
        .collect();
    let mut dropped = BTreeMap::<&str, usize>::new();
    for notch in &piece.notches {
        let base = frame.map(notch.base);
        let tip = frame.map(notch.tip);
        let length = base.distance(tip);
        if length <= 1e-6 {
            *dropped.entry("of zero length").or_default() += 1;
            continue;
        }
        let inward = (tip - base) / length;
        let on_sew = if material_rings.iter().any(|r| distance_to(base, r) < 1e-6) {
            Some(base)
        } else {
            ray_hit(base, inward, &material_rings).map(|t| base + inward * t)
        };
        let Some(at) = on_sew else {
            *dropped.entry("that do not reach the sew line").or_default() += 1;
            continue;
        };
        // The boundary it lies on and how far along.
        let best = rings
            .iter()
            .filter_map(|(name, samples)| {
                let mut walked = 0.;
                let mut best: Option<(f64, f64)> = None;
                for w in samples.windows(2) {
                    let d = w[1] - w[0];
                    let len = d.length();
                    let t = ((at - w[0]).dot(d) / (len * len).max(1e-300)).clamp(0., 1.);
                    let gap = at.distance(w[0] + d * t);
                    if best.is_none_or(|(g, _)| gap < g) {
                        best = Some((gap, walked + len * t));
                    }
                    walked += len;
                }
                best.map(|(gap, s)| (gap, name, s / walked.max(1e-300), walked))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((gap, name, fraction, total)) = best else {
            continue;
        };
        if gap > 0.0005 {
            *dropped.entry("that do not reach the sew line").or_default() += 1;
            continue;
        }
        let kind = match notch.shape {
            NotchShape::V => NotchKind::V,
            NotchShape::Slit => NotchKind::Slit,
            other => {
                notes.push(format!("A {} notch came in as a slit", other.label()));
                NotchKind::Slit
            }
        };
        let mark = Notch::new(fraction, length.min(MAX_NOTCH_LENGTH_M), kind);
        if length > MAX_NOTCH_LENGTH_M {
            notes.push(format!(
                "A notch {:.1} mm deep was shortened to {:.0} mm",
                length * 1000.,
                MAX_NOTCH_LENGTH_M * 1000.
            ));
        }
        if markings::validate_notch(&mark, total).is_err() {
            *dropped.entry("too close to a boundary's end").or_default() += 1;
            continue;
        }
        polygon.notches.entry(name.clone()).or_default().push(mark);
    }
    for (why, count) in dropped {
        notes.push(format!("{count} notch(es) {why} left out"));
    }
    for list in polygon.notches.values_mut() {
        list.sort_by(|a, b| a.at.total_cmp(&b.at));
    }

    // Internal lines inside the outline; at most 64 points each.
    for (k, line) in piece.internal.iter().enumerate() {
        let mut points: Vec<DVec2> = line.points.iter().map(|p| frame.map(*p)).collect();
        if points.iter().any(|p| {
            !crate::garment::domain::inside(*p, &outline) && distance_to(*p, &outline) > 1e-6
        }) {
            notes.push(format!(
                "Internal line {} leaves the piece; left out",
                k + 1
            ));
            continue;
        }
        // An internal line holds at most 64 anchor points.
        let deviation = simplify(&mut points, 64);
        if deviation > 0. {
            notes.push(format!(
                "Internal line {} simplified to {} points (within {:.2} mm)",
                k + 1,
                points.len(),
                deviation * 1000.
            ));
        }
        if points.len() < 2 || (line.closed && points.len() < 3) {
            continue;
        }
        polygon.lines.push(InternalLine {
            id: format!("line_{}", k + 1),
            tangents: vec![DVec2::ZERO; points.len()],
            points,
            closed: line.closed,
            group: None,
        });
    }

    let grain_degrees = match piece.grain {
        // A grain line is an axis: the files give it no sense, so it comes back in [0°, 180°).
        Some([a, b]) => {
            let d = b - a;
            let degrees = (-d.y).atan2(d.x).to_degrees().rem_euclid(180.);
            // To a hundredth of a degree: about what rounded coordinates carry.
            let degrees = (degrees * 100.).round() / 100.;
            if degrees >= 180. || degrees == 0. {
                0.
            } else {
                degrees
            }
        }
        None => {
            notes.push("No grain line; the grain is set to 90°".into());
            90.
        }
    };
    if !piece.drill_holes.is_empty() {
        notes.push(format!(
            "{} drill hole(s) not imported",
            piece.drill_holes.len()
        ));
    }
    if !piece.annotations.is_empty() {
        notes.push(format!(
            "Texts not imported: {}",
            piece.annotations.join(" · ")
        ));
    }
    Ok(Converted {
        polygon,
        grain_degrees,
        size: hi - lo,
        notes,
    })
}

/// Keep at most `limit` points of a path, dropping the ones whose loss bends it least
/// (Douglas–Peucker with a growing tolerance); returns the largest deviation allowed.
fn simplify(points: &mut Vec<DVec2>, limit: usize) -> f64 {
    if points.len() <= limit {
        return 0.;
    }
    fn keep(points: &[DVec2], tolerance: f64, out: &mut Vec<bool>, from: usize, to: usize) {
        let (a, b) = (points[from], points[to]);
        let mut worst = (0., from);
        for (i, p) in points.iter().enumerate().take(to).skip(from + 1) {
            let d = super::read::segment_distance(*p, a, b);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tolerance {
            out[worst.1] = true;
            keep(points, tolerance, out, from, worst.1);
            keep(points, tolerance, out, worst.1, to);
        }
    }
    let mut tolerance = 0.00005;
    loop {
        let mut flags = vec![false; points.len()];
        flags[0] = true;
        *flags.last_mut().unwrap() = true;
        keep(points, tolerance, &mut flags, 0, points.len() - 1);
        if flags.iter().filter(|f| **f).count() <= limit {
            let kept = points
                .iter()
                .zip(&flags)
                .filter(|(_, f)| **f)
                .map(|(p, _)| *p)
                .collect();
            *points = kept;
            return tolerance;
        }
        tolerance *= 1.5;
    }
}

/// Add the chosen pieces as new panels, laid out in a row like the other importers do.
pub fn prepare(
    source: &GarmentTemplate,
    file: &PatternFile,
    selected: &[usize],
    fabric: &str,
    meshing: MeshingSettings,
    origin: DVec3,
) -> Result<Edit, String> {
    meshing.validate()?;
    if !origin.is_finite() {
        return Err("Import placement must be finite".into());
    }
    if selected.is_empty() || source.panels.len() + selected.len() > 128 {
        return Err("Choose at least one piece; the garment supports at most 128 panels".into());
    }
    if !source.fabrics.iter().any(|f| f.id == fabric) {
        return Err("Choose an existing fabric".into());
    }
    let mut template = source.clone();
    let board_x = source
        .panels
        .iter()
        .enumerate()
        .map(|(i, p)| authoring::layout(source, i).x + p.pattern.width)
        .fold(0., f64::max)
        + 0.15;
    let mut x = 0.;
    let mut seen = BTreeSet::new();
    let mut selected_id = None;
    for &index in selected {
        if !seen.insert(index) {
            return Err("A piece was selected twice".into());
        }
        let piece = file.pieces.get(index).ok_or("Stale piece selection")?;
        let converted = convert(piece).map_err(|e| format!("{}: {e}", piece.name))?;
        let mut polygon = converted.polygon;
        polygon.meshing = Some(meshing.clone());
        let mut serial = 1;
        let id = loop {
            let id = format!("dxf_{serial}");
            if !template.panels.iter().any(|p| p.id == id) {
                break id;
            }
            serial += 1;
        };
        let name: String = if piece.name.trim().is_empty() {
            id.clone()
        } else {
            piece.name.chars().take(80).collect()
        };
        template.panels.push(Panel {
            id: id.clone(),
            name,
            pattern: Pattern {
                width: converted.size.x,
                height: converted.size.y,
                nx: 1,
                ny: 1,
            },
            polygon: Some(polygon),
            fabric: fabric.into(),
            grain_degrees: converted.grain_degrees,
            placement: Placement {
                translation: origin + DVec3::new(x, 0., 0.),
                ..Default::default()
            },
            layout_2d: Some(DVec2::new(board_x + x, 0.)),
            layout_rotation_degrees: 0.,
            pins: vec![],
            pin_points: vec![],
        });
        x += converted.size.x + 0.1;
        selected_id = Some(id);
    }
    template.version = 2;
    let garment = template.compile()?;
    Ok(Edit {
        template,
        garment,
        selected: selected_id,
        reset_simulation: true,
        new_point: None,
    })
}
