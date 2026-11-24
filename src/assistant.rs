//! The Assistant: a chat in FashionCraft's window that asks a model — Claude, or any
//! OpenAI-compatible API (ChatGPT, Gemini, a local model) — to work in the open project with the
//! tools `fashioncraft-cli mcp` offers (see [`crate::agent`]).
//!
//! This module is the protocol and the tool loop. The window supplies the HTTP [`Transport`]
//! and runs the tools on its own thread; tests supply canned answers.
use crate::agent::{Content, ToolResult, tools};
use serde_json::{Value, json};

/// The Messages API version FashionCraft speaks.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
/// The beta that lets a declined request be retried on Anthropic's recommended fallback model.
pub const ANTHROPIC_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// A turn stops after this many rounds of tool calls; the person can say "continue".
pub const MAX_ROUNDS: usize = 40;
const MAX_TOKENS: u32 = 16000;

/// What the model is told about FashionCraft and how to work in it. The Assistant sends it as
/// the system prompt (API and CLI providers alike) and `fashioncraft-cli mcp` gives it to MCP
/// clients as the server's instructions.
pub const SYSTEM_PROMPT: &str = "\
You work in FashionCraft, a desktop app for drafting sewing patterns and draping them on a 3D \
avatar, through its tools. What you change shows in the person's open window at once, and every \
pattern edit can be undone (command_run edit.undo).

Garment basics
- A garment is 2D pattern pieces sewn together. A piece's outline is numbered points joined by \
segments; a seam joins segments of pieces, one to one or several to several (M:N). Sewn sides \
should be equally long; a longer side needs ease and is gathered or eased in.
- Grain: the warp runs at the piece's grain angle. Fabric stretches differently along warp, weft \
and the bias (45 degrees).
- Ease is width added to a body measurement for fit or style. Seam allowance is the margin outside \
the sewing line (drafting only). Notches mark matching points. Darts and pleats take in fullness.
- Lengths are in cm; on the 2D board x goes right and y down. Fabric values are model parameters, \
not lab measurements of a real cloth.

How to work
1. Look: scene_summary (pieces, segments and lengths, seams, fabrics, avatar, selection, \
warnings); piece_detail before editing a piece.
2. Plan the steps. Ask only when something essential is missing; otherwise choose sensibly and \
say what you chose.
3. Act with the most specific tool: draft_garment for a new base, then create_piece, move_points, \
set_segment_length, add_point, add_dart, add_pleats, piece_edit, sew, seam_edit, place_piece, \
set_fabric, set_fabric_parameters, set_color, set_avatar, set_pose, grading. command_list shows \
the menu commands an agent can run; command_run runs one, with target to select first.
4. Simulate after changing the pattern, fabric, avatar or placement (5 to 15 s). Then check the \
seams: one that is still open (gap over 6 mm) needs its lengths, placement or ease fixed, and \
another simulation.
5. Look with screenshot (turn the view with command_run view.front, view.back, view.avatar_left, \
view.three_quarter_right) and describe what you actually see.
6. Report briefly what changed, with the key numbers, and what is left.

Attachments
- The person can attach pictures (photos, sketches, swatches), PDFs, text, CSV tables and \
patterns. Pictures and PDFs reach you as they are when your connection reads them; otherwise the \
message says so: tell the person plainly. A pattern or 3D model comes with FashionCraft's \
summary (pieces and sizes) and a picture of its pieces; open_attachment opens it in the \
importer for the person to confirm.
- To make a garment from a photo or sketch: describe what you see (silhouette, pieces, seams, \
closures, length, fit, fabric), draft the nearest base with draft_garment, adjust it, simulate, \
take a screenshot, compare it with the picture and adjust again. Say what you could not match.

Limits and honesty
- You cannot draw with the mouse, open windows or file dialogs, read files the person did not \
attach, edit textures or graphics, or animate. Say so plainly and name the menu path the person can use.
- Drafted bases are simplified blocks (straight armholes and sleeve caps), not production \
patterns. Fabric presets are indicative; for manufacturing, calibrate on the real fabric \
(Materials > Measure fabric).
- Never claim a fit, a closed seam or a look you have not checked with scene_summary, simulate \
or screenshot. When a tool fails, read its message and change something; never repeat the same \
failing call.
- draft_garment and set_avatar replace work: if scene_summary reports unsaved changes, say so \
first, or save_project when the person wants to keep it.

Reply in the person's language, short and practical.";

/// A model provider the Assistant can talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAi,
    CodexCli,
    ClaudeCli,
    Gemini,
    GeminiCli,
    AntigravityCli,
    /// Any server with the OpenAI chat-completions API on this computer: Ollama, LM Studio…
    Local,
}

impl Provider {
    pub const ALL: [Self; 8] = [
        Self::Anthropic,
        Self::ClaudeCli,
        Self::OpenAi,
        Self::CodexCli,
        Self::Gemini,
        Self::GeminiCli,
        Self::AntigravityCli,
        Self::Local,
    ];

    /// Whether this provider runs through a local CLI binary rather than HTTP.
    pub fn is_cli(self) -> bool {
        matches!(
            self,
            Self::CodexCli | Self::ClaudeCli | Self::GeminiCli | Self::AntigravityCli
        )
    }

    /// The name kept in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::CodexCli => "codex-cli",
            Self::ClaudeCli => "claude-cli",
            Self::Gemini => "gemini",
            Self::GeminiCli => "gemini-cli",
            Self::AntigravityCli => "antigravity-cli",
            Self::Local => "local",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Claude (Anthropic)",
            Self::OpenAi => "ChatGPT (OpenAI)",
            Self::CodexCli => "ChatGPT (Codex CLI)",
            Self::ClaudeCli => "Claude (Claude Code CLI)",
            Self::Gemini => "Gemini (Google)",
            Self::GeminiCli => "Gemini (Gemini CLI)",
            Self::AntigravityCli => "Antigravity (Google CLI)",
            Self::Local => "Local model (Ollama, LM Studio…)",
        }
    }
    /// The model used when the preferences name none.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-opus-5-5",
            _ => "",
        }
    }
    /// What the model field asks for when it is empty.
    pub fn model_hint(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-opus-5-5",
            Self::OpenAi => "the model name, as OpenAI lists it",
            Self::CodexCli => "Codex default",
            Self::ClaudeCli => "Claude Code default",
            Self::Gemini => "the model name, as Google lists it",
            Self::GeminiCli => "Gemini CLI default",
            Self::AntigravityCli => "Antigravity default",
            Self::Local => "the model name, as `ollama list` shows it",
        }
    }
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::OpenAi => "https://api.openai.com/v1",
            Self::CodexCli | Self::ClaudeCli | Self::GeminiCli | Self::AntigravityCli => "",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Local => "http://localhost:11434/v1",
        }
    }
    /// The environment variable that can hold this provider's API key.
    pub fn key_variable(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::OpenAi => Some("OPENAI_API_KEY"),
            Self::Gemini => Some("GEMINI_API_KEY"),
            Self::Local
            | Self::CodexCli
            | Self::ClaudeCli
            | Self::GeminiCli
            | Self::AntigravityCli => None,
        }
    }
    pub fn needs_key(self) -> bool {
        !matches!(self, Self::Local) && !self.is_cli()
    }
    /// Whether screenshots go to the model. Many local models read no pictures.
    fn sees_images(self) -> bool {
        self != Self::Local
    }
}

/// Where and how to ask: the preferences plus the key.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub provider: Provider,
    /// Empty: [`Provider::default_model`].
    pub model: String,
    /// Empty: [`Provider::default_base_url`].
    pub base_url: String,
    pub api_key: String,
}

impl Settings {
    pub fn model(&self) -> &str {
        match self.model.trim() {
            "" => self.provider.default_model(),
            model => model,
        }
    }
    pub fn base_url(&self) -> &str {
        match self.base_url.trim().trim_end_matches('/') {
            "" => self.provider.default_base_url(),
            url => url,
        }
    }
    /// What is missing before a message can be sent, if anything.
    pub fn missing(&self) -> Option<String> {
        if !self.provider.is_cli() && self.model().is_empty() {
            return Some("Choose a model under Connection, at the top of this window".into());
        }
        if self.provider.needs_key() && self.api_key.trim().is_empty() {
            let variable = self.provider.key_variable().unwrap_or("the API key");
            return Some(format!(
                "Add your {} API key under Connection, at the top of this window (or set {variable})",
                self.provider.label()
            ));
        }
        None
    }
}

/// Posts JSON and returns the JSON answer, or a message for the person.
pub trait Transport {
    fn post(
        &mut self,
        url: &str,
        headers: &[(&str, String)],
        body: &Value,
    ) -> Result<Value, String>;
}

/// What the transcript shows while a turn runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Text the model wrote.
    Text(String),
    /// A tool about to run in the window.
    ToolStarted { name: String, arguments: Value },
    /// How it went: its text, and how many pictures it returned.
    ToolFinished {
        name: String,
        text: String,
        images: Vec<String>,
        is_error: bool,
    },
    /// Something about the turn itself: stopped, cut off, declined.
    Notice(String),
}

/// One tool call the model asked for.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    id: String,
    name: String,
    /// The arguments, or why they could not be read.
    arguments: Result<Value, String>,
}

/// A conversation with one provider, in that provider's own message format: Anthropic's
/// content blocks (thinking and fallback blocks included) go back exactly as they came.
#[derive(Clone, Debug)]
pub struct Conversation {
    provider: Provider,
    messages: Vec<Value>,
}

impl Conversation {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            messages: Vec::new(),
        }
    }
    pub fn provider(&self) -> Provider {
        self.provider
    }
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
    pub fn messages(&self) -> &[Value] {
        &self.messages
    }

    /// Sends the person's `text` and lets the model work until it answers, `stop` turns true,
    /// or [`MAX_ROUNDS`] rounds of tools have run. `run_tool` runs one tool in the window.
    ///
    /// The conversation stays valid whatever happens: a failed request adds nothing, and a
    /// stop answers every call the model asked for.
    pub fn turn(
        &mut self,
        settings: &Settings,
        text: &str,
        transport: &mut dyn Transport,
        run_tool: &mut dyn FnMut(&str, &Value) -> ToolResult,
        events: &mut dyn FnMut(Event),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        self.turn_with(settings, text, &[], transport, run_tool, events, stop)
    }

    /// [`Self::turn`], with files the person attached to the message.
    #[allow(clippy::too_many_arguments)]
    pub fn turn_with(
        &mut self,
        settings: &Settings,
        text: &str,
        attachments: &[crate::attachments::Attachment],
        transport: &mut dyn Transport,
        run_tool: &mut dyn FnMut(&str, &Value) -> ToolResult,
        events: &mut dyn FnMut(Event),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if let Some(missing) = settings.missing() {
            return Err(missing);
        }
        self.push_user(text, attachments);
        let known: Vec<&str> = tools().iter().map(|t| t.name).collect();
        for _ in 0..MAX_ROUNDS {
            if stop() {
                events(Event::Notice("Stopped.".into()));
                return Ok(());
            }
            let calls = match self.provider {
                Provider::Anthropic => self.anthropic_step(settings, transport, events)?,
                _ => self.openai_step(settings, transport, events)?,
            };
            if calls.is_empty() {
                return Ok(());
            }
            let mut results = Vec::new();
            for call in calls {
                let result = if stop() {
                    ToolResult::error("Stopped by the person before it ran")
                } else {
                    match &call.arguments {
                        Err(e) => ToolResult::error(e.clone()),
                        Ok(_) if !known.contains(&call.name.as_str()) => {
                            ToolResult::error(format!("There is no tool called '{}'", call.name))
                        }
                        Ok(arguments) => {
                            events(Event::ToolStarted {
                                name: call.name.clone(),
                                arguments: arguments.clone(),
                            });
                            let result = run_tool(&call.name, arguments);
                            events(Event::ToolFinished {
                                name: call.name.clone(),
                                text: result.joined_text(),
                                images: images(&result),
                                is_error: result.is_error,
                            });
                            result
                        }
                    }
                };
                results.push((call, result));
            }
            match self.provider {
                Provider::Anthropic => self.push_anthropic_results(results),
                _ => self.push_openai_results(results),
            }
        }
        events(Event::Notice(format!(
            "Paused after {MAX_ROUNDS} rounds of tools: say \"continue\" to go on."
        )));
        Ok(())
    }

    /// The person's message and its attachments. After a stop or a failed request the last
    /// message may already be theirs: Anthropic's then takes these blocks too, so turns keep
    /// alternating.
    fn push_user(&mut self, text: &str, attachments: &[crate::attachments::Attachment]) {
        if self.provider == Provider::Anthropic {
            let blocks = crate::attachments::anthropic_blocks(text, attachments);
            if let Some(last) = self.messages.last_mut()
                && last["role"] == "user"
                && let Some(content) = last["content"].as_array_mut()
            {
                content.extend(blocks);
                return;
            }
            self.messages
                .push(json!({"role": "user", "content": blocks}));
            return;
        }
        let content =
            crate::attachments::openai_content(text, attachments, self.provider.sees_images());
        self.messages
            .push(json!({"role": "user", "content": content}));
    }

    fn anthropic_step(
        &mut self,
        settings: &Settings,
        transport: &mut dyn Transport,
        events: &mut dyn FnMut(Event),
    ) -> Result<Vec<Call>, String> {
        let (body, beta) = anthropic_request(settings, &self.messages);
        let mut headers = vec![
            ("x-api-key", settings.api_key.trim().to_owned()),
            ("anthropic-version", ANTHROPIC_VERSION.to_owned()),
        ];
        if beta {
            headers.push(("anthropic-beta", ANTHROPIC_FALLBACK_BETA.to_owned()));
        }
        let url = format!("{}/messages", settings.base_url());
        let answer = transport.post(&url, &headers, &body)?;
        let content = answer["content"]
            .as_array()
            .ok_or_else(|| format!("Unexpected answer from {url}"))?;
        let mut calls = Vec::new();
        for block in content {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str().filter(|t| !t.trim().is_empty()) {
                        events(Event::Text(text.to_owned()));
                    }
                }
                Some("tool_use") => calls.push(Call {
                    id: block["id"].as_str().unwrap_or_default().to_owned(),
                    name: block["name"].as_str().unwrap_or_default().to_owned(),
                    arguments: Ok(block["input"].clone()),
                }),
                Some("fallback") => events(Event::Notice(format!(
                    "{} declined; {} answered instead.",
                    block["from"]["model"].as_str().unwrap_or("The model"),
                    block["to"]["model"].as_str().unwrap_or("another model"),
                ))),
                _ => {}
            }
        }
        // Everything goes back as it came, thinking blocks included. An empty answer (a
        // refusal before any output) cannot be sent back, and is not needed.
        if !content.is_empty() {
            self.messages
                .push(json!({"role": "assistant", "content": content.clone()}));
        }
        match answer["stop_reason"].as_str() {
            Some("refusal") => {
                events(Event::Notice(
                    "The model declined this request. Try asking another way.".into(),
                ));
                return Ok(Vec::new());
            }
            Some("max_tokens") if calls.is_empty() => events(Event::Notice(
                "The answer was cut off: say \"continue\" for the rest.".into(),
            )),
            _ => {}
        }
        Ok(calls)
    }

    fn push_anthropic_results(&mut self, results: Vec<(Call, ToolResult)>) {
        let blocks: Vec<Value> = results
            .into_iter()
            .map(|(call, result)| {
                let mut content: Vec<Value> = result
                    .content
                    .iter()
                    .map(|c| match c {
                        Content::Text { text } => json!({"type": "text", "text": text}),
                        Content::Image { data, mime_type } => json!({
                            "type": "image",
                            "source": {"type": "base64", "media_type": mime_type, "data": data}
                        }),
                    })
                    .collect();
                if content.is_empty() {
                    content.push(json!({"type": "text", "text": "(done)"}));
                }
                json!({
                    "type": "tool_result",
                    "tool_use_id": call.id,
                    "content": content,
                    "is_error": result.is_error,
                })
            })
            .collect();
        self.messages
            .push(json!({"role": "user", "content": blocks}));
    }

    fn openai_step(
        &mut self,
        settings: &Settings,
        transport: &mut dyn Transport,
        events: &mut dyn FnMut(Event),
    ) -> Result<Vec<Call>, String> {
        let body = openai_request(settings, &self.messages);
        let mut headers = Vec::new();
        if !settings.api_key.trim().is_empty() {
            headers.push((
                "authorization",
                format!("Bearer {}", settings.api_key.trim()),
            ));
        }
        let url = format!("{}/chat/completions", settings.base_url());
        let answer = transport.post(&url, &headers, &body)?;
        let choice = &answer["choices"][0];
        let message = &choice["message"];
        if !message.is_object() {
            return Err(format!("Unexpected answer from {url}"));
        }
        let text = message["content"].as_str().unwrap_or_default();
        if !text.trim().is_empty() {
            events(Event::Text(text.to_owned()));
        }
        let tool_calls = message["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let calls: Vec<Call> = tool_calls
            .iter()
            .map(|c| {
                let raw = c["function"]["arguments"].as_str().unwrap_or("{}");
                Call {
                    id: c["id"].as_str().unwrap_or_default().to_owned(),
                    name: c["function"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    arguments: match raw.trim() {
                        "" => Ok(json!({})),
                        raw => serde_json::from_str(raw)
                            .map_err(|e| format!("The arguments were not valid JSON: {e}")),
                    },
                }
            })
            .collect();
        // Only the fields every compatible server accepts go back.
        let mut kept = json!({"role": "assistant", "content": message["content"].clone()});
        if !tool_calls.is_empty() {
            kept["tool_calls"] = Value::Array(tool_calls);
        }
        self.messages.push(kept);
        match choice["finish_reason"].as_str() {
            Some("length") if calls.is_empty() => events(Event::Notice(
                "The answer was cut off: say \"continue\" for the rest.".into(),
            )),
            Some("content_filter") => events(Event::Notice(
                "The provider filtered this answer. Try asking another way.".into(),
            )),
            _ => {}
        }
        Ok(calls)
    }

    fn push_openai_results(&mut self, results: Vec<(Call, ToolResult)>) {
        let mut pictures = Vec::new();
        for (call, result) in results {
            let mut text = result.joined_text();
            if result.is_error {
                text = format!("Error: {text}");
            }
            if text.is_empty() {
                text = "(done)".into();
            }
            self.messages.push(json!({
                "role": "tool", "tool_call_id": call.id, "content": text
            }));
            for c in &result.content {
                if let Content::Image { data, mime_type } = c {
                    pictures.push(json!({
                        "type": "image_url",
                        "image_url": {"url": format!("data:{mime_type};base64,{data}")}
                    }));
                }
            }
        }
        // A tool message carries no picture here: the screenshot follows as the person's.
        if !pictures.is_empty() && self.provider.sees_images() {
            let mut content =
                vec![json!({"type": "text", "text": "The screenshot tool's picture:"})];
            content.extend(pictures);
            self.messages
                .push(json!({"role": "user", "content": content}));
        }
    }
}

/// The pictures a result carries, base64.
fn images(result: &ToolResult) -> Vec<String> {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Image { data, .. } => Some(data.clone()),
            Content::Text { .. } => None,
        })
        .collect()
}

/// A Claude model new enough for effort and server-side fallbacks (Haiku has no fallback).
fn current_claude(model: &str) -> bool {
    ["claude-opus-5", "claude-sonnet-5", "claude-fable-5"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}

/// The Messages API request, and whether it needs the fallback beta header.
pub fn anthropic_request(settings: &Settings, messages: &[Value]) -> (Value, bool) {
    let tools: Vec<Value> = tools()
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
        .collect();
    let model = settings.model();
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": SYSTEM_PROMPT,
        "tools": tools,
        "messages": messages,
        // Caches the system prompt, the tools and the conversation so far between rounds.
        "cache_control": {"type": "ephemeral"},
    });
    let current = current_claude(model);
    if current {
        body["output_config"] = json!({"effort": "medium"});
        body["fallbacks"] = json!("default");
    }
    (body, current)
}

/// The chat-completions request.
pub fn openai_request(settings: &Settings, messages: &[Value]) -> Value {
    let tools: Vec<Value> = tools()
        .iter()
        .map(|t| {
            json!({"type": "function", "function": {
                "name": t.name, "description": t.description, "parameters": t.input_schema
            }})
        })
        .collect();
    let mut all = vec![json!({"role": "system", "content": SYSTEM_PROMPT})];
    all.extend(messages.iter().cloned());
    json!({"model": settings.model(), "messages": all, "tools": tools})
}

/// Whether `url` points at this computer (localhost, 127.x.x.x or ::1): a local model or the
/// stand-in server, never a provider on the internet.
pub fn is_local_address(url: &str) -> bool {
    let rest = url
        .trim()
        .split_once("://")
        .map_or(url.trim(), |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// A failed request, for the person: the provider's own message and what usually fixes it.
pub fn describe_error(status: u16, body: &Value) -> String {
    // OpenAI and Anthropic answer {"error": {...}}; Gemini's compatible API, [{"error": ...}].
    let error = if body.is_array() {
        &body[0]["error"]
    } else {
        &body["error"]
    };
    let message = error["message"]
        .as_str()
        .or_else(|| error.as_str())
        .unwrap_or("no details");
    let hint = match status {
        401 | 403 => " Check the API key under the Assistant's Connection.",
        404 => " Check the model name and the address under the Assistant's Connection.",
        429 => " Too many requests or no credit left: wait a moment, or check the account.",
        500..=599 => " The service has a problem or is busy: try again shortly.",
        _ => "",
    };
    format!("HTTP {status}: {message}.{hint}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// One request as sent: the address, the headers and the body.
    type Sent = (String, Vec<(String, String)>, Value);
    /// What a turn returned, the events it showed, and the tools it ran.
    type Outcome = (Result<(), String>, Vec<Event>, Vec<(String, Value)>);

    /// Answers each post with the next canned answer and keeps what was sent.
    struct Canned {
        answers: VecDeque<Result<Value, String>>,
        sent: Vec<Sent>,
    }
    impl Canned {
        fn new(answers: Vec<Result<Value, String>>) -> Self {
            Self {
                answers: answers.into(),
                sent: Vec::new(),
            }
        }
    }
    impl Transport for Canned {
        fn post(
            &mut self,
            url: &str,
            headers: &[(&str, String)],
            body: &Value,
        ) -> Result<Value, String> {
            self.sent.push((
                url.to_owned(),
                headers
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.clone()))
                    .collect(),
                body.clone(),
            ));
            self.answers
                .pop_front()
                .expect("an answer for every request")
        }
    }

    fn settings(provider: Provider) -> Settings {
        Settings {
            provider,
            model: String::new(),
            base_url: String::new(),
            api_key: "test-key".into(),
        }
    }

    fn run(
        conversation: &mut Conversation,
        settings: &Settings,
        transport: &mut Canned,
        text: &str,
    ) -> Outcome {
        let mut events = Vec::new();
        let mut ran = Vec::new();
        let result = conversation.turn(
            settings,
            text,
            transport,
            &mut |name, arguments| {
                ran.push((name.to_owned(), arguments.clone()));
                if name == "screenshot" {
                    ToolResult {
                        content: vec![
                            Content::Image {
                                data: "AAAA".into(),
                                mime_type: "image/png".into(),
                            },
                            Content::Text {
                                text: "The 3D view now.".into(),
                            },
                        ],
                        is_error: false,
                    }
                } else {
                    ToolResult::text(format!("did {name}"))
                }
            },
            &mut |e| events.push(e),
            &|| false,
        );
        (result, events, ran)
    }

    #[test]
    fn claude_runs_tools_and_gets_its_blocks_back_unchanged() {
        let thinking = json!({"type": "thinking", "thinking": "plan", "signature": "sig"});
        let mut transport = Canned::new(vec![
            Ok(json!({
                "content": [
                    thinking,
                    {"type": "text", "text": "Looking first."},
                    {"type": "tool_use", "id": "t1", "name": "scene_summary", "input": {}},
                    {"type": "tool_use", "id": "t2", "name": "screenshot", "input": {}}
                ],
                "stop_reason": "tool_use"
            })),
            Ok(json!({
                "content": [{"type": "text", "text": "A red tee on the avatar."}],
                "stop_reason": "end_turn"
            })),
        ]);
        let mut conversation = Conversation::new(Provider::Anthropic);
        let (result, events, ran) = run(
            &mut conversation,
            &settings(Provider::Anthropic),
            &mut transport,
            "What is on screen?",
        );
        result.unwrap();
        assert_eq!(ran.len(), 2);
        assert_eq!(events.first(), Some(&Event::Text("Looking first.".into())));
        assert_eq!(
            events.last(),
            Some(&Event::Text("A red tee on the avatar.".into()))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ToolFinished { name, images, .. }
            if name == "screenshot" && images.len() == 1))
        );

        let (url, headers, first) = &transport.sent[0];
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
        let header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(header("x-api-key"), Some("test-key"));
        assert_eq!(header("anthropic-version"), Some(ANTHROPIC_VERSION));
        assert_eq!(header("anthropic-beta"), Some(ANTHROPIC_FALLBACK_BETA));
        assert_eq!(first["model"], "claude-opus-5-5");
        assert_eq!(first["fallbacks"], "default");
        assert_eq!(first["output_config"]["effort"], "medium");
        assert_eq!(first["cache_control"]["type"], "ephemeral");
        assert!(first.get("thinking").is_none());
        assert!(first.get("tool_choice").is_none());
        assert_eq!(first["tools"].as_array().unwrap().len(), tools().len());
        assert!(first["tools"][0]["input_schema"].is_object());

        // The second request carries the assistant's blocks as they came, then one user
        // message with both results; the picture as an image block.
        let second = &transport.sent[1].2["messages"];
        assert_eq!(second[1]["content"][0], thinking);
        let results = second[2]["content"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["tool_use_id"], "t1");
        assert_eq!(results[1]["content"][0]["type"], "image");
        assert_eq!(
            results[1]["content"][0]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(results[1]["is_error"], false);
        assert_eq!(conversation.messages().len(), 4);
    }

    #[test]
    fn attached_pictures_reach_claude_and_openai_compatible_models() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 120, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let sketch = crate::attachments::from_bytes("sketch.png", &png);
        let no_tools = |_: &str, _: &Value| ToolResult::text("");
        for provider in [
            Provider::Anthropic,
            Provider::OpenAi,
            Provider::ClaudeCli,
            Provider::Local,
        ] {
            let answer = if provider == Provider::Anthropic {
                json!({"content": [{"type": "text", "text": "A navy dress."}], "stop_reason": "end_turn"})
            } else {
                json!({"choices": [{"message": {"role": "assistant", "content": "A navy dress."}, "finish_reason": "stop"}]})
            };
            let mut transport = Canned::new(vec![Ok(answer)]);
            let mut conversation = Conversation::new(provider);
            let mut s = settings(provider);
            s.model = "test-model".into();
            conversation
                .turn_with(
                    &s,
                    "What is this?",
                    std::slice::from_ref(&sketch),
                    &mut transport,
                    &mut { no_tools },
                    &mut |_| {},
                    &|| false,
                )
                .unwrap();
            let sent = transport.sent[0].2.to_string();
            let sees = provider != Provider::Local;
            assert_eq!(
                sent.contains(if provider == Provider::Anthropic {
                    "\"media_type\":\"image/png\""
                } else {
                    "data:image/png;base64,"
                }),
                sees,
                "{provider:?}"
            );
            assert!(sent.contains("sketch.png") && sent.contains("What is this?"));
        }
    }

    #[test]
    fn a_failed_request_leaves_a_conversation_that_can_go_on() {
        let mut transport = Canned::new(vec![
            Err("HTTP 529: Overloaded.".into()),
            Ok(json!({"content": [{"type": "text", "text": "Hi"}], "stop_reason": "end_turn"})),
        ]);
        let mut conversation = Conversation::new(Provider::Anthropic);
        let s = settings(Provider::Anthropic);
        assert!(run(&mut conversation, &s, &mut transport, "one").0.is_err());
        run(&mut conversation, &s, &mut transport, "two").0.unwrap();
        // The unanswered message and the next one share one user turn.
        let sent = &transport.sent[1].2["messages"];
        assert_eq!(sent.as_array().unwrap().len(), 1);
        assert_eq!(sent[0]["content"][1]["text"], "two");
    }

    #[test]
    fn refusals_unknown_tools_and_bad_arguments_are_reported() {
        let mut transport = Canned::new(vec![Ok(json!({"content": [], "stop_reason": "refusal"}))]);
        let mut conversation = Conversation::new(Provider::Anthropic);
        let (_, events, _) = run(
            &mut conversation,
            &settings(Provider::Anthropic),
            &mut transport,
            "x",
        );
        assert!(matches!(&events[0], Event::Notice(n) if n.contains("declined")));
        // An empty answer is not kept: the next message joins the person's turn.
        assert_eq!(conversation.messages().len(), 1);

        let mut transport = Canned::new(vec![
            Ok(
                json!({"choices": [{"message": {"role": "assistant", "content": null, "tool_calls": [
                {"id": "a", "type": "function", "function": {"name": "rm_rf", "arguments": "{}"}},
                {"id": "b", "type": "function", "function": {"name": "simulate", "arguments": "{oops"}}
            ]}, "finish_reason": "tool_calls"}]}),
            ),
            Ok(
                json!({"choices": [{"message": {"role": "assistant", "content": "Sorry."},
                "finish_reason": "stop"}]}),
            ),
        ]);
        let mut conversation = Conversation::new(Provider::OpenAi);
        let mut s = settings(Provider::OpenAi);
        s.model = "some-model".into();
        let (result, _, ran) = run(&mut conversation, &s, &mut transport, "go");
        result.unwrap();
        assert!(ran.is_empty(), "nothing unknown or unreadable runs");
        let replies = &transport.sent[1].2["messages"];
        assert!(
            replies[3]["content"]
                .as_str()
                .unwrap()
                .contains("no tool called 'rm_rf'")
        );
        assert!(
            replies[4]["content"]
                .as_str()
                .unwrap()
                .contains("not valid JSON")
        );
    }

    #[test]
    fn openai_compatible_apis_get_functions_tool_messages_and_the_picture() {
        let mut transport = Canned::new(vec![
            Ok(
                json!({"choices": [{"message": {"role": "assistant", "content": "",
                "refusal": null, "tool_calls": [{"id": "c1", "type": "function",
                "function": {"name": "screenshot", "arguments": ""}}]},
                "finish_reason": "tool_calls"}]}),
            ),
            Ok(
                json!({"choices": [{"message": {"role": "assistant", "content": "Looks good."},
                "finish_reason": "stop"}]}),
            ),
        ]);
        let mut conversation = Conversation::new(Provider::Gemini);
        let mut s = settings(Provider::Gemini);
        s.model = "a-gemini-model".into();
        let (result, events, ran) = run(&mut conversation, &s, &mut transport, "Show me");
        result.unwrap();
        assert_eq!(ran, vec![("screenshot".to_owned(), json!({}))]);
        assert_eq!(events.last(), Some(&Event::Text("Looks good.".into())));
        let (url, headers, first) = &transport.sent[0];
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
        assert_eq!(
            headers[0],
            ("authorization".to_owned(), "Bearer test-key".to_owned())
        );
        assert_eq!(first["messages"][0]["role"], "system");
        assert_eq!(first["tools"][0]["type"], "function");
        assert!(first["tools"][0]["function"]["parameters"].is_object());
        let second = &transport.sent[1].2["messages"];
        // system, user, assistant (without the extra fields), tool, picture
        assert!(second[2].get("refusal").is_none());
        assert_eq!(second[3]["role"], "tool");
        assert_eq!(second[3]["tool_call_id"], "c1");
        assert_eq!(second[4]["role"], "user");
        assert!(
            second[4]["content"][1]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
    }

    #[test]
    fn local_models_need_no_key_and_get_no_pictures() {
        let mut transport = Canned::new(vec![
            Ok(
                json!({"choices": [{"message": {"role": "assistant", "content": null,
                "tool_calls": [{"id": "c", "type": "function",
                "function": {"name": "screenshot", "arguments": "{}"}}]},
                "finish_reason": "tool_calls"}]}),
            ),
            Ok(json!({"choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]})),
        ]);
        let mut conversation = Conversation::new(Provider::Local);
        let s = Settings {
            provider: Provider::Local,
            model: "qwen".into(),
            base_url: "http://127.0.0.1:1234/v1/".into(),
            api_key: String::new(),
        };
        run(&mut conversation, &s, &mut transport, "hi").0.unwrap();
        assert_eq!(
            transport.sent[0].0,
            "http://127.0.0.1:1234/v1/chat/completions"
        );
        assert!(transport.sent[0].1.is_empty());
        let last = transport.sent[1].2["messages"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .clone();
        assert_eq!(last["role"], "tool");
    }

    #[test]
    fn a_stop_answers_every_call_and_settings_say_what_is_missing() {
        let mut transport = Canned::new(vec![Ok(json!({
            "content": [{"type": "tool_use", "id": "t", "name": "simulate", "input": {}}],
            "stop_reason": "tool_use"
        }))]);
        let mut conversation = Conversation::new(Provider::Anthropic);
        let mut ran = 0;
        // The person presses Stop while the model's first answer is on its way.
        let checks = std::cell::Cell::new(0);
        let mut notices = Vec::new();
        conversation
            .turn(
                &settings(Provider::Anthropic),
                "go",
                &mut transport,
                &mut |_, _| {
                    ran += 1;
                    ToolResult::text("x")
                },
                &mut |e| notices.push(e),
                &|| {
                    checks.set(checks.get() + 1);
                    checks.get() > 1
                },
            )
            .unwrap();
        assert_eq!(notices, vec![Event::Notice("Stopped.".into())]);
        assert_eq!(ran, 0);
        let last = conversation.messages().last().unwrap();
        assert_eq!(last["content"][0]["type"], "tool_result");
        assert_eq!(last["content"][0]["is_error"], true);

        let mut s = settings(Provider::OpenAi);
        assert!(s.missing().unwrap().contains("model"));
        s.model = "m".into();
        s.api_key = " ".into();
        assert!(s.missing().unwrap().contains("OPENAI_API_KEY"));
        s.api_key = "k".into();
        assert_eq!(s.missing(), None);
        let haiku = Settings {
            model: "claude-haiku-5-5".into(),
            ..settings(Provider::Anthropic)
        };
        let (body, beta) = anthropic_request(&haiku, &[]);
        assert!(!beta && body.get("fallbacks").is_none() && body.get("output_config").is_none());
    }

    #[test]
    fn only_this_computer_counts_as_local() {
        for local in [
            "http://127.0.0.1:18765/v1",
            "http://localhost:11434/v1",
            "http://127.9.9.9/v1",
            "http://[::1]:8080/v1",
            "http://studio.localhost/v1",
            "localhost:1234",
        ] {
            assert!(is_local_address(local), "{local}");
        }
        for remote in [
            Provider::Anthropic.default_base_url(),
            Provider::OpenAi.default_base_url(),
            Provider::Gemini.default_base_url(),
            "http://127.0.0.1.example.com/v1",
            "http://localhost.example.com/v1",
            "http://user@evil.example/v1",
            "http://10.0.0.5:11434/v1",
            "",
        ] {
            assert!(!is_local_address(remote), "{remote}");
        }
        assert!(is_local_address(Provider::Local.default_base_url()));
    }

    #[test]
    fn errors_name_the_fix() {
        let anthropic = json!({"type": "error", "error": {"type": "authentication_error",
            "message": "invalid x-api-key"}});
        let text = describe_error(401, &anthropic);
        assert!(
            text.contains("invalid x-api-key") && text.contains("API key"),
            "{text}"
        );
        let gemini = json!([{"error": {"code": 404, "message": "model not found"}}]);
        assert!(describe_error(404, &gemini).contains("model not found"));
        for p in Provider::ALL {
            assert_eq!(Provider::from_id(p.id()), Some(p));
            if p.is_cli() {
                let s = Settings {
                    provider: p,
                    model: String::new(),
                    base_url: String::new(),
                    api_key: String::new(),
                };
                assert!(!p.needs_key());
                assert_eq!(s.missing(), None);
            }
        }
    }
}
