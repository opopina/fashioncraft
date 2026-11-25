//! Run one Claude Code decision with the open conversation and its actual screenshot blocks.
use super::cli_process;
use fashioncraft::assistant::{SYSTEM_PROMPT, Transport};
use serde_json::{Value, json};
#[cfg(any(target_os = "macos", test))]
use std::fs;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

pub(super) struct Cli<'a>(pub &'a AtomicBool);

#[cfg(target_os = "macos")]
fn desktop_cli(home: &Path) -> Option<PathBuf> {
    let root = home.join("Library/Application Support/Claude/claude-code");
    let mut versions: Vec<_> = fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let version = name
                .to_str()?
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            Some((version, entry.path()))
        })
        .collect();
    versions.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (_, version) in versions {
        if let Ok(builds) = fs::read_dir(version)
            && let Some(path) = builds
                .flatten()
                .map(|b| b.path().join("claude.app/Contents/MacOS/claude"))
                .find(|p| p.is_file())
        {
            return Some(path);
        }
    }
    None
}

fn executable() -> PathBuf {
    if let Some(path) = std::env::var_os("FASHIONCRAFT_CLAUDE") {
        return path.into();
    }
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        if let Some(cli) = desktop_cli(&home) {
            return cli;
        }
        let local = home.join(".local/bin").join(name);
        if local.is_file() {
            return local;
        }
    }
    for path in ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"] {
        if Path::new(path).is_file() {
            return path.into();
        }
    }
    name.into()
}

fn input(body: &Value) -> io::Result<File> {
    let mut history = body.clone();
    let mut images = Vec::new();
    for msg in history["messages"].as_array_mut().into_iter().flatten() {
        for part in msg["content"].as_array_mut().into_iter().flatten() {
            if part["type"] == "image_url" {
                let data = part["image_url"]["url"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("data:image/png;base64,"))
                    .ok_or_else(|| io::Error::other("Invalid FashionCraft screenshot"))?;
                images.push(json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":data}}));
                *part =
                    json!({"type":"text","text":format!("Attached screenshot {}",images.len())});
            }
        }
    }
    let mut content = vec![
        json!({"type":"text","text":format!("Use this conversation and tool catalogue. Return a final answer with an empty tool, or request one FashionCraft tool with arguments encoded as a JSON object string. FashionCraft executes the tool and returns its result.\n{history}")}),
    ];
    content.extend(images);
    let mut stdin = tempfile::tempfile()?;
    serde_json::to_writer(
        &mut stdin,
        &json!({"type":"user","message":{"role":"user","content":content}}),
    )?;
    writeln!(stdin)?;
    stdin.rewind()?;
    Ok(stdin)
}

fn result(output: &mut File) -> io::Result<Value> {
    output.rewind()?;
    for line in BufReader::new(output).lines() {
        let event: Value = serde_json::from_str(&line?)?;
        if event["type"] != "result" {
            continue;
        }
        if event["is_error"] == true || event["subtype"] != "success" {
            return Err(io::Error::other(event["result"].as_str().unwrap_or("Claude Code could not finish. Check your Claude Code login and usage limits.").to_owned()));
        }
        return event
            .get("structured_output")
            .filter(|v| v.is_object())
            .cloned()
            .ok_or_else(|| {
                io::Error::other(
                    event["result"]
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("Claude Code returned no structured answer.")
                        .to_owned(),
                )
            });
    }
    Err(io::Error::other("Claude Code returned no result."))
}

impl Transport for Cli<'_> {
    fn post(&mut self, _: &str, _: &[(&str, String)], body: &Value) -> Result<Value, String> {
        self.run(&executable(), body).map_err(|e| e.to_string())
    }
}

impl Cli<'_> {
    fn run(&self, executable: &Path, body: &Value) -> io::Result<Value> {
        let dir = tempfile::tempdir()?;
        let mut output = tempfile::tempfile()?;
        let mut errors = tempfile::tempfile()?;
        let mut cmd = Command::new(executable);
        cmd.current_dir(dir.path())
            .args([
                "-p",
                "--no-session-persistence",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--tools",
                "",
                "--strict-mcp-config",
                "--disable-slash-commands",
                "--setting-sources",
                "",
                "--permission-mode",
                "dontAsk",
                "--system-prompt",
                SYSTEM_PROMPT,
                "--json-schema",
            ])
            .arg(cli_process::schema().to_string());
        if let Some(model) = body["model"].as_str().filter(|s| !s.is_empty()) {
            cmd.arg("--model").arg(model);
        }
        let mut child = cmd.stdin(input(body)?).stdout(output.try_clone()?).stderr(errors.try_clone()?).spawn()
            .map_err(|e| io::Error::other(format!("Could not start Claude Code: {e}. Set FASHIONCRAFT_CLAUDE to the Claude Code executable.")))?;
        if let Err(error) = cli_process::wait(&mut child, &mut errors, self.0, "Claude Code") {
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
    fn screenshots_travel_as_claude_image_blocks() {
        let body = json!({"messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,QUJD"}}]}]});
        let mut input_text = String::new();
        input(&body)
            .unwrap()
            .read_to_string(&mut input_text)
            .unwrap();
        let frame: Value = serde_json::from_str(&input_text).unwrap();
        let blocks = frame["message"]["content"].as_array().unwrap();
        assert!(!blocks[0]["text"].as_str().unwrap().contains("QUJD"));
        assert_eq!(blocks[1]["source"]["data"], "QUJD");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn login_errors_reach_the_panel() {
        let mut file = tempfile::tempfile().unwrap();
        writeln!(
            file,
            "{}",
            json!({"type":"result","subtype":"success","is_error":true,"result":"Not logged in"})
        )
        .unwrap();
        assert!(
            result(&mut file)
                .unwrap_err()
                .to_string()
                .contains("Not logged in")
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_requests_a_tool_and_reads_its_result() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("claude");
        fs::write(&exe,r#"#!/bin/sh
if grep -q 'actual shirt panels'; then
  answer='{"text":"Front and back","tool":"","arguments":"{}"}'
else
  answer='{"text":"","tool":"scene_summary","arguments":"{}"}'
fi
echo '{"type":"system","subtype":"init"}'
echo "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"structured_output\":$answer}"
"#).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        let stop = AtomicBool::new(false);
        let cli = Cli(&stop);
        let mut body = json!({"messages":[{"role":"user","content":"Which pieces?"}]});
        let first = cli.run(&exe, &body).unwrap();
        assert_eq!(
            first["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "scene_summary"
        );
        body["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"tool","content":"actual shirt panels"}));
        let next = cli.run(&exe, &body).unwrap();
        assert_eq!(next["choices"][0]["message"]["content"], "Front and back");
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
                fashioncraft::assistant::Provider::ClaudeCli,
                r#"echo '{"type":"system","subtype":"init"}'
    echo "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"structured_output\":$answer}""#,
                &mut |exe| Box::new(TwoTurns(exe)),
            );
        }
    }
}
