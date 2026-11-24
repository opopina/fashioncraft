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
/// A strained or tight spot is a problem when it covers at least this share of its piece, %
/// (or passes twice its limit); smaller ones are listed apart as small spots.
pub const SPOT_SHARE_PERCENT: f64 = 1.;
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
    /// The avatar's height, bust, waist, hip, arm and inseam, cm, for the pattern's sanity
    /// checks.
    pub body_cm: Option<[f64; 6]>,
}

/// Pattern choices that rarely make sense, found by the segments' names (drafted pieces and
/// pieces named like them): a bodice side seam so short the armhole runs to the waist, an
/// armhole far deeper or shallower than the body, a neckline that drops very low. Sizes scale
/// with the avatar's height (168 cm when unknown).
pub fn pattern_checks(template: &GarmentTemplate, body_cm: Option<[f64; 6]>) -> Vec<String> {
    let scale = body_cm.map_or(1., |b| (b[0] / 168.).clamp(0.5, 1.5));
    let mut out = Vec::new();
    for panel in &template.panels {
        let Some(polygon) = panel.polygon.as_ref() else {
            continue;
        };
        let n = polygon.points.len();
        let ends = |e: &str| {
            let i = polygon.edges.iter().position(|x| x == e)?;
            Some((polygon.points[i], polygon.points[(i + 1) % n]))
        };
        let has_armhole = polygon.edges.iter().any(|e| e.contains("armhole"));
        for e in &polygon.edges {
            let Some((a, b)) = ends(e) else { continue };
            let length = crate::authoring::edge_length(polygon, e).unwrap_or(a.distance(b));
            if has_armhole && e.contains("side") && length < 0.08 * scale {
                out.push(format!(
                    "Pattern: the side seam {}.{e} is only {:.1} cm, so the armhole runs almost to \
                     the waist; a bodice's side seam is usually {:.0}–{:.0} cm",
                    panel.id,
                    length * 100.,
                    15. * scale,
                    22. * scale
                ));
            }
            if e.contains("armhole") {
                let depth = (a.y - b.y).abs();
                let (low, high) = (0.12 * scale, 0.30 * scale);
                if depth > high || depth < low {
                    out.push(format!(
                        "Pattern: the armhole {}.{e} is {:.1} cm deep; for this body it is usually \
                         {:.0}–{:.0} cm",
                        panel.id,
                        depth * 100.,
                        17. * scale,
                        24. * scale
                    ));
                }
            }
        }
        // The neckline's drop: from the top of the piece to the neckline's lowest point.
        let top = polygon
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min);
        let neck_low = polygon
            .edges
            .iter()
            .filter(|e| e.contains("neck"))
            .filter_map(|e| ends(e))
            .flat_map(|(a, b)| [a.y, b.y])
            .fold(f64::NEG_INFINITY, f64::max);
        if neck_low.is_finite() && neck_low - top > 0.22 * scale {
            out.push(format!(
                "Pattern: the neckline of {} drops {:.0} cm from the shoulder; a deep V is about \
                 {:.0}–{:.0} cm. Check that it is meant",
                panel.id,
                (neck_low - top) * 100.,
                12. * scale,
                18. * scale
            ));
        }
    }
    out
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
                "share_of_piece_over_limit_percent": (over_area / area.max(1e-12) * 1000.).round() / 10.,
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
    // A single strained triangle at a seam's corner is how a seam pulls on cloth; a strained
    // area, or a very strained spot, is a fit problem.
    let significant = |s: &Value, limit: f64| {
        s["share_of_piece_over_limit_percent"]
            .as_f64()
            .unwrap_or(0.)
            >= SPOT_SHARE_PERCENT
            || s["worst"].as_f64().unwrap_or(0.) >= 2. * limit
    };
    let mut small_spots = Vec::new();
    for s in &strained {
        if !significant(s, STRAIN_LIMIT * 100.) {
            small_spots.push(format!(
                "strain {} % at {}",
                s["worst"],
                where_text(&s["where"])
            ));
            continue;
        }
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
        if !significant(s, PRESSURE_LIMIT_PA / 1000.) {
            small_spots.push(format!("{} kPa at {}", s["worst"], where_text(&s["where"])));
            continue;
        }
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

    problems.extend(pattern_checks(input.template, input.body_cm));

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
        "small_spots": small_spots,
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

/// A garment's girth at the height of `at`, as a tape around it reads it: the cuts through
/// every piece there whose middle lies within `reach` (horizontally) of `at`, so sleeves on
/// arms further out stay out, closed with the shortest loop around them all (their convex
/// hull). None when the plane misses the garment.
pub fn garment_girth(x: &[DVec3], triangles: &[[usize; 3]], at: DVec3, reach: f64) -> Option<f64> {
    let flat = |p: DVec3| DVec2::new(p.x - at.x, p.z - at.z);
    let mut points: Vec<DVec2> = Vec::new();
    for cut in crate::measure3d::level_cuts(x, triangles, at.y) {
        if cut.points.is_empty() {
            continue;
        }
        let middle = cut.points.iter().map(|p| flat(*p)).sum::<DVec2>() / cut.points.len() as f64;
        if middle.length() <= reach {
            points.extend(cut.points.iter().map(|p| flat(*p)));
        }
    }
    if points.len() < 3 {
        return None;
    }
    // Monotone chain convex hull.
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<DVec2> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &DVec2>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    Some(
        (0..hull.len())
            .map(|i| hull[i].distance(hull[(i + 1) % hull.len()]))
            .sum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_garment_girth_closes_around_pieces_cut_apart() {
        // Four separate flat pieces, one per side of a 20 cm square tube around the y axis,
        // and a sleeve-like piece far out to the side.
        let mut x = Vec::new();
        let mut triangles = Vec::new();
        let mut quad = |a: DVec3, b: DVec3| {
            let k = x.len();
            x.extend([a, b, b + DVec3::Y, a + DVec3::Y]);
            triangles.push([k, k + 1, k + 2]);
            triangles.push([k, k + 2, k + 3]);
        };
        let c = [(-0.1, -0.1), (0.1, -0.1), (0.1, 0.1), (-0.1, 0.1)];
        for i in 0..4 {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            quad(DVec3::new(a.0, 0., a.1), DVec3::new(b.0, 0., b.1));
        }
        quad(DVec3::new(0.6, 0., -0.05), DVec3::new(0.6, 0., 0.05));
        let girth = garment_girth(&x, &triangles, DVec3::new(0., 0.5, 0.), 0.3).unwrap();
        assert!((girth - 0.8).abs() < 1e-6, "{girth}");
    }

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
            body_cm: None,
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
            body_cm: None,
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

    #[test]
    fn odd_pattern_choices_are_pointed_out() {
        let shirt = crate::garment::presets::shirt(&Default::default(), 1).unwrap();
        // A drafted shirt passes.
        assert_eq!(
            pattern_checks(&shirt, Some([168., 88., 70., 96., 60., 78.])),
            Vec::<String>::new()
        );
        let dress = crate::garment::presets::dress(&Default::default(), 1).unwrap();
        assert_eq!(pattern_checks(&dress, None), Vec::<String>::new());
        // Its front's side seam cut to 2.5 cm and its neckline scooped 25 cm do not.
        let mut odd = shirt.clone();
        let front = odd.panels.iter_mut().find(|p| p.id == "front").unwrap();
        let q = front.polygon.as_mut().unwrap();
        let side = q.edges.iter().position(|e| e == "side-right").unwrap();
        let n = q.points.len();
        let (a, b) = (q.points[side], q.points[(side + 1) % n]);
        q.points[(side + 1) % n] = a + (b - a).normalize() * 0.025;
        let neck = q.edges.iter().position(|e| e == "neck-bottom").unwrap();
        q.points[neck].y += 0.25;
        q.points[(neck + 1) % n].y += 0.25;
        let found = pattern_checks(&odd, None);
        assert!(found.iter().any(|f| f.contains("side seam")), "{found:?}");
        assert!(found.iter().any(|f| f.contains("neckline")), "{found:?}");
    }
}
