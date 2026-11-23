//! Gemini CLI (`gemini`) supplies decisions in headless read-only mode. The existing conversation
//! loop validates and executes every garment tool in the open window, retaining history and
//! handling screenshots as usual.
use super::cli_process;
use fashioncraft::assistant::Transport;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

pub(super) struct Cli<'a>(pub &'a AtomicBool);

fn executable() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["gemini.cmd", "gemini.exe", "gemini"]
    } else {
        &["gemini"]
    };
    let home_relatives: &[&str] = if cfg!(windows) {
        &[".local/bin/gemini.exe", "AppData/Roaming/npm/gemini.cmd"]
    } else {
        &[".local/bin/gemini", ".npm-global/bin/gemini"]
    };
    cli_process::find_executable(
        "FASHIONCRAFT_GEMINI",
        names,
        home_relatives,
        &["/opt/homebrew/bin/gemini", "/usr/local/bin/gemini"],
    )
}

fn input(dir: &Path, body: &Value) -> io::Result<(File, Vec<String>)> {
    let mut history = body.clone();
    let mut image_refs = Vec::new();
    for msg in history["messages"].as_array_mut().into_iter().flatten() {
        for part in msg["content"].as_array_mut().into_iter().flatten() {
            if part["type"] != "image_url" {
                continue;
            }
            let bytes = part["image_url"]["url"]
                .as_str()
                .and_then(|url| url.split_once(','))
                .and_then(|(_, data)| fashioncraft::agent::unbase64(data))
                .ok_or_else(|| io::Error::other("Invalid FashionCraft screenshot"))?;
            let index = image_refs.len() + 1;
            let filename = format!("view-{index}.png");
            fs::write(dir.join(&filename), bytes)?;
            image_refs.push(format!("@{filename}"));
            *part = json!({
                "type": "text",
                "text": format!("Attached screenshot {index} (@{filename})")
            });
        }
    }
    let mut stdin = tempfile::tempfile()?;
    writeln!(
        stdin,
        "You are the FashionCraft assistant. Follow the supplied system message. Use the attached conversation and tool catalogue. Return ONLY one raw JSON object with no markdown fences and no surrounding commentary, matching this exact schema: {{\"text\": string, \"tool\": string, \"arguments\": string}}. For a final answer set \"tool\" to \"\" and \"arguments\" to \"{{}}\". To call one FashionCraft tool set \"tool\" to the tool name and \"arguments\" to a JSON object encoded as a string. The application executes the tool and returns its result. Do not run commands or inspect files outside any attached screenshots."
    )?;
    serde_json::to_writer(&mut stdin, &history)?;
    writeln!(stdin)?;
    stdin.rewind()?;
    Ok((stdin, image_refs))
}

fn result(output: &mut File) -> io::Result<Value> {
    output.rewind()?;
    let mut raw = String::new();
    output.read_to_string(&mut raw)?;
    let envelope = cli_process::extract_json_object(&raw)
        .ok_or_else(|| io::Error::other("Gemini CLI returned no JSON response."))?;
    if let Some(error) = envelope.get("error").filter(|v| !v.is_null()) {
        let message = error["message"]
            .as_str()
            .or_else(|| error.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string());
        return Err(io::Error::other(message));
    }
    if let Some(response_text) = envelope["response"].as_str() {
        if let Some(parsed) = cli_process::extract_json_object(response_text) {
            return cli_process::normalize_reply(&parsed);
        }
        let trimmed = response_text.trim();
        if !trimmed.is_empty() && !trimmed.contains('{') {
            return Ok(json!({"text": trimmed, "tool": "", "arguments": "{}"}));
        }
        return Err(io::Error::other(
            "Gemini CLI returned an unreadable reply instead of the expected JSON object.",
        ));
    }
    if envelope.get("text").is_some() || envelope.get("tool").is_some() {
        return cli_process::normalize_reply(&envelope);
    }
    Err(io::Error::other("Gemini CLI returned no answer."))
}

impl Transport for Cli<'_> {
    fn post(&mut self, _: &str, _: &[(&str, String)], body: &Value) -> Result<Value, String> {
        self.run(&executable(), body).map_err(|e| e.to_string())
    }
}

impl Cli<'_> {
    fn run(&self, executable: &Path, body: &Value) -> io::Result<Value> {
        let dir = tempfile::tempdir()?;
        let (stdin_file, image_refs) = input(dir.path(), body)?;
        let prompt = if image_refs.is_empty() {
            "Follow the FashionCraft instructions and conversation from stdin. Reply with only the required JSON object and do not run tools.".to_owned()
        } else {
            format!(
                "Follow the FashionCraft instructions and conversation from stdin. Attached screenshots: {}. Reply with only the required JSON object and do not run tools.",
                image_refs.join(" ")
            )
        };
        let mut output = tempfile::tempfile()?;
        let mut errors = tempfile::tempfile()?;
        let mut cmd = Command::new(executable);
        cmd.current_dir(dir.path()).args([
            "--skip-trust",
            "--approval-mode",
            "plan",
            "--output-format",
            "json",
            "-p",
            &prompt,
        ]);
        if let Some(model) = body["model"].as_str().filter(|s| !s.is_empty()) {
            cmd.arg("--model").arg(model);
        }
        let mut child = cmd
            .stdin(stdin_file)
            .stdout(output.try_clone()?)
            .stderr(errors.try_clone()?)
            .spawn()
            .map_err(|e| {
                io::Error::other(format!(
                    "Could not start Gemini CLI: {e}. Install Gemini CLI or set FASHIONCRAFT_GEMINI to its executable."
                ))
            })?;
        if let Err(error) = cli_process::wait(&mut child, &mut errors, self.0, "Gemini CLI") {
            if child.try_wait()?.is_some()
                && !self.0.load(std::sync::atomic::Ordering::Relaxed)
                && output.metadata()?.len() > 0
                && let Err(detail) = result(&mut output)
            {
                return Err(io::Error::other(format!("{detail} ({error})")));
            }
            return Err(error);
        }
        cli_process::response(&result(&mut output)?, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshots_are_written_and_referenced_with_at_syntax() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({"messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,QUJD"}}]}]});
        let (mut stdin_file, refs) = input(dir.path(), &body).unwrap();
        let mut input_text = String::new();
        stdin_file.read_to_string(&mut input_text).unwrap();
        assert_eq!(refs, vec!["@view-1.png"]);
        assert!(!input_text.contains("QUJD"));
        assert!(input_text.contains("@view-1.png"));
        assert_eq!(fs::read(dir.path().join("view-1.png")).unwrap(), b"ABC");
    }

    #[test]
    fn fenced_json_and_object_arguments_are_normalized() {
        let mut file = tempfile::tempfile().unwrap();
        writeln!(
            file,
            "{}",
            json!({"response": "```json\n{\"text\":\"\",\"tool\":\"set_fabric\",\"arguments\":{\"fabric\":\"denim\"}}\n```"})
        )
        .unwrap();
        let parsed = result(&mut file).unwrap();
        assert_eq!(parsed["tool"], "set_fabric");
        assert_eq!(parsed["arguments"], "{\"fabric\":\"denim\"}");
    }

    #[test]
    fn envelope_errors_reach_the_panel() {
        let mut file = tempfile::tempfile().unwrap();
        writeln!(
            file,
            "{}",
            json!({"error": {"type": "AuthError", "message": "Please sign in to Gemini CLI"}})
        )
        .unwrap();
        assert!(
            result(&mut file)
                .unwrap_err()
                .to_string()
                .contains("Please sign in to Gemini CLI")
        );
    }

    #[cfg(unix)]
    #[test]
    fn gemini_requests_a_tool_and_reads_its_result() {
        use fashioncraft::agent::ToolResult;
        use fashioncraft::assistant::{Conversation, Provider, Settings};
        use std::os::unix::fs::PermissionsExt;

        struct StandIn(PathBuf);
        impl Transport for StandIn {
            fn post(
                &mut self,
                _: &str,
                _: &[(&str, String)],
                body: &Value,
            ) -> Result<Value, String> {
                Cli(&AtomicBool::new(false))
                    .run(&self.0, body)
                    .map_err(|e| e.to_string())
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("gemini");
        fs::write(
            &exe,
            r#"#!/bin/sh
if grep -q 'verified trouser panels'; then
  printf '%s\n' '{"response":"{\"text\":\"Front and back legs\",\"tool\":\"\",\"arguments\":\"{}\"}"}'
else
  printf '%s\n' 'Warning: Basic terminal detected'
  printf '%s\n' '{"response":"```json\n{\"text\":\"\",\"tool\":\"scene_summary\",\"arguments\":{}}\n```"}'
fi
"#,
        )
        .unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        let settings = Settings {
            provider: Provider::GeminiCli,
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
                    ToolResult::text("verified trouser panels")
                },
                &mut |_| {},
                &|| false,
            )
            .unwrap();
        assert_eq!(calls, ["scene_summary"]);
        assert_eq!(
            conversation.messages().last().unwrap()["content"],
            "Front and back legs"
        );
    }
}
