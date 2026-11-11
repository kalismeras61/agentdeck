//! Team: who works on which branch of a project, read from the project's own git remote.
//!
//! There is no AgentDeck server. Everything comes from the repository everyone already pushes
//! to:
//! - **Branches**: every branch on the remote, with who committed to it, how far it is from the
//!   base branch and which files it changes (so the interface can show where two branches touch
//!   the same files before anyone merges).
//! - **Presence** (opt-in, per project): a teammate who turns on "Share what I'm working on"
//!   pushes a tiny commit to `refs/agentdeck/presence/<them>` with their name, the branches they
//!   have open and what their agent sessions are doing. It lives outside `refs/heads`, so it's no
//!   branch, never shows up in pull requests and is gone when they turn sharing off.
//! - **Review checkouts**: any teammate's branch can be checked out into its own worktree
//!   (`<project>.worktrees/review-<branch>`), updated with a fast-forward, reviewed by an agent
//!   and removed again; the project folder is never touched.
//!
//! Like `git.rs`, everything runs the user's `git` with fixed arguments, and names from the
//! interface are checked first. What comes from the remote (presence files) is data: it's size
//! limited, parsed and validated, and only ever displayed.

use crate::git::{self, check_rev, ChangedFile, Commit};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const PRESENCE_NS: &str = "refs/agentdeck/presence";
const PRESENCE_FILE: &str = "presence.json";
const MAX_PRESENCE_BYTES: usize = 64 * 1024;
const MAX_BRANCHES: usize = 120;
const MAX_FILES: usize = 400;
const REVIEW_PREFIX: &str = "agentdeck/review-";

/* ------------------------------------------------------------------------------- types */

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub name: String,
    pub email: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub name: String,
    pub email: String,
    pub commits: u32,
    /// Their newest commit on the branch, ms.
    pub last: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamBranch {
    /// The ref to use in git commands: "origin/feature-x", or "feature-x" for a local-only branch.
    pub name: String,
    /// The branch name without the remote: "feature-x".
    pub short: String,
    /// Only on this Mac, never pushed.
    pub local_only: bool,
    pub hash: String,
    /// The newest commit.
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    /// Everyone with commits on the branch that the base doesn't have, most commits first.
    pub authors: Vec<Author>,
    /// Commits the base doesn't have.
    pub ahead: u32,
    /// Commits on the base since the branch forked.
    pub behind: u32,
    /// Files the branch changes compared with where it forked from the base.
    pub files: Vec<String>,
    /// More files than `files` lists.
    pub more_files: bool,
    /// A worktree on this Mac has it (or its local branch) checked out.
    pub checked_out: Option<String>,
    /// The local branch that tracks it, with commits not pushed yet / not pulled yet.
    pub local: Option<String>,
    pub unpushed: u32,
    pub unpulled: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PresenceSession {
    pub title: String,
    /// The agent's name ("Claude Code").
    pub agent: String,
    pub branch: Option<String>,
    /// "idle" | "running" | "waiting" | "error"
    pub status: String,
    pub updated_at: i64,
}

/// What one teammate shares, as stored in their presence ref.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    #[serde(default)]
    pub version: u32,
    pub name: String,
    pub email: String,
    /// The branch the project folder has checked out.
    #[serde(default)]
    pub branch: Option<String>,
    /// Branches checked out in their worktrees.
    #[serde(default)]
    pub worktrees: Vec<String>,
    #[serde(default)]
    pub sessions: Vec<PresenceSession>,
    /// Time spent per branch and day (agents working, waiting, you), the last two weeks.
    #[serde(default)]
    pub time: Vec<crate::time::BranchDay>,
    pub updated_at: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCheckout {
    pub path: String,
    /// The local branch: agentdeck/review-<name>.
    pub branch: String,
    /// The branch it reviews: origin/<name>.
    pub upstream: Option<String>,
    /// Commits the teammate pushed since the checkout was updated.
    pub behind: u32,
    /// Commits made in the checkout itself.
    pub ahead: u32,
    pub dirty: bool,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TeamOverview {
    pub root: String,
    /// The remote everything is read from ("origin"); None: the repository has no remote.
    pub remote: Option<String>,
    pub remote_url: Option<String>,
    /// The branch work merges into ("origin/main").
    pub base: Option<String>,
    /// The project folder's branch.
    pub current: Option<String>,
    pub me: Person,
    pub branches: Vec<TeamBranch>,
    /// Teammates who share what they work on (yourself included when you share).
    pub presence: Vec<Presence>,
    pub reviews: Vec<ReviewCheckout>,
    /// You share what you work on in this project.
    pub sharing: bool,
    /// When the remote was last fetched from here, ms.
    pub fetched_at: Option<i64>,
    /// Why the last fetch or share didn't work (offline, no access…); what's shown is from before.
    pub sync_error: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BranchDetail {
    pub branch: String,
    pub base: String,
    /// Where the branch forked from the base; diffs compare against it.
    pub merge_base: String,
    pub commits: Vec<Commit>,
    pub files: Vec<ChangedFile>,
}

/* ------------------------------------------------------------------------------- helpers */

/// git with something on stdin (hash-object, mktree).
fn git_input(dir: &str, args: &[&str], input: &[u8]) -> Result<String, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PATH", crate::claude::augmented_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("Couldn't write to git")?
        .write_all(input)
        .map_err(|e| format!("Couldn't write to git: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn trimmed(dir: &str, args: &[&str]) -> Option<String> {
    git::git(dir, args)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn ref_exists(dir: &str, r: &str) -> bool {
    git::git(dir, &["rev-parse", "--verify", "-q", r]).is_ok()
}

fn root_of(dir: &str) -> Result<String, String> {
    git::root(dir).ok_or_else(|| "This project isn't a git repository".to_string())
}

/// "origin" when there is one, else the first remote.
pub fn remote_of(root: &str) -> Option<String> {
    let remotes: Vec<String> = git::git(root, &["remote"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    remotes
        .iter()
        .find(|r| *r == "origin")
        .or(remotes.first())
        .cloned()
}

/// A remote's URL with any password or token taken out, for display.
pub fn safe_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    match authority.rsplit_once('@') {
        Some((_, host)) => format!("{scheme}://{host}{path}"),
        None => url.to_string(),
    }
}

/// The branch everyone merges into: the remote's HEAD, else main/master/develop, else the
/// project folder's branch.
pub fn base_of(root: &str, remote: Option<&str>) -> Option<String> {
    if let Some(r) = remote {
        if let Some(h) = trimmed(
            root,
            &[
                "symbolic-ref",
                "-q",
                "--short",
                &format!("refs/remotes/{r}/HEAD"),
            ],
        ) {
            return Some(h);
        }
        for b in ["main", "master", "develop", "trunk"] {
            if ref_exists(root, &format!("refs/remotes/{r}/{b}")) {
                return Some(format!("{r}/{b}"));
            }
        }
    }
    for b in ["main", "master", "develop", "trunk"] {
        if ref_exists(root, &format!("refs/heads/{b}")) {
            return Some(b.into());
        }
    }
    trimmed(root, &["symbolic-ref", "-q", "--short", "HEAD"])
}

pub fn me(root: &str) -> Person {
    Person {
        name: trimmed(root, &["config", "user.name"]).unwrap_or_default(),
        email: trimmed(root, &["config", "user.email"]).unwrap_or_default(),
    }
}

/// The presence ref's last part for an email: lowercase letters, digits and dashes.
pub fn presence_key(email: &str) -> String {
    git::slug(email)
}

fn count_pair(root: &str, range: &str) -> (u32, u32) {
    git::git(root, &["rev-list", "--left-right", "--count", range])
        .ok()
        .map(|c| {
            let mut it = c.split_whitespace().filter_map(|n| n.parse::<u32>().ok());
            (it.next().unwrap_or(0), it.next().unwrap_or(0))
        })
        .unwrap_or((0, 0))
}

/// `git log --format=%an%x1f%ae%x1f%at` → authors, most commits first.
pub fn parse_authors(out: &str) -> Vec<Author> {
    let mut v: Vec<Author> = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\u{1f}').collect();
        if f.len() < 3 {
            continue;
        }
        let time = f[2].trim().parse::<i64>().unwrap_or(0) * 1000;
        let email = f[1].trim().to_lowercase();
        match v.iter_mut().find(|a| a.email == email) {
            Some(a) => {
                a.commits += 1;
                a.last = a.last.max(time);
            }
            None => v.push(Author {
                name: f[0].trim().to_string(),
                email,
                commits: 1,
                last: time,
            }),
        }
    }
    v.sort_by(|a, b| b.commits.cmp(&a.commits).then(b.last.cmp(&a.last)));
    v
}

/* ------------------------------------------------------------------------------- branches */

struct RefLine {
    name: String,
    hash: String,
    author: String,
    email: String,
    time: i64,
    subject: String,
    upstream: Option<String>,
    worktree: Option<String>,
}

fn ref_lines(root: &str, pattern: &str) -> Vec<RefLine> {
    let fmt = "%(refname:short)%1f%(objectname)%1f%(authorname)%1f%(authoremail:trim)%1f%(committerdate:unix)%1f%(contents:subject)%1f%(upstream:short)%1f%(worktreepath)";
    git::git(
        root,
        &[
            "for-each-ref",
            &format!("--format={fmt}"),
            "--sort=-committerdate",
            pattern,
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|l| {
        let f: Vec<&str> = l.split('\u{1f}').collect();
        if f.len() < 8 || f[0].ends_with("/HEAD") || f[0].is_empty() {
            return None;
        }
        Some(RefLine {
            name: f[0].to_string(),
            hash: f[1].to_string(),
            author: f[2].to_string(),
            email: f[3].to_lowercase(),
            time: f[4].trim().parse::<i64>().unwrap_or(0) * 1000,
            subject: f[5].to_string(),
            upstream: Some(f[6].to_string()).filter(|s| !s.is_empty()),
            worktree: Some(f[7].to_string()).filter(|s| !s.is_empty()),
        })
    })
    .collect()
}

fn branch_of(
    root: &str,
    base: Option<&str>,
    r: &RefLine,
    short: String,
    local_only: bool,
) -> TeamBranch {
    let (mut ahead, mut behind, mut authors, mut files, mut more_files) =
        (0, 0, Vec::new(), Vec::new(), false);
    if let Some(b) = base {
        let (be, ah) = count_pair(root, &format!("{b}...{}", r.name));
        behind = be;
        ahead = ah;
        if ahead > 0 {
            authors = parse_authors(
                &git::git(
                    root,
                    &[
                        "log",
                        "--no-merges",
                        "--format=%an%x1f%ae%x1f%at",
                        "--max-count=500",
                        &format!("{b}..{}", r.name),
                    ],
                )
                .unwrap_or_default(),
            );
            if let Ok(out) = git::git(
                root,
                &["diff", "--name-only", "-z", &format!("{b}...{}", r.name)],
            ) {
                let all: Vec<String> = out
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                more_files = all.len() > MAX_FILES;
                files = all.into_iter().take(MAX_FILES).collect();
            }
        }
    }
    TeamBranch {
        name: r.name.clone(),
        short,
        local_only,
        hash: r.hash.clone(),
        author: r.author.clone(),
        email: r.email.clone(),
        time: r.time,
        subject: r.subject.clone(),
        authors,
        ahead,
        behind,
        files,
        more_files,
        checked_out: r.worktree.clone(),
        local: None,
        unpushed: 0,
        unpulled: 0,
    }
}

/// Every branch on the remote (and local branches that were never pushed), newest first.
/// `days`: leave out branches with no commit in that many days (0 = all).
pub fn branches(
    root: &str,
    remote: Option<&str>,
    base: Option<&str>,
    days: u32,
) -> Vec<TeamBranch> {
    let since = if days == 0 {
        i64::MIN
    } else {
        now_ms() - days as i64 * 86_400_000
    };
    let locals = ref_lines(root, "refs/heads");
    let mut out = Vec::new();
    if let Some(rm) = remote {
        let prefix = format!("{rm}/");
        for r in ref_lines(root, &format!("refs/remotes/{rm}")) {
            if out.len() >= MAX_BRANCHES || r.time < since {
                continue;
            }
            if Some(r.name.as_str()) == base {
                continue;
            }
            let short = r.name.strip_prefix(&prefix).unwrap_or(&r.name).to_string();
            let mut b = branch_of(root, base, &r, short, false);
            if let Some(l) = locals
                .iter()
                .find(|l| l.upstream.as_deref() == Some(r.name.as_str()))
            {
                let (unpushed, unpulled) = count_pair(root, &format!("{}...{}", l.name, r.name));
                b.local = Some(l.name.clone());
                b.unpushed = unpushed;
                b.unpulled = unpulled;
                b.checked_out = l.worktree.clone().or(b.checked_out);
            }
            out.push(b);
        }
    }
    // Local branches nobody else can see yet.
    for l in &locals {
        if out.len() >= MAX_BRANCHES || l.time < since || l.upstream.is_some() {
            continue;
        }
        if Some(l.name.as_str()) == base || l.name.starts_with(REVIEW_PREFIX) {
            continue;
        }
        // A remote branch of the same name means it was pushed without tracking.
        if out.iter().any(|b| b.short == l.name) {
            continue;
        }
        let b = branch_of(root, base, l, l.name.clone(), true);
        out.push(b);
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.time));
    out
}

pub fn branch_detail(dir: &str, branch: &str, base: &str) -> Result<BranchDetail, String> {
    check_rev(branch)?;
    check_rev(base)?;
    let root = root_of(dir)?;
    let mb = trimmed(&root, &["merge-base", base, branch])
        .ok_or_else(|| format!("{branch} and {base} have no commit in common"))?;
    let fmt = "--format=%H\u{1f}%P\u{1f}%an\u{1f}%ae\u{1f}%at\u{1f}%s\u{1f}%D\u{1e}";
    let commits = git::parse_log(
        &git::git(
            &root,
            &["log", fmt, "--max-count=300", &format!("{base}..{branch}")],
        )
        .unwrap_or_default(),
    );
    let files = git::name_status(&git::git(
        &root,
        &["diff", "-z", "-M", "--name-status", &mb, branch],
    )?);
    Ok(BranchDetail {
        branch: branch.into(),
        base: base.into(),
        merge_base: mb,
        commits,
        files,
    })
}

/* ------------------------------------------------------------------------------- fetching */

/// Fetches the remote's branches (pruning the ones deleted there) and everyone's presence.
pub fn fetch(dir: &str) -> Result<(), String> {
    let root = root_of(dir)?;
    let remote = remote_of(&root).ok_or("This repository has no remote to fetch from. Add one (git remote add origin <url>) so your team's branches show up here.")?;
    git::git(
        &root,
        &[
            "fetch",
            "--prune",
            "--quiet",
            &remote,
            &format!("+refs/heads/*:refs/remotes/{remote}/*"),
            &format!("+{PRESENCE_NS}/*:{PRESENCE_NS}/*"),
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("Couldn't fetch from {remote}: {e}"))
}

/* ------------------------------------------------------------------------------- presence */

fn clip(s: &str, n: usize) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    if s.chars().count() > n {
        s.chars().take(n).collect::<String>() + "…"
    } else {
        s
    }
}

/// A presence file from the remote, cleaned: sizes limited, nothing but plain fields.
pub fn parse_presence(bytes: &[u8]) -> Option<Presence> {
    if bytes.len() > MAX_PRESENCE_BYTES {
        return None;
    }
    let mut p: Presence = serde_json::from_slice(bytes).ok()?;
    if p.email.trim().is_empty() || p.updated_at <= 0 {
        return None;
    }
    p.name = clip(&p.name, 80);
    p.email = clip(&p.email.to_lowercase(), 120);
    p.branch = p.branch.map(|b| clip(&b, 200));
    p.worktrees = p.worktrees.iter().take(30).map(|b| clip(b, 200)).collect();
    p.sessions = p
        .sessions
        .into_iter()
        .take(40)
        .map(|s| PresenceSession {
            title: clip(&s.title, 140),
            agent: clip(&s.agent, 40),
            branch: s.branch.map(|b| clip(&b, 200)),
            status: match s.status.as_str() {
                "running" | "waiting" | "error" => s.status,
                _ => "idle".into(),
            },
            updated_at: s.updated_at,
        })
        .collect();
    p.time = std::mem::take(&mut p.time)
        .into_iter()
        .take(400)
        .filter_map(crate::time::clean_branch_day)
        .collect();
    Some(p)
}

pub fn read_presence(root: &str) -> Vec<Presence> {
    let refs =
        git::git(root, &["for-each-ref", "--format=%(refname)", PRESENCE_NS]).unwrap_or_default();
    let mut out: Vec<Presence> = refs
        .lines()
        .filter(|r| r.starts_with(PRESENCE_NS))
        .filter_map(|r| {
            let o = Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["show", &format!("{r}:{PRESENCE_FILE}")])
                .env("PATH", crate::claude::augmented_path())
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            o.status
                .success()
                .then_some(o.stdout)
                .and_then(|b| parse_presence(&b))
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.updated_at));
    out.dedup_by(|a, b| a.email == b.email);
    out
}

/// Writes your presence as a commit under refs/agentdeck/presence/<you> and pushes it.
/// `push` false only writes it locally (tests, and when you're offline the next push catches up).
pub fn publish(dir: &str, presence: &Presence, push: bool) -> Result<(), String> {
    let root = root_of(dir)?;
    if presence.email.trim().is_empty() {
        return Err("Git doesn't know who you are in this repository. Set it with git config user.name and git config user.email, then share again.".into());
    }
    let key = presence_key(&presence.email);
    let json = serde_json::to_vec_pretty(presence).map_err(|e| e.to_string())?;
    let blob = git_input(&root, &["hash-object", "-w", "--stdin"], &json)?;
    let tree = git_input(
        &root,
        &["mktree"],
        format!("100644 blob {blob}\t{PRESENCE_FILE}\n").as_bytes(),
    )?;
    let commit = git_input(
        &root,
        &[
            "-c",
            &format!(
                "user.name={}",
                if presence.name.is_empty() {
                    "AgentDeck"
                } else {
                    &presence.name
                }
            ),
            "-c",
            &format!("user.email={}", presence.email),
            "commit-tree",
            &tree,
            "-m",
            "AgentDeck presence",
        ],
        b"",
    )?;
    let r = format!("{PRESENCE_NS}/{key}");
    git::git(&root, &["update-ref", &r, &commit])?;
    if push {
        let remote = remote_of(&root).ok_or("This repository has no remote to share through")?;
        git::git(
            &root,
            &["push", "--quiet", "--force", &remote, &format!("{r}:{r}")],
        )
        .map_err(|e| format!("Couldn't push to {remote}: {e}"))?;
    }
    Ok(())
}

/// Takes your presence off the remote and out of this repository.
pub fn unpublish(dir: &str, email: &str, push: bool) -> Result<(), String> {
    let root = root_of(dir)?;
    let r = format!("{PRESENCE_NS}/{}", presence_key(email));
    if push {
        if let Some(remote) = remote_of(&root) {
            git::git(&root, &["push", "--quiet", &remote, &format!(":{r}")])
                .map_err(|e| format!("Couldn't remove it from {remote}: {e}"))?;
        }
    }
    if ref_exists(&root, &r) {
        git::git(&root, &["update-ref", "-d", &r])?;
    }
    Ok(())
}

/* ------------------------------------------------------------------------------- reviews */

/// The local branch of a review checkout: a teammate's work, never merged from AgentDeck.
pub fn is_review_branch(branch: &str) -> bool {
    branch.starts_with(REVIEW_PREFIX)
}

pub fn reviews(root: &str) -> Vec<ReviewCheckout> {
    git::worktrees(root)
        .into_iter()
        .filter(|w| {
            !w.main
                && w.branch
                    .as_deref()
                    .is_some_and(|b| b.starts_with(REVIEW_PREFIX))
        })
        .map(|w| {
            let branch = w.branch.unwrap_or_default();
            let upstream = trimmed(
                root,
                &[
                    "rev-parse",
                    "--abbrev-ref",
                    "--symbolic-full-name",
                    &format!("{branch}@{{upstream}}"),
                ],
            );
            let (ahead, behind) = upstream
                .as_deref()
                .map(|u| count_pair(root, &format!("{branch}...{u}")))
                .unwrap_or((0, 0));
            let dirty = Path::new(&w.path).is_dir() && !git::status(&w.path).is_empty();
            ReviewCheckout {
                path: w.path,
                branch,
                upstream,
                behind,
                ahead,
                dirty,
            }
        })
        .collect()
}

/// Checks a teammate's branch out into its own worktree, or brings an existing review checkout
/// up to date. The project folder isn't touched.
pub fn checkout_review(dir: &str, branch: &str) -> Result<ReviewCheckout, String> {
    check_rev(branch)?;
    let root = root_of(dir)?;
    let remote = remote_of(&root);
    let is_remote = remote.as_deref().is_some_and(|r| {
        ref_exists(&root, &format!("refs/remotes/{branch}")) && branch.starts_with(&format!("{r}/"))
    });
    if !is_remote && !ref_exists(&root, &format!("refs/heads/{branch}")) {
        return Err(format!("There's no branch {branch} here. Fetch first."));
    }
    let short = match &remote {
        Some(r) if is_remote => branch.strip_prefix(&format!("{r}/")).unwrap_or(branch),
        _ => branch,
    };
    let tag = format!("review-{}", git::slug(short));
    let local = format!("agentdeck/{tag}");
    if let Some(existing) = reviews(&root).into_iter().find(|r| r.branch == local) {
        if existing.behind > 0 {
            update_review(&root, &existing.path)?;
        }
        return reviews(&root)
            .into_iter()
            .find(|r| r.branch == local)
            .ok_or_else(|| "The review checkout went missing".into());
    }
    let home = git::worktree_home(&root);
    std::fs::create_dir_all(&home).map_err(|e| format!("Couldn't make {}: {e}", home.display()))?;
    let path = home.join(&tag);
    if path.exists() {
        return Err(format!(
            "{} already exists and isn't a review checkout. Move it away, then try again.",
            path.display()
        ));
    }
    let p = path.to_string_lossy().into_owned();
    if ref_exists(&root, &format!("refs/heads/{local}")) {
        // A review branch left behind by a removed checkout: reuse it and catch up.
        git::git(&root, &["worktree", "add", &p, &local])?;
        if is_remote {
            git::git(
                &root,
                &["branch", "--quiet", "--set-upstream-to", branch, &local],
            )?;
            let _ = git::git(&p, &["merge", "--ff-only", "--quiet", branch]);
        }
    } else if is_remote {
        git::git(
            &root,
            &["worktree", "add", "--track", "-b", &local, &p, branch],
        )?;
    } else {
        git::git(&root, &["worktree", "add", "-b", &local, &p, branch])?;
        git::git(
            &root,
            &["branch", "--quiet", "--set-upstream-to", branch, &local],
        )?;
    }
    reviews(&root)
        .into_iter()
        .find(|r| r.branch == local)
        .ok_or_else(|| "The review checkout went missing".into())
}

/// Brings a review checkout up to what the teammate pushed (fast-forward only; your own commits
/// or changes in it are never overwritten).
pub fn update_review(dir: &str, path: &str) -> Result<ReviewCheckout, String> {
    let root = root_of(dir)?;
    let r = reviews(&root)
        .into_iter()
        .find(|r| r.path == path)
        .ok_or("That folder isn't one of this project's review checkouts")?;
    let up = r
        .upstream
        .ok_or("The review checkout doesn't follow a branch")?;
    git::git(path, &["merge", "--ff-only", "--quiet", &up]).map_err(|e| {
        format!("Couldn't bring it up to date without losing what's in it ({e}). Commit or discard the changes in the review checkout first.")
    })?;
    reviews(&root)
        .into_iter()
        .find(|x| x.path == path)
        .ok_or_else(|| "The review checkout went missing".into())
}

pub fn remove_review(dir: &str, path: &str) -> Result<(), String> {
    let root = root_of(dir)?;
    let r = reviews(&root)
        .into_iter()
        .find(|r| r.path == path)
        .ok_or("That folder isn't one of this project's review checkouts")?;
    git::remove_worktree(&root, &r.path, &r.branch, true)
}

/* ------------------------------------------------------------------------------- overview */

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn overview(dir: &str, days: u32) -> Result<TeamOverview, String> {
    let root = root_of(dir)?;
    let remote = remote_of(&root);
    let remote_url = remote
        .as_deref()
        .and_then(|r| trimmed(&root, &["remote", "get-url", r]))
        .map(|u| safe_url(&u));
    let base = base_of(&root, remote.as_deref());
    Ok(TeamOverview {
        branches: branches(&root, remote.as_deref(), base.as_deref(), days),
        presence: read_presence(&root),
        reviews: reviews(&root),
        current: trimmed(&root, &["symbolic-ref", "-q", "--short", "HEAD"]),
        me: me(&root),
        root,
        remote,
        remote_url,
        base,
        sharing: false,
        fetched_at: None,
        sync_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let s = dir.to_string_lossy();
        git::git(&s, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    }

    fn commit(dir: &Path, file: &str, text: &str, msg: &str) {
        std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
        std::fs::write(dir.join(file), text).unwrap();
        run(dir, &["add", "."]);
        run(dir, &["commit", "-qm", msg]);
    }

    /// A bare "server" repository and two teammates' clones of it.
    fn team() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let server = d.path().join("server.git");
        run(
            d.path(),
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                &server.to_string_lossy(),
            ],
        );
        let ayse = d.path().join("ayse");
        let mert = d.path().join("mert");
        for (dir, name) in [(&ayse, "Ayşe"), (&mert, "Mert")] {
            run(
                d.path(),
                &[
                    "clone",
                    "-q",
                    &server.to_string_lossy(),
                    &dir.to_string_lossy(),
                ],
            );
            run(dir, &["config", "user.name", name]);
            run(
                dir,
                &[
                    "config",
                    "user.email",
                    &format!("{}@team.dev", name.to_lowercase()),
                ],
            );
            run(dir, &["config", "commit.gpgsign", "false"]);
        }
        run(&ayse, &["checkout", "-qb", "main"]);
        commit(&ayse, "app/login.ts", "login v1\n", "first");
        commit(&ayse, "README.md", "readme\n", "readme");
        run(&ayse, &["push", "-q", "-u", "origin", "main"]);
        run(&mert, &["fetch", "-q"]);
        run(&mert, &["checkout", "-q", "main"]);
        (d, ayse, mert)
    }

    #[test]
    fn sees_teammates_branches_who_works_on_them_and_where_they_overlap() {
        let (_d, ayse, mert) = team();
        // Ayşe works on the login screen, Mert on the login API and the readme.
        run(&ayse, &["checkout", "-qb", "feature/login-ui"]);
        commit(&ayse, "app/login.ts", "login v2\n", "new login screen");
        commit(&ayse, "app/ui.ts", "ui\n", "ui");
        run(&ayse, &["push", "-q", "-u", "origin", "feature/login-ui"]);
        run(&mert, &["checkout", "-qb", "fix/login-api"]);
        commit(
            &mert,
            "app/login.ts",
            "login fix\n",
            "fix the login request",
        );
        run(&mert, &["push", "-q", "-u", "origin", "fix/login-api"]);
        // And a branch Mert never pushed.
        run(&mert, &["checkout", "-qb", "spike", "main"]);
        commit(&mert, "spike.txt", "x\n", "spike");

        let m = mert.to_string_lossy().into_owned();
        fetch(&m).unwrap();
        let o = overview(&m, 0).unwrap();
        assert_eq!(o.remote.as_deref(), Some("origin"));
        assert_eq!(o.base.as_deref(), Some("origin/main"));
        assert_eq!(o.me.email, "mert@team.dev");

        let ui = o
            .branches
            .iter()
            .find(|b| b.short == "feature/login-ui")
            .unwrap();
        assert_eq!(ui.name, "origin/feature/login-ui");
        assert_eq!((ui.ahead, ui.behind), (2, 0));
        assert_eq!(ui.authors.len(), 1);
        assert_eq!(
            (ui.authors[0].name.as_str(), ui.authors[0].commits),
            ("Ayşe", 2)
        );
        let mut files = ui.files.clone();
        files.sort();
        assert_eq!(files, ["app/login.ts", "app/ui.ts"]);
        assert!(ui.local.is_none());

        let api = o
            .branches
            .iter()
            .find(|b| b.short == "fix/login-api")
            .unwrap();
        assert_eq!(api.local.as_deref(), Some("fix/login-api"));
        assert_eq!((api.unpushed, api.unpulled), (0, 0));
        assert_eq!(api.files, ["app/login.ts"]);

        let spike = o.branches.iter().find(|b| b.short == "spike").unwrap();
        assert!(spike.local_only);
        assert!(
            !o.branches.iter().any(|b| b.short == "main"),
            "the base isn't listed"
        );

        // What Ayşe's branch changed, for the diff.
        let d = branch_detail(&m, "origin/feature/login-ui", "origin/main").unwrap();
        assert_eq!(d.commits.len(), 2);
        assert_eq!(d.commits[0].subject, "ui");
        assert_eq!(d.files.len(), 2);
        let pair = git::file_pair(
            &m,
            &d.merge_base,
            "origin/feature/login-ui",
            "app/login.ts",
            None,
        )
        .unwrap();
        assert_eq!(
            (pair.old.as_str(), pair.new.as_str()),
            ("login v1\n", "login v2\n")
        );

        // A branch deleted on the remote goes away on the next fetch.
        run(
            &ayse,
            &["push", "-q", "origin", "--delete", "feature/login-ui"],
        );
        fetch(&m).unwrap();
        assert!(!overview(&m, 0)
            .unwrap()
            .branches
            .iter()
            .any(|b| b.short == "feature/login-ui"));
    }

    #[test]
    fn presence_is_shared_through_the_remote_and_taken_back() {
        let (_d, ayse, mert) = team();
        let a = ayse.to_string_lossy().into_owned();
        let m = mert.to_string_lossy().into_owned();
        let p = Presence {
            version: 1,
            name: "Ayşe".into(),
            email: "ayse@team.dev".into(),
            branch: Some("feature/login-ui".into()),
            worktrees: vec!["agentdeck/claude-code-1a2b".into()],
            sessions: vec![PresenceSession {
                title: "Redesign the login screen".into(),
                agent: "Claude Code".into(),
                branch: Some("agentdeck/claude-code-1a2b".into()),
                status: "running".into(),
                updated_at: 5,
            }],
            time: vec![crate::time::BranchDay {
                branch: "feature/login-ui".into(),
                day: "2026-11-11".into(),
                agent_ms: 3_600_000,
                waiting_ms: 60_000,
                you_ms: 900_000,
            }],
            updated_at: now_ms(),
        };
        publish(&a, &p, true).unwrap();
        // It's no branch.
        assert!(!git::git(&a, &["ls-remote", "--heads", "origin"])
            .unwrap()
            .contains("presence"));

        fetch(&m).unwrap();
        let seen = read_presence(&m);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0], p);

        // Publishing again replaces it rather than piling up.
        let mut p2 = p.clone();
        p2.sessions.clear();
        p2.updated_at += 1;
        publish(&a, &p2, true).unwrap();
        fetch(&m).unwrap();
        assert_eq!(read_presence(&m), [p2]);

        unpublish(&a, "ayse@team.dev", true).unwrap();
        fetch(&m).unwrap();
        assert!(read_presence(&m).is_empty(), "pruned on the next fetch");
    }

    #[test]
    fn presence_from_the_remote_is_cleaned() {
        assert!(parse_presence(b"not json").is_none());
        assert!(parse_presence(br#"{"name":"x","email":"","updatedAt":1}"#).is_none());
        let long = "a".repeat(500);
        let raw = format!(
            r#"{{"name":"{long}","email":"X@Y.dev","updatedAt":3,"sessions":[{{"title":"t\u0007","agent":"a","status":"rm -rf","updatedAt":1}}]}}"#
        );
        let p = parse_presence(raw.as_bytes()).unwrap();
        assert_eq!(p.name.chars().count(), 81);
        assert_eq!(p.email, "x@y.dev");
        assert_eq!(p.sessions[0].title, "t");
        assert_eq!(p.sessions[0].status, "idle");
        assert!(parse_presence(&vec![b' '; MAX_PRESENCE_BYTES + 1]).is_none());
    }

    #[test]
    fn a_teammates_branch_is_checked_out_updated_and_removed_for_review() {
        let (_d, ayse, mert) = team();
        run(&ayse, &["checkout", "-qb", "feature/login-ui"]);
        commit(&ayse, "app/login.ts", "login v2\n", "new login screen");
        run(&ayse, &["push", "-q", "-u", "origin", "feature/login-ui"]);
        let m = mert.to_string_lossy().into_owned();
        fetch(&m).unwrap();

        let r = checkout_review(&m, "origin/feature/login-ui").unwrap();
        assert_eq!(r.branch, "agentdeck/review-feature-login-ui");
        assert_eq!(r.upstream.as_deref(), Some("origin/feature/login-ui"));
        assert!(
            r.path.ends_with("mert.worktrees/review-feature-login-ui"),
            "{}",
            r.path
        );
        assert_eq!(
            std::fs::read_to_string(Path::new(&r.path).join("app/login.ts")).unwrap(),
            "login v2\n"
        );
        // Mert's own folder is still on main.
        assert_eq!(
            std::fs::read_to_string(mert.join("app/login.ts")).unwrap(),
            "login v1\n"
        );

        // Ayşe pushes more; the checkout sees it and catches up.
        commit(&ayse, "app/login.ts", "login v3\n", "polish");
        run(&ayse, &["push", "-q"]);
        fetch(&m).unwrap();
        assert_eq!(reviews(&m)[0].behind, 1);
        let r = update_review(&m, &r.path).unwrap();
        assert_eq!(r.behind, 0);
        assert_eq!(
            std::fs::read_to_string(Path::new(&r.path).join("app/login.ts")).unwrap(),
            "login v3\n"
        );

        // Checking out again reuses it.
        assert_eq!(
            checkout_review(&m, "origin/feature/login-ui").unwrap().path,
            r.path
        );
        // The review branch isn't listed as Mert's own local work.
        assert!(!overview(&m, 0)
            .unwrap()
            .branches
            .iter()
            .any(|b| b.local_only && b.short.starts_with(REVIEW_PREFIX)));

        remove_review(&m, &r.path).unwrap();
        assert!(!Path::new(&r.path).exists());
        assert!(reviews(&m).is_empty());
        assert!(checkout_review(&m, "origin/nope").is_err());
        assert!(checkout_review(&m, "--upload-pack=x").is_err());
    }

    #[test]
    fn small_parts() {
        assert_eq!(
            safe_url("https://user:tok@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(safe_url("git@github.com:a/b.git"), "git@github.com:a/b.git");
        assert_eq!(
            safe_url("https://github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(presence_key("Ayse.Y@Team.dev"), "ayse-y-team-dev");
        let a = parse_authors("A\u{1f}a@x\u{1f}10\nB\u{1f}b@x\u{1f}30\nA\u{1f}A@x\u{1f}20\n");
        assert_eq!(a.len(), 2);
        assert_eq!(
            (a[0].name.as_str(), a[0].commits, a[0].last),
            ("A", 2, 20_000)
        );
    }
}
