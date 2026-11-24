//! Where agents' tool calls meet the window: a queue the window works through on its
//! interface thread, one call at a time, and the loopback server that `fashioncraft-cli mcp`
//! reaches while the person allows agents (Settings › AI agents). The Assistant panel puts its
//! calls on the same queue. See `fashioncraft::agent`.
use super::*;
use fashioncraft::agent::{Connection, LinkRequest, LinkResponse, ToolResult};
use serde_json::Value;
use std::io::{BufRead, Read, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

/// How long a call may wait for the window before the caller is told it timed out.
const CALL_LIMIT: Duration = Duration::from_secs(600);
/// How long a call may wait to be picked up. A native dialog blocks the window's thread; the
/// caller is told so, and the call is dropped rather than run once the dialog closes.
const PICKUP_LIMIT: Duration = Duration::from_secs(15);

/// A job's `state`: still queued, started by the window, or given up by its caller.
pub(super) const QUEUED: u8 = 0;
const STARTED: u8 = 1;
const ABANDONED: u8 = 2;

/// One tool call waiting for the window.
pub(super) struct Job {
    pub name: String,
    pub arguments: Value,
    pub reply: mpsc::Sender<ToolResult>,
    /// Who asked: shown in the status line.
    pub from: &'static str,
    /// QUEUED until the window starts it or the caller gives up on it, whichever is first.
    pub state: Arc<AtomicU8>,
}

impl Job {
    /// Claims the job for the window; false when its caller already gave up on it.
    fn start(&self) -> bool {
        self.state
            .compare_exchange(QUEUED, STARTED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// Waits for a queued job's answer: `PICKUP_LIMIT` for the window to start it, then
/// `CALL_LIMIT` in all.
pub(super) fn wait_for(state: &AtomicU8, answer: &mpsc::Receiver<ToolResult>) -> ToolResult {
    match answer.recv_timeout(PICKUP_LIMIT) {
        Ok(result) => return result,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return ToolResult::error("FashionCraft is closing");
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
    }
    if state
        .compare_exchange(QUEUED, ABANDONED, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        return ToolResult::error(
            "FashionCraft is busy: a dialog or message is probably open in its window. Ask \
             the person to close it, then try again. Nothing was done.",
        );
    }
    answer
        .recv_timeout(CALL_LIMIT.saturating_sub(PICKUP_LIMIT))
        .unwrap_or_else(|_| ToolResult::error("FashionCraft did not finish in time"))
}

/// A call the window started and finishes over later frames.
pub(super) enum Pending {
    /// The simulation runs until `until`, then pauses.
    Simulate { until: Instant, seconds: f64 },
    /// A screenshot was asked for; it arrives as an input event.
    Screenshot { asked: Instant, whole_window: bool },
    /// The photoreal render refines until `until`, then its image is saved to `path`.
    Render {
        started: Instant,
        until: Instant,
        path: std::path::PathBuf,
        /// When the save was asked for.
        saved: Option<Instant>,
        /// The render window was open before; otherwise it closes again.
        was_open: bool,
    },
}

pub(super) struct AgentHub {
    sender: mpsc::Sender<Job>,
    receiver: mpsc::Receiver<Job>,
    pub(super) running: Option<(Job, Pending)>,
    server: Option<Server>,
    output: PathBuf,
}

struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AgentHub {
    pub(super) fn new(output: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            running: None,
            server: None,
            output,
        }
    }
    /// A sender for the Assistant panel's calls.
    pub(super) fn sender(&self) -> mpsc::Sender<Job> {
        self.sender.clone()
    }
    pub(super) fn serving(&self) -> bool {
        self.server.is_some()
    }
    /// Starts or stops the loopback server and its connection file.
    pub(super) fn set_serving(&mut self, on: bool, ctx: &egui::Context) -> Result<(), String> {
        if on == self.serving() {
            return Ok(());
        }
        if !on {
            self.stop();
            return Ok(());
        }
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = fashioncraft::agent::new_token();
        Connection {
            port,
            token: token.clone(),
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
        .write(&self.output)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, sender, ctx) = (stop.clone(), self.sender.clone(), ctx.clone());
        let thread = std::thread::Builder::new()
            .name("agent-server".into())
            .spawn(move || accept(listener, &token, &flag, &sender, &ctx))
            .map_err(|e| e.to_string())?;
        self.server = Some(Server {
            port,
            stop,
            thread: Some(thread),
        });
        Ok(())
    }
    /// Stops the server and removes the connection file (also on drop).
    pub(super) fn stop(&mut self) {
        if let Some(mut server) = self.server.take() {
            server.stop.store(true, Ordering::Relaxed);
            // The accept loop blocks until someone connects: be that someone.
            let _ = std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], server.port)),
                Duration::from_secs(1),
            );
            if let Some(thread) = server.thread.take() {
                let _ = thread.join();
            }
        }
        Connection::remove(&self.output);
    }
    /// The next job nobody gave up on, claimed for the window.
    pub(super) fn next_job(&self) -> Option<Job> {
        std::iter::from_fn(|| self.receiver.try_recv().ok()).find(Job::start)
    }
}

impl Drop for AgentHub {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Accepts connections until told to stop; each is served on its own thread. The accept
/// blocks rather than polls: macOS's App Nap stretches a covered app's sleeps to seconds.
fn accept(
    listener: std::net::TcpListener,
    token: &str,
    stop: &AtomicBool,
    sender: &mpsc::Sender<Job>,
    ctx: &egui::Context,
) {
    for stream in listener.incoming() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match stream {
            Ok(stream) => {
                let (token, sender, ctx) = (token.to_owned(), sender.clone(), ctx.clone());
                let _ = std::thread::Builder::new()
                    .name("agent-call".into())
                    .spawn(move || serve(stream, &token, &sender, &ctx));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// One request line in, one answer line out. A wrong token gets nothing done.
fn serve(
    stream: std::net::TcpStream,
    token: &str,
    sender: &mpsc::Sender<Job>,
    ctx: &egui::Context,
) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut line = String::new();
    // A request is small: cap it so a stray client cannot fill memory.
    if std::io::BufReader::new(stream.take(1 << 20))
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let Ok(request) = serde_json::from_str::<LinkRequest>(&line) else {
        return;
    };
    let result = if !fashioncraft::agent::same_token(&request.token, token) {
        ToolResult::error("Wrong token: restart the MCP server or FashionCraft")
    } else {
        let (reply, answer) = mpsc::channel();
        let state = Arc::new(AtomicU8::new(QUEUED));
        let job = Job {
            name: request.name,
            arguments: request.arguments,
            reply,
            from: "MCP",
            state: state.clone(),
        };
        if sender.send(job).is_err() {
            ToolResult::error("FashionCraft is closing")
        } else {
            ctx.request_repaint();
            wait_for(&state, &answer)
        }
    };
    if let Ok(mut text) = serde_json::to_string(&LinkResponse {
        id: request.id,
        result,
    }) {
        text.push('\n');
        let _ = writer.write_all(text.as_bytes());
    }
}

impl Atelier {
    /// Once a frame: follow the AI-agents switch, finish the call in progress, start the next.
    pub(super) fn drive_agent(&mut self, ctx: &egui::Context) {
        let allowed = self.prefs.ai.agents_allowed;
        if allowed != self.agent.serving()
            && let Err(e) = self.agent.set_serving(allowed, ctx)
        {
            self.prefs.ai.agents_allowed = false;
            self.status = format!("AI agents could not start: {e}");
        }
        if let Some((job, mut pending)) = self.agent.running.take() {
            match self.continue_tool(ctx, &mut pending) {
                Some(result) => {
                    let _ = job.reply.send(result);
                }
                None => {
                    self.agent.running = Some((job, pending));
                    ctx.request_repaint_after(Duration::from_millis(50));
                    return;
                }
            }
        }
        if let Some(job) = self.agent.next_job() {
            self.status = format!("AI agent ({}): {}", job.from, job.name);
            match self.begin_tool(ctx, &job.name, &job.arguments) {
                Ok(result) => {
                    let _ = job.reply.send(result);
                }
                Err(pending) => {
                    self.agent.running = Some((job, pending));
                }
            }
            ctx.request_repaint();
        }
    }
}
