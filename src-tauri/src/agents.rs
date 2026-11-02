//! One entry point for "do this with the session's agent", whatever protocol it speaks.

use crate::acp::AcpManager;
use crate::agy::AgyManager;
use crate::api::ApiManager;
use crate::claude::{Decision, Manager};
use crate::codex::CodexManager;
use crate::db::Db;
use std::sync::Arc;

/// Agent processes kept open at once; starting one more closes the longest-idle one.
pub const MAX_LIVE: usize = 6;
/// An agent process with nothing to do for this long is closed. Its conversation isn't lost:
/// the next message starts it again and it picks the conversation up.
pub const IDLE_CLOSE_MS: i64 = 10 * 60 * 1000;

/// Providers AgentDeck can run as live sessions.
pub const LIVE_PROVIDERS: &[&str] = &["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code", "antigravity", "api"];

pub fn label(provider: &str) -> &'static str {
    match provider {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "gemini-cli" => "Gemini CLI",
        "grok-build" => "Grok Build",
        "kimi-code" => "Kimi Code",
        "antigravity" => "Antigravity",
        "api" => "Model API",
        _ => "another agent",
    }
}

pub struct Agents {
    pub db: Arc<Db>,
    pub claude: Arc<Manager>,
    pub codex: Arc<CodexManager>,
    pub acp: Arc<AcpManager>,
    pub agy: Arc<AgyManager>,
    pub api: Arc<ApiManager>,
}

enum Kind {
    Claude,
    Codex,
    Acp,
    Agy,
    Api,
}

impl Agents {
    fn kind(&self, sid: &str) -> Result<Kind, String> {
        let s = self
            .db
            .session(sid)
            .map_err(|e| e.to_string())?
            .ok_or("Session not found")?;
        Ok(match s.provider.as_str() {
            "codex" => Kind::Codex,
            "claude-code" => Kind::Claude,
            "api" => Kind::Api,
            crate::agy::PROVIDER => Kind::Agy,
            p if crate::acp::spec(p).is_some() => Kind::Acp,
            p => return Err(format!("AgentDeck can't run {p} sessions")),
        })
    }

    /// Sessions whose agent process is open and not in the middle of a turn or a question,
    /// longest idle first.
    fn idle_running(&self) -> Vec<crate::db::SessionRow> {
        let mut v: Vec<_> = self
            .db
            .list_sessions()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.status != "running" && s.status != "waiting" && self.is_running(&s.id))
            .collect();
        v.sort_by_key(|s| s.updated_at);
        v
    }

    /// Closes idle agent processes until there's room for `sid`'s.
    fn make_room(&self, sid: &str) {
        if self.is_running(sid) {
            return;
        }
        let open = self
            .db
            .list_sessions()
            .unwrap_or_default()
            .iter()
            .filter(|s| self.is_running(&s.id))
            .count();
        if open < MAX_LIVE {
            return;
        }
        for s in self.idle_running().into_iter().take(open + 1 - MAX_LIVE) {
            self.stop(&s.id);
        }
    }

    /// Closes agent processes that have been idle longer than `IDLE_CLOSE_MS`. Returns how many.
    pub fn close_idle(&self, now_ms: i64) -> usize {
        let old: Vec<_> = self
            .idle_running()
            .into_iter()
            .filter(|s| now_ms - s.updated_at > IDLE_CLOSE_MS)
            .collect();
        for s in &old {
            self.stop(&s.id);
        }
        old.len()
    }

    pub fn send(&self, sid: &str, text: &str) -> Result<(), String> {
        self.make_room(sid);
        match self.kind(sid)? {
            Kind::Claude => self.claude.send_message(sid, text),
            Kind::Codex => self.codex.send_message(sid, text),
            Kind::Acp => self.acp.send_message(sid, text),
            Kind::Agy => self.agy.send_message(sid, text),
            Kind::Api => self.api.send_message(sid, text),
        }
    }

    /// Sends a message with files the user dropped or pasted.
    pub fn send_with(&self, sid: &str, text: &str, files: &[String]) -> Result<(), String> {
        if files.is_empty() {
            return self.send(sid, text);
        }
        let s = self
            .db
            .session(sid)
            .map_err(|e| e.to_string())?
            .ok_or("Session not found")?;
        let dir = self
            .db
            .session_dir(&s)
            .map_err(|e| e.to_string())?
            .ok_or("Project not found")?;
        let att = crate::attach::resolve(files, &dir)?;
        self.make_room(sid);
        match self.kind(sid)? {
            Kind::Claude => self.claude.send_message_with(sid, text, &att),
            Kind::Codex => self.codex.send_message_with(sid, text, &att),
            Kind::Acp => self.acp.send_message_with(sid, text, &att),
            Kind::Agy => self.agy.send_message_with(sid, text, &att),
            Kind::Api => self.api.send_message_with(sid, text, &att),
        }
    }

    pub fn respond(&self, sid: &str, request_id: &str, d: Decision) -> Result<(), String> {
        match self.kind(sid)? {
            Kind::Claude => self.claude.respond_permission(sid, request_id, d),
            Kind::Codex => self.codex.respond_permission(sid, request_id, d),
            Kind::Acp => self.acp.respond_permission(sid, request_id, d),
            Kind::Agy => self.agy.respond_permission(sid, request_id, d),
            Kind::Api => self.api.respond_permission(sid, request_id, d),
        }
    }

    /// Answers an agent's question (Claude Code's AskUserQuestion, Codex's request_user_input).
    pub fn answer(
        &self,
        sid: &str,
        request_id: &str,
        answers: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Result<(), String> {
        match self.kind(sid)? {
            Kind::Claude => self.claude.answer_question(sid, request_id, answers),
            Kind::Codex => self.codex.answer_question(sid, request_id, answers),
            _ => Err("This agent doesn't ask questions in AgentDeck".into()),
        }
    }

    pub fn interrupt(&self, sid: &str) -> Result<(), String> {
        match self.kind(sid)? {
            Kind::Claude => self.claude.interrupt(sid),
            Kind::Codex => self.codex.interrupt(sid),
            Kind::Acp => self.acp.interrupt(sid),
            Kind::Agy => self.agy.interrupt(sid),
            Kind::Api => self.api.interrupt(sid),
        }
    }

    /// Applies a model change to a running agent; it takes effect from the next message.
    pub fn set_model(&self, sid: &str, model: &str) -> Result<(), String> {
        match self.kind(sid)? {
            Kind::Claude => self.claude.set_model(sid, model),
            Kind::Codex | Kind::Api => Ok(()), // read on every turn
            Kind::Acp => {
                self.acp.set_model(sid);
                Ok(())
            }
            Kind::Agy => {
                self.agy.restart_for_settings(sid);
                Ok(())
            }
        }
    }

    /// Applies a mode change to a running process; stopped sessions pick it up on start.
    pub fn set_mode(&self, sid: &str, mode: &str) -> Result<(), String> {
        match self.kind(sid)? {
            Kind::Claude => self.claude.set_mode(sid, mode),
            Kind::Codex | Kind::Api => Ok(()), // read on every turn
            Kind::Acp => {
                self.acp.set_mode(sid);
                Ok(())
            }
            Kind::Agy => {
                self.agy.restart_for_settings(sid);
                Ok(())
            }
        }
    }

    pub fn is_running(&self, sid: &str) -> bool {
        self.claude.is_running(sid)
            || self.codex.is_running(sid)
            || self.acp.is_running(sid)
            || self.agy.is_running(sid)
            || self.api.is_running(sid)
    }

    pub fn stop(&self, sid: &str) {
        self.claude.stop(sid);
        self.codex.stop(sid);
        self.acp.stop(sid);
        self.agy.stop(sid);
        self.api.stop(sid);
    }

    pub fn shutdown_all(&self) {
        self.claude.shutdown_all();
        self.codex.shutdown_all();
        self.acp.shutdown_all();
        self.agy.shutdown_all();
        self.api.shutdown_all();
    }
}

/// End-to-end with the real `claude` CLI against `scripts/mock_anthropic.py 8787` (AGENTDECK_IT=1).
#[cfg(test)]
mod integration {
    use super::*;
    use crate::claude::{Outbound, Sink};
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore]
    fn idle_agents_close_and_the_oldest_makes_room() {
        if std::env::var("AGENTDECK_IT").is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let proj = dir.path().join("p");
        std::fs::create_dir(&proj).unwrap();
        let p = db.add_project("p", &proj.to_string_lossy()).unwrap();
        let (tx, rx) = mpsc::channel::<String>();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| {
            if let Outbound::Event(e) = o {
                if e.kind.starts_with("result") {
                    let _ = tx.lock().unwrap().send(e.session_id);
                }
            }
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
        let agents = Agents {
            db: db.clone(),
            claude: Arc::new(claude),
            codex: Arc::new(CodexManager::new(db.clone(), sink.clone())),
            acp: Arc::new(AcpManager::new(db.clone(), sink.clone())),
            agy: Arc::new(crate::agy::AgyManager::new(db.clone(), sink.clone())),
            api: Arc::new(ApiManager::new(db.clone(), sink.clone())),
        };
        let wait_idle = |n: usize| {
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut seen = 0;
            while seen < n && Instant::now() < deadline {
                if rx.recv_timeout(Duration::from_millis(200)).is_ok() {
                    seen += 1;
                }
            }
            std::thread::sleep(Duration::from_millis(300));
        };
        let ids: Vec<String> = (0..MAX_LIVE + 1)
            .map(|i| {
                db.create_session(&p.id, "claude-code", "default", &format!("s{i}"))
                    .unwrap()
                    .id
            })
            .collect();
        for id in &ids[..MAX_LIVE] {
            agents.send(id, "hello").unwrap();
        }
        wait_idle(MAX_LIVE);
        assert!(ids[..MAX_LIVE].iter().all(|id| agents.is_running(id)));
        // The 7th closes the longest-idle one (the first that finished).
        agents.send(&ids[MAX_LIVE], "hello").unwrap();
        wait_idle(1);
        let open = ids.iter().filter(|id| agents.is_running(id)).count();
        assert_eq!(open, MAX_LIVE, "never more than {MAX_LIVE} open");
        // Idle for longer than the limit: all closed; a new message starts the agent again.
        assert_eq!(
            agents.close_idle(crate::db::now_ms() + IDLE_CLOSE_MS + 1),
            MAX_LIVE
        );
        std::thread::sleep(Duration::from_millis(500));
        assert!(ids.iter().all(|id| !agents.is_running(id)));
        agents.send(&ids[0], "hello again").unwrap();
        wait_idle(1);
        assert_eq!(db.session(&ids[0]).unwrap().unwrap().status, "idle");
        agents.shutdown_all();
    }
}
