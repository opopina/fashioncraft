//! Joint landmarks estimated from an avatar's shape, for avatars that ship none.
//!
//! The bundled MakeHuman mannequin's joints serve as a template. They are scaled to the
//! avatar's height and centred on its hips; each arm is aimed at its farthest point; then
//! every joint moves to the centre of the body's cross-section through it, cut across the
//! bone (horizontally on the torso and legs, across the arm on the arms). The avatar must
//! stand Y up, facing +Z, arms away from the body (A or T pose), like the template. The
//! estimate is refused when the body does not look like one: no hips, no two legs, or arms
//! that do not reach out sideways.
use crate::avatar::SurfaceMesh;
use glam::DVec3;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

const TEMPLATE: &str = include_str!("../assets/makehuman/measurements.json");

/// The joints estimated: the skeleton's and the arrangement anchors'.
pub const JOINTS: [&str; 25] = [
    "joint-pelvis",
    "joint-spine-3",
    "joint-spine-2",
    "joint-spine-1",
    "joint-neck",
    "joint-head",
    "joint-head-2",
    "joint-l-clavicle",
    "joint-r-clavicle",
    "joint-l-shoulder",
    "joint-r-shoulder",
    "joint-l-elbow",
    "joint-r-elbow",
    "joint-l-hand",
    "joint-r-hand",
    "joint-l-finger-3-4",
    "joint-r-finger-3-4",
    "joint-l-upper-leg",
    "joint-r-upper-leg",
    "joint-l-knee",
    "joint-r-knee",
    "joint-l-ankle",
    "joint-r-ankle",
    "joint-l-foot-2",
    "joint-r-foot-2",
];

fn template() -> (BTreeMap<String, DVec3>, f64, f64) {
    #[derive(Deserialize)]
    struct Prepared {
        landmarks: BTreeMap<String, [f64; 3]>,
        bounds_m: [[f64; 3]; 2],
    }
    let prepared: Prepared = serde_json::from_str(TEMPLATE).expect("bundled template");
    (
        prepared
            .landmarks
            .into_iter()
            .map(|(k, v)| (k, DVec3::from_array(v)))
            .collect(),
        prepared.bounds_m[0][1],
        prepared.bounds_m[1][1] - prepared.bounds_m[0][1],
    )
}

/// A closed cross-section of the surface: its centre and its length.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub centre: DVec3,
    pub length: f64,
    /// Corners of the box around the section.
    pub lo: DVec3,
    pub hi: DVec3,
}

/// The cross-sections of `mesh` by the plane through `point` with normal `normal`.
pub fn sections(mesh: &SurfaceMesh, point: DVec3, normal: DVec3) -> Vec<Section> {
    let d: Vec<f64> = mesh
        .vertices
        .iter()
        .map(|v| {
            let s = (*v - point).dot(normal);
            // A vertex on the plane counts as just above it.
            if s == 0. { 1e-12 } else { s }
        })
        .collect();
    let mut edge_index: HashMap<(usize, usize), usize> = HashMap::new();
    let mut crossing: Vec<DVec3> = Vec::new();
    let mut segments: Vec<(usize, usize)> = Vec::new();
    for t in &mesh.triangles {
        let mut ends = Vec::with_capacity(2);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if d[a] * d[b] < 0. {
                let key = (a.min(b), a.max(b));
                let id = *edge_index.entry(key).or_insert_with(|| {
                    let (va, vb) = (mesh.vertices[a], mesh.vertices[b]);
                    crossing.push(va + (vb - va) * (d[a] / (d[a] - d[b])));
                    crossing.len() - 1
                });
                ends.push(id);
            }
        }
        if ends.len() == 2 {
            segments.push((ends[0], ends[1]));
        }
    }
    // Join segments sharing a crossing point into loops.
    let mut parent: Vec<usize> = (0..crossing.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (a, b) in &segments {
        let (ra, rb) = (root(&mut parent, *a), root(&mut parent, *b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut loops: HashMap<usize, (DVec3, f64, DVec3, DVec3)> = HashMap::new();
    for (a, b) in &segments {
        let r = root(&mut parent, *a);
        let (pa, pb) = (crossing[*a], crossing[*b]);
        let length = pa.distance(pb);
        let entry = loops.entry(r).or_insert((
            DVec3::ZERO,
            0.,
            DVec3::splat(f64::INFINITY),
            DVec3::splat(f64::NEG_INFINITY),
        ));
        entry.0 += (pa + pb) * 0.5 * length;
        entry.1 += length;
        entry.2 = entry.2.min(pa).min(pb);
        entry.3 = entry.3.max(pa).max(pb);
    }
    loops
        .into_values()
        .filter(|(_, length, _, _)| *length > 0.)
        .map(|(sum, length, lo, hi)| Section {
            centre: sum / length,
            length,
            lo,
            hi,
        })
        .collect()
}

/// The centre of the cross-section nearest `point`, when one lies within `reach`.
fn refine(mesh: &SurfaceMesh, point: DVec3, normal: DVec3, reach: f64) -> Option<DVec3> {
    sections(mesh, point, normal)
        .into_iter()
        .map(|s| s.centre)
        .min_by(|a, b| a.distance(point).total_cmp(&b.distance(point)))
        .filter(|c| c.distance(point) <= reach)
}

/// The joints of a humanoid `mesh`, estimated from its shape.
pub fn estimate(mesh: &SurfaceMesh) -> Result<BTreeMap<String, DVec3>, String> {
    let not_a_body = |why: &str| format!("Joints cannot be estimated from this avatar: {why}");
    let (t, t_floor, t_height) = template();
    let (lo, hi) = mesh.vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let height = hi.y - lo.y;
    if !(0.3..=3.).contains(&height) {
        return Err(not_a_body("it is not between 0.3 and 3 m tall"));
    }
    let s = height / t_height;
    let at = |id: &str| t[id];
    let level = |id: &str| lo.y + s * (at(id).y - t_floor);
    // The hips: the longest cross-section at pelvis height.
    let hips = sections(mesh, DVec3::new(0., level("joint-pelvis"), 0.), DVec3::Y)
        .into_iter()
        .max_by(|a, b| a.length.total_cmp(&b.length))
        .ok_or_else(|| not_a_body("no section at hip height"))?;
    let centre = hips.centre;
    let place = |id: &str| {
        let p = at(id);
        DVec3::new(
            centre.x + s * p.x,
            level(id),
            centre.z + s * (p.z - at("joint-pelvis").z),
        )
    };
    let mut out: BTreeMap<String, DVec3> = BTreeMap::new();
    // Torso and head: across horizontal sections, at the depth the template's joints keep
    // from the back (fractions measured on the bundled template).
    let mut head_section = None;
    for (id, depth) in [
        ("joint-pelvis", 0.473),
        ("joint-spine-3", 0.301),
        ("joint-spine-2", 0.257),
        ("joint-spine-1", 0.211),
        ("joint-neck", 0.360),
        ("joint-head", 0.214),
    ] {
        let p = place(id);
        let section = sections(mesh, p, DVec3::Y)
            .into_iter()
            .min_by(|a, b| a.centre.distance(p).total_cmp(&b.centre.distance(p)))
            .filter(|c| c.centre.distance(p) <= 0.15 * s);
        let q = match section {
            Some(c) => DVec3::new(c.centre.x, p.y, c.lo.z + depth * (c.hi.z - c.lo.z)),
            None => p,
        };
        if id == "joint-head" {
            head_section = section;
        }
        out.insert(id.into(), q);
    }
    let head = out["joint-head"];
    let top_depth = head_section.map_or(head.z, |c| c.lo.z + 0.173 * (c.hi.z - c.lo.z));
    out.insert("joint-head-2".into(), DVec3::new(head.x, hi.y, top_depth));
    let neck = out["joint-neck"];
    let from_neck = |id: &str| neck + s * (at(id) - at("joint-neck"));
    for side in ["l", "r"] {
        let sign = if side == "l" { 1. } else { -1. };
        let id = |name: &str| format!("joint-{side}-{name}");
        out.insert(id("clavicle"), from_neck(&id("clavicle")));
        let shoulder = from_neck(&id("shoulder"));
        out.insert(id("shoulder"), shoulder);
        // The arm reaches its farthest point sideways, above the hips.
        let tip = mesh
            .vertices
            .iter()
            .filter(|v| v.y > hips.centre.y)
            .copied()
            .max_by(|a, b| (sign * a.x).total_cmp(&(sign * b.x)))
            .ok_or_else(|| not_a_body("no arms"))?;
        if sign * (tip.x - shoulder.x) < 0.15 * s {
            return Err(not_a_body("the arms do not reach out sideways"));
        }
        // The template's arm, bend and all, turned and scaled to reach the tip.
        let (t_shoulder, t_tip) = (at(&id("shoulder")), at(&id("finger-3-4")));
        let turn = glam::DQuat::from_rotation_arc(
            (t_tip - t_shoulder).normalize(),
            (tip - shoulder).normalize(),
        );
        let k = tip.distance(shoulder) / t_tip.distance(t_shoulder);
        let arm = |name: &str| shoulder + turn * (at(&id(name)) - t_shoulder) * k;
        let across = |a: &str, b: &str| turn * (at(&id(b)) - at(&id(a))).normalize();
        let (elbow_at, hand_at) = (arm("elbow"), arm("hand"));
        let elbow =
            refine(mesh, elbow_at, across("shoulder", "hand"), 0.08 * s).unwrap_or(elbow_at);
        let hand = refine(mesh, hand_at, across("elbow", "hand"), 0.06 * s).unwrap_or(hand_at);
        out.insert(id("elbow"), elbow);
        out.insert(id("hand"), hand);
        out.insert(id("finger-3-4"), tip);
        // Legs: horizontal sections on this side of the body.
        let hip = out["joint-pelvis"] + s * (at(&id("upper-leg")) - at("joint-pelvis"));
        out.insert(id("upper-leg"), hip);
        for joint in ["knee", "ankle"] {
            let p = hip + s * (at(&id(joint)) - at(&id("upper-leg")));
            let section = sections(mesh, p, DVec3::Y)
                .into_iter()
                .filter(|c| sign * (c.centre.x - centre.x) > 0.)
                .min_by(|a, b| a.centre.distance(p).total_cmp(&b.centre.distance(p)))
                .ok_or_else(|| not_a_body("no legs apart"))?;
            out.insert(
                id(joint),
                DVec3::new(section.centre.x, p.y, section.centre.z),
            );
        }
        let ankle = out[&id("ankle")];
        out.insert(
            id("foot-2"),
            ankle + s * (at(&id("foot-2")) - at(&id("ankle"))),
        );
    }
    if out.values().any(|p| !p.is_finite()) {
        return Err(not_a_body("the estimate is not finite"));
    }
    Ok(out)
}
