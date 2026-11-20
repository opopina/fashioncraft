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
    /// A round cord along a seam or an edge, drawn as a tube on the cloth's front face.
    Piping {
        path_m: Vec<DVec2>,
        diameter_m: f64,
    },
    /// A rigid 3D object fixed on the cloth (a buckle, a badge, a carved button) opened from a
    /// mesh file. Its mesh is kept in metres in its own frame: X along the piece's material u
    /// axis, Z out of the front face, centred across and with its back at Z = 0. The origin
    /// sits on the front face at `center_m`; `turn_degrees` turns it about the face normal and
    /// `scale` sizes it.
    Object {
        name: String,
        center_m: DVec2,
        #[serde(default)]
        turn_degrees: f64,
        #[serde(default = "unit_scale")]
        scale: f64,
        vertices: Vec<[f32; 3]>,
        triangles: Vec<[u32; 3]>,
    },
}

fn unit_scale() -> f64 {
    1.
}

/// Most triangles an object trim may have.
pub const MAX_OBJECT_TRIANGLES: usize = 100_000;
/// Largest size of an object trim, metres.
pub const MAX_OBJECT_SIZE_M: f64 = 2.;

/// The largest extent of a mesh's vertices.
fn extent(vertices: &[[f32; 3]]) -> f64 {
    let (lo, hi) = vertices.iter().fold(
        (
            glam::DVec3::splat(f64::INFINITY),
            glam::DVec3::splat(f64::NEG_INFINITY),
        ),
        |(lo, hi), v| {
            let v = glam::Vec3::from_array(*v).as_dvec3();
            (lo.min(v), hi.max(v))
        },
    );
    (hi - lo).max_element().max(0.)
}

/// An object trim made from a mesh file's triangles, centred at `center_m` of a piece.
///
/// The thinnest of the mesh's three axes becomes the one out of the cloth (the axes turn
/// cyclically, so the object is not mirrored). The unit is guessed from its size, since mesh
/// files rarely say: larger than 30 units is millimetres, larger than 1 centimetres, otherwise
/// metres. The mesh is centred across and its back laid at Z = 0.
pub fn object_from_mesh(
    name: &str,
    mesh: &crate::avatar::SurfaceMesh,
    center_m: DVec2,
) -> Result<TrimKind, String> {
    use glam::DVec3;
    if mesh.triangles.is_empty() || mesh.triangles.len() > MAX_OBJECT_TRIANGLES {
        return Err(format!(
            "A trim object needs 1 to {MAX_OBJECT_TRIANGLES} triangles; this one has {}",
            mesh.triangles.len()
        ));
    }
    let (lo, hi) = mesh.vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), v| (lo.min(*v), hi.max(*v)),
    );
    let size = hi - lo;
    if !size.is_finite() || size.max_element() <= 0. {
        return Err("The trim object has no size".into());
    }
    let unit = if size.max_element() > 30. {
        0.001
    } else if size.max_element() > 1. {
        0.01
    } else {
        1.
    };
    // Turn the axes so the thinnest points out of the cloth.
    let thin = (0..3)
        .min_by(|a, b| size[*a].total_cmp(&size[*b]))
        .unwrap_or(2);
    let turn = |v: DVec3| match thin {
        0 => DVec3::new(v.y, v.z, v.x),
        1 => DVec3::new(v.z, v.x, v.y),
        _ => v,
    };
    let (lo, hi) = (turn(lo), turn(hi));
    let origin = DVec3::new(0.5 * (lo.x + hi.x), 0.5 * (lo.y + hi.y), lo.z);
    let vertices: Vec<[f32; 3]> = mesh
        .vertices
        .iter()
        .map(|v| ((turn(*v) - origin) * unit).as_vec3().to_array())
        .collect();
    let triangles = mesh.triangles.iter().map(|t| t.map(|i| i as u32)).collect();
    let kind = TrimKind::Object {
        name: name.chars().take(120).collect(),
        center_m,
        turn_degrees: 0.,
        scale: 1.,
        vertices,
        triangles,
    };
    Trim {
        id: "trim".into(),
        panel_id: "panel".into(),
        kind: kind.clone(),
    }
    .validate()?;
    Ok(kind)
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
            TrimKind::Piping { path_m, diameter_m } => path(path_m) && size(*diameter_m),
            TrimKind::Object {
                name,
                center_m,
                turn_degrees,
                scale,
                vertices,
                triangles,
            } => {
                name.chars().count() <= 120
                    && point(*center_m)
                    && turn_degrees.is_finite()
                    && (0.01..=100.).contains(scale)
                    && (3..=3 * MAX_OBJECT_TRIANGLES).contains(&vertices.len())
                    && (1..=MAX_OBJECT_TRIANGLES).contains(&triangles.len())
                    && vertices.iter().flatten().all(|v| v.is_finite())
                    && triangles
                        .iter()
                        .flatten()
                        .all(|i| (*i as usize) < vertices.len())
                    && extent(vertices) * scale <= MAX_OBJECT_SIZE_M
            }
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
const PIPING_COLOR: [f32; 3] = [0.08, 0.1, 0.2];
/// A brushed-brass look for object trims.
const OBJECT_COLOR: [f32; 3] = [0.7, 0.58, 0.34];
/// Sides of a piping tube's cross-section.
const PIPING_SIDES: usize = 8;

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

/// Like `surface_point`, with the direction of the piece's material u axis on the cloth there,
/// at right angles to the normal.
fn surface_frame(
    garment: &super::CompiledGarment,
    x: &[glam::DVec3],
    panel: &str,
    uv: DVec2,
) -> Option<(glam::DVec3, glam::DVec3, glam::DVec3)> {
    let (p, n) = surface_point(garment, x, panel, uv)?;
    let terms = garment.material_point(panel, uv)?;
    let vertices: Vec<usize> = terms.iter().map(|(v, _)| *v).collect();
    let t = garment
        .mesh
        .triangles
        .iter()
        .find(|t| vertices.iter().all(|v| t.vertices.contains(v)))?;
    let [a, b, c] = t.vertices;
    let m = glam::DMat2::from_cols(
        garment.mesh.uv[b] - garment.mesh.uv[a],
        garment.mesh.uv[c] - garment.mesh.uv[a],
    );
    let along = if m.determinant().abs() > 1e-18 {
        let k = m.inverse().col(0);
        (x[b] - x[a]) * k.x + (x[c] - x[a]) * k.y
    } else {
        glam::DVec3::ZERO
    };
    let along = (along - n * along.dot(n)).normalize_or_zero();
    let along = if along == glam::DVec3::ZERO {
        n.any_orthonormal_vector()
    } else {
        along
    };
    Some((p, n, along))
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
            TrimKind::Piping { path_m, diameter_m } => {
                let r = 0.5 * diameter_m;
                let points = walk(panel, path_m, 0.003);
                // One ring of the tube around each sample, its centre a radius off the cloth.
                let rings: Vec<Vec<DVec3>> = (0..points.len())
                    .map(|k| {
                        let (p, n) = points[k];
                        let (prev, next) = (
                            points[k.saturating_sub(1)].0,
                            points[(k + 1).min(points.len() - 1)].0,
                        );
                        let t = (next - prev).normalize_or_zero();
                        let u = (n - t * n.dot(t)).normalize_or_zero();
                        let v = t.cross(u);
                        let centre = p + n * (r + TRIM_LIFT_M);
                        (0..PIPING_SIDES)
                            .map(|i| {
                                let a = std::f64::consts::TAU * i as f64 / PIPING_SIDES as f64;
                                centre + (u * a.cos() + v * a.sin()) * r
                            })
                            .collect()
                    })
                    .collect();
                for w in rings.windows(2) {
                    for i in 0..PIPING_SIDES {
                        let j = (i + 1) % PIPING_SIDES;
                        tri([w[0][i], w[1][i], w[1][j]], PIPING_COLOR);
                        tri([w[0][i], w[1][j], w[0][j]], PIPING_COLOR);
                    }
                }
            }
            TrimKind::Object {
                center_m,
                turn_degrees,
                scale,
                vertices,
                triangles,
                ..
            } => {
                let Some((p, n, along)) = surface_frame(garment, x, panel, *center_m) else {
                    continue;
                };
                let (sin, cos) = turn_degrees.to_radians().sin_cos();
                let across = n.cross(along);
                let (u, v) = (along * cos + across * sin, across * cos - along * sin);
                let origin = p + n * TRIM_LIFT_M;
                let place = |k: u32| {
                    let [a, b, c] = vertices[k as usize].map(f64::from);
                    origin + (u * a + v * b + n * c) * *scale
                };
                for t in triangles {
                    tri(t.map(place), OBJECT_COLOR);
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
