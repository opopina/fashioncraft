//! Window › Assistant: a chat with Claude, ChatGPT, Gemini or a local model that works in the
//! open project. The conversation runs on its own thread (see `fashioncraft::assistant`); its
//! tools run here, on the window's queue, like an MCP client's (see `agent_hub`).
use super::agent_hub::{Job, QUEUED, wait_for};
use super::assistant_keys::{self, Source};
use super::*;
use fashioncraft::agent::ToolResult;
use fashioncraft::assistant::{Conversation, Event, Provider, Settings, Transport, describe_error};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc,
};

/// Widest a screenshot shows in the transcript, points.
const PICTURE_WIDTH: f32 = 300.;

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
        }
    }
}

enum Entry {
    You(String),
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

impl Atelier {
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
        for update in updates {
            match update {
                Update::Event(Event::Text(text)) => {
                    self.assistant.entries.push(Entry::Model(tidy(&text)))
                }
                Update::Event(Event::Notice(text)) => {
                    self.assistant.entries.push(Entry::Notice(text))
                }
                Update::Event(Event::ToolStarted { name, arguments }) => {
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

    fn assistant_send(&mut self, ctx: &egui::Context) {
        let text = self.assistant.input.trim().to_owned();
        if text.is_empty() || self.assistant.worker.is_some() {
            return;
        }
        let settings = self.assistant_settings();
        if let Some(missing) = settings.missing() {
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
        self.assistant.entries.push(Entry::You(text.clone()));
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
                let mut http = Http::new();
                let mut conversation = conversation.lock().unwrap_or_else(|e| e.into_inner());
                let result = conversation.turn(
                    &settings,
                    &text,
                    &mut http,
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
    }

    pub(super) fn assistant_window(&mut self, ctx: &egui::Context) {
        // Development builds can send a first message on their own, for end-to-end tests
        // against a stand-in server.
        if cfg!(debug_assertions) && !self.assistant.test_message_checked {
            self.assistant.test_message_checked = true;
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
                let settings = self.assistant_settings();
                let accent = super::workspace::accent(ui.visuals());
                // Who answers, with a dot that is green when ready and FashionCraft's colour
                // while a turn runs; the connection and a new chat as icons at the right.
                ui.horizontal(|ui| {
                    let model = match settings.model() {
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
                let input_height = 112.;
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
                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .stroke(stroke)
                    .corner_radius(10.)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.assistant.input)
                                .id(input_id)
                                .frame(egui::Frame::NONE)
                                .hint_text(tr("Message the assistant (Enter sends)"))
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(tr("Changes can be undone with Undo."))
                                    .small()
                                    .weak(),
                            );
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
                                        let ready = !self.assistant.input.trim().is_empty();
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
                                        if (send.clicked() || enter) && ready {
                                            self.assistant_send(ctx);
                                            ctx.memory_mut(|m| m.request_focus(input_id));
                                        }
                                    }
                                },
                            );
                        });
                    });
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
