import { withoutHidden } from "../lib/setup";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import {
  BookOpenText,
  Check,
  ChevronDown,
  ChevronRight,
  Download,
  FolderGit2,
  GitBranch,
  GitGraph,
  FolderPlus,
  GitCompareArrows,
  LayoutList,
  MessageCircle,
  Monitor,
  PanelLeftClose,
  Plus,
  Search,
  Settings,
  Shapes,
  ShieldAlert,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { providerLabel, workspaces, type Session } from "../data/demo";
import { useWorkspaceProjects, useWorkspaceSessions } from "../hooks/useWorkspace";
import { t } from "../lib/i18n";
import { liveApi } from "../lib/live";
import { ago, MOD } from "../lib/format";
import { useAllChanges, usePermissions } from "../hooks/useDerived";
import { useStore, type AppView } from "../state/store";
import { AgentMark, cx, IconButton, Kbd, StatusDot, statusLabel, Tip } from "./ui";
import { NewProjectDialog } from "./NewProjectDialog";
import { ImportDialog } from "./ImportDialog";
import { RenameField } from "./RenameField";
import { UpdateNotice } from "./UpdateNotice";

function Logo() {
  return (
    <svg width="22" height="22" viewBox="0 0 32 32" aria-hidden>
      <rect width="32" height="32" rx="8" fill="var(--elevated)" />
      <path d="M9 22 16 9l7 13" fill="none" stroke="var(--violet)" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M12 18h8" stroke="var(--text)" strokeWidth="2.6" strokeLinecap="round" />
    </svg>
  );
}

const itemCls = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface data-[disabled]:opacity-45";

export function Sidebar() {
  const sessions = useWorkspaceSessions();
  const projects = useWorkspaceProjects();
  const addProjectFolder = useStore((s) => s.addProjectFolder);
  const liveState = useStore((s) => s.liveState);
  const liveError = useStore((s) => s.liveError);
  const selectedId = useStore((s) => s.selectedId);
  const select = useStore((s) => s.select);
  const search = useStore((s) => s.search);
  const setSearch = useStore((s) => s.setSearch);
  const newSession = useStore((s) => s.newSession);
  const newChat = useStore((s) => s.newChat);
  const setNewProjectOpen = useStore((s) => s.setNewProjectOpen);
  const setImportOpen = useStore((s) => s.setImportOpen);
  const profiles = useStore((s) => s.profiles);
  const workspaceId = useStore((s) => s.workspaceId);
  const setWorkspace = useStore((s) => s.setWorkspace);
  const toggleSidebar = useStore((s) => s.toggleSidebar);
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const openSetup = useStore((s) => s.openSetup);
  const openGit = useStore((s) => s.openGit);
  const agyEnabled = useStore((s) => s.agyEnabled);
  const { pending } = usePermissions();
  const changes = useAllChanges();
  const changedFiles = changes.reduce((n, c) => n + c.files.length, 0);
  const searchTick = useStore((s) => s.searchFocusTick);
  const searchRef = useRef<HTMLInputElement>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const ws = workspaces.find((w) => w.id === workspaceId) ?? workspaces[0]!;
  const isLocal = ws.id === "local";
  const hasProjects = projects.length > 0;
  const canStart = hasProjects || isLocal;

  useEffect(() => {
    if (searchTick > 0) {
      searchRef.current?.focus();
      searchRef.current?.select();
    }
  }, [searchTick]);

  const groups = useMemo(() => {
    const q = search.trim().toLowerCase();
    // Chats (sessions without a project) first, then project folders.
    const ordered = [...projects.filter((p) => p.kind === "chats"), ...projects.filter((p) => p.kind !== "chats")];
    const out = ordered
      .map((p) => ({
        project: p as (typeof projects)[number] | null,
        sessions: sessions.filter(
          (s) => s.projectId === p.id && (!q || s.title.toLowerCase().includes(q) || s.snippet.toLowerCase().includes(q) || p.name.toLowerCase().includes(q)),
        ),
      }))
      .filter((g) => g.sessions.length > 0 || (isLocal && !q));
    // Before the first chat there's no Chats folder yet; still show where chats go.
    if (isLocal && !q && !projects.some((p) => p.kind === "chats")) out.unshift({ project: null, sessions: [] });
    return out;
  }, [sessions, projects, search, isLocal]);

  return (
    <nav aria-label={t("Sessions")} className="flex h-full flex-col bg-sidebar">
      {/* Title bar row: space for macOS window controls */}
      <div data-tauri-drag-region className="drag relative flex h-12 shrink-0 items-center pl-[78px] pr-2">
        <span aria-hidden dir="ltr" className="absolute left-4 top-[18px] flex gap-2 opacity-90" data-web-only>
          <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
          <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
          <span className="h-3 w-3 rounded-full bg-[#28c840]" />
        </span>
        <IconButton label={t("Hide sidebar")} shortcut={`${MOD}B`} onClick={toggleSidebar} className="ms-auto" size="sm">
          <PanelLeftClose size={16} />
        </IconButton>
      </div>

      <div className="space-y-2 px-3 pb-3">
        <Dropdown.Root>
          <Dropdown.Trigger className="flex w-full items-center gap-2.5 rounded-lg px-1.5 py-1.5 text-start hover:bg-surface data-[state=open]:bg-surface">
            <Logo />
            <span className="min-w-0 flex-1">
              <span className="block text-[13.5px] font-semibold leading-tight">AgentDeck</span>
              <span className="block truncate text-xs text-fg-3">{isLocal ? t("Live sessions on this Mac") : t("Demo workspace, sample data")}</span>
            </span>
            <ChevronDown size={14} className="text-fg-3" />
          </Dropdown.Trigger>
          <Dropdown.Portal>
            <Dropdown.Content align="start" sideOffset={6} className="z-50 w-[232px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
              <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Workspaces")}</Dropdown.Label>
              {workspaces.map((w) => (
                <Dropdown.Item key={w.id} disabled={w.id === "local" && !liveApi.available} onSelect={() => setWorkspace(w.id)} className={itemCls}>
                  {w.id === "local" ? <Monitor size={14} className="text-fg-3" /> : <FolderGit2 size={14} className="text-fg-3" />}
                  {t(w.name)}
                  {w.id === "local" && !liveApi.available && <span className="ms-auto text-[11px] text-fg-3">{t("Desktop app")}</span>}
                  {w.id === ws.id && <Check size={14} className="ms-auto text-violet-text" />}
                </Dropdown.Item>
              ))}
            </Dropdown.Content>
          </Dropdown.Portal>
        </Dropdown.Root>

        <div className="flex gap-1">
          <button
            type="button"
            disabled={!canStart}
            onClick={() => newSession(undefined, "claude-code")}
            className="flex h-9 flex-1 items-center gap-2 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white shadow-[0_1px_0_rgb(255_255_255/0.15)_inset] hover:brightness-110 disabled:opacity-50"
          >
            <Plus size={16} strokeWidth={2.2} />
            {t("New session")}
            <span dir="ltr" className="ms-auto text-[11px] font-normal text-white/70">{MOD}N</span>
          </button>
          <Dropdown.Root>
            <Tip label={t("Choose agent")}>
              <Dropdown.Trigger
                disabled={!canStart}
                aria-label={t("New session with a specific agent")}
                className="flex h-9 w-9 items-center justify-center rounded-lg border border-line text-fg-2 hover:bg-surface hover:text-fg disabled:opacity-50 data-[state=open]:bg-surface"
              >
                <ChevronDown size={15} />
              </Dropdown.Trigger>
            </Tip>
            <Dropdown.Portal>
              <Dropdown.Content align="end" sideOffset={6} className="z-50 w-[240px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
                {isLocal && (
                  <>
                    <Dropdown.Item onSelect={() => newChat()} className={itemCls}>
                      <MessageCircle size={14} className="text-fg-3" /> {t("Chat without a project")}
                    </Dropdown.Item>
                    <Dropdown.Item onSelect={() => setNewProjectOpen(true)} className={itemCls}>
                      <FolderPlus size={14} className="text-fg-3" /> {t("New project…")}
                    </Dropdown.Item>
                    <Dropdown.Item onSelect={() => setImportOpen(true)} className={itemCls}>
                      <Download size={14} className="text-fg-3" /> {t("Import sessions…")}
                    </Dropdown.Item>
                    <Dropdown.Item onSelect={() => newSession(undefined, undefined, undefined, true)} className={itemCls}>
                      <GitBranch size={14} className="text-fg-3" /> {t("New session in a worktree")}
                    </Dropdown.Item>
                    <Dropdown.Separator className="my-1 h-px bg-line" />
                  </>
                )}
                <Dropdown.Item onSelect={() => newSession(undefined, "claude-code")} className={itemCls}>
                  <AgentMark provider="claude-code" size={18} /> {t("{agent} session", { agent: "Claude Code" })}
                </Dropdown.Item>
                {projects.length > 1 && (
                  <>
                    <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("{agent} in", { agent: "Claude Code" })}</Dropdown.Label>
                    {projects.map((p) => (
                      <Dropdown.Item key={p.id} onSelect={() => newSession(p.id, "claude-code")} className={itemCls}>
                        <FolderGit2 size={14} className="text-fg-3" /> <span className="truncate">{p.name}</span>
                      </Dropdown.Item>
                    ))}
                    <Dropdown.Separator className="my-1 h-px bg-line" />
                  </>
                )}
                {withoutHidden(["codex", "gemini-cli", "grok-build", "kimi-code", "antigravity"] as const, agyEnabled).map((p) => {
                  const liveOnly = p !== "codex" && !isLocal;
                  return (
                    <Dropdown.Item key={p} disabled={liveOnly} onSelect={() => newSession(undefined, p)} className={itemCls}>
                      <AgentMark provider={p} size={18} /> {t("{agent} session", { agent: providerLabel[p] })}
                      {liveOnly && <span className="ms-auto text-[11px] text-fg-3">{t("This Mac only")}</span>}
                    </Dropdown.Item>
                  );
                })}
                {isLocal && (
                  <>
                    <Dropdown.Separator className="my-1 h-px bg-line" />
                    <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Your models")}</Dropdown.Label>
                    {profiles.map((m) => (
                      <Dropdown.Item
                        key={m.id}
                        onSelect={() => newSession(undefined, m.kind === "anthropic" ? "claude-code" : "api", m.id)}
                        className={itemCls}
                      >
                        <AgentMark provider={m.kind === "anthropic" ? "claude-code" : "api"} size={18} />
                        <span className="truncate">{m.name}</span>
                        <span className="ms-auto shrink-0 text-[11px] text-fg-3">{m.kind === "anthropic" ? t("in {agent}", { agent: "Claude Code" }) : t("agent")}</span>
                      </Dropdown.Item>
                    ))}
                    <Dropdown.Item onSelect={() => setView("settings")} className={itemCls}>
                      <Plus size={14} className="text-fg-3" /> {profiles.length ? t("Manage models…") : t("Add DeepSeek, Kimi or a local model…")}
                    </Dropdown.Item>
                  </>
                )}
              </Dropdown.Content>
            </Dropdown.Portal>
          </Dropdown.Root>
        </div>

        <div className="relative">
          <Search size={14} className="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-fg-3" />
          <input
            ref={searchRef}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setSearch("");
                (e.target as HTMLInputElement).blur();
              }
            }}
            placeholder={t("Search sessions")}
            aria-label={t("Search sessions")}
            className="h-8 w-full rounded-lg border border-line-soft bg-bg/40 ps-8 pe-14 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus:shadow-[0_0_0_3px_var(--violet-soft)]"
          />
          {search ? (
            <button
              type="button"
              aria-label={t("Clear search")}
              onClick={() => setSearch("")}
              className="absolute end-1.5 top-1/2 -translate-y-1/2 rounded p-1 text-fg-3 hover:text-fg"
            >
              <X size={13} />
            </button>
          ) : (
            <span className="pointer-events-none absolute end-2 top-1/2 -translate-y-1/2">
              <Kbd>{MOD}K</Kbd>
            </span>
          )}
        </div>
      </div>

      <ul aria-label={t("Views")} className="space-y-px px-2 pb-2">
        <NavItem view="overview" current={view} onSelect={setView} icon={LayoutList} label={t("All sessions")} count={sessions.length} shortcut={`${MOD}⇧S`} />
        <NavItem view="review" current={view} onSelect={setView} icon={GitCompareArrows} label={t("Review changes")} count={changedFiles} shortcut={`${MOD}⇧D`} />
        <NavItem
          view="permissions"
          current={view}
          onSelect={setView}
          icon={ShieldAlert}
          label={t("Permissions")}
          count={pending.length}
          attention={pending.length > 0}
          shortcut={`${MOD}⇧A`}
        />
        <NavItem view="git" current={view} onSelect={() => openGit(null)} icon={GitGraph} label="Git" shortcut={`${MOD}⇧H`} />
        <NavItem view="setup" current={view} onSelect={() => openSetup(null)} icon={BookOpenText} label={t("Agent setup")} shortcut={`${MOD}⇧E`} />
        <NavItem view="gallery" current={view} onSelect={setView} icon={Shapes} label={t("Output gallery")} shortcut={`${MOD}⇧G`} />
      </ul>
      <div className="mx-4 mb-1 border-t border-line-soft" />

      <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {isLocal && liveState === "loading" ? (
          <p className="px-3 py-8 text-center text-[13px] text-fg-3">{t("Loading sessions…")}</p>
        ) : isLocal && liveState === "error" ? (
          <div className="px-3 py-8 text-center">
            <p className="text-[13px] text-error">{t("Couldn't open the session store.")}</p>
            <p className="mt-1 break-words text-xs text-fg-3">{liveError}</p>
          </div>
        ) : !hasProjects && !isLocal ? (
          <div className="px-3 py-8 text-center">
            <p className="text-[13px] text-fg-2">{t("No sample projects here.")}</p>
          </div>
        ) : groups.length === 0 ? (
          <div className="px-3 py-8 text-center">
            <p className="text-[13px] text-fg-2">{t("No sessions match “{search}”.", { search })}</p>
            <button type="button" onClick={() => setSearch("")} className="mt-2 rounded text-xs text-violet-text hover:underline">
              {t("Clear search")}
            </button>
          </div>
        ) : (
          groups.map(({ project, sessions: list }) => {
            const key = project?.id ?? "chats";
            const isChats = !project || project.kind === "chats";
            const isCollapsed = collapsed[key] && !search;
            const start = () => (isChats ? newChat() : newSession(project!.id, "claude-code"));
            return (
              <section key={key} className="mb-2">
                <div className="group flex items-center">
                  <button
                    type="button"
                    aria-expanded={!isCollapsed}
                    onClick={() => setCollapsed((c) => ({ ...c, [key]: !c[key] }))}
                    title={project?.path}
                    className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 pb-1 pt-2 text-start text-xs text-fg-3 hover:text-fg-2"
                  >
                    <ChevronRight size={12} className={cx("rtl:-scale-x-100 shrink-0 transition-transform", !isCollapsed && "rotate-90 rtl:-rotate-90")} />
                    {isChats && <MessageCircle size={12} className="shrink-0" />}
                    <span className="truncate font-medium">{isChats ? t("Chats") : project!.name}</span>
                    <span className="ms-auto tabular-nums opacity-0 transition-opacity group-hover:opacity-100">{list.length || ""}</span>
                  </button>
                  {isLocal && !isChats && (
                    <Tip label={t("Instructions, skills and commands for {name}", { name: project!.name })}>
                      <button
                        type="button"
                        onClick={() => openSetup(project!.id)}
                        aria-label={t("Agent setup for {name}", { name: project!.name })}
                        className="mt-1 rounded p-0.5 text-fg-3 opacity-0 hover:bg-surface hover:text-fg focus-visible:opacity-100 group-hover:opacity-100"
                      >
                        <BookOpenText size={13} />
                      </button>
                    </Tip>
                  )}
                  {isLocal && (
                    <Tip label={isChats ? t("New chat without a project") : t("New session in {name}", { name: project!.name })}>
                      <button
                        type="button"
                        onClick={start}
                        aria-label={isChats ? t("New chat") : t("New session in {name}", { name: project!.name })}
                        className="me-1 mt-1 rounded p-0.5 text-fg-3 opacity-0 hover:bg-surface hover:text-fg focus-visible:opacity-100 group-hover:opacity-100"
                      >
                        <Plus size={13} />
                      </button>
                    </Tip>
                  )}
                </div>
                {!isCollapsed &&
                  (list.length === 0 ? (
                    <button
                      type="button"
                      onClick={start}
                      className="ms-5 mt-0.5 rounded-md px-2 py-1 text-start text-xs text-fg-3 hover:bg-surface hover:text-fg"
                    >
                      {isChats ? t("Ask something, no project needed") : t("Start the first session")}
                    </button>
                  ) : (
                    <ul className="space-y-px">
                      {list.map((s) => (
                        <SessionRow key={s.id} s={s} selected={view === "workspace" && s.id === selectedId} onSelect={() => select(s.id)} />
                      ))}
                    </ul>
                  ))}
              </section>
            );
          })
        )}
        {isLocal && liveState === "ready" && (
          <div className="mt-1 flex flex-wrap gap-x-1">
            <button
              type="button"
              onClick={() => setNewProjectOpen(true)}
              className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-start text-xs text-fg-3 hover:bg-surface hover:text-fg-2"
            >
              <Plus size={13} /> {t("New project")}
            </button>
            <button
              type="button"
              onClick={() => void addProjectFolder()}
              className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-start text-xs text-fg-3 hover:bg-surface hover:text-fg-2"
            >
              <FolderPlus size={13} /> {t("Add folder")}
            </button>
            <button
              type="button"
              onClick={() => setImportOpen(true)}
              title={t("Bring in sessions from Claude Code, Codex and Gemini CLI")}
              className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-start text-xs text-fg-3 hover:bg-surface hover:text-fg-2"
            >
              <Download size={13} /> {t("Import")}
            </button>
          </div>
        )}
        {isLocal && liveError && liveState === "ready" && <p className="px-2 pt-2 text-xs text-error">{liveError}</p>}
      </div>

      <UpdateNotice />
      <div className="flex shrink-0 items-center gap-1 border-t border-line-soft px-3 py-2.5">
        <button
          type="button"
          onClick={() => setView("settings")}
          aria-current={view === "settings" ? "page" : undefined}
          className={cx(
            "flex h-8 flex-1 items-center gap-2 rounded-lg px-2 text-[13px]",
            view === "settings" ? "bg-elevated text-fg" : "text-fg-2 hover:bg-surface hover:text-fg",
          )}
        >
          <Settings size={15} />
          {t("Settings")}
          <span dir="ltr" className="ms-auto text-[11px] text-fg-3">{MOD},</span>
        </button>
      </div>
      <NewProjectDialog />
      {isLocal && <ImportDialog />}
    </nav>
  );
}

function NavItem({
  view,
  current,
  onSelect,
  icon: Icon,
  label,
  count,
  attention,
  shortcut,
}: {
  view: AppView;
  current: AppView;
  onSelect: (v: AppView) => void;
  icon: typeof LayoutList;
  label: string;
  count?: number;
  attention?: boolean;
  shortcut: string;
}) {
  const active = view === current;
  return (
    <li>
      <Tip label={label} shortcut={shortcut} side="right">
        <button
          type="button"
          onClick={() => onSelect(view)}
          aria-current={active ? "page" : undefined}
          className={cx(
            "relative flex h-8 w-full items-center gap-2.5 rounded-lg px-2 text-[13px] transition-colors",
            active ? "bg-elevated text-fg" : "text-fg-2 hover:bg-surface hover:text-fg",
          )}
        >
          {active && <span aria-hidden className="absolute bottom-1.5 start-0 top-1.5 w-[2px] rounded-full bg-violet" />}
          <Icon size={15} className={cx(attention ? "text-warning" : "text-fg-3")} />
          {label}
          {count !== undefined && count > 0 && (
            <span
              className={cx(
                "ms-auto min-w-5 rounded-full px-1.5 text-center text-[11px] tabular-nums",
                attention ? "bg-warning/15 font-medium text-warning" : "text-fg-3",
              )}
            >
              {count}
            </span>
          )}
        </button>
      </Tip>
    </li>
  );
}

function SessionRow({ s, selected, onSelect }: { s: Session; selected: boolean; onSelect: () => void }) {
  const renaming = useStore((st) => st.renaming?.id === s.id && st.renaming.at === "sidebar");
  const setRenaming = useStore((st) => st.setRenaming);
  if (renaming) {
    return (
      <li className="flex items-center gap-2.5 rounded-lg bg-elevated px-2 py-1.5">
        <AgentMark provider={s.provider} size={18} />
        <RenameField sessionId={s.id} title={s.title} />
      </li>
    );
  }
  return (
    <li>
      <button
        type="button"
        onClick={onSelect}
        onDoubleClick={() => setRenaming({ id: s.id, at: "sidebar" })}
        onKeyDown={(e) => {
          if (e.key === "F2") {
            e.preventDefault();
            setRenaming({ id: s.id, at: "sidebar" });
          }
        }}
        title={t("Double-click or press F2 to rename")}
        aria-current={selected ? "page" : undefined}
        aria-label={`${s.title}, ${providerLabel[s.provider]}, ${statusLabel(s.status)}`}
        className={cx(
          "relative flex w-full items-start gap-2.5 rounded-lg px-2 py-2 text-start transition-colors duration-150",
          selected ? "bg-elevated" : "hover:bg-surface",
        )}
      >
        {selected && <span aria-hidden className="absolute bottom-2 start-0 top-2 w-[2px] rounded-full bg-violet" />}
        <span className="pt-px">
          <AgentMark provider={s.provider} size={18} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span dir="auto" className={cx("truncate text-[13px]", selected ? "font-medium text-fg" : "text-fg/90")}>{s.title === "New session" ? t("New session") : s.title}</span>
            {s.worktree && <GitBranch size={11} aria-label={t("In worktree {branch}", { branch: s.branch ?? "" })} className="shrink-0 text-violet-text" />}
            <span className="ms-auto shrink-0 text-[11px] tabular-nums text-fg-3">{ago(s.updatedAt)}</span>
          </span>
          <span className="mt-0.5 flex items-center gap-1.5">
            <StatusDot status={s.status} className="h-1.5 w-1.5" />
            <span dir="auto" className={cx("truncate text-xs", s.status === "error" ? "text-error/90" : s.status === "waiting" ? "text-warning/90" : "text-fg-3")}>
              {s.snippet}
            </span>
          </span>
        </span>
      </button>
    </li>
  );
}
