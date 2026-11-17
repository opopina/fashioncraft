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
