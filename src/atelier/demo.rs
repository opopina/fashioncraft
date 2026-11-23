//! Demo mode: a scripted session that drives the real interface. A visible cursor glides to
//! menus, tools and points on the board or the body and clicks them; keys are pressed; captions
//! say what each step does. It runs at a pace a person can follow live, and can record the
//! window to an MP4 at the same time.
//!
//!     FASHIONCRAFT_DEMO=vest [FASHIONCRAFT_DEMO_RECORD=1] [FASHIONCRAFT_DEMO_CLEAN=1] fashioncraft
//!
//! `FASHIONCRAFT_DEMO_CLEAN=1` leaves out the cursor, the click ripples and the captions, for
//! stills and for footage that gets its own titles in an editor. `FASHIONCRAFT_DEMO_MAX_WIDTH`
//! (pixels, default 1920) caps the recording's width.
//!
//! The pointer and keys of the computer are ignored while it runs (only F9, pause or resume,
//! and Shift+Escape, stop, are read), so a stray mouse cannot change what it does. Every step is
//! written to `demo-<name>/review.md` in the session folder, with what failed and why, and as
//! it happens to `progress.log` beside it (which survives a crash); the recording goes there too.
//!
//! A demo sets the language, the layout and the display its script needs, but a demo window
//! never saves its preferences, so the person's own are as they left them afterwards.
use super::*;
use std::{collections::HashMap, io::Write, path::PathBuf, sync::mpsc, thread::JoinHandle};

/// Where widgets report their rectangles for the demo, in points.
const TARGETS: &str = "fashioncraft.demo.targets";
/// The rectangles of the frame before: a widget is aimed at only once it stands still (a menu
/// that just opened is laid out once at a provisional place).
const PREVIOUS: &str = "fashioncraft.demo.targets.previous";
/// Whether a demo runs (so widgets report themselves).
const ACTIVE: &str = "fashioncraft.demo.active";
/// Frames per second of a recording, and of the demo clock while recording.
const FPS: f64 = 30.;
/// How long the cursor takes to reach a target, seconds, at least.
const GLIDE: f64 = 0.55;

/// The window size a demo opens at, points: `FASHIONCRAFT_DEMO_SIZE` (`WxH`) or 1440 × 810,
/// 16:9 and within a 14-inch laptop's screen. None when no demo runs.
pub(super) fn window_size() -> Option<[f32; 2]> {
    let name = std::env::var("FASHIONCRAFT_DEMO").ok()?;
    if !super::demo_vest::NAMES.contains(&name.as_str()) {
        return None;
    }
    let size = std::env::var("FASHIONCRAFT_DEMO_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or([1440., 810.]);
    Some(size)
}

/// Report `rect` as the demo target `key` (only while a demo runs). `key` is only turned
/// into a string then, so callers can pass `format_args!` at no cost in normal use.
pub(super) fn mark(ctx: &egui::Context, key: impl std::fmt::Display, rect: egui::Rect) {
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(ACTIVE)).unwrap_or(false)) {
        return;
    }
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<HashMap<String, egui::Rect>>(egui::Id::new(TARGETS))
            .insert(key.to_string(), rect);
    });
}

/// Where the cursor goes.
pub(super) enum Target {
    /// A widget that reported itself with [`mark`].
    Widget(String),
    /// Computed from the session when the step starts.
    Find(Locate),
}

impl Target {
    pub(super) fn widget(key: &str) -> Self {
        Self::Widget(key.to_owned())
    }
}

type Action = Box<dyn FnMut(&mut Atelier, &egui::Context) -> Result<(), String>>;
type Check = Box<dyn Fn(&Atelier) -> bool>;
/// Where a target is now on screen, if it is.
type Locate = Box<dyn Fn(&Atelier) -> Option<egui::Pos2>>;

pub(super) enum Step {
    /// Show a caption until the next one (an empty one hides it); translated like the
    /// interface.
    Caption(&'static str),
    /// Wait, in seconds.
    Wait(f64),
    /// Move the cursor to a target without clicking.
    Hover(Target),
    /// Move there and click.
    Click(Target),
    /// Move to the first target, press, move to the second while holding, release.
    Drag(Target, Target),
    /// The same with the secondary button: in the 3D view, how a person turns the camera.
    DragRight(Target, Target),
    /// Press a key.
    Key(egui::Key, egui::Modifiers),
    /// Type text into the focused field.
    Type(String),
    /// Turn the mouse wheel over a target (negative zooms in).
    Scroll(Target, f32),
    /// Do something the interface has no pointer path for (logged as such).
    Act(String, Action),
    /// Wait until a condition holds, or fail after a time.
    Until(String, Check, f64),
    /// Start (true) or stop recording.
    Record(bool),
    /// Turn the 3D camera round the avatar by this many degrees over this many seconds.
    Orbit(f32, f64),
    /// Stop the demo unless this holds (before a step that would do harm otherwise).
    Guard(String, Check),
}

/// Steps with less typing.
pub(super) fn caption(text: &'static str) -> Step {
    Step::Caption(text)
}
pub(super) fn click(key: &str) -> Step {
    Step::Click(Target::widget(key))
}
pub(super) fn act(
    what: &str,
    f: impl FnMut(&mut Atelier, &egui::Context) -> Result<(), String> + 'static,
) -> Step {
    Step::Act(what.to_owned(), Box::new(f))
}
pub(super) fn guard(what: &str, f: impl Fn(&Atelier) -> bool + 'static) -> Step {
    Step::Guard(what.to_owned(), Box::new(f))
}
pub(super) fn until(what: &str, f: impl Fn(&Atelier) -> bool + 'static, seconds: f64) -> Step {
    Step::Until(what.to_owned(), Box::new(f), seconds)
}

/// The step running and how far it has got.
enum Phase {
    /// Gliding from `from` to `to`, then `then`.
    Glide {
        from: egui::Pos2,
        to: egui::Pos2,
        started: f64,
        then: AfterGlide,
    },
    /// Holding the button down until this time.
    Holding { until: f64, release_at: egui::Pos2 },
    /// Waiting until this time.
    Waiting(f64),
    /// Waiting for a condition, failing after this time.
    Polling(f64),
    /// Turning the camera from `from` (radians) by `by` between `started` and `ends`.
    Orbit {
        from: f32,
        by: f32,
        started: f64,
        ends: f64,
    },
}

enum AfterGlide {
    Nothing,
    Click,
    /// Press here, then glide to the target held.
    PressThenGlide(egui::Pos2),
    Release,
}

/// Raw RGBA frames piped to ffmpeg.
struct Recorder {
    size: [usize; 2],
    frames: mpsc::SyncSender<Vec<u8>>,
    writer: JoinHandle<Result<(), String>>,
    path: PathBuf,
    count: usize,
}

#[derive(Clone, Copy)]
struct Shot;

pub(super) struct Demo {
    name: String,
    script: Vec<Step>,
    next: usize,
    phase: Option<Phase>,
    /// Seconds since the demo started: frame time, or 1/FPS per frame while recording.
    clock: f64,
    step_started: f64,
    cursor: egui::Pos2,
    /// Clicks shown as a fading ring: where and when.
    ripples: Vec<(egui::Pos2, f64)>,
    pressed: bool,
    /// The button the next press uses (the secondary one during a DragRight).
    drag_button: egui::PointerButton,
    /// Events for the next frame's input.
    events: Vec<egui::Event>,
    caption: Option<(String, f64)>,
    paused: bool,
    stopped: bool,
    record_wanted: bool,
    /// Draw no cursor, ripples or captions.
    clean: bool,
    recorder: Option<Recorder>,
    shot_pending: bool,
    out: PathBuf,
    log: Vec<String>,
    failures: usize,
    /// When the target of the step being retried was first looked for.
    seeking_since: Option<f64>,
}

/// How long a step looks for a target that is not on screen yet (a menu opening, a dialog
/// loading) before it fails.
const SEEK: f64 = 6.;

impl Demo {
    /// The demo named by `FASHIONCRAFT_DEMO`, if any.
    pub(super) fn from_env(output: &std::path::Path) -> Option<Self> {
        let name = std::env::var("FASHIONCRAFT_DEMO").ok()?;
        let script = super::demo_vest::script(&name)?;
        let out = output.join(format!("demo-{name}"));
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::remove_file(out.join("progress.log"));
        Some(Self {
            name,
            script,
            next: 0,
            phase: None,
            clock: 0.,
            step_started: 0.,
            cursor: egui::pos2(400., 300.),
            ripples: vec![],
            pressed: false,
            drag_button: egui::PointerButton::Primary,
            events: vec![],
            caption: None,
            paused: false,
            stopped: false,
            record_wanted: std::env::var("FASHIONCRAFT_DEMO_RECORD").is_ok_and(|v| v == "1"),
            clean: std::env::var("FASHIONCRAFT_DEMO_CLEAN").is_ok_and(|v| v == "1"),
            recorder: None,
            shot_pending: false,
            out,
            log: vec![],
            failures: 0,
            seeking_since: None,
        })
    }

    /// Feed the pointer and keys the demo produces, instead of the computer's own.
    pub(super) fn input(&mut self, input: &mut egui::RawInput) {
        let mut stop = false;
        let mut toggle = false;
        input.events.retain(|event| match event {
            egui::Event::Key {
                key: egui::Key::F9,
                pressed: true,
                ..
            } => {
                toggle = true;
                false
            }
            egui::Event::Key {
                key: egui::Key::Escape,
                pressed: true,
                modifiers,
                ..
            } if modifiers.shift => {
                stop = true;
                false
            }
            egui::Event::PointerMoved(_)
            | egui::Event::MouseMoved(_)
            | egui::Event::PointerButton { .. }
            | egui::Event::PointerGone
            | egui::Event::MouseWheel { .. }
            | egui::Event::Zoom(_)
            | egui::Event::Key { .. }
            | egui::Event::Text(_) => false,
            _ => true,
        });
        if toggle {
            self.paused = !self.paused;
            self.note(if self.paused {
                "‖ paused (F9)"
            } else {
                "▶ resumed (F9)"
            });
        }
        if stop && !self.stopped {
            self.stopped = true;
            self.note("■ stopped (Shift+Esc)");
        }
        input.focused = true;
        input.events.append(&mut self.events);
    }

    /// Look again next frame for a target not on screen yet, within [`SEEK`] seconds.
    fn retry(&mut self, index: usize, now: f64) -> bool {
        let since = *self.seeking_since.get_or_insert(now);
        if now - since < SEEK {
            self.next = index;
            self.phase = Some(Phase::Waiting(now + 0.05));
            true
        } else {
            self.seeking_since = None;
            false
        }
    }

    fn note(&mut self, line: &str) {
        self.record(format!("{:7.1} s  {line}", self.clock));
    }

    fn fail(&mut self, what: &str, why: &str) {
        self.failures += 1;
        self.record(format!("{:7.1} s  ✗ {what}: {why}", self.clock));
    }

    /// Keeps a log line for review.md and appends it at once to `progress.log`, so a run that
    /// crashes (as one did inside a software renderer on Windows) still shows how far it got.
    fn record(&mut self, line: String) {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.out.join("progress.log"))
        {
            let _ = writeln!(file, "{line}");
        }
        self.log.push(line);
    }

    fn move_to(&mut self, p: egui::Pos2) {
        self.cursor = p;
        self.events.push(egui::Event::PointerMoved(p));
    }

    fn button(&mut self, pressed: bool) {
        self.pressed = pressed;
        self.events.push(egui::Event::PointerButton {
            pos: self.cursor,
            button: self.drag_button,
            pressed,
            modifiers: Default::default(),
        });
        if pressed {
            self.ripples.push((self.cursor, self.clock));
        }
    }

    pub(super) fn finished(&self) -> bool {
        self.stopped || self.next >= self.script.len() && self.phase.is_none()
    }

    fn write_review(&self) {
        let _ = std::fs::create_dir_all(&self.out);
        let text = format!(
            "# Demo «{}»\n\n{} steps, {} failed.\n\n```\n{}\n```\n",
            self.name,
            self.script.len(),
            self.failures,
            self.log.join("\n")
        );
        let _ = std::fs::write(self.out.join("review.md"), text);
    }
}

/// Where `target` is on screen now.
fn locate(atelier: &Atelier, ctx: &egui::Context, target: &Target) -> Option<egui::Pos2> {
    match target {
        Target::Widget(key) => ctx
            .data(|d| {
                let now = d
                    .get_temp::<HashMap<String, egui::Rect>>(egui::Id::new(TARGETS))?
                    .get(key)
                    .copied()?;
                let before = d
                    .get_temp::<HashMap<String, egui::Rect>>(egui::Id::new(PREVIOUS))?
                    .get(key)
                    .copied()?;
                (now == before).then(|| now.center())
            })
            .or_else(|| {
                // Controls that report themselves for the interface tests.
                let now = atelier.demo_points.get(key)?;
                (atelier.demo_points_before.get(key) == Some(now)).then_some(*now)
            }),
        Target::Find(f) => f(atelier),
    }
}

fn ease(t: f64) -> f32 {
    let t = t.clamp(0., 1.);
    (t * t * (3. - 2. * t)) as f32
}

impl Atelier {
    /// Run the demo one frame: start or advance its step, record, and draw the cursor and the
    /// caption over everything. Call once per frame after the views are painted.
    pub(super) fn drive_demo(&mut self, ctx: &egui::Context) {
        let Some(mut demo) = self.demo.take() else {
            return;
        };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(ACTIVE), true));
        ctx.request_repaint();
        // Recording: collect last frame's picture, then ask for this one's.
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot {
                    user_data, image, ..
                } if user_data
                    .data
                    .as_ref()
                    .is_some_and(|d| d.downcast_ref::<Shot>().is_some()) =>
                {
                    Some(image.clone())
                }
                _ => None,
            })
        }) {
            demo.shot_pending = false;
            if let Some(recorder) = &mut demo.recorder
                && image.size == recorder.size
            {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if recorder.frames.send(bytes).is_ok() {
                    recorder.count += 1;
                }
            }
        }
        let recording = demo.recorder.is_some();
        if recording && demo.shot_pending {
            // Hold the clock until the frame is captured, so the video keeps its pace.
            self.paint_demo(ctx, &demo);
            self.demo = Some(demo);
            return;
        }
        if !demo.paused {
            demo.clock += if recording {
                1. / FPS
            } else {
                ctx.input(|i| i.stable_dt as f64).min(0.1)
            };
            self.step_demo(ctx, &mut demo);
        }
        // Keep hovering where the cursor is, so the interface sees the pointer every frame.
        if demo.events.is_empty() {
            let p = demo.cursor;
            demo.events.push(egui::Event::PointerMoved(p));
        }
        self.paint_demo(ctx, &demo);
        if demo.recorder.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(Shot)));
            demo.shot_pending = true;
        }
        if demo.finished() {
            demo.note(if demo.stopped { "stopped" } else { "finished" });
            if let Some(recorder) = demo.recorder.take() {
                let message = finish_recording(recorder);
                demo.note(&message);
            }
            demo.write_review();
            self.status = format!(
                "Demo «{}» {}: {} failed steps · {}",
                demo.name,
                if demo.stopped { "stopped" } else { "finished" },
                demo.failures,
                demo.out.join("review.md").display()
            );
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(ACTIVE), false));
            // It no longer needs to stay in front of the other apps.
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
            return;
        }
        self.demo = Some(demo);
    }

    fn step_demo(&mut self, ctx: &egui::Context, demo: &mut Demo) {
        let now = demo.clock;
        if let Some(phase) = demo.phase.take() {
            match phase {
                Phase::Glide {
                    from,
                    to,
                    started,
                    then,
                } => {
                    let length = (to - from).length();
                    let duration = GLIDE.max(length as f64 / 1400.);
                    let t = (now - started) / duration;
                    demo.move_to(from + (to - from) * ease(t));
                    if t < 1. {
                        demo.phase = Some(Phase::Glide {
                            from,
                            to,
                            started,
                            then,
                        });
                        return;
                    }
                    match then {
                        AfterGlide::Nothing => {}
                        AfterGlide::Click => {
                            demo.button(true);
                            demo.phase = Some(Phase::Holding {
                                until: now + 0.08,
                                release_at: to,
                            });
                            return;
                        }
                        AfterGlide::PressThenGlide(end) => {
                            demo.button(true);
                            demo.phase = Some(Phase::Glide {
                                from: to,
                                to: end,
                                started: now + 0.12,
                                then: AfterGlide::Release,
                            });
                            return;
                        }
                        AfterGlide::Release => {
                            demo.button(false);
                            demo.drag_button = egui::PointerButton::Primary;
                        }
                    }
                }
                Phase::Holding { until, release_at } => {
                    if now < until {
                        demo.phase = Some(Phase::Holding { until, release_at });
                        return;
                    }
                    demo.move_to(release_at);
                    demo.button(false);
                }
                Phase::Waiting(until) => {
                    if now < until {
                        demo.phase = Some(Phase::Waiting(until));
                        return;
                    }
                    // A finished wait goes straight on to the next step.
                    return self.start_demo_step(ctx, demo, now);
                }
                Phase::Orbit {
                    from,
                    by,
                    started,
                    ends,
                } => {
                    let t = ((now - started) / (ends - started).max(1e-3)).clamp(0., 1.);
                    self.camera.yaw = from + by * ease(t);
                    if t < 1. {
                        demo.phase = Some(Phase::Orbit {
                            from,
                            by,
                            started,
                            ends,
                        });
                        return;
                    }
                }
                Phase::Polling(deadline) => {
                    let Some(Step::Until(what, check, _)) = demo.script.get(demo.next - 1) else {
                        return;
                    };
                    if check(self) {
                        let line = format!("✓ {what} ({:.1} s)", now - demo.step_started);
                        demo.note(&line);
                    } else if now < deadline {
                        demo.phase = Some(Phase::Polling(deadline));
                        return;
                    } else {
                        let what = what.clone();
                        demo.fail(&what, "did not happen in time");
                    }
                }
            }
            // A short breath between steps, as a person takes.
            demo.phase = Some(Phase::Waiting(now + 0.15));
            return;
        }
        self.start_demo_step(ctx, demo, now);
    }

    fn start_demo_step(&mut self, ctx: &egui::Context, demo: &mut Demo, now: f64) {
        if demo.next >= demo.script.len() {
            return;
        }
        // The step is taken out while it runs and put back, so a condition can be polled later.
        let index = demo.next;
        let mut step = std::mem::replace(&mut demo.script[index], Step::Wait(0.));
        demo.next += 1;
        demo.step_started = now;
        let label = describe(&step);
        match &mut step {
            Step::Caption(text) => {
                let text = i18n::tr(self.prefs.language, text).to_owned();
                demo.caption = (!text.is_empty()).then_some((text, now));
            }
            Step::Wait(seconds) => {
                demo.phase = Some(Phase::Waiting(now + *seconds));
            }
            Step::Hover(target) | Step::Click(target) => {
                let then = if label.starts_with("click") {
                    AfterGlide::Click
                } else {
                    AfterGlide::Nothing
                };
                match locate(self, ctx, target) {
                    Some(to) => {
                        demo.seeking_since = None;
                        demo.note(&format!("· {label}"));
                        demo.phase = Some(Phase::Glide {
                            from: demo.cursor,
                            to,
                            started: now,
                            then,
                        });
                    }
                    None => {
                        if demo.retry(index, now) {
                            demo.script[index] = step;
                            return;
                        }
                        demo.fail(&label, "not on screen")
                    }
                }
            }
            Step::Drag(a, b) | Step::DragRight(a, b) => {
                match (locate(self, ctx, a), locate(self, ctx, b)) {
                    (Some(from), Some(to)) => {
                        demo.seeking_since = None;
                        demo.note(&format!("· {label}"));
                        if matches!(step, Step::DragRight(..)) {
                            demo.drag_button = egui::PointerButton::Secondary;
                        }
                        demo.phase = Some(Phase::Glide {
                            from: demo.cursor,
                            to: from,
                            started: now,
                            then: AfterGlide::PressThenGlide(to),
                        });
                    }
                    _ => {
                        if demo.retry(index, now) {
                            demo.script[index] = step;
                            return;
                        }
                        demo.fail(&label, "not on screen")
                    }
                }
            }
            Step::Key(key, modifiers) => {
                for pressed in [true, false] {
                    demo.events.push(egui::Event::Key {
                        key: *key,
                        physical_key: Some(*key),
                        pressed,
                        repeat: false,
                        modifiers: *modifiers,
                    });
                }
                demo.note(&format!("· key {}", key.name()));
                demo.phase = Some(Phase::Waiting(now + 0.2));
            }
            Step::Scroll(target, delta) => match locate(self, ctx, target) {
                Some(at) => {
                    demo.move_to(at);
                    demo.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., *delta),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    });
                    demo.note(&format!("· scroll {delta}"));
                    demo.phase = Some(Phase::Waiting(now + 0.25));
                }
                None => demo.fail("scroll", "not on screen"),
            },
            Step::Type(text) => {
                demo.events.push(egui::Event::Text(text.clone()));
                demo.note(&format!("· type «{text}»"));
                demo.phase = Some(Phase::Waiting(now + 0.3));
            }
            Step::Act(what, action) => {
                match action(self, ctx) {
                    Ok(()) => demo.note(&format!("✓ {what} (no pointer path)")),
                    Err(e) => demo.fail(what, &e),
                }
                demo.phase = Some(Phase::Waiting(now + 0.2));
            }
            Step::Until(_, _, seconds) => {
                demo.phase = Some(Phase::Polling(now + *seconds));
            }
            Step::Guard(what, check) => {
                if check(self) {
                    demo.note(&format!("✓ {what}"));
                } else {
                    demo.fail(what, "does not hold: the demo stops here");
                    demo.stopped = true;
                }
            }
            Step::Orbit(degrees, seconds) => {
                demo.note(&format!("· orbit {degrees}° in {seconds} s"));
                demo.phase = Some(Phase::Orbit {
                    from: self.camera.yaw,
                    by: degrees.to_radians(),
                    started: now,
                    ends: now + *seconds,
                });
            }
            Step::Record(start) => {
                if *start && demo.record_wanted && demo.recorder.is_none() {
                    let ppp = ctx.pixels_per_point();
                    let r = ctx.content_rect();
                    let size = [
                        (r.width() * ppp).round() as usize,
                        (r.height() * ppp).round() as usize,
                    ];
                    match start_recording(&demo.out, &demo.name, size) {
                        Ok(recorder) => {
                            let line = format!("● recording {}", recorder.path.display());
                            demo.note(&line);
                            demo.recorder = Some(recorder);
                        }
                        Err(e) => demo.fail("recording", &e),
                    }
                } else if !*start && let Some(recorder) = demo.recorder.take() {
                    let message = finish_recording(recorder);
                    demo.note(&message);
                }
            }
        }
        demo.script[index] = step;
    }

    /// The cursor, its clicks and the caption, over everything else.
    fn paint_demo(&self, ctx: &egui::Context, demo: &Demo) {
        if demo.clean {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("fashioncraft.demo.overlay"),
        ));
        for (at, when) in &demo.ripples {
            let age = (demo.clock - when) as f32;
            if age < 0.45 {
                let a = 1. - age / 0.45;
                painter.circle_stroke(
                    *at,
                    8. + 26. * (1. - a),
                    egui::Stroke::new(
                        3. * a,
                        Color32::from_rgba_unmultiplied(221, 44, 206, (220. * a) as u8),
                    ),
                );
            }
        }
        // An arrow cursor: white with a dark edge, tip at the pointer.
        let p = demo.cursor;
        let s = 1.15;
        let shape = [
            (0., 0.),
            (0., 17.),
            (4.2, 13.2),
            (7.2, 20.),
            (9.8, 18.9),
            (6.9, 12.3),
            (12.4, 12.3),
        ]
        .map(|(x, y)| p + egui::vec2(x * s, y * s));
        let shadow: Vec<egui::Pos2> = shape.iter().map(|q| *q + egui::vec2(1.5, 2.)).collect();
        painter.add(egui::Shape::convex_polygon(
            shadow,
            Color32::from_black_alpha(70),
            egui::Stroke::NONE,
        ));
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: shape.to_vec(),
            closed: true,
            fill: if demo.pressed {
                Color32::from_rgb(255, 214, 248)
            } else {
                Color32::WHITE
            },
            stroke: egui::Stroke::new(1.4, Color32::from_rgb(20, 20, 22)).into(),
        }));
        if let Some((text, since)) = &demo.caption {
            let screen = ctx.content_rect();
            let fade = ((demo.clock - since) / 0.3).clamp(0., 1.) as f32;
            let galley = painter.layout(
                text.clone(),
                egui::FontId::proportional(26.),
                Color32::from_white_alpha((255. * fade) as u8),
                screen.width() * 0.7,
            );
            let size = galley.size() + egui::vec2(36., 22.);
            let rect = egui::Rect::from_center_size(
                egui::pos2(screen.center().x, screen.bottom() - 70. - size.y / 2.),
                size,
            );
            painter.rect_filled(rect, 10., Color32::from_black_alpha((190. * fade) as u8));
            painter.galley(rect.min + egui::vec2(18., 11.), galley, Color32::WHITE);
        }
        if demo.paused {
            let screen = ctx.content_rect();
            painter.text(
                screen.center_top() + egui::vec2(0., 60.),
                egui::Align2::CENTER_TOP,
                "‖ F9",
                egui::FontId::proportional(22.),
                Color32::from_rgb(221, 44, 206),
            );
        }
    }

    /// Clear last frame's reported targets; widgets report again as they are drawn.
    pub(super) fn begin_demo_frame(&mut self, ctx: &egui::Context) {
        if self.demo.is_some() {
            self.demo_points_before = std::mem::take(&mut self.demo_points);
            ctx.data_mut(|d| {
                let last =
                    std::mem::take(d.get_temp_mut_or_default::<HashMap<String, egui::Rect>>(
                        egui::Id::new(TARGETS),
                    ));
                d.insert_temp(egui::Id::new(PREVIOUS), last);
            });
        }
    }
}

fn describe(step: &Step) -> String {
    let target = |t: &Target| match t {
        Target::Widget(key) => format!("«{key}»"),
        Target::Find(_) => "a found point".into(),
    };
    match step {
        Step::Hover(t) => format!("hover {}", target(t)),
        Step::Click(t) => format!("click {}", target(t)),
        Step::Drag(a, b) => format!("drag {} → {}", target(a), target(b)),
        Step::DragRight(a, b) => format!("right-drag {} → {}", target(a), target(b)),
        _ => String::new(),
    }
}

fn start_recording(
    out: &std::path::Path,
    name: &str,
    size: [usize; 2],
) -> Result<Recorder, String> {
    let ffmpeg = super::animation_export::ffmpeg().ok_or("ffmpeg is not installed")?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = out.join(format!("{name}-{stamp}.mp4"));
    // Wider recordings keep detail for crops (a vertical video cut from a landscape window).
    let max_width = std::env::var("FASHIONCRAFT_DEMO_MAX_WIDTH")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|w| (640..=7680).contains(w))
        .unwrap_or(1920);
    let mut child = std::process::Command::new(ffmpeg)
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
        ])
        .arg(format!("{}x{}", size[0], size[1]))
        .args(["-r", &FPS.to_string(), "-i", "-"])
        .args(["-vf"])
        .arg(format!(
            "scale='min({max_width},iw)':-2:flags=lanczos,format=yuv420p"
        ))
        .args([
            "-c:v",
            "libx264",
            "-crf",
            "18",
            "-preset",
            "medium",
            "-movflags",
            "+faststart",
        ])
        .arg(&path)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("ffmpeg has no input")?;
    let (frames, receive) = mpsc::sync_channel::<Vec<u8>>(8);
    let writer = std::thread::spawn(move || {
        for frame in receive {
            stdin.write_all(&frame).map_err(|e| e.to_string())?;
        }
        drop(stdin);
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    });
    Ok(Recorder {
        size,
        frames,
        writer,
        path,
        count: 0,
    })
}

fn finish_recording(recorder: Recorder) -> String {
    let Recorder {
        frames,
        writer,
        path,
        count,
        ..
    } = recorder;
    drop(frames);
    match writer.join() {
        Ok(Ok(())) => format!(
            "■ recorded {count} frames ({:.1} s) to {}",
            count as f64 / FPS,
            path.display()
        ),
        Ok(Err(e)) => format!("✗ recording failed: {e}"),
        Err(_) => "✗ the video encoder stopped unexpectedly".into(),
    }
}
