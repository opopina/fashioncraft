//! A skeleton for the avatar built from its own joint landmarks, automatic skin weights and
//! linear blend skinning. Bones are segments between landmarks (pelvis, spine, neck, head,
//! clavicles, arms, hands, legs and feet). Each vertex first belongs to its nearest bone and
//! the weights are then smoothed along the surface, never through the air, so an arm never
//! drags the torso it hangs beside. A pose turns bones about their heads; children follow.
use crate::avatar::SurfaceMesh;
use glam::{DAffine3, DQuat, DVec3, EulerRot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Landmark ids, head and tail of each bone, and its parent, in an order where parents come
/// first. The pelvis is the root.
const BONES: &[(&str, &str, &str, Option<&str>)] = &[
    ("pelvis", "joint-pelvis", "joint-spine-3", None),
    ("spine", "joint-spine-3", "joint-spine-1", Some("pelvis")),
    ("chest", "joint-spine-1", "joint-neck", Some("spine")),
    ("neck", "joint-neck", "joint-head", Some("chest")),
    ("head", "joint-head", "joint-head-2", Some("neck")),
    (
        "clavicle.L",
        "joint-l-clavicle",
        "joint-l-shoulder",
        Some("chest"),
    ),
    (
        "upper_arm.L",
        "joint-l-shoulder",
        "joint-l-elbow",
        Some("clavicle.L"),
    ),
    (
        "forearm.L",
        "joint-l-elbow",
        "joint-l-hand",
        Some("upper_arm.L"),
    ),
    (
        "hand.L",
        "joint-l-hand",
        "joint-l-finger-3-4",
        Some("forearm.L"),
    ),
    (
        "clavicle.R",
        "joint-r-clavicle",
        "joint-r-shoulder",
        Some("chest"),
    ),
    (
        "upper_arm.R",
        "joint-r-shoulder",
        "joint-r-elbow",
        Some("clavicle.R"),
    ),
    (
        "forearm.R",
        "joint-r-elbow",
        "joint-r-hand",
        Some("upper_arm.R"),
    ),
    (
        "hand.R",
        "joint-r-hand",
        "joint-r-finger-3-4",
        Some("forearm.R"),
    ),
    (
        "thigh.L",
        "joint-l-upper-leg",
        "joint-l-knee",
        Some("pelvis"),
    ),
    ("shin.L", "joint-l-knee", "joint-l-ankle", Some("thigh.L")),
    ("foot.L", "joint-l-ankle", "joint-l-foot-2", Some("shin.L")),
    (
        "thigh.R",
        "joint-r-upper-leg",
        "joint-r-knee",
        Some("pelvis"),
    ),
    ("shin.R", "joint-r-knee", "joint-r-ankle", Some("thigh.R")),
    ("foot.R", "joint-r-ankle", "joint-r-foot-2", Some("shin.R")),
];

/// Smoothing passes over the surface: wider blends at the joints with more passes.
const SMOOTHING_PASSES: usize = 24;
/// Most bones one vertex follows.
const INFLUENCES: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: &'static str,
    pub parent: Option<usize>,
    pub head: DVec3,
    pub tail: DVec3,
}

/// Bones and per-vertex weights for one avatar surface.
#[derive(Clone, Debug)]
pub struct Rig {
    pub bones: Vec<Bone>,
    /// Up to [`INFLUENCES`] (bone, weight) pairs per vertex, weights summing to one.
    pub weights: Vec<Vec<(usize, f64)>>,
}

/// Bone rotations in degrees about the world X, Y and Z axes at rest, by bone name. Bones not
/// listed stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pose(pub BTreeMap<String, [f64; 3]>);

impl Pose {
    pub fn rotation(&self, bone: &str) -> DQuat {
        self.0.get(bone).map_or(DQuat::IDENTITY, |[x, y, z]| {
            DQuat::from_euler(
                EulerRot::XYZ,
                x.to_radians(),
                y.to_radians(),
                z.to_radians(),
            )
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        for (bone, angles) in &self.0 {
            if !BONES.iter().any(|b| b.0 == bone) {
                return Err(format!("Unknown bone {bone}"));
            }
            if angles.iter().any(|a| !a.is_finite() || a.abs() > 360.) {
                return Err(format!("Bone {bone} needs finite angles within ±360°"));
            }
        }
        Ok(())
    }
    /// Halfway poses for transitions: each bone's rotation slerped from `self` to `to`.
    pub fn blend(&self, to: &Pose, t: f64) -> BTreeMap<String, DQuat> {
        BONES
            .iter()
            .map(|b| {
                (
                    b.0.to_string(),
                    self.rotation(b.0).slerp(to.rotation(b.0), t),
                )
            })
            .collect()
    }
}

impl Pose {
    pub fn is_empty(&self) -> bool {
        self.0.values().all(|a| a.iter().all(|x| *x == 0.))
    }
}

/// Landmark ids the skeleton is built from.
pub fn joint_ids() -> std::collections::BTreeSet<&'static str> {
    BONES.iter().flat_map(|b| [b.1, b.2]).collect()
}

/// Names of the bones a pose can turn, parents first.
pub fn bone_names() -> impl Iterator<Item = &'static str> {
    BONES.iter().map(|b| b.0)
}

/// Distance from `p` to the segment `a–b`.
fn segment_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-18)).clamp(0., 1.);
    p.distance(a + ab * t)
}

impl Rig {
    /// Build the rig of `surface` from its joint landmarks.
    pub fn new(landmarks: &BTreeMap<String, DVec3>, surface: &SurfaceMesh) -> Result<Self, String> {
        let mut bones: Vec<Bone> = vec![];
        for (name, head, tail, parent) in BONES {
            let at = |id: &str| {
                landmarks
                    .get(id)
                    .copied()
                    .ok_or_else(|| format!("The avatar has no {id} landmark; it cannot be posed"))
            };
            let parent = parent.map(|p| {
                bones
                    .iter()
                    .position(|b| b.name == p)
                    .expect("parents come first")
            });
            bones.push(Bone {
                name,
                parent,
                head: at(head)?,
                tail: at(tail)?,
            });
        }
        let n = surface.vertices.len();
        // Nearest bone, as a one-hot weight vector.
        let mut w: Vec<Vec<f64>> = surface
            .vertices
            .iter()
            .map(|p| {
                let nearest = (0..bones.len())
                    .min_by(|&a, &b| {
                        segment_distance(*p, bones[a].head, bones[a].tail)
                            .total_cmp(&segment_distance(*p, bones[b].head, bones[b].tail))
                    })
                    .unwrap();
                let mut v = vec![0.; bones.len()];
                v[nearest] = 1.;
                v
            })
            .collect();
        // Smooth along the surface's edges.
        let mut neighbours = vec![vec![]; n];
        for t in &surface.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                neighbours[a].push(b);
                neighbours[b].push(a);
            }
        }
        for list in &mut neighbours {
            list.sort_unstable();
            list.dedup();
        }
        for _ in 0..SMOOTHING_PASSES {
            let next: Vec<Vec<f64>> = (0..n)
                .map(|i| {
                    if neighbours[i].is_empty() {
                        return w[i].clone();
                    }
                    let mut mean = w[i].clone();
                    for &j in &neighbours[i] {
                        for (m, x) in mean.iter_mut().zip(&w[j]) {
                            *m += x;
                        }
                    }
                    let k = (neighbours[i].len() + 1) as f64;
                    mean.iter_mut().for_each(|m| *m /= k);
                    mean
                })
                .collect();
            w = next;
        }
        let weights = w
            .into_iter()
            .map(|v| {
                let mut pairs: Vec<(usize, f64)> = v
                    .into_iter()
                    .enumerate()
                    .filter(|(_, x)| *x > 1e-6)
                    .collect();
                pairs.sort_by(|a, b| b.1.total_cmp(&a.1));
                pairs.truncate(INFLUENCES);
                let sum: f64 = pairs.iter().map(|p| p.1).sum();
                pairs.iter_mut().for_each(|p| p.1 /= sum);
                pairs
            })
            .collect();
        Ok(Self { bones, weights })
    }

    /// Each bone's transform from rest to posed, with bone rotations given by name.
    pub fn transforms(&self, rotation: impl Fn(&str) -> DQuat) -> Vec<DAffine3> {
        let mut out: Vec<DAffine3> = Vec::with_capacity(self.bones.len());
        for bone in &self.bones {
            let local = DAffine3::from_translation(bone.head)
                * DAffine3::from_quat(rotation(bone.name))
                * DAffine3::from_translation(-bone.head);
            let global = match bone.parent {
                Some(p) => out[p] * local,
                None => local,
            };
            out.push(global);
        }
        out
    }

    /// `surface` posed by bone transforms `t` (from [`Rig::transforms`]).
    pub fn skin(&self, surface: &SurfaceMesh, t: &[DAffine3]) -> SurfaceMesh {
        let vertices = surface
            .vertices
            .iter()
            .zip(&self.weights)
            .map(|(p, w)| w.iter().map(|(b, x)| t[*b].transform_point3(*p) * *x).sum())
            .collect();
        SurfaceMesh {
            vertices,
            triangles: surface.triangles.clone(),
        }
    }

    /// `surface` in `pose`.
    pub fn posed(&self, surface: &SurfaceMesh, pose: &Pose) -> SurfaceMesh {
        self.skin(surface, &self.transforms(|b| pose.rotation(b)))
    }

    /// Joints (bone heads and tails) in `pose`, for drawing the skeleton.
    pub fn posed_bones(&self, pose: &Pose) -> Vec<(DVec3, DVec3)> {
        let t = self.transforms(|b| pose.rotation(b));
        self.bones
            .iter()
            .zip(&t)
            .map(|(b, m)| (m.transform_point3(b.head), m.transform_point3(b.tail)))
            .collect()
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
}

/// Ready-made poses: rotations that bring the arms down or out, or bend the knees a little.
pub fn preset(name: &str) -> Option<Pose> {
    let p = |entries: &[(&str, [f64; 3])]| {
        Pose(entries.iter().map(|(b, a)| (b.to_string(), *a)).collect())
    };
    Some(match name {
        "Rest" => Pose::default(),
        "Arms down" => p(&[
            ("upper_arm.L", [0., 0., -30.]),
            ("upper_arm.R", [0., 0., 30.]),
        ]),
        "T-pose" => p(&[
            ("upper_arm.L", [0., 0., 50.]),
            ("forearm.L", [0., 0., 10.]),
            ("upper_arm.R", [0., 0., -50.]),
            ("forearm.R", [0., 0., -10.]),
        ]),
        "Arms forward" => p(&[
            ("upper_arm.L", [-60., 0., -20.]),
            ("upper_arm.R", [-60., 0., 20.]),
        ]),
        "Step" => p(&[
            ("thigh.L", [-25., 0., 0.]),
            ("shin.L", [30., 0., 0.]),
            ("thigh.R", [10., 0., 0.]),
        ]),
        _ => return None,
    })
}

/// Names of the ready-made poses.
pub const PRESETS: [&str; 5] = ["Rest", "Arms down", "T-pose", "Arms forward", "Step"];
