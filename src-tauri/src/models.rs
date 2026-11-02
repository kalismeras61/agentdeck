//! The models each agent offers, as the agent itself reports them.
//!
//! - Claude Code: the `models` list in its `initialize` control response.
//! - Codex: `model/list` on the app server.
//! - ACP agents (Gemini CLI, Grok Build): `models.availableModels` in the `session/new` answer.
//!
//! Lists are asked for with a short-lived agent process (no prompt is sent, nothing is billed),
//! and also picked up whenever a real session starts. The last list is saved so the picker is
//! filled at once on the next launch.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::db::Db;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    /// What is passed to the agent. "default" leaves the agent's own setting.
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModelList {
    pub provider: String,
    pub models: Vec<ModelChoice>,
    /// "agent" (asked just now or this run), "saved" (from an earlier run), "builtin".
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

static CACHE: Mutex<Option<HashMap<String, Vec<ModelChoice>>>> = Mutex::new(None);

const PROBE_TIMEOUT: Duration = Duration::from_secs(25);

fn default_choice(agent: &str) -> ModelChoice {
    ModelChoice {
        id: "default".into(),
        label: "Default".into(),
        description: Some(format!("Whatever {agent} is set to use")),
    }
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Claude Code's `initialize` response → choices. Its list already starts with "default".
pub fn from_claude(response: &Value) -> Vec<ModelChoice> {
    let mut out: Vec<ModelChoice> = response["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = clean(m["value"].as_str())?;
            Some(ModelChoice {
                label: clean(m["displayName"].as_str()).unwrap_or_else(|| id.clone()),
                description: clean(m["description"].as_str()),
                id,
            })
        })
        .collect();
    if !out.is_empty() && !out.iter().any(|m| m.id == "default") {
        out.insert(0, default_choice("Claude Code"));
    }
    out
}

/// Codex `model/list` result → choices, hidden models left out.
pub fn from_codex(result: &Value) -> Vec<ModelChoice> {
    let mut out = vec![default_choice("Codex")];
    for m in result["data"].as_array().into_iter().flatten() {
        if m["hidden"].as_bool() == Some(true) {
            continue;
        }
        let Some(id) = clean(m["model"].as_str()).or_else(|| clean(m["id"].as_str())) else {
            continue;
        };
        let mut description = clean(m["description"].as_str());
        if m["isDefault"].as_bool() == Some(true) {
            description = Some(match description {
                Some(d) => format!("{d} (Codex's default)"),
                None => "Codex's default".into(),
            });
        }
        out.push(ModelChoice {
            label: clean(m["displayName"].as_str()).unwrap_or_else(|| id.clone()),
            description,
            id,
        });
    }
    if out.len() == 1 {
        out.clear();
    }
    out
}

/// ACP `session/new` (or `session/load`) result → choices.
pub fn from_acp(result: &Value, agent: &str) -> Vec<ModelChoice> {
    let mut out = vec![default_choice(agent)];
    for m in result
        .pointer("/models/availableModels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = clean(m["modelId"].as_str()) else { continue };
        if id == "default" {
            continue;
        }
        out.push(ModelChoice {
            label: clean(m["name"].as_str()).unwrap_or_else(|| id.clone()),
            description: clean(m["description"].as_str()),
            id,
        });
    }
    if out.len() == 1 {
        out.clear();
    }
    out
}

/// `agy models --output-format json`: a list (or an object holding one) of model entries or
/// plain slugs. The slug is what `--model` takes.
pub fn from_agy(v: &Value) -> Vec<ModelChoice> {
    let list = v
        .as_array()
        .cloned()
        .or_else(|| v.as_object().and_then(|o| o.values().find_map(|x| x.as_array().cloned())))
        .unwrap_or_default();
    let mut out = vec![default_choice("Antigravity")];
    for m in list {
        let (id, label, description) = match &m {
            Value::String(s) => (Some(s.clone()), None, None),
            Value::Object(o) => {
                let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned).filter(|x| !x.trim().is_empty());
                (
                    s("slug").or_else(|| s("id")).or_else(|| s("model")).or_else(|| s("name")),
                    s("label").or_else(|| s("display_name")).or_else(|| s("displayName")).or_else(|| s("name")),
                    s("description"),
                )
            }
            _ => (None, None, None),
        };
        let Some(id) = id.filter(|i| i != "default") else { continue };
        if out.iter().any(|x| x.id == id) {
            continue;
        }
        out.push(ModelChoice { label: label.unwrap_or_else(|| id.clone()), description, id });
    }
    if out.len() == 1 {
        out.clear();
    }
    out
}

fn key(provider: &str) -> String {
    format!("models:{provider}")
}

/// Keeps a list an agent reported (in memory and in the database).
pub fn remember(db: &Db, provider: &str, models: &[ModelChoice]) {
    if models.is_empty() {
        return;
    }
    if let Ok(mut c) = CACHE.lock() {
        c.get_or_insert_with(HashMap::new)
            .insert(provider.to_string(), models.to_vec());
    }
    if let Ok(s) = serde_json::to_string(models) {
        let _ = db.set_meta(&key(provider), &s);
    }
}

fn cached(provider: &str) -> Option<Vec<ModelChoice>> {
    CACHE
        .lock()
        .ok()
        .and_then(|c| c.as_ref().and_then(|m| m.get(provider).cloned()))
}

fn saved(db: &Db, provider: &str) -> Option<Vec<ModelChoice>> {
    db.meta(&key(provider))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<ModelChoice>>(&s).ok())
        .filter(|m| !m.is_empty())
}

fn builtin(provider: &str) -> Vec<ModelChoice> {
    let c = |id: &str, label: &str| ModelChoice { id: id.into(), label: label.into(), description: None };
    match provider {
        "claude-code" => vec![
            default_choice("Claude Code"),
            c("opus", "Opus"),
            c("sonnet", "Sonnet"),
            c("haiku", "Haiku"),
        ],
        "codex" => vec![default_choice("Codex")],
        "gemini-cli" => vec![default_choice("Gemini CLI")],
        "grok-build" => vec![default_choice("Grok Build")],
        "antigravity" => vec![default_choice("Antigravity")],
        _ => vec![],
    }
}

/// The models for an agent: this run's list, else asks the agent, else the saved list.
pub fn list(db: &Db, provider: &str, refresh: bool) -> ModelList {
    let done = |models, source: &str, error| ModelList {
        provider: provider.to_string(),
        models,
        source: source.into(),
        error,
    };
    if !refresh {
        if let Some(m) = cached(provider) {
            return done(m, "agent", None);
        }
    }
    match probe(provider) {
        Ok(m) if !m.is_empty() => {
            remember(db, provider, &m);
            done(m, "agent", None)
        }
        other => {
            let error = match other {
                Err(e) => Some(e),
                Ok(_) => Some("The agent didn't list any models".into()),
            };
            match saved(db, provider) {
                Some(m) => done(m, "saved", error),
                None => done(builtin(provider), "builtin", error),
            }
        }
    }
}

/// A line-based JSON process for a quick question.
struct Probe {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    deadline: Instant,
}

impl Probe {
    fn start(bin: PathBuf, args: &[&str], cwd: &std::path::Path, env: &[(&str, String)]) -> Result<Self, String> {
        let mut cmd = Command::new(&bin);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::shellenv::apply(&mut cmd);
        cmd.env("PATH", crate::claude::augmented_path());
        for (k, _) in std::env::vars_os() {
            let k = k.to_string_lossy();
            if k == "CLAUDECODE" || k.starts_with("CLAUDE_CODE_") {
                cmd.env_remove(k.as_ref());
            }
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start {}: {e}", bin.display()))?;
        let stdin = child.stdin.take().ok_or("No stdin")?;
        let stdout = child.stdout.take().ok_or("No stdout")?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if tx.send(v).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self { child, stdin, rx, deadline: Instant::now() + PROBE_TIMEOUT })
    }

    fn send(&mut self, v: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{v}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }

    /// The first message `pick` returns something for.
    fn wait<T>(&mut self, mut pick: impl FnMut(&Value) -> Option<Result<T, String>>) -> Result<T, String> {
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("The agent took too long to answer".into());
            }
            match self.rx.recv_timeout(left) {
                Ok(v) => {
                    if let Some(r) = pick(&v) {
                        return r;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err("The agent took too long to answer".into())
                }
                Err(_) => return Err("The agent exited before answering".into()),
            }
        }
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn rpc_answer(v: &Value, id: u64) -> Option<Result<Value, String>> {
    if v.get("id").and_then(Value::as_u64) != Some(id) || v.get("method").is_some() {
        return None;
    }
    Some(match v.get("error") {
        Some(e) => Err(e["message"].as_str().unwrap_or("The agent returned an error").to_string()),
        None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
    })
}

/// Claude Code's answer to `initialize` when started in `cwd`: models, slash commands, skills…
/// It starts no conversation and makes no model call.
pub fn claude_initialize(cwd: &std::path::Path) -> Result<Value, String> {
    let bin = crate::providers::find_binary("claude").ok_or("Claude Code isn't installed")?;
    let mut p = Probe::start(
        bin,
        &["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"],
        cwd,
        &[],
    )?;
    p.send(&json!({ "type": "control_request", "request_id": "agentdeck-models", "request": { "subtype": "initialize" } }))?;
    p.wait(|v| {
        (v.pointer("/response/request_id").and_then(Value::as_str) == Some("agentdeck-models"))
            .then(|| match v.pointer("/response/subtype").and_then(Value::as_str) {
                Some("success") => Ok(v.pointer("/response/response").cloned().unwrap_or(Value::Null)),
                _ => Err(v.pointer("/response/error").and_then(Value::as_str).unwrap_or("Claude Code returned an error").to_string()),
            })
    })
}

fn probe(provider: &str) -> Result<Vec<ModelChoice>, String> {
    let tmp = std::env::temp_dir().join("agentdeck-models");
    let _ = std::fs::create_dir_all(&tmp);
    match provider {
        "claude-code" => Ok(from_claude(&claude_initialize(&tmp)?)),
        "codex" => {
            let bin = crate::providers::find_binary("codex").ok_or("Codex isn't installed")?;
            let mut p = Probe::start(bin, &["app-server"], &tmp, &[])?;
            p.send(&json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "agentdeck", "title": "AgentDeck", "version": env!("CARGO_PKG_VERSION") } } }))?;
            p.wait(|v| rpc_answer(v, 1))?;
            p.send(&json!({ "method": "initialized" }))?;
            p.send(&json!({ "id": 2, "method": "model/list", "params": {} }))?;
            let r = p.wait(|v| rpc_answer(v, 2))?;
            Ok(from_codex(&r))
        }
        "antigravity" => {
            let bin = crate::providers::find_binary("agy").ok_or("Antigravity CLI (agy) isn't installed")?;
            let mut cmd = Command::new(&bin);
            cmd.args(["models", "--output-format", "json"])
                .current_dir(&tmp)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            crate::shellenv::apply(&mut cmd);
            cmd.env("PATH", crate::claude::augmented_path());
            let child = cmd.spawn().map_err(|e| format!("Couldn't start agy: {e}"))?;
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(child.wait_with_output());
            });
            let out = rx
                .recv_timeout(PROBE_TIMEOUT)
                .map_err(|_| "agy took too long to list its models".to_string())?
                .map_err(|e| e.to_string())?;
            let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "agy didn't list its models as JSON".to_string())?;
            Ok(from_agy(&v))
        }
        "gemini-cli" | "grok-build" => {
            let spec = crate::acp::spec(provider).ok_or("Unknown agent")?;
            let bin = crate::providers::find_binary(spec.binary)
                .ok_or_else(|| format!("{} isn't installed", spec.label))?;
            // Same as a real session: a key saved in AgentDeck is used for this run only.
            let mut env = Vec::new();
            if let Some(k) = crate::secrets::get(&crate::acp::key_name(provider)).filter(|k| !k.trim().is_empty()) {
                env.push((spec.key_env, k));
                if provider == "gemini-cli" {
                    env.push(("GEMINI_CLI_AUTH_OVERRIDE", spec.key_method.to_string()));
                }
            }
            let mut p = Probe::start(bin, spec.args, &tmp, &env)?;
            p.send(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": 1, "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false } } }))?;
            p.wait(|v| rpc_answer(v, 1))?;
            p.send(&json!({ "jsonrpc": "2.0", "id": 2, "method": "session/new", "params": { "cwd": tmp.to_string_lossy(), "mcpServers": [] } }))?;
            let r = p.wait(|v| rpc_answer(v, 2))?;
            Ok(from_acp(&r, spec.label))
        }
        _ => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_agents_model_list() {
        let claude = from_claude(&json!({ "models": [
            { "value": "default", "displayName": "Default (recommended)", "description": "Opus 5.5" },
            { "value": "fable", "displayName": "Fable", "description": "Fable 5.1" },
            { "value": "", "displayName": "broken" }
        ] }));
        assert_eq!(claude.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["default", "fable"]);

        let codex = from_codex(&json!({ "data": [
            { "id": "gpt-6.1-sol", "model": "gpt-6.1-sol", "displayName": "GPT-6.1-Sol", "description": "Workhorse", "isDefault": true, "hidden": false },
            { "id": "secret", "model": "secret", "hidden": true }
        ] }));
        assert_eq!(codex.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["default", "gpt-6.1-sol"]);
        assert_eq!(codex[1].description.as_deref(), Some("Workhorse (Codex's default)"));

        let gemini = from_acp(&json!({ "sessionId": "s", "models": { "currentModelId": "auto", "availableModels": [
            { "modelId": "auto", "name": "Auto", "description": "Picks for you" },
            { "modelId": "gemini-2.5-pro", "name": "gemini-2.5-pro" }
        ] } }), "Gemini CLI");
        assert_eq!(gemini.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["default", "auto", "gemini-2.5-pro"]);
        assert!(from_acp(&json!({ "sessionId": "s" }), "Grok").is_empty());

        let agy = from_agy(&json!({ "models": [
            { "slug": "gemini-3.8-flash-medium", "label": "Gemini 3.8 Flash (medium)" },
            "gemini-3.1-pro-high"
        ] }));
        assert_eq!(agy.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["default", "gemini-3.8-flash-medium", "gemini-3.1-pro-high"]);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Asks the installed Claude Code and Codex for their models. Needs both CLIs.
    #[test]
    #[ignore]
    fn the_installed_agents_list_their_models() {
        let claude = probe("claude-code").expect("claude");
        eprintln!("claude: {:?}", claude.iter().map(|m| &m.id).collect::<Vec<_>>());
        assert!(claude.iter().any(|m| m.id == "default") && claude.len() > 2);
        let codex = probe("codex").expect("codex");
        eprintln!("codex: {:?}", codex.iter().map(|m| &m.id).collect::<Vec<_>>());
        assert!(codex.len() > 2);
    }
}
