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
                let offset = errors.metadata()?.len().saturating_sub(1500);
                errors.seek(SeekFrom::Start(offset))?;
                let mut tail = Vec::new();
                errors.read_to_end(&mut tail)?;
                return Err(io::Error::other(format!(
                    "{name} exited with {status}. Check the CLI login and connection. {}",
                    String::from_utf8_lossy(&tail)
                )));
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(80));
    }

    Ok(())
}
