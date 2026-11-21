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
pub const MAX_ROUNDS: usize = 25;
const MAX_TOKENS: u32 = 16000;

/// What the model is told about FashionCraft and how to work in it.
pub const SYSTEM_PROMPT: &str = "\
You are the assistant inside FashionCraft, a desktop app for drafting sewing patterns and \
simulating garments on a 3D avatar. You work in the person's open project through your tools; \
what you change shows in their window at once.

How to work:
- Look before you act: scene_summary tells you the pieces, seams, fabrics, avatar and where \
the garment hangs. Measurements and positions are in centimetres.
- draft_garment replaces the current garment with a new pattern. If scene_summary reports \
unsaved changes, say that the current garment will be replaced before you do it, or save it \
first with save_project when the person asked you to keep it.
- After changing a pattern or fabric, run simulate for a few seconds so the cloth settles, \
then take a screenshot when how it looks matters, and describe what you see.
- Fabric presets are indicative starting points, not lab measurements. When the person talks \
about manufacturing, say the fabric should be calibrated on the real material.
- You cannot open files or dialogs. save_project and export_garment write to the person's \
Documents/FashionCraft folder; command_run runs menu commands that need no dialog.
- If a tool fails, read its message: it says what to change. Do not repeat the same failing \
call.

Keep answers short and practical, and reply in the person's language.";

/// A model provider the Assistant can talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAi,
    Gemini,
    /// Any server with the OpenAI chat-completions API on this computer: Ollama, LM Studio…
    Local,
}

impl Provider {
    pub const ALL: [Self; 4] = [Self::Anthropic, Self::OpenAi, Self::Gemini, Self::Local];

    /// The name kept in the preferences.
    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
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
            Self::Gemini => "Gemini (Google)",
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
            Self::Gemini => "the model name, as Google lists it",
            Self::Local => "the model name, as `ollama list` shows it",
        }
    }
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::OpenAi => "https://api.openai.com/v1",
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
            Self::Local => None,
        }
    }
    pub fn needs_key(self) -> bool {
        self != Self::Local
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
        if self.model().is_empty() {
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
        if let Some(missing) = settings.missing() {
            return Err(missing);
        }
        self.push_user_text(text);
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

    /// The person's message. After a stop or a failed request the last message may already be
    /// theirs: Anthropic's then takes the text too, so turns keep alternating.
    fn push_user_text(&mut self, text: &str) {
        let block = json!({"type": "text", "text": text});
        if self.provider == Provider::Anthropic
            && let Some(last) = self.messages.last_mut()
            && last["role"] == "user"
            && let Some(content) = last["content"].as_array_mut()
        {
            content.push(block);
            return;
        }
        self.messages.push(match self.provider {
            Provider::Anthropic => json!({"role": "user", "content": [block]}),
            _ => json!({"role": "user", "content": text}),
        });
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
        }
    }
}
