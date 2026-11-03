import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import * as Tabs from "@radix-ui/react-tabs";
import { ArrowUp, ChevronRight, CircleAlert, MoreHorizontal, Pause, Pencil, Play, RotateCcw, RotateCw, Square, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Conversation } from "../components/Conversation";
import { BotMark } from "../components/bots/BotMark";
import { Markdown } from "../components/blocks/Markdown";
import { AgentMark, cx, IconButton, Kbd, Tip } from "../components/ui";
import { demoBotMemory } from "../data/demo";
import { useResolvedTheme } from "../hooks/useTheme";
import { agentLabel } from "../lib/agentName";
import { botState, elapsed, firstLine, orderTasks } from "../lib/bots";
import { ago } from "../lib/format";
import { t, tn } from "../lib/i18n";
import { errorText, liveApi, type Bot, type BotMemory, type BotTask } from "../lib/live";
import { effortName } from "../lib/models";
import { useStore } from "../state/store";
import { TextEditor } from "./setup/TextEditor";
import { ViewHeader } from "./ViewHeader";

/** A bot's page: its session, a box that gives it tasks, its queue and its memory. */
export function BotPage() {
  const botId = useStore((s) => s.selectedBotId);
  const bot = useStore((s) => (botId ? s.bots[botId] : undefined));
  if (!bot) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title={t("Bots")} />
        <p className="p-8 text-[13px] text-fg-3">{t("This bot isn't here anymore.")}</p>
      </div>
    );
  }
  return <BotView key={bot.id} bot={bot} />;
}

function BotView({ bot }: { bot: Bot }) {
  const session = useStore((s) => (bot.sessionId ? s.sessions.find((x) => x.id === bot.sessionId) : undefined));
  const ensureEvents = useStore((s) => s.ensureEvents);
  useEffect(() => {
    if (bot.sessionId) void ensureEvents(bot.sessionId);
  }, [bot.sessionId, ensureEvents]);
  return (
    <div className="flex h-full flex-col">
      <BotHeader bot={bot} />
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1">
            {session ? (
              <Conversation session={session} />
            ) : (
              <div className="flex h-full items-center justify-center px-8">
                <div className="max-w-[420px] text-center">
                  <BotMark name={bot.name} color={bot.color} size={40} className="mb-3" />
                  <p className="text-[14px] font-medium text-fg">{t("{name} hasn't started yet", { name: bot.name })}</p>
                  <p className="mt-1 text-[13px] leading-[1.6] text-fg-2">{t("Give it a task below. The first task starts its session; later tasks continue it.")}</p>
                </div>
              </div>
            )}
          </div>
          <BotComposer bot={bot} sessionId={session?.id ?? null} sessionBusy={session?.status === "running" || session?.status === "waiting"} />
        </div>
        <aside className="flex w-[320px] shrink-0 flex-col border-s border-line-soft bg-sidebar">
          <BotSidePanel bot={bot} />
        </aside>
      </div>
    </div>
  );
}

const menuItem = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface";

function BotHeader({ bot }: { bot: Bot }) {
  const setPaused = useStore((s) => s.setBotPaused);
  const openEditor = useStore((s) => s.openBotEditor);
  const restart = useStore((s) => s.restartBotSession);
  const deleteBot = useStore((s) => s.deleteBot);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const agent = agentLabel({ provider: bot.provider, profileId: bot.profileId });
  const model = bot.profileId ? null : bot.model === "default" ? t("Default model") : bot.model;
  return (
    <ViewHeader title={bot.name} sub={bot.role} lead={<BotMark name={bot.name} color={bot.color} size={20} className="ms-1" />}>
      <span className="hidden shrink-0 items-center gap-1.5 rounded-full border border-line px-2 py-0.5 text-xs text-fg-2 xl:inline-flex">
        <AgentMark provider={bot.provider} size={14} />
        {agent}
        {model && <span className="text-fg-3">· {model}</span>}
        {bot.effort && <span className="text-fg-3">· {effortName(bot.effort)}</span>}
      </span>
      <button
        type="button"
        onClick={() => setPaused(bot.id, !bot.paused)}
        title={bot.paused ? t("Start queued tasks again") : t("Finish the current task, then start nothing new")}
        className={cx(
          "inline-flex h-7 items-center gap-1.5 rounded-lg border px-2.5 text-xs",
          bot.paused ? "border-warning/40 bg-warning/10 text-warning hover:bg-warning/15" : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
        )}
      >
        {bot.paused ? <Play size={13} /> : <Pause size={13} />}
        {bot.paused ? t("Resume") : t("Pause")}
      </button>
      <button
        type="button"
        onClick={() => openEditor(bot.id)}
        className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
      >
        <Pencil size={12} /> {t("Edit")}
      </button>
      <Dropdown.Root>
        <Dropdown.Trigger asChild>
          <IconButton label={t("Bot actions")} size="sm">
            <MoreHorizontal size={16} />
          </IconButton>
        </Dropdown.Trigger>
        <Dropdown.Portal>
          <Dropdown.Content align="end" sideOffset={6} className="z-50 w-[240px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
            <Dropdown.Item disabled={Boolean(bot.runningTaskId) || !bot.sessionId} onSelect={() => restart(bot.id)} className={menuItem}>
              <RotateCcw size={13} className="text-fg-3" /> {t("Start fresh")}
              {bot.runningTaskId && <span className="ms-auto text-[11px] text-fg-3">{t("Busy")}</span>}
            </Dropdown.Item>
            <p className="px-2 pb-1.5 text-[11px] leading-[1.45] text-fg-3">{t("The next task starts a new session. The memory file stays.")}</p>
            <Dropdown.Separator className="my-1 h-px bg-line" />
            <Dropdown.Item onSelect={() => setConfirm(true)} className={cx(menuItem, "text-error")}>
              <Trash2 size={13} /> {t("Delete bot…")}
            </Dropdown.Item>
          </Dropdown.Content>
        </Dropdown.Portal>
      </Dropdown.Root>
      <Dialog.Root open={confirm} onOpenChange={setConfirm}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
          <Dialog.Content className="fixed left-1/2 top-1/2 z-50 w-[min(420px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
            <Dialog.Title className="text-[15px] font-semibold">{t("Delete {name}?", { name: bot.name })}</Dialog.Title>
            <Dialog.Description className="mt-2 text-[13px] leading-[1.6] text-fg-2">
              {t("Its queue and task history go. Its session stays as an ordinary session, and its memory file stays in the folder.")}
            </Dialog.Description>
            {error && <p className="mt-2 text-[12.5px] text-error">{error}</p>}
            <div className="mt-5 flex justify-end gap-2">
              <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
              <button
                type="button"
                onClick={async () => {
                  const err = await deleteBot(bot.id);
                  if (err) setError(err);
                  else setConfirm(false);
                }}
                className="h-8 rounded-lg bg-error/90 px-3 text-[13px] font-medium text-white hover:bg-error"
              >
                {t("Delete bot")}
              </button>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </ViewHeader>
  );
}

/** Sending adds a task; "Message" writes to the bot's session directly instead. */
function BotComposer({ bot, sessionId, sessionBusy }: { bot: Bot; sessionId: string | null; sessionBusy: boolean }) {
  const key = `bot:${bot.id}`;
  const draft = useStore((s) => s.drafts[key] ?? "");
  const setDraft = useStore((s) => s.setDraft);
  const addTask = useStore((s) => s.addBotTask);
  const sendPrompt = useStore((s) => s.sendPrompt);
  const cancelTask = useStore((s) => s.cancelBotTask);
  const interrupt = useStore((s) => s.interrupt);
  const botError = useStore((s) => s.botErrors[bot.id]);
  const clearBotError = useStore((s) => s.clearBotError);
  const [mode, setMode] = useState<"task" | "message">("task");
  const [error, setError] = useState<string | null>(null);
  const ta = useRef<HTMLTextAreaElement>(null);
  const shown = error ?? botError;
  const canMessage = Boolean(sessionId);
  const asMessage = mode === "message" && canMessage;

  useEffect(() => {
    const el = ta.current;
    if (!el) return;
    el.style.height = "0px";
    el.style.height = `${Math.min(200, el.scrollHeight)}px`;
  }, [draft]);

  const submit = async () => {
    const text = draft.trim();
    if (!text) return;
    setError(null);
    if (asMessage && sessionId) {
      sendPrompt(sessionId, text, []);
      setDraft(key, "");
      return;
    }
    setDraft(key, "");
    const err = await addTask(bot.id, text);
    if (err) {
      setError(err);
      setDraft(key, text);
    }
  };

  return (
    <div className="px-6 pb-5 pt-2">
      <div className="mx-auto max-w-[860px]">
        {shown && (
          <div role="alert" className="mb-2 flex items-start gap-2 rounded-xl border border-error/35 bg-error/[0.06] px-3 py-2 text-[12.5px] text-fg">
            <span className="min-w-0 flex-1 break-words">{shown}</span>
            <button
              type="button"
              onClick={() => {
                setError(null);
                clearBotError(bot.id);
              }}
              aria-label={t("Dismiss")}
              className="shrink-0 rounded p-0.5 text-fg-3 hover:text-fg"
            >
              <X size={13} />
            </button>
          </div>
        )}
        <div className="relative rounded-2xl border border-line bg-surface shadow-card focus-within:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-within:shadow-[0_0_0_3px_var(--violet-soft)]">
          <label htmlFor="bot-composer" className="sr-only">
            {asMessage ? t("Message {name}", { name: bot.name }) : t("Give {name} a task", { name: bot.name })}
          </label>
          <textarea
            id="bot-composer"
            ref={ta}
            rows={1}
            value={draft}
            onChange={(e) => setDraft(key, e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                void submit();
              }
            }}
            placeholder={asMessage ? t("Write to {name} directly. This isn't a task.", { name: bot.name }) : t("Give {name} a task…", { name: bot.name })}
            className="block w-full resize-none bg-transparent px-4 pb-1.5 pt-3.5 text-[14px] leading-[1.6] text-fg outline-none placeholder:text-fg-3"
          />
          <div className="flex items-center gap-1 px-2 pb-2">
            <div role="radiogroup" aria-label={t("Send as")} className="flex rounded-lg border border-line bg-bg/40 p-0.5">
              {(
                [
                  ["task", t("Task")],
                  ["message", t("Message")],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={(id === "message") === asMessage}
                  disabled={id === "message" && !canMessage}
                  title={
                    id === "task"
                      ? t("Added to the queue; the bot works on it when it's free")
                      : canMessage
                        ? t("Sent to the bot's session now, like any session")
                        : t("The bot's session starts with its first task")
                  }
                  onClick={() => setMode(id)}
                  className={cx(
                    "inline-flex h-6 items-center rounded-md px-2 text-xs transition-colors disabled:opacity-45",
                    (id === "message") === asMessage ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg",
                  )}
                >
                  {label}
                </button>
              ))}
            </div>
            <span className="ms-auto me-2 hidden items-center gap-1.5 text-[11px] text-fg-3 md:flex">
              <Kbd>↵</Kbd> {asMessage ? t("send") : t("add to queue")}
            </span>
            {sessionBusy && sessionId && (
              <Tip label={bot.runningTaskId ? t("Cancel the task in progress") : t("Stop the agent")} side="top">
                <button
                  type="button"
                  onClick={() => (bot.runningTaskId ? cancelTask(bot.runningTaskId) : interrupt(sessionId))}
                  aria-label={bot.runningTaskId ? t("Cancel the task in progress") : t("Stop the agent")}
                  className="inline-flex h-8 w-8 items-center justify-center rounded-lg bg-fg text-bg hover:opacity-90"
                >
                  <Square size={12} fill="currentColor" />
                </button>
              </Tip>
            )}
            <Tip label={asMessage ? t("Send") : t("Add task")} shortcut="↵" side="top">
              <button
                type="button"
                onClick={() => void submit()}
                disabled={!draft.trim()}
                aria-label={asMessage ? t("Send message") : t("Add task")}
                className="inline-flex h-8 w-8 items-center justify-center rounded-lg bg-violet-strong text-white transition-opacity hover:brightness-110 disabled:bg-elevated disabled:text-fg-3"
              >
                <ArrowUp size={16} />
              </button>
            </Tip>
          </div>
        </div>
      </div>
    </div>
  );
}

function BotSidePanel({ bot }: { bot: Bot }) {
  const [tab, setTab] = useState<"queue" | "memory">("queue");
  const tasks = useStore((s) => s.botTasks[bot.id]);
  const { queued } = orderTasks(tasks ?? []);
  return (
    <Tabs.Root value={tab} onValueChange={(v) => setTab(v as "queue" | "memory")} className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center border-b border-line-soft ps-3 pe-2">
        <Tabs.List aria-label={t("Bot")} className="flex gap-0.5">
          <Tabs.Trigger
            value="queue"
            className="relative h-7 rounded-md px-2.5 text-[12.5px] text-fg-2 hover:text-fg data-[state=active]:bg-elevated data-[state=active]:text-fg"
          >
            {t("Queue")}
            {queued.length > 0 && <span className="ms-1.5 text-[11px] tabular-nums text-fg-3">{queued.length}</span>}
          </Tabs.Trigger>
          <Tabs.Trigger
            value="memory"
            className="relative h-7 rounded-md px-2.5 text-[12.5px] text-fg-2 hover:text-fg data-[state=active]:bg-elevated data-[state=active]:text-fg"
          >
            {t("Memory")}
          </Tabs.Trigger>
        </Tabs.List>
      </div>
      <Tabs.Content value="queue" className="scroll-quiet min-h-0 flex-1 overflow-y-auto focus-visible:outline-offset-[-2px]">
        <QueueTab bot={bot} tasks={tasks} />
      </Tabs.Content>
      <Tabs.Content value="memory" className="min-h-0 flex-1 focus-visible:outline-offset-[-2px]">
        <MemoryTab bot={bot} />
      </Tabs.Content>
    </Tabs.Root>
  );
}

function useNow(on: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    const id = globalThis.setInterval(() => setNow(Date.now()), 1000);
    return () => globalThis.clearInterval(id);
  }, [on]);
  return now;
}

function QueueTab({ bot, tasks }: { bot: Bot; tasks: BotTask[] | undefined }) {
  const cancel = useStore((s) => s.cancelBotTask);
  const { running, queued, recent } = orderTasks(tasks ?? []);
  const now = useNow(Boolean(running));
  const state = botState(bot);
  if (!tasks) return <p className="p-4 text-[12.5px] text-fg-3">{t("Loading…")}</p>;
  return (
    <div className="space-y-4 p-3">
      {state === "needs" && (
        <p className="flex items-start gap-2 rounded-lg border border-warning/35 bg-warning/[0.07] px-2.5 py-2 text-[12.5px] leading-[1.5] text-fg">
          <CircleAlert size={14} className="mt-0.5 shrink-0 text-warning" />
          {t("{name} is waiting for you in the conversation.", { name: bot.name })}
        </p>
      )}
      {bot.paused && (
        <p className="rounded-lg border border-line bg-elevated px-2.5 py-2 text-[12.5px] leading-[1.5] text-fg-2">
          {t("Paused. Queued tasks wait until you resume.")}
        </p>
      )}
      {running && (
        <section aria-label={t("Working on")}>
          <h3 className="mb-1.5 px-1 text-[11px] font-medium text-fg-3">{t("Working on")}</h3>
          <div className="rounded-lg border border-[color-mix(in_srgb,var(--violet)_45%,var(--border))] bg-surface px-2.5 py-2">
            <p dir="auto" className="line-clamp-4 whitespace-pre-wrap break-words text-[13px] leading-[1.5] text-fg">
              {running.text}
            </p>
            <div className="mt-1.5 flex items-center gap-2 text-[11.5px] text-fg-3">
              <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-violet breathe" aria-hidden />
              <span className="tabular-nums">{elapsed(running.startedAt ?? running.createdAt, now)}</span>
              <button type="button" onClick={() => cancel(running.id)} className="ms-auto rounded px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg">
                {t("Cancel")}
              </button>
            </div>
          </div>
        </section>
      )}
      {queued.length > 0 && (
        <section aria-label={t("Queued")}>
          <h3 className="mb-1.5 px-1 text-[11px] font-medium text-fg-3">{tn(queued.length, "{n} queued", "{n} queued")}</h3>
          <ol className="space-y-1">
            {queued.map((x, i) => (
              <li key={x.id} className="group flex items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-surface">
                <span className="mt-px w-4 shrink-0 text-end text-[11px] tabular-nums text-fg-3">{i + 1}</span>
                <span dir="auto" className="line-clamp-2 min-w-0 flex-1 break-words text-[12.5px] leading-[1.5] text-fg/90">
                  {x.text}
                </span>
                <IconButton label={t("Remove from the queue")} size="sm" onClick={() => cancel(x.id)} className="-my-1 h-6 w-6 opacity-0 focus-visible:opacity-100 group-hover:opacity-100">
                  <X size={13} />
                </IconButton>
              </li>
            ))}
          </ol>
        </section>
      )}
      {!running && queued.length === 0 && recent.length === 0 && (
        <p className="px-1 py-6 text-center text-[12.5px] leading-[1.6] text-fg-3">{t("No tasks yet. Tasks you add show here, the running one on top.")}</p>
      )}
      {recent.length > 0 && (
        <section aria-label={t("Recent")}>
          <h3 className="mb-1.5 px-1 text-[11px] font-medium text-fg-3">{t("Recent")}</h3>
          <ul className="space-y-1">
            {recent.map((x) => (
              <RecentTask key={x.id} task={x} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

const STATUS_TEXT: Record<"done" | "failed" | "cancelled", string> = /* i18n */ {
  done: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
};

function RecentTask({ task }: { task: BotTask }) {
  const retry = useStore((s) => s.retryBotTask);
  const [open, setOpen] = useState(false);
  const status = task.status as "done" | "failed" | "cancelled";
  const body = task.status === "done" ? task.result : task.error;
  return (
    <li className="rounded-lg hover:bg-surface">
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="flex w-full items-start gap-2 px-2 py-1.5 text-start">
        <ChevronRight size={12} className={cx("mt-1 shrink-0 text-fg-3 transition-transform rtl:-scale-x-100", open && "rotate-90 rtl:-rotate-90")} />
        <span className="min-w-0 flex-1">
          <span dir="auto" className={cx("block break-words text-[12.5px] leading-[1.5]", open ? "text-fg" : "line-clamp-1 text-fg/90")}>
            {task.text}
          </span>
          <span className="mt-0.5 flex items-center gap-1.5 text-[11px]">
            <span className={cx(status === "done" ? "text-success" : status === "failed" ? "text-error" : "text-fg-3")}>{t(STATUS_TEXT[status] ?? "Done")}</span>
            <span className="text-fg-3">· {ago(task.finishedAt ?? task.createdAt)}</span>
          </span>
        </span>
      </button>
      {open && (
        <div className="px-2 pb-2 ps-6">
          {body ? (
            task.status === "done" ? (
              <div className="max-h-[320px] overflow-y-auto rounded-md border border-line-soft bg-bg px-2.5 py-2 text-[12.5px]">
                <Markdown text={body} />
              </div>
            ) : (
              <p className="break-words text-[12px] leading-[1.5] text-error/90">{body}</p>
            )
          ) : (
            <p className="text-[12px] text-fg-3">{task.status === "done" ? t("Finished without a report.") : t("No reason given.")}</p>
          )}
          {task.status !== "done" && (
            <button
              type="button"
              onClick={() => retry(task.id)}
              className="mt-2 inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
            >
              <RotateCw size={12} /> {t("Try again")}
            </button>
          )}
        </div>
      )}
    </li>
  );
}

/** The bot's memory file, editable. Shown again from the file when the bot is renamed or moved. */
function MemoryTab({ bot }: { bot: Bot }) {
  return <MemoryEditor key={`${bot.id}:${bot.name}:${bot.projectId}`} bot={bot} />;
}

function demoMemory(bot: Bot): BotMemory {
  return { path: `.agentdeck/bots/${firstLine(bot.name).toLowerCase().replace(/\s+/g, "-")}.md`, text: demoBotMemory[bot.id] ?? "" };
}

function MemoryEditor({ bot }: { bot: Bot }) {
  const dark = useResolvedTheme() === "dark";
  const demo = bot.id.startsWith("b-demo-") || !liveApi.available;
  const [file, setFile] = useState<BotMemory | null>(() => (demo ? demoMemory(bot) : null));
  const [text, setText] = useState(() => (demo ? demoMemory(bot).text : ""));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const dirty = file !== null && text !== file.text;

  const fetchFile = () =>
    liveApi.readBotMemory(bot.id).then(
      (f) => {
        setFile(f);
        setText(f.text);
        setError(null);
      },
      (e: unknown) => setError(errorText(e)),
    );
  useEffect(() => {
    if (!demo) void fetchFile();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const load = () => {
    if (!demo) void fetchFile();
    else if (file) setText(file.text);
  };

  const save = async () => {
    if (!file || !dirty) return;
    if (demo) {
      setFile({ ...file, text });
      setSaved(true);
      return;
    }
    setSaving(true);
    try {
      setFile(await liveApi.writeBotMemory(bot.id, text));
      setSaved(true);
      setError(null);
    } catch (e: unknown) {
      setError(errorText(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="shrink-0 border-b border-line-soft px-3 py-2">
        <p dir="ltr" title={file?.path} className="truncate font-mono text-[11px] text-fg-3">
          {file?.path ?? ""}
        </p>
        <p className="mt-1 text-[11.5px] leading-[1.5] text-fg-3">{t("{name} reads this at the start of every task and adds what's worth keeping.", { name: bot.name })}</p>
        <div className="mt-2 flex items-center gap-2">
          {saved && !dirty && (
            <span role="status" className="text-[12px] text-success">
              {t("Saved")}
            </span>
          )}
          {error && (
            <span role="alert" className="min-w-0 truncate text-[12px] text-error" title={error}>
              {error}
            </span>
          )}
          <span className="ms-auto flex gap-1.5">
            <button type="button" onClick={load} className="h-7 rounded-lg px-2 text-[12px] text-fg-2 hover:bg-elevated hover:text-fg">
              {dirty ? t("Revert") : t("Reload")}
            </button>
            <button
              type="button"
              onClick={() => void save()}
              disabled={!dirty || saving}
              className="h-7 rounded-lg bg-violet-strong px-3 text-[12px] font-medium text-white hover:brightness-110 disabled:opacity-50"
            >
              {saving ? t("Saving…") : t("Save")}
            </button>
          </span>
        </div>
      </div>
      <div className="relative min-h-0 flex-1 bg-bg">
        {file && (
          <TextEditor
            docKey={`${bot.id}:${file.path}`}
            value={text}
            language="markdown"
            dark={dark}
            label={t("Memory of {name}", { name: bot.name })}
            onChange={(next) => {
              setSaved(false);
              setText(next);
            }}
            onSave={() => void save()}
          />
        )}
      </div>
    </div>
  );
}
