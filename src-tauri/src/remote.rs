//! Remote Control: Claude Code's own way to continue a conversation from the Claude app or
//! claude.ai. It needs Claude Code's interactive mode, which AgentDeck's sessions don't run in,
//! so AgentDeck hands the conversation over: it ends its own Claude Code process for the
//! session, resumes the same conversation in a terminal with `--remote-control`, and doesn't
//! start Claude Code for that session again until the terminal ends. Then the messages written
//! meanwhile (from the phone, the web or the terminal) are added to the conversation here.

use crate::claude::{self, Outbound, Sink};
use crate::db::Db;
use crate::pty::{TermInfo, Terminals};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Active {
    session_id: String,
    transcript: PathBuf,
    /// Messages the conversation had when it was handed over.
    seen: usize,
}

pub struct Remote {
    db: Arc<Db>,
    claude: Arc<claude::Manager>,
    sink: Sink,
    /// Claude Code's `projects` folder, where it keeps each folder's conversations.
    projects: PathBuf,
    /// Terminal id → the session it runs.
    active: Mutex<HashMap<String, Active>>,
}

impl Remote {
    pub fn new(db: Arc<Db>, claude: Arc<claude::Manager>, sink: Sink, projects: PathBuf) -> Self {
        Self {
            db,
            claude,
            sink,
            projects,
            active: Mutex::new(HashMap::new()),
        }
    }

    /// The terminal a session is in Remote Control in, if it is.
    pub fn term_for(&self, session_id: &str) -> Option<String> {
        self.active.lock().ok().and_then(|a| {
            a.iter()
                .find(|(_, x)| x.session_id == session_id)
                .map(|(t, _)| t.clone())
        })
    }

    pub fn start(
        &self,
        terms: &Terminals,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<TermInfo, String> {
        if let Some(t) = self.term_for(session_id) {
            return terms.info(&t);
        }
        let s = self
            .db
            .session(session_id)
            .map_err(|e| e.to_string())?
            .ok_or("Session not found")?;
        if s.provider != "claude-code" {
            return Err("Remote Control is Claude Code's own feature, so it works in Claude Code sessions only.".into());
        }
        if s.profile_id.is_some() {
            return Err("Remote Control needs Claude Code signed in with a Claude plan (Pro, Max, Team or Enterprise). This session uses a custom model profile, so it can't.".into());
        }
        if self.db.bot_for_session(session_id).ok().flatten().is_some() {
            return Err(
                "A bot's sessions stay with the bot. Use Remote Control in a session of your own."
                    .into(),
            );
        }
        if matches!(s.status.as_str(), "running" | "waiting") {
            return Err(
                "Claude is still working. Let it finish (or stop it), then turn on Remote Control."
                    .into(),
            );
        }
        let args = claude::remote_control_args(&s)?;
        let bin = self.claude.binary()?;
        let dir = self
            .db
            .session_dir(&s)
            .map_err(|e| e.to_string())?
            .ok_or("Project not found")?;
        if !std::path::Path::new(&dir).is_dir() {
            return Err(format!("The folder {dir} doesn't exist anymore"));
        }
        let id = s.provider_session_id.clone().unwrap_or_default();
        let transcript = self
            .projects
            .join(crate::import::claude_dir_name(&dir))
            .join(format!("{id}.jsonl"));

        // From here AgentDeck doesn't start Claude Code for this session; its own process ends.
        self.claude.hold(session_id);
        self.claude.stop(session_id);
        let deadline = Instant::now() + Duration::from_secs(8);
        while self.claude.is_running(session_id) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        if self.claude.is_running(session_id) {
            self.claude.release(session_id);
            return Err("Claude Code didn't stop in time. Try again in a moment.".into());
        }
        let seen = crate::import::parse_claude(&transcript).messages.len();
        let info = match terms.open_program(&dir, cols, rows, &bin, &args) {
            Ok(i) => i,
            Err(e) => {
                self.claude.release(session_id);
                return Err(e);
            }
        };
        if let Ok(mut a) = self.active.lock() {
            a.insert(
                info.term_id.clone(),
                Active {
                    session_id: session_id.to_string(),
                    transcript,
                    seen,
                },
            );
        }
        self.record(session_id, "remote_control", json!({ "state": "on" }));
        Ok(info)
    }

    /// Ends Remote Control for a session (Claude Code in its terminal is stopped).
    pub fn stop(&self, terms: &Terminals, session_id: &str) {
        if let Some(t) = self.term_for(session_id) {
            terms.close(&t);
            // Closing ends the program; its exit (and `on_exit`) follows from the terminal.
        }
    }

    /// A terminal ended: if it was a session's Remote Control, the session comes back here with
    /// what was written meanwhile.
    pub fn on_exit(&self, term_id: &str) {
        let Some(a) = self.active.lock().ok().and_then(|mut m| m.remove(term_id)) else {
            return;
        };
        let parsed = crate::import::parse_claude(&a.transcript);
        let new: Vec<_> = parsed.messages.iter().skip(a.seen).collect();
        for m in &new {
            self.record(
                &a.session_id,
                "imported_message",
                json!({ "role": if m.user { "user" } else { "assistant" }, "text": m.text }),
            );
        }
        self.record(
            &a.session_id,
            "remote_control",
            json!({ "state": "off", "messages": new.len() }),
        );
        if let Some(last) = new.iter().rev().find(|m| !m.user).or(new.last()) {
            let snippet = claude::first_line(&last.text, 80);
            if let Ok(Some(row)) = self.db.update_session(
                &a.session_id,
                Some("idle"),
                Some(&snippet),
                None,
                None,
                None,
            ) {
                (self.sink)(Outbound::Session(row));
            }
        }
        self.claude.release(&a.session_id);
    }

    fn record(&self, session_id: &str, kind: &str, payload: serde_json::Value) {
        match self
            .db
            .append_event(session_id, "agentdeck", kind, &payload)
        {
            Ok(Some(ev)) => (self.sink)(Outbound::Event(ev)),
            Ok(None) => {}
            Err(e) => eprintln!("agentdeck: failed to store event: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct T {
        db: Arc<Db>,
        claude: Arc<claude::Manager>,
        remote: Remote,
        terms: Terminals,
        rx: mpsc::Receiver<crate::pty::TermEvent>,
        dir: tempfile::TempDir,
        project: String,
    }

    /// A stand-in `claude` that notes its arguments and adds a phone message and Claude's
    /// reply to the conversation file, as Remote Control would.
    fn setup() -> T {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("shop");
        std::fs::create_dir_all(&proj).unwrap();
        let proj = proj.canonicalize().unwrap();
        let projects = dir.path().join("projects");
        let convo = projects.join(crate::import::claude_dir_name(&proj.to_string_lossy()));
        std::fs::create_dir_all(&convo).unwrap();
        let line = |kind: &str, text: &str| {
            json!({ "type": kind, "timestamp": "2026-11-03T10:00:00Z", "message": { "role": kind, "content": [{ "type": "text", "text": text }] } })
                .to_string()
        };
        std::fs::write(
            convo.join("abc.jsonl"),
            format!(
                "{}\n{}\n",
                line("user", "Fix the cart"),
                line("assistant", "Fixed.")
            ),
        )
        .unwrap();
        let fake = dir.path().join("claude");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\nprintf '%s\\n%s\\n' '{u}' '{a}' >> '{file}'\n",
                args = dir.path().join("args").display(),
                u = line("user", "From my phone: add a test"),
                a = line("assistant", "Added the test."),
                file = convo.join("abc.jsonl").display(),
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let db = Arc::new(Db::open_in_memory().unwrap());
        let project = db.add_project("shop", &proj.to_string_lossy()).unwrap().id;
        let sink: Sink = Arc::new(|_| {});
        let mut c = claude::Manager::new(db.clone(), sink.clone());
        c.binary_override = Some(fake);
        let claude = Arc::new(c);
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let terms = Terminals::new(Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }));
        T {
            remote: Remote::new(db.clone(), claude.clone(), sink, projects),
            db,
            claude,
            terms,
            rx,
            dir,
            project,
        }
    }

    fn session(t: &T, provider: &str, agent_id: Option<&str>) -> String {
        let s =
            t.db.create_session(&t.project, provider, "default", "Cart fixes")
                .unwrap();
        if let Some(id) = agent_id {
            t.db.update_session(&s.id, Some("idle"), None, None, None, Some(id))
                .unwrap();
        }
        s.id
    }

    fn kinds(t: &T, sid: &str) -> Vec<(String, serde_json::Value)> {
        t.db.events(sid)
            .unwrap()
            .into_iter()
            .map(|e| (e.kind, e.payload))
            .collect()
    }

    #[test]
    fn the_conversation_goes_to_remote_control_and_comes_back_with_what_was_said_there() {
        let t = setup();
        let sid = session(&t, "claude-code", Some("abc"));
        let info = t.remote.start(&t.terms, &sid, 100, 30).unwrap();
        assert_eq!(
            t.remote.term_for(&sid).as_deref(),
            Some(info.term_id.as_str())
        );
        assert!(t.claude.is_held(&sid));
        assert_eq!(
            t.claude.send_message(&sid, "hello").unwrap_err(),
            claude::HELD,
            "AgentDeck doesn't start a second Claude Code on the same conversation"
        );

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut ended = None;
        while ended.is_none() && Instant::now() < deadline {
            if let Ok(crate::pty::TermEvent::Exit(x)) =
                t.rx.recv_timeout(Duration::from_millis(100))
            {
                ended = Some(x.term_id);
            }
        }
        assert_eq!(ended.as_deref(), Some(info.term_id.as_str()));
        t.remote.on_exit(&info.term_id);

        let args = std::fs::read_to_string(t.dir.path().join("args")).unwrap();
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            ["--resume", "abc", "--remote-control=Cart fixes"]
        );
        let ev = kinds(&t, &sid);
        let names: Vec<&str> = ev.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "remote_control",
                "imported_message",
                "imported_message",
                "remote_control"
            ]
        );
        assert_eq!(ev[1].1["text"], "From my phone: add a test");
        assert_eq!(ev[1].1["role"], "user");
        assert_eq!(ev[2].1["role"], "assistant");
        assert_eq!(ev[3].1, json!({ "state": "off", "messages": 2 }));
        assert!(!t.claude.is_held(&sid), "the session is AgentDeck's again");
        assert_eq!(t.remote.term_for(&sid), None);
        let row = t.db.session(&sid).unwrap().unwrap();
        assert_eq!(row.snippet, "Added the test.");
        t.remote.on_exit(&info.term_id); // a second exit changes nothing
        assert_eq!(kinds(&t, &sid).len(), 4);
    }

    #[test]
    fn only_a_claude_code_conversation_on_a_claude_plan_that_is_not_busy() {
        let t = setup();
        let codex = session(&t, "codex", Some("abc"));
        assert!(t
            .remote
            .start(&t.terms, &codex, 80, 24)
            .unwrap_err()
            .contains("Claude Code sessions only"));
        let fresh = session(&t, "claude-code", None);
        assert!(t
            .remote
            .start(&t.terms, &fresh, 80, 24)
            .unwrap_err()
            .contains("first message"));
        let busy = session(&t, "claude-code", Some("abc"));
        t.db.update_session(&busy, Some("running"), None, None, None, None)
            .unwrap();
        assert!(t
            .remote
            .start(&t.terms, &busy, 80, 24)
            .unwrap_err()
            .contains("still working"));
        for s in [&codex, &fresh, &busy] {
            assert!(!t.claude.is_held(s));
            assert!(kinds(&t, s).is_empty());
        }
    }
}
