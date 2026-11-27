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
pub const DETAILS: [&str; 4] = ["patch_pocket", "band_collar", "placket", "ruffle"];

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
             any order, the first where the collar starts; height_cm), placket (piece, segment, width_cm), \
             ruffle (segments: a hem's segments, front and back, the first where it starts; \
             height_cm deep, fullness 1.6 times the hem by default; gathered, hanging below the \
             hem: no place_piece). \
             guide('details') has typical sizes.",
            object(
                json!({
                    "kind": {"type": "string", "enum": DETAILS},
                    "piece": piece, "segment": segment,
                    "segments": {"type": "array", "items": object(json!({"piece": {"type": "string"}, "segment": {"type": "string"}}), &["piece", "segment"])},
                    "width_cm": {"type": "number"}, "height_cm": {"type": "number"},
                    "centre_cm": xy("[x, y] on the piece"),
                    "fullness": {"type": "number", "minimum": 1, "maximum": 3},
                    "name": {"type": "string"}
                }),
                &["kind"],
            ),
            true,
        ),
        tool(
            "set_sleeve",
            "Set a sleeve into its armhole exactly, as draft_garment's shirt does: the cap sewn \
             to the front and back armholes with ease (gathered when much longer: a puffed \
             sleeve), the underarm seam closed, and the sleeve wrapped round the avatar's arm \
             just below the shoulder, clear of the body. Seams already on those segments are \
             replaced. Never move a sleeve into place by hand. The sleeve's outline has its cap \
             at the top and its hem at the bottom; side is the avatar's own left or right.",
            object(
                json!({
                    "sleeve": piece,
                    "side": {"type": "string", "enum": ["left", "right"]},
                    "cap": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 4,
                        "description": "The sleeve's cap segments: two halves meeting at the top, or one."},
                    "underarm": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 2,
                        "description": "The sleeve's two side segments, sewn into a tube."},
                    "armhole": {"type": "array", "items": object(json!({"piece": {"type": "string"}, "segment": {"type": "string"}}), &["piece", "segment"]),
                        "minItems": 1, "maxItems": 4, "description": "The bodice's armhole segments on that side (front and back)."},
                    "ease": {"type": "boolean", "description": "Allow the cap to be longer (default true)."}
                }),
                &["sleeve", "side", "cap", "armhole"],
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
             everything). With screenshot true it returns a picture of the view afterwards (see \
             screenshot for when one is worth it).",
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
             cupsize, firmness (settings from 0 to 1, not centimetres: for a height in cm give \
             height_cm, which finds the setting; every answer says the body's height_cm), \
             african/asian/european shares, measures (local \
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
                    "height": {"type": "number", "description": "0 shortest to 1 tallest for the gender and age; not cm"},
                    "height_cm": {"type": "number", "minimum": 50, "maximum": 250, "description": "The body's height in cm: sets height to reach it, with the other settings as given"},
                    "proportions": {"type": "number"},
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
             plus the numbers. ok true means none. With view it adds a picture from that view: only \
             when the result needs looking at, not on every check.",
            object(
                json!({
                    "view": {"type": "string", "enum": VIEWS},
                    "details": {"type": "boolean", "description": "Every number: seam gaps, strain and pressure spots with their locations, settling. Otherwise ok, the problems and the counts."}
                }),
                &[],
            ),
            false,
        ),
        tool(
            "plan",
            "Show the person your plan for a job of several steps, as a short checklist in \
             their words, and call it again as steps finish (done: how many are done, from the \
             first). Not for a small change.",
            object(
                json!({
                    "steps": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 12},
                    "done": {"type": "integer", "minimum": 0}
                }),
                &["steps"],
            ),
            false,
        ),
        tool(
            "project_notes",
            "Notes kept with this project and read at the start of every conversation; the \
             person sees and edits them. Add one when they tell you something to keep for this \
             project (who it is for, sizes, fabrics, the workshop's rules); remove one by its \
             number when it no longer holds. Returns the notes, numbered from 1.",
            object(
                json!({
                    "add": {"type": "string", "description": "One short note, in the person's words."},
                    "remove": {"type": "integer", "minimum": 1}
                }),
                &[],
            ),
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

/// The tools the Assistant declares in every request: the ones most jobs need. The others are
/// listed and described by `more_tools` and run through `use_tool`, so each request carries a
/// short catalogue instead of every schema (MCP clients still see every tool).
pub const CORE: &[&str] = &[
    "scene_summary",
    "piece_detail",
    "check_garment",
    "simulate",
    "screenshot",
    "camera",
    "draft_garment",
    "create_piece",
    "piece_edit",
    "sew",
    "set_sleeve",
    "add_detail",
    "set_fabric",
    "set_color",
    "command_run",
    "edit",
    "guide",
    "plan",
];

/// `more_tools` and `use_tool`: the way to the tools outside [`CORE`].
pub fn gateway_tools() -> Vec<Tool> {
    vec![
        tool(
            "more_tools",
            "List FashionCraft's other tools (beyond the ones you have), or, with names, give \
             their full descriptions and input schemas. Run one with use_tool. They include: \
             move_points, set_segment_length, add_point, add_dart, add_pleats, seam_edit, \
             place_piece, transform_piece, bend_segment, style_line, add_internal_line, \
             add_buttons, pin, grab, trace_on_body, set_fabric_parameters, measure_fabric, \
             set_avatar, set_pose, pose_joints, avatar_maker, grading, size_chart, \
             points_of_measure, measure, simulation_settings, render_settings, render, \
             command_list, select, edit_ops, save_project, export_garment.",
            object(
                json!({"names": {"type": "array", "items": {"type": "string"}}}),
                &[],
            ),
            false,
        ),
        tool(
            "use_tool",
            "Run one of the tools more_tools describes: its name and its input.",
            object(
                json!({
                    "name": {"type": "string"},
                    "arguments": {"type": "object"}
                }),
                &["name"],
            ),
            true,
        ),
    ]
}

/// What the Assistant declares in a request: [`CORE`] and the gateway.
pub fn assistant_tools() -> Vec<Tool> {
    crate::agent::tools()
        .into_iter()
        .filter(|t| CORE.contains(&t.name))
        .chain(gateway_tools())
        .collect()
}

/// `more_tools`: every tool outside [`CORE`] in a line, or the named ones in full.
pub fn more_tools(names: &[String]) -> Value {
    let all = crate::agent::tools();
    if names.is_empty() {
        let others: Vec<Value> = all
            .iter()
            .filter(|t| !CORE.contains(&t.name))
            .map(|t| {
                let line = t.description.split(". ").next().unwrap_or(t.description);
                json!({"name": t.name, "does": line})
            })
            .collect();
        return json!({"tools": others, "note": "more_tools with names gives their inputs; use_tool runs one"});
    }
    let found: Vec<Value> = all
        .iter()
        .filter(|t| names.iter().any(|n| n == t.name))
        .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
        .collect();
    let unknown: Vec<&String> = names
        .iter()
        .filter(|n| !all.iter().any(|t| t.name == n.as_str()))
        .collect();
    json!({"tools": found, "unknown": unknown, "note": "Run one with use_tool {name, arguments}"})
}

/// The longest change a scene note tells; past it the model calls scene_summary itself.
pub const SCENE_NOTE_CHARS: usize = 2500;

/// A note for the person's next message, once the model has had a scene_summary in this
/// conversation (`last`: the conversation it was in and the summary): that nothing changed
/// since, or what did (the person may have worked by hand), so a small request needs no
/// scene_summary first. With the note, whether `now` is what the model knows from then on.
/// None before the first summary, or when the change is too long for a note.
pub fn scene_note(last: &(u64, Value), epoch: u64, now: &Value) -> Option<(String, bool)> {
    let (seen, before) = last;
    if *seen != epoch {
        return None;
    }
    if now == before {
        return Some((
            "the scene is as your last scene_summary showed it.".into(),
            false,
        ));
    }
    let changed = scene_diff(before, now).to_string();
    (changed.chars().count() <= SCENE_NOTE_CHARS).then(|| {
        (
            format!("since your last scene_summary the scene changed: {changed}"),
            true,
        )
    })
}

/// What changed between two scene summaries, for `scene_summary` after its first call: top-level
/// fields that changed (whole), and for the lists of pieces and seams only the entries added,
/// changed or removed (by id). Unchanged fields are named, not repeated.
pub fn scene_diff(before: &Value, after: &Value) -> Value {
    let mut changed = serde_json::Map::new();
    let mut same = Vec::new();
    for (key, now) in after.as_object().into_iter().flatten() {
        let was = &before[key];
        if was == now {
            same.push(key.clone());
            continue;
        }
        let by_id = |v: &Value| -> Option<Vec<(String, Value)>> {
            v.as_array()?
                .iter()
                .map(|e| Some((e.get("id")?.as_str()?.to_owned(), e.clone())))
                .collect()
        };
        match (by_id(was), by_id(now)) {
            (Some(old), Some(new)) => {
                let added: Vec<&Value> = new
                    .iter()
                    .filter(|(id, _)| !old.iter().any(|(o, _)| o == id))
                    .map(|(_, v)| v)
                    .collect();
                let removed: Vec<&String> = old
                    .iter()
                    .filter(|(id, _)| !new.iter().any(|(n, _)| n == id))
                    .map(|(id, _)| id)
                    .collect();
                let altered: Vec<&Value> = new
                    .iter()
                    .filter(|(id, v)| old.iter().any(|(o, w)| o == id && w != v))
                    .map(|(_, v)| v)
                    .collect();
                changed.insert(
                    key.clone(),
                    json!({"added": added, "changed": altered, "removed": removed,
                           "unchanged": new.len() - added.len() - altered.len()}),
                );
            }
            _ => {
                changed.insert(key.clone(), now.clone());
            }
        }
    }
    json!({
        "since_your_last_scene_summary": changed,
        "unchanged": same,
        "note": "Only what changed since your last scene_summary. If you no longer have it in \
                 full, call scene_summary with full: true.",
    })
}

/// The base tools that change nothing in the document and need no window state beyond it.
pub fn answer_without_window(name: &str, args: &Value) -> Option<crate::agent::ToolResult> {
    use crate::agent::ToolResult;
    match name {
        "more_tools" => {
            let names: Vec<String> = args
                .get("names")
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(|n| n.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            Some(ToolResult::json(&more_tools(&names)))
        }
        // The Assistant's window shows it as a checklist; the model only needs to hear it came.
        "plan" => Some(ToolResult::text(format!(
            "Plan shown to the person: {} of {} done.",
            args.get("done").and_then(Value::as_u64).unwrap_or(0),
            args.get("steps")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        ))),
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
    #[test]
    fn the_model_is_told_whether_the_scene_changed_since_it_looked() {
        let seen = json!({"pieces": [{"id": "front", "colour": "white"}, {"id": "back", "colour": "white"}],
                          "avatar": "female"});
        let last = (7, seen.clone());
        // Another conversation's summary says nothing about this one.
        assert!(scene_note(&last, 8, &seen).is_none());
        let (note, known) = scene_note(&last, 7, &seen).unwrap();
        assert!(note.contains("as your last scene_summary") && !known);
        let mut now = seen.clone();
        now["pieces"][0]["colour"] = json!("red");
        let (note, known) = scene_note(&last, 7, &now).unwrap();
        assert!(note.contains("\"red\"") && note.contains("front") && known);
        assert!(!note.contains("back"), "only what changed: {note}");
        // Too much to tell: the model looks for itself.
        now["avatar"] = json!("x".repeat(SCENE_NOTE_CHARS));
        assert!(scene_note(&last, 7, &now).is_none());
    }

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
    fn a_second_scene_summary_says_only_what_changed() {
        let before = json!({"garment_name": "Dress", "pieces": [{"id": "front", "cm": 40}, {"id": "back", "cm": 40}],
            "seams": [{"id": "s1", "gap_mm": 9}], "avatar": {"arrangement_points": ["a", "b"]}});
        let after = json!({"garment_name": "Dress", "pieces": [{"id": "front", "cm": 42}, {"id": "back", "cm": 40}, {"id": "sleeve", "cm": 30}],
            "seams": [], "avatar": {"arrangement_points": ["a", "b"]}});
        let d = scene_diff(&before, &after);
        let pieces = &d["since_your_last_scene_summary"]["pieces"];
        assert_eq!(pieces["added"][0]["id"], "sleeve");
        assert_eq!(pieces["changed"][0]["id"], "front");
        assert_eq!(pieces["unchanged"], 1);
        assert_eq!(
            d["since_your_last_scene_summary"]["seams"]["removed"][0],
            "s1"
        );
        assert!(
            d["unchanged"]
                .as_array()
                .unwrap()
                .contains(&json!("avatar"))
        );
        assert!(
            serde_json::to_string(&d).unwrap().len()
                < serde_json::to_string(&after).unwrap().len() + 300
        );
    }

    #[test]
    fn the_assistant_declares_a_short_catalogue_and_reaches_the_rest() {
        let declared = assistant_tools();
        assert!(declared.len() <= CORE.len() + 2);
        assert!(declared.iter().any(|t| t.name == "use_tool"));
        let size = |tools: &[Tool]| {
            tools
                .iter()
                .map(|t| t.description.len() + t.input_schema.to_string().len())
                .sum::<usize>()
        };
        assert!(
            size(&declared) * 2 < size(&crate::agent::tools()),
            "the short list is less than half"
        );
        let more = more_tools(&[]);
        assert!(
            more["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "pin")
        );
        let pin = more_tools(&["pin".into()]);
        assert_eq!(pin["tools"][0]["input_schema"]["type"], "object");
        // Every tool named in more_tools' description exists.
        let all: Vec<&str> = crate::agent::tools().iter().map(|t| t.name).collect();
        let gateway = gateway_tools();
        for word in gateway[0]
            .description
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        {
            if word.contains('_') && word != "use_tool" {
                assert!(all.contains(&word), "{word}");
            }
        }
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
