//! Bots: agents that stay.
//!
//! A bot has a name, a job, instructions, an agent and model, a project folder, a memory file it
//! keeps in that folder, and a queue of tasks. It owns one ongoing session (made on its first
//! task) and works through its queue one task at a time: a task starts when the bot isn't paused
//! and nothing else of it is running, and ends when the session's turn ends (the same signal
//! workflows use). While a task runs, the bot's approval requests get the unattended rule
//! (`unattended`): ordinary ones are allowed, risky ones and ones outside its folder wait for
//! the user, and so do questions.

use crate::agents::Agents;
use crate::claude::{Outbound, Sink};
use crate::db::{Bot, BotFields, BotTask, Db, StoredEvent};
use crate::relay::{turn_end, TurnEnd};
use crate::unattended::{self, Verdict};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Accent names a bot's mark can have.
pub const COLORS: &[&str] = &["violet", "blue", "green", "amber", "rose", "teal"];
pub const MAX_NAME: usize = 40;
pub const MAX_ROLE: usize = 200;
pub const MAX_INSTRUCTIONS: usize = 20_000;
pub const MAX_TASK: usize = 100_000;
pub const MAX_MEMORY: usize = 200_000;
const MAX_RESULT: usize = 100_000;

/// The memory file's name: the bot's name in lower case, words joined with dashes.
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

/// `{project}/.agentdeck/bots/{slug}.md`: inside the project, so agents can write it without
/// reaching outside their folder.
pub fn memory_path(project_dir: &str, name: &str) -> PathBuf {
    Path::new(project_dir)
        .join(".agentdeck")
        .join("bots")
        .join(format!("{}.md", slug(name)))
}

/// Adds `.agentdeck/` to the repository's own exclude list (never `.gitignore`), so bots'
/// memory never shows up in diffs or commits. Returns false when the folder isn't a git repo.
pub fn exclude_in_git(project_dir: &Path) -> std::io::Result<bool> {
    let git = project_dir.join(".git");
    if !git.is_dir() {
        return Ok(false);
    }
    let info = git.join("info");
    std::fs::create_dir_all(&info)?;
    let file = info.join("exclude");
    let cur = std::fs::read_to_string(&file).unwrap_or_default();
    if cur.lines().any(|l| {
        matches!(
            l.trim(),
            ".agentdeck/" | "/.agentdeck/" | ".agentdeck" | "/.agentdeck"
        )
    }) {
        return Ok(true);
    }
    let mut text = cur;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("# AgentDeck bots' memory\n.agentdeck/\n");
    std::fs::write(&file, text)?;
    Ok(true)
}

/// The memory file, made with a heading if it isn't there yet. Refuses a `.agentdeck` folder
/// that leads outside the project (a symlink).
pub fn ensure_memory(project_dir: &str, name: &str) -> Result<PathBuf, String> {
    let root = Path::new(project_dir);
    if !root.is_dir() {
        return Err(format!("The folder {project_dir} doesn't exist anymore"));
    }
    let path = memory_path(project_dir, name);
    let dir = path.parent().ok_or("Bad memory path")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Can't make {}: {e}", dir.display()))?;
    let (croot, cdir) = (
        root.canonicalize().map_err(|e| e.to_string())?,
        dir.canonicalize().map_err(|e| e.to_string())?,
    );
    if !cdir.starts_with(&croot) {
        return Err("The bot's memory folder leads outside the project".into());
    }
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
    let _ = exclude_in_git(root);
    Ok(path)
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

/// The first message of a bot's session: who it is, how tasks work, and its memory file.
pub fn brief(bot: &Bot, memory: &str) -> String {
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
    s.push_str(&format!("\n\nYour memory file is {memory}. Read it at the start of every task. When you learn something worth keeping for later tasks (decisions, conventions, where things are, what the user prefers), add it there in a line or two. Keep it short and current; remove what's no longer true."));
    s
}

/// "Task from you (2026-11-03 14:05 UTC):\n…".
pub fn task_header(task: &BotTask, now: i64) -> String {
    format!(
        "Task from {} ({}):\n{}",
        task.source,
        stamp(now),
        task.text.trim()
    )
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

pub struct Bots {
    db: Arc<Db>,
    sink: Sink,
    agents: Arc<Agents>,
    /// One pump at a time, so a bot never starts two tasks.
    gate: Mutex<()>,
    /// Bots with a request or question waiting for the user.
    needs: Mutex<HashSet<String>>,
}

impl Bots {
    pub fn new(db: Arc<Db>, sink: Sink, agents: Arc<Agents>) -> Self {
        Self {
            db,
            sink,
            agents,
            gate: Mutex::new(()),
            needs: Mutex::new(HashSet::new()),
        }
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

    fn project_dir(&self, project_id: &str) -> Result<String, String> {
        Ok(self
            .db
            .project(project_id)
            .map_err(|e| e.to_string())?
            .ok_or("The bot's project was removed")?
            .path)
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
        let dir = self.project_dir(f.project_id)?;
        let old = if id.is_empty() {
            None
        } else {
            Some(self.get(id)?)
        };
        let all = self.db.list_bots().map_err(|e| e.to_string())?;
        for b in all.iter().filter(|b| b.id != id) {
            if b.name.to_lowercase() == name.to_lowercase() {
                return Err(format!("There's already a bot called {}", b.name));
            }
            if b.project_id == f.project_id && slug(&b.name) == slug(name) {
                return Err(format!(
                    "{} works in this folder and would share the memory file {}.md. Pick another name.",
                    b.name,
                    slug(name)
                ));
            }
        }
        // Bring the memory along when the bot is renamed or moves to another folder.
        if let Some(o) = &old {
            if let Ok(odir) = self.project_dir(&o.project_id) {
                let (from, to) = (memory_path(&odir, &o.name), memory_path(&dir, name));
                if from != to && from.is_file() && !to.exists() {
                    if let Some(p) = to.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    let moved = if o.project_id == f.project_id {
                        std::fs::rename(&from, &to)
                    } else {
                        std::fs::copy(&from, &to).map(|_| ())
                    };
                    if let Err(e) = moved {
                        return Err(format!("Couldn't move the memory file: {e}"));
                    }
                }
            }
        }
        ensure_memory(&dir, name)?;
        let fields = BotFields {
            name,
            role,
            ..f.clone()
        };
        let bot = self.db.save_bot(id, &fields).map_err(|e| e.to_string())?;
        if let (Some(o), Some(sid)) = (&old, &bot.session_id) {
            // Same agent: the session follows the new name, model and effort.
            let title = (o.name != bot.name).then_some(bot.name.as_str());
            let model = (o.model != bot.model).then_some(bot.model.as_str());
            if title.is_some() || model.is_some() {
                if let Ok(Some(r)) = self.db.update_session(sid, None, None, title, model, None) {
                    (self.sink)(Outbound::Session(r));
                }
                if let (Some(m), true) = (model, self.agents.is_running(sid)) {
                    let _ = self.agents.set_model(sid, m);
                }
            }
            if o.effort != bot.effort {
                if let Ok(Some(r)) = self.db.set_session_effort(sid, bot.effort.as_deref()) {
                    (self.sink)(Outbound::Session(r));
                }
                if self.agents.is_running(sid) {
                    let _ = self.agents.set_effort(sid, bot.effort.as_deref());
                }
            }
        }
        // Another agent or folder: the old session stays as an ordinary one.
        if let Some(o) = &old {
            if let (Some(old_sid), None) = (&o.session_id, &bot.session_id) {
                self.let_go(old_sid, &format!("{} (earlier)", o.name));
            }
        }
        self.publish(&bot.id);
        self.pump(&bot.id, false);
        self.get(&bot.id)
    }

    /// A session the bot no longer uses becomes an ordinary one, named so you can tell.
    fn let_go(&self, sid: &str, title: &str) {
        if let Ok(Some(r)) = self.db.rename_session(sid, title) {
            (self.sink)(Outbound::Session(r));
        }
    }

    /// Deletes a bot and its tasks. A task in progress is stopped; the session stays.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let bot = self.get(id)?;
        if let (Some(sid), Some(_)) = (&bot.session_id, &bot.running_task_id) {
            if self.agents.interrupt(sid).is_err() {
                self.agents.stop(sid);
            }
        }
        self.db.delete_bot(id).map_err(|e| e.to_string())?;
        self.needs().remove(id);
        if let Some(sid) = &bot.session_id {
            self.let_go(sid, &format!("{} (deleted bot)", bot.name));
        }
        Ok(())
    }

    /// The bot's session was deleted: a task in it fails, and the next task starts a new one.
    pub fn session_deleted(&self, sid: &str) {
        let Ok(Some(bot)) = self.db.bot_for_session(sid) else {
            return;
        };
        if let Some(tid) = &bot.running_task_id {
            if let Ok(Some(t)) =
                self.db
                    .finish_bot_task(tid, "failed", None, Some("Its session was deleted"))
            {
                self.publish_task(&t);
            }
        }
        let _ = self.db.set_bot_session(&bot.id, None);
        self.needs().remove(&bot.id);
        self.pump(&bot.id, true);
        self.publish(&bot.id);
    }

    pub fn tasks(&self, bot_id: &str, limit: i64) -> Result<Vec<BotTask>, String> {
        self.db.bot_tasks(bot_id, limit).map_err(|e| e.to_string())
    }

    pub fn add_task(&self, bot_id: &str, text: &str, source: &str) -> Result<BotTask, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Write the task first".into());
        }
        if text.len() > MAX_TASK {
            return Err("That task is longer than 100,000 characters".into());
        }
        self.get(bot_id)?;
        let t = self
            .db
            .add_bot_task(bot_id, text, source, None)
            .map_err(|e| e.to_string())?;
        self.publish_task(&t);
        self.pump(bot_id, false);
        self.publish(bot_id);
        self.db
            .bot_task(&t.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Task not found".into())
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
        if was_running {
            self.set_needs(&t.bot_id, false);
            let sid = self.get(&t.bot_id).ok().and_then(|b| b.session_id);
            match sid {
                // The interrupted turn's end starts the next task, so it can't be taken for
                // that task's own end.
                Some(sid) if self.agents.is_running(&sid) => {
                    if self.agents.interrupt(&sid).is_err() {
                        self.agents.stop(&sid);
                        self.pump(&t.bot_id, true);
                    }
                }
                _ => self.pump(&t.bot_id, true),
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
        self.pump(&t.bot_id, false);
        self.publish(&t.bot_id);
        self.task(task_id)
    }

    pub fn set_paused(&self, id: &str, paused: bool) -> Result<Bot, String> {
        self.db
            .set_bot_paused(id, paused)
            .map_err(|e| e.to_string())?
            .ok_or("Bot not found")?;
        if !paused {
            self.pump(id, false);
        }
        self.publish(id);
        self.get(id)
    }

    /// The next task starts a new session; the old one stays as an ordinary session.
    pub fn restart_session(&self, id: &str) -> Result<Bot, String> {
        let bot = self.get(id)?;
        if bot.running_task_id.is_some() {
            return Err("Wait for the task in progress to end, or cancel it, first".into());
        }
        if let Some(sid) = &bot.session_id {
            self.db
                .set_bot_session(id, None)
                .map_err(|e| e.to_string())?;
            self.let_go(sid, &format!("{} (earlier)", bot.name));
        }
        self.publish(id);
        self.get(id)
    }

    pub fn read_memory(&self, id: &str) -> Result<Memory, String> {
        let bot = self.get(id)?;
        let path = ensure_memory(&self.project_dir(&bot.project_id)?, &bot.name)?;
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
        let path = ensure_memory(&self.project_dir(&bot.project_id)?, &bot.name)?;
        std::fs::write(&path, text).map_err(|e| format!("Can't save the memory file: {e}"))?;
        Ok(Memory {
            path: path.to_string_lossy().into_owned(),
            text: text.to_string(),
        })
    }

    /// Starts every bot's next task (after AgentDeck starts: no turn is running then).
    pub fn resume_all(&self) {
        for b in self.db.list_bots().unwrap_or_default() {
            self.pump(&b.id, true);
        }
    }

    /// Starts the bot's oldest queued task if it isn't paused and nothing of it is running.
    /// `turn_free`: the bot's session just ended a turn, whatever its status says yet.
    pub fn pump(&self, bot_id: &str, turn_free: bool) {
        let _gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let mut turn_free = turn_free;
        loop {
            let Ok(Some(bot)) = self.db.bot(bot_id) else {
                return;
            };
            if bot.paused || bot.running_task_id.is_some() {
                return;
            }
            // A message you sent the bot directly is still being worked on.
            if !turn_free {
                let busy = bot
                    .session_id
                    .as_deref()
                    .and_then(|sid| self.db.session(sid).ok().flatten())
                    .is_some_and(|s| s.status == "running" || s.status == "waiting");
                if busy {
                    return;
                }
            }
            let Ok(Some(next)) = self.db.next_bot_task(bot_id) else {
                return;
            };
            if !self.db.start_bot_task(&next.id).unwrap_or(false) {
                return;
            }
            let Ok(task) = self.task(&next.id) else {
                return;
            };
            self.publish_task(&task);
            match self.start(&bot, &task) {
                Ok(()) => {
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
                    turn_free = true;
                }
            }
        }
    }

    /// Makes sure the bot's session exists and sends it the task.
    fn start(&self, bot: &Bot, task: &BotTask) -> Result<(), String> {
        let dir = self.project_dir(&bot.project_id)?;
        let memory = ensure_memory(&dir, &bot.name)?;
        let existing = bot
            .session_id
            .as_deref()
            .and_then(|sid| self.db.session(sid).ok().flatten());
        let session = match existing {
            Some(s) => s,
            None => {
                let s = self
                    .db
                    .create_session(&bot.project_id, &bot.provider, &bot.model, &bot.name)
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
                    .set_bot_session(&bot.id, Some(&s.id))
                    .map_err(|e| e.to_string())?;
                (self.sink)(Outbound::Session(s.clone()));
                s
            }
        };
        let fresh = !self
            .db
            .has_user_message(&session.id)
            .map_err(|e| e.to_string())?;
        let memory = memory.to_string_lossy();
        let header = task_header(task, crate::db::now_ms());
        let prompt = if fresh {
            format!("{}\n\n{header}", brief(bot, &memory))
        } else {
            format!("{header}\n\n(Your memory file: {memory}. Read it first and keep it current.)")
        };
        self.record(
            &session.id,
            "bot_task",
            &json!({ "botId": bot.id, "taskId": task.id, "source": task.source, "text": task.text, "prompt": prompt.trim() }),
        );
        self.agents.send(&session.id, &prompt)
    }

    /// Called for every stored event that ends a turn: the bot's running task ends with it,
    /// and the next one starts.
    pub fn on_event(&self, ev: &StoredEvent) {
        let Some(end) = turn_end(ev) else { return };
        let Ok(Some(bot)) = self.db.bot_for_session(&ev.session_id) else {
            return;
        };
        self.set_needs(&bot.id, false);
        if let Some(tid) = &bot.running_task_id {
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
        self.pump(&bot.id, true);
        self.publish(&bot.id);
    }

    /// Requests from a bot's session while a task runs: ordinary approvals are allowed, risky
    /// ones and questions wait for the user (the bot then needs you).
    pub fn on_request(&self, ev: &StoredEvent) {
        // The agent process ended in the middle of a task: no turn end will come for it.
        if ev.kind == "process_exit" && ev.source == "agentdeck" {
            let Ok(Some(bot)) = self.db.bot_for_session(&ev.session_id) else {
                return;
            };
            if let Some(tid) = &bot.running_task_id {
                if let Ok(Some(t)) = self.db.finish_bot_task(
                    tid,
                    "failed",
                    None,
                    Some("The agent stopped before the task was done"),
                ) {
                    self.publish_task(&t);
                }
                self.needs().remove(&bot.id);
                self.pump(&bot.id, true);
                self.publish(&bot.id);
            }
            return;
        }
        if ev.kind == "permission_decision" || ev.kind == "question_answered" {
            if let Ok(Some(bot)) = self.db.bot_for_session(&ev.session_id) {
                self.set_needs(&bot.id, false);
            }
            return;
        }
        let Some(v) = unattended::verdict(&self.db, ev) else {
            return;
        };
        let Ok(Some(bot)) = self.db.bot_for_session(&ev.session_id) else {
            return;
        };
        // A message you sent the bot yourself: you're there to answer.
        let Some(task_id) = bot.running_task_id.clone() else {
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
            Verdict::Question => self.set_needs(&bot.id, true),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;

    struct T {
        db: Arc<Db>,
        bots: Bots,
        project: String,
        dir: tempfile::TempDir,
        out: Arc<Mutex<Vec<Outbound>>>,
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
            bots: Bots::new(db.clone(), sink, agents),
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

    fn fields<'a>(name: &'a str, project: &'a str) -> BotFields<'a> {
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

    #[test]
    fn names_slugs_and_dates() {
        assert_eq!(slug("PR Reviewer"), "pr-reviewer");
        assert_eq!(slug("  Docs / Writer!! "), "docs-writer");
        assert_eq!(slug("Çiçek Bot"), "çiçek-bot");
        assert_eq!(slug("../.."), "bot");
        assert_eq!(stamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(stamp(1_793_718_300_000), "2026-11-03 15:05 UTC");
        assert_eq!(stamp(951_782_400_000), "2000-02-29 00:00 UTC");
    }

    #[test]
    fn memory_file_lives_in_the_project_and_git_ignores_it() {
        let t = setup();
        let dir = t.db.project(&t.project).unwrap().unwrap().path;
        let path = ensure_memory(&dir, "PR Reviewer").unwrap();
        assert_eq!(path, Path::new(&dir).join(".agentdeck/bots/pr-reviewer.md"));
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .starts_with("# PR Reviewer memory"));
        ensure_memory(&dir, "PR Reviewer").unwrap();
        let exclude = std::fs::read_to_string(Path::new(&dir).join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches(".agentdeck/").count(), 1, "{exclude}");
        assert!(!Path::new(&dir).join(".gitignore").exists());
        // Not a repository: nothing to exclude, and no .git is made.
        let plain = t.dir.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        ensure_memory(&plain.to_string_lossy(), "x").unwrap();
        assert!(!plain.join(".git").exists());
        assert!(!exclude_in_git(&plain).unwrap());
    }

    #[test]
    fn saving_checks_names_and_renaming_moves_the_memory() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
        assert_eq!((b.paused, b.queued, b.session_id.clone()), (false, 0, None));
        assert!(t
            .bots
            .save("", &fields("reviewer", &t.project))
            .unwrap_err()
            .contains("already a bot"));
        assert!(t
            .bots
            .save("", &fields("Re-viewer", &t.project))
            .is_ok_and(|_| true));
        assert!(t
            .bots
            .save("", &fields("Re viewer", &t.project))
            .unwrap_err()
            .contains("memory file"));
        assert!(t.bots.save("", &fields("  ", &t.project)).is_err());
        assert!(t.bots.save("", &fields("Ghost", "p-missing")).is_err());
        let mut bad = fields("Painter", &t.project);
        bad.color = "chartreuse";
        assert!(t.bots.save("", &bad).is_err());

        t.bots
            .write_memory(&b.id, "# Reviewer memory\n\n- Tests: npm test\n")
            .unwrap();
        let renamed = t
            .bots
            .save(&b.id, &fields("Shop Reviewer", &t.project))
            .unwrap();
        assert_eq!(renamed.name, "Shop Reviewer");
        let mem = t.bots.read_memory(&b.id).unwrap();
        assert!(mem.path.ends_with(".agentdeck/bots/shop-reviewer.md"));
        assert!(mem.text.contains("npm test"), "the memory came along");
        assert!(t
            .bots
            .write_memory(&b.id, &"x".repeat(MAX_MEMORY + 1))
            .is_err());
    }

    #[test]
    fn tasks_run_one_at_a_time_in_order_and_end_with_the_turn() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
        assert!(t.bots.add_task(&b.id, "   ", "you").is_err());
        let t1 = t.bots.add_task(&b.id, "Review PR 12", "you").unwrap();
        let t2 = t.bots.add_task(&b.id, "Review PR 13", "you").unwrap();
        let t3 = t.bots.add_task(&b.id, "Review PR 14", "you").unwrap();
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
        let sid = bot
            .session_id
            .clone()
            .expect("the first task made the session");
        let s = t.db.session(&sid).unwrap().unwrap();
        assert_eq!(
            (s.title.as_str(), s.project_id.as_str()),
            ("Reviewer", t.project.as_str())
        );

        // The first message carries the brief, then the task.
        let p = prompts(&t, &sid);
        assert_eq!(p.len(), 1);
        assert!(p[0].starts_with("You're Reviewer, a bot in AgentDeck. Your job: Reviews pull requests in the shop repo.\nBe brief.\n\nYou work through tasks one at a time."), "{}", p[0]);
        assert!(p[0].contains(".agentdeck/bots/reviewer.md. Read it at the start of every task."));
        assert!(p[0].contains("\n\nTask from you ("));
        assert!(p[0].ends_with("UTC):\nReview PR 12"));

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
        let p = prompts(&t, &sid);
        assert!(
            p[1].starts_with("Task from you ("),
            "later tasks carry a short header: {}",
            p[1]
        );
        assert!(p[1].contains("Review PR 13"));

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
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
        t.bots.set_paused(&b.id, true).unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you").unwrap();
        let t2 = t.bots.add_task(&b.id, "Two", "you").unwrap();
        let t3 = t.bots.add_task(&b.id, "Three", "you").unwrap();
        assert_eq!(t1.status, "queued", "a paused bot starts nothing");
        assert!(t.bots.get(&b.id).unwrap().session_id.is_none());
        let bot = t.bots.set_paused(&b.id, false).unwrap();
        assert_eq!(bot.running_task_id.as_deref(), Some(t1.id.as_str()));
        let sid = bot.session_id.unwrap();

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
        let t4 = t.bots.add_task(&b.id, "Four", "you").unwrap();
        t.bots.on_event(&end(&sid, "result/success", "Two done"));
        assert_eq!(status(&t, &t2.id), "done");
        assert_eq!(status(&t, &t4.id), "queued");
    }

    #[test]
    fn restart_recovery_and_start_fresh() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you").unwrap();
        let sid = t.bots.get(&b.id).unwrap().session_id.unwrap();
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
        t.bots.resume_all();
        assert_eq!(status(&t, &t1.id), "running");
        assert_eq!(prompts(&t, &sid).len(), 2);
        assert!(
            prompts(&t, &sid)[1].starts_with("Task from you"),
            "same session, no second brief"
        );
        t.bots.on_event(&end(&sid, "result/success", "Done"));

        let fresh = t.bots.restart_session(&b.id).unwrap();
        assert!(fresh.session_id.is_none());
        assert_eq!(
            t.db.session(&sid).unwrap().unwrap().title,
            "Reviewer (earlier)"
        );
        t.bots.add_task(&b.id, "Two", "you").unwrap();
        let sid2 = t.bots.get(&b.id).unwrap().session_id.unwrap();
        assert_ne!(sid, sid2);
        assert!(
            prompts(&t, &sid2)[0].starts_with("You're Reviewer"),
            "a new session gets the brief"
        );
    }

    #[test]
    fn a_task_whose_agent_stops_or_session_goes_fails_and_the_queue_moves_on() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you").unwrap();
        let t2 = t.bots.add_task(&b.id, "Two", "you").unwrap();
        let t3 = t.bots.add_task(&b.id, "Three", "you").unwrap();
        let sid = t.bots.get(&b.id).unwrap().session_id.unwrap();
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
        let d1 = t.db.bot_task(&t1.id).unwrap().unwrap();
        assert_eq!(d1.status, "failed");
        assert_eq!(status(&t, &t2.id), "running");
        t.db.delete_session(&sid).unwrap();
        t.bots.session_deleted(&sid);
        assert_eq!(
            t.db.bot_task(&t2.id).unwrap().unwrap().error.as_deref(),
            Some("Its session was deleted")
        );
        assert_eq!(status(&t, &t3.id), "running");
        let fresh = t.bots.get(&b.id).unwrap().session_id.unwrap();
        assert_ne!(fresh, sid, "the next task made a new session");
    }

    #[test]
    fn deleting_keeps_the_session_as_an_ordinary_one() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
        let t1 = t.bots.add_task(&b.id, "One", "you").unwrap();
        let sid = t.bots.get(&b.id).unwrap().session_id.unwrap();
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
    }

    #[test]
    fn unattended_rule_for_bot_sessions() {
        let t = setup();
        let b = t.bots.save("", &fields("Reviewer", &t.project)).unwrap();
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
                .filter(|e| e.kind == "bot_needs_you")
                .count()
        };
        t.bots.add_task(&b.id, "One", "you").unwrap();
        let sid = t.bots.get(&b.id).unwrap().session_id.unwrap();
        // Ordinary: allowed (here the stand-in agent has no such request, so nothing is held).
        t.bots
            .on_request(&ask(&sid, "Bash", json!({ "command": "npm test" })));
        assert_eq!(held(&sid), 0);
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
        // Risky or outside the folder: waits for you.
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
        // Questions always wait; the turn's end clears the mark.
        t.bots.on_request(&ask(&sid, "AskUserQuestion", json!({})));
        assert!(t.bots.get(&b.id).unwrap().needs_you);
        t.bots.on_event(&end(&sid, "result/success", "Done"));
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
        // With no task running (a message you sent yourself), AgentDeck doesn't step in.
        t.bots
            .on_request(&ask(&sid, "Bash", json!({ "command": "rm -rf build" })));
        assert_eq!(held(&sid), 2);
        assert!(!t.bots.get(&b.id).unwrap().needs_you);
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
    fn two_queued_tasks_run_in_order_and_ordinary_requests_are_allowed() {
        if std::env::var("AGENTDECK_IT").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir_all(proj.join(".git")).unwrap();
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
        let bots = Arc::new(Bots::new(db.clone(), sink, agents.clone()));
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
                    project_id: &p.id,
                    color: "teal",
                },
            )
            .unwrap();
        let t1 = bots.add_task(&bot.id, "Say hello", "you").unwrap();
        // Claude Code asks before writing a file: an ordinary request the bot gets allowed.
        let t2 = bots
            .add_task(&bot.id, "write the greeting file", "you")
            .unwrap();
        assert_eq!(
            (t1.status.as_str(), t2.status.as_str()),
            ("running", "queued")
        );

        let deadline = Instant::now() + Duration::from_secs(150);
        let ended = |id: &str| {
            db.bot_task(id)
                .unwrap()
                .is_some_and(|t| t.status != "queued" && t.status != "running")
        };
        while !(ended(&t1.id) && ended(&t2.id)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        let (d1, d2) = (
            db.bot_task(&t1.id).unwrap().unwrap(),
            db.bot_task(&t2.id).unwrap().unwrap(),
        );
        let sid = bots.get(&bot.id).unwrap().session_id.unwrap();
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_BOT") {
            std::fs::write(
                path,
                serde_json::to_string_pretty(&db.events(&sid).unwrap()).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(d1.status, "done", "{:?}", d1.error);
        assert_eq!(d1.result.as_deref(), Some("Hello from the mock model."));
        assert_eq!(d2.status, "done", "{:?}", d2.error);
        assert!(
            d2.result
                .as_deref()
                .unwrap_or("")
                .contains("file is in place"),
            "{:?}",
            d2.result
        );
        assert!(proj.join("hello.txt").is_file(), "the write went ahead");
        assert!(
            d1.finished_at.unwrap() <= d2.started_at.unwrap(),
            "one task at a time, in order"
        );
        let events = db.events(&sid).unwrap();
        let prompts: Vec<&str> = events
            .iter()
            .filter(|e| e.kind == "bot_task")
            .filter_map(|e| e.payload["prompt"].as_str())
            .collect();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[0].starts_with("You're Checker, a bot in AgentDeck."));
        assert!(prompts[1].starts_with("Task from you ("));
        // The Write request was ordinary: AgentDeck allowed it for the bot.
        assert!(events.iter().any(|e| e.kind == "permission_decision"
            && e.payload["auto"] == "bot"
            && e.payload["decision"] == "allow"));
        assert!(!events.iter().any(|e| e.kind == "bot_needs_you"));
        assert!(proj.join(".agentdeck/bots/checker.md").is_file());
        agents.shutdown_all();
    }
}
