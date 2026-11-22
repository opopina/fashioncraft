//! The agent tools, the window's connection file and the MCP server of `fashioncraft-cli mcp`.
use fashioncraft::agent::{
    AppLink, Connection, Content, ToolResult, handle_mcp, plain_name, same_token, serve_mcp, tools,
};
use serde_json::{Value, json};

/// Records the calls it gets and answers each with its name.
#[derive(Default)]
struct FakeWindow {
    calls: Vec<(String, Value)>,
}
impl AppLink for FakeWindow {
    fn call(&mut self, name: &str, arguments: &Value) -> ToolResult {
        self.calls.push((name.to_owned(), arguments.clone()));
        ToolResult::text(format!("ran {name}"))
    }
}

#[test]
fn every_tool_has_a_unique_name_and_an_object_schema() {
    let tools = tools();
    let mut names: Vec<_> = tools.iter().map(|t| t.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), tools.len());
    for t in &tools {
        assert!(!t.description.is_empty(), "{}", t.name);
        assert_eq!(t.input_schema["type"], "object", "{}", t.name);
        assert_eq!(t.input_schema["additionalProperties"], false, "{}", t.name);
        for required in t.input_schema["required"].as_array().into_iter().flatten() {
            let key = required.as_str().unwrap();
            assert!(
                t.input_schema["properties"].get(key).is_some(),
                "{}: {key}",
                t.name
            );
        }
    }
    // The fabric presets the tool offers are the app's own.
    let fabric = tools.iter().find(|t| t.name == "set_fabric").unwrap();
    let offered = fabric.input_schema["properties"]["preset"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(offered.len(), fashioncraft::fabrics::PRESETS.len());
}

#[test]
fn the_mcp_server_initializes_lists_tools_and_forwards_calls() {
    let mut window = FakeWindow::default();
    let init = handle_mcp(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-06-18", "capabilities": {}}}),
        &mut window,
    )
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "fashioncraft");
    // A notification gets no answer.
    assert!(
        handle_mcp(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &mut window
        )
        .is_none()
    );
    let list = handle_mcp(
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        &mut window,
    )
    .unwrap();
    let listed = list["result"]["tools"].as_array().unwrap();
    assert_eq!(listed.len(), tools().len());
    assert!(listed.iter().all(|t| t["inputSchema"]["type"] == "object"));
    let call = handle_mcp(
        &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": {"name": "simulate", "arguments": {"seconds": 3}}}),
        &mut window,
    )
    .unwrap();
    assert_eq!(call["result"]["content"][0]["text"], "ran simulate");
    assert_eq!(call["result"]["isError"], false);
    assert_eq!(
        window.calls,
        vec![("simulate".to_owned(), json!({"seconds": 3}))]
    );
    // An unknown tool or method is an error, and the window is not asked.
    let unknown = handle_mcp(
        &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "rm_rf"}}),
        &mut window,
    )
    .unwrap();
    assert_eq!(unknown["error"]["code"], -32602);
    let missing = handle_mcp(
        &json!({"jsonrpc": "2.0", "id": 5, "method": "nope"}),
        &mut window,
    )
    .unwrap();
    assert_eq!(missing["error"]["code"], -32601);
    assert_eq!(window.calls.len(), 1);
}

#[test]
fn the_stdio_loop_answers_one_line_per_request() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        "not json\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"screenshot"}}"#,
        "\n"
    );
    let mut output = Vec::new();
    serve_mcp(input.as_bytes(), &mut output, &mut FakeWindow::default());
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["result"], json!({}));
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert_eq!(lines[2]["result"]["content"][0]["text"], "ran screenshot");
}

#[test]
fn the_connection_file_round_trips_and_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let connection = Connection {
        port: 4242,
        token: fashioncraft::agent::new_token(),
        pid: 7,
        version: "0.1.0".into(),
    };
    assert_eq!(connection.token.len(), 32);
    connection.write(dir.path()).unwrap();
    assert_eq!(Connection::read(dir.path()), Some(connection));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join(fashioncraft::agent::CONNECTION_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
    Connection::remove(dir.path());
    assert_eq!(Connection::read(dir.path()), None);
}

#[test]
fn a_closed_window_answers_with_how_to_open_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut link = fashioncraft::agent::WindowLink::new(dir.path().to_path_buf());
    let result = link.call("scene_summary", &json!({}));
    assert!(result.is_error);
    assert!(result.joined_text().contains("AI agents"));
}

#[test]
fn names_and_tokens_are_checked() {
    assert_eq!(plain_name(" vest ").unwrap(), "vest");
    for bad in ["", "../x", "a/b", "a\\b", ".hidden", "c:x"] {
        assert!(plain_name(bad).is_err(), "{bad}");
    }
    assert!(same_token("abc", "abc"));
    assert!(!same_token("abc", "abd"));
    assert!(!same_token("abc", "abcd"));
    let image = ToolResult {
        content: vec![
            Content::Text { text: "a".into() },
            Content::Image {
                data: "AA==".into(),
                mime_type: "image/png".into(),
            },
        ],
        is_error: false,
    };
    let wire = serde_json::to_value(&image).unwrap();
    assert_eq!(wire["content"][1]["type"], "image");
    assert_eq!(wire["content"][1]["mimeType"], "image/png");
    assert_eq!(image.joined_text(), "a");
}

#[test]
fn base64_matches_the_standard_alphabet_and_padding() {
    use fashioncraft::agent::base64;
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    use fashioncraft::agent::unbase64;
    for bytes in [
        &b""[..],
        b"f",
        b"fo",
        b"foo",
        b"foobar",
        &[0xff, 0xfe, 0xfd, 0, 7],
    ] {
        assert_eq!(unbase64(&base64(bytes)).as_deref(), Some(bytes));
    }
    assert_eq!(unbase64("Zm9v").unwrap(), b"foo");
    assert_eq!(unbase64("Zg").unwrap(), b"f");
    assert!(unbase64("Z").is_none() && unbase64("Zm9*").is_none());
}
