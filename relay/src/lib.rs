//! AgentDeck Relay: carries a team's presence and messages between AgentDeck apps, instantly.
//!
//! AgentDeck works without it (presence and messages travel through the project's git remote,
//! as `refs/agentdeck/*`); the relay is the faster, hostable alternative: no push access needed,
//! delivery within a second, several repositories per team.
//!
//! What it holds: teams, invite codes, members (a name, an email, a token), each member's latest
//! presence per repository, and messages. Never code, diffs or files.
//!
//! Security:
//! - A member's token is shown once when they join and stored only as a SHA-256 hash; so are
//!   invite codes.
//! - The server decides who sent what: a message's `from` and a presence's name and email are
//!   always the member's own, whatever the request says.
//! - A member sees presence of their team and messages from them, to them or to everyone.
//! - Bodies are limited (256 KB), texts too (8000 characters), writes are rate limited per
//!   member, and everything older than the retention period is deleted.
//! - TLS is left to a reverse proxy in front (see relay/README.md).
//!
//! HTTP API (JSON; `Authorization: Bearer <token>` except join and health):
//! - `POST /v1/join` `{invite, name, email}` → `{token, team, member}`
//! - `GET  /v1/me` · `GET /v1/members`
//! - `PUT  /v1/presence?repo=R` (a presence) · `GET /v1/presence?repo=R` · `DELETE /v1/presence?repo=R`
//! - `POST /v1/messages?repo=R` (a message) · `GET /v1/messages?repo=R&since=MS`
//! - `GET  /v1/events?after=SEQ&wait=SECONDS` → `{cursor, events: [{seq, repo, kind}]}`, waiting
//!   up to `wait` (≤ 30) seconds for something new: long polling, so it passes any proxy.
//! - `GET  /healthz`

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub const MAX_BODY: usize = 256 * 1024;
pub const MAX_TEXT: usize = 8000;
const MAX_WAIT_S: u64 = 30;
/// Writes (presence, messages) a member may make per minute.
const WRITES_PER_MINUTE: u32 = 120;
/// Joins (right or wrong invite) per minute, server-wide: invite codes can't be guessed.
const JOINS_PER_MINUTE: u32 = 30;

/* ------------------------------------------------------------------------------- store */

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn hash(secret: &str) -> String {
    let d = Sha256::digest(secret.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// A random secret with a readable prefix: "adr_…" (tokens), "adi_…" (invites).
pub fn secret(prefix: &str) -> String {
    use base64::Engine;
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    format!(
        "{prefix}_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

/// An event's sequence number, repository and kind.
pub type EventRef = (i64, String, String);

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Team {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Member {
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub email: String,
    pub created_at: i64,
    pub revoked: bool,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS teams (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS invites (
               code_hash TEXT PRIMARY KEY, team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
               created_at INTEGER NOT NULL, expires_at INTEGER, revoked INTEGER NOT NULL DEFAULT 0, label TEXT NOT NULL DEFAULT '');
             CREATE TABLE IF NOT EXISTS members (
               id TEXT PRIMARY KEY, team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
               name TEXT NOT NULL, email TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE,
               created_at INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0, last_seen INTEGER);
             CREATE TABLE IF NOT EXISTS events (
               seq INTEGER PRIMARY KEY AUTOINCREMENT, team_id TEXT NOT NULL, repo TEXT NOT NULL,
               kind TEXT NOT NULL, sender_email TEXT NOT NULL, recipient TEXT NOT NULL,
               body TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS events_team ON events(team_id, seq);
             CREATE TABLE IF NOT EXISTS presence (
               team_id TEXT NOT NULL, repo TEXT NOT NULL, email TEXT NOT NULL, body TEXT NOT NULL,
               updated_at INTEGER NOT NULL, PRIMARY KEY (team_id, repo, email));",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn create_team(&self, name: &str) -> rusqlite::Result<Team> {
        let t = Team {
            id: format!("t_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
            name: name.trim().into(),
        };
        self.c().execute(
            "INSERT INTO teams(id, name, created_at) VALUES (?1, ?2, ?3)",
            params![t.id, t.name, now_ms()],
        )?;
        Ok(t)
    }

    pub fn teams(&self) -> rusqlite::Result<Vec<(Team, i64)>> {
        let c = self.c();
        let mut st = c.prepare("SELECT t.id, t.name, (SELECT COUNT(*) FROM members m WHERE m.team_id = t.id AND m.revoked = 0) FROM teams t ORDER BY t.created_at")?;
        let rows = st.query_map([], |r| {
            Ok((
                Team {
                    id: r.get(0)?,
                    name: r.get(1)?,
                },
                r.get(2)?,
            ))
        })?;
        rows.collect()
    }

    pub fn team(&self, id: &str) -> rusqlite::Result<Option<Team>> {
        self.c()
            .query_row(
                "SELECT id, name FROM teams WHERE id = ?1",
                params![id],
                |r| {
                    Ok(Team {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .optional()
    }

    /// A new invite code for a team (shown once). `days`: how long it works (None: until revoked).
    pub fn create_invite(
        &self,
        team_id: &str,
        days: Option<u32>,
        label: &str,
    ) -> Result<String, String> {
        self.team(team_id)
            .map_err(|e| e.to_string())?
            .ok_or("No such team")?;
        let code = secret("adi");
        let now = now_ms();
        self.c()
            .execute(
                "INSERT INTO invites(code_hash, team_id, created_at, expires_at, label) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![hash(&code), team_id, now, days.map(|d| now + d as i64 * 86_400_000), label],
            )
            .map_err(|e| e.to_string())?;
        Ok(code)
    }

    /// Turns off every invite of a team (members who joined stay).
    pub fn revoke_invites(&self, team_id: &str) -> rusqlite::Result<usize> {
        self.c().execute(
            "UPDATE invites SET revoked = 1 WHERE team_id = ?1",
            params![team_id],
        )
    }

    /// Joins with an invite: a new member and their token (shown once).
    pub fn join(
        &self,
        invite: &str,
        name: &str,
        email: &str,
    ) -> Result<(String, Team, Member), String> {
        let name = clip(name.trim(), 80);
        let email = clip(&email.trim().to_lowercase(), 120);
        if name.is_empty() || !email.contains('@') {
            return Err("A name and an email are needed".into());
        }
        let team_id: Option<String> = self
            .c()
            .query_row(
                "SELECT team_id FROM invites WHERE code_hash = ?1 AND revoked = 0 AND (expires_at IS NULL OR expires_at > ?2)",
                params![hash(invite.trim()), now_ms()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let team = match team_id {
            Some(id) => self
                .team(&id)
                .map_err(|e| e.to_string())?
                .ok_or("No such team")?,
            None => return Err("That invite isn't valid (wrong, expired or revoked)".into()),
        };
        let token = secret("adr");
        let m = Member {
            id: format!("m_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
            team_id: team.id.clone(),
            name,
            email,
            created_at: now_ms(),
            revoked: false,
        };
        self.c()
            .execute(
                "INSERT INTO members(id, team_id, name, email, token_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![m.id, m.team_id, m.name, m.email, hash(&token), m.created_at],
            )
            .map_err(|e| e.to_string())?;
        Ok((token, team, m))
    }

    pub fn member_by_token(&self, token: &str) -> rusqlite::Result<Option<Member>> {
        let c = self.c();
        let m = c
            .query_row(
                "SELECT id, team_id, name, email, created_at, revoked FROM members WHERE token_hash = ?1 AND revoked = 0",
                params![hash(token)],
                |r| Ok(Member { id: r.get(0)?, team_id: r.get(1)?, name: r.get(2)?, email: r.get(3)?, created_at: r.get(4)?, revoked: r.get::<_, i64>(5)? != 0 }),
            )
            .optional()?;
        if let Some(m) = &m {
            c.execute(
                "UPDATE members SET last_seen = ?2 WHERE id = ?1",
                params![m.id, now_ms()],
            )?;
        }
        Ok(m)
    }

    pub fn members(&self, team_id: &str) -> rusqlite::Result<Vec<(Member, Option<i64>)>> {
        let c = self.c();
        let mut st = c.prepare("SELECT id, team_id, name, email, created_at, revoked, last_seen FROM members WHERE team_id = ?1 ORDER BY created_at")?;
        let rows = st.query_map(params![team_id], |r| {
            Ok((
                Member {
                    id: r.get(0)?,
                    team_id: r.get(1)?,
                    name: r.get(2)?,
                    email: r.get(3)?,
                    created_at: r.get(4)?,
                    revoked: r.get::<_, i64>(5)? != 0,
                },
                r.get(6)?,
            ))
        })?;
        rows.collect()
    }

    pub fn revoke_member(&self, member_id: &str) -> rusqlite::Result<bool> {
        Ok(self.c().execute(
            "UPDATE members SET revoked = 1 WHERE id = ?1",
            params![member_id],
        )? > 0)
    }

    fn add_event(
        &self,
        m: &Member,
        repo: &str,
        kind: &str,
        recipient: &str,
        body: &str,
    ) -> rusqlite::Result<i64> {
        let c = self.c();
        c.execute(
            "INSERT INTO events(team_id, repo, kind, sender_email, recipient, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![m.team_id, repo, kind, m.email, recipient, body, now_ms()],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn put_presence(&self, m: &Member, repo: &str, mut p: Value) -> rusqlite::Result<i64> {
        // Whatever the request says, it's this member's.
        p["name"] = m.name.clone().into();
        p["email"] = m.email.clone().into();
        p["updatedAt"] = now_ms().into();
        let body = p.to_string();
        self.c().execute(
            "INSERT INTO presence(team_id, repo, email, body, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(team_id, repo, email) DO UPDATE SET body = ?4, updated_at = ?5",
            params![m.team_id, repo, m.email, body, now_ms()],
        )?;
        self.add_event(m, repo, "presence", "*", "")
    }

    /// Takes a member's presence off a repository (they left the team there).
    pub fn delete_presence(&self, m: &Member, repo: &str) -> rusqlite::Result<i64> {
        self.c().execute(
            "DELETE FROM presence WHERE team_id = ?1 AND repo = ?2 AND email = ?3",
            params![m.team_id, repo, m.email],
        )?;
        self.add_event(m, repo, "presence", "*", "")
    }

    pub fn presence(&self, m: &Member, repo: &str) -> rusqlite::Result<Vec<Value>> {
        let c = self.c();
        let mut st = c.prepare(
            "SELECT body FROM presence WHERE team_id = ?1 AND repo = ?2 ORDER BY updated_at DESC",
        )?;
        let rows = st.query_map(params![m.team_id, repo], |r| r.get::<_, String>(0))?;
        Ok(rows
            .filter_map(|b| b.ok().and_then(|b| serde_json::from_str(&b).ok()))
            .collect())
    }

    pub fn post_message(
        &self,
        m: &Member,
        repo: &str,
        mut msg: Value,
    ) -> Result<(i64, Value), String> {
        let id = msg["id"].as_str().unwrap_or("").to_string();
        let thread = msg["thread"].as_str().unwrap_or("").to_string();
        let kind = msg["kind"].as_str().unwrap_or("");
        let text = msg["text"].as_str().unwrap_or("").trim().to_string();
        let to = msg["to"].as_str().unwrap_or("").trim().to_lowercase();
        if !ok_id(&id) || !ok_id(&thread) {
            return Err("A message needs an id and a thread of letters, digits and dashes".into());
        }
        if !matches!(kind, "question" | "answer" | "note" | "review") {
            return Err("Unknown kind of message".into());
        }
        if text.is_empty() || text.chars().count() > MAX_TEXT {
            return Err(format!("A message's text is 1 to {MAX_TEXT} characters"));
        }
        if to != "*" && !to.contains('@') {
            return Err("Send to an email or to \"*\" (everyone)".into());
        }
        let dup: bool = self
            .c()
            .query_row(
                "SELECT 1 FROM events WHERE team_id = ?1 AND kind = 'message' AND json_extract(body, '$.id') = ?2",
                params![m.team_id, id],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_some();
        if dup {
            return Err("A message with that id was already sent".into());
        }
        msg["from"] = json!({ "name": m.name, "email": m.email });
        msg["to"] = to.clone().into();
        msg["text"] = text.into();
        msg["createdAt"] = now_ms().into();
        let seq = self
            .add_event(m, repo, "message", &to, &msg.to_string())
            .map_err(|e| e.to_string())?;
        Ok((seq, msg))
    }

    /// Messages a member may read in a repository since `since` (ms), oldest first.
    pub fn messages(&self, m: &Member, repo: &str, since: i64) -> rusqlite::Result<Vec<Value>> {
        let c = self.c();
        let mut st = c.prepare(
            "SELECT body FROM events WHERE team_id = ?1 AND repo = ?2 AND kind = 'message' AND created_at > ?3
               AND (sender_email = ?4 OR recipient = ?4 OR recipient = '*') ORDER BY seq LIMIT 2000",
        )?;
        let rows = st.query_map(params![m.team_id, repo, since, m.email], |r| {
            r.get::<_, String>(0)
        })?;
        Ok(rows
            .filter_map(|b| b.ok().and_then(|b| serde_json::from_str(&b).ok()))
            .collect())
    }

    /// What happened after `after` that this member may know about: (seq, repo, kind).
    pub fn events_after(&self, m: &Member, after: i64) -> rusqlite::Result<(i64, Vec<EventRef>)> {
        let c = self.c();
        let last: i64 = c.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM events WHERE team_id = ?1",
            params![m.team_id],
            |r| r.get(0),
        )?;
        let mut st = c.prepare(
            "SELECT seq, repo, kind FROM events WHERE team_id = ?1 AND seq > ?2
               AND sender_email != ?3 AND (kind = 'presence' OR recipient = ?3 OR recipient = '*') ORDER BY seq LIMIT 500",
        )?;
        let rows = st.query_map(params![m.team_id, after, m.email], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        Ok((last, rows.collect::<rusqlite::Result<_>>()?))
    }

    /// Deletes messages and events older than `days`, and presence nobody updated in that time.
    pub fn purge(&self, days: u32) -> rusqlite::Result<usize> {
        let before = now_ms() - days as i64 * 86_400_000;
        let c = self.c();
        let n = c.execute("DELETE FROM events WHERE created_at < ?1", params![before])?;
        Ok(n + c.execute(
            "DELETE FROM presence WHERE updated_at < ?1",
            params![before],
        )?)
    }
}

fn clip(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(n).collect()
}

fn ok_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A repository key from the client: "github.com/acme/shop-app" and the like.
fn ok_repo(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 300
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/:@~".contains(c))
}

/* ------------------------------------------------------------------------------- server */

pub struct Relay {
    pub store: Store,
    /// The latest event sequence number: long polls wake up when it moves.
    tick: watch::Sender<i64>,
    writes: Mutex<HashMap<String, (Instant, u32)>>,
    joins: Mutex<(Instant, u32)>,
}

impl Relay {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self {
            store,
            tick: watch::channel(0).0,
            writes: Mutex::new(HashMap::new()),
            joins: Mutex::new((Instant::now(), 0)),
        })
    }

    fn bump(&self, seq: i64) {
        self.tick.send_replace(seq);
    }

    fn limited(&self, key: &str) -> bool {
        let mut w = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        let e = w.entry(key.to_string()).or_insert((Instant::now(), 0));
        if e.0.elapsed() > Duration::from_secs(60) {
            *e = (Instant::now(), 0);
        }
        e.1 += 1;
        e.1 > WRITES_PER_MINUTE
    }

    fn join_limited(&self) -> bool {
        let mut j = self.joins.lock().unwrap_or_else(|e| e.into_inner());
        if j.0.elapsed() > Duration::from_secs(60) {
            *j = (Instant::now(), 0);
        }
        j.1 += 1;
        j.1 > JOINS_PER_MINUTE
    }
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    eprintln!("agentdeck-relay: {e}");
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong on the relay".into(),
    )
}

fn auth(relay: &Relay, headers: &HeaderMap) -> Result<Member, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if token.is_empty() {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Sign in with your relay token".into(),
        ));
    }
    relay
        .store
        .member_by_token(token)
        .map_err(internal)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNAUTHORIZED,
                "That token isn't valid (or was revoked)".into(),
            )
        })
}

#[derive(Deserialize)]
struct JoinReq {
    invite: String,
    name: String,
    email: String,
}

async fn join(
    State(r): State<Arc<Relay>>,
    Json(req): Json<JoinReq>,
) -> Result<Json<Value>, ApiError> {
    if r.join_limited() {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many tries; wait a minute".into(),
        ));
    }
    let (token, team, member) = r
        .store
        .join(&req.invite, &req.name, &req.email)
        .map_err(|e| ApiError(StatusCode::FORBIDDEN, e))?;
    Ok(Json(
        json!({ "token": token, "team": team, "member": member }),
    ))
}

/// The team's members (name and email), for addressing messages.
async fn members(State(r): State<Arc<Relay>>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    let mut seen = std::collections::HashSet::new();
    let list: Vec<Value> = r
        .store
        .members(&m.team_id)
        .map_err(internal)?
        .into_iter()
        .filter(|(x, _)| !x.revoked && seen.insert(x.email.clone()))
        .map(|(x, _)| json!({ "name": x.name, "email": x.email }))
        .collect();
    Ok(Json(json!({ "members": list })))
}

async fn me(State(r): State<Arc<Relay>>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    let team = r.store.team(&m.team_id).map_err(internal)?;
    Ok(Json(json!({ "member": m, "team": team })))
}

#[derive(Deserialize)]
struct RepoQ {
    repo: String,
    #[serde(default)]
    since: i64,
}

async fn put_presence(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Query(q): Query<RepoQ>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    if !ok_repo(&q.repo) {
        return Err(bad("Name the repository"));
    }
    if !body.is_object() {
        return Err(bad("A presence is a JSON object"));
    }
    if r.limited(&m.id) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many updates; slow down".into(),
        ));
    }
    let seq = r.store.put_presence(&m, &q.repo, body).map_err(internal)?;
    r.bump(seq);
    Ok(Json(json!({ "ok": true })))
}

async fn delete_presence(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Query(q): Query<RepoQ>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    if !ok_repo(&q.repo) {
        return Err(bad("Name the repository"));
    }
    let seq = r.store.delete_presence(&m, &q.repo).map_err(internal)?;
    r.bump(seq);
    Ok(Json(json!({ "ok": true })))
}

async fn get_presence(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Query(q): Query<RepoQ>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    if !ok_repo(&q.repo) {
        return Err(bad("Name the repository"));
    }
    Ok(Json(
        json!({ "presence": r.store.presence(&m, &q.repo).map_err(internal)? }),
    ))
}

async fn post_message(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Query(q): Query<RepoQ>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    if !ok_repo(&q.repo) {
        return Err(bad("Name the repository"));
    }
    if r.limited(&m.id) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many messages; slow down".into(),
        ));
    }
    let (seq, msg) = r.store.post_message(&m, &q.repo, body).map_err(bad)?;
    r.bump(seq);
    Ok(Json(msg))
}

async fn get_messages(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Query(q): Query<RepoQ>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    if !ok_repo(&q.repo) {
        return Err(bad("Name the repository"));
    }
    Ok(Json(
        json!({ "messages": r.store.messages(&m, &q.repo, q.since).map_err(internal)? }),
    ))
}

#[derive(Deserialize)]
struct EventsQ {
    #[serde(default)]
    after: i64,
    #[serde(default)]
    wait: u64,
}

async fn events(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Query(q): Query<EventsQ>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(q.wait.min(MAX_WAIT_S));
    let mut rx = r.tick.subscribe();
    loop {
        let (last, list) = r.store.events_after(&m, q.after).map_err(internal)?;
        if !list.is_empty() || tokio::time::Instant::now() >= deadline {
            let cursor = if list.len() == 500 {
                list.last().map(|e| e.0).unwrap_or(last)
            } else {
                last.max(q.after)
            };
            let events: Vec<Value> = list
                .into_iter()
                .map(|(seq, repo, kind)| json!({ "seq": seq, "repo": repo, "kind": kind }))
                .collect();
            return Ok(Json(json!({ "cursor": cursor, "events": events })));
        }
        if tokio::time::timeout_at(deadline, rx.changed())
            .await
            .is_err()
        {
            continue; // timed out: answer with nothing new
        }
    }
}

pub fn router(relay: Arc<Relay>) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/join", post(join))
        .route("/v1/me", get(me))
        .route("/v1/members", get(members))
        .route(
            "/v1/presence",
            get(get_presence).put(put_presence).delete(delete_presence),
        )
        .route("/v1/messages", get(get_messages).post(post_message))
        .route("/v1/events", get(events))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .with_state(relay)
}

/// Serves until the process ends; purges old data every hour.
pub async fn serve(
    listener: tokio::net::TcpListener,
    store: Store,
    retention_days: u32,
) -> std::io::Result<()> {
    let relay = Relay::new(store);
    let r = relay.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = r.store.purge(retention_days) {
                eprintln!("agentdeck-relay: purge failed: {e}");
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
    axum::serve(listener, router(relay)).await
}
