//! Seams that break by themselves (seam inspector › Breaks at): after each step the worker
//! returns, the stitches of seams with a breaking strength are checked; those pulled harder
//! than that for a moment open as tears of their seam, in one undo step, and the simulation
//! goes on from where it was.
use super::*;
use fashioncraft::{
    authoring::{self, Command},
    garment::seam_break::{DEFAULT_STRENGTH_N_PER_M, STRENGTH_N_PER_M, SeamBreaker, StitchBreak},
};

/// What the seams with a breaking strength have been through in this simulation.
#[derive(Default)]
pub(super) struct SeamBreaking {
    breaker: SeamBreaker,
    /// The garment build the watch belongs to; another one (an edit, a reset) starts over.
    generation: u64,
    /// Simulated time of the last look, seconds.
    time: Option<f64>,
    /// What broke last and when, kept in the status line for a moment over the simulation's
    /// own messages.
    notice: Option<(String, std::time::Instant)>,
    /// How many edits could be undone right after the last break was recorded: while that
    /// is still the history's last step, later breaks join it, so one undo mends everything a
    /// run tore.
    entry: Option<usize>,
}

/// How long the status line keeps saying what broke.
const NOTICE: std::time::Duration = std::time::Duration::from_secs(4);

impl Atelier {
    /// Check the stitches of the state the worker just returned, and open those that broke.
    pub(super) fn observe_seam_breaks(&mut self) {
        let watch = &mut self.authoring.seam_breaking;
        if watch.generation != self.generation {
            watch.breaker.reset();
            watch.generation = self.generation;
            watch.time = None;
            watch.entry = None;
        }
        match watch
            .notice
            .as_ref()
            .filter(|(_, since)| since.elapsed() < NOTICE)
        {
            Some((notice, _)) => self.status = notice.clone(),
            None => watch.notice = None,
        }
        let strengths = &self.project.template.seam_strengths;
        if strengths.is_empty() {
            watch.time = None;
            return;
        }
        let now = self.scene.state.time;
        let dt = watch.time.map_or(0., |before| now - before);
        watch.time = Some(now);
        let breaks = watch
            .breaker
            .observe(strengths, &self.scene.garment, &self.scene.state.x, dt);
        if !breaks.is_empty() {
            self.install_breaks(breaks);
        }
    }

    /// Record broken stitches as tears of their seams (one undo step) and go on simulating
    /// from the same state with the seams as they are now.
    fn install_breaks(&mut self, breaks: Vec<StitchBreak>) {
        let ui_lang = self.prefs.language;
        let count = breaks.len();
        let pull = breaks.iter().map(|b| b.pull_n_per_m).fold(0., f64::max);
        let mut seams: Vec<String> = breaks.iter().map(|b| b.seam.clone()).collect();
        seams.dedup();
        let edit = match authoring::apply(&self.project.template, Command::BreakStitches { breaks })
        {
            Ok(edit) => edit,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let name = Command::BreakStitches { breaks: vec![] }.name();
        let history = &mut self.authoring.history;
        let continues = self.authoring.seam_breaking.entry == Some(history.done().count())
            && history.done().last() == Some(name);
        if !continues {
            history.record_named(self.project.template.clone(), name);
        }
        self.authoring.seam_breaking.entry = Some(history.done().count());
        if edit.garment.mesh.uv.len() != self.scene.state.x.len() {
            // Tears never remesh; should one, take the ordinary path.
            self.install_edit(edit);
            return;
        }
        self.project.template = edit.template;
        self.project.state = None;
        self.project.mesh_signature = None;
        self.scene.garment = std::sync::Arc::new(edit.garment);
        self.document.dirty = true;
        // A new build for the worker, from the same state; the watch carries over, so the
        // other stitches keep their history.
        self.generation += 1;
        self.authoring.seam_breaking.generation = self.generation;
        self.pending = false;
        self.metrics = None;
        // Tears never change the mesh: a grab in progress keeps hold of the cloth.
        self.status = format!(
            "{} {} · {} {} · {:.1} N/cm",
            i18n::tr(ui_lang, "Seam broke:"),
            seams.join(", "),
            count,
            i18n::tr(
                ui_lang,
                if count == 1 {
                    "stitch gave way"
                } else {
                    "stitches gave way"
                }
            ),
            pull / 100.
        );
        self.authoring.seam_breaking.notice =
            Some((self.status.clone(), std::time::Instant::now()));
    }

    /// The seam inspector's breaking strength: off (it never breaks) or the pull per
    /// centimetre of seam above which its stitches break while simulating.
    pub(super) fn seam_strength_controls(&mut self, ui: &mut egui::Ui, seam: &str) {
        let ui_lang = self.prefs.language;
        let current = self.project.template.seam_strengths.get(seam).copied();
        let mut wanted: Option<Option<f64>> = None;
        ui.horizontal_wrapped(|ui| {
            let mut on = current.is_some();
            let check = ui.checkbox(&mut on, "").on_hover_text(i18n::tr(
                ui_lang,
                "The seam breaks where it is pulled harder than this while simulating",
            ));
            super::demo::mark(ui.ctx(), "seam.breaks", check.rect);
            if check.clicked() {
                wanted = Some(current.is_none().then_some(DEFAULT_STRENGTH_N_PER_M));
            }
            if let Some(strength) = current {
                // Shown per centimetre of seam, kept per metre.
                let mut per_cm = strength / 100.;
                let r = ui
                    .add(
                        egui::DragValue::new(&mut per_cm)
                            .range(STRENGTH_N_PER_M.start() / 100.0..=STRENGTH_N_PER_M.end() / 100.)
                            .speed(0.05)
                            .max_decimals(2)
                            .suffix(" N/cm"),
                    )
                    .on_hover_text(i18n::tr(
                        ui_lang,
                        "Breaking strength: pull per centimetre of seam a stitch holds",
                    ));
                super::demo::mark(ui.ctx(), "seam.strength", r.rect);
                if (r.drag_stopped() || (r.changed() && !r.dragged()))
                    && (per_cm * 100. - strength).abs() > 1e-9
                {
                    wanted = Some(Some(per_cm * 100.));
                }
            }
        });
        if let Some(strength) = wanted {
            self.edit(Command::SetSeamStrength {
                seam: seam.into(),
                strength,
            });
        }
    }
}
