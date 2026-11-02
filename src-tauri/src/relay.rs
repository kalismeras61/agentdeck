//! Relay: two sessions talking to each other.
//!
//! When a turn in one session of an active relay ends with a reply, that reply is sent to the
//! other session as its next message, framed so the receiving agent knows who is speaking. The
//! relay stops when an agent says DONE on a line of its own, when the round limit is reached,
//! when a turn fails or is interrupted, or when the user stops it. Permission requests are never
//! answered by the other agent: they wait for the user like any other request.

use crate::agents::Agents;
use crate::claude::{Outbound, Sink};
use crate::db::{Db, RelayRow, SessionRow, StoredEvent};
use serde_json::{json, Value};
use std::sync::Arc;

pub const MAX_ROUNDS: i64 = 20;
const MAX_FORWARD_BYTES: usize = 100_000;

pub fn label(provider: &str) -> &'static str {
    crate::agents::label(provider)
}

/// True when a line of the reply is just "DONE" (allowing markdown emphasis or a period).
pub fn says_done(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l
            .trim()
            .trim_matches(|c: char| c == '*' || c == '_' || c == '`' || c == '.' || c == '!');
        t == "DONE"
    })
}

/// How a finished turn ended, read from the stored event that closes it.
pub enum TurnEnd {
    Reply(String),
    NoReply,
    Failed(String),
}

pub fn turn_end(ev: &StoredEvent) -> Option<TurnEnd> {
    let p = &ev.payload;
    if ev.source == "claude" && ev.kind.starts_with("result") {
        let ok = ev.kind == "result/success" && !p["is_error"].as_bool().unwrap_or(false);
        let text = p["result"].as_str().unwrap_or("").trim().to_string();
        return Some(if !ok {
            TurnEnd::Failed(if text.is_empty() {
                "the turn ended with an error".into()
            } else {
                text
            })
        } else if text.is_empty() {
            TurnEnd::NoReply
        } else {
            TurnEnd::Reply(text)
        });
    }
    if (ev.source == "acp" || ev.source == "api") && ev.kind == "turn/ended" {
        let stop = p["stopReason"].as_str().unwrap_or("end_turn");
        if stop != "end_turn" {
            return Some(TurnEnd::Failed(match stop {
                "cancelled" => "the turn was stopped".into(),
                "refusal" => "the agent refused".into(),
                other => format!("the turn ended early ({other})"),
            }));
        }
        let text = p["lastText"].as_str().unwrap_or("").trim().to_string();
        return Some(if text.is_empty() {
            TurnEnd::NoReply
        } else {
            TurnEnd::Reply(text)
        });
    }
    if ev.source == "codex" && ev.kind == "turn/completed" {
        let status = p
            .pointer("/turn/status")
            .and_then(Value::as_str)
            .unwrap_or("completed");
        if status != "completed" {
            return Some(TurnEnd::Failed(if status == "interrupted" {
                "the turn was stopped".into()
            } else {
                "the turn failed".into()
            }));
        }
        let text = p
            .pointer("/turn/items")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().rev().find(|i| i["type"] == "agentMessage"))
            .and_then(|i| i["text"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(if text.is_empty() {
            TurnEnd::NoReply
        } else {
            TurnEnd::Reply(text)
        });
    }
    None
}

fn intro(me: &SessionRow, my_role: &str, peer: &SessionRow, peer_role: &str, max: i64) -> String {
    let mut s = format!(
        "You're working with {} in an AgentDeck relay. Your replies are passed to {} automatically and theirs come back to you, for up to {} rounds.",
        label(&peer.provider),
        label(&peer.provider),
        max
    );
    if !my_role.is_empty() {
        s.push_str(&format!(" Your role: you {my_role}."));
    }
    if !peer_role.is_empty() {
        s.push_str(&format!(" {} {peer_role}.", label(&peer.provider)));
    }
    let _ = me;
    s.push_str(" Talk to them directly and keep replies focused. Approvals still go to the user. When the work is finished and checked, write DONE on its own line.");
    s
}

pub struct Relay {
    db: Arc<Db>,
    sink: Sink,
    agents: Arc<Agents>,
}

impl Relay {
    pub fn new(db: Arc<Db>, sink: Sink, agents: Arc<Agents>) -> Self {
        Self { db, sink, agents }
    }

    fn record(&self, sid: &str, kind: &str, payload: &Value) {
        if let Ok(Some(ev)) = self.db.append_event(sid, "agentdeck", kind, payload) {
            (self.sink)(Outbound::Event(ev));
        }
    }

    fn publish(&self, row: &RelayRow) {
        (self.sink)(Outbound::Relay(row.clone()));
    }

    fn send(&self, session: &SessionRow, text: &str) -> Result<(), String> {
        self.agents.send(&session.id, text)
    }

    fn session(&self, id: &str) -> Result<SessionRow, String> {
        self.db
            .session(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Session not found".to_string())
    }

    /// Links `a` and `b` and sends the task to `a`.
    pub fn start(
        &self,
        a_id: &str,
        b_id: &str,
        task: &str,
        a_role: &str,
        b_role: &str,
        max_rounds: i64,
    ) -> Result<RelayRow, String> {
        let task = task.trim();
        if task.is_empty() {
            return Err("Write the task for the agents first".into());
        }
        if !(1..=MAX_ROUNDS).contains(&max_rounds) {
            return Err(format!("Choose between 1 and {MAX_ROUNDS} rounds"));
        }
        if a_id == b_id {
            return Err("A relay needs two different sessions".into());
        }
        let a = self.session(a_id)?;
        let b = self.session(b_id)?;
        if a.project_id != b.project_id {
            return Err("Both sessions must be in the same project".into());
        }
        for s in [&a, &b] {
            if self
                .db
                .active_relay_for(&s.id)
                .map_err(|e| e.to_string())?
                .is_some()
                || self
                    .db
                    .active_flow_for(&s.id)
                    .map_err(|e| e.to_string())?
                    .is_some()
            {
                return Err(format!(
                    "\"{}\" is already working with other agents. Stop that first.",
                    s.title
                ));
            }
        }
        // Name fresh sessions after the task, not after the generated relay prompt.
        let base = if a.title == "New session" {
            crate::claude::first_line(task, 48)
        } else {
            a.title.clone()
        };
        let a = if a.title == "New session" {
            self.db
                .update_session(&a.id, None, None, Some(&base), None, None)
                .ok()
                .flatten()
                .unwrap_or(a)
        } else {
            a
        };
        let b = if b.title.starts_with("New session") {
            let t = format!("{base} ({})", label(&b.provider));
            self.db
                .update_session(&b.id, None, None, Some(&t), None, None)
                .ok()
                .flatten()
                .unwrap_or(b)
        } else {
            b
        };
        for s in [&a, &b] {
            (self.sink)(Outbound::Session(s.clone()));
        }
        let clip = |s: &str| s.chars().take(200).collect::<String>();
        let row = self
            .db
            .create_relay(&a.id, &b.id, &clip(a_role), &clip(b_role), max_rounds)
            .map_err(|e| e.to_string())?;
        let prompt = format!(
            "{}\n\nTask from the user:\n{}",
            intro(&a, &row.a_role, &b, &row.b_role, max_rounds),
            task
        );
        for (me, peer) in [(&a, &b), (&b, &a)] {
            self.record(
                &me.id,
                "relay_started",
                &json!({ "relayId": row.id, "peerSessionId": peer.id, "peerProvider": peer.provider, "peerTitle": peer.title, "task": task, "maxRounds": max_rounds, "starts": me.id == a.id, "prompt": if me.id == a.id { Value::String(prompt.clone()) } else { Value::Null } }),
            );
        }
        self.publish(&row);
        if let Err(e) = self.send(&a, &prompt) {
            self.finish(&row, "stopped", &format!("Couldn't start: {e}"));
            return Err(e);
        }
        Ok(row)
    }

    pub fn stop(&self, relay_id: &str, reason: &str) -> Result<RelayRow, String> {
        let row = self
            .db
            .relay(relay_id)
            .map_err(|e| e.to_string())?
            .ok_or("Relay not found")?;
        if row.status != "active" {
            return Ok(row);
        }
        Ok(self.finish(&row, "stopped", reason))
    }

    fn finish(&self, row: &RelayRow, status: &str, reason: &str) -> RelayRow {
        let updated = self
            .db
            .update_relay(&row.id, None, Some(status), Some(reason))
            .ok()
            .flatten()
            .unwrap_or_else(|| row.clone());
        for sid in [&row.a_session, &row.b_session] {
            self.record(sid, "relay_ended", &json!({ "relayId": row.id, "status": status, "reason": reason, "rounds": updated.rounds }));
        }
        self.publish(&updated);
        updated
    }

    /// Called for every stored event; acts only on the event that ends a turn.
    pub fn on_event(&self, ev: &StoredEvent) {
        let Some(end) = turn_end(ev) else { return };
        let Ok(Some(row)) = self.db.active_relay_for(&ev.session_id) else {
            return;
        };
        let Ok(from) = self.session(&ev.session_id) else {
            return;
        };
        let to_id = if row.a_session == from.id {
            &row.b_session
        } else {
            &row.a_session
        };
        let Ok(to) = self.session(to_id) else {
            self.finish(&row, "stopped", "The other session was deleted");
            return;
        };
        let who = label(&from.provider);
        let text = match end {
            TurnEnd::Failed(why) => {
                self.finish(&row, "stopped", &format!("{who}: {why}"));
                return;
            }
            TurnEnd::NoReply => {
                self.finish(
                    &row,
                    "stopped",
                    &format!("{who} finished without a reply to pass on"),
                );
                return;
            }
            TurnEnd::Reply(t) => t,
        };
        if says_done(&text) {
            self.finish(&row, "done", &format!("{who} said DONE"));
            return;
        }
        if row.rounds >= row.max_rounds {
            self.finish(
                &row,
                "stopped",
                &format!("Reached the round limit ({})", row.max_rounds),
            );
            return;
        }
        let round = row.rounds + 1;
        let body: String = if text.len() > MAX_FORWARD_BYTES {
            let cut = text
                .char_indices()
                .take_while(|(i, _)| *i < MAX_FORWARD_BYTES)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);
            format!(
                "{}\n\n[… cut by AgentDeck: the reply was longer than 100 KB]",
                &text[..cut]
            )
        } else {
            text.clone()
        };
        // The receiving side gets the relay introduction with its first message.
        let first_for_b = to.id == row.b_session && row.rounds == 0;
        let mut prompt = String::new();
        if first_for_b {
            let task = self
                .db
                .events(&to.id)
                .ok()
                .and_then(|evs| evs.into_iter().find(|e| e.kind == "relay_started"))
                .and_then(|e| e.payload["task"].as_str().map(str::to_string))
                .unwrap_or_default();
            prompt.push_str(&intro(&to, &row.b_role, &from, &row.a_role, row.max_rounds));
            prompt.push_str(&format!("\n\nThe user's task:\n{task}\n\n"));
        }
        prompt.push_str(&format!(
            "{who} (round {round} of {}):\n\n{body}",
            row.max_rounds
        ));
        let Ok(Some(row)) = self.db.update_relay(&row.id, Some(round), None, None) else {
            return;
        };
        self.record(
            &to.id,
            "relay_message",
            &json!({ "relayId": row.id, "fromSessionId": from.id, "fromProvider": from.provider, "text": body, "round": round, "maxRounds": row.max_rounds, "prompt": prompt }),
        );
        self.publish(&row);
        if let Err(e) = self.send(&to, &prompt) {
            self.finish(
                &row,
                "stopped",
                &format!("Couldn't pass the reply to {}: {e}", label(&to.provider)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;
    use std::sync::Mutex;

    #[test]
    fn done_is_a_line_of_its_own() {
        assert!(says_done("All tests pass.\n\nDONE"));
        assert!(says_done("**DONE**"));
        assert!(says_done("DONE."));
        assert!(!says_done("I'm not DONE yet"));
        assert!(!says_done("done"));
    }

    type Seen = Arc<Mutex<Vec<String>>>;

    fn setup() -> (Arc<Db>, Relay, Seen, String, String) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let sink: Sink = Arc::new(move |o| {
            if let Outbound::Event(e) = o {
                s2.lock()
                    .unwrap()
                    .push(format!("{}:{}", e.session_id, e.kind));
            }
        });
        let mut claude = Manager::new(db.clone(), sink.clone());
        // No real CLI here: sends fail fast, which is what these tests check around.
        claude.binary_override = Some("/nonexistent/claude".into());
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
        let relay = Relay::new(db.clone(), sink, agents);
        let p = db.add_project("demo", "/tmp").unwrap();
        let a = db
            .create_session(&p.id, "claude-code", "default", "Plan it")
            .unwrap();
        let b = db
            .create_session(&p.id, "codex", "default", "Build it")
            .unwrap();
        (db, relay, seen, a.id, b.id)
    }

    fn end(sid: &str, text: &str) -> StoredEvent {
        StoredEvent {
            session_id: sid.into(),
            seq: 1,
            ts: 1,
            source: "claude".into(),
            kind: "result/success".into(),
            payload: json!({ "result": text }),
        }
    }

    #[test]
    fn start_validates_and_failed_send_stops_the_relay() {
        let (db, relay, _seen, a, b) = setup();
        assert!(relay.start(&a, &a, "task", "", "", 6).is_err());
        assert!(relay.start(&a, &b, "  ", "", "", 6).is_err());
        assert!(relay.start(&a, &b, "task", "", "", 0).is_err());
        assert!(relay.start(&a, &b, "task", "", "", 21).is_err());
        // The CLI can't start here, so the relay is created and then stopped with the reason.
        assert!(relay.start(&a, &b, "task", "plans", "builds", 6).is_err());
        let r = db.list_relays().unwrap().remove(0);
        assert_eq!(r.status, "stopped");
        assert!(r.reason.unwrap().starts_with("Couldn't start"));
    }

    #[test]
    fn replies_are_forwarded_until_done_or_the_limit() {
        let (db, relay, seen, a, b) = setup();
        let row = db.create_relay(&a, &b, "plans", "builds", 2).unwrap();
        // A's reply goes to B (the send fails without a CLI, but the message is recorded first).
        relay.on_event(&end(&a, "Here is the plan: 1. fix rounding"));
        let msgs: Vec<StoredEvent> = db
            .events(&b)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "relay_message")
            .collect();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].payload["round"], 1);
        let prompt = msgs[0].payload["prompt"].as_str().unwrap();
        assert!(
            prompt.starts_with("You're working with Claude Code"),
            "B gets the introduction first"
        );
        assert!(prompt.contains("Claude Code (round 1 of 2):\n\nHere is the plan"));
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|k| k.ends_with(":relay_ended")),
            "failed send stops the relay"
        );

        // A fresh relay: DONE ends it without forwarding.
        let p = db.session(&a).unwrap().unwrap().project_id;
        let c = db
            .create_session(&p, "claude-code", "default", "Review")
            .unwrap();
        let d = db.create_session(&p, "codex", "default", "Build").unwrap();
        let r2 = db.create_relay(&c.id, &d.id, "", "", 3).unwrap();
        relay.on_event(&end(&c.id, "Looks good.\nDONE"));
        let r2 = db.relay(&r2.id).unwrap().unwrap();
        assert_eq!(r2.status, "done");
        assert_eq!(r2.rounds, 0);
        assert!(db
            .events(&d.id)
            .unwrap()
            .iter()
            .all(|e| e.kind != "relay_message"));

        // At the limit, the next reply ends the relay instead of being forwarded.
        let e = db
            .create_session(&p, "claude-code", "default", "x")
            .unwrap();
        let f = db.create_session(&p, "codex", "default", "y").unwrap();
        let r3 = db.create_relay(&e.id, &f.id, "", "", 1).unwrap();
        db.update_relay(&r3.id, Some(1), None, None).unwrap();
        relay.on_event(&end(&e.id, "more"));
        let r3 = db.relay(&r3.id).unwrap().unwrap();
        assert_eq!(r3.status, "stopped");
        assert_eq!(r3.reason.as_deref(), Some("Reached the round limit (1)"));
        let _ = row;
    }

    #[test]
    fn a_failed_turn_stops_the_relay() {
        let (db, relay, _seen, a, b) = setup();
        let row = db.create_relay(&a, &b, "", "", 4).unwrap();
        let mut ev = end(&a, "boom");
        ev.kind = "result/error_during_execution".into();
        relay.on_event(&ev);
        let r = db.relay(&row.id).unwrap().unwrap();
        assert_eq!(r.status, "stopped");
        assert!(r.reason.unwrap().starts_with("Claude Code:"));
    }
}

/// End-to-end: real `claude` and `codex` CLIs against the local mocks
/// (`scripts/mock_anthropic.py 8787`, `scripts/mock_openai.py 8788`). Run with AGENTDECK_IT=1.
#[cfg(test)]
mod integration {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    #[test]
    #[ignore]
    fn claude_and_codex_talk_until_done() {
        if std::env::var("AGENTDECK_IT").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("project");
        std::fs::create_dir(&proj).unwrap();
        let codex_home = dir.path().join("codexhome");
        std::fs::create_dir(&codex_home).unwrap();
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let a = db
            .create_session(&p.id, "claude-code", "default", "Fix the discount")
            .unwrap();
        let b = db
            .create_session(&p.id, "codex", "default", "Fix the discount (Codex)")
            .unwrap();

        let cell: Arc<OnceLock<Arc<Relay>>> = Arc::new(OnceLock::new());
        let c2 = cell.clone();
        let (tx, rx) = mpsc::channel::<RelayRow>();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| match o {
            Outbound::Event(e) => {
                if let Some(r) = c2.get() {
                    if turn_end(&e).is_some() {
                        let r = r.clone();
                        std::thread::spawn(move || r.on_event(&e));
                    }
                }
            }
            Outbound::Relay(r) => {
                let _ = tx.lock().unwrap().send(r);
            }
            _ => {}
        });
        let mut claude = Manager::new(db.clone(), sink.clone());
        claude.extra_env = vec![
            ("ANTHROPIC_BASE_URL".into(), "http://127.0.0.1:8787".into()),
            ("ANTHROPIC_API_KEY".into(), "sk-ant-test-dummy-key".into()),
            (
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
                "1".into(),
            ),
            ("DISABLE_AUTOUPDATER".into(), "1".into()),
        ];
        let mut codex = CodexManager::new(db.clone(), sink.clone());
        codex.extra_env = vec![
            (
                "CODEX_HOME".into(),
                codex_home.to_string_lossy().into_owned(),
            ),
            ("MOCK_OPENAI_KEY".into(), "test-key".into()),
        ];
        codex.extra_args = vec![
            "-c".into(),
            "model_provider=\"agentdeck_mock\"".into(),
            "-c".into(),
            "model=\"mock-model\"".into(),
            "-c".into(),
            "model_providers.agentdeck_mock={name=\"Mock\", base_url=\"http://127.0.0.1:8788/v1\", env_key=\"MOCK_OPENAI_KEY\", wire_api=\"responses\"}".into(),
        ];
        let (claude, codex) = (Arc::new(claude), Arc::new(codex));
        let agents = Arc::new(Agents {
            db: db.clone(),
            claude: claude.clone(),
            codex: codex.clone(),
            acp: Arc::new(crate::acp::AcpManager::new(db.clone(), sink.clone())),
            agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
            api: Arc::new(crate::api::ApiManager::new(db.clone(), sink.clone())),
        });
        let relay = Arc::new(Relay::new(db.clone(), sink, agents));
        let _ = cell.set(relay.clone());

        let row = relay
            .start(
                &a.id,
                &b.id,
                "Fix the discount bug in invoice.py",
                "plan and review",
                "implement",
                6,
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut last = row;
        while last.status == "active" && Instant::now() < deadline {
            if let Ok(r) = rx.recv_timeout(Duration::from_millis(200)) {
                last = r;
            }
        }
        assert_eq!(
            last.status, "done",
            "ended by DONE, not by the limit: {:?}",
            last.reason
        );
        assert_eq!(last.rounds, 2, "Claude → Codex, Codex → Claude");
        assert_eq!(last.reason.as_deref(), Some("Claude Code said DONE"));

        let to_codex: Vec<_> = db
            .events(&b.id)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "relay_message")
            .collect();
        assert_eq!(to_codex.len(), 1);
        assert!(to_codex[0].payload["text"]
            .as_str()
            .unwrap()
            .starts_with("Plan: apply the discount"));
        let to_claude: Vec<_> = db
            .events(&a.id)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "relay_message")
            .collect();
        assert_eq!(to_claude.len(), 1);
        assert!(to_claude[0].payload["text"]
            .as_str()
            .unwrap()
            .starts_with("Implemented it"));
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_RELAY") {
            let both = json!({ "a": db.events(&a.id).unwrap(), "b": db.events(&b.id).unwrap() });
            std::fs::write(path, serde_json::to_string_pretty(&both).unwrap()).unwrap();
        }
        claude.stop(&a.id);
        codex.stop(&b.id);
    }
}
