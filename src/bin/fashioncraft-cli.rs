//! `fashioncraft-cli`: FashionCraft for scripts and AI agents.
//!
//! `fashioncraft-cli mcp` serves the Model Context Protocol over stdin and stdout, the way
//! desktop MCP clients start a local server (and the way ArtCraft's `craft-ai-mcp` gateway starts
//! every Craft app's `<app>-cli mcp`). Its tools work in the open FashionCraft window, while the
//! person allows it in Settings › AI agents; see `fashioncraft::agent`.
use std::process::ExitCode;

const HELP: &str = "\
fashioncraft-cli: FashionCraft for scripts and AI agents

Usage:
  fashioncraft-cli mcp        Serve MCP over stdin and stdout. The tools work in the open
                              FashionCraft window, once Settings › AI agents lets them.
  fashioncraft-cli tools      List the tools, as JSON.
  fashioncraft-cli --version

An MCP client entry, for example in Claude Desktop:
  {\"mcpServers\": {\"FashionCraft\": {\"command\": \"/path/to/fashioncraft-cli\", \"args\": [\"mcp\"]}}}
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("mcp") => {
            if args.iter().any(|a| a == "--headless") {
                // A session without a window is not there yet: every call says to open the app.
                eprintln!(
                    "fashioncraft-cli: --headless is not supported yet; tools need the open window"
                );
            }
            let mut link = fashioncraft::agent::WindowLink::new(fashioncraft::agent::session_dir());
            let stdin = std::io::stdin();
            fashioncraft::agent::serve_mcp(stdin.lock(), std::io::stdout().lock(), &mut link);
            ExitCode::SUCCESS
        }
        Some("tools") => {
            let tools: Vec<serde_json::Value> = fashioncraft::agent::tools()
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name, "description": t.description, "inputSchema": t.input_schema
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&tools).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("fashioncraft-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        None | Some("--help" | "-h" | "help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("fashioncraft-cli: unknown command '{other}'\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}
