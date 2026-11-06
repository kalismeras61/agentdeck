//! Connections: what a bot can reach besides its files. Each is an MCP server: one that runs on
//! your Mac (a command such as `npx -y @playwright/mcp@latest`), one on the web (a URL such as
//! GitHub's), or AgentDeck's own "Mac" tools (Calendar, Reminders, Mail, Notes). You add them in
//! Settings → Connections with your own accounts, and choose for each bot which it gets.
//!
//! Secrets (tokens, keys) stay in the keychain; SQLite keeps only their names. A bot session's
//! agent gets its connections at start: Claude Code in its `--mcp-config` file (0600, the
//! secrets in the file and not in the environment its shell sees), Codex through `-c` with the
//! secrets in variables named `…_TOKEN` (which Codex keeps from its shell), ACP agents in
//! `session/new`.
//!
//! While a bot works on a task, what a connection's tools read is allowed for it; what they
//! change waits for you (in AgentDeck or on Telegram), unless you let that connection act on
//! its own.

use crate::db::{now_ms, Db, StoredEvent};
use crate::unattended::Verdict;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const LIST: &str = "connections";
const RESERVED: &[&str] = &["agentdeck", "mac"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    /// Empty for a secret (it's in the keychain).
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub secret: bool,
    /// A secret's value is saved.
    #[serde(default)]
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// The MCP server's name the agent sees (`mcp__<server>__<tool>`).
    pub server: String,
    /// "browser", "github", "notion", "mac", or "stdio" / "http" for your own.
    pub kind: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
    /// A command's environment.
    #[serde(default)]
    pub env: Vec<Field>,
    /// A URL's headers.
    #[serde(default)]
    pub headers: Vec<Field>,
    /// Its tools may change things without asking you first.
    #[serde(default)]
    pub act_alone: bool,
    #[serde(default)]
    pub created_at: i64,
}

impl Connection {
    pub fn transport(&self) -> &'static str {
        if self.kind == "mac" {
            "mac"
        } else if !self.url.is_empty() {
            "http"
        } else {
            "stdio"
        }
    }
}

/// A connection as an agent starts it, secrets filled in.
#[derive(Debug, Clone, PartialEq)]
pub enum Server {
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        /// Names in `env` whose values are secrets.
        secrets: Vec<String>,
        act_alone: bool,
    },
    Http {
        name: String,
        url: String,
        headers: Vec<(String, String)>,
        act_alone: bool,
    },
}

impl Server {
    pub fn name(&self) -> &str {
        match self {
            Server::Stdio { name, .. } | Server::Http { name, .. } => name,
        }
    }
    pub fn act_alone(&self) -> bool {
        match self {
            Server::Stdio { act_alone, .. } | Server::Http { act_alone, .. } => *act_alone,
        }
    }
}

fn secret_name(id: &str, key: &str) -> String {
    format!("connection:{id}:{key}")
}

fn slug(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "connection".into()
    } else {
        s.chars().take(32).collect()
    }
}

pub fn list(db: &Db) -> Vec<Connection> {
    db.meta(LIST)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store(db: &Db, all: &[Connection]) -> Result<(), String> {
    db.set_meta(
        LIST,
        &serde_json::to_string(all).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn bots_key(bot_id: &str) -> String {
    format!("bot.connections.{bot_id}")
}

/// The connections a bot has (their ids).
pub fn for_bot(db: &Db, bot_id: &str) -> Vec<String> {
    let all: HashSet<String> = list(db).into_iter().map(|c| c.id).collect();
    db.meta(&bots_key(bot_id))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|id| all.contains(id))
        .collect()
}

pub fn set_for_bot(db: &Db, bot_id: &str, ids: &[String]) -> Result<Vec<String>, String> {
    let all: HashSet<String> = list(db).into_iter().map(|c| c.id).collect();
    let mut keep: Vec<String> = Vec::new();
    for id in ids {
        if !all.contains(id) {
            return Err("That connection was removed".into());
        }
        if !keep.contains(id) {
            keep.push(id.clone());
        }
    }
    db.set_meta(
        &bots_key(bot_id),
        &serde_json::to_string(&keep).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(keep)
}

/// Adds or updates a connection (`input.id` empty: a new one). A secret field with an empty
/// value keeps what's saved.
pub fn save(db: &Db, mut input: Connection) -> Result<Connection, String> {
    let mut all = list(db);
    input.name = input.name.trim().to_string();
    if input.name.is_empty() {
        return Err("Give it a name".into());
    }
    input.command = input.command.trim().to_string();
    input.url = input.url.trim().to_string();
    match input.kind.as_str() {
        "mac" => {
            if all.iter().any(|c| c.kind == "mac" && c.id != input.id) {
                return Err("The Mac connection is already there".into());
            }
            input.command.clear();
            input.args.clear();
            input.url.clear();
            input.env.clear();
            input.headers.clear();
        }
        _ if !input.url.is_empty() => {
            let ok = reqwest::Url::parse(&input.url).is_ok_and(|u| match u.scheme() {
                "https" => u.host_str().is_some(),
                "http" => matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
                _ => false,
            });
            if !ok {
                return Err("The address must start with https:// (or be on this Mac)".into());
            }
            input.command.clear();
            input.args.clear();
            input.env.clear();
        }
        _ => {
            if input.command.is_empty() {
                return Err("Write the command that starts it, or its address".into());
            }
            input.headers.clear();
        }
    }
    let fresh = input.id.is_empty();
    if fresh {
        input.id = uuid::Uuid::new_v4().to_string();
        input.created_at = now_ms();
    }
    let old = all.iter().find(|c| c.id == input.id).cloned();
    if !fresh && old.is_none() {
        return Err("That connection was removed".into());
    }
    // The server's name: from the name, unique, never one of AgentDeck's own.
    input.server = if input.kind == "mac" {
        "mac".into()
    } else {
        let base = slug(&input.name);
        let taken: HashSet<String> = all
            .iter()
            .filter(|c| c.id != input.id)
            .map(|c| c.server.clone())
            .collect();
        let mut name = base.clone();
        let mut n = 2;
        while taken.contains(&name) || RESERVED.contains(&name.as_str()) {
            name = format!("{base}-{n}");
            n += 1;
        }
        name
    };
    for fields in [&mut input.env, &mut input.headers] {
        fields.retain(|f| !f.key.trim().is_empty());
        let mut seen = HashSet::new();
        for f in fields.iter_mut() {
            f.key = f.key.trim().to_string();
            if !seen.insert(f.key.clone()) {
                return Err(format!("{} is there twice", f.key));
            }
            if f.secret {
                let name = secret_name(&input.id, &f.key);
                if !f.value.is_empty() {
                    crate::secrets::set_named(&name, &f.value)?;
                }
                f.value.clear();
                f.saved = crate::secrets::get_named(&name).is_some();
            } else {
                f.saved = false;
            }
        }
    }
    // Secrets of fields that are gone (or no longer secret) leave the keychain.
    if let Some(o) = &old {
        for f in o.env.iter().chain(&o.headers).filter(|f| f.secret) {
            let still = input
                .env
                .iter()
                .chain(&input.headers)
                .any(|n| n.secret && n.key == f.key);
            if !still {
                crate::secrets::remove_named(&secret_name(&o.id, &f.key));
            }
        }
    }
    match all.iter_mut().find(|c| c.id == input.id) {
        Some(c) => {
            input.created_at = c.created_at;
            *c = input.clone();
        }
        None => all.push(input.clone()),
    }
    store(db, &all)?;
    Ok(input)
}

pub fn remove(db: &Db, id: &str) -> Result<(), String> {
    let mut all = list(db);
    if let Some(c) = all.iter().find(|c| c.id == id) {
        for f in c.env.iter().chain(&c.headers).filter(|f| f.secret) {
            crate::secrets::remove_named(&secret_name(&c.id, &f.key));
        }
    }
    all.retain(|c| c.id != id);
    store(db, &all)
}

/// Fields with their secrets filled in from the keychain (a missing secret is left out).
pub fn filled(c: &Connection, fields: &[Field]) -> Vec<(String, String)> {
    fields
        .iter()
        .filter_map(|f| {
            let v = if f.secret {
                crate::secrets::get_named(&secret_name(&c.id, &f.key))?
            } else {
                f.value.clone()
            };
            Some((f.key.clone(), v))
        })
        .collect()
}

/// A bot's connections as its agent starts them (the Mac one is the bridge, so it's not here).
pub fn servers(db: &Db, bot_id: &str) -> (Vec<Server>, Option<bool>) {
    let mine = for_bot(db, bot_id);
    let mut out = Vec::new();
    let mut mac = None;
    for c in list(db).into_iter().filter(|c| mine.contains(&c.id)) {
        let fill = |fields: &[Field]| filled(&c, fields);
        match c.transport() {
            "mac" => mac = Some(c.act_alone),
            "http" => out.push(Server::Http {
                name: c.server.clone(),
                url: c.url.clone(),
                headers: fill(&c.headers),
                act_alone: c.act_alone,
            }),
            _ => out.push(Server::Stdio {
                name: c.server.clone(),
                command: c.command.clone(),
                args: c.args.clone(),
                env: fill(&c.env),
                secrets: c
                    .env
                    .iter()
                    .filter(|f| f.secret)
                    .map(|f| f.key.clone())
                    .collect(),
                act_alone: c.act_alone,
            }),
        }
    }
    (out, mac)
}

/// The connection server and tool a stored approval request is for, if it's one.
pub fn requested(ev: &StoredEvent) -> Option<(String, String)> {
    if ev.source == "claude" && ev.kind == "control_request/can_use_tool" {
        let name = ev.payload.pointer("/request/tool_name")?.as_str()?;
        let rest = name.strip_prefix("mcp__")?;
        let (server, tool) = rest.split_once("__")?;
        return Some((server.to_string(), tool.to_string()));
    }
    if ev.kind != "approval" || ev.source == "agentdeck" {
        return None;
    }
    let p = &ev.payload;
    if let Some(server) = p
        .pointer("/params/server")
        .or_else(|| p.pointer("/params/item/server"))
        .and_then(Value::as_str)
    {
        let tool = p
            .pointer("/params/tool")
            .or_else(|| p.pointer("/params/item/tool"))
            .and_then(Value::as_str)
            .unwrap_or("");
        return Some((server.to_string(), tool.to_string()));
    }
    // ACP agents (Gemini CLI): "<tool> (<server> MCP Server)". The title is free text (a shell
    // command's title is the command), so only a plain name counts, and never for a request
    // that runs, edits, deletes or moves something.
    let call = p.get("toolCall")?;
    if matches!(
        call.get("kind").and_then(Value::as_str),
        Some("execute" | "edit" | "delete" | "move")
    ) {
        return None;
    }
    let title = call.get("title").and_then(Value::as_str)?;
    let inner = title.strip_suffix(" MCP Server)")?;
    let (tool, server) = inner.rsplit_once(" (")?;
    let plain = |x: &str| {
        !x.is_empty()
            && x.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
    };
    (plain(tool) && plain(server)).then(|| (server.to_string(), tool.to_string()))
}

/// Tools that only look: by the first word of their name (`get_issue`, `list_events`,
/// `browser_snapshot`, `search`…).
pub fn reads(tool: &str) -> bool {
    const LOOK: &[&str] = &[
        "get",
        "list",
        "search",
        "read",
        "find",
        "view",
        "describe",
        "show",
        "lookup",
        "count",
        "retrieve",
        "inspect",
        "snapshot",
        "screenshot",
        "console",
        "network",
        "take",
        "explain",
    ];
    let t = tool.to_lowercase();
    let mut words = t.split(['_', '-', '.', ' ']).filter(|w| !w.is_empty());
    let first = words.next().unwrap_or("");
    let word = if matches!(first, "browser" | "notion" | "github" | "api" | "mcp") {
        words.next().unwrap_or("")
    } else {
        first
    };
    LOOK.contains(&word) || crate::mac::READ.contains(&t.as_str())
}

/// For a request from a bot's session while it works on a task: allowed or held when it's one
/// of the bot's connections (None: not a connection's, the usual rules decide).
pub fn verdict(db: &Db, ev: &StoredEvent, bot_id: &str) -> Option<Verdict> {
    let (server, tool) = requested(ev)?;
    if server == crate::mcp::SERVER {
        return None;
    }
    let mine = for_bot(db, bot_id);
    let conn = list(db)
        .into_iter()
        .find(|c| c.server == server && mine.contains(&c.id))?;
    let request_id = crate::unattended::request_id(ev)?;
    if conn.act_alone || reads(&tool) {
        return Some(Verdict::Allow { request_id });
    }
    let what = if conn.kind == "mac" {
        format!("it {}", crate::mac::action(&tool))
    } else {
        format!("it changes something in {} ({tool})", conn.name)
    };
    Some(Verdict::Hold {
        request_id,
        why: what,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(name: &str, kind: &str) -> Connection {
        Connection {
            id: String::new(),
            name: name.into(),
            server: String::new(),
            kind: kind.into(),
            command: String::new(),
            args: vec![],
            url: String::new(),
            env: vec![],
            headers: vec![],
            act_alone: false,
            created_at: 0,
        }
    }

    #[test]
    fn secrets_stay_in_the_keychain_and_reach_the_agent_filled_in() {
        let db = Db::open_in_memory().unwrap();
        let mut gh = input("GitHub", "github");
        gh.url = "https://api.githubcopilot.com/mcp/".into();
        gh.headers = vec![Field {
            key: "Authorization".into(),
            value: "Bearer ghp_secret".into(),
            secret: true,
            saved: false,
        }];
        let gh = save(&db, gh).unwrap();
        assert_eq!(gh.server, "github");
        assert_eq!(gh.headers[0].value, "");
        assert!(gh.headers[0].saved);
        assert!(
            !db.meta(LIST).unwrap().unwrap().contains("ghp_secret"),
            "never in SQLite"
        );
        let mut notion = input("Notion", "notion");
        notion.command = "npx".into();
        notion.args = vec!["-y".into(), "@notionhq/notion-mcp-server".into()];
        notion.env = vec![Field {
            key: "NOTION_TOKEN".into(),
            value: "ntn_x".into(),
            secret: true,
            saved: false,
        }];
        let notion = save(&db, notion).unwrap();
        let mac = save(&db, input("Mac", "mac")).unwrap();
        assert!(save(&db, input("Mac again", "mac")).is_err());
        set_for_bot(&db, "b1", &[gh.id.clone(), mac.id.clone()]).unwrap();
        let (servers, m) = servers(&db, "b1");
        assert_eq!(m, Some(false));
        assert_eq!(
            servers,
            vec![Server::Http {
                name: "github".into(),
                url: "https://api.githubcopilot.com/mcp/".into(),
                headers: vec![("Authorization".into(), "Bearer ghp_secret".into())],
                act_alone: false
            }]
        );
        // Editing without retyping keeps the secret; removing takes it out of the keychain.
        let mut again = notion.clone();
        again.name = "Notion work".into();
        let again = save(&db, again).unwrap();
        assert!(again.env[0].saved);
        assert_eq!(again.server, "notion-work");
        remove(&db, &again.id).unwrap();
        assert_eq!(
            crate::secrets::get_named(&secret_name(&again.id, "NOTION_TOKEN")),
            None
        );
        assert!(set_for_bot(&db, "b1", std::slice::from_ref(&again.id)).is_err());
    }

    #[test]
    fn names_and_addresses_are_checked() {
        let db = Db::open_in_memory().unwrap();
        assert!(save(&db, input(" ", "stdio")).is_err());
        assert!(save(&db, input("Tool", "stdio"))
            .unwrap_err()
            .contains("command"));
        for bad in [
            "http://example.com/mcp",
            "http://localhost.evil.com/mcp",
            "http://127.0.0.1.nip.io/mcp",
            "ftp://x",
        ] {
            let mut h = input("Plain http", "http");
            h.url = bad.into();
            assert!(save(&db, h).unwrap_err().contains("https://"), "{bad}");
        }
        let mut local = input("Local", "http");
        local.url = "http://localhost:3000/mcp".into();
        assert!(save(&db, local).is_ok());
        let mut a = input("agentdeck", "stdio");
        a.command = "x".into();
        assert_eq!(
            save(&db, a).unwrap().server,
            "agentdeck-2",
            "AgentDeck's own names stay its own"
        );
    }

    #[test]
    fn reading_is_allowed_changing_waits_unless_it_may_act_alone() {
        let db = Db::open_in_memory().unwrap();
        let mut gh = input("GitHub", "github");
        gh.url = "https://api.githubcopilot.com/mcp/".into();
        let gh = save(&db, gh).unwrap();
        let mut br = input("Browser", "browser");
        br.command = "npx".into();
        br.act_alone = true;
        let br = save(&db, br).unwrap();
        let mac = save(&db, input("Mac", "mac")).unwrap();
        set_for_bot(&db, "b1", &[gh.id.clone(), br.id.clone(), mac.id.clone()]).unwrap();
        let ask = |tool: &str| StoredEvent {
            session_id: "s".into(),
            seq: 1,
            ts: 1,
            source: "claude".into(),
            kind: "control_request/can_use_tool".into(),
            payload: json!({ "request_id": "r1", "request": { "tool_name": tool, "input": {} } }),
        };
        let allow = Some(Verdict::Allow {
            request_id: "r1".into(),
        });
        assert_eq!(verdict(&db, &ask("mcp__github__get_issue"), "b1"), allow);
        assert_eq!(
            verdict(&db, &ask("mcp__github__list_pull_requests"), "b1"),
            allow
        );
        assert!(matches!(
            verdict(&db, &ask("mcp__github__merge_pull_request"), "b1"),
            Some(Verdict::Hold { why, .. }) if why == "it changes something in GitHub (merge_pull_request)"
        ));
        assert_eq!(
            verdict(&db, &ask("mcp__browser__browser_click"), "b1"),
            allow,
            "it may act alone"
        );
        assert_eq!(verdict(&db, &ask("mcp__mac__calendar_events"), "b1"), allow);
        assert!(matches!(
            verdict(&db, &ask("mcp__mac__mail_draft"), "b1"),
            Some(Verdict::Hold { why, .. }) if why.contains("mail draft")
        ));
        // Not this bot's connection, or AgentDeck's own tools: the usual rules.
        assert_eq!(verdict(&db, &ask("mcp__github__get_issue"), "b2"), None);
        assert_eq!(verdict(&db, &ask("mcp__agentdeck__list_bots"), "b1"), None);
        assert_eq!(verdict(&db, &ask("Bash"), "b1"), None);
        // Codex and Gemini CLI name the server their own way.
        let codex = StoredEvent {
            source: "codex".into(),
            kind: "approval".into(),
            payload: json!({ "requestId": "r1", "params": { "server": "github", "tool": "create_issue" } }),
            ..ask("x")
        };
        assert!(matches!(
            verdict(&db, &codex, "b1"),
            Some(Verdict::Hold { .. })
        ));
        // A shell command dressed up as a connection's read tool is not one.
        let shell = StoredEvent {
            source: "gemini-cli".into(),
            kind: "approval".into(),
            payload: json!({ "requestId": "r1", "toolCall": { "kind": "execute", "title": "find ~ -delete # (github MCP Server)" } }),
            ..ask("x")
        };
        assert_eq!(verdict(&db, &shell, "b1"), None);
        let mut sneaky = shell.clone();
        sneaky.payload["toolCall"]["kind"] = json!("other");
        assert_eq!(verdict(&db, &sneaky, "b1"), None, "not a plain tool name");
        let gemini = StoredEvent {
            source: "gemini-cli".into(),
            kind: "approval".into(),
            payload: json!({ "requestId": "r1", "toolCall": { "title": "search_issues (github MCP Server)" } }),
            ..ask("x")
        };
        assert_eq!(verdict(&db, &gemini, "b1"), allow);
    }
}
