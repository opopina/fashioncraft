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
    /// A text paste came since the paste key was last let go (see [`paste_asked`]).
    text_pasted: bool,
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
            text_pasted: false,
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
        /// The call's arguments (a plan's steps).
        arguments: Value,
        /// None while it runs.
        outcome: Option<(bool, String)>,
        pictures: Vec<egui::TextureHandle>,
        started: std::time::Instant,
        /// How long it took, once it ended.
        seconds: Option<f32>,
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
        "check_garment" => "Checking the garment",
        "camera" => "Turning the view",
        "set_sleeve" => "Setting a sleeve",
        "add_detail" => "Adding a detail",
        "add_buttons" => "Adding buttons",
        "add_internal_line" => "Adding a line",
        "bend_segment" => "Bending a segment",
        "transform_piece" => "Moving a piece",
        "measure" => "Measuring",
        "guide" => "Reading the guide",
        "more_tools" => "Looking up tools",
        "edit" => "Editing the pattern",
        _ => "Using a tool",
    }
}

/// A step's arguments as the folded steps show them: the values, not the JSON
/// ({"kind":"shirt"} is "shirt", {"seconds":8.0} "8", lists joined with commas).
fn step_detail(arguments: &Value) -> String {
    fn plain(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.as_f64().map_or_else(
                || n.to_string(),
                |x| format!("{}", (x * 100.).round() / 100.),
            ),
            Value::Bool(b) => b.to_string(),
            Value::Array(items) => items.iter().map(plain).collect::<Vec<_>>().join(", "),
            Value::Null => String::new(),
            Value::Object(_) => v.to_string(),
        }
    }
    match arguments {
        Value::Object(fields) => fields
            .values()
            .map(plain)
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        other => plain(other),
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

/// What the Assistant is doing, for its button in the main toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AssistantState {
    Idle,
    Working,
    /// Waiting for the person: paused for a review, or asking to be let do something.
    Waiting,
}

/// The main toolbar's Assistant button: a chat bubble and "Assistant" (the bubble alone when
/// the window is narrow), marked while the window is open; closed, a dot says it is working
/// (fuchsia) or waiting for an answer (amber).
fn toolbar_button(
    ui: &mut egui::Ui,
    label: &str,
    open: bool,
    state: AssistantState,
    compact: bool,
    tip: &str,
) -> egui::Response {
    let visuals = ui.visuals().clone();
    let accent = super::workspace::accent(&visuals);
    let font = egui::FontId::proportional(12.5);
    let galley = (!compact).then(|| {
        ui.painter()
            .layout_no_wrap(label.to_owned(), font.clone(), visuals.text_color())
    });
    let icon = 15.;
    let width = match &galley {
        Some(g) => 8. + icon + 6. + g.size().x + 10.,
        None => 28.,
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let (fill, stroke, ink) = if open {
            (
                accent.linear_multiply(0.24),
                egui::Stroke::new(1., accent),
                visuals.strong_text_color(),
            )
        } else {
            let fill = if response.hovered() {
                visuals.widgets.hovered.weak_bg_fill
            } else {
                Color32::TRANSPARENT
            };
            (
                fill,
                egui::Stroke::new(
                    1.,
                    visuals
                        .widgets
                        .noninteractive
                        .fg_stroke
                        .color
                        .gamma_multiply(0.35),
                ),
                visuals.text_color(),
            )
        };
        painter.rect(rect, 3., fill, stroke, egui::StrokeKind::Inside);
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(
                if compact {
                    rect.center().x
                } else {
                    rect.left() + 8. + icon / 2.
                },
                rect.center().y,
            ),
            egui::vec2(icon, icon),
        );
        super::icons::glyph(painter, icon_rect, super::icons::Icon::Chat, ink, 1.4);
        if let Some(g) = galley {
            let at = egui::pos2(icon_rect.right() + 6., rect.center().y - g.size().y / 2.);
            painter.galley_with_override_text_color(at, g, ink);
        }
        if !open && state != AssistantState::Idle {
            let dot = if state == AssistantState::Working {
                accent
            } else {
                visuals.warn_fg_color
            };
            let at = rect.right_top() + egui::vec2(-3., 4.);
            painter.circle(at, 5., dot, egui::Stroke::new(2., visuals.panel_fill));
        }
    }
    response.on_hover_text(tip)
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

    /// Attach the clipboard's files or picture; false when it holds neither. With `tell`, a
    /// clipboard that cannot be read says why under the chips (not after a text paste, which
    /// egui reads on its own).
    fn assistant_paste(&mut self, tell: bool) -> bool {
        // Under Wayland the desktop's own clipboard first: arboard reaches it only through
        // XWayland, which may not carry pictures (wl-paste comes with wl-clipboard).
        let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
        if wayland && let Some(clip) = clip_by_command(true, &mut run_clipboard_tool) {
            return self.assistant_attach_clip(clip);
        }
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(e) => {
                if cfg!(target_os = "linux")
                    && let Some(clip) = clip_by_command(wayland, &mut run_clipboard_tool)
                {
                    return self.assistant_attach_clip(clip);
                }
                if tell {
                    self.assistant.attach_note =
                        Some(format!("The clipboard could not be read: {e}"));
                }
                return false;
            }
        };
        if let Ok(files) = clipboard.get().file_list()
            && !files.is_empty()
        {
            self.assistant_attach_paths(files);
            return true;
        }
        match clipboard.get_image() {
            Ok(image) => {
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
            // Text only (it pastes as text) or nothing.
            Err(arboard::Error::ContentNotAvailable) => {}
            Err(e) => {
                if tell {
                    self.assistant.attach_note = Some(format!(
                        "The picture on the clipboard could not be read: {e}"
                    ))
                }
            }
        }
        // X11 without a picture arboard could read: xclip, when it is there.
        if cfg!(target_os = "linux")
            && !wayland
            && let Some(clip) = clip_by_command(false, &mut run_clipboard_tool)
        {
            return self.assistant_attach_clip(clip);
        }
        false
    }

    fn assistant_attach_clip(&mut self, clip: Clip) -> bool {
        match clip {
            Clip::Files(paths) => self.assistant_attach_paths(paths),
            Clip::Image(bytes) => {
                self.assistant_attach(attachments::from_bytes("pasted.png", &bytes))
            }
        }
        true
    }

    /// Files dropped on the window, a paste into the message and the thumbnails of what is
    /// attached, before the window is drawn: a paste of files or a picture must not also put
    /// the clipboard's text (a file's name) into the message.
    fn assistant_take_files(&mut self, ctx: &egui::Context, input_id: egui::Id, paste_key: bool) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            self.assistant_attach(attachments::from_path(file.path()));
        }
        let asked = ctx.input(|i| {
            paste_asked(
                &i.events,
                &mut self.assistant.text_pasted,
                cfg!(target_os = "macos"),
            )
        });
        let text_came = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))));
        // ⌘V / Ctrl+V attaches a picture or files wherever the focus is while this window is
        // open: nothing else takes them. Text still pastes as text, and a text paste into
        // another field (a piece's name, say) is left to that field.
        let other_field = text_came && ctx.memory(|m| m.focused().is_some_and(|id| id != input_id));
        if !other_field && (paste_key || asked) && self.assistant_paste(!text_came) {
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
                Update::Event(Event::Answered { .. }) => {}
                Update::Event(Event::ToolStarted { name, arguments }) => {
                    self.assistant.calls.push(fashioncraft::agent_eval::Call {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        is_error: false,
                    });
                    let mut detail = step_detail(&arguments);
                    if detail.chars().count() > 90 {
                        detail = detail.chars().take(89).collect::<String>() + "…";
                    }
                    self.assistant.entries.push(Entry::Tool {
                        name,
                        detail,
                        arguments,
                        outcome: None,
                        pictures: Vec::new(),
                        started: std::time::Instant::now(),
                        seconds: None,
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
                        name: began,
                        outcome,
                        pictures,
                        started,
                        seconds,
                        ..
                    }) = self.assistant.entries.last_mut()
                        && *began == name
                        && outcome.is_none()
                    {
                        *outcome = Some((is_error, text));
                        *pictures = shown;
                        *seconds = Some(started.elapsed().as_secs_f32());
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
        // What the model knows of the scene, so a small request needs no scene_summary first.
        let note = self.agent_scene_note(ctx);
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
            "scene_note": note,
        }));
        if let Some(note) = &note {
            text = fashioncraft::assistant::with_note(&text, note);
        }
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
            Update::Event(Event::Answered { seconds }) => {
                json!({"model_seconds": (seconds * 10.).round() / 10.})
            }
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

    /// The header under the window's title: a dot (green ready, fuchsia working, amber
    /// waiting), who answers, the model (a menu while idle), then the usage ring with this
    /// conversation's tokens, and copy, settings and new chat while idle.
    fn assistant_header(&mut self, ui: &mut egui::Ui, settings: &Settings, busy: bool) {
        let lang = self.prefs.language;
        let tr = |text| i18n::tr(lang, text);
        let state = self.assistant_state();
        let visuals = ui.visuals().clone();
        let accent = super::workspace::accent(&visuals);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.;
            let colour = match state {
                AssistantState::Working => accent,
                AssistantState::Waiting => visuals.warn_fg_color,
                AssistantState::Idle if settings.missing().is_some() => visuals.warn_fg_color,
                AssistantState::Idle => Color32::from_rgb(0x57, 0xab, 0x5a),
            };
            let (dot, _) = ui.allocate_exact_size(egui::vec2(7., 7.), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.5, colour);
            ui.label(
                egui::RichText::new(settings.provider.short_label())
                    .strong()
                    .size(12.5),
            )
            .on_hover_text(settings.provider.label());
            let model = match settings.model().trim() {
                "" => tr("Default").to_owned(),
                id => fashioncraft::assistant::model_name(id),
            };
            if state == AssistantState::Waiting {
                ui.label(
                    egui::RichText::new(tr("waiting for your answer"))
                        .size(12.)
                        .color(visuals.warn_fg_color),
                );
            } else if busy {
                egui::Frame::new()
                    .fill(visuals.widgets.inactive.bg_fill)
                    .corner_radius(3.)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&model).size(12.).weak());
                    });
            } else {
                self.assistant_model_menu(ui, settings, &model);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                if !busy {
                    if super::icons::icon_only(
                        ui,
                        super::icons::Icon::PagePlus,
                        28.,
                        false,
                        tr("New chat"),
                    )
                    .clicked()
                    {
                        self.assistant_new_conversation();
                    }
                    if super::icons::icon_only(
                        ui,
                        super::icons::Icon::Sliders,
                        28.,
                        self.assistant.show_connection,
                        tr("Assistant settings"),
                    )
                    .clicked()
                    {
                        self.assistant.show_connection = !self.assistant.show_connection;
                    }
                    if !self.assistant.entries.is_empty()
                        && super::icons::icon_only(
                            ui,
                            super::icons::Icon::Duplicate,
                            28.,
                            false,
                            tr("Copy the conversation"),
                        )
                        .clicked()
                    {
                        ui.ctx().copy_text(self.assistant_transcript());
                        self.status = tr("Conversation copied").into();
                    }
                }
                self.assistant_usage_ring(ui);
            });
        });
    }

    /// The model menu: the provider's default, the models it suggests, the one set by hand,
    /// and Other… for the connection settings.
    fn assistant_model_menu(&mut self, ui: &mut egui::Ui, settings: &Settings, shown: &str) {
        let lang = self.prefs.language;
        let current = self.prefs.ai.model.trim().to_owned();
        let mut options: Vec<String> = vec![String::new()];
        options.extend(
            settings
                .provider
                .suggested_models()
                .iter()
                .map(|m| (*m).to_owned()),
        );
        if !options.contains(&current) {
            options.push(current.clone());
        }
        let mut chosen = None;
        let mut other = false;
        egui::ComboBox::from_id_salt("assistant.model")
            .selected_text(egui::RichText::new(shown).size(12.))
            .width(0.)
            .show_ui(ui, |ui| {
                for id in &options {
                    let name = if id.is_empty() {
                        i18n::tr(lang, "Default").to_owned()
                    } else {
                        fashioncraft::assistant::model_name(id)
                    };
                    if ui.selectable_label(*id == current, name).clicked() {
                        chosen = Some(id.clone());
                    }
                }
                ui.separator();
                if ui
                    .selectable_label(false, i18n::tr(lang, "Other…"))
                    .clicked()
                {
                    other = true;
                }
            })
            .response
            .on_hover_text(i18n::tr(lang, "Model"));
        if let Some(id) = chosen.filter(|id| *id != current) {
            self.prefs.ai.model = id;
            self.prefs_dirty = true;
        }
        if other {
            self.assistant.show_connection = true;
        }
    }

    /// A ring for the plan's limit (or, where the provider does not report one, how full the
    /// context is) and this conversation's tokens in thousands; details on hover.
    fn assistant_usage_ring(&self, ui: &mut egui::Ui) {
        use fashioncraft::assistant::short_count;
        let lang = self.prefs.language;
        let u = &self.assistant.usage;
        let total = u.input + u.output;
        let share = match (&u.limit, u.context) {
            (Some(limit), _) => Some(limit.used_percent / 100.),
            (None, Some((used, window))) => Some(used as f64 / window.max(1) as f64),
            _ => None,
        };
        if total == 0 && share.is_none() {
            return;
        }
        let decimal = |text: String| {
            if lang == i18n::Language::Spanish {
                text.replace('.', ",")
            } else {
                text
            }
        };
        let text = if total > 0 {
            decimal(short_count(total))
        } else {
            format!("{:.0} %", share.unwrap_or(0.) * 100.)
        };
        let mut hover = format!(
            "{} {} · {} {} · {} {}",
            short_count(u.input - u.cached.min(u.input)),
            i18n::tr(lang, "in"),
            short_count(u.output),
            i18n::tr(lang, "out"),
            short_count(u.cached),
            i18n::tr(lang, "cached")
        );
        if u.priced {
            hover.push_str(&format!(" · ≈ ${:.2}", u.cost));
        }
        match (&u.limit, u.context) {
            (Some(limit), _) => hover.push_str(&format!(
                "\n{:.0} % {} ({})",
                limit.used_percent,
                i18n::tr(lang, "of the usage limit used, as the provider reports it"),
                limit.what
            )),
            (None, Some((used, window))) => hover.push_str(&format!(
                "\n{} {} / {}",
                i18n::tr(lang, "Context:"),
                short_count(used),
                short_count(window)
            )),
            _ => {}
        }
        ui.label(
            egui::RichText::new(text)
                .size(12.)
                .color(ui.visuals().weak_text_color()),
        )
        .on_hover_text(&hover);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(16., 16.), egui::Sense::hover());
        let painter = ui.painter();
        let (c, r) = (rect.center(), 6.);
        painter.circle_stroke(
            c,
            r,
            egui::Stroke::new(2.2, ui.visuals().widgets.inactive.bg_fill),
        );
        let share = share.unwrap_or(0.).clamp(0., 1.) as f32;
        if share > 0. {
            let colour = if share > 0.85 {
                ui.visuals().warn_fg_color
            } else {
                super::workspace::accent(ui.visuals())
            };
            let points: Vec<egui::Pos2> = (0..=32)
                .map(|k| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * share * k as f32 / 32.;
                    c + r * egui::vec2(a.cos(), a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(2.2, colour)));
        }
        response.on_hover_text(hover);
    }

    /// What the Assistant is doing now.
    pub(super) fn assistant_state(&self) -> AssistantState {
        if self.assistant_waiting() {
            AssistantState::Waiting
        } else if self.assistant_busy() {
            AssistantState::Working
        } else {
            AssistantState::Idle
        }
    }

    /// The Assistant waits for the person: paused for a review.
    fn assistant_waiting(&self) -> bool {
        !self.assistant_busy() && matches!(self.assistant.entries.last(), Some(Entry::Paused(_)))
    }

    /// The main toolbar's Assistant button; a click opens or closes the window.
    pub(super) fn assistant_toolbar_button(
        &self,
        ui: &mut egui::Ui,
        compact: bool,
    ) -> egui::Response {
        let lang = self.prefs.language;
        let open = self.assistant.open;
        let state = self.assistant_state();
        let tip = match (open, state) {
            (true, _) => "Close the Assistant",
            (false, AssistantState::Working) => "Open the Assistant; it is working",
            (false, AssistantState::Waiting) => "Open the Assistant; it is waiting for your answer",
            (false, AssistantState::Idle) => "Open the Assistant",
        };
        toolbar_button(
            ui,
            i18n::tr(lang, "Assistant"),
            open,
            state,
            compact,
            i18n::tr(lang, tip),
        )
    }

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
            .default_size([420., (screen.height() - 140.).clamp(560., 860.)])
            .min_width(320.)
            .resizable(true)
            .show(ctx, |ui| {
                // Every text in the chat can be selected and copied.
                ui.style_mut().interaction.selectable_labels = true;
                let settings = self.assistant_settings();
                let accent = super::workspace::accent(ui.visuals());
                self.assistant_header(ui, &settings, busy);
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
                        self.assistant_turns(ui, busy);
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

    /// The chat turn by turn: the person's messages as they are; each answer under the
    /// assistant's name once, with what it said before acting, its plan, its steps folded into
    /// one line, the check after simulating, and what it said at the end.
    fn assistant_turns(&self, ui: &mut egui::Ui, busy: bool) {
        let entries = &self.assistant.entries;
        let answers = |e: &Entry| matches!(e, Entry::Model(_) | Entry::Tool { .. });
        let mut i = 0;
        while i < entries.len() {
            if !answers(&entries[i]) {
                self.assistant_entry(ui, &entries[i]);
                i += 1;
                continue;
            }
            let end = (i..entries.len())
                .find(|&k| matches!(entries[k], Entry::You(_) | Entry::Attached(_)))
                .unwrap_or(entries.len());
            self.assistant_answer(ui, i, &entries[i..end], busy && end == entries.len());
            i = end;
        }
    }

    /// One answer: `key` tells its folded steps apart; `running` while its turn goes on.
    fn assistant_answer(&self, ui: &mut egui::Ui, key: usize, answer: &[Entry], running: bool) {
        let lang = self.prefs.language;
        let accent = super::workspace::accent(ui.visuals());
        ui.label(
            egui::RichText::new(i18n::tr(lang, "Assistant"))
                .size(11.)
                .strong()
                .color(accent),
        );
        let tools: Vec<usize> = answer
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, Entry::Tool { .. }))
            .map(|(k, _)| k)
            .collect();
        let (Some(&first), Some(&last)) = (tools.first(), tools.last()) else {
            for entry in answer {
                self.assistant_said(ui, entry);
            }
            return;
        };
        for entry in &answer[..first] {
            self.assistant_said(ui, entry);
        }
        let plan = answer.iter().rev().find_map(|e| match e {
            Entry::Tool {
                name, arguments, ..
            } if name == "plan" => Some(arguments),
            _ => None,
        });
        if let Some(plan) = plan {
            plan_card(ui, lang, plan, running);
        }
        self.assistant_steps(ui, key, &answer[first..=last], running);
        let template = &self.project.template;
        let piece_name = |id: &str| {
            template
                .panels
                .iter()
                .find(|p| p.id == id)
                .map_or_else(|| id.to_owned(), |p| p.name.clone())
        };
        check_card(ui, lang, answer, &piece_name);
        for entry in &answer[last + 1..] {
            self.assistant_said(ui, entry);
        }
    }

    /// What the assistant wrote, without its name (the answer shows it once), or a notice.
    fn assistant_said(&self, ui: &mut egui::Ui, entry: &Entry) {
        match entry {
            Entry::Model(text) => {
                ui.add(egui::Label::new(text).wrap().selectable(true));
            }
            other => self.assistant_entry(ui, other),
        }
    }

    /// An answer's steps in the project, folded into "7 steps in the project · 14 s"; a click
    /// opens them: each with its mark, name and detail, a failure's message, what the model
    /// said between them, and their pictures. Open while the turn runs.
    fn assistant_steps(&self, ui: &mut egui::Ui, key: usize, span: &[Entry], running: bool) {
        let lang = self.prefs.language;
        let visuals = ui.visuals().clone();
        let steps: Vec<&Entry> = span
            .iter()
            .filter(|e| matches!(e, Entry::Tool { name, .. } if name != "plan"))
            .collect();
        if steps.is_empty() {
            return;
        }
        let seconds: f32 = steps
            .iter()
            .map(|e| match e {
                Entry::Tool {
                    seconds, started, ..
                } => seconds.unwrap_or_else(|| started.elapsed().as_secs_f32()),
                _ => 0.,
            })
            .sum();
        let id = egui::Id::new(("assistant.steps", key));
        let open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(running);
        let green = Color32::from_rgb(0x7c, 0xc4, 0x7f);
        egui::Frame::new()
            .fill(visuals.extreme_bg_color)
            .stroke(visuals.widgets.noninteractive.bg_stroke)
            .corner_radius(6.)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (rect, header) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 30.),
                    egui::Sense::click(),
                );
                let painter = ui.painter();
                let c = egui::pos2(rect.left() + 16., rect.center().y);
                let chevron = if open {
                    vec![
                        c + egui::vec2(-4., -2.),
                        c + egui::vec2(0., 2.),
                        c + egui::vec2(4., -2.),
                    ]
                } else {
                    vec![
                        c + egui::vec2(-2., -4.),
                        c + egui::vec2(2., 0.),
                        c + egui::vec2(-2., 4.),
                    ]
                };
                painter.add(egui::Shape::line(
                    chevron,
                    egui::Stroke::new(1.6, visuals.weak_text_color()),
                ));
                let label = format!(
                    "{} {}",
                    steps.len(),
                    i18n::tr(
                        lang,
                        if steps.len() == 1 {
                            "step in the project"
                        } else {
                            "steps in the project"
                        }
                    )
                );
                painter.text(
                    egui::pos2(rect.left() + 28., rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(12.),
                    visuals.text_color(),
                );
                painter.text(
                    egui::pos2(rect.right() - 10., rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    format!("{seconds:.0} s"),
                    egui::FontId::proportional(12.),
                    visuals.weak_text_color(),
                );
                if header.clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(id, !open));
                }
                if !open {
                    return;
                }
                ui.add(egui::Separator::default().spacing(0.));
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.;
                        for entry in span {
                            match entry {
                                Entry::Tool { name, .. } if name == "plan" => {}
                                Entry::Tool {
                                    name,
                                    detail,
                                    outcome,
                                    pictures,
                                    ..
                                } => {
                                    ui.horizontal(|ui| {
                                        match outcome {
                                            None => {
                                                ui.add(
                                                    egui::Spinner::new()
                                                        .size(11.)
                                                        .color(super::workspace::accent(&visuals)),
                                                );
                                            }
                                            Some((false, _)) => {
                                                ui.label(
                                                    egui::RichText::new("✓").color(green).strong(),
                                                );
                                            }
                                            Some((true, _)) => {
                                                ui.label(
                                                    egui::RichText::new("✗")
                                                        .color(visuals.error_fg_color)
                                                        .strong(),
                                                );
                                            }
                                        }
                                        ui.label(
                                            egui::RichText::new(i18n::tr(lang, tool_label(name)))
                                                .size(12.),
                                        );
                                        if !detail.is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(detail)
                                                        .size(11.)
                                                        .monospace()
                                                        .weak(),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    });
                                    if let Some((true, text)) = outcome {
                                        let shown: String = text.chars().take(400).collect();
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(shown)
                                                    .size(11.)
                                                    .color(visuals.error_fg_color),
                                            )
                                            .wrap(),
                                        );
                                    }
                                    if !pictures.is_empty() {
                                        ui.horizontal(|ui| {
                                            for texture in pictures {
                                                ui.add(
                                                    egui::Image::new(texture)
                                                        .max_height(64.)
                                                        .corner_radius(4.),
                                                );
                                            }
                                        });
                                    }
                                }
                                Entry::Model(text) => {
                                    ui.label(egui::RichText::new(text).size(11.5).italics().weak());
                                }
                                other => self.assistant_entry(ui, other),
                            }
                        }
                    });
            });
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
                ..
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

/// The plan the model showed (its latest plan call): each step ticked when done, the next
/// one marked while the turn runs, and how many are done.
fn plan_card(ui: &mut egui::Ui, lang: i18n::Language, plan: &Value, running: bool) {
    let steps: Vec<&str> = plan["steps"]
        .as_array()
        .map(|s| s.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if steps.is_empty() {
        return;
    }
    let done = (plan["done"].as_u64().unwrap_or(0) as usize).min(steps.len());
    let visuals = ui.visuals().clone();
    let green = Color32::from_rgb(0x7c, 0xc4, 0x7f);
    egui::Frame::new()
        .fill(visuals.extreme_bg_color)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(6.)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(i18n::tr(lang, "Plan"))
                        .size(11.5)
                        .strong()
                        .weak(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let all = done == steps.len();
                    let text = if all {
                        format!(
                            "{done} {} {} {}",
                            i18n::tr(lang, "of"),
                            steps.len(),
                            i18n::tr(lang, "done")
                        )
                    } else {
                        format!("{done} {} {}", i18n::tr(lang, "of"), steps.len())
                    };
                    ui.label(egui::RichText::new(text).size(11.5).color(if all {
                        green
                    } else {
                        visuals.weak_text_color()
                    }));
                });
            });
            for (k, step) in steps.iter().enumerate() {
                ui.horizontal(|ui| {
                    if k < done {
                        ui.label(egui::RichText::new("✓").color(green).strong());
                        ui.add(egui::Label::new(*step).wrap());
                    } else if k == done && running {
                        ui.add(
                            egui::Spinner::new()
                                .size(11.)
                                .color(super::workspace::accent(&visuals)),
                        );
                        ui.add(egui::Label::new(egui::RichText::new(*step).strong()).wrap());
                    } else {
                        ui.label(egui::RichText::new("○").weak());
                        ui.add(egui::Label::new(egui::RichText::new(*step).weak()).wrap());
                    }
                });
            }
        });
}

/// One of check_garment's problems as the check card says it: short, with the piece's name,
/// the common kinds in the interface's language ("Strained 14.6 %: panel_1 middle left near
/// armhole-left, 129 cm up (…)" is "Tensión 14,6 % en camisa delantero").
fn short_problem(
    lang: i18n::Language,
    problem: &str,
    piece_name: &dyn Fn(&str) -> String,
) -> String {
    let tr = |t| i18n::tr(lang, t);
    let number = |n: &str| {
        if lang == i18n::Language::Spanish {
            n.replace('.', ",")
        } else {
            n.to_owned()
        }
    };
    // The piece: the first word of the place, a piece's id.
    let piece = |rest: &str| {
        let id = rest.split_whitespace().next().unwrap_or("");
        piece_name(id.trim_end_matches(','))
    };
    if let Some(rest) = problem.strip_prefix("Strained ")
        && let Some((share, place)) = rest.split_once(" %: ")
    {
        return format!(
            "{} {} % {} {}",
            tr("Strain"),
            number(share),
            tr("on"),
            piece(place)
        );
    }
    if let Some(rest) = problem.strip_prefix("Tight: ")
        && let Some((kpa, place)) = rest.split_once(" kPa on the body at ")
    {
        return format!(
            "{} {} kPa {} {}",
            tr("Tight:"),
            number(kpa),
            tr("on"),
            piece(place)
        );
    }
    if problem.starts_with("Still moving") {
        return tr("Still moving: simulate longer").to_owned();
    }
    let short: String = problem.chars().take(60).collect();
    if problem.chars().count() > 60 {
        format!("{short}…")
    } else {
        short
    }
}

/// The check after simulating, when the answer ran check_garment: its last picture of the
/// garment (from the check's view, the camera or a screenshot) beside the results: seams
/// closed, nothing inside the body, and each other problem.
fn check_card(
    ui: &mut egui::Ui,
    lang: i18n::Language,
    answer: &[Entry],
    piece_name: &dyn Fn(&str) -> String,
) {
    let Some(text) = answer.iter().rev().find_map(|e| match e {
        Entry::Tool {
            name,
            outcome: Some((false, text)),
            ..
        } if name == "check_garment" => Some(text),
        _ => None,
    }) else {
        return;
    };
    let Some(report) = text
        .find('{')
        .zip(text.rfind('}'))
        .and_then(|(a, b)| serde_json::from_str::<Value>(&text[a..=b]).ok())
    else {
        return;
    };
    let picture = answer.iter().rev().find_map(|e| match e {
        Entry::Tool { pictures, .. } => pictures.last(),
        _ => None,
    });
    let visuals = ui.visuals().clone();
    let green = Color32::from_rgb(0x7c, 0xc4, 0x7f);
    let amber = visuals.warn_fg_color;
    let total = report["seams"]["total"].as_u64().unwrap_or(0);
    let open = report["seams"]["open"]
        .as_u64()
        .or_else(|| report["seams"]["open"].as_array().map(|a| a.len() as u64))
        .unwrap_or(0);
    let problems: Vec<&str> = report["problems"]
        .as_array()
        .map(|p| p.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut lines: Vec<(bool, String)> = Vec::new();
    if total > 0 {
        lines.push((
            open == 0,
            format!(
                "{} {} {total} {}",
                total - open.min(total),
                i18n::tr(lang, "of"),
                i18n::tr(lang, "seams closed")
            ),
        ));
    }
    let inside = problems.iter().any(|p| p.contains("inside the body"));
    if !inside {
        lines.push((true, i18n::tr(lang, "Nothing inside the body").to_owned()));
    }
    for p in problems.iter().filter(|p| !p.starts_with("Seam ")) {
        lines.push((false, short_problem(lang, p, piece_name)));
    }
    egui::Frame::new()
        .fill(visuals.extreme_bg_color)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(6.)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if let Some(texture) = picture {
                    ui.add(
                        egui::Image::new(texture)
                            .max_size(egui::vec2(112., 140.))
                            .corner_radius(4.),
                    );
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 5.;
                    ui.label(
                        egui::RichText::new(i18n::tr(lang, "Review after simulating"))
                            .size(11.5)
                            .strong()
                            .weak(),
                    );
                    for (good, line) in &lines {
                        ui.horizontal(|ui| {
                            let colour = if *good { green } else { amber };
                            ui.label(
                                egui::RichText::new(if *good { "✓" } else { "⚠" }).color(colour),
                            );
                            ui.add(
                                egui::Label::new(egui::RichText::new(line).size(12.).color(colour))
                                    .wrap(),
                            );
                        });
                    }
                });
            });
        });
}

/// What a desktop clipboard tool found: files copied in the file manager, or a picture's
/// bytes (PNG, JPEG…).
#[derive(Debug, PartialEq)]
enum Clip {
    Files(Vec<PathBuf>),
    Image(Vec<u8>),
}

/// The clipboard through the desktop's own tool, for Linux: `wl-paste` under Wayland, `xclip`
/// under X11. Files (text/uri-list) first, then a picture (PNG first). `run` runs a command
/// line and gives its output; None when the tool is missing, fails or has nothing.
fn clip_by_command(wayland: bool, run: &mut dyn FnMut(&[&str]) -> Option<Vec<u8>>) -> Option<Clip> {
    let get = |kind: &str| -> Vec<String> {
        if wayland {
            ["wl-paste", "--no-newline", "--type", kind]
                .map(String::from)
                .to_vec()
        } else {
            ["xclip", "-selection", "clipboard", "-t", kind, "-o"]
                .map(String::from)
                .to_vec()
        }
    };
    let listed = if wayland {
        run(&["wl-paste", "--list-types"])?
    } else {
        run(&["xclip", "-selection", "clipboard", "-t", "TARGETS", "-o"])?
    };
    let listed = String::from_utf8_lossy(&listed).into_owned();
    let kinds: Vec<&str> = listed.lines().map(str::trim).collect();
    let mut fetch = |kind: &str| {
        let args = get(kind);
        run(&args.iter().map(String::as_str).collect::<Vec<_>>()).filter(|b| !b.is_empty())
    };
    if kinds.contains(&"text/uri-list")
        && let Some(list) = fetch("text/uri-list")
    {
        let files = file_uris(&String::from_utf8_lossy(&list));
        if !files.is_empty() {
            return Some(Clip::Files(files));
        }
    }
    let picture = kinds
        .iter()
        .find(|k| **k == "image/png")
        .or_else(|| kinds.iter().find(|k| k.starts_with("image/")))?;
    fetch(picture).map(Clip::Image)
}

/// The local files a text/uri-list names (file:// URIs, percent-escapes decoded).
fn file_uris(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("file://"))
        // file://host/path: a local file's host is empty or localhost.
        .map(|rest| rest.strip_prefix("localhost").unwrap_or(rest))
        .map(|path| {
            let bytes = path.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                let hex = |b: u8| (b as char).to_digit(16);
                match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
                    (b'%', Some(a), Some(b)) if hex(*a).is_some() && hex(*b).is_some() => {
                        out.push((hex(*a).unwrap() * 16 + hex(*b).unwrap()) as u8);
                        i += 3;
                    }
                    (b, ..) => {
                        out.push(b);
                        i += 1;
                    }
                }
            }
            PathBuf::from(String::from_utf8_lossy(&out).into_owned())
        })
        .collect()
}

/// Runs a clipboard tool for at most 2 seconds; its output when it succeeds.
fn run_clipboard_tool(args: &[&str]) -> Option<Vec<u8>> {
    use std::io::Read;
    let (program, rest) = args.split_first()?;
    let mut child = std::process::Command::new(program)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (send, read) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = send.send(stdout.read_to_end(&mut bytes).map(|_| bytes));
    });
    let bytes = read.recv_timeout(std::time::Duration::from_secs(2));
    if bytes.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    bytes.ok()?.ok().filter(|_| status.success())
}

/// Whether this frame's input asks for a paste the Assistant should look at for pictures and
/// files. A text paste does ([`egui::Event::Paste`]). On Linux and Windows egui-winit makes
/// Ctrl+V (Shift+Insert on Windows, a Paste key) a text paste only when the clipboard holds
/// text, and swallows the key press, so a clipboard with only a picture or files gives nothing
/// but the key's release: that release asks too, unless a text paste came with its press
/// (`text_pasted`). On macOS the AppKit key monitor tells instead ([`super::native_menu`]).
fn paste_asked(events: &[egui::Event], text_pasted: &mut bool, mac: bool) -> bool {
    let pasted = events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
    *text_pasted |= pasted;
    if mac {
        return pasted;
    }
    let paste_key = |key: egui::Key, modifiers: egui::Modifiers| {
        (key == egui::Key::V && modifiers.command)
            || key == egui::Key::Paste
            || (cfg!(windows) && key == egui::Key::Insert && modifiers.shift)
    };
    let released: Vec<(egui::Key, egui::Modifiers)> = events
        .iter()
        .filter_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: false,
                modifiers,
                ..
            } => Some((*key, *modifiers)),
            _ => None,
        })
        .collect();
    let asked = released.iter().any(|&(k, m)| paste_key(k, m));
    // The key's release ends this paste either way, even with Ctrl let go first.
    let ended = asked
        || released
            .iter()
            .any(|&(k, _)| matches!(k, egui::Key::V | egui::Key::Insert));
    let by_text = ended && std::mem::take(text_pasted);
    pasted || (asked && !by_text)
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
    fn linux_clipboard_tools_give_files_first_then_a_picture() {
        use super::{Clip, clip_by_command, file_uris};
        use std::path::PathBuf;
        // A fake wl-paste / xclip: what the clipboard offers, and each kind's bytes.
        let clipboard = |offers: &'static [(&'static str, &'static [u8])]| {
            move |args: &[&str]| -> Option<Vec<u8>> {
                let asked = args.join(" ");
                if asked.contains("--list-types") || asked.contains("TARGETS") {
                    return Some(
                        offers
                            .iter()
                            .map(|(k, _)| *k)
                            .collect::<Vec<_>>()
                            .join("\n")
                            .into_bytes(),
                    );
                }
                offers
                    .iter()
                    .find(|(k, _)| asked.contains(&format!(" {k}")))
                    .map(|(_, b)| b.to_vec())
            }
        };
        let png: &[u8] = b"\x89PNG...";
        for wayland in [true, false] {
            let mut picture = clipboard(&[("text/html", b"<img>"), ("image/png", b"\x89PNG...")]);
            assert_eq!(
                clip_by_command(wayland, &mut picture),
                Some(Clip::Image(png.to_vec()))
            );
            let mut files = clipboard(&[
                ("text/uri-list", b"# copied\nfile:///home/ana/Fotos/vestido%20azul.jpg\nfile://localhost/tmp/patr\xc3\xb3n.dxf\n"),
                ("text/plain", b"/home/ana/Fotos/vestido azul.jpg"),
            ]);
            assert_eq!(
                clip_by_command(wayland, &mut files),
                Some(Clip::Files(vec![
                    PathBuf::from("/home/ana/Fotos/vestido azul.jpg"),
                    PathBuf::from("/tmp/patrón.dxf")
                ]))
            );
            let mut text = clipboard(&[("text/plain", b"hola")]);
            assert_eq!(clip_by_command(wayland, &mut text), None);
            // No tool, or nothing copied.
            assert_eq!(clip_by_command(wayland, &mut |_: &[&str]| None), None);
            let mut jpeg = clipboard(&[("image/jpeg", b"\xff\xd8")]);
            assert_eq!(
                clip_by_command(wayland, &mut jpeg),
                Some(Clip::Image(b"\xff\xd8".to_vec()))
            );
        }
        assert_eq!(
            file_uris("https://x.org/a.png\nfile:///a%2Fb"),
            vec![PathBuf::from("/a/b")]
        );
    }

    #[test]
    fn ctrl_v_with_a_picture_or_files_is_seen_on_linux_and_windows() {
        use eframe::egui::{Event, Key, Modifiers};
        let release = |modifiers| Event::Key {
            key: Key::V,
            physical_key: Some(Key::V),
            pressed: false,
            repeat: false,
            modifiers,
        };
        let text = || Event::Paste("a.png".into());
        let mut seen = false;
        // Only a picture on the clipboard: the press gives nothing, the release asks.
        assert!(super::paste_asked(
            &[release(Modifiers::COMMAND)],
            &mut seen,
            false
        ));
        // Text: the paste asks once, its release does not ask again (same frame or the next).
        assert!(super::paste_asked(&[text()], &mut seen, false));
        assert!(!super::paste_asked(
            &[release(Modifiers::COMMAND)],
            &mut seen,
            false
        ));
        assert!(super::paste_asked(
            &[text(), release(Modifiers::COMMAND)],
            &mut seen,
            false
        ));
        assert!(!seen);
        // A V typed without Ctrl, or any key on macOS (its key monitor tells), asks nothing.
        assert!(!super::paste_asked(
            &[release(Modifiers::NONE)],
            &mut seen,
            false
        ));
        assert!(!super::paste_asked(
            &[release(Modifiers::COMMAND)],
            &mut seen,
            true
        ));
        // Ctrl let go before V after a text paste: the next picture paste still asks.
        let mut seen = false;
        assert!(super::paste_asked(&[text()], &mut seen, false));
        assert!(!super::paste_asked(
            &[release(Modifiers::NONE)],
            &mut seen,
            false
        ));
        assert!(super::paste_asked(
            &[release(Modifiers::COMMAND)],
            &mut seen,
            false
        ));
    }

    #[test]
    fn check_problems_read_short_with_the_piece_name() {
        use super::{i18n::Language, short_problem};
        let name = |id: &str| {
            if id == "panel_1" {
                "shirt front".to_owned()
            } else {
                id.to_owned()
            }
        };
        let es = |p: &str| short_problem(Language::Spanish, p, &name);
        assert_eq!(
            es(
                "Strained 14.6 %: panel_1 middle left near armhole-left, 129 cm up (1.2 % of the piece over 8 %)"
            ),
            "Tensión 14,6 % en shirt front"
        );
        assert_eq!(
            es("Tight: 6.4 kPa on the body at panel_1 top right near armhole-right, 144 cm up"),
            "Aprieta 6,4 kPa en shirt front"
        );
        assert_eq!(
            es("Still moving (16.0 cm/s on average): simulate longer"),
            "Aún se mueve: simula más"
        );
        let long = "x".repeat(80);
        assert_eq!(es(&long).chars().count(), 61);
        assert_eq!(
            short_problem(Language::English, "Strained 9.0 %: panel_2 top", &name),
            "Strain 9.0 % on panel_2"
        );
    }

    #[test]
    fn step_details_read_as_values() {
        use serde_json::json;
        assert_eq!(super::step_detail(&json!({})), "");
        assert_eq!(super::step_detail(&json!({"kind": "shirt"})), "shirt");
        assert_eq!(super::step_detail(&json!({"seconds": 8.0})), "8");
        assert_eq!(
            super::step_detail(&json!({"pieces": ["panel_1", "panel_2"], "rgb": [239, 236, 230]})),
            "panel_1, panel_2 · 239, 236, 230"
        );
    }

    #[test]
    fn markdown_marks_are_tidied() {
        assert_eq!(
            tidy("## Done\n**Denim** skirt:\n- waist 70 cm\n  * hip 102 cm"),
            "Done\nDenim skirt:\n• waist 70 cm\n  • hip 102 cm"
        );
    }
}
