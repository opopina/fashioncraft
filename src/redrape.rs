//! Re-drape (3D Garment › Re-drape 3D arrangement): carry a draped garment from one body to
//! another, keeping its size, for the simulation to settle it on the new body.
//!
//! Each cloth point is tied to the old body where the body is nearest to it. When both bodies
//! share one mesh (one reshaped from the other) the point goes to the same spot of the same
//! triangle, at the same offset in that triangle's frame. Otherwise it goes by the bones the
//! skin there follows: how far along each bone and which way round it the skin lies. The same
//! place on the new body's bone, a ray outward the same way finds the new skin, and the point
//! keeps its height off it. Where the skin follows several bones the results blend by its skin
//! weights, so the cloth is not torn apart at the joints.
//!
//! Bones come from joint landmarks: the ones given, or ones estimated from the body's shape
//! (which needs a standing body, arms away from it).
use crate::{
    avatar::{MeshCollider, SurfaceMesh},
    avatar_landmarks,
    avatar_rig::Rig,
};
use glam::DVec3;
use std::{collections::BTreeMap, sync::Arc};

/// Joint landmarks by id, metres.
pub type Landmarks = BTreeMap<String, DVec3>;

/// Least skin weight a bone needs to carry a point.
const MIN_WEIGHT: f64 = 0.02;
/// Most places along one ray where it meets the new body.
const MAX_HITS: usize = 8;

/// Barycentric weights of `q` in the triangle `[a, b, c]`.
fn barycentric(q: DVec3, [a, b, c]: [DVec3; 3]) -> [f64; 3] {
    let (e, f, g) = (b - a, c - a, q - a);
    let (ee, ef, ff) = (e.dot(e), e.dot(f), f.dot(f));
    let (ge, gf) = (g.dot(e), g.dot(f));
    let det = ee * ff - ef * ef;
    if det.abs() < 1e-30 {
        return [1., 0., 0.];
    }
    let v = (ff * ge - ef * gf) / det;
    let w = (ee * gf - ef * ge) / det;
    [1. - v - w, v, w]
}

/// A triangle's frame: along its first edge, across it in its plane, and its normal.
fn triangle_frame([a, b, c]: [DVec3; 3]) -> [DVec3; 3] {
    let e = (b - a).normalize_or_zero();
    let n = (b - a).cross(c - a).normalize_or_zero();
    [e, n.cross(e), n]
}

/// A bone's axis, length, and two directions across it chosen the same way on every body.
fn bone_frame(head: DVec3, tail: DVec3) -> Option<(DVec3, f64, DVec3, DVec3)> {
    let along = tail - head;
    let length = along.length();
    if length < 1e-6 {
        return None;
    }
    let axis = along / length;
    let up = if axis.z.abs() < 0.95 {
        DVec3::Z
    } else {
        DVec3::Y
    };
    let reference = (up - axis * up.dot(axis)).normalize();
    Some((axis, length, reference, axis.cross(reference)))
}

/// A body's skeleton for carrying: each bone's name and ends, and per surface vertex the
/// bones it follows.
struct Skeleton {
    bones: Vec<(String, DVec3, DVec3)>,
    weights: Vec<Vec<(usize, f64)>>,
}

fn skeleton(surface: &SurfaceMesh, landmarks: Option<&Landmarks>) -> Result<Skeleton, String> {
    let estimated;
    let landmarks = match landmarks {
        Some(landmarks) => landmarks,
        None => {
            estimated = avatar_landmarks::estimate(surface)?;
            &estimated
        }
    };
    let rig = Rig::new(landmarks, surface)?;
    Ok(Skeleton {
        bones: rig
            .bones
            .iter()
            .map(|b| (b.name.clone(), b.head, b.tail))
            .collect(),
        weights: rig.weights,
    })
}

/// Carry the cloth at `x`, draped on the body `from`, onto the body `to`. `from_landmarks` and
/// `to_landmarks` give each body's joints; absent, they are estimated from its shape (bodies
/// sharing one mesh need none).
pub fn carry(
    from: &SurfaceMesh,
    from_landmarks: Option<&Landmarks>,
    to: &SurfaceMesh,
    to_landmarks: Option<&Landmarks>,
    x: &[DVec3],
) -> Result<Vec<DVec3>, String> {
    let old = MeshCollider::new(Arc::new(from.clone()), 1.)?;
    if from.vertices.len() == to.vertices.len() && from.triangles == to.triangles {
        return Ok(x.iter().map(|p| same_mesh(&old, to, *p)).collect());
    }
    let new = MeshCollider::new(Arc::new(to.clone()), 1.)?;
    let (a, b) = (
        skeleton(from, from_landmarks).map_err(|e| format!("The old avatar: {e}"))?,
        skeleton(to, to_landmarks).map_err(|e| format!("The new avatar: {e}"))?,
    );
    // The new body's bones by name.
    let partner: Vec<Option<usize>> = a
        .bones
        .iter()
        .map(|(name, ..)| b.bones.iter().position(|(other, ..)| other == name))
        .collect();
    Ok(x.iter()
        .map(|p| by_bones(&old, &new, &a, &b, &partner, *p))
        .collect())
}

/// Same triangle, same spot, same offset in the triangle's frame.
fn same_mesh(old: &MeshCollider, to: &SurfaceMesh, p: DVec3) -> DVec3 {
    let (q, triangle, _) = old.nearest_point(p);
    let ids = old.surface.triangles[triangle];
    let before = ids.map(|i| old.surface.vertices[i]);
    let after = ids.map(|i| to.vertices[i]);
    let w = barycentric(q, before);
    let [e1, e2, n] = triangle_frame(before);
    let r = p - q;
    let (r1, r2, r3) = (r.dot(e1), r.dot(e2), r.dot(n));
    let q2 = after[0] * w[0] + after[1] * w[1] + after[2] * w[2];
    let [f1, f2, m] = triangle_frame(after);
    q2 + f1 * r1 + f2 * r2 + m * r3
}

/// By the bones the nearest skin follows, blended by its skin weights.
fn by_bones(
    old: &MeshCollider,
    new: &MeshCollider,
    a: &Skeleton,
    b: &Skeleton,
    partner: &[Option<usize>],
    p: DVec3,
) -> DVec3 {
    let (q, triangle, _) = old.nearest_point(p);
    let d = old.signed_distance(p);
    let ids = old.surface.triangles[triangle];
    let w = barycentric(q, ids.map(|i| old.surface.vertices[i]));
    // The skin weights at q, from its triangle's corners.
    let mut weights: BTreeMap<usize, f64> = BTreeMap::new();
    for (corner, share) in ids.iter().zip(w) {
        for (bone, weight) in a.weights.get(*corner).into_iter().flatten() {
            *weights.entry(*bone).or_default() += weight * share.max(0.);
        }
    }
    let mut sum = DVec3::ZERO;
    let mut total = 0.;
    for (bone, weight) in weights {
        if weight < MIN_WEIGHT {
            continue;
        }
        let Some(other) = partner[bone] else {
            continue;
        };
        let (_, h, t) = &a.bones[bone];
        let (_, h2, t2) = &b.bones[other];
        let (Some((axis, length, reference, third)), Some((axis2, length2, reference2, third2))) =
            (bone_frame(*h, *t), bone_frame(*h2, *t2))
        else {
            continue;
        };
        let s = (q - *h).dot(axis) / length;
        let radial = q - (*h + axis * (s * length));
        let rho = radial.length();
        let angle = radial.dot(third).atan2(radial.dot(reference));
        let centre = *h2 + axis2 * (s * length2);
        let out = reference2 * angle.cos() + third2 * angle.sin();
        // The new skin: of the places a ray from the bone meets it, the one nearest where the
        // old radius (grown with the bone) puts it, unless all are far from there; then the
        // skin nearest that guess.
        let expected = rho * length2 / length;
        let reach = 3. * expected + 0.05;
        let mut hits = vec![];
        let mut start = 0.;
        while hits.len() < MAX_HITS
            && let Some((t, face)) = new.first_hit(centre + out * start, out, reach - start)
        {
            hits.push((start + t, face));
            start += t + 1e-5;
        }
        let hit = hits
            .into_iter()
            .min_by(|a, b| (a.0 - expected).abs().total_cmp(&(b.0 - expected).abs()))
            .filter(|(t, _)| (t - expected).abs() < 0.5 * expected + 0.01);
        let (skin, normal) = match hit {
            Some((t, face)) => {
                let corners = new.surface.triangles[face].map(|i| new.surface.vertices[i]);
                (centre + out * t, triangle_frame(corners)[2])
            }
            None => {
                let (skin, _, normal) = new.nearest_point(centre + out * expected);
                (skin, normal)
            }
        };
        // Outward: away from the bone.
        let normal = if normal.dot(skin - centre) < 0. {
            -normal
        } else {
            normal
        };
        sum += (skin + normal * d) * weight;
        total += weight;
    }
    if total > 0. { sum / total } else { p }
}

/// Least change of a piece's size Auto fit makes.
const MIN_FIT_CHANGE: f64 = 0.005;

/// Auto fit: how much each piece of `garment` should grow along its material u and v axes for
/// the cloth to stretch on the new body as it did on the old. Compares the cloth's stretch
/// along each axis after carrying (`carried`) with before (`x`), averaged over the piece by
/// area. Factors are kept within 0.7 to 1.4; pieces changing by less than half a percent
/// either way are left out.
pub fn fit_scales(
    garment: &crate::garment::CompiledGarment,
    x: &[DVec3],
    carried: &[DVec3],
) -> Vec<(String, glam::DVec2)> {
    let uv = &garment.mesh.uv;
    let mut out = vec![];
    for range in &garment.panels {
        let mut sums = [0.; 4];
        for t in &garment.mesh.triangles[range.triangles.clone()] {
            let [i, j, k] = t.vertices;
            let m = glam::DMat2::from_cols(uv[j] - uv[i], uv[k] - uv[i]);
            let det = m.determinant();
            if det.abs() < 1e-18 || [i, j, k].iter().any(|v| *v >= x.len().min(carried.len())) {
                continue;
            }
            let area = 0.5 * det.abs();
            let inv = m.inverse();
            for (slot, p) in [(0, x), (2, carried)] {
                let (e1, e2) = (p[j] - p[i], p[k] - p[i]);
                let fu = e1 * inv.col(0).x + e2 * inv.col(0).y;
                let fv = e1 * inv.col(1).x + e2 * inv.col(1).y;
                sums[slot] += area * fu.length_squared();
                sums[slot + 1] += area * fv.length_squared();
            }
        }
        if sums[0] <= 0. || sums[1] <= 0. {
            continue;
        }
        let scale = glam::DVec2::new((sums[2] / sums[0]).sqrt(), (sums[3] / sums[1]).sqrt())
            .clamp(glam::DVec2::splat(0.7), glam::DVec2::splat(1.4));
        if (scale - glam::DVec2::ONE).abs().max_element() >= MIN_FIT_CHANGE {
            out.push((range.id.clone(), scale));
        }
    }
    out
}
