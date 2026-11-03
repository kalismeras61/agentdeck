//! Claude Code sessions.
//!
//! Each AgentDeck session owns one long-running `claude` process started from the user's own,
//! unmodified CLI in the project folder:
//!
//!   claude -p --input-format stream-json --output-format stream-json --verbose
//!          --include-partial-messages --permission-prompt-tool stdio [--model M] [--effort E]
//!          [--resume ID]
//!
//! Prompts go in on stdin as stream-json user messages. Permission prompts arrive on stdout as
//! `control_request` / `can_use_tool` and are answered on stdin with a `control_response` for that
//! exact `request_id`. AgentDeck never touches Claude credentials: sign-in stays in Claude Code.
//!
//! Reasoning effort: `--effort` on start; while running, an `apply_flag_settings` control
//! request with `effortLevel` (null goes back to Claude Code's default). What Claude Code
//! actually uses is `applied.effort` in the answer to a `get_settings` control request.

use crate::db::{now_ms, Db, SessionRow, StoredEvent};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Longest single stream-json line we accept (large tool outputs arrive as one line).
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
const STDERR_TAIL_BYTES: usize = 16 * 1024;
const STOP_GRACE: Duration = Duration::from_secs(3);
/// How long to wait for Claude Code to answer one of our own control requests.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(8);

/// The reasoning efforts Claude Code knows; each model lists the ones it takes.
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
/// Overrides any effort picked in Claude Code (or here) when set in the environment.
pub const EFFORT_ENV: &str = "CLAUDE_CODE_EFFORT_LEVEL";

/// Provider lines that are pure UI chatter and not worth persisting.
const SKIP_KINDS: &[&str] = &[
    "system/status",
    "system/task_summary",
    "system/post_turn_summary",
    "active_goal",
    "autocompact_state",
    "rate_limit_event",
    "keep_alive",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    pub session_id: String,
    pub text: String,
    /// True when a new assistant message starts; the UI clears its streaming buffer.
    pub reset: bool,
}

#[derive(Debug, Clone)]
pub enum Outbound {
    Event(StoredEvent),
    Session(SessionRow),
    Delta(Delta),
    Relay(crate::db::RelayRow),
    Flow(crate::db::FlowRow),
    Bot(crate::db::Bot),
    BotTask(crate::db::BotTask),
    BotSchedule(crate::db::BotSchedule),
    /// An agent's plan usage limits changed.
    Limits(crate::limits::PlanLimits),
}

pub type Sink = Arc<dyn Fn(Outbound) + Send + Sync>;

/// The stored user message: what the user wrote, and the files they attached.
pub fn user_message(text: &str, att: &crate::attach::Attach) -> Value {
    if att.is_empty() {
        json!({ "text": text })
    } else {
        json!({ "text": text, "attachments": att.shown, "note": att.note(false) })
    }
}

/// Environment that runs Claude Code against an Anthropic-compatible endpoint (DeepSeek, Kimi,
/// Ollama, …). Every model tier is set, since a partly configured CLI fails quietly in
/// background tasks and subagents.
pub fn profile_env(p: &crate::db::Profile, key: Option<&str>) -> Vec<(&'static str, String)> {
    let small = if p.small_model.trim().is_empty() {
        &p.model
    } else {
        &p.small_model
    };
    vec![
        (
            "ANTHROPIC_BASE_URL",
            p.base_url.trim_end_matches('/').to_string(),
        ),
        // Local servers ignore the token but the CLI wants one.
        (
            "ANTHROPIC_AUTH_TOKEN",
            key.filter(|k| !k.is_empty())
                .unwrap_or("agentdeck-local")
                .to_string(),
        ),
        ("ANTHROPIC_MODEL", p.model.clone()),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", p.model.clone()),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", p.model.clone()),
        ("ANTHROPIC_DEFAULT_FABLE_MODEL", p.model.clone()),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", small.clone()),
        ("ANTHROPIC_SMALL_FAST_MODEL", small.clone()),
        ("CLAUDE_CODE_SUBAGENT_MODEL", p.model.clone()),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1".into()),
    ]
}

struct Proc {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    pending: Mutex<HashMap<String, Value>>,
    stopping: AtomicBool,
    /// Our own control requests waiting for Claude Code's answer, by request id.
    waiters: Mutex<HashMap<String, std::sync::mpsc::Sender<Value>>>,
}

impl Proc {
    fn write_line(&self, v: &Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().map_err(|_| "stdin lock poisoned")?;
        let stdin = guard.as_mut().ok_or("The agent process is shutting down")?;
        let mut line = serde_json::to_vec(v).map_err(|e| e.to_string())?;
        line.push(b'\n');
        stdin
            .write_all(&line)
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Couldn't reach Claude Code: {e}"))
    }
}

pub struct Manager {
    db: Arc<Db>,
    sink: Sink,
    procs: Mutex<HashMap<String, Arc<Proc>>>,
    /// Test hooks: explicit binary and extra environment.
    pub binary_override: Option<PathBuf>,
    pub extra_env: Vec<(String, String)>,
    /// The `agentdeck` MCP server for bot sessions (`mcp::Hub::launcher`).
    pub mcp: Option<crate::mcp::Launcher>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    AllowSession,
    Deny,
}

impl Decision {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "allow" => Ok(Self::Allow),
            "allow_session" => Ok(Self::AllowSession),
            "deny" => Ok(Self::Deny),
            other => Err(format!("Unknown decision: {other}")),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::AllowSession => "allow_session",
            Self::Deny => "deny",
        }
    }
}

fn kind_of(v: &Value) -> String {
    let t = v.get("type").and_then(Value::as_str).unwrap_or("unknown");
    match v.get("subtype").and_then(Value::as_str) {
        Some(st) => format!("{t}/{st}"),
        None => match v.pointer("/request/subtype").and_then(Value::as_str) {
            Some(st) => format!("{t}/{st}"),
            None => t.to_string(),
        },
    }
}

/// First non-empty line as plain text: markdown emphasis, headings and code ticks removed.
pub(crate) fn first_line(s: &str, max: usize) -> String {
    let raw = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let stripped: String = raw
        .trim_start_matches(['#', '>', '-', ' '])
        .chars()
        .filter(|c| !matches!(c, '*' | '`'))
        .collect();
    let line = stripped.trim();
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    out
}

/// Reads one `\n`-terminated line without ever buffering more than `max` bytes of it.
/// Returns (bytes, truncated). `None` at EOF.
pub(crate) fn read_line_bounded<R: BufRead>(
    r: &mut R,
    max: usize,
) -> std::io::Result<Option<(Vec<u8>, bool)>> {
    let mut buf = Vec::new();
    let mut truncated = false;
    loop {
        let (done, used) = {
            let available = r.fill_buf()?;
            if available.is_empty() {
                return Ok(if buf.is_empty() && !truncated {
                    None
                } else {
                    Some((buf, truncated))
                });
            }
            match available.iter().position(|&b| b == b'\n') {
                Some(i) => {
                    if !truncated {
                        let room = max.saturating_sub(buf.len());
                        buf.extend_from_slice(&available[..i.min(room)]);
                        truncated = i > room;
                    }
                    (true, i + 1)
                }
                None => {
                    if !truncated {
                        let room = max.saturating_sub(buf.len());
                        buf.extend_from_slice(&available[..available.len().min(room)]);
                        truncated = available.len() > room;
                    }
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        if done {
            return Ok(Some((buf, truncated)));
        }
    }
}

/// Folders GUI apps don't get on PATH but CLIs commonly live in.
pub fn extra_path_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for rel in [
            ".local/bin",
            ".claude/local",
            ".npm-global/bin",
            ".bun/bin",
            ".volta/bin",
            ".cargo/bin",
            "bin",
        ] {
            dirs.push(home.join(rel));
        }
        // nvm: newest installed Node first.
        if let Ok(rd) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|e| e.path().join("bin")))
                .collect();
            versions.sort();
            versions.reverse();
            dirs.extend(versions);
        }
    }
    for d in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

/// The login shell's PATH first (what Terminal has), then AgentDeck's own, then the usual
/// install folders.
pub fn augmented_path() -> std::ffi::OsString {
    let mut all: Vec<PathBuf> = crate::shellenv::get()
        .get("PATH")
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    let own: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in own.into_iter().chain(extra_path_dirs()) {
        if !all.contains(&d) {
            all.push(d);
        }
    }
    std::env::join_paths(all).unwrap_or_default()
}

/// The `claude` command line for a session.
fn args(session: &SessionRow) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
    ]
    .map(String::from)
    .to_vec();
    if !session.model.is_empty() && session.model != "default" {
        a.extend(["--model".into(), session.model.clone()]);
    }
    if let Some(e) = session.effort.as_deref().filter(|e| EFFORTS.contains(e)) {
        a.extend(["--effort".into(), e.into()]);
    }
    if let Some(id) = &session.provider_session_id {
        a.extend(["--resume".into(), id.clone()]);
    }
    if session.mode == "plan" {
        a.extend(["--permission-mode".into(), "plan".into()]);
    }
    a
}

impl Manager {
    pub fn new(db: Arc<Db>, sink: Sink) -> Self {
        Self {
            db,
            sink,
            procs: Mutex::new(HashMap::new()),
            binary_override: None,
            extra_env: Vec::new(),
            mcp: None,
        }
    }

    fn emit_session(&self, row: Option<SessionRow>) {
        if let Some(r) = row {
            (self.sink)(Outbound::Session(r));
        }
    }

    fn record(&self, session_id: &str, source: &str, kind: &str, payload: &Value) {
        match self.db.append_event(session_id, source, kind, payload) {
            Ok(Some(ev)) => (self.sink)(Outbound::Event(ev)),
            Ok(None) => {}
            Err(e) => eprintln!("agentdeck: failed to store event: {e}"),
        }
    }

    fn set_status(&self, session_id: &str, status: &str, snippet: Option<&str>) {
        self.emit_session(
            self.db
                .update_session(session_id, Some(status), snippet, None, None, None)
                .ok()
                .flatten(),
        );
    }

    pub fn is_running(&self, session_id: &str) -> bool {
        self.procs
            .lock()
            .map(|p| p.contains_key(session_id))
            .unwrap_or(false)
    }

    fn proc(&self, session_id: &str) -> Option<Arc<Proc>> {
        self.procs
            .lock()
            .ok()
            .and_then(|p| p.get(session_id).cloned())
    }

    fn binary(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.binary_override {
            return Ok(p.clone());
        }
        crate::providers::find_binary("claude").ok_or_else(|| {
            "Claude Code isn't installed or isn't on your PATH. Install it, run `claude` once in a terminal to sign in, then try again.".to_string()
        })
    }

    fn spawn(self: &Arc<Self>, session: &SessionRow, cwd: &str) -> Result<Arc<Proc>, String> {
        let bin = self.binary()?;
        let mut cmd = Command::new(&bin);
        cmd.args(args(session));
        // A bot session: its agentdeck tools, allowed without asking.
        if let Some(l) = self.mcp.as_ref().and_then(|f| f(session)) {
            cmd.args(l.claude_args());
        }
        cmd.current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Don't let AgentDeck's own environment make the child think it's nested in another Claude session.
        for (k, _) in std::env::vars_os() {
            let k = k.to_string_lossy();
            if (k.starts_with("CLAUDE_CODE_")
                || k == "CLAUDECODE"
                || k.starts_with("CLAUDE_") && k != "CLAUDE_CONFIG_DIR")
                && !k.is_empty()
            {
                cmd.env_remove(k.as_ref());
            }
        }
        crate::shellenv::apply(&mut cmd);
        cmd.env("PATH", augmented_path());
        // A custom model profile: point the unmodified CLI at that Anthropic-compatible endpoint.
        if let Some(pid) = &session.profile_id {
            let p =
                self.db.profile(pid).map_err(|e| e.to_string())?.ok_or(
                    "This session's model profile was deleted. Pick another one in Settings.",
                )?;
            for (k, v) in profile_env(&p, crate::secrets::get(&p.id).as_deref()) {
                cmd.env(k, v);
            }
            cmd.env_remove("ANTHROPIC_API_KEY");
        }
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start Claude Code ({}): {e}", bin.display()))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        let proc = Arc::new(Proc {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            stopping: AtomicBool::new(false),
            waiters: Mutex::new(HashMap::new()),
        });
        proc.write_line(&json!({ "type": "control_request", "request_id": format!("agentdeck-init-{}", now_ms()), "request": { "subtype": "initialize" } }))?;

        let stderr_tail = Arc::new(Mutex::new(Vec::<u8>::new()));
        {
            let tail = stderr_tail.clone();
            std::thread::spawn(move || {
                let mut r = stderr;
                let mut chunk = [0u8; 4096];
                while let Ok(n) = r.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut t) = tail.lock() {
                        t.extend_from_slice(&chunk[..n]);
                        let excess = t.len().saturating_sub(STDERR_TAIL_BYTES);
                        t.drain(..excess);
                    }
                }
            });
        }

        let me = self.clone();
        let sid = session.id.clone();
        let p = proc.clone();
        std::thread::spawn(move || me.read_loop(sid, p, stdout, stderr_tail));
        Ok(proc)
    }

    fn read_loop(
        self: Arc<Self>,
        sid: String,
        proc: Arc<Proc>,
        stdout: std::process::ChildStdout,
        stderr_tail: Arc<Mutex<Vec<u8>>>,
    ) {
        let mut reader = BufReader::with_capacity(256 * 1024, stdout);
        loop {
            match read_line_bounded(&mut reader, MAX_LINE_BYTES) {
                Ok(None) => break,
                Err(e) => {
                    self.record(&sid, "agentdeck", "error", &json!({ "title": "Lost the connection to Claude Code", "message": e.to_string() }));
                    break;
                }
                Ok(Some((_, true))) => {
                    self.record(&sid, "agentdeck", "error", &json!({ "title": "Skipped an oversized message", "message": format!("Claude Code sent one message larger than {} MB. It was dropped to keep AgentDeck responsive.", MAX_LINE_BYTES / 1024 / 1024) }));
                }
                Ok(Some((bytes, false))) => {
                    if bytes.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(v) => self.handle_line(&sid, &proc, v),
                        Err(_) => {
                            let text = String::from_utf8_lossy(&bytes);
                            self.record(
                                &sid,
                                "agentdeck",
                                "stdout_text",
                                &json!({ "text": first_line(&text, 2000) }),
                            );
                        }
                    }
                }
            }
        }
        // Process ended.
        let status = proc.child.lock().ok().and_then(|mut c| c.wait().ok());
        let code = status.and_then(|s| s.code());
        let expected = proc.stopping.load(Ordering::SeqCst);
        let tail = stderr_tail
            .lock()
            .map(|t| String::from_utf8_lossy(&t).into_owned())
            .unwrap_or_default();
        if let Ok(mut procs) = self.procs.lock() {
            if procs.get(&sid).is_some_and(|p| Arc::ptr_eq(p, &proc)) {
                procs.remove(&sid);
            }
        }
        let ok = expected || code == Some(0);
        self.record(&sid, "agentdeck", "process_exit", &json!({ "code": code, "expected": expected, "stderrTail": if ok { String::new() } else { tail.clone() } }));
        if ok {
            self.set_status(&sid, "idle", None);
        } else {
            if tail.contains("No conversation found") {
                self.record(&sid, "agentdeck", "error", &json!({
                    "title": "Claude Code couldn't find this conversation",
                    "message": "Claude Code only continues a conversation from the folder it was started in, and this session's project folder isn't that one. Move the session to the project for that folder (Session actions → Move to…), or start a new session.",
                }));
            }
            let msg = match code {
                Some(c) => format!("Claude Code exited with code {c}"),
                None => "Claude Code was stopped by a signal".to_string(),
            };
            self.set_status(&sid, "error", Some(&msg));
        }
    }

    fn handle_line(&self, sid: &str, proc: &Arc<Proc>, v: Value) {
        let kind = kind_of(&v);
        match kind.as_str() {
            "stream_event" => {
                let ev = &v["event"];
                if v.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
                    return; // subagent output isn't streamed into the main thread
                }
                match ev.get("type").and_then(Value::as_str) {
                    Some("message_start") => (self.sink)(Outbound::Delta(Delta {
                        session_id: sid.into(),
                        text: String::new(),
                        reset: true,
                    })),
                    Some("content_block_delta") => {
                        if let Some(t) = ev.pointer("/delta/text").and_then(Value::as_str) {
                            (self.sink)(Outbound::Delta(Delta {
                                session_id: sid.into(),
                                text: t.into(),
                                reset: false,
                            }));
                        }
                    }
                    _ => {}
                }
                return;
            }
            k if k.starts_with("control_response") => {
                // An answer someone is waiting for (`get_settings`, `apply_flag_settings`).
                let rid = v.pointer("/response/request_id").and_then(Value::as_str);
                if let Some(tx) = rid.and_then(|r| proc.waiters.lock().ok()?.remove(r)) {
                    let _ = tx.send(v["response"].clone());
                    return;
                }
                // The answer to our `initialize`: keep the models this Claude Code offers.
                let ours = v
                    .pointer("/response/request_id")
                    .and_then(Value::as_str)
                    .is_some_and(|r| r.starts_with("agentdeck-init"));
                let on_profile = self
                    .db
                    .session(sid)
                    .ok()
                    .flatten()
                    .is_some_and(|s| s.profile_id.is_some());
                if ours {
                    if let Some(r) = v.pointer("/response/response") {
                        crate::commands::remember(sid, crate::commands::from_claude(r));
                    }
                }
                if ours && !on_profile {
                    if let Some(r) = v.pointer("/response/response") {
                        crate::models::remember(
                            &self.db,
                            "claude-code",
                            &crate::models::from_claude(r),
                        );
                    }
                }
                return;
            }
            "rate_limit_event" => {
                // Not a session event: the numbers are kept as the plan's limits instead.
                self.capture_limits(sid, &v);
                return;
            }
            k if SKIP_KINDS.contains(&k) => return,
            _ => {}
        }

        if kind == "control_request/can_use_tool" {
            // With a permission prompt tool, Claude Code forwards plan-mode-blocked tools to us
            // instead of refusing them itself. AgentDeck enforces plan mode: refuse anything but
            // the plan hand-back and questions to the user.
            let tool_name = v
                .pointer("/request/tool_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let in_plan = self
                .db
                .session(sid)
                .ok()
                .flatten()
                .is_some_and(|s| s.mode == "plan");
            if in_plan && !["ExitPlanMode", "AskUserQuestion"].contains(&tool_name) {
                if let Some(rid) = v.get("request_id").and_then(Value::as_str) {
                    self.record(sid, "claude", &kind, &v);
                    let _ = proc.write_line(&json!({ "type": "control_response", "response": { "subtype": "success", "request_id": rid, "response": { "behavior": "deny", "message": "This session is in plan mode: don't change anything yet. Finish the plan and present it with ExitPlanMode." } } }));
                    self.record(
                        sid,
                        "agentdeck",
                        "permission_decision",
                        &json!({ "requestId": rid, "decision": "deny", "auto": "plan_mode" }),
                    );
                    return;
                }
            }
            if let Some(rid) = v.get("request_id").and_then(Value::as_str) {
                if let Ok(mut p) = proc.pending.lock() {
                    p.insert(rid.to_string(), v["request"].clone());
                }
            }
            let tool = v
                .pointer("/request/display_name")
                .or_else(|| v.pointer("/request/tool_name"))
                .and_then(Value::as_str)
                .unwrap_or("a tool");
            let what = if tool_name == "ExitPlanMode" {
                "Has a plan for you".to_string()
            } else if tool_name == "AskUserQuestion" {
                "Has a question for you".to_string()
            } else {
                format!("Wants to use {tool}")
            };
            self.set_status(sid, "waiting", Some(&what));
            self.record(sid, "claude", &kind, &v);
            return;
        }
        if kind.starts_with("control_request/") {
            // Hooks, SDK MCP servers and other host features AgentDeck doesn't provide: answer so the CLI never hangs.
            if let Some(rid) = v.get("request_id").and_then(Value::as_str) {
                let sub = kind.trim_start_matches("control_request/");
                let _ = proc.write_line(&json!({ "type": "control_response", "response": { "subtype": "error", "request_id": rid, "error": format!("AgentDeck doesn't support {sub} requests") } }));
            }
            return;
        }
        if kind == "control_cancel_request" {
            if let (Some(rid), Ok(mut p)) = (
                v.get("request_id").and_then(Value::as_str),
                proc.pending.lock(),
            ) {
                p.remove(rid);
            }
            self.set_status(sid, "running", Some("Working"));
            self.record(sid, "claude", &kind, &v);
            return;
        }

        // Session state is updated before the event is published, so listeners never see an event
        // that its session row doesn't reflect yet.
        match kind.as_str() {
            "system/init" => {
                let psid = v.get("session_id").and_then(Value::as_str);
                self.emit_session(
                    self.db
                        .update_session(sid, None, None, None, None, psid)
                        .ok()
                        .flatten(),
                );
            }
            k if k.starts_with("result") => {
                let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false)
                    || k != "result/success";
                let text = v.get("result").and_then(Value::as_str).unwrap_or("");
                if is_error {
                    let msg = if text.is_empty() {
                        "The turn ended with an error".to_string()
                    } else {
                        first_line(text, 90)
                    };
                    self.set_status(sid, "error", Some(&msg));
                } else {
                    let snippet = if text.is_empty() {
                        "Finished".to_string()
                    } else {
                        first_line(text, 90)
                    };
                    self.set_status(sid, "idle", Some(&snippet));
                }
            }
            _ => {}
        }
        self.record(sid, "claude", &kind, &v);
    }

    /// Plan limits from a `rate_limit_event`. Sessions on a model profile talk to another
    /// endpoint, whose limits aren't the Claude plan's.
    fn capture_limits(&self, sid: &str, v: &Value) {
        let on_profile = self
            .db
            .session(sid)
            .ok()
            .flatten()
            .map_or(true, |s| s.profile_id.is_some());
        if on_profile {
            return;
        }
        let Some(info) = v.get("rate_limit_info") else {
            return;
        };
        let prev = crate::limits::previous(&self.db, "claude-code");
        let windows = crate::limits::from_claude(info, prev.as_deref(), now_ms());
        crate::limits::record(&self.db, &self.sink, "claude-code", windows);
    }

    /* ------------------------------------------------------------ commands */

    pub fn send_message(self: &Arc<Self>, session_id: &str, text: &str) -> Result<(), String> {
        self.send_message_with(session_id, text, &crate::attach::Attach::default())
    }

    /// Sends a message with files the user attached; images go in as images.
    pub fn send_message_with(
        self: &Arc<Self>,
        session_id: &str,
        text: &str,
        att: &crate::attach::Attach,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Write a message first".into());
        }
        if text.len() > 200_000 {
            return Err("That message is longer than 200,000 characters".into());
        }
        let session = self
            .db
            .session(session_id)
            .map_err(|e| e.to_string())?
            .ok_or("Session not found")?;
        // The session's worktree when it has one, otherwise the project folder.
        let dir = self
            .db
            .session_dir(&session)
            .map_err(|e| e.to_string())?
            .ok_or("Project not found")?;
        if !Path::new(&dir).is_dir() {
            return Err(format!("The folder {dir} doesn't exist anymore"));
        }
        let proc = match self.proc(session_id) {
            Some(p) => p,
            None => {
                let p = self.spawn(&session, &dir)?;
                self.procs
                    .lock()
                    .map_err(|_| "lock poisoned")?
                    .insert(session_id.to_string(), p.clone());
                p
            }
        };
        self.record(
            session_id,
            "agentdeck",
            "user_message",
            &user_message(text, att),
        );
        let title = (session.title == "New session").then(|| first_line(text, 48));
        self.emit_session(
            self.db
                .update_session(
                    session_id,
                    Some("running"),
                    Some(&first_line(text, 80)),
                    title.as_deref(),
                    None,
                    None,
                )
                .ok()
                .flatten(),
        );
        proc.write_line(&json!({
            "type": "user",
            "session_id": "",
            "message": { "role": "user", "content": std::iter::once(json!({ "type": "text", "text": format!("{text}{}", att.note(true)) })).chain(att.claude_images()).collect::<Vec<_>>() },
            "parent_tool_use_id": null
        }))
    }

    pub fn respond_permission(
        &self,
        session_id: &str,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), String> {
        self.respond(session_id, request_id, decision, None)
    }

    /// Denies a request with a message for Claude, e.g. when the user hands a plan to another agent.
    pub fn deny_with_message(
        &self,
        session_id: &str,
        request_id: &str,
        message: &str,
    ) -> Result<(), String> {
        self.respond(session_id, request_id, Decision::Deny, Some(message))
    }

    fn respond(
        &self,
        session_id: &str,
        request_id: &str,
        decision: Decision,
        deny_message: Option<&str>,
    ) -> Result<(), String> {
        let proc = self
            .proc(session_id)
            .ok_or("This session isn't running, so the request can't be answered anymore")?;
        let request = proc
            .pending
            .lock()
            .map_err(|_| "lock poisoned")?
            .get(request_id)
            .cloned()
            .ok_or("This request is no longer waiting. The agent may have moved on or stopped.")?;
        let input = request.get("input").cloned().unwrap_or(json!({}));
        let exits_plan = request.get("tool_name").and_then(Value::as_str) == Some("ExitPlanMode");
        let response = match decision {
            // Approving a plan leaves plan mode, the same as in Claude Code's own UI.
            Decision::Allow if exits_plan => {
                json!({ "behavior": "allow", "updatedInput": input, "updatedPermissions": [{ "type": "setMode", "mode": "default", "destination": "session" }] })
            }
            Decision::Allow => json!({ "behavior": "allow", "updatedInput": input }),
            Decision::AllowSession => {
                let suggestions: Vec<Value> = request
                    .get("permission_suggestions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut s| {
                        s["destination"] = json!("session");
                        s
                    })
                    .collect();
                if suggestions.is_empty() {
                    return Err("Claude Code didn't offer a lasting rule for this request".into());
                }
                json!({ "behavior": "allow", "updatedInput": input, "updatedPermissions": suggestions })
            }
            Decision::Deny => {
                json!({ "behavior": "deny", "message": deny_message.unwrap_or(if exits_plan { "The user wants to keep planning. Ask what to change, or refine the plan." } else { "The user denied this in AgentDeck." }) })
            }
        };
        proc.write_line(&json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } }))?;
        if let Ok(mut p) = proc.pending.lock() {
            p.remove(request_id);
        }
        self.record(
            session_id,
            "agentdeck",
            "permission_decision",
            &json!({ "requestId": request_id, "decision": decision.as_str() }),
        );
        if exits_plan && matches!(decision, Decision::Allow) {
            if let Ok(Some(row)) = self.db.set_session_mode(session_id, "default") {
                (self.sink)(Outbound::Session(row));
            }
            self.record(
                session_id,
                "agentdeck",
                "mode_changed",
                &json!({ "mode": "default" }),
            );
        }
        let still_waiting = proc.pending.lock().map(|p| !p.is_empty()).unwrap_or(false);
        self.set_status(
            session_id,
            if still_waiting { "waiting" } else { "running" },
            Some("Working"),
        );
        Ok(())
    }

    /// Answers Claude Code's AskUserQuestion: the chosen labels (or your own text) per question,
    /// keyed by the question text as Claude expects. `None` skips the question.
    pub fn answer_question(
        &self,
        session_id: &str,
        request_id: &str,
        answers: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Result<(), String> {
        let proc = self
            .proc(session_id)
            .ok_or("This session isn't running, so the question can't be answered anymore")?;
        let request = proc
            .pending
            .lock()
            .map_err(|_| "lock poisoned")?
            .get(request_id)
            .cloned()
            .ok_or("This question is no longer waiting. The agent may have moved on or stopped.")?;
        if request.get("tool_name").and_then(Value::as_str) != Some("AskUserQuestion") {
            return Err("That request isn't a question".into());
        }
        let mut input = request.get("input").cloned().unwrap_or(json!({}));
        let response = match answers {
            Some(a) => {
                let mut map = serde_json::Map::new();
                for q in input["questions"].as_array().cloned().unwrap_or_default() {
                    let text = q["question"].as_str().unwrap_or("").to_string();
                    if let Some(chosen) = a.get(&text).filter(|c| !c.is_empty()) {
                        map.insert(text, json!(chosen.join(", ")));
                    }
                }
                if map.is_empty() {
                    return Err("Pick or write an answer first".into());
                }
                input["answers"] = Value::Object(map);
                json!({ "behavior": "allow", "updatedInput": input })
            }
            None => {
                json!({ "behavior": "deny", "message": "The user skipped this question. Carry on with your best judgment and say which choice you made." })
            }
        };
        proc.write_line(&json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } }))?;
        if let Ok(mut p) = proc.pending.lock() {
            p.remove(request_id);
        }
        self.record(
            session_id,
            "agentdeck",
            "question_answered",
            &json!({ "requestId": request_id, "answers": answers, "skipped": answers.is_none() }),
        );
        let still_waiting = proc.pending.lock().map(|p| !p.is_empty()).unwrap_or(false);
        self.set_status(
            session_id,
            if still_waiting { "waiting" } else { "running" },
            Some("Working"),
        );
        Ok(())
    }

    pub fn interrupt(&self, session_id: &str) -> Result<(), String> {
        let proc = self.proc(session_id).ok_or("This session isn't running")?;
        proc.write_line(&json!({ "type": "control_request", "request_id": format!("agentdeck-int-{}", now_ms()), "request": { "subtype": "interrupt" } }))?;
        self.record(session_id, "agentdeck", "interrupt_requested", &json!({}));
        Ok(())
    }

    /// Switches a running session between "default" and "plan". A session that isn't running
    /// picks the mode up from the database when it next starts.
    pub fn set_mode(&self, session_id: &str, mode: &str) -> Result<(), String> {
        if let Some(proc) = self.proc(session_id) {
            proc.write_line(&json!({ "type": "control_request", "request_id": format!("agentdeck-mode-{}", now_ms()), "request": { "subtype": "set_permission_mode", "mode": mode } }))?;
        }
        Ok(())
    }

    /// Switches the model of a running Claude Code; the next message uses it.
    pub fn set_model(&self, session_id: &str, model: &str) -> Result<(), String> {
        if let Some(proc) = self.proc(session_id) {
            proc.write_line(&json!({ "type": "control_request", "request_id": format!("agentdeck-model-{}", now_ms()), "request": { "subtype": "set_model", "model": model } }))?;
        }
        Ok(())
    }

    /// Sends a control request to a running Claude Code and waits for its answer.
    fn control(&self, session_id: &str, purpose: &str, request: Value) -> Result<Value, String> {
        let proc = self.proc(session_id).ok_or("This session isn't running")?;
        let rid = format!(
            "agentdeck-{purpose}-{}-{}",
            now_ms(),
            uuid::Uuid::new_v4().simple()
        );
        let (tx, rx) = std::sync::mpsc::channel();
        if let Ok(mut w) = proc.waiters.lock() {
            w.insert(rid.clone(), tx);
        }
        let sent = proc.write_line(
            &json!({ "type": "control_request", "request_id": rid, "request": request }),
        );
        let answer = sent.and_then(|_| {
            rx.recv_timeout(CONTROL_TIMEOUT)
                .map_err(|_| "Claude Code didn't answer in time".to_string())
        });
        if let Ok(mut w) = proc.waiters.lock() {
            w.remove(&rid);
        }
        let answer = answer?;
        match answer["subtype"].as_str() {
            Some("success") => Ok(answer.get("response").cloned().unwrap_or(Value::Null)),
            _ => Err(answer["error"]
                .as_str()
                .unwrap_or("Claude Code returned an error")
                .to_string()),
        }
    }

    /// Changes the effort of a running Claude Code at once (None: back to its default).
    /// A session that isn't running gets `--effort` when it next starts.
    pub fn set_effort(&self, session_id: &str, effort: Option<&str>) -> Result<(), String> {
        if self.proc(session_id).is_none() {
            return Ok(());
        }
        self.control(
            session_id,
            "effort",
            json!({ "subtype": "apply_flag_settings", "settings": { "effortLevel": effort } }),
        )
        .map(|_| ())
    }

    /// The effort a running Claude Code uses right now (`applied.effort` from `get_settings`).
    /// None while the session isn't running, or for a model without effort levels.
    pub fn applied_effort(&self, session_id: &str) -> Result<Option<String>, String> {
        if self.proc(session_id).is_none() {
            return Ok(None);
        }
        let r = self.control(session_id, "settings", json!({ "subtype": "get_settings" }))?;
        Ok(r.pointer("/applied/effort")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    /// `CLAUDE_CODE_EFFORT_LEVEL` as Claude Code gets it from AgentDeck: it wins over any pick.
    pub fn effort_env(&self) -> Option<String> {
        self.extra_env
            .iter()
            .find(|(k, _)| k == EFFORT_ENV)
            .map(|(_, v)| v.clone())
            .or_else(|| crate::shellenv::get().get(EFFORT_ENV).cloned())
            .filter(|v| !v.trim().is_empty())
    }

    /// Ends the agent process: closes stdin, then kills it if it hasn't exited after a short grace period.
    pub fn stop(&self, session_id: &str) {
        if let Some(proc) = self.proc(session_id) {
            proc.stopping.store(true, Ordering::SeqCst);
            if let Ok(mut s) = proc.stdin.lock() {
                s.take();
            }
            std::thread::spawn(move || {
                std::thread::sleep(STOP_GRACE);
                if let Ok(mut c) = proc.child.lock() {
                    if matches!(c.try_wait(), Ok(None)) {
                        let _ = c.kill();
                    }
                }
            });
        }
    }

    pub fn shutdown_all(&self) {
        let procs: Vec<Arc<Proc>> = self
            .procs
            .lock()
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default();
        for p in procs {
            p.stopping.store(true, Ordering::SeqCst);
            if let Ok(mut c) = p.child.lock() {
                let _ = c.kill();
            }
        }
    }

    pub fn pending_count(&self, session_id: &str) -> usize {
        self.proc(session_id)
            .and_then(|p| p.pending.lock().ok().map(|x| x.len()))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_splits_and_truncates() {
        let data = b"short\nthis-line-is-way-too-long\nok\n";
        let mut r = BufReader::with_capacity(4, &data[..]);
        assert_eq!(
            read_line_bounded(&mut r, 10).unwrap(),
            Some((b"short".to_vec(), false))
        );
        let (long, truncated) = read_line_bounded(&mut r, 10).unwrap().unwrap();
        assert!(truncated);
        assert_eq!(long.len(), 10);
        assert_eq!(
            read_line_bounded(&mut r, 10).unwrap(),
            Some((b"ok".to_vec(), false))
        );
        assert_eq!(read_line_bounded(&mut r, 10).unwrap(), None);
    }

    #[test]
    fn kinds_include_subtypes() {
        assert_eq!(
            kind_of(&json!({ "type": "system", "subtype": "init" })),
            "system/init"
        );
        assert_eq!(
            kind_of(
                &json!({ "type": "control_request", "request": { "subtype": "can_use_tool" } })
            ),
            "control_request/can_use_tool"
        );
        assert_eq!(kind_of(&json!({ "type": "assistant" })), "assistant");
    }

    #[test]
    fn snippets_are_plain_text() {
        assert_eq!(
            first_line("\n## **Harbor 2.0** is a `fitness` app\nmore", 80),
            "Harbor 2.0 is a fitness app"
        );
        assert_eq!(first_line("abcdef", 3), "abc…");
    }

    #[test]
    fn command_line_carries_model_effort_resume_and_plan() {
        let mut s = SessionRow {
            id: "s".into(),
            project_id: "p".into(),
            provider: "claude-code".into(),
            model: "default".into(),
            title: "t".into(),
            status: "idle".into(),
            snippet: String::new(),
            provider_session_id: None,
            created_at: 0,
            updated_at: 0,
            mode: "default".into(),
            profile_id: None,
            worktree: None,
            branch: None,
            effort: None,
        };
        let a = args(&s).join(" ");
        assert!(
            !a.contains("--model") && !a.contains("--effort"),
            "defaults are left to Claude Code: {a}"
        );
        s.model = "opus".into();
        s.effort = Some("xhigh".into());
        s.provider_session_id = Some("abc".into());
        s.mode = "plan".into();
        assert!(args(&s)
            .join(" ")
            .ends_with("--model opus --effort xhigh --resume abc --permission-mode plan"));
        s.effort = Some("--danger".into());
        assert!(
            !args(&s).iter().any(|x| x == "--effort" || x == "--danger"),
            "only known levels are passed"
        );
    }

    #[test]
    fn decisions_parse_strictly() {
        assert_eq!(Decision::parse("allow").unwrap(), Decision::Allow);
        assert!(Decision::parse("always").is_err());
    }
}

/// End-to-end tests against the real `claude` CLI pointed at `scripts/mock_anthropic.py`.
/// Run with: python3 scripts/mock_anthropic.py 8787 &  AGENTDECK_IT=1 cargo test -- --ignored
#[cfg(test)]
mod integration {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    struct Harness {
        db: Arc<Db>,
        mgr: Arc<Manager>,
        rx: mpsc::Receiver<Outbound>,
        sid: String,
        _dir: tempfile::TempDir,
    }

    fn harness() -> Option<Harness> {
        if std::env::var("AGENTDECK_IT").is_err() {
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir(&proj).unwrap();
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let s = db
            .create_session(&p.id, "claude-code", "sonnet", "New session")
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| {
            let _ = tx.lock().unwrap().send(o);
        });
        let mut mgr = Manager::new(db.clone(), sink);
        let url =
            std::env::var("AGENTDECK_MOCK_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into());
        mgr.extra_env = vec![
            ("ANTHROPIC_BASE_URL".into(), url),
            ("ANTHROPIC_API_KEY".into(), "sk-ant-test-dummy-key".into()),
            (
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
                "1".into(),
            ),
            ("DISABLE_AUTOUPDATER".into(), "1".into()),
        ];
        Some(Harness {
            db,
            mgr: Arc::new(mgr),
            rx,
            sid: s.id,
            _dir: dir,
        })
    }

    impl Harness {
        /// Waits for a stored event whose kind starts with `prefix`.
        fn wait_event(&self, prefix: &str) -> StoredEvent {
            let deadline = Instant::now() + Duration::from_secs(60);
            while Instant::now() < deadline {
                if let Ok(Outbound::Event(e)) = self.rx.recv_timeout(Duration::from_millis(200)) {
                    if e.kind.starts_with(prefix) {
                        return e;
                    }
                }
            }
            panic!("timed out waiting for {prefix}");
        }
        fn status(&self) -> String {
            self.db.session(&self.sid).unwrap().unwrap().status
        }
    }

    #[test]
    #[ignore]
    fn dropped_image_and_file_reach_claude_code() {
        use base64::Engine as _;
        let Some(h) = harness() else { return };
        let proj =
            h.db.project(&h.db.session(&h.sid).unwrap().unwrap().project_id)
                .unwrap()
                .unwrap()
                .path;
        let png = std::path::Path::new(&proj).join("shot.png");
        std::fs::write(&png, base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==").unwrap()).unwrap();
        let txt = std::path::Path::new(&proj).join("notes.txt");
        std::fs::write(&txt, "hi").unwrap();
        let att = crate::attach::resolve(
            &[
                png.to_string_lossy().into_owned(),
                txt.to_string_lossy().into_owned(),
            ],
            &proj,
        )
        .unwrap();
        h.mgr
            .send_message_with(&h.sid, "what is in this picture", &att)
            .unwrap();
        let user = h.wait_event("user_message");
        assert_eq!(user.payload["text"], "what is in this picture");
        assert_eq!(
            user.payload["attachments"],
            json!(["shot.png", "notes.txt"])
        );
        let result = h.wait_event("result");
        assert_eq!(result.payload["result"], "I can see 1 image (image/png).");
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn plan_mode_keep_planning_then_approve_then_build() {
        let Some(h) = harness() else { return };
        h.db.set_session_mode(&h.sid, "plan").unwrap();
        // In plan mode a write is refused by AgentDeck without asking.
        h.mgr.send_message(&h.sid, "please write the file").unwrap();
        let init = h.wait_event("system/init");
        assert_eq!(init.payload["permissionMode"], "plan");
        let blocked = h.wait_event("permission_decision");
        assert_eq!(blocked.payload["auto"], "plan_mode");
        h.wait_event("result");
        assert_eq!(h.status(), "idle");
        h.mgr
            .send_message(&h.sid, "make a plan to fix the discount")
            .unwrap();
        let req = h.wait_event("control_request/can_use_tool");
        assert_eq!(req.payload["request"]["tool_name"], "ExitPlanMode");
        assert!(req.payload["request"]["input"]["plan"]
            .as_str()
            .unwrap()
            .contains("Apply the discount before tax"));
        // "Keep planning": the plan is refused and the session stays in plan mode.
        let rid = req.payload["request_id"].as_str().unwrap().to_string();
        h.mgr
            .respond_permission(&h.sid, &rid, Decision::Deny)
            .unwrap();
        h.wait_event("result");
        assert_eq!(h.db.session(&h.sid).unwrap().unwrap().mode, "plan");

        // Ask again and approve: Claude Code leaves plan mode and AgentDeck records it.
        h.mgr.send_message(&h.sid, "update the plan").unwrap();
        let req = h.wait_event("control_request/can_use_tool");
        assert_eq!(req.payload["request"]["tool_name"], "ExitPlanMode");
        let rid = req.payload["request_id"].as_str().unwrap().to_string();
        h.mgr
            .respond_permission(&h.sid, &rid, Decision::Allow)
            .unwrap();
        h.wait_event("result");
        assert_eq!(h.db.session(&h.sid).unwrap().unwrap().mode, "default");

        // Out of plan mode, a write is asked about (not refused by plan mode) and happens.
        h.mgr.send_message(&h.sid, "please write the file").unwrap();
        let req = h.wait_event("control_request/can_use_tool");
        assert_eq!(req.payload["request"]["tool_name"], "Write");
        let rid = req.payload["request_id"].as_str().unwrap().to_string();
        h.mgr
            .respond_permission(&h.sid, &rid, Decision::Allow)
            .unwrap();
        h.wait_event("result");
        let proj =
            h.db.project(&h.db.session(&h.sid).unwrap().unwrap().project_id)
                .unwrap()
                .unwrap();
        assert!(Path::new(&proj.path).join("hello.txt").exists());
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_PLAN") {
            let evs = h.db.events(&h.sid).unwrap();
            std::fs::write(path, serde_json::to_string_pretty(&evs).unwrap()).unwrap();
        }
        h.mgr.stop(&h.sid);
    }

    /// Claude Code on a custom Anthropic-compatible endpoint (as with DeepSeek, Kimi or Ollama):
    /// only the session's profile points it at the mock, not the test environment.
    #[test]
    #[ignore]
    fn custom_profile_runs_claude_code_on_another_endpoint() {
        if std::env::var("AGENTDECK_IT").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir(&proj).unwrap();
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let prof = db
            .save_profile(&crate::db::Profile {
                id: String::new(),
                name: "Mock DeepSeek".into(),
                kind: "anthropic".into(),
                base_url: "http://127.0.0.1:8787".into(),
                model: "deepseek-v4-pro".into(),
                small_model: "deepseek-flash".into(),
                created_at: 0,
            })
            .unwrap();
        crate::secrets::set(&prof.id, "sk-profile-key-456").unwrap();
        let s = db
            .create_session(&p.id, "claude-code", "default", "New session")
            .unwrap();
        db.set_session_profile(&s.id, Some(&prof.id)).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let mgr = Arc::new(Manager::new(
            db.clone(),
            Arc::new(move |o| {
                let _ = tx.lock().unwrap().send(o);
            }),
        ));
        let wait = |prefix: &str| -> StoredEvent {
            let deadline = Instant::now() + Duration::from_secs(60);
            while Instant::now() < deadline {
                if let Ok(Outbound::Event(e)) = rx.recv_timeout(Duration::from_millis(200)) {
                    if e.kind.starts_with(prefix) {
                        return e;
                    }
                }
            }
            panic!("timed out waiting for {prefix}");
        };
        mgr.send_message(&s.id, "please write the file").unwrap();
        let init = wait("system/init");
        assert_eq!(
            init.payload["model"], "deepseek-v4-pro",
            "the profile's model is used"
        );
        let req = wait("control_request/can_use_tool");
        let rid = req.payload["request_id"].as_str().unwrap().to_string();
        mgr.respond_permission(&s.id, &rid, Decision::Allow)
            .unwrap();
        wait("result");
        assert!(proj.join("hello.txt").exists());
        assert!(
            !db.events(&s.id)
                .unwrap()
                .iter()
                .any(|e| e.payload.to_string().contains("sk-profile-key-456")),
            "the key is never stored"
        );
        mgr.stop(&s.id);
    }

    #[test]
    #[ignore]
    fn answers_a_question_from_claude_code() {
        let Some(h) = harness() else { return };
        h.mgr
            .send_message(&h.sid, "please ask me which library to use")
            .unwrap();
        let req = h.wait_event("control_request/can_use_tool");
        assert_eq!(
            req.payload
                .pointer("/request/tool_name")
                .and_then(Value::as_str),
            Some("AskUserQuestion")
        );
        assert_eq!(h.status(), "waiting");
        let rid = req.payload["request_id"].as_str().unwrap().to_string();
        let mut answers = std::collections::HashMap::new();
        assert!(
            h.mgr.answer_question(&h.sid, &rid, Some(&answers)).is_err(),
            "an empty answer is refused"
        );
        answers.insert(
            "Which state library should the app use?".to_string(),
            vec!["Riverpod".to_string()],
        );
        h.mgr.answer_question(&h.sid, &rid, Some(&answers)).unwrap();
        let answered = h.wait_event("question_answered");
        assert_eq!(
            answered.payload["answers"]["Which state library should the app use?"][0],
            "Riverpod"
        );
        let done = h.wait_event("result");
        let reply = done.payload["result"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(
            reply.contains("Got your answer") && reply.contains("Riverpod"),
            "Claude got the answer: {reply}"
        );
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn allow_then_resume_then_deny() {
        let Some(h) = harness() else { return };
        // Turn 1: write needs approval; allow it.
        h.mgr.send_message(&h.sid, "please write the file").unwrap();
        let init = h.wait_event("system/init");
        assert!(init.payload["session_id"].is_string());
        let req = h.wait_event("control_request/can_use_tool");
        assert_eq!(h.status(), "waiting");
        let rid = req.payload["request_id"].as_str().unwrap().to_string();
        assert!(
            h.mgr
                .respond_permission(&h.sid, "not-a-real-id", Decision::Allow)
                .is_err(),
            "unknown request ids are refused"
        );
        h.mgr
            .respond_permission(&h.sid, &rid, Decision::Allow)
            .unwrap();
        assert!(
            h.mgr
                .respond_permission(&h.sid, &rid, Decision::Deny)
                .is_err(),
            "a request can only be answered once"
        );
        h.wait_event("result");
        assert_eq!(h.status(), "idle");
        let proj =
            h.db.project(&h.db.session(&h.sid).unwrap().unwrap().project_id)
                .unwrap()
                .unwrap();
        assert!(
            Path::new(&proj.path).join("hello.txt").exists(),
            "the approved write happened"
        );

        // Stop the process, then send again: AgentDeck must resume the same Claude session.
        let psid =
            h.db.session(&h.sid)
                .unwrap()
                .unwrap()
                .provider_session_id
                .unwrap();
        h.mgr.stop(&h.sid);
        h.wait_event("process_exit");
        assert!(!h.mgr.is_running(&h.sid));
        std::fs::remove_file(Path::new(&proj.path).join("hello.txt")).unwrap();
        h.mgr.send_message(&h.sid, "write it again").unwrap();
        let init2 = h.wait_event("system/init");
        assert_eq!(
            init2.payload["session_id"].as_str().unwrap(),
            psid,
            "resumed the same conversation"
        );
        let req2 = h.wait_event("control_request/can_use_tool");
        h.mgr
            .respond_permission(
                &h.sid,
                req2.payload["request_id"].as_str().unwrap(),
                Decision::Deny,
            )
            .unwrap();
        let tool_result = h.wait_event("user");
        assert_eq!(
            tool_result.payload.pointer("/message/content/0/is_error"),
            Some(&json!(true))
        );
        h.wait_event("result");
        assert!(
            !Path::new(&proj.path).join("hello.txt").exists(),
            "the denied write did not happen"
        );

        if let Ok(path) = std::env::var("AGENTDECK_DUMP") {
            std::fs::write(
                path,
                serde_json::to_string_pretty(&h.db.events(&h.sid).unwrap()).unwrap(),
            )
            .unwrap();
        }
        let kinds: Vec<String> =
            h.db.events(&h.sid)
                .unwrap()
                .into_iter()
                .map(|e| e.kind)
                .collect();
        assert!(kinds.contains(&"permission_decision".to_string()));
        assert!(
            !kinds
                .iter()
                .any(|k| k == "stream_event" || k.starts_with("control_response")),
            "noise isn't stored"
        );
        h.mgr.shutdown_all();
    }

    #[test]
    #[ignore]
    fn interrupt_stops_a_turn() {
        let Some(h) = harness() else { return };
        h.mgr.send_message(&h.sid, "be slow please").unwrap();
        h.wait_event("system/init");
        std::thread::sleep(Duration::from_millis(1500));
        h.mgr.interrupt(&h.sid).unwrap();
        let res = h.wait_event("result");
        assert!(
            h.mgr.is_running(&h.sid),
            "interrupt ends the turn, not the process"
        );
        assert!(res.payload.get("subtype").is_some());
        h.mgr.shutdown_all();
    }

    #[test]
    #[ignore]
    fn effort_is_started_read_and_changed_while_running() {
        let Some(h) = harness() else { return };
        h.db.set_session_effort(&h.sid, Some("low")).unwrap();
        h.mgr.send_message(&h.sid, "hello").unwrap();
        h.wait_event("result");
        assert_eq!(
            h.mgr.applied_effort(&h.sid).unwrap().as_deref(),
            Some("low"),
            "--effort on start"
        );
        h.mgr.set_effort(&h.sid, Some("max")).unwrap();
        assert_eq!(
            h.mgr.applied_effort(&h.sid).unwrap().as_deref(),
            Some("max"),
            "changed without a restart"
        );
        h.mgr.set_effort(&h.sid, None).unwrap();
        let back = h.mgr.applied_effort(&h.sid).unwrap();
        assert!(
            back.is_some() && back.as_deref() != Some("max"),
            "back to Claude Code's default: {back:?}"
        );
        h.mgr.send_message(&h.sid, "hello again").unwrap();
        h.wait_event("result");
        h.mgr.shutdown_all();
    }

    #[test]
    #[ignore]
    fn concurrent_sessions_stay_isolated() {
        let Some(a) = harness() else { return };
        let p = a.db.session(&a.sid).unwrap().unwrap().project_id;
        let b_sid =
            a.db.create_session(&p, "claude-code", "sonnet", "New session")
                .unwrap()
                .id;
        a.mgr.send_message(&a.sid, "hello there").unwrap();
        a.mgr.send_message(&b_sid, "please run the check").unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut done = std::collections::HashSet::new();
        while done.len() < 2 && Instant::now() < deadline {
            if let Ok(Outbound::Event(e)) = a.rx.recv_timeout(Duration::from_millis(200)) {
                if e.kind.starts_with("result") {
                    done.insert(e.session_id);
                }
            }
        }
        assert_eq!(done.len(), 2);
        let ea = a.db.events(&a.sid).unwrap();
        let eb = a.db.events(&b_sid).unwrap();
        assert!(ea.iter().all(|e| e.session_id == a.sid));
        assert!(
            eb.iter()
                .any(|e| e.payload.to_string().contains("agentdeck-ok")),
            "session B ran its own tool"
        );
        assert!(
            !ea.iter()
                .any(|e| e.payload.to_string().contains("agentdeck-ok")),
            "session A didn't see B's output"
        );
        let ia = ea.iter().find(|e| e.kind == "system/init").unwrap().payload["session_id"].clone();
        let ib = eb.iter().find(|e| e.kind == "system/init").unwrap().payload["session_id"].clone();
        assert_ne!(ia, ib);
        a.mgr.shutdown_all();
    }
}
