//! What an agent reads after simulating, to check its own work: each seam's gap and whether it
//! closed, where the cloth is strained or presses on the body, where it went inside the body,
//! and whether the drape has settled. [`check`] sums these up as a short list of problems, each
//! with where it is (the piece, the part of it and the nearest segment, and the height above
//! the floor), worked out from the document and the drape alone so it can be tested without a
//! window.
use crate::agent_ops::{self, Live, cm};
use crate::avatar::MeshCollider;
use crate::garment::{CompiledGarment, GarmentTemplate};
use glam::{DVec2, DVec3};
use serde_json::{Value, json};

/// Stretch beyond which cloth counts as strained, as a fraction of its rest length: woven
/// cloth hardly gives 3 %, so 8 % means a seam or the body is pulling it hard.
pub const STRAIN_LIMIT: f64 = 0.08;
/// Pressure on the body beyond which a garment feels tight, Pa (compression wear sits at 2–4
/// kPa; everyday clothes under 1 kPa).
pub const PRESSURE_LIMIT_PA: f64 = 2000.;
/// Cloth further inside the body than this, metres, is a problem rather than contact.
pub const INSIDE_LIMIT_M: f64 = 0.003;
/// The drape has settled when its fastest point moves slower than this, m/s…
pub const SETTLED_MAX_SPEED: f64 = 0.05;
/// …and its points move this fast on average (root mean square), m/s.
pub const SETTLED_RMS_SPEED: f64 = 0.01;

/// The drape and what it lies on.
pub struct Input<'a> {
    pub template: &'a GarmentTemplate,
    pub live: Live<'a>,
    /// The cloth's velocities, m/s.
    pub v: &'a [DVec3],
    pub body: Option<&'a MeshCollider>,
    /// Pressure the cloth puts on the body, by triangle, Pa (the window works it out with the
    /// 3D view's smoothed normals). None leaves pressure out.
    pub pressure: Option<Vec<f64>>,
}

/// Where a point of the cloth is: its piece, the part of the piece, the nearest segment and
/// its height above the lowest point of the body (or of the cloth).
fn place(input: &Input, vertex: usize, floor: f64) -> Value {
    let garment = input.live.garment;
    let Some(range) = garment.panels.iter().find(|p| p.vertices.contains(&vertex)) else {
        return json!({});
    };
    let uv = garment.mesh.uv[vertex];
    let panel = input.template.panels.iter().find(|p| p.id == range.id);
    let mut area = String::new();
    let mut nearest = None;
    if let Some(polygon) = panel.and_then(|p| p.polygon.as_ref())
        && !polygon.points.is_empty()
    {
        let (lo, hi) = polygon.points.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        let share = (uv - lo) / (hi - lo).max(DVec2::splat(1e-9));
        // The board draws y downward: a small y is the top of the piece.
        let row = ["top", "middle", "bottom"][((share.y * 3.).floor() as usize).min(2)];
        let column = ["left", "centre", "right"][((share.x * 3.).floor() as usize).min(2)];
        area = if row == "middle" && column == "centre" {
            "centre".into()
        } else {
            format!("{row} {column}")
        };
        let n = polygon.points.len();
        nearest = (0..n)
            .map(|i| {
                let (a, b) = (polygon.points[i], polygon.points[(i + 1) % n]);
                let t = ((uv - a).dot(b - a) / (b - a).length_squared().max(1e-12)).clamp(0., 1.);
                (uv.distance(a + (b - a) * t), &polygon.edges[i])
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(d, e)| json!({"segment": e, "cm_away": cm(d)}));
    }
    let x = input.live.x[vertex];
    json!({
        "piece": range.id,
        "area": area,
        "near": nearest,
        "at_cm": [cm(uv.x), cm(uv.y)],
        "height_cm": cm(x.y - floor),
    })
}

fn where_text(place: &Value) -> String {
    let piece = place["piece"].as_str().unwrap_or("?");
    let area = place["area"].as_str().unwrap_or("");
    let near = place["near"]["segment"]
        .as_str()
        .map(|s| format!(" near {s}"))
        .unwrap_or_default();
    let height = place["height_cm"]
        .as_f64()
        .map(|h| format!(", {h:.0} cm up"))
        .unwrap_or_default();
    format!("{piece} {area}{near}{height}")
}

/// The worst triangle of each piece by `value`, where it passes `limit`.
fn hot_spots(
    input: &Input,
    values: &[f64],
    limit: f64,
    floor: f64,
    unit: impl Fn(f64) -> Value,
) -> Vec<Value> {
    let garment = input.live.garment;
    let mut spots = Vec::new();
    for range in &garment.panels {
        let triangles = range.triangles.clone();
        let (mut worst, mut over_area, mut area) = (None::<(usize, f64)>, 0., 0.);
        for t in triangles {
            let Some(&value) = values.get(t) else {
                continue;
            };
            let tri = &garment.mesh.triangles[t];
            area += tri.area;
            if value > limit {
                over_area += tri.area;
            }
            if worst.is_none_or(|(_, w)| value > w) {
                worst = Some((t, value));
            }
        }
        if let Some((t, value)) = worst
            && value > limit
        {
            let tri = &garment.mesh.triangles[t];
            let at = place(input, tri.vertices[0], floor);
            spots.push(json!({
                "piece": range.id,
                "worst": unit(value),
                "share_of_piece_over_limit_percent": (over_area / area.max(1e-12) * 100.).round(),
                "where": at,
            }));
        }
    }
    spots.sort_by(|a, b| {
        b["worst"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["worst"].as_f64().unwrap_or(0.))
    });
    spots
}

/// Every signal, and the problems they show, most serious first.
pub fn check(input: &Input) -> Value {
    let garment: &CompiledGarment = input.live.garment;
    let x = input.live.x;
    let mut problems: Vec<String> = Vec::new();
    if x.is_empty() || garment.panels.is_empty() {
        return json!({"ok": false, "problems": ["The project has no pieces yet"]});
    }
    let floor = input
        .body
        .map(|b| {
            b.surface
                .vertices
                .iter()
                .map(|v| v.y)
                .fold(f64::INFINITY, f64::min)
        })
        .filter(|f| f.is_finite())
        .unwrap_or_else(|| x.iter().map(|p| p.y).fold(f64::INFINITY, f64::min));
    if !input.live.simulated {
        problems
            .push("Not simulated yet: seams are still apart. Simulate, then check again".into());
    }

    // Seams.
    let seams = agent_ops::describe_seams(input.template, Some(&input.live));
    let mut open = Vec::new();
    for s in &seams {
        if s["active"] == json!(false) {
            continue;
        }
        if s["closed"] == json!(false) {
            let gap = s["gap_mm"].as_f64().unwrap_or(0.);
            open.push(json!({"seam": s["id"], "a": s["a"], "b": s["b"], "gap_mm": gap}));
        }
    }
    open.sort_by(|a, b| {
        b["gap_mm"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&a["gap_mm"].as_f64().unwrap_or(0.))
    });
    for s in &open {
        problems.push(format!(
            "Seam {} is open: {} mm gap between {} and {}",
            s["seam"].as_str().unwrap_or("?"),
            s["gap_mm"],
            s["a"],
            s["b"]
        ));
    }

    // Strain, by triangle: the larger fibre stretch beyond rest.
    let strain: Vec<f64> = garment
        .mesh
        .triangles
        .iter()
        .map(|t| {
            let m = &garment.materials[t.material];
            crate::energy::stretches(x, t, m)
                .iter()
                .map(|s| s - 1.)
                .fold(0., f64::max)
        })
        .collect();
    let strained = if input.live.simulated {
        hot_spots(input, &strain, STRAIN_LIMIT, floor, |v| {
            json!((v * 1000.).round() / 10.)
        })
    } else {
        Vec::new()
    };
    for s in &strained {
        problems.push(format!(
            "Strained {} %: {} ({} % of the piece over {} %)",
            s["worst"],
            where_text(&s["where"]),
            s["share_of_piece_over_limit_percent"],
            STRAIN_LIMIT * 100.
        ));
    }

    // Pressure on the body.
    let pressed = match (&input.pressure, input.live.simulated) {
        (Some(p), true) => hot_spots(input, p, PRESSURE_LIMIT_PA, floor, |v| {
            json!((v / 10.).round() / 100.)
        }),
        _ => Vec::new(),
    };
    for s in &pressed {
        problems.push(format!(
            "Tight: {} kPa on the body at {}",
            s["worst"],
            where_text(&s["where"])
        ));
    }

    // Cloth inside the body.
    let mut inside = Vec::new();
    if let Some(body) = input.body.filter(|b| !b.surface.vertices.is_empty()) {
        for range in &garment.panels {
            let (mut count, mut deepest) = (0usize, None::<(usize, f64)>);
            for v in range.vertices.clone() {
                let depth = -body.signed_distance(x[v]);
                if depth > INSIDE_LIMIT_M {
                    count += 1;
                    if deepest.is_none_or(|(_, d)| depth > d) {
                        deepest = Some((v, depth));
                    }
                }
            }
            if let Some((v, depth)) = deepest {
                let at = place(input, v, floor);
                problems.push(format!(
                    "Inside the body: {count} point(s) of {}, up to {:.1} cm deep, at {}",
                    range.id,
                    depth * 100.,
                    where_text(&at)
                ));
                inside.push(json!({
                    "piece": range.id,
                    "points": count,
                    "deepest_cm": cm(depth),
                    "where": at,
                }));
            }
        }
    }

    // Settling: how fast the cloth still moves.
    let n = input.v.len().min(x.len());
    let (mut sum, mut max, mut kinetic) = (0., 0f64, 0.);
    for (i, v) in input.v.iter().take(n).enumerate() {
        let s2 = v.length_squared();
        sum += s2;
        max = max.max(s2.sqrt());
        kinetic += 0.5 * garment.mesh.masses.get(i).copied().unwrap_or(0.) * s2;
    }
    let rms = if n > 0 { (sum / n as f64).sqrt() } else { 0. };
    let settled = input.live.simulated && max < SETTLED_MAX_SPEED && rms < SETTLED_RMS_SPEED;
    if input.live.simulated && !settled {
        problems.push(format!(
            "Still moving ({:.1} cm/s on average, up to {:.0} cm/s): simulate longer before \
             judging the drape",
            rms * 100.,
            max * 100.
        ));
    }

    // What the document itself says is wrong, beyond what is above.
    for w in agent_ops::warnings(input.template, Some(&input.live)) {
        if !w.contains("open") && !problems.contains(&w) {
            problems.push(w);
        }
    }

    json!({
        "ok": problems.is_empty(),
        "problems": problems,
        "simulated": input.live.simulated,
        "seams": {"total": seams.len(), "open": open},
        "strain_over_limit": strained,
        "strain_limit_percent": STRAIN_LIMIT * 100.,
        "pressure_over_limit": pressed,
        "pressure_limit_kpa": PRESSURE_LIMIT_PA / 1000.,
        "pressure_measured": input.pressure.is_some(),
        "inside_body": inside,
        "settling": {
            "settled": settled,
            "rms_speed_cm_s": (rms * 1000.).round() / 10.,
            "max_speed_cm_s": (max * 1000.).round() / 10.,
            "kinetic_energy_mj": (kinetic * 1e6).round() / 1e3,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_laid_out_garment_is_not_judged_and_an_open_seam_is_found() {
        let template = crate::garment::presets::tshirt(
            &crate::garment::presets::TshirtMeasurements::default(),
            1,
        )
        .unwrap();
        let garment = template.compile().unwrap();
        let x = garment.initial_state.x.clone();
        let v = vec![DVec3::ZERO; x.len()];
        let report = check(&Input {
            template: &template,
            live: Live {
                garment: &garment,
                x: &x,
                simulated: false,
            },
            v: &v,
            body: None,
            pressure: None,
        });
        assert_eq!(report["ok"], json!(false));
        assert!(
            report["problems"][0]
                .as_str()
                .unwrap()
                .contains("Not simulated")
        );

        // Pretend it draped: seams apart stay open, a fast point is still moving, a pulled
        // triangle is strained.
        let mut moved = x.clone();
        for p in &mut moved {
            p.y -= 0.01;
        }
        let tri = garment.mesh.triangles[0].vertices;
        let pull = (moved[tri[0]] - moved[tri[1]]) * 0.5;
        moved[tri[0]] += pull;
        let mut v = v;
        v[0] = DVec3::new(0., 0.3, 0.);
        let report = check(&Input {
            template: &template,
            live: Live {
                garment: &garment,
                x: &moved,
                simulated: true,
            },
            v: &v,
            body: None,
            pressure: Some(vec![3000.; garment.mesh.triangles.len()]),
        });
        let problems: Vec<&str> = report["problems"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            problems.iter().any(|p| p.starts_with("Seam ")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.starts_with("Strained")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.starts_with("Tight")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.starts_with("Still moving")),
            "{problems:?}"
        );
        assert_eq!(report["settling"]["settled"], json!(false));
        let spot = &report["strain_over_limit"][0]["where"];
        assert!(spot["piece"].is_string() && spot["near"]["segment"].is_string());
    }
}
