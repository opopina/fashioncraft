//! CPU f64 Vertex Block Descent with OGC trust regions. Exact local gradients
//! and Hessians, positive local solve + objective-checked steps. No XPBD.
use crate::{
    avatar::SurfaceMesh,
    energy,
    garment::CompiledGarment,
    math::{Jet, Scalar},
    model::State,
    ogc::{ContactSet, OgcSettings, OgcWorld},
};
use glam::{DMat3, DVec3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc, time::Instant};
pub mod audit;
mod coloring;
pub mod friction;
pub mod grab;
mod local_step;
mod prediction;
mod seam_blocks;
pub use seam_blocks::SeamBlockMetrics;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VbdSettings {
    pub dt: f64,
    pub iterations: usize,
    pub damping: f64,
    pub gravity: DVec3,
    pub contact: OgcSettings,
    /// Coulomb friction coefficient μ of every contact (cloth–avatar, cloth–cloth, floor);
    /// zero is frictionless, as before this setting existed.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub friction: f64,
    /// Steady wind; none is still air (no aerodynamic force at all, as before).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind: Option<Wind>,
    /// Height of a physical floor in metres (world y), or none. The floor is a fixed
    /// obstacle like the avatar: cloth rests on it within the contact shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<f64>,
}
/// Stiffness per unit mass (1/s²) drawing a solidified panel to its fitted shape: a 10 Hz
/// spring, firm against the cloth's own sag yet soft enough to stay well conditioned.
pub const SOLIDIFY_STIFFNESS: f64 =
    (2. * std::f64::consts::PI * 10.) * (2. * std::f64::consts::PI * 10.);

/// The rotation part of `a` (Müller et al. 2016, "A robust method to extract the rotational
/// part of deformations"), refined from `q`. Unlike a polar decomposition it is well defined
/// when `a` is singular, as it is for a flat panel.
fn extract_rotation(a: glam::DMat3, mut q: glam::DQuat) -> glam::DQuat {
    for _ in 0..30 {
        let r = glam::DMat3::from_quat(q);
        let omega = r.x_axis.cross(a.x_axis) + r.y_axis.cross(a.y_axis) + r.z_axis.cross(a.z_axis);
        let denominator =
            (r.x_axis.dot(a.x_axis) + r.y_axis.dot(a.y_axis) + r.z_axis.dot(a.z_axis)).abs()
                + 1e-12;
        let w = omega / denominator;
        let angle = w.length();
        if angle < 1e-12 {
            break;
        }
        q = (glam::DQuat::from_axis_angle(w / angle, angle) * q).normalize();
    }
    q
}

/// The factor on a fabric's elastic energy in a local solve: 1 + k_d/dt with its internal
/// damping k_d (see `VbdSolver::prepare_damping`).
pub(crate) fn damped(m: &crate::model::Material, settings: &VbdSettings) -> f64 {
    1. + m.internal_damping / settings.dt
}

/// A steady, uniform wind acting on the cloth as pressure drag: each triangle of area A and
/// unit normal n, moving with velocity v, feels F = ½ ρ C_d A (r·n)|r·n| n with r = wind − v,
/// shared equally by its three corners. A thin sheet edge-on to the flow feels nothing;
/// broadside it feels the full drag. Evaluated at the start of each step (explicit), so it
/// is also the air resistance of falling cloth when the wind speed is zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wind {
    /// Wind velocity, m/s.
    pub velocity: DVec3,
    /// Drag coefficient C_d of a flat sheet broadside to the flow (about 1.2).
    pub drag: f64,
    /// Air density ρ, kg/m³.
    pub density: f64,
}

impl Default for Wind {
    fn default() -> Self {
        Self {
            // A light breeze from +Z.
            velocity: DVec3::new(0., 0., -2.),
            drag: 1.2,
            density: 1.2,
        }
    }
}

impl Wind {
    pub fn validate(&self) -> Result<(), String> {
        if !self.velocity.is_finite()
            || self.velocity.length() > 50.
            || !(0.0..=10.0).contains(&self.drag)
            || !(0.0..=10.0).contains(&self.density)
        {
            return Err("Wind needs a speed up to 50 m/s and a drag and density up to 10".into());
        }
        Ok(())
    }

    /// Acceleration of every vertex from the drag on its triangles, at positions `x` with
    /// velocities `v`. Pinned vertices get none.
    pub fn accelerations(&self, garment: &CompiledGarment, x: &[DVec3], v: &[DVec3]) -> Vec<DVec3> {
        let mut force = vec![DVec3::ZERO; x.len()];
        for t in &garment.mesh.triangles {
            let [a, b, c] = t.vertices;
            let cross = (x[b] - x[a]).cross(x[c] - x[a]);
            let double_area = cross.length();
            if double_area <= 1e-18 {
                continue;
            }
            let n = cross / double_area;
            let relative = self.velocity - (v[a] + v[b] + v[c]) / 3.;
            let normal_speed = relative.dot(n);
            let f = n
                * (0.5
                    * self.density
                    * self.drag
                    * 0.5
                    * double_area
                    * normal_speed
                    * normal_speed.abs());
            for i in t.vertices {
                force[i] += f / 3.;
            }
        }
        force
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let m = garment.mesh.masses[i];
                if garment.mesh.pins[i].is_some() || m <= 0. {
                    DVec3::ZERO
                } else {
                    f / m
                }
            })
            .collect()
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
impl Default for VbdSettings {
    fn default() -> Self {
        Self {
            dt: 1.0 / 240.0,
            iterations: 16,
            damping: 1.5,
            gravity: DVec3::new(0.0, -9.81, 0.0),
            contact: Default::default(),
            friction: 0.0,
            wind: None,
            floor: None,
        }
    }
}
impl VbdSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.contact.validate()?;
        if !self.dt.is_finite()
            || !(1e-6..=0.025).contains(&self.dt)
            || self.iterations == 0
            || self.iterations > 200
            || !self.damping.is_finite()
            || self.damping < 0.0
            || !self.gravity.is_finite()
            || self.floor.is_some_and(|h| !h.is_finite())
            || !self.friction.is_finite()
            || !(0.0..=5.0).contains(&self.friction)
            || self.wind.as_ref().is_some_and(|w| w.validate().is_err())
        {
            return Err("Invalid VBD settings".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Grab {
    pub vertices: [usize; 3],
    pub weights: [f64; 3],
    pub target: DVec3,
    pub stiffness: f64,
    /// Independent surface attachments, sorted by vertex. Empty retains the
    /// historical barycentric point grab exactly.
    pub patch: Vec<grab::PatchAttachment>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct GpuStepProfile {
    pub coloring_ms: f64,
    pub colors: usize,
    pub packing_ms: f64,
    pub upload_ms: f64,
    pub encode_ms: f64,
    /// Submission, fence wait and mapping, including compute (not additional to it).
    pub submit_wait_ms: f64,
    /// Readback processing, precision refinement, state update and diagnostics.
    pub post_ms: f64,
    /// Scope cleanup after the internal step timer, included in full step_ms.
    pub cleanup_ms: f64,
}

/// One-shot, read-only measurement of an accepted VBD step. The sample is
/// produced after the step and never changes the accepted trajectory.
#[derive(Clone, Debug, Serialize)]
pub struct FinalDiagnostics {
    /// Physical time of the accepted state.
    pub state_time_s: f64,
    /// The accepted step duration used by the implicit-Euler residual.
    pub step_dt_s: f64,
    /// Total wall time of the sample, including detection and force audit.
    pub sample_ms: f64,
    /// Final accepted-position OGC detection time inside `sample_ms`.
    pub detection_ms: f64,
    /// Free-degree force-audit time inside `sample_ms`.
    pub residual_ms: f64,
    pub backend: FinalDiagnosticsBackend,
    pub contacts: usize,
    /// Retained nearby interactions, including currently inactive potentials.
    pub contact_terms: usize,
    /// Minimum signed feature separation when at least one retained term
    /// exists; `None` means no contact term, never a fabricated distance.
    pub minimum_distance_m: Option<f64>,
    pub free_vertices: usize,
    /// RMS over free translational degrees of freedom. `None` when every
    /// vertex is pinned; an unavailable value is never reported as zero.
    pub residual_rms_n: Option<f64>,
    pub residual_kind: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalDiagnosticsBackend {
    /// The accepted step and the final-state audit both used the CPU solver.
    Cpu,
    /// The accepted state came from the GPU backend; the audit itself is the
    /// CPU force path, so no GPU timing or GPU residual is claimed.
    CpuAuditOfGpuState,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct VbdMetrics {
    pub seam_blocks: SeamBlockMetrics,
    pub motion: Option<crate::motion::MotionMetrics>,
    pub time_s: f64,
    pub dt_s: f64,
    pub step_ms: f64,
    pub detection_ms: f64,
    pub contact_profile: crate::ogc::DetectionProfile,
    pub solve_ms: f64,
    /// Hardware timestamps; absent for CPU or unsupported timestamp queries.
    pub gpu_compute_ms: Option<f64>,
    pub gpu_profile: Option<GpuStepProfile>,
    /// Native 32-lane subgroup reduction; portable workgroup fallback otherwise.
    pub gpu_subgroups: bool,
    /// Vertices advanced in f64 for roundoff or sparse directional restrictions.
    pub precision_vertices: usize,
    pub precision_ms: f64,
    pub directional_vertices: usize,
    pub iterations: usize,
    pub contacts: usize,
    /// Retained nearby interactions, including currently inactive potentials.
    pub contact_terms: usize,
    pub detection_updates: usize,
    pub broad_phase_rebuilds: usize,
    pub truncated_vertices: usize,
    pub minimum_distance_m: f64,
    pub max_seam_gap_m: f64,
    pub extension_p95_percent: f64,
    pub extension_max_percent: f64,
    pub residual_rms_n: Option<f64>,
}
#[derive(Clone)]
struct Stitch {
    terms: Vec<(usize, f64)>,
    stiffness: f64,
}
#[derive(Clone)]
pub struct VbdSolver {
    /// Optional CPU diagnostic: energy changes at identical geometry when the
    /// contact set refreshes inside the most recent step. Disabled by default.
    pub contact_refresh_audit: Option<Vec<f64>>,
    /// Highest signed refresh delta in the latest CPU step, when audit is on.
    /// Capture only; never read by the numerical algorithm.
    pub contact_refresh_capture: Option<crate::ogc::diagnostics::RefreshEvent>,
    local_probe: local_step::LocalSolveProbe,
    pub(crate) seam_block_sweeps: usize,
    seam_components: Vec<seam_blocks::Block>,
    pub(crate) motion: crate::motion::MotionMonitor,
    pub(crate) prediction: prediction::Prediction,
    pub garment: Arc<CompiledGarment>,
    pub world: OgcWorld,
    triangles: Vec<Vec<usize>>,
    hinges: Vec<Vec<usize>>,
    stitches: Vec<Stitch>,
    vertex_stitches: Vec<Vec<usize>>,
    graph: Vec<Vec<usize>>,
    /// Friction of the current contact set, frozen at its detection; matched to a contact by
    /// index and checked by kind and vertices, so another contact set never picks it up.
    pub(crate) friction: Vec<Option<friction::FrictionTerm>>,
    /// Accelerations besides gravity for the current step (wind), per vertex; empty if none.
    pub(crate) external: Vec<DVec3>,
    /// The constant part of internal damping for the current step, as an acceleration per
    /// vertex (see `prepare_damping`); empty if no fabric is damped.
    damping_shift: Vec<DVec3>,
    /// This step's solidify target of each vertex (none outside solidified panels), and the
    /// last fitted rotation of each solidified panel, to start the next fit from.
    solid_target: Vec<Option<DVec3>>,
    solid_rotation: Vec<glam::DQuat>,
    /// Collision distance of each vertex's fabric, the largest of its triangles'; none where
    /// the scene's applies. Empty when no fabric sets its own.
    collision: Vec<Option<f64>>,
}
impl VbdSolver {
    pub fn new(
        garment: Arc<CompiledGarment>,
        avatar: Option<&SurfaceMesh>,
        state: &State,
    ) -> Result<Self, String> {
        let n = garment.mesh.uv.len();
        if state.x.len() != n || state.v.len() != n {
            return Err("VBD state dimensions do not match garment".into());
        }
        if garment
            .mesh
            .pins
            .iter()
            .enumerate()
            .any(|(i, p)| p.is_some_and(|p| state.x[i].distance(p) > 1e-10))
        {
            return Err(
                "VBD pinned positions must match their constraints before initialization".into(),
            );
        }
        let mut x = state.x.clone();
        let mut faces: Vec<_> = garment.mesh.triangles.iter().map(|t| t.vertices).collect();
        if let Some(a) = avatar {
            a.validate()?;
            x.extend_from_slice(&a.vertices);
            faces.extend(a.triangles.iter().map(|t| t.map(|i| i + n)));
        }
        let mut excluded = BTreeSet::new();
        let mut stitches = Vec::new();
        for seam in &garment.seams {
            for sample in &seam.samples {
                for (a, wa) in &sample.point.terms {
                    for (b, wb) in &sample.point.terms {
                        if wa * wb < 0.0 {
                            excluded.insert([(*a).min(*b), (*a).max(*b)]);
                        }
                    }
                }
                stitches.push(Stitch {
                    terms: sample.point.terms.clone(),
                    stiffness: seam.stiffness * sample.weight_m,
                });
            }
        }
        let mut world = OgcWorld::new(&x, faces, n, excluded)?;
        world.set_inert(garment.inert.clone())?;
        world.validate_start(&x)?;
        world.directional_bounds = true;
        let mut triangles = vec![vec![]; n];
        let mut hinges = vec![vec![]; n];
        let mut vertex_stitches = vec![vec![]; n];
        let mut graph = vec![BTreeSet::new(); n];
        let mut connect = |ids: &[usize]| {
            for a in ids {
                for b in ids {
                    if a != b {
                        graph[*a].insert(*b);
                    }
                }
            }
        };
        for (i, t) in garment.mesh.triangles.iter().enumerate() {
            for v in t.vertices {
                triangles[v].push(i);
            }
            connect(&t.vertices);
        }
        for (i, h) in garment.mesh.hinges.iter().enumerate() {
            for v in h.vertices {
                hinges[v].push(i);
            }
            connect(&h.vertices);
        }
        for (i, s) in stitches.iter().enumerate() {
            let ids: Vec<_> = s.terms.iter().map(|(v, _)| *v).collect();
            for v in &ids {
                vertex_stitches[*v].push(i);
            }
            connect(&ids);
        }
        let mut solver = Self {
            contact_refresh_audit: None,
            contact_refresh_capture: None,
            local_probe: Default::default(),
            seam_block_sweeps: 0,
            seam_components: Vec::new(),
            motion: Default::default(),
            prediction: Default::default(),
            garment,
            world,
            triangles,
            hinges,
            stitches,
            vertex_stitches,
            graph: graph
                .into_iter()
                .map(|neighbors| neighbors.into_iter().collect())
                .collect(),
            friction: vec![],
            external: vec![],
            damping_shift: vec![],
            solid_target: vec![],
            solid_rotation: vec![],
            collision: vec![],
        };
        let mut collision = vec![None; n];
        for t in &solver.garment.mesh.triangles {
            if let Some(d) = solver.garment.materials[t.material].collision_distance {
                for v in t.vertices {
                    collision[v] = Some(collision[v].map_or(d, |c: f64| c.max(d)));
                }
            }
        }
        if collision.iter().any(Option::is_some) {
            solver.collision = collision;
        }
        Ok(solver)
    }
    /// Whether vertex `v` gives no way to contact `c`: it sits on the lower-layer side of a
    /// contact between pieces of different simulation layers. That side keeps no energy of
    /// the contact, so the outer piece alone moves; the conservative bounds still keep both
    /// apart. Contacts with the avatar or the floor, and layer-0 garments, are unaffected.
    pub(crate) fn yields(&self, c: &crate::ogc::potential::Contact, v: usize) -> bool {
        let layers = &self.garment.layers;
        if layers.is_empty() {
            return false;
        }
        use crate::ogc::potential::Kind;
        let first = match c.kind {
            Kind::EdgeEdge | Kind::EdgeEdgeInterior | Kind::SegmentSegment => 2,
            _ => 1,
        };
        let side = |vertices: &[usize]| {
            vertices
                .iter()
                .filter_map(|u| layers.get(*u))
                .max()
                .copied()
        };
        let (a, b) = (&c.vertices[..first], &c.vertices[first..c.count]);
        let (Some(la), Some(lb)) = (side(a), side(b)) else {
            return false;
        };
        let mine = if a.contains(&v) { la } else { lb };
        mine < la.max(lb)
    }
    /// Hand the contact detection each vertex's collision distance for these settings, and
    /// return the largest contact radius.
    pub(crate) fn prepare_collision(&mut self, settings: &VbdSettings) -> Result<f64, String> {
        let r = settings.contact.radius;
        self.world
            .set_cloth_radius(self.collision.iter().map(|d| d.unwrap_or(r)).collect())?;
        Ok(self.world.max_radius(r))
    }
    /// Gravity plus this step's external acceleration of vertex `v`.
    pub(crate) fn acceleration(&self, settings: &VbdSettings, v: usize) -> DVec3 {
        settings.gravity
            + self.external.get(v).copied().unwrap_or(DVec3::ZERO)
            + self.damping_shift.get(v).copied().unwrap_or(DVec3::ZERO)
    }
    /// Set this step's external accelerations (wind) and internal damping from the settings at
    /// the given state.
    pub(crate) fn prepare_external(&mut self, settings: &VbdSettings, state: &State) {
        self.external = match &settings.wind {
            Some(wind) => wind.accelerations(&self.garment, &state.x, &state.v),
            None => vec![],
        };
        self.prepare_damping(settings, state);
        self.prepare_solid(settings, state);
    }
    /// The inertia term of vertex `v` this step, as a target position and a weight (N/m): the
    /// implicit-Euler prediction with mass m·damp/dt², joined by a solidified panel's spring
    /// of stiffness m·ω² to its solidify target. Both are quadratic, so their sum is one.
    pub(crate) fn inertia_target(
        &self,
        settings: &VbdSettings,
        state: &State,
        v: usize,
    ) -> (DVec3, f64) {
        let dt = settings.dt;
        let damp = 1. + settings.damping * dt;
        let target =
            state.x[v] + (state.v[v] * dt + self.acceleration(settings, v) * dt * dt) / damp;
        let inertia = self.garment.mesh.masses[v] * damp / (dt * dt);
        match self.solid_target.get(v).copied().flatten() {
            Some(s) => {
                let k = self.garment.mesh.masses[v] * SOLIDIFY_STIFFNESS;
                ((target * inertia + s * k) / (inertia + k), inertia + k)
            }
            None => (target, inertia),
        }
    }
    /// Fit each solidified panel's captured shape, rotated and moved, to where its vertices
    /// are heading this step (their inertia targets), and draw them toward the fit. A rigid
    /// motion of the panel is followed exactly; only changes of shape are resisted.
    fn prepare_solid(&mut self, settings: &VbdSettings, state: &State) {
        self.solid_target.clear();
        if self.garment.solid.is_empty() {
            return;
        }
        self.solid_rotation
            .resize(self.garment.solid.len(), glam::DQuat::IDENTITY);
        let mut targets = vec![None; state.x.len()];
        let dt = settings.dt;
        let damp = 1. + settings.damping * dt;
        for (k, group) in self.garment.solid.iter().enumerate() {
            let heading: Vec<DVec3> = group
                .vertices
                .iter()
                .map(|&v| {
                    state.x[v] + (state.v[v] * dt + self.acceleration(settings, v) * dt * dt) / damp
                })
                .collect();
            let count = heading.len().max(1) as f64;
            let centre = heading.iter().sum::<DVec3>() / count;
            let rest = group.reference.iter().sum::<DVec3>() / count;
            let mut a = glam::DMat3::ZERO;
            for (p, r) in heading.iter().zip(&group.reference) {
                let (p, r) = (*p - centre, *r - rest);
                a += glam::DMat3::from_cols(p * r.x, p * r.y, p * r.z);
            }
            let q = extract_rotation(a, self.solid_rotation[k]);
            self.solid_rotation[k] = q;
            for (&v, r) in group.vertices.iter().zip(&group.reference) {
                if self.garment.mesh.pins[v].is_none() {
                    targets[v] = Some(centre + q * (*r - rest));
                }
            }
        }
        self.solid_target = targets;
    }
    /// Internal damping adds the force −(k_d/dt)(∇E(x) − ∇E(xᵗ)) on each vertex, with E a
    /// damped fabric's membrane and bending energy and xᵗ the start of the step: it resists
    /// stretching and bending motion, and none for a rigid motion, which leaves ∇E unchanged.
    /// In the local solves that is the elastic energy scaled by 1 + k_d/dt (see `damped`)
    /// plus the constant (k_d/dt)∇E(xᵗ)·x per vertex, which this folds into the inertia
    /// target as an acceleration ∇E(xᵗ)·k_d/dt / m.
    fn prepare_damping(&mut self, settings: &VbdSettings, state: &State) {
        self.damping_shift.clear();
        if self
            .garment
            .materials
            .iter()
            .all(|m| m.internal_damping == 0.)
        {
            return;
        }
        let shift = (0..state.x.len())
            .map(|v| {
                if self.garment.mesh.pins[v].is_some() {
                    return DVec3::ZERO;
                }
                let get = |i: usize| {
                    if i == v {
                        std::array::from_fn(|k| Jet::<3>::variable(state.x[v][k], k))
                    } else {
                        state.x[i].to_array().map(Jet::c)
                    }
                };
                let mut gradient = Jet::<3>::c(0.);
                for i in &self.triangles[v] {
                    let t = &self.garment.mesh.triangles[*i];
                    let m = &self.garment.materials[t.material];
                    if m.internal_damping > 0. {
                        gradient = gradient
                            + energy::membrane(t.vertices.map(get), t, m)
                                * Jet::c(m.internal_damping / settings.dt);
                    }
                }
                for i in &self.hinges[v] {
                    let h = &self.garment.mesh.hinges[*i];
                    let m = &self.garment.materials[h.material];
                    if m.internal_damping > 0. {
                        gradient = gradient
                            + energy::bending(h.vertices.map(get), h, m)
                                * Jet::c(m.internal_damping / settings.dt);
                    }
                }
                DVec3::from_array(gradient.g) / self.garment.mesh.masses[v]
            })
            .collect();
        self.damping_shift = shift;
    }
    pub(crate) fn colors(&self, contact: &ContactSet, grab: Option<&Grab>) -> Vec<Vec<usize>> {
        coloring::colors(&self.graph, contact, grab)
    }
    pub(crate) fn local<S: Scalar>(
        &self,
        v: usize,
        p: [S; 3],
        x: &[DVec3],
        contact: &ContactSet,
        settings: &VbdSettings,
        grab: Option<&Grab>,
    ) -> S {
        let get = |i: usize| if i == v { p } else { x[i].to_array().map(S::c) };
        let mut value = S::c(0.0);
        for i in &self.triangles[v] {
            let t = &self.garment.mesh.triangles[*i];
            let m = &self.garment.materials[t.material];
            value = value + energy::membrane(t.vertices.map(get), t, m) * S::c(damped(m, settings));
        }
        for i in &self.hinges[v] {
            let h = &self.garment.mesh.hinges[*i];
            let m = &self.garment.materials[h.material];
            value = value + energy::bending(h.vertices.map(get), h, m) * S::c(damped(m, settings));
        }
        for i in &self.vertex_stitches[v] {
            let s = &self.stitches[*i];
            let mut d = [S::c(0.0); 3];
            for (j, w) in &s.terms {
                let q = get(*j);
                for k in 0..3 {
                    d[k] = d[k] + S::c(*w) * q[k];
                }
            }
            value = value + S::c(0.5 * s.stiffness) * crate::math::dot(d, d);
        }
        for i in &contact.incident[v] {
            let c = &contact.contacts[*i];
            if self.yields(c, v) {
                continue;
            }
            value = value
                + c.energy(
                    c.vertices.map(get),
                    contact.radius(*i, settings.contact.radius),
                    settings.contact.stiffness,
                );
            if let Some(f) = self
                .friction
                .get(*i)
                .and_then(Option::as_ref)
                .filter(|f| f.kind == c.kind && f.vertices == c.vertices)
            {
                value = value + f.energy(get);
            }
        }
        if let Some(g) = grab.filter(|g| g.vertices.contains(&v)) {
            let mut d = g.target.to_array().map(|a| S::c(-a));
            for (j, w) in g.vertices.iter().zip(g.weights) {
                let q = get(*j);
                for k in 0..3 {
                    d[k] = d[k] + S::c(w) * q[k];
                }
            }
            value = value + S::c(0.5 * g.point_stiffness()) * crate::math::dot(d, d);
        }
        if let Some(g) = grab
            && let Some(a) = g.attachment(v)
        {
            let target = g.target + a.offset;
            let d = crate::math::sub(get(v), target.to_array().map(S::c));
            value = value + S::c(0.5 * g.patch_stiffness() * a.weight) * crate::math::dot(d, d);
        }
        value
    }
    /// State is committed only after a finite step. Fixed geometry never moves.
    pub fn step(
        &mut self,
        state: &mut State,
        settings: &VbdSettings,
        grab: Option<&Grab>,
    ) -> Result<VbdMetrics, String> {
        settings.validate()?;
        let n = self.garment.mesh.uv.len();
        if state.x.len() != n
            || state.v.len() != n
            || !state.time.is_finite()
            || state.x.iter().chain(&state.v).any(|v| !v.is_finite())
        {
            return Err("Invalid VBD state".into());
        }
        if let Some(g) = grab
            && (g.vertices.iter().any(|i| *i >= n)
                || !g.target.is_finite()
                || !g.stiffness.is_finite()
                || g.stiffness <= 0.0
                || g.weights.iter().any(|w| !w.is_finite() || *w < 0.0)
                || (g.weights.iter().sum::<f64>() - 1.0).abs() > 1e-8
                || !g.valid_patch(n))
        {
            return Err("Invalid mouse attachment".into());
        }
        let start = Instant::now();
        let mut metrics = VbdMetrics {
            dt_s: settings.dt,
            iterations: settings.iterations,
            ..Default::default()
        };
        let dt = settings.dt;
        let old_velocity = state.v.clone();
        self.prepare_external(settings, state);
        let guess = self.prediction.guess(state, settings, &self.external);
        let (target, inertia): (Vec<_>, Vec<_>) = (0..n)
            .map(|i| self.inertia_target(settings, state, i))
            .unzip();
        let predicted_motion = target
            .iter()
            .zip(&state.x)
            .map(|(a, b)| a.distance(*b))
            .fold(0.0, f64::max);
        let radius = self.prepare_collision(settings)?;
        let rq = (radius + predicted_motion).clamp(2.0 * radius, 4.0 * radius);
        let mut x = self.world.positions(&state.x);
        let start_positions = x.clone();
        self.contact_refresh_capture = None;
        if let Some(audit) = &mut self.contact_refresh_audit {
            audit.clear();
        }
        let detect = Instant::now();
        let mut contacts = self.world.detect(&x, rq, &settings.contact)?;
        if contacts.minimum_distance <= 1e-12 {
            return Err("OGC zero separation; step rejected, reset the arrangement".into());
        }
        self.friction =
            friction::friction_terms(&contacts, &start_positions, &start_positions, settings);
        metrics.detection_ms += detect.elapsed().as_secs_f64() * 1000.0;
        metrics.detection_updates += 1;
        metrics.broad_phase_rebuilds += usize::from(contacts.broad_phase_rebuilt);
        metrics.contact_profile += contacts.profile;
        let mut colors = self.colors(&contacts, grab);
        // Conservative warm start; no unbounded inertial prediction crosses a face.
        let mut clipped = 0;
        for i in 0..n {
            if self.garment.mesh.pins[i].is_none() {
                let (p, b) = contacts.truncate(i, guess[i]);
                x[i] = p;
                clipped += usize::from(b);
            }
        }
        for sweep in 0..settings.iterations {
            if clipped > 0 && clipped as f64 >= settings.contact.refresh_fraction * n as f64 {
                let detect = Instant::now();
                let previous_energy = self
                    .contact_refresh_audit
                    .as_ref()
                    .map(|_| contacts.value(&x, &settings.contact));
                let refreshed = self.world.detect(&x, rq, &settings.contact)?;
                if let (Some(audit), Some(previous)) =
                    (&mut self.contact_refresh_audit, previous_energy)
                {
                    let delta = refreshed.value(&x, &settings.contact) - previous;
                    audit.push(delta);
                    if self
                        .contact_refresh_capture
                        .as_ref()
                        .is_none_or(|e| delta > e.delta_j)
                    {
                        self.contact_refresh_capture =
                            Some(crate::ogc::diagnostics::refresh_event(
                                &contacts,
                                &refreshed,
                                &x,
                                n,
                                &settings.contact,
                                sweep + 1,
                                delta,
                            ));
                    }
                }
                contacts = refreshed;
                // Normal forces stay lagged to the start of the step: an intermediate iterate
                // pressed into the barrier would overstate them.
                self.friction = friction::friction_terms(
                    &contacts,
                    &start_positions,
                    &start_positions,
                    settings,
                );
                colors = self.colors(&contacts, grab);
                metrics.detection_ms += detect.elapsed().as_secs_f64() * 1000.0;
                metrics.detection_updates += 1;
                metrics.broad_phase_rebuilds += usize::from(contacts.broad_phase_rebuilt);
                metrics.contact_profile += contacts.profile;
            }
            clipped = 0;
            let mut residual = 0.0;
            for color in &colors {
                let updates: Vec<_> = color
                    .par_iter()
                    .map(|v| {
                        let v = *v;
                        if self.garment.mesh.pins[v].is_some() {
                            return (v, x[v], false, 0.0);
                        }
                        let (accepted, truncated, residual) = self.local_step(
                            v,
                            &x,
                            &contacts,
                            settings,
                            grab,
                            (target[v], inertia[v]),
                        );
                        (v, accepted, truncated, residual)
                    })
                    .collect();
                for (v, p, c, r) in updates {
                    x[v] = p;
                    clipped += usize::from(c);
                    residual += r;
                }
            }
            metrics.truncated_vertices += clipped;
            metrics.residual_rms_n = Some((residual / (3 * n) as f64).sqrt());
        }
        metrics.seam_blocks = self.refine_seams(&mut x, state, &contacts, settings, grab);
        if x.iter().any(|v| !v.is_finite()) {
            return Err("Non-finite VBD result; state unchanged".into());
        }
        // Final bound feasibility is an invariant, not a post-Newton correction.
        if (0..n).any(|i| !contacts.contains(i, x[i], 1e-10)) {
            return Err("OGC trust-region invariant failed".into());
        }
        for (i, position) in x.iter().take(n).enumerate() {
            state.v[i] = (*position - state.x[i]) / dt;
            state.x[i] = *position;
        }
        state.time += dt;
        self.prediction.record(&old_velocity, state, settings);
        metrics.motion = Some(self.motion.record(&old_velocity, state, dt, &self.garment));
        metrics.time_s = state.time;
        metrics.contacts = contacts.active_contacts;
        metrics.contact_terms = contacts.contacts.len();
        metrics.minimum_distance_m = contacts.minimum_distance;
        metrics.directional_vertices = contacts.directional.vertex_count();
        metrics.max_seam_gap_m = self.garment.max_seam_gap(&state.x);
        [metrics.extension_p95_percent, metrics.extension_max_percent] =
            crate::garment::diagnostics::extension_percent(&self.garment, state);
        metrics.step_ms = start.elapsed().as_secs_f64() * 1000.0;
        metrics.solve_ms = metrics.step_ms - metrics.detection_ms;
        Ok(metrics)
    }
}
