import { Check, ChevronRight, Copy, FolderGit2, GitBranch, GitCommitHorizontal, RefreshCw, Tag } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cx } from "../components/ui";
import { useResolvedTheme } from "../hooks/useTheme";
import { ago } from "../lib/format";
import { layoutGraph, type GraphRow } from "../lib/gitGraph";
import { errorText, gitApi, liveApi, type GitChangedFile, type GitCommit, type GitFilePair, type GitFileStatus, type GitOverview } from "../lib/live";
import { useStore } from "../state/store";
import { MonacoDiff } from "./review/MonacoDiff";
import { ViewHeader } from "./ViewHeader";

const PAGE = 300;
const ROW = 30;
const LANE = 14;
/** Lane colors; the first is the main line. */
const COLORS = ["var(--violet)", "var(--blue)", "var(--success)", "var(--warning)", "var(--error)", "#f472b6", "#22d3ee", "#a3e635"];
const colorOf = (n: number) => COLORS[n % COLORS.length]!;
const LANG: Record<string, string> = { dart: "dart", json: "json", yaml: "yaml", yml: "yaml", ts: "typescript", tsx: "typescript", js: "javascript", jsx: "javascript", md: "markdown", css: "css", html: "html", py: "python", rs: "rust", go: "go", java: "java", kt: "kotlin", swift: "swift", sql: "sql", sh: "shell" };
const langOf = (p: string) => LANG[p.split(".").pop()?.toLowerCase() ?? ""] ?? "plaintext";
const home = (p: string) => p.replace(/^\/(Users|home)\/[^/]+/, "~");
const UNCOMMITTED = "uncommitted";

type Change = { path: string; change: string; from: string | null };

function letter(change: string) {
  switch (change) {
    case "added":
    case "untracked":
      return ["A", "text-success"];
    case "deleted":
      return ["D", "text-error"];
    case "renamed":
      return ["R", "text-blue"];
    case "conflict":
      return ["!", "text-error"];
    default:
      return ["M", "text-warning"];
  }
}

function GraphCell({ row, width }: { row: GraphRow; width: number }) {
  const x = (lane: number) => lane * LANE + LANE / 2 + 4;
  const y = (part: 0 | 1 | 2) => (part === 0 ? 0 : part === 1 ? ROW / 2 : ROW);
  return (
    <svg width={width * LANE + 8} height={ROW} className="shrink-0" aria-hidden>
      {row.segments.map((s, i) => {
        const x1 = x(s.from);
        const x2 = x(s.to);
        const y1 = y(s.y1);
        const y2 = y(s.y2);
        const d = x1 === x2 ? `M${x1} ${y1}L${x2} ${y2}` : `M${x1} ${y1}C${x1} ${(y1 + y2) / 2} ${x2} ${(y1 + y2) / 2} ${x2} ${y2}`;
        return <path key={i} d={d} stroke={colorOf(s.color)} strokeWidth={1.6} fill="none" />;
      })}
      <circle cx={x(row.lane)} cy={ROW / 2} r={4} fill="var(--bg)" stroke={colorOf(row.color)} strokeWidth={2} />
    </svg>
  );
}

function RefChip({ name, current }: { name: string; current: boolean }) {
  const tag = name.startsWith("tag: ");
  const label = tag ? name.slice(5) : name.replace(/^HEAD -> /, "");
  const remote = !tag && label.includes("/") && !label.startsWith("agentdeck/");
  return (
    <span
      className={cx(
        "inline-flex max-w-[150px] min-w-0 shrink items-center gap-1 rounded-full border px-1.5 py-px font-mono text-[10.5px]",
        current
          ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-fg"
          : tag
            ? "border-[color-mix(in_srgb,var(--warning)_45%,var(--border))] text-warning"
            : remote
              ? "border-line text-fg-3"
              : "border-line text-fg-2",
      )}
    >
      {tag ? <Tag size={9} /> : <GitBranch size={9} />}
      <span className="truncate">{label}</span>
    </span>
  );
}

/** Git: branches, worktrees, the commit graph, and what each commit changed. */
export function GitView() {
  const projects = useStore((s) => s.projects);
  const sessions = useStore((s) => s.sessions);
  const selectedId = useStore((s) => s.selectedId);
  const target = useStore((s) => s.gitTarget);
  const openGit = useStore((s) => s.openGit);
  const select = useStore((s) => s.select);
  const dark = useResolvedTheme() === "dark";

  const folders = projects.filter((p) => p.workspaceId === "local" && p.kind !== "chats");
  const fallback = sessions.find((s) => s.id === selectedId)?.projectId;
  const projectId = target.projectId ?? (folders.some((p) => p.id === fallback) ? fallback! : (folders[0]?.id ?? null));
  const project = folders.find((p) => p.id === projectId);
  const dir = target.dir;

  const [overview, setOverview] = useState<GitOverview | null>(null);
  const [commits, setCommits] = useState<GitCommit[]>([]);
  const [more, setMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [picked, setPicked] = useState<string | null>(null);
  const [files, setFiles] = useState<Change[] | null>(null);
  const [file, setFile] = useState<Change | null>(null);
  const [pair, setPair] = useState<GitFilePair | null>(null);
  const [copied, setCopied] = useState(false);
  const [open, setOpen] = useState<Record<string, boolean>>({ branches: true, worktrees: true, tags: false, remotes: false });
  const list = useRef<HTMLDivElement>(null);

  const load = useCallback(() => {
    if (!projectId) return;
    setLoading(true);
    setError(null);
    Promise.all([gitApi.overview(projectId, dir), gitApi.log(projectId, 0, PAGE)])
      .then(([o, c]) => {
        setOverview(o);
        setCommits(c);
        setMore(c.length === PAGE);
        setPicked((p) => p ?? (o.status.length ? UNCOMMITTED : (c[0]?.hash ?? null)));
      })
      .catch((e: unknown) => {
        setOverview(null);
        setCommits([]);
        setError(errorText(e));
      })
      .finally(() => setLoading(false));
  }, [projectId, dir]);

  useEffect(() => {
    setPicked(null);
    setFiles(null);
    setFile(null);
    load();
  }, [load]);

  const loadMore = () => {
    if (!projectId) return;
    void gitApi.log(projectId, commits.length, PAGE).then((c) => {
      setCommits((cur) => [...cur, ...c]);
      setMore(c.length === PAGE);
    });
  };

  const rows = useMemo(() => layoutGraph(commits), [commits]);
  const width = useMemo(() => Math.max(1, ...rows.map((r) => r.width)), [rows]);
  const commit = commits.find((c) => c.hash === picked);
  const status: GitFileStatus[] = overview?.status ?? [];
  const folder = dir ?? project?.path ?? null;

  // Files of the picked commit (or the uncommitted ones).
  useEffect(() => {
    setFile(null);
    setPair(null);
    if (!projectId || !picked) return setFiles(null);
    if (picked === UNCOMMITTED) return setFiles(status.map((s) => ({ path: s.path, change: s.change, from: s.from })));
    setFiles(null);
    gitApi
      .commitFiles(projectId, picked)
      .then((f: GitChangedFile[]) => setFiles(f))
      .catch((e: unknown) => setError(errorText(e)));
  }, [projectId, picked, overview]); // eslint-disable-line react-hooks/exhaustive-deps

  // The picked file before and after.
  useEffect(() => {
    if (!projectId || !file || !picked) return setPair(null);
    setPair(null);
    const parent = picked === UNCOMMITTED ? "HEAD" : (commit?.parents[0] ?? "");
    const rev = picked === UNCOMMITTED ? "WORKTREE" : picked;
    gitApi
      .filePair(projectId, picked === UNCOMMITTED ? dir : null, file.change === "added" || file.change === "untracked" ? "" : parent, file.change === "deleted" ? "" : rev, file.path, file.from)
      .then(setPair)
      .catch((e: unknown) => setError(errorText(e)));
  }, [projectId, file, picked]); // eslint-disable-line react-hooks/exhaustive-deps

  const sessionAt = (path: string) => sessions.find((s) => s.worktree === path);
  const jumpTo = (hash: string) => {
    setPicked(hash);
    const i = commits.findIndex((c) => c.hash === hash);
    if (i >= 0 && list.current) list.current.scrollTo({ top: Math.max(0, (i + (status.length ? 1 : 0)) * ROW - 120) });
  };

  if (!liveApi.available) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Git" />
        <p className="m-auto max-w-[44ch] text-center text-[13px] leading-[1.6] text-fg-3">The Git view reads repositories on your Mac, so it works in the desktop app.</p>
      </div>
    );
  }

  const section = (key: string, title: string, count: number, body: React.ReactNode) => (
    <section className="pt-2">
      <button
        type="button"
        aria-expanded={open[key]}
        onClick={() => setOpen((o) => ({ ...o, [key]: !o[key] }))}
        className="flex w-full items-center gap-1.5 px-3 py-1 text-left text-[12px] font-semibold text-fg"
      >
        <ChevronRight size={12} className={cx("text-fg-3 transition-transform", open[key] && "rotate-90")} />
        {title}
        <span className="font-normal tabular-nums text-fg-3">{count}</span>
      </button>
      {open[key] && <div className="px-1.5 pb-1">{body}</div>}
    </section>
  );

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Git">
        <label className="flex items-center gap-2 text-[12.5px] text-fg-3">
          Project
          <select
            value={projectId ?? ""}
            onChange={(e) => openGit(e.target.value || null)}
            className="h-7 max-w-[200px] rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg outline-none"
          >
            {folders.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        {overview && overview.worktrees.length > 1 && (
          <label className="flex items-center gap-2 text-[12.5px] text-fg-3">
            Folder
            <select
              value={dir ?? ""}
              onChange={(e) => openGit(projectId, e.target.value || null)}
              className="h-7 max-w-[220px] rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg outline-none"
            >
              {overview.worktrees.map((w) => (
                <option key={w.path} value={w.main ? "" : w.path}>
                  {w.main ? "Project folder" : (w.branch ?? home(w.path))}
                </option>
              ))}
            </select>
          </label>
        )}
        <button type="button" onClick={load} aria-label="Refresh" title="Refresh" className="no-drag rounded-md p-1.5 text-fg-3 hover:bg-elevated hover:text-fg">
          <RefreshCw size={14} className={cx(loading && "animate-spin motion-reduce:animate-none")} />
        </button>
      </ViewHeader>

      {error && !overview ? (
        <div className="m-auto max-w-[52ch] px-8 text-center">
          <FolderGit2 size={22} className="mx-auto text-fg-3" />
          <p className="mt-3 text-[14px] text-fg">{project ? `${project.name} isn't a git repository` : "No project yet"}</p>
          <p className="mt-1.5 text-[13px] leading-[1.6] text-fg-3">{project ? `${error}. Run git init in ${home(project.path)} to see its history here.` : "Add a project folder to see its history."}</p>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1">
          {/* Branches, worktrees, tags */}
          <aside aria-label="Branches and worktrees" className="scroll-quiet w-[220px] shrink-0 overflow-y-auto border-r border-line-soft pb-4">
            {overview &&
              section(
                "branches",
                "Branches",
                overview.branches.length,
                overview.branches.map((b) => (
                  <button
                    key={b.name}
                    type="button"
                    onClick={() => jumpTo(b.hash)}
                    title={b.upstream ? `Tracks ${b.upstream}` : undefined}
                    className={cx("flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left hover:bg-surface", picked === b.hash && "bg-elevated")}
                  >
                    {b.current ? <Check size={12} className="shrink-0 text-violet-text" /> : <GitBranch size={12} className="shrink-0 text-fg-3" />}
                    <span className={cx("truncate font-mono text-[12px]", b.current ? "text-fg" : "text-fg-2")}>{b.name}</span>
                    <span className="ml-auto flex shrink-0 gap-1 text-[10.5px] tabular-nums text-fg-3">
                      {b.ahead > 0 && <span title={`${b.ahead} to push`}>↑{b.ahead}</span>}
                      {b.behind > 0 && <span title={`${b.behind} to pull`}>↓{b.behind}</span>}
                      {b.worktree && !b.current && <FolderGit2 size={11} aria-label="Checked out in a worktree" />}
                    </span>
                  </button>
                )),
              )}
            {overview &&
              section(
                "worktrees",
                "Worktrees",
                overview.worktrees.length,
                overview.worktrees.map((w) => {
                  const s = sessionAt(w.path);
                  const here = (dir ?? project?.path) === w.path || (!dir && w.main);
                  return (
                    <div key={w.path} className={cx("rounded-md px-2 py-1", here && "bg-elevated")}>
                      <button type="button" onClick={() => openGit(projectId, w.main ? null : w.path)} className="block w-full text-left">
                        <span className="flex items-center gap-1.5 text-[12px] text-fg">
                          <FolderGit2 size={12} className="shrink-0 text-fg-3" />
                          <span className="truncate">{w.main ? "Project folder" : (w.branch ?? "detached")}</span>
                        </span>
                        <span className="block truncate pl-[18px] font-mono text-[10.5px] text-fg-3" title={w.path}>
                          {home(w.path)}
                        </span>
                      </button>
                      {s && (
                        <button type="button" onClick={() => select(s.id)} className="ml-[18px] mt-0.5 truncate text-left text-[11.5px] text-violet-text hover:underline">
                          {s.title}
                        </button>
                      )}
                    </div>
                  );
                }),
              )}
            {overview &&
              overview.tags.length > 0 &&
              section(
                "tags",
                "Tags",
                overview.tags.length,
                overview.tags.slice(0, 100).map((t) => (
                  <div key={t} className="flex items-center gap-1.5 px-2 py-0.5 font-mono text-[11.5px] text-fg-2">
                    <Tag size={11} className="shrink-0 text-fg-3" />
                    <span className="truncate">{t}</span>
                  </div>
                )),
              )}
            {overview &&
              overview.remotes.length > 0 &&
              section(
                "remotes",
                "Remote branches",
                overview.remotes.length,
                overview.remotes.map((b) => (
                  <button key={b.name} type="button" onClick={() => jumpTo(b.hash)} className="flex w-full items-center gap-1.5 rounded-md px-2 py-0.5 text-left hover:bg-surface">
                    <GitBranch size={11} className="shrink-0 text-fg-3" />
                    <span className="truncate font-mono text-[11.5px] text-fg-3">{b.name}</span>
                  </button>
                )),
              )}
          </aside>

          {/* History */}
          <div ref={list} role="list" aria-label="Commits" className="scroll-quiet min-w-0 flex-1 overflow-y-auto">
            {status.length > 0 && (
              <button
                type="button"
                role="listitem"
                onClick={() => setPicked(UNCOMMITTED)}
                className={cx("flex w-full items-center gap-2 border-b border-line-soft px-3 text-left", picked === UNCOMMITTED ? "bg-elevated" : "hover:bg-surface")}
                style={{ height: ROW }}
              >
                <span className="flex shrink-0 justify-center" style={{ width: width * LANE + 8 }}>
                  <span className="h-2.5 w-2.5 rounded-full border-2 border-dashed border-fg-3" />
                </span>
                <span className="truncate text-[13px] text-fg">Uncommitted changes</span>
                <span className="text-[12px] text-fg-3">
                  {status.length} {status.length === 1 ? "file" : "files"}
                  {dir ? ` in ${home(dir)}` : ""}
                </span>
              </button>
            )}
            {commits.map((c, i) => {
              const row = rows[i]!;
              const isHead = c.hash === overview?.head;
              return (
                <button
                  key={c.hash}
                  type="button"
                  role="listitem"
                  onClick={() => setPicked(c.hash)}
                  aria-current={picked === c.hash ? "true" : undefined}
                  className={cx("flex w-full items-center gap-2 px-3 text-left", picked === c.hash ? "bg-elevated" : "hover:bg-surface")}
                  style={{ height: ROW }}
                >
                  <GraphCell row={row} width={width} />
                  <span className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden">
                    {c.refs.map((r) => (
                      <RefChip key={r} name={r} current={r.startsWith("HEAD -> ") || (isHead && r === "HEAD")} />
                    ))}
                    <span className={cx("truncate text-[13px]", c.parents.length > 1 ? "text-fg-2" : "text-fg")}>{c.subject}</span>
                  </span>
                  <span className="hidden w-[110px] shrink-0 truncate text-[12px] text-fg-3 2xl:block">{c.author}</span>
                  <span className="w-[64px] shrink-0 text-right text-[12px] tabular-nums text-fg-3">{ago(c.time)}</span>
                  <span className="w-[58px] shrink-0 text-right font-mono text-[11px] text-fg-3">{c.hash.slice(0, 7)}</span>
                </button>
              );
            })}
            {commits.length === 0 && !loading && overview && <p className="p-8 text-center text-[13px] text-fg-3">No commits yet.</p>}
            {more && commits.length > 0 && (
              <button type="button" onClick={loadMore} className="m-3 h-8 rounded-lg border border-line px-3 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg">
                Show older commits
              </button>
            )}
          </div>

          {/* What changed */}
          <section aria-label="Commit details" className="flex w-[38%] min-w-[340px] max-w-[720px] shrink-0 flex-col border-l border-line-soft">
            {!picked ? (
              <p className="m-auto px-8 text-center text-[13px] text-fg-3">Pick a commit to see what it changed.</p>
            ) : (
              <>
                <div className="border-b border-line-soft px-4 py-3">
                  {picked === UNCOMMITTED ? (
                    <>
                      <p className="text-[14px] font-semibold">Uncommitted changes</p>
                      <p className="mt-0.5 font-mono text-[11.5px] text-fg-3">{folder ? home(folder) : ""}</p>
                    </>
                  ) : commit ? (
                    <>
                      <p className="text-[14px] font-semibold leading-[1.4] [text-wrap:balance]">{commit.subject}</p>
                      <p className="mt-1 flex flex-wrap items-center gap-x-2 text-[12px] text-fg-3">
                        <span className="text-fg-2">{commit.author}</span>
                        <span>{new Date(commit.time).toLocaleString()}</span>
                        <button
                          type="button"
                          onClick={() => {
                            void navigator.clipboard?.writeText(commit.hash).then(() => {
                              setCopied(true);
                              setTimeout(() => setCopied(false), 1200);
                            });
                          }}
                          title="Copy the full hash"
                          className="inline-flex items-center gap-1 font-mono text-[11.5px] hover:text-fg"
                        >
                          <GitCommitHorizontal size={12} />
                          {commit.hash.slice(0, 10)}
                          {copied ? <Check size={11} /> : <Copy size={11} />}
                        </button>
                        {commit.parents.length > 1 && <span>Merge of {commit.parents.length} parents</span>}
                      </p>
                    </>
                  ) : null}
                </div>
                <ul className="scroll-quiet max-h-[34%] shrink-0 overflow-y-auto border-b border-line-soft py-1">
                  {files === null ? (
                    <li className="px-4 py-2 text-[12.5px] text-fg-3">Reading the changes…</li>
                  ) : files.length === 0 ? (
                    <li className="px-4 py-2 text-[12.5px] text-fg-3">No file changes.</li>
                  ) : (
                    files.map((f) => {
                      const [l, cls] = letter(f.change);
                      return (
                        <li key={f.path}>
                          <button
                            type="button"
                            onClick={() => setFile(f)}
                            className={cx("flex w-full items-center gap-2 px-4 py-1 text-left", file?.path === f.path ? "bg-elevated" : "hover:bg-surface")}
                          >
                            <span className={cx("w-3 shrink-0 text-center font-mono text-[11.5px]", cls)}>{l}</span>
                            <span className="truncate font-mono text-[12px] text-fg-2">{f.path}</span>
                            {f.from && <span className="truncate font-mono text-[11px] text-fg-3">from {f.from}</span>}
                          </button>
                        </li>
                      );
                    })
                  )}
                </ul>
                <div className="relative min-h-0 flex-1">
                  {!file ? (
                    <p className="p-6 text-center text-[12.5px] text-fg-3">{files?.length ? "Pick a file to see the diff." : ""}</p>
                  ) : !pair ? (
                    <p className="p-6 text-center text-[12.5px] text-fg-3">Opening {file.path}…</p>
                  ) : pair.binary ? (
                    <p className="p-6 text-center text-[12.5px] text-fg-3">{file.path} is a binary or very large file.</p>
                  ) : (
                    <MonacoDiff original={pair.old} modified={pair.new} language={langOf(file.path)} sideBySide={false} fileOnly={false} dark={dark} label={file.path} />
                  )}
                </div>
              </>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
