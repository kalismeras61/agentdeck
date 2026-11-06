//! Bots: agents that stay.
//!
//! A bot has a name, a job, instructions, an agent and model, a workspace of its own
//! (`~/AgentDeck/Bots/<Name>`, where its memory file `MEMORY.md` lives), an optional default
//! project, and a queue of tasks. Each task works in a project or in the workspace, and the bot
//! keeps one ongoing session per place it works in (made on its first task there). It works
//! through its queue one task at a time: a task starts when the bot isn't paused and nothing
//! else of it is running, and ends when that session's turn ends (the same signal workflows
//! use). While a task runs, the bot's approval requests get the unattended rule (`unattended`):
//! ordinary ones are allowed, risky ones and ones outside the task's folder and the workspace
//! wait for the user, and so do questions. Schedules (`schedule`) add tasks that repeat.

use crate::agents::Agents;
use crate::claude::{Outbound, Sink};
use crate::db::{Bot, BotFields, BotSchedule, BotTask, Db, Project, StoredEvent};
use crate::relay::{turn_end, TurnEnd};
use crate::schedule::{self, When};
use crate::unattended::{self, Verdict};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Accent names a bot's mark can have.
pub const COLORS: &[&str] = &["violet", "blue", "green", "amber", "rose", "teal"];
pub const MAX_NAME: usize = 40;
pub const MAX_ROLE: usize = 200;
pub const MAX_INSTRUCTIONS: usize = 20_000;
pub const MAX_TASK: usize = 100_000;
pub const MAX_MEMORY: usize = 200_000;
pub const MAX_SCHEDULES: usize = 50;
const MAX_RESULT: usize = 100_000;
pub const MEMORY_FILE: &str = "MEMORY.md";

/// Milestone 1's memory file name: the bot's name in lower case, words joined with dashes.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(48).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "bot".into()
    } else {
        out
    }
}

/// Where milestone 1 kept a bot's memory: `{project}/.agentdeck/bots/{slug}.md`.
pub fn old_memory_path(project_dir: &str, name: &str) -> PathBuf {
    Path::new(project_dir)
        .join(".agentdeck")
        .join("bots")
        .join(format!("{}.md", slug(name)))
}

/// The bot's name made safe for a folder: no slashes, colons or other characters Finder or
/// the shell trip on, no leading dot, at most 60 characters. "PR / Review: v2" → "PR - Review- v2".
pub fn folder_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        let c = if c.is_control()
            || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        {
            '-'
        } else if c.is_whitespace() {
            ' '
        } else {
            c
        };
        if c == ' ' && out.ends_with(' ') {
            continue;
        }
        out.push(c);
    }
    let out: String = out
        .trim_start_matches(['.', ' '])
        .chars()
        .take(60)
        .collect();
    let out = out.trim_end_matches(['.', ' ']).to_string();
    if out.is_empty() || out == "-" {
        "Bot".into()
    } else {
        out
    }
}

/// The memory file in a workspace, made with a heading if it isn't there yet. A link isn't
/// followed (it could point anywhere).
pub fn ensure_memory(workspace: &Path, name: &str) -> Result<PathBuf, String> {
    if !workspace.is_dir() {
        return Err(format!(
            "The folder {} doesn't exist anymore",
            workspace.display()
        ));
    }
    let path = workspace.join(MEMORY_FILE);
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("The bot's memory file is a link; AgentDeck won't follow it".into());
    }
    if !path.exists() {
        std::fs::write(
            &path,
            format!(
                "# {} memory\n\nWhat {} keeps for later tasks.\n",
                name.trim(),
                name.trim()
            ),
        )
        .map_err(|e| format!("Can't write {}: {e}", path.display()))?;
    }
    Ok(path)
}

/// Moves milestone 1's memory file into the workspace: copied (unless the workspace has one
/// already), then the old one removed once the copy is there. Returns whether it moved.
pub fn move_old_memory(old: &Path, workspace: &Path) -> bool {
    let regular = std::fs::symlink_metadata(old).is_ok_and(|m| m.file_type().is_file());
    if !regular {
        return false;
    }
    let new = workspace.join(MEMORY_FILE);
    if std::fs::symlink_metadata(&new).is_err() && std::fs::copy(old, &new).is_err() {
        return false;
    }
    let same = matches!(
        (std::fs::read(old), std::fs::read(&new)),
        (Ok(a), Ok(b)) if a == b
    );
    if !same {
        return false;
    }
    let _ = std::fs::remove_file(old);
    // Empty folders it leaves behind go too (`remove_dir` refuses a folder with anything in it).
    if let Some(bots) = old.parent() {
        if std::fs::remove_dir(bots).is_ok() {
            if let Some(dot) = bots.parent() {
                let _ = std::fs::remove_dir(dot);
            }
        }
    }
    true
}

/// "2026-11-03 14:05 UTC".
pub fn stamp(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// What a bot that can hand work to other bots is told (milestone 2c).
pub const MANAGER_BRIEF: &str = "You can hand tasks to other bots with the agentdeck tools: list_bots, delegate_task, then wait_for_tasks to get their results. Use them when another bot's job fits a part of the work better.";

/// The first message of each of a bot's sessions: who it is, how tasks work, its workspace
/// and its memory file, and (`manager`) that it can hand tasks to other bots.
pub fn brief(bot: &Bot, workspace: &str, memory: &str, manager: bool) -> String {
    let mut s = format!("You're {}, a bot in AgentDeck.", bot.name.trim());
    if !bot.role.trim().is_empty() {
        s.push_str(&format!(
            " Your job: {}.",
            bot.role.trim().trim_end_matches('.')
        ));
    }
    if !bot.instructions.trim().is_empty() {
        s.push('\n');
        s.push_str(bot.instructions.trim());
    }
    s.push_str("\n\nYou work through tasks one at a time. Each task arrives as a message starting with \"Task\". When you're done, end with a short report of what you did and what's left; that report is the task's result.");
    s.push_str(&format!("\n\nYour workspace is {workspace}. Work there when a task names no project, and keep the files you produce there. A task may name a project folder to work in instead; its header says which folder."));
    s.push_str(&format!("\n\nYour memory file is {memory}. Read it at the start of every task. When you learn something worth keeping for later tasks (decisions, conventions, where things are, what the user prefers), add it there in a line or two. Keep it short and current; remove what's no longer true."));
    if manager {
        s.push_str("\n\n");
        s.push_str(MANAGER_BRIEF);
    }
    s
}

/// "Task from you in /Users/me/shop (2026-11-03 14:05 UTC):\n…". `from`: who it's from, as the
/// agent reads it ("you", "schedule", "Lead (another bot)").
pub fn task_header(task: &BotTask, from: &str, folder: &str, now: i64) -> String {
    format!(
        "Task from {from} in {folder} ({}):\n{}",
        stamp(now),
        task.text.trim()
    )
}

/// Who a task is from, for its header: a bot's name for a task another bot handed over.
fn source_label(db: &Db, source: &str) -> String {
    if source.starts_with("telegram:") {
        return "you (sent from Telegram)".into();
    }
    match source.strip_prefix("bot:") {
        Some(id) => match db.bot(id).ok().flatten() {
            Some(b) => format!("{} (another bot)", b.name.trim()),
            None => "another bot".into(),
        },
        None => source.to_string(),
    }
}

/// A bot's session in a place is titled after it: "Reviewer", or "Reviewer · shop".
pub fn session_title(name: &str, project: Option<&str>) -> String {
    match project {
        Some(p) => format!("{} · {p}", name.trim()),
        None => name.trim().to_string(),
    }
}

fn clip(text: &str) -> String {
    if text.len() <= MAX_RESULT {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .take_while(|(i, _)| *i < MAX_RESULT)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!("{}\n\n[… cut by AgentDeck]", &text[..cut])
}

/// The memory file as the bot page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub path: String,
    pub text: String,
}

/// A schedule as the editor sends it; an empty or missing id makes a new one.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInput {
    #[serde(default)]
    pub id: Option<String>,
    pub bot_id: String,
    pub text: String,
    #[serde(default)]
    pub project_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub times: Vec<String>,
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub every_minutes: Option<i64>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn rule(s: &BotSchedule) -> Option<When> {
    schedule::when(&s.kind, &s.times, &s.weekdays, s.every_minutes)
}

pub struct Bots {
    db: Arc<Db>,
    sink: Sink,
    agents: Arc<Agents>,
    /// The home folder: workspaces are in `~/AgentDeck/Bots`, and Claude Code files its
    /// conversations under it (carried along when a workspace is renamed).
    home: PathBuf,
    /// One pump at a time, so a bot never starts two tasks.
    gate: Mutex<()>,
    /// Bots with a request or question waiting for the user.
    needs: Mutex<HashSet<String>>,
    /// The `agentdeck` MCP server is up, so bots on agents that take it can hand work to
    /// other bots (`mcp`).
    pub delegation: AtomicBool,
}

impl Bots {
    pub fn new(db: Arc<Db>, sink: Sink, agents: Arc<Agents>, home: PathBuf) -> Self {
        Self {
            db,
            sink,
            agents,
            home,
            gate: Mutex::new(()),
            needs: Mutex::new(HashSet::new()),
            delegation: AtomicBool::new(false),
        }
    }

    /// `~/AgentDeck/Bots`.
    pub fn root(&self) -> PathBuf {
        bots_root(&self.home)
    }

    fn needs(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.needs.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn view(&self, mut b: Bot) -> Bot {
        b.needs_you = self.needs().contains(&b.id);
        b
    }

    pub fn list(&self) -> Result<Vec<Bot>, String> {
        Ok(self
            .db
            .list_bots()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|b| self.view(b))
            .collect())
    }

    pub fn get(&self, id: &str) -> Result<Bot, String> {
        self.db
            .bot(id)
            .map_err(|e| e.to_string())?
            .map(|b| self.view(b))
            .ok_or_else(|| "Bot not found".to_string())
    }

    fn publish(&self, bot_id: &str) {
        if let Ok(b) = self.get(bot_id) {
            (self.sink)(Outbound::Bot(b));
        }
    }

    fn publish_task(&self, t: &BotTask) {
        (self.sink)(Outbound::BotTask(t.clone()));
    }

    fn publish_schedule(&self, s: &BotSchedule) {
        (self.sink)(Outbound::BotSchedule(s.clone()));
    }

    fn record(&self, sid: &str, kind: &str, payload: &Value) {
        if let Ok(Some(ev)) = self.db.append_event(sid, "agentdeck", kind, payload) {
            (self.sink)(Outbound::Event(ev));
        }
    }

    fn set_needs(&self, bot_id: &str, on: bool) {
        let changed = if on {
            self.needs().insert(bot_id.to_string())
        } else {
            self.needs().remove(bot_id)
        };
        if changed {
            self.publish(bot_id);
        }
    }

    /// A project a bot may be pointed at: one you added, not another bot's workspace.
    fn pick_project(&self, project_id: &str) -> Result<Project, String> {
        let p = self
            .db
            .project(project_id)
            .map_err(|e| e.to_string())?
            .ok_or("That project was removed")?;
        if p.hidden {
            return Err("That folder is a bot's workspace, not a project".into());
        }
        Ok(p)
    }

    /// Makes sure the bot has its workspace folder, registered as a hidden project, with its
    /// memory file in it. A bot from milestone 1 gets its old memory file moved in.
    pub fn ensure_workspace(&self, bot: &Bot) -> Result<(Project, PathBuf), String> {
        if let Some(p) = bot
            .workspace_id
            .as_deref()
            .and_then(|w| self.db.project(w).ok().flatten())
        {
            std::fs::create_dir_all(&p.path).map_err(|e| format!("Can't make {}: {e}", p.path))?;
            let memory = ensure_memory(Path::new(&p.path), &bot.name)?;
            return Ok((p, memory));
        }
        let root = self.root();
        std::fs::create_dir_all(&root)
            .map_err(|e| format!("Can't make {}: {e}", root.display()))?;
        let dir = root.join(folder_name(&bot.name));
        std::fs::create_dir_all(&dir).map_err(|e| format!("Can't make {}: {e}", dir.display()))?;
        let (croot, cdir) = (
            root.canonicalize().map_err(|e| e.to_string())?,
            dir.canonicalize().map_err(|e| e.to_string())?,
        );
        if cdir.parent() != Some(croot.as_path()) {
            return Err(format!(
                "{} leads outside {}",
                dir.display(),
                root.display()
            ));
        }
        let path = cdir.to_string_lossy().into_owned();
        let p = match self.db.project_by_path(&path).map_err(|e| e.to_string())? {
            Some(p) if !p.hidden => {
                return Err(format!(
                    "{path} is already a project in AgentDeck. Remove that project or pick another name."
                ))
            }
            Some(p) => {
                let taken = self
                    .db
                    .list_bots()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .any(|b| b.id != bot.id && b.workspace_id.as_deref() == Some(p.id.as_str()));
                if taken {
                    return Err(format!("{path} is another bot's workspace"));
                }
                p
            }
            None => self
                .db
                .add_hidden_project(bot.name.trim(), &path)
                .map_err(|e| e.to_string())?,
        };
        self.db
            .set_bot_workspace(&bot.id, &p.id)
            .map_err(|e| e.to_string())?;
        // Milestone 1 kept the memory in the bot's project.
        if let Some(proj) = bot
            .project_id
            .as_deref()
            .and_then(|id| self.db.project(id).ok().flatten())
        {
            move_old_memory(&old_memory_path(&proj.path, &bot.name), &cdir);
        }
        let memory = ensure_memory(&cdir, &bot.name)?;
        Ok((p, memory))
    }

    /// Every bot has its workspace (after AgentDeck starts: bots from milestone 1 get theirs,
    /// and folders removed by hand come back).
    pub fn ensure_workspaces(&self) {
        let _ = self.db.prune_bot_sessions();
        for b in self.db.list_bots().unwrap_or_default() {
            if let Err(e) = self.ensure_workspace(&b) {
                eprintln!("agentdeck: no workspace for bot {}: {e}", b.id);
            }
        }
    }

    /// Makes (empty `id`) or changes a bot. The agent and model were checked by the caller.
    pub fn save(&self, id: &str, f: &BotFields<'_>) -> Result<Bot, String> {
        let name = f.name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return Err(format!("Give the bot a name (up to {MAX_NAME} characters)"));
        }
        if name.chars().any(char::is_control) {
            return Err("Use a plain name, on one line".into());
        }
        let role = f.role.trim();
        if role.chars().count() > MAX_ROLE || role.contains('\n') {
            return Err(format!(
                "Keep the job to one line (up to {MAX_ROLE} characters)"
            ));
        }
        if f.instructions.len() > MAX_INSTRUCTIONS {
            return Err("The instructions are too long (over 20,000 characters)".into());
        }
        if !COLORS.contains(&f.color) {
            return Err(format!("Unknown color: {}", f.color));
        }
        let project_id = f.project_id.filter(|p| !p.is_empty());
        if let Some(pid) = project_id {
            self.pick_project(pid)?;
        }
        let old = if id.is_empty() {
            None
        } else {
            Some(self.get(id)?)
        };
        let folder = folder_name(name);
        for b in self
            .db
            .list_bots()
            .map_err(|e| e.to_string())?
            .iter()
            .filter(|b| b.id != id)
        {
            if b.name.to_lowercase() == name.to_lowercase() {
                return Err(format!("There's already a bot called {}", b.name));
            }
            if folder_name(&b.name).to_lowercase() == folder.to_lowercase() {
                return Err(format!(
                    "{} would share the workspace folder {folder}. Pick another name.",
                    b.name
                ));
            }
        }
        // A new name: the workspace folder follows it.
        if let Some(o) = &old {
            if folder_name(&o.name) != folder {
                self.move_workspace(o, name)?;
            }
        }
        let fields = BotFields {
            name,
            role,
            project_id,
            ..f.clone()
        };
        let bot = self.db.save_bot(id, &fields).map_err(|e| e.to_string())?;
        if let Err(e) = self.ensure_workspace(&bot) {
            if old.is_none() {
                let _ = self.db.delete_bot(&bot.id);
            }
            return Err(e);
        }
        if let Some(o) = &old {
            let same_agent = o.provider == bot.provider && o.profile_id == bot.profile_id;
            if same_agent {
                // The sessions follow the new name, model and effort.
                for place in &bot.places {
                    let sid = place.session_id.as_str();
                    let pname = place
                        .project_id
                        .as_deref()
                        .and_then(|p| self.db.project(p).ok().flatten())
                        .map(|p| p.name);
                    let title =
                        (o.name != bot.name).then(|| session_title(&bot.name, pname.as_deref()));
                    let model = (o.model != bot.model).then_some(bot.model.as_str());
                    if title.is_some() || model.is_some() {
                        if let Ok(Some(r)) =
                            self.db
                                .update_session(sid, None, None, title.as_deref(), model, None)
                        {
                            (self.sink)(Outbound::Session(r));
                        }
                        if let (Some(m), true) = (model, self.agents.is_running(sid)) {
                            let _ = self.agents.set_model(sid, m);
                        }
                    }
                    if o.effort != bot.effort {
                        if let Ok(Some(r)) = self.db.set_session_effort(sid, bot.effort.as_deref())
                        {
                            (self.sink)(Outbound::Session(r));
                        }
                        if self.agents.is_running(sid) {
                            let _ = self.agents.set_effort(sid, bot.effort.as_deref());
                        }
                    }
                }
            } else {
                // Another agent: the old sessions stay as ordinary ones; each place starts anew.
                for place in &o.places {
                    self.let_go(&place.session_id, &format!("{} (earlier)", o.name));
                }
            }
        }
        self.publish(&bot.id);
        self.pump(&bot.id, None);
        self.get(&bot.id)
    }

    /// Renames the bot's workspace folder for its new name, and carries its sessions along.
    fn move_workspace(&self, old: &Bot, new_name: &str) -> Result<(), String> {
        let Some(p) = old
            .workspace_id
            .as_deref()
            .and_then(|w| self.db.project(w).ok().flatten())
        else {
            return Ok(());
        };
        if old.running_task_id.is_some() && old.running_project_id.is_none() {
            return Err(
                "Wait for the task in its workspace to end, or cancel it, before renaming".into(),
            );
        }
        let from = PathBuf::from(&p.path);
        let to = self.root().join(folder_name(new_name));
        let same = from
            .canonicalize()
            .ok()
            .is_some_and(|f| to.canonicalize().ok() == Some(f));
        if to.exists() && !same {
            return Err(format!(
                "There's already a folder called {} in {}",
                folder_name(new_name),
                self.root().display()
            ));
        }
        let sessions: Vec<_> = self
            .db
            .list_sessions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.project_id == p.id)
            .collect();
        for s in &sessions {
            if self.agents.is_running(&s.id) {
                self.agents.stop(&s.id);
            }
        }
        if from.is_dir() {
            std::fs::rename(&from, &to)
                .map_err(|e| format!("Couldn't rename the workspace folder: {e}"))?;
        } else {
            std::fs::create_dir_all(&to)
                .map_err(|e| format!("Can't make {}: {e}", to.display()))?;
        }
        let new_path = to
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        self.db
            .move_project(&p.id, new_name.trim(), &new_path)
            .map_err(|e| e.to_string())?;
        // Claude Code finds a conversation by its folder, so it's copied to the new one; Codex
        // resumes from anywhere; the others start over in the new folder, with the brief again.
        for s in sessions {
            let Some(agent_id) = s.provider_session_id.as_deref() else {
                continue;
            };
            let kept = match s.provider.as_str() {
                "claude-code" => {
                    let projects = crate::context::Homes::from_home(&self.home)
                        .claude
                        .join("projects");
                    crate::import::carry_claude_conversation(
                        &projects, agent_id, &p.path, &new_path,
                    )
                }
                "codex" => true,
                _ => false,
            };
            if !kept {
                if let Ok(Some(r)) = self.db.update_session_agent_id_clear(&s.id) {
                    (self.sink)(Outbound::Session(r));
                }
                let _ = self.db.set_bot_briefed(&s.id, false);
            }
        }
        Ok(())
    }

    /// A session the bot no longer uses becomes an ordinary one, named so you can tell.
    fn let_go(&self, sid: &str, title: &str) {
        let _ = self.db.drop_bot_session(sid);
        if let Ok(Some(r)) = self.db.rename_session(sid, title) {
            (self.sink)(Outbound::Session(r));
        }
    }

    /// Deletes a bot, its tasks and schedules. A task in progress is stopped; its sessions
    /// stay, and its workspace becomes an ordinary project when it has any (the folder stays).
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let bot = self.get(id)?;
        if let (Some(sid), Some(_)) = (&bot.running_session_id, &bot.running_task_id) {
            if self.agents.interrupt(sid).is_err() {
                self.agents.stop(sid);
            }
        }
        self.db.delete_bot(id).map_err(|e| e.to_string())?;
        self.needs().remove(id);
        for place in &bot.places {
            self.let_go(&place.session_id, &format!("{} (deleted bot)", bot.name));
        }
        if let Some(wid) = &bot.workspace_id {
            let used = self
                .db
                .list_sessions()
                .map_err(|e| e.to_string())?
                .iter()
                .any(|s| &s.project_id == wid);
            if used {
                self.db
                    .set_project_hidden(wid, false)
                    .map_err(|e| e.to_string())?;
            } else {
                self.db.remove_project(wid).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// One of the bot's sessions was deleted: a task in it fails, and the next task in that
    /// place starts a new one.
    pub fn session_deleted(&self, sid: &str) {
        let Ok(Some(bot)) = self.db.bot_for_session(sid) else {
            return;
        };
        if let (Some(tid), Some(rs)) = (&bot.running_task_id, &bot.running_session_id) {
            if rs == sid {
                if let Ok(Some(t)) =
                    self.db
                        .finish_bot_task(tid, "failed", None, Some("Its session was deleted"))
                {
                    self.publish_task(&t);
                }
                self.needs().remove(&bot.id);
            }
        }
        let _ = self.db.drop_bot_session(sid);
        self.pump(&bot.id, Some(sid));
        self.publish(&bot.id);
    }

    /// A project was removed: tasks working in it fail, schedules for it stop, and bots that
    /// had it as their default project work in their workspace instead.
    pub fn project_removed(&self, project_id: &str) {
        for t in self.db.running_bot_tasks_in(project_id).unwrap_or_default() {
            if let Ok(Some(t)) =
                self.db
                    .finish_bot_task(&t.id, "failed", None, Some("Its project was removed"))
            {
                self.publish_task(&t);
            }
        }
        let _ = self.db.prune_bot_sessions();
        for b in self.db.list_bots().unwrap_or_default() {
            for s in self.db.bot_schedules(&b.id).unwrap_or_default() {
                if s.project_id.as_deref() == Some(project_id) && s.enabled {
                    if let Ok(Some(s)) = self.db.set_bot_schedule_enabled(&s.id, false, None) {
                        self.publish_schedule(&s);
                    }
                }
            }
            self.pump(&b.id, None);
            self.publish(&b.id);
        }
    }

    pub fn tasks(&self, bot_id: &str, limit: i64) -> Result<Vec<BotTask>, String> {
        self.db.bot_tasks(bot_id, limit).map_err(|e| e.to_string())
    }

    /// What a task handed to other bots.
    pub fn children(&self, task_id: &str) -> Result<Vec<BotTask>, String> {
        self.db
            .bot_task_children(task_id)
            .map_err(|e| e.to_string())
    }

    /// Whether the bot can hand tasks to other bots: its agent takes the `agentdeck` MCP
    /// server, the server is up, and there are other bots.
    pub fn can_delegate(&self, bot: &Bot) -> bool {
        self.delegation.load(Ordering::SeqCst)
            && crate::mcp::supports(&bot.provider)
            && self
                .db
                .list_bots()
                .is_ok_and(|all| all.iter().any(|b| b.id != bot.id))
    }

    /// Queues a task. `project_id`: the project it works in; None: the bot's workspace.
    pub fn add_task(
        &self,
        bot_id: &str,
        text: &str,
        source: &str,
        project_id: Option<&str>,
    ) -> Result<BotTask, String> {
        self.add_task_from(bot_id, text, source, project_id, None)
    }

    /// Queues a task another bot's task (`parent_id`) handed over (`source` = "bot:<id>").
    pub fn add_task_from(
        &self,
        bot_id: &str,
        text: &str,
        source: &str,
        project_id: Option<&str>,
        parent_id: Option<&str>,
    ) -> Result<BotTask, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Write the task first".into());
        }
        if text.len() > MAX_TASK {
            return Err("That task is longer than 100,000 characters".into());
        }
        let bot = self.get(bot_id)?;
        let project_id =
            project_id.filter(|p| !p.is_empty() && Some(*p) != bot.workspace_id.as_deref());
        if let Some(pid) = project_id {
            self.pick_project(pid)?;
        }
        let t = self
            .db
            .add_bot_task(bot_id, text, source, project_id, parent_id)
            .map_err(|e| e.to_string())?;
        self.publish_task(&t);
        // The parent's count of handed-on tasks changed.
        if let Some(p) = parent_id.and_then(|p| self.db.bot_task(p).ok().flatten()) {
            self.publish_task(&p);
        }
        self.pump(bot_id, None);
        self.publish(bot_id);
        self.task(&t.id)
    }

    fn task(&self, id: &str) -> Result<BotTask, String> {
        self.db
            .bot_task(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Task not found".into())
    }

    /// A queued task is dropped; a running one is stopped (its turn is interrupted).
    pub fn cancel(&self, task_id: &str) -> Result<BotTask, String> {
        let t = self.task(task_id)?;
        let was_running = t.status == "running";
        let Some(done) = self
            .db
            .finish_bot_task(task_id, "cancelled", None, None)
            .map_err(|e| e.to_string())?
        else {
            return Err("This task has already ended".into());
        };
        self.publish_task(&done);
        // What it handed to other bots and hasn't started yet goes too.
        for child in self.db.bot_task_children(task_id).unwrap_or_default() {
            if child.status == "queued" {
                let _ = self.cancel(&child.id);
            }
        }
        if was_running {
            self.set_needs(&t.bot_id, false);
            match t.session_id.as_deref() {
                // The interrupted turn's end starts the next task, so it can't be taken for
                // that task's own end.
                Some(sid) if self.agents.is_running(sid) => {
                    if self.agents.interrupt(sid).is_err() {
                        self.agents.stop(sid);
                        self.pump(&t.bot_id, Some(sid));
                    }
                }
                sid => self.pump(&t.bot_id, sid),
            }
        }
        self.publish(&t.bot_id);
        Ok(done)
    }

    /// A failed or cancelled task goes back to the end of the queue.
    pub fn retry(&self, task_id: &str) -> Result<BotTask, String> {
        let t = self
            .db
            .requeue_bot_task(task_id)
            .map_err(|e| e.to_string())?
            .ok_or("Only a failed or cancelled task can be tried again")?;
        self.publish_task(&t);
        self.pump(&t.bot_id, None);
        self.publish(&t.bot_id);
        self.task(task_id)
    }

    pub fn set_paused(&self, id: &str, paused: bool) -> Result<Bot, String> {
        self.db
            .set_bot_paused(id, paused)
            .map_err(|e| e.to_string())?
            .ok_or("Bot not found")?;
        if !paused {
            self.pump(id, None);
        }
        self.publish(id);
        self.get(id)
    }

    /// The next task in each place starts a new session; the old ones stay as ordinary sessions.
    pub fn restart_session(&self, id: &str) -> Result<Bot, String> {
        let bot = self.get(id)?;
        if bot.running_task_id.is_some() {
            return Err("Wait for the task in progress to end, or cancel it, first".into());
        }
        for place in &bot.places {
            self.let_go(&place.session_id, &format!("{} (earlier)", bot.name));
        }
        self.publish(id);
        self.get(id)
    }

    pub fn read_memory(&self, id: &str) -> Result<Memory, String> {
        let bot = self.get(id)?;
        let (_, path) = self.ensure_workspace(&bot)?;
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Ok(Memory {
            path: path.to_string_lossy().into_owned(),
            text,
        })
    }

    pub fn write_memory(&self, id: &str, text: &str) -> Result<Memory, String> {
        if text.len() > MAX_MEMORY {
            return Err("The memory file is too long (over 200 KB)".into());
        }
        let bot = self.get(id)?;
        let (_, path) = self.ensure_workspace(&bot)?;
        std::fs::write(&path, text).map_err(|e| format!("Can't save the memory file: {e}"))?;
        Ok(Memory {
            path: path.to_string_lossy().into_owned(),
            text: text.to_string(),
        })
    }

    /// Starts every bot's next task (after AgentDeck starts: no turn is running then).
    pub fn resume_all(&self) {
        for b in self.db.list_bots().unwrap_or_default() {
            let free = self
                .db
                .next_bot_task(&b.id)
                .ok()
                .flatten()
                .and_then(|t| {
                    self.db
                        .bot_session(&b.id, t.project_id.as_deref())
                        .ok()
                        .flatten()
                })
                .map(|(sid, _)| sid);
            self.pump(&b.id, free.as_deref());
        }
    }

    /// Starts the bot's oldest queued task if it isn't paused and nothing of it is running.
    /// `free`: a session of the bot that just ended a turn, whatever its status says yet.
    pub fn pump(&self, bot_id: &str, free: Option<&str>) {
        let _gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let Ok(Some(bot)) = self.db.bot(bot_id) else {
                return;
            };
            if bot.paused || bot.running_task_id.is_some() {
                return;
            }
            let Ok(Some(next)) = self.db.next_bot_task(bot_id) else {
                return;
            };
            // A message you sent the bot directly in that place is still being worked on.
            let busy = self
                .db
                .bot_session(bot_id, next.project_id.as_deref())
                .ok()
                .flatten()
                .filter(|(sid, _)| Some(sid.as_str()) != free)
                .and_then(|(sid, _)| self.db.session(&sid).ok().flatten())
                .is_some_and(|s| s.status == "running" || s.status == "waiting");
            if busy {
                return;
            }
            if !self.db.start_bot_task(&next.id).unwrap_or(false) {
                return;
            }
            let Ok(task) = self.task(&next.id) else {
                return;
            };
            self.publish_task(&task);
            match self.start(&bot, &task) {
                Ok(()) => {
                    if let Ok(t) = self.task(&task.id) {
                        self.publish_task(&t);
                    }
                    self.publish(bot_id);
                    return;
                }
                Err(e) => {
                    if let Ok(Some(t)) = self.db.finish_bot_task(
                        &task.id,
                        "failed",
                        None,
                        Some(&format!("Couldn't start: {e}")),
                    ) {
                        self.publish_task(&t);
                    }
                    self.publish(bot_id);
                }
            }
        }
    }

    /// Makes sure the bot's session for the task's place exists and sends it the task.
    fn start(&self, bot: &Bot, task: &BotTask) -> Result<(), String> {
        let (ws, memory) = self.ensure_workspace(bot)?;
        let place = task
            .project_id
            .as_deref()
            .filter(|p| Some(*p) != bot.workspace_id.as_deref() && *p != ws.id);
        let project = match place {
            None => ws.clone(),
            Some(pid) => {
                let p = self
                    .db
                    .project(pid)
                    .map_err(|e| e.to_string())?
                    .ok_or("This task's project was removed")?;
                if p.hidden {
                    return Err("That folder is a bot's workspace, not a project".into());
                }
                if !Path::new(&p.path).is_dir() {
                    return Err(format!("The folder {} doesn't exist anymore", p.path));
                }
                p
            }
        };
        let existing = self
            .db
            .bot_session(&bot.id, place)
            .map_err(|e| e.to_string())?
            .and_then(|(sid, briefed)| Some((self.db.session(&sid).ok().flatten()?, briefed)));
        let (session, briefed) = match existing {
            Some(x) => x,
            None => {
                let title = session_title(&bot.name, place.map(|_| project.name.as_str()));
                let s = self
                    .db
                    .create_session(&project.id, &bot.provider, &bot.model, &title)
                    .map_err(|e| e.to_string())?;
                if bot.effort.is_some() {
                    self.db
                        .set_session_effort(&s.id, bot.effort.as_deref())
                        .map_err(|e| e.to_string())?;
                }
                let s = self
                    .db
                    .set_session_profile(&s.id, bot.profile_id.as_deref())
                    .map_err(|e| e.to_string())?
                    .unwrap_or(s);
                self.db
                    .set_bot_session(&bot.id, place, &s.id)
                    .map_err(|e| e.to_string())?;
                (self.sink)(Outbound::Session(s.clone()));
                (s, false)
            }
        };
        self.db
            .set_bot_task_session(&task.id, &session.id)
            .map_err(|e| e.to_string())?;
        let memory = memory.to_string_lossy();
        let from = source_label(&self.db, &task.source);
        // The bot that handed it over, by name, for the conversation's label.
        let from_name = task
            .source
            .strip_prefix("bot:")
            .and_then(|id| self.db.bot(id).ok().flatten())
            .map(|b| b.name);
        let header = task_header(task, &from, &project.path, crate::db::now_ms());
        let prompt = if briefed {
            format!("{header}\n\n(Your memory file: {memory}. Read it first and keep it current.)")
        } else {
            format!(
                "{}\n\n{header}",
                brief(bot, &ws.path, &memory, self.can_delegate(bot))
            )
        };
        self.record(
            &session.id,
            "bot_task",
            &json!({ "botId": bot.id, "taskId": task.id, "source": task.source, "fromName": from_name, "projectId": place, "text": task.text, "prompt": prompt.trim() }),
        );
        self.agents.send(&session.id, &prompt)?;
        if !briefed {
            self.db
                .set_bot_briefed(&session.id, true)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Called for every stored event that ends a turn: the bot's running task ends with it
    /// (when it's that task's session), and the next one starts.
    pub fn on_event(&self, ev: &StoredEvent) {
        let Some(end) = turn_end(ev) else { return };
        let Ok(Some(bot)) = self.db.bot_for_session(&ev.session_id) else {
            return;
        };
        let mine = bot.running_session_id.as_deref() == Some(ev.session_id.as_str());
        if mine || bot.running_task_id.is_none() {
            self.set_needs(&bot.id, false);
        }
        if let (Some(tid), true) = (&bot.running_task_id, mine) {
            let (status, result, error) = match end {
                TurnEnd::Reply(t) => ("done", Some(clip(&t)), None),
                TurnEnd::NoReply => ("done", Some(String::new()), None),
                TurnEnd::Failed(why) => ("failed", None, Some(why)),
            };
            if let Ok(Some(t)) =
                self.db
                    .finish_bot_task(tid, status, result.as_deref(), error.as_deref())
            {
                self.publish_task(&t);
            }
        }
        self.pump(&bot.id, Some(&ev.session_id));
        self.publish(&bot.id);
    }

    /// Requests from a bot's session while a task runs there: ordinary approvals are allowed,
    /// risky ones, ones outside the task's folder and the workspace, and questions wait for the
    /// user (the bot then needs you).
    pub fn on_request(&self, ev: &StoredEvent) {
        let Ok(Some(bot)) = self.db.bot_for_session(&ev.session_id) else {
            return;
        };
        let mine = bot.running_task_id.is_some()
            && bot.running_session_id.as_deref() == Some(ev.session_id.as_str());
        // The agent process ended in the middle of a task: no turn end will come for it.
        if ev.kind == "process_exit" && ev.source == "agentdeck" {
            if let (Some(tid), true) = (&bot.running_task_id, mine) {
                if let Ok(Some(t)) = self.db.finish_bot_task(
                    tid,
                    "failed",
                    None,
                    Some("The agent stopped before the task was done"),
                ) {
                    self.publish_task(&t);
                }
                self.needs().remove(&bot.id);
                self.pump(&bot.id, Some(&ev.session_id));
                self.publish(&bot.id);
            }
            return;
        }
        if ev.kind == "permission_decision" || ev.kind == "question_answered" {
            self.set_needs(&bot.id, false);
            return;
        }
        // A message you sent the bot yourself: you're there to answer.
        let (Some(task_id), true) = (bot.running_task_id.clone(), mine) else {
            return;
        };
        // Its own agentdeck tools (handing work to other bots) never wait for the user.
        let v = match crate::mcp::requested_tool(ev) {
            Some(_) => unattended::request_id(ev).map(|request_id| Verdict::Allow { request_id }),
            None => unattended::verdict_with(&self.db, ev, &[bot.workspace.as_str()]),
        };
        let Some(v) = v else {
            return;
        };
        match v {
            Verdict::Allow { request_id } => unattended::allow(
                &self.db,
                &self.agents,
                &self.sink,
                &ev.session_id,
                &request_id,
                "bot",
            ),
            Verdict::Hold { request_id, why } => {
                self.record(
                    &ev.session_id,
                    "bot_needs_you",
                    &json!({ "botId": bot.id, "taskId": task_id, "requestId": request_id, "why": why }),
                );
                self.set_needs(&bot.id, true);
            }
            // Noted too, so a question can be answered from your phone (Telegram).
            Verdict::Question => {
                if let Some(request_id) = unattended::request_id(ev) {
                    self.record(
                        &ev.session_id,
                        "bot_needs_you",
                        &json!({ "botId": bot.id, "taskId": task_id, "requestId": request_id, "question": true }),
                    );
                }
                self.set_needs(&bot.id, true);
            }
        }
    }

    /* ------------------------------------------------------------ schedules */

    pub fn schedules(&self, bot_id: &str) -> Result<Vec<BotSchedule>, String> {
        self.db.bot_schedules(bot_id).map_err(|e| e.to_string())
    }

    fn schedule(&self, id: &str) -> Result<BotSchedule, String> {
        self.db
            .bot_schedule(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Schedule not found".into())
    }

    /// Makes or changes a schedule; its next run is worked out from now.
    pub fn save_schedule(&self, input: &ScheduleInput) -> Result<BotSchedule, String> {
        self.save_schedule_in(input, crate::db::now_ms(), &chrono::Local)
    }

    pub fn save_schedule_in<Tz: chrono::TimeZone>(
        &self,
        input: &ScheduleInput,
        now: i64,
        tz: &Tz,
    ) -> Result<BotSchedule, String> {
        let text = input.text.trim();
        if text.is_empty() {
            return Err("Say what the bot should do".into());
        }
        if text.len() > MAX_TASK {
            return Err("That task is longer than 100,000 characters".into());
        }
        let bot = self.get(&input.bot_id)?;
        let project_id = input
            .project_id
            .as_deref()
            .filter(|p| !p.is_empty() && Some(*p) != bot.workspace_id.as_deref());
        if let Some(pid) = project_id {
            self.pick_project(pid)?;
        }
        let (times, weekdays, every) = schedule::check(
            &input.kind,
            &input.times,
            &input.weekdays,
            input.every_minutes,
        )?;
        let id = input.id.clone().unwrap_or_default();
        let old = if id.is_empty() {
            if self.schedules(&bot.id)?.len() >= MAX_SCHEDULES {
                return Err(format!("A bot can have up to {MAX_SCHEDULES} schedules"));
            }
            None
        } else {
            let o = self.schedule(&id)?;
            if o.bot_id != bot.id {
                return Err("Schedule not found".into());
            }
            Some(o)
        };
        let w = schedule::when(&input.kind, &times, &weekdays, every)
            .ok_or("That schedule never runs")?;
        let next = input
            .enabled
            .then(|| schedule::next_run(&w, now, tz))
            .flatten();
        let row = BotSchedule {
            id,
            bot_id: bot.id.clone(),
            text: text.to_string(),
            project_id: project_id.map(str::to_owned),
            kind: input.kind.clone(),
            times,
            weekdays,
            every_minutes: every,
            enabled: input.enabled,
            last_run_at: old.as_ref().and_then(|o| o.last_run_at),
            next_run_at: next,
            last_task_id: old.as_ref().and_then(|o| o.last_task_id.clone()),
            skipped_at: None,
            created_at: old.as_ref().map(|o| o.created_at).unwrap_or(now),
        };
        let s = self.db.save_bot_schedule(&row).map_err(|e| e.to_string())?;
        self.publish_schedule(&s);
        self.publish(&bot.id);
        Ok(s)
    }

    pub fn delete_schedule(&self, id: &str) -> Result<(), String> {
        let s = self.schedule(id)?;
        self.db.delete_bot_schedule(id).map_err(|e| e.to_string())?;
        self.publish(&s.bot_id);
        Ok(())
    }

    pub fn set_schedule_enabled(&self, id: &str, enabled: bool) -> Result<BotSchedule, String> {
        self.set_schedule_enabled_in(id, enabled, crate::db::now_ms(), &chrono::Local)
    }

    pub fn set_schedule_enabled_in<Tz: chrono::TimeZone>(
        &self,
        id: &str,
        enabled: bool,
        now: i64,
        tz: &Tz,
    ) -> Result<BotSchedule, String> {
        let s = self.schedule(id)?;
        let next = if enabled {
            Some(
                rule(&s)
                    .and_then(|w| schedule::next_run(&w, now, tz))
                    .ok_or("That schedule never runs")?,
            )
        } else {
            None
        };
        let s = self
            .db
            .set_bot_schedule_enabled(id, enabled, next)
            .map_err(|e| e.to_string())?
            .ok_or("Schedule not found")?;
        self.publish_schedule(&s);
        self.publish(&s.bot_id);
        Ok(s)
    }

    /// Whether the task a schedule added last is still waiting or working.
    fn still_going(&self, s: &BotSchedule) -> bool {
        s.last_task_id
            .as_deref()
            .and_then(|t| self.db.bot_task(t).ok().flatten())
            .is_some_and(|t| t.status == "queued" || t.status == "running")
    }

    /// "Run now": adds the schedule's task at once; its next run stays as it was.
    pub fn run_schedule(&self, id: &str) -> Result<BotTask, String> {
        let s = self.schedule(id)?;
        if self.still_going(&s) {
            return Err("Its last run is still queued or in progress".into());
        }
        let t = self.add_task(&s.bot_id, &s.text, "schedule", s.project_id.as_deref())?;
        if let Ok(Some(s)) = self.db.set_bot_schedule_run(
            &s.id,
            Some(crate::db::now_ms()),
            Some(&t.id),
            s.next_run_at,
            None,
        ) {
            self.publish_schedule(&s);
        }
        Ok(t)
    }

    /// The scheduler's tick: each due schedule adds its task (unless its last one is still
    /// queued or running), and its next run is worked out.
    pub fn tick(&self, now: i64) {
        self.tick_in(now, &chrono::Local);
    }

    pub fn tick_in<Tz: chrono::TimeZone>(&self, now: i64, tz: &Tz) {
        for s in self.db.due_bot_schedules(now).unwrap_or_default() {
            let due = s.next_run_at.unwrap_or(now);
            let Some(w) = rule(&s) else {
                let _ = self.db.set_bot_schedule_enabled(&s.id, false, None);
                continue;
            };
            let row = if schedule::missed(due, now) {
                // AgentDeck or the Mac was off: not made up, the next run is counted from now.
                self.db.set_bot_schedule_run(
                    &s.id,
                    None,
                    None,
                    schedule::next_run(&w, now, tz),
                    None,
                )
            } else {
                let next = schedule::next_after_run(&w, due, now, tz);
                if self.still_going(&s) {
                    self.db
                        .set_bot_schedule_run(&s.id, None, None, next, Some(now))
                } else {
                    match self.add_task(&s.bot_id, &s.text, "schedule", s.project_id.as_deref()) {
                        Ok(t) => {
                            self.db
                                .set_bot_schedule_run(&s.id, Some(now), Some(&t.id), next, None)
                        }
                        Err(e) => {
                            eprintln!("agentdeck: schedule {} couldn't add its task: {e}", s.id);
                            self.db
                                .set_bot_schedule_run(&s.id, None, None, next, Some(now))
                        }
                    }
                }
            };
            if let Ok(Some(row)) = row {
                self.publish_schedule(&row);
            }
            self.publish(&s.bot_id);
        }
    }
}

/// `~/AgentDeck/Bots`: where bots' workspaces are.
pub fn bots_root(home: &Path) -> PathBuf {
    home.join("AgentDeck").join("Bots")
}

/// True for the stored events the bot runner looks at besides turn ends.
pub fn wants(ev: &StoredEvent) -> bool {
    matches!(
        ev.kind.as_str(),
        "control_request/can_use_tool"
            | "approval"
            | "user_input"
            | "permission_decision"
            | "question_answered"
            | "process_exit"
    )
}

/// A bot runner for other modules' tests (Telegram): a stand-in `claude` that never answers.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;

    pub struct Setup {
        pub db: Arc<Db>,
        pub bots: Arc<Bots>,
        pub agents: Arc<Agents>,
        pub project: String,
        _dir: tempfile::TempDir,
    }

    impl Setup {
        pub fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let fake = dir.path().join("claude");
            std::fs::write(&fake, "#!/bin/sh\nwhile read -r line; do :; done\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let proj = dir.path().join("shop");
            std::fs::create_dir_all(&proj).unwrap();
            let home = dir.path().join("home");
            std::fs::create_dir_all(&home).unwrap();
            let db = Arc::new(Db::open_in_memory().unwrap());
            let project = db
                .add_project("shop", &proj.canonicalize().unwrap().to_string_lossy())
                .unwrap()
                .id;
            let sink: Sink = Arc::new(|_| {});
            let mut claude = Manager::new(db.clone(), sink.clone());
            claude.binary_override = Some(fake);
            let mut codex = CodexManager::new(db.clone(), sink.clone());
            codex.binary_override = Some("/nonexistent/codex".into());
            let agents = Arc::new(Agents {
                db: db.clone(),
                claude: Arc::new(claude),
                codex: Arc::new(codex),
                acp: Arc::new(crate::acp::AcpManager::new(db.clone(), sink.clone())),
                agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
                api: Arc::new(crate::api::ApiManager::new(db.clone(), sink.clone())),
            });
            Self {
                bots: Arc::new(Bots::new(
                    db.clone(),
                    sink,
                    agents.clone(),
                    home.canonicalize().unwrap(),
                )),
                db,
                agents,
                project,
                _dir: dir,
            }
        }

        /// A session a bot's request can be in.
        pub fn session(&self, _bot: &Bot) -> String {
            self.db
                .create_session(&self.project, "claude-code", "default", "Bot")
                .unwrap()
                .id
        }
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            self.agents.shutdown_all();
        }
    }

    pub fn bot_fields(name: &str) -> BotFields<'_> {
        BotFields {
            name,
            role: "Helps",
            instructions: "Be brief.",
            provider: "claude-code",
            model: "default",
            effort: None,
            profile_id: None,
            project_id: None,
            color: "violet",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;
    use crate::schedule::testing::Cet2026;

    struct T {
        db: Arc<Db>,
        bots: Bots,
        project: String,
        dir: tempfile::TempDir,
        out: Arc<Mutex<Vec<Outbound>>>,
    }

    impl T {
        /// The home folder the bots see (a temp folder, never the real one), resolved the way
        /// the app resolves it: on macOS the temp folder sits behind `/var` → `/private/var`.
        fn home(&self) -> PathBuf {
            self.dir.path().join("home").canonicalize().unwrap()
        }
        fn project_dir(&self) -> String {
            self.db.project(&self.project).unwrap().unwrap().path
        }
    }

    /// A stand-in `claude` that takes messages and never answers: turn ends are fed in by hand.
    fn setup() -> T {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("claude");
        std::fs::write(&fake, "#!/bin/sh\nwhile read -r line; do :; done\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let proj = dir.path().join("shop");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        // `home` is a link, like macOS's `/var` → `/private/var`, so a test that compares
        // unresolved paths fails here too and not only on a Mac.
        let home = dir.path().join("home");
        std::fs::create_dir_all(dir.path().join("real-home")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("real-home"), &home).unwrap();
        #[cfg(not(unix))]
        std::fs::create_dir_all(&home).unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let p = db
            .add_project("shop", &proj.canonicalize().unwrap().to_string_lossy())
            .unwrap();
        let out = Arc::new(Mutex::new(Vec::new()));
        let o2 = out.clone();
        let sink: Sink = Arc::new(move |o| o2.lock().unwrap().push(o));
        let mut claude = Manager::new(db.clone(), sink.clone());
        claude.binary_override = Some(fake.clone());
        let mut codex = CodexManager::new(db.clone(), sink.clone());
        codex.binary_override = Some("/nonexistent/codex".into());
        let agents = Arc::new(Agents {
            db: db.clone(),
            claude: Arc::new(claude),
            codex: Arc::new(codex),
            acp: Arc::new(crate::acp::AcpManager::new(db.clone(), sink.clone())),
            agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
            api: Arc::new(crate::api::ApiManager::new(db.clone(), sink.clone())),
        });
        T {
            bots: Bots::new(db.clone(), sink, agents, home.canonicalize().unwrap()),
            db,
            project: p.id,
            dir,
            out,
        }
    }

    impl Drop for T {
        fn drop(&mut self) {
            self.bots.agents.shutdown_all();
        }
    }

    fn fields<'a>(name: &'a str, project: Option<&'a str>) -> BotFields<'a> {
        BotFields {
            name,
            role: "Reviews pull requests in the shop repo",
            instructions: "Be brief.",
            provider: "claude-code",
            model: "default",
            effort: None,
            profile_id: None,
            project_id: project,
            color: "violet",
        }
    }

    fn end(sid: &str, kind: &str, text: &str) -> StoredEvent {
        StoredEvent {
            session_id: sid.into(),
            seq: 1,
            ts: 1,
            source: "claude".into(),
            kind: kind.into(),
            payload: json!({ "result": text, "is_error": kind != "result/success" }),
        }
    }

    fn status(t: &T, id: &str) -> String {
        t.db.bot_task(id).unwrap().unwrap().status
    }

    fn prompts(t: &T, sid: &str) -> Vec<String> {
        t.db.events(sid)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "bot_task")
            .map(|e| e.payload["prompt"].as_str().unwrap().to_string())
            .collect()
    }

    fn running_session(t: &T, bot: &str) -> String {
        t.bots.get(bot).unwrap().running_session_id.unwrap()
    }

    #[test]
    fn names_folders_slugs_and_dates() {
        assert_eq!(slug("PR Reviewer"), "pr-reviewer");
        assert_eq!(slug("  Docs / Writer!! "), "docs-writer");
        assert_eq!(slug("Çiçek Bot"), "çiçek-bot");
        assert_eq!(slug("../.."), "bot");
        assert_eq!(folder_name("PR Reviewer"), "PR Reviewer");
        assert_eq!(folder_name("PR / Review: v2"), "PR - Review- v2");
        assert_eq!(folder_name("  ..hidden  "), "hidden");
        assert_eq!(folder_name("../.."), "Bot");
        assert_eq!(folder_name("/"), "Bot");
        assert_eq!(folder_name("Çiçek   Bot."), "Çiçek Bot");
        assert_eq!(folder_name(&"x".repeat(80)).len(), 60);
        assert_eq!(stamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(stamp(1_793_718_300_000), "2026-11-03 15:05 UTC");
        assert_eq!(stamp(951_782_400_000), "2000-02-29 00:00 UTC");
    }

    #[test]
    fn a_bot_gets_its_own_workspace_with_its_memory_and_no_folder_is_needed() {
        let t = setup();
        let b = t.bots.save("", &fields("PR Reviewer", None)).unwrap();
        let ws = t.home().join("AgentDeck/Bots/PR Reviewer");
        assert_eq!(b.workspace, ws.to_string_lossy());
        assert_eq!(b.project_id, None, "no default project");
        let p =
            t.db.project(b.workspace_id.as_deref().unwrap())
                .unwrap()
                .unwrap();
        assert!(p.hidden, "the workspace is a hidden project");
        assert_eq!(p.name, "PR Reviewer");
        let mem = t.bots.read_memory(&b.id).unwrap();
        assert_eq!(Path::new(&mem.path), ws.join("MEMORY.md"));
        assert!(mem.text.starts_with("# PR Reviewer memory"));
        // Nothing is written into a project.
        assert!(!Path::new(&t.project_dir()).join(".agentdeck").exists());
        // A default project, and checks on names and folders.
        let b2 = t
            .bots
            .save("", &fields("Shop Bot", Some(&t.project)))
            .unwrap();
        assert_eq!(b2.project_id.as_deref(), Some(t.project.as_str()));
        assert!(t
            .bots
            .save("", &fields("pr reviewer", None))
            .unwrap_err()
            .contains("already a bot"));
        assert!(
            t.bots.save("", &fields("PR: Reviewer", None)).is_ok(),
            "PR- Reviewer is another folder"
        );
        assert!(t
            .bots
            .save("", &fields("PR/ Reviewer", None))
            .unwrap_err()
            .contains("workspace folder"));
        assert!(t.bots.save("", &fields("  ", None)).is_err());
        assert!(t
            .bots
            .save("", &fields("Ghost", Some("p-missing")))
            .is_err());
        assert!(
            t.bots
                .save("", &fields("Sneaky", b.workspace_id.as_deref()))
                .unwrap_err()
                .contains("workspace"),
            "another bot's workspace isn't a project"
        );
        let mut bad = fields("Painter", None);
        bad.color = "chartreuse";
        assert!(t.bots.save("", &bad).is_err());
        assert!(t
            .bots
            .write_memory(&b.id, &"x".repeat(MAX_MEMORY + 1))
            .is_err());
        // The memory file is never followed through a link.
        #[cfg(unix)]
        {
            let link = ws.join("MEMORY.md");
            std::fs::remove_file(&link).unwrap();
            std::os::unix::fs::symlink("/etc/hosts", &link).unwrap();
            assert!(t.bots.read_memory(&b.id).unwrap_err().contains("link"));
        }
    }

    #[test]
    fn renaming_moves_the_workspace_and_keeps_its_sessions() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", None)).unwrap();
        t.bots
            .write_memory(&b.id, "# Reviewer memory\n\n- Tests: npm test\n")
            .unwrap();
        std::fs::write(Path::new(&b.workspace).join("notes.txt"), "kept").unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you", None).unwrap();
        let sid = running_session(&t, &b.id);
        assert!(
            t.bots
                .save(&b.id, &fields("Shop Reviewer", None))
                .unwrap_err()
                .contains("Wait"),
            "not while a task works in the workspace"
        );
        t.bots.on_event(&end(&sid, "result/success", "Done"));
        assert_eq!(status(&t, &t1.id), "done");
        let renamed = t.bots.save(&b.id, &fields("Shop Reviewer", None)).unwrap();
        let ws = t.home().join("AgentDeck/Bots/Shop Reviewer");
        assert_eq!(renamed.workspace, ws.to_string_lossy());
        assert!(!t.home().join("AgentDeck/Bots/Reviewer").exists());
        assert_eq!(
            std::fs::read_to_string(ws.join("notes.txt")).unwrap(),
            "kept"
        );
        let mem = t.bots.read_memory(&b.id).unwrap();
        assert!(mem.text.contains("npm test"), "the memory came along");
        assert_eq!(renamed.workspace_id, b.workspace_id, "same project row");
        let s = t.db.session(&sid).unwrap().unwrap();
        assert_eq!(s.title, "Shop Reviewer");
        assert_eq!(t.db.session_dir(&s).unwrap().unwrap(), ws.to_string_lossy());
        assert_eq!(renamed.places.len(), 1, "the session stays the bot's");
        // A folder of that name already there (not this bot's): refused.
        std::fs::create_dir_all(t.home().join("AgentDeck/Bots/Taken")).unwrap();
        assert!(t
            .bots
            .save(&b.id, &fields("Taken", None))
            .unwrap_err()
            .contains("already a folder"));
    }

    #[test]
    fn bots_from_milestone_one_get_a_workspace_and_their_memory_moves_in() {
        let t = setup();
        // A v8 bot: its memory in the project, its session in the project, no workspace yet.
        let old = old_memory_path(&t.project_dir(), "Reviewer");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "# Reviewer memory\n\n- Uses pnpm\n").unwrap();
        let bot =
            t.db.save_bot("", &fields("Reviewer", Some(&t.project)))
                .unwrap();
        let s =
            t.db.create_session(&t.project, "claude-code", "default", "Reviewer")
                .unwrap();
        t.db.set_bot_session(&bot.id, Some(&t.project), &s.id)
            .unwrap();
        t.db.set_bot_briefed(&s.id, true).unwrap();
        assert!(t.db.bot(&bot.id).unwrap().unwrap().workspace_id.is_none());

        t.bots.ensure_workspaces();
        let b = t.bots.get(&bot.id).unwrap();
        let ws = t.home().join("AgentDeck/Bots/Reviewer");
        assert_eq!(b.workspace, ws.to_string_lossy());
        assert_eq!(
            b.project_id.as_deref(),
            Some(t.project.as_str()),
            "its folder is its default project"
        );
        assert_eq!(
            std::fs::read_to_string(ws.join("MEMORY.md")).unwrap(),
            "# Reviewer memory\n\n- Uses pnpm\n"
        );
        assert!(!old.exists(), "the old file is removed once copied");
        assert!(
            !Path::new(&t.project_dir()).join(".agentdeck").exists(),
            "and its empty folders"
        );
        // Its session carries on as its session for that project: no second brief.
        let t1 = t
            .bots
            .add_task(&b.id, "Check the build", "you", Some(&t.project))
            .unwrap();
        assert_eq!(t1.session_id.as_deref(), Some(s.id.as_str()));
        let p = prompts(&t, &s.id);
        assert!(p[0].starts_with("Task from you in "), "{}", p[0]);
        assert!(p[0].contains(&ws.join("MEMORY.md").to_string_lossy().into_owned()));
        // Running it again changes nothing.
        t.bots.ensure_workspaces();
        assert_eq!(t.bots.get(&b.id).unwrap().workspace_id, b.workspace_id);

        // A workspace that already has a memory file keeps it, and the old one stays.
        let other = old_memory_path(&t.project_dir(), "Writer");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "old").unwrap();
        let ws2 = t.home().join("AgentDeck/Bots/Writer");
        std::fs::create_dir_all(&ws2).unwrap();
        std::fs::write(ws2.join("MEMORY.md"), "new").unwrap();
        assert!(!move_old_memory(&other, &ws2));
        assert_eq!(
            std::fs::read_to_string(ws2.join("MEMORY.md")).unwrap(),
            "new"
        );
        assert!(other.exists());
    }

    #[test]
    fn one_session_per_place_and_the_header_names_the_folder() {
        let t = setup();
        let b = t
            .bots
            .save("", &fields("Reviewer", Some(&t.project)))
            .unwrap();
        let t1 = t
            .bots
            .add_task(&b.id, "Tidy your notes", "you", None)
            .unwrap();
        let t2 = t
            .bots
            .add_task(&b.id, "Review PR 12", "you", Some(&t.project))
            .unwrap();
        let t3 = t.bots.add_task(&b.id, "Sum up", "you", None).unwrap();
        assert!(t
            .bots
            .add_task(&b.id, "x", "you", Some("p-missing"))
            .is_err());
        assert_eq!(t1.status, "running");
        let ws_sid = running_session(&t, &b.id);
        let ws = t.db.session(&ws_sid).unwrap().unwrap();
        assert_eq!(
            (Some(ws.project_id.as_str()), ws.title.as_str()),
            (b.workspace_id.as_deref(), "Reviewer")
        );
        let p = prompts(&t, &ws_sid);
        assert!(p[0].starts_with("You're Reviewer, a bot in AgentDeck. Your job: Reviews pull requests in the shop repo.\nBe brief.\n\nYou work through tasks one at a time."), "{}", p[0]);
        assert!(p[0].contains(&format!(
            "Your workspace is {}. Work there when a task names no project",
            b.workspace
        )));
        assert!(p[0].contains(&format!(
            "Your memory file is {}/MEMORY.md. Read it at the start of every task.",
            b.workspace
        )));
        assert!(p[0].contains(&format!("\n\nTask from you in {} (", b.workspace)));
        assert!(p[0].ends_with("UTC):\nTidy your notes"));

        t.bots.on_event(&end(&ws_sid, "result/success", "Tidied."));
        assert_eq!(status(&t, &t1.id), "done");
        assert_eq!(
            status(&t, &t2.id),
            "running",
            "{:?}",
            t.db.bot_task(&t2.id).unwrap().unwrap().error
        );
        // The project task gets a session of its own, in the project, with the brief.
        let proj_sid = running_session(&t, &b.id);
        assert_ne!(proj_sid, ws_sid);
        let ps = t.db.session(&proj_sid).unwrap().unwrap();
        assert_eq!(
            (ps.project_id.as_str(), ps.title.as_str()),
            (t.project.as_str(), "Reviewer · shop")
        );
        let p = prompts(&t, &proj_sid);
        assert!(p[0].starts_with("You're Reviewer"));
        assert!(p[0].contains(&format!("\n\nTask from you in {} (", t.project_dir())));
        assert_eq!(
            t.db.bot_task(&t2.id)
                .unwrap()
                .unwrap()
                .session_id
                .as_deref(),
            Some(proj_sid.as_str())
        );
        let bot = t.bots.get(&b.id).unwrap();
        assert_eq!(bot.running_project_id.as_deref(), Some(t.project.as_str()));
        assert_eq!(bot.places.len(), 2);
        // A turn end in the other place (a message you sent there) doesn't end this task.
        t.bots.on_event(&end(&ws_sid, "result/success", "Hi"));
        assert_eq!(status(&t, &t2.id), "running");
        // (The stand-in agent never says its turn ended; a real one would be idle now.)
        t.db.update_session(&ws_sid, Some("idle"), None, None, None, None)
            .unwrap();
        t.bots
            .on_event(&end(&proj_sid, "result/success", "Two notes."));
        assert_eq!(status(&t, &t2.id), "done");
        // Back in the workspace: the same session, a short header, no second brief.
        assert_eq!(running_session(&t, &b.id), ws_sid);
        let p = prompts(&t, &ws_sid);
        assert!(
            p[1].starts_with(&format!("Task from you in {} (", b.workspace)),
            "{}",
            p[1]
        );
        assert!(p[1].contains("Sum up\n\n(Your memory file: "));
        t.bots
            .on_event(&end(&ws_sid, "result/success", "Summed up."));
        assert_eq!(status(&t, &t3.id), "done");

        // Start fresh lets go of both; their sessions stay as ordinary ones.
        let fresh = t.bots.restart_session(&b.id).unwrap();
        assert!(fresh.places.is_empty());
        assert_eq!(
            t.db.session(&proj_sid).unwrap().unwrap().title,
            "Reviewer (earlier)"
        );
        t.bots.add_task(&b.id, "Again", "you", None).unwrap();
        let again = running_session(&t, &b.id);
        assert_ne!(again, ws_sid);
        assert!(prompts(&t, &again)[0].starts_with("You're Reviewer"));
    }

    #[test]
    fn tasks_run_one_at_a_time_in_order_and_end_with_the_turn() {
        let t = setup();
        let b = t
            .bots
            .save("", &fields("Reviewer", Some(&t.project)))
            .unwrap();
        assert!(t.bots.add_task(&b.id, "   ", "you", None).is_err());
        let t1 = t
            .bots
            .add_task(&b.id, "Review PR 12", "you", Some(&t.project))
            .unwrap();
        let t2 = t
            .bots
            .add_task(&b.id, "Review PR 13", "you", Some(&t.project))
            .unwrap();
        let t3 = t
            .bots
            .add_task(&b.id, "Review PR 14", "you", Some(&t.project))
            .unwrap();
        assert_eq!(t1.status, "running");
        assert_eq!(
            (status(&t, &t2.id), status(&t, &t3.id)),
            ("queued".into(), "queued".into())
        );
        let bot = t.bots.get(&b.id).unwrap();
        assert_eq!(
            (bot.queued, bot.running_task_id.as_deref()),
            (2, Some(t1.id.as_str()))
        );
        assert_eq!(bot.running_text.as_deref(), Some("Review PR 12"));
        let sid = bot.running_session_id.clone().unwrap();
        // A turn end from another session changes nothing.
        let other =
            t.db.create_session(&t.project, "claude-code", "default", "x")
                .unwrap();
        t.bots.on_event(&end(&other.id, "result/success", "Nope"));
        assert_eq!(status(&t, &t1.id), "running");

        t.bots
            .on_event(&end(&sid, "result/success", "Reviewed PR 12: two notes."));
        let d1 = t.db.bot_task(&t1.id).unwrap().unwrap();
        assert_eq!(
            (d1.status.as_str(), d1.result.as_deref()),
            ("done", Some("Reviewed PR 12: two notes."))
        );
        assert!(d1.finished_at.is_some());
        assert_eq!(status(&t, &t2.id), "running");
        t.bots.on_event(&end(
            &sid,
            "result/error_during_execution",
            "Out of credits",
        ));
        let d2 = t.db.bot_task(&t2.id).unwrap().unwrap();
        assert_eq!(
            (d2.status.as_str(), d2.error.as_deref()),
            ("failed", Some("Out of credits"))
        );
        assert_eq!(status(&t, &t3.id), "running");
        t.bots.on_event(&end(&sid, "result/success", ""));
        let d3 = t.db.bot_task(&t3.id).unwrap().unwrap();
        assert_eq!(
            (d3.status.as_str(), d3.result.as_deref()),
            ("done", Some(""))
        );
        // Nothing runs now: a turn end (a message you sent yourself) is ignored.
        t.bots.on_event(&end(&sid, "result/success", "Hi"));
        let all = t.bots.tasks(&b.id, 10).unwrap();
        assert_eq!(all.len(), 3);
        assert!(all
            .iter()
            .all(|x| x.status != "running" && x.status != "queued"));
        assert!(t
            .out
            .lock()
            .unwrap()
            .iter()
            .any(|o| matches!(o, Outbound::BotTask(x) if x.id == t3.id && x.status == "done")));
    }

    #[test]
    fn pause_resume_cancel_and_retry() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", None)).unwrap();
        t.bots.set_paused(&b.id, true).unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you", None).unwrap();
        let t2 = t.bots.add_task(&b.id, "Two", "you", None).unwrap();
        let t3 = t.bots.add_task(&b.id, "Three", "you", None).unwrap();
        assert_eq!(t1.status, "queued", "a paused bot starts nothing");
        assert!(t.bots.get(&b.id).unwrap().places.is_empty());
        let bot = t.bots.set_paused(&b.id, false).unwrap();
        assert_eq!(bot.running_task_id.as_deref(), Some(t1.id.as_str()));
        let sid = bot.running_session_id.unwrap();

        // A queued task is just dropped.
        assert_eq!(t.bots.cancel(&t2.id).unwrap().status, "cancelled");
        assert!(t.bots.cancel(&t2.id).is_err());
        // The running one is stopped; the next starts when the interrupted turn ends, and
        // that end isn't taken for the next task's.
        assert_eq!(t.bots.cancel(&t1.id).unwrap().status, "cancelled");
        assert_eq!(status(&t, &t3.id), "queued");
        t.bots
            .on_event(&end(&sid, "result/error_during_execution", "Interrupted"));
        assert_eq!(status(&t, &t1.id), "cancelled");
        assert_eq!(status(&t, &t3.id), "running");

        // Retry puts a cancelled task at the end of the queue.
        assert!(
            t.bots.retry(&t3.id).is_err(),
            "only failed or cancelled tasks"
        );
        assert_eq!(t.bots.retry(&t2.id).unwrap().status, "queued");
        t.bots.on_event(&end(&sid, "result/success", "Three done"));
        assert_eq!(status(&t, &t3.id), "done");
        assert_eq!(status(&t, &t2.id), "running");

        // Pausing keeps the running task going.
        t.bots.set_paused(&b.id, true).unwrap();
        let t4 = t.bots.add_task(&b.id, "Four", "you", None).unwrap();
        t.bots.on_event(&end(&sid, "result/success", "Two done"));
        assert_eq!(status(&t, &t2.id), "done");
        assert_eq!(status(&t, &t4.id), "queued");
    }

    #[test]
    fn restart_recovery() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", None)).unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you", None).unwrap();
        let sid = running_session(&t, &b.id);
        assert!(
            t.bots.restart_session(&b.id).is_err(),
            "not while a task runs"
        );
        t.bots.agents.shutdown_all();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while t.bots.agents.is_running(&sid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // AgentDeck closed while the task ran: it goes back to the queue and starts again.
        t.db.recover_after_restart().unwrap();
        assert_eq!(status(&t, &t1.id), "queued");
        t.bots.ensure_workspaces();
        t.bots.resume_all();
        assert_eq!(status(&t, &t1.id), "running");
        assert_eq!(prompts(&t, &sid).len(), 2);
        assert!(
            prompts(&t, &sid)[1].starts_with("Task from you"),
            "same session, no second brief"
        );
    }

    #[test]
    fn a_task_whose_agent_stops_or_session_or_project_goes_fails_and_the_queue_moves_on() {
        let t = setup();
        let b = t
            .bots
            .save("", &fields("Reviewer", Some(&t.project)))
            .unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you", None).unwrap();
        let t2 = t.bots.add_task(&b.id, "Two", "you", None).unwrap();
        let t3 = t.bots.add_task(&b.id, "Three", "you", None).unwrap();
        let t4 = t
            .bots
            .add_task(&b.id, "Four", "you", Some(&t.project))
            .unwrap();
        let t5 = t
            .bots
            .add_task(&b.id, "Five", "you", Some(&t.project))
            .unwrap();
        let sid = running_session(&t, &b.id);
        let exit = StoredEvent {
            session_id: sid.clone(),
            seq: 1,
            ts: 1,
            source: "agentdeck".into(),
            kind: "process_exit".into(),
            payload: json!({ "code": 1 }),
        };
        assert!(wants(&exit));
        t.bots.on_request(&exit);
        assert_eq!(t.db.bot_task(&t1.id).unwrap().unwrap().status, "failed");
        assert_eq!(status(&t, &t2.id), "running");
        t.db.delete_session(&sid).unwrap();
        t.bots.session_deleted(&sid);
        assert_eq!(
            t.db.bot_task(&t2.id).unwrap().unwrap().error.as_deref(),
            Some("Its session was deleted")
        );
        assert_eq!(status(&t, &t3.id), "running");
        let fresh = running_session(&t, &b.id);
        assert_ne!(fresh, sid, "the next task made a new session");
        t.bots.on_event(&end(&fresh, "result/success", "ok"));
        assert_eq!(status(&t, &t4.id), "running");
        // Its project is removed while it works there.
        t.db.remove_project(&t.project).unwrap();
        t.bots.project_removed(&t.project);
        assert_eq!(
            t.db.bot_task(&t4.id).unwrap().unwrap().error.as_deref(),
            Some("Its project was removed")
        );
        let t5 = t.db.bot_task(&t5.id).unwrap().unwrap();
        assert_eq!(t5.status, "failed");
        assert!(t5.error.unwrap().contains("project was removed"));
        let bot = t.bots.get(&b.id).unwrap();
        assert_eq!(
            bot.project_id, None,
            "it works in its workspace by default now"
        );
        assert_eq!(bot.places.len(), 1, "only the workspace session is left");
    }

    #[test]
    fn deleting_keeps_the_sessions_and_shows_the_workspace_as_a_project() {
        let t = setup();
        let b = t
            .bots
            .save("", &fields("Reviewer", Some(&t.project)))
            .unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you", None).unwrap();
        let sid = running_session(&t, &b.id);
        t.bots.delete(&b.id).unwrap();
        assert!(t.bots.get(&b.id).is_err());
        assert!(
            t.db.bot_task(&t1.id).unwrap().is_none(),
            "its tasks go with it"
        );
        assert_eq!(
            t.db.session(&sid).unwrap().unwrap().title,
            "Reviewer (deleted bot)"
        );
        assert!(t.db.bot_for_session(&sid).unwrap().is_none());
        let ws =
            t.db.project(b.workspace_id.as_deref().unwrap())
                .unwrap()
                .unwrap();
        assert!(!ws.hidden, "its session can be found in the sidebar");
        assert!(
            Path::new(&ws.path).join("MEMORY.md").is_file(),
            "the folder stays"
        );
        // A bot that never worked leaves no project behind.
        let b2 = t.bots.save("", &fields("Writer", None)).unwrap();
        t.bots.delete(&b2.id).unwrap();
        assert!(t
            .db
            .project(b2.workspace_id.as_deref().unwrap())
            .unwrap()
            .is_none());
    }

    #[test]
    fn unattended_rule_for_bot_sessions() {
        let t = setup();
        let b = t
            .bots
            .save("", &fields("Reviewer", Some(&t.project)))
            .unwrap();
        let ask = |sid: &str, tool: &str, input: Value| StoredEvent {
            session_id: sid.into(),
            seq: 1,
            ts: 1,
            source: "claude".into(),
            kind: "control_request/can_use_tool".into(),
            payload: json!({ "request_id": "r1", "request": { "tool_name": tool, "input": input } }),
        };
        let held = |sid: &str| {
            t.db.events(sid)
                .unwrap()
                .into_iter()
                .filter(|e| e.kind == "bot_needs_you" && e.payload["question"] != json!(true))
                .count()
        };
        let memory = format!("{}/MEMORY.md", b.workspace);
        let in_project = format!("{}/src/a.ts", t.project_dir());
        // A task in the project: the project and the workspace are in reach, nothing else.
        t.bots
            .add_task(&b.id, "One", "you", Some(&t.project))
            .unwrap();
        let sid = running_session(&t, &b.id);
        t.bots
            .on_request(&ask(&sid, "Bash", json!({ "command": "npm test" })));
        t.bots
            .on_request(&ask(&sid, "Edit", json!({ "file_path": memory })));
        t.bots
            .on_request(&ask(&sid, "Edit", json!({ "file_path": in_project })));
        assert_eq!(held(&sid), 0);
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
        // Risky or outside both folders: waits for you.
        t.bots
            .on_request(&ask(&sid, "Bash", json!({ "command": "git push --force" })));
        let e =
            t.db.events(&sid)
                .unwrap()
                .into_iter()
                .find(|e| e.kind == "bot_needs_you")
                .unwrap();
        assert_eq!(e.payload["why"], "it rewrites or pushes git history");
        assert!(t.bots.get(&b.id).unwrap().needs_you);
        // Your answer clears it.
        t.bots.on_request(&StoredEvent {
            kind: "permission_decision".into(),
            source: "agentdeck".into(),
            ..ask(&sid, "Bash", json!({}))
        });
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
        t.bots
            .on_request(&ask(&sid, "Write", json!({ "file_path": "/etc/hosts" })));
        assert_eq!(held(&sid), 2);
        // Another bot's workspace is outside too.
        let other = t.bots.save("", &fields("Writer", None)).unwrap();
        t.bots.on_request(&ask(
            &sid,
            "Write",
            json!({ "file_path": format!("{}/MEMORY.md", other.workspace) }),
        ));
        assert_eq!(held(&sid), 3);
        // Questions always wait; the turn's end clears the mark.
        t.bots.on_request(&ask(&sid, "AskUserQuestion", json!({})));
        assert!(t.bots.get(&b.id).unwrap().needs_you);
        // Noted too (so Telegram can ask it), as a question rather than a held request.
        assert!(t
            .db
            .events(&sid)
            .unwrap()
            .iter()
            .any(|e| e.kind == "bot_needs_you"
                && e.payload["question"] == json!(true)
                && e.payload["requestId"] == json!("r1")));
        t.bots.on_event(&end(&sid, "result/success", "Done"));
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
        // With no task running (a message you sent yourself), AgentDeck doesn't step in.
        t.bots
            .on_request(&ask(&sid, "Bash", json!({ "command": "rm -rf build" })));
        assert_eq!(held(&sid), 3);
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
        // A task in the workspace: the project is out of reach then.
        t.bots.add_task(&b.id, "Two", "you", None).unwrap();
        let ws_sid = running_session(&t, &b.id);
        t.bots
            .on_request(&ask(&ws_sid, "Edit", json!({ "file_path": memory })));
        assert_eq!(held(&ws_sid), 0);
        t.bots
            .on_request(&ask(&ws_sid, "Edit", json!({ "file_path": in_project })));
        assert_eq!(held(&ws_sid), 1);
    }

    fn sched(bot: &str, kind: &str) -> ScheduleInput {
        ScheduleInput {
            id: None,
            bot_id: bot.into(),
            text: "Check the build".into(),
            project_id: None,
            kind: kind.into(),
            times: vec!["09:00".into()],
            weekdays: vec![],
            every_minutes: Some(60),
            enabled: true,
        }
    }

    /// 2026-11-03 (a Tuesday) at hh:mm in the test zone (UTC+1 then).
    fn nov3(h: i64, m: i64) -> i64 {
        1_793_664_000_000 + ((h - 1) * 60 + m) * 60_000
    }

    #[test]
    fn schedules_add_tasks_when_due_and_never_twice_at_once() {
        let t = setup();
        let b = t
            .bots
            .save("", &fields("Reviewer", Some(&t.project)))
            .unwrap();
        let now = nov3(8, 0);
        let mut input = sched(&b.id, "daily");
        input.project_id = Some(t.project.clone());
        let s = t.bots.save_schedule_in(&input, now, &Cet2026).unwrap();
        assert_eq!(s.next_run_at, Some(nov3(9, 0)));
        assert_eq!(t.bots.get(&b.id).unwrap().next_run_at, Some(nov3(9, 0)));
        assert!(t
            .bots
            .save_schedule_in(
                &ScheduleInput {
                    every_minutes: Some(5),
                    ..sched(&b.id, "interval")
                },
                now,
                &Cet2026
            )
            .is_err());
        assert!(t
            .bots
            .save_schedule_in(
                &ScheduleInput {
                    text: " ".into(),
                    ..sched(&b.id, "daily")
                },
                now,
                &Cet2026
            )
            .is_err());

        // Not yet due.
        t.bots.tick_in(nov3(8, 59), &Cet2026);
        assert!(t.bots.tasks(&b.id, 10).unwrap().is_empty());
        // Due (a tick 20 s late): the task is added, in the schedule's project.
        t.bots.tick_in(nov3(9, 0) + 20_000, &Cet2026);
        let tasks = t.bots.tasks(&b.id, 10).unwrap();
        assert_eq!(tasks.len(), 1);
        let t1 = &tasks[0];
        assert_eq!(
            (
                t1.source.as_str(),
                t1.project_id.as_deref(),
                t1.status.as_str()
            ),
            ("schedule", Some(t.project.as_str()), "running")
        );
        let s = t.db.bot_schedule(&s.id).unwrap().unwrap();
        assert_eq!(s.last_task_id.as_deref(), Some(t1.id.as_str()));
        assert_eq!(
            s.next_run_at,
            Some(nov3(9, 0) + 86_400_000),
            "tomorrow at 09:00"
        );
        // Next day: the first is still running, so this run is skipped (and says so).
        t.bots.tick_in(nov3(9, 0) + 86_400_000 + 5_000, &Cet2026);
        assert_eq!(t.bots.tasks(&b.id, 10).unwrap().len(), 1, "no second task");
        let s = t.db.bot_schedule(&s.id).unwrap().unwrap();
        assert_eq!(s.skipped_at, Some(nov3(9, 0) + 86_400_000 + 5_000));
        assert_eq!(s.next_run_at, Some(nov3(9, 0) + 2 * 86_400_000));
        // "Run now" refuses too while it's still going, then works once it's done.
        assert!(t.bots.run_schedule(&s.id).is_err());
        let sid = running_session(&t, &b.id);
        t.bots
            .on_event(&end(&sid, "result/success", "Build is green."));
        t.db.update_session(&sid, Some("idle"), None, None, None, None)
            .unwrap();
        let now_task = t.bots.run_schedule(&s.id).unwrap();
        assert_eq!(now_task.source, "schedule");
        assert_eq!(
            t.db.bot_schedule(&s.id).unwrap().unwrap().next_run_at,
            s.next_run_at,
            "its next run stays"
        );
        // A paused bot still gets its task; it waits in the queue.
        t.bots
            .on_event(&end(&sid, "result/success", "Green again."));
        t.bots.set_paused(&b.id, true).unwrap();
        t.bots
            .tick_in(nov3(9, 0) + 2 * 86_400_000 + 1_000, &Cet2026);
        let queued: Vec<_> = t
            .bots
            .tasks(&b.id, 10)
            .unwrap()
            .into_iter()
            .filter(|x| x.status == "queued")
            .collect();
        assert_eq!(queued.len(), 1);
        // Off: no next run; on again: from now.
        let off = t
            .bots
            .set_schedule_enabled_in(&s.id, false, now, &Cet2026)
            .unwrap();
        assert_eq!((off.enabled, off.next_run_at), (false, None));
        assert_eq!(t.bots.get(&b.id).unwrap().next_run_at, None);
        let on = t
            .bots
            .set_schedule_enabled_in(&s.id, true, nov3(10, 0), &Cet2026)
            .unwrap();
        assert_eq!(on.next_run_at, Some(nov3(9, 0) + 86_400_000));
        assert!(t
            .out
            .lock()
            .unwrap()
            .iter()
            .any(|o| matches!(o, Outbound::BotSchedule(x) if x.id == s.id)));
        t.bots.delete_schedule(&s.id).unwrap();
        assert!(t.bots.schedules(&b.id).unwrap().is_empty());
    }

    #[test]
    fn missed_runs_are_not_made_up_and_intervals_repeat() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", None)).unwrap();
        let s = t
            .bots
            .save_schedule_in(&sched(&b.id, "daily"), nov3(8, 0), &Cet2026)
            .unwrap();
        // The Mac slept through 09:00 and wakes at 13:00: nothing is added, next is tomorrow.
        t.bots.tick_in(nov3(13, 0), &Cet2026);
        assert!(t.bots.tasks(&b.id, 10).unwrap().is_empty());
        assert_eq!(
            t.db.bot_schedule(&s.id).unwrap().unwrap().next_run_at,
            Some(nov3(9, 0) + 86_400_000)
        );
        // Every 60 minutes, in its workspace.
        let i = t
            .bots
            .save_schedule_in(&sched(&b.id, "interval"), nov3(8, 0), &Cet2026)
            .unwrap();
        assert_eq!(
            (i.kind.as_str(), i.every_minutes, i.times.len()),
            ("interval", Some(60), 0)
        );
        assert_eq!(i.next_run_at, Some(nov3(9, 0)));
        t.bots.tick_in(nov3(9, 0) + 30_000, &Cet2026);
        let tasks = t.bots.tasks(&b.id, 10).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].project_id, None);
        assert_eq!(
            t.db.bot_schedule(&i.id).unwrap().unwrap().next_run_at,
            Some(nov3(10, 0))
        );
        // A schedule for a project that's removed stops.
        let mut p = sched(&b.id, "interval");
        p.project_id = Some(t.project.clone());
        let ps = t.bots.save_schedule_in(&p, nov3(8, 0), &Cet2026).unwrap();
        t.db.remove_project(&t.project).unwrap();
        t.bots.project_removed(&t.project);
        let ps = t.db.bot_schedule(&ps.id).unwrap().unwrap();
        assert_eq!((ps.enabled, ps.next_run_at), (false, None));
        // Deleting the bot deletes its schedules.
        t.bots.delete(&b.id).unwrap();
        assert!(t.db.bot_schedule(&i.id).unwrap().is_none());
    }
}

/// End-to-end with the real `claude` CLI against `scripts/mock_anthropic.py 8787` (AGENTDECK_IT=1).
#[cfg(test)]
mod integration {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore]
    fn queued_tasks_run_in_order_in_the_workspace_and_a_project() {
        if std::env::var("AGENTDECK_IT").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let p = db
            .add_project("project", &proj.canonicalize().unwrap().to_string_lossy())
            .unwrap();
        let cell: Arc<OnceLock<Arc<Bots>>> = Arc::new(OnceLock::new());
        let c2 = cell.clone();
        // As in lib.rs: turn ends and requests reach the runner off the agent's reader thread.
        let sink: Sink = Arc::new(move |o| {
            if let (Outbound::Event(e), Some(b)) = (o, c2.get()) {
                let b = b.clone();
                if turn_end(&e).is_some() {
                    std::thread::spawn(move || b.on_event(&e));
                } else if wants(&e) {
                    std::thread::spawn(move || b.on_request(&e));
                }
            }
        });
        let mut claude = Manager::new(db.clone(), sink.clone());
        claude.extra_env = vec![
            (
                "ANTHROPIC_BASE_URL".into(),
                std::env::var("AGENTDECK_MOCK_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:8787".into()),
            ),
            ("ANTHROPIC_API_KEY".into(), "sk-ant-test-dummy-key".into()),
            (
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
                "1".into(),
            ),
            ("DISABLE_AUTOUPDATER".into(), "1".into()),
        ];
        let agents = Arc::new(Agents {
            db: db.clone(),
            claude: Arc::new(claude),
            codex: Arc::new(CodexManager::new(db.clone(), sink.clone())),
            acp: Arc::new(crate::acp::AcpManager::new(db.clone(), sink.clone())),
            agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
            api: Arc::new(crate::api::ApiManager::new(db.clone(), sink.clone())),
        });
        let bots = Arc::new(Bots::new(
            db.clone(),
            sink,
            agents.clone(),
            home.canonicalize().unwrap(),
        ));
        let _ = cell.set(bots.clone());

        let bot = bots
            .save(
                "",
                &BotFields {
                    name: "Checker",
                    role: "Checks the project",
                    instructions: "",
                    provider: "claude-code",
                    model: "default",
                    effort: None,
                    profile_id: None,
                    project_id: Some(&p.id),
                    color: "teal",
                },
            )
            .unwrap();
        let ws = PathBuf::from(&bot.workspace);
        assert_eq!(
            ws,
            home.canonicalize().unwrap().join("AgentDeck/Bots/Checker")
        );
        let t1 = bots.add_task(&bot.id, "Say hello", "you", None).unwrap();
        // Claude Code asks before writing a file: an ordinary request the bot gets allowed, in
        // the project for one task and in its own workspace for the other.
        let t2 = bots
            .add_task(&bot.id, "write the greeting file", "you", Some(&p.id))
            .unwrap();
        let t3 = bots
            .add_task(&bot.id, "write the greeting file", "you", None)
            .unwrap();
        assert_eq!(
            (t1.status.as_str(), t2.status.as_str(), t3.status.as_str()),
            ("running", "queued", "queued")
        );

        let deadline = Instant::now() + Duration::from_secs(200);
        let ended = |id: &str| {
            db.bot_task(id)
                .unwrap()
                .is_some_and(|t| t.status != "queued" && t.status != "running")
        };
        while !(ended(&t1.id) && ended(&t2.id) && ended(&t3.id)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        let (d1, d2, d3) = (
            db.bot_task(&t1.id).unwrap().unwrap(),
            db.bot_task(&t2.id).unwrap().unwrap(),
            db.bot_task(&t3.id).unwrap().unwrap(),
        );
        let b = bots.get(&bot.id).unwrap();
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_BOT") {
            let all: Vec<_> = b
                .places
                .iter()
                .flat_map(|pl| db.events(&pl.session_id).unwrap())
                .collect();
            std::fs::write(path, serde_json::to_string_pretty(&all).unwrap()).unwrap();
        }
        assert_eq!(d1.status, "done", "{:?}", d1.error);
        assert_eq!(d1.result.as_deref(), Some("Hello from the mock model."));
        for d in [&d2, &d3] {
            assert_eq!(d.status, "done", "{:?}", d.error);
            assert!(
                d.result
                    .as_deref()
                    .unwrap_or("")
                    .contains("file is in place"),
                "{:?}",
                d.result
            );
        }
        assert!(
            proj.join("hello.txt").is_file(),
            "the project task wrote in the project"
        );
        assert!(
            ws.join("hello.txt").is_file(),
            "the workspace task wrote in the workspace"
        );
        assert!(
            d1.finished_at.unwrap() <= d2.started_at.unwrap()
                && d2.finished_at.unwrap() <= d3.started_at.unwrap(),
            "one task at a time, in order"
        );
        // One session per place: the workspace's (tasks 1 and 3) and the project's (task 2).
        assert_eq!(b.places.len(), 2);
        let ws_sid = d1.session_id.clone().unwrap();
        assert_eq!(d3.session_id.as_deref(), Some(ws_sid.as_str()));
        let proj_sid = d2.session_id.clone().unwrap();
        assert_ne!(ws_sid, proj_sid);
        let wss = db.session(&ws_sid).unwrap().unwrap();
        assert_eq!(db.session_dir(&wss).unwrap().unwrap(), bot.workspace);
        assert_eq!(db.session(&proj_sid).unwrap().unwrap().project_id, p.id);
        let prompts = |sid: &str| -> Vec<String> {
            db.events(sid)
                .unwrap()
                .iter()
                .filter(|e| e.kind == "bot_task")
                .filter_map(|e| e.payload["prompt"].as_str().map(str::to_owned))
                .collect()
        };
        let (pw, pp) = (prompts(&ws_sid), prompts(&proj_sid));
        assert_eq!((pw.len(), pp.len()), (2, 1));
        assert!(pw[0].starts_with("You're Checker, a bot in AgentDeck."));
        assert!(pw[1].starts_with(&format!("Task from you in {} (", bot.workspace)));
        assert!(pp[0].starts_with("You're Checker, a bot in AgentDeck."));
        assert!(pp[0].contains(&format!("Task from you in {} (", p.path)));
        // Both Write requests were ordinary: AgentDeck allowed them for the bot.
        for sid in [&ws_sid, &proj_sid] {
            let events = db.events(sid).unwrap();
            assert!(events.iter().any(|e| e.kind == "permission_decision"
                && e.payload["auto"] == "bot"
                && e.payload["decision"] == "allow"));
            assert!(!events.iter().any(|e| e.kind == "bot_needs_you"));
        }
        assert!(ws.join("MEMORY.md").is_file());
        assert!(
            !proj.join(".agentdeck").exists(),
            "nothing of the bot's in the project"
        );
        agents.shutdown_all();
    }
}
