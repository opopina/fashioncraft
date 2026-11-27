//! Bodies made from MakeHuman's CC0 targets: the stock mannequin's mesh (MakeHuman's base mesh,
//! hm08) morphed by gender, age, muscle, weight, height, proportions, bust and ancestry, and by
//! local measurements, with its joints where MakeHuman's joint helpers land.
//!
//! The targets are packed in `assets/makehuman/macro-targets.bin` by
//! `scripts/pack-macro-targets.py` (format there) and built into the program; only MakeHuman's
//! data is used. The settings follow Anny, NAVER LABS Europe's body model (Apache License 2.0):
//! each setting runs between anchors of its own, every variation a target is named after
//! ("universal-female-young-maxmuscle-averageweight") weighs how near the setting is to it, and
//! the target weighs their product. Age in years goes through Anny's calibrated mapping, and
//! [`MacroBody::typical`] and [`MacroBody::realistic`] follow Anny's distributions of height,
//! weight, muscle and proportions by age and sex, calibrated on WHO growth data
//! (`assets/makehuman/anny-calibration.json`).
use crate::avatar::SurfaceMesh;
use glam::DVec3;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

const PACKED: &[u8] = include_bytes!("../assets/makehuman/macro-targets.bin");
const MANNEQUIN_OBJ: &str = include_str!("../assets/makehuman/mannequin.obj");
const CALIBRATION: &str = include_str!("../assets/makehuman/anny-calibration.json");
/// MakeHuman's unit, the decimetre, in metres.
const UNIT_M: f64 = 0.1;
/// The base mesh's body vertices, which are the mannequin's, in the same order.
pub const BODY_VERTICES: usize = 13380;

/// Local measurements: each the name of a pair of measure targets (`measure-<name>-incr` and
/// `-decr`) and its label.
pub const MEASURES: [(&str, &str); 20] = [
    ("bust-circ", "Bust"),
    ("underbust-circ", "Underbust"),
    ("waist-circ", "Waist"),
    ("hips-circ", "Hips"),
    ("neck-circ", "Neck"),
    ("neck-height", "Neck height"),
    ("shoulder-dist", "Shoulder width"),
    ("frontchest-dist", "Front chest width"),
    ("napetowaist-dist", "Nape to waist"),
    ("waisttohip-dist", "Waist to hip"),
    ("upperarm-circ", "Upper arm"),
    ("upperarm-length", "Upper arm length"),
    ("lowerarm-length", "Forearm length"),
    ("wrist-circ", "Wrist"),
    ("thigh-circ", "Thigh"),
    ("upperleg-height", "Upper leg length"),
    ("knee-circ", "Knee"),
    ("calf-circ", "Calf"),
    ("lowerleg-height", "Lower leg length"),
    ("ankle-circ", "Ankle"),
];

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
    /// 0 shortest … 1 tallest.
    pub height: f64,
    /// 0 ideal … 1 uncommon.
    pub proportions: f64,
    /// Bust size, 0 smallest … 0.5 average … 1 largest (women's bodies).
    pub cupsize: f64,
    /// Bust firmness, 0 least … 0.5 average … 1 most (women's bodies).
    pub firmness: f64,
    /// Ancestry shares, African, Asian and European; only their ratios count.
    pub african: f64,
    pub asian: f64,
    pub european: f64,
    /// Each of [`MEASURES`], −1 smaller … 0 as the settings make it … 1 larger.
    pub measures: [f64; MEASURES.len()],
}

impl Default for MacroBody {
    /// A typical adult between a man and a woman, 25 years old.
    fn default() -> Self {
        Self::typical(0.5, 25.)
    }
}

impl MacroBody {
    /// The typical body for `gender` (0 female … 1 male) at `age_years`: the mean height, weight,
    /// muscle and proportions of Anny's distributions for girls and boys, blended by gender.
    pub fn typical(gender: f64, age_years: f64) -> Self {
        let mut body = Self {
            gender,
            age_years,
            muscle: 0.5,
            weight: 0.5,
            height: 0.5,
            proportions: 0.5,
            cupsize: 0.5,
            firmness: 0.5,
            african: 1. / 3.,
            asian: 1. / 3.,
            european: 1. / 3.,
            measures: [0.; MEASURES.len()],
        };
        if let Ok(c) = calibration() {
            let a = c.anny_age(age_years);
            let mean =
                |boys: &Beta, girls: &Beta| gender * boys.mean(a) + (1. - gender) * girls.mean(a);
            body.height = mean(&c.boys.height, &c.girls.height);
            body.weight = mean(&c.boys.weight, &c.girls.weight);
            body.muscle = mean(&c.boys.muscle, &c.girls.muscle);
            body.proportions = mean(&c.boys.proportions, &c.girls.proportions);
        }
        body
    }

    /// A body drawn at random as a real person of this body's gender and age might be built:
    /// height, weight, muscle and proportions from Anny's distributions for boys (gender ≥ 0.5)
    /// or girls, bust at random. Gender, age and ancestry stay; the measurements return to 0.
    pub fn realistic(&self, seed: u64) -> Result<Self, String> {
        let c = calibration()?;
        let a = c.anny_age(self.age_years);
        let sex = if self.gender >= 0.5 {
            &c.boys
        } else {
            &c.girls
        };
        let mut rng = Rng(seed);
        Ok(Self {
            height: sex.height.sample(a, &mut rng),
            weight: sex.weight.sample(a, &mut rng),
            muscle: sex.muscle.sample(a, &mut rng),
            proportions: sex.proportions.sample(a, &mut rng),
            cupsize: rng.unit(),
            firmness: rng.unit(),
            measures: [0.; MEASURES.len()],
            ..*self
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let unit = [
            self.gender,
            self.muscle,
            self.weight,
            self.height,
            self.proportions,
            self.cupsize,
            self.firmness,
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
        if self
            .measures
            .iter()
            .any(|v| !v.is_finite() || !(-1.0..=1.0).contains(v))
        {
            return Err("Measurements must be between −1 and 1".into());
        }
        if !self.age_years.is_finite() || !(1.0..=90.0).contains(&self.age_years) {
            return Err("The age must be 1 to 90 years".into());
        }
        if self.african + self.asian + self.european <= 1e-9 {
            return Err("Give the body some ancestry".into());
        }
        Ok(())
    }

    /// How much each variation a target can be named after counts for this body, as Anny
    /// weighs them: linear between each setting's anchors.
    pub fn variation_weights(&self) -> BTreeMap<&'static str, f64> {
        let mut out = BTreeMap::new();
        let mut put = |value: f64, anchors: &[f64], names: &[&'static str]| {
            for (name, w) in names.iter().zip(interpolation(value, anchors)) {
                out.insert(*name, w);
            }
        };
        let age = calibration().map_or(0.5, |c| c.anny_age(self.age_years));
        put(
            age,
            &[-1. / 3., 0., 1. / 3., 2. / 3., 1.],
            &["newborn", "baby", "child", "young", "old"],
        );
        // Anny's gender runs from male to female.
        put(1. - self.gender, &[0., 1.], &["male", "female"]);
        let three = [0., 0.5, 1.];
        put(
            self.muscle,
            &three,
            &["minmuscle", "averagemuscle", "maxmuscle"],
        );
        put(
            self.weight,
            &three,
            &["minweight", "averageweight", "maxweight"],
        );
        put(self.height, &[0., 1.], &["minheight", "maxheight"]);
        put(
            self.proportions,
            &[0., 1.],
            &["idealproportions", "uncommonproportions"],
        );
        put(self.cupsize, &three, &["mincup", "averagecup", "maxcup"]);
        put(
            self.firmness,
            &three,
            &["minfirmness", "averagefirmness", "maxfirmness"],
        );
        let total = self.african + self.asian + self.european;
        out.insert("african", self.african / total);
        out.insert("asian", self.asian / total);
        out.insert("caucasian", self.european / total);
        out.insert("universal", 1.);
        out
    }

    /// The weight of the target `name` for this body: a measurement's own setting, or the
    /// product of the variations it is named after; none for a name of unknown variations.
    pub fn target_weight(&self, name: &str, variations: &BTreeMap<&str, f64>) -> Option<f64> {
        if let Some(measure) = name.strip_prefix("measure/measure-") {
            let (key, sign) = match measure.strip_suffix("-incr") {
                Some(key) => (key, 1.),
                None => (measure.strip_suffix("-decr")?, -1.),
            };
            let k = MEASURES.iter().position(|(m, _)| *m == key)?;
            return Some((sign * self.measures[k]).max(0.));
        }
        let base = name.rsplit('/').next()?;
        base.split('-')
            .map(|token| variations.get(token).copied())
            .product()
    }
}

/// Linear interpolation weights of `value` between `anchors` (clamped to their range), one per
/// anchor, as Anny's `linear_interpolation_coefficients` without extrapolation.
fn interpolation(value: f64, anchors: &[f64]) -> Vec<f64> {
    let mut out = vec![0.; anchors.len()];
    let value = value.clamp(anchors[0], anchors[anchors.len() - 1]);
    let k = anchors
        .iter()
        .position(|a| value <= *a)
        .unwrap_or(anchors.len() - 1)
        .clamp(1, anchors.len() - 1);
    let t = (value - anchors[k - 1]) / (anchors[k] - anchors[k - 1]);
    out[k - 1] = 1. - t;
    out[k] = t;
    out
}

/// `y` at `x` on the polyline through `xs`, `ys`; past the ends, along the end segments
/// (`extrapolate`) or held at the end values.
fn polyline(x: f64, xs: &[f64], ys: &[f64], extrapolate: bool) -> f64 {
    let n = xs.len();
    let x = if extrapolate {
        x
    } else {
        x.clamp(xs[0], xs[n - 1])
    };
    let k = xs
        .iter()
        .position(|a| x <= *a)
        .unwrap_or(n - 1)
        .clamp(1, n - 1);
    let t = (x - xs[k - 1]) / (xs[k] - xs[k - 1]);
    ys[k - 1] + t * (ys[k] - ys[k - 1])
}

/// A Beta distribution whose parameters vary with Anny's age.
#[derive(serde::Deserialize)]
struct Beta {
    age_anchors: Vec<f64>,
    alpha_anchors: Vec<f64>,
    beta_anchors: Vec<f64>,
}

impl Beta {
    fn params(&self, age: f64) -> (f64, f64) {
        (
            polyline(age, &self.age_anchors, &self.alpha_anchors, false),
            polyline(age, &self.age_anchors, &self.beta_anchors, false),
        )
    }
    fn mean(&self, age: f64) -> f64 {
        let (a, b) = self.params(age);
        a / (a + b)
    }
    fn sample(&self, age: f64, rng: &mut Rng) -> f64 {
        let (a, b) = self.params(age);
        let (x, y) = (rng.gamma(a), rng.gamma(b));
        (x / (x + y)).clamp(0., 1.)
    }
}

#[derive(serde::Deserialize)]
struct AgeMapping {
    anny_age_anchors: Vec<f64>,
    morphological_age_anchors: Vec<f64>,
}

#[derive(serde::Deserialize)]
struct Sex {
    morphological_age_mapping: AgeMapping,
    #[serde(rename = "conditional_height_distribution")]
    height: Beta,
    #[serde(rename = "conditional_weight_distribution")]
    weight: Beta,
    #[serde(rename = "conditional_muscle_distribution")]
    muscle: Beta,
    #[serde(rename = "conditional_proportions_distribution")]
    proportions: Beta,
}

/// Anny's calibration: years to Anny's age, and its distributions for boys and girls.
#[derive(serde::Deserialize)]
struct Calibration {
    boys: Sex,
    girls: Sex,
}

impl Calibration {
    /// Anny's age parameter for an age in years (the same mapping for boys and girls).
    fn anny_age(&self, years: f64) -> f64 {
        let m = &self.boys.morphological_age_mapping;
        polyline(
            years,
            &m.morphological_age_anchors,
            &m.anny_age_anchors,
            true,
        )
    }
}

fn calibration() -> Result<&'static Calibration, String> {
    static PARSED: OnceLock<Result<Calibration, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            serde_json::from_str(CALIBRATION).map_err(|e| format!("Anny's calibration: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// A small deterministic random generator (SplitMix64) for drawing bodies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in (0, 1).
    fn unit(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        (-2. * self.unit().ln()).sqrt() * (std::f64::consts::TAU * self.unit()).cos()
    }
    /// Gamma(shape, 1) by Marsaglia and Tsang's method.
    fn gamma(&mut self, shape: f64) -> f64 {
        if shape < 1. {
            return self.gamma(shape + 1.) * self.unit().powf(1. / shape);
        }
        let d = shape - 1. / 3.;
        let c = 1. / (9. * d).sqrt();
        loop {
            let x = self.normal();
            let v = (1. + c * x).powi(3);
            if v <= 0. {
                continue;
            }
            let u = self.unit();
            if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
                return d * v;
            }
        }
    }
}

/// One target: offsets of some kept vertices, in 0.001 dm.
struct Target {
    index: Vec<u32>,
    offset: Vec<[i32; 3]>,
}

/// MakeHuman's base mesh (its body and joint helpers) and its macro targets.
pub struct MacroTargets {
    /// Base positions of the kept vertices, decimetres: the body, then the helpers (joints,
    /// and those the bundled parts are fitted to).
    base: Vec<DVec3>,
    /// The base-mesh index of each kept vertex, ascending.
    kept: Arc<[u32]>,
    /// Joint names and the kept vertices of their helpers.
    joints: Vec<(String, Vec<usize>)>,
    targets: BTreeMap<String, Target>,
}

/// A body made from the targets.
pub struct MadeBody {
    pub surface: SurfaceMesh,
    /// Joint landmarks, metres, as the mannequin's `measurements.json` names them.
    pub joints: BTreeMap<String, DVec3>,
    /// The settings it was made with.
    pub settings: MacroBody,
    /// Every kept base-mesh vertex as made, decimetres, before standing it on y = 0.
    positions: Vec<DVec3>,
    kept: Arc<[u32]>,
    floor: f64,
}

impl MadeBody {
    /// A part fitted to this body, metres, in the body's frame.
    pub fn fit(&self, fitting: &crate::avatar_look::Fitting) -> Result<Vec<DVec3>, String> {
        let at = |i: usize| {
            let k = self.kept.binary_search(&u32::try_from(i).ok()?).ok()?;
            Some(self.positions[k])
        };
        Ok(fitting
            .fit(at)?
            .into_iter()
            .map(|p| DVec3::new(p.x, p.y - self.floor, p.z) * UNIT_M)
            .collect())
    }
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
        let mut base_index = Vec::with_capacity(kept);
        for k in 0..kept {
            let i = r.u32()?;
            let expected = if k < BODY_VERTICES {
                k as u32
            } else {
                base_index[k - 1] + 1
            };
            if (k < BODY_VERTICES && i != expected) || i < expected {
                return Err("The body targets' vertices are out of order".into());
            }
            base_index.push(i);
        }
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
            kept: base_index.into(),
            joints,
            targets,
        })
    }

    /// The targets `body` is made of, with their weights; those weighing nothing are left out.
    pub fn weights(&self, body: &MacroBody) -> Vec<(&str, f64)> {
        let variations = body.variation_weights();
        self.targets
            .keys()
            .filter_map(|name| {
                let w = body.target_weight(name, &variations)?;
                (w > 1e-9).then_some((name.as_str(), w))
            })
            .collect()
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
    /// How tall the body these settings make is, in cm: its lowest point to its highest, as
    /// the avatar's measurements give its height.
    pub fn height_cm(&self, settings: &MacroBody) -> Result<f64, String> {
        let made = self.make(settings)?;
        let (lo, hi) = made
            .surface
            .vertices
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.y), hi.max(p.y))
            });
        Ok((hi - lo) * 100.)
    }

    /// The `height` setting (0 shortest … 1 tallest) that makes the body `cm` tall with the
    /// other settings as they are, to within a millimetre. Gender and age set how tall a body
    /// can be: outside that range it says the range.
    pub fn height_for(&self, settings: &MacroBody, cm: f64) -> Result<f64, String> {
        let at = |h: f64| {
            let mut s = *settings;
            s.height = h;
            self.height_cm(&s)
        };
        let (shortest, tallest) = (at(0.)?, at(1.)?);
        if !(shortest - 0.5..=tallest + 0.5).contains(&cm) {
            return Err(format!(
                "A body of these settings is {shortest:.0} to {tallest:.0} cm tall: {cm:.0} cm is \
                 out of reach (gender and age set the range)"
            ));
        }
        let (mut lo, mut hi) = (0., 1.);
        for _ in 0..14 {
            let mid = (lo + hi) / 2.;
            if at(mid)? < cm {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Ok((lo + hi) / 2.)
    }

    pub fn make(&self, settings: &MacroBody) -> Result<MadeBody, String> {
        settings.validate()?;
        let mut positions = self.base.clone();
        for (name, w) in self.weights(settings) {
            let Some(t) = self.targets.get(name) else {
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
        Ok(MadeBody {
            surface,
            joints,
            settings: *settings,
            positions,
            kept: self.kept.clone(),
            floor,
        })
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

/// Whether `surface` is made of MakeHuman's base body, as the mannequin and every made body
/// are: the same vertices and triangles, so it has MakeHuman's texture coordinates.
pub fn is_makehuman_body(surface: &SurfaceMesh) -> bool {
    surface.vertices.len() == BODY_VERTICES
        && mannequin_triangles().is_ok_and(|t| t == surface.triangles.as_slice())
}
