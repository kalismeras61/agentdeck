import { WorktreeBadge } from "./blocks/Worktree";
import { agentLabel } from "../lib/agentName";
import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import {
  ChevronRight,
  Copy,
  FolderInput,
  Pencil,
  MessageCircle,
  MessageSquareText,
  MoreHorizontal,
  Power,
  Repeat2,
  Trash2,
  PanelLeftOpen,
  PanelRightOpen,
  Square,
  TerminalSquare,
} from "lucide-react";
import { useCallback, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { type Session } from "../data/demo";
import { useProject } from "../hooks/useWorkspace";
import { MOD } from "../lib/format";
import { t } from "../lib/i18n";
import { liveApi } from "../lib/live";
import { useStore } from "../state/store";
import { StartRelayDialog, useRelayFor } from "./blocks/Relay";
import { useFlowFor } from "../lib/workflows";
import { AgentMark, cx, DemoBadge, IconButton, StatusPill } from "./ui";
import { RenameField } from "./RenameField";

/* ---------------------------------------------------------------- header */

export function SessionHeader({ session }: { session: Session }) {
  const sidebarOpen = useStore((s) => s.sidebarOpen);
  const contextOpen = useStore((s) => s.contextOpen);
  const narrow = useStore((s) => s.narrow);
  const contextOverlay = useStore((s) => s.contextOverlay);
  const toggleSidebar = useStore((s) => s.toggleSidebar);
  const toggleContext = useStore((s) => s.toggleContext);
  const mode = useStore((s) => s.viewMode);
  const setMode = useStore((s) => s.setViewMode);
  const interrupt = useStore((s) => s.interrupt);
  const stopProcess = useStore((s) => s.stopProcess);
  const deleteSession = useStore((s) => s.deleteSession);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const relayOpen = useStore((s) => s.relayDialogFor === session.id);
  const openRelayDialog = useStore((s) => s.openRelayDialog);
  const setRelayOpen = (v: boolean) => openRelayDialog(v ? session.id : null);
  const relay = useRelayFor(session.id);
  const flow = useFlowFor(session.id);
  const busyWithOthers = relay?.status === "active" || flow?.status === "active";
  const canRelay = Boolean(session.live);
  const project = useProject(session.projectId);
  const projects = useStore((s) => s.projects);
  const moveSession = useStore((s) => s.moveSession);
  const moveTargets = projects.filter((p) => p.workspaceId === "local" && p.id !== session.projectId);
  const renaming = useStore((s) => s.renaming?.id === session.id && s.renaming.at === "header");
  const setRenaming = useStore((s) => s.setRenaming);

  return (
    <header data-tauri-drag-region className="drag relative flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-3">
      {!sidebarOpen && (
        <div className="flex items-center gap-2 pl-[66px] rtl:order-last">
          <span aria-hidden dir="ltr" className="absolute left-4 top-[18px] flex gap-2" data-web-only>
            <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
            <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
            <span className="h-3 w-3 rounded-full bg-[#28c840]" />
          </span>
          <IconButton label={t("Show sidebar")} shortcut={`${MOD}B`} onClick={toggleSidebar} size="sm">
            <PanelLeftOpen size={16} />
          </IconButton>
        </div>
      )}
      <nav aria-label={t("Breadcrumb")} className="flex min-w-0 items-center gap-1.5 text-[13px]">
        <span className="shrink-0 text-fg-3">{project?.name}</span>
        <span className="text-fg-3" aria-hidden>
          /
        </span>
        {renaming ? (
          <RenameField key={session.id} sessionId={session.id} title={session.title} className="w-[min(360px,40vw)] font-medium" />
        ) : (
          <h1
            className="truncate font-medium text-fg"
            title={t("Double-click to rename")}
            onDoubleClick={() => setRenaming({ id: session.id, at: "header" })}
            dir="auto"
          >
            {session.title === "New session" ? t("New session") : session.title}
          </h1>
        )}
      </nav>
      {session.worktree && <WorktreeBadge session={session} />}
      <span className="hidden shrink-0 items-center gap-1.5 rounded-full border border-line px-2 py-0.5 text-xs text-fg-2 2xl:inline-flex">
        <AgentMark provider={session.provider} size={14} />
        {agentLabel(session)}
        <span className="text-fg-3">{session.model}</span>
      </span>
      <span className="hidden shrink-0 min-[1720px]:inline-flex">
        <StatusPill status={session.status} />
      </span>

      <div className="ms-auto flex shrink-0 items-center gap-1.5">
        {session.demo && <DemoBadge className="me-1" />}
        {canRelay && !busyWithOthers && (
          <button
            type="button"
            onClick={() => setRelayOpen(true)}
            title={t("Give planning, building and reviewing to different agents, or let two agents talk")}
            className="me-1 inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
          >
            <Repeat2 size={13} /> {t("Work with…")}
          </button>
        )}
        <div role="radiogroup" aria-label={t("View")} className="flex rounded-lg border border-line bg-bg/40 p-0.5">
          {(
            [
              ["rich", t("Rich"), MessageSquareText],
              ["terminal", t("Terminal"), TerminalSquare],
            ] as const
          ).map(([id, label, Icon]) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={mode === id}
              onClick={() => setMode(id)}
              title={id === "rich" ? t("Rich view ({key})", { key: `${MOD}⇧R` }) : t("Terminal view ({key})", { key: `${MOD}⇧T` })}
              className={cx(
                "inline-flex h-6 items-center gap-1.5 rounded-md px-2 text-xs transition-colors",
                mode === id ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg",
              )}
            >
              <Icon size={13} />
              {label}
            </button>
          ))}
        </div>
        <Dropdown.Root>
          <Dropdown.Trigger asChild>
            <IconButton label={t("Session actions")} size="sm">
              <MoreHorizontal size={16} />
            </IconButton>
          </Dropdown.Trigger>
          <Dropdown.Portal>
            <Dropdown.Content align="end" sideOffset={6} className="z-50 w-[220px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
              <Dropdown.Item
                onSelect={() => setRenaming({ id: session.id, at: "header" })}
                className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
              >
                <Pencil size={13} className="text-fg-3" /> {t("Rename…")}
              </Dropdown.Item>
              <Dropdown.Item
                onSelect={() => void navigator.clipboard?.writeText(session.id).catch(() => undefined)}
                className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
              >
                <Copy size={14} className="text-fg-3" /> {t("Copy session ID")}
              </Dropdown.Item>
              <Dropdown.Item
                disabled={session.status !== "running" && session.status !== "waiting"}
                onSelect={() => interrupt(session.id)}
                className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface"
              >
                <Square size={13} className="text-fg-3" /> {t("Stop this turn")}
                {session.status !== "running" && session.status !== "waiting" && <span className="ms-auto text-[11px] text-fg-3">{t("Not working")}</span>}
              </Dropdown.Item>
              {canRelay && (
                <Dropdown.Item
                  disabled={busyWithOthers}
                  onSelect={() => setRelayOpen(true)}
                  className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface"
                >
                  <Repeat2 size={14} className="text-fg-3" /> {t("Work with other agents…")}
                  {busyWithOthers && <span className="ms-auto text-[11px] text-fg-3">{t("Busy")}</span>}
                </Dropdown.Item>
              )}
              {session.live && (
                <Dropdown.Sub>
                  <Dropdown.SubTrigger
                    disabled={busyWithOthers}
                    className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface data-[state=open]:bg-surface"
                  >
                    <FolderInput size={14} className="text-fg-3" /> {t("Move to…")}
                    <ChevronRight size={13} className="rtl:-scale-x-100 ms-auto text-fg-3" />
                  </Dropdown.SubTrigger>
                  <Dropdown.Portal>
                    <Dropdown.SubContent
                      sideOffset={4}
                      className="z-50 max-h-[320px] w-[220px] overflow-y-auto rounded-xl border border-line bg-elevated p-1 shadow-pop"
                    >
                      <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] leading-[1.45] text-fg-3">
                        {session.provider === "claude-code" || session.provider === "codex"
                          ? t("The history comes along, and the agent carries on the same conversation in the new folder.")
                          : t("The history comes along; the agent starts a fresh conversation in the new folder.")}
                      </Dropdown.Label>
                      {project?.kind !== "chats" && !moveTargets.some((p) => p.kind === "chats") && (
                        <Dropdown.Item
                          onSelect={() =>
                            void liveApi.openChats().then((c) => {
                              useStore.setState((s) => ({
                                projects: s.projects.some((x) => x.id === c.id)
                                  ? s.projects
                                  : [{ id: c.id, name: c.name, path: c.path, workspaceId: "local", kind: "chats" }, ...s.projects],
                              }));
                              void moveSession(session.id, c.id);
                            })
                          }
                          className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
                        >
                          <MessageCircle size={14} className="text-fg-3" /> {t("Chats")}
                        </Dropdown.Item>
                      )}
                      {moveTargets.map((p) => (
                        <Dropdown.Item
                          key={p.id}
                          onSelect={() => void moveSession(session.id, p.id)}
                          className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
                        >
                          {p.kind === "chats" ? (
                            <MessageCircle size={14} className="shrink-0 text-fg-3" />
                          ) : (
                            <FolderInput size={14} className="shrink-0 text-fg-3" />
                          )}
                          <span className="truncate">{p.kind === "chats" ? t("Chats") : p.name}</span>
                        </Dropdown.Item>
                      ))}
                    </Dropdown.SubContent>
                  </Dropdown.Portal>
                </Dropdown.Sub>
              )}
              {session.live && (
                <>
                  <Dropdown.Item
                    onSelect={() => stopProcess(session.id)}
                    className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
                  >
                    <Power size={13} className="text-fg-3" /> {t("End agent process")}
                  </Dropdown.Item>
                  <Dropdown.Separator className="my-1 h-px bg-line" />
                  <Dropdown.Item
                    onSelect={() => setConfirmDelete(true)}
                    className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] text-error outline-none data-[highlighted]:bg-surface"
                  >
                    <Trash2 size={13} /> {t("Delete session…")}
                  </Dropdown.Item>
                </>
              )}
            </Dropdown.Content>
          </Dropdown.Portal>
        </Dropdown.Root>
        {canRelay && <StartRelayDialog key={session.id} session={session} open={relayOpen} onOpenChange={setRelayOpen} />}
        <Dialog.Root open={confirmDelete} onOpenChange={setConfirmDelete}>
          <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
            <Dialog.Content className="fixed left-1/2 top-1/2 z-50 w-[min(420px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
              <Dialog.Title className="text-[15px] font-semibold">{t("Delete “{title}”?", { title: session.title })}</Dialog.Title>
              <Dialog.Description className="mt-2 text-[13px] leading-[1.6] text-fg-2">
                {t(
                  "Removes this session and its history from AgentDeck and ends its agent process. Files the agent changed in your project stay as they are, and Claude Code keeps its own transcript.",
                )}
              </Dialog.Description>
              <div className="mt-5 flex justify-end gap-2">
                <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
                <button
                  type="button"
                  onClick={() => {
                    setConfirmDelete(false);
                    deleteSession(session.id);
                  }}
                  className="h-8 rounded-lg bg-error/90 px-3 text-[13px] font-medium text-white hover:bg-error"
                >
                  {t("Delete session")}
                </button>
              </div>
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
        {(narrow ? !contextOverlay : !contextOpen) && (
          <IconButton label={t("Show panel")} shortcut={`${MOD}.`} onClick={toggleContext} size="sm">
            <PanelRightOpen size={16} />
          </IconButton>
        )}
      </div>
    </header>
  );
}

/* ---------------------------------------------------------------- resizer */

export function Resizer({
  side,
  value,
  min,
  max,
  def,
  onChange,
  label,
}: {
  side: "left" | "right";
  value: number;
  min: number;
  max: number;
  def: number;
  onChange: (v: number) => void;
  label: string;
}) {
  const start = useRef<{ x: number; w: number } | null>(null);

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      start.current = { x: e.clientX, w: value };
      document.body.style.cursor = "col-resize";
    },
    [value],
  );
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const dx = e.clientX - start.current.x;
    onChange(start.current.w + (side === "left" ? dx : -dx));
  };
  const end = () => {
    start.current = null;
    document.body.style.cursor = "";
  };
  const onKeyDown = (e: ReactKeyboardEvent) => {
    const step = e.shiftKey ? 48 : 16;
    const grow = side === "left" ? "ArrowRight" : "ArrowLeft";
    const shrink = side === "left" ? "ArrowLeft" : "ArrowRight";
    if (e.key === grow) onChange(value + step);
    else if (e.key === shrink) onChange(value - step);
    else if (e.key === "Home") onChange(min);
    else if (e.key === "End") onChange(max);
    else return;
    e.preventDefault();
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={() => onChange(def)}
      onKeyDown={onKeyDown}
      className="group relative z-20 w-px shrink-0 cursor-col-resize bg-line-soft outline-none focus-visible:shadow-[0_0_0_1px_var(--violet)]"
    >
      <span className="absolute inset-y-0 -start-[3px] -end-[3px]" />
      <span className="absolute inset-y-0 -start-px -end-px bg-violet opacity-0 transition-opacity duration-150 group-hover:opacity-60 group-focus-visible:opacity-100 group-active:opacity-100" />
    </div>
  );
}
