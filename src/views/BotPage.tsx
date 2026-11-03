import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import * as Tabs from "@radix-ui/react-tabs";
import {
  ArrowUp,
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  FolderGit2,
  History,
  House,
  MoreHorizontal,
  Pause,
  Pencil,
  Play,
  Plus,
  Repeat,
  RotateCcw,
  RotateCw,
  Square,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Conversation } from "../components/Conversation";
import { BotMark } from "../components/bots/BotMark";
import { Markdown } from "../components/blocks/Markdown";
import { AgentMark, cx, IconButton, Kbd, Switch, Tip } from "../components/ui";
import { demoBotMemory } from "../data/demo";
import { useResolvedTheme } from "../hooks/useTheme";
import { agentLabel } from "../lib/agentName";
import { botState, elapsed, orderTasks, placeSession, scheduleSummary, whenText } from "../lib/bots";
import { ago } from "../lib/format";
import { t, tn } from "../lib/i18n";
import { errorText, liveApi, type Bot, type BotMemory, type BotSchedule, type BotTask } from "../lib/live";
import { effortName } from "../lib/models";
import { useStore } from "../state/store";
import { TextEditor } from "./setup/TextEditor";
import { PlacePicker } from "../components/bots/PlacePicker";
import { usePlaceName } from "../hooks/usePlaceName";
import { ScheduleEditor } from "../components/bots/ScheduleEditor";
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

/** The place a bot's page shows when you haven't picked one: where it works now, else where it worked last. */
function usePlace(bot: Bot): string | null {
  const picked = useStore((s) => s.botPlace[bot.id]);
  const sessions = useStore((s) => s.sessions);
  if (picked !== undefined) return picked;
  if (bot.runningTaskId) return bot.runningProjectId;
  const last = [...bot.places].sort(
    (a, b) => (sessions.find((x) => x.id === b.sessionId)?.updatedAt ?? 0) - (sessions.find((x) => x.id === a.sessionId)?.updatedAt ?? 0),
  )[0];
  return last ? last.projectId : (bot.projectId ?? null);
}

function BotView({ bot }: { bot: Bot }) {
  const place = usePlace(bot);
  const [earlier, setEarlier] = useState<string | null>(null);
  const sessionId = earlier ?? placeSession(bot, place);
  const session = useStore((s) => (sessionId ? s.sessions.find((x) => x.id === sessionId) : undefined));
  const ensureEvents = useStore((s) => s.ensureEvents);
  const placeName = usePlaceName();
  useEffect(() => {
    if (sessionId) void ensureEvents(sessionId);
  }, [sessionId, ensureEvents]);
  return (
    <div className="flex h-full flex-col">
      <BotHeader bot={bot} place={place} earlier={earlier} onEarlier={setEarlier} />
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1">
            {session ? (
              <Conversation session={session} />
            ) : (
              <div className="flex h-full items-center justify-center px-8">
                <div className="max-w-[420px] text-center">
                  <BotMark name={bot.name} color={bot.color} size={40} className="mb-3" />
                  <p className="text-[14px] font-medium text-fg">
                    {bot.places.length === 0
                      ? t("{name} hasn't started yet", { name: bot.name })
                      : t("{name} hasn't worked in {place} yet", { name: bot.name, place: placeName(place) })}
                  </p>
                  <p className="mt-1 text-[13px] leading-[1.6] text-fg-2">
                    {t("Give it a task below. Its first task in a place starts a session there; later tasks there continue it.")}
                  </p>
                </div>
              </div>
            )}
          </div>
          <BotComposer
            bot={bot}
            sessionId={session?.id ?? null}
            sessionBusy={session?.status === "running" || session?.status === "waiting"}
            shownPlace={earlier ? null : place}
          />
        </div>
        <aside className="flex w-[320px] shrink-0 flex-col border-s border-line-soft bg-sidebar">
          <BotSidePanel bot={bot} />
        </aside>
      </div>
    </div>
  );
}

const pill =
  "inline-flex h-7 max-w-[220px] shrink-0 items-center gap-1.5 rounded-lg border border-line px-2 text-xs text-fg-2 hover:bg-elevated hover:text-fg data-[state=open]:bg-elevated data-[state=open]:text-fg";

/** Which of the bot's places the page shows: its workspace and each project it has worked in. */
function PlaceSwitcher({ bot, place, earlier, onEarlier }: { bot: Bot; place: string | null; earlier: string | null; onEarlier: (sid: string | null) => void }) {
  const setPlace = useStore((s) => s.setBotPlace);
  const sessions = useStore((s) => s.sessions);
  const placeName = usePlaceName();
  const projects = useStore((s) => s.projects);
  const places = useMemo(() => {
    const ids = bot.places.flatMap((p) => (p.projectId ? [p.projectId] : []));
    return [...new Set(ids)].sort((a, b) => placeName(a).localeCompare(placeName(b)));
  }, [bot.places, placeName]);
  // Sessions it let go of in its workspace (Start fresh, another agent) can still be read here.
  const current = new Set(bot.places.map((p) => p.sessionId));
  const old = sessions.filter((s) => bot.workspaceId && s.projectId === bot.workspaceId && !current.has(s.id)).sort((a, b) => b.updatedAt - a.updatedAt);
  const oldShown = earlier ? sessions.find((s) => s.id === earlier) : undefined;
  const pick = (pid: string | null) => {
    onEarlier(null);
    setPlace(bot.id, pid);
  };
  const row = (pid: string | null) => {
    const path = pid ? projects.find((p) => p.id === pid)?.path : bot.workspace;
    const here = !earlier && place === pid;
    const working = Boolean(bot.runningTaskId) && bot.runningProjectId === pid;
    return (
      <Dropdown.Item key={pid ?? "ws"} onSelect={() => pick(pid)} className={cx(menuItem, "items-start")}>
        {pid ? <FolderGit2 size={14} className="mt-0.5 shrink-0 text-fg-3" /> : <House size={14} className="mt-0.5 shrink-0 text-fg-3" />}
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5">
            <span className="truncate">{placeName(pid)}</span>
            {working && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-violet breathe" aria-label={t("Working here now")} />}
          </span>
          {path && (
            <span dir="ltr" className="block truncate text-start font-mono text-[11px] text-fg-3">
              {path}
            </span>
          )}
        </span>
        {here && <Check size={14} className="mt-0.5 shrink-0 text-violet-text" />}
      </Dropdown.Item>
    );
  };
  return (
    <Dropdown.Root>
      <Tip label={t("Where {name} works", { name: bot.name })}>
        <Dropdown.Trigger className={pill} aria-label={t("Place: {place}", { place: oldShown ? oldShown.title : placeName(place) })}>
          {oldShown ? <History size={13} className="shrink-0 text-fg-3" /> : place ? <FolderGit2 size={13} className="shrink-0 text-fg-3" /> : <House size={13} className="shrink-0 text-fg-3" />}
          <span className="truncate">{oldShown ? oldShown.title : placeName(place)}</span>
          <ChevronDown size={12} className="shrink-0 text-fg-3" />
        </Dropdown.Trigger>
      </Tip>
      <Dropdown.Portal>
        <Dropdown.Content align="start" sideOffset={6} className="z-50 max-h-[420px] w-[280px] overflow-y-auto rounded-xl border border-line bg-elevated p-1 shadow-pop">
          <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Its own folder")}</Dropdown.Label>
          {row(null)}
          {places.length > 0 && (
            <>
              <Dropdown.Separator className="my-1 h-px bg-line" />
              <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Projects it has worked in")}</Dropdown.Label>
              {places.map((pid) => row(pid))}
            </>
          )}
          {old.length > 0 && (
            <>
              <Dropdown.Separator className="my-1 h-px bg-line" />
              <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Earlier sessions in its workspace")}</Dropdown.Label>
              {old.slice(0, 10).map((s) => (
                <Dropdown.Item key={s.id} onSelect={() => onEarlier(s.id)} className={menuItem}>
                  <History size={13} className="shrink-0 text-fg-3" />
                  <span className="min-w-0 flex-1 truncate">{s.title}</span>
                  <span className="shrink-0 text-[11px] text-fg-3">{ago(s.updatedAt)}</span>
                  {earlier === s.id && <Check size={14} className="shrink-0 text-violet-text" />}
                </Dropdown.Item>
              ))}
            </>
          )}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

const menuItem = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface";

function BotHeader({ bot, place, earlier, onEarlier }: { bot: Bot; place: string | null; earlier: string | null; onEarlier: (sid: string | null) => void }) {
  const setPaused = useStore((s) => s.setBotPaused);
  const openEditor = useStore((s) => s.openBotEditor);
  const restart = useStore((s) => s.restartBotSession);
  const deleteBot = useStore((s) => s.deleteBot);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const agent = agentLabel({ provider: bot.provider, profileId: bot.profileId });
  const model = bot.profileId ? null : bot.model === "default" ? t("Default model") : bot.model;
  return (
    <ViewHeader
      title={bot.name}
      sub={bot.role}
      lead={<BotMark name={bot.name} color={bot.color} size={20} className="ms-1" />}
      after={<PlaceSwitcher bot={bot} place={place} earlier={earlier} onEarlier={onEarlier} />}
    >
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
            <Dropdown.Item disabled={Boolean(bot.runningTaskId) || bot.places.length === 0} onSelect={() => restart(bot.id)} className={menuItem}>
              <RotateCcw size={13} className="text-fg-3" /> {t("Start fresh")}
              {bot.runningTaskId && <span className="ms-auto text-[11px] text-fg-3">{t("Busy")}</span>}
            </Dropdown.Item>
            <p className="px-2 pb-1.5 text-[11px] leading-[1.45] text-fg-3">{t("The next task in each place starts a new session. The memory file stays.")}</p>
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
              {t("Its queue, schedules and task history go. Its sessions stay as ordinary sessions, and its workspace folder stays, with its memory file.")}
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

/** Sending adds a task (in the place picked, the bot's default at first); "Message" writes to the session shown instead. */
function BotComposer({ bot, sessionId, sessionBusy, shownPlace }: { bot: Bot; sessionId: string | null; sessionBusy: boolean; shownPlace: string | null }) {
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
  const [target, setTarget] = useState<string | null>(bot.projectId ?? null);
  const placeName = usePlaceName();
  const [error, setError] = useState<string | null>(null);
  const ta = useRef<HTMLTextAreaElement>(null);
  const shown = error ?? botError;
  const canMessage = Boolean(sessionId);
  const asMessage = mode === "message" && canMessage;
  // The running task works in the session shown (it may work in another place).
  const taskHere = Boolean(bot.runningTaskId) && bot.runningSessionId === sessionId;

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
    const err = await addTask(bot.id, text, target);
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
            placeholder={
              asMessage
                ? t("Write to {name} directly in {place}. This isn't a task.", { name: bot.name, place: placeName(shownPlace) })
                : t("Give {name} a task…", { name: bot.name })
            }
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
            {!asMessage && (
              <PlacePicker
                bot={bot}
                value={target}
                onChange={setTarget}
                className="ms-1 inline-flex h-7 min-w-0 max-w-[220px] items-center gap-1.5 rounded-lg px-2 text-xs text-fg-2 hover:bg-elevated hover:text-fg data-[state=open]:bg-elevated data-[state=open]:text-fg"
              />
            )}
            <span className="ms-auto me-2 hidden items-center gap-1.5 text-[11px] text-fg-3 md:flex">
              <Kbd>↵</Kbd> {asMessage ? t("send") : t("add to queue")}
            </span>
            {sessionBusy && sessionId && (
              <Tip label={taskHere ? t("Cancel the task in progress") : t("Stop the agent")} side="top">
                <button
                  type="button"
                  onClick={() => (taskHere && bot.runningTaskId ? cancelTask(bot.runningTaskId) : interrupt(sessionId))}
                  aria-label={taskHere ? t("Cancel the task in progress") : t("Stop the agent")}
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

const tabCls =
  "relative h-7 rounded-md px-2.5 text-[12.5px] text-fg-2 hover:text-fg data-[state=active]:bg-elevated data-[state=active]:text-fg";

function BotSidePanel({ bot }: { bot: Bot }) {
  const [tab, setTab] = useState<"queue" | "schedules" | "memory">("queue");
  const tasks = useStore((s) => s.botTasks[bot.id]);
  const schedules = useStore((s) => s.botSchedules[bot.id]);
  const { queued } = orderTasks(tasks ?? []);
  const on = (schedules ?? []).filter((x) => x.enabled).length;
  return (
    <Tabs.Root value={tab} onValueChange={(v) => setTab(v as "queue" | "schedules" | "memory")} className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center border-b border-line-soft ps-3 pe-2">
        <Tabs.List aria-label={t("Bot")} className="flex gap-0.5">
          <Tabs.Trigger value="queue" className={tabCls}>
            {t("Queue")}
            {queued.length > 0 && <span className="ms-1.5 text-[11px] tabular-nums text-fg-3">{queued.length}</span>}
          </Tabs.Trigger>
          <Tabs.Trigger value="schedules" className={tabCls}>
            {t("Schedules")}
            {on > 0 && <span className="ms-1.5 text-[11px] tabular-nums text-fg-3">{on}</span>}
          </Tabs.Trigger>
          <Tabs.Trigger value="memory" className={tabCls}>
            {t("Memory")}
          </Tabs.Trigger>
        </Tabs.List>
      </div>
      <Tabs.Content value="queue" className="scroll-quiet min-h-0 flex-1 overflow-y-auto focus-visible:outline-offset-[-2px]">
        <QueueTab bot={bot} tasks={tasks} />
      </Tabs.Content>
      <Tabs.Content value="schedules" className="scroll-quiet min-h-0 flex-1 overflow-y-auto focus-visible:outline-offset-[-2px]">
        <SchedulesTab bot={bot} schedules={schedules} />
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
              <TaskPlace task={running} />
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
                <span className="min-w-0 flex-1">
                  <span dir="auto" className="line-clamp-2 break-words text-[12.5px] leading-[1.5] text-fg/90">
                    {x.text}
                  </span>
                  <span className="mt-0.5 flex text-[11px] text-fg-3">
                    <TaskPlace task={x} />
                  </span>
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

/** Where a task works, and whether a schedule added it. */
function TaskPlace({ task }: { task: BotTask }) {
  const placeName = usePlaceName();
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      {task.source === "schedule" && (
        <Tip label={t("Added by a schedule")}>
          <Repeat size={11} className="shrink-0" aria-label={t("Added by a schedule")} />
        </Tip>
      )}
      <span className="truncate">{task.projectId ? placeName(task.projectId) : t("Workspace")}</span>
    </span>
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
            <span className="min-w-0 text-fg-3">
              · <TaskPlace task={task} />
            </span>
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

/** The bot's memory file, editable. Shown again from the file when the bot is renamed (its workspace moves). */
function MemoryTab({ bot }: { bot: Bot }) {
  return <MemoryEditor key={`${bot.id}:${bot.workspace}`} bot={bot} />;
}

function demoMemory(bot: Bot): BotMemory {
  return { path: `${bot.workspace}/MEMORY.md`, text: demoBotMemory[bot.id] ?? "" };
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

/** The bot's schedules: what each does, where, when it runs next, on/off, edit, delete, "Run now". */
function SchedulesTab({ bot, schedules }: { bot: Bot; schedules: BotSchedule[] | undefined }) {
  const [editing, setEditing] = useState<BotSchedule | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const live = liveApi.available && !bot.id.startsWith("b-demo-");
  if (!schedules && live) return <p className="p-4 text-[12.5px] text-fg-3">{t("Loading…")}</p>;
  const list = schedules ?? [];
  return (
    <div className="space-y-3 p-3">
      <div className="flex items-center gap-2 px-1">
        <p className="min-w-0 flex-1 text-[11.5px] leading-[1.5] text-fg-3">{t("Tasks that repeat. Each run joins the queue like any task.")}</p>
        <button
          type="button"
          onClick={() => setEditing(null)}
          className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
        >
          <Plus size={12} /> {t("New schedule")}
        </button>
      </div>
      {error && (
        <p role="alert" className="flex items-start gap-2 rounded-lg border border-error/35 bg-error/[0.06] px-2.5 py-2 text-[12.5px] text-fg">
          <span className="min-w-0 flex-1 break-words">{error}</span>
          <button type="button" onClick={() => setError(null)} aria-label={t("Dismiss")} className="shrink-0 rounded p-0.5 text-fg-3 hover:text-fg">
            <X size={13} />
          </button>
        </p>
      )}
      {list.length === 0 ? (
        <p className="px-1 py-6 text-center text-[12.5px] leading-[1.6] text-fg-3">
          {t("No schedules yet. Add one for work that comes back, like a review every weekday at 9:00.")}
        </p>
      ) : (
        <ul className="space-y-2">
          {list.map((x) => (
            <ScheduleRow key={x.id} schedule={x} onEdit={() => setEditing(x)} onError={setError} />
          ))}
        </ul>
      )}
      <ScheduleEditor bot={bot} schedule={editing ?? null} open={editing !== undefined} onClose={() => setEditing(undefined)} />
    </div>
  );
}

function ScheduleRow({ schedule: s, onEdit, onError }: { schedule: BotSchedule; onEdit: () => void; onError: (e: string | null) => void }) {
  const setEnabled = useStore((st) => st.setBotScheduleEnabled);
  const remove = useStore((st) => st.deleteBotSchedule);
  const run = useStore((st) => st.runBotSchedule);
  const placeName = usePlaceName();
  const [confirm, setConfirm] = useState(false);
  const [ran, setRan] = useState(false);
  const act = async (p: Promise<string | null>) => {
    const err = await p;
    onError(err);
    return err;
  };
  const skipped = s.skippedAt && (!s.lastRunAt || s.skippedAt > s.lastRunAt);
  return (
    <li className={cx("rounded-lg border bg-surface px-2.5 py-2", s.enabled ? "border-line" : "border-line-soft")}>
      <div className="flex items-start gap-2">
        <p dir="auto" className={cx("line-clamp-3 min-w-0 flex-1 whitespace-pre-wrap break-words text-[13px] leading-[1.5]", s.enabled ? "text-fg" : "text-fg-2")}>
          {s.text}
        </p>
        <Switch label={s.enabled ? t("Turn this schedule off") : t("Turn this schedule on")} checked={s.enabled} onChange={(v) => void act(setEnabled(s.id, v))} />
      </div>
      <p className="mt-1 flex min-w-0 items-center gap-1.5 text-[11.5px] text-fg-3">
        <Repeat size={11} className="shrink-0" aria-hidden />
        <span className="truncate">{scheduleSummary(s)}</span>
        <span className="shrink-0">·</span>
        {s.projectId ? <FolderGit2 size={11} className="shrink-0" aria-hidden /> : <House size={11} className="shrink-0" aria-hidden />}
        <span className="truncate">{placeName(s.projectId)}</span>
      </p>
      <p className={cx("mt-0.5 text-[11.5px]", s.enabled ? "text-fg-2" : "text-fg-3")}>
        {s.enabled ? (s.nextRunAt ? t("Next: {time}", { time: whenText(s.nextRunAt) }) : t("No run coming up")) : t("Off")}
        {s.lastRunAt && <span className="text-fg-3"> · {t("last run {time}", { time: whenText(s.lastRunAt) })}</span>}
      </p>
      {skipped && (
        <p className="mt-1 flex items-start gap-1.5 text-[11.5px] leading-[1.45] text-warning">
          <CircleAlert size={12} className="mt-px shrink-0" />
          {t("Skipped the {time} run: the one before was still queued or working.", { time: whenText(s.skippedAt!) })}
        </p>
      )}
      <div className="mt-1.5 flex items-center gap-1">
        <button
          type="button"
          onClick={async () => {
            const err = await act(run(s.id));
            setRan(!err);
          }}
          className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
        >
          <Play size={12} /> {t("Run now")}
        </button>
        {ran && (
          <span role="status" className="text-[11.5px] text-success">
            {t("Added to the queue")}
          </span>
        )}
        <span className="ms-auto flex items-center gap-0.5">
          <IconButton label={t("Edit schedule")} size="sm" onClick={onEdit} className="h-7 w-7">
            <Pencil size={13} />
          </IconButton>
          {confirm ? (
            <button
              type="button"
              onClick={() => void act(remove(s.id))}
              onBlur={() => setConfirm(false)}
              autoFocus
              className="h-7 rounded-lg bg-error/90 px-2.5 text-xs font-medium text-white hover:bg-error"
            >
              {t("Delete")}
            </button>
          ) : (
            <IconButton label={t("Delete schedule")} size="sm" onClick={() => setConfirm(true)} className="h-7 w-7">
              <Trash2 size={13} />
            </IconButton>
          )}
        </span>
      </div>
    </li>
  );
}
