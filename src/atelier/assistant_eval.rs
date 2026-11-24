//! The Assistant's evaluation, run in this window: `FASHIONCRAFT_ASSISTANT_EVAL=<cases.json>`
//! sends each case's request to the model chosen in Settings › Assistant, exactly as typed in
//! the Assistant panel, then scores what it did with `fashioncraft::agent_eval` and writes
//! `report.md` and `report.json` to `FASHIONCRAFT_ASSISTANT_EVAL_OUT` (default:
//! `assistant-eval` in the session folder). `FASHIONCRAFT_ASSISTANT_EVAL_ONLY` runs the cases
//! whose ids contain one of its comma-separated words. The window closes when it is done.
//!
//! Every case drafts its starting garment on the avatar the window opened with, so a case that
//! changes the avatar does not change the next. It spends the model provider's usage: run it in
//! a copy of FashionCraft with its own HOME, never in a window with someone's work.
use super::*;
use fashioncraft::agent_eval::{self, Case, CaseResult, Transcript};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Default)]
pub(super) struct EvalState {
    checked: bool,
    run: Option<EvalRun>,
}

struct EvalRun {
    cases: Vec<Case>,
    next: usize,
    current: Option<Running>,
    results: Vec<CaseResult>,
    out: PathBuf,
    /// The avatar the window opened with, put back before every case.
    avatar: PathBuf,
}

struct Running {
    case: Case,
    before: Value,
    started: Instant,
}

impl Atelier {
    /// Once a frame: start the evaluation when asked, and move it on when a case's turn ends.
    pub(super) fn drive_assistant_eval(&mut self, ctx: &egui::Context) {
        if !self.assistant.eval.checked {
            self.assistant.eval.checked = true;
            if let Ok(path) = std::env::var("FASHIONCRAFT_ASSISTANT_EVAL") {
                match self.load_eval(&path) {
                    Ok(run) => self.assistant.eval.run = Some(run),
                    Err(e) => {
                        eprintln!("assistant evaluation: {e}");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        let Some(mut run) = self.assistant.eval.run.take() else {
            return;
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        match run.current.take() {
            None if run.next < run.cases.len() => {
                let case = run.cases[run.next].clone();
                run.next += 1;
                let before = self.eval_start(ctx, &case, &run.avatar);
                eprintln!("assistant evaluation: {} …", case.id);
                self.assistant_new_conversation();
                match before {
                    Ok(before) => {
                        self.assistant_ask(ctx, &case.prompt);
                        run.current = Some(Running {
                            case,
                            before,
                            started: Instant::now(),
                        });
                    }
                    Err(e) => {
                        let transcript = Transcript {
                            failure: Some(format!("The starting garment failed: {e}")),
                            ..Default::default()
                        };
                        run.results.push(CaseResult {
                            id: case.id.clone(),
                            prompt: case.prompt.clone(),
                            outcomes: agent_eval::evaluate(
                                &case,
                                &Value::Null,
                                &Value::Null,
                                &transcript,
                            ),
                            transcript,
                            seconds: 0.,
                        });
                    }
                }
            }
            None => {
                let written = write_report(
                    &run.out,
                    &run.results,
                    &self.prefs.ai.model,
                    self.assistant_provider_id(),
                );
                match written {
                    Ok(path) => eprintln!("assistant evaluation: report in {}", path.display()),
                    Err(e) => eprintln!("assistant evaluation: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            Some(running) if self.assistant_busy() || self.agent.running.is_some() => {
                run.current = Some(running);
            }
            Some(running) => {
                let after = self.scene_summary();
                let transcript = Transcript {
                    calls: self.assistant.calls.clone(),
                    reply: self.assistant.replies.join("\n"),
                    failure: self.assistant.failure.clone(),
                };
                let outcomes =
                    agent_eval::evaluate(&running.case, &running.before, &after, &transcript);
                let result = CaseResult {
                    id: running.case.id.clone(),
                    prompt: running.case.prompt.clone(),
                    outcomes,
                    transcript,
                    seconds: running.started.elapsed().as_secs_f64(),
                };
                eprintln!(
                    "assistant evaluation: {} {}",
                    result.id,
                    if result.passed() { "pass" } else { "FAIL" }
                );
                run.results.push(result);
                // A report after every case, so a stopped run keeps what it did.
                let _ = write_report(
                    &run.out,
                    &run.results,
                    &self.prefs.ai.model,
                    self.assistant_provider_id(),
                );
            }
        }
        self.assistant.eval.run = Some(run);
    }

    fn load_eval(&mut self, path: &str) -> Result<EvalRun, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let suite: agent_eval::Suite =
            serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        let only: Vec<String> = std::env::var("FASHIONCRAFT_ASSISTANT_EVAL_ONLY")
            .map(|v| {
                v.split(',')
                    .map(|w| w.trim().to_owned())
                    .filter(|w| !w.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let cases: Vec<Case> = suite
            .cases
            .into_iter()
            .filter(|c| only.is_empty() || only.iter().any(|w| c.id.contains(w.as_str())))
            .collect();
        if cases.is_empty() {
            return Err("no case to run".into());
        }
        let out = std::env::var_os("FASHIONCRAFT_ASSISTANT_EVAL_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| fashioncraft::agent::session_dir().join("assistant-eval"));
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        self.assistant.open = true;
        Ok(EvalRun {
            cases,
            next: 0,
            current: None,
            results: vec![],
            out,
            avatar: self.project.avatar_path.clone(),
        })
    }

    /// Put the starting avatar back and draft the case's garment; returns the summary before.
    fn eval_start(
        &mut self,
        ctx: &egui::Context,
        case: &Case,
        avatar: &std::path::Path,
    ) -> Result<Value, String> {
        if self.playing {
            self.invoke_command(commands::CommandId::ToggleSimulation, ctx);
        }
        if self.project.avatar_path != avatar {
            self.install_avatar_file(avatar)?;
        }
        if case.start != "current" {
            self.agent_draft(&case.start, true, &BTreeMap::new())?;
        }
        Ok(self.scene_summary())
    }

    fn assistant_provider_id(&self) -> &str {
        &self.prefs.ai.provider
    }
}

fn write_report(
    out: &std::path::Path,
    results: &[CaseResult],
    model: &str,
    provider: &str,
) -> Result<PathBuf, String> {
    let model = if model.is_empty() {
        provider.to_owned()
    } else {
        format!("{provider} / {model}")
    };
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(results).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let path = out.join("report.md");
    std::fs::write(&path, agent_eval::report(results, &model)).map_err(|e| e.to_string())?;
    Ok(path)
}
