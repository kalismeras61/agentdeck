//! Local session store (SQLite).
//!
//! The provider's own stream is the source of truth: every line Claude Code prints (except
//! high-volume partial deltas and control acknowledgements) is stored verbatim, after secret
//! redaction, in `events`, together with AgentDeck's own events (prompts, permission decisions,
//! process exits). The UI derives its normalized view from this ordered log, so history renders
//! the same after a restart as it did live.

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

pub const SCHEMA_VERSION: i64 = 7;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    /// "folder" for a project folder, "chats" for the place sessions go when they have no project.
    pub kind: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub project_id: String,
    pub provider: String,
    pub model: String,
    pub title: String,
    pub status: String,
    pub snippet: String,
    pub provider_session_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// "default" or "plan" (plan only: the agent may read but not change anything).
    pub mode: String,
    /// Custom model profile this session runs on, if any.
    pub profile_id: Option<String>,
    /// A git worktree of the project this session works in, instead of the project folder.
    pub worktree: Option<String>,
    /// The worktree's branch.
    pub branch: Option<String>,
    /// Reasoning effort picked for this session ("high", …). None: the agent's own default.
    pub effort: Option<String>,
}

/// A custom model endpoint. The API key is not here: it lives in the OS keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// "anthropic": runs through Claude Code; "openai": runs on AgentDeck's own agent.
    pub kind: String,
    pub base_url: String,
    pub model: String,
    /// Smaller model for background work (Claude Code's fast model), optional.
    pub small_model: String,
    pub created_at: i64,
}

/// Two sessions passing each other's replies back and forth.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayRow {
    pub id: String,
    pub a_session: String,
    pub b_session: String,
    pub a_role: String,
    pub b_role: String,
    pub max_rounds: i64,
    /// Replies forwarded so far.
    pub rounds: i64,
    /// "active", "done" (an agent said DONE), or "stopped".
    pub status: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

const RELAY_COLS: &str = "id, a_session, b_session, a_role, b_role, max_rounds, rounds, status, reason, created_at, updated_at";

const PROJECT_COLS: &str = "id, name, path, kind, created_at";

fn row_to_project(r: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        kind: r.get(3)?,
        created_at: r.get(4)?,
    })
}

fn row_to_relay(r: &rusqlite::Row<'_>) -> rusqlite::Result<RelayRow> {
    Ok(RelayRow {
        id: r.get(0)?,
        a_session: r.get(1)?,
        b_session: r.get(2)?,
        a_role: r.get(3)?,
        b_role: r.get(4)?,
        max_rounds: r.get(5)?,
        rounds: r.get(6)?,
        status: r.get(7)?,
        reason: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

/// A saved workflow: which agent plans, builds and reviews. An agent is a provider id
/// ("claude-code", "codex", …) or "profile:<id>"; an empty plan or review agent skips that stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub plan_agent: String,
    pub build_agent: String,
    pub review_agent: String,
    /// Wait for the user to approve the plan before building.
    pub gate_plan: bool,
    /// How many times the reviewer may send the work back.
    pub max_reviews: i64,
    #[serde(default)]
    pub created_at: i64,
}

/// One run of a workflow: a session per stage, and where the work is now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub task: String,
    pub plan_session: Option<String>,
    pub build_session: String,
    pub review_session: Option<String>,
    pub gate_plan: bool,
    pub max_reviews: i64,
    /// Times the reviewer sent the work back so far.
    pub reviews: i64,
    /// "plan", "approve" (plan waiting for the user), "build" or "review".
    pub stage: String,
    /// "active", "done" or "stopped".
    pub status: String,
    pub reason: Option<String>,
    /// The latest plan.
    pub plan: Option<String>,
    /// Output captured during the current turn (Claude Code's ExitPlanMode plan), used when it ends.
    #[serde(skip)]
    pub captured: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

const FLOW_COLS: &str = "id, project_id, name, task, plan_session, build_session, review_session, gate_plan, max_reviews, reviews, stage, status, reason, plan, captured, created_at, updated_at";

fn row_to_flow(r: &rusqlite::Row<'_>) -> rusqlite::Result<FlowRow> {
    Ok(FlowRow {
        id: r.get(0)?,
        project_id: r.get(1)?,
        name: r.get(2)?,
        task: r.get(3)?,
        plan_session: r.get(4)?,
        build_session: r.get(5)?,
        review_session: r.get(6)?,
        gate_plan: r.get::<_, i64>(7)? != 0,
        max_reviews: r.get(8)?,
        reviews: r.get(9)?,
        stage: r.get(10)?,
        status: r.get(11)?,
        reason: r.get(12)?,
        plan: r.get(13)?,
        captured: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

/// Changes to a flow; `None` leaves a field as it is.
#[derive(Default)]
pub struct FlowUpdate<'a> {
    pub stage: Option<&'a str>,
    pub status: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub reviews: Option<i64>,
    pub plan: Option<&'a str>,
    /// `Some(None)` clears the captured output.
    pub captured: Option<Option<&'a str>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub session_id: String,
    pub seq: i64,
    pub ts: i64,
    /// "claude" for provider output, "agentdeck" for events AgentDeck itself records.
    pub source: String,
    pub kind: String,
    pub payload: serde_json::Value,
}

pub struct Db {
    conn: Mutex<Connection>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

static SECRET_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"sk-ant-[A-Za-z0-9_\-]{8,}",
        r"sk-[A-Za-z0-9]{32,}",
        r"gh[pousr]_[A-Za-z0-9]{20,}",
        r"github_pat_[A-Za-z0-9_]{20,}",
        r"AKIA[0-9A-Z]{16}",
        r"xox[abprs]-[A-Za-z0-9\-]{10,}",
        r"(?i)-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid secret pattern"))
    .collect()
});

/// Best-effort redaction of well-known credential formats before anything reaches disk.
pub fn redact(text: &str) -> std::borrow::Cow<'_, str> {
    let mut out = std::borrow::Cow::Borrowed(text);
    for re in SECRET_PATTERNS.iter() {
        if re.is_match(&out) {
            out = std::borrow::Cow::Owned(re.replace_all(&out, "[redacted]").into_owned());
        }
    }
    out
}

fn row_to_session(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        project_id: r.get(1)?,
        provider: r.get(2)?,
        model: r.get(3)?,
        title: r.get(4)?,
        status: r.get(5)?,
        snippet: r.get(6)?,
        provider_session_id: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        mode: r.get(10)?,
        profile_id: r.get(11)?,
        worktree: r.get(12)?,
        branch: r.get(13)?,
        effort: r.get(14)?,
    })
}

const SESSION_COLS: &str = "id, project_id, provider, model, title, status, snippet, provider_session_id, created_at, updated_at, mode, profile_id, worktree, branch, effort";

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS projects (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS sessions (
               id TEXT PRIMARY KEY,
               project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
               provider TEXT NOT NULL, model TEXT NOT NULL, title TEXT NOT NULL,
               status TEXT NOT NULL, snippet TEXT NOT NULL DEFAULT '',
               provider_session_id TEXT,
               created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS events (
               session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
               seq INTEGER NOT NULL,
               ts INTEGER NOT NULL,
               source TEXT NOT NULL,
               kind TEXT NOT NULL,
               uuid TEXT,
               payload TEXT NOT NULL,
               PRIMARY KEY (session_id, seq));
             CREATE UNIQUE INDEX IF NOT EXISTS events_uuid ON events(session_id, uuid) WHERE uuid IS NOT NULL;
             CREATE TABLE IF NOT EXISTS relays (
               id TEXT PRIMARY KEY,
               a_session TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
               b_session TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
               a_role TEXT NOT NULL DEFAULT '', b_role TEXT NOT NULL DEFAULT '',
               max_rounds INTEGER NOT NULL, rounds INTEGER NOT NULL DEFAULT 0,
               status TEXT NOT NULL, reason TEXT,
               created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
        )?;
        // v2: per-session mode. Older databases get the column with its default.
        let has_mode = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'mode'")?
            .exists([])?;
        if !has_mode {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN mode TEXT NOT NULL DEFAULT 'default'",
                [],
            )?;
        }
        // v3: custom model profiles.
        let has_profile = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'profile_id'")?
            .exists([])?;
        if !has_profile {
            conn.execute("ALTER TABLE sessions ADD COLUMN profile_id TEXT", [])?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS profiles (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, base_url TEXT NOT NULL,
               model TEXT NOT NULL, small_model TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL);",
        )?;
        // v6: sessions that work in their own git worktree.
        let has_worktree = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'worktree'")?
            .exists([])?;
        if !has_worktree {
            conn.execute_batch("ALTER TABLE sessions ADD COLUMN worktree TEXT; ALTER TABLE sessions ADD COLUMN branch TEXT;")?;
        }
        // v7: per-session reasoning effort (NULL: the agent's default).
        let has_effort = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'effort'")?
            .exists([])?;
        if !has_effort {
            conn.execute("ALTER TABLE sessions ADD COLUMN effort TEXT", [])?;
        }
        // v5: the Chats place for sessions without a project.
        let has_kind = conn
            .prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name = 'kind'")?
            .exists([])?;
        if !has_kind {
            conn.execute(
                "ALTER TABLE projects ADD COLUMN kind TEXT NOT NULL DEFAULT 'folder'",
                [],
            )?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflows (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, plan_agent TEXT NOT NULL, build_agent TEXT NOT NULL,
               review_agent TEXT NOT NULL, gate_plan INTEGER NOT NULL, max_reviews INTEGER NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS flows (
               id TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT NOT NULL, task TEXT NOT NULL,
               plan_session TEXT, build_session TEXT NOT NULL, review_session TEXT,
               gate_plan INTEGER NOT NULL, max_reviews INTEGER NOT NULL, reviews INTEGER NOT NULL DEFAULT 0,
               stage TEXT NOT NULL, status TEXT NOT NULL, reason TEXT, plan TEXT, captured TEXT,
               created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
        )?;
        conn.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            params![SCHEMA_VERSION.to_string()],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked mid-call; SQLite itself stays consistent.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /* ------------------------------------------------------------ projects */

    pub fn add_project(&self, name: &str, path: &str) -> rusqlite::Result<Project> {
        self.add_project_kind(name, path, "folder")
    }

    pub fn add_project_kind(
        &self,
        name: &str,
        path: &str,
        kind: &str,
    ) -> rusqlite::Result<Project> {
        let c = self.conn();
        if let Some(p) = c
            .query_row(
                &format!("SELECT {PROJECT_COLS} FROM projects WHERE path = ?1"),
                params![path],
                row_to_project,
            )
            .optional()?
        {
            return Ok(p);
        }
        let p = Project {
            id: format!("p-{}", uuid::Uuid::new_v4()),
            name: name.into(),
            path: path.into(),
            kind: kind.into(),
            created_at: now_ms(),
        };
        c.execute(
            "INSERT INTO projects(id, name, path, kind, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![p.id, p.name, p.path, p.kind, p.created_at],
        )?;
        Ok(p)
    }

    pub fn list_projects(&self) -> rusqlite::Result<Vec<Project>> {
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT {PROJECT_COLS} FROM projects ORDER BY created_at"
        ))?;
        let rows = st.query_map([], row_to_project)?;
        rows.collect()
    }

    pub fn project(&self, id: &str) -> rusqlite::Result<Option<Project>> {
        self.conn()
            .query_row(
                &format!("SELECT {PROJECT_COLS} FROM projects WHERE id = ?1"),
                params![id],
                row_to_project,
            )
            .optional()
    }

    /// The Chats place, if it was made already.
    pub fn chats_project(&self) -> rusqlite::Result<Option<Project>> {
        self.conn()
            .query_row(
                &format!("SELECT {PROJECT_COLS} FROM projects WHERE kind = 'chats' LIMIT 1"),
                [],
                row_to_project,
            )
            .optional()
    }

    pub fn remove_project(&self, id: &str) -> rusqlite::Result<()> {
        self.conn()
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Puts a session in another project. The agent's own conversation id is dropped: it belongs
    /// to the old folder, so the agent starts fresh in the new one.
    /// Moves a session to another project. The agent's conversation goes along when
    /// `keep_conversation` is set; otherwise the agent starts fresh in the new folder.
    pub fn move_session(
        &self,
        id: &str,
        project_id: &str,
        keep_conversation: bool,
    ) -> rusqlite::Result<Option<SessionRow>> {
        let sql = if keep_conversation {
            "UPDATE sessions SET project_id = ?2, status = 'idle', updated_at = ?3 WHERE id = ?1"
        } else {
            "UPDATE sessions SET project_id = ?2, provider_session_id = NULL, status = 'idle', updated_at = ?3 WHERE id = ?1"
        };
        self.conn().execute(sql, params![id, project_id, now_ms()])?;
        self.session(id)
    }

    /* ------------------------------------------------------------ sessions */

    pub fn create_session(
        &self,
        project_id: &str,
        provider: &str,
        model: &str,
        title: &str,
    ) -> rusqlite::Result<SessionRow> {
        let now = now_ms();
        let s = SessionRow {
            id: format!("s-{}", uuid::Uuid::new_v4()),
            project_id: project_id.into(),
            provider: provider.into(),
            model: model.into(),
            title: title.into(),
            status: "idle".into(),
            snippet: "No messages yet".into(),
            provider_session_id: None,
            created_at: now,
            updated_at: now,
            mode: "default".into(),
            profile_id: None,
            worktree: None,
            branch: None,
            effort: None,
        };
        self.conn().execute(
            &format!("INSERT INTO sessions({SESSION_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"),
            params![s.id, s.project_id, s.provider, s.model, s.title, s.status, s.snippet, s.provider_session_id, s.created_at, s.updated_at, s.mode, s.profile_id, s.worktree, s.branch, s.effort],
        )?;
        Ok(s)
    }

    pub fn list_sessions(&self) -> rusqlite::Result<Vec<SessionRow>> {
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT {SESSION_COLS} FROM sessions ORDER BY updated_at DESC"
        ))?;
        let rows = st.query_map([], row_to_session)?;
        rows.collect()
    }

    pub fn session(&self, id: &str) -> rusqlite::Result<Option<SessionRow>> {
        self.conn()
            .query_row(
                &format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"),
                params![id],
                row_to_session,
            )
            .optional()
    }

    pub fn set_session_profile(
        &self,
        id: &str,
        profile_id: Option<&str>,
    ) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute(
            "UPDATE sessions SET profile_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, profile_id, now_ms()],
        )?;
        self.session(id)
    }

    /* ------------------------------------------------------------ profiles */

    pub fn list_profiles(&self) -> rusqlite::Result<Vec<Profile>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id, name, kind, base_url, model, small_model, created_at FROM profiles ORDER BY created_at")?;
        let rows = st.query_map([], |r| {
            Ok(Profile {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                base_url: r.get(3)?,
                model: r.get(4)?,
                small_model: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn profile(&self, id: &str) -> rusqlite::Result<Option<Profile>> {
        Ok(self.list_profiles()?.into_iter().find(|p| p.id == id))
    }

    /// Inserts or updates a profile (by id; a new id is made when empty).
    pub fn save_profile(&self, p: &Profile) -> rusqlite::Result<Profile> {
        let mut p = p.clone();
        if p.id.is_empty() {
            p.id = format!("m-{}", uuid::Uuid::new_v4());
            p.created_at = now_ms();
        }
        self.conn().execute(
            "INSERT INTO profiles(id, name, kind, base_url, model, small_model, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name = ?2, kind = ?3, base_url = ?4, model = ?5, small_model = ?6",
            params![p.id, p.name, p.kind, p.base_url, p.model, p.small_model, p.created_at],
        )?;
        Ok(p)
    }

    pub fn delete_profile(&self, id: &str) -> rusqlite::Result<()> {
        let c = self.conn();
        c.execute(
            "UPDATE sessions SET profile_id = NULL WHERE profile_id = ?1",
            params![id],
        )?;
        c.execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_session_agent(
        &self,
        id: &str,
        provider: &str,
        model: &str,
        profile_id: Option<&str>,
        effort: Option<&str>,
    ) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute(
            "UPDATE sessions SET provider = ?2, model = ?3, profile_id = ?4, effort = ?6, provider_session_id = NULL, updated_at = ?5 WHERE id = ?1",
            params![id, provider, model, profile_id, now_ms(), effort],
        )?;
        self.session(id)
    }

    /// Sets (or with None clears) a session's reasoning effort.
    pub fn set_session_effort(&self, id: &str, effort: Option<&str>) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute(
            "UPDATE sessions SET effort = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, effort, now_ms()],
        )?;
        self.session(id)
    }

    pub fn set_session_mode(&self, id: &str, mode: &str) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute(
            "UPDATE sessions SET mode = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, mode, now_ms()],
        )?;
        self.session(id)
    }

    pub fn update_session(
        &self,
        id: &str,
        status: Option<&str>,
        snippet: Option<&str>,
        title: Option<&str>,
        model: Option<&str>,
        provider_session_id: Option<&str>,
    ) -> rusqlite::Result<Option<SessionRow>> {
        {
            let c = self.conn();
            c.execute(
                "UPDATE sessions SET
                   status = COALESCE(?2, status),
                   snippet = COALESCE(?3, snippet),
                   title = COALESCE(?4, title),
                   model = COALESCE(?5, model),
                   provider_session_id = COALESCE(?6, provider_session_id),
                   updated_at = ?7
                 WHERE id = ?1",
                params![
                    id,
                    status,
                    snippet,
                    title,
                    model,
                    provider_session_id,
                    now_ms()
                ],
            )?;
        }
        self.session(id)
    }

    pub fn delete_session(&self, id: &str) -> rusqlite::Result<()> {
        self.conn()
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* ------------------------------------------------------------ workflows */

    pub fn list_workflows(&self) -> rusqlite::Result<Vec<Workflow>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id, name, plan_agent, build_agent, review_agent, gate_plan, max_reviews, created_at FROM workflows ORDER BY created_at")?;
        let rows = st.query_map([], |r| {
            Ok(Workflow {
                id: r.get(0)?,
                name: r.get(1)?,
                plan_agent: r.get(2)?,
                build_agent: r.get(3)?,
                review_agent: r.get(4)?,
                gate_plan: r.get::<_, i64>(5)? != 0,
                max_reviews: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Inserts or updates a workflow (by id; a new id is made when empty).
    pub fn save_workflow(&self, w: &Workflow) -> rusqlite::Result<Workflow> {
        let mut w = w.clone();
        if w.id.is_empty() {
            w.id = format!("w-{}", uuid::Uuid::new_v4());
            w.created_at = now_ms();
        }
        self.conn().execute(
            "INSERT INTO workflows(id, name, plan_agent, build_agent, review_agent, gate_plan, max_reviews, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name = ?2, plan_agent = ?3, build_agent = ?4, review_agent = ?5, gate_plan = ?6, max_reviews = ?7",
            params![w.id, w.name, w.plan_agent, w.build_agent, w.review_agent, w.gate_plan as i64, w.max_reviews, w.created_at],
        )?;
        Ok(w)
    }

    pub fn delete_workflow(&self, id: &str) -> rusqlite::Result<()> {
        self.conn()
            .execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* ------------------------------------------------------------ flows */

    #[allow(clippy::too_many_arguments)]
    pub fn create_flow(
        &self,
        project_id: &str,
        name: &str,
        task: &str,
        plan_session: Option<&str>,
        build_session: &str,
        review_session: Option<&str>,
        gate_plan: bool,
        max_reviews: i64,
    ) -> rusqlite::Result<FlowRow> {
        let now = now_ms();
        let id = format!("f-{}", uuid::Uuid::new_v4());
        let stage = if plan_session.is_some() {
            "plan"
        } else {
            "build"
        };
        self.conn().execute(
            &format!("INSERT INTO flows({FLOW_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, 'active', NULL, NULL, NULL, ?11, ?11)"),
            params![id, project_id, name, task, plan_session, build_session, review_session, gate_plan as i64, max_reviews, stage, now],
        )?;
        self.flow(&id).map(|r| r.expect("just inserted"))
    }

    pub fn flow(&self, id: &str) -> rusqlite::Result<Option<FlowRow>> {
        self.conn()
            .query_row(
                &format!("SELECT {FLOW_COLS} FROM flows WHERE id = ?1"),
                params![id],
                row_to_flow,
            )
            .optional()
    }

    /// The active flow a session takes part in, if any.
    pub fn active_flow_for(&self, session_id: &str) -> rusqlite::Result<Option<FlowRow>> {
        self.conn()
            .query_row(
                &format!("SELECT {FLOW_COLS} FROM flows WHERE status = 'active' AND (plan_session = ?1 OR build_session = ?1 OR review_session = ?1) ORDER BY created_at DESC LIMIT 1"),
                params![session_id],
                row_to_flow,
            )
            .optional()
    }

    pub fn list_flows(&self) -> rusqlite::Result<Vec<FlowRow>> {
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT {FLOW_COLS} FROM flows ORDER BY created_at DESC LIMIT 200"
        ))?;
        let rows = st.query_map([], row_to_flow)?;
        rows.collect()
    }

    pub fn update_flow(&self, id: &str, u: FlowUpdate<'_>) -> rusqlite::Result<Option<FlowRow>> {
        let (set_captured, captured) = match u.captured {
            Some(v) => (1, v),
            None => (0, None),
        };
        self.conn().execute(
            "UPDATE flows SET stage = COALESCE(?2, stage), status = COALESCE(?3, status), reason = COALESCE(?4, reason),
               reviews = COALESCE(?5, reviews), plan = COALESCE(?6, plan),
               captured = CASE WHEN ?7 = 1 THEN ?8 ELSE captured END, updated_at = ?9 WHERE id = ?1",
            params![id, u.stage, u.status, u.reason, u.reviews, u.plan, set_captured, captured, now_ms()],
        )?;
        self.flow(id)
    }

    /* ------------------------------------------------------------ relays */

    pub fn create_relay(
        &self,
        a: &str,
        b: &str,
        a_role: &str,
        b_role: &str,
        max_rounds: i64,
    ) -> rusqlite::Result<RelayRow> {
        let now = now_ms();
        let id = format!("r-{}", uuid::Uuid::new_v4());
        self.conn().execute(
            &format!("INSERT INTO relays({RELAY_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'active', NULL, ?7, ?7)"),
            params![id, a, b, a_role, b_role, max_rounds, now],
        )?;
        self.relay(&id).map(|r| r.expect("just inserted"))
    }

    pub fn relay(&self, id: &str) -> rusqlite::Result<Option<RelayRow>> {
        self.conn()
            .query_row(
                &format!("SELECT {RELAY_COLS} FROM relays WHERE id = ?1"),
                params![id],
                row_to_relay,
            )
            .optional()
    }

    pub fn active_relay_for(&self, session_id: &str) -> rusqlite::Result<Option<RelayRow>> {
        self.conn()
            .query_row(
                &format!("SELECT {RELAY_COLS} FROM relays WHERE status = 'active' AND (a_session = ?1 OR b_session = ?1) ORDER BY created_at DESC LIMIT 1"),
                params![session_id],
                row_to_relay,
            )
            .optional()
    }

    pub fn list_relays(&self) -> rusqlite::Result<Vec<RelayRow>> {
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT {RELAY_COLS} FROM relays ORDER BY created_at DESC"
        ))?;
        let rows = st.query_map([], row_to_relay)?;
        rows.collect()
    }

    pub fn update_relay(
        &self,
        id: &str,
        rounds: Option<i64>,
        status: Option<&str>,
        reason: Option<&str>,
    ) -> rusqlite::Result<Option<RelayRow>> {
        self.conn().execute(
            "UPDATE relays SET rounds = COALESCE(?2, rounds), status = COALESCE(?3, status), reason = COALESCE(?4, reason), updated_at = ?5 WHERE id = ?1",
            params![id, rounds, status, reason, now_ms()],
        )?;
        self.relay(id)
    }

    /// Sessions whose process can't have survived an app exit. Returns the ids that were reset.
    /// Relays that were active are stopped too.
    pub fn recover_after_restart(&self) -> rusqlite::Result<Vec<String>> {
        self.conn().execute(
            "UPDATE relays SET status = 'stopped', reason = 'AgentDeck closed while the agents were talking', updated_at = ?1 WHERE status = 'active'",
            params![now_ms()],
        )?;
        self.conn().execute(
            "UPDATE flows SET status = 'stopped', reason = 'AgentDeck closed while the workflow was running', updated_at = ?1 WHERE status = 'active'",
            params![now_ms()],
        )?;
        let ids: Vec<String> = {
            let c = self.conn();
            let mut st =
                c.prepare("SELECT id FROM sessions WHERE status IN ('running', 'waiting')")?;
            let rows = st.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for id in &ids {
            self.append_event(
                id,
                "agentdeck",
                "app_restart",
                &serde_json::json!({ "message": "AgentDeck closed while this session was working. The agent process stopped with it." }),
            )?;
            self.update_session(
                id,
                Some("idle"),
                Some("Stopped when AgentDeck closed"),
                None,
                None,
                None,
            )?;
        }
        Ok(ids)
    }

    /* ------------------------------------------------------------ events */

    /// Appends one event with the next sequence number. Returns None for a duplicate provider event
    /// (same `uuid` already stored for this session).
    pub fn append_event(
        &self,
        session_id: &str,
        source: &str,
        kind: &str,
        payload: &serde_json::Value,
    ) -> rusqlite::Result<Option<StoredEvent>> {
        let text = redact(&payload.to_string()).into_owned();
        let payload: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        let uuid = payload
            .get("uuid")
            .and_then(|u| u.as_str())
            .map(str::to_owned);
        let ts = now_ms();
        let mut c = self.conn();
        let tx = c.transaction()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO events(session_id, seq, ts, source, kind, uuid, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session_id, seq, ts, source, kind, uuid, text],
        )?;
        tx.commit()?;
        Ok((inserted == 1).then(|| StoredEvent {
            session_id: session_id.into(),
            seq,
            ts,
            source: source.into(),
            kind: kind.into(),
            payload,
        }))
    }

    /// Stores many AgentDeck events with their own timestamps in one transaction (imports).
    pub fn append_events_at(
        &self,
        session_id: &str,
        items: &[(&str, serde_json::Value, i64)],
    ) -> rusqlite::Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM events WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;
        for (kind, payload, ts) in items {
            seq += 1;
            let text = redact(&payload.to_string()).into_owned();
            tx.execute(
                "INSERT INTO events(session_id, seq, ts, source, kind, uuid, payload) VALUES (?1, ?2, ?3, 'agentdeck', ?4, NULL, ?5)",
                params![session_id, seq, ts, kind, text],
            )?;
        }
        tx.commit()
    }

    /// A session brought in from an agent's own history, keeping its dates.
    #[allow(clippy::too_many_arguments)]
    pub fn create_imported_session(
        &self,
        project_id: &str,
        provider: &str,
        title: &str,
        snippet: &str,
        provider_session_id: Option<&str>,
        created_at: i64,
        updated_at: i64,
    ) -> rusqlite::Result<SessionRow> {
        let s = SessionRow {
            id: format!("s-{}", uuid::Uuid::new_v4()),
            project_id: project_id.into(),
            provider: provider.into(),
            model: "default".into(),
            title: title.into(),
            status: "idle".into(),
            snippet: snippet.into(),
            provider_session_id: provider_session_id.map(str::to_owned),
            created_at,
            updated_at,
            mode: "default".into(),
            profile_id: None,
            worktree: None,
            branch: None,
            effort: None,
        };
        self.conn().execute(
            &format!("INSERT INTO sessions({SESSION_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"),
            params![s.id, s.project_id, s.provider, s.model, s.title, s.status, s.snippet, s.provider_session_id, s.created_at, s.updated_at, s.mode, s.profile_id, s.worktree, s.branch, s.effort],
        )?;
        Ok(s)
    }

    /// The AgentDeck session made from an agent's session, if it was imported or started here.
    pub fn session_for_agent_id(
        &self,
        provider: &str,
        agent_id: &str,
    ) -> rusqlite::Result<Option<String>> {
        let c = self.conn();
        if let Some(id) = c
            .query_row(
                "SELECT id FROM sessions WHERE provider = ?1 AND provider_session_id = ?2 LIMIT 1",
                params![provider, agent_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(Some(id));
        }
        c.query_row(
            "SELECT s.id FROM meta m JOIN sessions s ON s.id = m.value WHERE m.key = ?1",
            params![format!("imported:{provider}:{agent_id}")],
            |r| r.get(0),
        )
        .optional()
    }

    /// Forgets the agent's own conversation id, so the next message starts a fresh one.
    /// Renames a session without moving it up the list.
    pub fn rename_session(&self, id: &str, title: &str) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute("UPDATE sessions SET title = ?2 WHERE id = ?1", params![id, title])?;
        self.session(id)
    }

    pub fn update_session_agent_id_clear(&self, id: &str) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute(
            "UPDATE sessions SET provider_session_id = NULL, status = 'idle', updated_at = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )?;
        self.session(id)
    }

    /// Puts a session in a git worktree (or back in the project folder with None).
    pub fn set_session_worktree(
        &self,
        id: &str,
        worktree: Option<&str>,
        branch: Option<&str>,
    ) -> rusqlite::Result<Option<SessionRow>> {
        self.conn().execute(
            "UPDATE sessions SET worktree = ?2, branch = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, worktree, branch, now_ms()],
        )?;
        self.session(id)
    }

    /// The folder a session's agent works in: its worktree, or the project folder.
    pub fn session_dir(&self, s: &SessionRow) -> rusqlite::Result<Option<String>> {
        if let Some(w) = s.worktree.as_deref().filter(|w| !w.is_empty()) {
            return Ok(Some(w.to_string()));
        }
        Ok(self.project(&s.project_id)?.map(|p| p.path))
    }

    pub fn meta(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn()
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0))
            .optional()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn mark_imported(
        &self,
        provider: &str,
        agent_id: &str,
        session_id: &str,
    ) -> rusqlite::Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
            params![format!("imported:{provider}:{agent_id}"), session_id],
        )?;
        Ok(())
    }

    pub fn events(&self, session_id: &str) -> rusqlite::Result<Vec<StoredEvent>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT session_id, seq, ts, source, kind, payload FROM events WHERE session_id = ?1 ORDER BY seq")?;
        let rows = st.query_map(params![session_id], |r| {
            let payload: String = r.get(5)?;
            Ok(StoredEvent {
                session_id: r.get(0)?,
                seq: r.get(1)?,
                ts: r.get(2)?,
                source: r.get(3)?,
                kind: r.get(4)?,
                payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chats_place_and_moving_sessions() {
        let (db, s) = setup();
        assert!(db.chats_project().unwrap().is_none());
        let chats = db.add_project_kind("Chats", "/tmp/chats", "chats").unwrap();
        assert_eq!(db.chats_project().unwrap().unwrap().id, chats.id);
        assert_eq!(db.project(&s.project_id).unwrap().unwrap().kind, "folder");
        db.update_session(&s.id, Some("running"), None, None, None, Some("claude-abc"))
            .unwrap();
        let moved = db.move_session(&s.id, &chats.id, false).unwrap().unwrap();
        assert_eq!(moved.project_id, chats.id);
        assert_eq!(
            moved.provider_session_id, None,
            "the agent starts fresh in the new folder"
        );
        assert_eq!(moved.status, "idle");
    }

    fn setup() -> (Db, SessionRow) {
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project("demo", "/tmp/demo").unwrap();
        let s = db
            .create_session(&p.id, "claude-code", "sonnet", "Test")
            .unwrap();
        (db, s)
    }

    #[test]
    fn old_database_gets_mode_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL, snippet TEXT NOT NULL DEFAULT '', provider_session_id TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             INSERT INTO sessions VALUES ('s1','p1','claude-code','default','t','idle','',NULL,1,1);",
        )
        .unwrap();
        let db = Db::init(conn).unwrap();
        let s = db.session("s1").unwrap().unwrap();
        assert_eq!(s.mode, "default");
        assert_eq!(
            db.set_session_mode("s1", "plan").unwrap().unwrap().mode,
            "plan"
        );
        assert_eq!(s.effort, None, "older sessions use the agent's default effort");
        assert_eq!(
            db.set_session_effort("s1", Some("high")).unwrap().unwrap().effort.as_deref(),
            Some("high")
        );
        assert_eq!(db.set_session_effort("s1", None).unwrap().unwrap().effort, None);
    }

    #[test]
    fn events_are_ordered_and_numbered() {
        let (db, s) = setup();
        for i in 0..5 {
            db.append_event(&s.id, "claude", "assistant", &json!({ "i": i }))
                .unwrap();
        }
        let ev = db.events(&s.id).unwrap();
        assert_eq!(
            ev.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(ev[4].payload["i"], 4);
    }

    #[test]
    fn duplicate_provider_events_are_dropped() {
        let (db, s) = setup();
        let e = json!({ "type": "assistant", "uuid": "u-1" });
        assert!(db
            .append_event(&s.id, "claude", "assistant", &e)
            .unwrap()
            .is_some());
        assert!(db
            .append_event(&s.id, "claude", "assistant", &e)
            .unwrap()
            .is_none());
        assert_eq!(db.events(&s.id).unwrap().len(), 1);
    }

    #[test]
    fn sessions_are_isolated() {
        let (db, s1) = setup();
        let s2 = db
            .create_session(&s1.project_id, "claude-code", "sonnet", "Other")
            .unwrap();
        db.append_event(&s1.id, "claude", "x", &json!({ "uuid": "same" }))
            .unwrap();
        db.append_event(&s2.id, "claude", "x", &json!({ "uuid": "same" }))
            .unwrap();
        assert_eq!(db.events(&s1.id).unwrap().len(), 1);
        assert_eq!(db.events(&s2.id).unwrap().len(), 1);
    }

    #[test]
    fn secrets_are_redacted_before_storage() {
        let (db, s) = setup();
        db.append_event(&s.id, "claude", "user", &json!({ "out": "key=sk-ant-api03-abcdefghijklmnop and ghp_abcdefghijklmnopqrstuvwxyz123" })).unwrap();
        let stored = db.events(&s.id).unwrap()[0].payload["out"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(!stored.contains("sk-ant-api03"));
        assert!(!stored.contains("ghp_"));
        assert_eq!(stored.matches("[redacted]").count(), 2);
    }

    #[test]
    fn restart_recovery_resets_live_sessions() {
        let (db, s) = setup();
        db.update_session(&s.id, Some("waiting"), None, None, None, None)
            .unwrap();
        let reset = db.recover_after_restart().unwrap();
        assert_eq!(reset, vec![s.id.clone()]);
        let row = db.session(&s.id).unwrap().unwrap();
        assert_eq!(row.status, "idle");
        assert_eq!(
            db.events(&s.id).unwrap().last().unwrap().kind,
            "app_restart"
        );
        assert!(db.recover_after_restart().unwrap().is_empty());
    }

    #[test]
    fn adding_the_same_folder_twice_returns_the_same_project() {
        let db = Db::open_in_memory().unwrap();
        let a = db.add_project("x", "/tmp/x").unwrap();
        let b = db.add_project("x", "/tmp/x").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(db.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_session_removes_its_events() {
        let (db, s) = setup();
        db.append_event(&s.id, "claude", "x", &json!({})).unwrap();
        db.delete_session(&s.id).unwrap();
        assert!(db.events(&s.id).unwrap().is_empty());
        assert!(db.session(&s.id).unwrap().is_none());
    }
}
