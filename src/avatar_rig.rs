//! The avatar's skeleton, skin weights and linear blend skinning.
//!
//! MakeHuman bodies (the bundled mannequin and every body built by [`crate::avatar_maker`]) receive
//! Anny's compact 104-bone skeleton placed from the body's own joint landmarks and vertices, paired
//! with MakeHuman/MPFB2's hand-painted skin weights (`assets/makehuman/anny-rig.bin`). Imported
//! non-MakeHuman avatars (OBJ, FBX, glTF, USD) keep the 19-bone automatic skeleton and
//! surface-smoothed nearest-bone weights.
//!
//! Legacy bone names from the 19-bone skeleton (`pelvis`, `spine`, `chest`, `neck`, `upper_arm.L`,
//! `forearm.L`, `hand.L`, `thigh.L`, `shin.L`, …) remain accepted everywhere: split long bones
//! keep swing at the proximal joint while sharing axial twist evenly across both segments, and
//! multi-segment chains (`spine`, `neck`) distribute their turn along the chain.
use crate::avatar::SurfaceMesh;
use glam::{DAffine3, DQuat, DVec3, EulerRot};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

const ANNY_RIG_PACKED: &[u8] = include_bytes!("../assets/makehuman/anny-rig.bin");

/// Legacy 19-bone skeleton: landmark ids, head and tail of each bone, and its parent, in an order
/// where parents come first. The pelvis is the root. Used for non-MakeHuman avatars and older
/// projects that only carry these 24 landmarks.
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

/// How a legacy 19-bone name maps onto the 104-bone Anny skeleton.
#[derive(Clone, Copy, Debug)]
enum LegacyTarget {
    /// One-to-one mapping to a single Anny bone.
    Single(&'static str),
    /// A rigid long bone split into proximal and distal halves (`upperarm01/02`, `lowerarm01/02`,
    /// `upperleg01/02`, `lowerleg01/02`): swing stays at the proximal joint while twist about the
    /// bone's axis is shared evenly between both segments.
    Split(&'static str, &'static str),
    /// A bendable multi-segment chain (`spine`, `neck`): the rotation is split equally along the
    /// chain by slerp.
    Chain(&'static [&'static str]),
}

/// Compatibility table from every legacy bone name to its Anny counterpart(s).
const LEGACY_TO_ANNY: &[(&str, LegacyTarget)] = &[
    ("pelvis", LegacyTarget::Single("root")),
    ("spine", LegacyTarget::Chain(&["spine03", "spine02"])),
    ("chest", LegacyTarget::Single("spine01")),
    ("neck", LegacyTarget::Chain(&["neck01", "neck02", "neck03"])),
    ("head", LegacyTarget::Single("head")),
    ("clavicle.L", LegacyTarget::Single("clavicle.L")),
    (
        "upper_arm.L",
        LegacyTarget::Split("upperarm01.L", "upperarm02.L"),
    ),
    (
        "forearm.L",
        LegacyTarget::Split("lowerarm01.L", "lowerarm02.L"),
    ),
    ("hand.L", LegacyTarget::Single("wrist.L")),
    ("clavicle.R", LegacyTarget::Single("clavicle.R")),
    (
        "upper_arm.R",
        LegacyTarget::Split("upperarm01.R", "upperarm02.R"),
    ),
    (
        "forearm.R",
        LegacyTarget::Split("lowerarm01.R", "lowerarm02.R"),
    ),
    ("hand.R", LegacyTarget::Single("wrist.R")),
    (
        "thigh.L",
        LegacyTarget::Split("upperleg01.L", "upperleg02.L"),
    ),
    (
        "shin.L",
        LegacyTarget::Split("lowerleg01.L", "lowerleg02.L"),
    ),
    ("foot.L", LegacyTarget::Single("foot.L")),
    (
        "thigh.R",
        LegacyTarget::Split("upperleg01.R", "upperleg02.R"),
    ),
    (
        "shin.R",
        LegacyTarget::Split("lowerleg01.R", "lowerleg02.R"),
    ),
    ("foot.R", LegacyTarget::Single("foot.R")),
];

/// Smoothing passes over the surface for automatic weights: wider blends at the joints with more
/// passes.
const SMOOTHING_PASSES: usize = 24;
/// Most bones one vertex follows. Raised from 4 to 8 for Anny's hand-painted weights: truncating
/// to 4 drops up to 22 % weight on 1,162 torso/shoulder/wrist vertices, whereas 8 drops at most
/// 0.1 % on 2 vertices across the 13,380-vertex body.
pub const INFLUENCES: usize = 8;

/// How one end (head or tail) of an Anny bone is placed on a body.
enum Endpoint {
    /// Centre of a MakeHuman joint-helper cube (directly from the body's landmarks).
    Cube(String),
    /// A single body vertex (exact when the surface is present), with a landmark-frame fallback
    /// for bones-only evaluation.
    Vertex {
        vertex: usize,
        origin: String,
        a: String,
        b: String,
        up: String,
        offset: [f64; 3],
    },
    /// Midpoint of two body vertices, with a landmark interpolation fallback when surface-less.
    MeanBody {
        v0: usize,
        v1: usize,
        lm_a: String,
        lm_b: String,
        t: f64,
    },
    /// Midpoint of one body vertex and one non-body helper vertex tracked via a local body
    /// triangle frame `(u, v, h)`, with a landmark interpolation fallback when surface-less.
    MeanBodyHelper {
        v_body: usize,
        tri: [usize; 3],
        uvh: [f64; 3],
        lm_a: String,
        lm_b: String,
        t: f64,
    },
}

impl Endpoint {
    fn collect_landmarks<'a>(&'a self, out: &mut BTreeSet<&'a str>) {
        match self {
            Endpoint::Cube(name) => {
                out.insert(name.as_str());
            }
            Endpoint::Vertex {
                origin, a, b, up, ..
            } => {
                out.insert(origin.as_str());
                out.insert(a.as_str());
                out.insert(b.as_str());
                out.insert(up.as_str());
            }
            Endpoint::MeanBody { lm_a, lm_b, .. } | Endpoint::MeanBodyHelper { lm_a, lm_b, .. } => {
                out.insert(lm_a.as_str());
                out.insert(lm_b.as_str());
            }
        }
    }

    fn eval(
        &self,
        landmarks: &BTreeMap<String, DVec3>,
        surface: Option<&[DVec3]>,
    ) -> Result<DVec3, String> {
        let at = |id: &str| {
            landmarks
                .get(id)
                .copied()
                .ok_or_else(|| format!("The avatar has no {id} landmark; it cannot be posed"))
        };
        match self {
            Endpoint::Cube(name) => at(name),
            Endpoint::Vertex {
                vertex,
                origin,
                a,
                b,
                up,
                offset,
            } => {
                if let Some(verts) = surface
                    && *vertex < verts.len()
                {
                    return Ok(verts[*vertex]);
                }
                let o = at(origin)?;
                let (pa, pb, pu) = (at(a)?, at(b)?, at(up)?);
                let width = (pa - pb).length();
                if width < 1e-9 {
                    return Ok(o);
                }
                let x_axis = (pa - pb) / width;
                let y_raw = pu - o;
                let y_axis = (y_raw - x_axis * y_raw.dot(x_axis)).normalize_or_zero();
                if y_axis == DVec3::ZERO {
                    return Ok(o);
                }
                let z_axis = x_axis.cross(y_axis);
                Ok(o + (x_axis * offset[0] + y_axis * offset[1] + z_axis * offset[2]) * width)
            }
            Endpoint::MeanBody {
                v0,
                v1,
                lm_a,
                lm_b,
                t,
            } => {
                if let Some(verts) = surface
                    && *v0 < verts.len()
                    && *v1 < verts.len()
                {
                    return Ok((verts[*v0] + verts[*v1]) * 0.5);
                }
                Ok(at(lm_a)?.lerp(at(lm_b)?, *t))
            }
            Endpoint::MeanBodyHelper {
                v_body,
                tri,
                uvh,
                lm_a,
                lm_b,
                t,
            } => {
                if let Some(verts) = surface
                    && *v_body < verts.len()
                    && tri.iter().all(|&i| i < verts.len())
                {
                    let (a, b, c) = (verts[tri[0]], verts[tri[1]], verts[tri[2]]);
                    let (e1, e2) = (b - a, c - a);
                    let n = e1.cross(e2);
                    let area2 = n.length();
                    let helper = if area2 > 1e-18 {
                        a + e1 * uvh[0] + e2 * uvh[1] + (n / area2) * (uvh[2] * area2.sqrt())
                    } else {
                        a
                    };
                    return Ok((verts[*v_body] + helper) * 0.5);
                }
                Ok(at(lm_a)?.lerp(at(lm_b)?, *t))
            }
        }
    }
}

struct AnnyBoneDef {
    name: String,
    parent: Option<usize>,
    head: Endpoint,
    tail: Endpoint,
}

struct AnnyRigData {
    bones: Vec<AnnyBoneDef>,
    weights: Vec<Vec<(usize, f64)>>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.bytes.len())
            .ok_or("The Anny rig asset ends early")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn name(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| "An Anny rig name is not valid UTF-8".into())
    }
    fn endpoint(&mut self, vertex_count: usize) -> Result<Endpoint, String> {
        match self.u8()? {
            0 => Ok(Endpoint::Cube(self.name()?)),
            1 => {
                let vertex = self.u32()? as usize;
                if vertex >= vertex_count {
                    return Err("Anny rig vertex endpoint out of range".into());
                }
                let origin = self.name()?;
                let a = self.name()?;
                let b = self.name()?;
                let up = self.name()?;
                let offset = [self.f32()? as f64, self.f32()? as f64, self.f32()? as f64];
                Ok(Endpoint::Vertex {
                    vertex,
                    origin,
                    a,
                    b,
                    up,
                    offset,
                })
            }
            2 => {
                let v0 = self.u32()? as usize;
                let v1 = self.u32()? as usize;
                if v0 >= vertex_count || v1 >= vertex_count {
                    return Err("Anny rig mean endpoint out of range".into());
                }
                let lm_a = self.name()?;
                let lm_b = self.name()?;
                let t = self.f32()? as f64;
                Ok(Endpoint::MeanBody {
                    v0,
                    v1,
                    lm_a,
                    lm_b,
                    t,
                })
            }
            3 => {
                let v_body = self.u32()? as usize;
                let tri = [
                    self.u32()? as usize,
                    self.u32()? as usize,
                    self.u32()? as usize,
                ];
                if v_body >= vertex_count || tri.iter().any(|&i| i >= vertex_count) {
                    return Err("Anny rig helper-frame endpoint out of range".into());
                }
                let uvh = [self.f32()? as f64, self.f32()? as f64, self.f32()? as f64];
                let lm_a = self.name()?;
                let lm_b = self.name()?;
                let t = self.f32()? as f64;
                Ok(Endpoint::MeanBodyHelper {
                    v_body,
                    tri,
                    uvh,
                    lm_a,
                    lm_b,
                    t,
                })
            }
            tag => Err(format!("Unknown Anny rig endpoint tag {tag}")),
        }
    }
}

fn parse_anny_rig(packed: &[u8]) -> Result<AnnyRigData, String> {
    let bytes = miniz_oxide::inflate::decompress_to_vec_zlib(packed)
        .map_err(|e| format!("The Anny rig asset cannot be unpacked: {e:?}"))?;
    let mut r = Reader {
        bytes: &bytes,
        at: 0,
    };
    if r.take(4)? != b"FCAR" || r.u32()? != 1 {
        return Err("Not a FashionCraft Anny rig asset of a known version".into());
    }
    let bone_count = r.u32()? as usize;
    let vertex_count = r.u32()? as usize;
    let max_inf = r.u8()? as usize;
    if bone_count == 0 || bone_count > 255 || vertex_count != crate::avatar_maker::BODY_VERTICES {
        return Err("Unexpected bone or vertex count in Anny rig asset".into());
    }
    if max_inf == 0 || max_inf > INFLUENCES {
        return Err("Unexpected influence count in Anny rig asset".into());
    }
    let mut bones = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let name = r.name()?;
        let parent_raw = r.i16()?;
        let parent = if parent_raw < 0 {
            None
        } else {
            let p = parent_raw as usize;
            if p >= i {
                return Err(format!("Anny rig bone {name} does not follow its parent"));
            }
            Some(p)
        };
        let head = r.endpoint(vertex_count)?;
        let tail = r.endpoint(vertex_count)?;
        bones.push(AnnyBoneDef {
            name,
            parent,
            head,
            tail,
        });
    }
    let mut weights = Vec::with_capacity(vertex_count);
    for v in 0..vertex_count {
        let count = r.u8()? as usize;
        if count == 0 || count > INFLUENCES {
            return Err(format!("Anny rig vertex {v} has {count} weights"));
        }
        let mut pairs = Vec::with_capacity(count);
        let mut sum = 0.0;
        for _ in 0..count {
            let b = r.u8()? as usize;
            let w = r.f32()? as f64;
            if b >= bone_count || !w.is_finite() || w < 0.0 {
                return Err(format!("Anny rig vertex {v} has an invalid weight"));
            }
            sum += w;
            pairs.push((b, w));
        }
        if sum <= 1e-12 {
            return Err(format!("Anny rig vertex {v} weights sum to zero"));
        }
        for (_, w) in &mut pairs {
            *w /= sum;
        }
        weights.push(pairs);
    }
    if r.at != bytes.len() {
        return Err("The Anny rig asset has trailing data".into());
    }
    Ok(AnnyRigData { bones, weights })
}

fn anny_rig() -> &'static AnnyRigData {
    static BUNDLED: OnceLock<AnnyRigData> = OnceLock::new();
    BUNDLED.get_or_init(|| parse_anny_rig(ANNY_RIG_PACKED).expect("bundled anny-rig.bin is valid"))
}

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

/// Whether `name` is a built-in bone of either the Anny 104-bone rig or the legacy 19-bone rig.
pub fn is_builtin_bone(name: &str) -> bool {
    BONES.iter().any(|b| b.0 == name) || anny_rig().bones.iter().any(|b| b.name == name)
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
    /// Angles in range, on built-in bones only (Anny or legacy names).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(bone) = self.0.keys().find(|b| !is_builtin_bone(b)) {
            return Err(format!("Unknown bone {bone}"));
        }
        self.validate_angles()
    }

    /// Angles in range, on bones that are built in or could have been added (the skeleton
    /// they are used with checks that it has them).
    pub fn validate_angles(&self) -> Result<(), String> {
        for (bone, angles) in &self.0 {
            if !is_builtin_bone(bone) && !valid_bone_name(bone) {
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
        let names: BTreeSet<&str> = BONES
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

/// Minimum landmark ids needed for the 19-bone automatic skeleton (non-MakeHuman avatars).
pub fn joint_ids() -> BTreeSet<&'static str> {
    BONES.iter().flat_map(|b| [b.1, b.2]).collect()
}

/// All landmark ids needed to place the 104-bone Anny skeleton (a superset of [`joint_ids`]).
pub fn anny_joint_ids() -> BTreeSet<&'static str> {
    let mut out = joint_ids();
    for bone in &anny_rig().bones {
        bone.head.collect_landmarks(&mut out);
        bone.tail.collect_landmarks(&mut out);
    }
    out
}

/// Whether `landmarks` contains every joint landmark required by the 104-bone Anny skeleton.
pub fn has_anny_joints<T>(landmarks: &BTreeMap<String, T>) -> bool {
    anny_joint_ids()
        .iter()
        .all(|id| landmarks.contains_key(*id))
}

/// Names of the 104 Anny bones, parents first.
pub fn bone_names() -> impl Iterator<Item = &'static str> {
    anny_rig().bones.iter().map(|b| b.name.as_str())
}

/// Names of the 19 legacy bones, parents first.
pub fn legacy_bone_names() -> impl Iterator<Item = &'static str> {
    BONES.iter().map(|b| b.0)
}

/// Built-in bone names for a skeleton built from `joints`: the 104 Anny bones when MakeHuman's
/// full joint landmarks are present, else the 19 legacy bones.
pub fn built_in_bone_names<T>(joints: &BTreeMap<String, T>) -> Vec<&'static str> {
    if has_anny_joints(joints) {
        bone_names().collect()
    } else {
        legacy_bone_names().collect()
    }
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

/// Surface vertex neighbours for smoothing weights along the mesh topology.
fn mesh_neighbours(surface: &SurfaceMesh) -> Vec<Vec<usize>> {
    let mut neighbours = vec![vec![]; surface.vertices.len()];
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
    neighbours
}

/// Smooth a per-vertex weight table along `neighbours` for [`SMOOTHING_PASSES`] passes.
fn smooth_weights(mut w: Vec<Vec<f64>>, neighbours: &[Vec<usize>]) -> Vec<Vec<f64>> {
    let n = w.len();
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
    w
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

/// Decompose `q` into `swing * twist`, where `twist` is a shortest-arc rotation about unit `axis`.
fn swing_twist(q: DQuat, axis: DVec3) -> (DQuat, DQuat) {
    let q = if q.w < 0. { -q } else { q }.normalize();
    let axis = axis.normalize_or_zero();
    if axis == DVec3::ZERO {
        return (q, DQuat::IDENTITY);
    }
    let v = DVec3::new(q.x, q.y, q.z);
    let proj = axis * v.dot(axis);
    let raw = DQuat::from_xyzw(proj.x, proj.y, proj.z, q.w);
    let twist = if raw.length_squared() > 1e-18 {
        let t = raw.normalize();
        if t.w < 0. { -t } else { t }
    } else {
        DQuat::IDENTITY
    };
    let swing = (q * twist.conjugate()).normalize();
    (swing, twist)
}

fn is_identity_quat(q: DQuat) -> bool {
    (q.w.abs() - 1.0).abs() < 1e-12 && q.x * q.x + q.y * q.y + q.z * q.z < 1e-24
}

fn legacy_target(name: &str) -> Option<LegacyTarget> {
    LEGACY_TO_ANNY
        .iter()
        .find_map(|(k, target)| (*k == name).then_some(*target))
}

fn resolve_bone_in(bones: &[Bone], is_anny: bool, name: &str) -> Option<usize> {
    if let Some(i) = bones.iter().position(|b| b.name == name) {
        return Some(i);
    }
    if !is_anny {
        return None;
    }
    let primary = match legacy_target(name)? {
        LegacyTarget::Single(b) => b,
        LegacyTarget::Split(a, _) => a,
        LegacyTarget::Chain(list) => *list.first()?,
    };
    bones.iter().position(|b| b.name == primary)
}

fn append_added_bones(
    bones: &mut Vec<Bone>,
    is_anny: bool,
    added: &[AddedBone],
) -> Result<(), String> {
    for bone in added {
        if !valid_bone_name(&bone.name) || bones.iter().any(|b| b.name == bone.name) {
            return Err(format!(
                "A bone's name must be new and made of letters, digits, '_', '.' or '-': {}",
                bone.name
            ));
        }
        let parent = resolve_bone_in(bones, is_anny, &bone.parent).ok_or_else(|| {
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
    Ok(())
}

fn apply_painted_weights(
    bones: &[Bone],
    is_anny: bool,
    weights: &mut [Vec<(usize, f64)>],
    painted: &PaintedWeights,
) -> Result<(), String> {
    let n = weights.len();
    // A skeleton without skin (bones only) has no weights to paint.
    for (vertex, list) in painted.iter().filter(|_| n > 0) {
        if *vertex >= n {
            return Err("Painted weights name a vertex the avatar lacks".into());
        }
        let mut pairs = vec![];
        for (name, x) in list {
            let bone = resolve_bone_in(bones, is_anny, name)
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
    Ok(())
}

impl Rig {
    /// Build the rig of `surface` from its joint landmarks: Anny's 104-bone rig with hand-painted
    /// weights for MakeHuman bodies, or the 19-bone automatic rig for imported non-MakeHuman
    /// meshes.
    pub fn new(landmarks: &BTreeMap<String, DVec3>, surface: &SurfaceMesh) -> Result<Self, String> {
        Self::build(landmarks, surface, &[], &PaintedWeights::new())
    }

    /// [`Rig::new`] with `added` bones after the built-in ones and `painted` weights replacing the
    /// base weights where given.
    pub fn build(
        landmarks: &BTreeMap<String, DVec3>,
        surface: &SurfaceMesh,
        added: &[AddedBone],
        painted: &PaintedWeights,
    ) -> Result<Self, String> {
        let use_anny = has_anny_joints(landmarks)
            && (surface.vertices.is_empty() || crate::avatar_maker::is_makehuman_body(surface));
        if use_anny {
            Self::build_anny(landmarks, surface, added, painted)
        } else {
            Self::build_automatic(landmarks, surface, added, painted)
        }
    }

    /// Build the 104-bone Anny rig with MakeHuman/MPFB2 hand-painted skin weights.
    fn build_anny(
        landmarks: &BTreeMap<String, DVec3>,
        surface: &SurfaceMesh,
        added: &[AddedBone],
        painted: &PaintedWeights,
    ) -> Result<Self, String> {
        let data = anny_rig();
        let verts = (!surface.vertices.is_empty()).then_some(surface.vertices.as_slice());
        let mut bones = Vec::with_capacity(data.bones.len() + added.len());
        for def in &data.bones {
            bones.push(Bone {
                name: def.name.clone(),
                parent: def.parent,
                head: def.head.eval(landmarks, verts)?,
                tail: def.tail.eval(landmarks, verts)?,
            });
        }
        let base_count = bones.len();
        append_added_bones(&mut bones, true, added)?;

        let n = surface.vertices.len();
        let mut weights = if n == 0 {
            Vec::new()
        } else {
            let mut w = data.weights.clone();
            if !added.is_empty() {
                // Blend added bones smoothly over the hand-painted base weights where they are the
                // nearest bone on the surface; distant vertices keep their Anny weights untouched.
                let k_added = added.len();
                let mut added_w: Vec<Vec<f64>> = surface
                    .vertices
                    .iter()
                    .map(|p| {
                        let nearest = (0..bones.len())
                            .min_by(|&a, &b| {
                                segment_distance(*p, bones[a].head, bones[a].tail)
                                    .total_cmp(&segment_distance(*p, bones[b].head, bones[b].tail))
                            })
                            .unwrap();
                        let mut row = vec![0.; k_added];
                        if nearest >= base_count {
                            row[nearest - base_count] = 1.;
                        }
                        row
                    })
                    .collect();
                let neighbours = mesh_neighbours(surface);
                added_w = smooth_weights(added_w, &neighbours);
                for (v, aw) in added_w.into_iter().enumerate() {
                    let s: f64 = aw.iter().sum::<f64>().min(1.);
                    if s <= 1e-6 {
                        continue;
                    }
                    let mut combined: Vec<(usize, f64)> = w[v]
                        .iter()
                        .map(|&(b, x)| (b, x * (1. - s)))
                        .chain(
                            aw.into_iter()
                                .enumerate()
                                .filter(|(_, x)| *x > 1e-6)
                                .map(|(k, x)| (base_count + k, x)),
                        )
                        .collect();
                    combined = strongest(combined);
                    if !combined.is_empty() {
                        w[v] = combined;
                    }
                }
            }
            w
        };
        apply_painted_weights(&bones, true, &mut weights, painted)?;
        Ok(Self { bones, weights })
    }

    /// Build the legacy 19-bone skeleton with nearest-bone surface-smoothed automatic weights.
    /// Used for non-MakeHuman avatars, older projects with only the 24 legacy joints, and
    /// regression comparisons against the Anny rig.
    pub fn build_automatic(
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
        append_added_bones(&mut bones, false, added)?;
        // Nearest bone, as a one-hot weight vector.
        let w: Vec<Vec<f64>> = surface
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
        let neighbours = mesh_neighbours(surface);
        let w = smooth_weights(w, &neighbours);
        let mut weights: Vec<Vec<(usize, f64)>> = w
            .into_iter()
            .map(|v| strongest(v.into_iter().enumerate().collect()))
            .collect();
        apply_painted_weights(&bones, false, &mut weights, painted)?;
        Ok(Self { bones, weights })
    }

    /// Whether this rig uses the 104-bone Anny skeleton rather than the 19-bone automatic one.
    pub fn is_anny(&self) -> bool {
        self.exact_bone("upperarm01.L").is_some()
    }

    fn exact_bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Each vertex's weights as (bone name, weight).
    pub fn named_weights(&self, vertex: usize) -> Vec<(String, f64)> {
        self.weights[vertex]
            .iter()
            .map(|(b, x)| (self.bones[*b].name.clone(), *x))
            .collect()
    }

    /// Resolve per-bone local rotations from `rotation`, distributing any legacy bone names onto
    /// their Anny counterparts (swing at the proximal joint and split twist for long bones, equal
    /// slerp along `spine` and `neck`).
    fn local_rotations(&self, rotation: impl Fn(&str) -> DQuat) -> Vec<DQuat> {
        let mut local: Vec<DQuat> = self.bones.iter().map(|b| rotation(&b.name)).collect();
        if self.is_anny() {
            for &(legacy, target) in LEGACY_TO_ANNY {
                if self.exact_bone(legacy).is_some() {
                    continue;
                }
                let q = rotation(legacy);
                if is_identity_quat(q) {
                    continue;
                }
                match target {
                    LegacyTarget::Single(name) => {
                        if let Some(i) = self.exact_bone(name) {
                            local[i] = (local[i] * q).normalize();
                        }
                    }
                    LegacyTarget::Split(a, b) => {
                        if let (Some(ia), Some(ib)) = (self.exact_bone(a), self.exact_bone(b)) {
                            let axis =
                                (self.bones[ib].tail - self.bones[ia].head).normalize_or_zero();
                            let (swing, twist) = swing_twist(q, axis);
                            let half = DQuat::IDENTITY.slerp(twist, 0.5);
                            local[ia] = (local[ia] * swing * half).normalize();
                            local[ib] = (local[ib] * half).normalize();
                        }
                    }
                    LegacyTarget::Chain(names) => {
                        let q = if q.w < 0. { -q } else { q }.normalize();
                        let step = DQuat::IDENTITY.slerp(q, 1.0 / names.len() as f64);
                        for &name in names {
                            if let Some(i) = self.exact_bone(name) {
                                local[i] = (local[i] * step).normalize();
                            }
                        }
                    }
                }
            }
        }
        local
    }

    /// Each bone's transform from rest to posed, with bone rotations given by name.
    pub fn transforms(&self, rotation: impl Fn(&str) -> DQuat) -> Vec<DAffine3> {
        let local_rots = self.local_rotations(rotation);
        let mut out: Vec<DAffine3> = Vec::with_capacity(self.bones.len());
        for (bone, q) in self.bones.iter().zip(local_rots) {
            let local = DAffine3::from_translation(bone.head)
                * DAffine3::from_quat(q)
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

    /// Index of bone `name`, resolving legacy 19-bone names to their primary Anny bone when this
    /// rig uses the Anny skeleton.
    pub fn bone(&self, name: &str) -> Option<usize> {
        resolve_bone_in(&self.bones, self.is_anny(), name)
    }

    /// `(first, last)` bone indices spanned by `name`. For split long bones (`upper_arm.L`,
    /// `upperarm01.L`, `forearm.L`, `lowerarm01.L`, `thigh.L`, `shin.L`, …) this returns the
    /// proximal and distal halves so `bones[first].head` is the proximal joint and
    /// `bones[last].tail` is the distal joint; for chains (`spine`, `neck`) it spans the whole
    /// chain; for single bones it returns `(i, i)`.
    pub fn bone_span(&self, name: &str) -> Option<(usize, usize)> {
        if self.is_anny() {
            if let Some(target) = legacy_target(name) {
                return match target {
                    LegacyTarget::Single(b) => self.exact_bone(b).map(|i| (i, i)),
                    LegacyTarget::Split(a, b) => Some((self.exact_bone(a)?, self.exact_bone(b)?)),
                    LegacyTarget::Chain(list) => Some((
                        self.exact_bone(*list.first()?)?,
                        self.exact_bone(*list.last()?)?,
                    )),
                };
            }
            for &(_, target) in LEGACY_TO_ANNY {
                if let LegacyTarget::Split(a, b) = target
                    && a == name
                {
                    return Some((self.exact_bone(a)?, self.exact_bone(b)?));
                }
            }
        }
        self.exact_bone(name).map(|i| (i, i))
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
        let (_, l_end) = self.bone_span(lower)?;
        let t = self.transforms(|b| pose.rotation(b));
        Some(t[l_end].transform_point3(self.bones[l_end].tail))
    }

    /// `pose` with the limb `upper → lower` turned so its end reaches `target`, or points at
    /// it when it is out of reach. The limb keeps the plane it bends in. Works for both unsplit
    /// 19-bone limbs and split Anny limbs (`upperarm01/02 → lowerarm01/02`,
    /// `upperleg01/02 → lowerleg01/02`).
    pub fn reach(
        &self,
        pose: &Pose,
        upper: &str,
        lower: &str,
        target: DVec3,
    ) -> Result<Pose, String> {
        let (u_start, u_end) = self.bone_span(upper).ok_or("Unknown bone")?;
        let (l_start, l_end) = self.bone_span(lower).ok_or("Unknown bone")?;
        if self.bones[l_start].parent != Some(u_end) || !target.is_finite() {
            return Err("Reach needs a bone, its child and a finite target".into());
        }
        let local = self.local_rotations(|b| pose.rotation(b));
        let t = self.transforms(|b| pose.rotation(b));
        let rot = |m: &DAffine3| DQuat::from_mat3(&m.matrix3).normalize();
        let shoulder = t[u_start].transform_point3(self.bones[u_start].head);
        let elbow = t[l_start].transform_point3(self.bones[l_start].head);
        let wrist = t[l_end].transform_point3(self.bones[l_end].tail);
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
        let parent_rot = self.bones[u_start]
            .parent
            .map_or(DQuat::IDENTITY, |p| rot(&t[p]));
        let delta = DQuat::from_rotation_arc(
            (elbow - shoulder).normalize(),
            (new_elbow - shoulder).normalize(),
        );
        let upper_local = (parent_rot.inverse() * delta * parent_rot * local[u_start]).normalize();
        let mut next = pose.clone();
        // If the input pose used legacy keys on an Anny rig, materialise them onto the Anny bones
        // so the legacy entry does not double-apply alongside the IK result.
        if self.is_anny() {
            if self.bones[u_start].name != upper
                && next.0.remove(upper).is_some()
                && u_end != u_start
                && !is_identity_quat(local[u_end])
            {
                next.0
                    .insert(self.bones[u_end].name.clone(), euler_degrees(local[u_end]));
            }
            if self.bones[l_start].name != lower && next.0.remove(lower).is_some() {
                next.0.insert(
                    self.bones[l_start].name.clone(),
                    euler_degrees(local[l_start]),
                );
                if l_end != l_start && !is_identity_quat(local[l_end]) {
                    next.0
                        .insert(self.bones[l_end].name.clone(), euler_degrees(local[l_end]));
                }
            }
        }
        next.0
            .insert(self.bones[u_start].name.clone(), euler_degrees(upper_local));
        let t2 = self.transforms(|b| next.rotation(b));
        let moved_wrist = t2[l_end].transform_point3(self.bones[l_end].tail);
        let moved_elbow = t2[l_start].transform_point3(self.bones[l_start].head);
        let upper_rot = rot(&t2[u_end]);
        let delta = DQuat::from_rotation_arc(
            (moved_wrist - moved_elbow).normalize(),
            (new_wrist - moved_elbow).normalize(),
        );
        let lower_local = (upper_rot.inverse() * delta * upper_rot * local[l_start]).normalize();
        next.0
            .insert(self.bones[l_start].name.clone(), euler_degrees(lower_local));
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
