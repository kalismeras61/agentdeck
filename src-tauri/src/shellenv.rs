//! The environment of the user's login shell, for the agents AgentDeck starts.
//!
//! A Mac app opened from Finder or the Dock doesn't get what `~/.zshrc` and friends set up: the
//! PATH with nvm/Homebrew/asdf tools, API keys and cloud settings exported there, proxies.
//! An agent that works in Terminal should work the same here, so AgentDeck asks the login shell
//! for its environment once (in the background at startup) and starts every agent with it.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ENV: OnceLock<HashMap<String, String>> = OnceLock::new();

const MARKER: &str = "__AGENTDECK_ENV_BEGIN__";
const TIMEOUT: Duration = Duration::from_secs(6);

/// Variables that describe the shell session itself, not the user's setup.
fn skip(key: &str) -> bool {
    matches!(
        key,
        "PWD" | "OLDPWD" | "SHLVL" | "_" | "TERM_SESSION_ID" | "PS1" | "PROMPT" | "CLAUDECODE"
    ) || key.starts_with("AGENTDECK")
        || key.starts_with("CLAUDE_CODE_SESSION")
        || key.starts_with("ZSH_EXECUTION")
}

/// Reads `KEY=value` entries separated by NUL, after the marker.
pub fn parse(out: &[u8]) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(out);
    let Some(start) = text.find(MARKER) else {
        return HashMap::new();
    };
    text[start + MARKER.len()..]
        .split('\0')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            let k = k.trim_start_matches('\n');
            (!k.is_empty() && !k.contains(char::is_whitespace) && !skip(k))
                .then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

#[cfg(unix)]
fn resolve() -> HashMap<String, String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    read_from(&shell, None)
}

#[cfg(unix)]
fn read_from(shell: &str, home: Option<&std::path::Path>) -> HashMap<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(shell);
    if let Some(h) = home {
        cmd.env("HOME", h);
    }
    let Ok(mut child) = cmd
        .args(["-l", "-i", "-c", &format!("printf '{MARKER}'; env -0")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return HashMap::new();
    };
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(30)),
            _ => {
                // A shell profile that waits for input or hangs: go on without it.
                let _ = child.kill();
                let _ = child.wait();
                return HashMap::new();
            }
        }
    }
    parse(&reader.join().unwrap_or_default())
}

#[cfg(not(unix))]
fn resolve() -> HashMap<String, String> {
    HashMap::new()
}

/// Starts reading the login shell's environment in the background.
pub fn start() {
    std::thread::spawn(|| {
        let _ = ENV.set(resolve());
    });
}

/// The login shell's environment, waiting briefly if it's still being read. Empty if it couldn't
/// be read, in which case agents get AgentDeck's own environment as before.
pub fn get() -> &'static HashMap<String, String> {
    static EMPTY: OnceLock<HashMap<String, String>> = OnceLock::new();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(e) = ENV.get() {
            return e;
        }
        if Instant::now() >= deadline || cfg!(test) {
            return EMPTY.get_or_init(HashMap::new);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Applies the shell environment to a command (PATH is handled by `augmented_path`).
pub fn apply(cmd: &mut std::process::Command) {
    for (k, v) in get() {
        if k != "PATH" {
            cmd.env(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shell_environment_after_the_marker() {
        let out = b"Welcome to zsh!\n__AGENTDECK_ENV_BEGIN__PATH=/opt/homebrew/bin:/usr/bin\0GEMINI_API_KEY=abc=def\0PWD=/Users/x\0SHLVL=2\0MULTI=line one\nline two\0bad key=1\0";
        let env = parse(out);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(env.get("GEMINI_API_KEY").map(String::as_str), Some("abc=def"));
        assert_eq!(env.get("MULTI").map(String::as_str), Some("line one\nline two"));
        assert!(!env.contains_key("PWD") && !env.contains_key("SHLVL") && !env.contains_key("bad key"));
        assert!(parse(b"no marker here").is_empty());
    }

    /// Reads what an interactive login shell sets up, as a Mac app opened from Finder would.
    #[test]
    #[cfg(unix)]
    fn picks_up_variables_from_the_shell_profile() {
        let home = std::env::temp_dir().join(format!("agentdeck-shellenv-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".bashrc"), "echo 'hello from rc'\nexport FROM_RC=\"set in rc\"\nexport PATH=/opt/fromrc/bin:$PATH\n").unwrap();
        std::fs::write(home.join(".bash_profile"), ". ~/.bashrc\n").unwrap();
        let env = read_from("/bin/bash", Some(&home));
        assert_eq!(env.get("FROM_RC").map(String::as_str), Some("set in rc"));
        assert!(env.get("PATH").is_some_and(|p| p.starts_with("/opt/fromrc/bin")));
        assert!(!env.contains_key("PWD"));
        let _ = std::fs::remove_dir_all(&home);
    }
}
