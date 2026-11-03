//! Codex sessions over the Codex App Server.
//!
//! Each AgentDeck session owns one `codex app-server` process (JSON-RPC 2.0 "lite", one JSON
//! object per line over stdio) started from the user's own Codex CLI in the project folder.
//! Handshake: `initialize` → `initialized` → `thread/start` (or `thread/resume` with the stored
//! thread id). Each prompt is a `turn/start`. Approval requests arrive as server requests
//! (`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, …) and are
//! answered with a JSON-RPC response to that exact request id. Codex handles its own sign-in.
//!
//! Reasoning effort is the `effort` of each `turn/start` (it sticks for later turns); what the
//! thread uses otherwise is the `reasoningEffort` in the `thread/start` answer.

use crate::claude::{
    augmented_path, first_line, read_line_bounded, Decision, Delta, Outbound, Sink,
};
use crate::db::{Db, SessionRow};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
const STDERR_TAIL_BYTES: usize = 16 * 1024;
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Notifications worth keeping in the session log. Everything else is either streamed live
/// (deltas) or bookkeeping the UI doesn't show.
const STORED: &[&str] = &[
    "thread/started",
    "turn/started",
    "turn/completed",
    "turn/plan/updated",
    "item/started",
    "item/completed",
    "error",
    "warning",
    "serverRequest/resolved",
    "model/rerouted",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pending {
    Initialize,
    ThreadStart,
    TurnStart,
    /// `account/rateLimits/read`: plan limits before the first turn. Errors are expected
    /// (API-key logins have none) and aren't shown.
    RateLimits,
    Other,
}

struct Proc {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    /// Our outstanding requests, by JSON-RPC id.
    calls: Mutex<HashMap<u64, Pending>>,
    /// Approval requests from Codex waiting for the user, by JSON-RPC id (as a string).
    approvals: Mutex<HashMap<String, Value>>,
    thread_id: Mutex<Option<String>>,
    turn_id: Mutex<Option<String>>,
    /// Prompts typed before the thread was ready.
    queued: Mutex<Vec<(String, Vec<Value>)>>,
    stopping: AtomicBool,
    /// The thread's own effort (Codex's setting), from the `thread/start` answer.
    base_effort: Mutex<Option<String>>,
    /// An effort was sent with a turn; it sticks, so going back to Default sends `base_effort`.
    effort_sent: AtomicBool,
    /// Prefix that keeps request ids unique across restarts of the same session
    /// (Codex numbers its requests from 0 in every process).
    gen: String,
    /// The usage bucket `account/rateLimits/read` answered for; updates for others are ignored.
    limit_id: Mutex<Option<String>>,
}

impl Proc {
    fn request_key(&self, id: &Value) -> String {
        let raw = match id {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        format!("{}-{raw}", self.gen)
    }

    fn write(&self, v: &Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().map_err(|_| "stdin lock poisoned")?;
        let stdin = guard.as_mut().ok_or("The agent process is shutting down")?;
        let mut line = serde_json::to_vec(v).map_err(|e| e.to_string())?;
        line.push(b'\n');
        stdin
            .write_all(&line)
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Couldn't reach Codex: {e}"))
    }

    fn request(&self, method: &str, params: Value, purpose: Pending) -> Result<u64, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut c) = self.calls.lock() {
            c.insert(id, purpose);
        }
        self.write(&json!({ "id": id, "method": method, "params": params }))?;
        Ok(id)
    }
}

pub struct CodexManager {
    db: Arc<Db>,
    sink: Sink,
    procs: Mutex<HashMap<String, Arc<Proc>>>,
    pub binary_override: Option<PathBuf>,
    pub extra_env: Vec<(String, String)>,
    /// Extra `codex app-server` arguments (tests point Codex at a mock provider with `-c`).
    pub extra_args: Vec<String>,
    /// The `agentdeck` MCP server for bot sessions (`mcp::Hub::launcher`).
    pub mcp: Option<crate::mcp::Launcher>,
}

fn user_input(text: &str) -> Value {
    json!([{ "type": "text", "text": text, "text_elements": [] }])
}

/// Shown to Codex ahead of the user's message in plan mode. The read-only sandbox is what
/// actually prevents changes; this only asks for the answer in plan form.
pub const PLAN_PREAMBLE: &str = "AgentDeck plan mode: this turn runs in a read-only sandbox. Read whatever you need, but don't try to change files. Reply with a concrete, numbered implementation plan another agent could follow.";

fn turn_params(
    thread_id: &str,
    text: &str,
    images: &[Value],
    mode: &str,
    cwd: &str,
    model: &str,
    effort: Option<&str>,
) -> Value {
    let plan = mode == "plan";
    let mut input = if plan {
        json!([
            { "type": "text", "text": PLAN_PREAMBLE, "text_elements": [] },
            { "type": "text", "text": text, "text_elements": [] }
        ])
    } else {
        user_input(text)
    };
    if let Some(items) = input.as_array_mut() {
        items.extend(images.iter().cloned());
    }
    // Sent on every turn: a turn-level sandbox override sticks for later turns.
    let sandbox = if plan {
        json!({ "type": "readOnly", "networkAccess": false })
    } else {
        json!({ "type": "workspaceWrite", "writableRoots": [cwd], "networkAccess": false, "excludeTmpdirEnvVar": false, "excludeSlashTmp": false })
    };
    let mut p = json!({ "threadId": thread_id, "input": input, "sandboxPolicy": sandbox, "approvalPolicy": "untrusted" });
    // A model picked in AgentDeck applies from the next turn on, even with the agent running.
    if !model.is_empty() && model != "default" {
        p["model"] = json!(model);
    }
    // Likewise the effort; it also sticks, so it's sent whenever one applies.
    if let Some(e) = effort.filter(|e| !e.is_empty()) {
        p["effort"] = json!(e);
    }
    p
}

impl CodexManager {
    pub fn new(db: Arc<Db>, sink: Sink) -> Self {
        Self {
            db,
            sink,
            procs: Mutex::new(HashMap::new()),
            binary_override: None,
            extra_env: Vec::new(),
            extra_args: Vec::new(),
            mcp: None,
        }
    }

    fn emit_session(&self, row: Option<SessionRow>) {
        if let Some(r) = row {
            (self.sink)(Outbound::Session(r));
        }
    }

    fn record(&self, sid: &str, source: &str, kind: &str, payload: &Value) {
        match self.db.append_event(sid, source, kind, payload) {
            Ok(Some(ev)) => (self.sink)(Outbound::Event(ev)),
            Ok(None) => {}
            Err(e) => eprintln!("agentdeck: failed to store event: {e}"),
        }
    }

    fn set_status(&self, sid: &str, status: &str, snippet: Option<&str>) {
        self.emit_session(
            self.db
                .update_session(sid, Some(status), snippet, None, None, None)
                .ok()
                .flatten(),
        );
    }

    fn proc(&self, sid: &str) -> Option<Arc<Proc>> {
        self.procs.lock().ok().and_then(|p| p.get(sid).cloned())
    }

    pub fn is_running(&self, sid: &str) -> bool {
        self.proc(sid).is_some()
    }

    pub fn pending_count(&self, sid: &str) -> usize {
        self.proc(sid)
            .and_then(|p| p.approvals.lock().ok().map(|a| a.len()))
            .unwrap_or(0)
    }

    fn binary(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.binary_override {
            return Ok(p.clone());
        }
        crate::providers::find_binary("codex").ok_or_else(|| {
            "Codex isn't installed or isn't on your PATH. Install it, run `codex login` once in a terminal, then try again.".to_string()
        })
    }

    fn spawn(self: &Arc<Self>, session: &SessionRow, cwd: &str) -> Result<Arc<Proc>, String> {
        let bin = self.binary()?;
        let mut cmd = Command::new(&bin);
        cmd.arg("app-server").args(&self.extra_args);
        cmd.current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::shellenv::apply(&mut cmd);
        cmd.env("PATH", augmented_path());
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        // A bot session: its agentdeck tools. The token and socket go in Codex's environment,
        // which passes them on to that server only (`env_vars`).
        if let Some(l) = self.mcp.as_ref().and_then(|f| f(session)) {
            cmd.args(l.codex_args());
            for (k, v) in &l.env {
                cmd.env(k, v);
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start Codex ({}): {e}", bin.display()))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        let proc = Arc::new(Proc {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            next_id: AtomicU64::new(1),
            calls: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            thread_id: Mutex::new(session.provider_session_id.clone()),
            turn_id: Mutex::new(None),
            queued: Mutex::new(Vec::new()),
            stopping: AtomicBool::new(false),
            base_effort: Mutex::new(None),
            effort_sent: AtomicBool::new(false),
            gen: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
            limit_id: Mutex::new(None),
        });
        proc.request(
            "initialize",
            json!({ "clientInfo": { "name": "agentdeck", "title": "AgentDeck", "version": env!("CARGO_PKG_VERSION") }, "capabilities": null }),
            Pending::Initialize,
        )?;

        let tail = Arc::new(Mutex::new(Vec::<u8>::new()));
        {
            let tail = tail.clone();
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
        let model = session.model.clone();
        let cwd = cwd.to_string();
        let p = proc.clone();
        std::thread::spawn(move || me.read_loop(sid, p, stdout, tail, model, cwd));
        Ok(proc)
    }

    fn start_turn(
        &self,
        sid: &str,
        proc: &Proc,
        thread_id: &str,
        text: &str,
        images: &[Value],
    ) -> Result<(), String> {
        let session = self.db.session(sid).ok().flatten();
        let mode = session
            .as_ref()
            .map(|s| s.mode.as_str())
            .unwrap_or("default");
        let cwd = session
            .as_ref()
            .and_then(|s| self.db.session_dir(s).ok().flatten())
            .unwrap_or_default();
        let picked = session.as_ref().and_then(|s| s.effort.clone());
        let effort = match picked {
            Some(e) => {
                proc.effort_sent.store(true, Ordering::SeqCst);
                Some(e)
            }
            // Back to Default after a pick: the picked effort would stick, so send Codex's own.
            None if proc.effort_sent.load(Ordering::SeqCst) => proc
                .base_effort
                .lock()
                .ok()
                .and_then(|b| b.clone())
                .or_else(|| {
                    let model = session.as_ref().map(|s| s.model.as_str()).unwrap_or("");
                    crate::models::default_effort("codex", model)
                }),
            None => None,
        };
        proc.request(
            "turn/start",
            turn_params(
                thread_id,
                text,
                images,
                mode,
                &cwd,
                session.as_ref().map(|s| s.model.as_str()).unwrap_or(""),
                effort.as_deref(),
            ),
            Pending::TurnStart,
        )
        .map(|_| ())
    }

    #[allow(clippy::too_many_arguments)]
    fn read_loop(
        self: Arc<Self>,
        sid: String,
        proc: Arc<Proc>,
        stdout: std::process::ChildStdout,
        tail: Arc<Mutex<Vec<u8>>>,
        model: String,
        cwd: String,
    ) {
        let mut reader = BufReader::with_capacity(256 * 1024, stdout);
        loop {
            match read_line_bounded(&mut reader, MAX_LINE_BYTES) {
                Ok(None) => break,
                Err(e) => {
                    self.record(&sid, "agentdeck", "error", &json!({ "title": "Lost the connection to Codex", "message": e.to_string() }));
                    break;
                }
                Ok(Some((_, true))) => {
                    self.record(&sid, "agentdeck", "error", &json!({ "title": "Skipped an oversized message", "message": "Codex sent one message larger than 16 MB. It was dropped to keep AgentDeck responsive." }));
                }
                Ok(Some((bytes, false))) => {
                    if bytes.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(v) => self.handle(&sid, &proc, v, &model, &cwd),
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
        let status = proc.child.lock().ok().and_then(|mut c| c.wait().ok());
        let code = status.and_then(|s| s.code());
        let expected = proc.stopping.load(Ordering::SeqCst);
        let tail = tail
            .lock()
            .map(|t| String::from_utf8_lossy(&t).into_owned())
            .unwrap_or_default();
        if let Ok(mut procs) = self.procs.lock() {
            if procs.get(&sid).is_some_and(|p| Arc::ptr_eq(p, &proc)) {
                procs.remove(&sid);
            }
        }
        let ok = expected || code == Some(0);
        self.record(&sid, "agentdeck", "process_exit", &json!({ "code": code, "expected": expected, "stderrTail": if ok { String::new() } else { tail } }));
        if ok {
            self.set_status(&sid, "idle", None);
        } else {
            let msg = match code {
                Some(c) => format!("Codex exited with code {c}"),
                None => "Codex was stopped by a signal".to_string(),
            };
            self.set_status(&sid, "error", Some(&msg));
        }
    }

    fn handle(&self, sid: &str, proc: &Arc<Proc>, v: Value, model: &str, cwd: &str) {
        let method = v.get("method").and_then(Value::as_str).map(str::to_owned);
        let id = v.get("id").cloned();

        // Response to one of our requests.
        if method.is_none() {
            let Some(n) = id.as_ref().and_then(Value::as_u64) else {
                return;
            };
            let purpose = proc
                .calls
                .lock()
                .ok()
                .and_then(|mut c| c.remove(&n))
                .unwrap_or(Pending::Other);
            if let Some(err) = v.get("error") {
                if purpose == Pending::RateLimits {
                    return;
                }
                let msg = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex returned an error");
                let title = match purpose {
                    Pending::Initialize => "Codex didn't start",
                    Pending::ThreadStart => "Codex couldn't open the conversation",
                    Pending::TurnStart => "Codex didn't accept the message",
                    Pending::RateLimits | Pending::Other => "Codex returned an error",
                };
                self.record(
                    sid,
                    "agentdeck",
                    "error",
                    &json!({ "title": title, "message": msg }),
                );
                if purpose != Pending::Other && purpose != Pending::RateLimits {
                    self.set_status(sid, "error", Some(&first_line(msg, 80)));
                }
                return;
            }
            match purpose {
                Pending::Initialize => {
                    let _ = proc.write(&json!({ "method": "initialized" }));
                    let existing = proc.thread_id.lock().ok().and_then(|t| t.clone());
                    let mut params = json!({ "cwd": cwd, "approvalPolicy": "untrusted", "sandbox": "workspace-write" });
                    if !model.is_empty() && model != "default" {
                        params["model"] = json!(model);
                    }
                    let res = match existing {
                        Some(tid) => {
                            params["threadId"] = json!(tid);
                            proc.request("thread/resume", params, Pending::ThreadStart)
                        }
                        None => proc.request("thread/start", params, Pending::ThreadStart),
                    };
                    if let Err(e) = res {
                        self.record(sid, "agentdeck", "error", &json!({ "title": "Codex couldn't open the conversation", "message": e }));
                    }
                    // The plan's limits, so they show before the first turn.
                    let _ =
                        proc.request("account/rateLimits/read", Value::Null, Pending::RateLimits);
                }
                Pending::ThreadStart => {
                    let tid = v
                        .pointer("/result/thread/id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    if let Ok(mut b) = proc.base_effort.lock() {
                        *b = v
                            .pointer("/result/reasoningEffort")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                    }
                    if let Some(tid) = tid {
                        if let Ok(mut t) = proc.thread_id.lock() {
                            *t = Some(tid.clone());
                        }
                        self.record(
                            sid,
                            "codex",
                            "thread/opened",
                            &json!({ "threadId": tid, "model": v.pointer("/result/model") }),
                        );
                        self.emit_session(
                            self.db
                                .update_session(sid, None, None, None, None, Some(&tid))
                                .ok()
                                .flatten(),
                        );
                        let queued: Vec<(String, Vec<Value>)> = proc
                            .queued
                            .lock()
                            .map(|mut q| q.drain(..).collect())
                            .unwrap_or_default();
                        for (text, images) in queued {
                            if let Err(e) = self.start_turn(sid, proc, &tid, &text, &images) {
                                self.record(sid, "agentdeck", "error", &json!({ "title": "Codex didn't accept the message", "message": e }));
                            }
                        }
                    }
                }
                Pending::TurnStart => {
                    if let Some(turn) = v.pointer("/result/turn/id").and_then(Value::as_str) {
                        if let Ok(mut t) = proc.turn_id.lock() {
                            *t = Some(turn.to_string());
                        }
                    }
                }
                Pending::RateLimits => {
                    if let Some(snap) = v.pointer("/result/rateLimits") {
                        if let Ok(mut l) = proc.limit_id.lock() {
                            *l = snap
                                .get("limitId")
                                .and_then(Value::as_str)
                                .map(str::to_owned);
                        }
                        let windows =
                            crate::limits::from_codex(snap, None, false, crate::db::now_ms());
                        crate::limits::record(&self.db, &self.sink, "codex", windows);
                    }
                }
                Pending::Other => {}
            }
            return;
        }

        let method = method.unwrap_or_default();
        let params = v.get("params").cloned().unwrap_or(Value::Null);

        // Server request: Codex is asking the client something.
        if let Some(id) = id {
            let rid = proc.request_key(&id);
            if method.ends_with("requestApproval")
                || method == "applyPatchApproval"
                || method == "execCommandApproval"
            {
                let in_plan = self
                    .db
                    .session(sid)
                    .ok()
                    .flatten()
                    .is_some_and(|s| s.mode == "plan");
                if in_plan {
                    // Plan mode: nothing that changes the project runs, so don't ask the user.
                    let legacy = method == "applyPatchApproval" || method == "execCommandApproval";
                    let _ = proc.write(&json!({ "id": id, "result": { "decision": if legacy { "denied" } else { "decline" } } }));
                    self.record(
                        sid,
                        "codex",
                        "approval",
                        &json!({ "requestId": rid, "method": method, "params": params }),
                    );
                    self.record(
                        sid,
                        "agentdeck",
                        "permission_decision",
                        &json!({ "requestId": rid, "decision": "deny", "auto": "plan_mode" }),
                    );
                    return;
                }
                if let Ok(mut a) = proc.approvals.lock() {
                    a.insert(
                        rid.clone(),
                        json!({ "id": id, "method": method, "params": params }),
                    );
                }
                let what = if method.contains("fileChange") || method == "applyPatchApproval" {
                    "Wants to change files"
                } else {
                    "Wants to run a command"
                };
                self.set_status(sid, "waiting", Some(what));
                self.record(
                    sid,
                    "codex",
                    "approval",
                    &json!({ "requestId": rid, "method": method, "params": params }),
                );
            } else if method == "item/tool/requestUserInput" {
                let rid = proc.request_key(&id);
                if let Ok(mut a) = proc.approvals.lock() {
                    a.insert(
                        rid.clone(),
                        json!({ "id": id, "method": method, "params": params }),
                    );
                }
                self.set_status(sid, "waiting", Some("Has a question for you"));
                self.record(
                    sid,
                    "codex",
                    "user_input",
                    &json!({ "requestId": rid, "params": params }),
                );
            } else {
                // Elicitations, dynamic tools: AgentDeck doesn't host these yet.
                let _ = proc.write(&json!({ "id": id, "error": { "code": -32601, "message": format!("AgentDeck doesn't support {method}") } }));
            }
            return;
        }

        // Notifications.
        match method.as_str() {
            "account/rateLimits/updated" => {
                if let Some(snap) = params.get("rateLimits") {
                    let base = proc.limit_id.lock().ok().and_then(|l| l.clone());
                    if crate::limits::codex_same_bucket(snap, base.as_deref()) {
                        let prev = crate::limits::previous(&self.db, "codex");
                        let windows = crate::limits::from_codex(
                            snap,
                            prev.as_deref(),
                            true,
                            crate::db::now_ms(),
                        );
                        crate::limits::record(&self.db, &self.sink, "codex", windows);
                    }
                }
                return;
            }
            "item/agentMessage/delta" => {
                if let Some(d) = params.get("delta").and_then(Value::as_str) {
                    (self.sink)(Outbound::Delta(Delta {
                        session_id: sid.into(),
                        text: d.into(),
                        reset: false,
                    }));
                }
                return;
            }
            "item/started"
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
            {
                (self.sink)(Outbound::Delta(Delta {
                    session_id: sid.into(),
                    text: String::new(),
                    reset: true,
                }));
                return;
            }
            "turn/started" => {
                if let Some(t) = params.pointer("/turn/id").and_then(Value::as_str) {
                    if let Ok(mut cur) = proc.turn_id.lock() {
                        *cur = Some(t.to_string());
                    }
                }
                self.set_status(sid, "running", Some("Working"));
            }
            "serverRequest/resolved" => {
                let rid = params
                    .get("requestId")
                    .map(|id| proc.request_key(id))
                    .unwrap_or_default();
                if let Ok(mut a) = proc.approvals.lock() {
                    a.remove(&rid);
                }
            }
            "turn/completed" => {
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                if let Ok(mut cur) = proc.turn_id.lock() {
                    *cur = None;
                }
                let last = params
                    .pointer("/turn/items")
                    .and_then(Value::as_array)
                    .and_then(|items| {
                        items
                            .iter()
                            .rev()
                            .find(|i| i.get("type").and_then(Value::as_str) == Some("agentMessage"))
                    })
                    .and_then(|i| i.get("text").and_then(Value::as_str))
                    .map(|t| first_line(t, 90));
                match status {
                    "failed" => {
                        let msg = params
                            .pointer("/turn/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("The turn failed");
                        self.set_status(sid, "error", Some(&first_line(msg, 90)));
                    }
                    "interrupted" => self.set_status(sid, "idle", Some("Stopped")),
                    _ => self.set_status(sid, "idle", Some(last.as_deref().unwrap_or("Finished"))),
                }
            }
            _ => {}
        }
        if STORED.contains(&method.as_str()) {
            let mut params = params;
            if method == "serverRequest/resolved" {
                if let Some(id) = params.get("requestId").cloned() {
                    params["requestId"] = Value::String(proc.request_key(&id));
                }
            }
            self.record(sid, "codex", &method, &params);
        }
    }

    /* ------------------------------------------------------------ commands */

    pub fn send_message(self: &Arc<Self>, sid: &str, text: &str) -> Result<(), String> {
        self.send_message_with(sid, text, &crate::attach::Attach::default())
    }

    /// Sends a message with files the user attached; images go in by local path.
    pub fn send_message_with(
        self: &Arc<Self>,
        sid: &str,
        text: &str,
        att: &crate::attach::Attach,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Write a message first".into());
        }
        let session = self
            .db
            .session(sid)
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
        let proc = match self.proc(sid) {
            Some(p) => p,
            None => {
                let p = self.spawn(&session, &dir)?;
                self.procs
                    .lock()
                    .map_err(|_| "lock poisoned")?
                    .insert(sid.to_string(), p.clone());
                p
            }
        };
        self.record(
            sid,
            "agentdeck",
            "user_message",
            &crate::claude::user_message(text, att),
        );
        let images = att.codex_images();
        let full = format!("{text}{}", att.note(true));
        let text = full.as_str();
        let title = (session.title == "New session").then(|| first_line(text, 48));
        self.emit_session(
            self.db
                .update_session(
                    sid,
                    Some("running"),
                    Some(&first_line(text, 80)),
                    title.as_deref(),
                    None,
                    None,
                )
                .ok()
                .flatten(),
        );
        // Before the thread is open (first message after start), queue; it's sent once the thread exists.
        let ready = proc
            .calls
            .lock()
            .map(|c| {
                !c.values()
                    .any(|p| matches!(p, Pending::Initialize | Pending::ThreadStart))
            })
            .unwrap_or(false);
        let tid = proc.thread_id.lock().ok().and_then(|t| t.clone());
        match (ready, tid) {
            (true, Some(tid)) => self.start_turn(sid, &proc, &tid, text, &images),
            _ => {
                proc.queued
                    .lock()
                    .map_err(|_| "lock poisoned")?
                    .push((text.to_string(), images));
                Ok(())
            }
        }
    }

    /// Answers Codex's request_user_input: chosen labels or your own text, by question id.
    /// `None` skips (an empty answer). Answers to secret questions aren't stored.
    pub fn answer_question(
        &self,
        sid: &str,
        request_id: &str,
        answers: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Result<(), String> {
        let proc = self
            .proc(sid)
            .ok_or("This session isn't running, so the question can't be answered anymore")?;
        let req = proc
            .approvals
            .lock()
            .map_err(|_| "lock poisoned")?
            .get(request_id)
            .cloned()
            .ok_or("This question is no longer waiting. The agent may have moved on or stopped.")?;
        if req.get("method").and_then(Value::as_str) != Some("item/tool/requestUserInput") {
            return Err("That request isn't a question".into());
        }
        let questions = req
            .pointer("/params/questions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = serde_json::Map::new();
        let mut shown = serde_json::Map::new();
        for q in &questions {
            let qid = q["id"].as_str().unwrap_or("").to_string();
            let chosen = answers
                .and_then(|a| a.get(&qid))
                .cloned()
                .unwrap_or_default();
            out.insert(qid.clone(), json!({ "answers": chosen }));
            if q["isSecret"].as_bool() != Some(true) {
                shown.insert(qid, json!(chosen));
            }
        }
        if answers.is_some()
            && out
                .values()
                .all(|v| v["answers"].as_array().map_or(true, |a| a.is_empty()))
        {
            return Err("Pick or write an answer first".into());
        }
        proc.write(&json!({ "id": req["id"], "result": { "answers": out } }))?;
        if let Ok(mut a) = proc.approvals.lock() {
            a.remove(request_id);
        }
        self.record(
            sid,
            "agentdeck",
            "question_answered",
            &json!({ "requestId": request_id, "answers": answers.map(|_| Value::Object(shown)), "skipped": answers.is_none() }),
        );
        self.set_status(sid, "running", Some("Working"));
        Ok(())
    }

    pub fn respond_permission(
        &self,
        sid: &str,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), String> {
        let proc = self
            .proc(sid)
            .ok_or("This session isn't running, so the request can't be answered anymore")?;
        let req = proc
            .approvals
            .lock()
            .map_err(|_| "lock poisoned")?
            .get(request_id)
            .cloned()
            .ok_or("This request is no longer waiting. The agent may have moved on or stopped.")?;
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let legacy = method == "applyPatchApproval" || method == "execCommandApproval";
        let available: Vec<Value> = req
            .pointer("/params/availableDecisions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let decision_value = match decision {
            Decision::Allow => json!(if legacy { "approved" } else { "accept" }),
            Decision::AllowSession => {
                // Only Codex's own session-scoped option; persistent exec-policy amendments are not offered.
                let ok = legacy || available.iter().any(|d| d == "acceptForSession");
                if !ok || method.contains("permissions") {
                    return Err(
                        "Codex didn't offer a session-wide approval for this request".into(),
                    );
                }
                json!(if legacy {
                    "approved_for_session"
                } else {
                    "acceptForSession"
                })
            }
            Decision::Deny => json!(if legacy { "denied" } else { "decline" }),
        };
        proc.write(&json!({ "id": req["id"], "result": { "decision": decision_value } }))?;
        if let Ok(mut a) = proc.approvals.lock() {
            a.remove(request_id);
        }
        let d = match decision {
            Decision::Allow => "allow",
            Decision::AllowSession => "allow_session",
            Decision::Deny => "deny",
        };
        self.record(
            sid,
            "agentdeck",
            "permission_decision",
            &json!({ "requestId": request_id, "decision": d }),
        );
        let still = proc
            .approvals
            .lock()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        self.set_status(
            sid,
            if still { "waiting" } else { "running" },
            Some("Working"),
        );
        Ok(())
    }

    /// The effort Codex uses for this session's next turn, while it runs: the one picked here,
    /// else the thread's own. None when not running or Codex didn't say.
    pub fn current_effort(&self, sid: &str) -> Option<String> {
        let proc = self.proc(sid)?;
        self.db
            .session(sid)
            .ok()
            .flatten()
            .and_then(|s| s.effort)
            .or_else(|| proc.base_effort.lock().ok().and_then(|b| b.clone()))
    }

    pub fn interrupt(&self, sid: &str) -> Result<(), String> {
        let proc = self.proc(sid).ok_or("This session isn't running")?;
        let tid = proc
            .thread_id
            .lock()
            .ok()
            .and_then(|t| t.clone())
            .ok_or("Codex hasn't opened the conversation yet")?;
        let turn = proc
            .turn_id
            .lock()
            .ok()
            .and_then(|t| t.clone())
            .ok_or("Codex isn't working on anything right now")?;
        proc.request(
            "turn/interrupt",
            json!({ "threadId": tid, "turnId": turn }),
            Pending::Other,
        )?;
        self.record(sid, "agentdeck", "interrupt_requested", &json!({}));
        Ok(())
    }

    pub fn stop(&self, sid: &str) {
        if let Some(proc) = self.proc(sid) {
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
}

/// End-to-end tests against the real `codex app-server` pointed at `scripts/mock_openai.py`.
/// Run with: python3 scripts/mock_openai.py 8788 &  AGENTDECK_IT=1 cargo test codex -- --ignored
#[cfg(test)]
mod integration {
    use super::*;
    use crate::db::StoredEvent;
    use std::sync::mpsc;
    use std::time::Instant;

    struct Harness {
        db: Arc<Db>,
        mgr: Arc<CodexManager>,
        rx: mpsc::Receiver<Outbound>,
        sid: String,
        proj: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn harness() -> Option<Harness> {
        std::env::var("AGENTDECK_IT").ok()?;
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir(&proj).unwrap();
        let home = dir.path().join("codexhome");
        std::fs::create_dir(&home).unwrap();
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let s = db
            .create_session(&p.id, "codex", "default", "New session")
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| {
            let _ = tx.lock().unwrap().send(o);
        });
        let mut mgr = CodexManager::new(db.clone(), sink);
        let url = std::env::var("AGENTDECK_OPENAI_MOCK_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8788/v1".into());
        mgr.extra_env = vec![
            ("CODEX_HOME".into(), home.to_string_lossy().into_owned()),
            ("MOCK_OPENAI_KEY".into(), "test-key".into()),
        ];
        mgr.extra_args = vec![
            "-c".into(),
            "model_provider=\"agentdeck_mock\"".into(),
            "-c".into(),
            "model=\"mock-model\"".into(),
            "-c".into(),
            format!("model_providers.agentdeck_mock={{name=\"Mock\", base_url=\"{url}\", env_key=\"MOCK_OPENAI_KEY\", wire_api=\"responses\"}}"),
        ];
        Some(Harness {
            db,
            mgr: Arc::new(mgr),
            rx,
            sid: s.id,
            proj,
            _dir: dir,
        })
    }

    impl Harness {
        fn wait(&self, kind: &str) -> StoredEvent {
            let deadline = Instant::now() + Duration::from_secs(60);
            while Instant::now() < deadline {
                if let Ok(Outbound::Event(e)) = self.rx.recv_timeout(Duration::from_millis(200)) {
                    if e.kind == kind {
                        return e;
                    }
                }
            }
            panic!("timed out waiting for {kind}");
        }
        fn status(&self) -> String {
            self.db.session(&self.sid).unwrap().unwrap().status
        }
    }

    #[test]
    #[ignore]
    fn dropped_image_reaches_codex_as_a_local_image() {
        use base64::Engine as _;
        let Some(h) = harness() else { return };
        let png = h.proj.join("shot.png");
        std::fs::write(&png, base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==").unwrap()).unwrap();
        let att = crate::attach::resolve(
            &[png.to_string_lossy().into_owned()],
            &h.proj.to_string_lossy(),
        )
        .unwrap();
        h.mgr
            .send_message_with(&h.sid, "what is in this picture", &att)
            .unwrap();
        let done = h.wait("turn/completed");
        let text = done
            .payload
            .pointer("/turn/items")
            .and_then(Value::as_array)
            .and_then(|i| i.iter().rev().find(|x| x["type"] == "agentMessage"))
            .and_then(|x| x["text"].as_str())
            .unwrap_or("")
            .to_string();
        assert_eq!(text, "I can see 1 image.");
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn a_model_picked_while_codex_runs_is_used_from_the_next_turn() {
        let Some(h) = harness() else { return };
        h.mgr.send_message(&h.sid, "hello").unwrap();
        h.wait("turn/completed");
        h.db.update_session(&h.sid, None, None, None, Some("mock-model-b"), None)
            .unwrap();
        let _ = std::fs::remove_file("/tmp/mock_openai_requests.jsonl");
        h.mgr.send_message(&h.sid, "hello again").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let log =
                std::fs::read_to_string("/tmp/mock_openai_requests.jsonl").unwrap_or_default();
            if log.contains("\"model\": \"mock-model-b\"")
                || log.contains("\"model\":\"mock-model-b\"")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the next turn used the new model: {log}"
            );
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    /// The `reasoning.effort` of the last Responses request Codex sent the mock.
    fn last_effort() -> Option<String> {
        let log = std::fs::read_to_string("/tmp/mock_openai_requests.jsonl").unwrap_or_default();
        log.lines()
            .rev()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter_map(|l| l.get("body").cloned())
            .find(|b| b.get("input").is_some())
            .and_then(|b| {
                b.pointer("/reasoning/effort")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }

    #[test]
    #[ignore]
    fn an_effort_picked_while_codex_runs_is_used_from_the_next_turn() {
        let Some(h) = harness() else { return };
        h.mgr.send_message(&h.sid, "hello").unwrap();
        h.wait("turn/completed");
        let base = h.mgr.current_effort(&h.sid);
        eprintln!("thread effort: {base:?}, sent: {:?}", last_effort());
        h.db.set_session_effort(&h.sid, Some("high")).unwrap();
        h.mgr.send_message(&h.sid, "hello again").unwrap();
        h.wait("turn/completed");
        assert_eq!(last_effort().as_deref(), Some("high"));
        assert_eq!(h.mgr.current_effort(&h.sid).as_deref(), Some("high"));
        h.db.set_session_effort(&h.sid, None).unwrap();
        h.mgr.send_message(&h.sid, "and back").unwrap();
        h.wait("turn/completed");
        if let Some(b) = base {
            assert_eq!(last_effort(), Some(b), "back to the thread's own effort");
        }
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn codex_plan_mode_declines_without_asking() {
        let Some(h) = harness() else { return };
        h.db.set_session_mode(&h.sid, "plan").unwrap();
        h.mgr.send_message(&h.sid, "please run the check").unwrap();
        let d = h.wait("permission_decision");
        assert_eq!(d.payload["auto"], "plan_mode");
        assert_eq!(d.payload["decision"], "deny");
        h.wait("turn/completed");
        let proj =
            h.db.project(&h.db.session(&h.sid).unwrap().unwrap().project_id)
                .unwrap()
                .unwrap();
        assert!(!Path::new(&proj.path).join("marker.txt").exists());
        let log = std::fs::read_to_string("/tmp/mock_openai_requests.jsonl").unwrap_or_default();
        assert!(
            log.contains("AgentDeck plan mode"),
            "the plan preamble reached the model"
        );
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn codex_accept_resume_decline() {
        let Some(h) = harness() else { return };
        h.mgr.send_message(&h.sid, "please run the check").unwrap();
        let opened = h.wait("thread/opened");
        let tid = opened.payload["threadId"].as_str().unwrap().to_string();
        let approval = h.wait("approval");
        assert_eq!(h.status(), "waiting");
        let rid = approval.payload["requestId"].as_str().unwrap().to_string();
        assert!(h
            .mgr
            .respond_permission(&h.sid, "999", Decision::Allow)
            .is_err());
        h.mgr
            .respond_permission(&h.sid, &rid, Decision::Allow)
            .unwrap();
        assert!(
            h.mgr
                .respond_permission(&h.sid, &rid, Decision::Allow)
                .is_err(),
            "answered once"
        );
        h.wait("turn/completed");
        assert_eq!(h.status(), "idle");
        assert!(
            h.proj.join("marker.txt").exists(),
            "the approved command ran"
        );

        h.mgr.stop(&h.sid);
        h.wait("process_exit");
        assert_eq!(
            h.db.session(&h.sid)
                .unwrap()
                .unwrap()
                .provider_session_id
                .as_deref(),
            Some(tid.as_str())
        );
        std::fs::remove_file(h.proj.join("marker.txt")).unwrap();
        h.mgr.send_message(&h.sid, "run it again").unwrap();
        let reopened = h.wait("thread/opened");
        assert_eq!(
            reopened.payload["threadId"].as_str().unwrap(),
            tid,
            "resumed the same thread"
        );
        let a2 = h.wait("approval");
        h.mgr
            .respond_permission(
                &h.sid,
                a2.payload["requestId"].as_str().unwrap(),
                Decision::Deny,
            )
            .unwrap();
        h.wait("turn/completed");
        assert!(
            !h.proj.join("marker.txt").exists(),
            "the declined command did not run"
        );

        if let Ok(path) = std::env::var("AGENTDECK_DUMP_CODEX") {
            std::fs::write(
                path,
                serde_json::to_string_pretty(&h.db.events(&h.sid).unwrap()).unwrap(),
            )
            .unwrap();
        }
        h.mgr.shutdown_all();
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn turns_carry_the_picked_model_and_effort() {
        let p = turn_params("t", "hi", &[], "default", "/p", "default", None);
        assert!(
            p.get("model").is_none() && p.get("effort").is_none(),
            "defaults are left to Codex"
        );
        let p = turn_params(
            "t",
            "hi",
            &[],
            "default",
            "/p",
            "gpt-6.1-sol",
            Some("xhigh"),
        );
        assert_eq!(p["model"], "gpt-6.1-sol");
        assert_eq!(p["effort"], "xhigh");
        assert_eq!(p["sandboxPolicy"]["type"], "workspaceWrite");
    }
}
