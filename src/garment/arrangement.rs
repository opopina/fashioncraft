//! Landmark-referenced arrangement anchors (prototype for the EXO-AV-002 scope review).
//!
//! Anchors are authored data, not anatomy inferred from a collider. Each anchor names confirmed
//! landmarks of one `AvatarFrame` (EXO-AV-001) and is refused for any other frame revision.
//! Placement is rigid and flat: it only produces an existing `Placement` (metres, R = Rx·Ry·Rz
//! rotating local `(u, −v, 0)`), so rest geometry, grain and material coordinates are untouched.
//! Curved wrapping, automatic arrangement and persistence inside fitting documents are out of scope.
use crate::avatar_measurements::{AvatarFrame, Landmark};
use crate::garment::template::Placement;
use glam::{DMat3, DQuat, DVec2, DVec3, EulerRot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A direction in the avatar frame, either fixed or derived from named landmarks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Direction {
    Fixed {
        vector: DVec3,
    },
    /// Unit vector from landmark `from` to landmark `to`.
    Along {
        from: String,
        to: String,
    },
    /// Component of `toward` perpendicular to the axis `from → to` (e.g. the front of an upper arm).
    Perpendicular {
        from: String,
        to: String,
        toward: DVec3,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedLandmark {
    pub landmark: String,
    pub weight: f64,
}

/// One authored anchor. `right` and `up` give the in-plane orientation of the panel's material
/// axes (`u` → right, `−v` → up); `outward` is the clearance direction. They are explicit because a
/// back panel may be authored mirrored or unmirrored, and the anchor must not guess which.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrangementAnchor {
    pub id: String,
    pub label: String,
    pub origin: Vec<WeightedLandmark>,
    pub outward: Direction,
    pub right: Direction,
    pub up: Direction,
    pub clearance_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrangementSet {
    pub version: u32,
    pub frame: AvatarFrame,
    pub anchors: Vec<ArrangementAnchor>,
}

/// An evaluated anchor: orthonormal `right`/`up`, unit `outward`, origin in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorFrame {
    pub origin: DVec3,
    pub outward: DVec3,
    pub right: DVec3,
    pub up: DVec3,
}

/// The panel plane must face the clearance direction within this angle (either side).
const MAX_FACING_DEGREES: f64 = 30.0;

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn unit(v: DVec3, what: &str) -> Result<DVec3, String> {
    let l = v.length();
    if !l.is_finite() || l < 1e-9 {
        return Err(format!("{what} has zero or non-finite length"));
    }
    Ok(v / l)
}

impl ArrangementSet {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let set: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        Ok(set)
    }

    /// Validate every anchor against the landmarks of `frame`. Nothing is evaluated partially:
    /// any error rejects the whole set.
    pub fn validate(&self, frame: &AvatarFrame, landmarks: &[Landmark]) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("Unsupported arrangement version {}", self.version));
        }
        if &self.frame != frame {
            return Err(format!(
                "Anchors were authored for {} revision {}, not {} revision {}; reconfirm them",
                self.frame.avatar_id, self.frame.revision, frame.avatar_id, frame.revision
            ));
        }
        let mut seen = BTreeSet::new();
        for a in &self.anchors {
            if !identifier(&a.id) || !seen.insert(a.id.as_str()) {
                return Err(format!("Invalid or duplicate anchor id {:?}", a.id));
            }
            evaluate_anchor(a, &landmark_map(landmarks)?)
                .map_err(|e| format!("Anchor {}: {e}", a.id))?;
        }
        Ok(())
    }

    /// Evaluate one anchor after validating the whole set.
    pub fn evaluate(
        &self,
        frame: &AvatarFrame,
        landmarks: &[Landmark],
        id: &str,
    ) -> Result<AnchorFrame, String> {
        self.validate(frame, landmarks)?;
        let a = self
            .anchors
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| format!("Anchor {id} not found"))?;
        evaluate_anchor(a, &landmark_map(landmarks)?)
    }
}

fn landmark_map(landmarks: &[Landmark]) -> Result<BTreeMap<&str, DVec3>, String> {
    let mut m = BTreeMap::new();
    for l in landmarks {
        if !l.position_m.is_finite() {
            return Err(format!("Landmark {} is not finite", l.id));
        }
        if m.insert(l.id.as_str(), l.position_m).is_some() {
            return Err(format!("Duplicate landmark {}", l.id));
        }
    }
    Ok(m)
}

fn landmark(m: &BTreeMap<&str, DVec3>, id: &str) -> Result<DVec3, String> {
    m.get(id)
        .copied()
        .ok_or_else(|| format!("unknown landmark {id}"))
}

fn direction(d: &Direction, m: &BTreeMap<&str, DVec3>, what: &str) -> Result<DVec3, String> {
    match d {
        Direction::Fixed { vector } => unit(*vector, what),
        Direction::Along { from, to } => unit(landmark(m, to)? - landmark(m, from)?, what),
        Direction::Perpendicular { from, to, toward } => {
            let axis = unit(landmark(m, to)? - landmark(m, from)?, what)?;
            unit(*toward - axis * toward.dot(axis), what)
        }
    }
}

fn evaluate_anchor(
    a: &ArrangementAnchor,
    m: &BTreeMap<&str, DVec3>,
) -> Result<AnchorFrame, String> {
    if a.origin.is_empty() {
        return Err("origin needs at least one landmark".into());
    }
    let total: f64 = a.origin.iter().map(|w| w.weight).sum();
    if a.origin
        .iter()
        .any(|w| !w.weight.is_finite() || w.weight < 0.0)
        || (total - 1.0).abs() > 1e-9
    {
        return Err(format!(
            "origin weights must be non-negative and sum to 1 (sum {total})"
        ));
    }
    let mut origin = DVec3::ZERO;
    for w in &a.origin {
        origin += landmark(m, &w.landmark)? * w.weight;
    }
    if !a.clearance_m.is_finite() || a.clearance_m < 0.0 {
        return Err(format!(
            "clearance must be finite and non-negative, got {}",
            a.clearance_m
        ));
    }
    let outward = direction(&a.outward, m, "outward")?;
    let right = direction(&a.right, m, "right")?;
    // Gram–Schmidt: keep `right`, make `up` orthogonal to it.
    let up_raw = direction(&a.up, m, "up")?;
    let up = unit(
        up_raw - right * up_raw.dot(right),
        "up (after removing its right component)",
    )?;
    let normal = right.cross(up);
    if normal.dot(outward).abs() < MAX_FACING_DEGREES.to_radians().cos() {
        return Err(format!(
            "panel plane (right × up) must face outward within {MAX_FACING_DEGREES}°"
        ));
    }
    Ok(AnchorFrame {
        origin,
        outward,
        right,
        up,
    })
}

/// Rigid placement that puts the centre of the panel's rest bounds at `origin + outward·clearance`,
/// with material `u` along `right` and `−v` along `up`. Rest points are material coordinates (metres).
pub fn placement_for(
    rest_points: &[DVec2],
    anchor: &AnchorFrame,
    clearance_m: f64,
) -> Result<Placement, String> {
    if rest_points.is_empty() || rest_points.iter().any(|p| !p.is_finite()) {
        return Err("panel has no finite rest points".into());
    }
    if !clearance_m.is_finite() || clearance_m < 0.0 {
        return Err(format!(
            "clearance must be finite and non-negative, got {clearance_m}"
        ));
    }
    let (lo, hi) = rest_points
        .iter()
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let c = (lo + hi) / 2.0;
    let local_ref = DVec3::new(c.x, -c.y, 0.0);
    let r = DMat3::from_cols(anchor.right, anchor.up, anchor.right.cross(anchor.up));
    let (x, y, z) = DQuat::from_mat3(&r).normalize().to_euler(EulerRot::XYZ);
    let rotation_degrees = DVec3::new(x, y, z) * (180.0 / std::f64::consts::PI);
    let target = anchor.origin + anchor.outward * clearance_m;
    let mut placement = Placement {
        translation: DVec3::ZERO,
        rotation_degrees,
    };
    placement.translation = target - placement.transform(local_ref);
    Ok(placement)
}
