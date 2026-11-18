//! Avatar animation: poses keyed in time, ready-made motions and named markers, and the
//! recorder that simulates the cloth while the body moves along them.
//!
//! Each bone angle runs through its keys on a cubic curve whose slopes are flat at the first
//! and last key and wherever the angle turns back, so the body starts and stops at rest and
//! never swings past a key. Recording moves the body in steps short enough that no avatar
//! vertex travels further than [`BODY_STEP_M`], below the contact radius, so the body pushes
//! the cloth instead of passing through it, and lands a step on every frame.
use crate::{
    avatar::SurfaceMesh,
    avatar_rig::{Pose, Rig, euler_degrees},
    fitting::FittingProject,
    garment::{CompiledGarment, GarmentTemplate},
    model::State,
    vbd::{VbdSettings, VbdSolver},
};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    sync::Arc,
};

/// Longest animation, in seconds.
pub const MAX_DURATION_S: f64 = 600.;
/// Frame rates a take may be recorded at.
pub const FPS_RANGE: std::ops::RangeInclusive<f64> = 1.0..=120.0;
/// Largest distance any avatar vertex moves in one simulation step while recording.
pub const BODY_STEP_M: f64 = 0.0015;
/// Shortest step the recorder takes, however fast the body moves.
pub const MIN_STEP_S: f64 = 1e-5;
/// Fewest solver iterations of a recording step. Shorter steps than the settings' own take
/// proportionally fewer iterations: short steps converge in fewer, and the solver still
/// iterates more per simulated second than at full steps.
pub const MIN_ITERATIONS: usize = 4;

fn default_fps() -> f64 {
    30.
}

/// A pose the avatar reaches at a time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseKey {
    pub time_s: f64,
    pub pose: Pose,
}

/// A named moment on the timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    pub time_s: f64,
    pub name: String,
}

/// The avatar's motion: keys in time order, markers, and the frame rate takes are recorded at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvatarAnimation {
    pub keys: Vec<PoseKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
    #[serde(default = "default_fps")]
    pub fps: f64,
    /// Wind velocity (m/s) at times of the animation; without keys the scene's wind blows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wind_keys: Vec<VectorKey>,
    /// Gravity (m/s²) at times of the animation; without keys the scene's gravity pulls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gravity_keys: Vec<VectorKey>,
    /// Pieces' fabric at times of the animation, by piece; a piece without keys keeps the
    /// template's pressure and shrinkage.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fabric_keys: BTreeMap<String, Vec<FabricKey>>,
}

/// A piece's pressure (Pa) and shrinkage along the warp and the weft (percent of the cut
/// size) at one time of the animation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FabricKey {
    pub time_s: f64,
    pub pressure_pa: f64,
    pub shrink_percent: [f64; 2],
}

/// Put a fabric key at `time` among `keys`, replacing a key already there.
pub fn set_fabric_key(
    keys: &mut Vec<FabricKey>,
    time: f64,
    pressure_pa: f64,
    shrink_percent: [f64; 2],
) {
    keys.retain(|k| (k.time_s - time).abs() > 1e-6);
    let at = keys.partition_point(|k| k.time_s < time);
    keys.insert(
        at,
        FabricKey {
            time_s: time,
            pressure_pa,
            shrink_percent,
        },
    );
}

/// A vector at one time of the animation: the wind's velocity or gravity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorKey {
    pub time_s: f64,
    pub value: [f64; 3],
}

/// The keyed vector at time `t`: straight between keys, held before the first and after the
/// last; `None` without keys.
fn vector_at(keys: &[VectorKey], t: f64) -> Option<DVec3> {
    let v = |k: &VectorKey| DVec3::from_array(k.value);
    match keys {
        [] => None,
        [first, ..] if t <= first.time_s => Some(v(first)),
        [.., last] if t >= last.time_s => Some(v(last)),
        _ => {
            let k = keys.partition_point(|k| k.time_s <= t);
            let (a, b) = (&keys[k - 1], &keys[k]);
            Some(v(a).lerp(v(b), (t - a.time_s) / (b.time_s - a.time_s)))
        }
    }
}

/// Put `value` at `time` among `keys`, replacing a key already there.
pub fn set_vector_key(keys: &mut Vec<VectorKey>, time: f64, value: DVec3) {
    keys.retain(|k| (k.time_s - time).abs() > 1e-6);
    let at = keys.partition_point(|k| k.time_s < time);
    keys.insert(
        at,
        VectorKey {
            time_s: time,
            value: value.to_array(),
        },
    );
}

impl Default for AvatarAnimation {
    fn default() -> Self {
        Self {
            keys: vec![PoseKey {
                time_s: 0.,
                pose: Pose::default(),
            }],
            markers: Vec::new(),
            fps: default_fps(),
            wind_keys: Vec::new(),
            gravity_keys: Vec::new(),
            fabric_keys: BTreeMap::new(),
        }
    }
}

impl AvatarAnimation {
    /// An animation holding `pose` at time zero.
    pub fn starting_at(pose: Pose) -> Self {
        Self {
            keys: vec![PoseKey { time_s: 0., pose }],
            ..Self::default()
        }
    }

    /// The wind's velocity at time `t`, when the animation keys it.
    pub fn wind_at(&self, t: f64) -> Option<DVec3> {
        vector_at(&self.wind_keys, t)
    }

    /// Gravity at time `t`, when the animation keys it.
    pub fn gravity_at(&self, t: f64) -> Option<DVec3> {
        vector_at(&self.gravity_keys, t)
    }

    /// Every keyed piece's fabric at time `t`: straight between keys, held before the first
    /// and after the last.
    pub fn fabric_at(&self, t: f64) -> BTreeMap<String, crate::garment::FabricValues> {
        self.fabric_keys
            .iter()
            .filter(|(_, keys)| !keys.is_empty())
            .map(|(piece, keys)| {
                let value = |k: &FabricKey| (k.pressure_pa, DVec2::from_array(k.shrink_percent));
                let (pressure, shrink) = match keys.as_slice() {
                    [first, ..] if t <= first.time_s => value(first),
                    [.., last] if t >= last.time_s => value(last),
                    _ => {
                        let k = keys.partition_point(|k| k.time_s <= t);
                        let (a, b) = (&keys[k - 1], &keys[k]);
                        let f = (t - a.time_s) / (b.time_s - a.time_s);
                        let (pa, sa) = value(a);
                        let (pb, sb) = value(b);
                        (pa + (pb - pa) * f, sa.lerp(sb, f))
                    }
                };
                (
                    piece.clone(),
                    crate::garment::FabricValues {
                        pressure,
                        shrink: shrink.to_array(),
                    },
                )
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        for keys in self.fabric_keys.values() {
            let mut previous = f64::NEG_INFINITY;
            for key in keys {
                if !key.time_s.is_finite()
                    || !(0.0..=MAX_DURATION_S).contains(&key.time_s)
                    || key.time_s <= previous + 1e-6
                    || !key.pressure_pa.is_finite()
                    || key.pressure_pa.abs() > crate::garment::template::MAX_PRESSURE
                    || key
                        .shrink_percent
                        .iter()
                        .any(|s| !crate::garment::template::SHRINK_RANGE.contains(s))
                {
                    return Err(format!(
                        "Fabric keys must be in time order within {MAX_DURATION_S:.0} s, with pressure and shrinkage in range"
                    ));
                }
                previous = key.time_s;
            }
        }
        for (keys, what, most) in [
            (&self.wind_keys, "Wind", 50.),
            (&self.gravity_keys, "Gravity", 100.),
        ] {
            let mut previous = f64::NEG_INFINITY;
            for key in keys {
                let value = DVec3::from_array(key.value);
                if !key.time_s.is_finite()
                    || !(0.0..=MAX_DURATION_S).contains(&key.time_s)
                    || key.time_s <= previous + 1e-6
                    || !value.is_finite()
                    || value.length() > most
                {
                    return Err(format!(
                        "{what} keys must be in time order within {MAX_DURATION_S:.0} s and at most {most} long"
                    ));
                }
                previous = key.time_s;
            }
        }
        if !self.fps.is_finite() || !FPS_RANGE.contains(&self.fps) {
            return Err("The frame rate must be between 1 and 120 frames per second".into());
        }
        if self.keys.is_empty() {
            return Err("An animation needs at least one key".into());
        }
        let mut previous = f64::NEG_INFINITY;
        for key in &self.keys {
            if !key.time_s.is_finite() || !(0.0..=MAX_DURATION_S).contains(&key.time_s) {
                return Err(format!(
                    "Keys must lie between 0 and {MAX_DURATION_S:.0} seconds"
                ));
            }
            if key.time_s <= previous + 1e-6 {
                return Err("Keys must be in time order, at different times".into());
            }
            previous = key.time_s;
            key.pose.validate_angles()?;
        }
        for marker in &self.markers {
            if !marker.time_s.is_finite() || !(0.0..=MAX_DURATION_S).contains(&marker.time_s) {
                return Err(format!(
                    "Markers must lie between 0 and {MAX_DURATION_S:.0} seconds"
                ));
            }
            if marker.name.trim().is_empty() || marker.name.len() > 200 {
                return Err("A marker needs a name of at most 200 characters".into());
            }
        }
        Ok(())
    }

    /// Time of the last key.
    pub fn duration(&self) -> f64 {
        self.keys.last().map_or(0., |k| k.time_s)
    }

    /// Frames in a take of the whole animation, both ends included.
    pub fn frame_count(&self) -> usize {
        (self.duration() * self.fps).round() as usize + 1
    }

    /// The pose at `time`: held before the first key and after the last, each bone angle
    /// following its curve between them.
    pub fn pose_at(&self, time: f64) -> Pose {
        let bones: BTreeSet<&String> = self.keys.iter().flat_map(|k| k.pose.0.keys()).collect();
        let times: Vec<f64> = self.keys.iter().map(|k| k.time_s).collect();
        let mut pose = Pose::default();
        for bone in bones {
            let mut angles = [0.; 3];
            for (axis, angle) in angles.iter_mut().enumerate() {
                let values: Vec<f64> = self
                    .keys
                    .iter()
                    .map(|k| k.pose.0.get(bone).map_or(0., |a| a[axis]))
                    .collect();
                *angle = curve(&times, &values, time);
            }
            pose.0.insert(bone.clone(), angles);
        }
        pose
    }

    /// Put `pose` at `time`, replacing a key within half a frame of it.
    pub fn set_key(&mut self, time: f64, pose: Pose) {
        let near = 0.5 / self.fps;
        match self
            .keys
            .iter_mut()
            .find(|k| (k.time_s - time).abs() < near)
        {
            Some(key) => key.pose = pose,
            None => {
                let at = self.keys.partition_point(|k| k.time_s < time);
                self.keys.insert(at, PoseKey { time_s: time, pose });
            }
        }
    }

    /// Index of the key within half a frame of `time`.
    pub fn key_near(&self, time: f64) -> Option<usize> {
        let near = 0.5 / self.fps;
        self.keys
            .iter()
            .position(|k| (k.time_s - time).abs() < near)
    }

    /// Add a marker at `time`, named after its order unless `name` is given.
    pub fn add_marker(&mut self, time: f64, name: Option<String>) {
        let name = name.unwrap_or_else(|| format!("Marker {}", self.markers.len() + 1));
        let at = self.markers.partition_point(|m| m.time_s <= time);
        self.markers.insert(at, Marker { time_s: time, name });
    }
}

/// Cubic Hermite curve through `(times, values)` with flat ends and flat turning points.
fn curve(times: &[f64], values: &[f64], t: f64) -> f64 {
    let n = times.len();
    if t <= times[0] {
        return values[0];
    }
    if t >= times[n - 1] {
        return values[n - 1];
    }
    let i = times.partition_point(|&k| k <= t) - 1;
    let slope = |j: usize| -> f64 {
        if j == 0 || j + 1 == n {
            return 0.;
        }
        let (before, after) = (values[j] - values[j - 1], values[j + 1] - values[j]);
        if before * after <= 0. {
            0.
        } else {
            (values[j + 1] - values[j - 1]) / (times[j + 1] - times[j - 1])
        }
    };
    let h = times[i + 1] - times[i];
    let u = (t - times[i]) / h;
    let (u2, u3) = (u * u, u * u * u);
    (2. * u3 - 3. * u2 + 1.) * values[i]
        + (u3 - 2. * u2 + u) * h * slope(i)
        + (-2. * u3 + 3. * u2) * values[i + 1]
        + (u3 - u2) * h * slope(i + 1)
}

/// Names of the ready-made motions.
pub const MOTIONS: [&str; 4] = ["Walk in place", "Raise arms", "Wave", "Twist"];

/// A ready-made motion that starts and ends in `start`, its bone turns added to it.
pub fn motion(name: &str, start: &Pose) -> Option<AvatarAnimation> {
    let at = |entries: &[(&str, [f64; 3])]| {
        let mut pose = start.clone();
        for (bone, [x, y, z]) in entries {
            let angle = pose.0.entry(bone.to_string()).or_insert([0.; 3]);
            angle[0] += x;
            angle[1] += y;
            angle[2] += z;
        }
        pose
    };
    let mut keys: Vec<(f64, Pose)> = vec![(0., start.clone())];
    match name {
        "Walk in place" => {
            let left = at(&[
                ("thigh.L", [-25., 0., 0.]),
                ("shin.L", [35., 0., 0.]),
                ("thigh.R", [10., 0., 0.]),
                ("upper_arm.L", [12., 0., 0.]),
                ("upper_arm.R", [-15., 0., 0.]),
            ]);
            let right = at(&[
                ("thigh.R", [-25., 0., 0.]),
                ("shin.R", [35., 0., 0.]),
                ("thigh.L", [10., 0., 0.]),
                ("upper_arm.R", [12., 0., 0.]),
                ("upper_arm.L", [-15., 0., 0.]),
            ]);
            for cycle in 0..2 {
                let t = 0.2 + 1.2 * cycle as f64;
                keys.push((t + 0.3, left.clone()));
                keys.push((t + 0.6, start.clone()));
                keys.push((t + 0.9, right.clone()));
                keys.push((t + 1.2, start.clone()));
            }
        }
        "Raise arms" => {
            let up = at(&[
                ("upper_arm.L", [0., 0., 50.]),
                ("forearm.L", [0., 0., 10.]),
                ("upper_arm.R", [0., 0., -50.]),
                ("forearm.R", [0., 0., -10.]),
            ]);
            keys.push((1.0, up.clone()));
            keys.push((1.5, up));
            keys.push((2.5, start.clone()));
        }
        "Wave" => {
            let raised = |swing: f64| {
                at(&[
                    ("upper_arm.R", [0., 0., -60.]),
                    ("forearm.R", [0., 0., -70. + swing]),
                ])
            };
            keys.push((0.8, raised(0.)));
            for k in 0..4 {
                keys.push((
                    1.1 + 0.3 * k as f64,
                    raised(if k % 2 == 0 { 30. } else { 0. }),
                ));
            }
            keys.push((2.8, start.clone()));
        }
        "Twist" => {
            keys.push((
                0.8,
                at(&[("spine", [0., 20., 0.]), ("chest", [0., 10., 0.])]),
            ));
            keys.push((
                1.8,
                at(&[("spine", [0., -20., 0.]), ("chest", [0., -10., 0.])]),
            ));
            keys.push((2.6, start.clone()));
        }
        _ => return None,
    }
    Some(AvatarAnimation {
        keys: keys
            .into_iter()
            .map(|(time_s, pose)| PoseKey { time_s, pose })
            .collect(),
        ..AvatarAnimation::default()
    })
}

/// Cloth positions recorded once per frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Take {
    pub fps: f64,
    pub frames: Vec<Vec<[f32; 3]>>,
}

impl Take {
    pub fn duration(&self) -> f64 {
        self.frames.len().saturating_sub(1) as f64 / self.fps
    }
    /// The frame shown at `time`.
    pub fn frame_index(&self, time: f64) -> usize {
        ((time * self.fps).round().max(0.) as usize).min(self.frames.len().saturating_sub(1))
    }
    pub fn positions(&self, frame: usize) -> Vec<DVec3> {
        self.frames[frame]
            .iter()
            .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
            .collect()
    }
    /// Write the take as a PC2 point cache: little-endian, a 32-byte header then every
    /// frame's points as three floats.
    pub fn write_pc2(&self, path: &Path) -> Result<(), String> {
        let points = self.frames.first().map_or(0, Vec::len);
        if self.frames.is_empty() || points == 0 {
            return Err("The take holds no frames".into());
        }
        let mut bytes = Vec::with_capacity(32 + self.frames.len() * points * 12);
        bytes.extend_from_slice(b"POINTCACHE2\0");
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&(points as i32).to_le_bytes());
        bytes.extend_from_slice(&0f32.to_le_bytes());
        bytes.extend_from_slice(&1f32.to_le_bytes());
        bytes.extend_from_slice(&(self.frames.len() as i32).to_le_bytes());
        for frame in &self.frames {
            for p in frame {
                for c in p {
                    bytes.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())
    }

    /// Write the take as an MDD point cache: big-endian frame and point counts, each frame's
    /// time in seconds, then every frame's points. `z_up` turns Y up into Z up, as Maya and
    /// 3ds Max scenes expect.
    pub fn write_mdd(&self, path: &Path, z_up: bool) -> Result<(), String> {
        let points = self.frames.first().map_or(0, Vec::len);
        if self.frames.is_empty() || points == 0 {
            return Err("The take holds no frames".into());
        }
        let mut bytes = Vec::with_capacity(8 + self.frames.len() * (4 + points * 12));
        bytes.extend_from_slice(&(self.frames.len() as i32).to_be_bytes());
        bytes.extend_from_slice(&(points as i32).to_be_bytes());
        for k in 0..self.frames.len() {
            bytes.extend_from_slice(&((k as f64 / self.fps) as f32).to_be_bytes());
        }
        for frame in &self.frames {
            for [x, y, z] in frame {
                let p = if z_up { [*x, -*z, *y] } else { [*x, *y, *z] };
                for c in p {
                    bytes.extend_from_slice(&c.to_be_bytes());
                }
            }
        }
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    /// Read a PC2 point cache as a take recorded at `fps`.
    pub fn read_pc2(path: &Path, fps: f64) -> Result<Self, String> {
        if !fps.is_finite() || !FPS_RANGE.contains(&fps) {
            return Err("The frame rate must be between 1 and 120 frames per second".into());
        }
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() < 32 || &bytes[..12] != b"POINTCACHE2\0" {
            return Err(format!("{} is not a PC2 point cache", path.display()));
        }
        let int = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let (points, samples) = (int(16), int(28));
        if points <= 0 || samples <= 0 {
            return Err(format!("{} holds no points", path.display()));
        }
        let (points, samples) = (points as usize, samples as usize);
        if bytes.len() != 32 + points * samples * 12 {
            return Err(format!("{} is truncated", path.display()));
        }
        let float = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let frames = (0..samples)
            .map(|s| {
                (0..points)
                    .map(|p| {
                        let at = 32 + (s * points + p) * 12;
                        [float(at), float(at + 4), float(at + 8)]
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if frames.iter().flatten().flatten().any(|c| !c.is_finite()) {
            return Err(format!("{} holds invalid positions", path.display()));
        }
        Ok(Self { fps, frames })
    }
}

impl Take {
    /// Write each frame of the cloth as `<stem>_0000.obj`, `<stem>_0001.obj`… in `dir`, with
    /// the garment's pattern UVs. Returns how many files were written.
    pub fn write_obj_sequence(
        &self,
        garment: &CompiledGarment,
        dir: &Path,
        stem: &str,
    ) -> Result<usize, String> {
        if self.frames.is_empty() {
            return Err("The take holds no frames".into());
        }
        if self.frames[0].len() != garment.mesh.uv.len() {
            return Err("The take was recorded on another mesh".into());
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for k in 0..self.frames.len() {
            let x = self.positions(k);
            let state = State {
                v: vec![DVec3::ZERO; x.len()],
                x,
                time: k as f64 / self.fps,
            };
            std::fs::write(
                dir.join(format!("{stem}_{k:04}.obj")),
                crate::garment::export::obj(garment, &state),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(self.frames.len())
    }
}

/// `surface` as OBJ text: positions in metres and triangles.
pub fn surface_obj(surface: &SurfaceMesh) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("# avatar and scene objects; metres\n");
    for p in &surface.vertices {
        writeln!(out, "v {:.9} {:.9} {:.9}", p.x, p.y, p.z).unwrap();
    }
    for [a, b, c] in &surface.triangles {
        writeln!(out, "f {} {} {}", a + 1, b + 1, c + 1).unwrap();
    }
    out
}

/// The avatar (with the scene objects) at each of `frames` frames of `animation`.
pub fn body_frames(
    project: &FittingProject,
    animation: &AvatarAnimation,
    frames: usize,
) -> Result<Vec<SurfaceMesh>, String> {
    animation.validate()?;
    let avatar = match &project.avatar_pose {
        Some(p) => p.clone(),
        None => crate::fitting::AvatarPose::from_sidecar(&project.avatar_path)?,
    };
    let rest = SurfaceMesh::load_obj(&project.avatar_path)?;
    let rig = avatar.rig(&rest)?;
    let props = crate::fitting::MovingProps::new(&project.scene_objects)?;
    Ok((0..frames)
        .map(|k| {
            let t = k as f64 / animation.fps;
            let body = rig.posed(&rest, &animation.pose_at(t));
            match &props {
                Some(props) => body.merged(&props.mesh_at(t)),
                None => body,
            }
        })
        .collect())
}

/// Write the avatar (with the scene objects) at each of `frames` frames of `animation` as
/// `<stem>_0000.obj`… in `dir`. Returns how many files were written.
pub fn write_body_sequence(
    project: &FittingProject,
    animation: &AvatarAnimation,
    frames: usize,
    dir: &Path,
    stem: &str,
) -> Result<usize, String> {
    let bodies = body_frames(project, animation, frames)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (k, body) in bodies.iter().enumerate() {
        std::fs::write(dir.join(format!("{stem}_{k:04}.obj")), surface_obj(body))
            .map_err(|e| e.to_string())?;
    }
    Ok(frames)
}

/// A pinned point held on to the avatar: the body triangle it was nearest, where on it, and
/// how far off it in that triangle's own frame.
#[derive(Clone, Debug)]
struct PinHold {
    vertex: usize,
    triangle: usize,
    at: [f64; 2],
    local: DVec3,
}

/// A triangle's corner, edges and frame (along its first edge, across, out).
fn triangle_frame(corners: [DVec3; 3]) -> (DVec3, DVec3, DVec3, [DVec3; 3]) {
    let [a, b, c] = corners;
    let (e1, e2) = (b - a, c - a);
    let n = e1.cross(e2).normalize_or_zero();
    let along = e1.normalize_or_zero();
    (a, e1, e2, [along, n.cross(along), n])
}

impl PinHold {
    fn bind(vertex: usize, p: DVec3, body: &[DVec3], triangles: &[[usize; 3]]) -> Option<Self> {
        let (triangle, closest) = triangles
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let c = crate::garment::transfer::closest_on_triangle(p, t.map(|i| body[i]));
                (k, c)
            })
            .min_by(|a, b| a.1.distance_squared(p).total_cmp(&b.1.distance_squared(p)))?;
        let (a, e1, e2, frame) = triangle_frame(triangles[triangle].map(|i| body[i]));
        let (g11, g12, g22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
        let det = g11 * g22 - g12 * g12;
        if det <= 1e-30 {
            return None;
        }
        let d = closest - a;
        let (r1, r2) = (d.dot(e1), d.dot(e2));
        let at = [(r1 * g22 - r2 * g12) / det, (g11 * r2 - g12 * r1) / det];
        let off = p - closest;
        Some(Self {
            vertex,
            triangle,
            at,
            local: DVec3::new(off.dot(frame[0]), off.dot(frame[1]), off.dot(frame[2])),
        })
    }
    /// Where the pinned point is with the body at `body`.
    fn place(&self, body: &[DVec3], triangles: &[[usize; 3]]) -> DVec3 {
        let (a, e1, e2, frame) = triangle_frame(triangles[self.triangle].map(|i| body[i]));
        a + e1 * self.at[0]
            + e2 * self.at[1]
            + frame[0] * self.local.x
            + frame[1] * self.local.y
            + frame[2] * self.local.z
    }
}

/// Moving into the animation's first pose before recording starts.
struct LeadIn {
    from: Pose,
    step: usize,
    steps: usize,
}

/// Drives the body along an animation one simulation step at a time and keeps a frame of the
/// cloth whenever a step lands on one. Call [`Recorder::begin`] with the settled cloth, then
/// for every step [`Recorder::plan`], step the solver against the returned obstacle for the
/// returned time, and [`Recorder::commit`] the result, until [`Recorder::finished`].
pub struct Recorder {
    rig: Rig,
    rest: SurfaceMesh,
    /// The scene objects, which may move along their own keys.
    props: Option<crate::fitting::MovingProps>,
    /// Where the scene objects' vertices are now, and where they were before recording.
    props_now: Vec<DVec3>,
    props_still: Vec<DVec3>,
    animation: AvatarAnimation,
    lead_in: Option<LeadIn>,
    time: f64,
    body: Vec<DVec3>,
    planned: Option<(f64, Vec<DVec3>, Vec<DVec3>)>,
    /// Pinned points held on to the avatar, and where the planned step puts them.
    holds: Vec<PinHold>,
    pins_planned: Vec<(usize, DVec3)>,
    frame_count: usize,
    take: Take,
    /// The template fabric keys change, and the fabric last given to the solver.
    fabric: Option<(
        GarmentTemplate,
        BTreeMap<String, crate::garment::FabricValues>,
    )>,
}

impl Recorder {
    /// A recorder for `animation` on `project`'s avatar, which must be riggable. When the
    /// avatar's pose differs from the animation's first one, the body moves into it first.
    pub fn new(project: &FittingProject, animation: &AvatarAnimation) -> Result<Self, String> {
        animation.validate()?;
        let avatar = match &project.avatar_pose {
            Some(p) => p.clone(),
            None => crate::fitting::AvatarPose::from_sidecar(&project.avatar_path)?,
        };
        let rest = SurfaceMesh::load_obj(&project.avatar_path)?;
        let rig = avatar.rig(&rest)?;
        // Keys may turn added bones, but only bones this skeleton has.
        for key in &animation.keys {
            if let Some(name) = key.pose.0.keys().find(|b| rig.bone(b).is_none()) {
                return Err(format!("Unknown bone {name}"));
            }
        }
        let props = crate::fitting::MovingProps::new(&project.scene_objects)?;
        // The objects stand where the scene put them, then move to their first keys
        // together with the body moving into the first pose.
        let props_still = match crate::fitting::scene_objects_mesh(&project.scene_objects)? {
            Some(mesh) => mesh.vertices,
            None => Vec::new(),
        };
        let props_first = props.as_ref().map_or(Vec::new(), |p| p.vertices_at(0.));
        let start = rig.posed(&rest, &avatar.pose).vertices;
        let first = rig.posed(&rest, &animation.pose_at(0.)).vertices;
        // Pins held on to the avatar keep to the spot of the skin they are nearest now.
        let mut holds = Vec::new();
        if project.template.pins_attached && !project.template.pin_targets.is_empty() {
            let garment = project.template.compile()?;
            for (vertex, target) in
                crate::garment::pins::resolve_targets(&garment, &project.template.pin_targets)?
            {
                if let Some(hold) = PinHold::bind(vertex, target, &start, &rest.triangles) {
                    holds.push(hold);
                }
            }
        }
        let farthest = start
            .iter()
            .zip(&first)
            .chain(props_still.iter().zip(&props_first))
            .map(|(a, b)| a.distance(*b))
            .fold(0., f64::max);
        let lead_in = (farthest > 1e-9).then(|| LeadIn {
            from: avatar.pose.clone(),
            step: 0,
            steps: ((farthest / BODY_STEP_M).ceil() as usize).max(1),
        });
        Ok(Self {
            rig,
            rest,
            props,
            props_now: props_still.clone(),
            props_still,
            holds,
            pins_planned: Vec::new(),
            animation: animation.clone(),
            lead_in,
            time: 0.,
            body: start,
            planned: None,
            frame_count: animation.frame_count(),
            take: Take {
                fps: animation.fps,
                frames: Vec::new(),
            },
            fabric: (!animation.fabric_keys.is_empty())
                .then(|| (project.template.clone(), BTreeMap::new())),
        })
    }

    /// The garment for the planned step when fabric keys change it: `base` (compiled from the
    /// template) with the keyed pieces' fabric at the step's end; none when it is as before.
    pub fn step_garment(
        &mut self,
        base: &Arc<CompiledGarment>,
    ) -> Result<Option<Arc<CompiledGarment>>, String> {
        let t = self.planned.as_ref().map_or(self.time, |p| p.0);
        let fabric = self.animation.fabric_at(t);
        let Some((template, last)) = &mut self.fabric else {
            return Ok(None);
        };
        if *last == fabric {
            return Ok(None);
        }
        let garment = base.with_fabric(template, &fabric)?;
        *last = fabric;
        Ok(Some(Arc::new(garment)))
    }

    /// Start from the settled cloth: frame zero, unless the body first moves into the pose.
    pub fn begin(&mut self, x: &[DVec3]) {
        if self.lead_in.is_none() && self.take.frames.is_empty() {
            self.capture(x);
        }
    }

    fn capture(&mut self, x: &[DVec3]) {
        self.take.frames.push(
            x.iter()
                .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                .collect(),
        );
    }

    fn obstacle(&self, body: &[DVec3], props: &[DVec3]) -> SurfaceMesh {
        let surface = SurfaceMesh {
            vertices: body.to_vec(),
            triangles: self.rest.triangles.clone(),
        };
        match &self.props {
            Some(moving) => {
                let mut objects = moving.mesh_at(0.);
                objects.vertices = props.to_vec();
                surface.merged(&objects)
            }
            None => surface,
        }
    }

    /// The scene objects' vertices at animation time `t`.
    fn props_at(&self, t: f64) -> Vec<DVec3> {
        self.props.as_ref().map_or(Vec::new(), |p| p.vertices_at(t))
    }

    fn farthest(&self, body: &[DVec3], props: &[DVec3]) -> f64 {
        self.body
            .iter()
            .zip(body)
            .chain(self.props_now.iter().zip(props))
            .map(|(a, b)| a.distance(*b))
            .fold(0., f64::max)
    }

    /// The next step: how long it lasts (at most `max_dt`) and the obstacle at its end.
    pub fn plan(&mut self, max_dt: f64) -> (f64, Arc<SurfaceMesh>) {
        if let Some(lead) = &self.lead_in {
            let fraction = (lead.step + 1) as f64 / lead.steps as f64;
            let blend = lead.from.blend(&self.animation.keys[0].pose, fraction);
            let transforms = self
                .rig
                .transforms(|b| blend.get(b).copied().unwrap_or_default());
            let body = self.rig.skin(&self.rest, &transforms).vertices;
            let first = self.props_at(0.);
            let props: Vec<DVec3> = self
                .props_still
                .iter()
                .zip(&first)
                .map(|(a, b)| a.lerp(*b, fraction))
                .collect();
            let obstacle = Arc::new(self.obstacle(&body, &props));
            self.hold_pins(&body);
            self.planned = Some((self.time, body, props));
            return (max_dt, obstacle);
        }
        let next_frame = self.take.frames.len() as f64 / self.animation.fps;
        let mut end = (self.time + max_dt).min(next_frame);
        let at = |end: f64| {
            (
                self.rig
                    .posed(&self.rest, &self.animation.pose_at(end))
                    .vertices,
                self.props_at(end),
            )
        };
        let (mut body, mut props) = at(end);
        for _ in 0..8 {
            let moved = self.farthest(&body, &props);
            if moved <= BODY_STEP_M || end - self.time <= MIN_STEP_S {
                break;
            }
            end = self.time + ((end - self.time) * 0.9 * BODY_STEP_M / moved).max(MIN_STEP_S);
            (body, props) = at(end);
        }
        let dt = (end - self.time).max(MIN_STEP_S);
        let obstacle = Arc::new(self.obstacle(&body, &props));
        self.hold_pins(&body);
        self.planned = Some((end, body, props));
        (dt, obstacle)
    }

    /// The planned step was taken and the cloth is now `x`.
    pub fn commit(&mut self, x: &[DVec3]) {
        let Some((time, body, props)) = self.planned.take() else {
            return;
        };
        self.body = body;
        self.props_now = props;
        if let Some(lead) = self.lead_in.as_mut() {
            lead.step += 1;
            if lead.step >= lead.steps {
                self.lead_in = None;
                self.capture(x);
            }
            return;
        }
        self.time = time;
        let next_frame = self.take.frames.len() as f64 / self.animation.fps;
        if self.time >= next_frame - 1e-9 {
            self.capture(x);
        }
    }

    pub fn finished(&self) -> bool {
        self.take.frames.len() >= self.frame_count
    }

    /// Share of the animation recorded, zero while moving into the first pose.
    pub fn progress(&self) -> f64 {
        self.take.frames.len() as f64 / self.frame_count as f64
    }

    pub fn moving_into_first_pose(&self) -> bool {
        self.lead_in.is_some()
    }

    fn hold_pins(&mut self, body: &[DVec3]) {
        self.pins_planned = self
            .holds
            .iter()
            .map(|h| (h.vertex, h.place(body, &self.rest.triangles)))
            .collect();
    }

    /// Where the planned step puts the pinned points held on to the avatar: the cloth's
    /// vertex and its place.
    pub fn pin_positions(&self) -> &[(usize, DVec3)] {
        &self.pins_planned
    }

    /// The obstacle where the body is now: the avatar and the scene objects.
    pub fn obstacle_now(&self) -> SurfaceMesh {
        self.obstacle(&self.body, &self.props_now)
    }

    /// The pose the body is in now, part way into the first pose while moving into it.
    pub fn pose_now(&self) -> Pose {
        match &self.lead_in {
            Some(lead) => {
                let fraction = lead.step as f64 / lead.steps as f64;
                let blend = lead.from.blend(&self.animation.keys[0].pose, fraction);
                Pose(
                    blend
                        .into_iter()
                        .map(|(bone, q)| (bone, euler_degrees(q)))
                        .filter(|(_, a)| a.iter().any(|x| x.abs() > 1e-9))
                        .collect(),
                )
            }
            None => self.animation.pose_at(self.time),
        }
    }

    /// Animation time the cloth has reached.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// The solver settings for the planned step: `base`, with the wind and gravity the
    /// animation keys at the step's end.
    pub fn step_settings(&self, base: &VbdSettings) -> VbdSettings {
        let t = self.planned.as_ref().map_or(self.time, |p| p.0);
        let mut settings = base.clone();
        if let Some(g) = self.animation.gravity_at(t) {
            settings.gravity = g;
        }
        if let Some(w) = self.animation.wind_at(t) {
            settings.wind = Some(crate::vbd::Wind {
                velocity: w,
                ..base.wind.clone().unwrap_or_default()
            });
        }
        settings
    }

    pub fn take(&self) -> &Take {
        &self.take
    }

    pub fn into_take(self) -> Take {
        self.take
    }
}

/// Simulate `state` on `garment` along the recorder's motion, `surface` being the obstacle
/// the cloth rests against now. After every step `progress` sees the recorder and the cloth
/// and returns false to stop early. Returns both where they stopped.
pub fn run(
    garment: Arc<CompiledGarment>,
    surface: &SurfaceMesh,
    state: State,
    recorder: Recorder,
    settings: &VbdSettings,
    progress: impl FnMut(&Recorder, &State) -> bool,
) -> Result<(Recorder, State), String> {
    run_stepping(
        garment,
        surface,
        state,
        recorder,
        settings,
        |solver, state, step| solver.step(state, step, None).map(|_| ()),
        progress,
    )
}

/// [`run`], each solver step taken by `step` — for instance on the GPU, which works on the
/// same solver (its obstacle, pins and contacts) as the CPU does.
pub fn run_stepping(
    garment: Arc<CompiledGarment>,
    surface: &SurfaceMesh,
    mut state: State,
    mut recorder: Recorder,
    settings: &VbdSettings,
    mut step_with: impl FnMut(&mut VbdSolver, &mut State, &VbdSettings) -> Result<(), String>,
    mut progress: impl FnMut(&Recorder, &State) -> bool,
) -> Result<(Recorder, State), String> {
    let floored = |surface: &SurfaceMesh| match settings.floor {
        Some(height) => surface.with_floor(height),
        None => surface.clone(),
    };
    let base = garment.clone();
    let mut solver = VbdSolver::new(garment, Some(&floored(surface)), &state)?;
    recorder.begin(&state.x);
    while !recorder.finished() {
        let (dt, obstacle) = recorder.plan(settings.dt);
        // Fabric keys reshape the pieces they name as the take goes on.
        if let Some(keyed) = recorder.step_garment(&base)? {
            solver.set_garment(keyed)?;
        }
        // Pins held on to the avatar go with it.
        for &(v, p) in recorder.pin_positions() {
            state.x[v] = p;
            state.v[v] = DVec3::ZERO;
        }
        solver.set_obstacle(&state.x, &floored(&obstacle))?;
        let mut step = recorder.step_settings(settings);
        step.dt = dt;
        step.iterations = ((settings.iterations as f64 * dt / settings.dt).ceil() as usize)
            .clamp(MIN_ITERATIONS.min(settings.iterations), settings.iterations);
        step_with(&mut solver, &mut state, &step)?;
        recorder.commit(&state.x);
        if !progress(&recorder, &state) {
            break;
        }
    }
    Ok((recorder, state))
}

/// Record `animation` on `project` without a window, from the project's cloth as it is.
pub fn record(
    project: &FittingProject,
    animation: &AvatarAnimation,
    settings: &VbdSettings,
) -> Result<Take, String> {
    let scene = project.compile()?;
    let recorder = Recorder::new(project, animation)?;
    let (recorder, _) = run(
        scene.garment.clone(),
        &scene.collider.surface,
        scene.state.clone(),
        recorder,
        settings,
        |_, _| true,
    )?;
    Ok(recorder.into_take())
}
