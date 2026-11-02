import { withoutHidden } from "../lib/setup";
import { agentLabel } from "../lib/agentName";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { ArrowUp, Check, ChevronDown, ClipboardList, GitBranch, FileText, Folder, Image as ImageIcon, Paperclip, Square, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { APP_COMMANDS, matchCommands, useAgentCommands, type SlashCommand } from "../lib/commands";
import { SlashMenu } from "./SlashMenu";
import { liveModelsFor, providerLabel, providerModels, type Session } from "../data/demo";
import { errorText, gitApi, liveApi, pickFiles, subscribeFileDrop } from "../lib/live";
import { isTauri } from "../lib/native";
import { useStore } from "../state/store";
import { useRelayFor } from "./blocks/Relay";
import { ModelMenu } from "./ModelMenu";
import { AgyApprovalsMenu } from "./AgyApprovals";
import { AgentMark, cx, IconButton, Kbd, Tip } from "./ui";

export function Composer({ session }: { session: Session }) {
  const draft = useStore((s) => s.drafts[session.id] ?? "");
  const setDraft = useStore((s) => s.setDraft);
  const send = useStore((s) => s.sendPrompt);
  const setModel = useStore((s) => s.setModel);
  const interrupt = useStore((s) => s.interrupt);
  const focusTick = useStore((s) => s.composerFocusTick);
  /** Absolute paths in the desktop app; names only in the browser demo. */
  const [files, setFiles] = useState<string[]>([]);
  const [dropping, setDropping] = useState(false);
  const [attachError, setAttachError] = useState<string | null>(null);
  const ta = useRef<HTMLTextAreaElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const models: [string, string][] = session.live ? liveModelsFor(session.provider) : providerModels[session.provider].map((m) => [m, m]);
  const modelLabel = models.find(([id]) => id === session.model)?.[1] ?? session.model;
  const running = session.status === "running" || session.status === "waiting";
  const error = useStore((s) => s.sessionErrors[session.id]);
  const clearError = useStore((s) => s.clearSessionError);
  const setMode = useStore((s) => s.setSessionMode);
  const planning = session.mode === "plan";
  const relay = useRelayFor(session.id);
  const profile = useStore((s) => (session.profileId ? s.profiles.find((m) => m.id === session.profileId) : undefined));
  const peer = useStore((s) => (relay ? s.sessions.find((x) => x.id === (relay.aSession === session.id ? relay.bSession : relay.aSession)) : undefined));
  // "@codex …" / "@claude …" sends to that agent when it's part of this relay.
  const mention = /^@(claude|codex|gemini|grok|agy|antigravity)\b[\s,:]*/i.exec(draft);
  const MENTION = { claude: "claude-code", codex: "codex", gemini: "gemini-cli", grok: "grok-build", agy: "antigravity", antigravity: "antigravity" } as const;
  const mentioned = mention ? MENTION[mention[1]!.toLowerCase() as keyof typeof MENTION] : undefined;
  const target = mentioned && peer && peer.provider === mentioned && session.provider !== mentioned ? peer : session;
  const canPlan = Boolean(session.live);
  // Before the first message the agent can still be changed.
  const noHistory = useStore((s) => (s.liveEvents[session.id]?.length ?? 1) === 0);
  const canPickAgent = Boolean(session.live) && noHistory && !running;

  useEffect(() => {
    if (focusTick > 0) ta.current?.focus();
  }, [focusTick]);

  // "/" at the start lists the agent's commands, your own, skills and AgentDeck's.
  const slash = /^\/(\S*)$/.exec(draft);
  const [slashActive, setSlashActive] = useState(0);
  const [slashClosed, setSlashClosed] = useState(false);
  const { commands: agentCommands, loading: slashLoading } = useAgentCommands(session.id, Boolean(slash) && Boolean(session.live));
  const slashItems = useMemo(() => {
    if (!slash) return [];
    const found = matchCommands([...agentCommands, ...APP_COMMANDS], slash[1] ?? "");
    return (["custom", "agent", "skill", "agentdeck"] as const).flatMap((src) => found.filter((c) => c.source === src));
  }, [slash?.[1], agentCommands]); // eslint-disable-line react-hooks/exhaustive-deps
  const slashOpen = Boolean(slash) && !slashClosed;
  useEffect(() => {
    setSlashActive(0);
    if (!draft.startsWith("/")) setSlashClosed(false);
  }, [draft]);
  const setViewMode = useStore((s) => s.setViewMode);
  const setRenaming = useStore((s) => s.setRenaming);
  const newSession = useStore((s) => s.newSession);
  const openGit = useStore((s) => s.openGit);
  const openSetup = useStore((s) => s.openSetup);
  const openRelayDialog = useStore((s) => s.openRelayDialog);
  const pickCommand = (c: SlashCommand) => {
    if (c.source === "agentdeck") {
      setDraft(session.id, "");
      if (c.name === "plan" && canPlan) setMode(session.id, planning ? "default" : "plan");
      else if (c.name === "terminal") setViewMode("terminal");
      else if (c.name === "rename") setRenaming({ id: session.id, at: "header" });
      else if (c.name === "new") newSession(session.projectId, session.provider);
      else if (c.name === "workflow") openRelayDialog(session.id);
      else if (c.name === "git") openGit(session.projectId, session.worktree ?? null);
      else if (c.name === "setup") openSetup(session.projectId);
      return;
    }
    setDraft(session.id, c.expand ?? `/${c.name} `);
    requestAnimationFrame(() => {
      const el = ta.current;
      if (el) {
        el.focus();
        el.setSelectionRange(el.value.length, el.value.length);
      }
    });
  };

  useEffect(() => setFiles([]), [session.id]);

  const addFiles = (paths: string[]) => {
    setAttachError(null);
    setFiles((cur) => {
      const all = Array.from(new Set([...cur, ...paths]));
      if (all.length > 20) setAttachError("Attach at most 20 files to one message.");
      return all.slice(0, 20);
    });
    ta.current?.focus();
  };

  // Files dropped anywhere on the window attach to this message (the desktop app gives real paths).
  useEffect(() => {
    if (!isTauri) return;
    let off: (() => void) | undefined;
    let gone = false;
    void subscribeFileDrop({ onHover: setDropping, onDrop: addFiles }).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off?.();
    };
  }, [session.id]); // eslint-disable-line react-hooks/exhaustive-deps

  // A pasted screenshot becomes an attachment too.
  const onPaste = (ev: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const fromItems = Array.from(ev.clipboardData.items)
      .filter((i) => i.kind === "file")
      .map((i) => i.getAsFile())
      .filter((f): f is File => f !== null);
    const images = (ev.clipboardData.files.length ? Array.from(ev.clipboardData.files) : fromItems).filter((f) => /^image\/(png|jpeg|gif|webp)$/.test(f.type));
    if (!images.length) {
      // Some web views don't hand pasted images to the page; ask the desktop app for the clipboard image.
      if (isTauri && session.live && !ev.clipboardData.types.includes("text/plain")) {
        ev.preventDefault();
        liveApi
          .pasteClipboardImage()
          .then((p) => p && addFiles([p]))
          .catch((e: unknown) => setAttachError(errorText(e)));
      }
      return;
    }
    ev.preventDefault();
    if (!isTauri) {
      addFiles(images.map((f) => f.name || "pasted-image.png"));
      return;
    }
    for (const f of images) {
      const reader = new FileReader();
      reader.onload = () => {
        const data = String(reader.result).split(",")[1] ?? "";
        const ext = f.type.split("/")[1] === "jpeg" ? "jpg" : f.type.split("/")[1];
        liveApi
          .stashImage(`pasted.${ext}`, data)
          .then((p) => addFiles([p]))
          .catch((e: unknown) => setAttachError(errorText(e)));
      };
      reader.readAsDataURL(f);
    }
  };

  // Auto-grow up to a comfortable maximum.
  useEffect(() => {
    const el = ta.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(220, el.scrollHeight)}px`;
  }, [draft]);

  const canSend = draft.trim().length > 0 || files.length > 0;
  const submit = () => {
    if (!canSend) return;
    if (target.id !== session.id && mention) {
      send(target.id, draft.slice(mention[0].length), []);
      setDraft(session.id, "");
      return;
    }
    send(session.id, draft.trim() ? draft : files.length === 1 ? "Look at the attached file." : "Look at the attached files.", files);
    setDraft(session.id, "");
    setFiles([]);
  };

  return (
    <div className="px-8 pb-5 pt-2">
      <div className="mx-auto max-w-[860px]">
        {error && (
          <div role="alert" className="mb-2 flex items-start gap-2 rounded-xl border border-error/35 bg-error/[0.06] px-3 py-2 text-[12.5px] text-fg">
            <span className="min-w-0 flex-1 break-words">{error}</span>
            <button type="button" onClick={() => clearError(session.id)} aria-label="Dismiss" className="shrink-0 rounded p-0.5 text-fg-3 hover:text-fg">
              <X size={13} />
            </button>
          </div>
        )}
        {attachError && (
          <div role="alert" className="mb-2 flex items-start gap-2 rounded-xl border border-error/35 bg-error/[0.06] px-3 py-2 text-[12.5px] text-fg">
            <span className="min-w-0 flex-1 break-words">{attachError}</span>
            <button type="button" onClick={() => setAttachError(null)} aria-label="Dismiss" className="shrink-0 rounded p-0.5 text-fg-3 hover:text-fg">
              <X size={13} />
            </button>
          </div>
        )}
        <div
          onDragOver={(ev) => {
            // Browser demo only: the desktop app reports drops through subscribeFileDrop.
            if (isTauri || !ev.dataTransfer.types.includes("Files")) return;
            ev.preventDefault();
            setDropping(true);
          }}
          onDragLeave={() => !isTauri && setDropping(false)}
          onDrop={(ev) => {
            if (isTauri) return;
            ev.preventDefault();
            setDropping(false);
            addFiles(Array.from(ev.dataTransfer.files).map((f) => f.name));
          }}
          className={cx(
            "relative rounded-2xl border bg-surface shadow-card transition-colors",
            dropping
              ? "border-dashed border-[color-mix(in_srgb,var(--violet)_75%,var(--border))] shadow-[0_0_0_3px_var(--violet-soft)]"
              : "border-line focus-within:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-within:shadow-[0_0_0_3px_var(--violet-soft)]",
          )}
        >
          {dropping && (
            <div
              aria-hidden
              className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center gap-2 rounded-2xl bg-bg/80 text-[13px] text-fg"
            >
              <Paperclip size={15} className="text-violet-text" /> Drop to attach to this message
            </div>
          )}
          {files.length > 0 && (
            <ul aria-label="Attached files" className="flex flex-wrap gap-1.5 px-3 pt-3">
              {files.map((f) => {
                const name = f.split(/[\\/]/).filter(Boolean).pop() ?? f;
                const Icon = /\.(png|jpe?g|gif|webp)$/i.test(f) ? ImageIcon : f.endsWith("/") ? Folder : FileText;
                return (
                  <li
                    key={f}
                    title={f}
                    className="inline-flex max-w-[260px] items-center gap-1.5 rounded-lg border border-line-soft bg-elevated py-1 pl-2 pr-1 text-[12px] text-fg-2"
                  >
                    <Icon size={13} className="shrink-0 text-fg-3" />
                    <span className="truncate">{name}</span>
                    <button
                      type="button"
                      aria-label={`Remove ${name}`}
                      onClick={() => setFiles(files.filter((x) => x !== f))}
                      className="shrink-0 rounded p-0.5 hover:bg-surface hover:text-fg"
                    >
                      <X size={11} />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {slashOpen && (
            <SlashMenu
              items={slashItems}
              active={slashActive}
              agent={agentLabel(session)}
              loading={slashLoading}
              onPick={pickCommand}
              onHover={setSlashActive}
            />
          )}
          <label htmlFor="composer" className="sr-only">
            Message {agentLabel(session)}
          </label>
          <textarea
            id="composer"
            ref={ta}
            rows={1}
            value={draft}
            aria-autocomplete="list"
            aria-controls={slashOpen ? "slash-menu" : undefined}
            aria-expanded={slashOpen}
            onChange={(ev) => setDraft(session.id, ev.target.value)}
            onPaste={onPaste}
            onKeyDown={(ev) => {
              if (slashOpen && !ev.nativeEvent.isComposing) {
                if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
                  ev.preventDefault();
                  const n = slashItems.length;
                  if (n) setSlashActive((i) => (ev.key === "ArrowDown" ? (i + 1) % n : (i - 1 + n) % n));
                  return;
                }
                if ((ev.key === "Enter" || ev.key === "Tab") && !ev.shiftKey && slashItems[slashActive]) {
                  ev.preventDefault();
                  pickCommand(slashItems[slashActive]!);
                  return;
                }
                if (ev.key === "Escape") {
                  ev.preventDefault();
                  setSlashClosed(true);
                  return;
                }
              }
              if (ev.key === "Enter" && !ev.shiftKey && !ev.nativeEvent.isComposing) {
                ev.preventDefault();
                submit();
              }
            }}
            placeholder={
              session.status === "waiting"
                ? "Reply, or handle the request above first"
                : planning
                  ? "Describe what to plan. Nothing changes until you approve the plan."
                  : peer
                    ? `Message ${agentLabel(session)}, or start with @${providerLabel[peer.provider].split(" ")[0]!.toLowerCase()} to message ${providerLabel[peer.provider]}`
                    : `Ask ${agentLabel(session)} to do something, or type / for commands`
            }
            className="block w-full resize-none bg-transparent px-4 pb-1.5 pt-3.5 text-[14px] leading-[1.6] text-fg outline-none placeholder:text-fg-3"
          />
          <div className="flex items-center gap-1 px-2 pb-2">
            <input
              ref={fileInput}
              type="file"
              multiple
              className="hidden"
              onChange={(ev) => {
                // Only names are shown in demo mode; file contents are never read here.
                const names = Array.from(ev.target.files ?? []).map((f) => f.name);
                setFiles((cur) => Array.from(new Set([...cur, ...names])).slice(0, 20));
                ev.target.value = "";
              }}
            />
            <IconButton
              label="Attach files (or drop them here)"
              tipSide="top"
              onClick={() => {
                if (!isTauri || !session.live) return fileInput.current?.click();
                void pickFiles()
                  .then((p) => p.length && addFiles(p))
                  .catch((e: unknown) => setAttachError(errorText(e)));
              }}
            >
              <Paperclip size={16} />
            </IconButton>
            {canPickAgent && <AgentPicker session={session} />}
            {canPickAgent && session.provider !== "api" && <WorktreeToggle session={session} onError={setAttachError} />}
            {profile ? (
              <Tip label={`Set in Settings → Models (${profile.baseUrl})`} side="top">
                <span tabIndex={0} className="inline-flex h-8 items-center rounded-lg px-2 font-mono text-[11.5px] text-fg-2">
                  {profile.model}
                </span>
              </Tip>
            ) : session.live && session.provider !== "api" ? (
              <ModelMenu provider={session.provider} agent={agentLabel(session)} value={session.model} onChange={(m) => setModel(session.id, m)} />
            ) : models.length > 0 ? (
              <Dropdown.Root>
                <Tip label="Model for this session" side="top">
                  <Dropdown.Trigger className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-xs text-fg-2 hover:bg-elevated hover:text-fg data-[state=open]:bg-elevated">
                    {modelLabel}
                    <ChevronDown size={13} />
                  </Dropdown.Trigger>
                </Tip>
                <Dropdown.Portal>
                  <Dropdown.Content
                    side="top"
                    align="start"
                    sideOffset={6}
                    className="z-50 min-w-[180px] rounded-xl border border-line bg-elevated p-1 shadow-pop"
                  >
                    <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{agentLabel(session)} models</Dropdown.Label>
                    {models.map(([id, label]) => (
                      <Dropdown.Item
                        key={id}
                        onSelect={() => setModel(session.id, id)}
                        className="flex items-center justify-between rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
                      >
                        {label}
                        {id === session.model && <Check size={14} className="text-violet-text" />}
                      </Dropdown.Item>
                    ))}
                  </Dropdown.Content>
                </Dropdown.Portal>
              </Dropdown.Root>
            ) : (
              <span className="px-2 text-xs text-fg-3">Model choice not supported</span>
            )}
            {session.live && session.provider === "antigravity" && <AgyApprovalsMenu sessionId={session.id} />}
            {canPlan && (
              <Tip
                label={
                  planning
                    ? "Plan mode is on: the agent reads and plans, AgentDeck refuses any change. Click to let it build."
                    : "Plan first: the agent reads and proposes a plan, and changes nothing until you approve it."
                }
                side="top"
              >
                <button
                  type="button"
                  aria-pressed={planning}
                  onClick={() => setMode(session.id, planning ? "default" : "plan")}
                  className={cx(
                    "inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-xs",
                    planning ? "bg-violet-soft font-medium text-violet-text hover:brightness-110" : "text-fg-2 hover:bg-elevated hover:text-fg",
                  )}
                >
                  <ClipboardList size={13} />
                  {planning ? "Plan mode" : "Plan"}
                </button>
              </Tip>
            )}
            {target.id !== session.id && (
              <span className="inline-flex items-center gap-1.5 rounded-lg bg-elevated px-2 py-1 text-[11.5px] text-fg-2" aria-live="polite">
                To {providerLabel[target.provider]}
              </span>
            )}
            <span className="ml-auto mr-2 hidden items-center gap-1.5 text-[11px] text-fg-3 md:flex">
              <Kbd>↵</Kbd> send <Kbd>⇧↵</Kbd> new line
            </span>
            {running ? (
              <Tip label="Stop the agent" side="top">
                <button
                  type="button"
                  onClick={() => interrupt(session.id)}
                  aria-label="Stop the agent"
                  className="inline-flex h-8 w-8 items-center justify-center rounded-lg bg-fg text-bg hover:opacity-90"
                >
                  <Square size={12} fill="currentColor" />
                </button>
              </Tip>
            ) : (
              <Tip label="Send" shortcut="↵" side="top">
                <button
                  type="button"
                  onClick={submit}
                  disabled={!canSend}
                  aria-label="Send message"
                  className="inline-flex h-8 w-8 items-center justify-center rounded-lg bg-violet-strong text-white transition-opacity hover:brightness-110 disabled:bg-elevated disabled:text-fg-3"
                >
                  <ArrowUp size={16} />
                </button>
              </Tip>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** Choose the agent for a session that has no messages yet: a CLI on this Mac or one of your models. */
function AgentPicker({ session }: { session: Session }) {
  const profiles = useStore((s) => s.profiles);
  const setAgent = useStore((s) => s.setSessionAgent);
  const agyEnabled = useStore((s) => s.agyEnabled);
  const item = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface";
  return (
    <Dropdown.Root>
      <Tip label="Agent for this session (you can change it until you send the first message)" side="top">
        <Dropdown.Trigger className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-line-soft px-2 text-xs text-fg hover:bg-elevated data-[state=open]:bg-elevated">
          <AgentMark provider={session.provider} size={16} />
          {agentLabel(session)}
          <ChevronDown size={13} className="text-fg-3" />
        </Dropdown.Trigger>
      </Tip>
      <Dropdown.Portal>
        <Dropdown.Content side="top" align="start" sideOffset={6} className="z-50 min-w-[220px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
          <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">Agents on this Mac</Dropdown.Label>
          {withoutHidden(["claude-code", "codex", "gemini-cli", "grok-build", "antigravity"] as const, agyEnabled).map((p) => (
            <Dropdown.Item key={p} onSelect={() => setAgent(session.id, p)} className={item}>
              <AgentMark provider={p} size={16} /> {providerLabel[p]}
              {session.provider === p && !session.profileId && <Check size={14} className="ml-auto text-violet-text" />}
            </Dropdown.Item>
          ))}
          {profiles.length > 0 && (
            <>
              <Dropdown.Separator className="my-1 h-px bg-line" />
              <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">Your models</Dropdown.Label>
              {profiles.map((m) => (
                <Dropdown.Item key={m.id} onSelect={() => setAgent(session.id, m.kind === "anthropic" ? "claude-code" : "api", m.id)} className={item}>
                  <AgentMark provider={m.kind === "anthropic" ? "claude-code" : "api"} size={16} />
                  <span className="truncate">{m.name}</span>
                  {session.profileId === m.id && <Check size={14} className="ml-auto text-violet-text" />}
                </Dropdown.Item>
              ))}
            </>
          )}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

/** Before the first message: work in the project folder or in a new git worktree of it. */
function WorktreeToggle({ session, onError }: { session: Session; onError: (e: string | null) => void }) {
  const upsert = useStore((s) => s.upsertLiveSession);
  const project = useStore((s) => s.projects.find((p) => p.id === session.projectId));
  const [busy, setBusy] = useState(false);
  const on = Boolean(session.worktree);
  if (!project || project.kind === "chats") return null;
  return (
    <Tip label={on ? `Works in ${session.branch}, apart from the project folder. Click to use the project folder instead.` : "Work in a new git worktree, so this agent doesn't touch the project folder until you merge"} side="top">
      <button
        type="button"
        aria-pressed={on}
        disabled={busy}
        onClick={() => {
          setBusy(true);
          onError(null);
          gitApi
            .setSessionWorktree(session.id, !on)
            .then(upsert)
            .catch((e: unknown) => onError(errorText(e)))
            .finally(() => setBusy(false));
        }}
        className={cx(
          "inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-xs disabled:opacity-60",
          on ? "bg-violet-soft text-fg" : "text-fg-2 hover:bg-elevated hover:text-fg",
        )}
      >
        <GitBranch size={13} className={on ? "text-violet-text" : ""} />
        {busy ? (on ? "Removing…" : "Making worktree…") : on ? "Own worktree" : "Worktree"}
      </button>
    </Tip>
  );
}
