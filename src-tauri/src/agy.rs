//! Sessions with Google's Antigravity CLI (`agy`) through its documented headless mode:
//! `agy --input-format stream-json --output-format stream-json`, one process per session,
//! one `{"event":"user",...}` line per message, `init` / `step_update` / `result` lines back.
//! See https://antigravity.google/docs/cli/headless/.
//!
//! What it writes is stored in the same vocabulary as the ACP agents (`agent_message`,
//! `update/tool_call`, `turn/ended`), so the conversation, file changes and hand-offs render the
//! same way. Headless `agy` can't ask before acting: AgentDeck starts it in `accept-edits` mode
//! (file edits in the project go ahead, review them in Changes) and actions that need your OK
//! are skipped and listed. Sign-in stays with `agy` itself.

use crate::claude::{
    augmented_path, first_line, read_line_bounded, Decision, Delta, Outbound, Sink,
};
use crate::db::{Db, SessionRow};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PROVIDER: &str = "antigravity";
pub const LABEL: &str = "Antigravity";
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
const STDERR_TAIL_BYTES: usize = 8 * 1024;
const STOP_GRACE: Duration = Duration::from_secs(3);
const MAX_DIFF_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
struct Step {
    call_id: String,
    /// For edits: the file and what it held before.
    before: Option<(PathBuf, String)>,
    done: bool,
}

struct Proc {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    busy: AtomicBool,
    stopping: AtomicBool,
    /// A model or mode change waits for the current turn, then the process restarts.
    stale: AtomicBool,
    interrupted: AtomicBool,
    queued: Mutex<Vec<String>>,
    text: Mutex<String>,
    last_text: Mutex<String>,
    conversation: Mutex<Option<String>>,
    steps: Mutex<HashMap<i64, Step>>,
    /// What files held after agy's earlier edits, for diffs when a step names its file late.
    known: Mutex<HashMap<PathBuf, String>>,
    /// The project as it was when the turn started (a git commit object), for diffs.
    base: Mutex<Option<String>>,
    /// What agy wrote to stderr during the current turn (its notices about skipped actions).
    turn_err: Arc<Mutex<String>>,
    cwd: String,
    opened: AtomicBool,
    resume: Option<String>,
    /// The approval level it was started with (see `APPROVALS_KEY`).
    level: String,
    gen: String,
}

impl Proc {
    fn write(&self, v: &Value) -> Result<(), String> {
        let mut g = self.stdin.lock().map_err(|_| "lock poisoned")?;
        let s = g.as_mut().ok_or("Antigravity isn't running")?;
        writeln!(s, "{v}").map_err(|e| format!("Couldn't write to Antigravity: {e}"))?;
        s.flush().map_err(|e| e.to_string())
    }
}

pub struct AgyManager {
    db: Arc<Db>,
    sink: Sink,
    procs: Mutex<HashMap<String, Arc<Proc>>>,
    pub binary_override: Option<PathBuf>,
    pub extra_env: Vec<(String, String)>,
}

/// What a tool does, from its name: `view_file`, `replace_file_content`, `run_command`…
pub fn tool_kind(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("command") || n.contains("shell") || n.contains("exec") || n.contains("terminal")
    {
        "execute"
    } else if n.contains("delete") || n.contains("remove") {
        "delete"
    } else if n.contains("move") || n.contains("rename") {
        "move"
    } else if n.contains("write")
        || n.contains("replace")
        || n.contains("edit")
        || n.contains("create")
        || n.contains("patch")
    {
        "edit"
    } else if n.contains("grep") || n.contains("search") || n.contains("find") {
        "search"
    } else if n.contains("url") || n.contains("web") || n.contains("fetch") || n.contains("browser")
    {
        "fetch"
    } else if n.contains("view")
        || n.contains("read")
        || n.contains("list")
        || n.contains("outline")
    {
        "read"
    } else {
        "other"
    }
}

/// The file a tool works on, from its parameters (whatever the key is called).
pub fn tool_path(params: &Value, cwd: &str) -> Option<PathBuf> {
    let obj = params.as_object()?;
    let keys = [
        "targetfile",
        "target_file",
        "absolutepath",
        "absolute_path",
        "file_path",
        "filepath",
        "path",
        "file",
        "uri",
    ];
    for (k, v) in obj {
        let kl = k.to_lowercase();
        if !keys.contains(&kl.as_str()) {
            continue;
        }
        let s = v.as_str()?.trim().trim_start_matches("file://");
        if s.is_empty() {
            continue;
        }
        let p = Path::new(s);
        return Some(if p.is_absolute() {
            p.to_path_buf()
        } else {
            Path::new(cwd).join(p)
        });
    }
    None
}

fn tool_command(params: &Value) -> Option<String> {
    let obj = params.as_object()?;
    obj.iter()
        .find(|(k, _)| {
            matches!(
                k.to_lowercase().as_str(),
                "commandline" | "command_line" | "command" | "cmd"
            )
        })
        .and_then(|(_, v)| v.as_str().map(str::to_owned))
}

fn read_small(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    if !m.is_file() || m.len() > MAX_DIFF_BYTES {
        return None;
    }
    String::from_utf8(std::fs::read(p).ok()?).ok()
}

/// The project's working tree right now as a git commit object, without touching anything
/// (`git stash create`; HEAD when nothing is uncommitted). None outside git.
fn snapshot(cwd: &str) -> Option<String> {
    let git = |a: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(a)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
    };
    if !git(&["rev-parse", "--verify", "-q", "HEAD"])?
        .status
        .success()
    {
        return None;
    }
    let out = git(&["stash", "create"])?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some(if out.status.success() && !id.is_empty() {
        id
    } else {
        "HEAD".into()
    })
}

/// `file` relative to the project folder. Agents may report paths with symlinks resolved
/// (on macOS /var and /tmp are links to /private/...), so a plain prefix check can miss.
fn relative_to(cwd: &str, file: &Path) -> Option<PathBuf> {
    if let Ok(rel) = file.strip_prefix(cwd) {
        return Some(rel.to_path_buf());
    }
    let real_cwd = std::fs::canonicalize(cwd).ok()?;
    let real_file = std::fs::canonicalize(file).ok().or_else(|| {
        Some(
            std::fs::canonicalize(file.parent()?)
                .ok()?
                .join(file.file_name()?),
        )
    })?;
    real_file
        .strip_prefix(&real_cwd)
        .ok()
        .map(Path::to_path_buf)
}

/// A file as of a snapshot ("" when it wasn't there).
fn at_commit(cwd: &str, rev: &str, file: &Path) -> Option<String> {
    let rel = relative_to(cwd, file)?.to_string_lossy().into_owned();
    let out = Command::new("git")
        .args(["-C", cwd, "show", &format!("{rev}:./{rel}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        Some(String::new())
    }
}

fn error_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Object(o) => o
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(v.to_string())),
        _ => None,
    }
}

/// Is this an allow rule as agy writes them: `command(npm test)`, `write_file(src/)`.
pub fn valid_rule(r: &str) -> bool {
    let Some((name, rest)) = r.split_once('(') else {
        return false;
    };
    let target = rest.strip_suffix(')').unwrap_or("").trim();
    // agy's help text names rules with placeholders ("command(<target>)"): not a real rule.
    let placeholder =
        (target.starts_with('<') && target.ends_with('>')) || target == "..." || target == "…";
    (2..=40).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        && rest.ends_with(')')
        && (2..=300).contains(&rest.len())
        && !r.chars().any(char::is_control)
        && !placeholder
}

/// Allow rules agy names in its notices ("…add `command(npm test)` to permissions.allow").
pub fn allow_rules(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| l.to_lowercase().contains("allow")) {
        let bytes = line.as_bytes();
        for (i, _) in line.match_indices('(') {
            let start = line[..i]
                .rfind(|c: char| !(c.is_ascii_lowercase() || c == '_'))
                .map(|p| p + 1)
                .unwrap_or(0);
            if start == i {
                continue;
            }
            // The matching close paren (targets can contain parens of their own).
            let mut depth = 0;
            let mut end = None;
            for (j, &b) in bytes.iter().enumerate().skip(i) {
                match b {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(e) = end {
                let rule = &line[start..=e];
                if valid_rule(rule) && !out.iter().any(|r| r == rule) {
                    out.push(rule.to_string());
                }
            }
        }
    }
    out
}

/// A rule from a `denied_actions` entry, when it names the action and its target.
fn rule_of(v: &Value) -> Option<String> {
    let rule = match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => {
            let s = |k: &str| {
                o.get(k)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
            };
            if let Some(r) = s("rule")
                .or_else(|| s("allow_rule"))
                .or_else(|| s("allowRule"))
            {
                r.to_string()
            } else {
                let action = s("action").or_else(|| s("tool")).or_else(|| s("type"))?;
                let target = s("target")
                    .or_else(|| s("command"))
                    .or_else(|| s("path"))
                    .or_else(|| s("command_line"))?;
                format!("{action}({target})")
            }
        }
        _ => return None,
    };
    valid_rule(&rule).then_some(rule)
}

/// Adds allow rules to agy's own settings (`~/.gemini/antigravity-cli/settings.json`,
/// `permissions.allow`), keeping everything else in the file.
pub fn add_allow_rules(home: &Path, rules: &[String]) -> Result<PathBuf, String> {
    if rules.is_empty() || rules.len() > 20 || !rules.iter().all(|r| valid_rule(r)) {
        return Err("Those aren't Antigravity allow rules".into());
    }
    let path = home
        .join(".gemini")
        .join("antigravity-cli")
        .join("settings.json");
    let mut v: Value = match std::fs::read_to_string(&path) {
        Ok(t) if !t.trim().is_empty() => serde_json::from_str(&t).map_err(|_| {
            format!("{} isn't valid JSON, so AgentDeck didn't change it. Fix it or add the rule with /permissions in agy.", path.display())
        })?,
        _ => json!({}),
    };
    let obj = v
        .as_object_mut()
        .ok_or("Antigravity's settings file isn't a JSON object")?;
    let perms = obj.entry("permissions").or_insert_with(|| json!({}));
    let perms = perms
        .as_object_mut()
        .ok_or("\"permissions\" in Antigravity's settings isn't an object")?;
    let allow = perms.entry("allow").or_insert_with(|| json!([]));
    let list = allow
        .as_array_mut()
        .ok_or("\"permissions.allow\" in Antigravity's settings isn't a list")?;
    for r in rules {
        if !list.iter().any(|x| x.as_str() == Some(r)) {
            list.push(json!(r));
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.agentdeck-tmp");
    std::fs::write(&tmp, text + "\n")
        .map_err(|e| format!("Couldn't save Antigravity's settings: {e}"))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("Couldn't save Antigravity's settings: {e}"))?;
    Ok(path)
}

/// Arguments for a session's `agy` process.
/// How much agy may do without asking, since it can't ask in AgentDeck. Set in Settings.
/// - "edits": file edits in the project go ahead; anything else that needs approval is skipped.
/// - "auto": agy's own Auto mode: edits and commands it judges safe go ahead, a separate agent
///   reviews commands that leave the sandbox, irreversible actions are skipped.
/// - "all": everything goes ahead (`--dangerously-skip-permissions`), only when you chose it.
///   Plan mode included: agy's plan mode still keeps it from editing, but a planner or a
///   workflow's reviewer can run what it needs to check the work (tests, git diff) instead of
///   having every command skipped.
pub const APPROVALS_KEY: &str = "agy:approvals";

pub fn approvals(db: &Db) -> String {
    match db.meta(APPROVALS_KEY).ok().flatten().as_deref() {
        Some("auto") => "auto".into(),
        Some("all") => "all".into(),
        _ => "edits".into(),
    }
}

pub fn args(session: &SessionRow, approvals: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Plan mode only reads, whatever else is chosen.
    let plan = session.mode == "plan";
    a.push("--mode".into());
    a.push(
        if plan {
            "plan"
        } else if approvals == "auto" {
            "auto"
        } else {
            "accept-edits"
        }
        .into(),
    );
    if approvals == "all" {
        a.push("--dangerously-skip-permissions".into());
    }
    if !session.model.is_empty() && session.model != "default" {
        a.push("--model".into());
        a.push(session.model.clone());
    }
    if let Some(c) = session
        .provider_session_id
        .as_deref()
        .filter(|c| !c.is_empty())
    {
        a.push("--conversation".into());
        a.push(c.into());
    }
    a
}

impl AgyManager {
    pub fn new(db: Arc<Db>, sink: Sink) -> Self {
        Self {
            db,
            sink,
            procs: Mutex::new(HashMap::new()),
            binary_override: None,
            extra_env: Vec::new(),
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

    fn binary(&self) -> Result<PathBuf, String> {
        if let Some(p) = &self.binary_override {
            return Ok(p.clone());
        }
        crate::providers::find_binary("agy").ok_or_else(|| {
            "Antigravity CLI (agy) isn't installed. Install it with `brew install antigravity-cli` (or `curl -fsSL https://antigravity.google/cli/install.sh | bash`), run `agy` once to sign in, then try again.".to_string()
        })
    }

    fn spawn(self: &Arc<Self>, session: &SessionRow, cwd: &str) -> Result<Arc<Proc>, String> {
        let bin = self.binary()?;
        let mut cmd = Command::new(&bin);
        let approvals = approvals(&self.db);
        cmd.args(args(session, &approvals))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::shellenv::apply(&mut cmd);
        cmd.env("PATH", augmented_path());
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start Antigravity ({}): {e}", bin.display()))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        let proc = Arc::new(Proc {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            busy: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            stale: AtomicBool::new(false),
            interrupted: AtomicBool::new(false),
            queued: Mutex::new(Vec::new()),
            text: Mutex::new(String::new()),
            last_text: Mutex::new(String::new()),
            conversation: Mutex::new(
                session
                    .provider_session_id
                    .clone()
                    .filter(|c| !c.is_empty()),
            ),
            steps: Mutex::new(HashMap::new()),
            known: Mutex::new(HashMap::new()),
            base: Mutex::new(None),
            turn_err: Arc::new(Mutex::new(String::new())),
            cwd: cwd.to_string(),
            opened: AtomicBool::new(false),
            resume: session
                .provider_session_id
                .clone()
                .filter(|c| !c.is_empty()),
            level: approvals.clone(),
            gen: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
        });
        let tail = Arc::new(Mutex::new(Vec::<u8>::new()));
        {
            let tail = tail.clone();
            let turn_err = proc.turn_err.clone();
            std::thread::spawn(move || {
                let mut r = stderr;
                let mut chunk = [0u8; 4096];
                while let Ok(n) = r.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut e) = turn_err.lock() {
                        if e.len() < 64 * 1024 {
                            e.push_str(&String::from_utf8_lossy(&chunk[..n]));
                        }
                    }
                    if let Ok(mut t) = tail.lock() {
                        t.extend_from_slice(&chunk[..n]);
                        let excess = t.len().saturating_sub(STDERR_TAIL_BYTES);
                        t.drain(..excess);
                    }
                }
            });
        }
        let (me, sid, p, cwd) = (
            self.clone(),
            session.id.clone(),
            proc.clone(),
            cwd.to_string(),
        );
        std::thread::spawn(move || me.read_loop(sid, p, stdout, tail, cwd));
        Ok(proc)
    }

    fn read_loop(
        self: Arc<Self>,
        sid: String,
        proc: Arc<Proc>,
        stdout: std::process::ChildStdout,
        tail: Arc<Mutex<Vec<u8>>>,
        cwd: String,
    ) {
        let mut reader = BufReader::with_capacity(256 * 1024, stdout);
        loop {
            match read_line_bounded(&mut reader, MAX_LINE_BYTES) {
                Ok(None) | Err(_) => break,
                Ok(Some((bytes, _))) => {
                    if bytes.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(v) => self.handle(&sid, &proc, &v, &cwd, &tail),
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
        let busy = proc.busy.swap(false, Ordering::SeqCst);
        if busy {
            self.close_steps(&sid, &proc);
            let stop = if proc.interrupted.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "error"
            };
            let last = proc.last_text.lock().map(|t| t.clone()).unwrap_or_default();
            self.record(
                &sid,
                "acp",
                "turn/ended",
                &json!({ "stopReason": stop, "lastText": last }),
            );
        }
        let ok = expected || code == Some(0);
        self.record(&sid, "agentdeck", "process_exit", &json!({ "code": code, "expected": expected, "stderrTail": if ok { String::new() } else { tail } }));
        if proc.interrupted.load(Ordering::SeqCst) {
            self.set_status(&sid, "idle", Some("Stopped"));
        } else if ok {
            self.set_status(&sid, "idle", None);
        } else {
            let msg = match code {
                Some(c) => format!("Antigravity exited with code {c}"),
                None => "Antigravity was stopped by a signal".to_string(),
            };
            self.set_status(&sid, "error", Some(&msg));
        }
    }

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

    /// Tools still marked as running when a turn ends didn't finish.
    fn close_steps(&self, sid: &str, proc: &Proc) {
        let open: Vec<String> = proc
            .steps
            .lock()
            .map(|mut s| {
                s.drain()
                    .filter(|(_, st)| !st.done)
                    .map(|(_, st)| st.call_id)
                    .collect()
            })
            .unwrap_or_default();
        for id in open {
            self.record(
                sid,
                "acp",
                "update/tool_call_update",
                &json!({ "toolCallId": id, "status": "failed" }),
            );
        }
    }

    fn note_conversation(&self, sid: &str, proc: &Proc, id: Option<&str>) {
        let Some(id) = id.filter(|s| !s.is_empty()) else {
            return;
        };
        let changed = proc
            .conversation
            .lock()
            .map(|mut c| {
                let new = c.as_deref() != Some(id);
                *c = Some(id.to_string());
                new
            })
            .unwrap_or(false);
        if changed || !proc.opened.load(Ordering::SeqCst) {
            if let Ok(Some(r)) = self
                .db
                .update_session(sid, None, None, None, None, Some(id))
            {
                (self.sink)(Outbound::Session(r));
            }
        }
    }

    fn handle(&self, sid: &str, proc: &Proc, v: &Value, cwd: &str, tail: &Mutex<Vec<u8>>) {
        let event = v.get("event").and_then(Value::as_str).unwrap_or("");
        // Payloads come nested under the event name; accept them flat too.
        let p = v.get(event).filter(|x| x.is_object()).unwrap_or(v);
        let conv = p
            .get("conversation_id")
            .or_else(|| v.get("conversation_id"))
            .and_then(Value::as_str);
        match event {
            "init" => {
                self.note_conversation(sid, proc, conv);
                if !proc.opened.swap(true, Ordering::SeqCst) {
                    self.record(sid, "acp", "initialized", &json!({ "agentInfo": { "name": "agy", "title": LABEL, "version": p.get("version").or_else(|| v.get("version")) } , "init": p }));
                    let id = conv
                        .map(str::to_owned)
                        .or_else(|| proc.conversation.lock().ok().and_then(|c| c.clone()));
                    let resumed = proc.resume.is_some() && id.is_some() && proc.resume == id;
                    self.record(sid, "acp", "session/opened", &json!({ "sessionId": id, "resumed": resumed, "modes": ["plan"], "approvals": proc.level }));
                }
            }
            "step_update" => {
                self.note_conversation(sid, proc, conv);
                let ty = p.get("step_type").and_then(Value::as_str).unwrap_or("");
                let idx = p.get("step_index").and_then(Value::as_i64).unwrap_or(-1);
                let done = p.get("state").and_then(Value::as_str) == Some("DONE");
                match ty {
                    "agent_response" => {
                        if let Some(t) = p
                            .get("text_delta")
                            .and_then(Value::as_str)
                            .filter(|t| !t.is_empty())
                        {
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
                        if done {
                            self.flush_text(sid, proc);
                        }
                    }
                    "tool" => self.tool_step(sid, proc, p, idx, done, cwd),
                    _ => {}
                }
            }
            "result" => {
                self.flush_text(sid, proc);
                self.close_steps(sid, proc);
                self.note_conversation(sid, proc, conv);
                let status = p.get("status").and_then(Value::as_str).unwrap_or("SUCCESS");
                let mut last = proc.last_text.lock().map(|t| t.clone()).unwrap_or_default();
                if last.trim().is_empty() {
                    if let Some(r) = p
                        .get("response")
                        .and_then(Value::as_str)
                        .filter(|r| !r.trim().is_empty())
                    {
                        // Nothing was streamed: the answer only came with the result.
                        self.record(sid, "acp", "agent_message", &json!({ "text": r }));
                        last = r.to_string();
                    }
                }
                let denied = p
                    .get("denied_actions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !denied.is_empty() {
                    // Give agy's stderr notice a moment to arrive; it names the rule to allow.
                    std::thread::sleep(Duration::from_millis(150));
                    let err = proc.turn_err.lock().map(|e| e.clone()).unwrap_or_default();
                    let mut rules = allow_rules(&err);
                    for d in &denied {
                        if let Some(r) = rule_of(d) {
                            if !rules.contains(&r) {
                                rules.push(r);
                            }
                        }
                    }
                    let notice: Vec<&str> = err
                        .lines()
                        .filter(|l| {
                            let l = l.to_lowercase();
                            l.contains("allow")
                                || l.contains("denied")
                                || l.contains("permission")
                                || l.contains("skipped")
                        })
                        .take(6)
                        .collect();
                    self.record(
                        sid,
                        "agentdeck",
                        "denied_actions",
                        &json!({ "actions": denied, "rules": rules, "notice": notice.join("\n") }),
                    );
                }
                let stop = match status {
                    "SUCCESS" => "end_turn",
                    "CANCELED" | "INTERRUPTED" => "cancelled",
                    _ => "error",
                };
                if stop == "error" {
                    let stderr = tail
                        .lock()
                        .map(|t| String::from_utf8_lossy(&t).into_owned())
                        .unwrap_or_default();
                    let agy_error = stderr
                        .lines()
                        .rev()
                        .find_map(|l| {
                            l.split_once("AGY_ERROR:")
                                .map(|(_, j)| j.trim().to_string())
                        })
                        .and_then(|j| serde_json::from_str::<Value>(&j).ok());
                    let message = p
                        .get("error")
                        .and_then(error_text)
                        .or_else(|| agy_error.as_ref().and_then(error_text))
                        .unwrap_or_else(|| format!("The turn ended with status {status}."));
                    self.record(sid, "agentdeck", "error", &json!({ "title": "Antigravity stopped with an error", "message": message }));
                }
                proc.busy.store(false, Ordering::SeqCst);
                self.record(
                    sid,
                    "acp",
                    "turn/ended",
                    &json!({ "stopReason": stop, "lastText": last, "usage": p.get("usage") }),
                );
                match stop {
                    "cancelled" => self.set_status(sid, "idle", Some("Stopped")),
                    "error" => {
                        self.set_status(sid, "error", Some("Antigravity stopped with an error"))
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
                if proc.stale.load(Ordering::SeqCst) {
                    // A new model or mode: the next message starts agy again with it.
                    self.stop(sid);
                    return;
                }
                let queued: Vec<String> = proc
                    .queued
                    .lock()
                    .map(|mut q| q.drain(..).collect())
                    .unwrap_or_default();
                if !queued.is_empty() && self.start_turn(proc, &queued.join("\n\n")).is_ok() {
                    self.set_status(sid, "running", Some("Working"));
                }
            }
            _ => {}
        }
    }

    fn tool_step(&self, sid: &str, proc: &Proc, p: &Value, idx: i64, done: bool, cwd: &str) {
        let info = p.get("tool_info").cloned().unwrap_or(Value::Null);
        let name = p
            .get("tool_name")
            .and_then(Value::as_str)
            .or_else(|| info.get("name").and_then(Value::as_str))
            .unwrap_or("tool")
            .to_string();
        let params = info.get("parameters").cloned().unwrap_or(Value::Null);
        let kind = tool_kind(&name);
        let path = tool_path(&params, cwd);
        let mut steps = match proc.steps.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let first = !steps.contains_key(&idx);
        if first {
            drop(steps);
            // Keep text and tools in the order they happened.
            self.flush_text(sid, proc);
            steps = match proc.steps.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let call_id = format!("agy-{}-{idx}", proc.gen);
            let before = match (kind, &path) {
                ("edit" | "delete" | "move", Some(fp)) => {
                    Some((fp.clone(), read_small(fp).unwrap_or_default()))
                }
                _ => None,
            };
            let title = match (kind, tool_command(&params), &path) {
                ("execute", Some(c), _) => format!("Run {}", first_line(&c, 80)),
                (_, _, Some(fp)) => format!(
                    "{} {}",
                    name.replace('_', " "),
                    relative_to(cwd, fp)
                        .as_deref()
                        .unwrap_or(fp)
                        .to_string_lossy()
                        .trim_start_matches('/')
                ),
                _ => name.replace('_', " "),
            };
            let mut raw = params.clone();
            if let (Some(c), Some(o)) = (tool_command(&params), raw.as_object_mut()) {
                o.insert("command".into(), json!(c));
            }
            self.record(sid, "acp", "update/tool_call", &json!({
                "toolCallId": call_id,
                "title": title,
                "kind": kind,
                "status": "in_progress",
                "rawInput": raw,
                "locations": path.as_ref().map(|fp| vec![json!({ "path": fp })]).unwrap_or_default(),
            }));
            steps.insert(
                idx,
                Step {
                    call_id,
                    before,
                    done: false,
                },
            );
        }
        if !done {
            return;
        }
        let Some(step) = steps.get_mut(&idx) else {
            return;
        };
        if step.done {
            return;
        }
        step.done = true;
        let call_id = step.call_id.clone();
        let mut before = step.before.take();
        drop(steps);
        // agy edits a file as soon as it names it, so a copy taken then may already be the new
        // version. Before the edit it held what agy last wrote to it, or what it held when the
        // turn started (git); the early copy is the last resort.
        if matches!(kind, "edit" | "delete" | "move") {
            if let Some(fp) = &path {
                let known = proc.known.lock().ok().and_then(|k| k.get(fp).cloned());
                let base = proc.base.lock().ok().and_then(|b| b.clone());
                let old = known.or_else(|| base.and_then(|b| at_commit(cwd, &b, fp)));
                if let Some(old) = old {
                    before = Some((fp.clone(), old));
                }
            }
        }
        let failed = info.get("error").is_some_and(|e| !e.is_null());
        let mut content = Vec::new();
        let output = info
            .get("output")
            .map(|o| match o {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        let text = if failed {
            let e = info.get("error").and_then(error_text).unwrap_or_default();
            if output.is_empty() {
                e
            } else {
                format!("{output}\n{e}")
            }
        } else {
            output
        };
        if !text.trim().is_empty() {
            content.push(json!({ "type": "content", "content": { "type": "text", "text": text } }));
        }
        if let (false, Some((fp, old))) = (failed, before) {
            let new = read_small(&fp).unwrap_or_default();
            if let Ok(mut k) = proc.known.lock() {
                k.insert(fp.clone(), new.clone());
            }
            if new != old {
                content.push(json!({ "type": "diff", "path": fp, "oldText": old, "newText": new }));
            }
        }
        self.record(
            sid,
            "acp",
            "update/tool_call_update",
            &json!({
                "toolCallId": call_id,
                "status": if failed { "failed" } else { "completed" },
                "content": content,
            }),
        );
    }

    fn start_turn(&self, proc: &Proc, text: &str) -> Result<(), String> {
        proc.busy.store(true, Ordering::SeqCst);
        if let Ok(mut e) = proc.turn_err.lock() {
            e.clear();
        }
        if let Ok(mut b) = proc.base.lock() {
            *b = snapshot(&proc.cwd);
        }
        if let Ok(mut l) = proc.last_text.lock() {
            l.clear();
        }
        proc.write(&json!({ "event": "user", "message": { "content": text } }))
            .inspect_err(|_| proc.busy.store(false, Ordering::SeqCst))
    }

    pub fn send_message(self: &Arc<Self>, sid: &str, text: &str) -> Result<(), String> {
        self.send_message_with(sid, text, &crate::attach::Attach::default())
    }

    /// Sends a message; attached files are listed by path (agy reads them itself).
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
        if !crate::setup::antigravity_enabled(&self.db) {
            return Err("Antigravity is turned off in AgentDeck. Turn it on in Settings → Providers (read the note about Google's terms first).".into());
        }
        let session = self
            .db
            .session(sid)
            .map_err(|e| e.to_string())?
            .ok_or("Session not found")?;
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
        let title = (session.title == "New session").then(|| first_line(&full, 48));
        if let Ok(Some(r)) = self.db.update_session(
            sid,
            Some("running"),
            Some(&first_line(&full, 80)),
            title.as_deref(),
            None,
            None,
        ) {
            (self.sink)(Outbound::Session(r));
        }
        if proc.busy.load(Ordering::SeqCst) {
            proc.queued.lock().map_err(|_| "lock poisoned")?.push(full);
            Ok(())
        } else {
            self.start_turn(&proc, &full)
        }
    }

    pub fn respond_permission(
        &self,
        _sid: &str,
        _request_id: &str,
        _d: Decision,
    ) -> Result<(), String> {
        Err("Antigravity doesn't ask for approvals in AgentDeck; actions that need your OK are skipped and listed instead.".into())
    }

    /// agy has no way to stop a turn in headless mode, so the process is ended; the next
    /// message continues the same conversation.
    pub fn interrupt(&self, sid: &str) -> Result<(), String> {
        let proc = self.proc(sid).ok_or("This session isn't running")?;
        proc.interrupted.store(true, Ordering::SeqCst);
        self.record(sid, "agentdeck", "interrupt_requested", &json!({}));
        proc.stopping.store(true, Ordering::SeqCst);
        if let Ok(mut c) = proc.child.lock() {
            let _ = c.kill();
        }
        Ok(())
    }

    /// Model and mode are start-up flags: an idle process restarts with the next message, a
    /// busy one after its turn.
    pub fn restart_for_settings(&self, sid: &str) {
        if let Some(proc) = self.proc(sid) {
            if proc.busy.load(Ordering::SeqCst) {
                proc.stale.store(true, Ordering::SeqCst);
            } else {
                self.stop(sid);
            }
        }
    }

    /// A new approval level, applied right away. The session you changed it from carries on
    /// under the new level: a turn in progress is ended and picked up again, and a turn that
    /// just skipped actions is redone. Other running sessions restart with it (idle ones now,
    /// busy ones after their turn). Returns whether `current` was asked to carry on.
    pub fn apply_approvals(
        self: &Arc<Self>,
        current: Option<&str>,
        level: &str,
    ) -> Result<bool, String> {
        let others: Vec<String> = self
            .procs
            .lock()
            .map(|p| {
                p.keys()
                    .filter(|k| Some(k.as_str()) != current)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for id in others {
            self.restart_for_settings(&id);
        }
        let Some(sid) = current else { return Ok(false) };
        let was_busy = self
            .proc(sid)
            .is_some_and(|p| p.busy.load(Ordering::SeqCst));
        let skipped = !was_busy && self.last_turn_skipped(sid);
        if let Some(proc) = self.proc(sid) {
            proc.stopping.store(true, Ordering::SeqCst);
            if was_busy {
                proc.interrupted.store(true, Ordering::SeqCst);
            }
            if let Ok(mut c) = proc.child.lock() {
                let _ = c.kill();
            }
            let start = std::time::Instant::now();
            while self.proc(sid).is_some() && start.elapsed() < std::time::Duration::from_secs(5) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        let label = match level {
            "all" => "everything, without asking",
            "auto" => "Auto mode",
            _ => "file edits only",
        };
        self.record(
            sid,
            "agentdeck",
            "approvals_changed",
            &json!({ "level": level, "continued": was_busy || skipped }),
        );
        if !(was_busy || skipped) {
            return Ok(false);
        }
        let text = format!(
            "Your approvals in AgentDeck just changed to {label}. Carry on with the task{}.",
            if skipped {
                ", including the steps you skipped"
            } else {
                " from where you stopped"
            }
        );
        self.send_message_with(sid, &text, &crate::attach::Attach::default())?;
        Ok(true)
    }

    /// Whether the last turn ended with actions Antigravity skipped for lack of approval.
    fn last_turn_skipped(&self, sid: &str) -> bool {
        self.db
            .events(sid)
            .map(|ev| {
                ev.iter()
                    .rev()
                    .find(|e| e.kind == "user_message" || e.kind == "denied_actions")
                    .is_some_and(|e| e.kind == "denied_actions")
            })
            .unwrap_or(false)
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self
            .procs
            .lock()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default();
        for id in ids {
            self.stop(&id);
        }
    }

    pub fn stop(&self, sid: &str) {
        if let Some(proc) = self.proc(sid) {
            proc.stopping.store(true, Ordering::SeqCst);
            if let Ok(mut s) = proc.stdin.lock() {
                s.take(); // closing stdin ends a streaming session cleanly
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_agy_tool_names_and_parameters() {
        assert_eq!(tool_kind("run_command"), "execute");
        assert_eq!(tool_kind("replace_file_content"), "edit");
        assert_eq!(tool_kind("write_to_file"), "edit");
        assert_eq!(tool_kind("view_file"), "read");
        assert_eq!(tool_kind("grep_search"), "search");
        assert_eq!(tool_kind("read_url_content"), "fetch");
        assert_eq!(
            tool_path(
                &json!({ "TargetFile": "/p/src/a.py", "CodeContent": "x" }),
                "/p"
            ),
            Some(PathBuf::from("/p/src/a.py"))
        );
        assert_eq!(
            tool_path(&json!({ "path": "src/b.py" }), "/p"),
            Some(PathBuf::from("/p/src/b.py"))
        );
        assert_eq!(
            tool_command(&json!({ "CommandLine": "pytest -q", "Cwd": "/p" })).as_deref(),
            Some("pytest -q")
        );
    }

    #[test]
    fn finds_the_rule_to_allow_and_saves_it_in_agys_settings() {
        let notice = "notice: skipped run_command \"npm test\": add `command(npm test)` to permissions.allow in settings.json to allow it\nunrelated (stuff)";
        assert_eq!(allow_rules(notice), ["command(npm test)"]);
        assert_eq!(
            rule_of(&json!({ "action": "write_file", "target": "src/" })).as_deref(),
            Some("write_file(src/)")
        );
        assert_eq!(rule_of(&json!("command")), None);
        // The placeholder in agy's own hint isn't a rule anyone can allow.
        let hint = "jetski: a tool required the \"command\" permission that headless mode cannot prompt for. Add an allow-rule under permissions.allow in settings.json (e.g. command(<target>)).";
        assert!(allow_rules(hint).is_empty());
        assert!(!valid_rule("command(<target>)") && !valid_rule("write_file(...)"));
        assert!(
            !valid_rule("command(rm -rf /)\nmore")
                && !valid_rule("Command(x)")
                && !valid_rule("command()")
        );

        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".gemini/antigravity-cli");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            r#"{"theme":"dark","permissions":{"allow":["command(git)"]}}"#,
        )
        .unwrap();
        add_allow_rules(
            home.path(),
            &["command(npm test)".into(), "command(git)".into()],
        )
        .unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(
            v["permissions"]["allow"],
            json!(["command(git)", "command(npm test)"])
        );
        std::fs::write(dir.join("settings.json"), "{ not json").unwrap();
        assert!(add_allow_rules(home.path(), &["command(ls)".into()]).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json")).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn starts_agy_with_the_sessions_settings() {
        let mut s = SessionRow {
            id: "s".into(),
            project_id: "p".into(),
            provider: PROVIDER.into(),
            model: "default".into(),
            title: "t".into(),
            status: "idle".into(),
            snippet: String::new(),
            provider_session_id: None,
            mode: "default".into(),
            profile_id: None,
            worktree: None,
            branch: None,
            effort: None,
            created_at: 0,
            updated_at: 0,
        };
        assert_eq!(
            args(&s, "edits").join(" "),
            "--input-format stream-json --output-format stream-json --mode accept-edits"
        );
        assert_eq!(
            args(&s, "auto").join(" "),
            "--input-format stream-json --output-format stream-json --mode auto"
        );
        assert_eq!(
            args(&s, "all").join(" "),
            "--input-format stream-json --output-format stream-json --mode accept-edits --dangerously-skip-permissions"
        );
        s.mode = "plan".into();
        s.model = "gemini-3.8-flash-medium".into();
        s.provider_session_id = Some("c3b66b04".into());
        assert_eq!(
            args(&s, "all").join(" "),
            "--input-format stream-json --output-format stream-json --mode plan --dangerously-skip-permissions --model gemini-3.8-flash-medium --conversation c3b66b04",
            "a reviewer in plan mode can still run tests when everything is allowed"
        );
        assert_eq!(
            args(&s, "edits").join(" "),
            "--input-format stream-json --output-format stream-json --mode plan --model gemini-3.8-flash-medium --conversation c3b66b04"
        );
    }

    /// macOS reports /private/var/... for a project opened as /var/...; both must match.
    #[cfg(unix)]
    #[test]
    fn a_file_reported_through_the_real_path_is_still_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("a.txt"), "x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let cwd = link.to_string_lossy().to_string();
        let real = std::fs::canonicalize(&real).unwrap();
        assert_eq!(
            relative_to(&cwd, &real.join("a.txt")),
            Some(PathBuf::from("a.txt"))
        );
        assert_eq!(
            relative_to(&cwd, &real.join("new.txt")),
            Some(PathBuf::from("new.txt")),
            "a file that doesn't exist yet"
        );
        assert_eq!(
            relative_to(&cwd, &link.join("a.txt")),
            Some(PathBuf::from("a.txt"))
        );
        assert_eq!(relative_to(&cwd, Path::new("/etc/hosts")), None);
    }

    /// The whole loop against `scripts/mock_agy.py`, which follows agy's documented protocol.
    #[test]
    fn a_session_with_agy_writes_files_reports_skipped_actions_and_continues() {
        use std::sync::mpsc;
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        // Opened through a symlink, as macOS's /var → /private/var: agy reports resolved paths.
        let real = dir.path().join("real-project");
        std::fs::create_dir(&real).unwrap();
        let proj = dir.path().join("project");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &proj).unwrap();
        #[cfg(not(unix))]
        let proj = real.clone();
        std::fs::write(proj.join("hello.txt"), "old\n").unwrap();
        let git = |a: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&proj)
                .args(a)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "start",
        ]);
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let s = db
            .create_session(&p.id, PROVIDER, "default", "New session")
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| {
            let _ = tx.lock().unwrap().send(o);
        });
        let mut mgr = AgyManager::new(db.clone(), sink);
        mgr.binary_override = Some(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../scripts/mock_agy.py"
        )));
        let mgr = Arc::new(mgr);
        let wait = |kind: &str| -> crate::db::StoredEvent {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            loop {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                match rx.recv_timeout(left) {
                    Ok(Outbound::Event(e)) if e.kind == kind => return e,
                    Ok(_) => {}
                    Err(_) => panic!("no {kind}: {:#?}", db.events(&s.id).unwrap()),
                }
            }
        };

        mgr.send_message(&s.id, "please write the file").unwrap();
        let opened = wait("session/opened");
        let conv = opened.payload["sessionId"].as_str().unwrap().to_string();
        let update = wait("update/tool_call_update");
        assert_eq!(update.payload["status"], "completed");
        let diff = &update.payload["content"][1];
        assert_eq!(
            (
                diff["type"].as_str(),
                diff["oldText"].as_str(),
                diff["newText"].as_str()
            ),
            (
                Some("diff"),
                Some("old\n"),
                Some("hello from antigravity\n")
            )
        );
        let ended = wait("turn/ended");
        assert_eq!(ended.payload["stopReason"], "end_turn");
        assert_eq!(ended.payload["lastText"], "Done: hello.txt is in place.");
        assert_eq!(
            db.session(&s.id)
                .unwrap()
                .unwrap()
                .provider_session_id
                .as_deref(),
            Some(conv.as_str())
        );

        // Same process, next turn: an action that needed approval is skipped and listed.
        mgr.send_message(&s.id, "delete the build folder").unwrap();
        let denied = wait("denied_actions");
        assert_eq!(denied.payload["actions"][0]["action"], "command");
        assert_eq!(denied.payload["rules"], json!(["command(rm -rf build)"]));
        wait("turn/ended");

        // Changing the approval level from this session redoes what was skipped, right away.
        db.set_meta(APPROVALS_KEY, "auto").unwrap();
        assert!(mgr.apply_approvals(Some(&s.id), "auto").unwrap());
        assert!(wait("user_message").payload["text"]
            .as_str()
            .unwrap()
            .contains("including the steps you skipped"));
        assert!(wait("turn/ended").payload["lastText"]
            .as_str()
            .unwrap()
            .contains("auto mode"));
        // Nothing was skipped this time, so going back only applies from the next message.
        db.set_meta(APPROVALS_KEY, "edits").unwrap();
        assert!(!mgr.apply_approvals(Some(&s.id), "edits").unwrap());

        // A failed turn shows agy's own error.
        mgr.send_message(&s.id, "this will fail").unwrap();
        let err = wait("error");
        assert!(err.payload["message"]
            .as_str()
            .unwrap()
            .contains("quota exhausted"));
        assert_eq!(wait("turn/ended").payload["stopReason"], "error");

        // A new model restarts agy on the same conversation.
        db.update_session(&s.id, None, None, None, Some("gemini-3.1-pro-high"), None)
            .unwrap();
        mgr.restart_for_settings(&s.id);
        let start = std::time::Instant::now();
        while mgr.is_running(&s.id) && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        mgr.send_message(&s.id, "hi again").unwrap();
        let ended = wait("turn/ended");
        assert_eq!(
            ended.payload["lastText"],
            "Hello from Antigravity (gemini-3.1-pro-high, accept-edits mode)."
        );
        let reopened = db
            .events(&s.id)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "session/opened")
            .last()
            .unwrap();
        assert_eq!(reopened.payload["sessionId"], conv.as_str());
        assert_eq!(reopened.payload["resumed"], true);
        mgr.shutdown_all();
    }
}
