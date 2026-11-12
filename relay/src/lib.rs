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

pub mod billing;

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

/// A team's subscription: (subscription id, item id, portal, payment page, email).
pub type SubscriptionRef = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

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
    /// Manages the team's plan (the team's first member, or one the operator made admin).
    pub admin: bool,
}

const MEMBER_COLS: &str = "id, team_id, name, email, created_at, revoked, admin";

fn member_row(r: &rusqlite::Row) -> rusqlite::Result<Member> {
    Ok(Member {
        id: r.get(0)?,
        team_id: r.get(1)?,
        name: r.get(2)?,
        email: r.get(3)?,
        created_at: r.get(4)?,
        revoked: r.get::<_, i64>(5)? != 0,
        admin: r.get::<_, i64>(6)? != 0,
    })
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
        // Billing (later versions): a team's plan, a member's admin flag.
        for (table, col, ty) in [
            ("teams", "status", "TEXT NOT NULL DEFAULT ''"),
            ("teams", "seats", "INTEGER"),
            ("teams", "trial_ends_at", "INTEGER"),
            ("teams", "renews_at", "INTEGER"),
            ("teams", "ends_at", "INTEGER"),
            ("teams", "subscription_id", "TEXT"),
            ("teams", "item_id", "TEXT"),
            ("teams", "portal_url", "TEXT"),
            ("teams", "payment_url", "TEXT"),
            ("teams", "billing_email", "TEXT"),
            ("members", "admin", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let has = conn
                .prepare(&format!(
                    "SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"
                ))?
                .exists(params![col])?;
            if !has {
                conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {ty}"), [])?;
            }
        }
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
    /// The team an invite lets you into, if it's good.
    pub fn team_for_invite(&self, invite: &str) -> rusqlite::Result<Option<Team>> {
        let id: Option<String> = self
            .c()
            .query_row(
                "SELECT team_id FROM invites WHERE code_hash = ?1 AND revoked = 0 AND (expires_at IS NULL OR expires_at > ?2)",
                params![hash(invite.trim()), now_ms()],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => self.team(&id),
            None => Ok(None),
        }
    }

    /// Joins with an invite: a new member and their token (shown once). The team's first member
    /// is its admin.
    pub fn join(
        &self,
        invite: &str,
        name: &str,
        email: &str,
    ) -> Result<(String, Team, Member), String> {
        let team = self
            .team_for_invite(invite)
            .map_err(|e| e.to_string())?
            .ok_or("That invite isn't valid (wrong, expired or revoked)")?;
        self.add_member(&team, name, email)
    }

    pub fn add_member(
        &self,
        team: &Team,
        name: &str,
        email: &str,
    ) -> Result<(String, Team, Member), String> {
        let name = clip(name.trim(), 80);
        let email = clip(&email.trim().to_lowercase(), 120);
        if name.is_empty() || !email.contains('@') {
            return Err("A name and an email are needed".into());
        }
        let first = self.active_members(&team.id).map_err(|e| e.to_string())? == 0;
        let token = secret("adr");
        let m = Member {
            id: format!("m_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
            team_id: team.id.clone(),
            name,
            email,
            created_at: now_ms(),
            revoked: false,
            admin: first,
        };
        self.c()
            .execute(
                "INSERT INTO members(id, team_id, name, email, token_hash, created_at, admin) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![m.id, m.team_id, m.name, m.email, hash(&token), m.created_at, m.admin as i64],
            )
            .map_err(|e| e.to_string())?;
        Ok((token, team.clone(), m))
    }

    /// People with a seat: distinct emails of members who aren't revoked.
    pub fn active_members(&self, team_id: &str) -> rusqlite::Result<i64> {
        self.c().query_row(
            "SELECT COUNT(DISTINCT email) FROM members WHERE team_id = ?1 AND revoked = 0",
            params![team_id],
            |r| r.get(0),
        )
    }

    pub fn is_member_email(&self, team_id: &str, email: &str) -> rusqlite::Result<bool> {
        Ok(self
            .c()
            .query_row(
                "SELECT 1 FROM members WHERE team_id = ?1 AND email = ?2 AND revoked = 0",
                params![team_id, email.trim().to_lowercase()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn set_admin(&self, member_id: &str, admin: bool) -> rusqlite::Result<bool> {
        Ok(self.c().execute(
            "UPDATE members SET admin = ?2 WHERE id = ?1",
            params![member_id, admin as i64],
        )? > 0)
    }

    pub fn plan(&self, team_id: &str) -> rusqlite::Result<billing::Plan> {
        self.c().query_row(
            "SELECT status, seats, trial_ends_at, renews_at, ends_at FROM teams WHERE id = ?1",
            params![team_id],
            |r| {
                Ok(billing::Plan {
                    status: r.get(0)?,
                    seats: r.get(1)?,
                    trial_ends_at: r.get(2)?,
                    renews_at: r.get(3)?,
                    ends_at: r.get(4)?,
                })
            },
        )
    }

    pub fn set_plan(&self, team_id: &str, p: &billing::Plan) -> rusqlite::Result<bool> {
        Ok(self.c().execute(
            "UPDATE teams SET status = ?2, seats = ?3, trial_ends_at = ?4, renews_at = ?5, ends_at = ?6 WHERE id = ?1",
            params![team_id, p.status, p.seats, p.trial_ends_at, p.renews_at, p.ends_at],
        )? > 0)
    }

    /// Billing addresses and ids: (subscription, item, portal, payment page, email).
    pub fn subscription(&self, team_id: &str) -> rusqlite::Result<SubscriptionRef> {
        self.c().query_row(
            "SELECT subscription_id, item_id, portal_url, payment_url, billing_email FROM teams WHERE id = ?1",
            params![team_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
    }

    /// Applies a subscription webhook: to the team in its custom data, else the one with that
    /// subscription. Returns the team it applied to.
    pub fn apply_subscription(
        &self,
        u: &billing::SubscriptionUpdate,
    ) -> rusqlite::Result<Option<String>> {
        let by_sub: Option<String> = self
            .c()
            .query_row(
                "SELECT id FROM teams WHERE subscription_id = ?1",
                params![u.subscription_id],
                |r| r.get(0),
            )
            .optional()?;
        let team = match (&u.team_id, by_sub) {
            (_, Some(t)) => t,
            (Some(t), None) if self.team(t)?.is_some() => t.clone(),
            _ => return Ok(None),
        };
        self.c().execute(
            "UPDATE teams SET status = ?2, seats = COALESCE(?3, seats), trial_ends_at = ?4, renews_at = ?5, ends_at = ?6,
               subscription_id = ?7, item_id = COALESCE(?8, item_id), portal_url = COALESCE(?9, portal_url),
               payment_url = COALESCE(?10, payment_url), billing_email = COALESCE(?11, billing_email) WHERE id = ?1",
            params![team, u.plan.status, u.plan.seats, u.plan.trial_ends_at, u.plan.renews_at, u.plan.ends_at, u.subscription_id, u.item_id, u.portal_url, u.payment_url, u.email],
        )?;
        Ok(Some(team))
    }

    pub fn member_by_token(&self, token: &str) -> rusqlite::Result<Option<Member>> {
        let c = self.c();
        let m = c
            .query_row(
                &format!("SELECT {MEMBER_COLS} FROM members WHERE token_hash = ?1 AND revoked = 0"),
                params![hash(token)],
                member_row,
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
        let mut st = c.prepare(&format!(
            "SELECT {MEMBER_COLS}, last_seen FROM members WHERE team_id = ?1 ORDER BY created_at"
        ))?;
        let rows = st.query_map(params![team_id], |r| Ok((member_row(r)?, r.get(7)?)))?;
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
    /// Lemon Squeezy, when this relay charges for teams.
    pub billing: Option<billing::Billing>,
    /// Anyone may create a team (`POST /v1/teams`), as on a hosted relay.
    pub open_signup: bool,
}

impl Relay {
    pub fn new(store: Store) -> Arc<Self> {
        Self::with(store, None, false)
    }

    pub fn with(store: Store, billing: Option<billing::Billing>, open_signup: bool) -> Arc<Self> {
        Arc::new(Self {
            store,
            tick: watch::channel(0).0,
            writes: Mutex::new(HashMap::new()),
            joins: Mutex::new((Instant::now(), 0)),
            billing,
            open_signup,
        })
    }

    /// Makes a team; with billing on it starts on the free trial.
    pub fn create_team(&self, name: &str) -> rusqlite::Result<Team> {
        let t = self.store.create_team(name)?;
        if let Some(b) = &self.billing {
            self.store.set_plan(
                &t.id,
                &billing::Plan {
                    status: "trialing".into(),
                    seats: Some(b.trial_seats),
                    trial_ends_at: Some(now_ms() + b.trial_days as i64 * 86_400_000),
                    ..Default::default()
                },
            )?;
        }
        Ok(t)
    }

    pub fn access(&self, team_id: &str) -> Result<billing::Access, ApiError> {
        let plan = self.store.plan(team_id).map_err(internal)?;
        Ok(billing::access(self.billing.is_some(), &plan, now_ms()))
    }

    fn may_write(&self, m: &Member) -> Result<(), ApiError> {
        let a = self.access(&m.team_id)?;
        if a.can_write {
            Ok(())
        } else {
            Err(ApiError(
                StatusCode::PAYMENT_REQUIRED,
                a.note
                    .unwrap_or_else(|| "This team's plan doesn't allow that".into()),
            ))
        }
    }

    /// A new person may join: the plan is good and a seat is free (someone already in, on
    /// another Mac, doesn't take a second seat).
    fn may_join(&self, team: &Team, email: &str) -> Result<(), ApiError> {
        let a = self.access(&team.id)?;
        if !a.can_write {
            return Err(ApiError(
                StatusCode::PAYMENT_REQUIRED,
                a.note.unwrap_or_default(),
            ));
        }
        if let Some(seats) = a.seats {
            let already = self
                .store
                .is_member_email(&team.id, email)
                .map_err(internal)?;
            if !already && self.store.active_members(&team.id).map_err(internal)? >= seats {
                return Err(ApiError(StatusCode::PAYMENT_REQUIRED, format!("All {seats} seats of {} are taken. A team admin can add seats in AgentDeck.", team.name)));
            }
        }
        Ok(())
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
    let team = r
        .store
        .team_for_invite(&req.invite)
        .map_err(internal)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::FORBIDDEN,
                "That invite isn't valid (wrong, expired or revoked)".into(),
            )
        })?;
    r.may_join(&team, &req.email)?;
    let (token, team, member) = r
        .store
        .add_member(&team, &req.name, &req.email)
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
    r.may_write(&m)?;
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
    r.may_write(&m)?;
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

#[derive(Deserialize)]
struct NewTeamReq {
    team: String,
    name: String,
    email: String,
}

/// Creates a team and makes you its admin (only on a relay that allows signups).
async fn create_team(
    State(r): State<Arc<Relay>>,
    Json(req): Json<NewTeamReq>,
) -> Result<Json<Value>, ApiError> {
    if !r.open_signup {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "This relay doesn't take new teams; ask its operator for an invite".into(),
        ));
    }
    if r.join_limited() {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many tries; wait a minute".into(),
        ));
    }
    let name = clip(req.team.trim(), 80);
    if name.is_empty() {
        return Err(bad("Name the team"));
    }
    let team = r.create_team(&name).map_err(internal)?;
    let (token, team, member) = r
        .store
        .add_member(&team, &req.name, &req.email)
        .map_err(bad)?;
    let invite = r
        .store
        .create_invite(&team.id, None, "created with the team")
        .map_err(bad)?;
    Ok(Json(
        json!({ "token": token, "team": team, "member": member, "invite": invite }),
    ))
}

/// The team's plan: status, seats, dates, and (for admins) where to manage it.
async fn get_billing(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    let plan = r.store.plan(&m.team_id).map_err(internal)?;
    let access = billing::access(r.billing.is_some(), &plan, now_ms());
    let used = r.store.active_members(&m.team_id).map_err(internal)?;
    let (sub, _, portal, payment, _) = r.store.subscription(&m.team_id).map_err(internal)?;
    Ok(Json(json!({
        "enabled": r.billing.is_some(),
        "plan": plan,
        "access": access,
        "used": used,
        "admin": m.admin,
        "subscribed": sub.is_some(),
        "portalUrl": if m.admin { portal } else { None },
        "paymentUrl": if m.admin { payment } else { None },
    })))
}

#[derive(Deserialize)]
struct SeatsReq {
    seats: i64,
}

fn admin_billing<'a>(r: &'a Relay, m: &Member) -> Result<&'a billing::Billing, ApiError> {
    let b = r
        .billing
        .as_ref()
        .ok_or_else(|| bad("This relay doesn't charge for teams"))?;
    if !m.admin {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Only a team admin can change the plan".into(),
        ));
    }
    Ok(b)
}

fn check_seats(r: &Relay, m: &Member, seats: i64) -> Result<(), ApiError> {
    let used = r.store.active_members(&m.team_id).map_err(internal)?;
    if seats < used.max(1) || seats > 1000 {
        return Err(bad(format!(
            "Pick between {} and 1000 seats (the team has {used} people)",
            used.max(1)
        )));
    }
    Ok(())
}

/// A Lemon Squeezy checkout for the team (admins). The app opens it in the browser.
async fn checkout(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Json(req): Json<SeatsReq>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    let b = admin_billing(&r, &m)?;
    check_seats(&r, &m, req.seats)?;
    let url = b
        .checkout(&m.team_id, &m.email, req.seats)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(json!({ "url": url })))
}

/// Changes the subscription's seats (admins); never below the people in the team.
async fn set_seats(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    Json(req): Json<SeatsReq>,
) -> Result<Json<Value>, ApiError> {
    let m = auth(&r, &headers)?;
    let b = admin_billing(&r, &m)?;
    check_seats(&r, &m, req.seats)?;
    let (_, item, ..) = r.store.subscription(&m.team_id).map_err(internal)?;
    let item = item.ok_or_else(|| bad("Subscribe first"))?;
    b.set_seats(&item, req.seats)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    let mut plan = r.store.plan(&m.team_id).map_err(internal)?;
    plan.seats = Some(req.seats);
    r.store.set_plan(&m.team_id, &plan).map_err(internal)?;
    Ok(Json(json!({ "ok": true, "seats": req.seats })))
}

/// Lemon Squeezy's webhook: subscription changes, signed with the webhook secret.
async fn lemonsqueezy(
    State(r): State<Arc<Relay>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let b = r
        .billing
        .as_ref()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Billing is off".into()))?;
    let sig = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !billing::verify(&b.webhook_secret, &body, sig) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "Bad signature".into()));
    }
    let v: Value = serde_json::from_slice(&body).map_err(|_| bad("Not JSON"))?;
    let Some(u) = billing::parse_webhook(&v) else {
        return Ok(Json(json!({ "ignored": true })));
    };
    match r.store.apply_subscription(&u).map_err(internal)? {
        Some(team) => {
            eprintln!(
                "agentdeck-relay: {} for team {team}: {} ({:?} seats)",
                u.event, u.plan.status, u.plan.seats
            );
            Ok(Json(json!({ "ok": true, "team": team })))
        }
        // Answer 200 anyway: retrying wouldn't help, and Lemon Squeezy would keep at it.
        None => {
            eprintln!(
                "agentdeck-relay: {} for subscription {} matches no team",
                u.event, u.subscription_id
            );
            Ok(Json(json!({ "ignored": true })))
        }
    }
}

pub fn router(relay: Arc<Relay>) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/join", post(join))
        .route("/v1/me", get(me))
        .route("/v1/members", get(members))
        .route("/v1/teams", post(create_team))
        .route("/v1/billing", get(get_billing))
        .route("/v1/billing/checkout", post(checkout))
        .route("/v1/billing/seats", post(set_seats))
        .route("/v1/billing/lemonsqueezy", post(lemonsqueezy))
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
    billing: Option<billing::Billing>,
    open_signup: bool,
) -> std::io::Result<()> {
    let relay = Relay::with(store, billing, open_signup);
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
