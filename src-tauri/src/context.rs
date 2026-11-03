//! What shapes an agent's behaviour besides the prompt: instruction files (CLAUDE.md,
//! AGENTS.md, GEMINI.md), skills, slash commands and subagents, for one project and for the
//! whole Mac ("global").
//!
//! Only files in the places the agents themselves read can be opened or saved here; every path
//! coming from the interface is checked against those places again before it is touched.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_ITEMS_PER_PLACE: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// One file, `rel` from the root.
    File(&'static str),
    /// `<root>/<name>/SKILL.md`.
    SkillDirs,
    /// Files with this extension anywhere under the root (subfolders namespace them).
    Tree(&'static str),
    /// Files with this extension directly in the root.
    Flat(&'static str),
}

#[derive(Clone, Debug)]
struct Place {
    kind: &'static str,
    scope: &'static str,
    agents: &'static [&'static str],
    dir: PathBuf,
    shape: Shape,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// "instructions" | "skill" | "command" | "subagent"
    pub kind: String,
    /// "project" | "global"
    pub scope: String,
    pub agents: Vec<String>,
    /// How the agent refers to it: "CLAUDE.md", "/review", "pdf-tools".
    pub name: String,
    pub description: Option<String>,
    pub path: String,
    pub exists: bool,
    pub size: u64,
    pub modified: i64,
    /// A link to a file kept elsewhere (common for shared skills).
    pub linked: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileText {
    pub path: String,
    pub text: String,
    pub modified: i64,
    pub exists: bool,
}

/// Where each agent keeps its settings folders.
#[derive(Clone, Debug)]
pub struct Homes {
    pub home: PathBuf,
    pub claude: PathBuf,
    pub codex: PathBuf,
    pub gemini: PathBuf,
}

impl Homes {
    pub fn from_home(home: &Path) -> Self {
        let env = |k: &str| {
            crate::shellenv::get()
                .get(k)
                .cloned()
                .or_else(|| std::env::var(k).ok())
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
        };
        Self {
            home: home.to_path_buf(),
            claude: env("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude")),
            codex: env("CODEX_HOME").unwrap_or_else(|| home.join(".codex")),
            gemini: home.join(".gemini"),
        }
    }
}

const CLAUDE: &[&str] = &["claude-code"];
const CODEX: &[&str] = &["codex"];
const GEMINI: &[&str] = &["gemini-cli"];

fn places(project: Option<&Path>, h: &Homes) -> Vec<Place> {
    let p = |kind, scope, agents, dir: PathBuf, shape| Place {
        kind,
        scope,
        agents,
        dir,
        shape,
    };
    let mut out = Vec::new();
    if let Some(root) = project {
        let r = root.to_path_buf();
        out.extend([
            p(
                "instructions",
                "project",
                CLAUDE,
                r.clone(),
                Shape::File("CLAUDE.md"),
            ),
            p(
                "instructions",
                "project",
                CLAUDE,
                r.clone(),
                Shape::File("CLAUDE.local.md"),
            ),
            p(
                "instructions",
                "project",
                CLAUDE,
                r.join(".claude"),
                Shape::File("CLAUDE.md"),
            ),
            p(
                "instructions",
                "project",
                CODEX,
                r.clone(),
                Shape::File("AGENTS.md"),
            ),
            p(
                "instructions",
                "project",
                CODEX,
                r.clone(),
                Shape::File("AGENTS.override.md"),
            ),
            p(
                "instructions",
                "project",
                GEMINI,
                r.clone(),
                Shape::File("GEMINI.md"),
            ),
            p(
                "skill",
                "project",
                CLAUDE,
                r.join(".claude/skills"),
                Shape::SkillDirs,
            ),
            p(
                "skill",
                "project",
                CODEX,
                r.join(".agents/skills"),
                Shape::SkillDirs,
            ),
            p(
                "skill",
                "project",
                GEMINI,
                r.join(".gemini/skills"),
                Shape::SkillDirs,
            ),
            p(
                "command",
                "project",
                CLAUDE,
                r.join(".claude/commands"),
                Shape::Tree("md"),
            ),
            p(
                "command",
                "project",
                GEMINI,
                r.join(".gemini/commands"),
                Shape::Tree("toml"),
            ),
            p(
                "subagent",
                "project",
                CLAUDE,
                r.join(".claude/agents"),
                Shape::Flat("md"),
            ),
        ]);
    }
    out.extend([
        p(
            "instructions",
            "global",
            CLAUDE,
            h.claude.clone(),
            Shape::File("CLAUDE.md"),
        ),
        p(
            "instructions",
            "global",
            CODEX,
            h.codex.clone(),
            Shape::File("AGENTS.md"),
        ),
        p(
            "instructions",
            "global",
            GEMINI,
            h.gemini.clone(),
            Shape::File("GEMINI.md"),
        ),
        p(
            "skill",
            "global",
            CLAUDE,
            h.claude.join("skills"),
            Shape::SkillDirs,
        ),
        p(
            "skill",
            "global",
            CODEX,
            h.home.join(".agents/skills"),
            Shape::SkillDirs,
        ),
        p(
            "skill",
            "global",
            CODEX,
            h.codex.join("skills"),
            Shape::SkillDirs,
        ),
        p(
            "skill",
            "global",
            GEMINI,
            h.gemini.join("skills"),
            Shape::SkillDirs,
        ),
        p(
            "command",
            "global",
            CLAUDE,
            h.claude.join("commands"),
            Shape::Tree("md"),
        ),
        p(
            "command",
            "global",
            CODEX,
            h.codex.join("prompts"),
            Shape::Flat("md"),
        ),
        p(
            "command",
            "global",
            GEMINI,
            h.gemini.join("commands"),
            Shape::Tree("toml"),
        ),
        p(
            "subagent",
            "global",
            CLAUDE,
            h.claude.join("agents"),
            Shape::Flat("md"),
        ),
    ]);
    out
}

fn modified_ms(m: &std::fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `name:` and `description:` from YAML front matter, or `description = "…"` from TOML.
fn describe(text: &str) -> (Option<String>, Option<String>) {
    let unquote = |v: &str| {
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|x| x.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))
            .unwrap_or(v);
        (!v.is_empty() && v != "|" && v != ">").then(|| v.to_string())
    };
    let mut name = None;
    let mut desc = None;
    if let Some(rest) = text.strip_prefix("---") {
        for line in rest.lines().skip(1) {
            if line.trim() == "---" {
                break;
            }
            if let Some(v) = line.strip_prefix("name:") {
                name = unquote(v);
            } else if let Some(v) = line.strip_prefix("description:") {
                desc = unquote(v);
            }
        }
    } else {
        for line in text.lines().take(40) {
            let l = line.trim_start();
            if let Some(v) = l
                .strip_prefix("description")
                .map(str::trim_start)
                .and_then(|v| v.strip_prefix('='))
            {
                desc = unquote(v);
                break;
            }
        }
    }
    let clip = |s: String| {
        if s.chars().count() > 300 {
            s.chars().take(300).collect::<String>() + "…"
        } else {
            s
        }
    };
    (name, desc.map(clip))
}

fn head(path: &Path) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; 8192];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut buf))
        .unwrap_or(0);
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn item(place: &Place, path: &Path, name: String) -> Item {
    let meta = std::fs::metadata(path).ok();
    let linked = std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
        || path
            .parent()
            .and_then(|d| std::fs::symlink_metadata(d).ok())
            .is_some_and(|m| m.file_type().is_symlink());
    let (fm_name, description) = if meta.is_some() {
        describe(&head(path))
    } else {
        (None, None)
    };
    let name = match (place.shape, fm_name) {
        (Shape::SkillDirs, Some(n)) => n,
        _ => name,
    };
    Item {
        kind: place.kind.into(),
        scope: place.scope.into(),
        agents: place.agents.iter().map(|a| a.to_string()).collect(),
        name,
        description,
        path: path.to_string_lossy().into_owned(),
        exists: meta.is_some(),
        size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
        modified: meta.as_ref().map(modified_ms).unwrap_or(0),
        linked,
    }
}

fn command_name(place: &Place, rel: &Path) -> String {
    let parts: Vec<String> = rel
        .with_extension("")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    match (place.agents.first().copied(), place.shape) {
        (Some("codex"), _) => format!("/prompts:{}", parts.join(":")),
        (Some("gemini-cli"), _) => format!("/{}", parts.join(":")),
        // Claude Code names a command by its file; subfolders only label it.
        _ => format!("/{}", parts.last().cloned().unwrap_or_default()),
    }
}

fn walk(dir: &Path, ext: &str, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 || out.len() >= MAX_ITEMS_PER_PLACE {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk(&p, ext, depth + 1, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
        if out.len() >= MAX_ITEMS_PER_PLACE {
            return;
        }
    }
}

/// Everything in the places the agents read, for one project (or none) plus global.
/// Instruction files that don't exist yet are listed too, so they can be created.
pub fn scan(project: Option<&Path>, homes: &Homes) -> Vec<Item> {
    let mut out = Vec::new();
    for place in places(project, homes) {
        match place.shape {
            Shape::File(rel) => {
                let path = place.dir.join(rel);
                // The nested `.claude/CLAUDE.md` is only shown when it's really there.
                let optional = rel == "CLAUDE.local.md"
                    || rel == "AGENTS.override.md"
                    || place.dir.ends_with(".claude") && place.scope == "project";
                if path.is_file() || !optional {
                    let shown = if place.dir.ends_with(".claude") && place.scope == "project" {
                        ".claude/CLAUDE.md".to_string()
                    } else {
                        rel.to_string()
                    };
                    out.push(item(&place, &path, shown));
                }
            }
            Shape::SkillDirs => {
                let Ok(rd) = std::fs::read_dir(&place.dir) else {
                    continue;
                };
                let mut dirs: Vec<_> = rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.join("SKILL.md").is_file())
                    .collect();
                dirs.sort();
                for d in dirs.into_iter().take(MAX_ITEMS_PER_PLACE) {
                    let name = d
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    out.push(item(&place, &d.join("SKILL.md"), name));
                }
            }
            Shape::Tree(ext) | Shape::Flat(ext) => {
                let mut files = Vec::new();
                if matches!(place.shape, Shape::Tree(_)) {
                    walk(&place.dir, ext, 0, &mut files);
                } else if let Ok(rd) = std::fs::read_dir(&place.dir) {
                    files = rd
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == ext))
                        .collect();
                    files.sort();
                    files.truncate(MAX_ITEMS_PER_PLACE);
                }
                for f in files {
                    let rel = f.strip_prefix(&place.dir).unwrap_or(&f).to_path_buf();
                    let name = if place.kind == "command" {
                        command_name(&place, &rel)
                    } else {
                        rel.with_extension("").to_string_lossy().into_owned()
                    };
                    out.push(item(&place, &f, name));
                }
            }
        }
    }
    out
}

fn plain(p: &Path) -> bool {
    p.is_absolute()
        && p.components().all(|c| {
            matches!(
                c,
                Component::RootDir | Component::Normal(_) | Component::Prefix(_)
            )
        })
}

/// The place a path belongs to, if it is one an agent reads. Used for every read and write.
fn place_of(path: &Path, projects: &[PathBuf], homes: &Homes) -> Option<Place> {
    if !plain(path) {
        return None;
    }
    let mut all = places(None, homes);
    for p in projects {
        all.extend(
            places(Some(p), homes)
                .into_iter()
                .filter(|x| x.scope == "project"),
        );
    }
    all.into_iter().find(|place| {
        let Ok(rel) = path.strip_prefix(&place.dir) else {
            return false;
        };
        let parts: Vec<&std::ffi::OsStr> = rel.iter().collect();
        let hidden = parts.iter().any(|s| s.to_string_lossy().starts_with('.'));
        match place.shape {
            Shape::File(f) => rel == Path::new(f),
            Shape::SkillDirs => parts.len() == 2 && parts[1] == "SKILL.md" && !hidden,
            Shape::Flat(ext) => {
                parts.len() == 1 && rel.extension().is_some_and(|x| x == ext) && !hidden
            }
            Shape::Tree(ext) => {
                (1..=5).contains(&parts.len())
                    && rel.extension().is_some_and(|x| x == ext)
                    && !hidden
            }
        }
    })
}

pub fn read(path: &str, projects: &[PathBuf], homes: &Homes) -> Result<FileText, String> {
    let p = PathBuf::from(path);
    place_of(&p, projects, homes)
        .ok_or("AgentDeck only opens agent instruction, skill and command files here")?;
    match std::fs::metadata(&p) {
        Ok(m) if m.len() > MAX_BYTES => {
            Err("This file is over 1 MB; open it in your editor instead".into())
        }
        Ok(m) => {
            let bytes = std::fs::read(&p).map_err(|e| format!("Couldn't read it: {e}"))?;
            let text =
                String::from_utf8(bytes).map_err(|_| "This file isn't plain text".to_string())?;
            Ok(FileText {
                path: path.into(),
                text,
                modified: modified_ms(&m),
                exists: true,
            })
        }
        Err(_) => Ok(FileText {
            path: path.into(),
            text: String::new(),
            modified: 0,
            exists: false,
        }),
    }
}

/// Saves a file. `expected_modified` is what the editor loaded (0 = it didn't exist); if the
/// file changed on disk since, nothing is written.
pub fn write(
    path: &str,
    text: &str,
    expected_modified: i64,
    projects: &[PathBuf],
    homes: &Homes,
) -> Result<FileText, String> {
    let p = PathBuf::from(path);
    place_of(&p, projects, homes)
        .ok_or("AgentDeck only saves agent instruction, skill and command files here")?;
    if text.len() as u64 > MAX_BYTES {
        return Err("That's over 1 MB".into());
    }
    let now = std::fs::metadata(&p).map(|m| modified_ms(&m)).unwrap_or(0);
    if now != expected_modified {
        return Err(if now == 0 {
            "It was deleted on disk since you opened it. Save again to create it.".into()
        } else {
            "It changed on disk since you opened it. Reload to see the new version, then make your edit again.".into()
        });
    }
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Couldn't make the folder: {e}"))?;
    }
    let is_link = std::fs::symlink_metadata(&p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if is_link {
        // A linked file is written where it lives, keeping the link.
        std::fs::write(&p, text).map_err(|e| format!("Couldn't save it: {e}"))?;
    } else {
        let tmp = p.with_extension(format!("agentdeck-{}.tmp", std::process::id()));
        std::fs::write(&tmp, text).map_err(|e| format!("Couldn't save it: {e}"))?;
        std::fs::rename(&tmp, &p).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Couldn't save it: {e}")
        })?;
    }
    read(path, projects, homes)
}

fn slug(name: &str) -> Result<String, String> {
    let s: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() || s.len() > 64 {
        return Err("Use a short name: letters, numbers and dashes".into());
    }
    Ok(s)
}

/// The path and starting text for a new skill, command or subagent.
pub fn new_item(
    kind: &str,
    scope: &str,
    agent: &str,
    name: &str,
    project: Option<&Path>,
    homes: &Homes,
) -> Result<(PathBuf, String), String> {
    let n = slug(name)?;
    let place = places(project, homes)
        .into_iter()
        .find(|p| {
            p.kind == kind
                && p.scope == scope
                && p.agents.contains(&agent)
                && !matches!(p.shape, Shape::File(_))
        })
        .ok_or("That agent doesn't read this kind of file from there")?;
    let (path, text) = match place.shape {
        Shape::SkillDirs => (
            place.dir.join(&n).join("SKILL.md"),
            format!("---\nname: {n}\ndescription: What this skill does and when the agent should use it.\n---\n\n# {n}\n\nSteps the agent follows when this skill applies.\n"),
        ),
        Shape::Tree("toml") => (
            place.dir.join(format!("{n}.toml")),
            format!("description = \"What /{n} does\"\nprompt = \"\"\"\nWhat the agent should do when you type /{n}. {{{{args}}}} is replaced with what you type after it.\n\"\"\"\n"),
        ),
        _ if kind == "subagent" => (
            place.dir.join(format!("{n}.md")),
            format!("---\nname: {n}\ndescription: When Claude Code should hand work to this subagent.\n---\n\nYou are a focused helper. Describe its job, what it may change and how it reports back.\n"),
        ),
        _ => (
            place.dir.join(format!("{n}.md")),
            format!("---\ndescription: What /{n} does\n---\n\nWhat the agent should do when you type /{n}. $ARGUMENTS is replaced with what you type after it.\n"),
        ),
    };
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    Ok((path, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, Homes) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let proj = dir.path().join("proj");
        let w = |p: PathBuf, t: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, t).unwrap();
        };
        w(proj.join("CLAUDE.md"), "# Rules\n");
        w(
            proj.join(".claude/skills/deploy/SKILL.md"),
            "---\nname: deploy-app\ndescription: Ships the app\n---\nbody",
        );
        w(
            proj.join(".claude/commands/git/review.md"),
            "---\ndescription: Review the diff\n---\n",
        );
        w(
            proj.join(".gemini/commands/git/sum.toml"),
            "description = \"Sum it up\"\nprompt = \"x\"\n",
        );
        w(home.join(".claude/CLAUDE.md"), "global rules");
        w(home.join(".codex/prompts/tidy.md"), "tidy");
        w(
            home.join(".claude/agents/tester.md"),
            "---\nname: tester\ndescription: Runs tests\n---\n",
        );
        let homes = Homes {
            home: home.clone(),
            claude: home.join(".claude"),
            codex: home.join(".codex"),
            gemini: home.join(".gemini"),
        };
        (dir, proj, homes)
    }

    #[test]
    fn finds_what_each_agent_reads() {
        let (_d, proj, homes) = setup();
        let items = scan(Some(&proj), &homes);
        let find = |n: &str| {
            items
                .iter()
                .find(|i| i.name == n)
                .unwrap_or_else(|| panic!("{n} in {items:#?}"))
        };
        assert!(find("CLAUDE.md").exists);
        assert!(
            !find("AGENTS.md").exists,
            "missing instruction files are offered"
        );
        let skill = find("deploy-app");
        assert_eq!(
            (skill.kind.as_str(), skill.scope.as_str()),
            ("skill", "project")
        );
        assert_eq!(skill.description.as_deref(), Some("Ships the app"));
        assert_eq!(
            find("/review").description.as_deref(),
            Some("Review the diff")
        );
        assert_eq!(find("/git:sum").description.as_deref(), Some("Sum it up"));
        assert_eq!(find("/prompts:tidy").agents, ["codex"]);
        assert_eq!(find("tester").kind, "subagent");
        assert!(items
            .iter()
            .filter(|i| i.name == "CLAUDE.md")
            .any(|i| i.scope == "global"));
    }

    #[test]
    fn only_agent_files_can_be_read_or_saved() {
        let (_d, proj, homes) = setup();
        let projects = vec![proj.clone()];
        let claude_md = proj.join("CLAUDE.md").to_string_lossy().into_owned();
        let f = read(&claude_md, &projects, &homes).unwrap();
        assert_eq!(f.text, "# Rules\n");
        let saved = write(
            &claude_md,
            "# Rules\nBe brief.\n",
            f.modified,
            &projects,
            &homes,
        )
        .unwrap();
        assert!(saved.text.contains("Be brief"));
        assert!(
            write(&claude_md, "again", f.modified - 1, &projects, &homes).is_err(),
            "stale edits are refused"
        );
        for bad in [
            proj.join("src/main.rs"),
            proj.join("../CLAUDE.md"),
            proj.join(".claude/skills/deploy/run.sh"),
            homes.home.join(".ssh/config"),
            proj.join(".claude/commands/../../secret.md"),
        ] {
            let p = bad.to_string_lossy().into_owned();
            assert!(read(&p, &projects, &homes).is_err(), "{p}");
            assert!(write(&p, "x", 0, &projects, &homes).is_err(), "{p}");
        }
        // A project that isn't in AgentDeck can't be written through.
        let other = proj
            .parent()
            .unwrap()
            .join("other/CLAUDE.md")
            .to_string_lossy()
            .into_owned();
        assert!(write(&other, "x", 0, &projects, &homes).is_err());
        // New items land where the agent looks for them.
        let (p, text) =
            new_item("skill", "global", "claude-code", "PDF tools", None, &homes).unwrap();
        assert_eq!(p, homes.claude.join("skills/pdf-tools/SKILL.md"));
        write(&p.to_string_lossy(), &text, 0, &projects, &homes).unwrap();
        assert!(scan(None, &homes)
            .iter()
            .any(|i| i.name == "pdf-tools" && i.kind == "skill"));
        assert!(
            new_item("skill", "global", "claude-code", "pdf tools", None, &homes).is_err(),
            "no overwrite"
        );
        assert!(new_item("instructions", "project", "codex", "x", Some(&proj), &homes).is_err());
    }
}
