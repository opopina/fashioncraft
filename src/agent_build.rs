//! Exact engines for what an agent would otherwise do by placing raw coordinates or dragging
//! the mouse: the model says what it wants in centimetres and FashionCraft works out every
//! point, seam and 3D position. A patch pocket, a band collar and a placket from a few
//! numbers; a segment bent by its bulge; internal lines, buttons and buttonholes; pieces moved
//! or turned on the board or in 3D.
//!
//! Like `agent_ops`, everything here reads the document alone and returns the command that
//! makes the change, so it can be tested without a window. Several changes that belong
//! together become one undo step: the window applies the list of commands one after another
//! and records them as one change, so pieces keep their drape.
use crate::agent_ops::{cm, find_piece};
use crate::authoring::{self, Command};
use crate::garment::{
    CompiledGarment, GarmentTemplate,
    seam::{BoundaryRef, Side},
    template::Panel,
    trims::{Trim, TrimKind},
};
use glam::{DQuat, DVec2, DVec3};
use serde_json::{Value, json};

/// A change ready to apply, what to say once it is done, and where a new piece starts in 3D.
pub struct Build {
    /// The edits, in order; the window records them as one undo step.
    pub commands: Vec<Command>,
    /// New pieces' names (id, name), given as they are installed.
    pub names: Vec<(String, String)>,
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

/// Edits applied one after another to a copy of the document, to check each and to learn
/// the ids they make; the window then applies the same list as one undo step.
struct Steps {
    template: GarmentTemplate,
    commands: Vec<Command>,
}

impl Steps {
    fn new(template: &GarmentTemplate) -> Self {
        Self {
            template: template.clone(),
            commands: Vec::new(),
        }
    }
    fn push(&mut self, command: Command) -> Result<&GarmentTemplate, String> {
        self.template = authoring::apply(&self.template, command.clone())?.template;
        self.commands.push(command);
        Ok(&self.template)
    }
}

/// The edit a list of commands makes, applied one after another (as the window records
/// them), with `reset_simulation` set when any of them resets it.
pub fn compose(
    template: &GarmentTemplate,
    commands: &[Command],
) -> Result<authoring::Edit, String> {
    let mut edit: Option<authoring::Edit> = None;
    for c in commands {
        let base = edit.as_ref().map_or(template, |e| &e.template);
        let mut next = authoring::apply(base, c.clone())?;
        if let Some(previous) = &edit {
            next.reset_simulation |= previous.reset_simulation;
        }
        edit = Some(next);
    }
    edit.ok_or_else(|| "Nothing to change".into())
}

/// Whether an edit's garment can start simulating from where the window would start it (the
/// drape of `draped` in `state` kept where the pieces are unchanged): the solver accepts the
/// start (no cloth inside cloth or the body), no two surfaces touch, and the `added` pieces
/// start at least a contact radius from the other cloth. Closer, the first steps press them
/// into it until the solver refuses to go on (two flat drafts of the same hip girth lie in
/// one plane, say).
pub fn starts_clear(
    edit: &authoring::Edit,
    added: &[String],
    draped: &CompiledGarment,
    state: &crate::model::State,
    body: &crate::avatar::SurfaceMesh,
    contact: &crate::ogc::OgcSettings,
) -> bool {
    use crate::ogc::{geometry, potential::Kind};
    let kept = edit.state_keeping_drape(draped, state);
    let garment = &edit.garment;
    let Ok(mut solver) =
        crate::vbd::VbdSolver::new(std::sync::Arc::new(garment.clone()), Some(body), &kept)
    else {
        return false;
    };
    let x = solver.world.positions(&kept.x);
    let Ok(found) = solver.world.detect(&x, 2. * contact.radius, contact) else {
        return false;
    };
    if found.minimum_distance <= 1e-6 {
        return false;
    }
    let cloth = garment.mesh.uv.len();
    let mut fresh = vec![false; cloth];
    for range in garment.panels.iter().filter(|r| added.contains(&r.id)) {
        fresh[range.vertices.clone()].fill(true);
    }
    let p = |v: usize| x[v].to_array();
    let gap = found
        .contacts
        .iter()
        .filter(|c| {
            let vs = &c.vertices[..c.count];
            vs.iter().all(|&v| v < cloth)
                && vs.iter().any(|&v| fresh[v])
                && vs.iter().any(|&v| !fresh[v])
        })
        .map(|c| {
            let [a, b, d, e] = c.vertices;
            match c.kind {
                Kind::PointPoint => x[a].distance(x[b]),
                Kind::PointEdge => geometry::point_edge_distance(p(a), p(b), p(d)),
                Kind::PointFace | Kind::PointTriangle => {
                    geometry::point_face_distance(p(a), p(b), p(d), p(e))
                }
                Kind::EdgeEdge | Kind::EdgeEdgeInterior | Kind::SegmentSegment => {
                    geometry::edge_edge_distance(p(a), p(b), p(d), p(e))
                }
            }
        })
        .fold(f64::INFINITY, f64::min);
    gap >= contact.radius
}

/// How far out, in metres, an added garment's pieces are tried, nearest first (see
/// [`wear_over`]).
pub const WEAR_PUSHES_M: [f64; 7] = [0., 0.01, 0.02, 0.03, 0.045, 0.06, 0.08];

/// A second garment worn over what is on the avatar (a shirt with a skirt): `incoming`, a draft
/// arranged around the avatar, is added keeping that 3D arrangement (a paste would move it
/// aside), on a simulation layer outside every piece there, its pieces named after the garment
/// (`garment` "shirt": "shirt front"), once the garment there is draped (`draped_now`). Each new
/// piece moves out from the avatar's vertical axis
/// (through `axis`) by the least of [`WEAR_PUSHES_M`] for which `starts_clear` accepts the edit
/// and its new pieces' ids (see [`starts_clear`], with the drape as it is now). With the
/// distance used; an error when none is far enough.
pub fn wear_over(
    current: &GarmentTemplate,
    incoming: &GarmentTemplate,
    garment: &str,
    axis: DVec3,
    draped_now: bool,
    starts_clear: &mut dyn FnMut(&authoring::Edit, &[String]) -> bool,
) -> Result<(Build, f64), String> {
    // Two garments laid out flat together close in on the body at once and catch on each
    // other; over a draped one, the new one settles on it.
    if !draped_now && !current.panels.is_empty() {
        return Err(format!(
            "The garment there is still laid out flat: simulate it first (5 to 10 s), then add \
             the {garment}, so it goes over it as it hangs"
        ));
    }
    let ids: Vec<String> = incoming.panels.iter().map(|p| p.id.clone()).collect();
    if ids.is_empty() {
        return Err(format!("The {garment} has no pieces"));
    }
    let paste = Command::Paste {
        clipboard: authoring::clipboard::PatternClipboard::capture(incoming, &ids)?,
        center: DVec2::ZERO,
    };
    let before = current.panels.len();
    let pasted = authoring::apply(current, paste.clone())?.template;
    let new: Vec<String> = pasted.panels[before..]
        .iter()
        .map(|p| p.id.clone())
        .collect();
    // The paste moved the copy aside; it goes back where it was drafted.
    let back =
        incoming.panels[0].placement.translation - pasted.panels[before].placement.translation;
    let drafted = incoming.compile()?;
    let outward: Vec<DVec3> = incoming
        .panels
        .iter()
        .map(|p| {
            let centre = drafted
                .panels
                .iter()
                .find(|r| r.id == p.id)
                .filter(|r| !r.vertices.is_empty())
                .map_or(axis, |r| {
                    r.vertices
                        .clone()
                        .map(|v| drafted.initial_state.x[v])
                        .sum::<DVec3>()
                        / r.vertices.len() as f64
                });
            DVec3::new(centre.x - axis.x, 0., centre.z - axis.z).normalize_or_zero()
        })
        .collect();
    let layer = current.panel_layers.values().max().map_or(1, |l| l + 1);
    let word = garment.to_lowercase();
    let names: Vec<(String, String)> = new
        .iter()
        .zip(&incoming.panels)
        .map(|(id, p)| {
            // "Tshirt front" becomes "t-shirt front", "Front" "t-shirt front".
            let name = p.name.to_lowercase();
            let compact = word.replace(['-', ' '], "");
            let name = if name.contains(&word) {
                name
            } else if name.contains(&compact) {
                name.replacen(&compact, &word, 1)
            } else {
                format!("{word} {name}")
            };
            (id.clone(), name)
        })
        .collect();
    for push in WEAR_PUSHES_M {
        let mut commands = vec![paste.clone()];
        commands.extend(
            new.iter()
                .zip(&outward)
                .map(|(id, out)| Command::PlaceGroup {
                    panels: vec![id.clone()],
                    pivot: DVec3::ZERO,
                    translation: back + *out * push,
                    rotation: DQuat::IDENTITY,
                }),
        );
        commands.push(Command::SetLayer {
            panels: new.clone(),
            layer,
        });
        let edit = compose(current, &commands)?;
        if starts_clear(&edit, &new) {
            let list: Vec<&str> = names.iter().map(|(_, n)| n.as_str()).collect();
            let done = format!(
                "Added the {garment} over the garment already there, on layer {layer} (outside \
                 it: where they touch, the {garment} gives way){}: {}. The pieces there keep \
                 their drape; simulate to drape the {garment} over them",
                if push > 0. {
                    format!(", {:.1} cm out so it starts clear of it", push * 100.)
                } else {
                    String::new()
                },
                list.join(", ")
            );
            return Ok((
                Build {
                    commands,
                    names,
                    done,
                    history_name: "Add garment",
                    lay: None,
                },
                push,
            ));
        }
    }
    Err(format!(
        "The {garment} would start inside the garment already there or the body even {:.0} cm \
         out from it. Take the other garment off first (save_project, then draft with mode \
         replace), or simulation.reset to lay everything out flat and try again",
        WEAR_PUSHES_M[WEAR_PUSHES_M.len() - 1] * 100.
    ))
}

/// The template a build's edits make (its new pieces named), for tests and checks.
pub fn applied(template: &GarmentTemplate, build: &Build) -> Result<GarmentTemplate, String> {
    let mut t = template.clone();
    for c in &build.commands {
        t = authoring::apply(&t, c.clone())?.template;
    }
    for (id, name) in &build.names {
        if let Some(p) = t.panels.iter_mut().find(|p| &p.id == id) {
            p.name = name.clone();
        }
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

/// Adds a rectangle `w` × `h` (edge_1 top, edge_2 right, edge_3 bottom, edge_4 left) of
/// `fabric` to the steps, beside the other pieces. Returns the new piece's id.
fn add_rectangle(steps: &mut Steps, w: f64, h: f64, fabric: &str) -> Result<String, String> {
    let before = steps.template.clone();
    let at = crate::agent_ops::free_board_spot(&before);
    let after = steps.push(Command::Polygon {
        points: vec![
            at,
            at + DVec2::new(w, 0.),
            at + DVec2::new(w, h),
            at + DVec2::new(0., h),
        ],
        tangents: vec![],
        meshing: meshing(&before),
        fabric: fabric.to_owned(),
        replace: false,
    })?;
    new_piece(&before, after)
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
        commands: vec![Command::StyleLine {
            moves: vec![],
            bends: vec![(panel.id.clone(), name.to_owned(), outward * bulge * 4. / 3.)],
            scales: vec![],
        }],
        names: vec![],
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
    let after = authoring::apply(template, command.clone())?.template;
    let id = after
        .panels
        .iter()
        .find(|p| p.id == panel.id)
        .and_then(|p| p.polygon.as_ref())
        .and_then(|q| q.lines.iter().find(|l| !before.contains(&l.id)))
        .map(|l| l.id.clone())
        .unwrap_or_default();
    Ok(Build {
        commands: vec![command],
        names: vec![],
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
    let mut steps = Steps::new(template);
    let mut ids = Vec::new();
    for k in 0..count {
        let c = start + step * k as f64;
        let id = (1..)
            .map(|n| format!("trim_{n}"))
            .find(|id| !steps.template.trims.iter().any(|x| &x.id == id))
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
        steps.push(Command::SetTrim {
            trim: Trim {
                id: id.clone(),
                panel_id: panel.id.clone(),
                kind,
            },
        })?;
        ids.push(id);
    }
    Ok(Build {
        commands: steps.commands,
        names: vec![],
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
    let commands: Vec<Command> = ids
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
    let mut steps = Steps::new(template);
    for c in commands {
        steps.push(c)?;
    }
    Ok(Build {
        commands: steps.commands,
        names: vec![],
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
        commands: vec![Command::PlaceGroup {
            panels: ids.clone(),
            pivot,
            translation,
            rotation: DQuat::from_axis_angle(axis, degrees.to_radians()),
        }],
        names: vec![],
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
    let mut steps = Steps::new(template);
    let pocket = add_rectangle(&mut steps, w, h, &host.fabric)?;
    // Where it is stitched on the piece: left side, bottom and right side, top left open.
    let lines_before: Vec<String> = polygon.lines.iter().map(|l| l.id.clone()).collect();
    let line = steps
        .push(Command::InternalLine {
            panel: host.id.clone(),
            points: corners.to_vec(),
            tangents: vec![DVec2::ZERO; 4],
        })?
        .panels
        .iter()
        .find(|p| p.id == host.id)
        .and_then(|p| p.polygon.as_ref())
        .and_then(|q| q.lines.iter().find(|l| !lines_before.contains(&l.id)))
        .map(|l| l.id.clone())
        .ok_or("The stitching line was not made")?;
    // The pocket's right, bottom and left run top right → bottom right → bottom left → top
    // left, the stitching line the other way round.
    steps.push(Command::SewChain {
        seam: None,
        a: vec![
            whole(&pocket, "edge_2", false),
            whole(&pocket, "edge_3", false),
            whole(&pocket, "edge_4", false),
        ],
        b: vec![whole(&host.id, &line, false)],
        reverse: true,
        ease: false,
    })?;
    Ok(Build {
        commands: steps.commands,
        names: vec![(pocket.clone(), name)],
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
    let mut steps = Steps::new(template);
    let collar = add_rectangle(&mut steps, length, height, &fabric)?;
    // The collar's bottom runs right to left; the neckline chain left to right.
    steps.push(Command::SewChain {
        seam: None,
        a: vec![whole(&collar, "edge_3", false)],
        b: chain.clone(),
        reverse: true,
        ease: false,
    })?;
    Ok(Build {
        commands: steps.commands,
        names: vec![(collar.clone(), name)],
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
    let mut steps = Steps::new(template);
    let strip = add_rectangle(&mut steps, length, width, &host.fabric)?;
    steps.push(Command::Sew {
        a: whole(&strip, "edge_1", false),
        b: whole(&host.id, edge, false),
        reverse: false,
        ease: false,
    })?;
    // The strip lies on the piece along the segment, its top edge on it: its x runs along
    // the segment and its y into the piece (a quarter turn on, the outline turning
    // counterclockwise).
    let along = (b - a).normalize_or_zero();
    Ok(Build {
        commands: steps.commands,
        names: vec![(strip.clone(), name)],
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

/// Where a material point of a piece starts in 3D: its placement, after its wrap.
fn arranged(template: &GarmentTemplate, panel: &str, q: DVec2) -> Option<DVec3> {
    let p = template.panels.iter().find(|p| p.id == panel)?;
    let mut local = DVec3::new(q.x, -q.y, 0.);
    if let Some(wrap) = template.panel_wraps.get(panel) {
        local = wrap.bend(local);
    }
    Some(p.placement.transform(local))
}

/// Where a chain of segments starts and ends in 3D, each segment in its chain direction.
fn chain_ends(template: &GarmentTemplate, chain: &[BoundaryRef]) -> Option<[DVec3; 2]> {
    let ends = |r: &BoundaryRef| -> Option<[DVec3; 2]> {
        let panel = template.panels.iter().find(|p| p.id == r.panel)?;
        let (_, a, b) = segment(panel, r.edge.as_deref()?).ok()?;
        let (a, b) = if r.reversed { (b, a) } else { (a, b) };
        Some([
            arranged(template, &r.panel, a)?,
            arranged(template, &r.panel, b)?,
        ])
    };
    Some([ends(chain.first()?)?[0], ends(chain.last()?)?[1]])
}

/// Whether side b must run the other way for its ends to meet side a's nearer ends.
fn crossed(template: &GarmentTemplate, a: &[BoundaryRef], b: &[BoundaryRef]) -> bool {
    match (chain_ends(template, a), chain_ends(template, b)) {
        (Some([a0, a1]), Some([b0, b1])) => {
            a0.distance(b0) + a1.distance(b1) > a0.distance(b1) + a1.distance(b0)
        }
        _ => false,
    }
}

fn names(list: Option<&Value>, what: &str) -> Result<Vec<String>, String> {
    let list = list
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .ok_or_else(|| format!("'{what}' is missing"))?;
    list.iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("'{what}' lists segment names"))
        })
        .collect()
}

/// A sleeve set into its armhole, as draft_garment's shirt does it: wrapped round the avatar's
/// arm just below the shoulder (clear of `body`), its cap sewn to the front and back armholes
/// with ease (gathered when much longer: a puffed sleeve) and its underarm seam closed. Seams
/// already on those segments are replaced. `arms` are the avatar's shoulder and elbow joints,
/// the +x arm (the avatar's left) first.
pub fn set_sleeve(
    template: &GarmentTemplate,
    args: &Value,
    arms: [[DVec3; 2]; 2],
    body: Option<&[DVec3]>,
) -> Result<Build, String> {
    let sleeve = find_piece(template, text(args, "sleeve")?)?;
    let polygon = sleeve
        .polygon
        .as_ref()
        .ok_or("The sleeve has no editable outline")?;
    let k = match text(args, "side")? {
        "left" => 0,
        "right" => 1,
        other => return Err(format!("side '{other}': the avatar's left or right")),
    };
    let cap = names(args.get("cap"), "cap")?;
    let underarm = match args.get("underarm") {
        None | Some(Value::Null) => None,
        Some(v) => Some(names(Some(v), "underarm")?),
    };
    if underarm.as_ref().is_some_and(|u| u.len() != 2) {
        return Err("underarm names the sleeve's two side segments".into());
    }
    for name in cap.iter().chain(underarm.iter().flatten()) {
        segment(sleeve, name)?;
    }
    let ease = args.get("ease").and_then(Value::as_bool).unwrap_or(true);
    let (lo, hi) = polygon.points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let size = hi - lo;
    // On the arm: placement and wrap for an outline from (0, 0), moved to where this one lies.
    let (mut placement, mut wrap) =
        crate::garment::presets::sleeve_on_arm(arms[k], k == 0, size.x, size.y, body)?;
    placement.translation -= placement.rotation() * DVec3::new(lo.x, -lo.y, 0.);
    wrap.axis_x_m += lo.x;

    let mut steps = Steps::new(template);
    // Seams already on the sleeve's cap and underarm, or on the armholes, give way.
    let armhole_args = json!({"segments": args.get("armhole").cloned().unwrap_or(Value::Null)});
    let armhole_refs: Vec<(String, String)> = args
        .get("armhole")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .ok_or("'armhole' is missing: [{piece, segment}, …] the bodice's armhole on that side")?
        .iter()
        .map(|s| {
            let piece = find_piece(
                template,
                s.get("piece").and_then(Value::as_str).unwrap_or(""),
            )?;
            let edge = s.get("segment").and_then(Value::as_str).unwrap_or("");
            segment(piece, edge)?;
            Ok((piece.id.clone(), edge.to_owned()))
        })
        .collect::<Result<_, String>>()?;
    let replaced: Vec<String> = template
        .seams
        .iter()
        .filter(|s| {
            s.references().any(|r| {
                let edge = r.edge.as_deref().unwrap_or("");
                (r.panel == sleeve.id
                    && (cap.iter().any(|c| c == edge)
                        || underarm.iter().flatten().any(|u| u == edge)))
                    || armhole_refs.iter().any(|(p, e)| *p == r.panel && e == edge)
            })
        })
        .map(|s| s.id.clone())
        .collect();
    if !replaced.is_empty() {
        steps.push(Command::RemoveSeams {
            seams: replaced.clone(),
        })?;
    }
    steps.push(Command::Arrange {
        panel: sleeve.id.clone(),
        placement,
        wrap: Some(wrap),
    })?;
    let placed = steps.template.clone();
    let cap_args = json!({"segments": cap.iter().map(|c| json!({"piece": sleeve.id, "segment": c})).collect::<Vec<_>>()});
    let cap_chain = segment_chain(&placed, &cap_args)?;
    let hole_chain = segment_chain(&placed, &armhole_args)?;
    let mut sewn = Vec::new();
    if cap_chain.len() == 2 && hole_chain.len() == 2 {
        // Each cap half to the armhole it faces, so the cap's top meets the shoulder.
        let middle = |r: &BoundaryRef| {
            chain_ends(&placed, std::slice::from_ref(r)).map(|[a, b]| (a + b) / 2.)
        };
        let straight = match (
            middle(&cap_chain[0]),
            middle(&cap_chain[1]),
            middle(&hole_chain[0]),
            middle(&hole_chain[1]),
        ) {
            (Some(c0), Some(c1), Some(h0), Some(h1)) => {
                c0.distance(h0) + c1.distance(h1) <= c0.distance(h1) + c1.distance(h0)
            }
            _ => true,
        };
        let pairs = if straight {
            [(0, 0), (1, 1)]
        } else {
            [(0, 1), (1, 0)]
        };
        // The cap's top meets the armhole's shoulder end. Pieces are drawn upright (y down the
        // body), so each is the segment's higher end on the board: exact, unlike distances
        // between pieces that still start apart.
        let higher_first = |r: &BoundaryRef| -> Option<bool> {
            let panel = placed.panels.iter().find(|p| p.id == r.panel)?;
            let (_, a, b) = segment(panel, r.edge.as_deref()?).ok()?;
            Some(a.y < b.y)
        };
        for (c, h) in pairs {
            let (a, b) = (cap_chain[c].clone(), hole_chain[h].clone());
            let reverse = match (higher_first(&a), higher_first(&b)) {
                (Some(t), Some(s)) => t != s,
                _ => crossed(&placed, std::slice::from_ref(&a), std::slice::from_ref(&b)),
            };
            sewn.push(format!(
                "{}.{} ↔ {}.{}",
                a.panel,
                a.edge.clone().unwrap_or_default(),
                b.panel,
                b.edge.clone().unwrap_or_default()
            ));
            steps.push(Command::Sew {
                a: BoundaryRef {
                    reversed: false,
                    ..a
                },
                b: BoundaryRef {
                    reversed: false,
                    ..b
                },
                reverse,
                ease,
            })?;
        }
    } else {
        let reverse = crossed(&placed, &cap_chain, &hole_chain);
        sewn.push(format!(
            "cap ({}) ↔ armhole ({})",
            cap.join(" + "),
            armhole_refs
                .iter()
                .map(|(p, e)| format!("{p}.{e}"))
                .collect::<Vec<_>>()
                .join(" + ")
        ));
        steps.push(Command::SewChain {
            seam: None,
            a: cap_chain,
            b: hole_chain,
            reverse,
            ease,
        })?;
    }
    if let Some([u0, u1]) = underarm.as_deref() {
        // Their ends at the cap (the top of the outline) meet.
        let top_first = |name: &str| -> Result<bool, String> {
            let (_, a, b) = segment(sleeve, name)?;
            Ok(a.y <= b.y)
        };
        let reverse = top_first(u0)? != top_first(u1)?;
        sewn.push(format!("underarm {u0} ↔ {u1}"));
        steps.push(Command::Sew {
            a: whole(&sleeve.id, u0, false),
            b: whole(&sleeve.id, u1, false),
            reverse,
            ease: false,
        })?;
    }
    Ok(Build {
        commands: steps.commands,
        names: vec![],
        done: format!(
            "Sleeve {} set into the {} armhole: {}{}. It starts wrapped round the arm below the \
             shoulder. Simulate, then check_garment",
            sleeve.id,
            if k == 0 { "left" } else { "right" },
            sewn.join("; "),
            if replaced.is_empty() {
                String::new()
            } else {
                format!(" (replacing seams {})", replaced.join(", "))
            }
        ),
        history_name: "Set sleeve",
        lay: None,
    })
}

/// A piece's box on the board: its layout origin and its size (turns ignored).
fn board_box(template: &GarmentTemplate, i: usize) -> (DVec2, DVec2) {
    let lo = authoring::layout(template, i);
    (lo, lo + crate::agent_ops::piece_size(&template.panels[i]))
}

/// Pieces `ids` that overlap another piece on the board move to free space, to the right of
/// every piece: new pieces from a cut, a copy or a merge, or a drafted garment's pieces, so
/// none lies on top of another. Board only; the cloth is not touched.
pub fn spread_on_board(template: &mut GarmentTemplate, ids: &[String]) {
    let overlaps = |a: (DVec2, DVec2), b: (DVec2, DVec2)| {
        let gap = 0.005;
        a.0.x + gap < b.1.x && b.0.x + gap < a.1.x && a.0.y + gap < b.1.y && b.0.y + gap < a.1.y
    };
    for id in ids {
        let Some(i) = template.panels.iter().position(|p| &p.id == id) else {
            continue;
        };
        let mine = board_box(template, i);
        let crowded =
            (0..template.panels.len()).any(|j| j != i && overlaps(mine, board_box(template, j)));
        if crowded {
            // Freeze every implicit layout first: moving one must not move the others.
            for k in 0..template.panels.len() {
                let at = authoring::layout(template, k);
                template.panels[k].layout_2d.get_or_insert(at);
            }
            let others = {
                let mut t = template.clone();
                t.panels.remove(i);
                t
            };
            template.panels[i].layout_2d = Some(crate::agent_ops::free_board_spot(&others));
        }
    }
}

/// Names for pieces an edit made (a cut, a copy, a merge) that would share a name with
/// another piece: a half of a cut piece is named for where it lies on the board against the
/// other half ("Front (upper)" and "Front (lower)", or left and right); anything else still
/// shared gets a number. Agents should then rename pieces for what they are.
pub fn name_new_pieces(before: &GarmentTemplate, after: &mut GarmentTemplate) {
    let base = |name: &str| -> String {
        let name = name.trim();
        match name.rsplit_once(' ') {
            Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => head.to_owned(),
            _ => name.to_owned(),
        }
    };
    let new: Vec<usize> = (0..after.panels.len())
        .filter(|&i| !before.panels.iter().any(|p| p.id == after.panels[i].id))
        .collect();
    for &i in &new {
        let stem = base(&after.panels[i].name);
        // The piece it was cut from: an old piece of the same name whose outline changed.
        let sibling = (0..after.panels.len()).find(|&j| {
            j != i
                && base(&after.panels[j].name) == stem
                && before.panels.iter().any(|p| {
                    p.id == after.panels[j].id
                        && serde_json::to_value(&p.polygon).ok()
                            != serde_json::to_value(&after.panels[j].polygon).ok()
                })
        });
        if let Some(j) = sibling {
            let centre = |k: usize| {
                let (lo, hi) = board_box(after, k);
                (lo + hi) / 2.
            };
            let d = centre(i) - centre(j);
            // y runs down the board.
            let (mine, theirs) = if d.y.abs() >= d.x.abs() {
                if d.y > 0. {
                    ("lower", "upper")
                } else {
                    ("upper", "lower")
                }
            } else if d.x > 0. {
                ("right", "left")
            } else {
                ("left", "right")
            };
            after.panels[i].name = format!("{stem} ({mine})");
            if base(&after.panels[j].name) == after.panels[j].name.trim()
                || after.panels[j].name.trim() != stem
            {
                after.panels[j].name = format!("{stem} ({theirs})");
            }
        }
    }
    // Whatever is still shared gets a number.
    for &i in &new {
        let taken = |name: &str, panels: &[crate::garment::template::Panel]| {
            panels
                .iter()
                .enumerate()
                .any(|(k, p)| k != i && p.name.trim() == name)
        };
        if taken(after.panels[i].name.trim(), &after.panels) {
            let stem = base(&after.panels[i].name);
            let name = (2..)
                .map(|n| format!("{stem} {n}"))
                .find(|n| !taken(n, &after.panels))
                .unwrap_or_default();
            after.panels[i].name = name;
        }
    }
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
        super::applied(t, &build).unwrap()
    }

    #[test]
    fn copies_get_their_own_names_and_their_own_place_on_the_board() {
        let t = shirt();
        let id = t.panels[0].id.clone();
        let name = t.panels[0].name.clone();
        let mut after = authoring::apply(&t, Command::Duplicate { panel: id.clone() })
            .unwrap()
            .template;
        let copy = after.panels.last().unwrap().id.clone();
        // A copy given the same name as its original…
        after.panels.last_mut().unwrap().name = name.clone();
        after.panels.last_mut().unwrap().layout_2d = authoring::layout(&t, 0).into();
        name_new_pieces(&t, &mut after);
        spread_on_board(&mut after, std::slice::from_ref(&copy));
        let c = after.panels.iter().find(|p| p.id == copy).unwrap();
        assert_ne!(c.name, name, "a name of its own");
        let i = after.panels.iter().position(|p| p.id == copy).unwrap();
        let (lo, hi) = board_box(&after, i);
        for j in 0..after.panels.len() {
            if j != i {
                let (a, b) = board_box(&after, j);
                assert!(
                    lo.x >= b.x || hi.x <= a.x || lo.y >= b.y || hi.y <= a.y,
                    "overlaps {j}"
                );
            }
        }
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
        // And it starts clear of the piece it lies on.
        let mut state = garment.initial_state.clone();
        state.x = x.clone();
        if let Err(e) =
            crate::vbd::VbdSolver::new(std::sync::Arc::new(garment.clone()), None, &state)
        {
            panic!("the pocket does not start clear: {e}");
        }
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
