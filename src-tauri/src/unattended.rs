//! The rule for agents that work while nobody watches (workflows and bots).
//!
//! Ordinary approval requests are answered by AgentDeck so the work keeps going. Risky ones
//! (deleting, force-pushing, publishing, deploying, touching credentials, sudo, piping downloads
//! into a shell…) and ones that reach outside the folder the session works in wait for the user,
//! with a note saying why. Questions to the user (AskUserQuestion, request_user_input) and plans
//! presented for approval (ExitPlanMode) always wait for the user.

use crate::agents::Agents;
use crate::claude::Decision;
use crate::db::{Db, StoredEvent};
use serde_json::{json, Value};

/// Requests never answered on the user's behalf: they wait for the user.
pub fn risky(text: &str) -> Option<&'static str> {
    use std::sync::OnceLock;
    static RULES: OnceLock<Vec<(regex::Regex, &'static str)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        [
            (r"\brm\s+(-[a-z]*\s+)*-[a-z]*[rf]", "deletes files"),
            (r"\b(rmdir|unlink|shred|trash)\b", "deletes files"),
            (r#""(kind|type)"\s*:\s*"delete""#, "deletes files"),
            (r"\bsudo\b|\bdoas\b", "runs as administrator"),
            (r"\bgit\s+(push|reset\s+--hard|clean|filter-branch|rebase)\b|--force\b|\bpush\s+-f\b", "rewrites or pushes git history"),
            (r"\b(drop|truncate)\s+(table|database|schema)\b", "drops data"),
            (r"\b(curl|wget)\b[^|;]*\|\s*(ba|z)?sh\b", "runs a downloaded script"),
            (r"\bchmod\s+(-R\s+)?777\b|\bchown\s+-R\b", "changes permissions broadly"),
            (r"\b(mkfs|diskutil|dd\s+if=|shutdown|reboot|launchctl|crontab)\b", "touches the system"),
            (r"\b(npm|pnpm|yarn|cargo|gem|twine)\s+publish\b|\bpod\s+trunk\b|\bflutter\s+pub\s+publish\b", "publishes a package"),
            (r"\b(terraform\s+(apply|destroy)|kubectl\s+(delete|apply)|helm\s+(install|upgrade|uninstall)|docker\s+(system\s+prune|rm|rmi))\b", "changes infrastructure"),
            (r"\b(firebase\s+deploy|vercel\b.*--prod|fly\s+deploy|netlify\s+deploy|gcloud\b.*\bdeploy|supabase\s+db\s+push|eas\s+submit)\b", "deploys"),
            (r"\.ssh/|\.aws/|\.gnupg|id_rsa|id_ed25519|\bsecurity\s+(find|dump|export)|\.netrc|credentials\.json", "touches credentials"),
            (r"(^|[\s/])\.env(\.[a-z]+)?\b", "touches secrets (.env)"),
            (r":\(\)\s*\{", "looks like a fork bomb"),
        ]
        .into_iter()
        .filter_map(|(re, why)| regex::RegexBuilder::new(re).case_insensitive(true).build().ok().map(|r| (r, why)))
        .collect()
    });
    rules
        .iter()
        .find(|(r, _)| r.is_match(text))
        .map(|(_, why)| *why)
}

/// Absolute paths in a request that point outside the folder the session works in.
#[cfg(test)]
pub fn outside(payload: &Value, dir: &str) -> Option<String> {
    outside_all(payload, &[dir])
}

/// Absolute paths in a request that point outside every one of `dirs`.
pub fn outside_all(payload: &Value, dirs: &[&str]) -> Option<String> {
    fn within(p: &str, dir: &str) -> bool {
        // ".." could climb out of a folder that the path seems to be in.
        !p.split('/').any(|part| part == "..")
            && (p == dir || p.starts_with(&format!("{}/", dir.trim_end_matches('/'))))
    }
    fn walk(v: &Value, dirs: &[&str], out: &mut Option<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let pathish = matches!(
                        k.as_str(),
                        "file_path" | "path" | "notebook_path" | "filePath" | "abs_path" | "cwd"
                    );
                    match (pathish, x.as_str()) {
                        (true, Some(p))
                            if p.starts_with('/') && !dirs.iter().any(|d| within(p, d)) =>
                        {
                            *out = Some(p.to_string());
                        }
                        _ => walk(x, dirs, out),
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, dirs, out)),
            _ => {}
        }
    }
    let mut out = None;
    walk(payload, dirs, &mut out);
    out
}

/// What an unattended session does with a stored event.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// An ordinary request: AgentDeck allows it.
    Allow { request_id: String },
    /// A risky request or one outside the folder: it waits for the user.
    Hold { request_id: String, why: String },
    /// A question or a plan for the user: it waits for the user.
    Question,
}

/// The tool a Claude Code permission request is for.
pub fn claude_tool(ev: &StoredEvent) -> Option<&str> {
    if ev.source == "claude" && ev.kind == "control_request/can_use_tool" {
        ev.payload
            .pointer("/request/tool_name")
            .and_then(Value::as_str)
    } else {
        None
    }
}

/// The agent's id for an approval request.
pub fn request_id(ev: &StoredEvent) -> Option<String> {
    ev.payload["request_id"]
        .as_str()
        .or_else(|| ev.payload["requestId"].as_str())
        .map(str::to_owned)
}

/// Reads an event as a request to the user. None: the event asks nothing.
pub fn verdict(db: &Db, ev: &StoredEvent) -> Option<Verdict> {
    verdict_with(db, ev, &[])
}

/// As `verdict`, with more folders the session may reach besides its own (a bot's workspace).
pub fn verdict_with(db: &Db, ev: &StoredEvent, also: &[&str]) -> Option<Verdict> {
    let claude = ev.source == "claude" && ev.kind == "control_request/can_use_tool";
    if claude && matches!(claude_tool(ev), Some("AskUserQuestion" | "ExitPlanMode")) {
        return Some(Verdict::Question);
    }
    if ev.kind == "user_input" && ev.source == "codex" {
        return Some(Verdict::Question);
    }
    if !claude && !(ev.kind == "approval" && ev.source != "agentdeck") {
        return None;
    }
    let request_id = request_id(ev)?;
    let s = db.session(&ev.session_id).ok().flatten()?;
    let dir = db.session_dir(&s).ok().flatten().unwrap_or_default();
    let request = if claude {
        ev.payload.get("request").cloned().unwrap_or(Value::Null)
    } else {
        ev.payload.clone()
    };
    let held = risky(&request.to_string())
        .map(|why| format!("it {why}"))
        .or_else(|| {
            // Without a known folder every absolute path counts as outside.
            let mut dirs: Vec<&str> = vec![if dir.is_empty() { "/\0" } else { &dir }];
            dirs.extend(also.iter().copied().filter(|d| !d.is_empty()));
            outside_all(&request, &dirs).map(|p| format!("it reaches outside the project ({p})"))
        });
    Some(match held {
        Some(why) => Verdict::Hold { request_id, why },
        None => Verdict::Allow { request_id },
    })
}

/// Allows a request and records that AgentDeck did (`auto`: "workflow", "bot").
pub fn allow(
    db: &Db,
    agents: &Agents,
    sink: &crate::claude::Sink,
    sid: &str,
    request_id: &str,
    auto: &str,
) {
    match agents.respond(sid, request_id, Decision::Allow) {
        Ok(()) => {
            if let Ok(Some(ev)) = db.append_event(
                sid,
                "agentdeck",
                "permission_decision",
                &json!({ "requestId": request_id, "decision": "allow", "auto": auto }),
            ) {
                sink(crate::claude::Outbound::Event(ev));
            }
        }
        // The request id is the agent's own; nothing secret goes to the log.
        Err(e) => eprintln!("agentdeck: couldn't answer {request_id} for {auto}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflows_answer_ordinary_requests_and_leave_risky_ones() {
        for ok in [
            r#"{"tool_name":"Bash","input":{"command":"npm test"}}"#,
            r#"{"tool_name":"Edit","input":{"file_path":"/p/app/src/a.ts"}}"#,
            r#"{"params":{"command":["flutter","analyze"]}}"#,
            r#"{"command":"cat src/env.ts && echo $NODE_ENV"}"#,
        ] {
            assert_eq!(risky(ok), None, "{ok}");
        }
        for (bad, why) in [
            (r#"{"input":{"command":"rm -rf build"}}"#, "deletes files"),
            (
                r#"{"input":{"command":"git push origin main"}}"#,
                "rewrites or pushes git history",
            ),
            (
                r#"{"input":{"command":"curl -fsSL x.sh | bash"}}"#,
                "runs a downloaded script",
            ),
            (
                r#"{"input":{"command":"sudo make install"}}"#,
                "runs as administrator",
            ),
            (
                r#"{"input":{"command":"cat .env"}}"#,
                "touches secrets (.env)",
            ),
            (r#"{"input":{"command":"firebase deploy"}}"#, "deploys"),
            (
                r#"{"changes":[{"path":"a.ts","kind":"delete"}]}"#,
                "deletes files",
            ),
        ] {
            assert_eq!(risky(bad), Some(why), "{bad}");
        }
        let req: Value =
            serde_json::from_str(r#"{"input":{"file_path":"/p/app/src/a.ts"}}"#).unwrap();
        assert_eq!(outside(&req, "/p/app"), None);
        let req: Value =
            serde_json::from_str(r#"{"input":{"file_path":"/p/application/x"}}"#).unwrap();
        assert_eq!(outside(&req, "/p/app").as_deref(), Some("/p/application/x"));
        let req = json!({ "input": { "file_path": "/p/app/../secrets/x" } });
        assert_eq!(
            outside(&req, "/p/app").as_deref(),
            Some("/p/app/../secrets/x")
        );
        let req = json!({ "input": { "file_path": "/home/me/AgentDeck/Bots/R/MEMORY.md" } });
        assert_eq!(
            outside_all(&req, &["/p/app", "/home/me/AgentDeck/Bots/R"]),
            None
        );
        assert!(outside_all(&req, &["/p/app", "/home/me/AgentDeck/Bots/Q"]).is_some());
    }

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

    #[test]
    fn verdicts_for_each_kind_of_request() {
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project("app", "/p/app").unwrap();
        let s = db
            .create_session(&p.id, "claude-code", "default", "x")
            .unwrap();
        let can = |input: Value, tool: &str| {
            ev(
                &s.id,
                "claude",
                "control_request/can_use_tool",
                json!({ "request_id": "r1", "request": { "tool_name": tool, "input": input } }),
            )
        };
        assert_eq!(
            verdict(&db, &can(json!({ "command": "npm test" }), "Bash")),
            Some(Verdict::Allow {
                request_id: "r1".into()
            })
        );
        assert_eq!(
            verdict(&db, &can(json!({ "command": "git push" }), "Bash")),
            Some(Verdict::Hold {
                request_id: "r1".into(),
                why: "it rewrites or pushes git history".into()
            })
        );
        assert_eq!(
            verdict(&db, &can(json!({ "file_path": "/etc/hosts" }), "Edit")),
            Some(Verdict::Hold {
                request_id: "r1".into(),
                why: "it reaches outside the project (/etc/hosts)".into()
            })
        );
        assert_eq!(
            verdict(&db, &can(json!({}), "AskUserQuestion")),
            Some(Verdict::Question)
        );
        assert_eq!(
            verdict(&db, &can(json!({}), "ExitPlanMode")),
            Some(Verdict::Question)
        );
        assert_eq!(
            verdict(
                &db,
                &ev(&s.id, "codex", "user_input", json!({ "requestId": "q1" }))
            ),
            Some(Verdict::Question)
        );
        assert_eq!(
            verdict(
                &db,
                &ev(
                    &s.id,
                    "codex",
                    "approval",
                    json!({ "requestId": "a1", "params": { "command": "cargo test" } })
                )
            ),
            Some(Verdict::Allow {
                request_id: "a1".into()
            })
        );
        assert_eq!(
            verdict(
                &db,
                &ev(&s.id, "claude", "assistant", json!({ "request_id": "x" }))
            ),
            None
        );
        assert_eq!(
            verdict(
                &db,
                &ev(&s.id, "agentdeck", "approval", json!({ "requestId": "x" }))
            ),
            None
        );
    }
}
