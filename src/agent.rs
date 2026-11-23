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
    /// Whether it changes the document (undoable) rather than only reading it.
    pub changes_document: bool,
}

/// Every tool, in the order clients list them.
pub fn tools() -> Vec<Tool> {
    let fabric_ids: Vec<&str> = crate::fabrics::PRESETS.iter().map(|p| p.id).collect();
    let pieces = json!({
        "type": "array",
        "items": {"type": "string"},
        "description": "Piece ids from scene_summary. Leave out for every piece."
    });
    vec![
        Tool {
            name: "scene_summary",
            description: "Describe what is open in FashionCraft: the pieces (id, name, size in cm, \
                where they are placed), the seams, the fabrics and colours, the avatar, and \
                whether the simulation is running. Call it first, and after changes.",
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            changes_document: false,
        },
        Tool {
            name: "command_list",
            description: "List FashionCraft's menu commands (key, label, whether it can run \
                now). Filter with a few words, such as 'view', 'sewing' or 'export'.",
            input_schema: json!({
                "type": "object",
                "properties": {"query": {"type": "string", "description": "Words to filter by."}},
                "additionalProperties": false
            }),
            changes_document: false,
        },
        Tool {
            name: "command_run",
            description: "Run one menu command by its key from command_list, as if chosen from \
                the menu (for example 'view.back', 'edit.undo', 'display.arrangement_points'). \
                Commands that open a dialog are refused: use save_project, export_garment or \
                draft_garment.",
            input_schema: json!({
                "type": "object",
                "properties": {"key": {"type": "string"}},
                "required": ["key"],
                "additionalProperties": false
            }),
            changes_document: true,
        },
        Tool {
            name: "draft_garment",
            description: "Replace the garment with one drafted from measurements: a T-shirt, a \
                skirt or trousers, sewn and placed around the avatar. With from_avatar the \
                measurements come from the avatar; measurements_cm overrides any of them \
                (T-shirt: bust, hip, ease, length, shoulder, sleeve, cuff; skirt and trousers: \
                waist, hip, length and the rest that draft_garment reports back).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["tshirt", "skirt", "trousers"]},
                    "from_avatar": {"type": "boolean", "description": "Measure the avatar first (default true)."},
                    "measurements_cm": {
                        "type": "object",
                        "additionalProperties": {"type": "number"},
                        "description": "Measurements in centimetres, by name."
                    }
                },
                "required": ["kind"],
                "additionalProperties": false
            }),
            changes_document: true,
        },
        Tool {
            name: "set_fabric",
            description: "Give pieces a fabric preset: its weight, thickness, stretch and \
                bending (indicative values, not calibrated on a real fabric).",
            input_schema: json!({
                "type": "object",
                "properties": {"preset": {"type": "string", "enum": fabric_ids}, "pieces": pieces},
                "required": ["preset"],
                "additionalProperties": false
            }),
            changes_document: true,
        },
        Tool {
            name: "set_color",
            description: "Colour the fabric of pieces, as sRGB 0-255.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "rgb": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "minItems": 3, "maxItems": 3},
                    "pieces": pieces
                },
                "required": ["rgb"],
                "additionalProperties": false
            }),
            changes_document: true,
        },
        Tool {
            name: "simulate",
            description: "Run the cloth simulation for some seconds of wall-clock time, so the \
                garment drapes on the avatar, then pause it. Reports how far it got.",
            input_schema: json!({
                "type": "object",
                "properties": {"seconds": {"type": "number", "minimum": 0.5, "maximum": 60}},
                "required": ["seconds"],
                "additionalProperties": false
            }),
            changes_document: true,
        },
        Tool {
            name: "screenshot",
            description: "A PNG picture of the 3D view as it is now, to look at the garment; \
                or of the whole window, panels included.",
            input_schema: json!({
                "type": "object",
                "properties": {"area": {"type": "string", "enum": ["view", "window"],
                    "description": "The 3D view (the default) or the whole window."}},
                "additionalProperties": false
            }),
            changes_document: false,
        },
        Tool {
            name: "size_chart",
            description: "The garment's sizes and points of measure: each size group (its sizes \
                smallest first, the base size the pattern is drawn in, the size shown in 3D) and \
                every point of measure in every size, in mm, with its difference from the base \
                and its tolerance. Show another size in 3D with command_run \
                'grading.next_size' or 'grading.previous_size'.",
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            changes_document: false,
        },
        Tool {
            name: "save_project",
            description: "Save the project as <name>.json in Documents/FashionCraft (the folder \
                is made if needed). Reports the path.",
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string", "description": "A file name without folders."}},
                "required": ["name"],
                "additionalProperties": false
            }),
            changes_document: false,
        },
        Tool {
            name: "export_garment",
            description: "Export the draped garment for other 3D tools into Documents/FashionCraft: \
                obj (with its materials) or glb (glTF binary). Reports the path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "format": {"type": "string", "enum": ["obj", "glb"]},
                    "name": {"type": "string", "description": "A file name without folders or extension."}
                },
                "required": ["format", "name"],
                "additionalProperties": false
            }),
            changes_document: false,
        },
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
                "instructions": "FashionCraft designs garments: 2D sewing patterns sewn and \
                    draped on an avatar in 3D. Start with scene_summary. Lengths are in cm."
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
