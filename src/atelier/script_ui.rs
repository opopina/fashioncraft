//! Python script (File › Import › Python script): choose a .py file; the window shows it and
//! runs it with Python 3 on the open garment. What it prints shows below; the garment it saves
//! becomes one undoable edit.
use super::*;
use fashioncraft::{authoring::Command, scripting};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
};

/// Most of a script shown in the window, bytes.
const MAX_SHOWN_BYTES: u64 = 256 * 1024;

type Outcome = Result<scripting::ScriptRun, String>;

#[derive(Default)]
pub(super) struct ScriptUi {
    pub open: bool,
    path: Option<PathBuf>,
    text: String,
    output: String,
    running: Option<Receiver<Outcome>>,
}

impl Atelier {
    pub(super) fn open_python_script(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Python script")
            .add_filter("Python script", &["py"])
            .pick_file()
        {
            self.load_python_script(path);
        }
    }

    /// Show the script at `path`, ready to run.
    pub(super) fn load_python_script(&mut self, path: PathBuf) {
        use std::io::Read;
        let mut bytes = Vec::new();
        let read = std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_SHOWN_BYTES).read_to_end(&mut bytes));
        if let Err(e) = read {
            self.status = format!("{}: {e}", path.display());
            return;
        }
        let s = &mut self.authoring.script;
        s.open = true;
        s.text = String::from_utf8_lossy(&bytes).into_owned();
        s.path = Some(path);
        s.output.clear();
    }

    /// Start the script on the open garment, away from the window's thread.
    pub(super) fn run_python_script(&mut self) {
        let Some(path) = self.authoring.script.path.clone() else {
            return;
        };
        if self.authoring.script.running.is_some() {
            return;
        }
        let template = self.project.template.clone();
        let work = self.output.join("scripts");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(scripting::run(
                &path,
                &template,
                &work,
                scripting::TIME_LIMIT,
            ));
        });
        self.authoring.script.running = Some(rx);
        self.authoring.script.output = i18n::tr(self.prefs.language, "Running…").into();
    }

    /// Whether a script is running.
    pub(super) fn script_running(&self) -> bool {
        self.authoring.script.running.is_some()
    }

    /// Take a finished run: show what it printed and apply the garment it saved.
    fn finish_python_script(&mut self) {
        let ui_lang = self.prefs.language;
        let Some(rx) = &self.authoring.script.running else {
            return;
        };
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("The script stopped unexpectedly".into()),
        };
        self.authoring.script.running = None;
        let run = match outcome {
            Ok(run) => run,
            Err(e) => {
                self.authoring.script.output = e.clone();
                self.status = e;
                return;
            }
        };
        let mut output = run.output;
        match (run.code, run.garment) {
            (Some(0), Some(garment)) => {
                if self.edit(Command::RunScript {
                    template: Box::new(garment),
                }) {
                    self.status =
                        i18n::tr(ui_lang, "Script applied as one edit; Undo takes it back").into();
                } else {
                    output.push_str(&format!(
                        "\n{}: {}\n",
                        i18n::tr(ui_lang, "The saved garment was not applied"),
                        self.status
                    ));
                }
            }
            (Some(0), None) => {
                self.status = i18n::tr(ui_lang, "The script ran and saved no change").into();
            }
            _ => self.status = i18n::tr(ui_lang, "The script failed; see its output").into(),
        }
        self.authoring.script.output = output;
    }

    pub(super) fn script_window(&mut self, ctx: &egui::Context) {
        self.finish_python_script();
        if !self.authoring.script.open {
            return;
        }
        let ui_lang = self.prefs.language;
        let t = |text| i18n::tr(ui_lang, text);
        if self.script_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut open = true;
        let (mut run, mut choose) = (false, false);
        let running = self.script_running();
        egui::Window::new(t("Python script"))
            .open(&mut open)
            .default_width(520.)
            .show(ctx, |ui| {
                let s = &self.authoring.script;
                if let Some(path) = &s.path {
                    ui.strong(
                        path.file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    );
                }
                ui.small(t(
                    "The script works on the open garment through the fashioncraft module: fashioncraft.garment() reads it, fashioncraft.save(garment) hands it back as one undoable edit. It runs with Python 3 on this computer.",
                ));
                egui::ScrollArea::vertical()
                    .id_salt("script_text")
                    .max_height(260.)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&s.text).monospace().small())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                ui.horizontal(|ui| {
                    run = ui
                        .add_enabled(!running && s.path.is_some(), egui::Button::new(t("Run")))
                        .clicked();
                    choose = ui
                        .add_enabled(!running, egui::Button::new(t("Choose another…")))
                        .clicked();
                    if running {
                        ui.spinner();
                    }
                });
                if !s.output.is_empty() {
                    ui.separator();
                    ui.label(t("Output"));
                    egui::ScrollArea::vertical()
                        .id_salt("script_output")
                        .max_height(180.)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&s.output).monospace().small(),
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                        });
                }
            });
        if run {
            self.run_python_script();
        }
        if choose {
            self.open_python_script();
        }
        // A running script finishes on its own; closing only hides the window.
        self.authoring.script.open &= open;
    }
}
