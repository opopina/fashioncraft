# AI agents and the Assistant

AI can work in FashionCraft in two ways:

- **The Assistant** (Window › Assistant): a chat inside FashionCraft. You pick the model: Claude,
  ChatGPT, Gemini or a model running on your own computer.
- **MCP**: AI apps that speak the Model Context Protocol work in FashionCraft from outside. These
  include Claude Desktop, Claude Code, Cursor, LM Studio and ArtCraft's AI gateway.

Both use the same ten tools, listed [below](#the-tools). The tools work in the open window, where
you see every change as it happens, and Undo takes any of them back. They save and export only
into your `Documents/FashionCraft` folder, and they never open file dialogs.

## The Assistant

Open **Window › Assistant** and press **Connection** to set it up:

| Provider | Model | Address | Key |
|---|---|---|---|
| Claude (Anthropic) | `claude-opus-5-5` unless you type another | `https://api.anthropic.com/v1` | An API key from the Claude Console, or `ANTHROPIC_API_KEY` |
| ChatGPT (OpenAI) | The model name, as OpenAI lists it | `https://api.openai.com/v1` | An OpenAI API key, or `OPENAI_API_KEY` |
| ChatGPT (Codex CLI) | Leave blank for the default from Codex, or enter a model | Detected on this computer | Sign in with ChatGPT using `codex login` |
| Gemini (Google) | The model name, as Google lists it | Google's OpenAI-compatible address | A Gemini API key, or `GEMINI_API_KEY` |
| Local model | The name your server shows, for example `ollama list` | `http://localhost:11434/v1` (Ollama); LM Studio uses `http://localhost:1234/v1` | None |

Leave the address empty to use the one shown. Any service with OpenAI's chat-completions API works
as "Local model" or "ChatGPT" with its own address.

**Keys.** A key you paste is saved in the macOS keychain or the Windows credential manager. On
Linux it goes to a file in FashionCraft's session folder that only your user can read. Keys are
never written to the preferences file. **Forget it** removes a saved key. When no key is saved, the
provider's environment variable is used.

**Privacy and cost.** Your messages, the scene summary and the screenshots the model asks for go to
the provider you pick. API-key connections use the provider’s API billing. A local model keeps everything
on your computer.

**ChatGPT through Codex CLI.** Choose **ChatGPT (Codex CLI)** under Connection. The panel starts
Codex in the background and sends the chat, available garment tools and any requested screenshots
to OpenAI using the CLI's ChatGPT login. Each tool result returns to the same visible chat. This
mode works with external MCP access turned off. It uses a recent Codex CLI with structured output
and ephemeral execution. On macOS it also detects the CLI bundled with ChatGPT. To select another
installation, set `FASHIONCRAFT_CODEX` to the executable path. Stop cancels the current model
request; garment edits already applied can be undone in the editor.

**Using it.** Type a request and press Enter; Shift+Enter starts a new line. You see each tool the
model runs, with its result, and a thumbnail of each screenshot it takes. **Stop** ends the turn
after the current step. **New chat** starts a fresh conversation. Some requests to try:

- "Draft a denim skirt for this avatar, simulate it and show me how it falls."
- "Make the t-shirt 5 cm longer with short sleeves, in red jersey."
- "Save this as summer-tee and export it as GLB."

With Claude, FashionCraft opts into Anthropic's server-side fallback (`fallbacks: "default"`). If
the model declines a request, Anthropic retries it on its recommended fallback model, and the chat
says so.

## MCP: Claude Desktop, Claude Code, Cursor, ArtCraft

1. In FashionCraft, open **Settings** and turn on **AI agents › Let AI agents use FashionCraft**.
   While it is off, nothing listens.
2. Copy the **MCP client entry** shown under the switch into your client's MCP settings. For
   Claude Desktop, that is `claude_desktop_config.json`. It looks like this:

   ```json
   {"mcpServers": {"FashionCraft": {"command": "/Applications/FashionCraft.app/Contents/MacOS/fashioncraft-cli", "args": ["mcp"]}}}
   ```

   For Claude Code:

   ```sh
   claude mcp add fashioncraft -- /Applications/FashionCraft.app/Contents/MacOS/fashioncraft-cli mcp
   ```

Where `fashioncraft-cli` is:

| Installed from | Path |
|---|---|
| macOS `.dmg` | `/Applications/FashionCraft.app/Contents/MacOS/fashioncraft-cli` |
| Windows `.msi` | `%LOCALAPPDATA%\Programs\FashionCraft\fashioncraft-cli.exe` |
| Windows portable `.zip` | `fashioncraft-cli.exe`, next to `fashioncraft.exe` |
| Linux `.tar.gz` | `bin/fashioncraft-cli`, next to `bin/fashioncraft` |
| Linux AppImage | The AppImage itself, with the arguments `cli mcp` |

`fashioncraft-cli mcp` follows the convention ArtCraft's AI gateway uses to start every Craft app,
`<app>-cli mcp`.

**How it connects.** `fashioncraft-cli mcp` speaks MCP over stdin and stdout. It forwards each
tool call to the open FashionCraft window over a loopback connection on `127.0.0.1`. Each call
carries a random token that the window writes to `agent-connection.json` in its session folder,
and only your user can read that file. When the window closes or the switch goes off, the file
goes away.

**Limits.**

- The tools need the FashionCraft window open. There is no headless mode yet.
- A screenshot needs the window to be visible. The other tools also work while the window is
  covered or minimised.
- A dialog open in the window blocks the tools. After 15 seconds, the agent gets "FashionCraft is
  busy", and nothing runs later.

## The tools

| Tool | What it does |
|---|---|
| `scene_summary` | Lists the pieces, seams, fabrics and avatar, where the garment hangs (cm), and whether there are unsaved changes. |
| `command_list` | Searches FashionCraft's menu commands. |
| `command_run` | Runs a menu command that opens no dialog. |
| `draft_garment` | Drafts a t-shirt, skirt or trousers from the avatar's or the window's measurements, with overrides in cm. It replaces the current garment. |
| `set_fabric` | Applies a fabric preset (cotton, jersey, wool, denim, leather, silk, linen, nylon, stretch) to some pieces or all of them. |
| `set_color` | Colours some fabrics or all of them. |
| `simulate` | Runs the simulation for 0.5 to 60 seconds, then reports where the garment hangs. |
| `screenshot` | Returns a PNG of the 3D view, or of the whole window. |
| `save_project` | Saves the project to `Documents/FashionCraft/<name>.json`. |
| `export_garment` | Exports the garment to `Documents/FashionCraft/<name>.obj` or `.glb`. |

`fashioncraft-cli tools` prints the tools with their input schemas.

The fabric presets are indicative starting points, not laboratory measurements. Calibrate them on
the real fabric before manufacturing.

## For developers

| File | Role |
|---|---|
| `src/agent.rs` | The tools, the connection file and the MCP server |
| `src/bin/fashioncraft-cli.rs` | `fashioncraft-cli` |
| `src/assistant.rs` | The Assistant's protocol for Claude and OpenAI-compatible APIs, and its tool loop |
| `src/atelier/agent_hub.rs` | The window's queue for tool calls and the loopback server |
| `src/atelier/agent_tools.rs` | The tools, run in the window |
| `src/atelier/assistant_ui.rs` | The Assistant window and its HTTPS transport |
| `src/atelier/assistant_keys.rs` | API key storage |

`tests/agent.rs` and the tests in `src/assistant.rs` run without a network. They feed canned
answers to the tool loop and to the MCP server.

To see the Assistant at work without an API key, run `scripts/assistant-stand-in.py`, a local
stand-in for both APIs; its header says how.
