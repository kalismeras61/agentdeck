//! Terminals: the user's own login shell in a project folder, through a pseudo-terminal.
//!
//! The frontend can only open a shell in a folder that is already a project in AgentDeck,
//! write keystrokes to it, resize it and close it. Output is streamed as UTF-8 text events and
//! the last 256 KB is kept so a terminal reattaches with its scrollback after a view switch.
//! Terminals end when AgentDeck quits.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

const MAX_TERMINALS: usize = 8;
const SCROLLBACK_BYTES: usize = 256 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermOutput {
    pub term_id: String,
    pub data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermExit {
    pub term_id: String,
    pub code: Option<u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermInfo {
    pub term_id: String,
    pub cwd: String,
    pub shell: String,
    pub running: bool,
    /// Recent output, so a reattached view shows what was already there.
    pub scrollback: String,
}

pub enum TermEvent {
    Output(TermOutput),
    Exit(TermExit),
}

pub type TermSink = Arc<dyn Fn(TermEvent) + Send + Sync>;

struct Term {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    cwd: String,
    shell: String,
    scrollback: Mutex<String>,
    running: Mutex<bool>,
}

pub struct Terminals {
    sink: TermSink,
    terms: Mutex<HashMap<String, Arc<Term>>>,
}

/// Splits `buf` into the longest valid UTF-8 prefix and the bytes of an unfinished character.
fn take_utf8(buf: &mut Vec<u8>) -> String {
    match std::str::from_utf8(buf) {
        Ok(s) => {
            let s = s.to_string();
            buf.clear();
            s
        }
        Err(e) if e.error_len().is_none() => {
            // Incomplete character at the end: keep it for the next read.
            let valid = e.valid_up_to();
            let s = String::from_utf8_lossy(&buf[..valid]).into_owned();
            buf.drain(..valid);
            s
        }
        Err(_) => {
            let s = String::from_utf8_lossy(buf).into_owned();
            buf.clear();
            s
        }
    }
}

fn login_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| std::path::Path::new(s).exists())
        .unwrap_or_else(|| {
            ["/bin/zsh", "/bin/bash", "/bin/sh"]
                .into_iter()
                .find(|s| std::path::Path::new(s).exists())
                .unwrap_or("/bin/sh")
                .to_string()
        })
}

impl Terminals {
    pub fn new(sink: TermSink) -> Self {
        Self {
            sink,
            terms: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, id: &str) -> Result<Arc<Term>, String> {
        self.terms
            .lock()
            .map_err(|_| "lock poisoned")?
            .get(id)
            .cloned()
            .ok_or_else(|| "This terminal is closed".to_string())
    }

    /// Opens the user's login shell in `cwd`, which the caller has checked is a project folder.
    pub fn open(&self, cwd: &str, cols: u16, rows: u16) -> Result<TermInfo, String> {
        if self.terms.lock().map(|t| t.len()).unwrap_or(0) >= MAX_TERMINALS {
            return Err(format!(
                "At most {MAX_TERMINALS} terminals can be open. Close one first."
            ));
        }
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: rows.clamp(2, 500),
                cols: cols.clamp(10, 1000),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Couldn't open a terminal: {e}"))?;
        let shell = login_shell();
        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("PATH", crate::claude::augmented_path());
        cmd.env("AGENTDECK_TERMINAL", "1");
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Couldn't start {shell}: {e}"))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let id = format!("t-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let term = Arc::new(Term {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            cwd: cwd.to_string(),
            shell: shell.clone(),
            scrollback: Mutex::new(String::new()),
            running: Mutex::new(true),
        });
        self.terms
            .lock()
            .map_err(|_| "lock poisoned")?
            .insert(id.clone(), term.clone());

        let sink = self.sink.clone();
        let tid = id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            let mut pending = Vec::<u8>::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                        let text = take_utf8(&mut pending);
                        if text.is_empty() {
                            continue;
                        }
                        if let Ok(mut sb) = term.scrollback.lock() {
                            sb.push_str(&text);
                            if sb.len() > SCROLLBACK_BYTES {
                                let mut cut = sb.len() - SCROLLBACK_BYTES;
                                while !sb.is_char_boundary(cut) {
                                    cut += 1;
                                }
                                sb.drain(..cut);
                            }
                        }
                        sink(TermEvent::Output(TermOutput {
                            term_id: tid.clone(),
                            data: text,
                        }));
                    }
                }
            }
            let code = term
                .child
                .lock()
                .ok()
                .and_then(|mut c| c.wait().ok())
                .map(|s| s.exit_code());
            if let Ok(mut r) = term.running.lock() {
                *r = false;
            }
            sink(TermEvent::Exit(TermExit { term_id: tid, code }));
        });
        Ok(TermInfo {
            term_id: id,
            cwd: cwd.to_string(),
            shell,
            running: true,
            scrollback: String::new(),
        })
    }

    pub fn info(&self, id: &str) -> Result<TermInfo, String> {
        let t = self.get(id)?;
        Ok(TermInfo {
            term_id: id.to_string(),
            cwd: t.cwd.clone(),
            shell: t.shell.clone(),
            running: t.running.lock().map(|r| *r).unwrap_or(false),
            scrollback: t.scrollback.lock().map(|s| s.clone()).unwrap_or_default(),
        })
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        if data.len() > 1024 * 1024 {
            return Err("That's too much to paste at once (over 1 MB)".into());
        }
        let t = self.get(id)?;
        let mut w = t.writer.lock().map_err(|_| "lock poisoned")?;
        w.write_all(data.as_bytes())
            .and_then(|_| w.flush())
            .map_err(|e| format!("The terminal isn't accepting input: {e}"))
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let t = self.get(id)?;
        let m = t.master.lock().map_err(|_| "lock poisoned")?;
        m.resize(PtySize {
            rows: rows.clamp(2, 500),
            cols: cols.clamp(10, 1000),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())
    }

    pub fn close(&self, id: &str) {
        let t = self.terms.lock().ok().and_then(|mut m| m.remove(id));
        if let Some(t) = t {
            if let Ok(mut c) = t.child.lock() {
                let _ = c.kill();
            }
        }
    }

    pub fn close_all(&self) {
        let ids: Vec<String> = self
            .terms
            .lock()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        for id in ids {
            self.close(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn utf8_split_across_reads_is_kept_whole() {
        let mut buf = "héllo".as_bytes()[..2].to_vec(); // "h" + first byte of "é"
        assert_eq!(take_utf8(&mut buf), "h");
        assert_eq!(buf.len(), 1);
        buf.extend_from_slice(&"héllo".as_bytes()[2..]);
        assert_eq!(take_utf8(&mut buf), "éllo");
        assert!(buf.is_empty());
    }

    #[test]
    fn runs_a_shell_in_the_folder_and_keeps_scrollback() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let terms = Terminals::new(Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }));
        let info = terms.open(&dir.path().to_string_lossy(), 80, 24).unwrap();
        terms
            .write(&info.term_id, "echo agentdeck-$((40+2)) && pwd\r")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut out = String::new();
        while Instant::now() < deadline
            && !(out.contains("agentdeck-42")
                && out.contains(&*dir.path().file_name().unwrap().to_string_lossy()))
        {
            if let Ok(TermEvent::Output(o)) = rx.recv_timeout(Duration::from_millis(100)) {
                out.push_str(&o.data);
            }
        }
        assert!(
            out.contains("agentdeck-42"),
            "shell ran the command: {out:?}"
        );
        terms.resize(&info.term_id, 120, 40).unwrap();
        assert!(terms
            .info(&info.term_id)
            .unwrap()
            .scrollback
            .contains("agentdeck-42"));
        terms.write(&info.term_id, "exit\r").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut exited = false;
        while Instant::now() < deadline && !exited {
            if let Ok(TermEvent::Exit(_)) = rx.recv_timeout(Duration::from_millis(100)) {
                exited = true;
            }
        }
        assert!(exited);
        assert!(!terms.info(&info.term_id).unwrap().running);
        terms.close(&info.term_id);
        assert!(terms.info(&info.term_id).is_err());
    }
}
