//! AgentDeck desktop shell.
//!
//! Commands exposed to the frontend are narrow: read-only detection and build info, the local
//! session store, and session control for the user's own Claude Code CLI. There is no generic
//! shell, filesystem or process access from the webview.

mod acp;
mod agents;
mod agy;
mod api;
mod approvals;
mod attach;
mod bots;
mod claude;
mod codex;
mod commands;
mod connections;
mod context;
mod db;
mod flow;
mod git;
mod import;
mod limits;
mod mac;
mod mcp;
mod models;
mod providers;
mod pty;
mod relay;
mod remote;
mod schedule;
mod secrets;
mod setup;
mod shellenv;
mod telegram;
mod triggers;
mod unattended;
mod updates;

use acp::AcpManager;
use agents::{Agents, LIVE_PROVIDERS};
use bots::Bots;
use claude::{Decision, Manager, Outbound};
use codex::CodexManager;
use db::{
    Bot, BotFields, BotSchedule, BotTask, Db, FlowRow, Project, RelayRow, SessionRow, StoredEvent,
    Workflow,
};
use flow::Flow;
use relay::Relay;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use tauri::{Emitter, Manager as _, State};

struct AppState {
    db: Arc<Db>,
    agents: Arc<Agents>,
    sink: claude::Sink,
    relay: Arc<Relay>,
    flow: Arc<Flow>,
    bots: Arc<Bots>,
    terms: Arc<pty::Terminals>,
    remote: Arc<remote::Remote>,
    telegram: Arc<telegram::Telegram>,
    triggers: Arc<triggers::Triggers>,
    approvals: Arc<approvals::Approvals>,
}

const MAX_HANDOFF_BYTES: usize = 200_000;

fn first_title(t: &str) -> String {
    let t = t.trim();
    if t.chars().count() > 40 {
        format!("{}…", t.chars().take(40).collect::<String>().trim_end())
    } else {
        t.to_string()
    }
}

fn provider_label(p: &str) -> &'static str {
    agents::label(p)
}

/// The first message a hand-off session receives.
fn handoff_prompt(from_provider: &str, from_title: &str, plan: &str) -> String {
    format!(
        "Implement this plan. {} wrote it in another AgentDeck session on this project (\"{}\"). Read the code it touches before changing anything, follow the plan, and tell me if something in the code contradicts it.\n\n{}",
        provider_label(from_provider),
        from_title,
        plan.trim()
    )
}

impl AppState {
    fn record(&self, sid: &str, kind: &str, payload: &serde_json::Value) {
        if let Ok(Some(ev)) = self.db.append_event(sid, "agentdeck", kind, payload) {
            (self.sink)(Outbound::Event(ev));
        }
    }

    /// A new session for `provider` (on a custom model profile, if given) in a project,
    /// titled after the work it continues.
    fn new_session_for(
        &self,
        project_id: &str,
        provider: &str,
        profile_id: Option<&str>,
        base_title: &str,
    ) -> Result<SessionRow, String> {
        if !LIVE_PROVIDERS.contains(&provider) {
            return Err(format!("AgentDeck can't run {provider} sessions"));
        }
        let profile = match profile_id {
            Some(pid) => Some(
                self.db
                    .profile(pid)
                    .map_err(err)?
                    .ok_or("Model profile not found")?,
            ),
            None => None,
        };
        if let Some(p) = &profile {
            let want = if p.kind == "anthropic" {
                "claude-code"
            } else {
                "api"
            };
            if provider != want {
                return Err(format!("{} runs on {}", p.name, provider_label(want)));
            }
        } else if provider == "api" {
            return Err("Pick a model profile".into());
        }
        let who = profile
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or(provider_label(provider));
        let title = format!("{} ({who})", first_title(base_title));
        let row = self
            .db
            .create_session(project_id, provider, "default", &title)
            .map_err(err)?;
        let row = match profile_id {
            Some(pid) => self
                .db
                .set_session_profile(&row.id, Some(pid))
                .map_err(err)?
                .unwrap_or(row),
            None => row,
        };
        Ok(row)
    }

    /// Creates a linked session for `provider` in the same project and records the link on
    /// both sides. Returns the new session and the prompt to send it.
    fn hand_off(
        &self,
        from_id: &str,
        provider: &str,
        profile_id: Option<&str>,
        plan: &str,
        plan_request_id: Option<&str>,
    ) -> Result<(SessionRow, String), String> {
        if !LIVE_PROVIDERS.contains(&provider) {
            return Err(format!("AgentDeck can't hand work to {provider} yet"));
        }
        let plan = plan.trim();
        if plan.is_empty() {
            return Err("There's no plan to hand off".into());
        }
        if plan.len() > MAX_HANDOFF_BYTES {
            return Err("The plan is too long to hand off (over 200 KB)".into());
        }
        let from = self
            .db
            .session(from_id)
            .map_err(err)?
            .ok_or("Session not found")?;
        let new = self.new_session_for(&from.project_id, provider, profile_id, &from.title)?;
        let prompt = handoff_prompt(&from.provider, &from.title, plan);
        self.record(
            &new.id,
            "handoff_in",
            &serde_json::json!({ "fromSessionId": from.id, "fromProvider": from.provider, "fromTitle": from.title, "plan": plan, "prompt": prompt }),
        );
        self.record(
            &from.id,
            "handoff_out",
            &serde_json::json!({ "toSessionId": new.id, "toProvider": provider, "plan": plan, "planRequestId": plan_request_id }),
        );
        (self.sink)(Outbound::Session(new.clone()));
        Ok((new, prompt))
    }

    fn stop_any(&self, session_id: &str) {
        self.agents.stop(session_id);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
}

type CmdResult<T> = Result<T, String>;
fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

#[tauri::command]
async fn detect_providers() -> CmdResult<Vec<providers::ProviderProbe>> {
    tauri::async_runtime::spawn_blocking(providers::probe_all)
        .await
        .map_err(err)
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<Project>> {
    state.db.list_projects().map_err(err)
}

#[tauri::command]
fn add_project(state: State<'_, AppState>, path: String) -> CmdResult<Project> {
    let p = Path::new(&path);
    if !p.is_absolute() {
        return Err("Choose a folder with a full path".into());
    }
    let canon = p
        .canonicalize()
        .map_err(|e| format!("Can't open that folder: {e}"))?;
    if !canon.is_dir() {
        return Err("That isn't a folder".into());
    }
    if state
        .db
        .project_by_path(&canon.to_string_lossy())
        .map_err(err)?
        .is_some_and(|p| p.hidden)
        || state
            .bots
            .root()
            .canonicalize()
            .is_ok_and(|r| canon.starts_with(r))
    {
        return Err(
            "That folder is where AgentDeck keeps bots' workspaces. Choose a project folder."
                .into(),
        );
    }
    let name = canon
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| canon.display().to_string());
    state
        .db
        .add_project(&name, &canon.to_string_lossy())
        .map_err(err)
}

/// Where AgentDeck makes folders: new projects and the Chats folder.
fn projects_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().home_dir().map_err(err)?.join("AgentDeck"))
}

fn ensure_chats(app: &tauri::AppHandle, db: &Db) -> Result<Project, String> {
    if let Some(p) = db.chats_project().map_err(err)? {
        std::fs::create_dir_all(&p.path).map_err(|e| format!("Can't make {}: {e}", p.path))?;
        return Ok(p);
    }
    let dir = projects_root(app)?.join("Chats");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Can't make {}: {e}", dir.display()))?;
    let canon = dir.canonicalize().map_err(err)?;
    db.add_project_kind("Chats", &canon.to_string_lossy(), "chats")
        .map_err(err)
}

/// The place for sessions without a project: ~/AgentDeck/Chats, made on first use.
#[tauri::command]
fn open_chats(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<Project> {
    ensure_chats(&app, &state.db)
}

/* ------------------------------------------------------------ git */

/// A folder of a project's repository: the project folder, or one of its worktrees.
fn repo_dir(db: &Db, project_id: &str, dir: Option<&str>) -> CmdResult<String> {
    let project = db
        .project(project_id)
        .map_err(err)?
        .ok_or("Project not found")?;
    match dir {
        None => Ok(project.path),
        Some(d) if d == project.path => Ok(project.path),
        Some(d) => {
            let root = git::root(&project.path).ok_or("This project isn't a git repository")?;
            git::worktrees(&root)
                .into_iter()
                .find(|w| w.path == d)
                .map(|w| w.path)
                .ok_or_else(|| "That folder isn't one of this project's worktrees".into())
        }
    }
}

#[tauri::command]
async fn git_overview(
    state: State<'_, AppState>,
    project_id: String,
    dir: Option<String>,
) -> CmdResult<git::Overview> {
    let d = repo_dir(&state.db, &project_id, dir.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || git::overview(&d))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn git_log(
    state: State<'_, AppState>,
    project_id: String,
    skip: usize,
    limit: usize,
) -> CmdResult<Vec<git::Commit>> {
    let d = repo_dir(&state.db, &project_id, None)?;
    tauri::async_runtime::spawn_blocking(move || git::log(&d, skip, limit))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn git_commit_files(
    state: State<'_, AppState>,
    project_id: String,
    hash: String,
) -> CmdResult<Vec<git::ChangedFile>> {
    let d = repo_dir(&state.db, &project_id, None)?;
    tauri::async_runtime::spawn_blocking(move || git::commit_files(&d, &hash))
        .await
        .map_err(err)?
}

/// A file before and after; `new_rev` "WORKTREE" is the file on disk in `dir`.
#[tauri::command]
async fn git_file_pair(
    state: State<'_, AppState>,
    project_id: String,
    dir: Option<String>,
    old_rev: String,
    new_rev: String,
    path: String,
    from: Option<String>,
) -> CmdResult<git::FilePair> {
    let d = repo_dir(&state.db, &project_id, dir.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || {
        git::file_pair(&d, &old_rev, &new_rev, &path, from.as_deref())
    })
    .await
    .map_err(err)?
}

fn session_worktree_of(
    db: &Db,
    session_id: &str,
) -> CmdResult<(SessionRow, String, String, String)> {
    let s = db
        .session(session_id)
        .map_err(err)?
        .ok_or("Session not found")?;
    let project = db
        .project(&s.project_id)
        .map_err(err)?
        .ok_or("Project not found")?;
    let (Some(path), Some(branch)) = (s.worktree.clone(), s.branch.clone()) else {
        return Err("This session works in the project folder, not a worktree".into());
    };
    Ok((s, project.path, path, branch))
}

/// What a session's worktree changed and how it stands against the project's branch.
#[tauri::command]
async fn session_worktree(
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<git::WorktreeState> {
    let (_, project, path, branch) = session_worktree_of(&state.db, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || git::worktree_state(&project, &path, &branch))
        .await
        .map_err(err)?
}

/// Before the first message: give the session its own worktree, or take it away again.
#[tauri::command]
async fn set_session_worktree(
    state: State<'_, AppState>,
    session_id: String,
    on: bool,
) -> CmdResult<SessionRow> {
    let s = state
        .db
        .session(&session_id)
        .map_err(err)?
        .ok_or("Session not found")?;
    let started = s.provider_session_id.is_some()
        || state
            .db
            .events(&session_id)
            .map_err(err)?
            .iter()
            .any(|e| e.kind == "user_message");
    if started {
        return Err("This session has already started in its folder. Start a new session to work in a worktree.".into());
    }
    let project = state
        .db
        .project(&s.project_id)
        .map_err(err)?
        .ok_or("Project not found")?;
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match (on, s.worktree.clone(), s.branch.clone()) {
            (true, None, _) => {
                let name = format!(
                    "{}-{}",
                    agents::label(&s.provider).to_lowercase().replace(' ', "-"),
                    &s.id.trim_start_matches("s-")[..4]
                );
                let (path, branch) = git::add_worktree(&project.path, &name)?;
                db.set_session_worktree(&s.id, Some(&path), Some(&branch))
                    .map_err(err)
            }
            (false, Some(path), Some(branch)) => {
                // Nothing happened in it yet, so the folder and its branch go.
                git::remove_worktree(&project.path, &path, &branch, true)?;
                db.set_session_worktree(&s.id, None, None).map_err(err)
            }
            _ => Ok(Some(s)),
        }?
        .ok_or_else(|| "Session not found".to_string())
    })
    .await
    .map_err(err)?
}

/// Commits what the session's worktree has and merges its branch into the project folder's.
#[tauri::command]
async fn merge_session_worktree(
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<git::MergeResult> {
    let (s, project, path, branch) = session_worktree_of(&state.db, &session_id)?;
    if state.agents.is_running(&session_id) && (s.status == "running" || s.status == "waiting") {
        return Err(
            "The agent is still working. Wait for it to finish, or stop it, then merge.".into(),
        );
    }
    let msg = format!("{} (AgentDeck)", s.title);
    let db = state.db.clone();
    let sink = state.sink.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let r = git::merge_worktree(&project, &path, &branch, &msg)?;
        if let Ok(Some(ev)) = db.append_event(&session_id, "agentdeck", "worktree_merge", &serde_json::json!({ "merged": r.merged, "base": r.base, "branch": branch, "conflicts": r.conflicts, "message": r.message })) {
            sink(Outbound::Event(ev));
        }
        Ok(r)
    })
    .await
    .map_err(err)?
}

/// Deletes the session's worktree folder (and its branch when asked); the session goes back to
/// the project folder with a fresh conversation.
#[tauri::command]
async fn remove_session_worktree(
    state: State<'_, AppState>,
    session_id: String,
    delete_branch: bool,
) -> CmdResult<SessionRow> {
    let (_, project, path, branch) = session_worktree_of(&state.db, &session_id)?;
    // Other sessions sharing the worktree (a workflow) go back to the project folder too.
    let sharing: Vec<String> = state
        .db
        .list_sessions()
        .map_err(err)?
        .into_iter()
        .filter(|x| x.worktree.as_deref() == Some(path.as_str()))
        .map(|x| x.id)
        .collect();
    for id in &sharing {
        state.agents.stop(id);
    }
    let agents = state.agents.clone();
    let wait: Vec<String> = sharing.clone();
    let b = branch.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let start = std::time::Instant::now();
        while wait.iter().any(|id| agents.is_running(id))
            && start.elapsed() < std::time::Duration::from_secs(5)
        {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        git::remove_worktree(&project, &path, &b, delete_branch)
    })
    .await
    .map_err(err)??;
    let mut me = None;
    for id in sharing {
        state
            .db
            .set_session_worktree(&id, None, None)
            .map_err(err)?;
        let row = state.db.update_session_agent_id_clear(&id).map_err(err)?;
        if let Ok(Some(ev)) = state.db.append_event(
            &id,
            "agentdeck",
            "worktree_removed",
            &serde_json::json!({ "branch": branch, "branchDeleted": delete_branch }),
        ) {
            (state.sink)(Outbound::Event(ev));
        }
        if let Some(r) = row {
            (state.sink)(Outbound::Session(r.clone()));
            if r.id == session_id {
                me = Some(r);
            }
        }
    }
    me.ok_or_else(|| "Session not found".into())
}

/* ------------------------------------------------------------ agent setup */

fn context_env(app: &tauri::AppHandle, db: &Db) -> CmdResult<(context::Homes, Vec<PathBuf>)> {
    let homes = context::Homes::from_home(&app.path().home_dir().map_err(err)?);
    let projects = db
        .list_projects()
        .map_err(err)?
        .into_iter()
        .map(|p| PathBuf::from(p.path))
        .collect();
    Ok((homes, projects))
}

/// Instruction files, skills, commands and subagents for a project (or only the global ones).
#[tauri::command]
async fn scan_context(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> CmdResult<Vec<context::Item>> {
    let (homes, _) = context_env(&app, &state.db)?;
    let root = match project_id {
        Some(id) => Some(PathBuf::from(
            state
                .db
                .project(&id)
                .map_err(err)?
                .ok_or("Project not found")?
                .path,
        )),
        None => None,
    };
    tauri::async_runtime::spawn_blocking(move || context::scan(root.as_deref(), &homes))
        .await
        .map_err(err)
}

#[tauri::command]
fn read_context_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<context::FileText> {
    let (homes, projects) = context_env(&app, &state.db)?;
    context::read(&path, &projects, &homes)
}

/// Saves an agent file; refused if it changed on disk since `expected_modified`.
#[tauri::command]
fn save_context_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    text: String,
    expected_modified: i64,
) -> CmdResult<context::FileText> {
    let (homes, projects) = context_env(&app, &state.db)?;
    context::write(&path, &text, expected_modified, &projects, &homes)
}

/// Makes a new skill, command or subagent from a short template.
#[tauri::command]
fn new_context_item(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: Option<String>,
    kind: String,
    scope: String,
    agent: String,
    name: String,
) -> CmdResult<context::FileText> {
    let (homes, projects) = context_env(&app, &state.db)?;
    let root = match &project_id {
        Some(id) => Some(PathBuf::from(
            state
                .db
                .project(id)
                .map_err(err)?
                .ok_or("Project not found")?
                .path,
        )),
        None => None,
    };
    let (path, text) = context::new_item(&kind, &scope, &agent, &name, root.as_deref(), &homes)?;
    context::write(&path.to_string_lossy(), &text, 0, &projects, &homes)
}

/* ------------------------------------------------------------ import */

/// Sessions Claude Code, Codex and Gemini CLI kept on their own, newest first.
#[tauri::command]
async fn scan_imports(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Vec<import::Found>> {
    let roots = import::Roots::from_home(&app.path().home_dir().map_err(err)?);
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        import::scan(&roots, &|p, id| {
            db.session_for_agent_id(p, id).ok().flatten()
        })
    })
    .await
    .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Imported {
    sessions: Vec<SessionRow>,
    projects: Vec<Project>,
    skipped: usize,
}

/// Brings the picked sessions in: into the project for the folder they ran in (added if new),
/// or into Chats when that folder is unknown or gone.
#[tauri::command]
async fn import_sessions(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    picks: Vec<import::Pick>,
) -> CmdResult<Imported> {
    if picks.len() > 1000 {
        return Err("Import at most 1,000 sessions at a time".into());
    }
    let home = app.path().home_dir().map_err(err)?;
    let roots = import::Roots::from_home(&home);
    let db = state.db.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Imported { sessions: Vec::new(), projects: Vec::new(), skipped: 0 };
        for pick in picks {
            let file = roots.check(&pick)?;
            let p = import::parse(&pick.provider, &file);
            if p.agent_id.is_empty() || p.messages.is_empty() {
                out.skipped += 1;
                continue;
            }
            if db.session_for_agent_id(&pick.provider, &p.agent_id).map_err(err)?.is_some() {
                out.skipped += 1;
                continue;
            }
            // The folder it ran in, unless that's gone or just the home folder.
            let folder = p
                .cwd
                .as_deref()
                .map(Path::new)
                .filter(|c| c.is_dir() && *c != home.as_path())
                .and_then(|c| c.canonicalize().ok());
            let project = match &folder {
                Some(dir) => {
                    let path = dir.to_string_lossy().into_owned();
                    match db.list_projects().map_err(err)?.into_iter().find(|x| x.path == path) {
                        // A bot's workspace isn't a project to bring sessions into.
                        Some(x) if x.hidden => ensure_chats(&app2, &db)?,
                        Some(x) => x,
                        None => {
                            let name = dir
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.clone());
                            db.add_project(&name, &path).map_err(err)?
                        }
                    }
                }
                None => ensure_chats(&app2, &db)?,
            };
            // Claude Code and Gemini find their conversation by the folder it ran in.
            let resumable = folder.is_some() || pick.provider == "codex";
            let snippet = p
                .messages
                .iter()
                .rev()
                .find(|m| !m.user)
                .or(p.messages.last())
                .map(|m| crate::claude::first_line(&m.text, 80))
                .unwrap_or_default();
            let row = db
                .create_imported_session(
                    &project.id,
                    &pick.provider,
                    &p.title(),
                    &snippet,
                    resumable.then_some(p.agent_id.as_str()),
                    p.started_at,
                    p.updated_at,
                )
                .map_err(err)?;
            let mut items: Vec<(&str, serde_json::Value, i64)> = vec![(
                "imported",
                serde_json::json!({
                    "from": provider_label(&pick.provider),
                    "messages": p.messages.len(),
                    "resumable": resumable,
                    "folder": p.cwd,
                }),
                p.started_at,
            )];
            for m in &p.messages {
                items.push((
                    "imported_message",
                    serde_json::json!({ "role": if m.user { "user" } else { "assistant" }, "text": m.text }),
                    m.ts,
                ));
            }
            db.append_events_at(&row.id, &items).map_err(err)?;
            db.mark_imported(&pick.provider, &p.agent_id, &row.id).map_err(err)?;
            if !out.projects.iter().any(|x| x.id == project.id) {
                out.projects.push(project);
            }
            out.sessions.push(row);
        }
        Ok::<_, String>(out)
    })
    .await
    .map_err(err)?
}

/// Makes ~/AgentDeck/<name> (or uses it if it's already there) and adds it as a project.
#[tauri::command]
fn create_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult<Project> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("Give the project a name (up to 80 characters)".into());
    }
    if name.starts_with('.')
        || name.eq_ignore_ascii_case("chats")
        || name.eq_ignore_ascii_case("bots")
        || name
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '\0') || c.is_control())
    {
        return Err(
            "Use a plain folder name: no slashes or colons, not starting with a dot".into(),
        );
    }
    let dir = projects_root(&app)?.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Can't make {}: {e}", dir.display()))?;
    let canon = dir.canonicalize().map_err(err)?;
    state
        .db
        .add_project(name, &canon.to_string_lossy())
        .map_err(err)
}

/// Moves a session to another project (or to Chats). Its history stays. Claude Code and Codex
/// carry on the same conversation in the new folder; the other agents start fresh there.
#[tauri::command]
fn move_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    project_id: String,
) -> CmdResult<SessionRow> {
    let s = state
        .db
        .session(&session_id)
        .map_err(err)?
        .ok_or("Session not found")?;
    if s.project_id == project_id {
        return Ok(s);
    }
    if state.db.bot_for_session(&s.id).map_err(err)?.is_some() {
        return Err("This is a bot's session; it stays where the bot works.".into());
    }
    if state.db.active_relay_for(&s.id).map_err(err)?.is_some()
        || state.db.active_flow_for(&s.id).map_err(err)?.is_some()
    {
        return Err("This session is working with other agents. Stop that first.".into());
    }
    if s.worktree.is_some() {
        return Err(
            "This session works in its own worktree. Merge or remove the worktree first.".into(),
        );
    }
    let from = state.db.project(&s.project_id).map_err(err)?;
    let to = state
        .db
        .project(&project_id)
        .map_err(err)?
        .ok_or("Project not found")?;
    if to.hidden {
        return Err("That folder is a bot's workspace".into());
    }
    state.stop_any(&s.id);
    // Claude Code only resumes a conversation from the folder it's filed under, so it's copied
    // there; Codex resumes a thread from any folder.
    let keep = match (&s.provider_session_id, s.provider.as_str()) {
        (Some(id), "claude-code") => {
            let home = app.path().home_dir().map_err(err)?;
            let projects = context::Homes::from_home(&home).claude.join("projects");
            let old_dir = from.as_ref().map(|p| p.path.clone()).unwrap_or_default();
            import::carry_claude_conversation(&projects, id, &old_dir, &to.path)
        }
        (Some(_), "codex") => true,
        _ => false,
    };
    // Remember the agent's old conversation so Import doesn't offer it again.
    if let Some(old) = &s.provider_session_id {
        state
            .db
            .mark_imported(&s.provider, old, &s.id)
            .map_err(err)?;
    }
    let row = state
        .db
        .move_session(&s.id, &to.id, keep)
        .map_err(err)?
        .ok_or("Session not found")?;
    state.record(
        &s.id,
        "session_moved",
        &serde_json::json!({ "from": from.map(|p| p.name), "to": to.name, "toPath": to.path, "kept": keep }),
    );
    (state.sink)(Outbound::Session(row.clone()));
    Ok(row)
}

/// Which agents are installed and signed in, and how to install the rest.
#[tauri::command]
async fn setup_status(state: State<'_, AppState>) -> CmdResult<setup::SetupStatus> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || setup::status(&db))
        .await
        .map_err(err)
}

/// Runs an agent's official install or sign-in command in macOS Terminal.
#[tauri::command]
fn open_setup_terminal(app: tauri::AppHandle, agent: String, action: String) -> CmdResult<String> {
    let cache = app.path().app_cache_dir().map_err(err)?;
    setup::open_in_terminal(cache, &agent, &action)
}

#[tauri::command]
fn finish_welcome(state: State<'_, AppState>) -> CmdResult<()> {
    state.db.set_meta(setup::WELCOME_KEY, "1").map_err(err)
}

/// Turns Antigravity sessions on or off (off by default; see Settings for why).
#[tauri::command]
fn set_antigravity_enabled(state: State<'_, AppState>, enabled: bool) -> CmdResult<bool> {
    state
        .db
        .set_meta(setup::AGY_ENABLED_KEY, if enabled { "1" } else { "0" })
        .map_err(err)?;
    if !enabled {
        state.agents.agy.stop_all();
    }
    Ok(enabled)
}

/// Whether a newer AgentDeck is out.
#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> CmdResult<updates::UpdateInfo> {
    updates::check(&app).await
}

/// Installs the newer AgentDeck and restarts into it. Agents that are working are stopped.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> CmdResult<()> {
    updates::install(&app).await
}

/// Gives a session a name of your own.
#[tauri::command]
fn rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> CmdResult<SessionRow> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("Give the session a name".into());
    }
    if title.chars().count() > 120 {
        return Err("Keep the name under 120 characters".into());
    }
    let row = state
        .db
        .rename_session(&session_id, &title)
        .map_err(err)?
        .ok_or("Session not found")?;
    (state.sink)(Outbound::Session(row.clone()));
    Ok(row)
}

#[tauri::command]
fn remove_project(state: State<'_, AppState>, project_id: String) -> CmdResult<()> {
    if state
        .db
        .project(&project_id)
        .map_err(err)?
        .is_some_and(|p| p.hidden)
    {
        return Err("That's a bot's workspace; it goes when the bot is deleted.".into());
    }
    for s in state.db.list_sessions().map_err(err)? {
        if s.project_id == project_id {
            state.stop_any(&s.id);
        }
    }
    state.db.remove_project(&project_id).map_err(err)?;
    // Bots working there: their task fails and they work in their workspace by default.
    let bots = state.bots.clone();
    std::thread::spawn(move || bots.project_removed(&project_id));
    Ok(())
}

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> CmdResult<Vec<SessionRow>> {
    state.db.list_sessions().map_err(err)
}

/// Checks an agent choice for a session and returns the model to store.
/// A model id as agents use them: "opus", "gpt-6.1-sol", "gemini-2.5-pro", "sonnet[1m]".
fn valid_model_id(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 128
        && !model.starts_with('-')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._:/[]@".contains(c))
}

/// Checks an effort pick for an agent. None, "" and "default" mean the agent's own default.
fn check_effort(provider: &str, effort: Option<&str>) -> Result<Option<String>, String> {
    let Some(e) = effort
        .map(str::trim)
        .filter(|e| !e.is_empty() && *e != "default")
    else {
        return Ok(None);
    };
    let ok = match provider {
        "claude-code" => claude::EFFORTS.contains(&e),
        // Codex lists its own per model (minimal, low, medium, high, xhigh, …).
        "codex" => e.len() <= 24 && e.chars().all(|c| c.is_ascii_lowercase()),
        _ => {
            return Err(format!(
                "{} has no effort setting",
                provider_label(provider)
            ))
        }
    };
    if !ok {
        return Err(format!("Unknown effort: {e}"));
    }
    Ok(Some(e.to_string()))
}

fn check_agent(
    db: &Db,
    provider: &str,
    model: &str,
    profile_id: Option<&str>,
) -> Result<String, String> {
    if let Some(pid) = profile_id {
        let p = db
            .profile(pid)
            .map_err(err)?
            .ok_or("Model profile not found")?;
        let want = if p.kind == "anthropic" {
            "claude-code"
        } else {
            "api"
        };
        if provider != want {
            return Err(format!("{} runs on {}", p.name, provider_label(want)));
        }
        return Ok("default".into());
    }
    // Model names change often and each agent lists its own (see models.rs); anything that
    // looks like a model id is passed through for the agent to accept or reject.
    let ok = match provider {
        "claude-code" | "codex" => valid_model_id(model),
        p if acp::spec(p).is_some() || p == agy::PROVIDER => valid_model_id(model),
        "api" => return Err("Pick a model profile for this session".into()),
        _ => return Err(format!("Unknown agent: {provider}")),
    };
    if !ok {
        return Err(format!("Unknown model: {model}"));
    }
    Ok(model.into())
}

#[tauri::command]
fn create_session(
    state: State<'_, AppState>,
    project_id: String,
    provider: String,
    model: String,
    profile_id: Option<String>,
    worktree: Option<bool>,
    effort: Option<String>,
) -> CmdResult<SessionRow> {
    let model = check_agent(&state.db, &provider, &model, profile_id.as_deref())?;
    // A custom endpoint's model may have no effort levels: leave it to the profile.
    let effort = match profile_id {
        Some(_) => None,
        None => check_effort(&provider, effort.as_deref())?,
    };
    let project = state
        .db
        .project(&project_id)
        .map_err(err)?
        .ok_or("Project not found")?;
    if project.hidden {
        return Err("That folder is a bot's workspace; give the bot a task instead.".into());
    }
    // A worktree first, so a failure (not a repository, no commits) leaves no empty session.
    let wt = if worktree.unwrap_or(false) {
        let name = format!(
            "{}-{}",
            agents::label(&provider).to_lowercase().replace(' ', "-"),
            &uuid::Uuid::new_v4().simple().to_string()[..4]
        );
        Some(git::add_worktree(&project.path, &name)?)
    } else {
        None
    };
    let s = state
        .db
        .create_session(&project_id, &provider, &model, "New session")
        .map_err(err)?;
    if let Some((path, branch)) = &wt {
        state
            .db
            .set_session_worktree(&s.id, Some(path), Some(branch))
            .map_err(err)?;
    }
    if effort.is_some() {
        state
            .db
            .set_session_effort(&s.id, effort.as_deref())
            .map_err(err)?;
    }
    state
        .db
        .set_session_profile(&s.id, profile_id.as_deref())
        .map_err(err)?
        .ok_or_else(|| "Session not found".into())
}

/// Changes the agent of a session that hasn't started yet (no messages).
#[tauri::command]
fn set_session_agent(
    state: State<'_, AppState>,
    session_id: String,
    provider: String,
    model: String,
    profile_id: Option<String>,
    effort: Option<String>,
) -> CmdResult<SessionRow> {
    let model = check_agent(&state.db, &provider, &model, profile_id.as_deref())?;
    let effort = match profile_id {
        Some(_) => None,
        None => check_effort(&provider, effort.as_deref())?,
    };
    if state.agents.is_running(&session_id)
        || !state.db.events(&session_id).map_err(err)?.is_empty()
    {
        return Err(
            "This session has already started. Start a new session for another agent.".into(),
        );
    }
    state
        .db
        .set_session_agent(
            &session_id,
            &provider,
            &model,
            profile_id.as_deref(),
            effort.as_deref(),
        )
        .map_err(err)?
        .ok_or_else(|| "Session not found".into())
}

#[tauri::command]
fn set_session_model(
    state: State<'_, AppState>,
    session_id: String,
    model: String,
) -> CmdResult<SessionRow> {
    let model = model.trim().to_string();
    if !valid_model_id(&model) {
        return Err(format!("\"{model}\" isn't a model id"));
    }
    let row = state
        .db
        .update_session(&session_id, None, None, None, Some(&model), None)
        .map_err(err)?
        .ok_or_else(|| "Session not found".to_string())?;
    // A running agent switches for the next message instead of having to restart.
    if state.agents.is_running(&session_id) {
        state.agents.set_model(&session_id, &model)?;
    }
    Ok(row)
}

/// Picks a session's reasoning effort (None or "default": the agent's own). A running Claude
/// Code switches at once; otherwise it's used from the next start (Codex: the next turn).
/// Runs off the main thread: Claude Code is asked to confirm the change.
#[tauri::command]
async fn set_session_effort(
    state: State<'_, AppState>,
    session_id: String,
    effort: Option<String>,
) -> CmdResult<SessionRow> {
    let s = state
        .db
        .session(&session_id)
        .map_err(err)?
        .ok_or("Session not found")?;
    if s.profile_id.is_some() {
        return Err("This session's effort comes from its model profile".into());
    }
    let effort = check_effort(&s.provider, effort.as_deref())?;
    let row = state
        .db
        .set_session_effort(&session_id, effort.as_deref())
        .map_err(err)?
        .ok_or("Session not found")?;
    let agents = state.agents.clone();
    tauri::async_runtime::spawn_blocking(move || agents.set_effort(&session_id, effort.as_deref()))
        .await
        .map_err(|e| e.to_string())??;
    Ok(row)
}

/// The effort a session's agent uses right now, as the agent reports it.
#[tauri::command]
async fn session_effort(
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<agents::EffortStatus> {
    let agents = state.agents.clone();
    tauri::async_runtime::spawn_blocking(move || agents.effort_status(&session_id))
        .await
        .map_err(|e| e.to_string())?
}

/// The latest plan usage limits each agent reported (Claude Code, Codex), saved ones included.
#[tauri::command]
fn plan_limits(state: State<'_, AppState>) -> CmdResult<Vec<limits::PlanLimits>> {
    Ok(limits::all(&state.db))
}

/// The models an agent offers, as the agent reports them. Runs off the main thread: asking
/// the agent can take a few seconds.
#[tauri::command]
async fn list_models(
    state: State<'_, AppState>,
    provider: String,
    refresh: Option<bool>,
) -> CmdResult<models::ModelList> {
    if ![
        "claude-code",
        "codex",
        "gemini-cli",
        "grok-build",
        "kimi-code",
        "antigravity",
    ]
    .contains(&provider.as_str())
    {
        return Err(format!("Unknown agent: {provider}"));
    }
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        models::list(&db, &provider, refresh.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_session_mode(
    state: State<'_, AppState>,
    session_id: String,
    mode: String,
) -> CmdResult<SessionRow> {
    if mode != "default" && mode != "plan" {
        return Err(format!("Unknown mode: {mode}"));
    }
    let row = state
        .db
        .set_session_mode(&session_id, &mode)
        .map_err(err)?
        .ok_or("Session not found")?;
    state.agents.set_mode(&session_id, &mode)?;
    state.record(
        &session_id,
        "mode_changed",
        &serde_json::json!({ "mode": mode }),
    );
    Ok(row)
}

#[tauri::command]
async fn hand_off(
    state: State<'_, AppState>,
    from_session_id: String,
    provider: String,
    profile_id: Option<String>,
    plan: String,
    plan_request_id: Option<String>,
) -> CmdResult<SessionRow> {
    let (new, prompt) = state.hand_off(
        &from_session_id,
        &provider,
        profile_id.as_deref(),
        &plan,
        plan_request_id.as_deref(),
    )?;
    // A plan Claude Code is waiting on: tell it the plan went elsewhere, so it stops here.
    if let Some(rid) = plan_request_id {
        let msg = format!(
            "The user handed this plan to {} in another AgentDeck session. Stop here and don't implement it.",
            provider_label(&provider)
        );
        let _ = state
            .agents
            .claude
            .deny_with_message(&from_session_id, &rid, &msg);
    }
    let id = new.id.clone();
    let agents = state.agents.clone();
    tauri::async_runtime::spawn_blocking(move || agents.send(&id, &prompt))
        .await
        .map_err(err)??;
    Ok(new)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn start_relay(
    state: State<'_, AppState>,
    session_id: String,
    partner_session_id: Option<String>,
    partner_provider: Option<String>,
    partner_profile_id: Option<String>,
    task: String,
    role: String,
    partner_role: String,
    max_rounds: i64,
) -> CmdResult<RelayRow> {
    let me = state
        .db
        .session(&session_id)
        .map_err(err)?
        .ok_or("Session not found")?;
    let partner = match (partner_session_id, partner_provider) {
        (Some(id), _) => id,
        (None, Some(provider)) => {
            let row = state.new_session_for(
                &me.project_id,
                &provider,
                partner_profile_id.as_deref(),
                &me.title,
            )?;
            (state.sink)(Outbound::Session(row.clone()));
            row.id
        }
        (None, None) => return Err("Choose who to talk to".into()),
    };
    let relay = state.relay.clone();
    tauri::async_runtime::spawn_blocking(move || {
        relay.start(
            &session_id,
            &partner,
            &task,
            &role,
            &partner_role,
            max_rounds,
        )
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn stop_relay(state: State<'_, AppState>, relay_id: String) -> CmdResult<RelayRow> {
    state.relay.stop(&relay_id, "You stopped the relay")
}

#[tauri::command]
fn list_relays(state: State<'_, AppState>) -> CmdResult<Vec<RelayRow>> {
    state.db.list_relays().map_err(err)
}

/* ------------------------------------------------------------ agent keys */

/// Which agents have an API key saved in AgentDeck (never the keys themselves).
#[tauri::command]
fn agent_keys() -> CmdResult<std::collections::HashMap<String, bool>> {
    Ok(acp::AGENTS
        .iter()
        .map(|a| {
            (
                a.provider.to_string(),
                secrets::get(&acp::key_name(a.provider)).is_some(),
            )
        })
        .collect())
}

/// Saves (or with an empty key, removes) an agent's API key in the keychain. The agent's
/// running process is closed so the next message starts it with the new key.
#[tauri::command]
fn set_agent_key(state: State<'_, AppState>, provider: String, key: String) -> CmdResult<bool> {
    acp::spec(&provider)
        .filter(|s| !s.key_env.is_empty())
        .ok_or("That agent doesn't take an API key here")?;
    let key = key.trim();
    if key.len() > 512 || key.chars().any(char::is_whitespace) {
        return Err("That doesn't look like an API key".into());
    }
    if key.is_empty() {
        secrets::remove(&acp::key_name(&provider));
    } else {
        secrets::set(&acp::key_name(&provider), key)?;
    }
    for s in state.db.list_sessions().map_err(err)? {
        if s.provider == provider && s.status != "running" && s.status != "waiting" {
            state.agents.stop(&s.id);
        }
    }
    Ok(!key.is_empty())
}

/* ------------------------------------------------------------ workflows */

fn check_workflow(state: &AppState, w: &Workflow) -> Result<(), String> {
    if w.name.trim().is_empty() || w.name.chars().count() > 80 {
        return Err("Give the workflow a name (up to 80 characters)".into());
    }
    if !(1..=flow::MAX_REVIEWS).contains(&w.max_reviews) {
        return Err(format!(
            "Choose between 1 and {} reviews",
            flow::MAX_REVIEWS
        ));
    }
    if w.build_agent.trim().is_empty() {
        return Err("Choose who builds".into());
    }
    for a in [&w.plan_agent, &w.build_agent, &w.review_agent] {
        if a == "this" {
            return Err("A saved workflow names its agents".into());
        }
        flow::parse_pick(&state.db, a)?;
    }
    Ok(())
}

#[tauri::command]
fn list_workflows(state: State<'_, AppState>) -> CmdResult<Vec<Workflow>> {
    state.db.list_workflows().map_err(err)
}

#[tauri::command]
fn save_workflow(state: State<'_, AppState>, workflow: Workflow) -> CmdResult<Workflow> {
    check_workflow(&state, &workflow)?;
    let mut w = workflow;
    w.name = w.name.trim().to_string();
    state.db.save_workflow(&w).map_err(err)
}

#[tauri::command]
fn delete_workflow(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_workflow(&id).map_err(err)
}

#[tauri::command]
fn list_flows(state: State<'_, AppState>) -> CmdResult<Vec<FlowRow>> {
    state.db.list_flows().map_err(err)
}

/// Runs a workflow from a session: "this" in a stage means that session; other stages get new ones.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn start_flow(
    state: State<'_, AppState>,
    session_id: String,
    name: String,
    task: String,
    plan_agent: String,
    build_agent: String,
    review_agent: String,
    gate_plan: bool,
    max_reviews: i64,
    worktree: Option<bool>,
) -> CmdResult<FlowRow> {
    let me = state
        .db
        .session(&session_id)
        .map_err(err)?
        .ok_or("Session not found")?;
    if task.trim().is_empty() {
        return Err("Write the task for the agents first".into());
    }
    let picks = [
        flow::parse_pick(&state.db, &plan_agent)?,
        flow::parse_pick(&state.db, &build_agent)?
            .ok_or("Choose who builds")?
            .into(),
        flow::parse_pick(&state.db, &review_agent)?,
    ];
    if picks
        .iter()
        .filter(|p| matches!(p, Some(flow::Pick::This)))
        .count()
        > 1
    {
        return Err("This session can take one stage only".into());
    }
    let mut rows: Vec<Option<SessionRow>> = Vec::new();
    for p in picks {
        rows.push(match p {
            None => None,
            Some(flow::Pick::This) => Some(me.clone()),
            Some(flow::Pick::New {
                provider,
                profile_id,
            }) => {
                let row = state.new_session_for(
                    &me.project_id,
                    &provider,
                    profile_id.as_deref(),
                    "New session",
                )?;
                (state.sink)(Outbound::Session(row.clone()));
                Some(row)
            }
        });
    }
    // All stages work in one worktree, so the reviewer sees what the builder changed.
    if worktree.unwrap_or(false) {
        let shared = rows
            .iter()
            .flatten()
            .find_map(|r| r.worktree.clone().zip(r.branch.clone()));
        let (path, branch) = match shared {
            Some(w) => w,
            None => {
                let project = state
                    .db
                    .project(&me.project_id)
                    .map_err(err)?
                    .ok_or("Project not found")?;
                git::add_worktree(&project.path, &crate::claude::first_line(&task, 40))?
            }
        };
        for r in rows.iter_mut().flatten() {
            if r.worktree.as_deref() == Some(path.as_str()) {
                continue;
            }
            if r.provider_session_id.is_some() || r.worktree.is_some() {
                return Err(format!("\"{}\" already has a conversation in another folder. Start the workflow from a new session to run it in a worktree.", r.title));
            }
            if let Some(row) = state
                .db
                .set_session_worktree(&r.id, Some(&path), Some(&branch))
                .map_err(err)?
            {
                (state.sink)(Outbound::Session(row.clone()));
                *r = row;
            }
        }
    }
    let flow = state.flow.clone();
    let project = me.project_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let build = rows[1].clone().expect("build stage is required");
        flow.start(
            &project,
            &name,
            &task,
            rows[0].as_ref(),
            &build,
            rows[2].as_ref(),
            gate_plan,
            max_reviews,
        )
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn stop_flow(state: State<'_, AppState>, flow_id: String) -> CmdResult<FlowRow> {
    state.flow.stop(&flow_id, "You stopped the workflow")
}

#[tauri::command]
async fn approve_flow_plan(state: State<'_, AppState>, flow_id: String) -> CmdResult<FlowRow> {
    let flow = state.flow.clone();
    tauri::async_runtime::spawn_blocking(move || flow.approve(&flow_id))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn revise_flow_plan(
    state: State<'_, AppState>,
    flow_id: String,
    notes: String,
) -> CmdResult<FlowRow> {
    let flow = state.flow.clone();
    tauri::async_runtime::spawn_blocking(move || flow.revise(&flow_id, &notes))
        .await
        .map_err(err)?
}

/* ------------------------------------------------------------ bots */

/// A bot as the editor sends it; an empty or missing id makes a new one.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BotInput {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    instructions: String,
    provider: String,
    model: String,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    profile_id: Option<String>,
    /// Its default project; None: tasks work in its own workspace unless they name one.
    #[serde(default)]
    project_id: Option<String>,
    color: String,
}

#[tauri::command]
fn list_bots(state: State<'_, AppState>) -> CmdResult<Vec<Bot>> {
    state.bots.list()
}

/// Makes or changes a bot: checks its name, agent, model, effort and folder.
#[tauri::command]
async fn save_bot(state: State<'_, AppState>, bot: BotInput) -> CmdResult<Bot> {
    if !LIVE_PROVIDERS.contains(&bot.provider.as_str()) {
        return Err(format!("Unknown agent: {}", bot.provider));
    }
    let profile_id = bot.profile_id.filter(|p| !p.is_empty());
    let model = check_agent(&state.db, &bot.provider, &bot.model, profile_id.as_deref())?;
    let effort = match profile_id {
        Some(_) => None,
        None => check_effort(&bot.provider, bot.effort.as_deref())?,
    };
    let bots = state.bots.clone();
    tauri::async_runtime::spawn_blocking(move || {
        bots.save(
            bot.id.as_deref().unwrap_or(""),
            &BotFields {
                name: &bot.name,
                role: &bot.role,
                instructions: &bot.instructions,
                provider: &bot.provider,
                model: &model,
                effort: effort.as_deref(),
                profile_id: profile_id.as_deref(),
                project_id: bot.project_id.as_deref(),
                color: &bot.color,
            },
        )
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn delete_bot(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let bots = state.bots.clone();
    let triggers = state.triggers.clone();
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        bots.delete(&id)?;
        triggers.bot_deleted(&id);
        let _ = connections::set_for_bot(&db, &id, &[]);
        Ok(())
    })
    .await
    .map_err(err)?
}

/// A bot's options: its own worktree per task, and the models a manager may pick for it.
#[tauri::command]
fn bot_options(state: State<'_, AppState>, bot_id: String) -> CmdResult<bots::BotOptions> {
    state.bots.get(&bot_id)?;
    Ok(bots::options(&state.db, &bot_id))
}

#[tauri::command]
fn set_bot_options(
    state: State<'_, AppState>,
    bot_id: String,
    options: bots::BotOptions,
) -> CmdResult<bots::BotOptions> {
    state.bots.get(&bot_id)?;
    bots::set_options(&state.db, &bot_id, &options)
}

/// What a bot's tasks used in the last day, week and 30 days.
#[derive(serde::Serialize)]
struct BotUsageView {
    day: db::BotUsage,
    week: db::BotUsage,
    month: db::BotUsage,
}

#[tauri::command]
fn bot_usage(state: State<'_, AppState>, bot_id: String) -> CmdResult<BotUsageView> {
    let now = db::now_ms();
    let since = |days: i64| {
        state
            .db
            .bot_usage(&bot_id, now - days * 86_400_000)
            .map_err(err)
    };
    Ok(BotUsageView {
        day: since(1)?,
        week: since(7)?,
        month: since(30)?,
    })
}

/// A bot's triggers (pages, folders, GitHub labels, hooks).
#[tauri::command]
fn bot_triggers(state: State<'_, AppState>, bot_id: String) -> CmdResult<Vec<triggers::Trigger>> {
    Ok(state.triggers.list(&bot_id))
}

#[tauri::command]
fn save_bot_trigger(
    state: State<'_, AppState>,
    input: triggers::Trigger,
) -> CmdResult<triggers::Trigger> {
    state.triggers.save(input)
}

#[tauri::command]
fn delete_bot_trigger(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.triggers.remove(&id)
}

/// Looks at a trigger now (a page, a folder, GitHub); it fires if something is new.
#[tauri::command]
async fn check_bot_trigger(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<triggers::Trigger>> {
    let t = state.triggers.clone();
    tauri::async_runtime::spawn_blocking(move || t.check(&id, db::now_ms()))
        .await
        .map_err(err)
}

/// The port hooks are called on (127.0.0.1), if one was free.
#[tauri::command]
fn hook_info(state: State<'_, AppState>) -> CmdResult<triggers::HookInfo> {
    Ok(state.triggers.hook_info())
}

#[tauri::command]
fn bot_tasks(
    state: State<'_, AppState>,
    bot_id: String,
    limit: Option<i64>,
) -> CmdResult<Vec<BotTask>> {
    state.bots.tasks(&bot_id, limit.unwrap_or(50))
}

/// The tasks a bot's task handed to other bots (with the agentdeck tools), oldest first.
#[tauri::command]
fn bot_task_children(state: State<'_, AppState>, task_id: String) -> CmdResult<Vec<BotTask>> {
    state.bots.children(&task_id)
}

/// Queues a task from you; it starts at once when the bot is free. `project_id`: the project
/// it works in; None: the bot's own workspace.
#[tauri::command]
async fn add_bot_task(
    state: State<'_, AppState>,
    bot_id: String,
    text: String,
    project_id: Option<String>,
) -> CmdResult<BotTask> {
    let bots = state.bots.clone();
    tauri::async_runtime::spawn_blocking(move || {
        bots.add_task(&bot_id, &text, "you", project_id.as_deref())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn cancel_bot_task(state: State<'_, AppState>, id: String) -> CmdResult<BotTask> {
    let bots = state.bots.clone();
    tauri::async_runtime::spawn_blocking(move || bots.cancel(&id))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn retry_bot_task(state: State<'_, AppState>, id: String) -> CmdResult<BotTask> {
    let bots = state.bots.clone();
    tauri::async_runtime::spawn_blocking(move || bots.retry(&id))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn set_bot_paused(state: State<'_, AppState>, id: String, paused: bool) -> CmdResult<Bot> {
    let bots = state.bots.clone();
    tauri::async_runtime::spawn_blocking(move || bots.set_paused(&id, paused))
        .await
        .map_err(err)?
}

/// The bot's next task starts a new session.
#[tauri::command]
fn restart_bot_session(state: State<'_, AppState>, id: String) -> CmdResult<Bot> {
    state.bots.restart_session(&id)
}

#[tauri::command]
fn read_bot_memory(state: State<'_, AppState>, id: String) -> CmdResult<bots::Memory> {
    state.bots.read_memory(&id)
}

#[tauri::command]
fn write_bot_memory(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> CmdResult<bots::Memory> {
    state.bots.write_memory(&id, &text)
}

#[tauri::command]
fn bot_schedules(state: State<'_, AppState>, bot_id: String) -> CmdResult<Vec<BotSchedule>> {
    state.bots.schedules(&bot_id)
}

/// Makes or changes a schedule (what to do, where, when); its next run is counted from now.
#[tauri::command]
fn save_bot_schedule(
    state: State<'_, AppState>,
    schedule: bots::ScheduleInput,
) -> CmdResult<BotSchedule> {
    state.bots.save_schedule(&schedule)
}

#[tauri::command]
fn delete_bot_schedule(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.bots.delete_schedule(&id)
}

#[tauri::command]
fn set_bot_schedule_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> CmdResult<BotSchedule> {
    state.bots.set_schedule_enabled(&id, enabled)
}

/// Adds a schedule's task now ("Run now").
#[tauri::command]
async fn run_bot_schedule(state: State<'_, AppState>, id: String) -> CmdResult<BotTask> {
    let bots = state.bots.clone();
    tauri::async_runtime::spawn_blocking(move || bots.run_schedule(&id))
        .await
        .map_err(err)?
}

/// Opens a shell in a project folder AgentDeck already knows; nothing else is reachable.
#[tauri::command]
fn term_open(
    state: State<'_, AppState>,
    project_id: String,
    session_id: Option<String>,
    cols: u16,
    rows: u16,
) -> CmdResult<pty::TermInfo> {
    let project = state
        .db
        .project(&project_id)
        .map_err(err)?
        .ok_or("Project not found")?;
    // A session in a worktree gets its terminal there.
    let dir = match session_id {
        Some(sid) => {
            let s = state
                .db
                .session(&sid)
                .map_err(err)?
                .ok_or("Session not found")?;
            if s.project_id != project_id {
                return Err("That session belongs to another project".into());
            }
            state
                .db
                .session_dir(&s)
                .map_err(err)?
                .unwrap_or(project.path)
        }
        None => project.path,
    };
    if !Path::new(&dir).is_dir() {
        return Err(format!("The folder {dir} doesn't exist anymore"));
    }
    state.terms.open(&dir, cols, rows)
}

/// Hands a Claude Code conversation to Remote Control: it continues in a terminal running
/// `claude --resume … --remote-control`, which the Claude app and claude.ai connect to.
#[tauri::command]
async fn remote_control_start(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> CmdResult<pty::TermInfo> {
    let remote = state.remote.clone();
    let terms = state.terms.clone();
    tauri::async_runtime::spawn_blocking(move || remote.start(&terms, &session_id, cols, rows))
        .await
        .map_err(err)?
}

/// Ends Remote Control: Claude Code in the terminal stops and the conversation is AgentDeck's again.
#[tauri::command]
fn remote_control_stop(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    state.remote.stop(&state.terms, &session_id);
    Ok(())
}

/// Settings → Connections: every connection (secrets only as "saved").
#[tauri::command]
fn connections_list(state: State<'_, AppState>) -> CmdResult<Vec<connections::Connection>> {
    Ok(connections::list(&state.db))
}

#[tauri::command]
fn connection_save(
    state: State<'_, AppState>,
    input: connections::Connection,
) -> CmdResult<connections::Connection> {
    connections::save(&state.db, input)
}

#[tauri::command]
fn connection_remove(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    connections::remove(&state.db, &id)
}

/// The connections a bot gets (ids). A change reaches its agent when its process next starts.
#[tauri::command]
fn bot_connections(state: State<'_, AppState>, bot_id: String) -> CmdResult<Vec<String>> {
    Ok(connections::for_bot(&state.db, &bot_id))
}

#[tauri::command]
fn set_bot_connections(
    state: State<'_, AppState>,
    bot_id: String,
    ids: Vec<String>,
) -> CmdResult<Vec<String>> {
    let bot = state.bots.get(&bot_id)?;
    let before = connections::for_bot(&state.db, &bot_id);
    let after = connections::set_for_bot(&state.db, &bot_id, &ids)?;
    // Its agent takes connections when it starts: an idle bot's agents stop, so its next task
    // starts them with the new ones (its conversations carry on).
    if before != after && bot.running_task_id.is_none() {
        for p in &bot.places {
            state.stop_any(&p.session_id);
        }
    }
    Ok(after)
}

/// Telegram: whether it's connected, as whom, and the paired chats.
#[tauri::command]
fn telegram_status(state: State<'_, AppState>) -> CmdResult<telegram::Status> {
    Ok(state.telegram.status())
}

/// Saves your Telegram bot's token (in the keychain) once Telegram accepts it.
#[tauri::command]
async fn telegram_connect(
    state: State<'_, AppState>,
    token: String,
) -> CmdResult<telegram::Status> {
    let tg = state.telegram.clone();
    tauri::async_runtime::spawn_blocking(move || tg.connect(&token))
        .await
        .map_err(err)?
}

#[tauri::command]
fn telegram_disconnect(state: State<'_, AppState>) -> CmdResult<telegram::Status> {
    Ok(state.telegram.disconnect())
}

/// A code to send to your bot as `/pair <code>`, valid for 10 minutes.
#[tauri::command]
fn telegram_pair(state: State<'_, AppState>) -> CmdResult<telegram::Status> {
    state.telegram.start_pairing()
}

#[tauri::command]
fn telegram_unpair(state: State<'_, AppState>, chat_id: i64) -> CmdResult<telegram::Status> {
    Ok(state.telegram.unpair(chat_id))
}

/// Which bot tasks report back to Telegram: "telegram", "schedules" or "all".
#[tauri::command]
fn telegram_set_notify(state: State<'_, AppState>, notify: String) -> CmdResult<telegram::Status> {
    state.telegram.set_notify(&notify)
}

/// The terminal a session is in Remote Control in, if it is.
#[tauri::command]
fn remote_control_term(
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<Option<String>> {
    Ok(state.remote.term_for(&session_id))
}

#[tauri::command]
fn term_info(state: State<'_, AppState>, term_id: String) -> CmdResult<pty::TermInfo> {
    state.terms.info(&term_id)
}

#[tauri::command]
fn term_write(state: State<'_, AppState>, term_id: String, data: String) -> CmdResult<()> {
    state.terms.write(&term_id, &data)
}

#[tauri::command]
fn term_resize(state: State<'_, AppState>, term_id: String, cols: u16, rows: u16) -> CmdResult<()> {
    state.terms.resize(&term_id, cols, rows)
}

/// Opens a web link in the default browser (sign-in pages printed in the terminal). Only http(s).
#[tauri::command]
fn open_link(url: String) -> CmdResult<()> {
    let ok = (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() < 4096
        && !url.chars().any(|c| c.is_control() || c.is_whitespace());
    if !ok {
        return Err("Only web links can be opened".into());
    }
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(&url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't open the link: {e}"))
}

/// Runs an agent's own sign-in in one of AgentDeck's terminals.
#[tauri::command]
fn term_sign_in(state: State<'_, AppState>, term_id: String, agent: String) -> CmdResult<String> {
    let cmd = setup::sign_in_command(&agent).ok_or_else(|| format!("Unknown agent: {agent}"))?;
    state.terms.write(&term_id, &format!("{cmd}\r"))?;
    Ok(cmd.to_string())
}

/// How much each agent may do without asking in your own sessions, by provider.
#[tauri::command]
fn agent_approvals(
    state: State<'_, AppState>,
) -> CmdResult<serde_json::Map<String, serde_json::Value>> {
    Ok(approvals::levels(&state.db))
}

/// Sets an agent's level; requests already waiting that it lets go ahead are answered.
/// Returns how many were.
#[tauri::command]
async fn set_agent_approvals(
    state: State<'_, AppState>,
    provider: String,
    level: String,
) -> CmdResult<usize> {
    approvals::set_level(&state.db, &provider, &level)?;
    let ap = state.approvals.clone();
    tauri::async_runtime::spawn_blocking(move || ap.apply_waiting(&provider))
        .await
        .map_err(err)
}

/// How much Antigravity may do without asking: "edits", "auto" or "all".
#[tauri::command]
fn antigravity_approvals(state: State<'_, AppState>) -> CmdResult<String> {
    Ok(agy::approvals(&state.db))
}

#[tauri::command]
async fn set_antigravity_approvals(
    state: State<'_, AppState>,
    level: String,
    session_id: Option<String>,
) -> CmdResult<String> {
    if !["edits", "auto", "all"].contains(&level.as_str()) {
        return Err(format!("Unknown approval level: {level}"));
    }
    state.db.set_meta(agy::APPROVALS_KEY, &level).map_err(err)?;
    let agy = state.agents.agy.clone();
    let l = level.clone();
    tauri::async_runtime::spawn_blocking(move || agy.apply_approvals(session_id.as_deref(), &l))
        .await
        .map_err(err)??;
    Ok(level)
}

/// Adds allow rules to Antigravity's own settings after you approve them.
#[tauri::command]
fn allow_antigravity_rules(app: tauri::AppHandle, rules: Vec<String>) -> CmdResult<String> {
    let home = app.path().home_dir().map_err(err)?;
    agy::add_allow_rules(&home, &rules).map(|p| p.to_string_lossy().into_owned())
}

/// After signing in: a fresh agent process (so it reads the new sign-in) gets the last message again.
#[tauri::command]
async fn retry_last_message(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    let agents = state.agents.clone();
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let session = db
            .session(&session_id)
            .map_err(err)?
            .ok_or("Session not found")?;
        let dir = db
            .session_dir(&session)
            .map_err(err)?
            .ok_or("Project not found")?;
        let last = db
            .events(&session_id)
            .map_err(err)?
            .into_iter()
            .rev()
            .find(|e| e.kind == "user_message")
            .ok_or("There's no message to send again")?;
        let text = last.payload["text"].as_str().unwrap_or("").to_string();
        let files: Vec<String> = last.payload["attachments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str())
            .map(|a| {
                let p = Path::new(a);
                if p.is_absolute() {
                    a.to_string()
                } else {
                    Path::new(&dir).join(p).to_string_lossy().into_owned()
                }
            })
            .filter(|p| Path::new(p).exists())
            .collect();
        agents.stop(&session_id);
        let start = std::time::Instant::now();
        while agents.is_running(&session_id) && start.elapsed() < std::time::Duration::from_secs(5)
        {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        agents.send_with(&session_id, &text, &files)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn term_close(state: State<'_, AppState>, term_id: String) -> CmdResult<()> {
    state.terms.close(&term_id);
    Ok(())
}

#[tauri::command]
fn list_profiles(state: State<'_, AppState>) -> CmdResult<Vec<serde_json::Value>> {
    let list = state.db.list_profiles().map_err(err)?;
    Ok(list
        .into_iter()
        .map(|p| {
            let has_key = secrets::get(&p.id).is_some_and(|k| !k.is_empty());
            let mut v = serde_json::to_value(&p).unwrap_or_default();
            v["hasKey"] = serde_json::json!(has_key);
            v
        })
        .collect())
}

/// Saves a profile. `key`: None keeps the stored key, Some("") removes it, Some(k) replaces it.
#[tauri::command]
fn save_profile(
    state: State<'_, AppState>,
    profile: db::Profile,
    key: Option<String>,
) -> CmdResult<db::Profile> {
    let name = profile.name.trim();
    if name.is_empty() || name.len() > 60 {
        return Err("Give the profile a short name".into());
    }
    if profile.kind != "anthropic" && profile.kind != "openai" {
        return Err("Unknown API type".into());
    }
    let url = profile.base_url.trim();
    if !(url.starts_with("https://")
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1")
        || url.starts_with("http://[::1]"))
    {
        return Err(
            "Use an https:// address (plain http only for this computer: localhost or 127.0.0.1)"
                .into(),
        );
    }
    if profile.model.trim().is_empty() {
        return Err("Enter the model name".into());
    }
    let saved = state
        .db
        .save_profile(&db::Profile {
            name: name.into(),
            base_url: url.trim_end_matches('/').into(),
            model: profile.model.trim().into(),
            small_model: profile.small_model.trim().into(),
            ..profile
        })
        .map_err(err)?;
    match key.as_deref() {
        Some("") => secrets::remove(&saved.id),
        Some(k) => secrets::set(&saved.id, k.trim())
            .map_err(|e| format!("Saved the profile, but not the key. {e}"))?,
        None => {}
    }
    Ok(saved)
}

#[tauri::command]
fn delete_profile(state: State<'_, AppState>, profile_id: String) -> CmdResult<()> {
    secrets::remove(&profile_id);
    state.db.delete_profile(&profile_id).map_err(err)
}

#[tauri::command]
async fn test_profile(state: State<'_, AppState>, profile_id: String) -> CmdResult<String> {
    let p = state
        .db
        .profile(&profile_id)
        .map_err(err)?
        .ok_or("Model profile not found")?;
    tauri::async_runtime::spawn_blocking(move || {
        api::test_profile(&p, secrets::get(&p.id).as_deref())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn load_events(state: State<'_, AppState>, session_id: String) -> CmdResult<Vec<StoredEvent>> {
    state.db.events(&session_id).map_err(err)
}

#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    files: Option<Vec<String>>,
) -> CmdResult<()> {
    let agents = state.agents.clone();
    let files = files.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || agents.send_with(&session_id, &text, &files))
        .await
        .map_err(err)?
}

/// The clipboard's image saved as a file, when the web view didn't hand it over on paste.
#[tauri::command]
async fn paste_clipboard_image(app: tauri::AppHandle) -> CmdResult<Option<String>> {
    let dir = app.path().app_data_dir().map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || attach::clipboard_image(&dir))
        .await
        .map_err(err)?
}

/// Keeps a pasted image in AgentDeck's data folder so it can be attached like a dropped file.
#[tauri::command]
fn stash_pasted_image(app: tauri::AppHandle, name: String, data: String) -> CmdResult<String> {
    let dir = app.path().app_data_dir().map_err(err)?;
    attach::stash(&dir, &name, &data)
}

#[tauri::command]
fn respond_permission(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    decision: String,
) -> CmdResult<()> {
    let d = Decision::parse(&decision)?;
    state.agents.respond(&session_id, &request_id, d)
}

/// The slash commands the session's agent offers in its folder.
#[tauri::command]
async fn list_commands(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<Vec<commands::SlashCommand>> {
    let home = app.path().home_dir().map_err(err)?;
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let s = db
            .session(&session_id)
            .map_err(err)?
            .ok_or("Session not found")?;
        Ok(commands::list(&db, &s, &context::Homes::from_home(&home)))
    })
    .await
    .map_err(err)?
}

/// An image an agent made or looked at (a screenshot it saved, a picture it opened), as a data
/// URI for the conversation and the preview. Only image files, up to 15 MB.
#[tauri::command]
async fn read_image(path: String) -> CmdResult<String> {
    use base64::Engine as _;
    tauri::async_runtime::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        if !p.is_absolute() {
            return Err("Not an absolute path".to_string());
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => return Err("Only PNG, JPEG, GIF and WebP images can be shown".to_string()),
        };
        let meta = std::fs::metadata(&p).map_err(|_| format!("{path} isn't there anymore"))?;
        if !meta.is_file() || meta.len() > 15 * 1024 * 1024 {
            return Err("That image is too large to show".to_string());
        }
        let bytes = std::fs::read(&p).map_err(err)?;
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(err)?
}

/// Answers an agent's question; no answers skips it.
#[tauri::command]
fn answer_question(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    answers: Option<std::collections::HashMap<String, Vec<String>>>,
) -> CmdResult<()> {
    if let Some(a) = &answers {
        if a.len() > 20 || a.values().flatten().any(|x| x.len() > 4000) {
            return Err("That answer is too long".into());
        }
    }
    state
        .agents
        .answer(&session_id, &request_id, answers.as_ref())
}

#[tauri::command]
fn interrupt_session(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    state.agents.interrupt(&session_id)
}

#[tauri::command]
fn stop_session(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    state.stop_any(&session_id);
    Ok(())
}

#[tauri::command]
fn delete_session(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    state.stop_any(&session_id);
    state.db.delete_session(&session_id).map_err(err)?;
    // A bot's session: its task in progress fails and its next task starts a new session.
    let bots = state.bots.clone();
    std::thread::spawn(move || bots.session_deleted(&session_id));
    Ok(())
}

#[tauri::command]
fn session_runtime(state: State<'_, AppState>, session_id: String) -> CmdResult<serde_json::Value> {
    Ok(serde_json::json!({
        "running": state.agents.is_running(&session_id),
        "pendingPermissions": state.agents.claude.pending_count(&session_id) + state.agents.codex.pending_count(&session_id)
    }))
}

/// `agentdeck --mcp-bridge`: the `agentdeck` MCP server a bot's agent starts (see `mcp`).
/// Runs before anything of the app does, and returns the exit code.
pub fn mcp_bridge() -> i32 {
    mcp::bridge_main()
}

pub use mcp::BRIDGE_FLAG;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    shellenv::start();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = Arc::new(Db::open(&dir.join("agentdeck.db"))?);
            db.recover_after_restart()?;
            let handle = app.handle().clone();
            // Relays, workflows and bots listen to every stored event; set once the managers exist.
            type Listeners = (Arc<Relay>, Arc<Flow>, Arc<Bots>, Arc<approvals::Approvals>);
            let relay_cell: Arc<OnceLock<Listeners>> = Arc::new(OnceLock::new());
            let cell = relay_cell.clone();
            // Telegram reports bots' results and requests (set once it exists).
            let tg_cell: Arc<OnceLock<Arc<telegram::Telegram>>> = Arc::new(OnceLock::new());
            let tgc = tg_cell.clone();
            let sink: claude::Sink = Arc::new(move |o| {
                let _ = match o {
                    Outbound::Event(e) => {
                        if let Some(tg) = tgc.get() {
                            if matches!(
                                e.kind.as_str(),
                                "bot_needs_you" | "permission_decision" | "question_answered"
                            ) {
                                let (tg, e) = (tg.clone(), e.clone());
                                std::thread::spawn(move || tg.on_event(&e));
                            }
                        }
                        if let Some((r, f, b, ap)) = cell.get() {
                            if relay::turn_end(&e).is_some() {
                                let (r, f, b, e) = (r.clone(), f.clone(), b.clone(), e.clone());
                                // Off the agent's reader thread: forwarding may start a process.
                                std::thread::spawn(move || {
                                    r.on_event(&e);
                                    f.on_event(&e);
                                    b.on_event(&e);
                                });
                            } else if e.kind == "control_request/can_use_tool"
                                || e.kind == "approval"
                            {
                                let (f, b, ap, e) = (f.clone(), b.clone(), ap.clone(), e.clone());
                                std::thread::spawn(move || {
                                    f.on_approval(&e);
                                    b.on_request(&e);
                                    ap.on_request(&e);
                                });
                            } else if bots::wants(&e) {
                                let (b, e) = (b.clone(), e.clone());
                                std::thread::spawn(move || b.on_request(&e));
                            }
                        }
                        handle.emit("agentdeck://event", e)
                    }
                    Outbound::Session(s) => handle.emit("agentdeck://session", s),
                    Outbound::Delta(d) => handle.emit("agentdeck://delta", d),
                    Outbound::Relay(r) => handle.emit("agentdeck://relay", r),
                    Outbound::Flow(f) => handle.emit("agentdeck://flow", f),
                    Outbound::Bot(b) => handle.emit("agentdeck://bot", b),
                    Outbound::BotTask(t) => {
                        if let Some(tg) = tgc.get() {
                            if matches!(t.status.as_str(), "done" | "failed" | "cancelled") {
                                let (tg, t) = (tg.clone(), t.clone());
                                std::thread::spawn(move || tg.on_task(&t));
                            }
                        }
                        handle.emit("agentdeck://bot-task", t)
                    }
                    Outbound::BotSchedule(s) => handle.emit("agentdeck://bot-schedule", s),
                    Outbound::Limits(l) => handle.emit("agentdeck://limits", l),
                    Outbound::Telegram(s) => handle.emit("agentdeck://telegram", s),
                };
            });
            // The agentdeck MCP server bots hand work to each other with: a socket in the data
            // folder; bot sessions' agents start this app with --mcp-bridge to reach it.
            let hub = Arc::new(mcp::Hub::new(
                db.clone(),
                dir.join("mcp"),
                mcp::bridge_command(),
            ));
            let hub_up = match hub.listen() {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("agentdeck: bots can't hand work to each other: {e}");
                    false
                }
            };
            let launcher = hub_up.then(|| hub.launcher());
            let mut claude_m = Manager::new(db.clone(), sink.clone());
            claude_m.mcp = launcher.clone();
            let mut codex_m = CodexManager::new(db.clone(), sink.clone());
            codex_m.mcp = launcher.clone();
            let mut acp_m = AcpManager::new(db.clone(), sink.clone());
            acp_m.mcp = launcher;
            let agents = Arc::new(Agents {
                db: db.clone(),
                claude: Arc::new(claude_m),
                codex: Arc::new(codex_m),
                acp: Arc::new(acp_m),
                agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
                api: Arc::new(api::ApiManager::new(db.clone(), sink.clone())),
            });
            let relay = Arc::new(Relay::new(db.clone(), sink.clone(), agents.clone()));
            // Agent processes with nothing to do are closed after a while to keep memory low.
            {
                let agents = agents.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    agents.close_idle(db::now_ms());
                });
            }
            let flow = Arc::new(Flow::new(db.clone(), sink.clone(), agents.clone()));
            let home = app.path().home_dir()?;
            let remote = Arc::new(remote::Remote::new(
                db.clone(),
                agents.claude.clone(),
                sink.clone(),
                context::Homes::from_home(&home).claude.join("projects"),
            ));
            let bots = Arc::new(Bots::new(db.clone(), sink.clone(), agents.clone(), home));
            hub.set_bots(&bots);
            bots.delegation
                .store(hub_up, std::sync::atomic::Ordering::SeqCst);
            let approvals = Arc::new(approvals::Approvals::new(
                db.clone(),
                agents.clone(),
                sink.clone(),
            ));
            let _ = relay_cell.set((relay.clone(), flow.clone(), bots.clone(), approvals.clone()));
            let telegram = Arc::new(telegram::Telegram::new(
                db.clone(),
                bots.clone(),
                agents.clone(),
                sink.clone(),
            ));
            let _ = tg_cell.set(telegram.clone());
            telegram.start_saved();
            // Triggers: hooks on 127.0.0.1, pages, folders and GitHub looked at on their own thread.
            let triggers = Arc::new(triggers::Triggers::new(db.clone(), bots.clone()));
            if triggers.listen().is_none() {
                eprintln!("agentdeck: no port was free for hooks");
            }
            {
                let triggers = triggers.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    triggers.tick(db::now_ms());
                });
            }
            // A bot's notify_me: to Telegram and as a notification on the Mac.
            {
                let tg = telegram.clone();
                let app_handle = app.handle().clone();
                hub.set_notifier(Arc::new(move |bot: &str, text: &str| {
                    tg.tell_all(&format!("💬 {bot}: {text}"));
                    use tauri_plugin_notification::NotificationExt;
                    let _ = app_handle
                        .notification()
                        .builder()
                        .title(bot)
                        .body(text)
                        .show();
                }));
            }
            // Every bot gets its workspace (bots from before get theirs and their memory moved
            // in), then tasks that were queued (or running when AgentDeck closed) carry on, and
            // schedules are looked at every 30 seconds.
            {
                let bots = bots.clone();
                std::thread::spawn(move || {
                    bots.ensure_workspaces();
                    bots.resume_all();
                    loop {
                        bots.tick(db::now_ms());
                        std::thread::sleep(std::time::Duration::from_secs(30));
                    }
                });
            }
            let th = app.handle().clone();
            let rc = remote.clone();
            let terms = Arc::new(pty::Terminals::new(Arc::new(move |e| {
                let _ = match e {
                    pty::TermEvent::Output(o) => th.emit("agentdeck://term-output", o),
                    pty::TermEvent::Exit(x) => {
                        // A conversation that was in Remote Control comes back.
                        rc.on_exit(&x.term_id);
                        th.emit("agentdeck://term-exit", x)
                    }
                };
            })));
            app.manage(AppState {
                db,
                agents,
                sink,
                relay,
                flow,
                bots,
                terms,
                remote,
                telegram,
                triggers,
                approvals,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            detect_providers,
            list_projects,
            add_project,
            open_chats,
            scan_imports,
            import_sessions,
            create_project,
            move_session,
            rename_session,
            check_update,
            answer_question,
            read_image,
            list_commands,
            setup_status,
            open_setup_terminal,
            finish_welcome,
            set_antigravity_enabled,
            install_update,
            remove_project,
            list_sessions,
            create_session,
            set_session_agent,
            set_session_model,
            set_session_effort,
            session_effort,
            list_models,
            plan_limits,
            git_overview,
            git_log,
            git_commit_files,
            git_file_pair,
            session_worktree,
            set_session_worktree,
            merge_session_worktree,
            remove_session_worktree,
            term_sign_in,
            allow_antigravity_rules,
            agent_approvals,
            set_agent_approvals,
            antigravity_approvals,
            set_antigravity_approvals,
            open_link,
            retry_last_message,
            scan_context,
            read_context_file,
            save_context_file,
            new_context_item,
            set_session_mode,
            hand_off,
            start_relay,
            stop_relay,
            list_relays,
            agent_keys,
            set_agent_key,
            list_workflows,
            save_workflow,
            delete_workflow,
            list_flows,
            start_flow,
            stop_flow,
            approve_flow_plan,
            revise_flow_plan,
            list_bots,
            save_bot,
            delete_bot,
            bot_tasks,
            bot_task_children,
            add_bot_task,
            cancel_bot_task,
            retry_bot_task,
            set_bot_paused,
            restart_bot_session,
            read_bot_memory,
            write_bot_memory,
            bot_schedules,
            save_bot_schedule,
            delete_bot_schedule,
            set_bot_schedule_enabled,
            run_bot_schedule,
            list_profiles,
            save_profile,
            delete_profile,
            test_profile,
            term_open,
            remote_control_start,
            remote_control_stop,
            remote_control_term,
            telegram_status,
            bot_triggers,
            bot_options,
            set_bot_options,
            bot_usage,
            save_bot_trigger,
            delete_bot_trigger,
            check_bot_trigger,
            hook_info,
            connections_list,
            connection_save,
            connection_remove,
            bot_connections,
            set_bot_connections,
            telegram_connect,
            telegram_disconnect,
            telegram_pair,
            telegram_unpair,
            telegram_set_notify,
            term_info,
            term_write,
            term_resize,
            term_close,
            load_events,
            send_message,
            stash_pasted_image,
            paste_clipboard_image,
            respond_permission,
            interrupt_session,
            stop_session,
            delete_session,
            session_runtime
        ])
        .build(tauri::generate_context!())
        .expect("failed to start AgentDeck")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.agents.shutdown_all();
                    state.terms.close_all();
                }
            }
        });
}

#[cfg(test)]
mod handoff_tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn hand_off_links_both_sessions() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let s2 = seen.clone();
        let sink: claude::Sink = Arc::new(move |o| {
            if let Outbound::Event(e) = o {
                s2.lock().unwrap().push(e.kind);
            }
        });
        let agents = Arc::new(Agents {
            db: db.clone(),
            claude: Arc::new(Manager::new(db.clone(), sink.clone())),
            codex: Arc::new(CodexManager::new(db.clone(), sink.clone())),
            acp: Arc::new(AcpManager::new(db.clone(), sink.clone())),
            agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
            api: Arc::new(api::ApiManager::new(db.clone(), sink.clone())),
        });
        let remote = Arc::new(remote::Remote::new(
            db.clone(),
            agents.claude.clone(),
            sink.clone(),
            std::env::temp_dir(),
        ));
        let bots = Arc::new(Bots::new(
            db.clone(),
            sink.clone(),
            agents.clone(),
            std::env::temp_dir().join("agentdeck-handoff-test-home"),
        ));
        let telegram = Arc::new(telegram::Telegram::new(
            db.clone(),
            bots.clone(),
            agents.clone(),
            sink.clone(),
        ));
        let triggers = Arc::new(triggers::Triggers::new(db.clone(), bots.clone()));
        let approvals = Arc::new(approvals::Approvals::new(
            db.clone(),
            agents.clone(),
            sink.clone(),
        ));
        let state = AppState {
            db: db.clone(),
            relay: Arc::new(Relay::new(db.clone(), sink.clone(), agents.clone())),
            flow: Arc::new(Flow::new(db.clone(), sink.clone(), agents.clone())),
            bots,
            agents,
            sink,
            terms: Arc::new(pty::Terminals::new(Arc::new(|_| {}))),
            remote,
            telegram,
            triggers,
            approvals,
        };
        let p = db.add_project("demo", "/tmp/demo").unwrap();
        let from = db
            .create_session(&p.id, "claude-code", "default", "Add a discount rule")
            .unwrap();

        assert!(state
            .hand_off(&from.id, "cursor", None, "1. do it", None)
            .is_err());
        assert!(state
            .hand_off(&from.id, "codex", None, "   ", None)
            .is_err());
        assert!(
            state
                .hand_off(&from.id, "api", None, "1. do it", None)
                .is_err(),
            "the API agent needs a profile"
        );
        assert!(state
            .hand_off(
                &from.id,
                "codex",
                None,
                &"x".repeat(MAX_HANDOFF_BYTES + 1),
                None
            )
            .is_err());

        let (new, prompt) = state
            .hand_off(
                &from.id,
                "codex",
                None,
                "1. Fix the rounding\n2. Add a test",
                Some("r1"),
            )
            .unwrap();
        assert_eq!(new.provider, "codex");
        assert_eq!(new.project_id, p.id);
        assert_eq!(new.title, "Add a discount rule (Codex)");
        assert!(prompt.starts_with("Implement this plan. Claude Code wrote it"));
        assert!(prompt.ends_with("1. Fix the rounding\n2. Add a test"));
        let into = db.events(&new.id).unwrap();
        assert_eq!(into[0].kind, "handoff_in");
        assert_eq!(into[0].payload["fromSessionId"], from.id);
        let out = db.events(&from.id).unwrap();
        assert_eq!(out[0].kind, "handoff_out");
        assert_eq!(out[0].payload["toSessionId"], new.id);
        assert_eq!(out[0].payload["planRequestId"], "r1");
        assert_eq!(*seen.lock().unwrap(), vec!["handoff_in", "handoff_out"]);
    }
}

#[cfg(test)]
mod effort_tests {
    use super::check_effort;

    #[test]
    fn efforts_are_checked_per_agent() {
        assert_eq!(check_effort("claude-code", None), Ok(None));
        assert_eq!(check_effort("claude-code", Some("default")), Ok(None));
        assert_eq!(
            check_effort("claude-code", Some(" max ")),
            Ok(Some("max".into()))
        );
        assert!(
            check_effort("claude-code", Some("minimal")).is_err(),
            "not a Claude Code level"
        );
        assert!(check_effort("claude-code", Some("--model")).is_err());
        assert_eq!(
            check_effort("codex", Some("minimal")),
            Ok(Some("minimal".into()))
        );
        assert!(check_effort("codex", Some("high; rm")).is_err());
        assert!(
            check_effort("gemini-cli", Some("high")).is_err(),
            "no effort setting"
        );
        assert_eq!(check_effort("gemini-cli", None), Ok(None));
    }
}
