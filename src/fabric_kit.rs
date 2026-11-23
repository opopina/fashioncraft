//! Fabric kit: turn a workshop's measurements of a real fabric into a FashionCraft fabric.
//!
//! The workshop measures strips under hanging weights (warp, weft and bias), the overhang at
//! which a strip on a cantilever bench touches a 41.5° ramp (warp and weft), the weight of a
//! known area and the thickness; optionally the angle at which fabric slides on fabric. The
//! procedure is in `docs/fabric-kit.md`.
//!
//! [`fit`] searches for the [`FabricProperties`] whose virtual copies of the same tests, run to
//! static equilibrium by FashionCraft's own CPU reference solver ([`ReferenceSolver`]) on the
//! same material model the garment simulation uses, give the measured readings. Each parameter
//! is found by a bracketed secant search on the solver's result; the warp and weft bending
//! rigidities, which are coupled through the hinge model, are alternated until neither moves.
//! Formulas only give the searches their starting points.
use crate::{
    energy,
    garment::template::FabricProperties,
    model::{Material, Mesh, Pattern, Scene, State},
    solver::{ReferenceSolver, SolverSettings},
};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Gravity of the kit, as the simulation uses it, m/s².
pub const GRAVITY: f64 = 9.81;
/// Angle of the cantilever ramp below the horizontal, degrees (as in ASTM D1388).
pub const CANTILEVER_ANGLE_DEGREES: f64 = 41.5;
/// Extension of a measured fabric file in the library.
pub const FILE_EXTENSION: &str = "fcfabric";
const FORMAT: &str = "fashioncraft.fabric";
const VERSION: u32 = 1;

/// Direction a test strip is cut along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Warp,
    Weft,
    /// 45° between warp and weft.
    Bias,
}

impl Direction {
    pub const ALL: [Self; 3] = [Self::Warp, Self::Weft, Self::Bias];
    pub fn label(self) -> &'static str {
        match self {
            Self::Warp => "Warp",
            Self::Weft => "Weft",
            Self::Bias => "Bias",
        }
    }
    /// Grain of a virtual strip whose length runs along material v: the warp's angle from u
    /// toward v, degrees.
    fn grain_degrees(self) -> f64 {
        match self {
            Self::Warp => 90.,
            Self::Weft => 0.,
            Self::Bias => 45.,
        }
    }
}

/// One reading of the strip test.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    /// Everything hanging from the strip: the bottom clamp and its weights, grams.
    pub load_g: f64,
    /// Distance between the two gauge marks under that load, millimetres.
    pub length_mm: f64,
}

/// What the workshop measured, in the units of its tools.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurements {
    /// Mass per area, g/m².
    pub gsm: f64,
    /// Thickness, mm.
    pub thickness_mm: f64,
    /// Width of the tensile strips, mm.
    pub strip_width_mm: f64,
    /// Free length of a tensile strip between its two clamps, mm.
    pub strip_length_mm: f64,
    /// Distance between the two marks on the relaxed strip, centred between the clamps, mm.
    pub gauge_mm: f64,
    /// Smallest length the ruler reads, mm; only used to warn about coarse readings.
    pub resolution_mm: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warp: Vec<Reading>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weft: Vec<Reading>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bias: Vec<Reading>,
    /// Mean overhang at which a warp strip's tip touches the ramp, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overhang_warp_mm: Option<f64>,
    /// Mean overhang at which a weft strip's tip touches the ramp, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overhang_weft_mm: Option<f64>,
    /// Incline angle at which a fabric-covered block starts to slide on the fabric, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_angle_degrees: Option<f64>,
}

impl Default for Measurements {
    fn default() -> Self {
        Self {
            gsm: 150.,
            thickness_mm: 0.3,
            strip_width_mm: 50.,
            strip_length_mm: 200.,
            gauge_mm: 100.,
            resolution_mm: 0.5,
            warp: vec![],
            weft: vec![],
            bias: vec![],
            overhang_warp_mm: None,
            overhang_weft_mm: None,
            slide_angle_degrees: None,
        }
    }
}

impl Measurements {
    pub fn readings(&self, d: Direction) -> &[Reading] {
        match d {
            Direction::Warp => &self.warp,
            Direction::Weft => &self.weft,
            Direction::Bias => &self.bias,
        }
    }
    pub fn readings_mut(&mut self, d: Direction) -> &mut Vec<Reading> {
        match d {
            Direction::Warp => &mut self.warp,
            Direction::Weft => &mut self.weft,
            Direction::Bias => &mut self.bias,
        }
    }
    /// Overhang of a cantilever direction (warp or weft), mm.
    pub fn overhang_mm(&self, d: Direction) -> Option<f64> {
        match d {
            Direction::Warp => self.overhang_warp_mm,
            Direction::Weft => self.overhang_weft_mm,
            Direction::Bias => None,
        }
    }
    /// Coulomb friction coefficient of fabric on fabric, from the slide angle.
    pub fn friction(&self) -> Option<f64> {
        self.slide_angle_degrees.map(|a| a.to_radians().tan())
    }

    pub fn validate(&self) -> Result<(), String> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !within(self.gsm, 5., 3000.) {
            return Err("Area weight must be between 5 and 3000 g/m²".into());
        }
        if !within(self.thickness_mm, 0.01, 10.) {
            return Err("Thickness must be between 0.01 and 10 mm".into());
        }
        if !within(self.strip_width_mm, 10., 200.) {
            return Err("Strip width must be between 10 and 200 mm".into());
        }
        if !within(self.gauge_mm, 20., 800.) {
            return Err("Gauge length must be between 20 and 800 mm".into());
        }
        if !within(self.strip_length_mm, self.gauge_mm + 20., 1000.) {
            return Err(
                "Free strip length must be at least the gauge length plus 20 mm, at most 1000 mm"
                    .into(),
            );
        }
        if !within(self.resolution_mm, 0.01, 5.) {
            return Err("Ruler resolution must be between 0.01 and 5 mm".into());
        }
        for d in Direction::ALL {
            for r in self.readings(d) {
                if !within(r.load_g, 1., 50_000.) {
                    return Err("Each load must be between 1 g and 50 kg".into());
                }
                if !r.length_mm.is_finite() || r.length_mm <= self.gauge_mm {
                    return Err(
                        "Each reading must be longer than the gauge length: the strip stretches"
                            .into(),
                    );
                }
                if r.length_mm > self.gauge_mm * 3. {
                    return Err("A reading is more than three times the gauge length".into());
                }
            }
        }
        if self.warp.is_empty() && self.weft.is_empty() {
            return Err("Measure at least the warp or the weft strip".into());
        }
        for o in [self.overhang_warp_mm, self.overhang_weft_mm]
            .into_iter()
            .flatten()
        {
            if !within(o, 5., 1000.) {
                return Err("Cantilever overhang must be between 5 and 1000 mm".into());
            }
        }
        if self
            .slide_angle_degrees
            .is_some_and(|a| !within(a, 1., 56.))
        {
            return Err("Slide angle must be between 1 and 56°".into());
        }
        Ok(())
    }
}

/// How finely the virtual tests are meshed and how closely the searches match.
#[derive(Clone, Copy, Debug)]
pub struct FitOptions {
    /// Cells across a virtual tensile strip (even).
    pub strip_cells: usize,
    /// Cells along the overhang of a virtual cantilever.
    pub cantilever_cells: usize,
    /// Cells across a virtual cantilever: three behave as a wide strip within 1 %, two do
    /// not (edge hinges weigh more).
    pub cantilever_across: usize,
    /// Relative tolerance of every search: strain ratios, rigidity ratios.
    pub tolerance: f64,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            strip_cells: 8,
            cantilever_cells: 32,
            cantilever_across: 3,
            tolerance: 1e-3,
        }
    }
}

/// A parameter of the fitted fabric.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parameter {
    Warp,
    Weft,
    Shear,
    BendWarp,
    BendWeft,
}

/// Something the user should know about a fit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Note {
    /// No strip readings in `direction`: its stiffness copies `from`'s.
    StretchCopied {
        direction: Direction,
        from: Direction,
    },
    /// No bias strip: shear is estimated from warp and weft.
    ShearEstimated,
    /// One cantilever direction missing: both bend like the measured one.
    BendingCopied {
        direction: Direction,
        from: Direction,
    },
    /// No cantilever: bending keeps the library default.
    BendingDefault,
    /// The largest load stretches the gauge so little that one ruler step changes the
    /// stiffness by `percent`.
    CoarseReading { direction: Direction, percent: f64 },
    /// The model's single stiffness per direction cannot follow the measured curve: the
    /// worst reading's extension is off by `percent`.
    CurveMismatch { direction: Direction, percent: f64 },
    /// The search stopped at the parameter's limit: the model cannot reproduce the reading.
    AtLimit { parameter: Parameter },
}

impl Note {
    /// The note in English.
    pub fn english(&self) -> String {
        match self {
            Self::StretchCopied { direction, from } => format!(
                "No {} strip: it stretches like the {}",
                direction.label().to_lowercase(),
                from.label().to_lowercase()
            ),
            Self::ShearEstimated => {
                "No bias strip: shear is estimated as 4 % of the warp and weft stiffness".into()
            }
            Self::BendingCopied { direction, from } => format!(
                "No {} cantilever: it bends like the {}",
                direction.label().to_lowercase(),
                from.label().to_lowercase()
            ),
            Self::BendingDefault => "No cantilever: bending keeps the default".into(),
            Self::CoarseReading { direction, percent } => format!(
                "{}: one ruler step changes the stiffness by {percent:.0} %; use a heavier load or a longer gauge",
                direction.label()
            ),
            Self::CurveMismatch { direction, percent } => format!(
                "{}: one stiffness cannot follow the measured curve; worst extension off by {percent:.0} %",
                direction.label()
            ),
            Self::AtLimit { parameter } => format!(
                "{} reached its limit: the model cannot reproduce that reading",
                match parameter {
                    Parameter::Warp => "Warp stiffness",
                    Parameter::Weft => "Weft stiffness",
                    Parameter::Shear => "Shear",
                    Parameter::BendWarp => "Warp bending",
                    Parameter::BendWeft => "Weft bending",
                }
            ),
        }
    }
}

/// A virtual test.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Test {
    Strip { direction: Direction, load_g: f64 },
    Cantilever { direction: Direction },
}

/// A measured reading beside the one the fitted fabric gives in the virtual test.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub test: Test,
    pub measured_mm: f64,
    pub simulated_mm: f64,
}

/// What [`fit`] reports while it runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Strip(Direction),
    Cantilever(Direction),
    Checking,
}

#[derive(Clone, Debug)]
pub struct FitReport {
    pub properties: FabricProperties,
    pub friction: Option<f64>,
    pub checks: Vec<Check>,
    pub notes: Vec<Note>,
    /// Virtual tests the solver ran to static equilibrium.
    pub solves: usize,
}

/// A measured fabric as the library keeps it: the fitted properties with the measurements
/// they came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FabricFile {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub properties: FabricProperties,
    pub measurements: Measurements,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friction: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
}

impl FabricFile {
    pub fn new(name: &str, measurements: &Measurements, report: &FitReport) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            name: name.trim().into(),
            properties: report.properties.clone(),
            measurements: measurements.clone(),
            friction: report.friction,
            checks: report.checks.clone(),
            notes: report.notes.clone(),
        }
    }
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: Self =
            serde_json::from_str(text).map_err(|e| format!("Not a fabric file: {e}"))?;
        if file.format != FORMAT {
            return Err("Not a FashionCraft fabric file".into());
        }
        if file.version > VERSION {
            return Err("The fabric file is from a newer FashionCraft".into());
        }
        file.properties.bind(0.).validate()?;
        Ok(file)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, self.to_json()?).map_err(|e| e.to_string())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::from_json(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
    }
}

/// A file name for a fabric called `name`: letters, digits, spaces, dashes, underscores and
/// parentheses; anything else becomes an underscore.
pub fn file_name(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_ ()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim();
    format!(
        "{}.{FILE_EXTENSION}",
        if stem.is_empty() { "fabric" } else { stem }
    )
}

// ---------------------------------------------------------------------------------------------
// Analytic references: the constitutive law and the heavy elastica.

/// Nominal traction (N/m) of a strip stretched to `stretch` along an axis of membrane modulus
/// `k`, the orthogonal stretch one and no shear: dW/dλ of `energy::axial`.
pub fn axial_traction(stretch: f64, k: f64, p: &FabricProperties) -> f64 {
    let e = (stretch * stretch - 1.) * 0.5;
    let delta = p.compression_transition;
    let root = (e * e + delta * delta).sqrt();
    let positive = (e + root - delta) * 0.5;
    let derivative = (1. + e / root) * 0.5;
    k * stretch * (p.compression_ratio * e + (1. - p.compression_ratio) * positive * derivative)
}

/// Engineering strain at which an axis of modulus `k` carries `traction` N/m.
fn axial_strain(traction: f64, k: f64, p: &FabricProperties) -> f64 {
    let (mut lo, mut hi) = (1., 2.);
    while axial_traction(hi, k, p) < traction && hi < 100. {
        hi *= 2.;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if axial_traction(mid, k, p) < traction {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi) - 1.
}

/// Modulus giving `strain` under `traction`.
fn axial_modulus(traction: f64, strain: f64, p: &FabricProperties) -> f64 {
    traction / axial_traction(1. + strain, 1., p)
}

/// Small-strain modulus of a bias strip: 1/k = (1/k_warp + 1/k_weft + 1/shear) / 4.
fn bias_modulus(p: &FabricProperties) -> f64 {
    4. / (1. / p.warp + 1. / p.weft + 1. / p.shear)
}

/// Heavy elastica clamped horizontally at s = 0 and free at s = 1, for λ = qL³/G:
/// φ'' = −λ(1 − s)cos φ with φ(0) = 0, φ'(1) = 0. Integrates from the clamp curvature `p` and
/// returns φ'(1) with the path (x, drop) at the steps' ends, per unit length.
fn elastica_shot(lambda: f64, p: f64) -> (f64, Vec<[f64; 2]>) {
    const STEPS: usize = 400;
    let h = 1. / STEPS as f64;
    let rhs = |s: f64, y: [f64; 4]| -> [f64; 4] {
        [
            y[1],
            -lambda * (1. - s) * y[0].cos(),
            y[0].cos(),
            y[0].sin(),
        ]
    };
    let mut y = [0., p, 0., 0.];
    let mut path = Vec::with_capacity(STEPS + 1);
    path.push([0., 0.]);
    for i in 0..STEPS {
        let s = i as f64 * h;
        let add = |a: [f64; 4], b: [f64; 4], f: f64| std::array::from_fn(|k| a[k] + f * b[k]);
        let k1 = rhs(s, y);
        let k2 = rhs(s + 0.5 * h, add(y, k1, 0.5 * h));
        let k3 = rhs(s + 0.5 * h, add(y, k2, 0.5 * h));
        let k4 = rhs(s + h, add(y, k3, h));
        y = std::array::from_fn(|k| y[k] + h / 6. * (k1[k] + 2. * k2[k] + 2. * k3[k] + k4[k]));
        path.push([y[2], y[3]]);
    }
    (y[1], path)
}

/// Path (x, drop) of the heavy elastica with λ = qL³/G, per unit length, at 401 even steps.
fn elastica_path(lambda: f64) -> Vec<[f64; 2]> {
    if lambda <= 0. {
        return elastica_shot(0., 0.).1;
    }
    // φ'(1) rises with the clamp curvature p; 0 < p ≤ λ/2 because cos φ ≤ 1.
    let (mut lo, mut hi) = (0., 0.5 * lambda);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if elastica_shot(lambda, mid).0 < 0. {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    elastica_shot(lambda, 0.5 * (lo + hi)).1
}

/// Angle below the horizontal (radians) of the chord from the clamp to the tip of a heavy
/// elastica with λ = qL³/G (q its weight per length, G its flexural rigidity).
pub fn elastica_chord_angle(lambda: f64) -> f64 {
    let [x, y] = *elastica_path(lambda).last().unwrap();
    y.atan2(x)
}

/// Table of (ln λ, chord angle) for inverting [`elastica_chord_angle`].
fn elastica_table() -> &'static [(f64, f64)] {
    static TABLE: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let (a, b, n) = (1e-3_f64.ln(), 1e3_f64.ln(), 360);
        (0..=n)
            .map(|i| {
                let l = a + (b - a) * i as f64 / n as f64;
                (l, elastica_chord_angle(l.exp()))
            })
            .collect()
    })
}

/// λ = qL³/G at which the heavy elastica's chord reaches `angle` (radians).
pub fn elastica_lambda(angle: f64) -> f64 {
    let table = elastica_table();
    let i = table.partition_point(|(_, a)| *a < angle);
    if i == 0 {
        // Small deflection: the chord angle is λ/8.
        return (8. * angle).max(1e-12);
    }
    if i >= table.len() {
        return table.last().unwrap().0.exp();
    }
    let ((l0, a0), (l1, a1)) = (table[i - 1], table[i]);
    (l0 + (l1 - l0) * (angle - a0) / (a1 - a0)).exp()
}

/// λ* at which the chord reaches the ramp's 41.5°, solved on the elastica itself.
pub fn ramp_lambda() -> f64 {
    static LAMBDA: OnceLock<f64> = OnceLock::new();
    *LAMBDA.get_or_init(|| {
        let target = CANTILEVER_ANGLE_DEGREES.to_radians();
        let (mut lo, mut hi) = (1., 50.);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if elastica_chord_angle(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    })
}

/// Overhang (m) at which a strip of flexural rigidity `rigidity` (N·m) and area weight `gsm`
/// touches the ramp, from the exact heavy elastica.
pub fn analytic_overhang(rigidity: f64, gsm: f64) -> f64 {
    (ramp_lambda() * rigidity / (gsm / 1000. * GRAVITY)).cbrt()
}

/// Flexural rigidity (N·m) from a measured overhang (m), by the exact elastica.
pub fn rigidity_from_overhang(overhang: f64, gsm: f64) -> f64 {
    gsm / 1000. * GRAVITY * overhang.powi(3) / ramp_lambda()
}

/// Flexural rigidity (N·m) by the textile standard's formula G = w·g·c³ with the bending
/// length c = overhang / 2.
pub fn standard_rigidity(overhang: f64, gsm: f64) -> f64 {
    gsm / 1000. * GRAVITY * (overhang / 2.).powi(3)
}

/// Continuum rigidities (warp, weft) of the hinge model on the virtual tests' square grid of
/// alternating diagonals, small curvature: each hinge adds ½·3|e|²/(A₁+A₂)·B·θ², which sums to
/// G_warp = (5·bend_warp + bend_weft)/3 and G_weft = (bend_warp + 5·bend_weft)/3.
pub fn grid_rigidity(p: &FabricProperties) -> [f64; 2] {
    [
        (5. * p.bend_warp + p.bend_weft) / 3.,
        (p.bend_warp + 5. * p.bend_weft) / 3.,
    ]
}

/// The same on a mesh of equilateral triangles, the even statistics of the garment mesher:
/// G_warp = 1.5·bend_warp + 0.5·bend_weft.
pub fn isotropic_mesh_rigidity(p: &FabricProperties) -> [f64; 2] {
    [
        1.5 * p.bend_warp + 0.5 * p.bend_weft,
        0.5 * p.bend_warp + 1.5 * p.bend_weft,
    ]
}

// ---------------------------------------------------------------------------------------------
// The virtual laboratory.

/// Masses of a virtual test are scaled down by this and gravity up by its inverse: every
/// weight stays the same while inertia all but vanishes, so each implicit step of the reference
/// solver is a static Newton solve with a tiny proximal term.
const MASS_SCALE: f64 = 1e-4;
/// Most steps one settle may take.
const MAX_SETTLE_STEPS: usize = 40;
/// Pinned rows of a cantilever: the strip lying on the bench behind its edge.
const BENCH_ROWS: usize = 2;

/// Settings of one settle: `tolerance` is the residual force (N, RMS per coordinate) taken
/// as equilibrium.
fn lab_settings(gravity_share: f64, tolerance: f64) -> SolverSettings {
    SolverSettings {
        dt: 0.1,
        gravity: DVec3::new(0., -GRAVITY * gravity_share / MASS_SCALE, 0.),
        damping: 0.,
        newton_iterations: 60,
        pcg_iterations: 4000,
        absolute_tolerance: tolerance,
        relative_tolerance: 1e-12,
    }
}

/// Bookkeeping shared by the virtual tests of one fit.
struct Lab<'a> {
    options: FitOptions,
    solves: usize,
    cancel: &'a AtomicBool,
    /// For each strip direction and load, the clamp's drop at equilibrium over the
    /// constitutive law's: the next search evaluation starts from it.
    drops: std::collections::HashMap<(u8, u64), f64>,
}

impl<'a> Lab<'a> {
    fn new(options: FitOptions, cancel: &'a AtomicBool) -> Self {
        Self {
            options,
            solves: 0,
            cancel,
            drops: Default::default(),
        }
    }
}

impl Lab<'_> {
    /// Runs the reference solver until a step starts at equilibrium: residual forces below
    /// `tolerance` N. With inertia scaled away each step is a static Newton solve, so this is
    /// one solving step and one confirming step unless Newton runs out of iterations.
    fn settle(
        &mut self,
        mesh: &Mesh,
        state: &mut State,
        material: &Material,
        gravity_share: f64,
        tolerance: f64,
    ) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        self.solves += 1;
        let settings = lab_settings(gravity_share, tolerance);
        let mut converged = false;
        for _ in 0..MAX_SETTLE_STEPS {
            let result = ReferenceSolver::step(mesh, state, material, &settings, None);
            match result {
                Ok(metrics) => {
                    state.v.fill(DVec3::ZERO);
                    if metrics.converged && metrics.newton_iterations == 0 {
                        return Ok(());
                    }
                    converged |= metrics.converged;
                }
                // Already converged once: the line search can only fail on round-off, and the
                // step left the converged state as it was.
                Err(_) if converged => return Ok(()),
                Err(e) => return Err(e),
            }
        }
        Err("A virtual test did not settle".into())
    }
}

/// A virtual tensile strip hanging between two clamps: length along material v, world −y.
struct Strip {
    mesh: Mesh,
    state: State,
    material: Material,
    nx: usize,
    ny: usize,
    length: f64,
    gauge: f64,
    rest: Vec<DVec3>,
    drop: f64,
}

impl Strip {
    fn new(
        p: &FabricProperties,
        d: Direction,
        m: &Measurements,
        options: &FitOptions,
    ) -> Result<Self, String> {
        let width = m.strip_width_mm / 1000.;
        let length = m.strip_length_mm / 1000.;
        let nx = options.strip_cells.max(2).div_ceil(2) * 2;
        let ny = ((length * nx as f64 / width).round() as usize).clamp(4, 128);
        let material = p.bind(d.grain_degrees());
        let (mut mesh, state) = Pattern {
            width,
            height: length,
            nx,
            ny,
        }
        .build(&material, Scene::Hanging)?;
        let mut state = state;
        // Top clamp at the origin: small coordinates keep the energy's round-off small.
        let top = state.x[nx / 2];
        for x in &mut state.x {
            *x -= top;
        }
        for mass in &mut mesh.masses {
            *mass *= MASS_SCALE;
        }
        mesh.pins.fill(None);
        for i in (0..=nx).chain(ny * (nx + 1)..=ny * (nx + 1) + nx) {
            mesh.pins[i] = Some(state.x[i]);
        }
        Ok(Self {
            rest: state.x.clone(),
            mesh,
            state,
            material,
            nx,
            ny,
            length,
            gauge: m.gauge_mm / 1000.,
            drop: 0.,
        })
    }
    fn is_bottom(&self, i: usize) -> bool {
        i >= self.ny * (self.nx + 1)
    }
    /// Lowers the bottom clamp to `drop` below rest, moving the free cloth evenly with it as
    /// the solver's starting point.
    fn set_drop(&mut self, drop: f64) {
        let change = drop - self.drop;
        for (i, x) in self.state.x.iter_mut().enumerate() {
            let along = self.mesh.uv[i].y / self.length;
            *x -= DVec3::Y * change * along;
        }
        let bottom = self.ny * (self.nx + 1);
        for (pin, rest) in self.mesh.pins[bottom..]
            .iter_mut()
            .zip(&self.rest[bottom..])
        {
            *pin = Some(*rest - DVec3::Y * drop);
        }
        self.drop = drop;
    }
    /// Weight (N) the bottom clamp carries: the cloth's pull on its row less that row's own
    /// weight.
    fn load(&self) -> f64 {
        let x = &self.state.x;
        let mut pull = 0.;
        let mut add = |dofs: &[usize], gradient: &[f64]| {
            for (k, dof) in dofs.iter().enumerate() {
                if dof % 3 == 1 && self.is_bottom(dof / 3) {
                    pull -= gradient[k];
                }
            }
        };
        for t in &self.mesh.triangles {
            if t.vertices.iter().any(|&v| self.is_bottom(v)) {
                let e = energy::membrane_element(x, t, &self.material);
                add(&e.dofs, &e.gradient);
            }
        }
        for h in &self.mesh.hinges {
            if h.vertices.iter().any(|&v| self.is_bottom(v)) {
                let e = energy::bending_element(x, h, &self.material);
                add(&e.dofs, &e.gradient);
            }
        }
        let own: f64 = (0..x.len())
            .filter(|&i| self.is_bottom(i))
            .map(|i| self.mesh.masses[i] / MASS_SCALE * GRAVITY)
            .sum();
        pull - own
    }
    /// Position of the material point at height `v` on the strip's centre line.
    fn centre(&self, v: f64) -> DVec3 {
        let column = self.nx / 2;
        let r = (v / self.length * self.ny as f64).clamp(0., self.ny as f64);
        let row = (r.floor() as usize).min(self.ny - 1);
        let f = r - row as f64;
        let at = |j: usize| self.state.x[j * (self.nx + 1) + column];
        at(row).lerp(at(row + 1), f)
    }
    /// Distance between the two gauge marks, m.
    fn gauge_length(&self) -> f64 {
        let half = 0.5 * self.length;
        self.centre(half - 0.5 * self.gauge)
            .distance(self.centre(half + 0.5 * self.gauge))
    }
}

/// Gauge lengths (m) of a strip of `p` cut along `d` under each of `loads` (N), any order.
fn strip_lengths(
    lab: &mut Lab,
    p: &FabricProperties,
    d: Direction,
    m: &Measurements,
    loads: &[f64],
) -> Result<Vec<f64>, String> {
    let mut strip = Strip::new(p, d, m, &lab.options)?;
    let width = m.strip_width_mm / 1000.;
    let modulus = match d {
        Direction::Warp => p.warp,
        Direction::Weft => p.weft,
        Direction::Bias => bias_modulus(p),
    };
    let mut order: Vec<usize> = (0..loads.len()).collect();
    order.sort_by(|a, b| loads[*a].total_cmp(&loads[*b]));
    let mut out = vec![0.; loads.len()];
    // (drop, load) of the last equilibrium, starting from the unloaded strip.
    let mut last = (0., 0.);
    for i in order {
        let target = loads[i];
        let mut a = last;
        let key = (d as u8, target.to_bits());
        let law = (axial_strain(target / width, modulus, p) * strip.length).max(1e-7);
        let mut drop = law * lab.drops.get(&key).copied().unwrap_or(1.);
        if drop <= a.0 {
            drop = a.0 * 1.01 + 1e-7;
        }
        let mut converged = false;
        for _ in 0..40 {
            strip.set_drop(drop);
            let tolerance = 1e-6 * target / (strip.nx + 1) as f64;
            lab.settle(
                &strip.mesh,
                &mut strip.state,
                &strip.material,
                1.,
                tolerance,
            )?;
            let load = strip.load();
            if (load - target).abs() <= 1e-5 * target {
                converged = true;
                break;
            }
            // Secant on the drop, the load rising with it.
            let slope = (load - a.1) / (drop - a.0);
            let next = if slope.is_finite() && slope > 0. {
                drop + (target - load) / slope
            } else {
                drop * 1.5
            };
            a = (drop, load);
            drop = next.clamp(0.25 * drop, 4. * drop).max(1e-9);
        }
        if !converged {
            return Err("A virtual strip did not reach its load".into());
        }
        last = (strip.drop, strip.load());
        lab.drops.insert(key, strip.drop / law);
        out[i] = strip.gauge_length();
    }
    Ok(out)
}

/// A virtual cantilever: a strip on a bench (its first rows pinned flat) overhanging the edge
/// by `overhang`, length along material v (world +z), drooping under its own weight.
struct Cantilever {
    mesh: Mesh,
    state: State,
    material: Material,
    nx: usize,
    ny: usize,
    direction: Direction,
    overhang: f64,
    /// Flat positions, the bench edge at the origin.
    rest: Vec<DVec3>,
    /// Residual force taken as equilibrium, N: a millionth of a vertex's weight.
    tolerance: f64,
    /// Ratio of the λ the last settle showed to the one its starting shape assumed: corrects
    /// the next start for the hinge model's rigidity on this mesh.
    correction: f64,
}

impl Cantilever {
    fn new(
        p: &FabricProperties,
        d: Direction,
        overhang: f64,
        options: &FitOptions,
    ) -> Result<Self, String> {
        let cells = options.cantilever_cells.clamp(4, 128 - BENCH_ROWS);
        let h = overhang / cells as f64;
        // Rigidity is per width and the model has no Poisson coupling, so a few cells across
        // are enough. `Pattern` wants 10 mm of width: build the grid larger, then shrink it.
        let nx = options.cantilever_across.max(1);
        let ny = cells + BENCH_ROWS;
        let scale = (0.0101 / (nx as f64 * h)).max(1.);
        let material = p.bind(d.grain_degrees());
        let (mut mesh, state) = Pattern {
            width: nx as f64 * h * scale,
            height: ny as f64 * h * scale,
            nx,
            ny,
        }
        .build(&material, Scene::Sphere)?;
        let mut state = state;
        if scale > 1. {
            shrink(&mut mesh, &mut state, 1. / scale);
        }
        // Bench edge at the origin: small coordinates keep the energy's round-off small.
        let edge = state.x[BENCH_ROWS * (nx + 1) + nx / 2];
        for x in &mut state.x {
            *x -= edge;
        }
        let weight = mesh.masses.iter().fold(0., |m: f64, v| m.max(*v)) * GRAVITY;
        for mass in &mut mesh.masses {
            *mass *= MASS_SCALE;
        }
        mesh.pins.fill(None);
        let bench = (BENCH_ROWS + 1) * (nx + 1);
        for (pin, x) in mesh.pins[..bench].iter_mut().zip(&state.x) {
            *pin = Some(*x);
        }
        Ok(Self {
            rest: state.x.clone(),
            mesh,
            state,
            material,
            nx,
            ny,
            direction: d,
            overhang,
            tolerance: 1e-6 * weight,
            // The grid's measured rigidity is about 2.3 B against the 2 B of `grid_rigidity`.
            correction: 0.87,
        })
    }
    fn row_mean(&self, j: usize) -> DVec3 {
        let row = &self.state.x[j * (self.nx + 1)..(j + 1) * (self.nx + 1)];
        row.iter().sum::<DVec3>() / row.len() as f64
    }
    /// Settles with bending rigidities `p` and returns the chord angle from the bench edge to
    /// the tip, radians below the horizontal. Newton starts from the exact elastica of the
    /// grid's small-curvature rigidity, a few iterations from the discrete equilibrium.
    fn chord_angle(&mut self, lab: &mut Lab, p: &FabricProperties) -> Result<f64, String> {
        self.material = p.bind(self.direction.grain_degrees());
        let rigidity = grid_rigidity(p)[usize::from(self.direction == Direction::Weft)];
        let guess = p.density * GRAVITY * self.overhang.powi(3) / rigidity.max(1e-30);
        let path = elastica_path((guess * self.correction).min(1e4));
        let steps = (path.len() - 1) as f64;
        let cells = (self.ny - BENCH_ROWS) as f64;
        for j in BENCH_ROWS + 1..=self.ny {
            let r = (j - BENCH_ROWS) as f64 / cells * steps;
            let k = (r.floor() as usize).min(path.len() - 2);
            let f = r - k as f64;
            let along = path[k][0] + f * (path[k + 1][0] - path[k][0]);
            let drop = path[k][1] + f * (path[k + 1][1] - path[k][1]);
            for i in 0..=self.nx {
                let n = j * (self.nx + 1) + i;
                self.state.x[n] =
                    DVec3::new(self.rest[n].x, -drop * self.overhang, along * self.overhang);
            }
        }
        self.state.v.fill(DVec3::ZERO);
        lab.settle(
            &self.mesh,
            &mut self.state,
            &self.material,
            1.,
            self.tolerance,
        )?;
        let edge = self.row_mean(BENCH_ROWS);
        let tip = self.row_mean(self.ny);
        let angle = (edge.y - tip.y).atan2(tip.z - edge.z);
        let shown = elastica_lambda(angle) / guess;
        if shown.is_finite() && shown > 0. {
            self.correction = shown.clamp(0.1, 10.);
        }
        Ok(angle)
    }
}

/// Scales a mesh's rest shape, masses and positions by `f`. Hinge weights 3|e|²/(A₁+A₂) and
/// directions do not change with scale.
fn shrink(mesh: &mut Mesh, state: &mut State, f: f64) {
    for uv in &mut mesh.uv {
        *uv *= f;
    }
    for t in &mut mesh.triangles {
        for r in &mut t.rest {
            *r *= f;
        }
        t.inv_dm *= 1. / f;
        t.area *= f * f;
    }
    for a in mesh.rest_area.iter_mut().chain(&mut mesh.masses) {
        *a *= f * f;
    }
    for x in &mut state.x {
        *x *= f;
    }
}

// ---------------------------------------------------------------------------------------------
// Search.

/// Finds where `f`, a decreasing function of ln x of slope near −1, crosses zero within
/// `[lo, hi]`: steps by the expected slope until the root is bracketed, then Illinois regula
/// falsi in ln x. Returns the root and whether the search stopped at a bound instead.
fn search(
    mut f: impl FnMut(f64) -> Result<f64, String>,
    start: f64,
    (lo, hi): (f64, f64),
    tolerance: f64,
) -> Result<(f64, bool), String> {
    let (llo, lhi) = (lo.ln(), hi.ln());
    let mut a = start.clamp(lo, hi).ln();
    let mut fa = f(a.exp())?;
    if fa.abs() <= tolerance {
        return Ok((a.exp(), false));
    }
    let mut step = fa;
    let (mut b, mut fb);
    let mut tries = 0;
    loop {
        b = (a + step.clamp(-4., 4.)).clamp(llo, lhi);
        if b == a {
            return Ok((a.exp(), true));
        }
        fb = f(b.exp())?;
        if fb.abs() <= tolerance {
            return Ok((b.exp(), false));
        }
        if fa.signum() != fb.signum() {
            break;
        }
        tries += 1;
        if tries > 30 {
            return Err("A search did not bracket its target".into());
        }
        let slope = (fb - fa) / (b - a);
        // Same side: extrapolate, overshooting a little so the next try brackets.
        step = if slope < 0. {
            -1.3 * fb / slope
        } else {
            2. * fb
        };
        a = b;
        fa = fb;
    }
    let mut side = 0;
    for _ in 0..80 {
        let c = (a * fb - b * fa) / (fb - fa);
        let fc = f(c.exp())?;
        if fc.abs() <= tolerance || (b - a).abs() < 1e-12 {
            return Ok((c.exp(), false));
        }
        if fc.signum() == fb.signum() {
            b = c;
            fb = fc;
            if side == -1 {
                fa *= 0.5;
            }
            side = -1;
        } else {
            a = c;
            fa = fc;
            if side == 1 {
                fb *= 0.5;
            }
            side = 1;
        }
    }
    Err("A search did not converge".into())
}

/// Membrane stiffness bounds, N/m.
const STIFFNESS: (f64, f64) = (1., 1e6);
/// Shear bounds, N/m.
const SHEAR: (f64, f64) = (0.01, 1e5);
/// Bending bounds, N·m.
const BENDING: (f64, f64) = (1e-10, 0.1);
/// Shear of an unmeasured bias, as a share of the warp and weft stiffness.
const SHEAR_SHARE: f64 = 0.04;

/// Weighted mean ln(simulated / measured extension) of the readings in `d`, and the simulated
/// lengths.
fn strip_residual(
    lab: &mut Lab,
    p: &FabricProperties,
    d: Direction,
    m: &Measurements,
) -> Result<(f64, Vec<f64>), String> {
    let readings = m.readings(d);
    let loads: Vec<f64> = readings
        .iter()
        .map(|r| r.load_g / 1000. * GRAVITY)
        .collect();
    let lengths = strip_lengths(lab, p, d, m, &loads)?;
    // Mean log ratio of the extensions, each reading weighted by its extension: a ruler step
    // matters less on a longer one.
    let gauge = m.gauge_mm / 1000.;
    let (mut sum, mut weights) = (0., 0.);
    for (r, l) in readings.iter().zip(&lengths) {
        let measured = r.length_mm / 1000. - gauge;
        sum += measured * ((l - gauge) / measured).max(1e-12).ln();
        weights += measured;
    }
    Ok((sum / weights, lengths))
}

/// Starting modulus for a strip direction: the law's modulus for each reading, geometric mean.
fn strip_guess(p: &FabricProperties, m: &Measurements, d: Direction) -> f64 {
    let readings = m.readings(d);
    let width = m.strip_width_mm / 1000.;
    let ln_sum: f64 = readings
        .iter()
        .map(|r| {
            let traction = r.load_g / 1000. * GRAVITY / width;
            let strain = r.length_mm / m.gauge_mm - 1.;
            axial_modulus(traction, strain, p).ln()
        })
        .sum();
    (ln_sum / readings.len() as f64).exp()
}

/// ln λ the virtual cantilever shows, less the ramp's: positive while it droops too far.
fn cantilever_residual(
    lab: &mut Lab,
    cantilever: &mut Cantilever,
    p: &FabricProperties,
) -> Result<f64, String> {
    let angle = cantilever.chord_angle(lab, p)?;
    Ok(elastica_lambda(angle).ln() - ramp_lambda().ln())
}

/// Fits a fabric to `m`. `progress` hears which test runs; `cancel` stops the fit.
pub fn fit(
    m: &Measurements,
    options: FitOptions,
    progress: &mut dyn FnMut(Stage),
    cancel: &AtomicBool,
) -> Result<FitReport, String> {
    m.validate()?;
    let mut lab = Lab::new(options, cancel);
    let tol = options.tolerance;
    let mut notes = vec![];
    let mut p = FabricProperties {
        density: m.gsm / 1000.,
        thickness: m.thickness_mm / 1000.,
        ..Default::default()
    };

    // Warp and weft stretch: each strip depends on its own modulus only.
    let mut fitted = [None, None];
    for (slot, d) in [Direction::Warp, Direction::Weft].into_iter().enumerate() {
        if m.readings(d).is_empty() {
            continue;
        }
        progress(Stage::Strip(d));
        let guess = strip_guess(&p, m, d);
        let (k, at_limit) = search(
            |k| {
                let mut q = p.clone();
                q.warp = k;
                q.weft = k;
                Ok(strip_residual(&mut lab, &q, d, m)?.0)
            },
            guess,
            STIFFNESS,
            tol,
        )?;
        if at_limit {
            notes.push(Note::AtLimit {
                parameter: if d == Direction::Warp {
                    Parameter::Warp
                } else {
                    Parameter::Weft
                },
            });
        }
        fitted[slot] = Some(k);
    }
    match fitted {
        [Some(w), Some(f)] => (p.warp, p.weft) = (w, f),
        [Some(w), None] => {
            (p.warp, p.weft) = (w, w);
            notes.push(Note::StretchCopied {
                direction: Direction::Weft,
                from: Direction::Warp,
            });
        }
        [None, Some(f)] => {
            (p.warp, p.weft) = (f, f);
            notes.push(Note::StretchCopied {
                direction: Direction::Warp,
                from: Direction::Weft,
            });
        }
        [None, None] => unreachable!("validate requires a warp or weft strip"),
    }

    // Shear from the bias strip, the warp and weft now known.
    p.shear = SHEAR_SHARE * (p.warp * p.weft).sqrt();
    if m.bias.is_empty() {
        notes.push(Note::ShearEstimated);
    } else {
        progress(Stage::Strip(Direction::Bias));
        let bias = strip_guess(&p, m, Direction::Bias);
        let compliance = 4. / bias - 1. / p.warp - 1. / p.weft;
        let guess = if compliance > 0. {
            1. / compliance
        } else {
            SHEAR.1
        };
        let (g, at_limit) = search(
            |g| {
                let mut q = p.clone();
                q.shear = g;
                Ok(strip_residual(&mut lab, &q, Direction::Bias, m)?.0)
            },
            guess,
            SHEAR,
            tol,
        )?;
        p.shear = g;
        if at_limit {
            notes.push(Note::AtLimit {
                parameter: Parameter::Shear,
            });
        }
    }

    // Bending from the cantilevers.
    let overhangs = [m.overhang_warp_mm, m.overhang_weft_mm].map(|o| o.map(|o| o / 1000.));
    match overhangs {
        [None, None] => notes.push(Note::BendingDefault),
        [Some(o), None] | [None, Some(o)] => {
            let d = if overhangs[0].is_some() {
                Direction::Warp
            } else {
                Direction::Weft
            };
            notes.push(Note::BendingCopied {
                direction: if d == Direction::Warp {
                    Direction::Weft
                } else {
                    Direction::Warp
                },
                from: d,
            });
            progress(Stage::Cantilever(d));
            let mut cantilever = Cantilever::new(&p, d, o, &options)?;
            // Isotropic on the grid: G = 2B.
            let guess = 0.5 * rigidity_from_overhang(o, m.gsm);
            let (b, at_limit) = search(
                |b| {
                    let mut q = p.clone();
                    q.bend_warp = b;
                    q.bend_weft = b;
                    cantilever_residual(&mut lab, &mut cantilever, &q)
                },
                guess,
                BENDING,
                tol,
            )?;
            (p.bend_warp, p.bend_weft) = (b, b);
            if at_limit {
                notes.push(Note::AtLimit {
                    parameter: if d == Direction::Warp {
                        Parameter::BendWarp
                    } else {
                        Parameter::BendWeft
                    },
                });
            }
        }
        [Some(ow), Some(of)] => {
            // Invert the grid's map from the elastica rigidities for the start.
            let gw = rigidity_from_overhang(ow, m.gsm);
            let gf = rigidity_from_overhang(of, m.gsm);
            p.bend_warp = ((5. * gw - gf) / 8.).max(0.02 * gw);
            p.bend_weft = ((5. * gf - gw) / 8.).max(0.02 * gf);
            let mut warp = Cantilever::new(&p, Direction::Warp, ow, &options)?;
            let mut weft = Cantilever::new(&p, Direction::Weft, of, &options)?;
            let mut limits = [false, false];
            for _ in 0..8 {
                let before = [p.bend_warp, p.bend_weft];
                progress(Stage::Cantilever(Direction::Warp));
                let (bw, lw) = search(
                    |b| {
                        let mut q = p.clone();
                        q.bend_warp = b;
                        cantilever_residual(&mut lab, &mut warp, &q)
                    },
                    p.bend_warp,
                    BENDING,
                    tol,
                )?;
                p.bend_warp = bw;
                progress(Stage::Cantilever(Direction::Weft));
                let (bf, lf) = search(
                    |b| {
                        let mut q = p.clone();
                        q.bend_weft = b;
                        cantilever_residual(&mut lab, &mut weft, &q)
                    },
                    p.bend_weft,
                    BENDING,
                    tol,
                )?;
                p.bend_weft = bf;
                limits = [lw, lf];
                let moved = (bw / before[0] - 1.).abs().max((bf / before[1] - 1.).abs());
                if moved <= 2. * tol {
                    break;
                }
            }
            for (limit, parameter) in limits
                .into_iter()
                .zip([Parameter::BendWarp, Parameter::BendWeft])
            {
                if limit {
                    notes.push(Note::AtLimit { parameter });
                }
            }
        }
    }
    p.bind(0.).validate()?;

    // Run every test once more with the fitted fabric.
    progress(Stage::Checking);
    let mut checks = vec![];
    for d in Direction::ALL {
        let readings = m.readings(d);
        if readings.is_empty() {
            continue;
        }
        let (_, lengths) = strip_residual(&mut lab, &p, d, m)?;
        let mut worst: f64 = 0.;
        for (r, l) in readings.iter().zip(&lengths) {
            let simulated_mm = l * 1000.;
            worst =
                worst.max(((simulated_mm - m.gauge_mm) / (r.length_mm - m.gauge_mm) - 1.).abs());
            checks.push(Check {
                test: Test::Strip {
                    direction: d,
                    load_g: r.load_g,
                },
                measured_mm: r.length_mm,
                simulated_mm,
            });
        }
        if worst > 0.15 {
            notes.push(Note::CurveMismatch {
                direction: d,
                percent: worst * 100.,
            });
        }
        let largest = readings.iter().map(|r| r.length_mm).fold(0., f64::max) - m.gauge_mm;
        let percent = m.resolution_mm / largest * 100.;
        if percent > 15. {
            notes.push(Note::CoarseReading {
                direction: d,
                percent,
            });
        }
    }
    for d in [Direction::Warp, Direction::Weft] {
        let Some(measured) = m.overhang_mm(d) else {
            continue;
        };
        let simulated = virtual_overhang(&mut lab, &p, d, measured / 1000.)?;
        checks.push(Check {
            test: Test::Cantilever { direction: d },
            measured_mm: measured,
            simulated_mm: simulated * 1000.,
        });
    }
    Ok(FitReport {
        properties: p,
        friction: m.friction(),
        checks,
        notes,
        solves: lab.solves,
    })
}

/// Overhang (m) at which the virtual cantilever of `p` cut along `d` touches the ramp,
/// searched from `start`.
fn virtual_overhang(
    lab: &mut Lab,
    p: &FabricProperties,
    d: Direction,
    start: f64,
) -> Result<f64, String> {
    let tolerance = lab.options.tolerance / 3.;
    let (o, _) = search(
        |o| {
            let options = lab.options;
            let mut cantilever = Cantilever::new(p, d, o, &options)?;
            // λ grows as L³: a third of its log is a decreasing residual of slope −1 in ln L
            // once negated.
            Ok(-cantilever_residual(lab, &mut cantilever, p)? / 3.)
        },
        start,
        (0.005, 2.),
        tolerance,
    )?;
    Ok(o)
}

/// The readings a fabric `p` gives in the virtual tests, with the geometry, loads and
/// cantilever directions of `template`: synthetic measurements.
pub fn virtual_readings(
    p: &FabricProperties,
    template: &Measurements,
    options: FitOptions,
) -> Result<Measurements, String> {
    let cancel = AtomicBool::new(false);
    let mut lab = Lab::new(options, &cancel);
    let mut m = template.clone();
    m.gsm = p.density * 1000.;
    m.thickness_mm = p.thickness * 1000.;
    for d in Direction::ALL {
        let loads: Vec<f64> = m
            .readings(d)
            .iter()
            .map(|r| r.load_g / 1000. * GRAVITY)
            .collect();
        if loads.is_empty() {
            continue;
        }
        let lengths = strip_lengths(&mut lab, p, d, &m, &loads)?;
        for (r, l) in m.readings_mut(d).iter_mut().zip(lengths) {
            r.length_mm = l * 1000.;
        }
    }
    for d in [Direction::Warp, Direction::Weft] {
        let Some(guess) = m.overhang_mm(d) else {
            continue;
        };
        let o = virtual_overhang(&mut lab, p, d, guess / 1000.)? * 1000.;
        match d {
            Direction::Warp => m.overhang_warp_mm = Some(o),
            _ => m.overhang_weft_mm = Some(o),
        }
    }
    Ok(m)
}

/// Chord angle (radians) of the virtual cantilever of `p` cut along `d` with `overhang` (m).
pub fn virtual_chord_angle(
    p: &FabricProperties,
    d: Direction,
    overhang: f64,
    options: FitOptions,
) -> Result<f64, String> {
    let cancel = AtomicBool::new(false);
    let mut lab = Lab::new(options, &cancel);
    Cantilever::new(p, d, overhang, &options)?.chord_angle(&mut lab, p)
}

/// Overhang (m) at which the virtual cantilever of `p` cut along `d` touches the ramp.
pub fn virtual_cantilever(
    p: &FabricProperties,
    d: Direction,
    options: FitOptions,
) -> Result<f64, String> {
    let cancel = AtomicBool::new(false);
    let mut lab = Lab::new(options, &cancel);
    let start = analytic_overhang(
        grid_rigidity(p)[usize::from(d == Direction::Weft)],
        p.density * 1000.,
    );
    virtual_overhang(&mut lab, p, d, start)
}

/// Gauge length (m) of the virtual strip of `p` cut along `d` under `load` N.
pub fn virtual_strip(
    p: &FabricProperties,
    d: Direction,
    m: &Measurements,
    load: f64,
    options: FitOptions,
) -> Result<f64, String> {
    let cancel = AtomicBool::new(false);
    let mut lab = Lab::new(options, &cancel);
    Ok(strip_lengths(&mut lab, p, d, m, &[load])?[0])
}

/// Engineering strain the constitutive law gives a strip of modulus `k` under `traction` N/m.
pub fn law_strain(traction: f64, k: f64, p: &FabricProperties) -> f64 {
    axial_strain(traction, k, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_deflection_elastica_is_the_linear_cantilever() {
        // Linear theory: tip drop qL⁴/(8G), so the chord angle is λ/8.
        for lambda in [1e-3, 1e-2] {
            let angle = elastica_chord_angle(lambda);
            assert!(
                (angle / (lambda / 8.) - 1.).abs() < 2e-3,
                "{lambda}: {angle}"
            );
        }
        let star = ramp_lambda();
        assert!((elastica_chord_angle(star) - CANTILEVER_ANGLE_DEGREES.to_radians()).abs() < 1e-9);
        assert!((elastica_lambda(CANTILEVER_ANGLE_DEGREES.to_radians()) / star - 1.).abs() < 1e-3);
    }

    #[test]
    fn search_finds_a_power_law_root_and_reports_limits() {
        let (x, limit) = search(|x: f64| Ok((2.5 / x).ln()), 1., (1e-3, 1e3), 1e-6).unwrap();
        assert!((x / 2.5 - 1.).abs() < 1e-5 && !limit);
        let (x, limit) = search(|x: f64| Ok((2.5 / x).ln()), 1., (1e-3, 1.), 1e-6).unwrap();
        assert!(limit && (x - 1.).abs() < 1e-12);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("Denim 12 oz / raw"), "Denim 12 oz _ raw.fcfabric");
        assert_eq!(file_name("  "), "fabric.fcfabric");
        assert_eq!(file_name("Poplin (example)"), "Poplin (example).fcfabric");
    }
}
