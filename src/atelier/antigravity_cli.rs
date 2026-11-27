//! Antigravity CLI (`agy`) supplies decisions using Google Antigravity's native structured output.
//! The existing conversation loop validates and executes every garment tool in the open window,
//! retaining history and handling screenshots as usual.
use super::cli_process;
use fashioncraft::assistant::Transport;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Seek, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

pub(super) struct Cli<'a>(pub &'a AtomicBool);

fn executable() -> PathBuf {
    let names: &[&str] = if cfg!(windows) {
        &["agy.exe", "agy"]
    } else {
        &["agy"]
    };
    let home_relatives: &[&str] = if cfg!(windows) {
        &[
            ".local/bin/agy.exe",
            "AppData/Local/Programs/Antigravity/agy.exe",
        ]
    } else {
        &[".local/bin/agy", ".antigravity/bin/agy"]
    };
    cli_process::find_executable(
        "FASHIONCRAFT_ANTIGRAVITY",
        names,
        home_relatives,
        &["/opt/homebrew/bin/agy", "/usr/local/bin/agy"],
    )
}

fn input(dir: &Path, body: &Value) -> io::Result<File> {
    let mut history = super::cli_process::history(body);
    let mut screenshots = 0usize;
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
            screenshots += 1;
            let filename = format!("view-{screenshots}.png");
            fs::write(dir.join(&filename), bytes)?;
            *part = json!({
                "type": "text",
                "text": format!("Attached screenshot {screenshots} saved at {filename} in the current working directory.")
            });
        }
    }
    let screenshot_note = if screenshots > 0 {
        " If you need to inspect an attached screenshot, read only its view-N.png file from the current working directory, and do not include local file paths or file:// links in your answer."
    } else {
        " Do not inspect files."
    };
    // Antigravity caches nothing between requests: the conversation goes as plain text, which
    // says the same as the JSON request in fewer words.
    let prompt = format!(
        "You are the FashionCraft assistant. Follow the system message below. Use the conversation and tool list below. Return either a final answer (tool empty, arguments \"{{}}\") or one FashionCraft tool request (arguments is a JSON object encoded as a string). The application executes the tool and returns the result. Do not run shell commands, edit files, or search the web.{screenshot_note}\n\n{}",
        super::cli_process::transcript(&history)
    );
    let mut stdin = tempfile::tempfile()?;
    serde_json::to_writer(
        &mut stdin,
        &json!({"event": "user", "message": {"content": prompt}}),
    )?;
    writeln!(stdin)?;
    stdin.rewind()?;
    Ok(stdin)
}

fn result(output: &mut File) -> io::Result<Value> {
    output.rewind()?;
    let mut last_result: Option<Value> = None;
    for line in BufReader::new(output).lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if event["event"] == "result" && event["result"].is_object() {
            last_result = Some(event["result"].clone());
        } else if event.get("status").is_some()
            && (event.get("structured_output").is_some()
                || event.get("response").is_some()
                || event.get("error").is_some())
        {
            last_result = Some(event);
        }
    }
    let res = last_result.ok_or_else(|| io::Error::other("Antigravity CLI returned no result."))?;
    if res["status"].as_str() != Some("SUCCESS") {
        let detail = res["error"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| res["response"].as_str().filter(|s| !s.trim().is_empty()))
            .unwrap_or(
                "Antigravity CLI could not finish. Check your Antigravity login and usage limits.",
            );
        return Err(io::Error::other(detail.to_owned()));
    }
    if let Some(structured) = res.get("structured_output").filter(|v| v.is_object()) {
        return cli_process::normalize_reply(structured);
    }
    if let Some(text) = res["response"].as_str()
        && let Some(parsed) = cli_process::extract_json_object(text)
    {
        return cli_process::normalize_reply(&parsed);
    }
    Err(io::Error::other(
        "Antigravity CLI returned no structured answer.",
    ))
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
        fs::write(&schema, cli_process::schema().to_string())?;
        let mut output = tempfile::tempfile()?;
        let mut errors = tempfile::tempfile()?;
        let mut cmd = Command::new(executable);
        cmd.current_dir(dir.path())
            .args([
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--disable-slash-commands",
                "--sandbox",
                "--json-schema",
            ])
            .arg(&schema);
        if let Some(model) = body["model"].as_str().filter(|s| !s.is_empty()) {
            cmd.arg("--model").arg(model);
        }
        if let Some(effort) = cli_process::effort(body) {
            cmd.arg("--effort").arg(effort);
        }
        let mut child = cmd
            .stdin(input(dir.path(), body)?)
            .stdout(output.try_clone()?)
            .stderr(errors.try_clone()?)
            .spawn()
            .map_err(|e| {
                io::Error::other(format!(
                    "Could not start Antigravity CLI: {e}. Install agy or set FASHIONCRAFT_ANTIGRAVITY to its executable."
                ))
            })?;
        if let Err(error) = cli_process::wait(&mut child, &mut errors, self.0, "Antigravity CLI") {
            if child.try_wait()?.is_some()
                && !self.0.load(std::sync::atomic::Ordering::Relaxed)
                && output.metadata()?.len() > 0
                && let Err(detail) = result(&mut output)
            {
                return Err(io::Error::other(format!("{detail} ({error})")));
            }
            return Err(error);
        }
        let mut answer = cli_process::response(&result(&mut output)?, body)?;
        cli_process::attach_usage(&mut answer, &mut output);
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn screenshots_are_written_to_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({"messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,QUJD"}}]}]});
        let mut input_text = String::new();
        input(dir.path(), &body)
            .unwrap()
            .read_to_string(&mut input_text)
            .unwrap();
        let frame: Value = serde_json::from_str(&input_text).unwrap();
        assert_eq!(frame["event"], "user");
        let content = frame["message"]["content"].as_str().unwrap();
        assert!(!content.contains("QUJD"));
        assert!(content.contains("view-1.png"));
        assert_eq!(fs::read(dir.path().join("view-1.png")).unwrap(), b"ABC");
    }

    #[test]
    fn errors_from_result_event_reach_the_panel() {
        let mut file = tempfile::tempfile().unwrap();
        writeln!(
            file,
            "{}",
            json!({"event":"result","result":{"status":"ERROR","error":"Not signed in to Antigravity"}})
        )
        .unwrap();
        assert!(
            result(&mut file)
                .unwrap_err()
                .to_string()
                .contains("Not signed in to Antigravity")
        );
    }

    #[cfg(unix)]
    #[test]
    fn antigravity_requests_a_tool_and_reads_its_result() {
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
        let exe = dir.path().join("agy");
        fs::write(
            &exe,
            r#"#!/bin/sh
if grep -q 'verified garment scene'; then
  answer='{"text":"Skirt and waistband","tool":"","arguments":"{}"}'
else
  answer='{"text":"","tool":"scene_summary","arguments":"{}"}'
fi
echo '{"event":"init","conversation_id":"c1"}'
echo "{\"event\":\"result\",\"result\":{\"status\":\"SUCCESS\",\"structured_output\":$answer}}"
"#,
        )
        .unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        let settings = Settings {
            provider: Provider::AntigravityCli,
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            pause_every: 0,
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
                    ToolResult::text("verified garment scene")
                },
                &mut |_| {},
                &|| false,
            )
            .unwrap();
        assert_eq!(calls, ["scene_summary"]);
        assert_eq!(
            conversation.messages().last().unwrap()["content"],
            "Skirt and waistband"
        );
    }

    #[cfg(unix)]
    mod two_turns {
        use super::*;

        struct TwoTurns(PathBuf);
        impl Transport for TwoTurns {
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

        #[test]
        fn the_second_turn_remembers_the_first() {
            super::super::super::cli_process::check_two_turns(
                fashioncraft::assistant::Provider::AntigravityCli,
                Some("--effort\nlow\n"),
                r#"echo '{"event":"init","conversation_id":"c1"}'
    echo "{\"event\":\"result\",\"result\":{\"status\":\"SUCCESS\",\"structured_output\":$answer}}""#,
                &mut |exe| Box::new(TwoTurns(exe)),
            );
        }
    }
}
