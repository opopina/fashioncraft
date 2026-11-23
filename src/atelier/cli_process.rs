//! Shared output contract and cancellable child-process wait for assistant CLIs.
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    process::Child,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub(super) fn schema() -> Value {
    json!({"type":"object","properties":{
            "text":{"type":"string"},"tool":{"type":"string"},"arguments":{"type":"string"}
        },"required":["text","tool","arguments"],"additionalProperties":false})
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
