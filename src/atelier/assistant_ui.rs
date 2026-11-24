//! Window › Assistant: a chat with Claude, ChatGPT, Gemini or a local model that works in the
//! open project. The conversation runs on its own thread (see `fashioncraft::assistant`); its
//! tools run here, on the window's queue, like an MCP client's (see `agent_hub`).
#[path = "antigravity_cli.rs"]
mod antigravity_cli;
#[path = "claude_cli.rs"]
mod claude_cli;
#[path = "cli_process.rs"]
mod cli_process;
#[path = "codex_cli.rs"]
mod codex_cli;
#[path = "gemini_cli.rs"]
mod gemini_cli;

use super::agent_hub::{Job, QUEUED, wait_for};
use super::assistant_keys::{self, Source};
use super::*;
use fashioncraft::agent::ToolResult;
use fashioncraft::assistant::{Conversation, Event, Provider, Settings, Transport, describe_error};
use fashioncraft::attachments::{self, Attachment};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc,
};

/// Widest a screenshot shows in the transcript, points.
const PICTURE_WIDTH: f32 = 300.;
/// Height of the row of attachments under the message (it scrolls sideways).
const CHIPS_HEIGHT: f32 = 40.;
/// Tallest the message field grows before it scrolls.
const INPUT_MAX_HEIGHT: f32 = 120.;

pub(super) struct AssistantUi {
    pub(super) open: bool,
    input: String,
    entries: Vec<Entry>,
    /// Shared with the turn's thread while one runs.
    conversation: Arc<Mutex<Conversation>>,
    worker: Option<Worker>,
    /// The key for the provider it was looked up for (lookups can ask the keychain).
    key: Option<(Provider, String, Source)>,
    key_input: String,
    key_message: Option<String>,
    show_connection: bool,
    /// Development builds: FASHIONCRAFT_ASSISTANT_SEND was looked at.
    test_message_checked: bool,
    /// The current conversation's tool calls, what the model wrote and how the last turn
    /// failed, for the evaluation.
    pub(super) calls: Vec<fashioncraft::agent_eval::Call>,
    pub(super) replies: Vec<String>,
    pub(super) failure: Option<String>,
    /// FASHIONCRAFT_ASSISTANT_EVAL's run, when it was asked for.
    pub(super) eval: super::assistant_eval::EvalState,
    /// Files waiting to go with the next message.
    pub(super) pending: Vec<Attachment>,
    /// Every file sent in this conversation, which open_attachment can open.
    pub(super) sent: Vec<Attachment>,
    /// Thumbnails of attached pictures, by attachment name.
    thumbs: std::collections::HashMap<String, egui::TextureHandle>,
    /// Why the last attach or paste found nothing, shown under the chips.
    attach_note: Option<String>,
    /// What this conversation has used so far (kept in memory only).
    usage: Totals,
    /// This conversation's log in the session folder (assistant-logs/), once it has begun.
    log: Option<PathBuf>,
}

/// A conversation's usage, added up from each request's report.
#[derive(Default)]
struct Totals {
    input: u64,
    cached: u64,
    output: u64,
    /// Money where it is known for every request.
    cost: f64,
    priced: bool,
    /// The plan limit, as the provider last reported it.
    limit: Option<fashioncraft::assistant::Limit>,
    /// The last request's input against the model's context window.
    context: Option<(u64, u64)>,
}

impl Totals {
    fn add(&mut self, u: &fashioncraft::assistant::Usage) {
        let first = self.input + self.output == 0;
        self.input += u.input;
        self.cached += u.cached;
        self.output += u.output;
        match u.cost_usd {
            Some(c) if first || self.priced => {
                self.cost += c;
                self.priced = true;
            }
            _ => self.priced = false,
        }
        if u.limit.is_some() {
            self.limit = u.limit.clone();
        }
        if let Some(window) = u.context_window {
            self.context = Some((u.input, window));
        }
    }
}

impl Default for AssistantUi {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            entries: Vec::new(),
            conversation: Arc::new(Mutex::new(Conversation::new(Provider::Anthropic))),
            worker: None,
            key: None,
            key_input: String::new(),
            key_message: None,
            show_connection: false,
            test_message_checked: false,
            calls: Vec::new(),
            replies: Vec::new(),
            failure: None,
            eval: Default::default(),
            pending: Vec::new(),
            sent: Vec::new(),
            thumbs: Default::default(),
            attach_note: None,
            usage: Totals::default(),
            log: None,
        }
    }
}

enum Entry {
    You(String),
    /// The files sent with the next message, by name.
    Attached(Vec<String>),
    Model(String),
    Tool {
        name: String,
        detail: String,
        /// None while it runs.
        outcome: Option<(bool, String)>,
        pictures: Vec<egui::TextureHandle>,
    },
    Notice(String),
    Problem(String),
    /// Paused for a review after this many steps; Continue resumes.
    Paused(usize),
}

struct Worker {
    stop: Arc<AtomicBool>,
    updates: mpsc::Receiver<Update>,
}

enum Update {
    Event(Event),
    Done(Result<(), String>),
}

/// HTTPS with rustls and Mozilla's roots; a slow model gets ten minutes.
struct Http(ureq::Agent);

impl Http {
    fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .http_status_as_error(false)
            .build();
        Self(config.into())
    }
}

impl Transport for Http {
    fn post(
        &mut self,
        url: &str,
        headers: &[(&str, String)],
        body: &Value,
    ) -> Result<Value, String> {
        let mut request = self.0.post(url);
        for (name, value) in headers {
            request = request.header(*name, value.as_str());
        }
        let mut response = request.send_json(body).map_err(|e| match e {
            ureq::Error::Timeout(_) => "The model took too long to answer".to_owned(),
            e => format!("Could not reach {url}: {e}. Check the connection and the address."),
        })?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(64 << 20)
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_str(&text).map_err(|_| {
            let start: String = text.chars().take(200).collect();
            format!("HTTP {status}: {start}")
        })?;
        if (200..300).contains(&status) {
            Ok(value)
        } else {
            Err(describe_error(status, &value))
        }
    }
}

/// Runs one tool on the window's queue and waits for it.
fn run_in_window(
    sender: &mpsc::Sender<Job>,
    ctx: &egui::Context,
    name: &str,
    arguments: &Value,
) -> ToolResult {
    let (reply, answer) = mpsc::channel();
    let state = Arc::new(AtomicU8::new(QUEUED));
    let job = Job {
        name: name.to_owned(),
        arguments: arguments.clone(),
        reply,
        from: "Assistant",
        state: state.clone(),
    };
    if sender.send(job).is_err() {
        return ToolResult::error("FashionCraft is closing");
    }
    ctx.request_repaint();
    wait_for(&state, &answer)
}

/// What a tool does, for the transcript.
fn tool_label(name: &str) -> &'static str {
    match name {
        "scene_summary" => "Reading the scene",
        "command_list" => "Looking up commands",
        "command_run" => "Running a command",
        "draft_garment" => "Drafting a pattern",
        "set_fabric" => "Changing the fabric",
        "set_color" => "Changing the colour",
        "simulate" => "Simulating",
        "screenshot" => "Looking at the 3D view",
        "save_project" => "Saving the project",
        "export_garment" => "Exporting the garment",
        "piece_detail" => "Reading a piece",
        "select" => "Selecting",
        "create_piece" => "Creating a piece",
        "move_points" => "Moving points",
        "set_segment_length" => "Setting a length",
        "add_point" => "Adding a point",
        "add_dart" => "Adding a dart",
        "add_pleats" => "Adding pleats",
        "piece_edit" => "Editing a piece",
        "sew" => "Sewing segments",
        "seam_edit" => "Editing a seam",
        "place_piece" => "Placing a piece on the body",
        "set_fabric_parameters" => "Setting fabric parameters",
        "set_avatar" => "Changing the avatar",
        "set_pose" => "Posing the avatar",
        "grading" => "Grading sizes",
        "size_chart" => "Reading the size chart",
        "render" => "Rendering",
        "open_attachment" => "Opening an attached file",
        _ => "Using a tool",
    }
}

/// Markdown the transcript does not render, made readable: no emphasis marks or heading
/// hashes, bullets for list dashes.
fn tidy(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.replace("**", "").replace("__", "");
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                format!("{indent}• {rest}")
            } else if trimmed.starts_with('#') {
                trimmed.trim_start_matches('#').trim_start().to_owned()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A picture from a tool, as a texture for the transcript.
fn picture(ctx: &egui::Context, base64: &str, name: String) -> Option<egui::TextureHandle> {
    let bytes = fashioncraft::agent::unbase64(base64)?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

/// A name not yet used by `taken`: "photo.png", then "photo (2).png"…
fn unique_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|k| format!("{stem} ({k}){ext}"))
        .find(|n| !taken(n))
        .unwrap_or_default()
}

impl Atelier {
    /// Put a file with the next message.
    pub(super) fn assistant_attach(&mut self, mut attachment: Attachment) {
        let a = &self.assistant;
        attachment.name = unique_name(&attachment.name, &|n| {
            a.pending.iter().chain(&a.sent).any(|x| x.name == n)
        });
        self.assistant.attach_note = None;
        self.assistant.pending.push(attachment);
    }

    fn assistant_attach_paths(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            self.assistant_attach(attachments::from_path(&path));
        }
    }

    /// Attach the clipboard's files or picture; false when it holds neither.
    fn assistant_paste(&mut self) -> bool {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return false;
        };
        if let Ok(files) = clipboard.get().file_list()
            && !files.is_empty()
        {
            self.assistant_attach_paths(files);
            return true;
        }
        if let Ok(image) = clipboard.get_image() {
            match attachments::from_rgba(
                "pasted.png",
                image.width as u32,
                image.height as u32,
                image.bytes.into_owned(),
            ) {
                Ok(a) => self.assistant_attach(a),
                Err(e) => self.assistant.attach_note = Some(e),
            }
            return true;
        }
        false
    }

    /// Files dropped on the window, a paste into the message and the thumbnails of what is
    /// attached, before the window is drawn: a paste of files or a picture must not also put
    /// the clipboard's text (a file's name) into the message.
    fn assistant_take_files(&mut self, ctx: &egui::Context, input_id: egui::Id, paste_key: bool) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            self.assistant_attach(attachments::from_path(file.path()));
        }
        let focused = ctx.memory(|m| m.has_focus(input_id));
        let pasted = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))));
        if focused && (paste_key || pasted) && self.assistant_paste() {
            ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
        }
        for a in self.assistant.pending.iter().chain(&self.assistant.sent) {
            if !self.assistant.thumbs.contains_key(&a.name)
                && let Some(texture) = a
                    .picture()
                    .and_then(|png| picture(ctx, png, format!("attachment-{}", a.name)))
            {
                self.assistant.thumbs.insert(a.name.clone(), texture);
            }
        }
    }

    /// The files waiting to be sent, each with its picture or kind and a button to take it off.
    fn assistant_chips(&mut self, ui: &mut egui::Ui) {
        let lang = self.prefs.language;
        let mut remove = None;
        // One row of fixed height that scrolls sideways: attachments never make the window
        // taller.
        egui::ScrollArea::horizontal()
            .id_salt("assistant.chips")
            .max_height(CHIPS_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (k, a) in self.assistant.pending.iter().enumerate() {
                        let unreadable =
                            matches!(a.payload, attachments::Payload::Unreadable { .. });
                        egui::Frame::new()
                            .fill(ui.visuals().widgets.inactive.bg_fill)
                            .corner_radius(6.)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(texture) = self.assistant.thumbs.get(&a.name) {
                                        ui.add(
                                            egui::Image::new(texture)
                                                .max_height(28.)
                                                .max_width(40.),
                                        );
                                    }
                                    let name: String = a.name.chars().take(28).collect();
                                    let text = egui::RichText::new(name).small();
                                    let label = ui.label(if unreadable {
                                        text.color(ui.visuals().warn_fg_color)
                                    } else {
                                        text
                                    });
                                    label.on_hover_text(a.describe());
                                    if ui
                                        .small_button("×")
                                        .on_hover_text(i18n::tr(lang, "Remove"))
                                        .clicked()
                                    {
                                        remove = Some(k);
                                    }
                                });
                            });
                    }
                })
            });
        if let Some(k) = remove {
            let gone = self.assistant.pending.remove(k);
            self.assistant.thumbs.remove(&gone.name);
        }
    }

    fn assistant_provider(&self) -> Provider {
        Provider::from_id(&self.prefs.ai.provider).unwrap_or(Provider::Anthropic)
    }

    /// The key for the chosen provider, looked up once per provider.
    fn assistant_key(&mut self) -> (String, Source) {
        let provider = self.assistant_provider();
        match &self.assistant.key {
            Some((p, key, source)) if *p == provider => (key.clone(), source.clone()),
            _ => {
                let (key, source) = assistant_keys::find(provider);
                self.assistant.key = Some((provider, key.clone(), source.clone()));
                (key, source)
            }
        }
    }

    fn assistant_settings(&mut self) -> Settings {
        let (api_key, _) = self.assistant_key();
        Settings {
            provider: self.assistant_provider(),
            model: self.prefs.ai.model.clone(),
            base_url: self.prefs.ai.base_url.clone(),
            api_key,
            pause_every: self.prefs.ai.pause_every,
        }
    }

    /// Takes what the turn's thread reported.
    fn assistant_updates(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.assistant.worker else {
            return;
        };
        // One pass over the channel: a check for a finished thread must not take a message.
        let mut updates = Vec::new();
        let ended = loop {
            match worker.updates.try_recv() {
                Ok(update) => updates.push(update),
                Err(mpsc::TryRecvError::Empty) => break false,
                Err(mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        for update in &updates {
            self.assistant_log_update(update);
        }
        for update in updates {
            match update {
                Update::Event(Event::Text(text)) => {
                    self.assistant.replies.push(text.clone());
                    self.assistant.entries.push(Entry::Model(tidy(&text)))
                }
                Update::Event(Event::Notice(text)) => {
                    self.assistant.entries.push(Entry::Notice(text))
                }
                Update::Event(Event::Paused { steps }) => {
                    self.assistant.entries.push(Entry::Paused(steps))
                }
                Update::Event(Event::Usage(usage)) => self.assistant.usage.add(&usage),
                Update::Event(Event::ToolStarted { name, arguments }) => {
                    self.assistant.calls.push(fashioncraft::agent_eval::Call {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        is_error: false,
                    });
                    let mut detail = arguments.to_string();
                    if detail == "{}" {
                        detail.clear();
                    } else if detail.chars().count() > 90 {
                        detail = detail.chars().take(89).collect::<String>() + "…";
                    }
                    self.assistant.entries.push(Entry::Tool {
                        name,
                        detail,
                        outcome: None,
                        pictures: Vec::new(),
                    });
                }
                Update::Event(Event::ToolFinished {
                    name,
                    text,
                    images,
                    is_error,
                }) => {
                    if let Some(call) = self.assistant.calls.last_mut()
                        && call.name == name
                    {
                        call.is_error = is_error;
                    }
                    let count = self.assistant.entries.len();
                    let shown: Vec<egui::TextureHandle> = images
                        .iter()
                        .enumerate()
                        .filter_map(|(i, data)| {
                            picture(ctx, data, format!("assistant-{count}-{i}"))
                        })
                        .collect();
                    if let Some(Entry::Tool {
                        name: started,
                        outcome,
                        pictures,
                        ..
                    }) = self.assistant.entries.last_mut()
                        && *started == name
                        && outcome.is_none()
                    {
                        *outcome = Some((is_error, text));
                        *pictures = shown;
                    }
                }
                Update::Done(result) => {
                    if let Err(e) = result {
                        self.assistant.failure = Some(e.clone());
                        self.assistant.entries.push(Entry::Problem(e));
                    }
                    self.assistant.worker = None;
                }
            }
        }
        // A thread that ended without a word (it panicked) frees the window too.
        if ended && self.assistant.worker.take().is_some() {
            self.assistant.entries.push(Entry::Problem(
                i18n::tr(self.prefs.language, "The assistant stopped unexpectedly.").into(),
            ));
        }
    }

    /// Start a new conversation, forgetting the transcript.
    pub(super) fn assistant_new_conversation(&mut self) {
        let provider = self.assistant_provider();
        *self
            .assistant
            .conversation
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Conversation::new(provider);
        self.assistant.entries.clear();
        self.assistant.calls.clear();
        self.assistant.replies.clear();
        self.assistant.failure = None;
        self.assistant.sent.clear();
        self.assistant.usage = Totals::default();
        self.assistant.log = None;
    }

    /// Send `text` as if typed in the Assistant's box.
    pub(super) fn assistant_ask(&mut self, ctx: &egui::Context, text: &str) {
        self.assistant.open = true;
        self.assistant.input = text.to_owned();
        self.assistant_send(ctx);
    }

    fn assistant_send(&mut self, ctx: &egui::Context) {
        let mut text = self.assistant.input.trim().to_owned();
        if (text.is_empty() && self.assistant.pending.is_empty()) || self.assistant.worker.is_some()
        {
            return;
        }
        if text.is_empty() {
            text = i18n::tr(self.prefs.language, "Look at the attached files.").to_owned();
        }
        let settings = self.assistant_settings();
        if let Some(missing) = settings.missing() {
            self.assistant.failure = Some(missing.clone());
            self.assistant.entries.push(Entry::Problem(missing));
            self.assistant.show_connection = true;
            return;
        }
        {
            let mut conversation = self
                .assistant
                .conversation
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if conversation.provider() != settings.provider {
                if !conversation.is_empty() {
                    self.assistant.entries.push(Entry::Notice(format!(
                        "A new conversation with {}.",
                        settings.provider.label()
                    )));
                }
                *conversation = Conversation::new(settings.provider);
            }
        }
        let attached = std::mem::take(&mut self.assistant.pending);
        if !attached.is_empty() {
            self.assistant.entries.push(Entry::Attached(
                attached.iter().map(|a| a.name.clone()).collect(),
            ));
            self.assistant.sent.extend(attached.iter().cloned());
        }
        self.assistant.entries.push(Entry::You(text.clone()));
        self.assistant_log(json!({
            "you": text,
            "attached": attached.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
        }));
        self.assistant.input.clear();
        let (updates, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (conversation, sender, ctx, stopped) = (
            self.assistant.conversation.clone(),
            self.agent.sender(),
            ctx.clone(),
            stop.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("assistant".into())
            .spawn(move || {
                let mut transport: Box<dyn Transport> = match settings.provider {
                    Provider::CodexCli => Box::new(codex_cli::Cli(&stopped)),
                    Provider::ClaudeCli => Box::new(claude_cli::Cli(&stopped)),
                    Provider::GeminiCli => Box::new(gemini_cli::Cli(&stopped)),
                    Provider::AntigravityCli => Box::new(antigravity_cli::Cli(&stopped)),
                    _ => Box::new(Http::new()),
                };
                let mut conversation = conversation.lock().unwrap_or_else(|e| e.into_inner());
                let result = conversation.turn_with(
                    &settings,
                    &text,
                    &attached,
                    transport.as_mut(),
                    &mut |name, arguments| run_in_window(&sender, &ctx, name, arguments),
                    &mut |event| {
                        let _ = updates.send(Update::Event(event));
                        ctx.request_repaint();
                    },
                    &|| stopped.load(Ordering::Relaxed),
                );
                let _ = updates.send(Update::Done(result));
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.assistant.worker = Some(Worker {
                    stop,
                    updates: receiver,
                })
            }
            Err(e) => self.assistant.entries.push(Entry::Problem(e.to_string())),
        }
    }

    /// Provider, model, address and key, at the top of the window.
    fn assistant_connection(&mut self, ui: &mut egui::Ui) {
        let lang = self.prefs.language;
        let tr = |text| i18n::tr(lang, text);
        let provider = self.assistant_provider();
        super::workspace::group_title(ui, tr("Connection"));
        super::workspace::prop_row(ui, tr("Provider"), |ui| {
            egui::ComboBox::from_id_salt("assistant.provider")
                .selected_text(provider.label())
                .width(ui.available_width().min(230.))
                .show_ui(ui, |ui| {
                    for p in Provider::ALL {
                        if ui.selectable_label(p == provider, p.label()).clicked() && p != provider
                        {
                            self.prefs.ai.provider = p.id().into();
                            self.prefs.ai.model.clear();
                            self.prefs.ai.base_url.clear();
                            self.assistant.key_message = None;
                        }
                    }
                });
        });
        let provider = self.assistant_provider();
        super::workspace::prop_row(ui, tr("Model"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.prefs.ai.model)
                    .hint_text(provider.model_hint())
                    .desired_width(ui.available_width().min(230.)),
            );
        });
        if provider == Provider::ClaudeCli {
            ui.small(tr(
                "Uses your Claude Code login. Messages and requested screenshots go to Claude.",
            ));
            return;
        }
        if provider == Provider::CodexCli {
            ui.small(tr("Uses the ChatGPT account signed in with codex login. Messages and requested screenshots go to OpenAI."));
            return;
        }
        if provider == Provider::GeminiCli {
            ui.small(tr(
                "Uses your Gemini CLI login. Messages and requested screenshots go to Google.",
            ));
            return;
        }
        if provider == Provider::AntigravityCli {
            ui.small(tr(
                "Uses your Antigravity CLI (agy) login. Messages and requested screenshots go to Google Antigravity.",
            ));
            return;
        }
        super::workspace::prop_row(ui, tr("Address"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.prefs.ai.base_url)
                    .hint_text(provider.default_base_url())
                    .desired_width(ui.available_width().min(230.)),
            );
        });
        if provider.needs_key() {
            super::workspace::prop_row(ui, tr("API key"), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.assistant.key_input)
                        .password(true)
                        .hint_text(tr("paste a key to save it"))
                        .desired_width((ui.available_width() - 64.).clamp(80., 166.)),
                );
                if ui
                    .add_enabled(
                        !self.assistant.key_input.trim().is_empty(),
                        egui::Button::new(tr("Save")),
                    )
                    .clicked()
                {
                    self.assistant.key_message =
                        assistant_keys::save(provider, &self.assistant.key_input)
                            .err()
                            .map(|e| format!("Could not save the key: {e}"));
                    self.assistant.key_input.clear();
                    self.assistant.key = None;
                }
            });
        }
        if provider.needs_key() {
            let (_, source) = self.assistant_key();
            ui.horizontal(|ui| match source {
                Source::Saved => {
                    ui.small(tr(assistant_keys::SAVED_NOTE));
                    if ui.small_button(tr("Forget it")).clicked() {
                        self.assistant.key_message = assistant_keys::forget(provider)
                            .err()
                            .map(|e| format!("Could not remove the key: {e}"));
                        self.assistant.key = None;
                    }
                }
                Source::Environment(variable) => {
                    ui.small(format!("{} {variable}.", tr("Using the key in")));
                }
                Source::Missing => {
                    ui.small(tr("No key yet: create one in the provider's console."));
                }
            });
        }
        if let Some(message) = &self.assistant.key_message {
            ui.colored_label(
                ui.visuals().error_fg_color,
                egui::RichText::new(message).small(),
            );
        }
        ui.label(
            egui::RichText::new(tr(
                "Your messages, the scene's summary and screenshots go to this provider. A local \
                 model keeps them on this computer.",
            ))
            .small()
            .weak(),
        );
        super::workspace::prop_row(ui, tr("Review"), |ui| {
            let mut on = self.prefs.ai.pause_every > 0;
            if ui
                .checkbox(&mut on, tr("Pause every"))
                .on_hover_text(tr(
                    "Stop after this many steps so you can look before it goes on (off: it works \
                     until the job is done)",
                ))
                .changed()
            {
                self.prefs.ai.pause_every = if on { 20 } else { 0 };
            }
            if on {
                ui.add(
                    egui::DragValue::new(&mut self.prefs.ai.pause_every)
                        .range(1..=500)
                        .suffix(tr(" steps")),
                );
            }
        });
    }

    /// The conversation as text, for the clipboard: who said what, and each step with its
    /// result.
    fn assistant_transcript(&self) -> String {
        let mut out = String::new();
        for entry in &self.assistant.entries {
            match entry {
                Entry::You(t) => out.push_str(&format!("You: {t}\n\n")),
                Entry::Attached(names) => {
                    out.push_str(&format!("Attached: {}\n\n", names.join(", ")))
                }
                Entry::Model(t) => out.push_str(&format!("Assistant: {t}\n\n")),
                Entry::Tool {
                    name,
                    detail,
                    outcome,
                    ..
                } => {
                    out.push_str(&format!("· {name} {detail}\n"));
                    if let Some((error, text)) = outcome {
                        out.push_str(&format!(
                            "  {}{}\n",
                            if *error { "Error: " } else { "→ " },
                            text.replace('\n', "\n  ")
                        ));
                    }
                }
                Entry::Notice(t) | Entry::Problem(t) => out.push_str(&format!("({t})\n\n")),
                Entry::Paused(steps) => out.push_str(&format!("(Paused after {steps} steps)\n\n")),
            }
        }
        out
    }

    /// Adds one line to this conversation's log, `assistant-logs/<when it began>.jsonl` in the
    /// session folder: messages, tool calls and their results, without picture bytes, so a
    /// run can be looked at afterwards.
    fn assistant_log(&mut self, mut line: serde_json::Value) {
        let path = self.assistant.log.get_or_insert_with(|| {
            let when = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.output
                .join("assistant-logs")
                .join(format!("conversation-{when}.jsonl"))
        });
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.);
        line["at"] = json!((at * 10.).round() / 10.);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&*path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{line}");
        }
    }

    fn assistant_log_update(&mut self, update: &Update) {
        let line = match update {
            Update::Event(Event::Text(t)) => json!({"assistant": t}),
            Update::Event(Event::Notice(t)) => json!({"notice": t}),
            Update::Event(Event::Paused { steps }) => json!({"paused_after_steps": steps}),
            Update::Event(Event::Usage(u)) => json!({"usage": {
                "input": u.input, "cached": u.cached, "output": u.output, "cost_usd": u.cost_usd,
            }}),
            Update::Event(Event::ToolStarted { name, arguments }) => {
                json!({"tool": name, "arguments": arguments})
            }
            Update::Event(Event::ToolFinished {
                name,
                text,
                images,
                is_error,
            }) => {
                json!({"result": name, "error": is_error, "text": text, "pictures": images.len()})
            }
            Update::Done(Err(e)) => json!({"failed": e}),
            Update::Done(Ok(())) => json!({"turn": "ended"}),
        };
        self.assistant_log(line);
    }

    /// Tokens, money and the limit this conversation has used, beside the provider's name.
    fn assistant_usage_readout(&self, ui: &mut egui::Ui) {
        use fashioncraft::assistant::short_count;
        let lang = self.prefs.language;
        let u = &self.assistant.usage;
        if u.input + u.output == 0 {
            return;
        }
        let mut text = format!(
            "{} {} · {} {}",
            short_count(u.input - u.cached.min(u.input)),
            i18n::tr(lang, "in"),
            short_count(u.output),
            i18n::tr(lang, "out")
        );
        if u.cached > 0 {
            text.push_str(&format!(
                " · {} {}",
                short_count(u.cached),
                i18n::tr(lang, "cached")
            ));
        }
        if u.priced {
            text.push_str(&format!(" · ≈ ${:.2}", u.cost));
        }
        ui.label(egui::RichText::new(text).weak().size(11.))
            .on_hover_text(i18n::tr(
                lang,
                "This conversation's tokens, as the provider reported them: new input, output and \
             input read from its cache. Money is estimated from published prices, where they are \
             known (≈).",
            ));
        // The plan's limit where the provider reports it; otherwise how full the context is.
        let (share, hover) = match (&u.limit, u.context) {
            (Some(limit), _) => (
                limit.used_percent / 100.,
                format!(
                    "{:.0} % {} ({})",
                    limit.used_percent,
                    i18n::tr(lang, "of the usage limit used, as the provider reports it"),
                    limit.what
                ),
            ),
            (None, Some((used, window))) => (
                used as f64 / window.max(1) as f64,
                format!(
                    "{} {} / {} ({})",
                    i18n::tr(lang, "Context:"),
                    short_count(used),
                    short_count(window),
                    i18n::tr(
                        lang,
                        "how full the model's context window is; not the plan's limit, which \
                         this provider does not report"
                    )
                ),
            ),
            _ => return,
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(14., 14.), egui::Sense::hover());
        let painter = ui.painter();
        let c = rect.center();
        let r = 5.5;
        painter.circle_stroke(
            c,
            r,
            egui::Stroke::new(2., ui.visuals().widgets.inactive.bg_fill),
        );
        let share = share.clamp(0., 1.) as f32;
        let colour = if share > 0.85 {
            ui.visuals().warn_fg_color
        } else {
            super::workspace::accent(ui.visuals())
        };
        let points: Vec<egui::Pos2> = (0..=32)
            .map(|k| {
                let a =
                    -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * share * k as f32 / 32.;
                c + r * egui::vec2(a.cos(), a.sin())
            })
            .collect();
        if share > 0. {
            painter.add(egui::Shape::line(points, egui::Stroke::new(2., colour)));
        }
        response.on_hover_text(hover);
    }

    /// A turn is running (for the demo mode).
    pub(super) fn assistant_busy(&self) -> bool {
        self.assistant.worker.is_some()
    }

    /// How many entries the chat shows (for the demo mode).
    pub(super) fn assistant_entries(&self) -> usize {
        self.assistant.entries.len()
    }

    pub(super) fn assistant_window(&mut self, ctx: &egui::Context) {
        // Development builds can send a first message on their own, for end-to-end tests
        // against a stand-in server.
        if cfg!(debug_assertions) && !self.assistant.test_message_checked {
            self.assistant.test_message_checked = true;
            // Files to attach, as the paperclip and a drop attach them; sent with
            // FASHIONCRAFT_ASSISTANT_SEND when it is given too.
            if let Ok(files) = std::env::var("FASHIONCRAFT_ASSISTANT_ATTACH") {
                self.assistant.open = true;
                let paths = files
                    .split(';')
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from);
                self.assistant_attach_paths(paths.collect());
            }
            if let Ok(text) = std::env::var("FASHIONCRAFT_ASSISTANT_SEND") {
                self.assistant.open = true;
                // A test message never reaches a provider on the internet: with a mistyped or
                // unread preferences file the address would be the real API's.
                let address = self.assistant_settings().base_url().to_owned();
                if fashioncraft::assistant::is_local_address(&address) {
                    self.assistant.input = text;
                    self.assistant_send(ctx);
                } else {
                    self.assistant.entries.push(Entry::Problem(format!(
                        "FASHIONCRAFT_ASSISTANT_SEND talks only to a server on this computer, \
                         such as scripts/assistant-stand-in.py, and the address is {address}. \
                         Nothing was sent."
                    )));
                }
            }
        }
        self.assistant_updates(ctx);
        // Read every frame, so a ⌘V made elsewhere is not taken for one in the chat later.
        let paste_key = super::native_menu::take_paste_key();
        // A screenshot of the 3D view shows the garment, not this chat over it.
        let shooting = matches!(
            self.agent.running,
            Some((
                _,
                super::agent_hub::Pending::Screenshot {
                    whole_window: false,
                    ..
                }
            ))
        );
        if !self.assistant.open || shooting {
            return;
        }
        let lang = self.prefs.language;
        let tr = |text| i18n::tr(lang, text);
        let mut open = true;
        let busy = self.assistant.worker.is_some();
        let input_id = egui::Id::new("assistant.input");
        self.assistant_take_files(ctx, input_id, paste_key);
        let dragging_files = ctx.input(|i| !i.raw.hovered_files.is_empty());
        // First shown at the right, over the properties rather than the 3D view.
        let screen = ctx.content_rect();
        egui::Window::new(tr("Assistant"))
            .open(&mut open)
            .default_pos(egui::pos2(
                (screen.right() - 420.).max(screen.left()),
                screen.top() + 80.,
            ))
            .default_size([400., 560.])
            .min_width(320.)
            .resizable(true)
            .show(ctx, |ui| {
                // Every text in the chat can be selected and copied.
                ui.style_mut().interaction.selectable_labels = true;
                let settings = self.assistant_settings();
                let accent = super::workspace::accent(ui.visuals());
                // Who answers, with a dot that is green when ready and FashionCraft's colour
                // while a turn runs; the connection and a new chat as icons at the right.
                ui.horizontal(|ui| {
                    let model = match settings.model() {
                        "" if settings.provider == Provider::ClaudeCli => tr("Claude Code default"),
                        "" if settings.provider == Provider::CodexCli => tr("Codex default"),
                        "" if settings.provider == Provider::GeminiCli => tr("Gemini CLI default"),
                        "" if settings.provider == Provider::AntigravityCli => {
                            tr("Antigravity default")
                        }
                        "" => tr("no model chosen"),
                        _ => settings.model(),
                    };
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(8., 8.), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        3.5,
                        if busy {
                            accent
                        } else if settings.missing().is_some() {
                            ui.visuals().warn_fg_color
                        } else {
                            Color32::from_rgb(0x57, 0xab, 0x5a)
                        },
                    );
                    ui.label(
                        egui::RichText::new(settings.provider.label())
                            .strong()
                            .size(12.5),
                    );
                    self.assistant_usage_readout(ui);
                    ui.add(
                        egui::Label::new(egui::RichText::new(model).weak().size(12.)).truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let new_chat = ui.add_enabled_ui(!busy, |ui| {
                            super::icons::icon_only(
                                ui,
                                super::icons::Icon::PagePlus,
                                24.,
                                false,
                                tr("New chat"),
                            )
                        });
                        if new_chat.inner.clicked() {
                            self.assistant.entries.clear();
                            self.assistant.usage = Totals::default();
                            self.assistant.log = None;
                            *self
                                .assistant
                                .conversation
                                .lock()
                                .unwrap_or_else(|e| e.into_inner()) =
                                Conversation::new(settings.provider);
                        }
                        if super::icons::icon_only(
                            ui,
                            super::icons::Icon::Settings,
                            24.,
                            self.assistant.show_connection,
                            tr("Connection"),
                        )
                        .clicked()
                        {
                            self.assistant.show_connection = !self.assistant.show_connection;
                        }
                        if !self.assistant.entries.is_empty()
                            && super::icons::icon_only(
                                ui,
                                super::icons::Icon::Duplicate,
                                24.,
                                false,
                                tr("Copy conversation"),
                            )
                            .clicked()
                        {
                            ui.ctx().copy_text(self.assistant_transcript());
                            self.status = tr("Conversation copied").into();
                        }
                    });
                });
                if self.assistant.show_connection || settings.missing().is_some() {
                    ui.add_space(2.);
                    egui::Frame::new()
                        .fill(ui.visuals().extreme_bg_color)
                        .corner_radius(8.)
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.assistant_connection(ui);
                        });
                }
                ui.add_space(4.);
                // The message box's height as last drawn (with its attachments), so the
                // conversation above takes exactly the rest and the window never grows.
                let input_height_id = egui::Id::new("assistant.input_height");
                let mut resume = false;
                let input_height = ui
                    .ctx()
                    .data(|d| d.get_temp::<f32>(input_height_id))
                    .unwrap_or(112.);
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - input_height).max(80.))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.;
                        if self.assistant.entries.is_empty() {
                            ui.add_space(24.);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(tr("What should we make?"))
                                        .size(15.)
                                        .color(accent),
                                );
                                ui.add_space(4.);
                                ui.label(
                                    egui::RichText::new(tr(
                                        "Ask for something to do in this project, for example: \
                                         \"Draft a denim skirt for this avatar, simulate it and \
                                         show me how it falls.\"",
                                    ))
                                    .weak(),
                                );
                            });
                        }
                        for entry in &self.assistant.entries {
                            self.assistant_entry(ui, entry);
                        }
                        if !busy && matches!(self.assistant.entries.last(), Some(Entry::Paused(_)))
                        {
                            ui.vertical_centered(|ui| {
                                if ui
                                    .button(tr("Continue"))
                                    .on_hover_text(tr(
                                        "Go on from where it paused, with everything so far",
                                    ))
                                    .clicked()
                                {
                                    resume = true;
                                }
                            });
                        }
                        if busy {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(12.).color(accent));
                                let running = matches!(
                                    self.assistant.entries.last(),
                                    Some(Entry::Tool { outcome: None, .. })
                                );
                                let stopping = self
                                    .assistant
                                    .worker
                                    .as_ref()
                                    .is_some_and(|w| w.stop.load(Ordering::Relaxed));
                                ui.weak(if stopping {
                                    tr("Stopping after this step…")
                                } else if running {
                                    tr("Working in the project…")
                                } else {
                                    tr("Thinking…")
                                });
                            });
                        }
                    });
                if resume {
                    self.assistant.input = tr("Continue.").to_owned();
                    self.assistant_send(ctx);
                }
                let below_top = ui.cursor().top();
                ui.add_space(6.);
                // The message field and its send button share one rounded box, as chat apps
                // draw it. Enter sends; Shift+Enter starts a new line.
                let focused = ui.memory(|m| m.has_focus(input_id));
                let enter = focused
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                let stroke = if focused {
                    egui::Stroke::new(1., accent.gamma_multiply(0.8))
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                };
                let input_box = egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .stroke(stroke)
                    .corner_radius(10.)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        if dragging_files {
                            ui.label(
                                egui::RichText::new(tr("Drop the files here to attach them"))
                                    .color(accent),
                            );
                        }
                        if !self.assistant.pending.is_empty() {
                            self.assistant_chips(ui);
                        }
                        if let Some(note) = &self.assistant.attach_note {
                            ui.add(
                                egui::Label::new(egui::RichText::new(note).small().weak())
                                    .truncate(),
                            );
                        }
                        // A long message scrolls inside the box instead of growing it.
                        let field = egui::ScrollArea::vertical()
                            .id_salt("assistant.input_scroll")
                            .max_height(INPUT_MAX_HEIGHT)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.assistant.input)
                                        .id(input_id)
                                        .frame(egui::Frame::NONE)
                                        .hint_text(tr("Message the assistant (Enter sends)"))
                                        .desired_rows(2)
                                        .desired_width(f32::INFINITY),
                                )
                            })
                            .inner;
                        super::demo::mark(ui.ctx(), "assistant.input", field.rect);
                        ui.horizontal(|ui| {
                            let attach = ui
                                .small_button(tr("Attach…"))
                                .on_hover_text(tr(
                                    "Pictures, PDFs, text, CSV tables and DXF or SVG patterns. You can also drop files here or paste a picture with Cmd+V.",
                                ));
                            super::demo::mark(ui.ctx(), "assistant.attach", attach.rect);
                            if attach.clicked()
                                && let Some(paths) = rfd::FileDialog::new()
                                    .set_title(tr("Attach files"))
                                    .add_filter(
                                        tr("Pictures, documents, tables and patterns"),
                                        &[
                                            "png", "jpg", "jpeg", "bmp", "tga", "pdf", "txt", "md",
                                            "csv", "tsv", "json", "dxf", "svg",
                                        ],
                                    )
                                    .add_filter(tr("All files"), &["*"])
                                    .pick_files()
                            {
                                self.assistant_attach_paths(paths);
                            }
                            if ui
                                .small_button(tr("Paste"))
                                .on_hover_text(tr("Attach the picture or files on the clipboard"))
                                .clicked()
                                && !self.assistant_paste()
                            {
                                self.assistant.attach_note =
                                    Some(tr("The clipboard holds no picture or file.").into());
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if busy {
                                        if ui.button(tr("Stop")).clicked()
                                            && let Some(worker) = &self.assistant.worker
                                        {
                                            worker.stop.store(true, Ordering::Relaxed);
                                        }
                                    } else {
                                        let ready = !self.assistant.input.trim().is_empty()
                                            || !self.assistant.pending.is_empty();
                                        let send = ui.add_enabled(
                                            ready,
                                            egui::Button::new(
                                                egui::RichText::new(tr("Send"))
                                                    .strong()
                                                    .color(Color32::from_rgb(0x1b, 0x1b, 0x1f)),
                                            )
                                            .fill(accent)
                                            .stroke(egui::Stroke::NONE)
                                            .corner_radius(7.),
                                        );
                                        super::demo::mark(ui.ctx(), "assistant.send", send.rect);
                                        if (send.clicked() || enter) && ready {
                                            self.assistant_send(ctx);
                                            ctx.memory_mut(|m| m.request_focus(input_id));
                                        }
                                    }
                                },
                            );
                        });
                    });
                ui.label(
                    egui::RichText::new(tr("Changes can be undone with Undo."))
                        .small()
                        .weak(),
                );
                let _ = input_box;
                let used = ui.cursor().top() - below_top;
                if (used - input_height).abs() > 0.5 {
                    ui.ctx().data_mut(|d| d.insert_temp(input_height_id, used));
                    ui.ctx().request_repaint();
                }
            });
        self.assistant.open &= open;
    }

    fn assistant_entry(&self, ui: &mut egui::Ui, entry: &Entry) {
        let lang = self.prefs.language;
        let tr = |text| i18n::tr(lang, text);
        let accent = super::workspace::accent(ui.visuals());
        match entry {
            // The person's messages sit in a bubble at the right.
            Entry::You(text) => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    let width = ui.available_width() * 0.82;
                    egui::Frame::new()
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .corner_radius(egui::CornerRadius {
                            nw: 12,
                            ne: 12,
                            sw: 12,
                            se: 4,
                        })
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_max_width(width);
                            ui.add(egui::Label::new(text).wrap().selectable(true));
                        });
                });
            }
            // The files sent, at the right above the message: pictures as thumbnails.
            Entry::Attached(names) => {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for name in names {
                            match self.assistant.thumbs.get(name) {
                                Some(texture) => {
                                    ui.add(
                                        egui::Image::new(texture)
                                            .max_height(72.)
                                            .max_width(120.)
                                            .corner_radius(6.),
                                    )
                                    .on_hover_text(name);
                                }
                                None => {
                                    ui.label(egui::RichText::new(format!("📄 {name}")).small());
                                }
                            }
                        }
                    });
                });
            }
            // The assistant's answers read as text, under its name in FashionCraft's colour.
            Entry::Model(text) => {
                ui.label(
                    egui::RichText::new(tr("Assistant"))
                        .small()
                        .strong()
                        .color(accent),
                );
                ui.add(egui::Label::new(text).wrap().selectable(true));
            }
            // Each step in the project: a quiet row with its mark, name and detail.
            Entry::Tool {
                name,
                detail,
                outcome,
                pictures,
            } => {
                let (mark, color) = match outcome {
                    None => ("…", ui.visuals().weak_text_color()),
                    Some((false, _)) => ("✓", Color32::from_rgb(0x57, 0xab, 0x5a)),
                    Some((true, _)) => ("✗", ui.visuals().error_fg_color),
                };
                let frame = egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .corner_radius(6.)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(mark).color(color).strong());
                            ui.label(egui::RichText::new(tr(tool_label(name))).small());
                            if !detail.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(detail).small().monospace().weak(),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                        if let Some((true, text)) = outcome {
                            let shown: String = text.chars().take(1200).collect();
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(shown)
                                        .small()
                                        .color(ui.visuals().error_fg_color),
                                )
                                .wrap(),
                            );
                        }
                        for texture in pictures {
                            let size = texture.size_vec2();
                            let width = size.x.min(PICTURE_WIDTH).min(ui.available_width());
                            ui.add(
                                egui::Image::new(texture)
                                    .fit_to_exact_size(egui::vec2(
                                        width,
                                        width * size.y / size.x.max(1.),
                                    ))
                                    .corner_radius(6.),
                            );
                        }
                    });
                if let Some((_, text)) = outcome {
                    let shown: String = text.chars().take(1200).collect();
                    frame.response.on_hover_text(shown);
                }
            }
            Entry::Notice(text) => {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(text).small().italics().weak());
                });
            }
            Entry::Paused(steps) => {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {steps} {}",
                            i18n::tr(lang, "Paused for your review after"),
                            i18n::tr(lang, "steps.")
                        ))
                        .small()
                        .italics()
                        .weak(),
                    );
                });
            }
            Entry::Problem(text) => {
                egui::Frame::new()
                    .fill(ui.visuals().error_fg_color.gamma_multiply(0.12))
                    .corner_radius(6.)
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text)
                                    .small()
                                    .color(ui.visuals().error_fg_color),
                            )
                            .wrap(),
                        );
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tidy;

    #[test]
    fn the_transcript_marks_have_glyphs() {
        let ctx = eframe::egui::Context::default();
        super::workspace::style(&ctx);
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        for mark in ["✓", "✗", "…", "•"] {
            let font = eframe::egui::FontId::proportional(13.);
            assert!(
                ctx.fonts_mut(|f| f.has_glyphs(&font, mark)),
                "{mark} has no glyph"
            );
        }
    }

    #[test]
    fn markdown_marks_are_tidied() {
        assert_eq!(
            tidy("## Done\n**Denim** skirt:\n- waist 70 cm\n  * hip 102 cm"),
            "Done\nDenim skirt:\n• waist 70 cm\n  • hip 102 cm"
        );
    }
}
