//! Shared output contract and cancellable child-process wait for assistant CLIs.
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Child,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn schema() -> Value {
    json!({"type":"object","properties":{
            "text":{"type":"string"},"tool":{"type":"string"},"arguments":{"type":"string"}
        },"required":["text","tool","arguments"],"additionalProperties":false})
}

pub(super) fn find_executable(
    env_var: &str,
    names: &[&str],
    home_relatives: &[&str],
    fallbacks: &[&str],
) -> PathBuf {
    if let Some(path) = std::env::var_os(env_var)
        && !path.is_empty()
    {
        return path.into();
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        for rel in home_relatives {
            let candidate = home.join(rel);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    for path in fallbacks {
        if Path::new(path).is_file() {
            return path.into();
        }
    }
    names.first().copied().unwrap_or_default().into()
}

/// Extracts a JSON object from model text that may contain markdown code fences or surrounding prose.
pub(super) fn extract_json_object(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed)
        && value.is_object()
    {
        return Some(value);
    }
    let bytes = trimmed.as_bytes();
    let mut best: Option<Value> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(end) = find_object_end(bytes, i)
            && let Ok(value) = serde_json::from_str::<Value>(&trimmed[i..=end])
            && value.is_object()
        {
            if value.get("tool").is_some() || value.get("text").is_some() || best.is_none() {
                best = Some(value);
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    best
}

fn find_object_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

/// Normalizes a CLI JSON reply into the strict `{text, tool, arguments}` shape expected by [`response`].
pub(super) fn normalize_reply(reply: &Value) -> io::Result<Value> {
    let obj = reply
        .as_object()
        .ok_or_else(|| io::Error::other("CLI reply is not a JSON object"))?;
    let has_text = obj.get("text").is_some_and(|v| !v.is_null());
    let has_tool = obj.get("tool").is_some_and(|v| !v.is_null());
    if !has_text && !has_tool {
        return Err(io::Error::other(
            "CLI reply is missing both 'text' and 'tool' fields",
        ));
    }
    let text = match obj.get("text") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(_) => return Err(io::Error::other("Invalid CLI response text")),
    };
    let tool = match obj.get("tool") {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Null) | None => String::new(),
        Some(_) => return Err(io::Error::other("Invalid CLI tool field")),
    };
    let arguments = match obj.get("arguments") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(_)) => obj["arguments"].to_string(),
        Some(Value::Null) | None => "{}".to_owned(),
        Some(_) => return Err(io::Error::other("Invalid CLI tool arguments")),
    };
    Ok(json!({"text": text, "tool": tool, "arguments": arguments}))
}

pub(super) fn response(reply: &Value, body: &Value) -> io::Result<Value> {
    let text = reply["text"]
        .as_str()
        .ok_or_else(|| io::Error::other("Missing CLI response text"))?;
    let tool = reply["tool"]
        .as_str()
        .ok_or_else(|| io::Error::other("Missing CLI tool field"))?;
    let mut message = json!({"role":"assistant","content":text});
    if !tool.is_empty() {
        let index = body["messages"].as_array().map_or(0, Vec::len);
        message["tool_calls"] = json!([{"id":format!("fc_{index}"),"type":"function",
                "function":{"name":tool,"arguments":reply["arguments"]}}]);
    }
    Ok(json!({"choices":[{"message":message}]}))
}

pub(super) fn wait(
    child: &mut Child,
    errors: &mut File,
    stop: &AtomicBool,
    name: &str,
) -> io::Result<()> {
    let start = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(600) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other(if stop.load(Ordering::Relaxed) {
                "Stopped."
            } else {
                "The CLI timed out."
            }));
        }
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                let offset = errors.metadata()?.len().saturating_sub(6000);
                errors.seek(SeekFrom::Start(offset))?;
                let mut tail = Vec::new();
                errors.read_to_end(&mut tail)?;
                return Err(io::Error::other(failure(
                    name,
                    &status.to_string(),
                    &String::from_utf8_lossy(&tail),
                )));
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(80));
    }

    Ok(())
}

/// What a CLI that failed said, without its own chatter ("Reading prompt from stdin…", empty
/// lines): its last lines, at most about 600 characters. The login hint only when nothing
/// better came out.
pub(super) fn failure(name: &str, status: &str, stderr: &str) -> String {
    let noise = |line: &str| {
        let l = line.trim();
        l.is_empty()
            || l.starts_with("Reading prompt from stdin")
            || l.starts_with("Reading additional input from stdin")
            || l == "--------"
    };
    let lines: Vec<&str> = stderr.lines().filter(|l| !noise(l)).collect();
    let mut said = String::new();
    for line in lines.iter().rev().take(8).rev() {
        said.push_str(line.trim());
        said.push('\n');
    }
    let said = said.trim();
    let said: String = if said.chars().count() > 600 {
        let all: Vec<char> = said.chars().collect();
        format!("…{}", all[all.len() - 600..].iter().collect::<String>())
    } else {
        said.to_owned()
    };
    if said.is_empty() {
        format!("{name} exited with {status} and said nothing. Check the CLI login and connection.")
    } else {
        format!("{name} exited with {status}: {said}")
    }
}

/// The error messages in a CLI's JSON event stream (one JSON object per line), such as
/// Codex's `{"type":"error","message":…}` or `{"type":"turn.failed","error":{…}}`.
pub(super) fn json_errors(output: &mut File) -> Option<String> {
    output.rewind().ok()?;
    let mut text = String::new();
    output.read_to_string(&mut text).ok()?;
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let kind = event["type"].as_str().unwrap_or_default();
        if !(kind.contains("error") || kind.contains("failed")) {
            continue;
        }
        let message = event["message"]
            .as_str()
            .or_else(|| event["error"]["message"].as_str())
            .or_else(|| event["error"].as_str());
        if let Some(m) = message
            && !found.iter().any(|f| f == m)
        {
            found.push(m.to_owned());
        }
    }
    (!found.is_empty()).then(|| found.join(" · "))
}

#[cfg(test)]
mod failure_tests {
    use super::*;

    #[test]
    fn a_failed_cli_says_what_it_said_not_its_chatter() {
        let said = failure(
            "Codex",
            "exit status: 1",
            "Reading prompt from stdin...\n\nERROR: stream disconnected: request too large\n",
        );
        assert_eq!(
            said,
            "Codex exited with exit status: 1: ERROR: stream disconnected: request too large"
        );
        assert!(
            failure("Codex", "exit status: 1", "Reading prompt from stdin...\n").contains("login")
        );
        let mut out = tempfile::tempfile().unwrap();
        use std::io::Write;
        writeln!(out, r#"{{"type":"thread.started"}}"#).unwrap();
        writeln!(
            out,
            r#"{{"type":"error","message":"context window exceeded"}}"#
        )
        .unwrap();
        writeln!(
            out,
            r#"{{"type":"turn.failed","error":{{"message":"context window exceeded"}}}}"#
        )
        .unwrap();
        assert_eq!(
            json_errors(&mut out).as_deref(),
            Some("context window exceeded")
        );
    }
}
