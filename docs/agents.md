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

Open the Assistant (its toolbar button, or **Window › Assistant**) and press the settings icon in
its header. **Connection** sets it up:

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
stay off. Each step is a new `agy` run that keeps and caches nothing from the last, so it gets the
conversation as plain text (the system message, one line per tool, then each call and result)
rather than the JSON request: about a quarter fewer tokens. A turn's first step (the plan) uses
Antigravity's usual reasoning effort and the steps after it `--effort medium`; a small request
asks for low, as with the other CLIs.

**Using it.** **Assistant** in the main toolbar opens and closes it (its window stays below the toolbar); while it is closed, a fuchsia
dot on the button means it is working and an amber one that it waits for you. Type a request and
press Enter; Shift+Enter starts a new line. The header shows who answers, the model (a menu of
that provider's models), what the conversation has used, and copy, settings and new chat. Each
answer folds the tools it ran into one line ("5 steps in the project · 3 min 20 s", the whole
turn's time, the model's thinking included) that opens to each step with its result and
screenshots, the details in words (pieces and fabrics by name, colours by name, "6 points" for an
outline); a plan the model shows (the `plan` tool) and the check after
simulating (`check_garment`) show as cards. **Back to before this message**, under each of your
messages, puts the project back as it was when you sent it (Undo brings the work back) and tells
the model so with your next message. With pieces selected, the message box shows **This piece:
…**: the message is about them, and × takes them off until the selection changes. While it works,
the step in progress shows its time (a simulation, how far it got of its seconds), and what you
write then waits above the box and goes when the turn ends (× takes it back to change it).
**Stop** ends the turn after the current step. Paste a picture or files with Cmd+V (Ctrl+V on Windows and
Linux) while the Assistant is open, wherever the focus is.

**A new chat** says what the project holds and offers requests that fit it (a garment to go with
the one there, a check of seams, strain and pressure, sizes, one from a photo, which first asks
for the picture); a click sends it. Under them, **Remembered for this project** lists the notes
kept in the project file (`assistant_notes`): the person edits them there or in Settings › Own
instructions, the model with `project_notes`, and they go at the start of every conversation.

**Settings.** The settings icon in the Assistant's header opens its own window:

- **Connection**: who answers, whether it is ready, and the model. With providers that suggest
  models (Claude), **Automatic** picks a small model for simple steps (a colour, a fabric,
  simulating again) and a larger one for new garments, fixes, files and a conversation's first
  request (`large_task` in `src/assistant.rs`); **Always the same** uses one. The header's model
  menu has Automatic too, and shows the model the last message went to.
- **Own instructions**: text that goes at the start of every conversation, in every project.
- **Permissions**: see below.
- **Limits and usage**: the plan's limit or the context, this conversation's tokens and money, the
  review pause, pictures (**Only when needed**, or **After each simulation**) and, with an API
  key, a spend cap per conversation: past it, the turn stops after its step and new messages wait
  for a new chat or a higher cap.

**Defaults** puts permissions, the pause, pictures, the cap and Automatic back as they come; the
connection and your instructions stay.

**Asking first.** What the Assistant does inside the project is undone with Undo; what leaves it
is not. So by default it asks before saving the project and before exporting or writing files
(`save_project`, `export_garment`, `render`): a card in the chat shows the file and **Allow**,
**Always in this project** (until FashionCraft closes) and **No**; the window's dot and the
toolbar's turn amber while it waits. Writing something else instead says no and stops the turn
after that step; the message goes then. **Settings › Permissions** sets each kind of step
(editing, simulating and pictures, opening attachments, saving, exporting) to **Without asking**,
**Ask first** or **Never**; looking (summaries, measures, the plan) never asks.

**When the connection fails.** A request that fails for a moment (the network, a busy service,
or a CLI error that says it can be retried, as Antigravity's `"retryable": true`) is sent again
twice, after 3 and 10 seconds, and the chat says so. If it still fails, the turn ends with what
went wrong in plain words (no JSON) and the work so far stays: write "continue" to go on. A wrong
key, a model name or a used-up limit is not sent again.

**How long a job runs.** The Assistant works until the job is done: there is no limit on its
steps. Two safety stops end a runaway loop, and the chat says which one: the same tool call failing
5 times in a row, or 400 rounds of tools in one turn. To look at the work as it goes, turn on
**Settings › Limits and usage › Review › Pause every N steps** (off by default): it stops after that many steps; the
card says so, **Continue** goes on with everything so far and **Back to before the pause** puts the
project back as it was before those steps.

**Usage.** Beside the provider's name the panel shows what this conversation has used, kept in
memory only: new input, output and cached input tokens (12.4k in · 3.1k out · 40k cached), the
money for API keys (≈, from published prices in `PRICES` in `src/assistant.rs`, which lists
Claude's models; other models show tokens without money until their prices are added) and a small
ring. The ring shows the plan's usage limit where the provider reports it, and otherwise how full
the model's context window is (its tooltip says which):

| Provider | Tokens | Money | Ring |
|---|---|---|---|
| Claude, API key | Each response's `usage` | Price table | Context window |
| ChatGPT / Gemini / local, API | Each response's `usage` | Price table (where listed) | Context window (where listed) |
| Claude Code CLI | `usage` in its `result` | Its `total_cost_usd` (API-equivalent; a subscription is not billed per token) | None reported |
| Codex CLI | `usage` in `turn.completed` | None | The plan limit, when an event carries `rate_limits` (`used_percent`, `window_minutes`) |
| Gemini CLI | `stats.models.*.tokens` | None | None reported |
| Antigravity CLI | What it prints, if any | None | Its `used_percent`, if it prints one |

**Long sessions.** Each request declares a short list of common tools; `more_tools` lists the
others and gives their inputs, and `use_tool` runs one (MCP clients see every tool). After the
first `scene_summary`, the next ones give only what changed, and each later message of the
person's ends with a note from FashionCraft: the scene is as the model last saw it, or what
changed since (by hand, say), so the model need not look again. Every turn remembers the earlier
ones:

- **Claude (API key)** gets the conversation unchanged, only ever added to: Claude's thinking is
  bound to everything before it, so an edited history would be refused or lose it, and the
  unchanged start is what the prompt cache serves.
- **ChatGPT, Gemini, local models and the four CLIs** get the person's messages and attachments
  whole and the recent messages whole; before them, tool results are cut short and screenshots
  become a line of text, in steps of ten messages so that part stays the same from one request
  to the next and the provider's prompt cache serves it. Past about 80 messages, the oldest tool
  steps are left out (said so), but what was said stays: every message of the person's and every
  reply, so a long first job is still known in the next turn.

On a replayed 33-step dress job this took the input from about 751k to 427k tokens (the last
request from 36k to 18k). When a CLI fails, the chat shows the error it gave.

**Small requests.** The system prompt carries a short map of the app (the drafted bases' pieces
and segments, and which tool does each common request), checked against the real drafts by a
test, so a request needs no looking up first. A small change ("make the shirt red") is one tool
call and a one-line reply:
no screenshot, simulation or check unless the drape changes. A message of up to 80 characters
with nothing attached asks the CLIs for low reasoning effort for its first 3 steps, then their
usual effort if the job turns out bigger: Codex `-c model_reasoning_effort="low"`, Claude Code and
Antigravity `--effort low` (Gemini CLI has no such option). Starting a CLI takes 0.02 s (Codex,
Claude Code) to 1.1 s (Gemini), so the time a step takes is the model's.

**Copying and logs.** All text in the chat can be selected and copied, and the copy icon at the
top copies the whole conversation. Each conversation is also written, as it happens, to
`assistant-logs/conversation-<time>.jsonl` in FashionCraft's session folder (on macOS
`~/Library/Application Support/FashionCraft/session/`, on Linux
`~/.local/share/fashioncraft/session/`): the messages, every tool call with its arguments and
result, how long each answer took (`model_seconds`), usage and failures, each line with its time
(`at`) and without picture bytes, so a run can be looked at afterwards.

**Rules the Assistant keeps.** It never changes the avatar's pose or body unless asked; with a
draped garment, a pose moves the avatar into place gradually and the cloth follows. Every change
is checked: one that would make cloth start inside other cloth or the body is undone, and while
such a state stands, tools that build on the garment wait until it is cleared. Pieces a cut, a
copy or a merge makes get names of their own and free space on the board, and the Assistant
names pieces for what they are.

Some requests to try:

- "Draft a denim skirt for this avatar, simulate it and show me how it falls."
- "Make a shirt with set-in sleeves, then add a 15 × 17 cm pocket to the front."
- "Add a 2 cm dart at the middle of the front hem and check that every seam closes."
- "Set up sizes S, M, L and XL with M as the base and show me the size chart."
- "Save this as summer-tee and export it as GLB."

**Attaching files.** Press **Attach**, drop files on the Assistant, or press Cmd+V (Ctrl+V on
Windows and Linux) while the Assistant is open, wherever the focus is, with a picture or files on
the clipboard; text still pastes as text into the field that has the focus. On Linux under
Wayland FashionCraft reads the clipboard with `wl-paste` (the wl-clipboard package) when it is
installed, else through XWayland; under X11 with its own reader, then `xclip` when it is
installed. If the clipboard cannot be read, the reason shows under the attachments. Each file shows as a chip above the message; hover it to see what was
read, × takes it off. What the model receives:

| File | The model gets |
|---|---|
| Picture (PNG, JPEG, BMP, TGA, HDR) | The picture, as PNG, at most 1568 px a side. Claude, ChatGPT, Gemini and the four CLIs see it; a local model is told it was attached but cannot see it, and says so. |
| PDF | Claude (API key) reads it whole. Other connections are told a PDF was attached that they cannot read, and ask for its pages as pictures or its text. |
| Text, Markdown, JSON, CSV/TSV table | Its text, at most 40,000 characters. |
| DXF, DXF-AAMA/ASTM, SVG pattern | FashionCraft reads it: the number of pieces, each one's size and perimeter, and a picture of the pieces with a 10 cm bar. `open_attachment` opens it in FashionCraft's importer, where the person checks the scale and imports it (undoable). |
| 3D model (OBJ, FBX, glTF, USD, Alembic…) | A note on how FashionCraft opens it; `open_attachment` opens the avatar importer. |
| BVH, PC2, MDD, `.fcfabric`, projects | A note on where FashionCraft opens it. |
| Video, audio, Word, Excel, HEIC, WebP | Not read yet: the model is told so and tells the person (attach screenshots, PDF, CSV or PNG instead). |

Attachments go to the provider you chose, like your messages.

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
- Pictures of the 3D view (`screenshot`, `camera` with `screenshot`, `check_garment` with a
  view) are drawn for the purpose, so they work while the window is covered or minimised, and
  without the marks drawn over the view. Each costs time and about 1-2k tokens, so the prompt
  asks for one only to verify a finished job, to compare with the person's picture, to place a
  problem the numbers cannot, or when asked, one view per check; a picture fewer than 3 rounds
  after the one before in the same turn comes with a short note saying so. The bench's colour
  cases (`navy-colour-es`, `red-shirt-en`) fail if the model takes a picture. A screenshot of the whole window needs the window
  shown. The other tools also work while the window is covered or minimised.
- A dialog open in the window blocks the tools. After 15 seconds, the agent gets "FashionCraft is
  busy", and nothing runs later.

## The tools

**Looking**

| Tool | What it does |
|---|---|
| `scene_summary` | Every piece (size, segments with ids, lengths and the side they face, internal lines, placement), every seam (the length of each side, and after a simulation its gap in mm and whether it closed), the fabrics with their physical parameters, the avatar's measurements, pose and arrangement points, the sizes, the current selection, warnings (unequal seams, open seams, missed target lengths, pieces sewn to nothing), and the simulation state. The avatar's arrangement points and the avatar, pose and fabric presets come as counts; `lists: true` names them. |
| `piece_detail` | One piece in full: numbered outline points (cm, x right, y down), each segment's end points, length, curve, target length, seam allowance, notches and seams, its internal lines and placement. |
| `size_chart` | The sizes and every point of measure in every size. |
| `screenshot` | A PNG of the 3D view (drawn for the purpose, whatever the window's state), or of the whole window (shown). |

**Selecting and menu commands**

| Tool | What it does |
|---|---|
| `select` | Selects pieces, a segment or internal line, an outline point or a seam, as a click would. |
| `command_list` | The menu commands an agent can run, with what each acts on and whether it can run now. Commands that wait for the mouse or open a window, a file dialog or a web page are left out unless asked for, and then say which agent tool does the job instead. |
| `command_run` | Runs one menu command, optionally selecting its `target` first. It refuses commands an agent cannot use, and commands whose selection is missing, with a message that says what to do. |

**Pattern**

| Tool | What it does |
|---|---|
| `draft_garment` | A base drafted from the avatar's measurements (or the defaults), with overrides in cm: `tshirt` (kimono sleeves), `shirt` (front, back and two set-in sleeves wrapped around the arms, each cap half eased into the armhole it faces), `dress` (sleeveless A-line), `skirt`, `trousers`. `mode` `add` (the default) wears it over the garment there, which keeps its pieces and drape: it must be draped first; the new garment keeps its arrangement around the avatar, goes on an outer layer, moves up to 8 cm out where it would start inside or within 3 mm of the other cloth, and its pieces are named after it ("t-shirt front"), in one undo step. `replace` drafts it instead of everything there. It warns when a piece starts touching the body. Each base is cut from a fitting fabric (a t-shirt in jersey, the others in light woven cotton), its own even when added over a garment of the same preset, and the reply names it. |
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
| `open_attachment` | Opens a pattern or 3D model the person attached in FashionCraft's importer, for the person to check and confirm. |

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
| `set_sleeve` | Sets a sleeve into its armhole as draft_garment's shirt does: the cap sewn to the front and back armholes with ease (a puffed sleeve is gathered), the underarm closed, and the sleeve wrapped round the avatar's arm below the shoulder, clear of the body. Agents never place a sleeve by moving it. On the stock mannequin the armholes close to under 8 mm (`tests/set_sleeve.rs`). |
| `add_detail` | A patch pocket, a band collar or a placket from a few numbers: cut, sewn and laid on the garment, in one undo step. Add details before simulating: a piece that gets a new stitching line is laid out again, so on a draped garment the detail is refused and nothing changes. A band collar's segments may come in any order; they are followed around the neck through the corners the seams join. |
| `pin` | Pins cloth around a point of a piece, removes or moves pins. |
| `grab` | Pulls the cloth at a point by a 3D offset while simulating, then lets go. |
| `trace_on_body` | The 3D pen: a closed line through points on the body becomes a piece laid there. |

**Windows as tools**

| Tool | Window |
|---|---|
| `simulation_settings` | Simulation properties: quality, GPU, iterations, damping, friction, collision distance, gravity, wind, floor, particle distance. |
| `camera` | Camera properties and the View menu; with `screenshot` it returns a picture of the view. |
| `render_settings` | The photoreal render's settings and the 3D view's style. |
| `avatar_maker` | Create avatar (MakeHuman · Anny): settings (0–1, not centimetres), local measures and look; `height_cm` finds the height setting for a height in cm, and every answer gives the body's `height_cm`. |
| `pose_joints` | The pose editor's joints, in degrees. |
| `measure_fabric` | Measure fabric: fits a fabric's parameters to tests on the real cloth and gives it to pieces. |
| `points_of_measure` | Points of measure of the size chart. |
| `measure` | The tape tools: body and garment girth at a height and the ease between them, or the distance between two outline points. |

**Checking and knowing**

| Tool | What it does |
|---|---|
| `check_garment` | After simulating: a short list of problems, each with where it is (piece, area, nearest segment, height): open seams with their gaps, strained cloth, tight spots pressing on the body, cloth inside the body, and a drape that has not settled; with `view`, a picture from that view. The shared prompt tells agents to call it every time they simulate. It answers ok, the problems and the counts; `details: true` adds every number. A sleeve cap eased into its armhole on purpose is not a problem. |
| `guide` | Short notes on pattern making and sewing (grain, ease, seam allowance, notches, darts, set-in sleeves…) and how FashionCraft does each. |
| `project_notes` | Adds or removes one of the notes kept with the project (who it is for, sizes, fabrics, the workshop's rules); they go to the model at the start of every conversation and show in the Assistant's new chat. |

`fashioncraft-cli tools` prints the tools with their input schemas.

**Cloth that starts inside cloth.** A change that would make cloth start inside other cloth or
the body is undone and refused, with the pieces named. If the simulation still pauses on such an
intersection, `simulate` and `check_garment` say so first, name the pieces and where, and tell the
agent to undo, move the piece clear or reset the arrangement.

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
| `src/attachments.rs` | Reads attached files into what a model takes |
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
