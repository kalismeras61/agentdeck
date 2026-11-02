//! AgentDeck's own coding agent for models reached over an OpenAI-compatible API
//! (DeepSeek, Kimi, OpenRouter, Ollama, LM Studio, vLLM, …). No CLI needed.
//!
//! One turn = a loop of streamed `chat/completions` calls with function tools. The tools only
//! work inside the session's project folder: list, read and search run freely; writing,
//! editing and running commands wait for the user's decision, like any other permission
//! request in AgentDeck. Plan mode offers the read-only tools only. The conversation is
//! rebuilt from the stored events, so it survives an app restart.

use crate::claude::{augmented_path, first_line, Decision, Delta, Outbound, Sink};
use crate::db::{Db, Profile, StoredEvent};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_STEPS: usize = 40;
const MAX_READ_BYTES: u64 = 256 * 1024;
const MAX_TOOL_OUTPUT_TO_MODEL: usize = 30 * 1024;
const MAX_TOOL_OUTPUT_STORED: usize = 200 * 1024;
const MAX_BEFORE_STORED: usize = 200 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "build",
    "dist",
    ".dart_tool",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".idea",
    "Pods",
];

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n… cut by AgentDeck ({} more bytes)",
        &s[..cut],
        s.len() - cut
    )
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!(
        "… {} earlier bytes cut by AgentDeck\n{}",
        start,
        &s[start..]
    )
}

/* ------------------------------------------------------------------ paths */

/// Resolves a path the model gave, relative to the project, refusing anything outside it
/// (including through symlinks).
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim();
    let rel = if rel.is_empty() { "." } else { rel };
    let root = root
        .canonicalize()
        .map_err(|e| format!("The project folder isn't readable: {e}"))?;
    let given = Path::new(rel);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        root.join(given)
    };
    let mut clean = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                if !clean.pop() {
                    return Err(format!("{rel} is outside the project folder"));
                }
            }
            Component::CurDir => {}
            other => clean.push(other),
        }
    }
    if !clean.starts_with(&root) {
        return Err(format!("{rel} is outside the project folder"));
    }
    // Follow symlinks for whatever part exists already.
    let mut existing = clean.clone();
    while !existing.exists() {
        if !existing.pop() {
            break;
        }
    }
    let real = existing.canonicalize().unwrap_or(existing);
    if !real.starts_with(&root) {
        return Err(format!("{rel} points outside the project folder"));
    }
    Ok(clean)
}

fn rel_display(root: &Path, p: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    p.strip_prefix(&root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_else(|_| p.to_string_lossy().into_owned())
}

/* ------------------------------------------------------------------ tools */

pub fn tool_specs(plan: bool) -> Value {
    let mut tools = vec![
        json!({ "type": "function", "function": { "name": "list_files", "description": "List files and folders in the project (skips .git, node_modules, build output).", "parameters": { "type": "object", "properties": { "path": { "type": "string", "description": "Folder relative to the project root. Default: the root." }, "depth": { "type": "integer", "description": "How many levels deep, 1 to 4. Default 2." } } } } }),
        json!({ "type": "function", "function": { "name": "read_file", "description": "Read a text file from the project, optionally a range of lines.", "parameters": { "type": "object", "properties": { "path": { "type": "string" }, "start_line": { "type": "integer" }, "end_line": { "type": "integer" } }, "required": ["path"] } } }),
        json!({ "type": "function", "function": { "name": "search", "description": "Search file contents in the project with a regular expression. Returns path:line: text matches.", "parameters": { "type": "object", "properties": { "pattern": { "type": "string" }, "path": { "type": "string", "description": "Folder to search in. Default: the root." } }, "required": ["pattern"] } } }),
    ];
    if !plan {
        tools.push(json!({ "type": "function", "function": { "name": "write_file", "description": "Create a file or replace its whole content. The user approves each write.", "parameters": { "type": "object", "properties": { "path": { "type": "string" }, "content": { "type": "string" } }, "required": ["path", "content"] } } }));
        tools.push(json!({ "type": "function", "function": { "name": "edit_file", "description": "Replace one exact occurrence of old_string with new_string in a file. Read the file first. The user approves each edit.", "parameters": { "type": "object", "properties": { "path": { "type": "string" }, "old_string": { "type": "string" }, "new_string": { "type": "string" } }, "required": ["path", "old_string", "new_string"] } } }));
        tools.push(json!({ "type": "function", "function": { "name": "run_command", "description": "Run a shell command in the project folder (tests, builds, git status, …). The user approves each command. Times out after 120 seconds.", "parameters": { "type": "object", "properties": { "command": { "type": "string" } }, "required": ["command"] } } }));
    }
    Value::Array(tools)
}

pub fn system_prompt(root: &str, plan: bool) -> String {
    let mut s = format!(
        "You are a coding agent working in the project folder {root} on the user's computer, through AgentDeck.\n\
         Use the tools to look at and change the project. Paths are relative to the project folder; you can't reach anything outside it.\n\
         Read a file before editing it, and prefer edit_file for small changes. Use run_command for tests and builds when it helps.\n\
         The user approves each file change and command. If one is denied, don't retry it the same way: say what you wanted and ask.\n\
         Keep replies short and concrete. When you're done, say what changed and how you checked it."
    );
    if plan {
        s.push_str("\n\nPlan mode is on: you can only read. Investigate, then reply with a concrete, numbered implementation plan another agent could follow. Don't try to change anything.");
    }
    s
}

fn walk(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= limit {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            out.push(e.path());
            if depth + 1 < max_depth {
                walk(&e.path(), depth + 1, max_depth, out, limit);
            }
        } else {
            out.push(e.path());
        }
    }
}

pub fn list_files(root: &Path, args: &Value) -> Result<String, String> {
    let dir = resolve(root, args["path"].as_str().unwrap_or("."))?;
    if !dir.is_dir() {
        return Err(format!("{} isn't a folder", rel_display(root, &dir)));
    }
    let depth = args["depth"].as_u64().unwrap_or(2).clamp(1, 4) as usize;
    let mut out = Vec::new();
    walk(&dir, 0, depth, &mut out, 500);
    let lines: Vec<String> = out
        .iter()
        .map(|p| {
            format!(
                "{}{}",
                rel_display(root, p),
                if p.is_dir() { "/" } else { "" }
            )
        })
        .collect();
    Ok(if lines.is_empty() {
        "(empty)".into()
    } else {
        lines.join("\n")
            + if out.len() >= 500 {
                "\n… more not shown"
            } else {
                ""
            }
    })
}

pub fn read_file(root: &Path, args: &Value) -> Result<String, String> {
    let path = resolve(root, args["path"].as_str().ok_or("path is required")?)?;
    let meta = std::fs::metadata(&path)
        .map_err(|_| format!("{} doesn't exist", rel_display(root, &path)))?;
    if meta.is_dir() {
        return Err(format!(
            "{} is a folder; use list_files",
            rel_display(root, &path)
        ));
    }
    let mut buf = Vec::new();
    std::fs::File::open(&path)
        .and_then(|f| f.take(MAX_READ_BYTES).read_to_end(&mut buf))
        .map_err(|e| e.to_string())?;
    if buf.contains(&0) {
        return Err(format!(
            "{} looks like a binary file",
            rel_display(root, &path)
        ));
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let start = args["start_line"].as_u64().unwrap_or(1).max(1) as usize;
    let end = args["end_line"].as_u64().map(|e| e as usize);
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if n < start || end.is_some_and(|e| n > e) {
            continue;
        }
        out.push_str(&format!("{n:>5}  {line}\n"));
    }
    if meta.len() > MAX_READ_BYTES {
        out.push_str(&format!(
            "… file is {} bytes; only the first {} were read. Use start_line/end_line.",
            meta.len(),
            MAX_READ_BYTES
        ));
    }
    Ok(if out.is_empty() {
        "(empty file)".into()
    } else {
        out
    })
}

pub fn search(root: &Path, args: &Value) -> Result<String, String> {
    let pattern = args["pattern"].as_str().ok_or("pattern is required")?;
    let re = regex::RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .or_else(|_| regex::Regex::new(&regex::escape(pattern)))
        .map_err(|e| e.to_string())?;
    let dir = resolve(root, args["path"].as_str().unwrap_or("."))?;
    let mut files = Vec::new();
    walk(&dir, 0, 12, &mut files, 20_000);
    let mut hits = Vec::new();
    for f in files.iter().filter(|p| p.is_file()) {
        if hits.len() >= 200 {
            break;
        }
        let Ok(meta) = std::fs::metadata(f) else {
            continue;
        };
        if meta.len() > 1024 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(f) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                hits.push(format!(
                    "{}:{}: {}",
                    rel_display(root, f),
                    i + 1,
                    first_line(line.trim(), 300)
                ));
                if hits.len() >= 200 {
                    break;
                }
            }
        }
    }
    Ok(if hits.is_empty() {
        "No matches.".into()
    } else {
        hits.join("\n")
    })
}

/// What a write or edit would do, worked out before asking the user.
#[derive(Debug)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
}

pub fn plan_change(root: &Path, name: &str, args: &Value) -> Result<Change, String> {
    let path = resolve(root, args["path"].as_str().ok_or("path is required")?)?;
    if path.is_dir() {
        return Err(format!("{} is a folder", rel_display(root, &path)));
    }
    let before = std::fs::read_to_string(&path).ok();
    match name {
        "write_file" => Ok(Change {
            path,
            before,
            after: args["content"]
                .as_str()
                .ok_or("content is required")?
                .to_string(),
        }),
        "edit_file" => {
            let old = args["old_string"]
                .as_str()
                .ok_or("old_string is required")?;
            let new = args["new_string"]
                .as_str()
                .ok_or("new_string is required")?;
            let text = before.clone().ok_or_else(|| {
                format!(
                    "{} doesn't exist; use write_file to create it",
                    rel_display(root, &path)
                )
            })?;
            if old.is_empty() {
                return Err("old_string can't be empty".into());
            }
            match text.matches(old).count() {
                0 => Err("old_string wasn't found in the file. Read the file again and copy the text exactly.".into()),
                1 => Ok(Change { path, before, after: text.replacen(old, new, 1) }),
                n => Err(format!("old_string appears {n} times; include more surrounding text so it's unique")),
            }
        }
        _ => Err(format!("{name} doesn't change files")),
    }
}

pub fn apply_change(c: &Change) -> Result<(), String> {
    if let Some(dir) = c.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&c.path, &c.after).map_err(|e| format!("Couldn't write the file: {e}"))
}

/// Runs a command with the user's login shell in the project folder; returns (exit code, output).
pub fn run_command(root: &Path, command: &str, cancel: &AtomicBool) -> (Option<i32>, String) {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| Path::new(s).exists())
        .unwrap_or_else(|| "/bin/sh".into());
    let mut cmd = Command::new(&shell);
    crate::shellenv::apply(&mut cmd);
    let child = cmd
        .arg("-lc")
        .arg(command)
        .current_dir(root)
        .env("PATH", augmented_path())
        .env("AGENTDECK_AGENT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return (None, format!("Couldn't start {shell}: {e}")),
    };
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut readers = Vec::new();
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let out = out.clone();
        readers.push(std::thread::spawn(move || {
            let mut r = stream;
            let mut buf = [0u8; 8192];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut o) = out.lock() {
                    if o.len() < 4 * MAX_TOOL_OUTPUT_STORED {
                        o.extend_from_slice(&buf[..n]);
                    }
                }
            }
        }));
    }
    let start = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) => {}
            Err(_) => break None,
        }
        if cancel.load(Ordering::SeqCst) || start.elapsed() > COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            if let Ok(mut o) = out.lock() {
                o.extend_from_slice(if cancel.load(Ordering::SeqCst) {
                    b"\n[stopped by the user]"
                } else {
                    b"\n[stopped: took longer than 120 seconds]"
                });
            }
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    for r in readers {
        let _ = r.join();
    }
    let text = out
        .lock()
        .map(|o| String::from_utf8_lossy(&o).into_owned())
        .unwrap_or_default();
    (code, text)
}

/* ------------------------------------------------------------- history */

/// The chat messages of a session, rebuilt from its stored events.
pub fn history(events: &[StoredEvent]) -> Vec<Value> {
    let mut msgs: Vec<Value> = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let close_open = |msgs: &mut Vec<Value>, open: &mut Vec<String>| {
        for id in open.drain(..) {
            msgs.push(json!({ "role": "tool", "tool_call_id": id, "content": "Not run: the turn was stopped." }));
        }
    };
    for e in events {
        match (e.source.as_str(), e.kind.as_str()) {
            ("agentdeck", "user_message") => {
                close_open(&mut msgs, &mut open);
                let text = e.payload["text"].as_str().unwrap_or("");
                let note = e.payload["note"].as_str().unwrap_or("");
                msgs.push(json!({ "role": "user", "content": format!("{text}{note}") }));
            }
            ("api", "assistant") => {
                close_open(&mut msgs, &mut open);
                let calls = e.payload["toolCalls"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut m = json!({ "role": "assistant", "content": e.payload["text"].as_str().unwrap_or("") });
                if !calls.is_empty() {
                    open = calls
                        .iter()
                        .filter_map(|c| c["id"].as_str().map(str::to_owned))
                        .collect();
                    m["tool_calls"] = Value::Array(
                        calls.iter().map(|c| json!({ "id": c["id"], "type": "function", "function": { "name": c["name"], "arguments": c["arguments"] } })).collect(),
                    );
                }
                msgs.push(m);
            }
            ("api", "tool_result") => {
                let id = e.payload["toolCallId"].as_str().unwrap_or("").to_string();
                open.retain(|x| x != &id);
                msgs.push(json!({ "role": "tool", "tool_call_id": id, "content": e.payload["forModel"].as_str().unwrap_or("") }));
            }
            _ => {}
        }
    }
    // The last assistant turn may have tool calls whose results were never recorded.
    close_open(&mut msgs, &mut open);
    msgs
}

/* ------------------------------------------------------------- streaming */

#[derive(Default, Debug)]
pub struct Reply {
    pub text: String,
    pub calls: Vec<(String, String, String)>, // id, name, arguments
    pub finish: String,
}

/// Reads an OpenAI-style SSE stream of chat completion chunks.
pub fn read_stream(
    r: impl BufRead,
    mut on_text: impl FnMut(&str),
    cancel: &AtomicBool,
) -> Result<Reply, String> {
    let mut reply = Reply::default();
    let mut calls: Vec<(String, String, String)> = Vec::new();
    for line in r.lines() {
        if cancel.load(Ordering::SeqCst) {
            reply.finish = "cancelled".into();
            break;
        }
        let line = line.map_err(|e| format!("The connection dropped: {e}"))?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(err) = v.get("error") {
            return Err(err["message"]
                .as_str()
                .unwrap_or("The model API returned an error")
                .to_string());
        }
        let Some(choice) = v.pointer("/choices/0") else {
            continue;
        };
        let delta = &choice["delta"];
        if let Some(t) = delta["content"].as_str() {
            if !t.is_empty() {
                reply.text.push_str(t);
                on_text(t);
            }
        }
        if let Some(tcs) = delta["tool_calls"].as_array() {
            for tc in tcs {
                let idx = tc["index"].as_u64().unwrap_or(calls.len() as u64) as usize;
                while calls.len() <= idx {
                    calls.push(Default::default());
                }
                let c = &mut calls[idx];
                if let Some(id) = tc["id"].as_str() {
                    c.0 = id.to_string();
                }
                if let Some(n) = tc.pointer("/function/name").and_then(Value::as_str) {
                    c.1.push_str(n);
                }
                if let Some(a) = tc.pointer("/function/arguments").and_then(Value::as_str) {
                    c.2.push_str(a);
                }
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            reply.finish = f.to_string();
        }
    }
    for (i, c) in calls.iter_mut().enumerate() {
        if c.0.is_empty() {
            c.0 = format!("call_{}_{}", crate::db::now_ms(), i);
        }
    }
    reply.calls = calls.into_iter().filter(|c| !c.1.is_empty()).collect();
    Ok(reply)
}

/* ------------------------------------------------------------- manager */

#[derive(Default)]
struct Run {
    cancel: AtomicBool,
    busy: AtomicBool,
    queued: Mutex<Vec<String>>,
    approvals: Mutex<HashMap<String, mpsc::Sender<Decision>>>,
    allow_edits: AtomicBool,
    allowed_commands: Mutex<HashSet<String>>,
}

pub struct ApiManager {
    db: Arc<Db>,
    sink: Sink,
    runs: Mutex<HashMap<String, Arc<Run>>>,
}

impl ApiManager {
    pub fn new(db: Arc<Db>, sink: Sink) -> Self {
        Self {
            db,
            sink,
            runs: Mutex::new(HashMap::new()),
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

    fn run(&self, sid: &str) -> Arc<Run> {
        self.runs
            .lock()
            .unwrap()
            .entry(sid.to_string())
            .or_default()
            .clone()
    }

    pub fn is_running(&self, sid: &str) -> bool {
        self.runs
            .lock()
            .ok()
            .and_then(|r| r.get(sid).map(|x| x.busy.load(Ordering::SeqCst)))
            .unwrap_or(false)
    }

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
        let pid = session
            .profile_id
            .clone()
            .ok_or("This session has no model profile. Pick one in Settings → Models.")?;
        self.db
            .profile(&pid)
            .map_err(|e| e.to_string())?
            .ok_or("This session's model profile was deleted")?;
        let run = self.run(sid);
        self.record(
            sid,
            "agentdeck",
            "user_message",
            &crate::claude::user_message(text, att),
        );
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
        if run.busy.swap(true, Ordering::SeqCst) {
            // Picked up when the current turn ends: it's already in the history.
            run.queued
                .lock()
                .map_err(|_| "lock poisoned")?
                .push(text.to_string());
            return Ok(());
        }
        run.cancel.store(false, Ordering::SeqCst);
        let (me, sid) = (self.clone(), sid.to_string());
        std::thread::spawn(move || me.turn_loop(&sid, run));
        Ok(())
    }

    fn turn_loop(&self, sid: &str, run: Arc<Run>) {
        loop {
            self.turn(sid, &run);
            let more = run
                .queued
                .lock()
                .map(|mut q| !std::mem::take(&mut *q).is_empty())
                .unwrap_or(false);
            if !more || run.cancel.load(Ordering::SeqCst) {
                break;
            }
            self.set_status(sid, "running", Some("Working"));
        }
        run.busy.store(false, Ordering::SeqCst);
    }

    fn end(&self, sid: &str, stop: &str, last: &str, status: &str, snippet: &str) {
        self.record(
            sid,
            "api",
            "turn/ended",
            &json!({ "stopReason": stop, "lastText": last }),
        );
        self.set_status(sid, status, Some(snippet));
    }

    fn turn(&self, sid: &str, run: &Run) {
        let session = match self.db.session(sid) {
            Ok(Some(s)) => s,
            _ => return,
        };
        let Some(profile) = session
            .profile_id
            .as_deref()
            .and_then(|p| self.db.profile(p).ok().flatten())
        else {
            self.record(sid, "agentdeck", "error", &json!({ "title": "No model profile", "message": "This session's model profile was deleted. Pick another one in Settings → Models." }));
            return self.end(sid, "error", "", "error", "No model profile");
        };
        let root = match self.db.session_dir(&session) {
            Ok(Some(p)) => PathBuf::from(p),
            _ => return self.end(sid, "error", "", "error", "Project not found"),
        };
        let key = crate::secrets::get(&profile.id);
        let mut last_text = String::new();
        for _ in 0..MAX_STEPS {
            if run.cancel.load(Ordering::SeqCst) {
                return self.end(sid, "cancelled", &last_text, "idle", "Stopped");
            }
            let plan = self
                .db
                .session(sid)
                .ok()
                .flatten()
                .is_some_and(|s| s.mode == "plan");
            let mut messages = vec![
                json!({ "role": "system", "content": system_prompt(&root.to_string_lossy(), plan) }),
            ];
            messages.extend(history(&self.db.events(sid).unwrap_or_default()));
            let reply = match self.complete(sid, &profile, key.as_deref(), &messages, plan, run) {
                Ok(r) => r,
                Err(e) => {
                    self.record(sid, "agentdeck", "error", &json!({ "title": format!("{} returned an error", profile.name), "message": redact(&e, key.as_deref()) }));
                    return self.end(sid, "error", &last_text, "error", &first_line(&e, 80));
                }
            };
            let calls: Vec<Value> = reply
                .calls
                .iter()
                .map(|(id, name, args)| json!({ "id": id, "name": name, "arguments": args }))
                .collect();
            self.record(
                sid,
                "api",
                "assistant",
                &json!({ "text": reply.text, "toolCalls": calls }),
            );
            if !reply.text.trim().is_empty() {
                last_text = reply.text.clone();
            }
            if reply.finish == "cancelled" || run.cancel.load(Ordering::SeqCst) {
                return self.end(sid, "cancelled", &last_text, "idle", "Stopped");
            }
            if reply.calls.is_empty() {
                let snippet = if last_text.is_empty() {
                    "Finished".to_string()
                } else {
                    first_line(&last_text, 90)
                };
                let stop = if reply.finish == "length" {
                    "max_tokens"
                } else {
                    "end_turn"
                };
                return self.end(sid, stop, &last_text, "idle", &snippet);
            }
            for (id, name, args) in &reply.calls {
                if run.cancel.load(Ordering::SeqCst) {
                    break;
                }
                self.tool(sid, run, &root, id, name, args, plan);
            }
        }
        self.record(sid, "agentdeck", "error", &json!({ "title": "Stopped after 40 steps", "message": "The model kept calling tools. Send a message to let it continue." }));
        self.end(
            sid,
            "max_turn_requests",
            &last_text,
            "idle",
            "Stopped after 40 steps",
        )
    }

    fn complete(
        &self,
        sid: &str,
        p: &Profile,
        key: Option<&str>,
        messages: &[Value],
        plan: bool,
        run: &Run,
    ) -> Result<Reply, String> {
        let url = format!("{}/chat/completions", p.base_url.trim_end_matches('/'));
        let body = json!({ "model": p.model, "messages": messages, "tools": tool_specs(plan), "stream": true });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(None)
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.post(&url).json(&body);
        if let Some(k) = key.filter(|k| !k.is_empty()) {
            req = req.bearer_auth(k);
        }
        let resp = req
            .send()
            .map_err(|e| format!("Couldn't reach {url}: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| first_line(&text, 400));
            return Err(format!("HTTP {}: {msg}", status.as_u16()));
        }
        let mut reset = true;
        read_stream(
            BufReader::new(resp),
            |t| {
                (self.sink)(Outbound::Delta(Delta {
                    session_id: sid.into(),
                    text: t.into(),
                    reset,
                }));
                reset = false;
            },
            &run.cancel,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn tool(
        &self,
        sid: &str,
        run: &Run,
        root: &Path,
        id: &str,
        name: &str,
        args: &str,
        plan: bool,
    ) {
        let result = |ok: bool, for_model: String, extra: Value| {
            let mut p = json!({ "toolCallId": id, "name": name, "ok": ok, "forModel": clip(&for_model, MAX_TOOL_OUTPUT_TO_MODEL) });
            if let Value::Object(m) = extra {
                for (k, v) in m {
                    p[k] = v;
                }
            }
            self.record(sid, "api", "tool_result", &p);
        };
        let args: Value =
            match serde_json::from_str(if args.trim().is_empty() { "{}" } else { args }) {
                Ok(v) => v,
                Err(e) => {
                    return result(
                        false,
                        format!("The arguments weren't valid JSON: {e}"),
                        json!({}),
                    )
                }
            };
        let mutating = matches!(name, "write_file" | "edit_file" | "run_command");
        if mutating && plan {
            return result(
                false,
                "Plan mode is on: nothing can be changed. Reply with the plan instead.".into(),
                json!({ "refused": "plan_mode" }),
            );
        }
        match name {
            "list_files" | "read_file" | "search" => {
                let r = match name {
                    "list_files" => list_files(root, &args),
                    "read_file" => read_file(root, &args),
                    _ => search(root, &args),
                };
                match r {
                    Ok(out) => result(
                        true,
                        out.clone(),
                        json!({ "output": clip(&out, MAX_TOOL_OUTPUT_STORED) }),
                    ),
                    Err(e) => result(false, e.clone(), json!({ "output": e })),
                }
            }
            "write_file" | "edit_file" => {
                let change = match plan_change(root, name, &args) {
                    Ok(c) => c,
                    Err(e) => return result(false, e.clone(), json!({ "output": e })),
                };
                let rel = rel_display(root, &change.path);
                let before = change.before.as_deref().map(|b| clip(b, MAX_BEFORE_STORED));
                let auto = run.allow_edits.load(Ordering::SeqCst);
                let decision = if auto {
                    Decision::Allow
                } else {
                    self.ask(sid, run, id, name, &json!({ "path": rel, "before": before, "after": clip(&change.after, MAX_BEFORE_STORED), "created": change.before.is_none() }))
                };
                match decision {
                    Decision::Deny => result(
                        false,
                        "The user denied this change.".into(),
                        json!({ "denied": true, "path": rel }),
                    ),
                    d => {
                        if d == Decision::AllowSession {
                            run.allow_edits.store(true, Ordering::SeqCst);
                        }
                        match apply_change(&change) {
                            Ok(()) => result(
                                true,
                                format!(
                                    "{} {rel}.",
                                    if change.before.is_none() {
                                        "Created"
                                    } else {
                                        "Updated"
                                    }
                                ),
                                json!({ "path": rel, "before": before, "after": clip(&change.after, MAX_BEFORE_STORED), "created": change.before.is_none() }),
                            ),
                            Err(e) => result(false, e.clone(), json!({ "output": e, "path": rel })),
                        }
                    }
                }
            }
            "run_command" => {
                let Some(cmd) = args["command"]
                    .as_str()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                else {
                    return result(false, "command is required".into(), json!({}));
                };
                let auto = run
                    .allowed_commands
                    .lock()
                    .map(|a| a.contains(cmd))
                    .unwrap_or(false);
                let decision = if auto {
                    Decision::Allow
                } else {
                    self.ask(sid, run, id, name, &json!({ "command": cmd }))
                };
                match decision {
                    Decision::Deny => result(
                        false,
                        "The user denied this command.".into(),
                        json!({ "denied": true, "command": cmd }),
                    ),
                    d => {
                        if d == Decision::AllowSession {
                            if let Ok(mut a) = run.allowed_commands.lock() {
                                a.insert(cmd.to_string());
                            }
                        }
                        self.set_status(
                            sid,
                            "running",
                            Some(&first_line(&format!("Running {cmd}"), 80)),
                        );
                        let (code, out) = run_command(root, cmd, &run.cancel);
                        let summary = format!(
                            "exit code: {}\n{}",
                            code.map(|c| c.to_string()).unwrap_or_else(|| "none".into()),
                            tail(&out, MAX_TOOL_OUTPUT_TO_MODEL)
                        );
                        result(
                            code == Some(0),
                            summary,
                            json!({ "command": cmd, "exitCode": code, "output": tail(&out, MAX_TOOL_OUTPUT_STORED) }),
                        )
                    }
                }
            }
            other => result(false, format!("There's no tool called {other}."), json!({})),
        }
    }

    /// Waits for the user's decision on one tool call.
    fn ask(&self, sid: &str, run: &Run, id: &str, name: &str, details: &Value) -> Decision {
        let rid = format!("{}-{}", &uuid::Uuid::new_v4().simple().to_string()[..8], id);
        let (tx, rx) = mpsc::channel();
        if let Ok(mut a) = run.approvals.lock() {
            a.insert(rid.clone(), tx);
        }
        let mut payload = json!({ "requestId": rid, "toolCallId": id, "tool": name });
        if let Value::Object(m) = details {
            for (k, v) in m {
                payload[k] = v.clone();
            }
        }
        self.record(sid, "api", "approval", &payload);
        let what = if name == "run_command" {
            "Wants to run a command"
        } else {
            "Wants to change a file"
        };
        self.set_status(sid, "waiting", Some(what));
        let d = loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(d) => break d,
                Err(mpsc::RecvTimeoutError::Timeout) if !run.cancel.load(Ordering::SeqCst) => {
                    continue
                }
                _ => break Decision::Deny,
            }
        };
        if let Ok(mut a) = run.approvals.lock() {
            a.remove(&rid);
        }
        if !run.cancel.load(Ordering::SeqCst) {
            self.set_status(sid, "running", Some("Working"));
        }
        d
    }

    pub fn respond_permission(
        &self,
        sid: &str,
        request_id: &str,
        decision: Decision,
    ) -> Result<(), String> {
        let run = self
            .runs
            .lock()
            .ok()
            .and_then(|r| r.get(sid).cloned())
            .ok_or("This session isn't working on anything")?;
        let tx = run
            .approvals
            .lock()
            .map_err(|_| "lock poisoned")?
            .remove(request_id)
            .ok_or("This request is no longer waiting. The agent may have moved on or stopped.")?;
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
        tx.send(decision)
            .map_err(|_| "The agent stopped waiting for this request".to_string())
    }

    pub fn interrupt(&self, sid: &str) -> Result<(), String> {
        let run = self
            .runs
            .lock()
            .ok()
            .and_then(|r| r.get(sid).cloned())
            .ok_or("This session isn't working on anything")?;
        if !run.busy.load(Ordering::SeqCst) {
            return Err("This session isn't working on anything".into());
        }
        run.cancel.store(true, Ordering::SeqCst);
        self.record(sid, "agentdeck", "interrupt_requested", &json!({}));
        Ok(())
    }

    pub fn stop(&self, sid: &str) {
        if let Some(run) = self.runs.lock().ok().and_then(|r| r.get(sid).cloned()) {
            run.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn shutdown_all(&self) {
        if let Ok(r) = self.runs.lock() {
            for run in r.values() {
                run.cancel.store(true, Ordering::SeqCst);
            }
        }
    }
}

/// Removes the API key from error text, in case a server echoes it back.
fn redact(text: &str, key: Option<&str>) -> String {
    match key.filter(|k| k.len() >= 6) {
        Some(k) => text.replace(k, "•••"),
        None => text.to_string(),
    }
}

/// Checks a profile: lists models (OpenAI-compatible) or sends a tiny message (Anthropic).
pub fn test_profile(p: &Profile, key: Option<&str>) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let base = p.base_url.trim_end_matches('/');
    let resp = if p.kind == "anthropic" {
        let mut req = client
            .post(format!("{base}/v1/messages"))
            .header("anthropic-version", "2023-06-01")
            .json(&json!({ "model": p.model, "max_tokens": 8, "messages": [{ "role": "user", "content": "Reply with OK." }] }));
        if let Some(k) = key.filter(|k| !k.is_empty()) {
            req = req.header("x-api-key", k).bearer_auth(k);
        }
        req.send()
    } else {
        let mut req = client.post(format!("{base}/chat/completions")).json(&json!({ "model": p.model, "max_tokens": 8, "messages": [{ "role": "user", "content": "Reply with OK." }] }));
        if let Some(k) = key.filter(|k| !k.is_empty()) {
            req = req.bearer_auth(k);
        }
        req.send()
    };
    let resp = resp.map_err(|e| redact(&format!("Couldn't reach {base}: {e}"), key))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if status.is_success() {
        Ok(format!("{} answered (HTTP {}).", p.model, status.as_u16()))
    } else {
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| first_line(&text, 300));
        Err(redact(&format!("HTTP {}: {msg}", status.as_u16()), key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("src")).unwrap();
        assert!(resolve(root, "src/a.rs").is_ok());
        assert!(resolve(root, "./src/../b.txt").is_ok());
        assert!(resolve(root, "../outside").is_err());
        assert!(resolve(root, "src/../../outside").is_err());
        assert!(resolve(root, "/etc/passwd").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", root.join("etc-link")).unwrap();
            assert!(
                resolve(root, "etc-link/passwd").is_err(),
                "symlinks out are refused"
            );
        }
    }

    #[test]
    fn edits_need_one_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x = 1\ny = 1\n").unwrap();
        let edit = |old: &str| {
            plan_change(
                dir.path(),
                "edit_file",
                &json!({ "path": "a.txt", "old_string": old, "new_string": "z = 2" }),
            )
        };
        assert!(edit("= 1").unwrap_err().contains("2 times"));
        assert!(edit("w = 0").unwrap_err().contains("wasn't found"));
        let c = edit("x = 1").unwrap();
        assert_eq!(c.after, "z = 2\ny = 1\n");
        apply_change(&c).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "z = 2\ny = 1\n"
        );
        let w = plan_change(
            dir.path(),
            "write_file",
            &json!({ "path": "new/b.txt", "content": "hi" }),
        )
        .unwrap();
        assert!(w.before.is_none());
        apply_change(&w).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("new/b.txt")).unwrap(),
            "hi"
        );
    }

    #[test]
    fn read_list_and_search() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/x")).unwrap();
        std::fs::write(dir.path().join("node_modules/x/i.js"), "needle").unwrap();
        std::fs::write(dir.path().join("a.py"), "def f():\n    return 'needle'\n").unwrap();
        let listing = list_files(dir.path(), &json!({})).unwrap();
        assert!(listing.contains("a.py") && !listing.contains("node_modules"));
        assert_eq!(
            search(dir.path(), &json!({ "pattern": "needle" })).unwrap(),
            "a.py:2: return 'needle'"
        );
        assert!(
            read_file(dir.path(), &json!({ "path": "a.py", "start_line": 2 }))
                .unwrap()
                .starts_with("    2  ")
        );
    }

    #[test]
    fn stream_with_text_and_split_tool_calls() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"pa\"}}]}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"a.py\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n";
        let mut seen = String::new();
        let r = read_stream(
            std::io::Cursor::new(sse),
            |t| seen.push_str(t),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(seen, "Hello");
        assert_eq!(
            r.calls,
            vec![(
                "c1".into(),
                "read_file".into(),
                "{\"path\":\"a.py\"}".into()
            )]
        );
        assert_eq!(r.finish, "tool_calls");
    }

    #[test]
    fn history_closes_tool_calls_that_never_got_a_result() {
        let ev = |kind: &str, source: &str, payload: Value| StoredEvent {
            session_id: "s".into(),
            seq: 0,
            ts: 0,
            source: source.into(),
            kind: kind.into(),
            payload,
        };
        let h = history(&[
            ev("user_message", "agentdeck", json!({ "text": "fix it" })),
            ev(
                "assistant",
                "api",
                json!({ "text": "", "toolCalls": [{ "id": "c1", "name": "run_command", "arguments": "{}" }] }),
            ),
            ev("user_message", "agentdeck", json!({ "text": "never mind" })),
        ]);
        assert_eq!(h.len(), 4);
        assert_eq!(h[2]["role"], "tool");
        assert_eq!(h[2]["tool_call_id"], "c1");
        assert_eq!(h[3]["content"], "never mind");
    }

    #[test]
    fn commands_report_exit_codes() {
        let dir = tempfile::tempdir().unwrap();
        let (code, out) = run_command(
            dir.path(),
            "echo agentdeck-$((40+2)); exit 3",
            &AtomicBool::new(false),
        );
        assert_eq!(code, Some(3));
        assert!(out.contains("agentdeck-42"));
    }

    #[test]
    fn keys_never_appear_in_errors() {
        assert_eq!(
            redact("bad key sk-secret-123 sent", Some("sk-secret-123")),
            "bad key ••• sent"
        );
    }
}

/// End-to-end against `scripts/mock_openai.py 8788` (its chat completions endpoint).
/// Run with AGENTDECK_IT=1 cargo test api_agent -- --ignored
#[cfg(test)]
mod integration {
    use super::*;
    use crate::db::Profile;

    fn wait<F: Fn(&StoredEvent) -> bool>(rx: &mpsc::Receiver<Outbound>, f: F) -> StoredEvent {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok(Outbound::Event(e)) = rx.recv_timeout(Duration::from_millis(100)) {
                if f(&e) {
                    return e;
                }
            }
        }
        panic!("timed out");
    }

    #[test]
    #[ignore]
    fn api_agent_asks_writes_resumes_and_respects_deny() {
        if std::env::var("AGENTDECK_IT").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir(&proj).unwrap();
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let prof = db
            .save_profile(&Profile {
                id: String::new(),
                name: "Mock".into(),
                kind: "openai".into(),
                base_url: "http://127.0.0.1:8788/v1".into(),
                model: "mock-chat".into(),
                small_model: String::new(),
                created_at: 0,
            })
            .unwrap();
        crate::secrets::set(&prof.id, "sk-test-key-123").unwrap();
        let s = db
            .create_session(&p.id, "api", "default", "New session")
            .unwrap();
        db.set_session_profile(&s.id, Some(&prof.id)).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let mgr = Arc::new(ApiManager::new(
            db.clone(),
            Arc::new(move |o| {
                let _ = tx.lock().unwrap().send(o);
            }),
        ));

        mgr.send_message(&s.id, "please write the file").unwrap();
        let ask = wait(&rx, |e| e.kind == "approval");
        assert_eq!(ask.payload["tool"], "write_file");
        assert_eq!(ask.payload["path"], "hello.txt");
        assert!(
            !proj.join("hello.txt").exists(),
            "nothing written before approval"
        );
        let rid = ask.payload["requestId"].as_str().unwrap().to_string();
        mgr.respond_permission(&s.id, &rid, Decision::Allow)
            .unwrap();
        let end = wait(&rx, |e| e.kind == "turn/ended");
        assert_eq!(end.payload["stopReason"], "end_turn");
        assert_eq!(
            std::fs::read_to_string(proj.join("hello.txt")).unwrap(),
            "hello from agentdeck\n"
        );

        // A new manager (as after a restart) rebuilds the conversation from the store.
        let (tx2, rx2) = mpsc::channel();
        let tx2 = Mutex::new(tx2);
        let mgr2 = Arc::new(ApiManager::new(
            db.clone(),
            Arc::new(move |o| {
                let _ = tx2.lock().unwrap().send(o);
            }),
        ));
        mgr2.send_message(&s.id, "please run the check").unwrap();
        let ask = wait(&rx2, |e| e.kind == "approval");
        assert_eq!(ask.payload["command"], "echo agentdeck-ok");
        mgr2.respond_permission(
            &s.id,
            ask.payload["requestId"].as_str().unwrap(),
            Decision::Deny,
        )
        .unwrap();
        let res = wait(&rx2, |e| e.kind == "tool_result");
        assert_eq!(res.payload["denied"], true);
        wait(&rx2, |e| e.kind == "turn/ended");
        let log = std::fs::read_to_string("/tmp/mock_openai_requests.jsonl").unwrap();
        let last = log
            .lines()
            .rev()
            .find(|l| l.contains("chat/completions"))
            .unwrap();
        assert!(
            last.contains("please write the file"),
            "history was sent again after the restart"
        );
        assert!(
            !db.events(&s.id)
                .unwrap()
                .iter()
                .any(|e| e.payload.to_string().contains("sk-test-key-123")),
            "the key is never stored"
        );
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_API") {
            std::fs::write(
                path,
                serde_json::to_string_pretty(&db.events(&s.id).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
}
