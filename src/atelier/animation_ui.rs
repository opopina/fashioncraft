//! Animation: a timeline under the views with the avatar's keyed poses, ready-made motions
//! and markers. Scrubbing or playing it shows the avatar in motion; recording simulates the
//! cloth while the body moves, keeping one frame per tick, and the timeline then plays that
//! take back. Recording runs on its own thread with its own solver; the live drape moves on
//! to where it ends. Nothing else shown on the timeline touches the live drape: closing the
//! editor shows it again.
use super::*;
use marvelous_cloth::{
    avatar::SurfaceMesh,
    avatar_animation::{self, AvatarAnimation, Recorder, Take},
    avatar_rig::Rig,
    fitting::{AvatarPose, SceneObject},
    model::State,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const SPEEDS: [f64; 5] = [0.25, 0.5, 1., 2., 4.];
/// Height of the timeline strip, points.
const STRIP: f32 = 48.;

pub(super) struct AnimationUi {
    pub open: bool,
    /// Timeline time, seconds.
    time: f64,
    /// The timeline plays on its own (avatar and take only, no simulation).
    previewing: bool,
    speed: f64,
    looping: bool,
    motion: usize,
    /// The avatar's skeleton and rest surface, keyed by its path and joints.
    rig: Option<(String, Arc<Rig>, Arc<SurfaceMesh>)>,
    props: Option<(Vec<SceneObject>, Option<Arc<SurfaceMesh>>)>,
    /// The avatar posed at a timeline time, keyed by that time and the animation.
    posed: Option<(u64, AvatarAnimation, Arc<SurfaceMesh>)>,
    recording: Option<RecordingJob>,
    /// The last take, with the vertex count it was recorded on.
    take: Option<(usize, Take)>,
    /// The take's point cache on disk, which saving the project copies next to it.
    take_file: Option<std::path::PathBuf>,
    error: Option<String>,
}

impl Default for AnimationUi {
    fn default() -> Self {
        Self {
            open: false,
            time: 0.,
            previewing: false,
            speed: 1.,
            looping: true,
            motion: 0,
            rig: None,
            props: None,
            posed: None,
            recording: None,
            take: None,
            take_file: None,
            error: None,
        }
    }
}

impl AnimationUi {
    pub(super) fn recording(&self) -> bool {
        self.recording.is_some()
    }
}

/// What the recording thread last showed: where the body and the cloth are.
#[derive(Default)]
struct Progress {
    time: f64,
    share: f64,
    lead_in: bool,
    cloth: Option<Vec<glam::DVec3>>,
    body: Option<Arc<SurfaceMesh>>,
}

type Recorded = Result<(Recorder, State), String>;

/// A recording in progress on its own thread.
struct RecordingJob {
    progress: Arc<Mutex<Progress>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Recorded>>,
    /// The scene the recording started from, so a later edit is not overwritten.
    generation: u64,
}

/// A timeline button's work, run once the panel is drawn.
type TimelineAction = fn(&mut Atelier, &egui::Context) -> Result<(), String>;

/// How often the recording thread shares its cloth and body for display.
const SHOW_EVERY: std::time::Duration = std::time::Duration::from_millis(60);

/// A tick spacing that leaves room for its labels.
fn tick_step(seconds: f64, width: f32) -> f64 {
    [0.1, 0.2, 0.5, 1., 2., 5., 10., 30., 60.]
        .into_iter()
        .find(|s| (width as f64) * s / seconds >= 56.)
        .unwrap_or(120.)
}

impl Atelier {
    /// The avatar's pose as stored, or its joints at rest when it has none yet.
    fn animated_avatar(&self) -> Result<AvatarPose, String> {
        match &self.project.avatar_pose {
            Some(p) => Ok(p.clone()),
            None => AvatarPose::from_sidecar(&self.project.avatar_path),
        }
    }

    /// The project's animation, created at the avatar's pose when there is none yet.
    fn animation_or_new(&mut self) -> Result<&mut AvatarAnimation, String> {
        if self.project.avatar_animation.is_none() {
            let avatar = self.animated_avatar()?;
            self.project.avatar_animation = Some(AvatarAnimation::starting_at(avatar.pose));
        }
        self.document.dirty = true;
        Ok(self.project.avatar_animation.as_mut().unwrap())
    }

    /// Whether the take still fits the cloth on show.
    pub(super) fn current_take(&self) -> Option<&Take> {
        self.authoring
            .animation
            .take
            .as_ref()
            .filter(|(n, _)| *n == self.scene.state.x.len())
            .map(|(_, t)| t)
    }

    /// The timeline shows something other than the live drape: the editor is open and nothing
    /// is being simulated or recorded.
    fn animation_previewing(&self) -> bool {
        let a = &self.authoring.animation;
        a.open
            && a.recording.is_none()
            && !self.playing
            && (self.project.avatar_animation.is_some() || self.current_take().is_some())
    }

    /// The cloth of the take at the timeline time, in place of the live drape.
    pub(super) fn animation_frame_state(&self) -> Option<State> {
        if let Some(job) = &self.authoring.animation.recording {
            let cloth = job.progress.lock().ok()?.cloth.clone()?;
            return Some(State {
                v: vec![glam::DVec3::ZERO; cloth.len()],
                x: cloth,
                time: self.scene.state.time,
            });
        }
        if !self.animation_previewing() {
            return None;
        }
        let take = self.current_take()?;
        let frame = take.frame_index(self.authoring.animation.time);
        Some(State {
            x: take.positions(frame),
            v: vec![glam::DVec3::ZERO; self.scene.state.x.len()],
            time: self.scene.state.time,
        })
    }

    /// The avatar at the timeline time, in place of the live one.
    pub(super) fn animation_avatar(&mut self) -> Option<Arc<SurfaceMesh>> {
        if let Some(job) = &self.authoring.animation.recording {
            return job.progress.lock().ok()?.body.clone();
        }
        if !self.animation_previewing() {
            return None;
        }
        let animation = self.project.avatar_animation.clone()?;
        let time = self.authoring.animation.time;
        if let Some((t, a, surface)) = &self.authoring.animation.posed
            && *t == time.to_bits()
            && *a == animation
        {
            return Some(surface.clone());
        }
        let posed = (|| -> Result<Arc<SurfaceMesh>, String> {
            let avatar = self.animated_avatar()?;
            let key = format!(
                "{}|{}",
                self.project.avatar_path.display(),
                serde_json::to_string(&avatar.joints).map_err(|e| e.to_string())?
            );
            let state = &mut self.authoring.animation;
            if state.rig.as_ref().is_none_or(|(k, _, _)| *k != key) {
                let rest = SurfaceMesh::load_obj(&self.project.avatar_path)?;
                let rig = avatar.rig(&rest)?;
                state.rig = Some((key, Arc::new(rig), Arc::new(rest)));
            }
            if state
                .props
                .as_ref()
                .is_none_or(|(objects, _)| *objects != self.project.scene_objects)
            {
                let mesh =
                    marvelous_cloth::fitting::scene_objects_mesh(&self.project.scene_objects)?;
                state.props = Some((self.project.scene_objects.clone(), mesh.map(Arc::new)));
            }
            let (_, rig, rest) = state.rig.as_ref().unwrap();
            let mut body = rig.posed(rest, &animation.pose_at(time));
            if let Some(props) = &state.props.as_ref().unwrap().1 {
                body = body.merged(props);
            }
            Ok(Arc::new(body))
        })();
        match posed {
            Ok(surface) => {
                self.authoring.animation.posed = Some((time.to_bits(), animation, surface.clone()));
                Some(surface)
            }
            Err(e) => {
                self.authoring.animation.error = Some(e);
                None
            }
        }
    }

    /// Move the timeline to `time`; the pose window, when open, edits the pose found there.
    pub(super) fn seek_animation(&mut self, time: f64) {
        let a = &mut self.authoring.animation;
        a.time = time.max(0.);
        if self.authoring.pose.open
            && let Some(animation) = &self.project.avatar_animation
            && let Ok(mut avatar) = self.animated_avatar()
        {
            avatar.pose = animation.pose_at(time);
            self.authoring.pose.draft = Some(avatar);
        }
    }

    /// Simulate the cloth along the animation from the live drape, a frame at a time, on a
    /// thread of its own.
    pub(super) fn start_animation_recording(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.authoring.animation.recording.is_some() {
            return Err("Already recording".into());
        }
        if self.authoring.pose.transition_running() {
            return Err("Wait for the pose change to finish".into());
        }
        let animation = self
            .project
            .avatar_animation
            .clone()
            .filter(|a| a.duration() > 0.)
            .ok_or("Insert a motion or set a second key before recording")?;
        if !self.prepare_simulation() {
            return Err(self.status.clone());
        }
        let recorder = Recorder::new(&self.project, &animation)?;
        // The live simulation stops: the recording takes over from the drape as it is.
        self.playing = false;
        self.grab = None;
        let progress = Arc::new(Mutex::new(Progress {
            body: Some(self.scene.collider.surface.clone()),
            ..Default::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let garment = self.scene.garment.clone();
        let surface = self.scene.collider.surface.clone();
        let state = self.scene.state.clone();
        let settings = self.settings.clone();
        let (shared, halt, repaint) = (progress.clone(), stop.clone(), ctx.clone());
        let thread = std::thread::Builder::new()
            .name("cloth-record".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let mut shown = std::time::Instant::now() - SHOW_EVERY;
                avatar_animation::run(garment, &surface, state, recorder, &settings, |r, s| {
                    if shown.elapsed() >= SHOW_EVERY {
                        shown = std::time::Instant::now();
                        if let Ok(mut p) = shared.lock() {
                            p.time = r.time();
                            p.share = r.progress();
                            p.lead_in = r.moving_into_first_pose();
                            p.cloth = Some(s.x.clone());
                            p.body = Some(Arc::new(r.obstacle_now()));
                        }
                        repaint.request_repaint();
                    }
                    !halt.load(Ordering::Relaxed)
                })
            })
            .map_err(|e| e.to_string())?;
        let a = &mut self.authoring.animation;
        a.recording = Some(RecordingJob {
            progress,
            stop,
            thread: Some(thread),
            generation: self.generation,
        });
        a.previewing = false;
        a.take = None;
        a.error = None;
        self.status = "Recording…".into();
        Ok(())
    }

    /// Ask the recording to stop after its current step; what it recorded is kept.
    pub(super) fn stop_animation_recording(&mut self) {
        if let Some(job) = &self.authoring.animation.recording {
            job.stop.store(true, Ordering::Relaxed);
        }
    }

    /// The recording thread ended: keep its take and move the live drape and avatar to where
    /// it stopped, unless the scene was edited meanwhile.
    fn finish_animation_recording(&mut self, recorded: Recorded) {
        let generation = self
            .authoring
            .animation
            .recording
            .take()
            .map_or(self.generation, |j| j.generation);
        let (recorder, state) = match recorded {
            Ok(done) => done,
            Err(e) => {
                self.authoring.animation.error = Some(e.clone());
                self.status = e;
                return;
            }
        };
        let completed = recorder.finished();
        let pose = recorder.pose_now();
        let obstacle = recorder.obstacle_now();
        let take = recorder.into_take();
        let frames = take.frames.len();
        if generation == self.generation && state.x.len() == self.scene.state.x.len() {
            match marvelous_cloth::avatar::MeshCollider::with_clearance(
                Arc::new(obstacle),
                self.project.contact_stiffness,
                self.project.contact_clearance,
            ) {
                Ok(collider) => {
                    self.scene.collider = Arc::new(collider);
                    self.scene.state = state;
                    if let Ok(mut avatar) = self.animated_avatar() {
                        avatar.pose = pose;
                        self.project.avatar_pose = Some(avatar);
                    }
                    // The simulation worker starts again from here, with this body.
                    self.generation += 1;
                    self.pending = false;
                    self.issue.invalidate();
                    self.document.dirty = true;
                }
                Err(e) => self.authoring.animation.error = Some(e),
            }
        }
        // Kept on disk so the project can be saved with it.
        let file = self.output.join("recorded-take.pc2");
        let written = frames > 0 && take.write_pc2(&file).is_ok();
        let a = &mut self.authoring.animation;
        a.time = take.duration();
        a.take_file = written.then_some(file);
        if frames > 0 {
            a.take = Some((self.scene.state.x.len(), take));
        }
        self.status = if completed {
            format!("Recorded {frames} frames · scrub the timeline to play the take back")
        } else {
            format!("Recording stopped after {frames} frames")
        };
    }

    /// Every frame: play the timeline, and collect a recording that has ended.
    pub(super) fn animation_tick(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.authoring.animation.recording.as_mut() {
            if job.thread.as_ref().is_some_and(|t| t.is_finished()) {
                let recorded = job
                    .thread
                    .take()
                    .unwrap()
                    .join()
                    .unwrap_or_else(|_| Err("The recording stopped unexpectedly".into()));
                self.finish_animation_recording(recorded);
            } else {
                if let Ok(p) = job.progress.lock() {
                    self.status = if p.lead_in {
                        "Recording · moving into the first pose…".into()
                    } else {
                        format!("Recording… {:.0} %", p.share * 100.)
                    };
                }
                ctx.request_repaint_after(SHOW_EVERY);
            }
            return;
        }
        let a = &self.authoring.animation;
        if !a.previewing {
            return;
        }
        if !self.animation_previewing() {
            self.authoring.animation.previewing = false;
            return;
        }
        let end = self.animation_end();
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64 * self.authoring.animation.speed;
        let mut time = self.authoring.animation.time + dt;
        if time > end {
            if self.authoring.animation.looping && end > 0. {
                time = 0.;
            } else {
                time = end;
                self.authoring.animation.previewing = false;
            }
        }
        self.seek_animation(time);
        ctx.request_repaint();
    }

    /// The take to save with the project: its point cache and frame rate.
    pub(super) fn take_to_save(&self) -> Option<marvelous_cloth::fitting::AnimationTake> {
        let take = self.current_take()?;
        let path = self.authoring.animation.take_file.clone()?;
        Some(marvelous_cloth::fitting::AnimationTake {
            path,
            fps: take.fps,
        })
    }

    /// Read back a take saved with the project, when it still fits the cloth on show.
    pub(super) fn restore_take(&mut self, saved: Option<marvelous_cloth::fitting::AnimationTake>) {
        let a = &mut self.authoring.animation;
        a.take = None;
        a.take_file = None;
        a.time = 0.;
        a.previewing = false;
        let Some(saved) = saved else {
            return;
        };
        match Take::read_pc2(&saved.path, saved.fps) {
            Ok(take) if take.frames[0].len() == self.scene.state.x.len() => {
                a.take = Some((self.scene.state.x.len(), take));
                a.take_file = Some(saved.path);
            }
            Ok(_) => a.error = Some("The saved take was recorded on another mesh".into()),
            Err(e) => a.error = Some(format!("The saved take could not be read: {e}")),
        }
    }

    /// How far a recording in progress has come.
    fn recording_time(&self) -> Option<f64> {
        let job = self.authoring.animation.recording.as_ref()?;
        job.progress.lock().ok().map(|p| p.time)
    }

    /// Length of the timeline: the animation or the take, whichever is longer.
    fn animation_end(&self) -> f64 {
        let keys = self
            .project
            .avatar_animation
            .as_ref()
            .map_or(0., AvatarAnimation::duration);
        let take = self.current_take().map_or(0., Take::duration);
        keys.max(take)
    }

    pub(super) fn stop_animation_preview(&mut self) {
        self.authoring.animation.previewing = false;
    }

    pub(super) fn toggle_animation_preview(&mut self) -> Result<(), String> {
        if self.authoring.animation.recording.is_some() {
            return Err("Stop recording first".into());
        }
        if self.project.avatar_animation.is_none() && self.current_take().is_none() {
            return Err("The avatar has no animation yet".into());
        }
        self.authoring.animation.open = true;
        self.playing = false;
        let end = self.animation_end();
        let a = &mut self.authoring.animation;
        a.previewing = !a.previewing;
        if a.previewing && a.time >= end - 1e-9 {
            a.time = 0.;
        }
        Ok(())
    }

    pub(super) fn delete_animation(&mut self) -> Result<(), String> {
        if self.authoring.animation.recording.is_some() {
            return Err("Stop recording first".into());
        }
        if self.project.avatar_animation.is_none() && self.authoring.animation.take.is_none() {
            return Err("The avatar has no animation".into());
        }
        self.project.avatar_animation = None;
        let a = &mut self.authoring.animation;
        a.take = None;
        a.take_file = None;
        a.time = 0.;
        a.previewing = false;
        a.posed = None;
        self.document.dirty = true;
        self.status = "Animation deleted".into();
        Ok(())
    }

    /// The timeline panel under the views.
    pub(super) fn animation_timeline(&mut self, ui: &mut egui::Ui) {
        let language = self.prefs.language;
        let t = |s: &'static str| i18n::tr(language, s);
        let recording = self.authoring.animation.recording.is_some();
        let fps = self
            .project
            .avatar_animation
            .as_ref()
            .map_or(30., |a| a.fps);
        let end = self.animation_end();
        let mut seek = None;
        let mut action: Option<TimelineAction> = None;
        ui.horizontal(|ui| {
            ui.strong(t("Animation"));
            ui.separator();
            ui.add_enabled_ui(!recording, |ui| {
                if ui.button("⏮").on_hover_text(t("To the start")).clicked() {
                    seek = Some(0.);
                }
                let label = if self.authoring.animation.previewing {
                    t("Pause")
                } else {
                    t("Play")
                };
                if ui.button(label).clicked() {
                    action = Some(|s, _| s.toggle_animation_preview());
                }
            });
            let time = self.recording_time().unwrap_or(self.authoring.animation.time);
            ui.monospace(format!(
                "{time:5.2} s / {end:.2} s · {} {}",
                t("frame"),
                (time * fps).round() as usize
            ));
            egui::ComboBox::from_id_salt("animation.speed")
                .width(58.)
                .selected_text(format!("{}×", self.authoring.animation.speed))
                .show_ui(ui, |ui| {
                    for s in SPEEDS {
                        ui.selectable_value(
                            &mut self.authoring.animation.speed,
                            s,
                            format!("{s}×"),
                        );
                    }
                })
                .response
                .on_hover_text(t("Playback speed"));
            ui.checkbox(&mut self.authoring.animation.looping, t("Loop"));
            ui.separator();
            ui.add_enabled_ui(!recording, |ui| {
                if ui
                    .button(t("Set key"))
                    .on_hover_text(t(
                        "Key the pose being edited in the Pose window, or the avatar's pose, at this time",
                    ))
                    .clicked()
                {
                    action = Some(|s, _| {
                        let pose = match &s.authoring.pose.draft {
                            Some(d) if s.authoring.pose.open => d.pose.clone(),
                            _ => s.animated_avatar()?.pose,
                        };
                        let time = s.authoring.animation.time;
                        s.animation_or_new()?.set_key(time, pose);
                        s.status = format!("Key set at {time:.2} s");
                        Ok(())
                    });
                }
                let near = self
                    .project
                    .avatar_animation
                    .as_ref()
                    .and_then(|a| a.key_near(self.authoring.animation.time))
                    .filter(|_| {
                        self.project
                            .avatar_animation
                            .as_ref()
                            .is_some_and(|a| a.keys.len() > 1)
                    });
                if ui
                    .add_enabled(near.is_some(), egui::Button::new(t("Delete key")))
                    .clicked()
                    && let Some(i) = near
                    && let Some(a) = self.project.avatar_animation.as_mut()
                {
                    a.keys.remove(i);
                    if i == 0 {
                        // The first key always sits at the start.
                        a.keys[0].time_s = 0.;
                    }
                    self.document.dirty = true;
                }
                egui::ComboBox::from_id_salt("animation.motion")
                    .width(110.)
                    .selected_text(t(avatar_animation::MOTIONS[self.authoring.animation.motion]))
                    .show_ui(ui, |ui| {
                        for (k, name) in avatar_animation::MOTIONS.iter().enumerate() {
                            ui.selectable_value(&mut self.authoring.animation.motion, k, t(name));
                        }
                    });
                if ui
                    .button(t("Use motion"))
                    .on_hover_text(t(
                        "Replace the keys with this motion, starting from the avatar's pose",
                    ))
                    .clicked()
                {
                    action = Some(|s, _| {
                        let start = s.animated_avatar()?.pose;
                        let name = avatar_animation::MOTIONS[s.authoring.animation.motion];
                        let made = avatar_animation::motion(name, &start).unwrap();
                        let a = s.animation_or_new()?;
                        a.keys = made.keys;
                        s.authoring.animation.time = 0.;
                        s.status = format!("Motion: {name}");
                        Ok(())
                    });
                }
                if ui.button(t("Add marker")).clicked() {
                    action = Some(|s, _| {
                        let time = s.authoring.animation.time;
                        s.animation_or_new()?.add_marker(time, None);
                        Ok(())
                    });
                }
                if let Some(a) = self.project.avatar_animation.as_mut() {
                    let mut f = a.fps;
                    if ui
                        .add(
                            egui::DragValue::new(&mut f)
                                .range(avatar_animation::FPS_RANGE)
                                .speed(1.)
                                .suffix(" fps"),
                        )
                        .on_hover_text(t("Frames recorded per second"))
                        .changed()
                    {
                        a.fps = f.round();
                        self.document.dirty = true;
                    }
                }
            });
            ui.separator();
            if recording {
                if ui.button(t("Stop")).clicked() {
                    self.stop_animation_recording();
                }
            } else if ui
                .button(t("Record"))
                .on_hover_text(t(
                    "Simulate the cloth while the avatar moves, from the drape as it is now",
                ))
                .clicked()
            {
                action = Some(|s, ctx| s.start_animation_recording(ctx));
            }
            if let Some(take) = self.current_take() {
                ui.label(format!("{} · {} {}", t("Take"), take.frames.len(), t("frames")));
                if ui
                    .add_enabled(!recording, egui::Button::new(t("Clear take")))
                    .clicked()
                {
                    self.authoring.animation.take = None;
                    self.authoring.animation.take_file = None;
                    self.document.dirty = true;
                }
            }
        });
        if let Some(e) = &self.authoring.animation.error {
            ui.colored_label(Color32::from_rgb(225, 120, 75), e);
        }
        if let Some(time) = self.timeline_strip(ui, end, fps, recording) {
            seek = Some(time);
        }
        if let Some(time) = seek {
            self.authoring.animation.previewing = false;
            self.seek_animation(time);
        }
        if let Some(run) = action {
            self.authoring.animation.error = None;
            let ctx = ui.ctx().clone();
            if let Err(e) = run(self, &ctx) {
                self.authoring.animation.error = Some(e);
            }
        }
    }

    /// The strip with ticks, keys, markers, the recorded span and the playhead. Returns a time
    /// picked with the pointer.
    fn timeline_strip(
        &mut self,
        ui: &mut egui::Ui,
        end: f64,
        fps: f64,
        locked: bool,
    ) -> Option<f64> {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), STRIP),
            if locked {
                egui::Sense::hover()
            } else {
                egui::Sense::click_and_drag()
            },
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3., ui.visuals().extreme_bg_color);
        let span = end.max(1.);
        let inner = rect.shrink2(egui::vec2(10., 0.));
        let x_of = |t: f64| inner.left() + (t / span) as f32 * inner.width();
        let text = ui.visuals().weak_text_color();
        let step = tick_step(span, inner.width());
        let mut tick = 0.;
        while tick <= span + 1e-9 {
            let x = x_of(tick);
            painter.line_segment(
                [
                    egui::pos2(x, rect.bottom() - 8.),
                    egui::pos2(x, rect.bottom()),
                ],
                egui::Stroke::new(1., text),
            );
            painter.text(
                egui::pos2(x + 2., rect.bottom() - 9.),
                egui::Align2::LEFT_BOTTOM,
                format!("{tick:.1}"),
                egui::FontId::proportional(9.),
                text,
            );
            tick += step;
        }
        if let Some(take) = self.current_take() {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(x_of(0.), rect.bottom() - 3.),
                    egui::pos2(x_of(take.duration()), rect.bottom()),
                ),
                0.,
                Color32::from_rgb(90, 180, 110),
            );
        }
        if let Some(time) = self.recording_time() {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(x_of(0.), rect.bottom() - 3.),
                    egui::pos2(x_of(time), rect.bottom()),
                ),
                0.,
                Color32::from_rgb(220, 70, 60),
            );
        }
        let mut picked = None;
        if let Some(a) = &self.project.avatar_animation {
            let y = rect.center().y;
            for key in &a.keys {
                let c = egui::pos2(x_of(key.time_s), y);
                let r = 5.;
                let at_head = (key.time_s - self.authoring.animation.time).abs() < 0.5 / fps;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        c + egui::vec2(0., -r),
                        c + egui::vec2(r, 0.),
                        c + egui::vec2(0., r),
                        c + egui::vec2(-r, 0.),
                    ],
                    if at_head {
                        Color32::from_rgb(255, 214, 90)
                    } else {
                        Color32::from_rgb(210, 170, 70)
                    },
                    egui::Stroke::new(1., Color32::BLACK),
                ));
                if response.clicked()
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|p| p.distance(c) < 8.)
                {
                    picked = Some(key.time_s);
                }
            }
            for marker in &a.markers {
                let x = x_of(marker.time_s);
                let blue = Color32::from_rgb(90, 150, 230);
                painter.line_segment(
                    [
                        egui::pos2(x, rect.top() + 2.),
                        egui::pos2(x, rect.top() + 14.),
                    ],
                    egui::Stroke::new(1.5, blue),
                );
                painter.text(
                    egui::pos2(x + 3., rect.top() + 2.),
                    egui::Align2::LEFT_TOP,
                    &marker.name,
                    egui::FontId::proportional(10.),
                    blue,
                );
                if response.clicked()
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|p| (p.x - x).abs() < 6. && p.y < rect.top() + 16.)
                {
                    picked = Some(marker.time_s);
                }
            }
        }
        let head = self
            .recording_time()
            .unwrap_or(self.authoring.animation.time);
        let x = x_of(head);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2., Color32::from_rgb(230, 80, 70)),
        );
        if picked.is_some() {
            return picked;
        }
        if (response.dragged() || response.clicked())
            && let Some(p) = response.interact_pointer_pos()
        {
            let t = ((p.x - inner.left()) / inner.width()) as f64 * span;
            return Some(((t.clamp(0., span)) * fps).round() / fps);
        }
        None
    }
}
