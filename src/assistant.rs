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
/// A message of the person's up to this many characters, with nothing attached, is a small
/// request (make it red, rename the front): the CLIs that can are asked for low reasoning effort
/// for its first [`LIGHT_ROUNDS`] steps, then for their usual effort if the job turns out bigger.
pub const LIGHT_REQUEST_CHARS: usize = 80;
pub const LIGHT_ROUNDS: usize = 3;
/// Where a note of FashionCraft's starts after the person's words (see [`with_note`]).
const NOTE_START: &str = "\n\n(FashionCraft: ";

/// The person's message with a note of FashionCraft's for the model after it, such as what
/// changed in the scene since the model last looked.
pub fn with_note(text: &str, note: &str) -> String {
    format!("{text}{NOTE_START}{note})")
}

/// A picture fewer than this many rounds of tools after the one before, in one turn, comes with
/// [`PICTURE_NOTE`]: pictures cost time and tokens, and the numbers say most of it.
pub const PICTURE_GAP_ROUNDS: usize = 3;
/// What a picture taken too soon after another says to the model (the picture still comes).
pub const PICTURE_NOTE: &str = "(Note from FashionCraft: another picture so soon. Each costs time \
     and about 1-2k tokens, and check_garment, scene_summary and the seam gaps already give the \
     numbers. Take one only to verify the finished result, compare with the person's picture, \
     place a problem the numbers cannot, or when the person asks; one view per check.)";

/// A safety stop against a runaway loop: this many rounds of tools in one turn. A turn has no
/// other limit; [`Settings::pause_every`] pauses it for a review when the person wants that.
pub const SAFETY_ROUNDS: usize = 400;
/// …or the same call failing this many times in a row.
pub const SAME_FAILURES: usize = 5;
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

FashionCraft at a glance (the drafted bases' names, so a request needs no looking up first)
- tshirt: front, back, sleeves cut in one (segments neck-left, neck-bottom, neck-right, \
shoulder-*, sleeve-top-*, cuff-*, sleeve-under-*, side-*, hem). shirt: front, back (neck-*, \
shoulder-*, armhole-*, side-*, hem), sleeve-right, sleeve-left (cap-right, cap-left, \
underarm-right, underarm-left, hem). dress: front, back, as the shirt's. skirt: front, back \
(waist, hip-*, side-*, hem). trousers: front-right, front-left, back-right, back-left (waist, \
centre, crotch, inseam, hem, outseam-upper, outseam-lower). Right and left are the pattern's: \
the shirt's sleeve-right sits on the avatar's left arm (set_sleeve's side is the avatar's).
- Which tool: colour set_color; fabric set_fabric (weight, stretch: set_fabric_parameters); \
longer or shorter move_points on the hem's end points of every body piece, the same amount; \
pocket, collar, placket add_detail; buttons add_buttons; a sleeve set_sleeve; another garment \
draft_garment (mode add); the same garment redrafted with other measurements draft_garment \
mode replace (it replaces the garment: say so); how it fits simulate, then check_garment; a \
picture screenshot; sizes grading; saving and exporting save_project, export_garment.

Small requests
- A small change (a colour, a fabric, a name, one setting, a view) is its one tool call and a \
one-line reply: no planning, no guide, no screenshot, no simulating or check_garment unless the \
change alters the drape (a colour never does). set_color and set_fabric without pieces change \
the whole garment, so they need no scene_summary first.

How to work
1. Look: scene_summary (pieces, segments and lengths, seams, fabrics, avatar, selection, \
warnings); piece_detail before editing a piece. When the person's message ends with a note from \
FashionCraft that the scene is unchanged since your last scene_summary, or with what changed, \
you already know the scene: do not call it again.
2. Plan the steps; for a job of several steps show them with plan, and call it again as they \
finish. Ask only when something essential is missing; otherwise choose sensibly and say what \
you chose.
3. Act with the most specific tool: draft_garment for a new base, then create_piece, move_points, \
set_segment_length, add_point, add_dart, add_pleats, piece_edit, sew, seam_edit, place_piece, \
set_fabric, set_fabric_parameters, set_color, set_avatar, set_pose, grading. command_list shows \
the menu commands an agent can run; command_run runs one, with target to select first. Let \
FashionCraft compute geometry rather than placing raw points: add_detail (patch pocket, band \
collar, placket), bend_segment, add_internal_line, add_buttons, transform_piece, style_line, pin, \
grab, trace_on_body. Any other edit: edit_ops, then edit. The windows are tools too: \
simulation_settings, camera, render_settings, avatar_maker, pose_joints, measure_fabric, \
points_of_measure, measure. Your tool list holds the common tools; more_tools lists the others \
and gives their inputs, and use_tool runs one. guide(topic) gives pattern-making basics and how FashionCraft does \
each thing.
4. Simulate after changing the pattern, fabric, avatar or placement (5 to 15 s), then always call \
check_garment: it lists open seams, strained cloth, tight spots, cloth inside the body and a drape \
still moving, each with where it is. Fix what it reports, simulate and check again until it says \
ok or you can explain what is left. If simulate or check_garment says the simulation is \
paused because cloth starts inside other cloth or the body, deal with that first: undo the \
change that caused it, move that piece clear or reset the arrangement, then simulate again. \
Another garment the person asks for (a shirt to go with \
the skirt) is added with draft_garment, mode add, the default: the garment there stays and the \
new one is worn over it (simulate the one there first, so the new one goes over it as it hangs). \
draft_garment cuts each base from a fitting fabric and says which: tell the person that fabric \
and the colour, or the ones you set. Use mode replace only when they ask for a new garment instead of the \
current one, and say first that the current one will go. Never place a sleeve by moving it in cm: every sleeve goes in with set_sleeve, which sews and \
places it. Never change the avatar's pose or body unless the person asks for it. Give pieces \
names that say what they are (piece_edit rename), such as front bodice, waistband or skirt \
front.
5. Pictures (screenshot, camera with screenshot, check_garment with a view) cost time and about \
1-2k tokens each, and check_garment, scene_summary and the seam gaps already say most of what \
you need. Take one only to verify the finished result at the end of a job, to compare with a \
picture the person attached, to locate a problem the numbers show but cannot place, or when the \
person asks: one view per check unless they ask for more, never after every step, never \
several angles by default. Describe what you actually see.
6. Report briefly what changed, with the key numbers, and what is left.

Attachments
- The person can attach pictures (photos, sketches, swatches), PDFs, text, CSV tables and \
patterns. Pictures and PDFs reach you as they are when your connection reads them; otherwise the \
message says so: tell the person plainly. A pattern or 3D model comes with FashionCraft's \
summary (pieces and sizes) and a picture of its pieces; open_attachment opens it in the \
importer for the person to confirm.
- To make a garment from a photo or sketch: describe what you see (silhouette, pieces, seams, \
closures, length, fit, fabric), draft the nearest base with draft_garment, adjust it, simulate, \
take one screenshot from the picture's angle, compare it with the picture and adjust again. Say what you could not match.

Limits and honesty
- Every mouse tool and form window has a tool equivalent (command_list says which). You cannot \
draw freehand, open file dialogs, read files the person did not attach (open attached ones with \
open_attachment), place image graphics, edit textures, or animate. Say so plainly and name the \
menu path the person can use.
- Drafted bases are simplified blocks (straight armholes and sleeve caps), not production \
patterns. Fabric presets are indicative; for manufacturing, calibrate on the real fabric \
(measure_fabric, or Materials > Measure fabric).
- Never claim a fit, a closed seam or a look you have not checked with check_garment or a \
picture. When a tool fails, read its message and change something; never repeat the same \
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
    /// The short name the Assistant's header shows.
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Anthropic => "Claude",
            Self::OpenAi => "ChatGPT",
            Self::CodexCli => "Codex",
            Self::ClaudeCli => "Claude Code",
            Self::Gemini => "Gemini",
            Self::GeminiCli => "Gemini CLI",
            Self::AntigravityCli => "Antigravity",
            Self::Local => "Local model",
        }
    }
    /// Models the Assistant's model menu offers, besides the provider's default and whatever
    /// was typed in the connection settings.
    pub fn suggested_models(self) -> &'static [&'static str] {
        match self {
            Self::Anthropic => &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"],
            // Claude Code takes these aliases for its current models.
            Self::ClaudeCli => &["opus", "sonnet", "haiku"],
            _ => &[],
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
    /// Pause for a review after this many rounds of tools (0: never).
    pub pause_every: usize,
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
            return Some("Choose a model in the Assistant's settings, under Connection".into());
        }
        if self.provider.needs_key() && self.api_key.trim().is_empty() {
            let variable = self.provider.key_variable().unwrap_or("the API key");
            return Some(format!(
                "Add your {} API key in the Assistant's settings, under Connection (or set {variable})",
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
    /// The turn paused for a review after `steps` rounds of tools ([`Settings::pause_every`]);
    /// a message such as "Continue." resumes it with everything so far.
    Paused { steps: usize },
    /// What one request used.
    Usage(Usage),
    /// How long the provider took to answer one request, in seconds (the model's thinking
    /// and writing, and for a CLI its start).
    Answered { seconds: f64 },
    /// A request failed for a moment (the network, a busy service) and is sent again:
    /// `attempt` of `of` retries, after what went wrong, readable.
    Retrying {
        attempt: usize,
        of: usize,
        error: String,
    },
}

/// How many times a request that failed for a moment is sent again, and how long to wait
/// before each.
pub const NETWORK_RETRIES: usize = 2;
const RETRY_WAIT_SECONDS: [u64; NETWORK_RETRIES] = [3, 10];

/// What a turn's error starts with when the provider could not be reached after the
/// retries; the window translates it.
pub const UNREACHABLE: &str = "Could not reach the model after 3 tries:";

/// Whether a failed request is worth sending again: the connection or the service failed for
/// a moment (the CLI says so, or the network or a busy server does), not a login, a key, a
/// limit or the request itself.
pub fn retryable(error: &str) -> bool {
    let e = error.to_lowercase().replace(": ", ":");
    if e.contains("\"retryable\":true") {
        return true;
    }
    if e.contains("\"retryable\":false") || e.contains("request too large") {
        return false;
    }
    const SIGNS: &[&str] = &[
        "network issue",
        "network error",
        "network is unreachable",
        "timed out",
        "connection reset",
        "connection refused",
        "connection closed",
        "connection aborted",
        "broken pipe",
        "temporarily unavailable",
        "no such host",
        "dial tcp",
        "stream disconnected",
        "unexpected eof",
        "http 500",
        "http 502",
        "http 503",
        "http 504",
        "http 529",
        "overloaded",
        "service unavailable",
        "bad gateway",
    ];
    SIGNS.iter().any(|sign| e.contains(sign))
}

/// A failed request as a person reads it: without the JSON a CLI appends ("AGY_ERROR: {…}"),
/// without the same sentence twice, at most about 300 characters.
pub fn readable_error(error: &str) -> String {
    let mut text = error.split("AGY_ERROR:").next().unwrap_or(error).to_owned();
    // JSON objects in the text go: they are for programs.
    while let Some(start) = text.find('{') {
        let mut depth = 0usize;
        let mut end = None;
        for (k, c) in text[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + k + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(end) => text.replace_range(start..end, ""),
            None => text.truncate(start),
        }
    }
    let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // A CLI's own message repeated in its exit line: keep the first.
    if let Some(cut) = text.find(" (") {
        let first = text[..cut].trim_end_matches('.').to_lowercase();
        if first.len() > 12 && text[cut..].to_lowercase().contains(&first) {
            text.truncate(cut);
        }
    }
    let text = text.trim_end_matches(['(', ' ', ':']).to_owned();
    if text.chars().count() > 300 {
        format!("{}…", text.chars().take(299).collect::<String>())
    } else {
        text
    }
}

/// What one request used, as the provider reported it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    /// Input tokens, cached ones included.
    pub input: u64,
    /// Of those, read from the provider's cache.
    pub cached: u64,
    pub output: u64,
    /// US dollars: from [`PRICES`] for an API key, or as a CLI reports it.
    pub cost_usd: Option<f64>,
    /// How much of the plan's usage limit is used, when the provider says.
    pub limit: Option<Limit>,
    /// The model's context window in tokens, when [`PRICES`] knows the model.
    pub context_window: Option<u64>,
}

/// A plan's usage limit, as a provider reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub used_percent: f64,
    /// What it is, for the person: "Codex plan, 5 h window".
    pub what: String,
}

/// Prices in US dollars per million tokens (input, output, cache read) and the context window
/// in tokens, by model id prefix, most specific first. Anthropic's published API rates
/// (2026-11-03); cache writes cost 1.25 × input. Models not listed show tokens without money:
/// add them here.
pub const PRICES: &[(&str, f64, f64, f64, u64)] = &[
    ("claude-fable-5-2", 8., 40., 0.2, 1_000_000),
    ("claude-mythos-5-2", 8., 40., 0.2, 1_000_000),
    ("claude-fable-5-1", 10., 50., 1., 1_000_000),
    ("claude-fable-5", 10., 50., 1., 1_000_000),
    ("claude-opus-5-5", 4., 20., 0.2, 1_000_000),
    ("claude-opus-5", 5., 25., 0.5, 1_000_000),
    ("claude-opus-4-8", 5., 25., 0.5, 1_000_000),
    ("claude-opus-4-7", 5., 25., 0.5, 1_000_000),
    ("claude-opus-4-6", 5., 25., 0.5, 1_000_000),
    ("claude-sonnet-5-5", 2., 10., 0.1, 1_000_000),
    ("claude-sonnet-5", 2., 10., 0.2, 1_000_000),
    ("claude-sonnet-4-6", 3., 15., 0.3, 1_000_000),
    ("claude-haiku-5-5", 0.1, 0.5, 0.01, 1_000_000),
    ("claude-haiku-4-5", 1., 5., 0.1, 200_000),
];

/// What a request used, from the provider's answer: Anthropic's `usage`, the OpenAI-style
/// `usage` (which the CLIs' answers are given too), and what a CLI adds (`x_cost_usd`,
/// `x_rate_limit`).
pub fn usage_of(provider: Provider, model: &str, answer: &Value) -> Option<Usage> {
    let u = &answer["usage"];
    if !u.is_object() && answer.get("x_cost_usd").is_none() && answer.get("x_rate_limit").is_none()
    {
        return None;
    }
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let (input, cached, written, output) = if provider == Provider::Anthropic {
        let (read, write) = (
            n(&u["cache_read_input_tokens"]),
            n(&u["cache_creation_input_tokens"]),
        );
        (
            n(&u["input_tokens"]) + read + write,
            read,
            write,
            n(&u["output_tokens"]),
        )
    } else {
        (
            n(&u["prompt_tokens"]),
            n(&u["prompt_tokens_details"]["cached_tokens"]),
            0,
            n(&u["completion_tokens"]),
        )
    };
    let price = PRICES.iter().find(|p| model.starts_with(p.0));
    let cost_usd = if provider.is_cli() || provider == Provider::Local {
        answer["x_cost_usd"].as_f64()
    } else {
        price.map(|(_, i, o, r, _)| {
            let plain = input.saturating_sub(cached + written) as f64;
            (plain * i + written as f64 * i * 1.25 + cached as f64 * r + output as f64 * o) / 1e6
        })
    };
    let limit = answer["x_rate_limit"]["used_percent"]
        .as_f64()
        .map(|used_percent| Limit {
            used_percent,
            what: answer["x_rate_limit"]["what"]
                .as_str()
                .unwrap_or("plan limit")
                .to_owned(),
        });
    Some(Usage {
        input,
        cached,
        output,
        cost_usd,
        limit,
        context_window: price.map(|p| p.4),
    })
}

/// Whether a request needs the larger model when the model is picked by the size of the task:
/// a conversation's first request, one with files, a long one, or one that draws, adds to or
/// fixes a garment. Changing a colour or a fabric, simulating or saving does not.
pub fn large_task(text: &str, attachments: usize, first: bool) -> bool {
    if first || attachments > 0 || text.chars().count() > 160 {
        return true;
    }
    let text = text.to_lowercase();
    const LARGE: &[&str] = &[
        // English
        "draft", "make", "create", "design", "new", "fix", "repair", "why", "add", "pattern",
        "garment", "shirt", "skirt", "trousers", "pants", "dress", "jacket", "coat", "sleeve",
        "pocket", "collar", "dart", "pleat", "hood", "cuff", "fit", // Spanish
        "dibuja", "haz", "crea", "diseñ", "nuev", "arregl", "repar", "por qué", "añad", "agreg",
        "patrón", "prenda", "camis", "falda", "pantal", "vestido", "chaqueta", "abrigo", "manga",
        "bolsillo", "cuello", "pinza", "pliegue", "capucha", "puño", "ajust", "entalla",
    ];
    LARGE.iter().any(|word| text.contains(word))
}

/// A model id as people say it: "claude-sonnet-5-5" Sonnet 5.5, "opus" Opus; others as they are.
pub fn model_name(id: &str) -> String {
    let id = id.trim();
    let capital = |w: &str| {
        let mut c = w.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect::<String>())
            .unwrap_or_default()
    };
    if let Some(rest) = id.strip_prefix("claude-") {
        let mut parts = rest.split('-');
        if let Some(family) = parts.next() {
            // "5-5", not a date stamp after it.
            let version: Vec<&str> = parts
                .take_while(|p| p.len() <= 2 && p.chars().all(|c| c.is_ascii_digit()))
                .collect();
            return if version.is_empty() {
                capital(family)
            } else {
                format!("{} {}", capital(family), version.join("."))
            };
        }
    }
    match id {
        "opus" | "sonnet" | "haiku" => capital(id),
        _ => id.to_owned(),
    }
}

/// A count of tokens as the panel shows it: 950, 12.4k, 3.1M.
pub fn short_count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

/// One tool call the model asked for.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    id: String,
    name: String,
    /// The arguments, or why they could not be read.
    arguments: Result<Value, String>,
}

/// Counts conversations, so what a tool remembers between calls (such as the last scene
/// summary) starts afresh with a new one.
static CONVERSATION_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Which conversation this is (see [`Conversation::new`]).
pub fn conversation_epoch() -> u64 {
    CONVERSATION_EPOCH.load(std::sync::atomic::Ordering::Relaxed)
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
        CONVERSATION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    /// or a safety stop or the optional pause (see [`SAFETY_ROUNDS`]) ends it. `run_tool` runs one tool in the window.
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
        let own_words = text.split(NOTE_START).next().unwrap_or(text);
        let light =
            own_words.trim().chars().count() <= LIGHT_REQUEST_CHARS && attachments.is_empty();
        let all = tools();
        let known: Vec<&str> = all
            .iter()
            .map(|t| t.name)
            .chain(["more_tools", "use_tool"])
            .collect();
        let mut rounds = 0usize;
        // The last call that failed, and how many times in a row it has.
        let mut failing: Option<(String, String)> = None;
        let mut failures = 0usize;
        // The round of the last picture a tool returned in this turn.
        let mut last_picture: Option<usize> = None;
        loop {
            if stop() {
                events(Event::Notice("Stopped.".into()));
                return Ok(());
            }
            if rounds >= SAFETY_ROUNDS {
                events(Event::Notice(format!(
                    "Stopped after {SAFETY_ROUNDS} rounds of tools in one turn: a safety stop \
                     against a loop that never ends. Say \"continue\" to go on."
                )));
                return Ok(());
            }
            let asked = std::time::Instant::now();
            // A request that fails for a moment goes again, twice, after a short wait.
            let mut retries = 0;
            let calls = loop {
                let step = match self.provider {
                    Provider::Anthropic => self.anthropic_step(settings, transport, events),
                    _ => {
                        let effort = if light && rounds < LIGHT_ROUNDS {
                            Some("low")
                        } else if self.provider == Provider::AntigravityCli && rounds > 0 {
                            // Antigravity thinks hard by default, minutes a step: the plan in
                            // the first step keeps that, the steps after it think less.
                            Some("medium")
                        } else {
                            None
                        };
                        self.openai_step(settings, transport, events, effort)
                    }
                };
                match step {
                    Ok(calls) => break calls,
                    Err(e) if retries < NETWORK_RETRIES && retryable(&e) && !stop() => {
                        retries += 1;
                        events(Event::Retrying {
                            attempt: retries,
                            of: NETWORK_RETRIES,
                            error: readable_error(&e),
                        });
                        let wait = if cfg!(test) {
                            std::time::Duration::from_millis(5)
                        } else {
                            std::time::Duration::from_secs(RETRY_WAIT_SECONDS[retries - 1])
                        };
                        let until = std::time::Instant::now() + wait;
                        while std::time::Instant::now() < until && !stop() {
                            std::thread::sleep(std::time::Duration::from_millis(100).min(wait));
                        }
                    }
                    Err(e) if retries > 0 => {
                        return Err(format!(
                            "{UNREACHABLE} {}. Check the connection, then write \"continue\" to \
                             go on from here.",
                            readable_error(&e).trim_end_matches('.')
                        ));
                    }
                    Err(e) => return Err(readable_error(&e)),
                }
            };
            events(Event::Answered {
                seconds: asked.elapsed().as_secs_f64(),
            });
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
                        Ok(arguments) => {
                            // use_tool runs a tool outside the short catalogue.
                            let (name, arguments) = if call.name == "use_tool" {
                                (
                                    arguments["name"].as_str().unwrap_or_default().to_owned(),
                                    arguments.get("arguments").cloned().unwrap_or(json!({})),
                                )
                            } else {
                                (call.name.clone(), arguments.clone())
                            };
                            if !known.contains(&name.as_str()) || name == "use_tool" {
                                ToolResult::error(format!(
                                    "There is no tool called '{name}': more_tools lists them"
                                ))
                            } else {
                                events(Event::ToolStarted {
                                    name: name.clone(),
                                    arguments: arguments.clone(),
                                });
                                let mut result = run_tool(&name, &arguments);
                                let pictures = images(&result);
                                events(Event::ToolFinished {
                                    name,
                                    text: result.joined_text(),
                                    images: pictures.clone(),
                                    is_error: result.is_error,
                                });
                                if !pictures.is_empty() {
                                    if last_picture.is_some_and(|r| rounds - r < PICTURE_GAP_ROUNDS)
                                    {
                                        result.content.push(Content::Text {
                                            text: PICTURE_NOTE.into(),
                                        });
                                    }
                                    last_picture = Some(rounds);
                                }
                                result
                            }
                        }
                    }
                };
                // The same call failing again and again is a loop, not progress.
                if result.is_error {
                    let key = (
                        call.name.clone(),
                        call.arguments
                            .as_ref()
                            .map(Value::to_string)
                            .unwrap_or_default(),
                    );
                    if failing.as_ref() == Some(&key) {
                        failures += 1;
                    } else {
                        failing = Some(key);
                        failures = 1;
                    }
                } else {
                    failing = None;
                    failures = 0;
                }
                results.push((call, result));
            }
            match self.provider {
                Provider::Anthropic => self.push_anthropic_results(results),
                _ => self.push_openai_results(results),
            }
            if failures >= SAME_FAILURES {
                let name = failing.map(|(n, _)| n).unwrap_or_default();
                events(Event::Notice(format!(
                    "Stopped: the same {name} call failed {SAME_FAILURES} times in a row. \
                     Something must change before trying again."
                )));
                return Ok(());
            }
            rounds += 1;
            if settings.pause_every > 0 && rounds.is_multiple_of(settings.pause_every) {
                events(Event::Paused { steps: rounds });
                return Ok(());
            }
        }
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
        if let Some(usage) = usage_of(self.provider, settings.model(), &answer) {
            events(Event::Usage(usage));
        }
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

    /// One request in the chat-completions form. `effort` asks a CLI for a reasoning effort
    /// (`reasoning_effort` in the body, which each CLI turns into its own option and leaves out
    /// of the prompt); servers on the network never get it, since models without reasoning
    /// refuse the field.
    fn openai_step(
        &mut self,
        settings: &Settings,
        transport: &mut dyn Transport,
        events: &mut dyn FnMut(Event),
        effort: Option<&str>,
    ) -> Result<Vec<Call>, String> {
        let mut body = openai_request(settings, &self.messages);
        if let Some(effort) = effort
            && self.provider.is_cli()
        {
            body["reasoning_effort"] = json!(effort);
        }
        let mut headers = Vec::new();
        if !settings.api_key.trim().is_empty() {
            headers.push((
                "authorization",
                format!("Bearer {}", settings.api_key.trim()),
            ));
        }
        let url = format!("{}/chat/completions", settings.base_url());
        let answer = transport.post(&url, &headers, &body)?;
        if let Some(usage) = usage_of(self.provider, settings.model(), &answer) {
            events(Event::Usage(usage));
        }
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

/// Tool results before the recent part of the conversation are cut to this many characters,
/// and their screenshots become a line of text.
pub const OLD_RESULT_CHARS: usize = 600;
/// At most about this many messages are sent; older ones are left out from the start.
pub const MAX_MESSAGES: usize = 80;
/// At least this many of the latest messages are sent whole…
const RECENT_MESSAGES: usize = 8;
/// …and where the whole part starts moves in steps of this many messages, so what comes before
/// stays byte for byte the same from one request to the next and the provider's prompt cache
/// serves it.
const CHUNK: usize = 10;

const TOOL_STEPS_LEFT_OUT: &str = "(The tool steps before this point are left out to keep the \
     conversation short; what was said is above.)";

const OMITTED_SCREENSHOT: &str =
    "(An earlier screenshot, left out to keep the conversation short.)";

/// Whether a message is the person's own (not tool results, not a screenshot passed on).
fn from_person(message: &Value) -> bool {
    if message["role"] != "user" {
        return false;
    }
    match &message["content"] {
        Value::String(_) => true,
        Value::Array(parts) => !parts.iter().any(|p| {
            p["type"] == "tool_result"
                || (p["type"] == "text" && p["text"] == "The screenshot tool's picture:")
        }),
        _ => false,
    }
}

fn shorten(text: &str) -> String {
    if text.chars().count() <= OLD_RESULT_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(OLD_RESULT_CHARS * 3 / 4).collect();
    let tail: String = {
        let rest: Vec<char> = text.chars().collect();
        rest[rest.len() - OLD_RESULT_CHARS / 4..].iter().collect()
    };
    format!(
        "{head}\n… ({} characters left out of this earlier result) …\n{tail}",
        text.chars().count() - OLD_RESULT_CHARS
    )
}

/// What is sent of a conversation to OpenAI-compatible providers and the CLIs: the person's
/// messages and attachments whole; the recent messages whole; before them, tool results cut to
/// [`OLD_RESULT_CHARS`] and screenshots as a line of text; and past about [`MAX_MESSAGES`]
/// messages, only what was said before the cut (the person's messages and the model's replies,
/// not the tool steps). The boundaries move in steps, so the compacted part is the same from
/// one request to the next and stays in the provider's cache. The conversation itself keeps
/// everything. Claude's Messages API gets the conversation as it is (see [`anthropic_request`]).
pub fn compact(messages: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut rest_start = 0;
    if messages.len() > MAX_MESSAGES {
        // Before the cut only what was said stays: every message of the person's and every
        // reply, so a long first job is still remembered in the next turn. The cut moves in
        // steps and lands where a round starts, so no tool result loses its call.
        let mut cut = (messages.len() - MAX_MESSAGES).div_ceil(CHUNK) * CHUNK;
        while cut < messages.len() && !starts_round(&messages[cut]) {
            cut += 1;
        }
        if cut < messages.len() {
            out.extend(messages[..cut].iter().filter_map(said));
            out.push(json!({"role": "assistant", "content": TOOL_STEPS_LEFT_OUT}));
            rest_start = cut;
        }
    }
    let rest = &messages[rest_start..];
    // Before `whole`, results are cut short and screenshots left out; from it on, nothing
    // changes. It moves in steps of CHUNK, so the cut part stays the same between requests.
    let whole = rest.len().saturating_sub(RECENT_MESSAGES) / CHUNK * CHUNK;
    out.extend(rest.iter().enumerate().map(|(i, message)| {
        let mut m = message.clone();
        if i >= whole || from_person(message) {
            return m;
        }
        // `get_mut`, never `[]` on a mutable value: indexing would add the missing key.
        if let Some(parts) = m.get_mut("content").and_then(Value::as_array_mut) {
            for part in parts.iter_mut().filter(|p| p["type"] == "image_url") {
                *part = json!({"type": "text", "text": OMITTED_SCREENSHOT});
            }
        } else if message["role"] == "tool"
            && let Some(text) = message["content"].as_str()
        {
            m["content"] = json!(shorten(text));
        }
        m
    }));
    out
}

/// Whether a message starts a round: the person's, or the model's (its tool calls follow).
fn starts_round(message: &Value) -> bool {
    from_person(message) || message["role"] == "assistant"
}

/// What a message said, for the part of a long conversation before the cut: the person's
/// messages whole, the model's text without its tool calls, nothing else.
fn said(message: &Value) -> Option<Value> {
    if from_person(message) {
        return Some(message.clone());
    }
    let text = message["content"].as_str().unwrap_or_default().trim();
    (message["role"] == "assistant" && !text.is_empty())
        .then(|| json!({"role": "assistant", "content": text}))
}

/// The Messages API request, and whether it needs the fallback beta header.
pub fn anthropic_request(settings: &Settings, messages: &[Value]) -> (Value, bool) {
    let tools: Vec<Value> = crate::agent_base::assistant_tools()
        .iter()
        .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
        .collect();
    let model = settings.model();
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": SYSTEM_PROMPT,
        "tools": tools,
        // As it is, only ever added to: Claude's thinking blocks are bound to everything before
        // them, so an edited history would be refused or lose them; and an unchanged start is
        // what the prompt cache serves.
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
    let tools: Vec<Value> = crate::agent_base::assistant_tools()
        .iter()
        .map(|t| {
            json!({"type": "function", "function": {
                "name": t.name, "description": t.description, "parameters": t.input_schema
            }})
        })
        .collect();
    let mut all = vec![json!({"role": "system", "content": SYSTEM_PROMPT})];
    all.extend(compact(messages));
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
mod name_tests {
    use super::*;

    #[test]
    fn network_failures_are_retried_and_read_plainly() {
        let agy = "There was a network issue connecting to the server, please try again. \
                   (Antigravity CLI exited with exit status: 3: error: There was a network issue \
                   connecting to the server, please try again.\nAGY_ERROR: {\"short_error\":\"agent \
                   executor error\",\"retryable\": true})";
        assert!(retryable(agy));
        assert_eq!(
            readable_error(agy),
            "There was a network issue connecting to the server, please try again."
        );
        assert!(retryable("HTTP 529: Overloaded."));
        assert!(!retryable(
            "HTTP 401: invalid x-api-key. Check the API key."
        ));
        assert!(!retryable(
            "Codex exited with exit status: 1: ERROR: stream disconnected: request too large"
        ));
        assert!(!retryable("HTTP 429: Too many requests or no credit left"));
    }

    #[test]
    fn small_requests_take_the_small_model() {
        assert!(!large_task("ponlo en rojo", 0, false));
        assert!(!large_task("simula 5 s más", 0, false));
        assert!(!large_task("change the colour to navy", 0, false));
        assert!(large_task("hazle una camisa para esa falda", 0, false));
        assert!(large_task("add a pocket to the front", 0, false));
        assert!(
            large_task("ponlo en rojo", 1, false),
            "files need the larger model"
        );
        assert!(
            large_task("ponlo en rojo", 0, true),
            "so does the first request"
        );
    }

    #[test]
    fn models_are_named_as_people_say_them() {
        assert_eq!(model_name("claude-sonnet-5-5"), "Sonnet 5.5");
        assert_eq!(model_name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(model_name("claude-haiku-5-5-20261101"), "Haiku 5.5");
        assert_eq!(model_name("opus"), "Opus");
        assert_eq!(model_name("gpt-6.2-sol"), "gpt-6.2-sol");
        assert_eq!(Provider::ClaudeCli.short_label(), "Claude Code");
        assert!(
            Provider::Anthropic
                .suggested_models()
                .contains(&"claude-sonnet-5-5")
        );
    }
}

#[cfg(test)]
mod primer_tests {
    use super::SYSTEM_PROMPT;
    use crate::garment::presets;

    /// Every piece, segment and tool the system prompt's "at a glance" names exists, so the model
    /// can act on it without looking first.
    #[test]
    fn the_primer_names_real_pieces_segments_and_tools() {
        let start = SYSTEM_PROMPT.find("FashionCraft at a glance").unwrap();
        let end = SYSTEM_PROMPT[start..].find("\n\nSmall requests").unwrap() + start;
        let primer = &SYSTEM_PROMPT[start..end];
        let bases = [
            ("tshirt", presets::tshirt(&Default::default(), 1).unwrap()),
            (
                "shirt",
                presets::shirt_around(&Default::default(), 1, None).unwrap(),
            ),
            ("dress", presets::dress(&Default::default(), 1).unwrap()),
            ("skirt", presets::skirt(&Default::default(), 1).unwrap()),
            (
                "trousers",
                presets::trousers(&Default::default(), 1).unwrap(),
            ),
        ];
        for (kind, template) in &bases {
            assert!(primer.contains(&format!("{kind}:")), "{kind}");
            for panel in &template.panels {
                assert!(primer.contains(&panel.id), "{kind}: piece {}", panel.id);
                for edge in &panel.polygon.as_ref().unwrap().edges {
                    // "shoulder-*" stands for both sides.
                    let wildcard = edge
                        .rsplit_once('-')
                        .filter(|(_, side)| matches!(*side, "right" | "left"))
                        .map(|(stem, _)| format!("{stem}-*"));
                    assert!(
                        primer.contains(edge.as_str())
                            || wildcard.is_some_and(|w| primer.contains(&w)),
                        "{kind}: segment {edge} of {}",
                        panel.id
                    );
                }
            }
        }
        let tools: Vec<&str> = crate::agent::tools().iter().map(|t| t.name).collect();
        let named = primer.split("Which tool:").nth(1).unwrap();
        for word in named.split(|c: char| !(c.is_ascii_lowercase() || c == '_')) {
            if word.contains('_') {
                assert!(tools.contains(&word), "no tool {word}");
            }
        }
        for tool in ["simulate", "screenshot", "grading"] {
            assert!(named.contains(tool) && tools.contains(&tool), "{tool}");
        }
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;

    /// Whether a message carries a tool's screenshot (in Anthropic's or OpenAI's form).
    fn carries_screenshot(message: &Value) -> bool {
        !from_person(message)
            && message["content"].as_array().is_some_and(|parts| {
                parts.iter().any(|p| {
                    p["type"] == "image_url"
                        || p["content"]
                            .as_array()
                            .is_some_and(|c| c.iter().any(|b| b["type"] == "image"))
                })
            })
    }

    fn shot(i: usize) -> [Value; 3] {
        [
            json!({"role": "assistant", "content": "", "tool_calls": [{"id": format!("c{i}"), "type": "function",
                "function": {"name": "screenshot", "arguments": "{}"}}]}),
            json!({"role": "tool", "tool_call_id": format!("c{i}"), "content": "x".repeat(5000)}),
            json!({"role": "user", "content": [
                {"type": "text", "text": "The screenshot tool's picture:"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}]}),
        ]
    }

    #[test]
    fn a_long_session_sends_old_parts_short_and_keeps_them_stable() {
        let mut messages = vec![json!({"role": "user", "content": [
            {"type": "text", "text": "Make this"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,PHOTO"}}]})];
        for i in 0..20 {
            messages.extend(shot(i));
        }
        let sent = compact(&messages);
        let text = serde_json::to_string(&sent).unwrap();
        // The person's photo stays; screenshots before the whole part become a line.
        assert!(text.contains("PHOTO"));
        let whole = (messages.len() - RECENT_MESSAGES) / CHUNK * CHUNK;
        let kept = (0..messages.len())
            .filter(|&i| i >= whole && carries_screenshot(&messages[i]))
            .count();
        assert_eq!(text.matches("base64,AAAA").count(), kept);
        assert!(text.matches(OMITTED_SCREENSHOT).count() >= 15);
        assert!(text.contains("characters left out"));
        assert_eq!(
            sent[sent.len() - 2]["content"].as_str().unwrap().len(),
            5000
        );
        assert_eq!(sent.len(), messages.len());
        // From one request to the next the compacted start stays byte for byte the same, so
        // the provider's cache serves it: only at a step does it change.
        let mut same = 0;
        for n in 30..messages.len() {
            let a = compact(&messages[..n]);
            let b = compact(&messages[..n + 1]);
            if a[..] == b[..n] {
                same += 1;
            }
        }
        assert!(same >= (messages.len() - 30) * 8 / 10, "{same}");

        // Past the cap, the tool steps of the oldest part are left out, said so; what the
        // person and the model said stays, and every result still follows its call.
        for i in 0..40 {
            messages.push(json!({"role": "user", "content": format!("again {i}")}));
            messages.extend(shot(100 + i));
            messages.push(json!({"role": "assistant", "content": format!("done {i}")}));
        }
        let sent = compact(&messages);
        assert!(
            sent.len() <= MAX_MESSAGES + CHUNK + 2 * 40 + 2,
            "{}",
            sent.len()
        );
        assert!(sent.len() < messages.len() / 2);
        let text = serde_json::to_string(&sent).unwrap();
        assert!(text.contains("PHOTO") && text.contains("Make this"));
        for i in 0..40 {
            assert!(text.contains(&format!("\"again {i}\"")), "again {i}");
            assert!(text.contains(&format!("\"done {i}\"")), "done {i}");
        }
        assert_eq!(text.matches("tool steps before this point").count(), 1);
        for (i, m) in sent.iter().enumerate() {
            if m["role"] == "tool" {
                let calls = sent[i - 1]["tool_calls"].as_array().unwrap();
                assert!(calls.iter().any(|c| c["id"] == m["tool_call_id"]));
            }
        }
        // Stable between requests here too, except where the cut moves.
        let mut same = 0;
        for n in 150..messages.len() {
            let (a, b) = (compact(&messages[..n]), compact(&messages[..n + 1]));
            if a.len() <= b.len() && a[..] == b[..a.len()] {
                same += 1;
            }
        }
        assert!(same >= (messages.len() - 150) * 7 / 10, "{same}");
    }
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
            pause_every: 0,
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
            // How long each answer took is checked on its own.
            &mut |e| {
                if !matches!(e, Event::Answered { .. }) {
                    events.push(e)
                }
            },
            &|| false,
        );
        (result, events, ran)
    }

    fn calls(n: usize, tool: &str) -> Vec<Result<Value, String>> {
        (0..n)
            .map(|k| {
                Ok(json!({
                    "content": [{"type": "tool_use", "id": format!("t{k}"), "name": tool, "input": {}}],
                    "stop_reason": "tool_use",
                    "usage": {"input_tokens": 1000, "cache_read_input_tokens": 9000, "output_tokens": 100}
                }))
            })
            .collect()
    }

    #[test]
    fn pictures_too_close_together_come_with_a_note_and_the_prompt_says_when() {
        // Round 0 a screenshot, round 1 another (too soon), rounds 2-4 other tools, round 5 a
        // third (far enough).
        let script = [
            "screenshot",
            "screenshot",
            "set_color",
            "set_color",
            "set_color",
            "screenshot",
        ];
        let mut answers: Vec<Result<Value, String>> = script
            .iter()
            .enumerate()
            .map(|(k, tool)| {
                Ok(json!({"content": [{"type": "tool_use", "id": format!("t{k}"), "name": tool, "input": {}}],
                          "stop_reason": "tool_use"}))
            })
            .collect();
        answers.push(Ok(
            json!({"content": [{"type": "text", "text": "Done."}], "stop_reason": "end_turn"}),
        ));
        let mut transport = Canned::new(answers);
        let mut conversation = Conversation::new(Provider::Anthropic);
        let (result, ..) = run(
            &mut conversation,
            &settings(Provider::Anthropic),
            &mut transport,
            "Make it red and show me",
        );
        assert!(result.is_ok());
        let noted: Vec<bool> = conversation
            .messages()
            .iter()
            .filter(|m| m["role"] == "user")
            .filter_map(|m| m["content"][0]["content"].as_array())
            .filter(|c| c.iter().any(|b| b["type"] == "image"))
            .map(|c| c.iter().any(|b| b["text"] == PICTURE_NOTE))
            .collect();
        assert_eq!(noted, [false, true, false]);
        // The shared prompt (the Assistant's and the MCP server's) says when a picture is
        // worth it, and a small change takes none.
        for rule in [
            "only to verify the finished result",
            "compare with a \
picture the person attached",
            "one view per check",
            "never after every step",
        ] {
            let flat = SYSTEM_PROMPT.replace('\n', " ");
            assert!(flat.contains(&rule.replace('\n', " ")), "{rule}");
        }
        assert!(SYSTEM_PROMPT.contains("no screenshot"));
    }

    /// A tool call and, after `rounds` of them, a final reply, in the provider's answer form.
    fn job(provider: Provider, rounds: usize, reply: &str) -> Vec<Result<Value, String>> {
        let mut answers: Vec<Result<Value, String>> = (0..rounds)
            .map(|k| {
                Ok(if provider == Provider::Anthropic {
                    json!({"content": [{"type": "thinking", "thinking": format!("plan {k}"), "signature": format!("sig{k}")},
                        {"type": "tool_use", "id": format!("t{k}"), "name": "set_color", "input": {"color": "red"}}],
                        "stop_reason": "tool_use"})
                } else {
                    json!({"choices": [{"message": {"role": "assistant", "content": null, "tool_calls": [
                        {"id": format!("c{k}"), "type": "function",
                         "function": {"name": "set_color", "arguments": "{\"color\":\"red\"}"}}]},
                        "finish_reason": "tool_calls"}]})
                })
            })
            .collect();
        answers.push(Ok(if provider == Provider::Anthropic {
            json!({"content": [{"type": "text", "text": reply}], "stop_reason": "end_turn"})
        } else {
            json!({"choices": [{"message": {"role": "assistant", "content": reply}, "finish_reason": "stop"}]})
        }));
        answers
    }

    /// The messages a request sent, whichever provider's form.
    fn messages_of(sent: &Sent) -> &Vec<Value> {
        sent.2["messages"].as_array().unwrap()
    }

    #[test]
    fn the_second_turn_remembers_the_first_through_each_api() {
        for provider in [Provider::Anthropic, Provider::OpenAi] {
            for rounds in [1, 70] {
                let mut answers = job(provider, rounds, "Done: the shirt is red.");
                answers.extend(job(provider, 0, "It is blue now."));
                let mut transport = Canned::new(answers);
                let mut conversation = Conversation::new(provider);
                let settings = Settings {
                    model: "some-model".into(),
                    ..settings(provider)
                };
                let (first, ..) = run(
                    &mut conversation,
                    &settings,
                    &mut transport,
                    "Make the shirt red",
                );
                let (second, ..) = run(
                    &mut conversation,
                    &settings,
                    &mut transport,
                    "Now make it blue",
                );
                assert!(
                    first.is_ok() && second.is_ok(),
                    "{provider:?} {rounds}: {first:?} {second:?}"
                );
                let last = serde_json::to_string(transport.sent.last().unwrap()).unwrap();
                for carried in [
                    "Make the shirt red",
                    "did set_color",
                    "Done: the shirt is red.",
                    "Now make it blue",
                ] {
                    assert!(
                        last.contains(carried),
                        "{provider:?}, {rounds} rounds: {carried:?} is missing"
                    );
                }
                if provider == Provider::Anthropic {
                    // Only ever added to: each request starts with the one before, thinking
                    // blocks and all, so none is invalidated and the cache serves the start.
                    for pair in transport.sent.windows(2) {
                        let (a, b) = (messages_of(&pair[0]), messages_of(&pair[1]));
                        assert!(b.len() > a.len() && b[..a.len()] == a[..]);
                    }
                    assert_eq!(last.matches("\"signature\"").count(), rounds);
                }
            }
        }
    }

    #[test]
    fn small_requests_ask_a_cli_for_low_effort_at_first_and_every_answer_is_timed() {
        let noted = with_note("Make it red", &"the scene changed: ".repeat(200));
        let long = "Make a dress like the one in the photo, fitted at the waist. ".repeat(3);
        for (provider, text, low) in [
            (Provider::CodexCli, "Make the shirt red", LIGHT_ROUNDS),
            (Provider::ClaudeCli, noted.as_str(), LIGHT_ROUNDS),
            (Provider::CodexCli, long.as_str(), 0),
            (Provider::OpenAi, "Make the shirt red", 0),
        ] {
            let mut transport = Canned::new(job(provider, 5, "Done."));
            let mut conversation = Conversation::new(provider);
            let settings = Settings {
                model: "some-model".into(),
                ..settings(provider)
            };
            let mut timed = 0;
            let result = conversation.turn(
                &settings,
                text,
                &mut transport,
                &mut |_, _| ToolResult::text("Coloured it red"),
                &mut |e| {
                    timed += usize::from(matches!(e, Event::Answered { seconds } if seconds >= 0.))
                },
                &|| false,
            );
            assert!(result.is_ok(), "{provider:?}: {result:?}");
            let asked: Vec<bool> = transport
                .sent
                .iter()
                .map(|s| s.2["reasoning_effort"] == "low")
                .collect();
            // The first steps, then the CLI's usual effort; never for a server on the network.
            assert_eq!(
                asked,
                (0..6).map(|i| i < low).collect::<Vec<_>>(),
                "{provider:?}"
            );
            assert_eq!(timed, 6);
        }
        // Antigravity: its usual effort for the plan, medium for every step after it.
        let mut transport = Canned::new(job(Provider::AntigravityCli, 3, "Done."));
        let mut conversation = Conversation::new(Provider::AntigravityCli);
        let (result, _, _) = run(
            &mut conversation,
            &settings(Provider::AntigravityCli),
            &mut transport,
            "Make a dress like the one in the photo, fitted at the waist, with a ruffle at the hem.",
        );
        assert!(result.is_ok(), "{result:?}");
        let efforts: Vec<&Value> = transport
            .sent
            .iter()
            .map(|s| &s.2["reasoning_effort"])
            .collect();
        assert_eq!(
            efforts,
            [
                &Value::Null,
                &json!("medium"),
                &json!("medium"),
                &json!("medium")
            ]
        );
    }

    #[test]
    fn a_long_job_runs_on_unless_a_pause_is_asked_for() {
        // 60 rounds of tools: more than the old cap of 40, and the job finishes.
        let mut answers = calls(60, "scene_summary");
        answers.push(Ok(
            json!({"content": [{"type": "text", "text": "Done."}], "stop_reason": "end_turn"}),
        ));
        let mut transport = Canned::new(answers);
        let mut conversation = Conversation::new(Provider::Anthropic);
        let (result, events, ran) = run(
            &mut conversation,
            &settings(Provider::Anthropic),
            &mut transport,
            "do it",
        );
        assert!(result.is_ok());
        assert_eq!(ran.len(), 60);
        assert!(events.contains(&Event::Text("Done.".into())));
        // Every request reported its usage.
        let used: Vec<&Usage> = events
            .iter()
            .filter_map(|e| match e {
                Event::Usage(u) => Some(u),
                _ => None,
            })
            .collect();
        // (The last answer reported none.)
        assert_eq!(used.len(), 60);

        // With a pause every 25 steps it stops after 25, and goes on when asked.
        let mut transport = Canned::new(calls(30, "scene_summary"));
        let mut conversation = Conversation::new(Provider::Anthropic);
        let paused = Settings {
            pause_every: 25,
            ..settings(Provider::Anthropic)
        };
        let (result, events, ran) = run(&mut conversation, &paused, &mut transport, "do it");
        assert!(result.is_ok());
        assert_eq!(ran.len(), 25);
        assert_eq!(events.last(), Some(&Event::Paused { steps: 25 }));
    }

    #[test]
    fn the_same_failing_call_stops_the_turn() {
        let mut transport = Canned::new(calls(20, "no_such_tool"));
        let mut conversation = Conversation::new(Provider::Anthropic);
        let (result, events, _) = run(
            &mut conversation,
            &settings(Provider::Anthropic),
            &mut transport,
            "do it",
        );
        assert!(result.is_ok());
        let stopped = events.iter().any(|e| {
            matches!(e, Event::Notice(t) if t.contains(&format!("failed {SAME_FAILURES} times")))
        });
        assert!(stopped, "{events:?}");
        assert_eq!(transport.sent.len(), SAME_FAILURES);
    }

    #[test]
    fn usage_is_read_and_priced_where_the_price_is_known() {
        let answer = json!({"usage": {"input_tokens": 1000, "cache_read_input_tokens": 9000,
            "cache_creation_input_tokens": 0, "output_tokens": 500}});
        let u = usage_of(Provider::Anthropic, "claude-opus-5-5", &answer).unwrap();
        assert_eq!((u.input, u.cached, u.output), (10_000, 9_000, 500));
        // 1000 × $4 + 9000 × $0.20 + 500 × $20, per million.
        assert!((u.cost_usd.unwrap() - 0.0158).abs() < 1e-9);
        assert_eq!(u.context_window, Some(1_000_000));
        // An unknown model: tokens, no money. A CLI: its own figures.
        let openai = json!({"usage": {"prompt_tokens": 50, "completion_tokens": 5}});
        assert_eq!(
            usage_of(Provider::OpenAi, "some-model", &openai)
                .unwrap()
                .cost_usd,
            None
        );
        let cli = json!({"usage": {"prompt_tokens": 50, "completion_tokens": 5}, "x_cost_usd": 0.01,
            "x_rate_limit": {"used_percent": 40.0, "what": "plan limit, 5 h window"}});
        let u = usage_of(Provider::ClaudeCli, "", &cli).unwrap();
        assert_eq!(u.cost_usd, Some(0.01));
        assert_eq!(u.limit.unwrap().used_percent, 40.);
        assert_eq!(short_count(950), "950");
        assert_eq!(short_count(12_400), "12.4k");
        assert_eq!(short_count(3_100_000), "3.1M");
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
        assert_eq!(
            first["tools"].as_array().unwrap().len(),
            crate::agent_base::assistant_tools().len()
        );
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
        // A failure that is not sent again (a busy service is: see the test above).
        let mut transport = Canned::new(vec![
            Err("HTTP 400: The request was not accepted.".into()),
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
            pause_every: 0,
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
    fn a_request_that_fails_for_a_moment_goes_again() {
        let reply = |text: &str| {
            Ok(
                json!({"choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}]}),
            )
        };
        let busy = || Err("HTTP 503: The service is busy.".to_owned());
        let mut openai = settings(Provider::OpenAi);
        openai.model = "test-model".into();
        // Once: it goes again and the turn ends well, with a notice.
        let mut transport = Canned::new(vec![busy(), reply("Done.")]);
        let mut conversation = Conversation::new(Provider::OpenAi);
        let (result, events, _) = run(&mut conversation, &openai, &mut transport, "Hello");
        assert!(result.is_ok(), "{result:?}");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Retrying { attempt: 1, .. }))
        );
        assert_eq!(transport.sent.len(), 2);
        // Three times: the turn fails, plainly.
        let mut transport = Canned::new(vec![busy(), busy(), busy()]);
        let mut conversation = Conversation::new(Provider::OpenAi);
        let (result, _, _) = run(&mut conversation, &openai, &mut transport, "Hello");
        let error = result.unwrap_err();
        assert!(error.starts_with(UNREACHABLE), "{error}");
        // A wrong key is not sent again.
        let mut transport = Canned::new(vec![Err("HTTP 401: invalid key.".into())]);
        let mut conversation = Conversation::new(Provider::OpenAi);
        let (result, _, _) = run(&mut conversation, &openai, &mut transport, "Hello");
        assert_eq!(result.unwrap_err(), "HTTP 401: invalid key.");
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
        notices.retain(|e| !matches!(e, Event::Answered { .. }));
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
                    pause_every: 0,
                };
                assert!(!p.needs_key());
                assert_eq!(s.missing(), None);
            }
        }
    }
}
