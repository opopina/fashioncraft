//! Nonphysical garment construction metadata in panel-local metres.
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// A stable document identity and its owning panel. Paths contain material
/// coordinates rather than mesh vertices or transient boundary indices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trim {
    pub id: String,
    pub panel_id: String,
    pub kind: TrimKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrimKind {
    Button {
        center_m: DVec2,
        diameter_m: f64,
    },
    Buttonhole {
        start_m: DVec2,
        end_m: DVec2,
        width_m: f64,
    },
    Zipper {
        path_m: Vec<DVec2>,
        width_m: f64,
    },
    Topstitch {
        path_m: Vec<DVec2>,
        rows: u8,
        spacing_m: f64,
    },
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn point(p: DVec2) -> bool {
    p.is_finite() && p.abs().max_element() <= 1e6
}
fn size(v: f64) -> bool {
    v.is_finite() && (1e-6..=1e3).contains(&v)
}
fn path(points: &[DVec2]) -> bool {
    (2..=1024).contains(&points.len())
        && points.iter().copied().all(point)
        && points.windows(2).all(|p| p[0].distance(p[1]) >= 1e-6)
}

impl Trim {
    /// Checks representation only; document validation resolves the panel ID.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) || !valid_id(&self.panel_id) {
            return Err(
                "Trim and panel IDs must use 1–80 ASCII letters, digits, '_' or '-'".into(),
            );
        }
        let valid = match &self.kind {
            TrimKind::Button {
                center_m,
                diameter_m,
            } => point(*center_m) && size(*diameter_m),
            TrimKind::Buttonhole {
                start_m,
                end_m,
                width_m,
            } => path(&[*start_m, *end_m]) && size(*width_m),
            TrimKind::Zipper { path_m, width_m } => path(path_m) && size(*width_m),
            TrimKind::Topstitch {
                path_m,
                rows,
                spacing_m,
            } => path(path_m) && (1..=8).contains(rows) && size(*spacing_m),
        };
        if !valid {
            return Err("Invalid trim geometry or dimensions".into());
        }
        Ok(())
    }
}

/// One coloured triangle of a trim's 3D look, linear RGB.
#[derive(Clone, Debug, PartialEq)]
pub struct TrimTriangle {
    pub points: [glam::DVec3; 3],
    pub color: [f32; 3],
}

/// How far trims sit off the cloth's front face, so they draw over it, metres.
pub const TRIM_LIFT_M: f64 = 0.0015;
const BUTTON_COLOR: [f32; 3] = [0.72, 0.66, 0.55];
const HOLE_COLOR: [f32; 3] = [0.12, 0.11, 0.1];
const THREAD_COLOR: [f32; 3] = [0.92, 0.84, 0.62];
const TAPE_COLOR: [f32; 3] = [0.05, 0.05, 0.06];
const TEETH_COLOR: [f32; 3] = [0.62, 0.62, 0.64];

/// A point of the cloth at material coordinates `uv` of `panel`, now: where it is and the
/// unit normal of the front face there (away from the faces' winding normal).
fn surface_point(
    garment: &super::CompiledGarment,
    x: &[glam::DVec3],
    panel: &str,
    uv: DVec2,
) -> Option<(glam::DVec3, glam::DVec3)> {
    let terms = garment.material_point(panel, uv)?;
    let p: glam::DVec3 = terms.iter().map(|(v, w)| x[*v] * *w).sum();
    let vertices: Vec<usize> = terms.iter().map(|(v, _)| *v).collect();
    let t = garment
        .mesh
        .triangles
        .iter()
        .find(|t| vertices.iter().all(|v| t.vertices.contains(v)))?;
    let [a, b, c] = t.vertices.map(|i| x[i]);
    let n = (b - a).cross(c - a).normalize_or_zero();
    (n != glam::DVec3::ZERO).then_some((p, -n))
}

/// What the zippers on seams draw, as trims of the pieces they lie on: a tape with teeth along
/// each side of the seam (half the zipper's width each, so a closed zipper shows one tape) and
/// the slider where the closed share ends, along the seam's first side.
pub fn zipper_trims(template: &super::GarmentTemplate) -> Vec<Trim> {
    // The material path of one sewn boundary interval, walked in its seam direction.
    let path_of = |r: &super::seam::BoundaryRef| -> Option<Vec<DVec2>> {
        let polygon = template
            .panels
            .iter()
            .find(|p| p.id == r.panel)?
            .polygon
            .as_ref()?;
        let points = polygon.named_boundary(r.edge.as_deref()?)?;
        let mut path = sub_path(&points, r.range[0], r.range[1]);
        if r.reversed {
            path.reverse();
        }
        (path.len() >= 2).then_some(path)
    };
    let mut out = vec![];
    for (seam, zipper) in &template.seam_zippers {
        let Some(spec) = template.seams.iter().find(|s| &s.id == seam) else {
            continue;
        };
        let mut first_side = vec![];
        for (side, refs) in [("a", spec.side(false)), ("b", spec.side(true))] {
            for (k, r) in refs.enumerate() {
                let Some(path) = path_of(r) else {
                    continue;
                };
                out.push(Trim {
                    id: format!("zipper-{seam}-{side}{k}"),
                    panel_id: r.panel.clone(),
                    kind: TrimKind::Zipper {
                        path_m: path.clone(),
                        width_m: 0.5 * zipper.width_m,
                    },
                });
                if side == "a" {
                    first_side.push((r.panel.clone(), path));
                }
            }
        }
        // The slider sits where the closed share ends along the first side.
        let total: f64 = first_side.iter().map(|(_, p)| length(p)).sum();
        let mut left = zipper.closed * total;
        for (panel, path) in &first_side {
            let here = length(path);
            if left <= here + 1e-12 {
                if let Some(centre) = point_at(path, left) {
                    out.push(Trim {
                        id: format!("zipper-{seam}-slider"),
                        panel_id: panel.clone(),
                        kind: TrimKind::Button {
                            center_m: centre,
                            diameter_m: 1.4 * zipper.width_m,
                        },
                    });
                }
                break;
            }
            left -= here;
        }
    }
    out
}

fn length(path: &[DVec2]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The point `at` metres along `path`.
fn point_at(path: &[DVec2], at: f64) -> Option<DVec2> {
    let mut left = at.max(0.);
    for w in path.windows(2) {
        let d = w[0].distance(w[1]);
        if left <= d {
            return Some(w[0].lerp(w[1], if d > 0. { left / d } else { 0. }));
        }
        left -= d;
    }
    path.last().copied()
}

/// The part of `path` between the shares `from` and `to` of its length.
fn sub_path(path: &[DVec2], from: f64, to: f64) -> Vec<DVec2> {
    let total = length(path);
    let (a, b) = (from * total, to * total);
    let mut out = vec![];
    if let Some(p) = point_at(path, a) {
        out.push(p);
    }
    let mut walked = 0.;
    for w in path.windows(2) {
        walked += w[0].distance(w[1]);
        if walked > a + 1e-12 && walked < b - 1e-12 {
            out.push(w[1]);
        }
    }
    if let Some(p) = point_at(path, b) {
        out.push(p);
    }
    out.dedup_by(|p, q| p.distance(*q) < 1e-9);
    out
}

/// The 3D look of `trims` on the cloth at positions `x`: button discs with their holes,
/// stitched buttonholes, zipper tape and teeth, and topstitch dashes, all lying on the front
/// face. Trims on pieces that are not in `garment` are skipped.
pub fn trim_triangles(
    trims: &[Trim],
    garment: &super::CompiledGarment,
    x: &[glam::DVec3],
) -> Vec<TrimTriangle> {
    use glam::DVec3;
    let mut out = vec![];
    let mut tri = |points: [DVec3; 3], color: [f32; 3]| out.push(TrimTriangle { points, color });
    // A flat disc of `radius` facing along `n` at `centre`.
    let disc = |tri: &mut dyn FnMut([DVec3; 3], [f32; 3]),
                centre: DVec3,
                n: DVec3,
                radius: f64,
                color: [f32; 3]| {
        let u = n.any_orthonormal_vector();
        let v = n.cross(u);
        let k = 16;
        for i in 0..k {
            let (a0, a1) = (
                std::f64::consts::TAU * i as f64 / k as f64,
                std::f64::consts::TAU * (i + 1) as f64 / k as f64,
            );
            let p0 = centre + (u * a0.cos() + v * a0.sin()) * radius;
            let p1 = centre + (u * a1.cos() + v * a1.sin()) * radius;
            tri([centre, p0, p1], color);
        }
    };
    // A flat band of `width` from `a` to `b`, lying on the surface with normal `n`.
    let band = |tri: &mut dyn FnMut([DVec3; 3], [f32; 3]),
                a: DVec3,
                b: DVec3,
                n: DVec3,
                width: f64,
                color: [f32; 3]| {
        let side = (b - a).cross(n).normalize_or_zero() * (0.5 * width);
        tri([a - side, a + side, b + side], color);
        tri([a - side, b + side, b - side], color);
    };
    // Points along a material polyline, every `step` metres, with their surface normals.
    let walk = |panel: &str, path: &[DVec2], step: f64| {
        let mut samples = vec![];
        for w in path.windows(2) {
            let n = ((w[1] - w[0]).length() / step).ceil().max(1.) as usize;
            for k in 0..n {
                let uv = w[0].lerp(w[1], k as f64 / n as f64);
                if let Some(s) = surface_point(garment, x, panel, uv) {
                    samples.push(s);
                }
            }
        }
        if let Some(s) = path
            .last()
            .and_then(|p| surface_point(garment, x, panel, *p))
        {
            samples.push(s);
        }
        samples
    };
    for trim in trims {
        if !garment.panels.iter().any(|r| r.id == trim.panel_id) {
            continue;
        }
        let panel = trim.panel_id.as_str();
        match &trim.kind {
            TrimKind::Button {
                center_m,
                diameter_m,
            } => {
                let Some((p, n)) = surface_point(garment, x, panel, *center_m) else {
                    continue;
                };
                let top = p + n * (TRIM_LIFT_M + 0.001);
                disc(&mut tri, top, n, 0.5 * diameter_m, BUTTON_COLOR);
                // Four holes, a little above the face so they draw on it.
                let u = n.any_orthonormal_vector() * (0.15 * diameter_m);
                let v = n.cross(u);
                for h in [u, -u, v, -v] {
                    disc(
                        &mut tri,
                        top + h + n * 1e-4,
                        n,
                        0.06 * diameter_m,
                        HOLE_COLOR,
                    );
                }
            }
            TrimKind::Buttonhole {
                start_m,
                end_m,
                width_m,
            } => {
                let (Some((a, na)), Some((b, nb))) = (
                    surface_point(garment, x, panel, *start_m),
                    surface_point(garment, x, panel, *end_m),
                ) else {
                    continue;
                };
                let n = (na + nb).normalize_or_zero();
                let lift = n * TRIM_LIFT_M;
                let side = (b - a).cross(n).normalize_or_zero() * (0.5 * width_m);
                // Two stitched lips and the slit between them.
                band(
                    &mut tri,
                    a + lift + side,
                    b + lift + side,
                    n,
                    0.4 * width_m,
                    THREAD_COLOR,
                );
                band(
                    &mut tri,
                    a + lift - side,
                    b + lift - side,
                    n,
                    0.4 * width_m,
                    THREAD_COLOR,
                );
                band(&mut tri, a + lift, b + lift, n, 0.25 * width_m, HOLE_COLOR);
            }
            TrimKind::Zipper { path_m, width_m } => {
                let points = walk(panel, path_m, 0.002);
                for (k, w) in points.windows(2).enumerate() {
                    let n = (w[0].1 + w[1].1).normalize_or_zero();
                    let (a, b) = (w[0].0 + n * TRIM_LIFT_M, w[1].0 + n * TRIM_LIFT_M);
                    band(&mut tri, a, b, n, *width_m, TAPE_COLOR);
                    // Teeth alternate from each side across the middle of the tape.
                    let side = (b - a).cross(n).normalize_or_zero() * (0.25 * width_m);
                    let mid =
                        (a + b) * 0.5 + n * 2e-4 + if k % 2 == 0 { side } else { -side } * 0.4;
                    let along = (b - a) * 0.35;
                    band(
                        &mut tri,
                        mid - along,
                        mid + along,
                        n,
                        0.55 * width_m,
                        TEETH_COLOR,
                    );
                }
            }
            TrimKind::Topstitch {
                path_m,
                rows,
                spacing_m,
            } => {
                let points = walk(panel, path_m, 0.0025);
                for row in 0..*rows {
                    let offset = (row as f64 - (*rows as f64 - 1.) * 0.5) * spacing_m;
                    for (k, w) in points.windows(2).enumerate() {
                        // Dashes: every other step.
                        if k % 2 == 1 {
                            continue;
                        }
                        let n = (w[0].1 + w[1].1).normalize_or_zero();
                        let side = (w[1].0 - w[0].0).cross(n).normalize_or_zero() * offset;
                        let lift = n * TRIM_LIFT_M;
                        band(
                            &mut tri,
                            w[0].0 + lift + side,
                            w[1].0 + lift + side,
                            n,
                            0.0007,
                            THREAD_COLOR,
                        );
                    }
                }
            }
        }
    }
    out
}
