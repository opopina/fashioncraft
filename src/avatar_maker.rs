//! Bodies made from MakeHuman's CC0 macro targets: the stock mannequin's mesh (MakeHuman's base
//! mesh, hm08) morphed by gender, age, muscle, weight, height, proportions and ancestry, with its
//! joints where MakeHuman's joint helpers land.
//!
//! The targets are packed in `assets/makehuman/macro-targets.bin` by
//! `scripts/pack-macro-targets.py` (format there) and built into the program. Only MakeHuman's
//! data is used. The blend is this project's own: each target is named after the categories it
//! stands for ("universal-female-young-maxmuscle-averageweight"), and weighs the product of how
//! near each setting is to its categories.
use crate::avatar::SurfaceMesh;
use glam::DVec3;
use std::{collections::BTreeMap, sync::OnceLock};

const PACKED: &[u8] = include_bytes!("../assets/makehuman/macro-targets.bin");
const MANNEQUIN_OBJ: &str = include_str!("../assets/makehuman/mannequin.obj");
/// MakeHuman's unit, the decimetre, in metres.
const UNIT_M: f64 = 0.1;
/// The base mesh's body vertices, which are the mannequin's, in the same order.
pub const BODY_VERTICES: usize = 13380;
/// The ages the targets are drawn at, years: baby, child, young adult, old.
const AGES: [(&str, f64); 4] = [("baby", 1.), ("child", 11.), ("young", 25.), ("old", 90.)];

/// The settings of a made body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroBody {
    /// 0 female … 1 male.
    pub gender: f64,
    /// 1 to 90 years.
    pub age_years: f64,
    /// 0 least … 0.5 average … 1 most.
    pub muscle: f64,
    /// 0 least … 0.5 average … 1 most.
    pub weight: f64,
    /// 0 shortest … 0.5 average for the age and gender … 1 tallest.
    pub height: f64,
    /// 0 uncommon … 0.5 regular … 1 ideal.
    pub proportions: f64,
    /// Ancestry shares, African, Asian and European; only their ratios count.
    pub african: f64,
    pub asian: f64,
    pub european: f64,
}

impl Default for MacroBody {
    fn default() -> Self {
        Self {
            gender: 0.5,
            age_years: 25.,
            muscle: 0.5,
            weight: 0.5,
            height: 0.5,
            proportions: 0.5,
            african: 1. / 3.,
            asian: 1. / 3.,
            european: 1. / 3.,
        }
    }
}

impl MacroBody {
    pub fn validate(&self) -> Result<(), String> {
        let unit = [
            self.gender,
            self.muscle,
            self.weight,
            self.height,
            self.proportions,
            self.african,
            self.asian,
            self.european,
        ];
        if unit
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Body settings must be between 0 and 1".into());
        }
        if !self.age_years.is_finite() || !(1.0..=90.0).contains(&self.age_years) {
            return Err("The age must be 1 to 90 years".into());
        }
        if self.african + self.asian + self.european <= 1e-9 {
            return Err("Give the body some ancestry".into());
        }
        Ok(())
    }

    /// The targets this body is made of, with their weights; those weighing nothing are left out.
    pub fn weights(&self) -> Vec<(String, f64)> {
        let genders = [("female", 1. - self.gender), ("male", self.gender)];
        let ages = age_weights(self.age_years);
        let three = |v: f64, names: [&'static str; 3]| {
            let w = if v < 0.5 {
                [1. - 2. * v, 2. * v, 0.]
            } else {
                [0., 2. - 2. * v, 2. * v - 1.]
            };
            [(names[0], w[0]), (names[1], w[1]), (names[2], w[2])]
        };
        let muscles = three(self.muscle, ["minmuscle", "averagemuscle", "maxmuscle"]);
        let weights = three(self.weight, ["minweight", "averageweight", "maxweight"]);
        let height = if self.height < 0.5 {
            ("minheight", 1. - 2. * self.height)
        } else {
            ("maxheight", 2. * self.height - 1.)
        };
        let proportions = if self.proportions < 0.5 {
            ("uncommonproportions", 1. - 2. * self.proportions)
        } else {
            ("idealproportions", 2. * self.proportions - 1.)
        };
        let total = self.african + self.asian + self.european;
        let ancestry = [
            ("african", self.african / total),
            ("asian", self.asian / total),
            ("caucasian", self.european / total),
        ];
        let mut out = Vec::new();
        let mut push = |name: String, w: f64| {
            if w > 1e-9 {
                out.push((name, w));
            }
        };
        for (g, gw) in genders {
            for (a, aw) in ages {
                for (e, ew) in ancestry {
                    push(format!("{e}-{g}-{a}"), ew * gw * aw);
                }
                for (m, mw) in muscles {
                    for (w, ww) in weights {
                        let base = gw * aw * mw * ww;
                        push(format!("universal-{g}-{a}-{m}-{w}"), base);
                        push(
                            format!("height/{g}-{a}-{m}-{w}-{}", height.0),
                            base * height.1,
                        );
                        push(
                            format!("proportions/{g}-{a}-{m}-{w}-{}", proportions.0),
                            base * proportions.1,
                        );
                    }
                }
            }
        }
        out
    }
}

/// How near `years` is to each age the targets are drawn at: linear between the two around it.
fn age_weights(years: f64) -> [(&'static str, f64); 4] {
    let mut out = AGES.map(|(name, _)| (name, 0.));
    let years = years.clamp(AGES[0].1, AGES[3].1);
    for k in 0..3 {
        let (lo, hi) = (AGES[k].1, AGES[k + 1].1);
        if years <= hi {
            let t = (years - lo) / (hi - lo);
            out[k].1 = 1. - t;
            out[k + 1].1 = t;
            break;
        }
    }
    out
}

/// One target: offsets of some kept vertices, in 0.001 dm.
struct Target {
    index: Vec<u32>,
    offset: Vec<[i32; 3]>,
}

/// MakeHuman's base mesh (its body and joint helpers) and its macro targets.
pub struct MacroTargets {
    /// Base positions of the kept vertices, decimetres: the body, then the joint helpers.
    base: Vec<DVec3>,
    /// Joint names and the kept vertices of their helpers.
    joints: Vec<(String, Vec<usize>)>,
    targets: BTreeMap<String, Target>,
}

/// A body made from the targets.
pub struct MadeBody {
    pub surface: SurfaceMesh,
    /// Joint landmarks, metres, as the mannequin's `measurements.json` names them.
    pub joints: BTreeMap<String, DVec3>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.bytes.len());
        let end = end.ok_or("The body targets end early")?;
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
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn name(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "A target name is not UTF-8".into())
    }
}

impl MacroTargets {
    /// The targets built into the program, read once.
    pub fn bundled() -> Result<&'static MacroTargets, String> {
        static BUNDLED: OnceLock<Result<MacroTargets, String>> = OnceLock::new();
        BUNDLED
            .get_or_init(|| MacroTargets::parse(PACKED))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Read targets packed by `scripts/pack-macro-targets.py`.
    pub fn parse(packed: &[u8]) -> Result<Self, String> {
        let bytes = miniz_oxide::inflate::decompress_to_vec_zlib(packed)
            .map_err(|e| format!("The body targets cannot be unpacked: {e:?}"))?;
        let mut r = Reader {
            bytes: &bytes,
            at: 0,
        };
        if r.take(4)? != b"FCMH" || r.u32()? != 1 {
            return Err("These are not FashionCraft body targets of a known version".into());
        }
        let kept = r.u32()? as usize;
        if !(BODY_VERTICES..=100_000).contains(&kept) {
            return Err("The body targets keep an unexpected number of vertices".into());
        }
        r.take(4 * kept)?; // base-mesh indices, for reference
        let mut base = Vec::with_capacity(kept);
        for _ in 0..kept {
            base.push(DVec3::new(
                r.f32()? as f64,
                r.f32()? as f64,
                r.f32()? as f64,
            ));
        }
        let mut joints = Vec::new();
        for _ in 0..r.u32()? {
            let name = r.name()?;
            let n = r.u8()? as usize;
            let mut index = Vec::with_capacity(n);
            for _ in 0..n {
                let i = r.u16()? as usize;
                if i >= kept {
                    return Err(format!("Joint {name} points past the body"));
                }
                index.push(i);
            }
            if index.is_empty() {
                return Err(format!("Joint {name} has no helper"));
            }
            joints.push((name, index));
        }
        let mut targets = BTreeMap::new();
        for _ in 0..r.u32()? {
            let name = r.name()?;
            let n = r.u32()? as usize;
            if n > kept {
                return Err(format!("Target {name} moves more vertices than there are"));
            }
            let mut index = Vec::with_capacity(n);
            let mut at = 0u32;
            for _ in 0..n {
                at += r.u16()? as u32;
                if at as usize >= kept {
                    return Err(format!("Target {name} points past the body"));
                }
                index.push(at);
            }
            // x, y and z each as low bytes then high bytes of zigzagged steps.
            let mut columns: [Vec<i32>; 3] = Default::default();
            for column in &mut columns {
                let (low, high) = (r.take(n)?, r.take(n)?);
                let mut last = 0i32;
                column.extend(low.iter().zip(high).map(|(lo, hi)| {
                    let zigzag = *lo as u32 | (*hi as u32) << 8;
                    last += (zigzag >> 1) as i32 ^ -((zigzag & 1) as i32);
                    last
                }));
            }
            let [x, y, z] = &columns;
            let offset = x
                .iter()
                .zip(y)
                .zip(z)
                .map(|((x, y), z)| [*x, *y, *z])
                .collect();
            targets.insert(name, Target { index, offset });
        }
        if r.at != bytes.len() {
            return Err("The body targets have trailing data".into());
        }
        Ok(Self {
            base,
            joints,
            targets,
        })
    }

    pub fn target_names(&self) -> impl Iterator<Item = &str> {
        self.targets.keys().map(String::as_str)
    }

    pub fn joint_names(&self) -> impl Iterator<Item = &str> {
        self.joints.iter().map(|(n, _)| n.as_str())
    }

    /// Base positions of the body vertices, decimetres.
    pub fn base_body(&self) -> &[DVec3] {
        &self.base[..BODY_VERTICES]
    }

    /// A target's offset of kept vertex `vertex`, decimetres; none when it does not move it.
    pub fn offset(&self, target: &str, vertex: usize) -> Option<DVec3> {
        let t = self.targets.get(target)?;
        let k = t.index.binary_search(&(vertex as u32)).ok()?;
        let [x, y, z] = t.offset[k];
        Some(DVec3::new(x as f64, y as f64, z as f64) * 0.001)
    }

    /// The body for `settings`, in metres, standing on y = 0, with the mannequin's triangles.
    pub fn make(&self, settings: &MacroBody) -> Result<MadeBody, String> {
        settings.validate()?;
        let mut positions = self.base.clone();
        for (name, w) in settings.weights() {
            let Some(t) = self.targets.get(&name) else {
                continue;
            };
            let scale = w * 0.001;
            for (i, [x, y, z]) in t.index.iter().zip(&t.offset) {
                positions[*i as usize] += DVec3::new(*x as f64, *y as f64, *z as f64) * scale;
            }
        }
        let floor = positions[..BODY_VERTICES]
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min);
        let to_metres = |p: DVec3| DVec3::new(p.x, p.y - floor, p.z) * UNIT_M;
        let joints = self
            .joints
            .iter()
            .map(|(name, index)| {
                let sum: DVec3 = index.iter().map(|i| positions[*i]).sum();
                (name.clone(), to_metres(sum / index.len() as f64))
            })
            .collect();
        let surface = SurfaceMesh {
            vertices: positions[..BODY_VERTICES]
                .iter()
                .map(|p| to_metres(*p))
                .collect(),
            triangles: mannequin_triangles()?.to_vec(),
        };
        Ok(MadeBody { surface, joints })
    }
}

/// The mannequin's triangles, over the base mesh's body vertices.
fn mannequin_triangles() -> Result<&'static [[usize; 3]], String> {
    static TRIANGLES: OnceLock<Result<Vec<[usize; 3]>, String>> = OnceLock::new();
    TRIANGLES
        .get_or_init(|| {
            let mesh = SurfaceMesh::parse_obj(MANNEQUIN_OBJ)?;
            if mesh.vertices.len() != BODY_VERTICES {
                return Err("The mannequin is not MakeHuman's base body".into());
            }
            Ok(mesh.triangles)
        })
        .as_deref()
        .map_err(Clone::clone)
}
