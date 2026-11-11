//! How much each agent may do without asking, in the sessions you run yourself.
//!
//! One level per agent, kept in meta `approvals.<provider>`:
//! - "ask": every request waits for you (the agents' own default).
//! - "edits": file changes inside the project go ahead; everything else waits.
//! - "auto": ordinary edits and commands go ahead; risky ones (deleting, pushing, deploying,
//!   secrets, sudo…) and anything outside the project wait, with the rule workflows use
//!   (`unattended`).
//! - "all": every request goes ahead.
//!
//! Questions to you and plans presented for approval always wait, at every level. Sessions that
//! belong to a bot or an active workflow follow their own rules and are left alone here.
//! Antigravity has its own setting (`agy::approvals`): it can't ask in AgentDeck at all.

use crate::agents::Agents;
use crate::claude::Sink;
use crate::db::{Db, StoredEvent};
use crate::unattended::{self, Verdict};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::Arc;

pub const LEVELS: [&str; 4] = ["ask", "edits", "auto", "all"];

/// The agents this setting is for.
pub const PROVIDERS: [&str; 6] = [
    "claude-code",
    "codex",
    "gemini-cli",
    "grok-build",
    "kimi-code",
    "api",
];

fn key(provider: &str) -> String {
    format!("approvals.{provider}")
}

/// An agent's level; "ask" when it was never set.
pub fn level(db: &Db, provider: &str) -> String {
    db.meta(&key(provider))
        .ok()
        .flatten()
        .filter(|l| LEVELS.contains(&l.as_str()))
        .unwrap_or_else(|| "ask".into())
}

/// Every agent's level, by provider.
pub fn levels(db: &Db) -> Map<String, Value> {
    PROVIDERS
        .iter()
        .map(|p| (p.to_string(), Value::String(level(db, p))))
        .collect()
}

pub fn set_level(db: &Db, provider: &str, level: &str) -> Result<(), String> {
    if !PROVIDERS.contains(&provider) {
        return Err(format!("Unknown agent: {provider}"));
    }
    if !LEVELS.contains(&level) {
        return Err(format!("Unknown approval level: {level}"));
    }
    db.set_meta(&key(provider), level)
        .map_err(|e| e.to_string())
}

/// A request to change files (as opposed to running a command or another tool).
fn is_edit(ev: &StoredEvent) -> bool {
    match ev.source.as_str() {
        "claude" => matches!(
            unattended::claude_tool(ev),
            Some("Edit" | "Write" | "MultiEdit" | "NotebookEdit")
        ),
        "codex" => ev.payload["method"]
            .as_str()
            .is_some_and(|m| m.contains("fileChange") || m == "applyPatchApproval"),
        "acp" => ev.payload.pointer("/toolCall/kind").and_then(Value::as_str) == Some("edit"),
        "api" => matches!(
            ev.payload["tool"].as_str(),
            Some("write_file" | "edit_file")
        ),
        _ => false,
    }
}

/// The request id AgentDeck answers for you, if the agent's level lets this request go ahead.
pub fn decide(db: &Db, ev: &StoredEvent) -> Option<String> {
    let (request_id, ordinary) = match unattended::verdict(db, ev)? {
        Verdict::Question => return None,
        Verdict::Allow { request_id } => (request_id, true),
        Verdict::Hold { request_id, .. } => (request_id, false),
    };
    let s = db.session(&ev.session_id).ok().flatten()?;
    // Plan mode refuses changes on its own; a plan waits for you.
    if s.mode == "plan" {
        return None;
    }
    if db.bot_for_session(&s.id).ok().flatten().is_some()
        || db.active_flow_for(&s.id).ok().flatten().is_some()
    {
        return None;
    }
    let go = match level(db, &s.provider).as_str() {
        "all" => true,
        "auto" => ordinary,
        "edits" => ordinary && is_edit(ev),
        _ => false,
    };
    go.then_some(request_id)
}

/// Requests in a session nobody has answered yet, oldest first.
fn unanswered(db: &Db, sid: &str) -> Vec<StoredEvent> {
    let Ok(events) = db.events(sid) else {
        return Vec::new();
    };
    let answered: HashSet<String> = events
        .iter()
        .filter(|e| matches!(e.kind.as_str(), "permission_decision" | "question_answered"))
        .filter_map(|e| e.payload["requestId"].as_str().map(str::to_owned))
        .collect();
    events
        .into_iter()
        .filter(|e| {
            (e.kind == "control_request/can_use_tool" && e.source == "claude")
                || (e.kind == "approval" && e.source != "agentdeck")
        })
        .filter(|e| unattended::request_id(e).is_some_and(|r| !answered.contains(&r)))
        .collect()
}

pub struct Approvals {
    db: Arc<Db>,
    agents: Arc<Agents>,
    sink: Sink,
}

impl Approvals {
    pub fn new(db: Arc<Db>, agents: Arc<Agents>, sink: Sink) -> Self {
        Self { db, agents, sink }
    }

    /// Called for every approval request an agent makes.
    pub fn on_request(&self, ev: &StoredEvent) {
        if let Some(rid) = decide(&self.db, ev) {
            unattended::allow(
                &self.db,
                &self.agents,
                &self.sink,
                &ev.session_id,
                &rid,
                "approvals",
            );
        }
    }

    /// After a level changes: requests already waiting in that agent's sessions that the new
    /// level lets go ahead are answered now. Returns how many.
    pub fn apply_waiting(&self, provider: &str) -> usize {
        let Ok(sessions) = self.db.list_sessions() else {
            return 0;
        };
        let mut n = 0;
        for s in sessions
            .iter()
            .filter(|s| s.provider == provider && s.status == "waiting")
        {
            if !self.agents.is_running(&s.id) {
                continue;
            }
            for ev in unanswered(&self.db, &s.id) {
                if let Some(rid) = decide(&self.db, &ev) {
                    if self
                        .agents
                        .respond(&s.id, &rid, crate::claude::Decision::Allow)
                        .is_ok()
                    {
                        if let Ok(Some(e)) = self.db.append_event(
                            &s.id,
                            "agentdeck",
                            "permission_decision",
                            &serde_json::json!({ "requestId": rid, "decision": "allow", "auto": "approvals" }),
                        ) {
                            (self.sink)(crate::claude::Outbound::Event(e));
                        }
                        n += 1;
                    }
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(sid: &str, source: &str, kind: &str, payload: Value) -> StoredEvent {
        StoredEvent {
            session_id: sid.into(),
            seq: 1,
            ts: 1,
            source: source.into(),
            kind: kind.into(),
            payload,
        }
    }

    fn claude(sid: &str, tool: &str, input: Value) -> StoredEvent {
        ev(
            sid,
            "claude",
            "control_request/can_use_tool",
            json!({ "request_id": "r1", "request": { "tool_name": tool, "input": input } }),
        )
    }

    #[test]
    fn each_level_lets_through_what_it_says() {
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project("app", "/p/app").unwrap();
        let s = db
            .create_session(&p.id, "claude-code", "default", "x")
            .unwrap();
        let edit = claude(&s.id, "Edit", json!({ "file_path": "/p/app/src/a.ts" }));
        let test = claude(&s.id, "Bash", json!({ "command": "npm test" }));
        let push = claude(&s.id, "Bash", json!({ "command": "git push origin main" }));
        let away = claude(&s.id, "Write", json!({ "file_path": "/etc/hosts" }));
        let ask = claude(&s.id, "AskUserQuestion", json!({}));
        let plan = claude(&s.id, "ExitPlanMode", json!({}));
        let got =
            |db: &Db| [&edit, &test, &push, &away, &ask, &plan].map(|e| decide(db, e).is_some());

        assert_eq!(level(&db, "claude-code"), "ask");
        assert_eq!(got(&db), [false; 6]);
        set_level(&db, "claude-code", "edits").unwrap();
        assert_eq!(got(&db), [true, false, false, false, false, false]);
        set_level(&db, "claude-code", "auto").unwrap();
        assert_eq!(got(&db), [true, true, false, false, false, false]);
        set_level(&db, "claude-code", "all").unwrap();
        assert_eq!(got(&db), [true, true, true, true, false, false]);

        // Another agent's level doesn't apply.
        let c = db.create_session(&p.id, "codex", "default", "y").unwrap();
        let cmd = ev(
            &c.id,
            "codex",
            "approval",
            json!({ "requestId": "a1", "method": "item/commandExecution/requestApproval", "params": { "command": "cargo test" } }),
        );
        let patch = ev(
            &c.id,
            "codex",
            "approval",
            json!({ "requestId": "a2", "method": "item/fileChange/requestApproval", "params": {} }),
        );
        assert_eq!(decide(&db, &cmd), None);
        set_level(&db, "codex", "edits").unwrap();
        assert_eq!(decide(&db, &cmd), None);
        assert_eq!(decide(&db, &patch).as_deref(), Some("a2"));

        // Plan mode keeps its own rule.
        db.set_session_mode(&s.id, "plan").unwrap();
        assert_eq!(decide(&db, &edit), None);

        assert!(set_level(&db, "antigravity", "all").is_err());
        assert!(set_level(&db, "codex", "yolo").is_err());
    }

    #[test]
    fn edits_of_other_agents_are_recognized() {
        let a = ev(
            "s",
            "acp",
            "approval",
            json!({ "toolCall": { "kind": "edit" } }),
        );
        let b = ev(
            "s",
            "acp",
            "approval",
            json!({ "toolCall": { "kind": "execute" } }),
        );
        let c = ev("s", "api", "approval", json!({ "tool": "edit_file" }));
        let d = ev("s", "api", "approval", json!({ "tool": "run_command" }));
        assert_eq!([&a, &b, &c, &d].map(is_edit), [true, false, true, false]);
    }

    #[test]
    fn only_requests_nobody_answered_are_waiting() {
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project("app", "/p/app").unwrap();
        let s = db.create_session(&p.id, "codex", "default", "x").unwrap();
        for (source, kind, payload) in [
            ("codex", "approval", json!({ "requestId": "a1" })),
            (
                "agentdeck",
                "permission_decision",
                json!({ "requestId": "a1", "decision": "allow" }),
            ),
            ("codex", "approval", json!({ "requestId": "a2" })),
            ("codex", "user_input", json!({ "requestId": "q1" })),
        ] {
            db.append_event(&s.id, source, kind, &payload).unwrap();
        }
        let waiting: Vec<String> = unanswered(&db, &s.id)
            .iter()
            .filter_map(unattended::request_id)
            .collect();
        assert_eq!(waiting, ["a2"]);
    }
}
