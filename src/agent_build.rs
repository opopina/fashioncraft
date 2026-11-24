//! Exact engines for what an agent would otherwise do by placing raw coordinates or dragging
//! the mouse: the model says what it wants in centimetres and FashionCraft works out every
//! point, seam and 3D position. A patch pocket, a band collar and a placket from a few
//! numbers; a segment bent by its bulge; internal lines, buttons and buttonholes; pieces moved
//! or turned on the board or in 3D.
//!
//! Like `agent_ops`, everything here reads the document alone and returns the command that
//! makes the change, so it can be tested without a window. Several changes that belong
//! together become one undo step (the final document, applied as one `RunScript`).
use crate::agent_ops::{cm, find_piece};
use crate::authoring::{self, Command};
use crate::garment::{
    CompiledGarment, GarmentTemplate,
    seam::{BoundaryRef, Side},
    template::Panel,
    trims::{Trim, TrimKind},
};
use glam::{DQuat, DVec2, DVec3};
use serde_json::Value;

/// A change ready to apply, what to say once it is done, and where a new piece starts in 3D.
pub struct Build {
    pub command: Command,
    pub done: String,
    pub history_name: &'static str,
    /// The new piece and how to lay it on the drape, when the change adds one.
    pub lay: Option<Lay>,
}

/// How a new piece starts in 3D, on the garment as it is draped now.
#[derive(Clone, Debug)]
pub enum Lay {
    /// Flat on `host`: the new piece's material point `p` lies on the host's
    /// `offset + turn(p)`, where `turn` takes x to `along` (a unit vector) and y a quarter turn
    /// on from it.
    Over {
        piece: String,
        host: String,
        offset: DVec2,
        along: DVec2,
    },
    /// Standing up along a chain of outline segments, as a collar on a neckline: the new
    /// piece's x runs along the chain, its bottom edge (y = `height`) on it.
    Up {
        piece: String,
        chain: Vec<BoundaryRef>,
        /// Where each segment of the chain starts on its piece (material coordinates), so its
        /// cloth points can be put in the segment's own direction.
        starts: Vec<DVec2>,
        height: f64,
    },
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("'{key}' is missing"))
}
fn number(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("'{key}' is missing or not a number"))
}
fn optional(args: &Value, key: &str) -> Option<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}
/// `[x, y]` in cm, as metres.
fn pair(v: &Value) -> Option<DVec2> {
    Some(DVec2::new(v.get(0)?.as_f64()?, v.get(1)?.as_f64()?) / 100.)
}
fn triple(v: &Value) -> Option<DVec3> {
    Some(
        DVec3::new(
            v.get(0)?.as_f64()?,
            v.get(1)?.as_f64()?,
            v.get(2)?.as_f64()?,
        ) / 100.,
    )
}

fn segment(panel: &Panel, name: &str) -> Result<(usize, DVec2, DVec2), String> {
    let polygon = panel
        .polygon
        .as_ref()
        .ok_or_else(|| format!("Piece '{}' has no editable outline", panel.id))?;
    let i = polygon
        .edges
        .iter()
        .position(|e| e == name)
        .ok_or_else(|| {
            format!(
                "Piece '{}' has no segment '{name}'. Its segments are: {}",
                panel.id,
                polygon.edges.join(", ")
            )
        })?;
    let n = polygon.points.len();
    Ok((i, polygon.points[i], polygon.points[(i + 1) % n]))
}

/// Whether `p` is inside the closed outline `poly` (even-odd rule).
fn inside(poly: &[DVec2], p: DVec2) -> bool {
    let mut odd = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            odd = !odd;
        }
    }
    odd
}

fn whole(panel: &str, edge: &str, reversed: bool) -> BoundaryRef {
    BoundaryRef {
        reversed,
        panel: panel.to_owned(),
        side: Side::Top,
        edge: Some(edge.to_owned()),
        range: [0., 1.],
    }
}

/// Applies `commands` one after another to a copy of `template`; the result is one step.
fn chain(template: &GarmentTemplate, commands: Vec<Command>) -> Result<GarmentTemplate, String> {
    let mut t = template.clone();
    for c in commands {
        t = authoring::apply(&t, c)?.template;
    }
    Ok(t)
}

fn new_piece(before: &GarmentTemplate, after: &GarmentTemplate) -> Result<String, String> {
    after
        .panels
        .iter()
        .map(|p| p.id.clone())
        .find(|id| !before.panels.iter().any(|p| &p.id == id))
        .ok_or_else(|| "The piece was not created".into())
}

fn meshing(template: &GarmentTemplate) -> crate::garment::meshing::MeshingSettings {
    template
        .panels
        .iter()
        .find_map(|p| p.polygon.as_ref().map(|q| q.meshing_settings()))
        .unwrap_or_default()
}

/// A rectangle `w` × `h` (edge_1 top, edge_2 right, edge_3 bottom, edge_4 left), named, of
/// `fabric`, added to a copy of `template`. Returns the copy and the new piece's id.
fn add_rectangle(
    template: &GarmentTemplate,
    w: f64,
    h: f64,
    name: &str,
    fabric: &str,
) -> Result<(GarmentTemplate, String), String> {
    let at = crate::agent_ops::free_board_spot(template);
    let mut t = authoring::apply(
        template,
        Command::Polygon {
            points: vec![
                at,
                at + DVec2::new(w, 0.),
                at + DVec2::new(w, h),
                at + DVec2::new(0., h),
            ],
            tangents: vec![],
            meshing: meshing(template),
            fabric: fabric.to_owned(),
            replace: false,
        },
    )?
    .template;
    let id = new_piece(template, &t)?;
    if let Some(p) = t.panels.iter_mut().find(|p| p.id == id) {
        p.name = name.to_owned();
    }
    Ok((t, id))
}

/// Bend a segment so its middle moves `bulge_cm` outward (negative: inward), as dragging it
/// would: a smooth curve through both ends.
pub fn bend_segment(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let name = text(args, "segment")?;
    let (_, a, b) = segment(panel, name)?;
    let bulge = number(args, "bulge_cm")? / 100.;
    let length = a.distance(b);
    if bulge.abs() > length {
        return Err(format!(
            "A bulge of {:.1} cm on a {:.1} cm segment is surely a mistake",
            bulge * 100.,
            cm(length)
        ));
    }
    // Outlines turn counterclockwise, so (dy, -dx) points out of the piece. Both handles move
    // by d and the middle by 3/4 d.
    let d = (b - a) / length.max(1e-9);
    let outward = DVec2::new(d.y, -d.x);
    Ok(Build {
        command: Command::StyleLine {
            moves: vec![],
            bends: vec![(panel.id.clone(), name.to_owned(), outward * bulge * 4. / 3.)],
            scales: vec![],
        },
        done: format!(
            "Bent {}.{name}: its middle moved {:.1} cm {}. Seams along it take up the length \
             difference",
            panel.id,
            bulge.abs() * 100.,
            if bulge >= 0. { "outward" } else { "inward" }
        ),
        history_name: "Bend segment",
        lay: None,
    })
}

/// An internal line through points in cm (a style line, a placement line, a fold).
pub fn internal_line(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let points: Vec<DVec2> = args
        .get("points_cm")
        .and_then(Value::as_array)
        .ok_or("points_cm: [[x, y], …] in cm, as piece_detail gives them")?
        .iter()
        .map(|p| pair(p).ok_or("Each point is [x, y] in cm"))
        .collect::<Result<_, _>>()?;
    if points.len() < 2 {
        return Err("An internal line needs at least two points".into());
    }
    let before: Vec<String> = panel
        .polygon
        .as_ref()
        .map(|q| q.lines.iter().map(|l| l.id.clone()).collect())
        .unwrap_or_default();
    let command = Command::InternalLine {
        panel: panel.id.clone(),
        tangents: vec![DVec2::ZERO; points.len()],
        points: points.clone(),
    };
    let after = authoring::apply(template, command)?.template;
    let id = after
        .panels
        .iter()
        .find(|p| p.id == panel.id)
        .and_then(|p| p.polygon.as_ref())
        .and_then(|q| q.lines.iter().find(|l| !before.contains(&l.id)))
        .map(|l| l.id.clone())
        .unwrap_or_default();
    Ok(Build {
        command: Command::RunScript {
            template: Box::new(after),
        },
        done: format!(
            "Internal line {id} on {} through {} points; sew to it, fold it (edit SetLineFold) \
             or cut along it (command_run pattern.cut_and_sew)",
            panel.id,
            points.len()
        ),
        history_name: "Internal line",
        lay: None,
    })
}

/// Buttons or buttonholes on a piece, `count` of them `spacing_cm` apart from `at_cm`.
pub fn buttons(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let panel = find_piece(template, text(args, "piece")?)?;
    let start = args
        .get("at_cm")
        .and_then(pair)
        .ok_or("at_cm: [x, y] of the first one, in cm on the piece")?;
    let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
    if !(1..=40).contains(&count) {
        return Err("count is 1 to 40".into());
    }
    let step = args
        .get("spacing_cm")
        .and_then(pair)
        .unwrap_or(DVec2::new(0., 0.08));
    let hole = match args.get("kind").and_then(Value::as_str).unwrap_or("button") {
        "button" => false,
        "buttonhole" => true,
        other => return Err(format!("kind '{other}': button or buttonhole")),
    };
    let size = optional(args, "size_cm").map_or(if hole { 0.018 } else { 0.015 }, |s| s / 100.);
    let vertical = args
        .get("vertical")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut t = template.clone();
    let mut ids = Vec::new();
    for k in 0..count {
        let c = start + step * k as f64;
        let id = (1..)
            .map(|n| format!("trim_{n}"))
            .find(|id| !t.trims.iter().any(|x| &x.id == id))
            .unwrap_or_default();
        let half = if vertical {
            DVec2::new(0., size / 2.)
        } else {
            DVec2::new(size / 2., 0.)
        };
        let kind = if hole {
            TrimKind::Buttonhole {
                start_m: c - half,
                end_m: c + half,
                width_m: 0.004,
            }
        } else {
            TrimKind::Button {
                center_m: c,
                diameter_m: size,
            }
        };
        t = authoring::apply(
            &t,
            Command::SetTrim {
                trim: Trim {
                    id: id.clone(),
                    panel_id: panel.id.clone(),
                    kind,
                },
            },
        )?
        .template;
        ids.push(id);
    }
    Ok(Build {
        command: Command::RunScript {
            template: Box::new(t),
        },
        done: format!(
            "{count} {} on {} ({}), {:.1} cm{}. command_run trim.fasten buttons them to \
             the nearest buttonholes",
            if hole { "buttonhole(s)" } else { "button(s)" },
            panel.id,
            ids.join(", "),
            size * 100.,
            if hole { " long" } else { " across" }
        ),
        history_name: if hole { "Buttonholes" } else { "Buttons" },
        lay: None,
    })
}

/// The pieces an agent named (one `piece` or several `pieces`).
fn pieces(template: &GarmentTemplate, args: &Value) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    if let Some(p) = args.get("piece").and_then(Value::as_str) {
        out.push(find_piece(template, p)?.id.clone());
    }
    for p in args
        .get("pieces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.push(find_piece(template, p.as_str().unwrap_or(""))?.id.clone());
    }
    out.dedup();
    if out.is_empty() {
        return Err("Name a piece (or pieces)".into());
    }
    Ok(out)
}

/// Move or turn pieces on the 2D board (their layout only; the cloth is not touched).
pub fn move_on_board(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let ids = pieces(template, args)?;
    let delta = args.get("move_cm").and_then(pair).unwrap_or(DVec2::ZERO);
    let turn = optional(args, "rotate_degrees").unwrap_or(0.);
    if delta == DVec2::ZERO && turn == 0. {
        return Err("Give move_cm [dx, dy] and/or rotate_degrees".into());
    }
    let commands = ids
        .iter()
        .map(|id| {
            let i = template
                .panels
                .iter()
                .position(|p| &p.id == id)
                .unwrap_or(0);
            Command::BoardTransform {
                panel: id.clone(),
                origin: authoring::layout(template, i) + delta,
                rotation_degrees: template.panels[i].layout_rotation_degrees + turn,
            }
        })
        .collect();
    Ok(Build {
        command: Command::RunScript {
            template: Box::new(chain(template, commands)?),
        },
        done: format!(
            "Moved {} on the board by ({:.1}, {:.1}) cm{}",
            ids.join(", "),
            delta.x * 100.,
            delta.y * 100.,
            if turn != 0. {
                format!(" and turned it {turn:.0}° about its board origin")
            } else {
                String::new()
            }
        ),
        history_name: "Move on board",
        lay: None,
    })
}

/// Move or turn pieces in 3D about `pivot` (their centre as draped now), in world cm: x to
/// the avatar's left, y up, z to its front.
pub fn move_in_3d(template: &GarmentTemplate, args: &Value, pivot: DVec3) -> Result<Build, String> {
    let ids = pieces(template, args)?;
    let translation = args.get("move_cm").and_then(triple).unwrap_or(DVec3::ZERO);
    let degrees = optional(args, "rotate_degrees").unwrap_or(0.);
    let axis = match args.get("axis").and_then(Value::as_str).unwrap_or("y") {
        "x" => DVec3::X,
        "y" => DVec3::Y,
        "z" => DVec3::Z,
        other => return Err(format!("axis '{other}': x, y or z")),
    };
    if translation == DVec3::ZERO && degrees == 0. {
        return Err("Give move_cm [x, y, z] and/or rotate_degrees".into());
    }
    if translation.length() > 3. {
        return Err("A move of more than 3 m is surely a mistake".into());
    }
    Ok(Build {
        command: Command::PlaceGroup {
            panels: ids.clone(),
            pivot,
            translation,
            rotation: DQuat::from_axis_angle(axis, degrees.to_radians()),
        },
        done: format!(
            "Moved {} in 3D by ({:.1}, {:.1}, {:.1}) cm{}. The drape restarts from there: \
             simulate",
            ids.join(", "),
            translation.x * 100.,
            translation.y * 100.,
            translation.z * 100.,
            if degrees != 0. {
                format!(
                    ", turned {degrees:.0}° about {}",
                    ["x", "y", "z"][axis.abs().max_position()]
                )
            } else {
                String::new()
            }
        ),
        history_name: "Move in 3D",
        lay: None,
    })
}

/// A garment detail from a few numbers: a patch pocket, a band collar or a placket, cut,
/// sewn and laid on the drape in one undo step.
pub fn add_detail(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    match text(args, "kind")? {
        "patch_pocket" => patch_pocket(template, args),
        "band_collar" => band_collar(template, args),
        "placket" => placket(template, args),
        other => Err(format!(
            "kind '{other}': patch_pocket, band_collar or placket"
        )),
    }
}

fn patch_pocket(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let host = find_piece(template, text(args, "piece")?)?;
    let w = number(args, "width_cm")? / 100.;
    let h = number(args, "height_cm")? / 100.;
    if !(0.03..=0.6).contains(&w) || !(0.03..=0.6).contains(&h) {
        return Err("A pocket is 3 to 60 cm wide and tall".into());
    }
    let centre = args
        .get("centre_cm")
        .and_then(pair)
        .ok_or("centre_cm: [x, y] of the pocket's middle on the piece, cm (piece_detail)")?;
    let polygon = host
        .polygon
        .as_ref()
        .ok_or("The piece has no editable outline")?;
    let corner = centre - DVec2::new(w, h) / 2.;
    let corners = [
        corner,
        corner + DVec2::new(0., h),
        corner + DVec2::new(w, h),
        corner + DVec2::new(w, 0.),
    ];
    if !corners.iter().all(|c| inside(&polygon.points, *c)) {
        return Err(format!(
            "The pocket does not fit inside {} there: move centre_cm or make it smaller",
            host.id
        ));
    }
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Pocket")
        .to_owned();
    let (t, pocket) = add_rectangle(template, w, h, &name, &host.fabric)?;
    // Where it is stitched on the piece: left side, bottom and right side, top left open.
    let lines_before: Vec<String> = polygon.lines.iter().map(|l| l.id.clone()).collect();
    let t = authoring::apply(
        &t,
        Command::InternalLine {
            panel: host.id.clone(),
            points: corners.to_vec(),
            tangents: vec![DVec2::ZERO; 4],
        },
    )?
    .template;
    let line = t
        .panels
        .iter()
        .find(|p| p.id == host.id)
        .and_then(|p| p.polygon.as_ref())
        .and_then(|q| q.lines.iter().find(|l| !lines_before.contains(&l.id)))
        .map(|l| l.id.clone())
        .ok_or("The stitching line was not made")?;
    // The pocket's right, bottom and left run top right → bottom right → bottom left → top
    // left, the stitching line the other way round.
    let t = authoring::apply(
        &t,
        Command::SewChain {
            seam: None,
            a: vec![
                whole(&pocket, "edge_2", false),
                whole(&pocket, "edge_3", false),
                whole(&pocket, "edge_4", false),
            ],
            b: vec![whole(&host.id, &line, false)],
            reverse: true,
            ease: false,
        },
    )?
    .template;
    Ok(Build {
        command: Command::RunScript {
            template: Box::new(t),
        },
        done: format!(
            "Patch pocket {pocket} ({:.1} × {:.1} cm) on {} centred at ({:.1}, {:.1}) cm, \
             stitched on three sides along {line}, open at the top. It starts on the piece as \
             draped now: simulate, then check_garment",
            w * 100.,
            h * 100.,
            host.id,
            centre.x * 100.,
            centre.y * 100.
        ),
        history_name: "Patch pocket",
        lay: Some(Lay::Over {
            piece: pocket,
            host: host.id.clone(),
            offset: corner,
            along: DVec2::X,
        }),
    })
}

/// The segments an agent named, in order, each turned to continue the one before.
/// Outline points that are one point of the garment: the same point, or the ends of whole
/// one-to-one seams sewn together (a shoulder seam joins the front's and the back's neck
/// corners). Each point maps to a representative.
fn sewn_points(
    template: &GarmentTemplate,
) -> std::collections::BTreeMap<(String, usize), (String, usize)> {
    use std::collections::BTreeMap;
    let mut parent: BTreeMap<(String, usize), (String, usize)> = BTreeMap::new();
    fn find(
        parent: &mut BTreeMap<(String, usize), (String, usize)>,
        p: (String, usize),
    ) -> (String, usize) {
        let mut root = p.clone();
        while let Some(up) = parent.get(&root).filter(|u| **u != root) {
            root = up.clone();
        }
        if root != p {
            parent.insert(p, root.clone());
        }
        root
    }
    let ends = |r: &BoundaryRef| -> Option<[(String, usize); 2]> {
        let panel = template.panels.iter().find(|p| p.id == r.panel)?;
        let (i, ..) = segment(panel, r.edge.as_deref()?).ok()?;
        let n = panel.polygon.as_ref()?.points.len();
        let whole = r.range[0] <= 1e-6 && r.range[1] >= 1. - 1e-6;
        whole.then(|| [(r.panel.clone(), i), (r.panel.clone(), (i + 1) % n)])
    };
    for seam in template
        .seams
        .iter()
        .filter(|s| s.a_tail.is_empty() && s.b_tail.is_empty())
    {
        let (Some([a0, a1]), Some([b0, b1])) = (ends(&seam.a), ends(&seam.b)) else {
            continue;
        };
        let pairs = if seam.reverse_b {
            [(a0, b1), (a1, b0)]
        } else {
            [(a0, b0), (a1, b1)]
        };
        for (x, y) in pairs {
            let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
            if rx != ry {
                parent.insert(rx, ry);
            }
        }
    }
    let keys: Vec<(String, usize)> = parent.keys().cloned().collect();
    keys.into_iter()
        .map(|k| {
            let root = find(&mut parent, k.clone());
            (k, root)
        })
        .collect()
}

/// The segments an agent named, put in order along the edge from the first one, each turned
/// to continue the one before. Segments follow each other where they share a point of the
/// garment (the same corner, or corners a seam joins); failing that, the nearest in 3D.
fn segment_chain(template: &GarmentTemplate, args: &Value) -> Result<Vec<BoundaryRef>, String> {
    let list = args
        .get("segments")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .ok_or("segments: [{piece, segment}, …], the first where the collar starts")?;
    let sewn = sewn_points(template);
    let identity = |p: (String, usize)| sewn.get(&p).cloned().unwrap_or(p);
    // Each segment: its reference, its end points as garment points, and in 3D.
    type End = ((String, usize), DVec3);
    let mut segments: Vec<(BoundaryRef, End, End)> = Vec::new();
    for s in list {
        let panel = find_piece(
            template,
            s.get("piece").and_then(Value::as_str).unwrap_or(""),
        )?;
        let edge = s
            .get("segment")
            .and_then(Value::as_str)
            .ok_or("Each segment needs piece and segment")?;
        let (i, a, b) = segment(panel, edge)?;
        let n = panel.polygon.as_ref().map_or(1, |q| q.points.len());
        let world = |q: DVec2| {
            let mut local = DVec3::new(q.x, -q.y, 0.);
            if let Some(wrap) = template.panel_wraps.get(&panel.id) {
                local = wrap.bend(local);
            }
            panel.placement.transform(local)
        };
        segments.push((
            whole(&panel.id, edge, false),
            (identity((panel.id.clone(), i)), world(a)),
            (identity((panel.id.clone(), (i + 1) % n)), world(b)),
        ));
    }
    // How well a segment end continues from a chain end: sharing a garment point beats any
    // distance.
    let gap = |from: &End, to: &End| {
        if from.0 == to.0 {
            -1.
        } else {
            from.1.distance(to.1)
        }
    };
    let mut rest = segments.split_off(1);
    let (mut first, a, b) = segments.remove(0);
    let best = |end: &End, rest: &[(BoundaryRef, End, End)]| {
        rest.iter()
            .map(|(_, x, y)| gap(end, x).min(gap(end, y)))
            .fold(f64::INFINITY, f64::min)
    };
    first.reversed = !rest.is_empty() && best(&a, &rest) < best(&b, &rest);
    let mut end = if first.reversed { a } else { b };
    let mut chain = vec![first];
    while !rest.is_empty() {
        let k = (0..rest.len())
            .min_by(|&i, &j| {
                let d = |k: usize| gap(&end, &rest[k].1).min(gap(&end, &rest[k].2));
                d(i).total_cmp(&d(j))
            })
            .unwrap_or(0);
        let (mut r, x, y) = rest.remove(k);
        r.reversed = gap(&end, &y) < gap(&end, &x);
        end = if r.reversed { x } else { y };
        chain.push(r);
    }
    Ok(chain)
}

fn chain_length(template: &GarmentTemplate, chain: &[BoundaryRef]) -> f64 {
    chain
        .iter()
        .filter_map(|r| {
            let p = template.panels.iter().find(|p| p.id == r.panel)?;
            authoring::edge_length(p.polygon.as_ref()?, r.edge.as_deref()?).ok()
        })
        .sum()
}

fn band_collar(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let chain = segment_chain(template, args)?;
    let height = optional(args, "height_cm").unwrap_or(3.5) / 100.;
    if !(0.01..=0.2).contains(&height) {
        return Err("A band collar is 1 to 20 cm tall".into());
    }
    let length = chain_length(template, &chain);
    let fabric = template
        .panels
        .iter()
        .find(|p| p.id == chain[0].panel)
        .map(|p| p.fabric.clone())
        .unwrap_or_default();
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Collar")
        .to_owned();
    let (t, collar) = add_rectangle(template, length, height, &name, &fabric)?;
    // The collar's bottom runs right to left; the neckline chain left to right.
    let t = authoring::apply(
        &t,
        Command::SewChain {
            seam: None,
            a: vec![whole(&collar, "edge_3", false)],
            b: chain.clone(),
            reverse: true,
            ease: false,
        },
    )?
    .template;
    Ok(Build {
        command: Command::RunScript {
            template: Box::new(t),
        },
        done: format!(
            "Band collar {collar}: {:.1} cm long (the neckline's length) and {:.1} cm tall, \
             sewn along {}; it starts standing on the neckline. Simulate, then check_garment",
            length * 100.,
            height * 100.,
            chain
                .iter()
                .map(|r| format!("{}.{}", r.panel, r.edge.as_deref().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        history_name: "Band collar",
        lay: Some(Lay::Up {
            piece: collar,
            starts: chain
                .iter()
                .map(|r| {
                    template
                        .panels
                        .iter()
                        .find(|p| p.id == r.panel)
                        .and_then(|p| segment(p, r.edge.as_deref()?).ok())
                        .map_or(DVec2::ZERO, |(_, a, _)| a)
                })
                .collect(),
            chain,
            height,
        }),
    })
}

fn placket(template: &GarmentTemplate, args: &Value) -> Result<Build, String> {
    let host = find_piece(template, text(args, "piece")?)?;
    let edge = text(args, "segment")?;
    let (_, a, b) = segment(host, edge)?;
    let width = optional(args, "width_cm").unwrap_or(3.) / 100.;
    if !(0.01..=0.15).contains(&width) {
        return Err("A placket is 1 to 15 cm wide".into());
    }
    let length = authoring::edge_length(host.polygon.as_ref().ok_or("No outline")?, edge)?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Placket")
        .to_owned();
    let (t, strip) = add_rectangle(template, length, width, &name, &host.fabric)?;
    let t = authoring::apply(
        &t,
        Command::Sew {
            a: whole(&strip, "edge_1", false),
            b: whole(&host.id, edge, false),
            reverse: false,
            ease: false,
        },
    )?
    .template;
    // The strip lies on the piece along the segment, its top edge on it: its x runs along
    // the segment and its y into the piece (a quarter turn on, the outline turning
    // counterclockwise).
    let along = (b - a).normalize_or_zero();
    Ok(Build {
        command: Command::RunScript {
            template: Box::new(t),
        },
        done: format!(
            "Placket {strip}: {:.1} × {:.1} cm, sewn to {}.{edge} and lying on the piece along \
             it. Add buttons with add_buttons; simulate, then check_garment",
            length * 100.,
            width * 100.,
            host.id
        ),
        history_name: "Placket",
        lay: Some(Lay::Over {
            piece: strip,
            host: host.id.clone(),
            offset: a,
            along,
        }),
    })
}

/// Where a host's material point lies on the drape now.
fn on_drape(garment: &CompiledGarment, x: &[DVec3], panel: &str, uv: DVec2) -> Option<DVec3> {
    let weights = garment.material_point(panel, uv)?;
    Some(weights.iter().map(|(v, w)| x[*v] * *w).sum())
}

/// Each segment of a chain on the drape, in chain order: its cloth points and its length at
/// rest (along the material).
fn chain_runs(
    garment: &CompiledGarment,
    x: &[DVec3],
    chain: &[BoundaryRef],
    starts: &[DVec2],
) -> Vec<(Vec<DVec3>, f64)> {
    let uv = &garment.mesh.uv;
    chain
        .iter()
        .zip(starts)
        .filter_map(|(r, start)| {
            let range = garment.panels.iter().find(|p| p.id == r.panel)?;
            let mut run = range.boundaries.get(r.edge.as_ref()?)?.clone();
            // The compiled boundary may run either way: first its segment's own direction,
            // then the chain's.
            if run.len() > 1
                && uv[run[run.len() - 1]].distance(*start) < uv[run[0]].distance(*start)
            {
                run.reverse();
            }
            if r.reversed {
                run.reverse();
            }
            let rest = run.windows(2).map(|w| uv[w[0]].distance(uv[w[1]])).sum();
            Some((run.iter().map(|v| x[*v]).collect(), rest))
        })
        .collect()
}

/// The point at share `t` (0 to 1) of a polyline's length.
fn at_share(points: &[DVec3], t: f64) -> DVec3 {
    let total: f64 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = t.clamp(0., 1.) * total;
    for w in points.windows(2) {
        let d = w[0].distance(w[1]);
        if left <= d {
            return if d > 0. {
                w[0].lerp(w[1], left / d)
            } else {
                w[0]
            };
        }
        left -= d;
    }
    points.last().copied().unwrap_or(DVec3::ZERO)
}

/// The point of a chain at `s` metres of rest length from its start.
fn on_chain(runs: &[(Vec<DVec3>, f64)], s: f64) -> DVec3 {
    let mut left = s.max(0.);
    for (k, (run, rest)) in runs.iter().enumerate() {
        if left <= *rest || k + 1 == runs.len() {
            return at_share(run, if *rest > 0. { left / rest } else { 0. });
        }
        left -= rest;
    }
    DVec3::ZERO
}

/// Where each point of the new piece starts, on the drape as it is: `(vertex, position)`.
/// `clearance` keeps it off the cloth it lies on; `axis` is a point on the body's vertical
/// axis, so "outward" means away from it.
pub fn lay(
    garment: &CompiledGarment,
    x: &[DVec3],
    lay: &Lay,
    clearance: f64,
    axis: DVec3,
) -> Vec<(usize, DVec3)> {
    let piece = match lay {
        Lay::Over { piece, .. } | Lay::Up { piece, .. } => piece,
    };
    let Some(range) = garment.panels.iter().find(|p| &p.id == piece) else {
        return vec![];
    };
    let vertices: Vec<usize> = range.vertices.clone().collect();
    match lay {
        Lay::Over {
            host,
            offset,
            along,
            ..
        } => {
            let turn = |p: DVec2| *offset + *along * p.x + DVec2::new(-along.y, along.x) * p.y;
            let Some(host_range) = garment.panels.iter().find(|p| &p.id == host) else {
                return vec![];
            };
            let on: Vec<DVec3> = vertices
                .iter()
                .map(|v| on_drape(garment, x, host, turn(garment.mesh.uv[*v])).unwrap_or(x[*v]))
                .collect();
            let triangles: Vec<[usize; 3]> = garment.mesh.triangles[host_range.triangles.clone()]
                .iter()
                .map(|t| t.vertices)
                .collect();
            let laid = crate::garment::transfer::superimpose_positions(
                &on, x, &triangles, axis, true, clearance,
            );
            vertices.into_iter().zip(laid).collect()
        }
        Lay::Up {
            chain,
            starts,
            height,
            ..
        } => {
            let runs = chain_runs(garment, x, chain, starts);
            let total: f64 = runs.iter().map(|(_, r)| r).sum();
            // Keep the ends a little apart: round a closed neckline they would start on top of
            // each other. The seam closes the gap while simulating.
            let margin = (2. * clearance).min(total * 0.02);
            let length = vertices
                .iter()
                .map(|v| garment.mesh.uv[*v].x)
                .fold(0f64, f64::max)
                .max(1e-9);
            vertices
                .iter()
                .map(|v| {
                    let uv = garment.mesh.uv[*v];
                    let p = on_chain(&runs, margin + uv.x / length * (total - 2. * margin));
                    let mut out = p - axis;
                    out.y = 0.;
                    let out = out.normalize_or_zero();
                    (*v, p + DVec3::Y * (height - uv.y).max(0.) + out * clearance)
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garment::presets::{TshirtMeasurements, tshirt};
    use serde_json::json;

    fn shirt() -> GarmentTemplate {
        tshirt(&TshirtMeasurements::default(), 1).unwrap()
    }
    fn applied(t: &GarmentTemplate, build: Build) -> GarmentTemplate {
        authoring::apply(t, build.command).unwrap().template
    }

    #[test]
    fn bending_a_segment_moves_its_middle_by_the_bulge() {
        let t = shirt();
        let front = &t.panels[0];
        let edge = front.polygon.as_ref().unwrap().edges[0].clone();
        let before = authoring::edge_length(front.polygon.as_ref().unwrap(), &edge).unwrap();
        let build = bend_segment(
            &t,
            &json!({"piece": front.id, "segment": edge, "bulge_cm": 2.0}),
        )
        .unwrap();
        let after = applied(&t, build);
        let p = after.panels[0].polygon.as_ref().unwrap();
        assert!(p.curves.contains_key(&edge));
        assert!(authoring::edge_length(p, &edge).unwrap() > before);
    }

    #[test]
    fn a_patch_pocket_is_cut_stitched_on_three_sides_and_laid_on_the_piece() {
        let t = shirt();
        let front = t.panels[0].clone();
        let size = crate::agent_ops::piece_size(&front);
        let lo = front
            .polygon
            .as_ref()
            .unwrap()
            .points
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |a, b| a.min(*b));
        let centre = (lo + size / 2.) * 100.;
        let build = add_detail(
            &t,
            &json!({"kind": "patch_pocket", "piece": front.id, "width_cm": 14, "height_cm": 15,
                    "centre_cm": [centre.x, centre.y]}),
        )
        .unwrap();
        let lay_spec = build.lay.clone().unwrap();
        let after = applied(&t, build);
        assert_eq!(after.panels.len(), t.panels.len() + 1);
        assert_eq!(after.seams.len(), t.seams.len() + 1);
        // Two new pieces never start in the same place.
        let again = add_detail(
            &after,
            &json!({"kind": "patch_pocket", "piece": front.id, "width_cm": 10, "height_cm": 10,
                    "centre_cm": [centre.x, centre.y + 20.]}),
        )
        .unwrap();
        let twice = applied(&after, again);
        let [p1, p2] = [
            &twice.panels[twice.panels.len() - 2],
            &twice.panels[twice.panels.len() - 1],
        ];
        assert!(p1.placement.translation.distance(p2.placement.translation) > 0.05);
        assert_ne!(p1.layout_2d, p2.layout_2d);
        // Both sides measure the same: no "unequal seam" warning for the stitching line.
        let warnings = crate::agent_ops::warnings(&after, None);
        assert!(
            !warnings.iter().any(|w| w.contains("without ease")),
            "{warnings:?}"
        );
        let garment = after.compile().unwrap();
        let mut x = garment.initial_state.x.clone();
        let axis = DVec3::ZERO;
        for (v, p) in lay(&garment, &x.clone(), &lay_spec, 0.004, axis) {
            x[v] = p;
        }
        // Sewn the right way round, the pocket's stitched sides start next to their line.
        let seam = garment.seams.last().unwrap();
        let gap = seam.max_gap(&x);
        assert!(gap < 0.02, "pocket seam starts {gap} m apart");
    }

    #[test]
    fn a_band_collar_is_as_long_as_the_neckline_it_is_sewn_to() {
        let t = shirt();
        // The front's neckline: three segments in a row (given in any direction).
        let front = &t.panels[0];
        let neck = ["neck-right", "neck-bottom", "neck-left"];
        let segments: Vec<Value> = neck
            .iter()
            .map(|e| json!({"piece": front.id, "segment": e}))
            .collect();
        let build = add_detail(
            &t,
            &json!({"kind": "band_collar", "segments": segments.clone(), "height_cm": 4}),
        )
        .unwrap();
        assert!(matches!(build.lay, Some(Lay::Up { .. })));
        let after = applied(&t, build);
        let collar = after.panels.last().unwrap();
        let q = front.polygon.as_ref().unwrap();
        let expected: f64 = neck
            .iter()
            .map(|e| authoring::edge_length(q, e).unwrap())
            .sum();
        let size = crate::agent_ops::piece_size(collar);
        assert!((size.x - expected).abs() < 1e-6 && (size.y - 0.04).abs() < 1e-6);
        let garment = after.compile().unwrap();
        let seam = garment
            .seams
            .iter()
            .find(|s| s.id == after.seams.last().unwrap().id)
            .unwrap();
        // Laid standing on the neckline, the collar's bottom starts on it, end to end.
        let lay_spec = add_detail(
            &t,
            &json!({"kind": "band_collar", "segments": segments, "height_cm": 4}),
        )
        .unwrap()
        .lay
        .unwrap();
        let mut x = garment.initial_state.x.clone();
        for (v, p) in lay(&garment, &x.clone(), &lay_spec, 0.004, DVec3::ZERO) {
            x[v] = p;
        }
        let gap = seam.max_gap(&x);
        assert!(gap < 0.02, "collar seam starts {gap} m apart");
    }

    #[test]
    fn a_neckline_given_in_any_order_is_followed_around_the_neck() {
        let t = crate::garment::presets::shirt(&Default::default(), 1).unwrap();
        let seg = |p: &str, e: &str| json!({"piece": p, "segment": e});
        let args = json!({"segments": [
            seg("front", "neck-bottom"), seg("front", "neck-left"), seg("back", "neck-left"),
            seg("back", "neck-bottom"), seg("back", "neck-right"), seg("front", "neck-right"),
        ]});
        let chain = segment_chain(&t, &args).unwrap();
        let names: Vec<String> = chain
            .iter()
            .map(|r| format!("{}.{}", r.panel, r.edge.as_deref().unwrap_or("")))
            .collect();
        // From the front's bottom along one side, over the shoulder, round the back, and back
        // over the other shoulder.
        assert_eq!(names[0], "front.neck-bottom");
        assert!(names[1].starts_with("front.neck-"), "{names:?}");
        assert!(names[2].starts_with("back."), "{names:?}");
        assert_eq!(names[3], "back.neck-bottom", "{names:?}");
        assert!(names[4].starts_with("back."), "{names:?}");
        assert!(names[5].starts_with("front.neck-"), "{names:?}");
        assert_ne!(names[1], names[5]);
    }

    #[test]
    fn a_collar_round_the_whole_neckline_starts_clear_of_itself() {
        let t = crate::garment::presets::shirt(&Default::default(), 1).unwrap();
        let seg = |p: &str, e: &str| json!({"piece": p, "segment": e});
        let build = add_detail(
            &t,
            &json!({"kind": "band_collar", "height_cm": 3.5, "segments": [
                seg("front", "neck-bottom"), seg("front", "neck-left"), seg("back", "neck-left"),
                seg("back", "neck-bottom"), seg("back", "neck-right"), seg("front", "neck-right"),
            ]}),
        )
        .unwrap();
        let spec = build.lay.clone().unwrap();
        let after = applied(&t, build);
        let garment = after.compile().unwrap();
        let mut state = garment.initial_state.clone();
        let x = state.x.clone();
        for (v, p) in lay(&garment, &x, &spec, 0.008, DVec3::new(0., 1.2, 0.)) {
            state.x[v] = p;
        }
        if let Err(e) =
            crate::vbd::VbdSolver::new(std::sync::Arc::new(garment.clone()), None, &state)
        {
            panic!("the collar does not start clear: {e}");
        }
    }

    #[test]
    fn a_placket_lies_along_its_segment() {
        let t = shirt();
        let front = &t.panels[0];
        let polygon = front.polygon.as_ref().unwrap();
        // An unsewn segment of the front.
        let edge = polygon
            .edges
            .iter()
            .find(|e| {
                !t.seams.iter().any(|s| {
                    s.references()
                        .any(|r| r.panel == front.id && r.edge.as_deref() == Some(e.as_str()))
                })
            })
            .cloned()
            .expect("an unsewn segment");
        let build = add_detail(
            &t,
            &json!({"kind": "placket", "piece": front.id, "segment": edge, "width_cm": 3}),
        )
        .unwrap();
        let lay_spec = build.lay.clone().unwrap();
        let after = applied(&t, build);
        let garment = after.compile().unwrap();
        let mut x = garment.initial_state.x.clone();
        for (v, p) in lay(&garment, &x.clone(), &lay_spec, 0.004, DVec3::ZERO) {
            x[v] = p;
        }
        let seam = garment.seams.last().unwrap();
        let gap = seam.max_gap(&x);
        assert!(gap < 0.02, "placket seam starts {gap} m apart");
    }

    #[test]
    fn buttons_lines_and_moves_are_one_step_each() {
        let t = shirt();
        let id = t.panels[0].id.clone();
        let after = applied(
            &t,
            buttons(
                &t,
                &json!({"piece": id, "at_cm": [20, 10], "count": 5, "spacing_cm": [0, 8]}),
            )
            .unwrap(),
        );
        assert_eq!(after.trims.len(), t.trims.len() + 5);
        let lo = t.panels[0]
            .polygon
            .as_ref()
            .unwrap()
            .points
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |a, b| a.min(*b));
        let mid = (lo + crate::agent_ops::piece_size(&t.panels[0]) / 2.) * 100.;
        let after = applied(
            &t,
            internal_line(
                &t,
                &json!({"piece": id, "points_cm": [[mid.x - 5., mid.y], [mid.x + 5., mid.y + 10.]]}),
            )
            .unwrap(),
        );
        assert_eq!(
            after.panels[0].polygon.as_ref().unwrap().lines.len(),
            t.panels[0].polygon.as_ref().unwrap().lines.len() + 1
        );
        let after = applied(
            &t,
            move_on_board(
                &t,
                &json!({"piece": id, "move_cm": [5, 0], "rotate_degrees": 90}),
            )
            .unwrap(),
        );
        assert_eq!(after.panels[0].layout_rotation_degrees, 90.);
        let moved = move_in_3d(
            &t,
            &json!({"piece": id, "move_cm": [0, 10, 0]}),
            DVec3::ZERO,
        )
        .unwrap();
        let after = applied(&t, moved);
        let dy = after.panels[0].placement.translation.y - t.panels[0].placement.translation.y;
        assert!((dy - 0.1).abs() < 1e-9);
    }
}
