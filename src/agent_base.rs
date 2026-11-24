//! The tools that give agents the whole of FashionCraft, beyond the first set in `agent`: any
//! edit by name (`edit`, `edit_ops`), cm equivalents of the mouse tools, the windows a person
//! fills in as structured tools, exact engines for garment details, the checks an agent runs
//! on its own work, and knowledge on demand. `agent::tools` lists them after its own.
use crate::agent::Tool;
use serde_json::{Value, json};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false})
}

fn tool(
    name: &'static str,
    description: &'static str,
    input_schema: Value,
    changes_document: bool,
) -> Tool {
    Tool {
        name,
        description,
        input_schema,
        changes_document,
    }
}

/// The views `camera` and `check_garment` turn to.
pub const VIEWS: [&str; 9] = [
    "front",
    "back",
    "left",
    "right",
    "three_quarter_left",
    "three_quarter_right",
    "top",
    "bottom",
    "isometric",
];

/// The kinds of detail `add_detail` makes.
pub const DETAILS: [&str; 3] = ["patch_pocket", "band_collar", "placket"];

pub fn tools() -> Vec<Tool> {
    let piece =
        json!({"type": "string", "description": "A piece id (or name) from scene_summary."});
    let pieces = json!({"type": "array", "items": {"type": "string"}});
    let segment = json!({"type": "string", "description": "A segment id of that piece."});
    let xy = |d: &str| json!({"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": d});
    let xyz = |d: &str| json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": d});
    let world = "World cm: x to the avatar's left, y up, z to its front.";
    vec![
        tool(
            "edit_ops",
            "List FashionCraft's own edit commands (over 100: every undoable change to a \
             pattern, such as Sew, SewChain, SetLineFold, SetSeamZipper, OffsetAsLines, \
             SlashSpread, SetShrink, AddTack…) with their fields as JSON Schema. Filter with \
             words, or ask for some by name to get their fields. Use edit to run one.",
            object(
                json!({
                    "query": {"type": "string"},
                    "names": {"type": "array", "items": {"type": "string"}}
                }),
                &[],
            ),
            false,
        ),
        tool(
            "edit",
            "Run one of FashionCraft's edit commands (edit_ops lists them) with its fields: \
             the raw editing layer, in metres, checked before it is applied and undoable. \
             Prefer the cm tools when one does the job.",
            object(
                json!({
                    "op": {"type": "string", "description": "The command, such as SetSeamZipper."},
                    "fields": {"type": "object", "description": "Its fields, as edit_ops gives them."}
                }),
                &["op"],
            ),
            true,
        ),
        tool(
            "transform_piece",
            "Move or turn pieces, as dragging them would: on the 2D board (space board: \
             move_cm [dx, dy], x right, y down; rotate_degrees about the piece's board origin) \
             or in 3D (space 3d: move_cm [x, y, z] in world cm, x to the avatar's left, y up, z \
             to its front; rotate_degrees about axis x, y or z through the piece's centre).",
            object(
                json!({
                    "piece": piece, "pieces": pieces,
                    "space": {"type": "string", "enum": ["board", "3d"]},
                    "move_cm": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 3},
                    "rotate_degrees": {"type": "number"},
                    "axis": {"type": "string", "enum": ["x", "y", "z"]}
                }),
                &["space"],
            ),
            true,
        ),
        tool(
            "bend_segment",
            "Curve a segment exactly: its middle moves bulge_cm outward (negative: inward, as \
             a scooped neckline or armhole), the ends stay. Seams along it take up the length \
             change.",
            object(
                json!({"piece": piece, "segment": segment, "bulge_cm": {"type": "number"}}),
                &["piece", "segment", "bulge_cm"],
            ),
            true,
        ),
        tool(
            "style_line",
            "Change a style line from the 3D view, as dragging it there would: pick the \
             segment at a share `at` of its length (0 to 1), and move it by move_cm in world \
             cm. mode edit bends it (or moves the corner when at is near an end), move moves \
             the whole segment, scale stretches it. The same change is made across the seams \
             it is sewn into unless across_seams is false.",
            object(
                json!({
                    "piece": piece, "segment": segment,
                    "mode": {"type": "string", "enum": ["edit", "move", "scale"]},
                    "at": {"type": "number", "minimum": 0, "maximum": 1},
                    "move_cm": xyz(world),
                    "across_seams": {"type": "boolean"}
                }),
                &["piece", "segment", "mode", "move_cm"],
            ),
            true,
        ),
        tool(
            "add_internal_line",
            "Draw an internal line on a piece through points in cm (piece_detail's \
             coordinates: x right, y down): a style line, a placement line or a fold.",
            object(
                json!({"piece": piece, "points_cm": {"type": "array", "items": xy("[x, y]"), "minItems": 2}}),
                &["piece", "points_cm"],
            ),
            true,
        ),
        tool(
            "add_buttons",
            "Place buttons or buttonholes on a piece: count of them from at_cm, spacing_cm \
             apart ([dx, dy], default 8 cm down), size_cm across (buttons, default 1.5) or long \
             (buttonholes, default 1.8; vertical true turns them). command_run trim.fasten \
             then buttons them through.",
            object(
                json!({
                    "piece": piece,
                    "kind": {"type": "string", "enum": ["button", "buttonhole"]},
                    "at_cm": xy("[x, y] of the first one on the piece"),
                    "count": {"type": "integer", "minimum": 1, "maximum": 40},
                    "spacing_cm": xy("[dx, dy] between them"),
                    "size_cm": {"type": "number"},
                    "vertical": {"type": "boolean"}
                }),
                &["piece", "at_cm"],
            ),
            true,
        ),
        tool(
            "add_detail",
            "Make a garment detail exactly from a few numbers, cut, sewn and laid on the drape \
             in one undo step (add them before simulating: a piece with a new stitching line \
             is laid out again): patch_pocket (piece, width_cm, height_cm, centre_cm on the \
             piece; stitched on three sides), band_collar (segments: the neckline's segments, \
             any order, the first where the collar starts; height_cm), placket (piece, segment, width_cm). \
             guide('details') has typical sizes.",
            object(
                json!({
                    "kind": {"type": "string", "enum": DETAILS},
                    "piece": piece, "segment": segment,
                    "segments": {"type": "array", "items": object(json!({"piece": {"type": "string"}, "segment": {"type": "string"}}), &["piece", "segment"])},
                    "width_cm": {"type": "number"}, "height_cm": {"type": "number"},
                    "centre_cm": xy("[x, y] on the piece"),
                    "name": {"type": "string"}
                }),
                &["kind"],
            ),
            true,
        ),
        tool(
            "pin",
            "Pin cloth where it is now, as the pin tool does: add pins on a piece around \
             at_cm (its own cm coordinates) within radius_cm, remove them there, remove_all, \
             or move the pins there by move_cm (world cm). Pinned cloth stays put while \
             simulating.",
            object(
                json!({
                    "action": {"type": "string", "enum": ["add", "remove", "remove_all", "move"]},
                    "piece": piece,
                    "at_cm": xy("[x, y] on the piece"),
                    "radius_cm": {"type": "number", "minimum": 0.5, "maximum": 50},
                    "move_cm": xyz(world)
                }),
                &["action"],
            ),
            true,
        ),
        tool(
            "grab",
            "Pull the cloth while it simulates, as dragging it with the grab tool does: the \
             point at at_cm on a piece is pulled by pull_cm (world cm) for `seconds`, then let \
             go and the simulation pauses.",
            object(
                json!({
                    "piece": piece,
                    "at_cm": xy("[x, y] on the piece"),
                    "pull_cm": xyz(world),
                    "seconds": {"type": "number", "minimum": 0.5, "maximum": 20}
                }),
                &["piece", "at_cm", "pull_cm"],
            ),
            true,
        ),
        tool(
            "trace_on_body",
            "The 3D pen: a closed line through points on the body (world cm; each snaps to the \
             nearest point of the avatar's skin) encloses part of the body, which is \
             flattened into a new piece that starts on the body there.",
            object(
                json!({
                    "points_cm": {"type": "array", "items": xyz(world), "minItems": 3, "maxItems": 64}
                }),
                &["points_cm"],
            ),
            true,
        ),
        tool(
            "simulation_settings",
            "Read or change the simulation's settings (the Simulation properties panel): \
             quality (fast, normal, accurate), gpu, iterations, damping, friction, \
             collision_distance_mm, gravity_m_s2, wind_m_s, wind_from_degrees (0 = from the \
             front), floor, floor_height_cm, particle_distance_mm (remeshes and restarts the \
             drape). Without arguments it reports them.",
            object(
                json!({
                    "quality": {"type": "string", "enum": ["fast", "normal", "accurate"]},
                    "gpu": {"type": "boolean"},
                    "iterations": {"type": "integer", "minimum": 2, "maximum": 128},
                    "damping": {"type": "number", "minimum": 0, "maximum": 10},
                    "friction": {"type": "number", "minimum": 0, "maximum": 1.5},
                    "collision_distance_mm": {"type": "number", "minimum": 0.5, "maximum": 20},
                    "gravity_m_s2": {"type": "number"},
                    "wind_m_s": {"type": "number", "minimum": 0, "maximum": 40},
                    "wind_from_degrees": {"type": "number"},
                    "floor": {"type": "boolean"},
                    "floor_height_cm": {"type": "number"},
                    "particle_distance_mm": {"type": "number", "minimum": 8, "maximum": 60}
                }),
                &[],
            ),
            true,
        ),
        tool(
            "camera",
            "Turn the 3D view: a named view, or yaw/pitch in degrees (yaw 0 looks at the \
             avatar's front), distance_cm, target_cm, fov_degrees, orthographic, frame (fit \
             everything). With screenshot true it returns a picture of the view afterwards.",
            object(
                json!({
                    "view": {"type": "string", "enum": VIEWS},
                    "yaw_degrees": {"type": "number"}, "pitch_degrees": {"type": "number"},
                    "distance_cm": {"type": "number"}, "target_cm": xyz(world),
                    "fov_degrees": {"type": "number", "minimum": 10, "maximum": 120},
                    "orthographic": {"type": "boolean"}, "frame": {"type": "boolean"},
                    "screenshot": {"type": "boolean"}
                }),
                &[],
            ),
            false,
        ),
        tool(
            "render_settings",
            "Read or change the photoreal render's and the 3D view's settings: width, height, \
             samples, bounces, exposure_ev, depth_of_field, aperture_mm, focus_cm, \
             environment_gain, environment_turn_degrees, background, key_light, floor; and \
             the 3D view's style (textured_thick, textured, plain, translucent) and \
             high_quality. render makes the picture.",
            object(
                json!({
                    "width": {"type": "integer", "minimum": 64, "maximum": 8192},
                    "height": {"type": "integer", "minimum": 64, "maximum": 8192},
                    "samples": {"type": "integer", "minimum": 1, "maximum": 100000},
                    "bounces": {"type": "integer", "minimum": 1, "maximum": 32},
                    "exposure_ev": {"type": "number"},
                    "depth_of_field": {"type": "boolean"}, "aperture_mm": {"type": "number"},
                    "focus_cm": {"type": "number"},
                    "environment_gain": {"type": "number"}, "environment_turn_degrees": {"type": "number"},
                    "background": {"type": "boolean"}, "key_light": {"type": "boolean"}, "floor": {"type": "boolean"},
                    "style": {"type": "string", "enum": ["textured_thick", "textured", "plain", "translucent"]},
                    "high_quality": {"type": "boolean"}
                }),
                &[],
            ),
            false,
        ),
        tool(
            "avatar_maker",
            "Make a new body with the avatar maker (MakeHuman shapes, Anny settings) and use \
             it: gender (0 female to 1 male), age_years, muscle, weight, height, proportions, \
             cupsize, firmness (0 to 1), african/asian/european shares, measures (local \
             adjustments -1 to 1 by name, such as waist-circ or upperarm-length), and its look \
             (skin, eyes, hair…). start: typical (for the gender and age, the default), \
             current (the maker's last settings) or random_realistic with seed. Without \
             make true it only reports the settings and the measures' names. The drape is \
             reset.",
            object(
                json!({
                    "start": {"type": "string", "enum": ["typical", "current", "random_realistic"]},
                    "seed": {"type": "integer", "minimum": 0},
                    "gender": {"type": "number"}, "age_years": {"type": "number"},
                    "muscle": {"type": "number"}, "weight": {"type": "number"},
                    "height": {"type": "number"}, "proportions": {"type": "number"},
                    "cupsize": {"type": "number"}, "firmness": {"type": "number"},
                    "african": {"type": "number"}, "asian": {"type": "number"}, "european": {"type": "number"},
                    "measures": {"type": "object", "additionalProperties": {"type": "number"}},
                    "look": {"type": "object", "properties": {
                        "skin": {"type": "string"}, "eyes": {"type": "string"},
                        "eyebrows": {"type": "string"}, "eyelashes": {"type": "string"},
                        "hair": {"type": "string"}, "hair_colour": {"type": "string"}
                    }},
                    "make": {"type": "boolean"}
                }),
                &[],
            ),
            true,
        ),
        tool(
            "pose_joints",
            "Turn the avatar's joints, as the pose editor does: joints maps a bone name to \
             [x, y, z] degrees about the world axes at rest (x to the avatar's left, y up, z \
             front). reset first clears every joint. list true reports the bone names and the \
             current angles. The drape restarts from the arrangement: simulate.",
            object(
                json!({
                    "joints": {"type": "object", "additionalProperties": xyz("[x, y, z] degrees")},
                    "reset": {"type": "boolean"},
                    "list": {"type": "boolean"}
                }),
                &[],
            ),
            true,
        ),
        tool(
            "measure_fabric",
            "Fit a fabric's physical parameters to tests on the real cloth (the Measure fabric \
             window) and give them to pieces: weight gsm, thickness_mm, the strip test (strip \
             width, gauge length and readings of load_g → stretched length_mm along warp, weft \
             and bias), the cantilever overhang along warp and weft (mm, at 41.5°) and the slide \
             angle (friction). It runs virtual tests, so it takes some seconds.",
            object(
                json!({
                    "name": {"type": "string"},
                    "pieces": pieces,
                    "gsm": {"type": "number"}, "thickness_mm": {"type": "number"},
                    "strip_width_mm": {"type": "number"}, "strip_length_mm": {"type": "number"},
                    "gauge_mm": {"type": "number"},
                    "warp": {"type": "array", "items": object(json!({"load_g": {"type": "number"}, "length_mm": {"type": "number"}}), &["load_g", "length_mm"])},
                    "weft": {"type": "array", "items": {"type": "object"}},
                    "bias": {"type": "array", "items": {"type": "object"}},
                    "overhang_warp_mm": {"type": "number"}, "overhang_weft_mm": {"type": "number"},
                    "slide_angle_degrees": {"type": "number"}
                }),
                &["gsm", "thickness_mm"],
            ),
            true,
        ),
        tool(
            "points_of_measure",
            "The size chart's points of measure (the Points of measure window): add one, a \
             distance between two outline points (piece, from_point, to_point) or the length of \
             segments ([{piece, segment}]), with tolerance_cm; remove one; set its tolerance; \
             or list them. Needs sizes (grading create). size_chart reports them in every size.",
            object(
                json!({
                    "action": {"type": "string", "enum": ["add", "remove", "set_tolerance", "list"]},
                    "name": {"type": "string"}, "id": {"type": "string"},
                    "piece": piece,
                    "from_point": {"type": "integer", "minimum": 0}, "to_point": {"type": "integer", "minimum": 0},
                    "segments": {"type": "array", "items": object(json!({"piece": {"type": "string"}, "segment": {"type": "string"}}), &["piece", "segment"])},
                    "tolerance_cm": {"type": "number", "minimum": 0}
                }),
                &["action"],
            ),
            true,
        ),
        tool(
            "measure",
            "Measure as the tape tools do: the body's girth at a height (height_cm above the \
             floor), the draped garment's girth there, and the ease between them \
             (what: girth); or the straight distance between two outline points of a piece \
             (what: points, piece, from_point, to_point).",
            object(
                json!({
                    "what": {"type": "string", "enum": ["girth", "points"]},
                    "height_cm": {"type": "number"},
                    "piece": piece,
                    "from_point": {"type": "integer", "minimum": 0}, "to_point": {"type": "integer", "minimum": 0}
                }),
                &["what"],
            ),
            false,
        ),
        tool(
            "check_garment",
            "Check the garment after simulating, and call it every time you simulate: a short \
             list of problems with where each is (open seams and their gaps, strained cloth, \
             tight spots pressing on the body, cloth inside the body, a drape still moving), \
             plus the numbers. ok true means none. With view it adds a picture from that view.",
            object(json!({"view": {"type": "string", "enum": VIEWS}}), &[]),
            false,
        ),
        tool(
            "guide",
            "Short knowledge on demand: pattern making and sewing basics and how FashionCraft \
             does each. Topics: workflow, grain, ease, seam_allowance, notches, darts, \
             set_in_sleeves, sewing, simulation, fabrics, fit_checks, details, grading, \
             avatar, pleats_and_folds. An empty topic lists them.",
            object(json!({"topic": {"type": "string"}}), &[]),
            false,
        ),
    ]
}

fn number(args: &Value, key: &str) -> Option<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}

/// `simulation_settings`' changes to the solver's settings (all but the GPU switch and the
/// particle distance, which the window owns): the names of what changed. `presets` are the
/// quality presets (name, time step, iterations); `body_floor` the avatar's lowest point.
pub fn change_simulation(
    settings: &mut crate::vbd::VbdSettings,
    args: &Value,
    presets: &[(&str, f64, usize)],
    body_floor: f64,
) -> Result<Vec<&'static str>, String> {
    let mut changed = Vec::new();
    if let Some(q) = args.get("quality").and_then(Value::as_str) {
        let (_, dt, iterations) = presets
            .iter()
            .find(|(n, ..)| n.eq_ignore_ascii_case(q))
            .ok_or("quality: fast, normal or accurate")?;
        settings.dt = *dt;
        settings.iterations = *iterations;
        changed.push("quality");
    }
    if let Some(n) = args.get("iterations").and_then(Value::as_u64) {
        settings.iterations = (n as usize).clamp(2, 128);
        changed.push("iterations");
    }
    if let Some(v) = number(args, "damping") {
        settings.damping = v.clamp(0., 10.);
        changed.push("damping");
    }
    if let Some(v) = number(args, "friction") {
        settings.friction = v.clamp(0., 1.5);
        changed.push("friction");
    }
    if let Some(v) = number(args, "collision_distance_mm") {
        settings.contact.radius = v.clamp(0.5, 20.) / 1000.;
        changed.push("collision distance");
    }
    if let Some(v) = number(args, "gravity_m_s2") {
        settings.gravity = glam::DVec3::new(0., -v, 0.);
        changed.push("gravity");
    }
    if let Some(speed) = number(args, "wind_m_s") {
        if speed <= 0. {
            settings.wind = None;
        } else {
            // From 0° = from the front (+z), as the panel's wind direction.
            let from = number(args, "wind_from_degrees").unwrap_or(0.).to_radians();
            let velocity = glam::DVec3::new(-from.sin() * speed, 0., -from.cos() * speed);
            let mut wind = settings.wind.clone().unwrap_or(crate::vbd::Wind {
                velocity,
                drag: 1.2,
                density: 1.2,
            });
            wind.velocity = velocity;
            settings.wind = Some(wind);
        }
        changed.push("wind");
    }
    if let Some(on) = args.get("floor").and_then(Value::as_bool) {
        settings.floor = on.then(|| {
            if body_floor.is_finite() {
                body_floor
            } else {
                0.
            }
        });
        changed.push("floor");
    }
    if let Some(h) = number(args, "floor_height_cm") {
        settings.floor = Some(h / 100.);
        changed.push("floor height");
    }
    Ok(changed)
}

/// `avatar_maker`'s body: a typical one for the gender and age (or `current`, or a random
/// realistic one), with the settings and local measures the agent gave, checked.
pub fn macro_body(
    args: &Value,
    current: crate::avatar_maker::MacroBody,
) -> Result<crate::avatar_maker::MacroBody, String> {
    use crate::avatar_maker::{MEASURES, MacroBody};
    let gender = number(args, "gender").unwrap_or(current.gender);
    let age = number(args, "age_years").unwrap_or(current.age_years);
    let mut body = match args
        .get("start")
        .and_then(Value::as_str)
        .unwrap_or("typical")
    {
        "typical" => MacroBody::typical(gender, age),
        "current" => current,
        "random_realistic" => MacroBody::typical(gender, age)
            .realistic(args.get("seed").and_then(Value::as_u64).unwrap_or(1))?,
        other => {
            return Err(format!(
                "start '{other}': typical, current or random_realistic"
            ));
        }
    };
    for (key, field) in [
        ("gender", &mut body.gender),
        ("age_years", &mut body.age_years),
        ("muscle", &mut body.muscle),
        ("weight", &mut body.weight),
        ("height", &mut body.height),
        ("proportions", &mut body.proportions),
        ("cupsize", &mut body.cupsize),
        ("firmness", &mut body.firmness),
        ("african", &mut body.african),
        ("asian", &mut body.asian),
        ("european", &mut body.european),
    ] {
        if let Some(v) = number(args, key) {
            *field = v;
        }
    }
    if let Some(measures) = args.get("measures").and_then(Value::as_object) {
        for (name, value) in measures {
            let k = MEASURES
                .iter()
                .position(|(m, _)| m == name)
                .ok_or_else(|| {
                    let names: Vec<&str> = MEASURES.iter().map(|(n, _)| *n).collect();
                    format!("No measure '{name}'. They are: {}", names.join(", "))
                })?;
            body.measures[k] = value.as_f64().unwrap_or(0.).clamp(-1., 1.);
        }
    }
    body.validate()?;
    Ok(body)
}

/// `pose_joints`' change to a pose: every joint cleared first when `reset`, then each named
/// bone turned ([x, y, z] degrees; all zero removes it). `bones` are the rig's bone names; the
/// legacy names work on every rig.
pub fn change_pose(
    pose: &mut crate::avatar_rig::Pose,
    joints: Option<&serde_json::Map<String, Value>>,
    reset: bool,
    bones: &[&str],
) -> Result<(), String> {
    if reset {
        pose.0.clear();
    }
    for (bone, angles) in joints.into_iter().flatten() {
        if !bones.contains(&bone.as_str())
            && !crate::avatar_rig::legacy_bone_names().any(|b| b == bone)
        {
            return Err(format!(
                "No bone '{bone}': pose_joints with list true names them"
            ));
        }
        let a = angles
            .as_array()
            .filter(|a| a.len() == 3)
            .and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?]))
            .filter(|a| a.iter().all(|v| v.is_finite()))
            .ok_or("Angles are [x, y, z] degrees")?;
        if a.iter().any(|v| v.abs() > 360.) {
            return Err("Angles are at most 360 degrees".into());
        }
        if a == [0.; 3] {
            pose.0.remove(bone);
        } else {
            pose.0.insert(bone.clone(), a);
        }
    }
    Ok(())
}

/// `measure_fabric`'s measurements: the Measure fabric window's defaults for what the agent
/// left out (strip and gauge sizes), the rest as given, checked.
pub fn fabric_measurements(args: &Value) -> Result<crate::fabric_kit::Measurements, String> {
    use crate::fabric_kit::Measurements;
    let defaults = serde_json::to_value(Measurements::default()).map_err(|e| e.to_string())?;
    let mut given = defaults.as_object().cloned().unwrap_or_default();
    for (k, v) in args.as_object().into_iter().flatten() {
        if k != "name" && k != "pieces" {
            given.insert(k.clone(), v.clone());
        }
    }
    let m: Measurements = serde_json::from_value(Value::Object(given))
        .map_err(|e| format!("The measurements are wrong: {e}"))?;
    m.validate()?;
    if m.warp.is_empty() && m.weft.is_empty() {
        return Err("Give the strip test along warp or weft: [{load_g, length_mm}, …]".into());
    }
    Ok(m)
}

/// The base tools that change nothing in the document and need no window state beyond it.
pub fn answer_without_window(name: &str, args: &Value) -> Option<crate::agent::ToolResult> {
    use crate::agent::ToolResult;
    match name {
        "guide" => Some(ToolResult::text(crate::agent_guide::guide(
            args.get("topic").and_then(Value::as_str).unwrap_or(""),
        ))),
        "edit_ops" => {
            let names: Vec<String> = args
                .get("names")
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(|n| n.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            Some(ToolResult::json(&crate::agent_edit::describe(
                args.get("query").and_then(Value::as_str).unwrap_or(""),
                &names,
            )))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_base_tool_is_listed_once_with_an_object_schema() {
        let all = crate::agent::tools();
        let mut names: Vec<&str> = all.iter().map(|t| t.name).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "a tool is listed twice");
        for t in tools() {
            assert!(names.contains(&t.name));
            assert_eq!(t.input_schema["type"], "object", "{}", t.name);
            assert!(t.description.len() > 40, "{}", t.name);
        }
        assert!(answer_without_window("guide", &json!({"topic": "darts"})).is_some());
        let ops = answer_without_window("edit_ops", &json!({"names": ["SetSeamZipper"]})).unwrap();
        assert!(ops.joined_text().contains("SeamZipper"));
    }

    #[test]
    fn simulation_settings_change_only_what_is_given() {
        let mut s = crate::vbd::VbdSettings::default();
        let presets = [("Fast", 1. / 60., 8), ("Accurate", 1. / 120., 32)];
        let before = s.clone();
        let changed = change_simulation(
            &mut s,
            &json!({"quality": "accurate", "friction": 0.5, "wind_m_s": 3, "wind_from_degrees": 90,
                    "collision_distance_mm": 4, "floor": true}),
            &presets,
            -0.01,
        )
        .unwrap();
        assert_eq!(changed.len(), 5);
        assert_eq!((s.iterations, s.friction), (32, 0.5));
        assert!((s.dt - 1. / 120.).abs() < 1e-12 && (s.contact.radius - 0.004).abs() < 1e-12);
        let wind = s.wind.as_ref().unwrap().velocity;
        assert!(
            (wind.x + 3.).abs() < 1e-9 && wind.z.abs() < 1e-9,
            "from the avatar's left"
        );
        assert_eq!(s.floor, Some(-0.01));
        assert_eq!(s.gravity, before.gravity);
        assert!(change_simulation(&mut s, &json!({"quality": "best"}), &presets, 0.).is_err());
    }

    #[test]
    fn the_avatar_maker_takes_settings_and_named_measures() {
        let current = crate::avatar_maker::MacroBody::default();
        let body = macro_body(
            &json!({"gender": 1, "age_years": 40, "muscle": 0.7, "measures": {"waist-circ": 0.5}}),
            current,
        )
        .unwrap();
        assert_eq!((body.gender, body.age_years, body.muscle), (1., 40., 0.7));
        let k = crate::avatar_maker::MEASURES
            .iter()
            .position(|(n, _)| *n == "waist-circ")
            .unwrap();
        assert_eq!(body.measures[k], 0.5);
        assert!(macro_body(&json!({"measures": {"tail": 1}}), current).is_err());
        assert!(macro_body(&json!({"age_years": 500}), current).is_err());
    }

    #[test]
    fn pose_joints_turn_known_bones_only() {
        let mut pose = crate::avatar_rig::Pose(Default::default());
        let joints = json!({"upper_arm.L": [0, 0, -60], "forearm.L": [0, 0, 0]});
        change_pose(&mut pose, joints.as_object(), false, &[]).unwrap();
        assert_eq!(pose.0.get("upper_arm.L"), Some(&[0., 0., -60.]));
        assert!(!pose.0.contains_key("forearm.L"));
        let bad = json!({"wing": [0, 0, 1]});
        assert!(change_pose(&mut pose, bad.as_object(), false, &[]).is_err());
        let wild = json!({"head": [0, 720, 0]});
        assert!(change_pose(&mut pose, wild.as_object(), false, &[]).is_err());
        change_pose(&mut pose, None, true, &[]).unwrap();
        assert!(pose.0.is_empty());
    }

    #[test]
    fn measured_fabric_needs_a_strip_test_and_keeps_the_window_defaults() {
        let m = fabric_measurements(&json!({
            "gsm": 210, "thickness_mm": 0.45, "name": "Denim", "pieces": [],
            "warp": [{"load_g": 500, "length_mm": 101.2}, {"load_g": 1000, "length_mm": 102.1}]
        }))
        .unwrap();
        assert_eq!((m.gsm, m.warp.len(), m.gauge_mm), (210., 2, 100.));
        assert!(fabric_measurements(&json!({"gsm": 210, "thickness_mm": 0.4})).is_err());
        assert!(
            fabric_measurements(&json!({"gsm": 210, "thickness_mm": 0.4, "colour": 1})).is_err()
        );
    }
}
