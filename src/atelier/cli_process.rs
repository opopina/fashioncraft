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

/// The reasoning effort FashionCraft asks of a small request (`reasoning_effort` in the body),
/// for the CLIs that take one. Low, medium or high only: anything else is not passed on.
pub(super) fn effort(body: &Value) -> Option<&str> {
    body["reasoning_effort"]
        .as_str()
        .filter(|e| ["low", "medium", "high"].contains(e))
}

/// The conversation as a CLI's prompt carries it: the request without FashionCraft's hints to
/// the CLI itself, which go on its command line.
pub(super) fn history(body: &Value) -> Value {
    let mut history = body.clone();
    if let Some(fields) = history.as_object_mut() {
        fields.remove("reasoning_effort");
    }
    history
}

/// The conversation as plain text for a CLI that keeps nothing between requests and caches
/// nothing (Antigravity): the system message, each tool on one line with its arguments, then
/// who said what and each call with its result. The same as the JSON request in fewer words:
/// no schemas, ids or escaped arguments.
pub(super) fn transcript(history: &Value) -> String {
    let mut out = String::new();
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    };
    let messages = history["messages"].as_array().cloned().unwrap_or_default();
    for m in messages.iter().filter(|m| m["role"] == "system") {
        out.push_str("SYSTEM\n");
        out.push_str(&text_of(&m["content"]));
        out.push_str("\n\n");
    }
    if let Some(tools) = history["tools"].as_array() {
        out.push_str(
            "TOOLS (name(arguments): what it does; ? marks an optional argument; give the \
             arguments as one JSON object)\n",
        );
        for t in tools {
            let f = &t["function"];
            let required: Vec<&str> = f["parameters"]["required"]
                .as_array()
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let arguments: Vec<String> = f["parameters"]["properties"]
                .as_object()
                .map(|props| {
                    props
                        .iter()
                        .map(|(name, spec)| {
                            let optional = if required.contains(&name.as_str()) {
                                ""
                            } else {
                                "?"
                            };
                            let mut kind = type_of(spec);
                            if let Some(about) = spec["description"].as_str() {
                                kind.push_str(&format!(" ({about})"));
                            }
                            format!("{name}{optional}: {kind}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.push_str(&format!(
                "- {}({}): {}\n",
                f["name"].as_str().unwrap_or(""),
                arguments.join(", "),
                f["description"].as_str().unwrap_or("")
            ));
        }
        out.push('\n');
    }
    out.push_str("CONVERSATION\n");
    for m in messages.iter().filter(|m| m["role"] != "system") {
        let text = text_of(&m["content"]);
        match m["role"].as_str() {
            Some("user") => out.push_str(&format!("PERSON: {text}\n")),
            Some("tool") => out.push_str(&format!("RESULT: {text}\n")),
            _ => {
                if !text.trim().is_empty() {
                    out.push_str(&format!("ASSISTANT: {text}\n"));
                }
                for call in m["tool_calls"].as_array().into_iter().flatten() {
                    out.push_str(&format!(
                        "CALL {} {}\n",
                        call["function"]["name"].as_str().unwrap_or(""),
                        call["function"]["arguments"].as_str().unwrap_or("{}")
                    ));
                }
            }
        }
    }
    out
}

/// A JSON schema's type in a few words: string, number, a|b|c, array of number, object.
fn type_of(spec: &Value) -> String {
    if let Some(options) = spec["enum"].as_array() {
        return options
            .iter()
            .map(|o| o.as_str().map_or_else(|| o.to_string(), str::to_owned))
            .collect::<Vec<_>>()
            .join("|");
    }
    match spec["type"].as_str() {
        Some("array") => format!("array of {}", type_of(&spec["items"])),
        Some(kind) => kind.to_owned(),
        None => "value".to_owned(),
    }
}

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

/// Puts what a CLI reported about its usage into `answer`, in the form
/// `fashioncraft::assistant::usage_of` reads: OpenAI-style `usage` (prompt, completion and
/// cached tokens), `x_cost_usd` when the CLI states a cost, and `x_rate_limit` when it states
/// how much of the plan's limit is used. It reads every JSON value the CLI printed (one per
/// line, or one document) and takes only what is there:
/// - Claude Code's `result`: `usage` (input, output, cache read and cache write tokens) and
///   `total_cost_usd`;
/// - Codex's `turn.completed`: `usage` (input with cached, output), and `rate_limits` with
///   `used_percent` and `window_minutes` when an event carries them;
/// - Gemini CLI's `stats.models.*.tokens` (prompt, candidates, cached);
/// - any other `used_percent` (Antigravity's binary knows the field).
pub(super) fn attach_usage(answer: &mut Value, output: &mut File) {
    let mut text = String::new();
    if output.rewind().is_err() || output.read_to_string(&mut text).is_err() {
        return;
    }
    let mut values: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .collect();
    if values.is_empty()
        && let Ok(v) = serde_json::from_str::<Value>(text.trim())
    {
        values.push(v);
    }
    let (mut input, mut cached, mut output_tokens, mut found) = (0u64, 0u64, 0u64, false);
    let mut cost = None;
    let mut limit: Option<(f64, String)> = None;
    fn walk(v: &Value, f: &mut dyn FnMut(&serde_json::Map<String, Value>)) {
        match v {
            Value::Object(m) => {
                f(m);
                m.values().for_each(|x| walk(x, f));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, f)),
            _ => {}
        }
    }
    for value in &values {
        walk(value, &mut |m| {
            let n = |k: &str| m.get(k).and_then(Value::as_u64);
            if let (Some(i), Some(o)) = (n("input_tokens"), n("output_tokens")) {
                // Anthropic counts cache reads and writes apart from input; OpenAI's cached
                // tokens are part of it.
                let read = n("cache_read_input_tokens").unwrap_or(0);
                let write = n("cache_creation_input_tokens").unwrap_or(0);
                input = i + read + write;
                cached = read + n("cached_input_tokens").unwrap_or(0);
                output_tokens = o;
                found = true;
            }
            if let Some(Value::Object(models)) = m.get("models") {
                let (mut i, mut c, mut o) = (0, 0, 0);
                for model in models.values() {
                    let t = &model["tokens"];
                    i += t["prompt"].as_u64().unwrap_or(0);
                    c += t["cached"].as_u64().unwrap_or(0);
                    o += t["candidates"].as_u64().unwrap_or(0);
                }
                if i + o > 0 {
                    (input, cached, output_tokens, found) = (i, c, o, true);
                }
            }
            if let Some(c) = m.get("total_cost_usd").and_then(Value::as_f64) {
                cost = Some(c);
            }
            if let Some(used) = m.get("used_percent").and_then(Value::as_f64) {
                let window = m
                    .get("window_minutes")
                    .and_then(Value::as_f64)
                    .map(|w| {
                        if w >= 1440. {
                            format!(", {:.0}-day window", w / 1440.)
                        } else {
                            format!(", {:.0} h window", w / 60.)
                        }
                    })
                    .unwrap_or_default();
                if limit.as_ref().is_none_or(|(u, _)| used > *u) {
                    limit = Some((used, format!("plan limit{window}")));
                }
            }
        });
    }
    if found {
        answer["usage"] = json!({
            "prompt_tokens": input,
            "completion_tokens": output_tokens,
            "prompt_tokens_details": {"cached_tokens": cached},
        });
    }
    if let Some(c) = cost {
        answer["x_cost_usd"] = json!(c);
    }
    if let Some((used, what)) = limit {
        answer["x_rate_limit"] = json!({"used_percent": used, "what": what});
    }
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

/// Two turns through a CLI stand-in, the second must carry the first: a short first job (one
/// tool call) and a long one (more steps than are sent whole). The stand-in saves every prompt
/// it is given and asks for a colour change until `rounds` prompts have come, then replies.
/// `cli_output` prints (or writes) `$answer` in the CLI's own form; `transport` runs the
/// stand-in as the real CLI would be.
#[cfg(all(test, unix))]
pub(super) fn check_two_turns(
    provider: fashioncraft::assistant::Provider,
    effort_option: Option<&str>,
    cli_output: &str,
    transport: &mut dyn FnMut(PathBuf) -> Box<dyn fashioncraft::assistant::Transport>,
) {
    use fashioncraft::agent::ToolResult;
    use fashioncraft::assistant::{Conversation, Settings};
    use std::os::unix::fs::PermissionsExt;
    for rounds in [1, 45] {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("cli");
        let script = format!(
            r#"#!/bin/sh
d=$(dirname "$0")
n=$(ls "$d" | grep -c '^prompt-')
cat > "$d/prompt-$n.txt"
printf '%s\n' "$@" > "$d/args-$n.txt"
if [ "$n" -ge {rounds} ]; then
  answer='{{"text":"Done: the shirt is red.","tool":"","arguments":"{{}}"}}'
else
  answer='{{"text":"","tool":"set_color","arguments":"{{\"rgb\":[200,0,0]}}"}}'
fi
{cli_output}
"#
        );
        std::fs::write(&exe, script).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let settings = Settings {
            provider,
            model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            pause_every: 0,
        };
        let mut conversation = Conversation::new(provider);
        for text in ["Make the shirt red", "Now make it blue"] {
            conversation
                .turn(
                    &settings,
                    text,
                    transport(exe.clone()).as_mut(),
                    &mut |_, _| ToolResult::text("Coloured it red: rgb(200, 0, 0)"),
                    &mut |_| {},
                    &|| false,
                )
                .unwrap();
        }
        // A small request: low effort for the first steps, on the command line, never in
        // the prompt.
        let args =
            |n: usize| std::fs::read_to_string(dir.path().join(format!("args-{n}.txt"))).unwrap();
        if let Some(option) = effort_option {
            assert!(args(0).contains(option), "{provider:?}: {}", args(0));
            if rounds > fashioncraft::assistant::LIGHT_ROUNDS {
                assert!(!args(rounds).contains(option));
            }
        }
        assert!(
            !std::fs::read_to_string(dir.path().join("prompt-0.txt"))
                .unwrap()
                .contains("reasoning_effort")
        );
        // Turn 1 took `rounds` tool calls and a reply; the last prompt is turn 2's.
        let prompts = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().starts_with("prompt-"))
            })
            .count();
        assert_eq!(prompts, rounds + 2);
        let second =
            std::fs::read_to_string(dir.path().join(format!("prompt-{}.txt", rounds + 1))).unwrap();
        for carried in [
            "Make the shirt red",
            "Coloured it red",
            "Done: the shirt is red.",
            "Now make it blue",
        ] {
            assert!(
                second.contains(carried),
                "{provider:?}, {rounds} steps: turn 2 lacks {carried:?}"
            );
        }
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;

    #[test]
    fn a_transcript_says_the_same_in_fewer_words() {
        let body = json!({
            "model": "",
            "tools": [{"type": "function", "function": {"name": "draft_garment",
                "description": "Draft a garment.",
                "parameters": {"type": "object", "required": ["kind"], "properties": {
                    "kind": {"type": "string", "enum": ["shirt", "dress"]},
                    "from_avatar": {"type": "boolean", "description": "To the avatar's measurements"}}}}}],
            "messages": [
                {"role": "system", "content": "You are FashionCraft's assistant."},
                {"role": "user", "content": "make a dress"},
                {"role": "assistant", "content": "", "tool_calls": [{"id": "fc_1", "type": "function",
                    "function": {"name": "draft_garment", "arguments": "{\"kind\":\"dress\"}"}}]},
                {"role": "tool", "tool_call_id": "fc_1", "content": "Drafted a dress"},
                {"role": "assistant", "content": "Done."}
            ]
        });
        let text = transcript(&body);
        assert!(text.starts_with("SYSTEM\nYou are FashionCraft's assistant."));
        assert!(text.contains(
            "- draft_garment(from_avatar?: boolean (To the avatar's measurements), kind: shirt|dress): Draft a garment."
        ));
        assert!(text.contains(
            "PERSON: make a dress\nCALL draft_garment {\"kind\":\"dress\"}\nRESULT: Drafted a dress\nASSISTANT: Done.\n"
        ));
        assert!(text.len() < body.to_string().len());
    }

    #[test]
    fn what_each_cli_reports_of_its_usage_is_read() {
        use std::io::Write;
        let read = |lines: &[&str]| {
            let mut f = tempfile::tempfile().unwrap();
            for l in lines {
                writeln!(f, "{l}").unwrap();
            }
            let mut answer = json!({"choices": []});
            attach_usage(&mut answer, &mut f);
            answer
        };
        // Claude Code's result.
        let a = read(&[
            r#"{"type":"result","total_cost_usd":0.042,"usage":{"input_tokens":100,"cache_read_input_tokens":900,"output_tokens":50}}"#,
        ]);
        assert_eq!(a["usage"]["prompt_tokens"], 1000);
        assert_eq!(a["usage"]["prompt_tokens_details"]["cached_tokens"], 900);
        assert_eq!(a["x_cost_usd"], 0.042);
        // Codex's turn and its plan limit.
        let a = read(&[
            r#"{"type":"turn.completed","usage":{"input_tokens":1200,"cached_input_tokens":1000,"output_tokens":80}}"#,
            r#"{"type":"token_count","rate_limits":{"primary":{"used_percent":37.5,"window_minutes":300},"secondary":{"used_percent":12.0,"window_minutes":10080}}}"#,
        ]);
        assert_eq!(a["usage"]["prompt_tokens"], 1200);
        assert_eq!(a["x_rate_limit"]["used_percent"], 37.5);
        assert_eq!(a["x_rate_limit"]["what"], "plan limit, 5 h window");
        // Gemini CLI's stats.
        let a = read(&[
            r#"{"response":"…","stats":{"models":{"gemini-x":{"tokens":{"prompt":700,"candidates":30,"cached":200}}}}}"#,
        ]);
        assert_eq!(a["usage"]["completion_tokens"], 30);
        assert!(a.get("x_rate_limit").is_none());
    }

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
