//! What the agent tools read from a garment and what they change in it, worked out from the
//! document alone so it can be tested without a window. The window adds what only it knows
//! (the selection, the avatar, the running simulation) and applies every [`Plan`] through its
//! undo history, as a menu command would.
//!
//! Lengths an agent reads or gives are centimetres; the document keeps metres. Pattern
//! coordinates are the board's: x to the right, y downward, from the piece's own origin.
use crate::authoring::{self, Command, arrange2d::SegmentEnd};
use crate::garment::{
    CompiledGarment, GarmentTemplate,
    pleats::PleatKind,
    polygon::PolygonPattern,
    seam::{BoundaryRef, Side},
    shapes::MirrorAxis,
    template::Panel,
};
use glam::{DVec2, DVec3};
use serde_json::{Value, json};

/// A seam counts as closed after a simulation when its widest gap is below this, metres. A
/// seam is a spring, so a strained corner stays a few millimetres open (see
/// `DEFAULT_SEAM_STIFFNESS`).
pub const CLOSED_GAP_M: f64 = 0.006;
/// Two sides of a seam whose lengths differ by more than this, metres, without ease allowed,
/// are reported: the longer one gathers or the seam pulls.
pub const LENGTH_MISMATCH_M: f64 = 0.005;

/// Metres as centimetres, to the millimetre.
pub fn cm(m: f64) -> f64 {
    (m * 1000.).round() / 10.
}

/// The simulated garment, when the window has one: its compiled seams and where the cloth is.
pub struct Live<'a> {
    pub garment: &'a CompiledGarment,
    pub x: &'a [DVec3],
    /// The cloth has moved since it was laid out, so seam gaps mean something.
    pub simulated: bool,
}

/// A change an agent asked for: the command that makes it, and what to tell the agent once it
/// is done.
pub struct Plan {
    pub command: Command,
    pub done: String,
    /// The history's name for it, when the command's own name would mislead (a whole-document
    /// replacement used for a small change).
    pub history_name: Option<&'static str>,
}

impl Plan {
    fn new(command: Command, done: impl Into<String>) -> Self {
        Self {
            command,
            done: done.into(),
            history_name: None,
        }
    }
}

// ----- Reading arguments -------------------------------------------------------------------

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("'{key}' is missing"))
}
fn number(args: &Value, key: &str) -> Result<f64, String> {
    let v = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("'{key}' is missing or not a number"))?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("'{key}' is not finite"))
    }
}
fn optional(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => number(args, key).map(Some),
    }
}

/// The piece an agent named, by id or by name (any case).
pub fn find_piece<'a>(template: &'a GarmentTemplate, key: &str) -> Result<&'a Panel, String> {
    let key = key.trim();
    template
        .panels
        .iter()
        .find(|p| p.id == key)
        .or_else(|| {
            template
                .panels
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(key))
        })
        .ok_or_else(|| {
            let ids: Vec<&str> = template.panels.iter().map(|p| p.id.as_str()).collect();
            format!("No piece '{key}'. The pieces are: {}", ids.join(", "))
        })
}

fn polygon_of(panel: &Panel) -> Result<&PolygonPattern, String> {
    panel.polygon.as_ref().ok_or_else(|| {
        format!(
            "Piece '{}' is a legacy rectangle without an editable outline",
            panel.id
        )
    })
}

fn segment_index(panel: &Panel, segment: &str) -> Result<usize, String> {
    let polygon = polygon_of(panel)?;
    polygon
        .edges
        .iter()
        .position(|e| e == segment)
        .ok_or_else(|| {
            format!(
                "Piece '{}' has no segment '{segment}'. Its segments are: {}",
                panel.id,
                polygon.edges.join(", ")
            )
        })
}

// ----- Measuring -----------------------------------------------------------------------------

/// Length of a whole segment, metres: an outline segment or internal line of a drafted piece,
/// or a side of a legacy rectangle.
pub fn segment_length(panel: &Panel, segment: Option<&str>, side: Side) -> Option<f64> {
    match (&panel.polygon, segment) {
        (Some(polygon), Some(edge)) => authoring::edge_length(polygon, edge).ok().or_else(|| {
            // An internal line (a patch pocket is sewn to one).
            let line = polygon.lines.iter().find(|l| l.id == edge)?;
            Some(line_length(
                &line.samples().unwrap_or_else(|_| line.points.clone()),
            ))
        }),
        _ => Some(match side {
            Side::Top | Side::Bottom => panel.pattern.width,
            Side::Left | Side::Right => panel.pattern.height,
        }),
    }
}

/// Length of the stretch of a boundary a seam uses, metres.
fn reference_length(template: &GarmentTemplate, r: &BoundaryRef) -> Option<f64> {
    let panel = template.panels.iter().find(|p| p.id == r.panel)?;
    Some(segment_length(panel, r.edge.as_deref(), r.side)? * (r.range[1] - r.range[0]).abs())
}

/// Where a segment faces on the board: top, bottom, left or right of its piece.
fn facing(polygon: &PolygonPattern, i: usize) -> &'static str {
    let n = polygon.points.len();
    let (a, b) = (polygon.points[i], polygon.points[(i + 1) % n]);
    let d = b - a;
    // Outlines turn counterclockwise in (u, v), so the outward normal is (dy, -dx); v is drawn
    // downward, so a negative v points to the top of the board.
    let out = DVec2::new(d.y, -d.x);
    if out.y.abs() >= out.x.abs() {
        if out.y < 0. { "top" } else { "bottom" }
    } else if out.x > 0. {
        "right"
    } else {
        "left"
    }
}

fn bounds(points: &[DVec2]) -> (DVec2, DVec2) {
    points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

/// Where a new piece can go on the board: to the right of every piece, level with the
/// highest. New pieces start in 3D where they lie on the board, so two placed alike would
/// start on top of each other.
pub fn free_board_spot(template: &GarmentTemplate) -> DVec2 {
    let (mut right, mut top) = (f64::NEG_INFINITY, f64::INFINITY);
    for (i, panel) in template.panels.iter().enumerate() {
        let origin = authoring::layout(template, i);
        right = right.max(origin.x + piece_size(panel).x);
        top = top.min(origin.y);
    }
    if right.is_finite() {
        DVec2::new(right + 0.1, top)
    } else {
        DVec2::ZERO
    }
}

/// Width and height of a piece on the board, metres.
pub fn piece_size(panel: &Panel) -> DVec2 {
    match &panel.polygon {
        Some(polygon) if !polygon.points.is_empty() => {
            let (lo, hi) = bounds(&polygon.points);
            hi - lo
        }
        _ => DVec2::new(panel.pattern.width, panel.pattern.height),
    }
}

fn piece_area(panel: &Panel) -> f64 {
    match &panel.polygon {
        Some(polygon) => {
            let p = &polygon.points;
            (0..p.len())
                .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
                .sum::<f64>()
                .abs()
                / 2.
        }
        None => panel.pattern.width * panel.pattern.height,
    }
}

/// The seams that use a segment of a piece.
fn seams_on<'a>(template: &'a GarmentTemplate, panel: &str, edge: &str) -> Vec<&'a str> {
    template
        .seams
        .iter()
        .filter(|s| {
            s.references()
                .any(|r| r.panel == panel && r.edge.as_deref() == Some(edge))
        })
        .map(|s| s.id.as_str())
        .collect()
}

fn line_length(points: &[DVec2]) -> f64 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn label(r: &BoundaryRef) -> String {
    let edge = r.edge.clone().unwrap_or_else(|| format!("{:?}", r.side));
    let whole = (r.range[0] - 0.).abs() < 1e-9 && (r.range[1] - 1.).abs() < 1e-9;
    if whole {
        format!("{}.{edge}", r.panel)
    } else {
        format!(
            "{}.{edge}({:.0}-{:.0}%)",
            r.panel,
            r.range[0] * 100.,
            r.range[1] * 100.
        )
    }
}

// ----- Perception ----------------------------------------------------------------------------

/// Every piece, with its size, its segments (id, length, which way it faces, the seam it is in)
/// and its internal lines.
pub fn describe_pieces(template: &GarmentTemplate) -> Vec<Value> {
    template
        .panels
        .iter()
        .map(|p| {
            let size = piece_size(p);
            let segments: Vec<Value> = match &p.polygon {
                Some(polygon) => polygon
                    .edges
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let seams = seams_on(template, &p.id, e);
                        json!({
                            "id": e,
                            "cm": authoring::edge_length(polygon, e).map(cm).unwrap_or(0.),
                            "faces": facing(polygon, i),
                            "seam": seams.first(),
                        })
                    })
                    .collect(),
                None => [Side::Top, Side::Right, Side::Bottom, Side::Left]
                    .into_iter()
                    .map(|side| {
                        json!({
                            "id": format!("{side:?}").to_lowercase(),
                            "cm": cm(segment_length(p, None, side).unwrap_or(0.)),
                        })
                    })
                    .collect(),
            };
            let lines: Vec<Value> = p
                .polygon
                .iter()
                .flat_map(|polygon| &polygon.lines)
                .map(|l| {
                    let samples = l.samples().unwrap_or_else(|_| l.points.clone());
                    json!({"id": l.id, "cm": cm(line_length(&samples)), "closed": l.closed})
                })
                .collect();
            let t = p.placement.translation;
            let mut state = vec![];
            if template.deactivated_panels.iter().any(|d| d.panel == p.id) {
                state.push("deactivated");
            }
            if template.frozen_panels.iter().any(|f| f.panel == p.id) {
                state.push("frozen");
            }
            if template.strengthened_panels.contains(&p.id) {
                state.push("strengthened");
            }
            json!({
                "id": p.id,
                "name": p.name,
                "fabric": p.fabric,
                "size_cm": [cm(size.x), cm(size.y)],
                "area_cm2": (piece_area(p) * 1e4).round(),
                "grain_degrees": p.grain_degrees,
                "segments": segments,
                "internal_lines": lines,
                "placed_at_cm": [cm(t.x), cm(t.y), cm(t.z)],
                "wrapped_around_body": template.panel_wraps.contains_key(&p.id),
                "layer": template.panel_layers.get(&p.id).copied().unwrap_or(0),
                "state": state,
            })
        })
        .collect()
}

/// Every seam: what it joins, the length of each side, and after a simulation how far apart
/// its two sides still are.
pub fn describe_seams(template: &GarmentTemplate, live: Option<&Live>) -> Vec<Value> {
    template
        .seams
        .iter()
        .map(|s| {
            let side = |b: bool| -> (Vec<String>, f64) {
                let refs: Vec<&BoundaryRef> = s.side(b).collect();
                (
                    refs.iter().map(|r| label(r)).collect(),
                    refs.iter()
                        .filter_map(|r| reference_length(template, r))
                        .sum(),
                )
            };
            let ((a, la), (b, lb)) = (side(false), side(true));
            let mut seam = json!({
                "id": s.id,
                "a": a,
                "b": b,
                "a_cm": cm(la),
                "b_cm": cm(lb),
                "difference_cm": cm(la - lb),
                "ease_allowed": s.allow_ease,
                "reversed": s.reverse_b,
                "active": !template.inactive_seams.contains(&s.id),
            });
            if let Some(live) = live.filter(|l| l.simulated)
                && let Some(compiled) = live.garment.seams.iter().find(|c| c.id == s.id)
                && compiled.validate(live.x.len()).is_ok()
            {
                let gap = compiled.max_gap(live.x);
                seam["gap_mm"] = json!((gap * 1000.).round());
                seam["closed"] = json!(gap < CLOSED_GAP_M);
            }
            seam
        })
        .collect()
}

/// Every fabric with its physical parameters, in the units a fabric sheet uses.
pub fn describe_fabrics(template: &GarmentTemplate) -> Vec<Value> {
    template
        .fabrics
        .iter()
        .map(|f| {
            let p = &f.properties;
            let colour = template.fabric_appearance.get(&f.id).map(|a| a.color_srgb);
            let users: Vec<&str> = template
                .panels
                .iter()
                .filter(|panel| panel.fabric == f.id)
                .map(|panel| panel.id.as_str())
                .collect();
            json!({
                "id": f.id,
                "name": f.name,
                "rgb": colour,
                "weight_gsm": (p.density * 1e4).round() / 10.,
                "thickness_mm": (p.thickness * 1e4).round() / 10.,
                "stretch_percent_at_5N_per_5cm": [
                    (crate::fabrics::extension_percent(p.warp, p) * 10.).round() / 10.,
                    (crate::fabrics::extension_percent(p.weft, p) * 10.).round() / 10.,
                ],
                "shear_n_per_m": p.shear,
                "bending_n_m": [p.bend_warp, p.bend_weft],
                "friction": p.friction,
                "pieces": users,
            })
        })
        .collect()
}

/// Size groups, their sizes, the base size and the size on show.
pub fn describe_grading(template: &GarmentTemplate) -> Value {
    let Some(g) = &template.grading else {
        return json!({"graded": false});
    };
    json!({
        "graded": true,
        "groups": g.groups.iter().map(|group| json!({
            "id": group.id,
            "name": group.name,
            "sizes": group.sizes.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            "base": group.base_name(),
            "shown": group.name_of(group.active()),
        })).collect::<Vec<_>>(),
        "points_of_measure": g.measurements.len(),
    })
}

/// What looks wrong in the garment: compile warnings, seams whose sides differ, open seams
/// after a simulation, missed target lengths and pieces sewn to nothing.
pub fn warnings(template: &GarmentTemplate, live: Option<&Live>) -> Vec<String> {
    let mut out: Vec<String> = live.map_or_else(Vec::new, |l| l.garment.warnings.clone());
    for s in &template.seams {
        let length = |b: bool| -> f64 {
            s.side(b)
                .filter_map(|r| reference_length(template, r))
                .sum()
        };
        let (a, b) = (length(false), length(true));
        if !s.allow_ease && (a - b).abs() > LENGTH_MISMATCH_M {
            out.push(format!(
                "Seam {} joins {:.1} cm to {:.1} cm without ease: match the lengths or allow ease",
                s.id,
                cm(a),
                cm(b)
            ));
        }
    }
    for seam in describe_seams(template, live) {
        if seam["closed"] == json!(false) {
            out.push(format!(
                "Seam {} is still {} mm open after simulating",
                seam["id"].as_str().unwrap_or(""),
                seam["gap_mm"]
            ));
        }
    }
    for p in &template.panels {
        if let Some(polygon) = &p.polygon {
            out.extend(
                polygon
                    .target_diagnostics()
                    .into_iter()
                    .map(|d| format!("{}: {d}", p.id)),
            );
        }
        if template.panels.len() > 1
            && !template
                .seams
                .iter()
                .any(|s| s.references().any(|r| r.panel == p.id))
        {
            out.push(format!("Piece {} is not sewn to anything", p.id));
        }
    }
    out
}

/// One piece in full: its points, its segments with lengths and markings, its internal lines
/// and where it is placed.
pub fn piece_detail(template: &GarmentTemplate, key: &str) -> Result<Value, String> {
    let p = find_piece(template, key)?;
    let size = piece_size(p);
    let mut detail = json!({
        "id": p.id,
        "name": p.name,
        "fabric": p.fabric,
        "grain_degrees": p.grain_degrees,
        "size_cm": [cm(size.x), cm(size.y)],
        "coordinates": "cm on the 2D board, x to the right, y downward, from the piece's origin",
        "placement": {
            "position_cm": [cm(p.placement.translation.x), cm(p.placement.translation.y), cm(p.placement.translation.z)],
            "rotation_degrees": [p.placement.rotation_degrees.x, p.placement.rotation_degrees.y, p.placement.rotation_degrees.z],
            "wrapped_around_body": template.panel_wraps.contains_key(&p.id),
        },
        "mirrored_with": template.mirror_links.iter().find_map(|l| {
            if l.a == p.id { Some(l.b.clone()) } else if l.b == p.id { Some(l.a.clone()) } else { None }
        }),
    });
    let Some(polygon) = &p.polygon else {
        detail["segments"] = json!(["top", "right", "bottom", "left"]);
        detail["note"] = json!("A legacy rectangle: its outline cannot be edited point by point");
        return Ok(detail);
    };
    let n = polygon.points.len();
    detail["points"] = polygon
        .points
        .iter()
        .enumerate()
        .map(|(i, q)| json!({"index": i, "x": cm(q.x), "y": cm(q.y)}))
        .collect();
    detail["segments"] = polygon
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let length = authoring::edge_length(polygon, e).unwrap_or(0.);
            let notches: Vec<f64> = polygon
                .notches
                .get(e)
                .map(|v| v.iter().map(|n| cm(n.at * length)).collect())
                .unwrap_or_default();
            json!({
                "id": e,
                "from_point": i,
                "to_point": (i + 1) % n,
                "cm": cm(length),
                "faces": facing(polygon, i),
                "curved": polygon.curves.contains_key(e),
                "target_cm": polygon.target_lengths.get(e).map(|t| cm(*t)),
                "seam_allowance_cm": polygon.seam_allowances.get(e).map(|w| cm(*w)),
                "notches_at_cm": notches,
                "seams": seams_on(template, &p.id, e),
            })
        })
        .collect();
    detail["internal_lines"] = polygon
        .lines
        .iter()
        .map(|l| {
            let samples = l.samples().unwrap_or_else(|_| l.points.clone());
            json!({
                "id": l.id,
                "cm": cm(line_length(&samples)),
                "points": l.points.iter().map(|q| [cm(q.x), cm(q.y)]).collect::<Vec<_>>(),
                "closed": l.closed,
                "group": l.group,
            })
        })
        .collect();
    Ok(detail)
}

// ----- Changes -------------------------------------------------------------------------------

/// Turns an editing tool's arguments into the command that makes the change. Nothing is
/// changed here: the window applies the plan through its history.
pub fn plan(template: &GarmentTemplate, tool: &str, args: &Value) -> Result<Plan, String> {
    match tool {
        "move_points" => move_points(template, args),
        "set_segment_length" => set_segment_length(template, args),
        "add_point" => add_point(template, args),
        "add_dart" => add_dart(template, args),
        "add_pleats" => add_pleats(template, args),
        "create_piece" => create_piece(template, args),
        "piece_edit" => piece_edit(template, args),
        "sew" => sew(template, args),
        "seam_edit" => seam_edit(template, args),
        "set_fabric_parameters" => set_fabric_parameters(template, args),
        other => Err(format!("'{other}' is not an editing tool")),
    }
}

fn point_list(panel: &Panel, args: &Value) -> Result<Vec<usize>, String> {
    let n = polygon_of(panel)?.points.len();
    let points: Vec<usize> = args
        .get("points")
        .and_then(Value::as_array)
        .ok_or("'points' needs a list of point indexes (see piece_detail)")?
        .iter()
        .map(|v| {
            v.as_u64()
                .map(|i| i as usize)
                .filter(|i| *i < n)
                .ok_or_else(|| {
                    format!(
                        "Point {v} does not exist: the piece has points 0 to {}",
                        n - 1
                    )
                })
        })
        .collect::<Result<_, _>>()?;
    if points.is_empty() {
        return Err("Name at least one point".into());
    }
    Ok(points)
}

fn move_points(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let points = point_list(panel, args)?;
    let delta = DVec2::new(
        optional(args, "dx_cm")?.unwrap_or(0.),
        optional(args, "dy_cm")?.unwrap_or(0.),
    ) / 100.;
    if delta == DVec2::ZERO {
        return Err("Give dx_cm and/or dy_cm (x to the right, y downward)".into());
    }
    if delta.length() > 2. {
        return Err("A move of more than 2 m is surely a mistake".into());
    }
    Ok(Plan::new(
        Command::MovePoints {
            panel: panel.id.clone(),
            points: points.clone(),
            delta,
        },
        format!(
            "Moved point(s) {points:?} of {} by ({:.1}, {:.1}) cm",
            panel.id,
            delta.x * 100.,
            delta.y * 100.
        ),
    ))
}

fn set_segment_length(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let segment = text(args, "segment")?;
    segment_index(panel, segment)?;
    let length = number(args, "length_cm")?;
    let moving = match args.get("keep").and_then(Value::as_str).unwrap_or("start") {
        "start" => SegmentEnd::End,
        "end" => SegmentEnd::Start,
        "middle" | "centre" | "center" => SegmentEnd::Center,
        other => return Err(format!("keep '{other}': start, end or middle")),
    };
    let before = authoring::edge_length(polygon_of(panel)?, segment)?;
    Ok(Plan::new(
        Command::SetLength {
            panel: panel.id.clone(),
            edge: segment.into(),
            length_m: length / 100.,
            moving,
        },
        format!(
            "{}.{segment}: {:.1} cm → {length:.1} cm",
            panel.id,
            cm(before)
        ),
    ))
}

fn add_point(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let segment = text(args, "segment")?;
    segment_index(panel, segment)?;
    let length = authoring::edge_length(polygon_of(panel)?, segment)?;
    let at = optional(args, "at_cm")?.map_or(length / 2., |c| c / 100.);
    if at <= 0. || at >= length {
        return Err(format!(
            "at_cm must lie inside the segment, which is {:.1} cm long",
            cm(length)
        ));
    }
    Ok(Plan::new(
        Command::SplitBoundary {
            panel: panel.id.clone(),
            edge: segment.into(),
            at_m: at,
        },
        format!(
            "Added a point on {}.{segment} at {:.1} cm from its start",
            panel.id,
            cm(at)
        ),
    ))
}

fn add_dart(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let width = number(args, "width_cm")? / 100.;
    let depth = number(args, "depth_cm")? / 100.;
    if !(0.002..=0.3).contains(&width) || !(0.01..=0.6).contains(&depth) {
        return Err("A dart is 0.2 to 30 cm wide and 1 to 60 cm deep".into());
    }
    if args.get("segment").is_none() {
        // A diamond dart inside the piece, as on a dress's waist.
        let centre = args
            .get("centre_cm")
            .and_then(Value::as_array)
            .and_then(|v| Some(DVec2::new(v.first()?.as_f64()?, v.get(1)?.as_f64()?)))
            .ok_or("Give a segment for an edge dart, or centre_cm [x, y] for a diamond dart")?
            / 100.;
        return Ok(Plan::new(
            Command::DiamondDart {
                panel: panel.id.clone(),
                center: centre,
                width,
                height: depth,
            },
            format!(
                "Diamond dart on {}: {:.1} cm wide, {:.1} cm tall",
                panel.id,
                width * 100.,
                depth * 100.
            ),
        ));
    }
    let segment = text(args, "segment")?;
    let i = segment_index(panel, segment)?;
    let polygon = polygon_of(panel)?;
    let (a, b) = (
        polygon.points[i],
        polygon.points[(i + 1) % polygon.points.len()],
    );
    let length = a.distance(b);
    let at = optional(args, "at_cm")?.map_or(length / 2., |c| c / 100.);
    let (left, right) = ((at - width / 2.) / length, (at + width / 2.) / length);
    if left <= 0.02 || right >= 0.98 {
        return Err(format!(
            "The dart does not fit: {}.{segment} is {:.1} cm long; keep it away from the ends",
            panel.id,
            cm(length)
        ));
    }
    let along = (b - a) / length;
    // Outward is (dy, -dx) on a counterclockwise outline; the apex goes the other way.
    let inward = DVec2::new(-along.y, along.x);
    let apex = a + along * at + inward * depth;
    Ok(Plan::new(
        Command::DartCutSew {
            panel: panel.id.clone(),
            edge: segment.into(),
            left_t: left,
            right_t: right,
            apex,
        },
        format!(
            "Dart on {}.{segment}: {:.1} cm wide at {:.1} cm, {:.1} cm deep, cut and sewn",
            panel.id,
            width * 100.,
            at * 100.,
            depth * 100.
        ),
    ))
}

fn add_pleats(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let segment = text(args, "segment")?;
    segment_index(panel, segment)?;
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
    let depth = number(args, "depth_cm")? / 100.;
    let kind = match args.get("kind").and_then(Value::as_str).unwrap_or("knife") {
        "knife" => PleatKind::Knife,
        "box" => PleatKind::Box,
        "inverted_box" => PleatKind::InvertedBox,
        other => return Err(format!("kind '{other}': knife, box or inverted_box")),
    };
    let tack = args.get("tack").and_then(Value::as_bool).unwrap_or(true);
    Ok(Plan::new(
        Command::AddPleats {
            panel: panel.id.clone(),
            edge: segment.into(),
            count,
            depth_m: depth,
            kind,
            tack,
        },
        format!(
            "{count} {kind:?} pleat(s), {:.1} cm deep, along {}.{segment}{}",
            depth * 100.,
            panel.id,
            if tack { ", tacked at the edge" } else { "" }
        ),
    ))
}

fn create_piece(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let points: Vec<DVec2> = if let Some(size) = args.get("rectangle_cm") {
        let w = size.get(0).and_then(Value::as_f64).unwrap_or(0.) / 100.;
        let h = size.get(1).and_then(Value::as_f64).unwrap_or(0.) / 100.;
        if !(0.01..=5.).contains(&w) || !(0.01..=5.).contains(&h) {
            return Err("rectangle_cm is [width, height], each 1 to 500 cm".into());
        }
        vec![
            DVec2::ZERO,
            DVec2::new(w, 0.),
            DVec2::new(w, h),
            DVec2::new(0., h),
        ]
    } else {
        args.get("points_cm")
            .and_then(Value::as_array)
            .ok_or("Give rectangle_cm [width, height] or points_cm [[x, y], …]")?
            .iter()
            .map(|p| {
                let xy = || Some(DVec2::new(p.get(0)?.as_f64()?, p.get(1)?.as_f64()?) / 100.);
                xy().filter(|p| p.is_finite())
                    .ok_or_else(|| "Each point is [x, y] in cm".to_string())
            })
            .collect::<Result<_, _>>()?
    };
    let fabric = match args.get("fabric").and_then(Value::as_str) {
        Some(id) => {
            if !template.fabrics.iter().any(|f| f.id == id) {
                return Err(format!("No fabric '{id}': see scene_summary"));
            }
            id.to_owned()
        }
        None => template
            .panels
            .first()
            .map(|p| p.fabric.clone())
            .or_else(|| template.fabrics.first().map(|f| f.id.clone()))
            .ok_or("The garment has no fabric yet")?,
    };
    let meshing = template
        .panels
        .iter()
        .find_map(|p| p.polygon.as_ref().map(|q| q.meshing_settings()))
        .unwrap_or_default();
    // Beside the other pieces on the board (and so in 3D), never on top of one.
    let lo = points
        .iter()
        .copied()
        .fold(DVec2::splat(f64::INFINITY), DVec2::min);
    let spot = free_board_spot(template);
    let command = Command::Polygon {
        points: points.iter().map(|p| *p - lo + spot).collect(),
        tangents: vec![],
        meshing,
        fabric,
        replace: false,
    };
    // Apply it here to learn the new piece's id and give it its name in the same step.
    let mut edit = authoring::apply(template, command)?;
    let id = edit
        .template
        .panels
        .iter()
        .map(|p| p.id.clone())
        .find(|id| !template.panels.iter().any(|p| &p.id == id))
        .ok_or("The piece was not created")?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(&id)
        .to_owned();
    if let Some(p) = edit.template.panels.iter_mut().find(|p| p.id == id) {
        p.name = name.clone();
    }
    let segments = edit
        .template
        .panels
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| p.polygon.as_ref())
        .map(|q| q.edges.join(", "))
        .unwrap_or_default();
    Ok(Plan {
        command: Command::RunScript {
            template: Box::new(edit.template),
        },
        done: format!(
            "Created piece {id} ('{name}') with {} points; its segments are {segments}. It lies \
             beside the body: place it with place_piece, then sew it.",
            points.len()
        ),
        history_name: Some("Create piece"),
    })
}

fn piece_edit(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let id = panel.id.clone();
    let operation = text(args, "operation")?;
    Ok(match operation {
        "rename" => {
            let name = text(args, "name")?.to_owned();
            let mut t = template.clone();
            if let Some(p) = t.panels.iter_mut().find(|p| p.id == id) {
                p.name = name.clone();
            }
            Plan {
                command: Command::RunScript {
                    template: Box::new(t),
                },
                done: format!("Renamed {id} to '{name}'"),
                history_name: Some("Rename piece"),
            }
        }
        "duplicate" => Plan::new(
            Command::Duplicate { panel: id.clone() },
            format!("Duplicated {id}"),
        ),
        "mirror_copy" => Plan::new(
            Command::LinkedMirror {
                panel: id.clone(),
                axis: MirrorAxis::X,
            },
            format!("Made a mirrored copy of {id}, linked to it: edits of one show on the other"),
        ),
        "delete" => Plan::new(
            Command::RemovePanels {
                panels: vec![id.clone()],
            },
            format!("Deleted {id} and its seams"),
        ),
        "set_grain" => {
            let degrees = number(args, "degrees")?;
            Plan::new(
                Command::SetPanelGrain {
                    panel: id.clone(),
                    degrees,
                },
                format!("Grain of {id} set to {degrees}°"),
            )
        }
        "set_seam_allowance" => {
            let segment = text(args, "segment")?;
            segment_index(panel, segment)?;
            let width = number(args, "width_cm")?;
            Plan::new(
                Command::SeamAllowance {
                    panel: id.clone(),
                    edge: segment.into(),
                    width_m: width / 100.,
                },
                format!("Seam allowance of {id}.{segment}: {width} cm"),
            )
        }
        other => {
            return Err(format!(
                "operation '{other}': rename, duplicate, mirror_copy, delete, set_grain or \
                 set_seam_allowance"
            ));
        }
    })
}

/// One side of a seam as an agent gives it: segments in order, each whole or a stretch of it.
fn seam_side(
    template: &GarmentTemplate,
    side: &Value,
    name: &str,
) -> Result<Vec<BoundaryRef>, String> {
    let items: Vec<&Value> = match side {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![side],
        _ => return Err(format!("'{name}' is a list of {{piece, segment}}")),
    };
    if items.is_empty() {
        return Err(format!("'{name}' names no segment"));
    }
    items
        .into_iter()
        .map(|item| {
            let panel = find_piece(template, text(item, "piece")?)?;
            let segment = text(item, "segment")?;
            segment_index(panel, segment)?;
            if let Some(seam) = seams_on(template, &panel.id, segment).first() {
                return Err(format!(
                    "{}.{segment} is already sewn in seam {seam}: remove_seam it first, or sew \
                     only a free stretch with from_cm/to_cm",
                    panel.id
                ));
            }
            let length = authoring::edge_length(polygon_of(panel)?, segment)?;
            let from = optional(item, "from_cm")?.map_or(0., |c| c / 100. / length);
            let to = optional(item, "to_cm")?.map_or(1., |c| c / 100. / length);
            if !(0. ..1.).contains(&from) || !(0. ..=1.).contains(&to) || to - from < 0.01 {
                return Err(format!(
                    "from_cm/to_cm must lie within {}.{segment}, which is {:.1} cm long",
                    panel.id,
                    cm(length)
                ));
            }
            Ok(BoundaryRef {
                reversed: false,
                panel: panel.id.clone(),
                side: Side::Top,
                edge: Some(segment.into()),
                range: [from, to],
            })
        })
        .collect()
}

/// Where a boundary's start and end are in the world, as the pieces are placed now.
fn ends_in_world(template: &GarmentTemplate, refs: &[BoundaryRef]) -> Option<[DVec3; 2]> {
    let point = |r: &BoundaryRef, t: f64| -> Option<DVec3> {
        let panel = template.panels.iter().find(|p| p.id == r.panel)?;
        let polygon = panel.polygon.as_ref()?;
        let i = polygon
            .edges
            .iter()
            .position(|e| Some(e) == r.edge.as_ref())?;
        let (a, b) = (
            polygon.points[i],
            polygon.points[(i + 1) % polygon.points.len()],
        );
        let q = a.lerp(b, t);
        let mut local = DVec3::new(q.x, -q.y, 0.);
        if let Some(wrap) = template.panel_wraps.get(&panel.id) {
            local = wrap.bend(local);
        }
        Some(panel.placement.transform(local))
    };
    let (first, last) = (refs.first()?, refs.last()?);
    Some([point(first, first.range[0])?, point(last, last.range[1])?])
}

fn sew(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let a = seam_side(template, args.get("a").ok_or("'a' is missing")?, "a")?;
    let b = seam_side(template, args.get("b").ok_or("'b' is missing")?, "b")?;
    let ease = args.get("ease").and_then(Value::as_bool).unwrap_or(false);
    // Unless told, sew so that the nearer ends meet, as the pieces are placed now: a seam
    // sewn the wrong way round twists the cloth.
    let (reverse, how) = match args.get("reverse").and_then(Value::as_bool) {
        Some(r) => (r, "as asked"),
        None => match (ends_in_world(template, &a), ends_in_world(template, &b)) {
            (Some([a0, a1]), Some([b0, b1])) => (
                a0.distance(b0) + a1.distance(b1) > a0.distance(b1) + a1.distance(b0),
                "chosen from where the pieces are",
            ),
            _ => (false, "start to start"),
        },
    };
    let length = |refs: &[BoundaryRef]| -> f64 {
        refs.iter()
            .filter_map(|r| reference_length(template, r))
            .sum()
    };
    let (la, lb) = (length(&a), length(&b));
    let names = |refs: &[BoundaryRef]| refs.iter().map(label).collect::<Vec<_>>().join(" + ");
    let mut done = format!(
        "Sewed {} ({:.1} cm) to {} ({:.1} cm), {}{}",
        names(&a),
        cm(la),
        names(&b),
        cm(lb),
        if reverse { "reversed" } else { "not reversed" },
        format_args!(" ({how})"),
    );
    if !ease && (la - lb).abs() > LENGTH_MISMATCH_M {
        done.push_str(&format!(
            ". Warning: the sides differ by {:.1} cm without ease; the seam will pull",
            cm((la - lb).abs())
        ));
    }
    let command = if a.len() == 1 && b.len() == 1 {
        Command::Sew {
            a: a.into_iter().next().unwrap_or_else(|| unreachable!()),
            b: b.into_iter().next().unwrap_or_else(|| unreachable!()),
            reverse,
            ease,
        }
    } else {
        Command::SewChain {
            seam: None,
            a,
            b,
            reverse,
            ease,
        }
    };
    Ok(Plan::new(command, done))
}

fn seam_edit(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let id = text(args, "seam")?;
    let seam = template
        .seams
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("No seam '{id}': see scene_summary"))?;
    let ease = match text(args, "operation")? {
        "remove" => {
            return Ok(Plan::new(
                Command::RemoveSeam { seam: id.into() },
                format!("Removed seam {id}"),
            ));
        }
        "allow_ease" => true,
        "forbid_ease" => false,
        other => {
            return Err(format!(
                "operation '{other}': remove, allow_ease or forbid_ease"
            ));
        }
    };
    // Sewing the same sides again under the same id keeps the seam's stiffness and notches.
    Ok(Plan::new(
        Command::SewChain {
            seam: Some(id.into()),
            a: seam.side(false).cloned().collect(),
            b: seam.side(true).cloned().collect(),
            reverse: seam.reverse_b,
            ease,
        },
        if ease {
            format!("Seam {id} may now join unequal lengths: the longer side is eased in")
        } else {
            format!("Seam {id} now needs equal lengths")
        },
    ))
}

fn set_fabric_parameters(template: &GarmentTemplate, args: &Value) -> Result<Plan, String> {
    let fabric = match (
        args.get("fabric").and_then(Value::as_str),
        args.get("piece").and_then(Value::as_str),
    ) {
        (Some(id), _) => template
            .fabrics
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| format!("No fabric '{id}': see scene_summary"))?,
        (None, Some(piece)) => {
            let panel = find_piece(template, piece)?;
            template
                .fabrics
                .iter()
                .find(|f| f.id == panel.fabric)
                .ok_or("The piece's fabric is missing")?
        }
        (None, None) => return Err("Name the fabric (or a piece cut from it)".into()),
    };
    let mut p = fabric.properties.clone();
    let mut changed = vec![];
    if let Some(gsm) = optional(args, "weight_gsm")? {
        p.density = gsm / 1000.;
        changed.push(format!("weight {gsm} g/m²"));
    }
    if let Some(mm) = optional(args, "thickness_mm")? {
        p.thickness = mm / 1000.;
        changed.push(format!("thickness {mm} mm"));
    }
    // Stretch is set after weight and thickness, which the conversion depends on.
    for (key, warp) in [
        ("stretch_warp_percent", true),
        ("stretch_weft_percent", false),
    ] {
        if let Some(percent) = optional(args, key)? {
            let k = crate::fabrics::stiffness_for_extension(percent, &p)?;
            if warp {
                p.warp = k;
            } else {
                p.weft = k;
            }
            changed.push(format!("{key} {percent}%"));
        }
    }
    if let Some(v) = optional(args, "shear_n_per_m")? {
        p.shear = v;
        changed.push(format!("shear {v} N/m"));
    }
    if let Some(v) = optional(args, "bending_warp_n_m")? {
        p.bend_warp = v;
        changed.push(format!("bending (warp) {v} N·m"));
    }
    if let Some(v) = optional(args, "bending_weft_n_m")? {
        p.bend_weft = v;
        changed.push(format!("bending (weft) {v} N·m"));
    }
    if let Some(v) = optional(args, "friction")? {
        p.friction = Some(v);
        changed.push(format!("friction {v}"));
    }
    if changed.is_empty() {
        return Err("Give at least one parameter to change".into());
    }
    p.bind(0.).validate()?;
    let mut t = template.clone();
    let id = fabric.id.clone();
    if let Some(f) = t.fabrics.iter_mut().find(|f| f.id == id) {
        f.properties = p;
    }
    let users = template.panels.iter().filter(|q| q.fabric == id).count();
    Ok(Plan {
        command: Command::RunScript {
            template: Box::new(t),
        },
        done: format!(
            "Fabric {id} ({users} piece(s)): {}. These are model parameters, not a lab \
             measurement of a real fabric.",
            changed.join(", ")
        ),
        history_name: Some("Fabric parameters"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garment::presets::{TshirtMeasurements, tshirt};

    fn shirt() -> GarmentTemplate {
        tshirt(&TshirtMeasurements::default(), 1).unwrap()
    }
    fn applied(t: &GarmentTemplate, tool: &str, args: Value) -> GarmentTemplate {
        let plan = plan(t, tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"));
        authoring::apply(t, plan.command)
            .unwrap_or_else(|e| panic!("{tool} apply: {e}"))
            .template
    }
    fn length(t: &GarmentTemplate, piece: &str, edge: &str) -> f64 {
        let p = t.panels.iter().find(|p| p.id == piece).unwrap();
        authoring::edge_length(p.polygon.as_ref().unwrap(), edge).unwrap()
    }

    #[test]
    fn pieces_report_their_size_and_segment_lengths() {
        let t = shirt();
        let pieces = describe_pieces(&t);
        assert_eq!(pieces.len(), 2);
        let front = &pieces[0];
        let size = front["size_cm"].as_array().unwrap();
        assert!(size[0].as_f64().unwrap() > 40. && size[1].as_f64().unwrap() > 60.);
        let hem = front["segments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "hem")
            .unwrap();
        assert_eq!(hem["faces"], "bottom");
        assert!((hem["cm"].as_f64().unwrap() - cm(length(&t, "front", "hem"))).abs() < 1e-9);
        let neck = front["segments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "neck-bottom")
            .unwrap();
        assert_eq!(neck["faces"], "top");
        assert_eq!(neck["seam"], Value::Null);
    }

    #[test]
    fn seams_report_both_lengths_and_gaps_only_after_simulating() {
        let t = shirt();
        let garment = t.compile().unwrap();
        let x = garment.initial_state.x.clone();
        let seams = describe_seams(&t, None);
        assert!(!seams.is_empty());
        for s in &seams {
            assert!(s["a_cm"].as_f64().unwrap() > 0.);
            assert_eq!(s["a_cm"], s["b_cm"], "front and back share their outline");
            assert!(s.get("gap_mm").is_none());
        }
        let laid_out = Live {
            garment: &garment,
            x: &x,
            simulated: true,
        };
        let seams = describe_seams(&t, Some(&laid_out));
        // Laid out flat, front and back hang apart: every seam is open.
        assert!(seams.iter().all(|s| s["closed"] == json!(false)));
        assert!(
            warnings(&t, Some(&laid_out))
                .iter()
                .any(|w| w.contains("open after simulating"))
        );
    }

    #[test]
    fn fabrics_report_physical_parameters_in_sheet_units() {
        let mut t = shirt();
        let preset = &crate::fabrics::PRESETS[0];
        t.fabrics[0].properties = preset.properties();
        let f = &describe_fabrics(&t)[0];
        assert!((f["weight_gsm"].as_f64().unwrap() - preset.gsm).abs() < 0.11);
        assert!((f["thickness_mm"].as_f64().unwrap() - preset.thickness_mm).abs() < 0.051);
        let stretch = f["stretch_percent_at_5N_per_5cm"].as_array().unwrap();
        assert!((stretch[0].as_f64().unwrap() - preset.extension[0]).abs() < 0.051);
        assert_eq!(f["pieces"], json!(["front", "back"]));
    }

    #[test]
    fn piece_detail_lists_points_segments_and_seams() {
        let t = shirt();
        let d = piece_detail(&t, "nothing").unwrap_err();
        assert!(d.contains("front, back"), "{d}");
        let name = t.panels[0].name.clone();
        let d = piece_detail(&t, &name).unwrap();
        assert_eq!(d["id"], "front");
        let points = d["points"].as_array().unwrap();
        let segments = d["segments"].as_array().unwrap();
        assert_eq!(points.len(), segments.len());
        let shoulder = segments
            .iter()
            .find(|s| s["id"] == "shoulder-right")
            .unwrap();
        assert!(!shoulder["seams"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_segment_takes_the_length_asked_for() {
        let t = shirt();
        let after = applied(
            &t,
            "set_segment_length",
            json!({"piece": "front", "segment": "neck-bottom", "length_cm": 12.0, "keep": "middle"}),
        );
        assert!((length(&after, "front", "neck-bottom") - 0.12).abs() < 1e-6);
    }

    #[test]
    fn points_move_by_the_offset_asked_for() {
        let t = shirt();
        let points = |t: &GarmentTemplate| t.panels[0].polygon.as_ref().unwrap().points.clone();
        let before = points(&t);
        let after = applied(
            &t,
            "move_points",
            json!({"piece": "front", "points": [1], "dx_cm": 0.5, "dy_cm": 1.0}),
        );
        // The outline may be re-anchored at the origin: compare with a point that stayed.
        let moved = points(&after);
        let shift = (moved[1] - moved[0]) - (before[1] - before[0]);
        assert!((shift - DVec2::new(0.005, 0.01)).length() < 1e-9, "{shift}");
        assert!(
            plan(
                &t,
                "move_points",
                &json!({"piece": "front", "points": [99], "dx_cm": 1})
            )
            .is_err()
        );
    }

    #[test]
    fn a_point_splits_a_free_segment_and_sewn_ones_are_refused() {
        let t = shirt();
        let n = t.panels[0].polygon.as_ref().unwrap().points.len();
        let after = applied(
            &t,
            "add_point",
            json!({"piece": "front", "segment": "hem", "at_cm": 10}),
        );
        assert_eq!(
            after.panels[0].polygon.as_ref().unwrap().points.len(),
            n + 1
        );
        let sewn = plan(
            &t,
            "add_point",
            &json!({"piece": "front", "segment": "side-right"}),
        )
        .unwrap();
        assert!(authoring::apply(&t, sewn.command).is_err());
    }

    #[test]
    fn darts_and_pleats_change_the_outline() {
        let t = shirt();
        let n = t.panels[0].polygon.as_ref().unwrap().points.len();
        let dart = applied(
            &t,
            "add_dart",
            json!({"piece": "front", "segment": "hem", "width_cm": 3, "depth_cm": 10}),
        );
        let polygon = dart.panels[0].polygon.as_ref().unwrap();
        assert!(polygon.points.len() > n);
        // The apex points into the piece: above the hem.
        let hem_y = t.panels[0].polygon.as_ref().unwrap().points[8].y;
        assert!(polygon.points.iter().any(|p| p.y < hem_y - 0.09));
        assert!(dart.seams.len() > t.seams.len());
        let diamond = applied(
            &t,
            "add_dart",
            json!({"piece": "back", "centre_cm": [30, 40], "width_cm": 3, "depth_cm": 14}),
        );
        assert!(
            !diamond.panels[1].polygon.as_ref().unwrap().lines.is_empty()
                || diamond.panels[1].polygon.as_ref().unwrap().holes.len() + diamond.seams.len()
                    > t.seams.len()
        );
        let pleats = applied(
            &t,
            "add_pleats",
            json!({"piece": "front", "segment": "hem", "count": 3, "depth_cm": 2, "kind": "box"}),
        );
        assert!(!pleats.panels[0].polygon.as_ref().unwrap().lines.is_empty());
    }

    #[test]
    fn a_new_piece_is_named_and_can_be_sewn_with_m_to_n_segments() {
        let t = shirt();
        let with = applied(
            &t,
            "create_piece",
            json!({"name": "Pocket", "rectangle_cm": [14, 16]}),
        );
        let pocket = with.panels.iter().find(|p| p.name == "Pocket").unwrap();
        let size = piece_size(pocket);
        assert!((size - DVec2::new(0.14, 0.16)).length() < 1e-9);
        assert_eq!(describe_pieces(&with)[2]["segments"][0]["faces"], "top");
        // 1:1, then 2:1 on free segments.
        let one = applied(
            &with,
            "sew",
            json!({"a": {"piece": "Pocket", "segment": "edge_1"}, "b": [{"piece": "front", "segment": "hem", "from_cm": 5, "to_cm": 19}]}),
        );
        assert_eq!(one.seams.len(), t.seams.len() + 1);
        let chain = plan(
            &one,
            "sew",
            &json!({"a": [{"piece": "Pocket", "segment": "edge_2"}, {"piece": "Pocket", "segment": "edge_3"}],
                    "b": {"piece": "back", "segment": "hem"}, "ease": true}),
        )
        .unwrap();
        assert!(matches!(chain.command, Command::SewChain { .. }));
        let two = authoring::apply(&one, chain.command).unwrap().template;
        assert_eq!(two.seams.last().unwrap().a_tail.len(), 1);
        let again = plan(
            &two,
            "sew",
            &json!({"a": {"piece": "Pocket", "segment": "edge_1"}, "b": {"piece": "back", "segment": "neck-bottom"}}),
        );
        assert!(again.err().unwrap().contains("already sewn"));
        let seam = two.seams.last().unwrap().id.clone();
        let removed = applied(
            &two,
            "seam_edit",
            json!({"seam": seam, "operation": "remove"}),
        );
        assert_eq!(removed.seams.len(), two.seams.len() - 1);
    }

    #[test]
    fn sewing_follows_where_the_pieces_are_unless_told() {
        let t = shirt();
        let mut free = t.clone();
        free.seams.retain(|s| s.id != "side-right");
        let plan_auto = plan(
            &free,
            "sew",
            &json!({"a": {"piece": "front", "segment": "side-right"}, "b": {"piece": "back", "segment": "side-right"}}),
        )
        .unwrap();
        // Front and back are cut from one outline and hang face to face: start meets start.
        assert!(matches!(
            plan_auto.command,
            Command::Sew { reverse: false, .. }
        ));
        let forced = plan(
            &free,
            "sew",
            &json!({"a": {"piece": "front", "segment": "side-right"}, "b": {"piece": "back", "segment": "side-right"}, "reverse": true}),
        )
        .unwrap();
        assert!(matches!(forced.command, Command::Sew { reverse: true, .. }));
    }

    #[test]
    fn fabric_parameters_change_in_place_for_every_piece_of_that_fabric() {
        let t = shirt();
        let after = applied(
            &t,
            "set_fabric_parameters",
            json!({"piece": "front", "weight_gsm": 220, "thickness_mm": 0.6, "stretch_weft_percent": 12}),
        );
        assert_eq!(after.fabrics.len(), t.fabrics.len());
        let p = &after.fabrics[0].properties;
        assert!((p.density - 0.22).abs() < 1e-12);
        assert!((crate::fabrics::extension_percent(p.weft, p) - 12.).abs() < 0.05);
        assert!(after.panels.iter().all(|q| q.fabric == after.fabrics[0].id));
        assert!(plan(&t, "set_fabric_parameters", &json!({"piece": "front"})).is_err());
    }

    #[test]
    fn piece_operations_rename_duplicate_mirror_and_delete() {
        let t = shirt();
        let renamed = applied(
            &t,
            "piece_edit",
            json!({"piece": "front", "operation": "rename", "name": "Delantero"}),
        );
        assert_eq!(renamed.panels[0].name, "Delantero");
        let dup = applied(
            &t,
            "piece_edit",
            json!({"piece": "back", "operation": "duplicate"}),
        );
        assert_eq!(dup.panels.len(), 3);
        let gone = applied(
            &t,
            "piece_edit",
            json!({"piece": "back", "operation": "delete"}),
        );
        assert_eq!(gone.panels.len(), 1);
        assert!(gone.seams.is_empty());
        let grain = applied(
            &t,
            "piece_edit",
            json!({"piece": "back", "operation": "set_grain", "degrees": 45}),
        );
        assert_eq!(grain.panels[1].grain_degrees, 45.);
    }

    #[test]
    fn mismatched_seams_are_warned_about_and_ease_can_be_allowed() {
        let t = shirt();
        assert!(warnings(&t, None).is_empty(), "{:?}", warnings(&t, None));
        // The authoring layer refuses a sewn segment that no longer matches without ease…
        let longer = plan(
            &t,
            "set_segment_length",
            &json!({"piece": "front", "segment": "side-right", "length_cm": 60, "keep": "start"}),
        )
        .unwrap();
        assert!(authoring::apply(&t, longer.command).is_err());
        // …so allow ease on the seam first, then lengthen it: the warning names it.
        let eased = applied(
            &t,
            "seam_edit",
            json!({"seam": "side-right", "operation": "allow_ease"}),
        );
        assert!(
            eased
                .seams
                .iter()
                .any(|s| s.id == "side-right" && s.allow_ease)
        );
        let longer = applied(
            &eased,
            "set_segment_length",
            json!({"piece": "front", "segment": "side-right", "length_cm": 40, "keep": "start"}),
        );
        assert!(warnings(&longer, None).is_empty());
        let mut strict = longer.clone();
        for s in &mut strict.seams {
            s.allow_ease = false;
        }
        assert!(
            warnings(&strict, None)
                .iter()
                .any(|w| w.contains("without ease"))
        );
    }

    #[test]
    fn grading_is_summarised() {
        let mut t = shirt();
        assert_eq!(describe_grading(&t)["graded"], false);
        t.grading =
            Some(crate::garment::grading::Grading::new("Sizes", &["S", "M", "L"], 1).unwrap());
        let g = describe_grading(&t);
        assert_eq!(g["groups"][0]["sizes"], json!(["S", "M", "L"]));
        assert_eq!(g["groups"][0]["base"], "M");
    }
}
