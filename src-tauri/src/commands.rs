//! Slash commands for the message box: what each agent offers in the session's folder.
//!
//! - Claude Code: the `commands` in its `initialize` answer (built-ins that work headless,
//!   your custom commands, skills, plugin commands), from the running process or a short probe
//!   started in the session's folder.
//! - Gemini CLI (ACP): the `available_commands_update` it sends, or its command files.
//! - Codex: your custom prompts (`~/.codex/prompts`), whose text AgentDeck puts in the message.
//! - Everyone: commands AgentDeck itself carries out (model, plan mode, terminal…), added by
//!   the interface.

use crate::context::{self, Homes};
use crate::db::{Db, SessionRow};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    /// Without the slash: "compact", "review", "prompts:deploy".
    pub name: String,
    pub description: Option<String>,
    /// What goes after the name, e.g. "[file]".
    pub hint: Option<String>,
    /// "agent" (the agent's own), "custom" (your command files), "skill".
    pub source: String,
    /// Text to put in the message instead of sending "/name" (Codex prompts).
    pub expand: Option<String>,
}

fn cache() -> &'static Mutex<HashMap<String, Vec<SlashCommand>>> {
    static C: OnceLock<Mutex<HashMap<String, Vec<SlashCommand>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Keeps what a running agent reported, by session.
pub fn remember(session_id: &str, list: Vec<SlashCommand>) {
    if let Ok(mut c) = cache().lock() {
        c.insert(session_id.to_string(), list);
    }
}

fn cached(key: &str) -> Option<Vec<SlashCommand>> {
    cache().lock().ok().and_then(|c| c.get(key).cloned())
}

fn short(text: Option<&str>, max: usize) -> Option<String> {
    let t = text?.trim();
    if t.is_empty() {
        return None;
    }
    let t: String = t.chars().take(max).collect();
    Some(t)
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 80 && n.chars().all(|c| c.is_alphanumeric() || "-_:.".contains(c))
}

/// Claude Code's `initialize` answer → commands. Its descriptions end with "(user)",
/// "(project)", "(plugin …)" for ones that come from files; the rest are built in.
pub fn from_claude(init: &Value) -> Vec<SlashCommand> {
    let mut out: Vec<SlashCommand> = init["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let name = c["name"].as_str()?.trim_start_matches('/').to_string();
            if !valid_name(&name) {
                return None;
            }
            let desc = c["description"].as_str().unwrap_or("");
            let from_file = desc.trim_end().ends_with("(user)")
                || desc.trim_end().ends_with("(project)")
                || desc.contains("(plugin");
            let skill = desc.to_lowercase().contains("skill") && from_file;
            Some(SlashCommand {
                name,
                description: short(Some(desc), 300),
                hint: short(c["argumentHint"].as_str(), 80),
                source: if skill {
                    "skill"
                } else if from_file {
                    "custom"
                } else {
                    "agent"
                }
                .into(),
                expand: None,
            })
        })
        .collect();
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// An ACP `available_commands_update` → commands.
pub fn from_acp(update: &Value) -> Vec<SlashCommand> {
    update["availableCommands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let name = c["name"].as_str()?.trim_start_matches('/').to_string();
            valid_name(&name).then(|| SlashCommand {
                name,
                description: short(c["description"].as_str(), 300),
                hint: short(c.pointer("/input/hint").and_then(Value::as_str), 80),
                source: "agent".into(),
                expand: None,
            })
        })
        .collect()
}

/// Strips a `---` front matter block and returns (description, body).
fn front_matter(text: &str) -> (Option<String>, String) {
    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let head = &rest[..end];
            let body = rest[end + 4..].trim_start_matches(['\r', '\n']).to_string();
            let desc = head
                .lines()
                .find_map(|l| l.trim().strip_prefix("description:"))
                .map(|d| d.trim().trim_matches('"').to_string());
            return (desc, body);
        }
    }
    (None, text.to_string())
}

/// Commands from files (Codex prompts, Gemini command files), for agents that don't report them.
fn from_files(agent: &str, project: Option<&Path>, homes: &Homes) -> Vec<SlashCommand> {
    let mut out = Vec::new();
    for item in context::scan(project, homes) {
        if item.kind != "command" || !item.exists || !item.agents.iter().any(|a| a == agent) {
            continue;
        }
        let base = item.name.trim_start_matches('/').to_string();
        if agent == "codex" {
            let text = std::fs::read_to_string(&item.path).unwrap_or_default();
            if text.len() > 100_000 {
                continue;
            }
            let (desc, body) = front_matter(&text);
            let name = if base.starts_with("prompts:") {
                base.clone()
            } else {
                format!("prompts:{base}")
            };
            if valid_name(&name) {
                out.push(SlashCommand {
                    name,
                    description: short(desc.as_deref().or(item.description.as_deref()), 300),
                    hint: None,
                    source: "custom".into(),
                    expand: Some(body),
                });
            }
        } else if valid_name(&base) {
            out.push(SlashCommand {
                name: base,
                description: item.description.clone(),
                hint: None,
                source: "custom".into(),
                expand: None,
            });
        }
    }
    out
}

/// What `/` offers in this session.
pub fn list(db: &Db, session: &SessionRow, homes: &Homes) -> Vec<SlashCommand> {
    let dir = db.session_dir(session).ok().flatten();
    let project = dir.as_deref().map(Path::new).filter(|d| d.is_dir());
    match session.provider.as_str() {
        "claude-code" => {
            if let Some(c) = cached(&session.id) {
                return c;
            }
            let key = format!("dir:{}", dir.clone().unwrap_or_default());
            if let Some(c) = cached(&key) {
                return c;
            }
            let probe_dir = project
                .map(Path::to_path_buf)
                .unwrap_or_else(std::env::temp_dir);
            match crate::models::claude_initialize(&probe_dir) {
                Ok(init) => {
                    let list = from_claude(&init);
                    remember(&key, list.clone());
                    list
                }
                Err(_) => from_files("claude-code", project, homes),
            }
        }
        "gemini-cli" => {
            cached(&session.id).unwrap_or_else(|| from_files("gemini-cli", project, homes))
        }
        "codex" => from_files("codex", project, homes),
        _ => cached(&session.id).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_claude_and_acp_command_lists() {
        let init = json!({ "commands": [
            { "name": "compact", "description": "Clear conversation history but keep a summary", "argumentHint": "<optional custom summarization instructions>" },
            { "name": "deploy-check", "description": "Check the release (project)", "argumentHint": "" },
            { "name": "pdf-tools", "description": "Use this skill for PDFs (user)", "argumentHint": "" },
            { "name": "bad name; rm", "description": "x" },
        ]});
        let c = from_claude(&init);
        assert_eq!(
            c.iter()
                .map(|x| (x.name.as_str(), x.source.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("compact", "agent"),
                ("deploy-check", "custom"),
                ("pdf-tools", "skill")
            ]
        );
        assert_eq!(
            c[0].hint.as_deref(),
            Some("<optional custom summarization instructions>")
        );
        assert_eq!(c[1].hint, None);
        let a = from_acp(
            &json!({ "availableCommands": [{ "name": "memory", "description": "Manage memory", "input": { "hint": "show|add" } }] }),
        );
        assert_eq!(
            (a[0].name.as_str(), a[0].hint.as_deref()),
            ("memory", Some("show|add"))
        );
    }

    #[test]
    fn codex_prompts_put_their_text_in_the_message() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join(".codex/prompts")).unwrap();
        std::fs::write(
            home.join(".codex/prompts/release.md"),
            "---\ndescription: Cut a release\n---\nBump the version and write the changelog.\n",
        )
        .unwrap();
        let homes = Homes {
            home: home.to_path_buf(),
            claude: home.join(".claude"),
            codex: home.join(".codex"),
            gemini: home.join(".gemini"),
        };
        let c = from_files("codex", None, &homes);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "prompts:release");
        assert_eq!(c[0].description.as_deref(), Some("Cut a release"));
        assert_eq!(
            c[0].expand.as_deref(),
            Some("Bump the version and write the changelog.\n")
        );
    }
}
