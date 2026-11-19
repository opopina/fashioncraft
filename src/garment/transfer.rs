//! Carry a draped shape over to a remeshed panel. Both meshes of the panel are laid out in the
//! same flat arrangement, so a new vertex is located on the old arrangement (nearest triangle,
//! barycentric coordinates in its plane) and takes the same combination of the old draped
//! positions. Outside the old outline the nearest triangle's affine map is extended, so a
//! piece that grew keeps following the local drape instead of tearing back to the flat layout.
use glam::DVec3;

/// Closest distance from `q` to triangle `t` and the (unclamped) barycentric coordinates of
/// `q`'s projection onto the triangle's plane.
fn locate(q: DVec3, t: [DVec3; 3]) -> (f64, [f64; 3]) {
    let [a, b, c] = t;
    let (e1, e2) = (b - a, c - a);
    let d = q - a;
    let (g11, g12, g22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = g11 * g22 - g12 * g12;
    if det <= 1e-30 {
        return (f64::INFINITY, [1., 0., 0.]);
    }
    let (r1, r2) = (d.dot(e1), d.dot(e2));
    let s = (r1 * g22 - r2 * g12) / det;
    let u = (g11 * r2 - g12 * r1) / det;
    let weights = [1. - s - u, s, u];
    let distance = if s >= 0. && u >= 0. && s + u <= 1. {
        (d - e1 * s - e2 * u).length()
    } else {
        let segment = |p: DVec3, r: DVec3| {
            let along = r - p;
            let k = ((q - p).dot(along) / along.length_squared().max(1e-30)).clamp(0., 1.);
            q.distance(p + along * k)
        };
        segment(a, b).min(segment(b, c)).min(segment(c, a))
    };
    (distance, weights)
}

/// Draped positions for the vertices `new_rest` of a remeshed panel, from the old mesh's
/// arrangement `old_rest`, local triangles `old_triangles` and draped positions `old_x`.
pub fn transfer_positions(
    old_rest: &[DVec3],
    old_triangles: &[[usize; 3]],
    old_x: &[DVec3],
    new_rest: &[DVec3],
) -> Vec<DVec3> {
    if old_triangles.is_empty() {
        return new_rest.to_vec();
    }
    let spheres: Vec<(DVec3, f64)> = old_triangles
        .iter()
        .map(|t| {
            let centre = (old_rest[t[0]] + old_rest[t[1]] + old_rest[t[2]]) / 3.;
            let radius = t
                .iter()
                .map(|&i| old_rest[i].distance(centre))
                .fold(0., f64::max);
            (centre, radius)
        })
        .collect();
    new_rest
        .iter()
        .map(|&q| {
            let mut best = (f64::INFINITY, 0usize, [1., 0., 0.]);
            for (k, t) in old_triangles.iter().enumerate() {
                let (centre, radius) = spheres[k];
                if q.distance(centre) - radius > best.0 {
                    continue;
                }
                let (distance, weights) = locate(q, t.map(|i| old_rest[i]));
                if distance < best.0 {
                    best = (distance, k, weights);
                }
            }
            let (_, k, w) = best;
            let t = old_triangles[k];
            old_x[t[0]] * w[0] + old_x[t[1]] * w[1] + old_x[t[2]] * w[2]
        })
        .collect()
}

/// Closest point to `q` on triangle `[a, b, c]`.
pub(crate) fn closest_on_triangle(q: DVec3, [a, b, c]: [DVec3; 3]) -> DVec3 {
    let (e1, e2) = (b - a, c - a);
    let d = q - a;
    let (g11, g12, g22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = g11 * g22 - g12 * g12;
    if det > 1e-30 {
        let (r1, r2) = (d.dot(e1), d.dot(e2));
        let s = (r1 * g22 - r2 * g12) / det;
        let u = (g11 * r2 - g12 * r1) / det;
        if s >= 0. && u >= 0. && s + u <= 1. {
            return a + e1 * s + e2 * u;
        }
    }
    let segment = |p: DVec3, r: DVec3| {
        let along = r - p;
        let k = ((q - p).dot(along) / along.length_squared().max(1e-30)).clamp(0., 1.);
        p + along * k
    };
    [segment(a, b), segment(b, c), segment(c, a)]
        .into_iter()
        .min_by(|x, y| x.distance_squared(q).total_cmp(&y.distance_squared(q)))
        .unwrap()
}

/// Lay a piece on another piece's draped surface: each of `moved` goes onto the plane of the
/// nearest face of the `target` surface (positions `target_x`, triangles `target_triangles`
/// indexing them) and then `offset` along that face's normal, outwards (away from the vertical axis
/// through `axis`) when `over`, inwards otherwise. For facings, linings, cuffs and collars
/// that are sewn to the target and should start on it, not through it.
pub fn superimpose_positions(
    moved: &[DVec3],
    target_x: &[DVec3],
    target_triangles: &[[usize; 3]],
    axis: DVec3,
    over: bool,
    offset: f64,
) -> Vec<DVec3> {
    if target_triangles.is_empty() {
        return moved.to_vec();
    }
    moved
        .iter()
        .map(|&q| {
            let mut best = (f64::INFINITY, q, DVec3::ZERO);
            for t in target_triangles {
                let corners = t.map(|i| target_x[i]);
                let p = closest_on_triangle(q, corners);
                let d = p.distance_squared(q);
                if d < best.0 {
                    let normal = (corners[1] - corners[0])
                        .cross(corners[2] - corners[0])
                        .normalize_or_zero();
                    best = (d, p, normal);
                }
            }
            let (_, closest, mut normal) = best;
            // Onto the plane of the nearest face rather than its closest point: points past
            // the target's edge keep their spacing instead of piling up on that edge.
            let p = q - normal * (q - closest).dot(normal);
            let mut away = p - axis;
            away.y = 0.;
            if normal.dot(away) < 0. {
                normal = -normal;
            }
            if !over {
                normal = -normal;
            }
            p + normal * offset
        })
        .collect()
}

/// Fold arrangement: the part of `panel` on one side of the straight line through its
/// material points `line` (left of it, looking from the first point to the second; right of it
/// when `other_side`) turned about that line as it lies in `x`, so the piece starts folded
/// there at `angle_degrees`, a line fold's angle: 180 is flat, less folds toward the side the
/// piece's normal faces, more away from it. Returns the moved vertices and their positions.
pub fn fold_about_line(
    garment: &super::CompiledGarment,
    x: &[DVec3],
    panel: &str,
    line: [glam::DVec2; 2],
    angle_degrees: f64,
    other_side: bool,
) -> Result<Vec<(usize, DVec3)>, String> {
    if !(0.0..=360.0).contains(&angle_degrees) {
        return Err("A fold angle is between 0° and 360°".into());
    }
    let range = garment
        .panels
        .iter()
        .find(|r| r.id == panel)
        .ok_or_else(|| format!("There is no piece {panel}"))?;
    let at = |uv: glam::DVec2| -> Result<DVec3, String> {
        let weights = garment
            .material_point(panel, uv)
            .ok_or("The fold line must lie on the piece")?;
        // The lookup falls back to the nearest face; a point off the piece is refused.
        let found: glam::DVec2 = weights.iter().map(|&(v, w)| garment.mesh.uv[v] * w).sum();
        if found.distance(uv) > 1e-3 {
            return Err("The fold line must lie on the piece".into());
        }
        Ok(weights.iter().map(|&(v, w)| x[v] * w).sum())
    };
    let (a, b) = (at(line[0])?, at(line[1])?);
    let along = line[1] - line[0];
    if along.length() < 1e-6 || a.distance(b) < 1e-6 {
        return Err("The fold line is too short".into());
    }
    let sign = if other_side { -1. } else { 1. };
    let moving: Vec<usize> = range
        .vertices
        .clone()
        .filter(|&v| sign * along.perp_dot(garment.mesh.uv[v] - line[0]) > 1e-9)
        .collect();
    if moving.is_empty() {
        return Err("Nothing of the piece lies on that side of the line".into());
    }
    // The piece's front, as its faces wind, near where it is now.
    let normal = garment.mesh.triangles[range.triangles.clone()]
        .iter()
        .map(|t| {
            let [p, q, r] = t.vertices.map(|v| x[v]);
            (q - p).cross(r - p)
        })
        .sum::<DVec3>()
        .normalize_or_zero();
    let axis = (b - a).normalize();
    let turn = (180. - angle_degrees).to_radians();
    let place = |turn: f64| -> Vec<(usize, DVec3)> {
        let q = glam::DQuat::from_axis_angle(axis, turn);
        moving.iter().map(|&v| (v, a + q * (x[v] - a))).collect()
    };
    let moved = place(turn);
    let shift: DVec3 = moved.iter().map(|&(v, p)| p - x[v]).sum();
    // Below 180° the moving part turns toward the front: pick the turn's sense accordingly.
    Ok(if shift.dot(normal) * turn.signum() < 0. {
        place(-turn)
    } else {
        moved
    })
}

/// Reset 2D arrangement: every piece of `garment` laid flat in 3D as it lies on the board of
/// `template`, on an upright plane a hand's breadth in front of `body` (the avatar's
/// vertices), centred on it, the board's downward y turned upward. Returns each vertex with
/// its position.
pub fn lay_out_as_board(
    template: &super::GarmentTemplate,
    garment: &super::CompiledGarment,
    body: &[DVec3],
) -> Vec<(usize, DVec3)> {
    use crate::authoring::arrange2d::board_point;
    let mut board: Vec<(usize, glam::DVec2)> = vec![];
    for range in &garment.panels {
        let Some(index) = template.panels.iter().position(|p| p.id == range.id) else {
            continue;
        };
        for v in range.vertices.clone() {
            board.push((v, board_point(template, index, garment.mesh.uv[v])));
        }
    }
    let (lo, hi) = board.iter().fold(
        (
            glam::DVec2::splat(f64::INFINITY),
            glam::DVec2::splat(f64::NEG_INFINITY),
        ),
        |(lo, hi), (_, b)| (lo.min(*b), hi.max(*b)),
    );
    let centre = (lo + hi) * 0.5;
    let (body_lo, body_hi) = if body.is_empty() {
        (DVec3::ZERO, DVec3::new(0., 1.7, 0.))
    } else {
        body.iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        )
    };
    let body_centre = (body_lo + body_hi) * 0.5;
    let front = body_hi.z + 0.25;
    board
        .into_iter()
        .map(|(v, b)| {
            (
                v,
                DVec3::new(
                    body_centre.x + (b.x - centre.x),
                    body_centre.y - (b.y - centre.y),
                    front,
                ),
            )
        })
        .collect()
}
