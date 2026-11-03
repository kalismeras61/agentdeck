//! Bringing in sessions the agents kept on their own: Claude Code (`~/.claude/projects`),
//! Codex (`$CODEX_HOME/sessions`, default `~/.codex`) and Gemini CLI (`~/.gemini/tmp`).
//!
//! Only conversation files are read, never credentials. Each session keeps its messages and
//! dates; the folder it ran in decides its project (added if it isn't one yet). Sessions whose
//! folder is unknown or gone go to Chats. Where the agent can pick the conversation up again
//! (its own session id, same folder), the next message continues it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const MAX_FILES_PER_AGENT: usize = 400;
const MAX_MESSAGES: usize = 2000;
const MAX_TEXT: usize = 100_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub provider: String,
    pub agent_id: String,
    pub file: String,
    pub title: String,
    pub cwd: Option<String>,
    /// The folder is still there, so the session can go into that project.
    pub folder_exists: bool,
    pub messages: usize,
    pub started_at: i64,
    pub updated_at: i64,
    /// AgentDeck session it already is, if any.
    pub existing: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub provider: String,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub user: bool,
    pub text: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub agent_id: String,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub messages: Vec<Msg>,
    pub started_at: i64,
    pub updated_at: i64,
}

/// Where each agent keeps its sessions.
pub struct Roots {
    pub claude: PathBuf,
    pub codex: PathBuf,
    pub gemini: PathBuf,
}

impl Roots {
    pub fn from_home(home: &Path) -> Self {
        // Same folders the agents use, including CLAUDE_CONFIG_DIR / CODEX_HOME from your shell.
        let h = crate::context::Homes::from_home(home);
        Self {
            claude: h.claude.join("projects"),
            codex: h.codex.join("sessions"),
            gemini: home.join(".gemini").join("tmp"),
        }
    }

    fn root_for(&self, provider: &str) -> Option<&Path> {
        match provider {
            "claude-code" => Some(&self.claude),
            "codex" => Some(&self.codex),
            "gemini-cli" => Some(&self.gemini),
            _ => None,
        }
    }

    /// Only conversation files under an agent's own folder can be read.
    pub fn check(&self, p: &Pick) -> Result<PathBuf, String> {
        let root = self
            .root_for(&p.provider)
            .ok_or_else(|| format!("AgentDeck can't import {} sessions", p.provider))?;
        let file = std::fs::canonicalize(&p.file)
            .map_err(|_| format!("{} isn't there anymore", p.file))?;
        let root = std::fs::canonicalize(root)
            .map_err(|_| "That agent has no saved sessions".to_string())?;
        let ext_ok = matches!(
            file.extension().and_then(|e| e.to_str()),
            Some("jsonl" | "json")
        );
        if !file.starts_with(&root) || !ext_ok {
            return Err("That file isn't one of the agent's saved sessions".into());
        }
        Ok(file)
    }
}

/// "2026-11-02T05:53:08.969Z" → milliseconds since 1970 (UTC; offsets other than Z are read as UTC).
pub fn iso_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let ms = if b.get(19) == Some(&b'.') {
        let frac: String = s[20..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .take(3)
            .collect();
        format!("{frac:0<3}").parse::<i64>().unwrap_or(0)
    } else {
        0
    };
    // Days from 1970-01-01 (Howard Hinnant's civil-from-days, inverted).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + mi) * 60_000 + se * 1000 + ms)
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn clip(t: &str) -> String {
    if t.len() <= MAX_TEXT {
        return t.to_string();
    }
    let mut end = MAX_TEXT;
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n[… cut by AgentDeck on import]", &t[..end])
}

/// Text an agent's UI added around the conversation, not something the user typed.
fn is_noise(t: &str) -> bool {
    let t = t.trim_start();
    t.is_empty()
        || [
            "<command-",
            "<local-command",
            "<system-reminder>",
            "<environment_context",
            "<user_instructions",
            "<skills_instructions",
            "<permissions",
            "<turn_aborted",
            "<session_context>",
            "# AGENTS.md",
            "Caveat: The messages below",
            "[Request interrupted",
            "AgentDeck plan mode:",
        ]
        .iter()
        .any(|p| t.starts_with(p))
}

fn texts(content: &Value, kinds: &[&str]) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter(|i| {
                let k = i["type"].as_str().unwrap_or("text");
                kinds.contains(&k)
            })
            .filter_map(|i| i["text"].as_str())
            .filter(|t| !is_noise(t))
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

impl Parsed {
    /// Adds a message; consecutive replies from the agent become one.
    fn push(&mut self, user: bool, text: String, ts: i64) {
        let text = text.trim().to_string();
        if text.is_empty() || (user && is_noise(&text)) {
            return;
        }
        if ts > 0 {
            if self.started_at == 0 || ts < self.started_at {
                self.started_at = ts;
            }
            self.updated_at = self.updated_at.max(ts);
        }
        if let Some(last) = self.messages.last_mut() {
            if !user && !last.user {
                last.text.push_str("\n\n");
                last.text.push_str(&text);
                return;
            }
        }
        self.messages.push(Msg { user, text, ts });
    }

    fn finish(mut self, file: &Path) -> Self {
        if self.messages.len() > MAX_MESSAGES {
            let cut = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(..cut);
        }
        for m in &mut self.messages {
            m.text = clip(&m.text);
        }
        if self.updated_at == 0 {
            self.updated_at = mtime_ms(file);
        }
        if self.started_at == 0 {
            self.started_at = self.updated_at;
        }
        // Messages without a time sit just before the next one that has it.
        let mut last = self.started_at;
        for m in &mut self.messages {
            if m.ts == 0 {
                m.ts = last;
            }
            last = m.ts;
        }
        self
    }

    pub fn title(&self) -> String {
        let t = self
            .title
            .clone()
            .or_else(|| {
                self.messages
                    .iter()
                    .find(|m| m.user)
                    .map(|m| m.text.clone())
            })
            .unwrap_or_else(|| "Imported session".into());
        crate::claude::first_line(&t, 60)
    }
}

fn lines(file: &Path) -> impl Iterator<Item = Value> {
    let f = std::fs::File::open(file).ok();
    f.into_iter()
        .flat_map(|f| BufReader::new(f).lines().map_while(Result::ok))
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
}

pub fn parse_claude(file: &Path) -> Parsed {
    let mut p = Parsed {
        agent_id: file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ..Default::default()
    };
    let mut title_rank = 0;
    let mut cwds: Vec<String> = Vec::new();
    for v in lines(file) {
        let kind = v["type"].as_str().unwrap_or("");
        if let Some(c) = v["cwd"].as_str().filter(|c| !c.is_empty()) {
            if !cwds.iter().any(|x| x == c) && cwds.len() < 64 {
                cwds.push(c.to_string());
            }
        }
        // Named sessions: a title the user set beats one Claude made, which beats a summary.
        for (key, rank) in [
            ("customTitle", 3),
            ("aiTitle", 2),
            ("title", 2),
            ("summary", 1),
        ] {
            if kind.contains("title") || kind == "summary" {
                if let Some(t) = v[key].as_str().filter(|t| !t.trim().is_empty()) {
                    if rank > title_rank {
                        title_rank = rank;
                        p.title = Some(t.trim().to_string());
                    }
                }
            }
        }
        if v["isSidechain"].as_bool() == Some(true)
            || v["isMeta"].as_bool() == Some(true)
            || v["isCompactSummary"].as_bool() == Some(true)
        {
            continue;
        }
        let ts = v["timestamp"].as_str().and_then(iso_ms).unwrap_or(0);
        match kind {
            "user" => p.push(true, texts(&v["message"]["content"], &["text"]), ts),
            "assistant" => p.push(false, texts(&v["message"]["content"], &["text"]), ts),
            _ => {}
        }
    }
    p.cwd = claude_folder(file, &cwds);
    p.finish(file)
}

/// The name Claude Code gives the folder under `projects/` where it keeps a working folder's
/// conversations: every character other than a letter or digit becomes `-`.
pub fn claude_dir_name(folder: &str) -> String {
    folder
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The folder a Claude Code conversation belongs to. Claude Code files it under the folder it
/// was started in, and only resumes it from there, while `cwd` on each line follows the shell
/// (and lines copied from an earlier conversation keep that one's folder). So the folder whose
/// name matches wins; failing that, the name is read back against the disk; failing that, the
/// last folder seen.
fn claude_folder(file: &Path, cwds: &[String]) -> Option<String> {
    let dir = file
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str());
    if let Some(dir) = dir {
        if let Some(c) = cwds.iter().find(|c| claude_dir_name(c) == dir) {
            return Some(c.clone());
        }
        if let Some(found) = folder_from_dir_name(dir) {
            return Some(found.to_string_lossy().into_owned());
        }
    }
    cwds.last().cloned()
}

/// Finds the existing folder whose Claude Code name is `name`, walking down from `/`.
/// `-` stands for `/`, `.`, `_`, a space and so on, so each step tries the entries that fit.
pub fn folder_from_dir_name(name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, rest: &str, budget: &mut usize) -> Option<PathBuf> {
        if rest.is_empty() {
            return Some(dir.to_path_buf());
        }
        let rest = rest.strip_prefix('-')?;
        let entries = std::fs::read_dir(dir).ok()?;
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type()
                    .map(|t| t.is_dir() || t.is_symlink())
                    .unwrap_or(false)
            })
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| {
                let k = claude_dir_name(n);
                rest == k || rest.starts_with(&format!("{k}-"))
            })
            .collect();
        // Longer names first: "my-app" before "my" when both fit.
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        for n in names {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
            let k = claude_dir_name(&n);
            let next = dir.join(&n);
            if !next.is_dir() {
                continue;
            }
            if let Some(found) = walk(&next, &rest[k.len()..], budget) {
                return Some(found);
            }
        }
        None
    }
    if name.is_empty() || name.len() > 1024 {
        return None;
    }
    let mut budget = 2000;
    walk(Path::new("/"), name, &mut budget)
}

/// Makes a Claude Code conversation resumable from another folder, as Claude Code only looks
/// for it under the folder it runs in: copies the conversation (and its side folder of tool
/// output, if any) to where Claude Code will look. The original stays. Returns whether the
/// conversation is now there.
pub fn carry_claude_conversation(projects: &Path, id: &str, from: &str, to: &str) -> bool {
    let id_ok = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return false;
    }
    let name = format!("{id}.jsonl");
    let dst_dir = projects.join(claude_dir_name(to));
    if dst_dir.join(&name).is_file() {
        return true;
    }
    let mut src = projects.join(claude_dir_name(from)).join(&name);
    if !src.is_file() {
        let found = std::fs::read_dir(projects)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path().join(&name))
            .find(|p| p.is_file());
        match found {
            Some(f) => src = f,
            None => return false,
        }
    }
    if std::fs::create_dir_all(&dst_dir).is_err()
        || std::fs::copy(&src, dst_dir.join(&name)).is_err()
    {
        return false;
    }
    if let Some(side) = src.parent().map(|d| d.join(id)).filter(|d| d.is_dir()) {
        let _ = copy_tree(&side, &dst_dir.join(id), 0);
    }
    true
}

fn copy_tree(from: &Path, to: &Path, depth: usize) -> std::io::Result<()> {
    if depth > 6 {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let t = e.file_type()?;
        let dst = to.join(e.file_name());
        if t.is_dir() {
            copy_tree(&e.path(), &dst, depth + 1)?;
        } else if t.is_file() && !dst.exists() {
            std::fs::copy(e.path(), dst)?;
        }
    }
    Ok(())
}

pub fn parse_codex(file: &Path) -> Parsed {
    let mut p = Parsed::default();
    for v in lines(file) {
        let pl = &v["payload"];
        let ts = v["timestamp"].as_str().and_then(iso_ms).unwrap_or(0);
        match v["type"].as_str().unwrap_or("") {
            "session_meta" => {
                p.agent_id = pl["id"].as_str().unwrap_or_default().to_string();
                p.cwd = pl["cwd"].as_str().map(str::to_owned);
            }
            "response_item" if pl["type"] == "message" => match pl["role"].as_str() {
                Some("user") => p.push(true, texts(&pl["content"], &["input_text", "text"]), ts),
                Some("assistant") => {
                    p.push(false, texts(&pl["content"], &["output_text", "text"]), ts)
                }
                _ => {}
            },
            _ => {}
        }
    }
    p.finish(file)
}

/// Gemini CLI keeps `chats/session-*.jsonl` (records, updated by id) or older `.json` files
/// under a folder per project, with that project's path in `.project_root`.
pub fn parse_gemini(file: &Path) -> Parsed {
    let mut p = Parsed::default();
    let project_dir = file.parent().and_then(|d| d.parent());
    p.cwd = project_dir
        .and_then(|d| std::fs::read_to_string(d.join(".project_root")).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut records: Vec<Value> = Vec::new();
    let is_json = file.extension().is_some_and(|e| e == "json");
    if is_json {
        if let Ok(Ok(v)) = std::fs::read_to_string(file).map(|s| serde_json::from_str::<Value>(&s))
        {
            p.agent_id = v["sessionId"].as_str().unwrap_or_default().to_string();
            records = v["messages"].as_array().cloned().unwrap_or_default();
        }
    } else {
        for v in lines(file) {
            if let Some(id) = v["sessionId"].as_str().filter(|_| v.get("type").is_none()) {
                p.agent_id = id.to_string();
            }
            if let Some(id) = v["$set"]["sessionId"].as_str() {
                p.agent_id = id.to_string();
            }
            if v.get("type").is_some() && v.get("id").is_some() {
                // The same record is written again when it changes; keep the newest.
                match records.iter_mut().find(|r| r["id"] == v["id"]) {
                    Some(r) => *r = v,
                    None => records.push(v),
                }
            }
        }
    }
    for r in records {
        let ts = r["timestamp"].as_str().and_then(iso_ms).unwrap_or(0);
        match r["type"].as_str() {
            Some("user") => p.push(true, texts(&r["content"], &["text"]), ts),
            Some("gemini") => p.push(false, texts(&r["content"], &["text"]), ts),
            _ => {}
        }
    }
    p.finish(file)
}

pub fn parse(provider: &str, file: &Path) -> Parsed {
    match provider {
        "claude-code" => parse_claude(file),
        "codex" => parse_codex(file),
        _ => parse_gemini(file),
    }
}

fn collect(dir: &Path, depth: usize, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && depth > 0 {
            collect(&p, depth - 1, keep, out);
        } else if ft.is_file() && keep(&p) {
            out.push(p);
        }
    }
}

fn newest(mut files: Vec<PathBuf>) -> Vec<PathBuf> {
    files.sort_by_key(|f| std::cmp::Reverse(mtime_ms(f)));
    files.truncate(MAX_FILES_PER_AGENT);
    files
}

/// Every session the agents kept, newest first, with what AgentDeck already has.
pub fn scan(roots: &Roots, existing: &dyn Fn(&str, &str) -> Option<String>) -> Vec<Found> {
    let name = |p: &Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    };
    let mut sources: Vec<(&str, Vec<PathBuf>)> = Vec::new();
    let mut claude = Vec::new();
    collect(
        &roots.claude,
        1,
        &|p| p.extension().is_some_and(|e| e == "jsonl"),
        &mut claude,
    );
    sources.push(("claude-code", newest(claude)));
    let mut codex = Vec::new();
    collect(
        &roots.codex,
        4,
        &|p| name(p).starts_with("rollout-") && name(p).ends_with(".jsonl"),
        &mut codex,
    );
    sources.push(("codex", newest(codex)));
    let mut gemini = Vec::new();
    collect(
        &roots.gemini,
        2,
        &|p| {
            p.parent()
                .and_then(|d| d.file_name())
                .is_some_and(|n| n == "chats")
                && name(p).starts_with("session-")
        },
        &mut gemini,
    );
    sources.push(("gemini-cli", newest(gemini)));

    let mut out = Vec::new();
    for (provider, files) in sources {
        for f in files {
            let p = parse(provider, &f);
            if p.messages.iter().all(|m| !m.user) || p.agent_id.is_empty() {
                continue; // nothing the user said, or not a session file
            }
            out.push(Found {
                provider: provider.into(),
                existing: existing(provider, &p.agent_id),
                title: p.title(),
                folder_exists: p.cwd.as_deref().is_some_and(|c| Path::new(c).is_dir()),
                cwd: p.cwd.clone(),
                messages: p.messages.len(),
                started_at: p.started_at,
                updated_at: p.updated_at,
                agent_id: p.agent_id,
                file: f.to_string_lossy().into_owned(),
            });
        }
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_times() {
        assert_eq!(iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_ms("2026-11-02T05:53:08.969Z"), Some(1_793_598_788_969));
        assert_eq!(iso_ms("2000-02-29T12:00:00.5Z"), Some(951_825_600_500));
        assert_eq!(iso_ms("nope"), None);
    }

    fn write(dir: &Path, rel: &str, body: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn claude_conversation_goes_to_the_folder_it_was_filed_under() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        let shop = home.join("code/shop.web");
        let api = home.join("code/shop_api");
        std::fs::create_dir_all(&shop).unwrap();
        std::fs::create_dir_all(&api).unwrap();
        let (m, p) = (shop.to_string_lossy(), api.to_string_lossy());
        let line = |cwd: &str, text: &str| {
            format!(
                r#"{{"type":"user","cwd":"{cwd}","sessionId":"ddd","timestamp":"2026-11-01T10:00:00Z","message":{{"role":"user","content":"{text}"}}}}"#
            )
        };
        // Started in shop_api, but the first lines came from an earlier conversation in shop.web.
        let body = [
            line(&m, "earlier"),
            line(&p, "now in the api"),
            line(&m, "looked at the web app"),
        ]
        .join("\n");
        let f = write(
            &home,
            &format!(".claude/projects/{}/ddd.jsonl", claude_dir_name(&p)),
            &body,
        );
        assert_eq!(parse_claude(&f).cwd.as_deref(), Some(p.as_ref()));

        // No line names the folder: it's read back from the folder name on disk.
        let only_m = [line(&m, "hi")].join("\n");
        let g = write(
            &home,
            &format!(".claude/projects/{}/eee.jsonl", claude_dir_name(&p)),
            &only_m,
        );
        assert_eq!(parse_claude(&g).cwd.as_deref(), Some(p.as_ref()));
        assert_eq!(
            folder_from_dir_name(&claude_dir_name(&m)),
            Some(shop.clone())
        );
        assert_eq!(folder_from_dir_name("-no-such-place-here"), None);
    }

    #[test]
    fn moving_a_claude_conversation_keeps_it_resumable() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        let from = "/Users/me/code/shop.web";
        let to = "/Users/me/code/shop_api";
        write(
            &projects,
            &format!("{}/abc-1.jsonl", claude_dir_name(from)),
            "{}",
        );
        write(
            &projects,
            &format!("{}/abc-1/tool-results/t1.txt", claude_dir_name(from)),
            "out",
        );
        assert!(carry_claude_conversation(&projects, "abc-1", from, to));
        let dst = projects.join(claude_dir_name(to));
        assert!(dst.join("abc-1.jsonl").is_file());
        assert!(dst.join("abc-1/tool-results/t1.txt").is_file());
        assert!(
            projects
                .join(claude_dir_name(from))
                .join("abc-1.jsonl")
                .is_file(),
            "the original stays"
        );
        // Found even when it isn't under the old folder; ids that aren't ids are refused.
        assert!(carry_claude_conversation(
            &projects,
            "abc-1",
            "/elsewhere",
            "/Users/me/third"
        ));
        assert!(!carry_claude_conversation(&projects, "../abc-1", from, to));
        assert!(!carry_claude_conversation(&projects, "missing", from, to));
    }

    #[test]
    #[allow(clippy::useless_format)]
    fn reads_each_agents_own_format() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let proj = home.join("work/app");
        std::fs::create_dir_all(&proj).unwrap();
        let cwd = proj.to_string_lossy();
        // Claude Code: meta lines, a sidechain and command noise are left out; split replies merge.
        write(home, &format!(".claude/projects/{}/aaa-111.jsonl", claude_dir_name(&cwd)), &[
            format!(r#"{{"type":"summary","summary":"Fix login"}}"#),
            format!(r#"{{"type":"user","cwd":"{cwd}","sessionId":"aaa-111","timestamp":"2026-11-01T10:00:00Z","message":{{"role":"user","content":"why does login fail?"}}}}"#),
            format!(r#"{{"type":"user","isMeta":true,"timestamp":"2026-11-01T10:00:01Z","message":{{"role":"user","content":"Caveat: The messages below"}}}}"#),
            format!(r#"{{"type":"assistant","timestamp":"2026-11-01T10:00:05Z","message":{{"role":"assistant","content":[{{"type":"thinking","thinking":"x"}},{{"type":"text","text":"Checking."}}]}}}}"#),
            format!(r#"{{"type":"assistant","timestamp":"2026-11-01T10:00:06Z","message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Read"}}]}}}}"#),
            format!(r#"{{"type":"user","timestamp":"2026-11-01T10:00:07Z","message":{{"role":"user","content":[{{"type":"tool_result","content":"..."}}]}}}}"#),
            format!(r#"{{"type":"assistant","timestamp":"2026-11-01T10:00:09Z","message":{{"role":"assistant","content":[{{"type":"text","text":"The token expired."}}]}}}}"#),
            format!(r#"{{"type":"user","isSidechain":true,"timestamp":"2026-11-01T10:00:10Z","message":{{"role":"user","content":"subagent"}}}}"#),
            format!(r#"{{"type":"user","timestamp":"2026-11-01T10:01:00Z","message":{{"role":"user","content":"<command-name>/clear</command-name>"}}}}"#),
        ].join("\n"));
        // Codex rollout: environment context and developer text are left out.
        write(home, ".codex/sessions/2026/11/01/rollout-x-bbb.jsonl", &[
            r#"{"timestamp":"2026-11-01T09:00:00Z","type":"session_meta","payload":{"id":"bbb-222","cwd":"/gone/folder"}}"#.to_string(),
            r#"{"timestamp":"2026-11-01T09:00:00Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"rules"}]}}"#.to_string(),
            r#"{"timestamp":"2026-11-01T09:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>x</environment_context>"}]}}"#.to_string(),
            r#"{"timestamp":"2026-11-01T09:00:02Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"add a test"}]}}"#.to_string(),
            r#"{"timestamp":"2026-11-01T09:00:09Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Added."}]}}"#.to_string(),
        ].join("\n"));
        // Gemini CLI: records rewritten by id, project path in .project_root.
        write(home, ".gemini/tmp/app/.project_root", &cwd);
        write(home, ".gemini/tmp/app/chats/session-2026-11-01T08-00-ccc.jsonl", &[
            r#"{"sessionId":"ccc-333","projectHash":"h","startTime":"2026-11-01T08:00:00Z","kind":"main"}"#.to_string(),
            r#"{"id":"1","timestamp":"2026-11-01T08:00:00Z","type":"user","content":[{"text":"<session_context>ctx</session_context>"}]}"#.to_string(),
            r#"{"id":"2","timestamp":"2026-11-01T08:00:01Z","type":"user","content":[{"text":"explain main.dart"}]}"#.to_string(),
            r#"{"id":"3","timestamp":"2026-11-01T08:00:02Z","type":"gemini","content":"It sets up"}"#.to_string(),
            r#"{"$set":{"lastUpdated":"2026-11-01T08:00:02Z"}}"#.to_string(),
            r#"{"id":"3","timestamp":"2026-11-01T08:00:02Z","type":"gemini","content":"It sets up the app."}"#.to_string(),
        ].join("\n"));

        let roots = Roots {
            claude: home.join(".claude/projects"),
            codex: home.join(".codex/sessions"),
            gemini: home.join(".gemini/tmp"),
        };
        let found = scan(&roots, &|p, id| {
            (p == "codex" && id == "bbb-222").then(|| "s-1".to_string())
        });
        assert_eq!(found.len(), 3);
        let by = |p: &str| found.iter().find(|f| f.provider == p).unwrap();

        let c = by("claude-code");
        assert_eq!(
            (c.agent_id.as_str(), c.title.as_str(), c.folder_exists),
            ("aaa-111", "Fix login", true)
        );
        let pc = parse_claude(Path::new(&c.file));
        assert_eq!(
            pc.messages
                .iter()
                .map(|m| (m.user, m.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (true, "why does login fail?"),
                (false, "Checking.\n\nThe token expired.")
            ]
        );
        assert_eq!(pc.started_at, iso_ms("2026-11-01T10:00:00Z").unwrap());

        let x = by("codex");
        assert_eq!(
            (x.title.as_str(), x.folder_exists, x.existing.as_deref()),
            ("add a test", false, Some("s-1"))
        );
        assert_eq!(x.messages, 2);

        let g = by("gemini-cli");
        assert_eq!(
            (g.agent_id.as_str(), g.cwd.as_deref()),
            ("ccc-333", Some(cwd.as_ref()))
        );
        let pg = parse_gemini(Path::new(&g.file));
        assert_eq!(
            pg.messages[1].text, "It sets up the app.",
            "the rewritten record wins"
        );

        // Files outside the agents' folders are refused.
        let outside = write(home, "notes.jsonl", "{}");
        assert!(roots
            .check(&Pick {
                provider: "claude-code".into(),
                file: outside.to_string_lossy().into()
            })
            .is_err());
        assert!(roots
            .check(&Pick {
                provider: "claude-code".into(),
                file: c.file.clone()
            })
            .is_ok());
        assert!(roots
            .check(&Pick {
                provider: "grok-build".into(),
                file: c.file.clone()
            })
            .is_err());
    }
}
