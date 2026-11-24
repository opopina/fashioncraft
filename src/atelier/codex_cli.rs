//! Codex supplies decisions. The existing conversation loop validates and executes every
//! garment tool in the open window, retaining history and handling screenshots as usual.
use fashioncraft::assistant::Transport;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

pub(super) struct Cli<'a>(pub &'a AtomicBool);

fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("FASHIONCRAFT_CODEX") {
        return path.into();
    }
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(paths) = std::env::var_os("PATH")
        && let Some(path) = std::env::split_paths(&paths)
            .map(|p| p.join(name))
            .find(|p| p.is_file())
    {
        return path;
    }
    for path in [
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex",
        "/Applications/Codex.app/Contents/Resources/codex",
        "/opt/homebrew/bin/codex",
        "/usr/local/bin/codex",
    ] {
        if Path::new(path).is_file() {
            return path.into();
        }
    }
    name.into()
}

fn prompt(dir: &Path, body: &Value, command: &mut Command) -> io::Result<File> {
    let mut history = body.clone();
    let mut n = 0;
    for msg in history["messages"].as_array_mut().into_iter().flatten() {
        for part in msg["content"].as_array_mut().into_iter().flatten() {
            if part["type"] != "image_url" {
                continue;
            }
            let bytes = part["image_url"]["url"]
                .as_str()
                .and_then(|url| url.split_once(","))
                .and_then(|(_, data)| fashioncraft::agent::unbase64(data))
                .ok_or_else(|| io::Error::other("Invalid FashionCraft screenshot"))?;
            n += 1;
            let path = dir.join(format!("view-{n}.png"));
            fs::write(&path, bytes)?;
            command.arg("--image").arg(path);
            *part = json!({"type":"text","text":format!("Attached screenshot {n}")});
        }
    }
    let mut file = tempfile::tempfile()?;
    writeln!(
        file,
        "You are the FashionCraft assistant. Follow the supplied system message. Use the attached conversation and tool catalogue. Return either a final answer (tool empty) or one FashionCraft tool request (arguments is a JSON object encoded as a string). The application executes it and returns the result. Do not run commands or inspect files."
    )?;
    serde_json::to_writer(&mut file, &history)?;
    file.rewind()?;
    Ok(file)
}

impl Transport for Cli<'_> {
    fn post(&mut self, _: &str, _: &[(&str, String)], body: &Value) -> Result<Value, String> {
        self.run(&executable(), body).map_err(|e| e.to_string())
    }
}

impl Cli<'_> {
    fn run(&self, executable: &Path, body: &Value) -> io::Result<Value> {
        let dir = tempfile::tempdir()?;
        let schema = dir.path().join("response.schema.json");
        let answer = dir.path().join("answer.json");
        fs::write(&schema, super::cli_process::schema().to_string())?;
        let mut command = Command::new(executable);
        command
            .current_dir(dir.path())
            .args([
                "exec",
                "--ignore-user-config",
                "--ephemeral",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--disable",
                "shell_tool",
                "--disable",
                "plugins",
                "-c",
                "web_search=\"disabled\"",
                "-c",
                "forced_login_method=\"chatgpt\"",
                "--json",
                "--output-schema",
            ])
            .arg(schema)
            .arg("--output-last-message")
            .arg(&answer);
        if let Some(model) = body["model"].as_str().filter(|s| !s.is_empty()) {
            command.arg("--model").arg(model);
        }
        let input = prompt(dir.path(), body, &mut command)?;
        let mut errors = tempfile::tempfile()?;
        // Its JSON events go to a file: when it fails, that is where it says why.
        let mut events = tempfile::tempfile()?;
        let mut child = command.arg("-").stdin(input).stdout(events.try_clone()?)
            .stderr(errors.try_clone()?).spawn().map_err(|e| io::Error::other(format!("Could not start Codex CLI: {e}. Install Codex CLI or set FASHIONCRAFT_CODEX to its executable.")))?;
        if let Err(error) = super::cli_process::wait(&mut child, &mut errors, self.0, "Codex") {
            return Err(match super::cli_process::json_errors(&mut events) {
                Some(said) if !error.to_string().contains(&said) => {
                    io::Error::other(format!("{error} ({said})"))
                }
                _ => error,
            });
        }
        let reply: Value = serde_json::from_slice(&fs::read(answer)?)?;
        super::cli_process::response(&reply, body)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use fashioncraft::agent::ToolResult;
    use fashioncraft::assistant::{Conversation, Provider, Settings};
    use std::os::unix::fs::PermissionsExt;

    struct StandIn(PathBuf);
    impl Transport for StandIn {
        fn post(&mut self, _: &str, _: &[(&str, String)], body: &Value) -> Result<Value, String> {
            Cli(&AtomicBool::new(false))
                .run(&self.0, body)
                .map_err(|e| e.to_string())
        }
    }

    #[test]
    fn cli_gets_the_result_from_the_window_before_answering() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("codex");
        fs::write(
            &exe,
            r#"#!/bin/sh
while [ "$1" != "--output-last-message" ]; do shift || exit 2; done
out=$2
if grep -q 'verified scene result' ; then
  echo '{"text":"Two pieces","tool":"","arguments":"{}"}' > "$out"
else
  echo '{"text":"","tool":"scene_summary","arguments":"{}"}' > "$out"
fi
"#,
        )
        .unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        let settings = Settings {
            provider: Provider::CodexCli,
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
        };
        let mut conversation = Conversation::new(settings.provider);
        let mut calls = Vec::new();
        conversation
            .turn(
                &settings,
                "What pieces are open?",
                &mut StandIn(exe),
                &mut |name, _| {
                    calls.push(name.to_owned());
                    ToolResult::text("verified scene result")
                },
                &mut |_| {},
                &|| false,
            )
            .unwrap();
        assert_eq!(calls, ["scene_summary"]);
        assert_eq!(
            conversation.messages().last().unwrap()["content"],
            "Two pieces"
        );
    }
}
