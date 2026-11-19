//! Automatic sewing: pairs of unsewn outline segments on two pieces, as they are laid around
//! the body, that are as long as each other and face each other across a gap the body does
//! not fill. Segments with the same name pair first. A pair whose gap the body passes through
//! (a hem, a neckline, a cuff) is an opening and stays open; pieces laid away from the body
//! do not pair, since their openings cannot be told.
use super::{
    CompiledGarment, GarmentTemplate,
    seam::{BoundaryRef, Side},
};
use crate::avatar::SurfaceMesh;
use glam::DVec3;

/// One proposed seam: `a` sewn to `b`, `b` run backwards when `reverse`.
#[derive(Clone, Debug)]
pub struct AutoSeam {
    pub a: BoundaryRef,
    pub b: BoundaryRef,
    pub reverse: bool,
    /// Mean distance between the two segments as laid out, metres.
    pub gap_m: f64,
    /// Rest lengths of `a` and `b`, metres.
    pub lengths_m: [f64; 2],
}

/// Two segments pair when their lengths differ by no more than this share (or 2 mm).
const LENGTH_TOLERANCE: f64 = 0.02;
/// Farthest apart two segments with the same name may be laid and still pair, metres.
const MAX_GAP_M: f64 = 0.8;
/// Farthest apart two segments with different names may be laid and still pair: they must
/// have been laid next to each other, metres.
const MAX_UNNAMED_GAP_M: f64 = 0.15;
/// How far the two ends' gaps may differ, as a share of the gap: the segments must face each
/// other, not cross.
const SKEW: f64 = 0.35;
/// Points along each pair checked against the body.
const SAMPLES: usize = 9;
/// How far beyond the body's box the middle of a pair's gap may lie, metres.
const BODY_MARGIN_M: f64 = 0.1;

struct Segment {
    panel: String,
    edge: String,
    /// Positions along the segment, from its start.
    points: Vec<DVec3>,
    rest_m: f64,
}

fn boundary(panel: &str, edge: &str) -> BoundaryRef {
    BoundaryRef {
        reversed: false,
        panel: panel.into(),
        side: Side::Top,
        edge: Some(edge.into()),
        range: [0., 1.],
    }
}

/// Seams to add to `template` (compiled as `garment`, its cloth at `x`) around `body`.
pub fn auto_sew(
    template: &GarmentTemplate,
    garment: &CompiledGarment,
    x: &[DVec3],
    body: &SurfaceMesh,
) -> Vec<AutoSeam> {
    let sewn = |panel: &str, edge: &str| {
        template.seams.iter().any(|s| {
            s.references()
                .any(|r| r.panel == panel && r.edge.as_deref() == Some(edge))
        })
    };
    let mut segments = vec![];
    for range in &garment.panels {
        let Some(polygon) = template
            .panels
            .iter()
            .find(|p| p.id == range.id)
            .and_then(|p| p.polygon.as_ref())
        else {
            continue;
        };
        for edge in &polygon.edges {
            let Some(ids) = range.boundaries.get(edge) else {
                continue;
            };
            if ids.len() < 2 || sewn(&range.id, edge) {
                continue;
            }
            let rest_m = ids
                .windows(2)
                .map(|w| garment.mesh.uv[w[0]].distance(garment.mesh.uv[w[1]]))
                .sum();
            segments.push(Segment {
                panel: range.id.clone(),
                edge: edge.clone(),
                points: ids.iter().map(|i| x[*i]).collect(),
                rest_m,
            });
        }
    }
    let sample = |s: &Segment, t: f64| s.points[((s.points.len() - 1) as f64 * t).round() as usize];
    // With a body, only pieces laid around it pair: the gap between them must be in the body's
    // box (a little grown), or openings could not be told from seams.
    let around = (!body.vertices.is_empty()).then(|| {
        let (lo, hi) = body.vertices.iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        (
            lo - DVec3::splat(BODY_MARGIN_M),
            hi + DVec3::splat(BODY_MARGIN_M),
        )
    });
    let mut candidates = vec![];
    for (i, a) in segments.iter().enumerate() {
        for b in &segments[i + 1..] {
            if a.panel == b.panel {
                continue;
            }
            let longer = a.rest_m.max(b.rest_m);
            if (a.rest_m - b.rest_m).abs() > (LENGTH_TOLERANCE * longer).max(0.002) {
                continue;
            }
            let (a0, a1) = (sample(a, 0.), sample(a, 1.));
            let (b0, b1) = (sample(b, 0.), sample(b, 1.));
            let along = (a0.distance(b0), a1.distance(b1));
            let across = (a0.distance(b1), a1.distance(b0));
            let reverse = across.0 + across.1 < along.0 + along.1;
            let (g0, g1) = if reverse { across } else { along };
            let gap = 0.5 * (g0 + g1);
            let same_name = a.edge == b.edge;
            if !same_name && gap > MAX_UNNAMED_GAP_M {
                continue;
            }
            let middle = (sample(a, 0.5) + sample(b, 0.5)) * 0.5;
            if around.is_some_and(|(lo, hi)| middle.cmplt(lo).any() || middle.cmpgt(hi).any()) {
                continue;
            }
            if gap > MAX_GAP_M || (g0 - g1).abs() > SKEW * gap.max(0.02) {
                continue;
            }
            // The body passes between them: an opening, not a seam.
            let crosses_body = (0..SAMPLES).any(|k| {
                let t = k as f64 / (SAMPLES - 1) as f64;
                let p = sample(a, t);
                let q = sample(b, if reverse { 1. - t } else { t });
                let length = p.distance(q);
                if length < 1e-6 {
                    return false;
                }
                let direction = (q - p) / length;
                body.triangles.iter().any(|tri| {
                    crate::picking::ray_triangle(p, direction, tri.map(|v| body.vertices[v]))
                        .is_some_and(|(d, _)| d > 0. && d < length)
                })
            });
            if crosses_body {
                continue;
            }
            candidates.push((
                (!same_name, gap),
                AutoSeam {
                    a: boundary(&a.panel, &a.edge),
                    b: boundary(&b.panel, &b.edge),
                    reverse,
                    gap_m: gap,
                    lengths_m: [a.rest_m, b.rest_m],
                },
            ));
        }
    }
    // Same names first, then the closest pairs; each segment is sewn once.
    candidates.sort_by(|(x, _), (y, _)| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
    let mut used: Vec<(String, String)> = vec![];
    let mut out = vec![];
    for (_, seam) in candidates {
        let key = |r: &BoundaryRef| (r.panel.clone(), r.edge.clone().unwrap_or_default());
        if used.contains(&key(&seam.a)) || used.contains(&key(&seam.b)) {
            continue;
        }
        used.push(key(&seam.a));
        used.push(key(&seam.b));
        out.push(seam);
    }
    out
}
