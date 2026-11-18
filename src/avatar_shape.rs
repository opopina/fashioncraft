//! Body measurements of a standing avatar, and the avatar reshaped to new ones.
//!
//! Girths are the lengths of the torso's horizontal cross-sections: bust at the chest joint,
//! waist the narrowest between the lowest spine joint and the chest, hip the widest between
//! the pelvis and the upper legs' joints. Reshaping scales the whole body to the new height
//! about its feet, then scales the torso across (x and z, about the spine) by a factor that
//! runs smoothly through the bust, waist and hip levels and back to one at the shoulders and
//! the knees. Skin weights carry the change to the limbs: arms move with their shoulder, legs
//! with their hip joint. A few rounds of measuring and correcting land the girths on target.
//!
//! The arm is measured shoulder to elbow to wrist along the joints, the inseam from the floor
//! up to the crotch, where the body's horizontal section near the spine splits into the two
//! legs. Reshaping stretches the body vertically below and above the crotch, keeping the
//! height, so the legs get the inseam asked for, and stretches each arm along itself from
//! its shoulder.
use crate::{avatar::SurfaceMesh, avatar_landmarks::sections, avatar_rig::Rig};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// Height and girths, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyMeasurements {
    pub height_m: f64,
    pub bust_m: f64,
    pub waist_m: f64,
    pub hip_m: f64,
    /// Shoulder to elbow to wrist, along the joints.
    pub arm_m: f64,
    /// Floor to crotch.
    pub inseam_m: f64,
}

/// The levels the girths are taken at, metres above the floor's origin.
#[derive(Clone, Copy, Debug)]
struct Levels {
    bust: f64,
    waist: f64,
    hip: f64,
}

/// Length of the cross-section through the torso at height `y`: the one nearest the spine.
fn girth(surface: &SurfaceMesh, axis: DVec2, y: f64) -> Option<f64> {
    sections(surface, DVec3::new(axis.x, y, axis.y), DVec3::Y)
        .into_iter()
        .min_by(|a, b| {
            let d = |s: &crate::avatar_landmarks::Section| {
                DVec2::new(s.centre.x - axis.x, s.centre.z - axis.y).length()
            };
            d(a).total_cmp(&d(b))
        })
        .map(|s| s.length)
}

fn joint(landmarks: &BTreeMap<String, DVec3>, id: &str) -> Result<DVec3, String> {
    landmarks
        .get(id)
        .copied()
        .ok_or_else(|| format!("The avatar lacks the joint {id}"))
}

fn levels(
    surface: &SurfaceMesh,
    landmarks: &BTreeMap<String, DVec3>,
) -> Result<(Levels, DVec2), String> {
    let pelvis = joint(landmarks, "joint-pelvis")?;
    let axis = DVec2::new(pelvis.x, pelvis.z);
    let chest = joint(landmarks, "joint-spine-1")?.y;
    let low_spine = joint(landmarks, "joint-spine-3")?.y;
    let hips = joint(landmarks, "joint-l-upper-leg")?.y;
    let scan = |from: f64, to: f64, widest: bool| -> Result<f64, String> {
        (0..=20)
            .map(|k| from + (to - from) * k as f64 / 20.)
            .filter_map(|y| girth(surface, axis, y).map(|g| (y, g)))
            .max_by(|a, b| {
                if widest {
                    a.1.total_cmp(&b.1)
                } else {
                    b.1.total_cmp(&a.1)
                }
            })
            .map(|(y, _)| y)
            .ok_or_else(|| "The torso has no cross-section there".to_string())
    };
    Ok((
        Levels {
            bust: chest,
            waist: scan(low_spine, chest, false)?,
            hip: scan(hips, pelvis.y, true)?,
        },
        axis,
    ))
}

/// Height of the crotch: scanning down from the pelvis, the first level where the section
/// nearest the spine is no longer around it but around a leg.
fn crotch(surface: &SurfaceMesh, landmarks: &BTreeMap<String, DVec3>) -> Result<f64, String> {
    let pelvis = joint(landmarks, "joint-pelvis")?;
    let knee = joint(landmarks, "joint-l-knee")?;
    let axis = DVec2::new(pelvis.x, pelvis.z);
    let mut y = pelvis.y;
    while y > knee.y {
        let split = sections(surface, DVec3::new(axis.x, y, axis.y), DVec3::Y)
            .into_iter()
            .map(|s| DVec2::new(s.centre.x, s.centre.z).distance(axis))
            .fold(f64::INFINITY, f64::min)
            > 0.03;
        if split {
            return Ok(y);
        }
        y -= 0.002;
    }
    Err("The legs do not part below the pelvis".into())
}

/// Shoulder to elbow to wrist on one side, with that side's shoulder and hand joints.
fn arm(landmarks: &BTreeMap<String, DVec3>, side: &str) -> Result<(f64, DVec3, DVec3), String> {
    let s = joint(landmarks, &format!("joint-{side}-shoulder"))?;
    let e = joint(landmarks, &format!("joint-{side}-elbow"))?;
    let h = joint(landmarks, &format!("joint-{side}-hand"))?;
    Ok((s.distance(e) + e.distance(h), s, h))
}

/// The avatar's height and girths.
pub fn measure(
    surface: &SurfaceMesh,
    landmarks: &BTreeMap<String, DVec3>,
) -> Result<BodyMeasurements, String> {
    let (lo, hi) = bounds(surface);
    let (l, axis) = levels(surface, landmarks)?;
    let at = |y| girth(surface, axis, y).ok_or("The torso has no cross-section there");
    Ok(BodyMeasurements {
        height_m: hi.y - lo.y,
        bust_m: at(l.bust)?,
        waist_m: at(l.waist)?,
        hip_m: at(l.hip)?,
        arm_m: arm(landmarks, "l")?.0,
        inseam_m: crotch(surface, landmarks)? - lo.y,
    })
}

fn bounds(surface: &SurfaceMesh) -> (DVec3, DVec3) {
    surface.vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

/// The torso's scale across at height `y`: the given factors at the three levels, easing to
/// one at the shoulders above and the knees below.
fn factor(y: f64, l: &Levels, top: f64, bottom: f64, f: [f64; 3]) -> f64 {
    let smooth = |a: f64, b: f64, fa: f64, fb: f64| {
        let t = ((y - a) / (b - a)).clamp(0., 1.);
        let s = t * t * (3. - 2. * t);
        fa + (fb - fa) * s
    };
    if y >= l.bust {
        smooth(l.bust, top, f[0], 1.)
    } else if y >= l.waist {
        smooth(l.waist, l.bust, f[1], f[0])
    } else if y >= l.hip {
        smooth(l.hip, l.waist, f[2], f[1])
    } else {
        smooth(bottom, l.hip, 1., f[2])
    }
}

/// Bones whose vertices follow the torso's scale across, and the joints the limbs hang from.
fn region(bone: &str) -> Option<&'static str> {
    match bone {
        "pelvis" | "spine" | "chest" => Some("torso"),
        "clavicle.L" | "upper_arm.L" | "forearm.L" | "hand.L" => Some("joint-l-shoulder"),
        "clavicle.R" | "upper_arm.R" | "forearm.R" | "hand.R" => Some("joint-r-shoulder"),
        "thigh.L" | "shin.L" | "foot.L" => Some("joint-l-upper-leg"),
        "thigh.R" | "shin.R" | "foot.R" => Some("joint-r-upper-leg"),
        _ => None,
    }
}

/// `surface` and its joint `landmarks` reshaped to `target`. Returns both.
pub fn reshape(
    surface: &SurfaceMesh,
    landmarks: &BTreeMap<String, DVec3>,
    target: &BodyMeasurements,
) -> Result<(SurfaceMesh, BTreeMap<String, DVec3>), String> {
    let valid = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
    if !valid(target.height_m, 0.5, 2.5)
        || !valid(target.bust_m, 0.4, 2.0)
        || !valid(target.waist_m, 0.3, 2.0)
        || !valid(target.hip_m, 0.4, 2.2)
    {
        return Err("Measurements must be a height of 50–250 cm and girths of 30–220 cm".into());
    }
    if !valid(target.arm_m, 0.15, 1.2)
        || !valid(target.inseam_m, 0.2, 1.4)
        || target.inseam_m > target.height_m * 0.6
    {
        return Err(
            "The arm must be 15–120 cm and the inseam 20–140 cm, below 60 % of the height".into(),
        );
    }
    let current = measure(surface, landmarks)?;
    // The whole body to the new height, about its feet.
    let (lo, _) = bounds(surface);
    let pelvis = joint(landmarks, "joint-pelvis")?;
    let foot = DVec3::new(pelvis.x, lo.y, pelvis.z);
    let k = target.height_m / current.height_m;
    let grow = |p: DVec3| foot + (p - foot) * k;
    let mut body = SurfaceMesh {
        vertices: surface.vertices.iter().map(|p| grow(*p)).collect(),
        triangles: surface.triangles.clone(),
    };
    let mut joints: BTreeMap<String, DVec3> = landmarks
        .iter()
        .map(|(id, p)| (id.clone(), grow(*p)))
        .collect();
    // The legs to the inseam asked for, the rest above them taking up the difference so the
    // height stays: one stretch below the crotch and another above it.
    let floor = lo.y;
    let fork = crotch(&body, &joints)?;
    let below = target.inseam_m / (fork - floor);
    let above = (target.height_m - target.inseam_m) / (floor + target.height_m - fork);
    if !(0.5..=2.).contains(&below) || !(0.5..=2.).contains(&above) {
        return Err("The inseam asked for is too far from the avatar's".into());
    }
    let stretch = |p: DVec3| {
        let y = if p.y <= fork {
            floor + (p.y - floor) * below
        } else {
            floor + target.inseam_m + (p.y - fork) * above
        };
        DVec3::new(p.x, y, p.z)
    };
    for p in &mut body.vertices {
        *p = stretch(*p);
    }
    for p in joints.values_mut() {
        *p = stretch(*p);
    }
    let rig = Rig::new(&joints, &body)?;
    let rest = body.clone();
    let rest_joints = joints.clone();
    let (l, axis) = levels(&rest, &rest_joints)?;
    let top = joint(&rest_joints, "joint-neck")?.y;
    let bottom = joint(&rest_joints, "joint-l-knee")?.y;
    let mut f = [1.; 3];
    for _ in 0..6 {
        let now = measure(&body, &joints)?;
        f[0] *= target.bust_m / now.bust_m;
        f[1] *= target.waist_m / now.waist_m;
        f[2] *= target.hip_m / now.hip_m;
        // Across the torso at a point, about the spine.
        let across = |p: DVec3| {
            let s = factor(p.y, &l, top, bottom, f);
            DVec3::new(
                axis.x + (p.x - axis.x) * s,
                p.y,
                axis.y + (p.z - axis.y) * s,
            )
        };
        let moved = |id: &str| -> DVec3 {
            let j = rest_joints[id];
            across(j) - j
        };
        body.vertices = rest
            .vertices
            .iter()
            .zip(&rig.weights)
            .map(|(p, weights)| {
                let mut shift = DVec3::ZERO;
                for (bone, w) in weights {
                    match region(rig.bones[*bone].name) {
                        Some("torso") => shift += (across(*p) - *p) * *w,
                        Some(hang) => shift += moved(hang) * *w,
                        None => {}
                    }
                }
                *p + shift
            })
            .collect();
        joints = rest_joints
            .iter()
            .map(|(id, p)| {
                let side = |s: &str| id.contains(&format!("-{s}-"));
                let shift = if id.contains("arm")
                    || id.contains("shoulder")
                    || id.contains("elbow")
                    || id.contains("hand")
                    || id.contains("finger")
                    || id.contains("clavicle")
                {
                    if side("l") {
                        moved("joint-l-shoulder")
                    } else {
                        moved("joint-r-shoulder")
                    }
                } else if id.contains("leg")
                    || id.contains("knee")
                    || id.contains("ankle")
                    || id.contains("foot")
                    || id.contains("toe")
                {
                    if side("l") {
                        moved("joint-l-upper-leg")
                    } else {
                        moved("joint-r-upper-leg")
                    }
                } else if id.contains("pelvis") || id.contains("spine") {
                    across(*p) - *p
                } else {
                    DVec3::ZERO
                };
                (id.clone(), *p + shift)
            })
            .collect();
    }
    // Each arm along itself, from its shoulder, to the length asked for.
    for (side, bones) in [
        ("l", ["upper_arm.L", "forearm.L", "hand.L"]),
        ("r", ["upper_arm.R", "forearm.R", "hand.R"]),
    ] {
        let (length, shoulder, hand) = arm(&joints, side)?;
        let k = target.arm_m / length;
        if !(0.5..=2.).contains(&k) {
            return Err("The arm length asked for is too far from the avatar's".into());
        }
        let along = (hand - shoulder).normalize_or_zero();
        let reach = |p: DVec3| along * ((k - 1.) * (p - shoulder).dot(along).max(0.));
        for (p, weights) in body.vertices.iter_mut().zip(&rig.weights) {
            let w: f64 = weights
                .iter()
                .filter(|(bone, _)| bones.contains(&rig.bones[*bone].name))
                .map(|(_, w)| *w)
                .sum();
            *p += reach(*p) * w;
        }
        for (id, p) in joints.iter_mut() {
            let on_arm = ["elbow", "hand", "finger", "wrist"]
                .iter()
                .any(|part| id.contains(part));
            if on_arm && id.contains(&format!("-{side}-")) {
                *p += reach(*p);
            }
        }
    }
    body.validate()?;
    Ok((body, joints))
}
