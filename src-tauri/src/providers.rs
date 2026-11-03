//! Read-only detection of agent CLIs installed on this machine.
//!
//! Looks for known binary names on PATH plus common install locations (GUI apps on macOS
//! don't inherit the shell PATH), then runs `<binary> --version` with a fixed argument,
//! no shell, closed stdin, a 5 second timeout and a bounded read. Nothing else is executed
//! and no credentials are read.

use serde::Serialize;
use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const KNOWN: [(&str, &str); 6] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("gemini-cli", "gemini"),
    ("grok-build", "grok"),
    ("kimi-code", "kimi"),
    ("antigravity", "agy"),
];
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_VERSION_BYTES: u64 = 512;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProbe {
    pub id: &'static str,
    pub binary: &'static str,
    pub path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
}

fn candidate_dirs() -> Vec<PathBuf> {
    // The login shell's PATH comes first, so the same `gemini` or `claude` as in Terminal is used.
    let mut dirs: Vec<PathBuf> = env::split_paths(&crate::claude::augmented_path()).collect();
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

pub fn find_binary(binary: &str) -> Option<PathBuf> {
    for dir in candidate_dirs() {
        let p = dir.join(binary);
        if is_executable(&p) {
            return Some(p);
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd"] {
            let pe = p.with_extension(ext);
            if is_executable(&pe) {
                return Some(pe);
            }
        }
    }
    None
}

pub fn read_version(path: &Path) -> Result<String, String> {
    let mut child = Command::new(path)
        .arg("--version")
        .env("PATH", crate::claude::augmented_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn't start it: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > VERSION_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Didn't answer --version within 5 seconds".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut out = String::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout.take(MAX_VERSION_BYTES).read_to_string(&mut out);
    }
    let line: String = out
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    if line.trim().is_empty() {
        Err("Printed no version".into())
    } else {
        Ok(line.trim().to_string())
    }
}

pub fn probe_all() -> Vec<ProviderProbe> {
    KNOWN
        .iter()
        .map(|&(id, binary)| match find_binary(binary) {
            None => ProviderProbe {
                id,
                binary,
                path: None,
                version: None,
                error: None,
            },
            Some(p) => {
                let (version, error) = match read_version(&p) {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                ProviderProbe {
                    id,
                    binary,
                    path: Some(p.display().to_string()),
                    version,
                    error,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_is_not_found() {
        assert!(find_binary("agentdeck-definitely-missing-binary").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn finds_a_standard_binary() {
        assert!(find_binary("sh").is_some());
    }

    #[test]
    fn probes_every_known_provider() {
        let ids: Vec<_> = probe_all().into_iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            vec![
                "claude-code",
                "codex",
                "gemini-cli",
                "grok-build",
                "kimi-code",
                "antigravity"
            ]
        );
    }
}
