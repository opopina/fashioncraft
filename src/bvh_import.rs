//! BVH motion import: a skeleton's motion from a BVH file, retargeted onto the avatar's rig.
//!
//! The file's joints are matched to the rig's bones by their usual names (Hips, Spine,
//! LeftArm, LeftForeArm, LeftUpLeg… in the common naming, a `prefix:` dropped). The pelvis
//! takes the hips' turn as it is; every other bone is swung so that it points where the
//! file's matching bone points at that frame, from its parent's new place, so the motion
//! carries over between skeletons of other proportions and rest poses. Bone twist and the
//! hips' travel are not carried over: the avatar turns and bends in place. A file facing the
//! other way, or sideways, is turned so its left side is the avatar's left.
use crate::{
    avatar_animation::{AvatarAnimation, PoseKey},
    avatar_rig::{Pose, Rig, euler_degrees},
};
use glam::{DQuat, DVec3};
use std::collections::BTreeMap;

/// Most joints and frames a file may have.
pub const MAX_JOINTS: usize = 512;
pub const MAX_FRAMES: usize = 200_000;
/// Keys made per second of motion, at most.
pub const KEYS_PER_SECOND: f64 = 30.;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Channel {
    X,
    Y,
    Z,
    RotX,
    RotY,
    RotZ,
}

#[derive(Clone, Debug)]
struct Joint {
    name: String,
    parent: Option<usize>,
    offset: DVec3,
    channels: Vec<Channel>,
    /// Index of this joint's first channel in a frame's values.
    first: usize,
    /// Offset of the joint's end site, when it has one.
    end: Option<DVec3>,
}

/// A parsed BVH file.
#[derive(Clone, Debug)]
pub struct Bvh {
    joints: Vec<Joint>,
    pub frame_time: f64,
    frames: Vec<Vec<f64>>,
}

/// One joint's name in a common form: lower case, letters and digits, any `prefix:` dropped.
fn plain(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or(name)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

pub fn parse(text: &str) -> Result<Bvh, String> {
    let mut words = text.split_whitespace().peekable();
    let mut next = |what: &str| {
        words
            .next()
            .ok_or_else(|| format!("BVH ends early: {what} expected"))
    };
    if next("HIERARCHY")? != "HIERARCHY" {
        return Err("Not a BVH file: it does not start with HIERARCHY".into());
    }
    let mut joints: Vec<Joint> = Vec::new();
    let mut stack: Vec<Option<usize>> = Vec::new();
    let mut channels = 0usize;
    let number = |w: &str| {
        w.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("BVH number expected, found {w}"))
    };
    loop {
        let word = next("a joint or MOTION")?;
        match word {
            "ROOT" | "JOINT" => {
                let name = next("a joint name")?.to_string();
                if next("{")? != "{" {
                    return Err(format!("BVH joint {name}: {{ expected"));
                }
                if next("OFFSET")? != "OFFSET" {
                    return Err(format!("BVH joint {name}: OFFSET expected"));
                }
                let offset = DVec3::new(
                    number(next("x")?)?,
                    number(next("y")?)?,
                    number(next("z")?)?,
                );
                if next("CHANNELS")? != "CHANNELS" {
                    return Err(format!("BVH joint {name}: CHANNELS expected"));
                }
                let count: usize = next("a channel count")?
                    .parse()
                    .map_err(|_| format!("BVH joint {name}: bad channel count"))?;
                if count > 6 {
                    return Err(format!("BVH joint {name}: at most 6 channels"));
                }
                let mut list = Vec::with_capacity(count);
                for _ in 0..count {
                    list.push(match next("a channel")? {
                        "Xposition" => Channel::X,
                        "Yposition" => Channel::Y,
                        "Zposition" => Channel::Z,
                        "Xrotation" => Channel::RotX,
                        "Yrotation" => Channel::RotY,
                        "Zrotation" => Channel::RotZ,
                        other => return Err(format!("BVH joint {name}: unknown channel {other}")),
                    });
                }
                let parent = stack.last().copied().flatten();
                if word == "ROOT" && !joints.is_empty() {
                    return Err("BVH files with several roots are not read".into());
                }
                joints.push(Joint {
                    name,
                    parent,
                    offset,
                    channels: list,
                    first: channels,
                    end: None,
                });
                channels += count;
                if joints.len() > MAX_JOINTS {
                    return Err(format!("BVH skeleton exceeds {MAX_JOINTS} joints"));
                }
                stack.push(Some(joints.len() - 1));
            }
            "End" => {
                next("Site")?;
                if next("{")? != "{" || next("OFFSET")? != "OFFSET" {
                    return Err("BVH end site: { OFFSET expected".into());
                }
                let end = DVec3::new(
                    number(next("x")?)?,
                    number(next("y")?)?,
                    number(next("z")?)?,
                );
                if next("}")? != "}" {
                    return Err("BVH end site: } expected".into());
                }
                if let Some(Some(owner)) = stack.last() {
                    joints[*owner].end = Some(end);
                }
            }
            "}" => {
                stack
                    .pop()
                    .ok_or("BVH hierarchy closes more braces than it opens")?;
            }
            "MOTION" => break,
            other => return Err(format!("BVH: unexpected {other} in the hierarchy")),
        }
    }
    if joints.is_empty() || !stack.is_empty() {
        return Err("BVH hierarchy is empty or unbalanced".into());
    }
    if next("Frames:")? != "Frames:" {
        return Err("BVH motion: Frames: expected".into());
    }
    let count: usize = next("a frame count")?
        .parse()
        .map_err(|_| "BVH motion: bad frame count")?;
    if count == 0 || count > MAX_FRAMES {
        return Err(format!("BVH motion needs 1 to {MAX_FRAMES} frames"));
    }
    if next("Frame")? != "Frame" || next("Time:")? != "Time:" {
        return Err("BVH motion: Frame Time: expected".into());
    }
    let frame_time = number(next("a frame time")?)?;
    if !(1e-4..=10.).contains(&frame_time) {
        return Err("BVH motion: the frame time must be between 0.0001 and 10 s".into());
    }
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let mut values = Vec::with_capacity(channels);
        for _ in 0..channels {
            values.push(number(next("a channel value")?)?);
        }
        frames.push(values);
    }
    Ok(Bvh {
        joints,
        frame_time,
        frames,
    })
}

impl Bvh {
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Each joint's world rotation and position at `frame`, or at rest (every channel zero).
    fn pose(&self, frame: Option<usize>) -> (Vec<DQuat>, Vec<DVec3>) {
        let mut rotations: Vec<DQuat> = Vec::with_capacity(self.joints.len());
        let mut positions: Vec<DVec3> = Vec::with_capacity(self.joints.len());
        for joint in &self.joints {
            let mut local = DQuat::IDENTITY;
            let mut shift = DVec3::ZERO;
            for (k, channel) in joint.channels.iter().enumerate() {
                let v = frame.map_or(0., |f| self.frames[f][joint.first + k]);
                match channel {
                    Channel::X => shift.x = v,
                    Channel::Y => shift.y = v,
                    Channel::Z => shift.z = v,
                    Channel::RotX => local *= DQuat::from_rotation_x(v.to_radians()),
                    Channel::RotY => local *= DQuat::from_rotation_y(v.to_radians()),
                    Channel::RotZ => local *= DQuat::from_rotation_z(v.to_radians()),
                }
            }
            // A root's position channels place it; elsewhere they replace the offset.
            let offset = if joint
                .channels
                .iter()
                .any(|c| matches!(c, Channel::X | Channel::Y | Channel::Z))
                && joint.parent.is_some()
            {
                shift
            } else {
                joint.offset
            };
            let (r, p) = match joint.parent {
                Some(parent) => (
                    rotations[parent] * local,
                    positions[parent] + rotations[parent] * offset,
                ),
                // The hips' travel is left out: the avatar moves in place.
                None => (local, joint.offset),
            };
            rotations.push(r);
            positions.push(p);
        }
        (rotations, positions)
    }

    fn find(&self, names: &[&str]) -> Option<usize> {
        names
            .iter()
            .find_map(|n| self.joints.iter().position(|j| plain(&j.name) == *n))
    }

    /// Where joint `k`'s bone ends: its first child joint, else its end site.
    fn tip(&self, k: usize, rotations: &[DQuat], positions: &[DVec3]) -> Option<DVec3> {
        match self.joints.iter().position(|j| j.parent == Some(k)) {
            Some(child) => Some(positions[child]),
            None => self.joints[k]
                .end
                .map(|end| positions[k] + rotations[k] * end),
        }
    }
}

/// The file's joints for each rig bone: where the bone starts and, when it is not the next
/// joint down, where it ends.
fn matches(bvh: &Bvh) -> BTreeMap<&'static str, (usize, Option<usize>)> {
    let mut out = BTreeMap::new();
    let mut add = |bone: &'static str, from: &[&str], to: &[&str]| {
        if let Some(a) = bvh.find(from) {
            let b = bvh.find(to).filter(|b| *b != a);
            out.insert(bone, (a, b));
        }
    };
    add("pelvis", &["hips", "pelvis", "root"], &[]);
    add(
        "spine",
        &["spine", "abdomen", "lowerback", "spine1"],
        &["spine2", "chest", "upperback", "thorax", "spine1"],
    );
    add(
        "chest",
        &["spine2", "chest", "upperback", "thorax"],
        &["neck", "neck1", "lowerneck"],
    );
    add("neck", &["neck", "neck1", "lowerneck"], &["head"]);
    add("head", &["head"], &[]);
    const SIDES: [(&str, &str, [&str; 7]); 2] = [
        (
            "left",
            "l",
            [
                "clavicle.L",
                "upper_arm.L",
                "forearm.L",
                "hand.L",
                "thigh.L",
                "shin.L",
                "foot.L",
            ],
        ),
        (
            "right",
            "r",
            [
                "clavicle.R",
                "upper_arm.R",
                "forearm.R",
                "hand.R",
                "thigh.R",
                "shin.R",
                "foot.R",
            ],
        ),
    ];
    for (s, l, bones) in SIDES {
        let n = |list: &[&str]| -> Vec<String> {
            list.iter()
                .map(|p| p.replace("{s}", s).replace("{l}", l))
                .collect()
        };
        let shoulder = n(&["{s}shoulder", "{l}clavicle", "{s}collar"]);
        let arm = n(&["{s}arm", "{l}humerus", "{s}upperarm"]);
        let fore = n(&["{s}forearm", "{l}radius", "{s}lowerarm"]);
        let hand = n(&["{s}hand", "{l}wrist", "{l}hand"]);
        let thigh = n(&["{s}upleg", "{l}femur", "{s}upperleg", "{s}thigh"]);
        let shin = n(&["{s}leg", "{l}tibia", "{s}lowerleg", "{s}shin"]);
        let foot = n(&["{s}foot", "{l}foot"]);
        let toe = n(&["{s}toebase", "{l}toes", "{s}toe"]);
        fn r(v: &[String]) -> Vec<&str> {
            v.iter().map(String::as_str).collect()
        }
        add(bones[0], &r(&shoulder), &r(&arm));
        add(bones[1], &r(&arm), &r(&fore));
        add(bones[2], &r(&fore), &r(&hand));
        add(bones[3], &r(&hand), &[]);
        add(bones[4], &r(&thigh), &r(&shin));
        add(bones[5], &r(&shin), &r(&foot));
        add(bones[6], &r(&foot), &r(&toe));
    }
    out
}

/// The rotation turning `a` onto `b` the shortest way.
fn arc(a: DVec3, b: DVec3) -> DQuat {
    let (a, b) = (a.normalize_or_zero(), b.normalize_or_zero());
    if a == DVec3::ZERO || b == DVec3::ZERO {
        return DQuat::IDENTITY;
    }
    DQuat::from_rotation_arc(a, b)
}

/// The file's motion on `rig`, as an animation with a key every frame (at most
/// [`KEYS_PER_SECOND`] a second), starting from the file's first frame.
pub fn retarget(bvh: &Bvh, rig: &Rig) -> Result<AvatarAnimation, String> {
    let found = matches(bvh);
    for needed in ["pelvis", "upper_arm.L", "upper_arm.R", "thigh.L", "thigh.R"] {
        if !found.contains_key(needed) {
            return Err(format!(
                "The BVH skeleton has no joint for the avatar's {needed}; its joints should use the usual names (Hips, LeftArm, LeftUpLeg…)"
            ));
        }
    }
    let (rest_rotations, rest_positions) = bvh.pose(None);
    // Turn the file so its left side is the avatar's left (+X) and it stands Y up.
    let side = |l: &str, r: &str| {
        let (a, b) = (bvh.find(&[l])?, bvh.find(&[r])?);
        Some(rest_positions[a] - rest_positions[b])
    };
    let left = side("leftupleg", "rightupleg")
        .or_else(|| side("lfemur", "rfemur"))
        .or_else(|| side("leftarm", "rightarm"))
        .unwrap_or(DVec3::X);
    let flat = DVec3::new(left.x, 0., left.z).normalize_or_zero();
    // Facing the other way exactly: half a turn about the vertical, never a flip.
    let facing = if flat.dot(DVec3::X) < -0.999 {
        DQuat::from_rotation_y(std::f64::consts::PI)
    } else {
        arc(flat, DVec3::X)
    };
    // Rest direction of each mapped bone in the file, to swing ours by.
    let direction = |k: (usize, Option<usize>), rotations: &[DQuat], positions: &[DVec3]| {
        let tip = match k.1 {
            Some(t) => Some(positions[t]),
            None => bvh.tip(k.0, rotations, positions),
        }?;
        Some(facing * (tip - positions[k.0]))
    };
    let step = ((1. / KEYS_PER_SECOND) / bvh.frame_time).ceil().max(1.) as usize;
    let mut keys: Vec<PoseKey> = Vec::new();
    let mut previous: BTreeMap<&str, [f64; 3]> = BTreeMap::new();
    for frame in (0..bvh.frame_count()).step_by(step) {
        let (rotations, positions) = bvh.pose(Some(frame));
        let mut world: Vec<DQuat> = vec![DQuat::IDENTITY; rig.bones.len()];
        let mut pose = BTreeMap::new();
        for (index, bone) in rig.bones.iter().enumerate() {
            let parent = bone.parent.map_or(DQuat::IDENTITY, |p| world[p]);
            let local = match found.get(bone.name) {
                Some(&(joint, _)) if bone.name == "pelvis" => {
                    facing * rotations[joint] * rest_rotations[joint].inverse() * facing.inverse()
                }
                Some(&k) => match direction(k, &rotations, &positions) {
                    Some(d) => arc(bone.tail - bone.head, parent.inverse() * d),
                    None => DQuat::IDENTITY,
                },
                None => DQuat::IDENTITY,
            };
            world[index] = parent * local;
            let mut angles = euler_degrees(local.normalize());
            // Keep each angle near the previous key's, so curves between keys do not
            // swing the long way round.
            if let Some(before) = previous.get(bone.name) {
                for (a, b) in angles.iter_mut().zip(before) {
                    while *a - b > 180. && *a - 360. >= -360. {
                        *a -= 360.;
                    }
                    while b - *a > 180. && *a + 360. <= 360. {
                        *a += 360.;
                    }
                }
            }
            if angles.iter().any(|a| a.abs() > 1e-6) {
                pose.insert(bone.name.to_string(), angles);
            }
            previous.insert(bone.name, angles);
        }
        keys.push(PoseKey {
            time_s: frame as f64 * bvh.frame_time,
            pose: Pose(pose),
        });
    }
    let fps = (1. / bvh.frame_time).clamp(1., 120.).min(60.);
    let animation = AvatarAnimation {
        keys,
        fps,
        ..AvatarAnimation::default()
    };
    animation.validate()?;
    Ok(animation)
}
