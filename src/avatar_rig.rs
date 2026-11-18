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
    pub name: String,
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

/// A bone added to the skeleton, hanging from `parent` (a built-in bone or one added before
/// it): its ends at rest, avatar metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddedBone {
    pub name: String,
    pub parent: String,
    pub head: [f64; 3],
    pub tail: [f64; 3],
}

/// Skin weights painted by hand, by avatar vertex: its bones and their weights, which replace
/// the automatic ones there.
pub type PaintedWeights = BTreeMap<usize, Vec<(String, f64)>>;

/// Bone rotations in degrees about the world X, Y and Z axes at rest, by bone name. Bones not
/// listed stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pose(pub BTreeMap<String, [f64; 3]>);

/// Format tag of a pose saved on its own ([`Pose::to_file`]).
pub const POSE_FILE_FORMAT: &str = "clothcraft-pose";

#[derive(Serialize, Deserialize)]
struct PoseFile {
    format: String,
    version: u32,
    pose: Pose,
}

impl Pose {
    /// The pose as a file of its own, to use again on another project or avatar.
    pub fn to_file(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&PoseFile {
            format: POSE_FILE_FORMAT.into(),
            version: 1,
            pose: self.clone(),
        })
        .map_err(|e| e.to_string())
    }
    /// A pose read back from [`Pose::to_file`]: the right format, angles in range and bone
    /// names that can exist. Whether this avatar's skeleton has those bones is checked when
    /// the pose is put on it.
    pub fn from_file(text: &str) -> Result<Self, String> {
        let file: PoseFile =
            serde_json::from_str(text).map_err(|e| format!("Not a pose file: {e}"))?;
        if file.format != POSE_FILE_FORMAT {
            return Err(format!("Not a pose file (format {:?})", file.format));
        }
        if file.version != 1 {
            return Err(format!(
                "Pose file version {} is not supported",
                file.version
            ));
        }
        file.pose.validate_angles()?;
        Ok(file.pose)
    }
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
    /// Angles in range, on built-in bones only.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(bone) = self.0.keys().find(|b| !BONES.iter().any(|x| x.0 == *b)) {
            return Err(format!("Unknown bone {bone}"));
        }
        self.validate_angles()
    }

    /// Angles in range, on bones that are built in or could have been added (the skeleton
    /// they are used with checks that it has them).
    pub fn validate_angles(&self) -> Result<(), String> {
        for (bone, angles) in &self.0 {
            if !BONES.iter().any(|b| b.0 == bone) && !valid_bone_name(bone) {
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
        let names: std::collections::BTreeSet<&str> = BONES
            .iter()
            .map(|b| b.0)
            .chain(self.0.keys().map(String::as_str))
            .chain(to.0.keys().map(String::as_str))
            .collect();
        names
            .into_iter()
            .map(|b| (b.to_string(), self.rotation(b).slerp(to.rotation(b), t)))
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

/// Whether `name` can name an added bone: 1 to 64 letters, digits, '_', '.' or '-'.
pub fn valid_bone_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Distance from `p` to the segment `a–b`.
pub fn segment_distance_to(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    segment_distance(p, a, b)
}

/// Distance from `p` to the segment `a–b`.
pub(crate) fn segment_distance(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-18)).clamp(0., 1.);
    p.distance(a + ab * t)
}

/// The strongest [`INFLUENCES`] of (bone, weight) pairs, normalised; empty when none weighs.
fn strongest(mut pairs: Vec<(usize, f64)>) -> Vec<(usize, f64)> {
    pairs.retain(|(_, x)| *x > 1e-6);
    pairs.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    pairs.truncate(INFLUENCES);
    let sum: f64 = pairs.iter().map(|p| p.1).sum();
    pairs.iter_mut().for_each(|p| p.1 /= sum);
    pairs
}

/// Paint `bone` onto the skin around `centre`: each vertex of `surface` within `radius` moves
/// its weights toward that bone by `strength` (0–1) times a falloff that is one at the centre
/// and nothing at the edge, or away from it when `strength` is negative. Starts from `rig`'s
/// weights; the painted vertices' weights are put in `painted`. Returns how many changed.
pub fn paint_weights(
    rig: &Rig,
    surface: &SurfaceMesh,
    painted: &mut PaintedWeights,
    bone: &str,
    centre: DVec3,
    radius: f64,
    strength: f64,
) -> Result<usize, String> {
    let index = rig
        .bone(bone)
        .ok_or_else(|| format!("Unknown bone {bone}"))?;
    if !(radius.is_finite() && radius > 0.) || !strength.is_finite() || strength.abs() > 1. {
        return Err("Paint with a positive radius and a strength between -1 and 1".into());
    }
    let mut changed = 0;
    for (v, p) in surface.vertices.iter().enumerate() {
        let d = p.distance(centre);
        if d >= radius {
            continue;
        }
        let f = (1. - d / radius).powi(2) * strength.abs();
        let mut w = vec![0.; rig.bones.len()];
        let current: Vec<(usize, f64)> = match painted.get(&v) {
            Some(list) => list
                .iter()
                .filter_map(|(name, x)| Some((rig.bone(name)?, *x)))
                .collect(),
            None => rig.weights[v].clone(),
        };
        for (b, x) in current {
            w[b] += x;
        }
        if strength > 0. {
            for (b, x) in w.iter_mut().enumerate() {
                *x = *x * (1. - f) + if b == index { f } else { 0. };
            }
        } else {
            w[index] *= 1. - f;
            if w.iter().sum::<f64>() < 1e-9 {
                continue;
            }
        }
        let pairs = strongest(w.into_iter().enumerate().collect());
        painted.insert(
            v,
            pairs
                .into_iter()
                .map(|(b, x)| (rig.bones[b].name.clone(), x))
                .collect(),
        );
        changed += 1;
    }
    Ok(changed)
}

impl Rig {
    /// Build the rig of `surface` from its joint landmarks.
    pub fn new(landmarks: &BTreeMap<String, DVec3>, surface: &SurfaceMesh) -> Result<Self, String> {
        Self::build(landmarks, surface, &[], &PaintedWeights::new())
    }

    /// [`Rig::new`] with `added` bones after the built-in ones (their automatic weights
    /// computed with the rest) and `painted` weights replacing the automatic ones where given.
    pub fn build(
        landmarks: &BTreeMap<String, DVec3>,
        surface: &SurfaceMesh,
        added: &[AddedBone],
        painted: &PaintedWeights,
    ) -> Result<Self, String> {
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
                name: name.to_string(),
                parent,
                head: at(head)?,
                tail: at(tail)?,
            });
        }
        for bone in added {
            if !valid_bone_name(&bone.name) || bones.iter().any(|b| b.name == bone.name) {
                return Err(format!(
                    "A bone's name must be new and made of letters, digits, '_', '.' or '-': {}",
                    bone.name
                ));
            }
            let parent = bones
                .iter()
                .position(|b| b.name == bone.parent)
                .ok_or_else(|| {
                    format!(
                        "Bone {} hangs from a missing bone {}",
                        bone.name, bone.parent
                    )
                })?;
            let (head, tail) = (DVec3::from_array(bone.head), DVec3::from_array(bone.tail));
            if !head.is_finite() || !tail.is_finite() || head.distance(tail) < 0.005 {
                return Err(format!(
                    "Bone {} needs two ends at least 5 mm apart",
                    bone.name
                ));
            }
            bones.push(Bone {
                name: bone.name.clone(),
                parent: Some(parent),
                head,
                tail,
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
        let mut weights: Vec<Vec<(usize, f64)>> = w
            .into_iter()
            .map(|v| strongest(v.into_iter().enumerate().collect()))
            .collect();
        // A skeleton without skin (bones only) has no weights to paint.
        for (vertex, list) in painted.iter().filter(|_| n > 0) {
            if *vertex >= n {
                return Err("Painted weights name a vertex the avatar lacks".into());
            }
            let mut pairs = vec![];
            for (name, x) in list {
                let bone = bones
                    .iter()
                    .position(|b| &b.name == name)
                    .ok_or_else(|| format!("Painted weights name a missing bone {name}"))?;
                if !x.is_finite() || *x < 0. {
                    return Err("Painted weights must be positive".into());
                }
                pairs.push((bone, *x));
            }
            let pairs = strongest(pairs);
            if !pairs.is_empty() {
                weights[*vertex] = pairs;
            }
        }
        Ok(Self { bones, weights })
    }

    /// Each vertex's weights as (bone name, weight).
    pub fn named_weights(&self, vertex: usize) -> Vec<(String, f64)> {
        self.weights[vertex]
            .iter()
            .map(|(b, x)| (self.bones[*b].name.clone(), *x))
            .collect()
    }

    /// Each bone's transform from rest to posed, with bone rotations given by name.
    pub fn transforms(&self, rotation: impl Fn(&str) -> DQuat) -> Vec<DAffine3> {
        let mut out: Vec<DAffine3> = Vec::with_capacity(self.bones.len());
        for bone in &self.bones {
            let local = DAffine3::from_translation(bone.head)
                * DAffine3::from_quat(rotation(&bone.name))
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

/// Limbs inverse kinematics can move: the two bones, the end they bring to a target.
pub const LIMBS: [(&str, &str, &str); 4] = [
    ("Left hand", "upper_arm.L", "forearm.L"),
    ("Right hand", "upper_arm.R", "forearm.R"),
    ("Left foot", "thigh.L", "shin.L"),
    ("Right foot", "thigh.R", "shin.R"),
];

/// `q` as rotations in degrees about X, Y and Z, in the order poses use.
pub fn euler_degrees(q: DQuat) -> [f64; 3] {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

impl Rig {
    /// Where the end of the two-bone limb `upper → lower` is in `pose`.
    pub fn limb_end(&self, pose: &Pose, lower: &str) -> Option<DVec3> {
        let i = self.bone(lower)?;
        let t = self.transforms(|b| pose.rotation(b));
        Some(t[i].transform_point3(self.bones[i].tail))
    }

    /// `pose` with the limb `upper → lower` turned so its end reaches `target`, or points at
    /// it when it is out of reach. The limb keeps the plane it bends in.
    pub fn reach(
        &self,
        pose: &Pose,
        upper: &str,
        lower: &str,
        target: DVec3,
    ) -> Result<Pose, String> {
        let (u, l) = (
            self.bone(upper).ok_or("Unknown bone")?,
            self.bone(lower).ok_or("Unknown bone")?,
        );
        if self.bones[l].parent != Some(u) || !target.is_finite() {
            return Err("Reach needs a bone, its child and a finite target".into());
        }
        let t = self.transforms(|b| pose.rotation(b));
        let rot = |m: &DAffine3| DQuat::from_mat3(&m.matrix3).normalize();
        let shoulder = t[u].transform_point3(self.bones[u].head);
        let elbow = t[l].transform_point3(self.bones[l].head);
        let wrist = t[l].transform_point3(self.bones[l].tail);
        let (a, b) = (shoulder.distance(elbow), elbow.distance(wrist));
        let to = target - shoulder;
        let d = to.length().clamp((a - b).abs() + 1e-6, a + b - 1e-6);
        let dir = to.normalize_or_zero();
        if dir == DVec3::ZERO {
            return Err("The target is at the limb's root".into());
        }
        // The plane the limb bends in, and the side the joint bends to.
        let mut normal = (elbow - shoulder).cross(wrist - elbow);
        if normal.length_squared() < 1e-12 {
            normal = (wrist - shoulder).cross(DVec3::Z);
        }
        let side = normal.cross(dir).normalize_or_zero();
        let cos = ((a * a + d * d - b * b) / (2. * a * d)).clamp(-1., 1.);
        let new_elbow = shoulder + dir * (a * cos) + side * (a * (1. - cos * cos).sqrt());
        let new_wrist = shoulder + dir * d;
        // Turn the upper bone (in world space) so the elbow lands, then the lower one.
        let parent_rot = self.bones[u].parent.map_or(DQuat::IDENTITY, |p| rot(&t[p]));
        let delta = DQuat::from_rotation_arc(
            (elbow - shoulder).normalize(),
            (new_elbow - shoulder).normalize(),
        );
        let upper_local =
            (parent_rot.inverse() * delta * parent_rot * pose.rotation(upper)).normalize();
        let mut next = pose.clone();
        next.0.insert(upper.to_string(), euler_degrees(upper_local));
        let t2 = self.transforms(|b| next.rotation(b));
        let moved_wrist = t2[l].transform_point3(self.bones[l].tail);
        let moved_elbow = t2[l].transform_point3(self.bones[l].head);
        let upper_rot = rot(&t2[u]);
        let delta = DQuat::from_rotation_arc(
            (moved_wrist - moved_elbow).normalize(),
            (new_wrist - moved_elbow).normalize(),
        );
        let lower_local =
            (upper_rot.inverse() * delta * upper_rot * pose.rotation(lower)).normalize();
        next.0.insert(lower.to_string(), euler_degrees(lower_local));
        Ok(next)
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
