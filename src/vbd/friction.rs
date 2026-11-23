//! Lagged, smoothed Coulomb friction for the VBD step (the IPC friction potential used by
//! Vertex Block Descent). Every contact that presses at the start of the step gives a normal
//! force magnitude λ, a contact normal n and barycentric weights w (positive on one side,
//! negative on the other), all frozen for the rest of the step; contacts that only appear
//! during the step get friction from the next one. The friction potential of a contact is
//!
//!   D(x) = μ λ f0(|P u|),  u = Σ w_i (x_i − x_i⁰),  P = I − n nᵀ,
//!
//! where x⁰ are the positions at the start of the step, so P u is the tangential slip of the
//! two sides over the step. f0(y) = y²/ε − y³/(3ε²) + ε/3 for y < ε and y beyond, with
//! ε = ε_v·dt: a slip slower than ε_v counts as sticking. Its gradient is the Coulomb force
//! μ λ (opposing the slip) once sliding, and its Hessian is positive semidefinite. Units: m,
//! N, J. μ is dimensionless: the scene's coefficient, or a fabric's own where it sets one
//! (see [`contact_friction`]).
use super::*;
use crate::ogc::potential::{Contact, Kind};

/// Slip speed (m/s) below which contacts stick; IPC's usual value.
pub const STICK_SPEED_M_S: f64 = 1e-3;

/// The frozen friction data of one contact.
#[derive(Clone, Debug)]
pub struct FrictionTerm {
    pub kind: Kind,
    pub vertices: [usize; 4],
    pub count: usize,
    pub weights: [f64; 4],
    pub normal: DVec3,
    /// μ λ, the sliding friction force (N).
    pub mu_lambda: f64,
    /// Σ w_i x_i⁰, so that u = Σ w_i x_i − anchor.
    pub anchor: DVec3,
    /// ε = ε_v·dt (m).
    pub eps: f64,
}

impl FrictionTerm {
    /// μ λ f0(|P u|) at positions `get(i)`.
    pub fn energy<S: Scalar>(&self, get: impl Fn(usize) -> [S; 3]) -> S {
        let mut u = self.anchor.to_array().map(|a| S::c(-a));
        for k in 0..self.count {
            let q = get(self.vertices[k]);
            for a in 0..3 {
                u[a] = u[a] + S::c(self.weights[k]) * q[a];
            }
        }
        let n = self.normal.to_array().map(S::c);
        let along = crate::math::dot(u, n);
        let t = std::array::from_fn::<S, 3, _>(|a| u[a] - n[a] * along);
        let s = crate::math::dot(t, t);
        let eps = self.eps;
        let f0 = if s.value() >= eps * eps {
            s.sqrt()
        } else if s.value() < 1e-30 {
            // The cubic term is negligible and its square root has no derivative at zero.
            s / S::c(eps) + S::c(eps / 3.)
        } else {
            s / S::c(eps) - s * s.sqrt() / S::c(3. * eps * eps) + S::c(eps / 3.)
        };
        S::c(self.mu_lambda) * f0
    }
}

/// Friction data for every contact of `contacts`, from the positions `x` at detection and the
/// positions `start` at the start of the step (fixed obstacles included). `None` for contacts
/// that push with no force, or when μ is zero. Contacts are independent, so they are computed
/// in parallel, in their order. Every contact takes the scene's μ.
pub fn friction_terms(
    contacts: &ContactSet,
    x: &[DVec3],
    start: &[DVec3],
    settings: &VbdSettings,
) -> Vec<Option<FrictionTerm>> {
    fabric_friction_terms(contacts, x, start, settings, &[])
}

/// The friction coefficient of contact `c`. `fabric` holds each cloth vertex's own
/// coefficient, none where its fabric takes the scene's (`scene`); vertices beyond it are
/// the avatar and the floor. A side of the contact takes the largest coefficient of its cloth
/// vertices: two pieces of cloth rub with the mean of their two sides, and cloth on the
/// avatar or the floor with its own.
pub fn contact_friction(c: &Contact, fabric: &[Option<f64>], scene: f64, moving: usize) -> f64 {
    let first = match c.kind {
        Kind::EdgeEdge | Kind::EdgeEdgeInterior | Kind::SegmentSegment => 2,
        _ => 1,
    };
    let side = |vertices: &[usize]| {
        vertices
            .iter()
            .filter(|v| **v < moving)
            .map(|v| fabric.get(*v).copied().flatten().unwrap_or(scene))
            .reduce(f64::max)
    };
    match (
        side(&c.vertices[..first]),
        side(&c.vertices[first..c.count]),
    ) {
        (Some(a), Some(b)) => 0.5 * (a + b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => scene,
    }
}

/// [`friction_terms`] with each fabric's own coefficient (see [`contact_friction`]); an empty
/// `fabric` gives every contact the scene's.
pub fn fabric_friction_terms(
    contacts: &ContactSet,
    x: &[DVec3],
    start: &[DVec3],
    settings: &VbdSettings,
    fabric: &[Option<f64>],
) -> Vec<Option<FrictionTerm>> {
    if settings.friction <= 0. && !fabric.iter().flatten().any(|mu| *mu > 0.) {
        return vec![];
    }
    let k = settings.contact.stiffness;
    let eps = STICK_SPEED_M_S * settings.dt;
    let moving = contacts.radii.len();
    // Most found pairs lie beyond the contact radius, where the energy and all its derivatives
    // are exactly zero: no push, no friction. One plain evaluation each tells, in one pass on
    // this thread; only the few that press (a few percent) are differentiated, in parallel.
    let pressing: Vec<(usize, f64)> = contacts
        .contacts
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let mu = if fabric.is_empty() {
                settings.friction
            } else {
                contact_friction(c, fabric, settings.friction, moving)
            };
            (mu > 0. && c.value(x, contacts.radius(i, settings.contact.radius), k) > 0.)
                .then_some((i, mu))
        })
        .collect();
    let mut terms = vec![None; contacts.contacts.len()];
    let found: Vec<_> = pressing
        .par_iter()
        .map(|&(i, mu)| {
            let c = &contacts.contacts[i];
            let r = contacts.radius(i, settings.contact.radius);
            let mut gradients = [DVec3::ZERO; 4];
            for (g, &v) in gradients.iter_mut().zip(&c.vertices[..c.count]) {
                *g = c.block(x, v, r, k).1;
            }
            let gradients = &gradients[..c.count];
            if gradients.iter().any(|g| !g.is_finite()) {
                return None;
            }
            // The first side: one point, or the first edge's two ends.
            let first = match c.kind {
                Kind::EdgeEdge | Kind::EdgeEdgeInterior | Kind::SegmentSegment => 2,
                _ => 1,
            };
            let push: DVec3 = -gradients[..first].iter().copied().sum::<DVec3>();
            let lambda = push.length();
            // Also rejects NaN: only a real push carries friction.
            if lambda.is_nan() || lambda <= 1e-12 {
                return None;
            }
            let normal = push / lambda;
            let mut weights = [0.; 4];
            let mut anchor = DVec3::ZERO;
            for j in 0..c.count {
                weights[j] = -gradients[j].dot(normal) / lambda;
                anchor += start[c.vertices[j]] * weights[j];
            }
            Some((
                i,
                FrictionTerm {
                    kind: c.kind,
                    vertices: c.vertices,
                    count: c.count,
                    weights,
                    normal,
                    mu_lambda: mu * lambda,
                    anchor,
                    eps,
                },
            ))
        })
        .collect();
    for (i, term) in found.into_iter().flatten() {
        terms[i] = Some(term);
    }
    terms
}
