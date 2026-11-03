//! Workflows: plan → build → review, each stage run by whichever agent the user picked.
//!
//! Every stage has its own session. The planner and the reviewer run in plan mode, so they can
//! read the project but AgentDeck refuses any change. When a stage's turn ends, its output goes
//! to the next stage: the plan to the builder, the builder's report to the reviewer, and the
//! reviewer's notes back to the builder until the reviewer writes APPROVED or the review limit is
//! reached. With `gate_plan`, the plan waits for the user before anything is built.
//!
//! A workflow runs on its own: while it's active, the agents' approval requests are answered
//! by AgentDeck, except risky ones (deleting, force-pushing, publishing, deploying, touching
//! credentials or files outside the project, sudo, piping downloads into a shell…), which
//! wait for the user as usual.

use crate::agents::Agents;
use crate::claude::{Outbound, Sink};
use crate::db::{Db, FlowRow, FlowUpdate, SessionRow, StoredEvent};
use crate::relay::{turn_end, TurnEnd};
use crate::unattended::Verdict;
use serde_json::{json, Value};
use std::sync::Arc;

pub const MAX_REVIEWS: i64 = 10;
const MAX_FORWARD_BYTES: usize = 100_000;

/// True when a line of the text is just one of `words` (allowing markdown emphasis or a period).
pub fn says(text: &str, words: &[&str]) -> bool {
    text.lines().any(|l| {
        let t = l.trim().trim_matches(|c: char| {
            c == '*' || c == '_' || c == '`' || c == '.' || c == '!' || c == '#' || c == ' '
        });
        words.contains(&t)
    })
}

fn clip(text: &str) -> String {
    if text.len() <= MAX_FORWARD_BYTES {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .take_while(|(i, _)| *i < MAX_FORWARD_BYTES)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!(
        "{}\n\n[… cut by AgentDeck: this was longer than 100 KB]",
        &text[..cut]
    )
}

/// What the user picked for a stage.
pub enum Pick {
    /// The session the workflow was started from.
    This,
    /// A new session for this provider, on a model profile if given.
    New {
        provider: String,
        profile_id: Option<String>,
    },
}

/// Reads a stage choice: "" (skip), "this", a provider id, or "profile:<id>".
pub fn parse_pick(db: &Db, value: &str) -> Result<Option<Pick>, String> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    if v == "this" {
        return Ok(Some(Pick::This));
    }
    if let Some(pid) = v.strip_prefix("profile:") {
        let p = db
            .profile(pid)
            .map_err(|e| e.to_string())?
            .ok_or("A model in this workflow was deleted. Pick another one.")?;
        let provider = if p.kind == "anthropic" {
            "claude-code"
        } else {
            "api"
        };
        return Ok(Some(Pick::New {
            provider: provider.into(),
            profile_id: Some(p.id),
        }));
    }
    if v == "api" || !crate::agents::LIVE_PROVIDERS.contains(&v) {
        return Err(format!("AgentDeck can't run {v} in a workflow"));
    }
    Ok(Some(Pick::New {
        provider: v.into(),
        profile_id: None,
    }))
}

pub struct Flow {
    db: Arc<Db>,
    sink: Sink,
    agents: Arc<Agents>,
}

impl Flow {
    pub fn new(db: Arc<Db>, sink: Sink, agents: Arc<Agents>) -> Self {
        Self { db, sink, agents }
    }

    fn record(&self, sid: &str, kind: &str, payload: &Value) {
        if let Ok(Some(ev)) = self.db.append_event(sid, "agentdeck", kind, payload) {
            (self.sink)(Outbound::Event(ev));
        }
    }

    fn publish(&self, row: &FlowRow) {
        (self.sink)(Outbound::Flow(row.clone()));
    }

    fn session(&self, id: &str) -> Result<SessionRow, String> {
        self.db
            .session(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "A session in this workflow was deleted".to_string())
    }

    /// The agent's name as the user knows it: the model profile's name, or the provider's.
    pub fn who(&self, s: &SessionRow) -> String {
        s.profile_id
            .as_deref()
            .and_then(|id| self.db.profile(id).ok().flatten())
            .map(|p| p.name)
            .unwrap_or_else(|| crate::agents::label(&s.provider).to_string())
    }

    fn sessions(&self, row: &FlowRow) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if let Some(p) = &row.plan_session {
            v.push(("plan", p.clone()));
        }
        v.push(("build", row.build_session.clone()));
        if let Some(r) = &row.review_session {
            v.push(("review", r.clone()));
        }
        v
    }

    fn stage_session(&self, row: &FlowRow, stage: &str) -> Option<String> {
        match stage {
            "plan" | "approve" => row.plan_session.clone(),
            "build" => Some(row.build_session.clone()),
            "review" => row.review_session.clone(),
            _ => None,
        }
    }

    fn update(&self, row: &FlowRow, u: FlowUpdate<'_>) -> FlowRow {
        let updated = self
            .db
            .update_flow(&row.id, u)
            .ok()
            .flatten()
            .unwrap_or_else(|| row.clone());
        self.publish(&updated);
        updated
    }

    fn finish(&self, row: &FlowRow, status: &str, reason: &str) -> FlowRow {
        let updated = self
            .db
            .update_flow(
                &row.id,
                FlowUpdate {
                    status: Some(status),
                    reason: Some(reason),
                    captured: Some(None),
                    ..Default::default()
                },
            )
            .ok()
            .flatten()
            .unwrap_or_else(|| row.clone());
        for (_, sid) in self.sessions(row) {
            self.record(
                &sid,
                "flow_ended",
                &json!({ "flowId": row.id, "status": status, "reason": reason, "reviews": updated.reviews }),
            );
        }
        self.publish(&updated);
        updated
    }

    /// The line every stage gets about who does what.
    fn lineup(&self, row: &FlowRow) -> String {
        let mut parts = Vec::new();
        for (stage, sid) in self.sessions(row) {
            if let Ok(s) = self.session(&sid) {
                let verb = match stage {
                    "plan" => "plans",
                    "build" => "builds",
                    _ => "reviews",
                };
                parts.push(format!("{} {verb}", self.who(&s)));
            }
        }
        parts.join(", ")
    }

    /// Passes `body` to the session for `stage` and records what it received.
    fn deliver(
        &self,
        row: &FlowRow,
        stage: &str,
        from: Option<&SessionRow>,
        label: &str,
        body: &str,
        prompt: &str,
    ) -> Result<(), String> {
        let to = self
            .stage_session(row, stage)
            .ok_or("This workflow has no session for that stage")?;
        self.record(
            &to,
            "flow_input",
            &json!({
                "flowId": row.id,
                "stage": stage,
                "label": label,
                "fromSessionId": from.map(|s| s.id.clone()),
                "fromProvider": from.map(|s| s.provider.clone()),
                "fromName": from.map(|s| self.who(s)),
                "text": body,
                "prompt": prompt,
            }),
        );
        self.agents.send(&to, prompt)
    }

    /// Creates the run, puts the planner and reviewer in plan mode and sends the first stage its task.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        project_id: &str,
        name: &str,
        task: &str,
        plan: Option<&SessionRow>,
        build: &SessionRow,
        review: Option<&SessionRow>,
        gate_plan: bool,
        max_reviews: i64,
    ) -> Result<FlowRow, String> {
        let task = task.trim();
        if task.is_empty() {
            return Err("Write the task for the agents first".into());
        }
        if !(1..=MAX_REVIEWS).contains(&max_reviews) {
            return Err(format!("Choose between 1 and {MAX_REVIEWS} reviews"));
        }
        let all: Vec<&SessionRow> = plan.into_iter().chain([build]).chain(review).collect();
        for (i, s) in all.iter().enumerate() {
            if all[..i].iter().any(|o| o.id == s.id) {
                return Err("Each stage needs its own session".into());
            }
            if s.project_id != project_id {
                return Err("All stages must be in the same project".into());
            }
            if self
                .db
                .active_flow_for(&s.id)
                .map_err(|e| e.to_string())?
                .is_some()
                || self
                    .db
                    .active_relay_for(&s.id)
                    .map_err(|e| e.to_string())?
                    .is_some()
            {
                return Err(format!(
                    "\"{}\" is already working with other agents. Stop that first.",
                    s.title
                ));
            }
        }
        let base = crate::claude::first_line(task, 48);
        for (s, stage) in plan
            .map(|p| (p, "Plan"))
            .into_iter()
            .chain([(build, "Build")])
            .chain(review.map(|r| (r, "Review")))
        {
            if s.title.starts_with("New session") {
                let t = format!("{stage} · {base}");
                if let Ok(Some(r)) = self
                    .db
                    .update_session(&s.id, None, None, Some(&t), None, None)
                {
                    (self.sink)(Outbound::Session(r));
                }
            }
        }
        // Planner and reviewer only read: plan mode makes AgentDeck refuse their changes.
        for s in plan.into_iter().chain(review) {
            if s.mode != "plan" {
                if let Ok(Some(r)) = self.db.set_session_mode(&s.id, "plan") {
                    (self.sink)(Outbound::Session(r));
                }
                let _ = self.agents.set_mode(&s.id, "plan");
            }
        }
        if build.mode == "plan" {
            if let Ok(Some(r)) = self.db.set_session_mode(&build.id, "default") {
                (self.sink)(Outbound::Session(r));
            }
            let _ = self.agents.set_mode(&build.id, "default");
        }
        let name = name.trim();
        let row = self
            .db
            .create_flow(
                project_id,
                if name.is_empty() { "Workflow" } else { name },
                task,
                plan.map(|s| s.id.as_str()),
                &build.id,
                review.map(|s| s.id.as_str()),
                gate_plan,
                max_reviews,
            )
            .map_err(|e| e.to_string())?;
        let stages: Vec<Value> = self
            .sessions(&row)
            .into_iter()
            .filter_map(|(stage, sid)| {
                let s = self.session(&sid).ok()?;
                Some(json!({ "stage": stage, "sessionId": s.id, "provider": s.provider, "name": self.who(&s) }))
            })
            .collect();
        for (stage, sid) in self.sessions(&row) {
            self.record(
                &sid,
                "flow_started",
                &json!({ "flowId": row.id, "name": row.name, "task": task, "stage": stage, "stages": stages, "gatePlan": gate_plan, "maxReviews": max_reviews }),
            );
        }
        self.publish(&row);

        let lineup = self.lineup(&row);
        let sent = if let Some(p) = plan {
            let builder = self.who(build);
            let prompt = format!(
                "You're the planner in an AgentDeck workflow ({lineup}).\n\nRead the code and write a concrete plan for the task below: which files change and how, the steps in order, and how to test it. Don't change anything. {builder} will build from your plan, so make it complete on its own. Present the whole plan as your answer.\n\nTask from the user:\n{task}"
            );
            let _ = p;
            self.deliver(&row, "plan", None, "Task", task, &prompt)
        } else {
            let prompt = self.build_prompt(&row, None, task);
            self.deliver(&row, "build", None, "Task", task, &prompt)
        };
        if let Err(e) = sent {
            self.finish(&row, "stopped", &format!("Couldn't start: {e}"));
            return Err(e);
        }
        Ok(row)
    }

    fn build_prompt(&self, row: &FlowRow, planner: Option<&SessionRow>, plan: &str) -> String {
        let lineup = self.lineup(row);
        let reviewer = row
            .review_session
            .as_deref()
            .and_then(|id| self.session(id).ok())
            .map(|s| self.who(&s));
        let report = match &reviewer {
            Some(r) => format!(" {r} will review your changes, so end with a short report: what you changed, in which files, and how you checked it."),
            None => " End with a short report of what you changed and how you checked it.".into(),
        };
        match planner {
            Some(p) => format!(
                "You're the builder in an AgentDeck workflow ({lineup}).\n\n{} wrote the plan below for this task. Implement it: read the code it touches first, follow the plan, and run the tests if the project has them. If the code contradicts the plan, say so.{report}\n\nTask from the user:\n{}\n\nPlan from {}:\n{}",
                self.who(p),
                row.task,
                self.who(p),
                plan
            ),
            None => format!(
                "You're the builder in an AgentDeck workflow ({lineup}).\n\nImplement the task below. Read the code first and run the tests if the project has them.{report}\n\nTask from the user:\n{}",
                row.task
            ),
        }
    }

    pub fn stop(&self, flow_id: &str, reason: &str) -> Result<FlowRow, String> {
        let row = self
            .db
            .flow(flow_id)
            .map_err(|e| e.to_string())?
            .ok_or("Workflow not found")?;
        if row.status != "active" {
            return Ok(row);
        }
        Ok(self.finish(&row, "stopped", reason))
    }

    /// The user approved the plan: send it to the builder.
    pub fn approve(&self, flow_id: &str) -> Result<FlowRow, String> {
        let row = self.waiting(flow_id)?;
        let plan = row.plan.clone().unwrap_or_default();
        let planner = row
            .plan_session
            .as_deref()
            .and_then(|id| self.session(id).ok());
        Ok(self.to_build(&row, planner.as_ref(), &plan))
    }

    /// The user wants the plan changed: send the planner their notes.
    pub fn revise(&self, flow_id: &str, notes: &str) -> Result<FlowRow, String> {
        let notes = notes.trim();
        if notes.is_empty() {
            return Err("Write what should change in the plan".into());
        }
        let row = self.waiting(flow_id)?;
        let row = self.update(
            &row,
            FlowUpdate {
                stage: Some("plan"),
                ..Default::default()
            },
        );
        let prompt = format!("The user asks for changes to your plan:\n\n{notes}\n\nUpdate the plan and present the whole plan again.");
        if let Err(e) = self.deliver(&row, "plan", None, "Changes from you", notes, &prompt) {
            return Ok(self.finish(&row, "stopped", &format!("Couldn't reach the planner: {e}")));
        }
        Ok(row)
    }

    fn waiting(&self, flow_id: &str) -> Result<FlowRow, String> {
        let row = self
            .db
            .flow(flow_id)
            .map_err(|e| e.to_string())?
            .ok_or("Workflow not found")?;
        if row.status != "active" || row.stage != "approve" {
            return Err("This plan isn't waiting for you anymore".into());
        }
        Ok(row)
    }

    fn to_build(&self, row: &FlowRow, planner: Option<&SessionRow>, plan: &str) -> FlowRow {
        let row = self.update(
            row,
            FlowUpdate {
                stage: Some("build"),
                plan: Some(plan),
                ..Default::default()
            },
        );
        // Planner sessions show their plan card as passed on.
        if let (Some(p), Ok(b)) = (planner, self.session(&row.build_session)) {
            self.record(
                &p.id,
                "flow_passed",
                &json!({ "flowId": row.id, "toSessionId": b.id, "toProvider": b.provider }),
            );
        }
        let prompt = self.build_prompt(&row, planner, plan);
        let label = match planner {
            Some(p) => format!("Plan from {}", self.who(p)),
            None => "Plan".into(),
        };
        if let Err(e) = self.deliver(&row, "build", planner, &label, plan, &prompt) {
            return self.finish(&row, "stopped", &format!("Couldn't reach the builder: {e}"));
        }
        row
    }

    /// Claude Code presents a plan with ExitPlanMode and waits for an answer. In a workflow the
    /// plan goes to the next stage instead, so AgentDeck keeps it and tells Claude its part is done.
    pub fn on_plan_request(&self, ev: &StoredEvent) {
        if ev.source != "claude" || ev.kind != "control_request/can_use_tool" {
            return;
        }
        let p = &ev.payload;
        if p.pointer("/request/tool_name").and_then(Value::as_str) != Some("ExitPlanMode") {
            return;
        }
        let Some(rid) = p["request_id"].as_str() else {
            return;
        };
        let Ok(Some(row)) = self.db.active_flow_for(&ev.session_id) else {
            return;
        };
        if self.stage_session(&row, &row.stage).as_deref() != Some(ev.session_id.as_str()) {
            return;
        }
        let input = p.pointer("/request/input").cloned().unwrap_or(json!({}));
        let mut plan = input["plan"].as_str().unwrap_or("").trim().to_string();
        if plan.is_empty() {
            // Older CLIs only point at the plan file they wrote under ~/.claude/plans.
            if let Some(path) = input["planFilePath"].as_str() {
                let path = std::path::Path::new(path);
                let in_plans = path
                    .parent()
                    .and_then(|d| d.file_name())
                    .is_some_and(|n| n == "plans");
                if in_plans && path.extension().is_some_and(|e| e == "md") {
                    plan = std::fs::read_to_string(path).unwrap_or_default();
                }
            }
        }
        if !plan.trim().is_empty() {
            self.update(
                &row,
                FlowUpdate {
                    captured: Some(Some(plan.trim())),
                    ..Default::default()
                },
            );
        }
        self.record(
            &ev.session_id,
            "flow_plan_taken",
            &json!({ "flowId": row.id, "requestId": rid }),
        );
        let msg = "AgentDeck took this and passed it to the next agent in the workflow. Your part is finished for now: don't implement anything, just reply with one short line.";
        let _ = self
            .agents
            .claude
            .deny_with_message(&ev.session_id, rid, msg);
    }

    /// An agent in an active workflow asks for approval: AgentDeck answers it so the workflow
    /// keeps going, unless it's risky or reaches outside the project; those wait for the user
    /// (the rule in `unattended`). Questions wait for the user too.
    pub fn on_approval(&self, ev: &StoredEvent) {
        if crate::unattended::claude_tool(ev) == Some("ExitPlanMode") {
            self.on_plan_request(ev);
            return;
        }
        let Some(v) = crate::unattended::verdict(&self.db, ev) else {
            return;
        };
        let Ok(Some(row)) = self.db.active_flow_for(&ev.session_id) else {
            return;
        };
        match v {
            Verdict::Question => {}
            Verdict::Hold { request_id, why } => self.record(
                &ev.session_id,
                "flow_needs_you",
                &json!({ "flowId": row.id, "requestId": request_id, "why": why }),
            ),
            Verdict::Allow { request_id } => crate::unattended::allow(
                &self.db,
                &self.agents,
                &self.sink,
                &ev.session_id,
                &request_id,
                "workflow",
            ),
        }
    }

    /// Called for every stored event that ends a turn.
    pub fn on_event(&self, ev: &StoredEvent) {
        let Some(end) = turn_end(ev) else { return };
        let Ok(Some(row)) = self.db.active_flow_for(&ev.session_id) else {
            return;
        };
        // Only the stage that has the work moves the workflow on.
        if self.stage_session(&row, &row.stage).as_deref() != Some(ev.session_id.as_str())
            || row.stage == "approve"
        {
            return;
        }
        let Ok(from) = self.session(&ev.session_id) else {
            return;
        };
        let who = self.who(&from);
        let captured = row.captured.clone();
        let row = self.update(
            &row,
            FlowUpdate {
                captured: Some(None),
                ..Default::default()
            },
        );
        let text = match (end, captured) {
            (_, Some(c)) => c,
            (TurnEnd::Failed(why), None) => {
                self.finish(&row, "stopped", &format!("{who}: {why}"));
                return;
            }
            (TurnEnd::NoReply, None) => {
                // Often a step that needed an OK was skipped (Antigravity can't ask) and the
                // agent carries on once that's sorted out. Wait for its next reply instead of
                // ending the workflow; the user can still stop it.
                self.record(
                    &ev.session_id,
                    "flow_no_reply",
                    &json!({ "flowId": row.id, "who": who }),
                );
                return;
            }
            (TurnEnd::Reply(t), None) => t,
        };
        let text = clip(&text);
        match row.stage.as_str() {
            "plan" => {
                if row.gate_plan {
                    let row = self.update(
                        &row,
                        FlowUpdate {
                            stage: Some("approve"),
                            plan: Some(&text),
                            ..Default::default()
                        },
                    );
                    self.record(
                        &from.id,
                        "flow_waiting",
                        &json!({ "flowId": row.id, "plan": text }),
                    );
                } else {
                    self.to_build(&row, Some(&from), &text);
                }
            }
            "build" => {
                let Some(rid) = row.review_session.clone() else {
                    self.finish(&row, "done", &format!("{who} finished building"));
                    return;
                };
                let Ok(reviewer) = self.session(&rid) else {
                    self.finish(&row, "stopped", "The reviewer's session was deleted");
                    return;
                };
                let row = self.update(
                    &row,
                    FlowUpdate {
                        stage: Some("review"),
                        ..Default::default()
                    },
                );
                let first = row.reviews == 0;
                let prompt = if first {
                    let plan = match (&row.plan, &row.plan_session) {
                        (Some(p), Some(_)) => format!("\n\nThe plan it followed:\n{p}"),
                        _ => String::new(),
                    };
                    format!(
                        "You're the reviewer in an AgentDeck workflow ({}).\n\n{who} has implemented the task below. Review the change: read the files it touched, check them against the task{}, and look for bugs, missed cases and missing tests. You can't change files yourself.\n\nIf everything is right, write APPROVED on its own line. Otherwise list the concrete fixes {who} must make; they'll be sent to {who}.\n\nTask from the user:\n{}{plan}\n\nReport from {who}:\n{text}",
                        self.lineup(&row),
                        if row.plan.is_some() { " and the plan" } else { "" },
                        row.task,
                    )
                } else {
                    format!(
                        "{who} made changes after your review. Report from {who}:\n\n{text}\n\nCheck them. Write APPROVED on its own line if everything is right now, otherwise list what's still wrong."
                    )
                };
                let label = format!("Report from {who}");
                if let Err(e) = self.deliver(&row, "review", Some(&from), &label, &text, &prompt) {
                    self.finish(
                        &row,
                        "stopped",
                        &format!("Couldn't reach {}: {e}", self.who(&reviewer)),
                    );
                }
            }
            "review" => {
                if says(&text, &["APPROVED", "DONE"]) {
                    self.finish(&row, "done", &format!("{who} approved the work"));
                    return;
                }
                if row.reviews >= row.max_reviews {
                    self.finish(
                        &row,
                        "stopped",
                        &format!(
                            "{who} still had notes after {} reviews",
                            row.max_reviews + 1
                        ),
                    );
                    return;
                }
                let n = row.reviews + 1;
                let row = self.update(
                    &row,
                    FlowUpdate {
                        stage: Some("build"),
                        reviews: Some(n),
                        ..Default::default()
                    },
                );
                let prompt = format!(
                    "{who} reviewed your changes and asks for these fixes (review {n} of at most {}):\n\n{text}\n\nMake the fixes, then end with a short report of what you changed.",
                    row.max_reviews + 1
                );
                let label = format!("Review notes from {who}");
                if let Err(e) = self.deliver(&row, "build", Some(&from), &label, &text, &prompt) {
                    self.finish(&row, "stopped", &format!("Couldn't reach the builder: {e}"));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;

    #[test]
    fn approval_is_a_line_of_its_own() {
        assert!(says("Looks right.\n\nAPPROVED", &["APPROVED", "DONE"]));
        assert!(says("**APPROVED**", &["APPROVED"]));
        assert!(says("## APPROVED", &["APPROVED"]));
        assert!(!says("Not APPROVED yet", &["APPROVED"]));
        assert!(!says("approved", &["APPROVED"]));
    }

    fn setup() -> (Arc<Db>, Flow, String) {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let sink: Sink = Arc::new(|_| {});
        let mut claude = Manager::new(db.clone(), sink.clone());
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
        let p = db.add_project("demo", "/tmp").unwrap();
        (db.clone(), Flow::new(db, sink, agents), p.id)
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

    fn inputs(db: &Db, sid: &str) -> Vec<Value> {
        db.events(sid)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "flow_input")
            .map(|e| e.payload)
            .collect()
    }

    #[test]
    fn start_validates_and_puts_planner_and_reviewer_in_plan_mode() {
        let (db, flow, p) = setup();
        let a = db
            .create_session(&p, "claude-code", "default", "New session")
            .unwrap();
        let b = db
            .create_session(&p, "codex", "default", "New session")
            .unwrap();
        let c = db
            .create_session(&p, "claude-code", "default", "New session")
            .unwrap();
        assert!(flow
            .start(&p, "w", " ", Some(&a), &b, Some(&c), false, 2)
            .is_err());
        assert!(flow
            .start(&p, "w", "task", Some(&a), &a, None, false, 2)
            .is_err());
        assert!(flow
            .start(&p, "w", "task", None, &b, None, false, 0)
            .is_err());
        // No CLI here: the run is created, the planner can't be reached, and it stops with the reason.
        assert!(flow
            .start(
                &p,
                "w",
                "Fix the discount",
                Some(&a),
                &b,
                Some(&c),
                false,
                2
            )
            .is_err());
        let f = db.list_flows().unwrap().remove(0);
        assert_eq!(f.status, "stopped");
        assert!(f.reason.unwrap().starts_with("Couldn't start"));
        assert_eq!(db.session(&a.id).unwrap().unwrap().mode, "plan");
        assert_eq!(db.session(&c.id).unwrap().unwrap().mode, "plan");
        assert_eq!(
            db.session(&b.id).unwrap().unwrap().title,
            "Build · Fix the discount"
        );
        let started = db
            .events(&c.id)
            .unwrap()
            .into_iter()
            .find(|e| e.kind == "flow_started")
            .unwrap();
        assert_eq!(started.payload["stages"].as_array().unwrap().len(), 3);
    }

    /// Sends fail here (no CLI), which stops a run; put it back to check the routing alone.
    fn revive(db: &Db, id: &str) {
        db.update_flow(
            id,
            FlowUpdate {
                status: Some("active"),
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn plan_goes_to_build_then_review_and_notes_go_back_until_approved() {
        let (db, flow, p) = setup();
        let a = db
            .create_session(&p, "claude-code", "default", "Plan")
            .unwrap();
        let b = db.create_session(&p, "codex", "default", "Build").unwrap();
        let c = db
            .create_session(&p, "claude-code", "default", "Review")
            .unwrap();
        let f = db
            .create_flow(
                &p,
                "w",
                "Fix the discount",
                Some(&a.id),
                &b.id,
                Some(&c.id),
                false,
                2,
            )
            .unwrap();

        // A Claude Code plan presented with ExitPlanMode is what the builder gets, not the short reply.
        flow.on_plan_request(&StoredEvent {
            session_id: a.id.clone(),
            seq: 2,
            ts: 1,
            source: "claude".into(),
            kind: "control_request/can_use_tool".into(),
            payload: json!({ "request_id": "r1", "request": { "tool_name": "ExitPlanMode", "input": { "plan": "1. Apply the discount before tax" } } }),
        });
        assert!(db
            .events(&a.id)
            .unwrap()
            .iter()
            .any(|e| e.kind == "flow_plan_taken"));
        // A turn from a session that doesn't have the work is ignored.
        flow.on_event(&end(&c.id, "unrelated"));
        assert_eq!(db.flow(&f.id).unwrap().unwrap().stage, "plan");

        flow.on_event(&end(&a.id, "OK."));
        revive(&db, &f.id);
        let to_b = inputs(&db, &b.id);
        assert_eq!(to_b.len(), 1);
        assert_eq!(to_b[0]["text"], "1. Apply the discount before tax");
        assert!(to_b[0]["prompt"]
            .as_str()
            .unwrap()
            .starts_with("You're the builder"));
        let f1 = db.flow(&f.id).unwrap().unwrap();
        assert_eq!(
            (f1.stage.as_str(), f1.plan.as_deref()),
            ("build", Some("1. Apply the discount before tax"))
        );
        assert!(f1.captured.is_none());
        assert!(db
            .events(&a.id)
            .unwrap()
            .iter()
            .any(|e| e.kind == "flow_passed"));

        flow.on_event(&end(&b.id, "Implemented it."));
        revive(&db, &f.id);
        let to_c = inputs(&db, &c.id);
        assert_eq!(to_c.len(), 1);
        assert!(to_c[0]["prompt"]
            .as_str()
            .unwrap()
            .contains("The plan it followed"));
        assert_eq!(db.flow(&f.id).unwrap().unwrap().stage, "review");

        flow.on_event(&end(&c.id, "1. Add a test for a zero discount."));
        revive(&db, &f.id);
        let to_b = inputs(&db, &b.id);
        assert_eq!(to_b.len(), 2);
        assert_eq!(to_b[1]["label"], "Review notes from Claude Code");
        let f2 = db.flow(&f.id).unwrap().unwrap();
        assert_eq!((f2.stage.as_str(), f2.reviews), ("build", 1));

        flow.on_event(&end(&b.id, "Added the test."));
        revive(&db, &f.id);
        assert!(inputs(&db, &c.id)[1]["prompt"]
            .as_str()
            .unwrap()
            .starts_with("Codex made changes"));
        // A turn without a reply (Antigravity skipped a step it couldn't ask about) doesn't
        // end the workflow: it waits for the reviewer's next reply.
        flow.on_event(&end(&c.id, ""));
        let waiting = db.flow(&f.id).unwrap().unwrap();
        assert_eq!(
            (waiting.status.as_str(), waiting.stage.as_str()),
            ("active", "review")
        );
        assert!(db
            .events(&c.id)
            .unwrap()
            .iter()
            .any(|e| e.kind == "flow_no_reply"));
        flow.on_event(&end(&c.id, "All good.\nAPPROVED"));
        let done = db.flow(&f.id).unwrap().unwrap();
        assert_eq!(done.status, "done");
        assert_eq!(
            done.reason.as_deref(),
            Some("Claude Code approved the work")
        );
        for s in [&a.id, &b.id, &c.id] {
            assert!(db.events(s).unwrap().iter().any(|e| e.kind == "flow_ended"));
        }
    }

    #[test]
    fn gate_waits_for_the_user_and_review_limit_stops() {
        let (db, flow, p) = setup();
        let a = db.create_session(&p, "codex", "default", "Plan").unwrap();
        let b = db
            .create_session(&p, "claude-code", "default", "Build")
            .unwrap();
        let c = db.create_session(&p, "codex", "default", "Review").unwrap();
        let f = db
            .create_flow(&p, "w", "task", Some(&a.id), &b.id, Some(&c.id), true, 1)
            .unwrap();
        let mut ev = end(&a.id, "");
        ev.source = "codex".into();
        ev.kind = "turn/completed".into();
        ev.payload = json!({ "turn": { "status": "completed", "items": [{ "type": "agentMessage", "text": "The plan" }] } });
        flow.on_event(&ev);
        let w = db.flow(&f.id).unwrap().unwrap();
        assert_eq!(
            (w.stage.as_str(), w.plan.as_deref()),
            ("approve", Some("The plan"))
        );
        assert!(
            inputs(&db, &b.id).is_empty(),
            "nothing is built before approval"
        );
        assert!(flow.revise(&f.id, "  ").is_err());
        // Approving sends the plan on (the send fails here without a CLI, which stops the run).
        let r = flow.approve(&f.id).unwrap();
        assert_eq!(inputs(&db, &b.id).len(), 1);
        assert_eq!(r.status, "stopped");
        assert!(flow.approve(&f.id).is_err());

        // Review limit: one round of notes allowed, the second set ends the run.
        let g = db
            .create_flow(&p, "w", "task", None, &b.id, Some(&c.id), false, 1)
            .unwrap();
        db.update_flow(
            &g.id,
            FlowUpdate {
                stage: Some("review"),
                reviews: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        let mut ev = end(&c.id, "");
        ev.source = "codex".into();
        ev.kind = "turn/completed".into();
        ev.payload = json!({ "turn": { "status": "completed", "items": [{ "type": "agentMessage", "text": "Still wrong" }] } });
        flow.on_event(&ev);
        let g = db.flow(&g.id).unwrap().unwrap();
        assert_eq!(g.status, "stopped");
        assert_eq!(
            g.reason.as_deref(),
            Some("Codex still had notes after 2 reviews")
        );
    }
}

/// End-to-end: real `claude` and `codex` CLIs against the local mocks
/// (`scripts/mock_anthropic.py 8787`, `scripts/mock_openai.py 8788`). Run with AGENTDECK_IT=1.
#[cfg(test)]
mod integration {
    use super::*;
    use crate::claude::Manager;
    use crate::codex::CodexManager;
    use crate::db::FlowRow;
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    #[test]
    #[ignore]
    fn claude_plans_codex_builds_claude_reviews_until_approved() {
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
        let mk = |prov: &str| {
            db.create_session(&p.id, prov, "default", "New session")
                .unwrap()
        };
        let (a, b, c) = (mk("claude-code"), mk("codex"), mk("claude-code"));

        let cell: Arc<OnceLock<Arc<Flow>>> = Arc::new(OnceLock::new());
        let c2 = cell.clone();
        let (tx, rx) = mpsc::channel::<FlowRow>();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| match o {
            Outbound::Event(e) => {
                if let Some(f) = c2.get() {
                    let f = f.clone();
                    if turn_end(&e).is_some() {
                        std::thread::spawn(move || f.on_event(&e));
                    } else if e.kind == "control_request/can_use_tool" || e.kind == "approval" {
                        std::thread::spawn(move || f.on_approval(&e));
                    }
                }
            }
            Outbound::Flow(r) => {
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
        let flow = Arc::new(Flow::new(db.clone(), sink, agents));
        let _ = cell.set(flow.clone());

        let row = flow
            .start(
                &p.id,
                "Plan, build, review",
                "Fix the discount bug in invoice.py",
                Some(&a),
                &b,
                Some(&c),
                false,
                3,
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(150);
        let mut last = row;
        while last.status == "active" && Instant::now() < deadline {
            if let Ok(r) = rx.recv_timeout(Duration::from_millis(200)) {
                last = r;
            }
        }
        if let Ok(path) = std::env::var("AGENTDECK_DUMP_FLOW") {
            let all = json!({ "plan": db.events(&a.id).unwrap(), "build": db.events(&b.id).unwrap(), "review": db.events(&c.id).unwrap() });
            std::fs::write(path, serde_json::to_string_pretty(&all).unwrap()).unwrap();
        }
        assert_eq!(last.status, "done", "{:?}", last.reason);
        assert_eq!(
            last.reason.as_deref(),
            Some("Claude Code approved the work")
        );
        assert_eq!(last.reviews, 1, "one round of notes, then approval");
        assert!(
            last.plan.as_deref().unwrap_or("").contains("discount"),
            "the builder got the real plan: {:?}",
            last.plan
        );

        let to_codex: Vec<Value> = db
            .events(&b.id)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "flow_input")
            .map(|e| e.payload)
            .collect();
        assert_eq!(to_codex.len(), 2, "the plan, then the review notes");
        assert!(to_codex[0]["text"]
            .as_str()
            .unwrap()
            .contains("Apply the discount before tax"));
        assert!(to_codex[1]["text"]
            .as_str()
            .unwrap()
            .contains("zero discount"));
        let to_reviewer = db
            .events(&c.id)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "flow_input")
            .count();
        assert_eq!(to_reviewer, 2);
        // The planner never left plan mode: its plan went on instead of being approved.
        assert_eq!(db.session(&a.id).unwrap().unwrap().mode, "plan");
        assert!(db
            .events(&a.id)
            .unwrap()
            .iter()
            .any(|e| e.kind == "flow_plan_taken"));
        claude.stop(&a.id);
        claude.stop(&c.id);
        codex.stop(&b.id);
    }
}
