//! Materials › Measure fabric…: type a workshop's measurements of a real fabric, fit a fabric
//! to them with the solver's virtual copies of the same tests, keep it in the library with its
//! measurements and cut the selected piece from it. The procedure is in docs/fabric-kit.md.
use super::*;
use fashioncraft::{
    authoring::Command,
    fabric_kit::{
        self, Direction, FabricFile, FitOptions, FitReport, Measurements, Note, Parameter, Reading,
        Stage, Test,
    },
    fabrics,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool, mpsc};

/// The procedure, as the repository publishes it.
pub(super) const PROCEDURE_URL: &str =
    "https://github.com/opopina/fashioncraft/blob/main/docs/fabric-kit.md";
/// Folder of measured fabrics inside the library.
pub(super) const LIBRARY_FOLDER: &str = "Fabrics";

enum Message {
    Stage(Stage),
    Done(Result<FitReport, String>),
}

struct Job {
    rx: mpsc::Receiver<Message>,
    cancel: Arc<AtomicBool>,
    stage: Option<Stage>,
    started: std::time::Instant,
}

pub(super) struct FabricKitUi {
    pub open: bool,
    name: String,
    measurements: Measurements,
    tab: Direction,
    report: Option<FitReport>,
    /// The measurements `report` was fitted to: once edited, the report is stale.
    fitted: Option<Measurements>,
    job: Option<Job>,
    error: Option<String>,
}

impl Default for FabricKitUi {
    fn default() -> Self {
        Self {
            open: false,
            name: String::new(),
            measurements: Measurements::default(),
            tab: Direction::Warp,
            report: None,
            fitted: None,
            job: None,
            error: None,
        }
    }
}

/// Readings of a light cotton poplin as the procedure would give them: the readings the
/// virtual tests give a fabric of 2400 / 1500 N/m, shear 70 N/m and bending 15 / 9 µN·m at
/// 120 g/m², rounded to what a ruler reads.
pub(super) fn example() -> Measurements {
    let reading = |load_g, length_mm| Reading { load_g, length_mm };
    Measurements {
        gsm: 120.,
        thickness_mm: 0.25,
        strip_width_mm: 50.,
        strip_length_mm: 200.,
        gauge_mm: 100.,
        resolution_mm: 0.5,
        warp: vec![
            reading(500., 104.),
            reading(1000., 107.5),
            reading(2000., 113.5),
        ],
        weft: vec![
            reading(500., 106.),
            reading(1000., 111.5),
            reading(2000., 120.),
        ],
        bias: vec![reading(25., 102.), reading(100., 107.)],
        overhang_warp_mm: Some(61.),
        overhang_weft_mm: Some(53.),
        slide_angle_degrees: Some(24.),
    }
}

/// What the window asks for once it has drawn.
enum Action {
    Fit,
    Cancel,
    Example,
    Clear,
    Save,
    Apply,
    UseFriction(f64),
}

/// A number field with a unit.
fn number(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    suffix: &str,
) {
    super::workspace::prop_row(ui, label, |ui| {
        ui.add(
            egui::DragValue::new(v)
                .range(range)
                .speed(speed)
                .max_decimals(2)
                .suffix(suffix),
        )
    });
}

/// An optional number: a checkbox to say it was measured, then its field.
fn optional(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut Option<f64>,
    default: f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
) {
    super::workspace::prop_row(ui, label, |ui| {
        let mut on = v.is_some();
        if ui.checkbox(&mut on, "").changed() {
            *v = on.then_some(v.unwrap_or(default));
        }
        if let Some(value) = v.as_mut() {
            ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(suffix),
            );
        }
    });
}

fn stage_text(lang: i18n::Language, stage: Stage) -> &'static str {
    let t = |s| i18n::tr(lang, s);
    match stage {
        Stage::Strip(Direction::Warp) => t("Fitting the warp strip…"),
        Stage::Strip(Direction::Weft) => t("Fitting the weft strip…"),
        Stage::Strip(Direction::Bias) => t("Fitting the bias strip…"),
        Stage::Cantilever(Direction::Weft) => t("Fitting the weft cantilever…"),
        Stage::Cantilever(_) => t("Fitting the warp cantilever…"),
        Stage::Checking => t("Running every test with the fitted fabric…"),
    }
}

fn direction_text(lang: i18n::Language, d: Direction) -> &'static str {
    i18n::tr(lang, d.label())
}

/// A note in the interface language.
pub(super) fn note_text(lang: i18n::Language, note: &Note) -> String {
    if lang == i18n::Language::English {
        return note.english();
    }
    let dir = |d: &Direction| match d {
        Direction::Warp => "la urdimbre",
        Direction::Weft => "la trama",
        Direction::Bias => "el bies",
    };
    match note {
        Note::StretchCopied { direction, from } => {
            format!("Sin tira de {}: estira como {}", dir(direction), dir(from))
        }
        Note::ShearEstimated => {
            "Sin tira al bies: la cizalla se estima como el 4 % de la rigidez de urdimbre y trama"
                .into()
        }
        Note::BendingCopied { direction, from } => {
            format!(
                "Sin voladizo de {}: se dobla como {}",
                dir(direction),
                dir(from)
            )
        }
        Note::BendingDefault => "Sin voladizo: la flexión queda en el valor por defecto".into(),
        Note::CoarseReading { direction, percent } => format!(
            "{}: un paso de la regla cambia la rigidez un {percent:.0} %; usa más peso o una distancia entre marcas mayor",
            i18n::tr(lang, direction.label())
        ),
        Note::CurveMismatch { direction, percent } => format!(
            "{}: una sola rigidez no sigue la curva medida; la peor extensión se desvía un {percent:.0} %",
            i18n::tr(lang, direction.label())
        ),
        Note::AtLimit { parameter } => format!(
            "{} llegó a su límite: el modelo no puede reproducir esa lectura",
            match parameter {
                Parameter::Warp => "La rigidez de urdimbre",
                Parameter::Weft => "La rigidez de trama",
                Parameter::Shear => "La cizalla",
                Parameter::BendWarp => "La flexión de urdimbre",
                Parameter::BendWeft => "La flexión de trama",
            }
        ),
    }
}

/// A path for `name` in `dir` that does not overwrite another fabric.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let file = fabric_kit::file_name(name);
    let path = dir.join(&file);
    if !path.exists() {
        return path;
    }
    let stem = file.trim_end_matches(&format!(".{}", fabric_kit::FILE_EXTENSION));
    (2..)
        .map(|i| dir.join(format!("{stem} {i}.{}", fabric_kit::FILE_EXTENSION)))
        .find(|p| !p.exists())
        .unwrap()
}

impl Atelier {
    pub(super) fn open_fabric_kit(&mut self) {
        let kit = &mut self.authoring.fabric_kit;
        kit.open = true;
        if kit.name.trim().is_empty() {
            kit.name = i18n::tr(self.prefs.language, "Measured fabric").into();
        }
        self.status = i18n::tr(
            self.prefs.language,
            "Type the workshop's readings, then Fit fabric",
        )
        .into();
    }

    /// Opens a measured fabric from the library in the window, measurements and fit.
    pub(super) fn open_fabric_file(&mut self, path: &Path) {
        match FabricFile::load(path) {
            Ok(file) => {
                let kit = &mut self.authoring.fabric_kit;
                kit.open = true;
                kit.name = file.name.clone();
                kit.measurements = file.measurements.clone();
                kit.fitted = Some(file.measurements.clone());
                kit.report = Some(FitReport {
                    properties: file.properties,
                    friction: file.friction,
                    checks: file.checks,
                    notes: file.notes,
                    solves: 0,
                });
                kit.error = None;
            }
            Err(e) => self.status = format!("{}: {e}", path.display()),
        }
    }

    /// Cuts the selected piece from the measured fabric in `path`.
    pub(super) fn apply_fabric_file(&mut self, path: &Path) {
        match FabricFile::load(path) {
            Ok(file) => self.apply_measured_fabric(file.name, file.properties),
            Err(e) => self.status = format!("{}: {e}", path.display()),
        }
    }

    fn apply_measured_fabric(
        &mut self,
        name: String,
        properties: fashioncraft::garment::template::FabricProperties,
    ) {
        let lang = self.prefs.language;
        let Some(panel) = self.authoring.selected.clone() else {
            self.status = i18n::tr(lang, "Select a piece to cut from this fabric").into();
            return;
        };
        self.authoring.fabric_draft = None;
        if self.edit(Command::Fabric {
            panel,
            name,
            properties,
            all_panels: false,
        }) {
            self.status =
                i18n::tr(lang, "Measured fabric applied · simulate to see it drape").into();
        }
    }

    pub(super) fn fabric_kit_window(&mut self, ctx: &egui::Context) {
        if !self.authoring.fabric_kit.open {
            return;
        }
        let lang = self.prefs.language;
        let t = |text| i18n::tr(lang, text);
        self.poll_fabric_fit(ctx);
        let mut open = true;
        let mut actions = vec![];
        let library = self.library_dir().join(LIBRARY_FOLDER);
        let selected = self.authoring.selected.is_some();
        egui::Window::new(t("Measure fabric"))
            .id(egui::Id::new("fabric_kit_window"))
            .open(&mut open)
            .default_width(460.)
            .default_height(640.)
            .resizable(true)
            .show(ctx, |ui| {
                let kit = &mut self.authoring.fabric_kit;
                let running = kit.job.is_some();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.small(t("Measure a sample of your fabric with simple tools, type the readings and FashionCraft fits a fabric whose virtual tests give the same readings."));
                    ui.hyperlink_to(t("Measuring procedure"), PROCEDURE_URL);
                    ui.add_enabled_ui(!running, |ui| {
                        super::workspace::prop_row(ui, t("Name"), |ui| ui.text_edit_singleline(&mut kit.name));
                        let m = &mut kit.measurements;
                        super::workspace::group_title(ui, t("Weight and thickness"));
                        number(ui, t("Area weight"), &mut m.gsm, 5.0..=3000., 1., " g/m²");
                        ui.small(t("Weigh a 10 × 10 cm square: its grams × 100 are its g/m²."));
                        number(ui, t("Thickness"), &mut m.thickness_mm, 0.01..=10., 0.01, " mm");

                        super::workspace::group_title(ui, t("Strip test with weights"));
                        number(ui, t("Strip width"), &mut m.strip_width_mm, 10.0..=200., 1., " mm");
                        number(ui, t("Length between clamps"), &mut m.strip_length_mm, 40.0..=1000., 1., " mm");
                        number(ui, t("Distance between marks"), &mut m.gauge_mm, 20.0..=800., 1., " mm");
                        number(ui, t("Ruler resolution"), &mut m.resolution_mm, 0.01..=5., 0.05, " mm");
                        ui.horizontal(|ui| {
                            for d in Direction::ALL {
                                let count = m.readings(d).len();
                                let label = format!("{} ({count})", direction_text(lang, d));
                                if ui.selectable_label(kit.tab == d, label).clicked() {
                                    kit.tab = d;
                                }
                            }
                        });
                        let d = kit.tab;
                        let gauge = m.gauge_mm;
                        ui.small(t("Load: the bottom clamp and everything hanging from it. Length: between the two marks, at the centre of the strip, after a minute."));
                        let mut remove = None;
                        egui::Grid::new(("fabric_kit_readings", d.label()))
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(t("Hanging load"));
                                ui.label(t("Length between marks"));
                                ui.end_row();
                                for (i, r) in m.readings_mut(d).iter_mut().enumerate() {
                                    ui.add(egui::DragValue::new(&mut r.load_g).range(1.0..=50_000.).speed(5.).max_decimals(1).suffix(" g"));
                                    ui.add(egui::DragValue::new(&mut r.length_mm).range(gauge..=gauge * 3.).speed(0.1).max_decimals(2).suffix(" mm"));
                                    if ui.small_button("×").on_hover_text(t("Remove reading")).clicked() {
                                        remove = Some(i);
                                    }
                                    ui.end_row();
                                }
                            });
                        if let Some(i) = remove {
                            m.readings_mut(d).remove(i);
                        }
                        if ui.small_button(t("Add reading")).clicked() {
                            let readings = m.readings_mut(d);
                            let (load, length) = readings
                                .last()
                                .map_or((100., gauge + 1.), |r| (r.load_g * 2., r.length_mm + 1.));
                            readings.push(Reading { load_g: load, length_mm: length });
                        }

                        super::workspace::group_title(ui, t("Cantilever at 41.5°"));
                        ui.small(t("Overhang when the tip touches the 41.5° line: the mean of four readings, both ends, face up and face down."));
                        optional(ui, t("Warp overhang"), &mut m.overhang_warp_mm, 60., 5.0..=1000., " mm");
                        optional(ui, t("Weft overhang"), &mut m.overhang_weft_mm, 50., 5.0..=1000., " mm");

                        super::workspace::group_title(ui, t("Friction (optional)"));
                        optional(ui, t("Slide angle"), &mut m.slide_angle_degrees, 20., 1.0..=56., "°");
                        if let Some(mu) = m.friction() {
                            ui.small(format!("μ = tan θ = {mu:.2}"));
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if running {
                            if ui.button(t("Cancel")).clicked() {
                                actions.push(Action::Cancel);
                            }
                            ui.spinner();
                        } else {
                            if ui.button(t("Fit fabric")).on_hover_text(t("Run the virtual tests and search the fabric that matches the readings")).clicked() {
                                actions.push(Action::Fit);
                            }
                            if ui.small_button(t("Example readings")).on_hover_text(t("Fill in readings of a light cotton poplin to try the window")).clicked() {
                                actions.push(Action::Example);
                            }
                            if ui.small_button(t("Clear")).clicked() {
                                actions.push(Action::Clear);
                            }
                        }
                    });
                    if let Some(job) = &kit.job {
                        let stage = job.stage.map_or(t("Starting…"), |s| stage_text(lang, s));
                        ui.small(format!("{stage} {:.0} s", job.started.elapsed().as_secs_f64()));
                    }
                    if let Some(error) = &kit.error {
                        ui.colored_label(ui.visuals().error_fg_color, i18n::tr_any(lang, error));
                    }
                    let stale = kit.fitted.as_ref() != Some(&kit.measurements);
                    if let Some(report) = &kit.report {
                        fabric_kit_results(ui, lang, report, stale);
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(!stale && !running, |ui| {
                                if ui.button(t("Save to library")).on_hover_text(library.display().to_string()).clicked() {
                                    actions.push(Action::Save);
                                }
                                if ui
                                    .add_enabled(selected, egui::Button::new(t("Apply to selected piece")))
                                    .on_disabled_hover_text(t("Select a piece first"))
                                    .clicked()
                                {
                                    actions.push(Action::Apply);
                                }
                                if let Some(mu) = report.friction
                                    && ui.small_button(t("Use friction in simulation")).clicked()
                                {
                                    actions.push(Action::UseFriction(mu));
                                }
                            });
                        });
                    }
                });
            });
        self.authoring.fabric_kit.open &= open;
        if !open && let Some(job) = &self.authoring.fabric_kit.job {
            job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        for action in actions {
            self.fabric_kit_action(action, &library);
        }
    }

    fn fabric_kit_action(&mut self, action: Action, library: &Path) {
        let lang = self.prefs.language;
        let kit = &mut self.authoring.fabric_kit;
        match action {
            Action::Fit => {
                if let Err(e) = kit.measurements.validate() {
                    kit.error = Some(e);
                    return;
                }
                kit.error = None;
                let (tx, rx) = mpsc::channel();
                let cancel = Arc::new(AtomicBool::new(false));
                let measurements = kit.measurements.clone();
                let stop = cancel.clone();
                let spawned = std::thread::Builder::new()
                    .name("fabric-kit-fit".into())
                    .spawn(move || {
                        let progress_tx = tx.clone();
                        let result = fabric_kit::fit(
                            &measurements,
                            FitOptions::default(),
                            &mut |stage| {
                                let _ = progress_tx.send(Message::Stage(stage));
                            },
                            &stop,
                        );
                        let _ = tx.send(Message::Done(result));
                    });
                match spawned {
                    Ok(_) => {
                        kit.job = Some(Job {
                            rx,
                            cancel,
                            stage: None,
                            started: std::time::Instant::now(),
                        });
                        kit.fitted = Some(kit.measurements.clone());
                        kit.report = None;
                    }
                    Err(e) => kit.error = Some(e.to_string()),
                }
            }
            Action::Cancel => {
                if let Some(job) = &kit.job {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Action::Example => {
                kit.measurements = example();
                kit.name = i18n::tr(lang, "Cotton poplin (example)").into();
            }
            Action::Clear => {
                kit.measurements = Measurements::default();
                kit.report = None;
                kit.fitted = None;
                kit.error = None;
            }
            Action::Save => {
                let Some(report) = &kit.report else {
                    return;
                };
                if kit.name.trim().is_empty() {
                    kit.error = Some("Name the fabric first".into());
                    return;
                }
                let file = FabricFile::new(&kit.name, &kit.measurements, report);
                let path = free_path(library, &kit.name);
                self.status = match file.save(&path) {
                    Ok(()) => format!(
                        "{} {}",
                        i18n::tr(lang, "Fabric saved to the library:"),
                        path.display()
                    ),
                    Err(e) => format!("{}: {e}", path.display()),
                };
                self.authoring.library.invalidate();
            }
            Action::Apply => {
                if let Some(report) = &kit.report {
                    let (name, properties) =
                        (kit.name.trim().to_owned(), report.properties.clone());
                    self.apply_measured_fabric(name, properties);
                }
            }
            Action::UseFriction(mu) => {
                self.settings.friction = mu.clamp(0., 1.5);
                self.status = format!(
                    "{} {:.2}",
                    i18n::tr(lang, "Simulation friction set to"),
                    self.settings.friction
                );
            }
        }
    }

    /// Takes the fit thread's messages.
    fn poll_fabric_fit(&mut self, ctx: &egui::Context) {
        let kit = &mut self.authoring.fabric_kit;
        let Some(job) = kit.job.as_mut() else {
            return;
        };
        let mut done = None;
        while let Ok(message) = job.rx.try_recv() {
            match message {
                Message::Stage(stage) => job.stage = Some(stage),
                Message::Done(result) => done = Some(result),
            }
        }
        match done {
            Some(Ok(report)) => {
                kit.report = Some(report);
                kit.job = None;
                self.status = i18n::tr(
                    self.prefs.language,
                    "Fabric fitted · compare the readings, then save or apply it",
                )
                .into();
            }
            Some(Err(e)) => {
                kit.fitted = None;
                kit.error = Some(e);
                kit.job = None;
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
        }
    }
}

/// The fitted fabric and how each virtual test compares with its reading.
fn fabric_kit_results(ui: &mut egui::Ui, lang: i18n::Language, report: &FitReport, stale: bool) {
    let t = |text| i18n::tr(lang, text);
    super::workspace::group_title(ui, t("Fitted fabric"));
    if stale {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            t("The readings changed: fit again before saving"),
        );
    }
    let p = &report.properties;
    egui::Grid::new("fabric_kit_fitted")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let mut row = |label: &str, value: String| {
                ui.label(egui::RichText::new(label).weak());
                ui.label(value);
                ui.end_row();
            };
            row(t("Area weight"), format!("{:.0} g/m²", p.density * 1000.));
            row(t("Thickness"), format!("{:.2} mm", p.thickness * 1000.));
            row(
                t("Warp stiffness"),
                format!(
                    "{:.0} N/m · {:.1} % {}",
                    p.warp,
                    fabrics::extension_percent(p.warp, p),
                    t("at 5 N / 5 cm")
                ),
            );
            row(
                t("Weft stiffness"),
                format!(
                    "{:.0} N/m · {:.1} % {}",
                    p.weft,
                    fabrics::extension_percent(p.weft, p),
                    t("at 5 N / 5 cm")
                ),
            );
            row(t("Shear"), format!("{:.1} N/m", p.shear));
            row(t("Bending warp"), format!("{:.2} µN·m", p.bend_warp * 1e6));
            row(t("Bending weft"), format!("{:.2} µN·m", p.bend_weft * 1e6));
            if let Some(mu) = report.friction {
                row(t("Friction"), format!("μ {mu:.2}"));
            }
        });
    if !report.checks.is_empty() {
        super::workspace::group_title(ui, t("Readings and virtual tests"));
        egui::Grid::new("fabric_kit_checks")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(t("Test")).weak());
                ui.label(egui::RichText::new(t("Measured")).weak());
                ui.label(egui::RichText::new(t("Simulated")).weak());
                ui.label("");
                ui.end_row();
                for check in &report.checks {
                    let label = match check.test {
                        Test::Strip { direction, load_g } => {
                            format!("{} · {load_g:.0} g", direction_text(lang, direction))
                        }
                        Test::Cantilever { direction } => {
                            format!(
                                "{} · {}",
                                t("Cantilever"),
                                direction_text(lang, direction).to_lowercase()
                            )
                        }
                    };
                    ui.label(label);
                    ui.label(format!("{:.1} mm", check.measured_mm));
                    ui.label(format!("{:.1} mm", check.simulated_mm));
                    // Under 0.05 mm reads 0.0, not −0.0.
                    let off = check.simulated_mm - check.measured_mm;
                    let off = if off.abs() < 0.05 { 0. } else { off };
                    ui.label(egui::RichText::new(format!("{off:+.1}")).weak());
                    ui.end_row();
                }
            });
    }
    for note in &report.notes {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("• {}", note_text(lang, note)),
        );
    }
    if report.solves > 0 {
        ui.small(format!("{} {}", report.solves, t("virtual tests solved")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_a_valid_set_of_readings() {
        let m = example();
        m.validate().unwrap();
        for d in Direction::ALL {
            assert!(!m.readings(d).is_empty(), "{d:?}");
        }
        assert!(m.friction().is_some());
    }

    #[test]
    fn every_note_reads_in_spanish() {
        let notes = [
            Note::StretchCopied {
                direction: Direction::Weft,
                from: Direction::Warp,
            },
            Note::ShearEstimated,
            Note::BendingCopied {
                direction: Direction::Warp,
                from: Direction::Weft,
            },
            Note::BendingDefault,
            Note::CoarseReading {
                direction: Direction::Bias,
                percent: 40.,
            },
            Note::CurveMismatch {
                direction: Direction::Warp,
                percent: 20.,
            },
            Note::AtLimit {
                parameter: Parameter::Shear,
            },
        ];
        for note in &notes {
            let es = note_text(i18n::Language::Spanish, note);
            assert_ne!(es, note.english());
            assert_eq!(note_text(i18n::Language::English, note), note.english());
        }
    }

    #[test]
    fn saving_never_overwrites_another_fabric() {
        let dir = tempfile::tempdir().unwrap();
        let first = free_path(dir.path(), "Poplin");
        assert_eq!(first, dir.path().join("Poplin.fcfabric"));
        std::fs::write(&first, "x").unwrap();
        assert_eq!(
            free_path(dir.path(), "Poplin"),
            dir.path().join("Poplin 2.fcfabric")
        );
    }
}
