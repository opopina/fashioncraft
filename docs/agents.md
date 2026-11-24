# AI agents and the Assistant

AI can work in FashionCraft in two ways:

- **The Assistant** (Window › Assistant): a chat inside FashionCraft. You pick the model: Claude,
  ChatGPT, Gemini or a model running on your own computer.
- **MCP**: AI apps that speak the Model Context Protocol work in FashionCraft from outside. These
  include Claude Desktop, Claude Code, Cursor, LM Studio and ArtCraft's AI gateway.

Both use the same tools, listed [below](#the-tools), and the same instructions (`SYSTEM_PROMPT` in
`src/assistant.rs`, which the MCP server also sends as its instructions). The tools work in the
open window, where you see every change as it happens. Every pattern edit goes through the same
undoable commands as the menus, so Edit › Undo takes it back. The tools save, export and render
only into your `Documents/FashionCraft` folder, and they never open file dialogs.

## The Assistant

Open **Window › Assistant** and press **Connection** to set it up:

| Provider | Model | Address | Key |
|---|---|---|---|
| Claude (Anthropic) | `claude-opus-5-5` unless you type another | `https://api.anthropic.com/v1` | An API key from the Claude Console, or `ANTHROPIC_API_KEY` |
| Claude (Claude Code CLI) | Leave blank for the Claude Code default, or enter a model | Detected on this computer | Your Claude Code login |
| ChatGPT (OpenAI) | The model name, as OpenAI lists it | `https://api.openai.com/v1` | An OpenAI API key, or `OPENAI_API_KEY` |
| ChatGPT (Codex CLI) | Leave blank for the default from Codex, or enter a model | Detected on this computer | Sign in with ChatGPT using `codex login` |
| Gemini (Google) | The model name, as Google lists it | Google's OpenAI-compatible address | A Gemini API key, or `GEMINI_API_KEY` |
| Gemini (Gemini CLI) | Leave blank for the Gemini CLI default, or enter a model | Detected on this computer | Your Gemini CLI login |
| Antigravity (Google CLI) | Leave blank for the Antigravity default, or enter a model | Detected on this computer | Your Antigravity CLI (`agy`) login |
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

**Claude through Claude Code.** Select **Claude (Claude Code CLI)** in Connection. Sign in to Claude Code with `claude auth login` if prompted. On macOS the app detects the Claude Code executable installed by Claude Desktop, including when Finder has a different PATH from Terminal. Set `FASHIONCRAFT_CLAUDE` to select a particular executable. The assistant passes chat history and requested screenshots to Claude Code and runs the requested garment tools in the open editor. External MCP access can stay off.

**ChatGPT through Codex CLI.** Choose **ChatGPT (Codex CLI)** under Connection. The panel starts
Codex in the background and sends the chat, available garment tools and any requested screenshots
to OpenAI using the CLI's ChatGPT login. Each tool result returns to the same visible chat. This
mode works with external MCP access turned off. It uses a recent Codex CLI with structured output
and ephemeral execution. On macOS it also detects the CLI bundled with ChatGPT. To select another
installation, set `FASHIONCRAFT_CODEX` to the executable path. Stop cancels the current model
request; garment edits already applied can be undone in the editor.

**Gemini through Gemini CLI.** Select **Gemini (Gemini CLI)** under Connection. The assistant runs
`gemini` in read-only headless mode (`--approval-mode plan --skip-trust --output-format json`) with
your existing Gemini CLI login and passes requested screenshots as temporary files (`@view-N.png`).
Set `FASHIONCRAFT_GEMINI` to pick a specific executable. External MCP access can stay off.

**Google Antigravity through Antigravity CLI (`agy`).** Select **Antigravity (Google CLI)** under
Connection. The assistant runs `agy` in sandboxed headless streaming mode with `--json-schema`
structured output using your existing Antigravity login, and passes requested screenshots in the
isolated working directory. Run `agy models` to see available model IDs, or leave Model blank for
your default. Set `FASHIONCRAFT_ANTIGRAVITY` to pick a specific executable. External MCP access can
stay off.

**Using it.** Type a request and press Enter; Shift+Enter starts a new line. You see each tool the
model runs, with its result, and a thumbnail of each screenshot it takes. **Stop** ends the turn
after the current step. **New chat** starts a fresh conversation. Some requests to try:

- "Draft a denim skirt for this avatar, simulate it and show me how it falls."
- "Make a shirt with set-in sleeves, then add a 15 × 17 cm pocket to the front."
- "Add a 2 cm dart at the middle of the front hem and check that every seam closes."
- "Set up sizes S, M, L and XL with M as the base and show me the size chart."
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

**Looking**

| Tool | What it does |
|---|---|
| `scene_summary` | Every piece (size, segments with ids, lengths and the side they face, internal lines, placement), every seam (the length of each side, and after a simulation its gap in mm and whether it closed), the fabrics with their physical parameters, the avatar's measurements, pose and arrangement points, the sizes, the current selection, warnings (unequal seams, open seams, missed target lengths, pieces sewn to nothing), and the simulation state. |
| `piece_detail` | One piece in full: numbered outline points (cm, x right, y down), each segment's end points, length, curve, target length, seam allowance, notches and seams, its internal lines and placement. |
| `size_chart` | The sizes and every point of measure in every size. |
| `screenshot` | A PNG of the 3D view, or of the whole window. |

**Selecting and menu commands**

| Tool | What it does |
|---|---|
| `select` | Selects pieces, a segment or internal line, an outline point or a seam, as a click would. |
| `command_list` | The menu commands an agent can run, with what each acts on and whether it can run now. Commands that wait for the mouse or open a window, a file dialog or a web page are left out unless asked for, and then say which agent tool does the job instead. |
| `command_run` | Runs one menu command, optionally selecting its `target` first. It refuses commands an agent cannot use, and commands whose selection is missing, with a message that says what to do. |

**Pattern**

| Tool | What it does |
|---|---|
| `draft_garment` | Replaces the garment with a base drafted from the avatar's measurements (or the defaults), with overrides in cm: `tshirt` (kimono sleeves), `shirt` (front, back and two set-in sleeves wrapped around the arms, each cap half eased into the armhole it faces), `dress` (sleeveless A-line), `skirt`, `trousers`. It warns when a piece starts touching the body. |
| `create_piece` | A new piece from a rectangle or an outline of points, named. |
| `move_points` | Moves outline points by an offset in cm. |
| `set_segment_length` | Makes a straight segment a given length, keeping its start, end or middle. |
| `add_point` | Splits an unsewn segment. |
| `add_dart` | An edge dart cut and sewn into a straight unsewn segment, or a diamond dart inside the piece. |
| `add_pleats` | Knife, box or inverted box pleats along a segment, tacked or not. |
| `piece_edit` | Renames, duplicates, mirrors (a linked copy), deletes a piece, sets its grain or a segment's seam allowance. |
| `sew` | Sews segments 1:1 or M:N, whole or a stretch, with or without ease. Unless told, it sews the way that does not twist the seam, judged from where the pieces are placed. |
| `seam_edit` | Removes a seam, or allows or forbids ease on it. |
| `place_piece` | Places a piece on an arrangement point of the avatar (wrapped or flat, at a clearance), after checking it starts clear of the body. |

**Fabric, avatar, sizes**

| Tool | What it does |
|---|---|
| `set_fabric` | Applies a fabric preset (cotton, jersey, wool, denim, leather, silk, linen, nylon, stretch). |
| `set_fabric_parameters` | Weight, thickness, stretch along warp and weft, shear, bending and friction of one fabric. |
| `set_color` | Colours fabrics. |
| `set_avatar` | A body from the avatar library, or the current one reshaped to new measurements. The drape is reset; this is not on the Undo history, as in the menus. |
| `set_pose` | A pose preset: Rest, Arms down, T-pose, Arms forward, Step. |
| `grading` | Creates sizes, adds a size, grows an outline point by an equal step per size, shows a size in 3D. |

**Result**

| Tool | What it does |
|---|---|
| `simulate` | Runs the simulation for 0.5 to 60 seconds, then reports every seam's gap and the warnings. |
| `render` | A photoreal path-traced picture of the 3D view, saved to `Documents/FashionCraft/<name>.png`. Uses the GPU. |
| `save_project` | Saves the project to `Documents/FashionCraft/<name>.json`. |
| `export_garment` | Exports the garment to `Documents/FashionCraft/<name>.obj` or `.glb`. |

**Any edit, by name**

| Tool | What it does |
|---|---|
| `edit_ops` | FashionCraft's own edit commands (over 100: every undoable change to a pattern) with their fields as JSON Schema, filtered by words or asked for by name. |
| `edit` | Runs one of them with its fields. It is the raw editing layer (metres), checked by the same code as the menus before it is applied, and undoable. |

The schemas come from the Rust source of `authoring::Command` and the types its fields use, so a
new command or field shows up by itself; a test builds a value from every schema and checks that
the command accepts it. Four commands are not offered, each with its reason (`edit_ops` lists them).

**Mouse tools in centimetres, and exact engines**

| Tool | What it does |
|---|---|
| `transform_piece` | Moves or turns pieces on the 2D board, or in 3D about their centre (world cm: x to the avatar's left, y up, z to its front). |
| `bend_segment` | Curves a segment so its middle moves a given number of cm outward or inward. |
| `style_line` | The 3D view's style-line tool: bends, moves or stretches a segment by a 3D move in cm, across the seams it is sewn into. |
| `add_internal_line` | An internal line through points in cm. |
| `add_buttons` | Buttons or buttonholes, a number of them a given spacing apart. |
| `add_detail` | A patch pocket, a band collar or a placket from a few numbers: cut, sewn and laid on the drape as it is, in one undo step. |
| `pin` | Pins cloth around a point of a piece, removes or moves pins. |
| `grab` | Pulls the cloth at a point by a 3D offset while simulating, then lets go. |
| `trace_on_body` | The 3D pen: a closed line through points on the body becomes a piece laid there. |

**Windows as tools**

| Tool | Window |
|---|---|
| `simulation_settings` | Simulation properties: quality, GPU, iterations, damping, friction, collision distance, gravity, wind, floor, particle distance. |
| `camera` | Camera properties and the View menu; with `screenshot` it returns a picture of the view. |
| `render_settings` | The photoreal render's settings and the 3D view's style. |
| `avatar_maker` | Create avatar (MakeHuman · Anny): settings, local measures and look. |
| `pose_joints` | The pose editor's joints, in degrees. |
| `measure_fabric` | Measure fabric: fits a fabric's parameters to tests on the real cloth and gives it to pieces. |
| `points_of_measure` | Points of measure of the size chart. |
| `measure` | The tape tools: body and garment girth at a height and the ease between them, or the distance between two outline points. |

**Checking and knowing**

| Tool | What it does |
|---|---|
| `check_garment` | After simulating: a short list of problems, each with where it is (piece, area, nearest segment, height): open seams with their gaps, strained cloth, tight spots pressing on the body, cloth inside the body, and a drape that has not settled; with `view`, a picture from that view. The shared prompt tells agents to call it every time they simulate. |
| `guide` | Short notes on pattern making and sewing (grain, ease, seam allowance, notches, darts, set-in sleeves…) and how FashionCraft does each. |

`fashioncraft-cli tools` prints the tools with their input schemas.

**Every menu command is covered.** Each command in the registry either runs with `command_run`,
is done by an agent tool (`agent_instead` in `src/atelier/commands.rs`), or has a written reason
why agents cannot do it (`agent_reason`), which `command_list` and `command_run` pass on. A test
fails when a command has none, and `agent_use` has no catch-all, so a new command must be
classified before it builds. Of 225 commands, 122 run, 63 are done by an agent tool and 40 have a
reason: file dialogs (agents read and write only `Documents/FashionCraft`), the person's own
settings, image graphics, UV islands by hand, and animation.

**What agents cannot do yet, and why.** Drawing freehand, file dialogs (import, open, export
formats other than OBJ and GLB), placing image graphics, arranging UV islands by hand and
animation. Agents are told so and point the person to the menu. The drafted bases are simplified
blocks: straight armholes and sleeve caps; collars, plackets and pockets come from `add_detail`.
The fabric presets are indicative starting points, not laboratory measurements: calibrate them on
the real fabric (`measure_fabric`) before manufacturing.

## For developers

| File | Role |
|---|---|
| `src/agent.rs` | The tools, the connection file and the MCP server |
| `src/agent_ops.rs` | What the tools read from a garment and the authoring commands their edits become, testable without a window |
| `src/agent_base.rs` | The base tools' descriptions, and the form tools' logic that needs no window |
| `src/agent_edit.rs` | The generic edit: `authoring::Command` from JSON, schemas derived from its source |
| `src/agent_build.rs` | Exact engines: details, bends, buttons, lines, moves, and laying new pieces on the drape |
| `src/agent_check.rs` | `check_garment`'s signals and problems |
| `src/agent_guide.rs` | `guide`'s topics |
| `src/agent_eval.rs` | The evaluation's checks and report |
| `evals/assistant/cases.json` | The evaluation's requests, in Spanish and English |
| `src/bin/fashioncraft-cli.rs` | `fashioncraft-cli` |
| `src/assistant.rs` | The Assistant's protocol for Claude and OpenAI-compatible APIs, and its tool loop |
| `src/atelier/agent_hub.rs` | The window's queue for tool calls and the loopback server |
| `src/atelier/agent_tools.rs` | The tools, run in the window |
| `src/atelier/agent_base_tools.rs` | The base tools, run in the window |
| `src/atelier/assistant_ui.rs` | The Assistant window and its HTTPS transport |
| `src/atelier/assistant_keys.rs` | API key storage |
| `src/atelier/assistant_eval.rs` | Runs the evaluation in the window |
| `src/atelier/commands.rs` | `agent_use`, `agent_target`, `agent_instead`: what each menu command needs from an agent |

`tests/agent.rs` and the tests in `src/assistant.rs` run without a network. They feed canned
answers to the tool loop and to the MCP server. The tests in `src/agent_ops.rs` apply every
editing tool to a drafted garment through `authoring::apply` and check the result; the ones in
`src/agent_eval.rs` check the scorer and that every case names real tools.

### Evaluating the Assistant

`evals/assistant/cases.json` holds typical requests, about half in Spanish, each with checks: the
tools a good answer calls (with which arguments), what the garment's summary shows afterwards or
how it changed, the reply's language and words, and limits on failed calls. A FashionCraft window
runs them against the model chosen in Settings › Assistant:

```sh
HOME=/tmp/fc-eval FASHIONCRAFT_ASSISTANT_EVAL=evals/assistant/cases.json \
  FASHIONCRAFT_ASSISTANT_EVAL_OUT=/tmp/fc-eval/report ./target/release/fashioncraft
```

Each case starts from its garment drafted on the avatar the window opened with, sends its request
as if typed in the Assistant, waits for the turn to end and scores it. `report.md` lists every case
and every failed check, `report.json` keeps the transcripts. `FASHIONCRAFT_ASSISTANT_EVAL_ONLY=dress,seam`
runs only the cases whose ids contain those words. A run spends the provider's usage (about 20
conversations), so run it in a copy with its own `HOME`, never in a window with unsaved work.

To see the Assistant at work without an API key, run `scripts/assistant-stand-in.py`, a local
stand-in for both APIs; its header says how.
