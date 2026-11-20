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
    /// Distance from the skin along `outward` (from the origin when no avatar surface is given).
    pub clearance_m: f64,
    /// Wrap the panel around the anchor axis (`up` through the origin), like a curved arrangement.
    #[serde(default = "default_wrap")]
    pub wrap: bool,
    /// Largest wrap angle either side of the panel centre; beyond it the panel continues flat.
    #[serde(default = "default_max_wrap")]
    pub max_wrap_degrees: f64,
}

fn default_wrap() -> bool {
    true
}

fn default_max_wrap() -> f64 {
    80.0
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
    // `up` (e.g. a bone) stays exact; `outward` is made perpendicular to it; `right` becomes the exact
    // perpendicular to both, keeping the side the author chose (mirrored or unmirrored panels).
    let up = direction(&a.up, m, "up")?;
    let outward_raw = direction(&a.outward, m, "outward")?;
    let outward = unit(
        outward_raw - up * outward_raw.dot(up),
        "outward (after removing its up component)",
    )?;
    let right_raw = direction(&a.right, m, "right")?;
    let side = up.cross(outward);
    let along = right_raw.dot(side);
    if along.abs() < MAX_FACING_DEGREES.to_radians().sin() {
        return Err(format!(
            "right must point across the panel (within {:.0}° of up × outward), so the panel plane faces outward",
            90.0 - MAX_FACING_DEGREES
        ));
    }
    let right = side * along.signum();
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

/// Anchors and landmarks shipped next to an avatar OBJ (`arrangement.json` + `measurements.json`).
/// The landmark file may be the prepared MakeHuman map (`{"landmarks": {id: [x, y, z]}}`).
#[derive(Clone, Debug)]
pub struct Sidecar {
    pub set: ArrangementSet,
    pub landmarks: Vec<Landmark>,
    /// The landmarks were estimated from the avatar's shape, not shipped with it.
    pub estimated: bool,
}

impl Sidecar {
    /// Evaluate one anchor in the sidecar's own frame.
    pub fn frame(&self, id: &str) -> Result<AnchorFrame, String> {
        self.set.evaluate(&self.set.frame, &self.landmarks, id)
    }

    /// The sidecar of the avatar in `pose`: each landmark goes with the bone it lies on (the
    /// nearest bone), and each anchor's fixed directions turn with the bone nearest its origin,
    /// so the anchors follow the pose.
    pub fn posed(&self, rig: &crate::avatar_rig::Rig, pose: &crate::avatar_rig::Pose) -> Sidecar {
        let t = rig.transforms(|b| pose.rotation(b));
        let nearest = |p: DVec3| {
            rig.bones
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    crate::avatar_rig::segment_distance(p, a.head, a.tail)
                        .total_cmp(&crate::avatar_rig::segment_distance(p, b.head, b.tail))
                })
                .map(|(k, _)| k)
        };
        let rest: BTreeMap<&str, DVec3> = self
            .landmarks
            .iter()
            .map(|l| (l.id.as_str(), l.position_m))
            .collect();
        let landmarks = self
            .landmarks
            .iter()
            .map(|l| Landmark {
                id: l.id.clone(),
                position_m: nearest(l.position_m)
                    .map_or(l.position_m, |k| t[k].transform_point3(l.position_m)),
            })
            .collect();
        let mut set = self.set.clone();
        for anchor in &mut set.anchors {
            let origin: DVec3 = anchor
                .origin
                .iter()
                .filter_map(|w| rest.get(w.landmark.as_str()).map(|p| *p * w.weight))
                .sum();
            let Some(k) = nearest(origin) else {
                continue;
            };
            let turn = |v: DVec3| t[k].transform_vector3(v);
            for d in [&mut anchor.outward, &mut anchor.right, &mut anchor.up] {
                match d {
                    Direction::Fixed { vector } => *vector = turn(*vector),
                    Direction::Perpendicular { toward, .. } => *toward = turn(*toward),
                    Direction::Along { .. } => {}
                }
            }
        }
        Sidecar {
            set,
            landmarks,
            estimated: self.estimated,
        }
    }
}

/// The bundled MakeHuman anchors, which name only joint landmarks.
const TEMPLATE_ANCHORS: &str = include_str!("../../assets/makehuman/arrangement.json");

/// Anchors for an avatar that ships none: the bundled anchors on joints estimated from its
/// shape. `None` when the avatar does not look like a standing body.
pub fn estimated_sidecar(surface: &crate::avatar::SurfaceMesh) -> Option<Sidecar> {
    let landmarks: Vec<Landmark> = crate::avatar_landmarks::estimate(surface)
        .ok()?
        .into_iter()
        .map(|(id, position_m)| Landmark { id, position_m })
        .collect();
    let set = ArrangementSet::from_json(TEMPLATE_ANCHORS).ok()?;
    set.validate(&set.frame.clone(), &landmarks).ok()?;
    Some(Sidecar {
        set,
        landmarks,
        estimated: true,
    })
}

/// Load the sidecar for an avatar: the anchors and landmarks shipped next to it, or else
/// anchors on joints estimated from its shape. `Ok(None)` when it ships none and its shape
/// does not allow an estimate.
/// The anchors and joints shipped with an avatar: `<stem>.arrangement.json` and
/// `<stem>.measurements.json` next to it, else `arrangement.json` and `measurements.json` in
/// its folder. `None` when there are none.
pub fn sidecar_files(
    avatar_path: &std::path::Path,
) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = avatar_path.parent().unwrap_or(std::path::Path::new("."));
    if let Some(stem) = avatar_path.file_stem().map(|s| s.to_string_lossy()) {
        let own = (
            dir.join(format!("{stem}.arrangement.json")),
            dir.join(format!("{stem}.measurements.json")),
        );
        if own.0.exists() {
            return Some(own);
        }
    }
    let shared = (dir.join("arrangement.json"), dir.join("measurements.json"));
    shared.0.exists().then_some(shared)
}

pub fn load_sidecar(avatar_path: &std::path::Path) -> Result<Option<Sidecar>, String> {
    let Some((anchors, measurements)) = sidecar_files(avatar_path) else {
        return Ok(crate::avatar::SurfaceMesh::load_obj(avatar_path)
            .ok()
            .and_then(|surface| estimated_sidecar(&surface)));
    };
    let set = ArrangementSet::from_json(
        &std::fs::read_to_string(&anchors).map_err(|e| format!("{}: {e}", anchors.display()))?,
    )?;
    let text = std::fs::read_to_string(&measurements)
        .map_err(|e| format!("{}: {e}", measurements.display()))?;
    #[derive(Deserialize)]
    struct Prepared {
        landmarks: BTreeMap<String, [f64; 3]>,
    }
    let prepared: Prepared =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", measurements.display()))?;
    let landmarks: Vec<Landmark> = prepared
        .landmarks
        .into_iter()
        .map(|(id, v)| Landmark {
            id,
            position_m: DVec3::from_array(v),
        })
        .collect();
    set.validate(&set.frame.clone(), &landmarks)?;
    Ok(Some(Sidecar {
        set,
        landmarks,
        estimated: false,
    }))
}

/// Rest outline of a panel in material coordinates: its polygon, or its rectangle.
pub fn rest_outline(panel: &crate::garment::template::Panel) -> Vec<DVec2> {
    match &panel.polygon {
        Some(p) => p.points.clone(),
        None => {
            let (w, h) = (panel.pattern.width, panel.pattern.height);
            vec![
                DVec2::ZERO,
                DVec2::new(w, 0.0),
                DVec2::new(w, h),
                DVec2::new(0.0, h),
            ]
        }
    }
}

/// First hit of the ray `origin + dir·t` (t > 0) with the avatar surface (Möller–Trumbore), in metres.
pub fn surface_distance(
    surface: &crate::avatar::SurfaceMesh,
    origin: DVec3,
    dir: DVec3,
) -> Option<f64> {
    let mut best = f64::INFINITY;
    for t in &surface.triangles {
        let (a, b, c) = (
            surface.vertices[t[0]],
            surface.vertices[t[1]],
            surface.vertices[t[2]],
        );
        let (e1, e2) = (b - a, c - a);
        let h = dir.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-14 {
            continue;
        }
        let f = 1.0 / det;
        let s = origin - a;
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = s.cross(e1);
        let v = f * dir.dot(q);
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let d = f * e2.dot(q);
        if d > 1e-6 && d < best {
            best = d;
        }
    }
    best.is_finite().then_some(best)
}

/// Result of arranging one panel on an anchor.
#[derive(Clone, Debug)]
pub struct Arrangement {
    pub placement: Placement,
    pub wrap: Option<crate::garment::template::PanelWrap>,
    /// Distance from the anchor axis to the panel centre (wrap radius when wrapped), metres.
    pub radius_m: f64,
    /// Skin distance straight ahead of the anchor, when an avatar surface was available.
    pub skin_m: Option<f64>,
}

/// Arrange a panel on an anchor: the panel centre sits `clearance_m` outside the skin (measured by
/// casting rays from the anchor axis), and with `max_wrap_degrees` the panel wraps around the axis on a
/// cylinder whose radius clears the body across the panel's width and height.
pub fn arrange(
    rest_points: &[DVec2],
    anchor: &AnchorFrame,
    clearance_m: f64,
    max_wrap_degrees: Option<f64>,
    surface: Option<&crate::avatar::SurfaceMesh>,
) -> Result<Arrangement, String> {
    arrange_with(
        rest_points,
        anchor,
        clearance_m,
        max_wrap_degrees,
        surface,
        false,
    )
}

/// Like [`arrange`], but the wrap's section is the smallest ellipse around the anchor axis
/// that clears, by `clearance_m`, the body under the panel along every sampled ray (the last
/// place each ray leaves the body, so an arm beside the torso counts): closer to a body wider
/// than it is deep. A pair of tightly wrapped pieces can meet at the sides, so callers check
/// the start and fall back to [`arrange`].
pub fn arrange_elliptical(
    rest_points: &[DVec2],
    anchor: &AnchorFrame,
    clearance_m: f64,
    max_wrap_degrees: Option<f64>,
    surface: Option<&crate::avatar::SurfaceMesh>,
) -> Result<Arrangement, String> {
    arrange_with(
        rest_points,
        anchor,
        clearance_m,
        max_wrap_degrees,
        surface,
        true,
    )
}

/// How far along its anchor's axis a wrapped panel that hits the body is moved, nearest first
/// and downward first (a long piece centred on a mid-torso anchor reaches the neck; a sleeve
/// centred on the upper arm reaches past the shoulder), metres.
const AXIS_SHIFTS_M: [f64; 8] = [-0.05, 0.05, -0.1, 0.1, -0.18, 0.18, -0.28, 0.28];
/// How much a wrapped panel's section is enlarged to clear the body before it is unwrapped.
const SECTION_GROWTH: [f64; 3] = [1.0, 1.15, 1.35];
/// The share of the clearance every point of an arranged panel keeps from the skin.
const CLEAR_SHARE: f64 = 0.4;

fn arrange_with(
    rest_points: &[DVec2],
    anchor: &AnchorFrame,
    clearance_m: f64,
    max_wrap_degrees: Option<f64>,
    surface: Option<&crate::avatar::SurfaceMesh>,
    elliptical: bool,
) -> Result<Arrangement, String> {
    if rest_points.is_empty() || rest_points.iter().any(|p| !p.is_finite()) {
        return Err("panel has no finite rest points".into());
    }
    if !clearance_m.is_finite() || clearance_m < 0.0 {
        return Err(format!(
            "clearance must be finite and non-negative, got {clearance_m}"
        ));
    }
    let first = fit(
        rest_points,
        anchor,
        clearance_m,
        max_wrap_degrees,
        surface,
        elliptical,
    )?;
    let Some(surface) = surface else {
        return Ok(first);
    };
    // Check samples across the whole panel against the body.
    let samples = panel_samples(rest_points, 9);
    let clear = |placement: &Placement, wrap: Option<crate::garment::template::PanelWrap>| {
        samples.iter().all(|uv| {
            let local = DVec3::new(uv.x, -uv.y, 0.0);
            let local = wrap.map_or(local, |w| w.bend(local));
            let p = placement.transform(local);
            !inside(surface, p) && distance_to_surface(surface, p) >= CLEAR_SHARE * clearance_m
        })
    };
    if clear(&first.placement, first.wrap) {
        return Ok(first);
    }
    // A wrapped panel that hits the body keeps its wrap if it can: moved along the anchor's
    // axis (as far as half its height), with a slightly larger section if need be; of the
    // places that clear, the one snuggest to the body and nearest the anchor.
    if first.wrap.is_some() {
        let mut best: Option<(f64, Arrangement)> = None;
        let height = rest_points
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.y), hi.max(p.y))
            });
        let reach = (0.75 * (height.1 - height.0)).max(0.05);
        let shifts = std::iter::once(0.0).chain(
            AXIS_SHIFTS_M
                .iter()
                .copied()
                .filter(|s| s.abs() <= reach + 1e-9),
        );
        for shift in shifts {
            // Farther along the axis cannot beat what is found already.
            if best
                .as_ref()
                .is_some_and(|(score, _)| shift.abs() + 2.0 * clearance_m >= *score)
            {
                break;
            }
            let moved = AnchorFrame {
                origin: anchor.origin + anchor.up * shift,
                ..*anchor
            };
            let a = if shift == 0.0 {
                first.clone()
            } else {
                fit(
                    rest_points,
                    &moved,
                    clearance_m,
                    max_wrap_degrees,
                    Some(surface),
                    elliptical,
                )?
            };
            let Some(w) = a.wrap else { continue };
            for grow in SECTION_GROWTH {
                let wrap = crate::garment::template::PanelWrap {
                    radius_m: w.radius_m * grow,
                    half_width_m: w.half_width_m.map(|h| h * grow),
                    ..w
                };
                let placement = placement_for(rest_points, &moved, a.radius_m * grow)?;
                if clear(&placement, Some(wrap)) {
                    // Snug to the body counts twice what nearness to the anchor does.
                    let score = 2.0 * a.radius_m * grow + shift.abs();
                    if best.as_ref().is_none_or(|(b, _)| score < *b) {
                        best = Some((
                            score,
                            Arrangement {
                                placement,
                                wrap: Some(wrap),
                                radius_m: a.radius_m * grow,
                                skin_m: a.skin_m,
                            },
                        ));
                    }
                    break;
                }
            }
        }
        if let Some((_, a)) = best {
            return Ok(a);
        }
    }
    // Otherwise wrap less, then move out, until clear.
    let (mut placement, mut wrap, mut radius) = (first.placement, first.wrap, first.radius_m);
    let mut tries = 0;
    while !clear(&placement, wrap) && tries < 12 {
        tries += 1;
        match &mut wrap {
            Some(w) if w.max_angle_degrees > 20.0 + 1e-9 => w.max_angle_degrees -= 20.0,
            Some(_) => wrap = None,
            None => {
                radius += 0.02;
                placement = placement_for(rest_points, anchor, radius)?;
            }
        }
    }
    if !clear(&placement, wrap) {
        return Err("no clear arrangement found for this panel on this anchor; try another anchor or a larger clearance".into());
    }
    Ok(Arrangement {
        placement,
        wrap,
        radius_m: radius,
        skin_m: first.skin_m,
    })
}

/// The arrangement measured on the anchor, before any check against the body.
fn fit(
    rest_points: &[DVec2],
    anchor: &AnchorFrame,
    clearance_m: f64,
    max_wrap_degrees: Option<f64>,
    surface: Option<&crate::avatar::SurfaceMesh>,
    elliptical: bool,
) -> Result<Arrangement, String> {
    let (lo, hi) = rest_points
        .iter()
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let centre = (lo + hi) / 2.0;
    let (half_width, height) = ((hi.x - lo.x) / 2.0, hi.y - lo.y);
    let heights: Vec<f64> = (0..5).map(|k| (k as f64 / 4.0 - 0.5) * height).collect();
    // Whether the anchor's axis is inside the body at height `h`: a ray from a point outside
    // (past a bent elbow, between the legs) says nothing about the skin round the axis.
    let axis_inside = |h: f64| surface.is_some_and(|s| inside(s, anchor.origin + anchor.up * h));
    let inside_heights: Vec<bool> = heights.iter().map(|h| axis_inside(*h)).collect();
    let ray = |angle: f64, h: f64| {
        let dir = DQuat::from_axis_angle(anchor.up, angle) * anchor.outward;
        surface.and_then(|s| surface_distance(s, anchor.origin + anchor.up * h, dir))
    };
    let ray_at = |angle: f64, k: usize| {
        if inside_heights[k] {
            ray(angle, heights[k])
        } else {
            None
        }
    };
    // Where a ray leaves the body for good within a panel's reach: past the torso, and past
    // an arm beside it too when the panel would not fit in between.
    let reach = half_width.max(height) + clearance_m + 0.3;
    let last_exit = |angle: f64, h: f64, from_inside: bool| {
        let dir = DQuat::from_axis_angle(anchor.up, angle) * anchor.outward;
        let s = surface?;
        let mut origin = anchor.origin + anchor.up * h;
        let (mut travelled, mut last) = (0.0, None);
        let mut leaving = from_inside;
        for _ in 0..16 {
            let Some(d) = surface_distance(s, origin, dir) else {
                break;
            };
            if !leaving && last.is_some() && d >= 2.0 * clearance_m {
                // Room for the panel between the body and the next part.
                break;
            }
            travelled += d;
            if travelled > reach {
                break;
            }
            if leaving {
                last = Some(travelled);
            }
            leaving = !leaving;
            origin += dir * (d + 1e-6);
            travelled += 1e-6;
        }
        last
    };
    let skin_ahead = (0..heights.len())
        .filter_map(|k| ray_at(0.0, k))
        .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.max(d))));
    let mut radius = skin_ahead.unwrap_or(0.0) + clearance_m;
    let wrap = match max_wrap_degrees {
        Some(max) if surface.is_some() && radius > 0.01 && !elliptical => {
            let max = max.clamp(1.0, 180.0).to_radians();
            for _ in 0..4 {
                let span = (half_width / radius).min(max);
                for k in 0..=12 {
                    let angle = -span + 2.0 * span * k as f64 / 12.0;
                    for k in 0..heights.len() {
                        if let Some(d) = ray_at(angle, k) {
                            radius = radius.max(d + clearance_m);
                        }
                    }
                }
            }
            // positive radius curves toward local −z; local z maps to right × up
            let toward_body = if anchor.right.cross(anchor.up).dot(anchor.outward) > 0.0 {
                1.0
            } else {
                -1.0
            };
            Some(crate::garment::template::PanelWrap {
                axis_x_m: centre.x,
                radius_m: toward_body * radius,
                max_angle_degrees: max.to_degrees(),
                half_width_m: None,
            })
        }
        Some(max) if surface.is_some() && radius > 0.01 => {
            let max = max.clamp(1.0, 180.0).to_radians();
            // An ellipse around the anchor axis, `depth` toward the panel centre and `across`
            // sideways, as small as clears the body by `clearance_m` along every sampled ray.
            let front = radius;
            let (mut across, mut depth) = (radius, radius);
            // Which way along the pattern's u a positive turn about `up` goes.
            let turn = if anchor.up.cross(anchor.outward).dot(anchor.right) > 0.0 {
                1.0
            } else {
                -1.0
            };
            let levels: Vec<f64> = (0..9).map(|k| (k as f64 / 8.0 - 0.5) * height).collect();
            let levels_inside: Vec<bool> = levels.iter().map(|h| axis_inside(*h)).collect();
            for _ in 0..8 {
                let span = ellipse_parameter(across, depth, half_width).min(max);
                let mut needs = Vec::new();
                for k in 0..=24 {
                    let t = -span + 2.0 * span * k as f64 / 24.0;
                    let angle = (across * t.sin()).atan2(depth * t.cos());
                    let u = centre.x + turn * t.signum() * ellipse_arc(across, depth, t.abs());
                    for (h, from_inside) in levels.iter().zip(&levels_inside) {
                        // Only where the panel itself lies.
                        if !point_in_outline(rest_points, DVec2::new(u, centre.y - h)) {
                            continue;
                        }
                        if let Some(d) = last_exit(angle, *h, *from_inside) {
                            needs.push((angle, d + clearance_m));
                        }
                    }
                }
                // Deep enough straight ahead and wherever the ray leans little.
                depth = needs
                    .iter()
                    .map(|(a, n)| n * a.cos().abs() * 1.01)
                    .fold(front, f64::max);
                // Then wide enough for every ray: r(φ) = ab / √(b² sin²φ + a² cos²φ) ≥ need.
                let wide = needs
                    .iter()
                    .filter(|(a, _)| a.sin().abs() > 1e-3)
                    .map(|(a, n)| {
                        let (s, c) = (a.sin().abs(), a.cos().abs());
                        n * depth * s / (depth * depth - n * n * c * c).max(1e-12).sqrt()
                    })
                    .fold(0.5 * depth, f64::max);
                across = wide.min(3.0 * depth);
            }
            radius = depth;
            // positive radius curves toward local −z; local z maps to right × up
            let toward_body = if anchor.right.cross(anchor.up).dot(anchor.outward) > 0.0 {
                1.0
            } else {
                -1.0
            };
            Some(crate::garment::template::PanelWrap {
                axis_x_m: centre.x,
                radius_m: toward_body * depth,
                max_angle_degrees: max.to_degrees(),
                half_width_m: Some(across),
            })
        }
        _ => None,
    };
    Ok(Arrangement {
        placement: placement_for(rest_points, anchor, radius)?,
        wrap,
        radius_m: radius,
        skin_m: skin_ahead,
    })
}

/// Arc length of the ellipse `(a sin t, b cos t)` from `t = 0` to `t`.
fn ellipse_arc(a: f64, b: f64, t: f64) -> f64 {
    let speed = |t: f64| (a * a * t.cos().powi(2) + b * b * t.sin().powi(2)).sqrt();
    let steps = 64;
    let h = t / steps as f64;
    (0..steps)
        .map(|k| {
            let t0 = k as f64 * h;
            h / 6. * (speed(t0) + 4. * speed(t0 + h / 2.) + speed(t0 + h))
        })
        .sum()
}

/// The parameter `t` of the ellipse `(a sin t, b cos t)` an arc `length` long from `t = 0`
/// reaches (at most a half turn).
fn ellipse_parameter(a: f64, b: f64, length: f64) -> f64 {
    let speed = |t: f64| (a * a * t.cos().powi(2) + b * b * t.sin().powi(2)).sqrt();
    let h = std::f64::consts::PI / 512.;
    let (mut t, mut walked) = (0.0, 0.0);
    while t < std::f64::consts::PI {
        let segment = h / 6. * (speed(t) + 4. * speed(t + h / 2.) + speed(t + h));
        if walked + segment >= length {
            return t + h * (length - walked) / segment;
        }
        walked += segment;
        t += h;
    }
    std::f64::consts::PI
}

/// Sample points of a panel in material coordinates: its outline every 1.5 cm (an edge is
/// what meets an arm or a neck first) plus a grid inside it.
fn panel_samples(outline: &[DVec2], n: usize) -> Vec<DVec2> {
    let (lo, hi) = outline
        .iter()
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
    let mut out = Vec::new();
    for (k, a) in outline.iter().enumerate() {
        let b = outline[(k + 1) % outline.len()];
        let steps = ((a.distance(b) / 0.015).ceil() as usize).clamp(1, 200);
        out.extend((0..steps).map(|i| a.lerp(b, i as f64 / steps as f64)));
    }
    for i in 0..n {
        for j in 0..n {
            let p = lo
                + (hi - lo) * DVec2::new((i as f64 + 0.5) / n as f64, (j as f64 + 0.5) / n as f64);
            if crate::garment::arrangement::point_in_outline(outline, p) {
                out.push(p);
            }
        }
    }
    out
}

pub(crate) fn point_in_outline(pts: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Parity test along a fixed oblique ray; valid for closed avatar surfaces.
fn inside(surface: &crate::avatar::SurfaceMesh, p: DVec3) -> bool {
    let dir = DVec3::new(0.2311, 0.4423, 0.8668).normalize();
    let mut hits = 0;
    let mut origin = p;
    while let Some(t) = surface_distance(surface, origin, dir) {
        hits += 1;
        origin += dir * (t + 1e-7);
        if hits > 64 {
            break;
        }
    }
    hits % 2 == 1
}

fn distance_to_surface(surface: &crate::avatar::SurfaceMesh, p: DVec3) -> f64 {
    let mut best = f64::INFINITY;
    for t in &surface.triangles {
        let (a, b, c) = (
            surface.vertices[t[0]],
            surface.vertices[t[1]],
            surface.vertices[t[2]],
        );
        best = best.min(point_triangle_distance(p, a, b, c));
    }
    best
}

/// Ericson, Real-Time Collision Detection §5.1.5.
fn point_triangle_distance(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.length();
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return bp.length();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return (p - (a + ab * (d1 / (d1 - d3)))).length();
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return cp.length();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return (p - (a + ac * (d2 / (d2 - d6)))).length();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return (p - (b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))))).length();
    }
    let denom = 1.0 / (va + vb + vc);
    (p - (a + ab * (vb * denom) + ac * (vc * denom))).length()
}
