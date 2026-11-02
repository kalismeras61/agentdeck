//! Git for the Git view and for sessions that work in their own worktree.
//!
//! Everything runs the user's `git` with fixed arguments (never a shell); revisions, branch
//! names and paths coming from the interface are checked first. Nothing here pushes, rewrites
//! history or deletes a branch unless a command says so by name, and those are only called after
//! the user confirms in the interface.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

fn git(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=off", "-c", "color.ui=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PATH", crate::claude::augmented_path())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { format!("git {} failed", args.first().unwrap_or(&"")) } else { err })
    }
}

fn git_ok(dir: &str, args: &[&str]) -> bool {
    git(dir, args).is_ok()
}

/// A revision from the interface: a commit hash or a plain ref name.
pub fn check_rev(rev: &str) -> Result<(), String> {
    let ok = !rev.is_empty()
        && rev.len() <= 200
        && !rev.starts_with('-')
        && !rev.contains("..")
        && !rev.contains("@{")
        && rev.chars().all(|c| c.is_ascii_alphanumeric() || "._/-^~".contains(c));
    if ok { Ok(()) } else { Err(format!("\"{rev}\" isn't a revision")) }
}

/// A path inside the repository, relative and without `..`.
pub fn check_rel(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    let ok = !path.is_empty()
        && !path.starts_with('-')
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    if ok { Ok(()) } else { Err(format!("\"{path}\" isn't a path in the repository")) }
}

/// The repository's top folder, if `dir` is inside one.
pub fn root(dir: &str) -> Option<String> {
    git(dir, &["rev-parse", "--show-toplevel"]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub hash: String,
    pub current: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The worktree that has it checked out.
    pub worktree: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub hash: String,
    pub main: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    /// "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflict"
    pub change: String,
    pub staged: bool,
    pub from: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub root: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub branches: Vec<Branch>,
    pub remotes: Vec<Branch>,
    pub tags: Vec<String>,
    pub worktrees: Vec<Worktree>,
    pub status: Vec<FileStatus>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    pub refs: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub change: String,
    pub from: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilePair {
    pub old: String,
    pub new: String,
    pub binary: bool,
}

fn parse_track(t: &str) -> (u32, u32) {
    let num = |key: &str| {
        t.split([',', '[', ']'])
            .map(str::trim)
            .find_map(|p| p.strip_prefix(key).and_then(|n| n.trim().parse().ok()))
            .unwrap_or(0)
    };
    (num("ahead "), num("behind "))
}

fn refs(dir: &str, pattern: &str) -> Vec<Branch> {
    let fmt = "%(refname:short)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(HEAD)%00%(worktreepath)";
    git(dir, &["for-each-ref", &format!("--format={fmt}"), "--sort=-committerdate", pattern])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\0').collect();
            if f.len() < 6 || f[0].ends_with("/HEAD") {
                return None;
            }
            let (ahead, behind) = parse_track(f[3]);
            Some(Branch {
                name: f[0].to_string(),
                hash: f[1].to_string(),
                upstream: Some(f[2].to_string()).filter(|s| !s.is_empty()),
                ahead,
                behind,
                current: f[4] == "*",
                worktree: Some(f[5].to_string()).filter(|s| !s.is_empty()),
            })
        })
        .collect()
}

pub fn parse_worktrees(porcelain: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    for (i, block) in porcelain.split("\n\n").filter(|b| !b.trim().is_empty()).enumerate() {
        let mut w = Worktree { path: String::new(), branch: None, hash: String::new(), main: i == 0, locked: false, prunable: false };
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                w.path = p.to_string();
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                w.hash = h.to_string();
            } else if let Some(b) = line.strip_prefix("branch ") {
                w.branch = Some(b.trim_start_matches("refs/heads/").to_string());
            } else if line.starts_with("locked") {
                w.locked = true;
            } else if line.starts_with("prunable") {
                w.prunable = true;
            }
        }
        if !w.path.is_empty() {
            out.push(w);
        }
    }
    out
}

pub fn worktrees(dir: &str) -> Vec<Worktree> {
    parse_worktrees(&git(dir, &["worktree", "list", "--porcelain"]).unwrap_or_default())
}

/// `git status --porcelain=v1 -z`.
pub fn parse_status(z: &str) -> Vec<FileStatus> {
    let mut out = Vec::new();
    let mut parts = z.split('\0').filter(|s| !s.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry.as_bytes()[0] as char, entry.as_bytes()[1] as char);
        let path = entry[3..].to_string();
        let from = if x == 'R' || x == 'C' || y == 'R' { parts.next().map(str::to_owned) } else { None };
        let change = match (x, y) {
            ('?', '?') => "untracked",
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => "conflict",
            ('R', _) | (_, 'R') => "renamed",
            ('A', _) => "added",
            ('D', _) | (_, 'D') => "deleted",
            _ => "modified",
        };
        out.push(FileStatus { path, change: change.into(), staged: x != ' ' && x != '?', from });
    }
    out
}

pub fn status(dir: &str) -> Vec<FileStatus> {
    parse_status(&git(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).unwrap_or_default())
}

pub fn overview(dir: &str) -> Result<Overview, String> {
    let root = root(dir).ok_or("This folder isn't a git repository")?;
    let head = git(&root, &["rev-parse", "--verify", "-q", "HEAD"]).ok().map(|s| s.trim().to_string());
    let branch = git(&root, &["symbolic-ref", "-q", "--short", "HEAD"]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let tags = git(&root, &["tag", "--sort=-creatordate"]).unwrap_or_default().lines().take(200).map(str::to_owned).collect();
    Ok(Overview {
        branches: refs(&root, "refs/heads"),
        remotes: refs(&root, "refs/remotes"),
        tags,
        worktrees: worktrees(&root),
        status: status(dir),
        root,
        head,
        branch,
    })
}

const SEP: char = '\u{1f}';

pub fn parse_log(z: &str) -> Vec<Commit> {
    z.split('\u{1e}')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches('\n');
            let f: Vec<&str> = rec.split(SEP).collect();
            if f.len() < 7 || f[0].len() < 7 {
                return None;
            }
            Some(Commit {
                hash: f[0].to_string(),
                parents: f[1].split_whitespace().map(str::to_owned).collect(),
                author: f[2].to_string(),
                email: f[3].to_string(),
                time: f[4].trim().parse::<i64>().unwrap_or(0) * 1000,
                subject: f[5].to_string(),
                refs: f[6].split(", ").map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect(),
            })
        })
        .collect()
}

/// Commits newest first, all branches, in an order that keeps a branch's commits together.
pub fn log(dir: &str, skip: usize, limit: usize) -> Result<Vec<Commit>, String> {
    let root = root(dir).ok_or("This folder isn't a git repository")?;
    if git(&root, &["rev-parse", "--verify", "-q", "HEAD"]).is_err() && refs(&root, "refs/heads").is_empty() {
        return Ok(Vec::new());
    }
    let fmt = format!("--format=%H{SEP}%P{SEP}%an{SEP}%ae{SEP}%at{SEP}%s{SEP}%D\u{1e}");
    let out = git(
        &root,
        &["log", "--all", "--topo-order", "--decorate=short", &fmt, &format!("--skip={skip}"), &format!("--max-count={}", limit.min(2000))],
    )?;
    Ok(parse_log(&out))
}

fn name_status(out: &str) -> Vec<ChangedFile> {
    let mut v = Vec::new();
    let mut parts = out.split('\0').filter(|s| !s.is_empty());
    while let Some(code) = parts.next() {
        let kind = code.chars().next().unwrap_or('M');
        let (from, path) = if kind == 'R' || kind == 'C' {
            let a = parts.next().unwrap_or_default().to_string();
            (Some(a), parts.next().unwrap_or_default().to_string())
        } else {
            (None, parts.next().unwrap_or_default().to_string())
        };
        let change = match kind {
            'A' => "added",
            'D' => "deleted",
            'R' => "renamed",
            'C' => "added",
            _ => "modified",
        };
        v.push(ChangedFile { path, change: change.into(), from });
    }
    v
}

/// Files a commit changed, compared with its first parent.
pub fn commit_files(dir: &str, hash: &str) -> Result<Vec<ChangedFile>, String> {
    check_rev(hash)?;
    let root = root(dir).ok_or("This folder isn't a git repository")?;
    let out = git(&root, &["diff-tree", "--no-commit-id", "-r", "-z", "-M", "--name-status", "--root", "--first-parent", "-m", hash])?;
    Ok(name_status(&out))
}

fn show_file(dir: &str, rev: &str, path: &str) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["show", &format!("{rev}:./{path}")])
        .env("PATH", crate::claude::augmented_path())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

fn text_of(bytes: Option<Vec<u8>>) -> (String, bool) {
    match bytes {
        None => (String::new(), false),
        Some(b) if b.len() > MAX_FILE_BYTES || b.contains(&0) => (String::new(), true),
        Some(b) => (String::from_utf8_lossy(&b).into_owned(), false),
    }
}

/// A file before and after: `new_rev` "WORKTREE" means the file on disk in `dir`.
pub fn file_pair(dir: &str, old_rev: &str, new_rev: &str, path: &str, from: Option<&str>) -> Result<FilePair, String> {
    check_rel(path)?;
    if let Some(f) = from {
        check_rel(f)?;
    }
    let old_path = from.unwrap_or(path);
    let old = if old_rev.is_empty() {
        None
    } else {
        check_rev(old_rev)?;
        show_file(dir, old_rev, old_path)
    };
    let new = if new_rev == "WORKTREE" {
        let p = PathBuf::from(dir).join(path);
        std::fs::metadata(&p).ok().filter(|m| m.is_file()).and_then(|_| std::fs::read(&p).ok())
    } else {
        check_rev(new_rev)?;
        show_file(dir, new_rev, path)
    };
    let (o, ob) = text_of(old);
    let (n, nb) = text_of(new);
    Ok(FilePair { old: o, new: n, binary: ob || nb })
}

/* ---------------------------------------------------------------- worktrees for sessions */

fn slug(s: &str) -> String {
    let mut out: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').chars().take(40).collect::<String>();
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "session".into() } else { out }
}

/// Where a project's worktrees go: next to it, `<project>.worktrees/<name>`.
pub fn worktree_home(repo_root: &str) -> PathBuf {
    let r = Path::new(repo_root);
    let name = r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
    r.parent().unwrap_or(r).join(format!("{name}.worktrees"))
}

/// Makes a worktree on a new branch `agentdeck/<name>` from the current commit.
pub fn add_worktree(project_dir: &str, name: &str) -> Result<(String, String), String> {
    let root = root(project_dir).ok_or("This project isn't a git repository. Run `git init` in it (or pick another project) to work in a worktree.")?;
    if git(&root, &["rev-parse", "--verify", "-q", "HEAD"]).is_err() {
        return Err("This repository has no commits yet. Make a first commit, then start a session in a worktree.".into());
    }
    let base = slug(name);
    let home = worktree_home(&root);
    std::fs::create_dir_all(&home).map_err(|e| format!("Couldn't make {}: {e}", home.display()))?;
    for n in 0..100 {
        let tag = if n == 0 { base.clone() } else { format!("{base}-{}", n + 1) };
        let branch = format!("agentdeck/{tag}");
        let path = home.join(&tag);
        if path.exists() || git_ok(&root, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")]) {
            continue;
        }
        let p = path.to_string_lossy().into_owned();
        git(&root, &["worktree", "add", "-b", &branch, &p, "HEAD"])?;
        return Ok((p, branch));
    }
    Err("Couldn't find a free name for the worktree".into())
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeState {
    pub path: String,
    pub branch: String,
    /// The branch it merges into (what the project folder has checked out).
    pub base: Option<String>,
    pub exists: bool,
    /// Commits on the worktree's branch that the base doesn't have.
    pub ahead: u32,
    /// Commits on the base since the worktree branched off.
    pub behind: u32,
    pub uncommitted: Vec<FileStatus>,
    /// Everything the worktree changed compared with the base, committed or not.
    pub files: Vec<ChangedFile>,
}

pub fn worktree_state(project_dir: &str, path: &str, branch: &str) -> Result<WorktreeState, String> {
    check_rev(branch)?;
    let root = root(project_dir).ok_or("This project isn't a git repository")?;
    let base = git(&root, &["symbolic-ref", "-q", "--short", "HEAD"]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let exists = Path::new(path).is_dir();
    let (mut ahead, mut behind, mut files) = (0, 0, Vec::new());
    if let Some(b) = &base {
        if let Ok(c) = git(&root, &["rev-list", "--left-right", "--count", &format!("{b}...{branch}")]) {
            let mut it = c.split_whitespace().filter_map(|n| n.parse::<u32>().ok());
            behind = it.next().unwrap_or(0);
            ahead = it.next().unwrap_or(0);
        }
        if exists {
            // Committed changes on the branch plus whatever is still uncommitted in the worktree.
            if let Ok(mb) = git(&root, &["merge-base", b, branch]) {
                files = name_status(&git(path, &["diff", "-z", "-M", "--name-status", mb.trim()]).unwrap_or_default());
            }
        }
    }
    let uncommitted = if exists { status(path) } else { Vec::new() };
    for u in uncommitted.iter().filter(|u| u.change == "untracked") {
        if !files.iter().any(|f| f.path == u.path) {
            files.push(ChangedFile { path: u.path.clone(), change: "added".into(), from: None });
        }
    }
    Ok(WorktreeState { path: path.into(), branch: branch.into(), base, exists, ahead, behind, uncommitted, files })
}

/// Commits everything in the worktree (when the user asked to), then merges its branch into the
/// project folder's branch with a merge commit. On a conflict nothing is left half-merged: the
/// merge is undone and the conflicting files are returned.
pub fn merge_worktree(project_dir: &str, path: &str, branch: &str, message: &str) -> Result<MergeResult, String> {
    check_rev(branch)?;
    let root = root(project_dir).ok_or("This project isn't a git repository")?;
    let base = git(&root, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("The project folder isn't on a branch (detached HEAD); check out a branch there first")?;
    if base == branch {
        return Err("The project folder has the worktree's branch checked out".into());
    }
    let main_dirty: Vec<FileStatus> = status(&root).into_iter().filter(|f| f.change != "untracked").collect();
    if !main_dirty.is_empty() {
        return Err(format!(
            "The project folder has uncommitted changes ({}). Commit or stash them first, so the merge can't mix with them.",
            main_dirty.iter().take(3).map(|f| f.path.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    if Path::new(path).is_dir() && !status(path).is_empty() {
        git(path, &["add", "-A"])?;
        git(path, &["-c", "user.useConfigOnly=false", "commit", "-q", "-m", message])
            .map_err(|e| format!("Couldn't commit the worktree's changes: {e}"))?;
    }
    let ahead = git(&root, &["rev-list", "--count", &format!("{base}..{branch}")]).unwrap_or_default();
    if ahead.trim() == "0" {
        return Ok(MergeResult { merged: false, base, conflicts: Vec::new(), message: "Nothing to merge: the branch has no new commits.".into() });
    }
    match git(&root, &["merge", "--no-ff", "--no-edit", "-m", &format!("Merge {branch}"), branch]) {
        Ok(_) => Ok(MergeResult { merged: true, base: base.clone(), conflicts: Vec::new(), message: format!("Merged {branch} into {base}.") }),
        Err(e) => {
            let conflicts: Vec<String> = git(&root, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default().lines().map(str::to_owned).collect();
            let _ = git(&root, &["merge", "--abort"]);
            if conflicts.is_empty() {
                Err(e)
            } else {
                Ok(MergeResult { merged: false, base, conflicts, message: "The branches change the same lines; nothing was merged.".into() })
            }
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub merged: bool,
    pub base: String,
    pub conflicts: Vec<String>,
    pub message: String,
}

/// Removes a session's worktree folder (and, when asked, its branch). Uncommitted changes in it
/// are lost, which the interface says before calling this.
pub fn remove_worktree(project_dir: &str, path: &str, branch: &str, delete_branch: bool) -> Result<(), String> {
    check_rev(branch)?;
    let root = root(project_dir).ok_or("This project isn't a git repository")?;
    let known = worktrees(&root).into_iter().any(|w| !w.main && w.path == path);
    if known {
        git(&root, &["worktree", "remove", "--force", path])?;
    } else {
        let _ = git(&root, &["worktree", "prune"]);
    }
    if delete_branch && branch.starts_with("agentdeck/") {
        git(&root, &["branch", "-D", branch])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> (tempfile::TempDir, String) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("app");
        std::fs::create_dir(&p).unwrap();
        let s = p.to_string_lossy().into_owned();
        let g = |a: &[&str]| git(&s, a).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "Tester"]);
        std::fs::write(p.join("a.txt"), "one\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "first"]);
        (d, s)
    }

    #[test]
    fn checks_what_comes_from_the_interface() {
        assert!(check_rev("a1b2c3d").is_ok() && check_rev("agentdeck/fix-login").is_ok() && check_rev("HEAD~1").is_ok());
        for bad in ["--output=/tmp/x", "a..b", "main@{1}", "x;rm", ""] {
            assert!(check_rev(bad).is_err(), "{bad}");
        }
        assert!(check_rel("src/a.rs").is_ok());
        for bad in ["../x", "/etc/passwd", "-p", "a/../../b", ""] {
            assert!(check_rel(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_history_branches_and_status() {
        let (_d, r) = repo();
        let g = |a: &[&str]| git(&r, a).unwrap();
        g(&["checkout", "-qb", "feature"]);
        std::fs::write(Path::new(&r).join("b.txt"), "two\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "add b"]);
        g(&["checkout", "-q", "main"]);
        g(&["merge", "-q", "--no-ff", "-m", "Merge feature", "feature"]);
        std::fs::write(Path::new(&r).join("a.txt"), "one\nmore\n").unwrap();
        std::fs::write(Path::new(&r).join("new file.txt"), "x").unwrap();

        let log = log(&r, 0, 50).unwrap();
        assert_eq!(log.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["Merge feature", "add b", "first"]);
        assert_eq!(log[0].parents.len(), 2);
        assert!(log[0].refs.iter().any(|x| x.contains("main")));
        assert_eq!(log[0].author, "Tester");

        let o = overview(&r).unwrap();
        assert_eq!(o.branch.as_deref(), Some("main"));
        assert!(o.branches.iter().any(|b| b.name == "feature" && !b.current));
        assert_eq!(o.worktrees.len(), 1);
        let st: Vec<_> = o.status.iter().map(|f| (f.path.as_str(), f.change.as_str())).collect();
        assert!(st.contains(&("a.txt", "modified")) && st.contains(&("new file.txt", "untracked")), "{st:?}");

        let files = commit_files(&r, &log[0].hash).unwrap();
        assert_eq!(files, vec![ChangedFile { path: "b.txt".into(), change: "added".into(), from: None }]);
        let pair = file_pair(&r, "HEAD", "WORKTREE", "a.txt", None).unwrap();
        assert_eq!((pair.old.as_str(), pair.new.as_str()), ("one\n", "one\nmore\n"));
        assert!(file_pair(&r, "HEAD", "WORKTREE", "../outside", None).is_err());
    }

    #[test]
    fn a_session_worktree_is_made_merged_and_removed() {
        let (_d, r) = repo();
        let (wt, branch) = add_worktree(&r, "Fix the login bug!").unwrap();
        assert_eq!(branch, "agentdeck/fix-the-login-bug");
        assert!(wt.ends_with("app.worktrees/fix-the-login-bug"), "{wt}");
        let (_, b2) = add_worktree(&r, "Fix the login bug!").unwrap();
        assert_eq!(b2, "agentdeck/fix-the-login-bug-2");

        // The agent changes a file and adds one, without committing.
        std::fs::write(Path::new(&wt).join("a.txt"), "one\nfixed\n").unwrap();
        std::fs::write(Path::new(&wt).join("c.txt"), "new\n").unwrap();
        let st = worktree_state(&r, &wt, &branch).unwrap();
        assert_eq!(st.base.as_deref(), Some("main"));
        assert_eq!(st.uncommitted.len(), 2);
        let mut f: Vec<_> = st.files.iter().map(|x| (x.path.as_str(), x.change.as_str())).collect();
        f.sort();
        assert_eq!(f, [("a.txt", "modified"), ("c.txt", "added")]);
        // The project folder was untouched.
        assert_eq!(std::fs::read_to_string(Path::new(&r).join("a.txt")).unwrap(), "one\n");

        // An uncommitted change in the project folder blocks the merge.
        std::fs::write(Path::new(&r).join("a.txt"), "dirty\n").unwrap();
        assert!(merge_worktree(&r, &wt, &branch, "Fix login").unwrap_err().contains("uncommitted"));
        git(&r, &["checkout", "--", "a.txt"]).unwrap();

        let m = merge_worktree(&r, &wt, &branch, "Fix login").unwrap();
        assert!(m.merged, "{}", m.message);
        assert_eq!(std::fs::read_to_string(Path::new(&r).join("a.txt")).unwrap(), "one\nfixed\n");
        let st = worktree_state(&r, &wt, &branch).unwrap();
        assert_eq!((st.ahead, st.uncommitted.len()), (0, 0));

        remove_worktree(&r, &wt, &branch, true).unwrap();
        assert!(!Path::new(&wt).exists());
        assert!(!git_ok(&r, &["rev-parse", "--verify", "-q", "refs/heads/agentdeck/fix-the-login-bug"]));
    }

    #[test]
    fn a_conflicting_merge_is_undone_and_reported() {
        let (_d, r) = repo();
        let (wt, branch) = add_worktree(&r, "edit a").unwrap();
        std::fs::write(Path::new(&wt).join("a.txt"), "from the agent\n").unwrap();
        std::fs::write(Path::new(&r).join("a.txt"), "from you\n").unwrap();
        git(&r, &["commit", "-qam", "my change"]).unwrap();
        let m = merge_worktree(&r, &wt, &branch, "agent change").unwrap();
        assert!(!m.merged);
        assert_eq!(m.conflicts, ["a.txt"]);
        assert!(status(&r).is_empty(), "nothing half-merged is left behind");
        assert_eq!(std::fs::read_to_string(Path::new(&r).join("a.txt")).unwrap(), "from you\n");
    }
}
