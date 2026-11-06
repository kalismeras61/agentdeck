//! Sessions with agents that speak the Agent Client Protocol (ACP) over stdio: Gemini CLI
//! (`gemini --acp`) and Grok Build (`grok agent stdio`).
//!
//! JSON-RPC 2.0, one message per line. Handshake: `initialize` → `session/new` (or
//! `session/load` with the stored session id when the agent supports it) → `session/prompt` per
//! message. The agent streams `session/update` notifications and asks before tool calls with
//! `session/request_permission`, which is answered with one of the options it offered.
//! AgentDeck advertises no file-system or terminal capabilities, so the agent does its own I/O
//! under its own rules. Sign-in stays with the agent's CLI.

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
const STDERR_TAIL_BYTES: usize = 8 * 1024;
const STOP_GRACE: Duration = Duration::from_secs(3);

/// How AgentDeck starts each ACP agent.
pub struct AgentSpec {
    pub provider: &'static str,
    pub label: &'static str,
    pub binary: &'static str,
    pub args: &'static [&'static str],
    pub sign_in: &'static str,
    /// Environment variable for an API key saved in AgentDeck, and the ACP auth method that uses it.
    pub key_env: &'static str,
    pub key_method: &'static str,
}

/// Keychain name of an agent's API key saved in AgentDeck.
pub fn key_name(provider: &str) -> String {
    format!("agent:{provider}")
}

pub const AGENTS: &[AgentSpec] = &[
    AgentSpec {
        provider: "gemini-cli",
        label: "Gemini CLI",
        binary: "gemini",
        args: &["--acp"],
        sign_in: "Gemini CLI couldn't sign in. AgentDeck starts it with the same setup as your Terminal, so run `gemini` there: if it works in Terminal, restart AgentDeck and try again. If Terminal shows the same message, Google has ended sign-in for that account type; a Gemini API key (Settings → Providers → Gemini CLI) or a Google Cloud project works instead.",
        key_env: "GEMINI_API_KEY",
        key_method: "gemini-api-key",
    },
    AgentSpec {
        provider: "grok-build",
        label: "Grok Build",
        binary: "grok",
        args: &["agent", "stdio"],
        sign_in: "Run `grok` once in a terminal and sign in, or add an xAI API key in Settings → Providers → Grok Build, then try again.",
        key_env: "XAI_API_KEY",
        key_method: "xai-api-key",
    },
    AgentSpec {
        provider: "kimi-code",
        label: "Kimi Code",
        binary: "kimi",
        args: &["acp"],
        sign_in: "Kimi Code isn't signed in. Run `kimi login` in a terminal (or Sign in below), then try again.",
        key_env: "",
        key_method: "",
    },
];

pub fn spec(provider: &str) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|a| a.provider == provider)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pending {
    Initialize,
    Authenticate,
    OpenSession,
    Prompt,
    Other,
}

struct Proc {
    /// Signs in with the API key saved in AgentDeck.
    use_key: AtomicBool,
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    calls: Mutex<HashMap<u64, Pending>>,
    /// Permission requests waiting for the user, by AgentDeck request key.
    approvals: Mutex<HashMap<String, Value>>,
    session_id: Mutex<Option<String>>,
    /// Ids of the modes the agent offers ("plan", …).
    modes: Mutex<Vec<String>>,
    /// Ids of the models the agent lets clients pick.
    models: Mutex<Vec<String>>,
    provider: String,
    load_session: AtomicBool,
    /// While `session/load` replays history, its updates are already in our store.
    replaying: AtomicBool,
    /// When the last session/update arrived (ms since the epoch), to tell when a replay is over.
    last_update: AtomicU64,
    prompting: AtomicBool,
    queued: Mutex<Vec<String>>,
    /// Agent text of the current message, flushed as one stored event.
    text: Mutex<String>,
    /// Last flushed agent text in this turn (what a relay passes on).
    last_text: Mutex<String>,
    stopping: AtomicBool,
    gen: String,
    /// The `mcpServers` list for `session/new` and `session/load`: the agentdeck tools for a
    /// bot session, else none.
    mcp_servers: Value,
}

impl Proc {
    fn key(&self, id: &Value) -> String {
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
            .map_err(|e| format!("Couldn't reach the agent: {e}"))
    }

    fn request(&self, method: &str, params: Value, purpose: Pending) -> Result<u64, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut c) = self.calls.lock() {
            c.insert(id, purpose);
        }
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        Ok(id)
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    fn sid(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|s| s.clone())
    }

    fn has_mode(&self, id: &str) -> bool {
        self.modes
            .lock()
            .map(|m| m.iter().any(|x| x == id))
            .unwrap_or(false)
    }
}

pub struct AcpManager {
    db: Arc<Db>,
    sink: Sink,
    procs: Mutex<HashMap<String, Arc<Proc>>>,
    /// Test hooks: explicit binary per provider and extra environment.
    pub binary_override: HashMap<String, PathBuf>,
    pub extra_env: Vec<(String, String)>,
    /// The `agentdeck` MCP server for bot sessions (`mcp::Hub::launcher`).
    pub mcp: Option<crate::mcp::Launcher>,
}

impl AcpManager {
    pub fn new(db: Arc<Db>, sink: Sink) -> Self {
        Self {
            db,
            sink,
            procs: Mutex::new(HashMap::new()),
            binary_override: HashMap::new(),
            extra_env: Vec::new(),
            mcp: None,
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
        if let Ok(Some(r)) = self
            .db
            .update_session(sid, Some(status), snippet, None, None, None)
        {
            (self.sink)(Outbound::Session(r));
        }
    }

    fn proc(&self, sid: &str) -> Option<Arc<Proc>> {
        self.procs.lock().ok().and_then(|p| p.get(sid).cloned())
    }

    pub fn is_running(&self, sid: &str) -> bool {
        self.proc(sid).is_some()
    }

    fn in_plan(&self, sid: &str) -> bool {
        self.db
            .session(sid)
            .ok()
            .flatten()
            .is_some_and(|s| s.mode == "plan")
    }

    fn binary(&self, spec: &AgentSpec) -> Result<PathBuf, String> {
        if let Some(p) = self.binary_override.get(spec.provider) {
            return Ok(p.clone());
        }
        crate::providers::find_binary(spec.binary).ok_or_else(|| {
            format!(
                "{} isn't installed or isn't on your PATH. {}",
                spec.label, spec.sign_in
            )
        })
    }

    fn spawn(self: &Arc<Self>, session: &SessionRow, cwd: &str) -> Result<Arc<Proc>, String> {
        let spec = spec(&session.provider).ok_or("Unknown agent")?;
        let bin = self.binary(spec)?;
        let mut cmd = Command::new(&bin);
        cmd.args(spec.args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PATH", augmented_path());
        // The same environment the agent gets in Terminal (sign-in settings, proxies, keys).
        crate::shellenv::apply(&mut cmd);
        // An API key saved in AgentDeck (Keychain) goes to the agent's own variable.
        let key = (!spec.key_env.is_empty())
            .then(|| crate::secrets::get(&key_name(spec.provider)))
            .flatten()
            .filter(|k| !k.trim().is_empty());
        if let Some(k) = &key {
            cmd.env(spec.key_env, k);
            // Gemini CLI signs in at startup with whatever its settings name (a personal Google
            // login, which Google no longer serves to this client), before ACP starts. This
            // override picks the API key for this run only; the user's settings stay as they are.
            if spec.provider == "gemini-cli" {
                cmd.env("GEMINI_CLI_AUTH_OVERRIDE", spec.key_method);
            }
        }
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        // A bot session gets the agentdeck tools (in `session/new`). Gemini CLI starts a stdio MCP
        // server only in a folder it trusts: a bot's own workspace (made by AgentDeck) is trusted
        // for it; in a project, the user's own Gemini CLI trust settings decide.
        let mcp = self.mcp.as_ref().and_then(|f| f(session));
        if spec.provider == "gemini-cli" && mcp.as_ref().is_some_and(|l| l.own_folder) {
            cmd.env("GEMINI_CLI_TRUST_WORKSPACE", "true");
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start {} ({}): {e}", spec.label, bin.display()))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        let proc = Arc::new(Proc {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            next_id: AtomicU64::new(1),
            calls: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            session_id: Mutex::new(None),
            modes: Mutex::new(Vec::new()),
            models: Mutex::new(Vec::new()),
            provider: spec.provider.to_string(),
            load_session: AtomicBool::new(false),
            replaying: AtomicBool::new(false),
            last_update: AtomicU64::new(0),
            prompting: AtomicBool::new(false),
            queued: Mutex::new(Vec::new()),
            text: Mutex::new(String::new()),
            last_text: Mutex::new(String::new()),
            stopping: AtomicBool::new(false),
            use_key: AtomicBool::new(key.is_some()),
            gen: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
            mcp_servers: json!(mcp.map(|l| l.acp_servers()).unwrap_or_default()),
        });
        proc.request(
            "initialize",
            json!({ "protocolVersion": 1, "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false }, "clientInfo": { "name": "agentdeck", "title": "AgentDeck", "version": env!("CARGO_PKG_VERSION") } }),
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
        let resume = session.provider_session_id.clone();
        let cwd = cwd.to_string();
        let p = proc.clone();
        std::thread::spawn(move || me.read_loop(sid, p, stdout, tail, resume, cwd));
        Ok(proc)
    }

    /// Opens the agent's session: picks up the earlier one when the agent can, else a new one.
    fn open_session(&self, sid: &str, proc: &Proc, resume: Option<&str>, cwd: &str) {
        let can_load = proc.load_session.load(Ordering::SeqCst);
        let res = match resume {
            Some(old) if can_load => {
                if let Ok(mut s) = proc.session_id.lock() {
                    *s = Some(old.to_string());
                }
                proc.replaying.store(true, Ordering::SeqCst);
                proc.request(
                    "session/load",
                    json!({ "sessionId": old, "cwd": cwd, "mcpServers": proc.mcp_servers }),
                    Pending::OpenSession,
                )
            }
            _ => proc.request(
                "session/new",
                json!({ "cwd": cwd, "mcpServers": proc.mcp_servers }),
                Pending::OpenSession,
            ),
        };
        if let Err(e) = res {
            self.record(
                sid,
                "agentdeck",
                "error",
                &json!({ "title": "The agent couldn't open the session", "message": e }),
            );
        }
    }

    fn read_loop(
        self: Arc<Self>,
        sid: String,
        proc: Arc<Proc>,
        stdout: std::process::ChildStdout,
        tail: Arc<Mutex<Vec<u8>>>,
        resume: Option<String>,
        cwd: String,
    ) {
        let mut reader = BufReader::with_capacity(256 * 1024, stdout);
        loop {
            match read_line_bounded(&mut reader, MAX_LINE_BYTES) {
                Ok(None) => break,
                Err(e) => {
                    self.record(&sid, "agentdeck", "error", &json!({ "title": "Lost the connection to the agent", "message": e.to_string() }));
                    break;
                }
                Ok(Some((_, true))) => {
                    self.record(&sid, "agentdeck", "error", &json!({ "title": "Skipped an oversized message", "message": "The agent sent one message larger than 16 MB. It was dropped to keep AgentDeck responsive." }));
                }
                Ok(Some((bytes, false))) => {
                    if bytes.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(v) => self.handle(&sid, &proc, v, resume.as_deref(), &cwd),
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
        self.flush_text(&sid, &proc);
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
                Some(c) => format!("The agent exited with code {c}"),
                None => "The agent was stopped by a signal".to_string(),
            };
            self.set_status(&sid, "error", Some(&msg));
        }
    }

    /// Stores the agent text gathered so far as one message.
    fn flush_text(&self, sid: &str, proc: &Proc) {
        let text = proc
            .text
            .lock()
            .map(|mut t| std::mem::take(&mut *t))
            .unwrap_or_default();
        if text.trim().is_empty() {
            return;
        }
        if let Ok(mut l) = proc.last_text.lock() {
            l.clone_from(&text);
        }
        self.record(sid, "acp", "agent_message", &json!({ "text": text }));
    }

    fn start_prompt(&self, sid: &str, proc: &Proc, text: &str) -> Result<(), String> {
        let session_id = proc
            .sid()
            .ok_or("The agent hasn't opened the session yet")?;
        proc.prompting.store(true, Ordering::SeqCst);
        if let Ok(mut l) = proc.last_text.lock() {
            l.clear();
        }
        proc.request(
            "session/prompt",
            json!({ "sessionId": session_id, "prompt": [{ "type": "text", "text": text }] }),
            Pending::Prompt,
        )
        .map(|_| ())
        .inspect_err(|_| {
            proc.prompting.store(false, Ordering::SeqCst);
            let _ = sid;
        })
    }

    /// Sends what the user typed while the session was opening, as one message.
    fn send_queued(&self, sid: &str, proc: &Proc) {
        let queued: Vec<String> = proc
            .queued
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default();
        if queued.is_empty() {
            return;
        }
        if let Err(e) = self.start_prompt(sid, proc, &queued.join("\n\n")) {
            self.record(
                sid,
                "agentdeck",
                "error",
                &json!({ "title": "The agent didn't accept the message", "message": e }),
            );
        }
    }

    fn apply_mode(&self, sid: &str, proc: &Proc) {
        let want = if self.in_plan(sid) { "plan" } else { "default" };
        if proc.has_mode(want) {
            if let Some(s) = proc.sid() {
                let _ = proc.request(
                    "session/set_mode",
                    json!({ "sessionId": s, "modeId": want }),
                    Pending::Other,
                );
            }
        }
    }

    /// Picks the session's model when the agent lets clients choose one.
    fn apply_model(&self, sid: &str, proc: &Proc) {
        let Some(model) = self.db.session(sid).ok().flatten().map(|s| s.model) else {
            return;
        };
        if model.is_empty() || model == "default" {
            return;
        }
        let offered = proc
            .models
            .lock()
            .map(|m| m.iter().any(|x| x == &model))
            .unwrap_or(false);
        if let (true, Some(s)) = (offered, proc.sid()) {
            let _ = proc.request(
                "session/set_model",
                json!({ "sessionId": s, "modelId": model }),
                Pending::Other,
            );
        }
    }

    fn handle(
        self: &Arc<Self>,
        sid: &str,
        proc: &Arc<Proc>,
        v: Value,
        resume: Option<&str>,
        cwd: &str,
    ) {
        let method = v.get("method").and_then(Value::as_str).map(str::to_owned);
        let id = v.get("id").cloned();

        // A response to one of our requests.
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
                let raw = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("The agent returned an error");
                // Agents often pass the API's JSON error through; show its message.
                let inner = serde_json::from_str::<Value>(raw.trim())
                    .ok()
                    .and_then(|j| {
                        j.pointer("/error/message")
                            .or_else(|| j.get("message"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    });
                let msg = inner.as_deref().unwrap_or(raw);
                // The agent couldn't reopen its earlier conversation (e.g. it was never saved
                // because the first turn failed): start a new one instead of stopping here.
                if purpose == Pending::OpenSession && proc.replaying.swap(false, Ordering::SeqCst) {
                    if let Ok(mut s) = proc.session_id.lock() {
                        *s = None;
                    }
                    self.open_session(sid, proc, None, cwd);
                    return;
                }
                let auth = err.get("code").and_then(Value::as_i64) == Some(-32000)
                    || msg.to_lowercase().contains("auth");
                let provider = self
                    .db
                    .session(sid)
                    .ok()
                    .flatten()
                    .map(|s| s.provider)
                    .unwrap_or_default();
                let hint = spec(&provider).map(|s| s.sign_in).unwrap_or("");
                let (title, message) = match purpose {
                    _ if auth => (
                        "The agent needs you to sign in".to_string(),
                        format!("{msg}. {hint}"),
                    ),
                    Pending::Initialize => ("The agent didn't start".into(), msg.to_string()),
                    Pending::Authenticate => (
                        "The agent didn't accept the API key".into(),
                        format!("{msg}. Check the key in Settings → Providers."),
                    ),
                    Pending::OpenSession => (
                        "The agent couldn't open the session".into(),
                        msg.to_string(),
                    ),
                    Pending::Prompt => (
                        "The agent didn't accept the message".into(),
                        msg.to_string(),
                    ),
                    Pending::Other => ("The agent returned an error".into(), msg.to_string()),
                };
                if purpose == Pending::Prompt {
                    proc.prompting.store(false, Ordering::SeqCst);
                    self.flush_text(sid, proc);
                    self.record(
                        sid,
                        "acp",
                        "turn/ended",
                        &json!({ "stopReason": "error", "error": msg }),
                    );
                }
                self.record(
                    sid,
                    "agentdeck",
                    "error",
                    &json!({ "title": title, "message": message }),
                );
                if purpose != Pending::Other {
                    self.set_status(sid, "error", Some(&first_line(&title, 80)));
                }
                return;
            }
            let result = v.get("result").cloned().unwrap_or(Value::Null);
            match purpose {
                Pending::Initialize => {
                    let can_load = result
                        .pointer("/agentCapabilities/loadSession")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    proc.load_session.store(can_load, Ordering::SeqCst);
                    self.record(
                        sid,
                        "acp",
                        "initialized",
                        &json!({ "agentInfo": result.get("agentInfo"), "loadSession": can_load }),
                    );
                    // With a key saved in AgentDeck, sign in with it instead of the agent's own login.
                    let provider = self
                        .db
                        .session(sid)
                        .ok()
                        .flatten()
                        .map(|s| s.provider)
                        .unwrap_or_default();
                    let method = spec(&provider).map(|s| s.key_method).unwrap_or("");
                    let offered = result
                        .get("authMethods")
                        .and_then(Value::as_array)
                        .is_some_and(|m| m.iter().any(|x| x["id"] == method));
                    // Gemini CLI already signed in with the key at startup (see spawn); signing in again
                    // over ACP would rewrite the user's own Gemini settings.
                    if proc.use_key.load(Ordering::SeqCst) && offered && provider != "gemini-cli" {
                        if let Err(e) = proc.request(
                            "authenticate",
                            json!({ "methodId": method }),
                            Pending::Authenticate,
                        ) {
                            self.record(
                                sid,
                                "agentdeck",
                                "error",
                                &json!({ "title": "The agent didn't start", "message": e }),
                            );
                        }
                        return;
                    }
                    self.open_session(sid, proc, resume, cwd);
                }
                Pending::Authenticate => {
                    self.open_session(sid, proc, resume, cwd);
                }
                Pending::OpenSession => {
                    let resumed = proc.replaying.load(Ordering::SeqCst);
                    if let Some(new_id) = result.get("sessionId").and_then(Value::as_str) {
                        if let Ok(mut s) = proc.session_id.lock() {
                            *s = Some(new_id.to_string());
                        }
                    }
                    let modes: Vec<String> = result
                        .pointer("/modes/availableModes")
                        .and_then(Value::as_array)
                        .map(|m| {
                            m.iter()
                                .filter_map(|x| x["id"].as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Ok(mut m) = proc.modes.lock() {
                        *m = modes.clone();
                    }
                    let models = crate::models::from_acp(
                        &result,
                        spec(&proc.provider).map(|s| s.label).unwrap_or("The agent"),
                    );
                    if let Ok(mut m) = proc.models.lock() {
                        *m = models.iter().map(|x| x.id.clone()).collect();
                    }
                    crate::models::remember(&self.db, &proc.provider, &models);
                    let Some(session_id) = proc.sid() else { return };
                    self.record(
                        sid,
                        "acp",
                        "session/opened",
                        &json!({ "sessionId": session_id, "resumed": resumed, "modes": modes }),
                    );
                    if let Ok(Some(r)) =
                        self.db
                            .update_session(sid, None, None, None, None, Some(&session_id))
                    {
                        (self.sink)(Outbound::Session(r));
                    }
                    self.apply_mode(sid, proc);
                    self.apply_model(sid, proc);
                    if resumed {
                        // Some agents answer session/load while the history replay is still
                        // streaming. Wait until it goes quiet, then send what the user typed.
                        let (me, sid, proc) = (self.clone(), sid.to_string(), proc.clone());
                        std::thread::spawn(move || {
                            let start = crate::db::now_ms();
                            loop {
                                std::thread::sleep(Duration::from_millis(100));
                                let now = crate::db::now_ms();
                                let quiet =
                                    now - proc.last_update.load(Ordering::SeqCst) as i64 >= 400;
                                if quiet || now - start > 5_000 {
                                    break;
                                }
                            }
                            proc.replaying.store(false, Ordering::SeqCst);
                            me.send_queued(&sid, &proc);
                        });
                    } else {
                        self.send_queued(sid, proc);
                    }
                }
                Pending::Prompt => {
                    proc.prompting.store(false, Ordering::SeqCst);
                    self.flush_text(sid, proc);
                    let stop = result
                        .get("stopReason")
                        .and_then(Value::as_str)
                        .unwrap_or("end_turn")
                        .to_string();
                    let last = proc.last_text.lock().map(|t| t.clone()).unwrap_or_default();
                    match stop.as_str() {
                        "cancelled" => self.set_status(sid, "idle", Some("Stopped")),
                        "refusal" => {
                            self.set_status(sid, "error", Some("The agent refused this request"))
                        }
                        _ => self.set_status(
                            sid,
                            "idle",
                            Some(&if last.is_empty() {
                                "Finished".into()
                            } else {
                                first_line(&last, 90)
                            }),
                        ),
                    }
                    self.record(
                        sid,
                        "acp",
                        "turn/ended",
                        &json!({ "stopReason": stop, "lastText": last }),
                    );
                    // Messages sent while the agent was busy go next.
                    let queued: Vec<String> = proc
                        .queued
                        .lock()
                        .map(|mut q| q.drain(..).collect())
                        .unwrap_or_default();
                    if !queued.is_empty() {
                        let _ = self.start_prompt(sid, proc, &queued.join("\n\n"));
                        self.set_status(sid, "running", Some("Working"));
                    }
                }
                Pending::Other => {}
            }
            return;
        }
        let method = method.unwrap_or_default();
        let params = v.get("params").cloned().unwrap_or(Value::Null);

        // A request from the agent.
        if let Some(id) = id {
            if method == "session/request_permission" {
                self.flush_text(sid, proc);
                let rid = proc.key(&id);
                let options = params.get("options").cloned().unwrap_or(json!([]));
                if self.in_plan(sid) && !proc.has_mode("plan") {
                    // AgentDeck enforces plan mode for agents without one: refuse without asking.
                    let reject = pick(&options, &["reject_once", "reject_always"]);
                    let outcome = match reject {
                        Some(o) => json!({ "outcome": "selected", "optionId": o }),
                        None => json!({ "outcome": "cancelled" }),
                    };
                    let _ = proc.write(
                        &json!({ "jsonrpc": "2.0", "id": id, "result": { "outcome": outcome } }),
                    );
                    self.record(sid, "acp", "approval", &json!({ "requestId": rid, "toolCall": params.get("toolCall"), "options": options }));
                    self.record(
                        sid,
                        "agentdeck",
                        "permission_decision",
                        &json!({ "requestId": rid, "decision": "deny", "auto": "plan_mode" }),
                    );
                    return;
                }
                if let Ok(mut a) = proc.approvals.lock() {
                    a.insert(rid.clone(), json!({ "id": id, "options": options }));
                }
                let title = params
                    .pointer("/toolCall/title")
                    .and_then(Value::as_str)
                    .unwrap_or("use a tool");
                self.set_status(
                    sid,
                    "waiting",
                    Some(&first_line(&format!("Wants to: {title}"), 80)),
                );
                self.record(sid, "acp", "approval", &json!({ "requestId": rid, "toolCall": params.get("toolCall"), "options": options }));
            } else {
                let _ = proc.write(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("AgentDeck doesn't provide {method}") } }));
            }
            return;
        }

        if method != "session/update" {
            return;
        }
        proc.last_update
            .store(crate::db::now_ms() as u64, Ordering::SeqCst);
        if params
            .pointer("/update/sessionUpdate")
            .and_then(Value::as_str)
            == Some("available_commands_update")
        {
            crate::commands::remember(sid, crate::commands::from_acp(&params["update"]));
            return;
        }
        if proc.replaying.load(Ordering::SeqCst) {
            return; // history we already have
        }
        let update = params.get("update").cloned().unwrap_or(Value::Null);
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind {
            "agent_message_chunk" => {
                if let Some(t) = update.pointer("/content/text").and_then(Value::as_str) {
                    if let Ok(mut buf) = proc.text.lock() {
                        let reset = buf.is_empty();
                        buf.push_str(t);
                        (self.sink)(Outbound::Delta(Delta {
                            session_id: sid.into(),
                            text: t.into(),
                            reset,
                        }));
                    }
                }
            }
            "tool_call" | "tool_call_update" | "plan" | "current_mode_update" => {
                self.flush_text(sid, proc);
                self.record(sid, "acp", &format!("update/{kind}"), &update);
            }
            _ => {} // thoughts, usage, command lists: not shown
        }
    }

    /* ------------------------------------------------------------ commands */

    pub fn send_message(self: &Arc<Self>, sid: &str, text: &str) -> Result<(), String> {
        self.send_message_with(sid, text, &crate::attach::Attach::default())
    }

    /// Sends a message with files the user attached, listed by path.
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
        let full = format!("{text}{}", att.note(false));
        let text = full.as_str();
        let title = (session.title == "New session").then(|| first_line(text, 48));
        if let Ok(Some(r)) = self.db.update_session(
            sid,
            Some("running"),
            Some(&first_line(text, 80)),
            title.as_deref(),
            None,
            None,
        ) {
            (self.sink)(Outbound::Session(r));
        }
        let ready = proc.sid().is_some()
            && !proc.replaying.load(Ordering::SeqCst)
            && !proc
                .calls
                .lock()
                .map(|c| {
                    c.values()
                        .any(|p| matches!(p, Pending::Initialize | Pending::OpenSession))
                })
                .unwrap_or(true);
        if ready && !proc.prompting.load(Ordering::SeqCst) {
            self.start_prompt(sid, &proc, text)
        } else {
            proc.queued
                .lock()
                .map_err(|_| "lock poisoned")?
                .push(text.to_string());
            Ok(())
        }
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
        let options = req.get("options").cloned().unwrap_or(json!([]));
        let chosen = match decision {
            Decision::Allow => pick(&options, &["allow_once"]),
            // Only the agent's own "always" option; its name says how long it lasts.
            Decision::AllowSession => Some(
                pick(&options, &["allow_always"])
                    .ok_or("The agent didn't offer a lasting approval for this request")?,
            ),
            Decision::Deny => pick(&options, &["reject_once"]),
        };
        let outcome = match chosen {
            Some(o) => json!({ "outcome": "selected", "optionId": o }),
            None if matches!(decision, Decision::Deny) => json!({ "outcome": "cancelled" }),
            None => {
                return Err("The agent didn't offer a one-time approval for this request".into())
            }
        };
        proc.write(
            &json!({ "jsonrpc": "2.0", "id": req["id"], "result": { "outcome": outcome } }),
        )?;
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

    pub fn set_model(&self, sid: &str) {
        if let Some(proc) = self.proc(sid) {
            self.apply_model(sid, &proc);
        }
    }

    pub fn set_mode(&self, sid: &str) {
        if let Some(proc) = self.proc(sid) {
            self.apply_mode(sid, &proc);
        }
    }

    pub fn interrupt(&self, sid: &str) -> Result<(), String> {
        let proc = self.proc(sid).ok_or("This session isn't running")?;
        let session_id = proc
            .sid()
            .ok_or("The agent hasn't opened the session yet")?;
        // Pending permission requests must be answered "cancelled" before the turn can end.
        let pending: Vec<(String, Value)> = proc
            .approvals
            .lock()
            .map(|mut a| a.drain().collect())
            .unwrap_or_default();
        for (_, req) in pending {
            let _ = proc.write(&json!({ "jsonrpc": "2.0", "id": req["id"], "result": { "outcome": { "outcome": "cancelled" } } }));
        }
        proc.notify("session/cancel", json!({ "sessionId": session_id }))?;
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

/// The id of the first offered option whose kind is in `kinds`, in order of preference.
fn pick(options: &Value, kinds: &[&str]) -> Option<String> {
    let list = options.as_array()?;
    kinds.iter().find_map(|k| {
        list.iter()
            .find(|o| o["kind"] == *k)
            .and_then(|o| o["optionId"].as_str().map(str::to_owned))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_options_by_kind() {
        let opts = json!([
            { "optionId": "proceed_always", "name": "Allow for this session", "kind": "allow_always" },
            { "optionId": "proceed_once", "name": "Allow", "kind": "allow_once" },
            { "optionId": "cancel", "name": "Reject", "kind": "reject_once" }
        ]);
        assert_eq!(
            pick(&opts, &["allow_once"]).as_deref(),
            Some("proceed_once")
        );
        assert_eq!(
            pick(&opts, &["allow_always"]).as_deref(),
            Some("proceed_always")
        );
        assert_eq!(
            pick(&opts, &["reject_always", "reject_once"]).as_deref(),
            Some("cancel")
        );
        assert_eq!(pick(&json!([]), &["allow_once"]), None);
    }

    #[test]
    fn known_agents() {
        assert_eq!(spec("gemini-cli").unwrap().args, &["--acp"]);
        assert_eq!(spec("grok-build").unwrap().args, &["agent", "stdio"]);
        assert!(spec("claude-code").is_none());
    }
}

/// End-to-end against the real `gemini --acp` pointed at `scripts/mock_gemini.py 8789`.
/// Run with AGENTDECK_IT=1 cargo test acp -- --ignored
#[cfg(test)]
mod integration {
    use super::*;
    use crate::db::StoredEvent;
    use std::sync::mpsc;
    use std::time::Instant;

    struct H {
        db: Arc<Db>,
        mgr: Arc<AcpManager>,
        rx: mpsc::Receiver<Outbound>,
        sid: String,
        proj: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn harness() -> Option<H> {
        if std::env::var("AGENTDECK_IT").is_err() {
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir(&proj).unwrap();
        std::fs::create_dir(&home).unwrap();
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let s = db
            .create_session(&p.id, "gemini-cli", "default", "New session")
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| {
            let _ = tx.lock().unwrap().send(o);
        });
        let mut mgr = AcpManager::new(db.clone(), sink);
        mgr.extra_env = vec![
            ("HOME".into(), home.to_string_lossy().into_owned()),
            ("GEMINI_API_KEY".into(), "test-key".into()),
            (
                "GOOGLE_GEMINI_BASE_URL".into(),
                "http://127.0.0.1:8789".into(),
            ),
            ("NO_BROWSER".into(), "1".into()),
        ];
        Some(H {
            db,
            mgr: Arc::new(mgr),
            rx,
            sid: s.id,
            proj,
            _dir: dir,
        })
    }

    impl H {
        fn wait(&self, kind: &str) -> StoredEvent {
            let deadline = Instant::now() + Duration::from_secs(60);
            while Instant::now() < deadline {
                if let Ok(Outbound::Event(e)) = self.rx.recv_timeout(Duration::from_millis(200)) {
                    if std::env::var("AGENTDECK_DEBUG").is_ok() {
                        eprintln!(
                            "{} {} {}",
                            e.source,
                            e.kind,
                            e.payload.to_string().chars().take(300).collect::<String>()
                        );
                    }
                    if e.kind == kind {
                        return e;
                    }
                }
            }
            panic!("timed out waiting for {kind}");
        }
    }

    /// Gemini CLI set to a personal Google login (which Google no longer serves) still works with
    /// a Gemini API key saved in AgentDeck: the key goes in as GEMINI_API_KEY and AgentDeck signs
    /// in with the agent's "gemini-api-key" method.
    #[test]
    #[ignore]
    fn acp_gemini_signs_in_with_the_key_saved_in_agentdeck() {
        let Some(mut h) = harness() else { return };
        let home = h.proj.parent().unwrap().join("home");
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        std::fs::write(
            home.join(".gemini/settings.json"),
            r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
        )
        .unwrap();
        // No key in the environment: only the one saved in AgentDeck.
        let mgr = Arc::get_mut(&mut h.mgr).unwrap();
        mgr.extra_env.retain(|(k, _)| k != "GEMINI_API_KEY");
        crate::secrets::set(&key_name("gemini-cli"), "test-key").unwrap();
        h.mgr.send_message(&h.sid, "hello").unwrap();
        let ended = h.wait("turn/ended");
        crate::secrets::remove(&key_name("gemini-cli"));
        let settings = std::fs::read_to_string(home.join(".gemini/settings.json")).unwrap();
        assert!(
            settings.contains("oauth-personal"),
            "the user's own settings are left alone"
        );
        assert_eq!(
            ended.payload["stopReason"], "end_turn",
            "{:?}",
            ended.payload
        );
        assert_eq!(ended.payload["lastText"], "Hello from the mock model.");
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn acp_gemini_lists_its_models_and_uses_the_one_picked() {
        let Some(h) = harness() else { return };
        h.db.update_session(&h.sid, None, None, None, Some("gemini-2.5-pro"), None)
            .unwrap();
        h.mgr.send_message(&h.sid, "hello").unwrap();
        h.wait("turn/ended");
        let saved = h.db.meta("models:gemini-cli").unwrap().unwrap_or_default();
        assert!(
            saved.contains("gemini-2.5-pro") && saved.contains("\"default\""),
            "{saved}"
        );
        // The mock records each request; the answer came from the model that was picked.
        let log = std::fs::read_to_string("/tmp/mock_gemini_requests.jsonl").unwrap_or_default();
        assert!(
            log.lines()
                .rev()
                .take(3)
                .any(|l| l.contains("models/gemini-2.5-pro:stream")),
            "the picked model answered"
        );
    }

    #[test]
    #[ignore]
    fn acp_gemini_starts_fresh_when_its_old_conversation_is_gone() {
        let Some(h) = harness() else { return };
        h.db.update_session(&h.sid, None, None, None, None, Some("never-saved-session"))
            .unwrap();
        h.mgr.send_message(&h.sid, "hello").unwrap();
        let opened = h.wait("session/opened");
        assert_eq!(opened.payload["resumed"], false);
        assert_ne!(opened.payload["sessionId"], "never-saved-session");
        assert_eq!(h.wait("turn/ended").payload["stopReason"], "end_turn");
    }

    #[test]
    #[ignore]
    fn acp_gemini_allow_resume_deny() {
        let Some(h) = harness() else { return };
        h.mgr.send_message(&h.sid, "please write the file").unwrap();
        let opened = h.wait("session/opened");
        let gemini_sid = opened.payload["sessionId"].as_str().unwrap().to_string();
        assert!(opened.payload["modes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "plan"));
        let approval = h.wait("approval");
        assert_eq!(h.db.session(&h.sid).unwrap().unwrap().status, "waiting");
        let rid = approval.payload["requestId"].as_str().unwrap().to_string();
        assert!(h
            .mgr
            .respond_permission(&h.sid, "nope", Decision::Allow)
            .is_err());
        h.mgr
            .respond_permission(&h.sid, &rid, Decision::Allow)
            .unwrap();
        let end = h.wait("turn/ended");
        assert_eq!(end.payload["stopReason"], "end_turn");
        assert!(end.payload["lastText"]
            .as_str()
            .unwrap()
            .starts_with("Done."));
        assert!(
            h.proj.join("hello.txt").exists(),
            "the approved write happened"
        );

        // Stop, then send again: the same Gemini session is loaded, and a denial is honored.
        h.mgr.stop(&h.sid);
        h.wait("process_exit");
        std::fs::remove_file(h.proj.join("hello.txt")).unwrap();
        h.mgr.send_message(&h.sid, "write it again").unwrap();
        let opened2 = h.wait("session/opened");
        assert_eq!(opened2.payload["resumed"], true);
        assert_eq!(opened2.payload["sessionId"].as_str().unwrap(), gemini_sid);
        let approval2 = h.wait("approval");
        // The history Gemini replays on load is not stored a second time.
        let after: Vec<StoredEvent> =
            h.db.events(&h.sid)
                .unwrap()
                .into_iter()
                .filter(|e| e.seq > opened2.seq)
                .collect();
        let texts: Vec<&str> = after
            .iter()
            .filter(|e| e.kind == "agent_message")
            .filter_map(|e| e.payload["text"].as_str())
            .collect();
        assert_eq!(texts, vec!["I'll create the file."]);
        assert_eq!(
            after
                .iter()
                .filter(|e| e.kind == "update/tool_call")
                .count(),
            1
        );
        let rid2 = approval2.payload["requestId"].as_str().unwrap().to_string();
        assert_ne!(rid, rid2, "request ids stay unique across processes");
        h.mgr
            .respond_permission(&h.sid, &rid2, Decision::Deny)
            .unwrap();
        h.wait("turn/ended");
        assert!(
            !h.proj.join("hello.txt").exists(),
            "the denied write did not happen"
        );
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_ACP") {
            std::fs::write(
                path,
                serde_json::to_string_pretty(&h.db.events(&h.sid).unwrap()).unwrap(),
            )
            .unwrap();
        }
        h.mgr.stop(&h.sid);
    }

    #[test]
    #[ignore]
    fn acp_gemini_plan_mode_uses_the_agents_plan_mode() {
        let Some(h) = harness() else { return };
        h.db.set_session_mode(&h.sid, "plan").unwrap();
        h.mgr.send_message(&h.sid, "please write the file").unwrap();
        h.wait("session/opened");
        h.wait("turn/ended");
        assert!(
            !h.proj.join("hello.txt").exists(),
            "nothing was written in plan mode"
        );
        h.mgr.stop(&h.sid);
    }
}
