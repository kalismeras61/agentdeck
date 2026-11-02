import { BookOpenText, Check, ChevronRight, Copy, FilePlus2, Folder, Globe, Link2, Plus, RefreshCw, Search, TriangleAlert, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type SetStateAction } from "react";
import { create } from "zustand";
import { AgentMark, cx, Kbd } from "../components/ui";
import { providerLabel } from "../data/demo";
import { useResolvedTheme } from "../hooks/useTheme";
import { MOD } from "../lib/format";
import { errorText, liveApi, type ContextFile, type ContextItem, type LiveProvider } from "../lib/live";
import { useStore } from "../state/store";
import { fmt, t } from "../lib/i18n";
import { ViewHeader } from "./ViewHeader";
import { TextEditor } from "./setup/TextEditor";

type Kind = ContextItem["kind"];

const KINDS: { kind: Kind; title: string; note: string }[] = /* i18n */ [
  { kind: "instructions", title: "Instructions", note: "Read at the start of every session" },
  { kind: "skill", title: "Skills", note: "Loaded when a task needs them" },
  { kind: "command", title: "Commands", note: "Run when you type /name" },
  { kind: "subagent", title: "Subagents", note: "Helpers Claude Code hands work to" },
];

/** Which agents read each kind of file from the project and from the global folders. */
const SUPPORT: Record<Exclude<Kind, "instructions">, { project: LiveProvider[]; global: LiveProvider[] }> = {
  skill: { project: ["claude-code", "codex", "gemini-cli"], global: ["claude-code", "codex", "gemini-cli"] },
  command: { project: ["claude-code", "gemini-cli"], global: ["claude-code", "codex", "gemini-cli"] },
  subagent: { project: ["claude-code"], global: ["claude-code"] },
};

const AGENTS: LiveProvider[] = ["claude-code", "codex", "gemini-cli"];
const GLOBAL = "global";

function home(path: string) {
  return path.replace(/^\/(Users|home)\/[^/]+/, "~").replace(/^\/root(?=\/)/, "~");
}

function readsIt(item: ContextItem, project?: string) {
  const who = fmt.list(item.agents.map((a) => providerLabel[a]));
  const vars = { who, name: item.name, project: project ?? t("this project") };
  const global = item.scope === "global";
  switch (item.kind) {
    case "instructions":
      return global
        ? t("{who} reads this at the start of every session in every project on this Mac.", vars)
        : t("{who} reads this at the start of every session in {project}.", vars);
    case "skill":
      return global
        ? t("{who} loads this skill in every project on this Mac when a task matches its description.", vars)
        : t("{who} loads this skill in {project} when a task matches its description.", vars);
    case "command":
      return global ? t("Type {name} in {who} to run it, in every project on this Mac.", vars) : t("Type {name} in {who} to run it, in {project}.", vars);
    default:
      return global
        ? t("{who} can hand work to this subagent in every project on this Mac.", vars)
        : t("{who} can hand work to this subagent in {project}.", vars);
  }
}

/** The "new item" button's label for a kind, in a scope. */
function newItemLabel(kind: Exclude<Kind, "instructions">, global: boolean, project: string) {
  if (kind === "skill") return global ? t("New skill for every project") : t("New skill in {project}", { project });
  if (kind === "command") return global ? t("New command for every project") : t("New command in {project}", { project });
  return global ? t("New subagent for every project") : t("New subagent in {project}", { project });
}

const NEW_TITLE: Record<Exclude<Kind, "instructions">, string> = /* i18n */ { skill: "New skill", command: "New command", subagent: "New subagent" };

/** Unsaved edits per file; kept while you look at other files or other views. */
const useDrafts = create<{ drafts: Record<string, string>; setDrafts: (u: SetStateAction<Record<string, string>>) => void }>((set) => ({
  drafts: {},
  setDrafts: (u) => set((s) => ({ drafts: typeof u === "function" ? u(s.drafts) : u })),
}));

/**
 * Agent setup: the instruction files, skills, commands and subagents each agent reads, as a tree:
 * the global ones first, then each project folder, opened when you need it.
 */
export function AgentSetup() {
  const projects = useStore((s) => s.projects);
  const setupProjectId = useStore((s) => s.setupProjectId);
  const dark = useResolvedTheme() === "dark";

  const folders = projects.filter((p) => p.workspaceId === "local" && p.kind !== "chats");

  /** Items per scope: "global" or a project id. Undefined = not loaded, null = loading. */
  const [scopes, setScopes] = useState<Record<string, ContextItem[] | null>>({});
  const [scanErrors, setScanErrors] = useState<Record<string, string>>({});
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [agent, setAgent] = useState<"all" | LiveProvider>("all");
  const [query, setQuery] = useState("");
  const [openPath, setOpenPath] = useState<string | null>(null);
  const [file, setFile] = useState<ContextFile | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const drafts = useDrafts((s) => s.drafts);
  const setDrafts = useDrafts((s) => s.setDrafts);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [adding, setAdding] = useState<{ key: string; kind: Exclude<Kind, "instructions"> } | null>(null);
  const [copied, setCopied] = useState(false);

  const scan = useCallback((key: string) => {
    setScopes((s) => ({ ...s, [key]: s[key] ?? null }));
    return liveApi
      .scanContext(key === GLOBAL ? null : key)
      .then((found) => {
        const mine = found.filter((i) => (key === GLOBAL ? i.scope === "global" : i.scope === "project"));
        setScopes((s) => ({ ...s, [key]: mine }));
        setScanErrors(({ [key]: _gone, ...rest }) => rest);
      })
      .catch((e: unknown) => setScanErrors((s) => ({ ...s, [key]: errorText(e) })));
  }, []);

  // Opened from the sidebar: the global files. From a project: that project's.
  useEffect(() => {
    if (setupProjectId) {
      setExpanded((x) => ({ ...x, [GLOBAL]: false, [setupProjectId]: true }));
      void scan(setupProjectId);
      setTimeout(() => document.getElementById(`setup-scope-${setupProjectId}`)?.scrollIntoView({ block: "start" }), 50);
    } else {
      setExpanded((x) => ({ ...x, [GLOBAL]: true }));
    }
    void scan(GLOBAL);
  }, [setupProjectId, scan]);

  const toggle = (key: string) => {
    const open = !expanded[key];
    setExpanded((x) => ({ ...x, [key]: open }));
    if (open && !key.includes(":") && scopes[key] === undefined) void scan(key);
  };

  const open = useCallback((path: string) => {
    setOpenPath(path);
    setFileError(null);
    setSaved(null);
    setFile(null);
    liveApi
      .readContextFile(path)
      .then(setFile)
      .catch((e: unknown) => setFileError(errorText(e)));
  }, []);

  // Bring the open file's row into view (a new item may be far down the list).
  useEffect(() => {
    document.querySelector('[data-setup-list] [aria-current="true"]')?.scrollIntoView({ block: "nearest" });
  }, [openPath, scopes]);

  const all = useMemo(() => Object.values(scopes).flatMap((v) => v ?? []), [scopes]);
  const current = all.find((i) => i.path === openPath);
  const currentKey = Object.entries(scopes).find(([, v]) => v?.some((i) => i.path === openPath))?.[0];
  const currentProject = folders.find((p) => p.id === currentKey);
  const text = openPath ? (drafts[openPath] ?? file?.text ?? "") : "";
  const dirty = Boolean(openPath && file && drafts[openPath] !== undefined && drafts[openPath] !== file.text);

  const save = useCallback(async () => {
    if (!openPath || !file || saving) return;
    if (!dirty && file.exists) return;
    setSaving(true);
    setFileError(null);
    try {
      const f = await liveApi.saveContextFile(openPath, drafts[openPath] ?? file.text, file.modified);
      setFile(f);
      setDrafts((d) => {
        const n = { ...d };
        delete n[openPath];
        return n;
      });
      setSaved(file.exists ? t("Saved") : t("Created"));
      if (currentKey) void scan(currentKey);
    } catch (e: unknown) {
      setFileError(errorText(e));
    }
    setSaving(false);
  }, [openPath, file, saving, dirty, drafts, scan, currentKey, setDrafts]);

  const revert = () => {
    if (!openPath) return;
    setDrafts((d) => {
      const n = { ...d };
      delete n[openPath];
      return n;
    });
    setFileError(null);
  };

  const reload = () => openPath && (revert(), open(openPath));

  const q = query.trim().toLowerCase();
  const matches = (i: ContextItem) =>
    (agent === "all" || i.agents.includes(agent)) &&
    (!q || i.name.toLowerCase().includes(q) || (i.description ?? "").toLowerCase().includes(q) || i.path.toLowerCase().includes(q));

  const counts = useMemo(() => {
    const c: Record<string, number> = { all: 0 };
    for (const i of all) {
      if (!i.exists) continue;
      c.all = (c.all ?? 0) + 1;
      for (const a of i.agents) c[a] = (c[a] ?? 0) + 1;
    }
    return c;
  }, [all]);

  if (!liveApi.available) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title={t("Agent setup")} />
        <p className="m-auto max-w-[44ch] text-center text-[13px] leading-[1.6] text-fg-3">
          {t("Agent setup reads the instruction files, skills and commands on your Mac, so it works in the desktop app.")}
        </p>
      </div>
    );
  }

  const itemRow = (i: ContextItem, kind: Kind) => (
    <li key={i.path}>
      <button
        type="button"
        onClick={() => open(i.path)}
        aria-current={i.path === openPath ? "true" : undefined}
        className={cx("flex w-full items-start gap-2 rounded-lg py-1.5 pl-[42px] pr-2 text-left", i.path === openPath ? "bg-elevated" : "hover:bg-surface")}
      >
        <span className="mt-[1px] flex shrink-0 -space-x-1">
          {i.agents.map((a) => (
            <AgentMark key={a} provider={a} size={15} />
          ))}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5">
            <span className={cx("truncate text-[13px]", i.exists ? "text-fg" : "text-fg-3", kind === "command" && "font-mono text-[12px]")}>{i.name}</span>
            {i.linked && <Link2 size={11} aria-label={t("Linked file")} className="shrink-0 text-fg-3" />}
            {drafts[i.path] !== undefined && <span aria-label={t("Unsaved changes")} className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--violet)]" />}
          </span>
          {!i.exists ? (
            <span className="block text-[11.5px] text-fg-3">{t("Not created yet")}</span>
          ) : (
            i.description && <span className="block truncate text-[11.5px] text-fg-3">{i.description}</span>
          )}
        </span>
      </button>
    </li>
  );

  const scopeNode = (key: string, title: string, sub: string, icon: React.ReactNode) => {
    const items = scopes[key];
    const isOpen = Boolean(expanded[key]);
    const existing = items?.filter((i) => i.exists).length;
    const scope = key === GLOBAL ? "global" : "project";
    return (
      <section key={key} id={`setup-scope-${key}`} className="border-b border-line-soft last:border-b-0">
        <button
          type="button"
          aria-expanded={isOpen}
          onClick={() => toggle(key)}
          className="flex w-full items-center gap-2 px-3 py-2.5 text-left hover:bg-surface"
        >
          <ChevronRight size={13} className={cx("shrink-0 text-fg-3 transition-transform", isOpen && "rotate-90")} />
          {icon}
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13px] font-semibold text-fg">{title}</span>
            <span className="block truncate text-[11px] text-fg-3" title={sub}>
              {sub}
            </span>
          </span>
          {existing !== undefined && <span className="shrink-0 text-[11.5px] tabular-nums text-fg-3">{existing}</span>}
        </button>
        {isOpen && (
          <div className="pb-2">
            {scanErrors[key] ? (
              <p role="alert" className="px-9 py-1 text-[12px] text-error">
                {scanErrors[key]}
              </p>
            ) : !items ? (
              <p className="px-9 py-1 text-[12px] text-fg-3">{t("Looking in the agents' folders…")}</p>
            ) : (
              KINDS.map(({ kind, title: kt, note }) => {
                const list = items.filter((i) => i.kind === kind && matches(i));
                const addable = kind !== "instructions" && (agent === "all" || SUPPORT[kind][scope].includes(agent));
                if (list.length === 0 && !addable) return null;
                const kKey = `${key}:${kind}`;
                const kOpen = expanded[kKey] ?? (q !== "" || kind === "instructions" || list.length <= 6);
                const isAdding = adding?.key === key && adding.kind === kind;
                return (
                  <div key={kind}>
                    <div className="group flex items-center pr-2">
                      <button
                        type="button"
                        aria-expanded={kOpen}
                        onClick={() => setExpanded((x) => ({ ...x, [kKey]: !kOpen }))}
                        className="flex min-w-0 flex-1 items-center gap-1.5 py-1 pl-7 pr-1 text-left"
                        title={t(note)}
                      >
                        <ChevronRight size={11} className={cx("shrink-0 text-fg-3 transition-transform", kOpen && "rotate-90")} />
                        <span className="text-[12px] font-medium text-fg-2">{t(kt)}</span>
                        <span className="text-[11px] tabular-nums text-fg-3">{list.filter((i) => i.exists).length}</span>
                      </button>
                      {kind !== "instructions" && addable && (
                        <button
                          type="button"
                          onClick={() => {
                            setAdding({ key, kind });
                            setExpanded((x) => ({ ...x, [kKey]: true }));
                          }}
                          aria-label={newItemLabel(kind, scope === "global", title)}
                          title={t(NEW_TITLE[kind])}
                          className="rounded p-0.5 text-fg-3 opacity-0 hover:bg-elevated hover:text-fg focus-visible:opacity-100 group-hover:opacity-100"
                        >
                          <Plus size={13} />
                        </button>
                      )}
                    </div>
                    {kOpen && (
                      <>
                        {isAdding && (
                          <NewItemForm
                            kind={kind}
                            scope={scope}
                            projectId={key === GLOBAL ? null : key}
                            preferAgent={agent === "all" ? undefined : agent}
                            onCancel={() => setAdding(null)}
                            onCreated={(f) => {
                              setAdding(null);
                              void scan(key).then(() => open(f.path));
                            }}
                          />
                        )}
                        <ul className="space-y-px px-1">{list.map((i) => itemRow(i, kind))}</ul>
                        {list.length === 0 && !isAdding && <p className="pb-1 pl-[42px] text-[11.5px] text-fg-3">{t("None yet")}</p>}
                      </>
                    )}
                  </div>
                );
              })
            )}
          </div>
        )}
      </section>
    );
  };

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title={t("Agent setup")}>
        <button
          type="button"
          onClick={() => Object.keys(scopes).forEach((k) => void scan(k))}
          aria-label={t("Look again")}
          title={t("Look again")}
          className="no-drag rounded-md p-1.5 text-fg-3 hover:bg-elevated hover:text-fg"
        >
          <RefreshCw size={14} />
        </button>
      </ViewHeader>

      <div className="flex min-h-0 flex-1">
        {/* The tree */}
        <div className="flex w-[340px] shrink-0 flex-col border-r border-line-soft">
          <div className="space-y-2.5 border-b border-line-soft px-3 py-3">
            <div role="radiogroup" aria-label={t("Agent")} className="flex flex-wrap gap-1">
              {(["all", ...AGENTS] as const).map((a) => (
                <button
                  key={a}
                  type="button"
                  role="radio"
                  aria-checked={agent === a}
                  onClick={() => setAgent(a)}
                  className={cx(
                    "inline-flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-[12px]",
                    agent === a
                      ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-fg"
                      : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
                  )}
                >
                  {a !== "all" && <AgentMark provider={a} size={13} />}
                  {a === "all" ? t("All") : providerLabel[a]}
                  <span className="tabular-nums text-fg-3">{all.length ? (counts[a] ?? 0) : ""}</span>
                </button>
              ))}
            </div>
            <label className="flex h-7 items-center gap-2 rounded-md border border-line bg-bg px-2 focus-within:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))]">
              <Search size={13} className="text-fg-3" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("Filter by name or description")}
                aria-label={t("Filter")}
                className="min-w-0 flex-1 bg-transparent text-[12.5px] text-fg outline-none placeholder:text-fg-3"
              />
              {query && (
                <button type="button" aria-label={t("Clear filter")} onClick={() => setQuery("")} className="text-fg-3 hover:text-fg">
                  <X size={12} />
                </button>
              )}
            </label>
          </div>

          <div data-setup-list className="scroll-quiet min-h-0 flex-1 overflow-y-auto pb-6">
            {scopeNode(GLOBAL, t("Global"), t("Every project on this Mac"), <Globe size={14} className="shrink-0 text-violet-text" />)}
            {folders.length > 0 && <p className="px-3 pb-1 pt-3 text-[11px] text-fg-3">{t("Projects")}</p>}
            {folders.map((p) => scopeNode(p.id, p.name, home(p.path), <Folder size={14} className="shrink-0 text-fg-3" />))}
          </div>
        </div>

        {/* The editor */}
        <div className="flex min-w-0 flex-1 flex-col">
          {!openPath || !current ? (
            <div className="m-auto max-w-[52ch] px-8 text-center">
              <BookOpenText size={22} className="mx-auto text-fg-3" />
              <p className="mt-3 text-[14px] text-fg">{t("What your agents read before they work")}</p>
              <p className="mt-1.5 text-[13px] leading-[1.6] text-fg-3">
                {t(
                  "The global instructions, skills and commands every project gets, and each project's own: open a project on the left to see them. Pick a file to read or change it; saving writes it where the agent looks for it.",
                )}
              </p>
            </div>
          ) : (
            <>
              <div className="border-b border-line-soft px-5 py-3">
                <div className="flex items-start gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <h2 className={cx("truncate text-[15px] font-semibold", current.kind === "command" && "font-mono text-[14px]")}>{current.name}</h2>
                      <span className={cx("rounded-full px-2 py-px text-[11px]", current.scope === "global" ? "bg-[color-mix(in_srgb,var(--warning)_14%,transparent)] text-warning" : "bg-elevated text-fg-2")}>
                        {current.scope === "global" ? t("Global") : t("Project")}
                      </span>
                    </div>
                    <p className="mt-1 text-[12.5px] leading-[1.5] text-fg-2">{readsIt(current, currentProject?.name)}</p>
                    <div className="mt-1.5 flex min-w-0 items-center gap-1.5">
                      <code className="truncate font-mono text-[11.5px] text-fg-3" title={current.path}>
                        {home(current.path)}
                      </code>
                      <button
                        type="button"
                        aria-label={t("Copy path")}
                        title={t("Copy path")}
                        onClick={() => {
                          void navigator.clipboard?.writeText(current.path).then(() => {
                            setCopied(true);
                            setTimeout(() => setCopied(false), 1200);
                          });
                        }}
                        className="shrink-0 rounded p-0.5 text-fg-3 hover:bg-elevated hover:text-fg"
                      >
                        {copied ? <Check size={12} /> : <Copy size={12} />}
                      </button>
                      {current.linked && <span className="shrink-0 text-[11.5px] text-fg-3">{t("Linked: saving changes the file it points to.")}</span>}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    {saved && !dirty && (
                      <span role="status" className="text-[12px] text-success">
                        {saved}
                      </span>
                    )}
                    {dirty && (
                      <button type="button" onClick={revert} className="h-8 rounded-lg border border-line px-3 text-[13px] text-fg-2 hover:bg-elevated hover:text-fg">
                        {t("Revert")}
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => void save()}
                      disabled={!file || saving || (!dirty && file.exists)}
                      className="inline-flex h-8 items-center gap-2 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
                    >
                      {file && !file.exists ? <FilePlus2 size={14} /> : null}
                      {saving ? t("Saving…") : file && !file.exists ? t("Create") : t("Save")}
                      {file?.exists && <Kbd>{MOD}S</Kbd>}
                    </button>
                  </div>
                </div>
                {current.scope === "global" && dirty && (
                  <p className="mt-2 flex items-center gap-1.5 text-[12px] text-warning">
                    <TriangleAlert size={13} /> {t("This changes every project that uses {agents} on this Mac.", { agents: fmt.list(current.agents.map((a) => providerLabel[a])) })}
                  </p>
                )}
                {fileError && (
                  <p role="alert" className="mt-2 flex flex-wrap items-center gap-2 text-[12.5px] text-error">
                    {fileError}
                    {/changed on disk/.test(fileError) && (
                      <button type="button" onClick={reload} className="rounded-md border border-line px-2 py-0.5 text-[12px] text-fg hover:bg-elevated">
                        {t("Reload")}
                      </button>
                    )}
                  </p>
                )}
              </div>
              <div className="relative min-h-0 flex-1">
                {file ? (
                  <TextEditor
                    docKey={openPath}
                    value={text}
                    language={openPath.endsWith(".toml") ? "ini" : "markdown"}
                    dark={dark}
                    label={current.name}
                    onChange={(next) => {
                      setSaved(null);
                      setDrafts((d) => (next === file.text ? (({ [openPath]: _, ...rest }) => rest)(d) : { ...d, [openPath]: next }));
                    }}
                    onSave={() => void save()}
                  />
                ) : (
                  !fileError && <p className="p-6 text-[13px] text-fg-3">{t("Opening…")}</p>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function NewItemForm({
  kind,
  scope,
  projectId,
  preferAgent,
  onCancel,
  onCreated,
}: {
  kind: Exclude<Kind, "instructions">;
  scope: "project" | "global";
  projectId: string | null;
  preferAgent?: LiveProvider;
  onCancel: () => void;
  onCreated: (f: ContextFile) => void;
}) {
  const allowed = SUPPORT[kind][scope];
  const [agent, setAgent] = useState<LiveProvider>(preferAgent && allowed.includes(preferAgent) ? preferAgent : allowed[0]!);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  return (
    <form
      className="mb-1 ml-[42px] mr-2 mt-1 space-y-2.5 rounded-xl border border-line bg-surface p-3"
      onSubmit={(e) => {
        e.preventDefault();
        setBusy(true);
        setError(null);
        liveApi
          .newContextItem(scope === "project" ? projectId : null, kind, scope, agent, name)
          .then(onCreated)
          .catch((er: unknown) => {
            setError(errorText(er));
            setBusy(false);
          });
      }}
    >
      <label className="block">
        <span className="text-[11.5px] text-fg-3">{kind === "command" ? t("Command name (what you type after /)") : kind === "skill" ? t("Name of the skill") : t("Name of the subagent")}</span>
        <input
          ref={input}
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onCancel()}
          placeholder={kind === "skill" ? "pdf-tools" : kind === "command" ? "review" : "test-runner"}
          spellCheck={false}
          className="mt-1 h-7 w-full rounded-md border border-line bg-bg px-2 font-mono text-[12px] text-fg outline-none focus:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))]"
        />
      </label>
      {SUPPORT[kind][scope].length > 1 && (
        <div className="flex flex-wrap gap-1" role="radiogroup" aria-label={t("Agent")}>
          {SUPPORT[kind][scope].map((a) => (
            <button
              key={a}
              type="button"
              role="radio"
              aria-checked={agent === a}
              onClick={() => setAgent(a)}
              className={cx(
                "inline-flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-[12px]",
                agent === a ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-fg" : "border-line text-fg-2 hover:bg-elevated",
              )}
            >
              <AgentMark provider={a} size={13} /> {providerLabel[a]}
            </button>
          ))}
        </div>
      )}
      {error && (
        <p role="alert" className="text-[12px] text-error">
          {error}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button type="button" onClick={onCancel} className="h-7 rounded-md px-2.5 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg">
          {t("Cancel")}
        </button>
        <button
          type="submit"
          disabled={busy || !name.trim()}
          className="h-7 rounded-md bg-violet-strong px-3 text-[12.5px] font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          {busy ? t("Creating…") : kind === "skill" ? t("Create skill") : kind === "command" ? t("Create command") : t("Create subagent")}
        </button>
      </div>
    </form>
  );
}
