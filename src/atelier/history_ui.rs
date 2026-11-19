//! History window: the edits that can be undone, oldest first, then the ones undone. Picking
//! a step goes back, or forward, to just after it in one move: only the template reached is
//! rebuilt and checked, and a step that cannot be reached leaves everything as it was.
use super::*;
use fashioncraft::authoring::HISTORY_STEPS;

#[derive(Default)]
pub(super) struct HistoryUi {
    pub open: bool,
}

impl Atelier {
    pub(super) fn history_window(&mut self, ctx: &egui::Context) {
        let ui_lang = self.prefs.language;
        if !self.authoring.history_ui.open {
            return;
        }
        let done: Vec<String> = self
            .authoring
            .history
            .done()
            .map(|name| i18n::tr_any(ui_lang, name).to_owned())
            .collect();
        let undone: Vec<String> = self
            .authoring
            .history
            .undone()
            .map(|name| i18n::tr_any(ui_lang, name).to_owned())
            .collect();
        let mut open = true;
        let mut jump: Option<(bool, usize)> = None;
        egui::Window::new(i18n::tr(ui_lang, "History"))
            .open(&mut open)
            .default_width(240.)
            .show(ctx, |ui| {
                ui.small(format!(
                    "{HISTORY_STEPS} {}",
                    i18n::tr(ui_lang, "edits are kept · pick one to go back to it")
                ));
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let start =
                            ui.selectable_label(done.is_empty(), i18n::tr(ui_lang, "Start"));
                        if start.clicked() && !done.is_empty() {
                            jump = Some((false, done.len()));
                        }
                        for (k, name) in done.iter().enumerate() {
                            let current = k + 1 == done.len();
                            let row = ui.selectable_label(current, format!("{}. {name}", k + 1));
                            if row.clicked() && !current {
                                jump = Some((false, done.len() - 1 - k));
                            }
                        }
                        for (k, name) in undone.iter().enumerate() {
                            let text =
                                egui::RichText::new(format!("{}. {name}", done.len() + k + 1))
                                    .italics()
                                    .weak();
                            if ui
                                .selectable_label(false, text)
                                .on_hover_text(i18n::tr(
                                    ui_lang,
                                    "Undone · pick it to redo up to here",
                                ))
                                .clicked()
                            {
                                jump = Some((true, k + 1));
                            }
                        }
                    });
            });
        self.authoring.history_ui.open &= open;
        if let Some((redo, steps)) = jump {
            self.undo_steps(redo, steps);
        }
    }
}
