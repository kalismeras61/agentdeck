//! First-run setup: which agent CLIs are on this Mac, how to install the missing ones and how to
//! sign in. Installing and signing in run the agents' own official commands in macOS Terminal,
//! where you can see and answer them; AgentDeck only ever runs the fixed commands listed here.

use crate::db::Db;
use crate::providers::{self, ProviderProbe};
use serde::Serialize;
use std::path::PathBuf;

pub const WELCOME_KEY: &str = "welcome:done";
pub const AGY_ENABLED_KEY: &str = "agy:enabled";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetup {
    pub id: &'static str,
    pub name: &'static str,
    pub maker: &'static str,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
    /// The command "Install" runs in Terminal, when there's one that works here.
    pub install: Option<String>,
    /// What to install first when there's no install command yet (Node.js for npm packages).
    pub needs: Option<String>,
    pub sign_in: Option<String>,
    /// Signed in, signed out, or unknown (the agent has no way to ask without signing in).
    pub signed_in: Option<bool>,
    pub account: &'static str,
    pub docs: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub agents: Vec<AgentSetup>,
    pub antigravity_enabled: bool,
    pub welcome_done: bool,
}

struct Known {
    id: &'static str,
    name: &'static str,
    maker: &'static str,
    account: &'static str,
    docs: &'static str,
}

const AGENTS: [Known; 5] = [
    Known { id: "claude-code", name: "Claude Code", maker: "Anthropic", account: "A Claude Pro, Max, Team or Enterprise plan, or an Anthropic Console account.", docs: "https://code.claude.com/docs/en/setup" },
    Known { id: "codex", name: "Codex", maker: "OpenAI", account: "A ChatGPT Plus, Pro, Business or Enterprise plan, or an OpenAI API key.", docs: "https://github.com/openai/codex" },
    Known { id: "gemini-cli", name: "Gemini CLI", maker: "Google", account: "A Google Workspace or Cloud account, or a Gemini API key (personal Google accounts no longer work).", docs: "https://github.com/google-gemini/gemini-cli" },
    Known { id: "antigravity", name: "Antigravity", maker: "Google", account: "A personal Google account. Experimental in AgentDeck: see the note about Google's terms.", docs: "https://antigravity.google" },
    Known { id: "grok-build", name: "Grok Build", maker: "xAI", account: "An xAI account or API key.", docs: "https://x.ai" },
];

/// The official install command for an agent, picked by what this Mac has (npm or Homebrew).
pub fn install_command(id: &str, npm: bool, brew: bool) -> Option<&'static str> {
    Some(match id {
        "claude-code" => "curl -fsSL https://claude.ai/install.sh | bash",
        "codex" if npm => "npm install -g @openai/codex",
        "codex" if brew => "brew install --cask codex",
        "gemini-cli" if npm => "npm install -g @google/gemini-cli",
        "gemini-cli" if brew => "brew install gemini-cli",
        "antigravity" => "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        _ => return None,
    })
}

pub fn sign_in_command(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude-code" => "claude auth login",
        "codex" => "codex login",
        // Gemini CLI asks how to sign in when it starts; /auth switches accounts later.
        "gemini-cli" => "gemini",
        "grok-build" => "grok",
        "antigravity" => "agy",
        _ => return None,
    })
}

/// Whether Antigravity sessions are on. Off unless you turned it on, except for people who
/// already had Antigravity sessions before the switch existed.
pub fn antigravity_enabled(db: &Db) -> bool {
    match db.meta(AGY_ENABLED_KEY).ok().flatten().as_deref() {
        Some("1") => true,
        Some(_) => false,
        None => db
            .list_sessions()
            .map(|s| s.iter().any(|x| x.provider == "antigravity"))
            .unwrap_or(false),
    }
}

/// Codex says whether it's signed in without starting a conversation; the others don't.
fn codex_signed_in(path: &str) -> Option<bool> {
    let mut cmd = std::process::Command::new(path);
    cmd.args(["login", "status"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::shellenv::apply(&mut cmd);
    let mut child = cmd.spawn().ok()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Some(s.success()),
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(_) => return None,
        }
    }
}

pub fn status(db: &Db) -> SetupStatus {
    let probes: Vec<ProviderProbe> = providers::probe_all();
    let npm = providers::find_binary("npm").is_some();
    let brew = providers::find_binary("brew").is_some();
    let agy = antigravity_enabled(db);
    let agents = AGENTS
        .iter()
        .filter(|k| k.id != "antigravity" || agy)
        .filter_map(|k| {
            let p = probes.iter().find(|p| p.id == k.id)?;
            let installed = p.path.is_some();
            // Grok Build has no public install command to run; it's listed once it's there.
            if k.id == "grok-build" && !installed {
                return None;
            }
            let install = install_command(k.id, npm, brew).map(str::to_owned);
            let needs = (!installed && install.is_none() && matches!(k.id, "codex" | "gemini-cli"))
                .then(|| "Node.js (nodejs.org) or Homebrew (brew.sh)".to_string());
            Some(AgentSetup {
                id: k.id,
                name: k.name,
                maker: k.maker,
                installed,
                version: p.version.clone(),
                path: p.path.clone(),
                error: p.error.clone(),
                install,
                needs,
                sign_in: sign_in_command(k.id).map(str::to_owned),
                signed_in: match (k.id, p.path.as_deref()) {
                    ("codex", Some(path)) => codex_signed_in(path),
                    _ => None,
                },
                account: k.account,
                docs: k.docs,
            })
        })
        .collect();
    SetupStatus {
        agents,
        antigravity_enabled: agy,
        welcome_done: db.meta(WELCOME_KEY).ok().flatten().is_some()
            || db.list_sessions().map(|s| !s.is_empty()).unwrap_or(false),
    }
}

/// Opens macOS Terminal running one of the fixed commands above, so you see what it does and
/// can answer its questions. The command file lives in AgentDeck's cache folder.
pub fn open_in_terminal(cache: PathBuf, agent: &str, action: &str) -> Result<String, String> {
    let npm = providers::find_binary("npm").is_some();
    let brew = providers::find_binary("brew").is_some();
    let cmd = match action {
        "install" => install_command(agent, npm, brew),
        "sign_in" => sign_in_command(agent),
        _ => None,
    }
    .ok_or_else(|| format!("Nothing to {action} for {agent}"))?;
    if !cfg!(target_os = "macos") {
        return Err(format!("Run this in a terminal: {cmd}"));
    }
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let file = cache.join(format!("{agent}-{action}.command"));
    let then = if action == "install" {
        "Installed. Go back to AgentDeck and press Check again."
    } else {
        "Done. Go back to AgentDeck and press Check again."
    };
    let script = format!(
        "#!/bin/zsh -l\nclear\necho \"AgentDeck is running: {cmd}\"\necho\n{cmd}\necho\necho \"{then} You can close this window.\"\n"
    );
    std::fs::write(&file, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    std::process::Command::new("/usr/bin/open")
        .args(["-a", "Terminal"])
        .arg(&file)
        .status()
        .map_err(|e| format!("Couldn't open Terminal: {e}"))?;
    Ok(cmd.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_commands_follow_what_the_mac_has() {
        assert_eq!(install_command("codex", true, true), Some("npm install -g @openai/codex"));
        assert_eq!(install_command("codex", false, true), Some("brew install --cask codex"));
        assert_eq!(install_command("codex", false, false), None);
        assert!(install_command("claude-code", false, false).unwrap().starts_with("curl -fsSL https://claude.ai/"));
        assert_eq!(install_command("grok-build", true, true), None);
        assert_eq!(install_command("rm -rf", true, true), None);
    }

    #[test]
    fn antigravity_is_off_unless_turned_on_or_already_used() {
        let db = Db::open_in_memory().unwrap();
        assert!(!antigravity_enabled(&db));
        let p = db.add_project("app", "/tmp/app").unwrap();
        db.create_session(&p.id, "antigravity", "default", "x").unwrap();
        assert!(antigravity_enabled(&db), "people already using it keep it");
        db.set_meta(AGY_ENABLED_KEY, "0").unwrap();
        assert!(!antigravity_enabled(&db));
        db.set_meta(AGY_ENABLED_KEY, "1").unwrap();
        assert!(antigravity_enabled(&db));
    }
}
