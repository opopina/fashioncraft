//! AI agents in FashionCraft: the tools an agent can call, the local connection to an open
//! FashionCraft window, and the MCP server that `fashioncraft-cli mcp` runs.
//!
//! The same tools serve two kinds of agent. An MCP client (Claude Desktop, Claude Code, Cursor,
//! VS Code, Codex, or ArtCraft's `craft-ai-mcp` gateway) starts `fashioncraft-cli mcp`, which
//! forwards each call to the open window. The window's own Assistant panel calls the same tools
//! directly. Either way the work happens in the window, on its interface thread, with undo.
//!
//! The window serves agents only while the person allows it (Settings › AI agents). It then
//! listens on loopback and writes its port and a fresh token to [`CONNECTION_FILE`] in its
//! session folder; the CLI reads that file before every call, so an MCP client's configuration
//! holds no port and no token, and keeps working when FashionCraft restarts.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// The file in the session folder that says where an open window listens for agents.
pub const CONNECTION_FILE: &str = "agent-connection.json";
/// The MCP protocol version this server speaks when a client asks for one it does not know.
pub const MCP_VERSION: &str = "2025-06-18";

/// Where FashionCraft keeps its work: the platform's folder for application data, `session`.
/// The window and the CLI both find the connection file here.
pub fn session_dir() -> PathBuf {
    let base: Option<PathBuf> = if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/FashionCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("FashionCraft"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(|dir| PathBuf::from(dir).join("fashioncraft"))
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local/share/fashioncraft"))
            })
    };
    base.unwrap_or_else(|| PathBuf::from("FashionCraft"))
        .join("session")
}

/// Where an open window listens for agents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
}

impl Connection {
    /// Writes the file readable by its owner only (on Unix), replacing any older one.
    pub fn write(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(CONNECTION_FILE);
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())
    }
    pub fn read(dir: &Path) -> Option<Self> {
        serde_json::from_str(&std::fs::read_to_string(dir.join(CONNECTION_FILE)).ok()?).ok()
    }
    pub fn remove(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(CONNECTION_FILE));
    }
}

/// A fresh random token, 32 hexadecimal characters.
pub fn new_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap_or_else(|_| {
        // Without the system's randomness, the clock and the process still make it unguessable
        // enough for a loopback port that only lives while the window is open.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&(nanos ^ u128::from(std::process::id())).to_le_bytes());
    });
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares two tokens in time that does not depend on where they differ.
pub fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// One piece of a tool's result, as MCP carries it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text {
        text: String,
    },
    Image {
        /// Base64, without line breaks.
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

/// What a tool call returns: its content, and whether it failed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<Content>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text { text: text.into() }],
            is_error: false,
        }
    }
    pub fn json(value: &Value) -> Self {
        Self::text(serde_json::to_string_pretty(value).unwrap_or_default())
    }
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text { text: text.into() }],
            is_error: true,
        }
    }
    /// The text parts joined, for a log or a model that takes text only.
    pub fn joined_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                Content::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A tool an agent can call: its name, what it does, and the JSON Schema of its input.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    /// Whether it changes the project (the garment, the avatar, the sizes) rather than only
    /// reading it or writing a file.
    pub changes_document: bool,
}

/// The avatar library's presets as an agent names them: "Woman · medium" is woman-medium.
pub fn avatar_preset_ids() -> Vec<String> {
    crate::avatar_look::PRESETS
        .iter()
        .map(|p| avatar_preset_id(p.name))
        .collect()
}
pub fn avatar_preset_id(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The garment bases draft_garment knows.
pub const GARMENT_KINDS: [&str; 5] = ["tshirt", "shirt", "dress", "skirt", "trousers"];

/// Every tool, in the order clients list them.
pub fn tools() -> Vec<Tool> {
    let fabric_ids: Vec<&str> = crate::fabrics::PRESETS.iter().map(|p| p.id).collect();
    let pieces = json!({
        "type": "array",
        "items": {"type": "string"},
        "description": "Piece ids (or names) from scene_summary. Leave out for every piece."
    });
    let piece =
        json!({"type": "string", "description": "A piece id (or name) from scene_summary."});
    let segment = json!({"type": "string", "description": "A segment id of that piece, from scene_summary or piece_detail."});
    let stretch = json!({
        "type": "object",
        "properties": {
            "piece": {"type": "string"},
            "segment": {"type": "string"},
            "from_cm": {"type": "number", "description": "Sew only from here, cm from the segment's start."},
            "to_cm": {"type": "number", "description": "…to here."}
        },
        "required": ["piece", "segment"],
        "additionalProperties": false
    });
    let side = json!({
        "type": "array",
        "items": stretch,
        "minItems": 1,
        "description": "One segment, or several in order for M:N sewing."
    });
    let object = |properties: Value, required: &[&str]| json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false});
    let tool = |name, description, input_schema, changes_document| Tool {
        name,
        description,
        input_schema,
        changes_document,
    };
    vec![
        tool(
            "scene_summary",
            "Read the open project: every piece (id, name, size, segments with ids, lengths and \
             which side they face, internal lines, placement), every seam (both sides' lengths, \
             and after a simulation its gap and whether it closed), fabrics with their physical \
             parameters, the avatar's measurements and pose, sizes and grading, the current \
             selection, warnings and the simulation state. Call it first and after changes.",
            object(json!({}), &[]),
            false,
        ),
        tool(
            "piece_detail",
            "One piece in full: its numbered outline points (x right, y down, cm), each segment \
             with its end points, length, curve, target length, seam allowance, notches and \
             seams, its internal lines and its placement. Use it before editing a piece.",
            object(json!({"piece": piece}), &["piece"]),
            false,
        ),
        tool(
            "select",
            "Select in the window, as a click would: pieces, one segment, one outline point or \
             one seam (clear to select nothing). Menu commands act on the selection.",
            object(
                json!({
                    "pieces": pieces,
                    "piece": piece,
                    "segment": {"type": "string", "description": "A segment or internal line of `piece`."},
                    "point": {"type": "integer", "description": "An outline point index of `piece`."},
                    "seam": {"type": "string"},
                    "clear": {"type": "boolean"}
                }),
                &[],
            ),
            false,
        ),
        tool(
            "command_list",
            "List FashionCraft's menu commands an agent can run: key, label, what selection it \
             needs and whether it can run now. Commands that need the mouse or open a window or \
             file dialog are left out unless include_interactive is true. Filter with a few \
             words, such as 'view', 'sewing' or 'pleat'.",
            object(
                json!({
                    "query": {"type": "string", "description": "Words to filter by."},
                    "include_interactive": {"type": "boolean"}
                }),
                &[],
            ),
            false,
        ),
        tool(
            "command_run",
            "Run one menu command by its key from command_list (for example 'view.back', \
             'edit.undo', 'sewing.auto', 'pattern.smooth_curve'). Give `target` to select first, \
             with the same fields as select. Commands that need the mouse or open a window or \
             dialog are refused with what to use instead.",
            object(
                json!({
                    "key": {"type": "string"},
                    "target": {"type": "object", "description": "What to select first: pieces, piece, segment, point or seam."}
                }),
                &["key"],
            ),
            true,
        ),
        tool(
            "draft_garment",
            "Replace the garment with a base drafted from measurements, sewn and placed around \
             the avatar: tshirt (kimono sleeves), shirt (set-in sleeves), dress (sleeveless \
             A-line), skirt or trousers. With from_avatar (default) the measurements come from \
             the avatar; measurements_cm overrides any of them and the reply lists them all. \
             Unsaved work is replaced: save_project first if it matters.",
            object(
                json!({
                    "kind": {"type": "string", "enum": GARMENT_KINDS},
                    "from_avatar": {"type": "boolean", "description": "Measure the avatar first (default true)."},
                    "measurements_cm": {
                        "type": "object",
                        "additionalProperties": {"type": "number"},
                        "description": "Measurements in centimetres, by the names draft_garment reports."
                    }
                }),
                &["kind"],
            ),
            true,
        ),
        tool(
            "create_piece",
            "Add a new piece from a rectangle or an outline of points (x right, y down, cm). It \
             appears beside the garment; then place_piece and sew it. Segments are named \
             edge_1, edge_2… in point order (a rectangle: edge_1 top, edge_2 right, edge_3 \
             bottom, edge_4 left).",
            object(
                json!({
                    "name": {"type": "string"},
                    "rectangle_cm": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "[width, height]"},
                    "points_cm": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "minItems": 3, "maxItems": 64},
                    "fabric": {"type": "string", "description": "A fabric id; the first piece's by default."}
                }),
                &[],
            ),
            true,
        ),
        tool(
            "move_points",
            "Move outline points of a piece by an offset in cm (x right, y down on the board). \
             Point indexes come from piece_detail. A sewn segment must keep the length of the \
             segment it is sewn to unless its seam allows ease.",
            object(
                json!({
                    "piece": piece,
                    "points": {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 1},
                    "dx_cm": {"type": "number"},
                    "dy_cm": {"type": "number"}
                }),
                &["piece", "points"],
            ),
            true,
        ),
        tool(
            "set_segment_length",
            "Make a straight segment exactly this long, keeping its start, end or middle in \
             place (start by default).",
            object(
                json!({
                    "piece": piece,
                    "segment": segment,
                    "length_cm": {"type": "number", "minimum": 0.1},
                    "keep": {"type": "string", "enum": ["start", "end", "middle"]}
                }),
                &["piece", "segment", "length_cm"],
            ),
            true,
        ),
        tool(
            "add_point",
            "Add an outline point on an unsewn segment, splitting it in two (at its middle \
             unless at_cm, measured from the segment's start).",
            object(
                json!({"piece": piece, "segment": segment, "at_cm": {"type": "number"}}),
                &["piece", "segment"],
            ),
            true,
        ),
        tool(
            "add_dart",
            "Add a dart. With a segment: an edge dart cut into that straight, unsewn segment and \
             sewn shut, width_cm wide at the edge (centred at at_cm, the middle by default) and \
             depth_cm long into the piece. Without a segment: a diamond dart inside the piece, \
             centred at centre_cm [x, y], width_cm wide and depth_cm tall.",
            object(
                json!({
                    "piece": piece,
                    "segment": segment,
                    "at_cm": {"type": "number"},
                    "centre_cm": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2},
                    "width_cm": {"type": "number"},
                    "depth_cm": {"type": "number"}
                }),
                &["piece", "width_cm", "depth_cm"],
            ),
            true,
        ),
        tool(
            "add_pleats",
            "Fold pleats along a segment: count pleats depth_cm deep, knife, box or inverted \
             box, tacked shut at the edge unless tack is false.",
            object(
                json!({
                    "piece": piece,
                    "segment": segment,
                    "count": {"type": "integer", "minimum": 1, "maximum": 40},
                    "depth_cm": {"type": "number"},
                    "kind": {"type": "string", "enum": ["knife", "box", "inverted_box"]},
                    "tack": {"type": "boolean"}
                }),
                &["piece", "segment", "depth_cm"],
            ),
            true,
        ),
        tool(
            "piece_edit",
            "Change a piece: rename it, duplicate it, make a mirrored copy linked to it (the \
             other side of a symmetric pair), delete it with its seams, set its grain angle, or \
             set the seam allowance of one segment (drafting only, not simulated).",
            object(
                json!({
                    "piece": piece,
                    "operation": {"type": "string", "enum": ["rename", "duplicate", "mirror_copy", "delete", "set_grain", "set_seam_allowance"]},
                    "name": {"type": "string"},
                    "degrees": {"type": "number"},
                    "segment": segment,
                    "width_cm": {"type": "number"}
                }),
                &["piece", "operation"],
            ),
            true,
        ),
        tool(
            "sew",
            "Sew side a to side b. Each side is one segment or several in order (M:N), whole or \
             a stretch (from_cm/to_cm). Unless reverse is given, the ends that are nearer where \
             the pieces are placed meet, so the seam does not twist. Sides of different lengths \
             need ease: true (gathering or easing).",
            object(
                json!({"a": side, "b": side, "reverse": {"type": "boolean"}, "ease": {"type": "boolean"}}),
                &["a", "b"],
            ),
            true,
        ),
        tool(
            "seam_edit",
            "Remove a seam, or allow or forbid ease on it (unequal side lengths).",
            object(
                json!({
                    "seam": {"type": "string"},
                    "operation": {"type": "string", "enum": ["remove", "allow_ease", "forbid_ease"]}
                }),
                &["seam", "operation"],
            ),
            true,
        ),
        tool(
            "place_piece",
            "Place a piece on the avatar at an arrangement point (scene_summary lists them, \
             such as torso-middle-front or upper-arm-left-outer), clearance_cm from the skin, \
             wrapped around the body unless wrap is false. It is checked to start clear of the \
             body. Simulate afterwards.",
            object(
                json!({
                    "piece": piece,
                    "anchor": {"type": "string"},
                    "clearance_cm": {"type": "number", "minimum": 0, "maximum": 60},
                    "wrap": {"type": "boolean"}
                }),
                &["piece", "anchor"],
            ),
            true,
        ),
        tool(
            "set_fabric",
            "Give pieces a fabric preset: its weight, thickness, stretch and bending \
             (indicative values, not calibrated on a real fabric).",
            object(
                json!({"preset": {"type": "string", "enum": fabric_ids}, "pieces": pieces}),
                &["preset"],
            ),
            true,
        ),
        tool(
            "set_fabric_parameters",
            "Change the physical parameters of one fabric (named, or the one a piece is cut \
             from); every piece of that fabric changes. Weight g/m², thickness mm, stretch in % \
             under 5 N on a 5 cm strip along warp or weft, shear N/m, bending N·m, friction.",
            object(
                json!({
                    "fabric": {"type": "string"},
                    "piece": piece,
                    "weight_gsm": {"type": "number"},
                    "thickness_mm": {"type": "number"},
                    "stretch_warp_percent": {"type": "number"},
                    "stretch_weft_percent": {"type": "number"},
                    "shear_n_per_m": {"type": "number"},
                    "bending_warp_n_m": {"type": "number"},
                    "bending_weft_n_m": {"type": "number"},
                    "friction": {"type": "number"}
                }),
                &[],
            ),
            true,
        ),
        tool(
            "set_color",
            "Colour the fabric of pieces, as sRGB 0-255. Every piece of the same fabric changes.",
            object(
                json!({
                    "rgb": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "minItems": 3, "maxItems": 3},
                    "pieces": pieces
                }),
                &["rgb"],
            ),
            true,
        ),
        tool(
            "set_avatar",
            "Replace the avatar: a body from the avatar library, or the current body reshaped to \
             measurements in cm (height, bust, waist, hip, arm, inseam; the rest stay). The \
             drape is reset: simulate again. Not undoable with edit.undo.",
            object(
                json!({
                    "preset": {"type": "string", "enum": avatar_preset_ids()},
                    "measurements_cm": {
                        "type": "object",
                        "properties": {
                            "height": {"type": "number"}, "bust": {"type": "number"},
                            "waist": {"type": "number"}, "hip": {"type": "number"},
                            "arm": {"type": "number"}, "inseam": {"type": "number"}
                        },
                        "additionalProperties": false
                    }
                }),
                &[],
            ),
            true,
        ),
        tool(
            "set_pose",
            "Pose the avatar with a preset; the garment follows when simulated.",
            object(
                json!({"pose": {"type": "string", "enum": crate::avatar_rig::PRESETS}}),
                &["pose"],
            ),
            true,
        ),
        tool(
            "grading",
            "Sizes: create a size range from the base the pattern is drawn in, add a size, grow \
             an outline point by an equal step between sizes (dx/dy mm per size, x right, y \
             down; the point is the start of `segment`), or show a size in 3D. size_chart \
             reports every point of measure in every size.",
            object(
                json!({
                    "action": {"type": "string", "enum": ["create", "add_size", "set_step", "show"]},
                    "sizes": {"type": "array", "items": {"type": "string"}, "description": "create: smallest to largest."},
                    "base": {"type": "string", "description": "create: the size the pattern is drawn in."},
                    "size": {"type": "string", "description": "add_size: its name; show: which."},
                    "after": {"type": "string", "description": "add_size: the size it follows (the largest by default)."},
                    "piece": piece,
                    "segment": segment,
                    "dx_mm": {"type": "number"},
                    "dy_mm": {"type": "number"}
                }),
                &["action"],
            ),
            true,
        ),
        tool(
            "size_chart",
            "The garment's sizes and points of measure: each size group (its sizes \
             smallest first, the base size, the size shown in 3D) and every point of measure in \
             every size, in mm, with its difference from the base and its tolerance.",
            object(json!({}), &[]),
            false,
        ),
        tool(
            "simulate",
            "Run the cloth simulation for some seconds of wall-clock time, so the garment drapes \
             on the avatar, then pause it. Reports each seam's gap and what still looks wrong.",
            object(
                json!({"seconds": {"type": "number", "minimum": 0.5, "maximum": 60}}),
                &["seconds"],
            ),
            true,
        ),
        tool(
            "screenshot",
            "A PNG picture of the 3D view as it is now, to look at the garment; or of the whole \
             window, panels included. Turn the view first with command_run 'view.front', \
             'view.back', 'view.avatar_left'…",
            object(
                json!({"area": {"type": "string", "enum": ["view", "window"],
                    "description": "The 3D view (the default) or the whole window."}}),
                &[],
            ),
            false,
        ),
        tool(
            "render",
            "A photoreal, path-traced picture of the 3D view, saved as <name>.png in \
             Documents/FashionCraft. It refines for up to `seconds` (default 20). Uses the GPU.",
            object(
                json!({
                    "name": {"type": "string", "description": "A file name without folders or extension."},
                    "seconds": {"type": "number", "minimum": 3, "maximum": 120}
                }),
                &["name"],
            ),
            false,
        ),
        tool(
            "save_project",
            "Save the project as <name>.json in Documents/FashionCraft (the folder is made if \
             needed). Reports the path.",
            object(
                json!({"name": {"type": "string", "description": "A file name without folders."}}),
                &["name"],
            ),
            false,
        ),
        tool(
            "export_garment",
            "Export the draped garment for other 3D tools into Documents/FashionCraft: obj \
             (with its materials) or glb (glTF binary). Reports the path.",
            object(
                json!({
                    "format": {"type": "string", "enum": ["obj", "glb"]},
                    "name": {"type": "string", "description": "A file name without folders or extension."}
                }),
                &["format", "name"],
            ),
            false,
        ),
    ]
}

/// Standard base64 with padding, for images in tool results.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes [`base64`]'s output (padding optional); None for anything else.
pub fn unbase64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let digits = text.trim_end_matches('=').as_bytes();
    if digits.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut n = 0;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

/// A file name an agent gave, checked: no folders, no leading dot, a sane length.
pub fn plain_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 100
        || name.starts_with('.')
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
    {
        return Err(format!("'{name}' is not a plain file name"));
    }
    Ok(name.to_owned())
}

/// The folder agents save and export into: Documents/FashionCraft.
pub fn documents_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("FashionCraft")
}

/// A request on the window's loopback connection, one JSON object per line.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkRequest {
    pub token: String,
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// The window's answer to a [`LinkRequest`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkResponse {
    pub id: u64,
    pub result: ToolResult,
}

/// How the MCP server reaches the open window.
pub trait AppLink {
    fn call(&mut self, name: &str, arguments: &Value) -> ToolResult;
}

/// Calls the open window over loopback, reading the connection file before every call.
pub struct WindowLink {
    pub dir: PathBuf,
    next: u64,
}

impl WindowLink {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, next: 1 }
    }
}

impl AppLink for WindowLink {
    fn call(&mut self, name: &str, arguments: &Value) -> ToolResult {
        let Some(connection) = Connection::read(&self.dir) else {
            return ToolResult::error(
                "FashionCraft is not open, or AI agents are off. Open FashionCraft and turn on \
                 Settings › AI agents › Let AI agents use FashionCraft, then try again.",
            );
        };
        let id = self.next;
        self.next += 1;
        let request = LinkRequest {
            token: connection.token,
            id,
            name: name.to_owned(),
            arguments: arguments.clone(),
        };
        match call_window(connection.port, &request) {
            Ok(response) if response.id == id => response.result,
            Ok(_) => ToolResult::error("FashionCraft answered a different request"),
            Err(e) => ToolResult::error(format!(
                "Cannot reach the FashionCraft window ({e}). Is it still open?"
            )),
        }
    }
}

fn call_window(port: u16, request: &LinkRequest) -> Result<LinkResponse, String> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let stream = std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    // A simulation can take a minute; the window answers when the work is done.
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(600)))
        .map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut line = serde_json::to_string(request).map_err(|e| e.to_string())?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(stream);
    let mut answer = String::new();
    reader.read_line(&mut answer).map_err(|e| e.to_string())?;
    serde_json::from_str(&answer).map_err(|e| format!("unreadable answer: {e}"))
}

/// Answers one MCP JSON-RPC message; None for a notification, which gets no answer.
pub fn handle_mcp(message: &Value, app: &mut dyn AppLink) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(MCP_VERSION);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "fashioncraft", "version": env!("CARGO_PKG_VERSION")},
                "instructions": crate::assistant::SYSTEM_PROMPT
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools().iter().map(|t| json!({
            "name": t.name,
            "description": t.description,
            "inputSchema": t.input_schema,
            "annotations": {"readOnlyHint": !t.changes_document}
        })).collect::<Vec<_>>()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            if !tools().iter().any(|t| t.name == name) {
                Err((-32602, format!("unknown tool '{name}'")))
            } else {
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                serde_json::to_value(app.call(name, &arguments))
                    .map_err(|e| (-32603, e.to_string()))
            }
        }
        _ => Err((-32601, format!("method '{method}' not found"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
        }
    })
}

/// MCP over stdio: one JSON-RPC message per line in, one per line out, until input ends.
pub fn serve_mcp(input: impl BufRead, mut output: impl Write, app: &mut dyn AppLink) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle_mcp(&message, app),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": format!("parse error: {e}")}
            })),
        };
        if let Some(answer) = answer {
            let _ = writeln!(output, "{answer}");
            let _ = output.flush();
        }
    }
}
