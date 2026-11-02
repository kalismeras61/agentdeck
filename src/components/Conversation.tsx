import { agentLabel } from "../lib/agentName";
import { ClipboardList, FileCode2, Repeat2, FlaskConical, Image as ImageIcon, MessageCircleQuestion, Send, ShieldAlert, Sparkles, SquareTerminal, XCircle } from "lucide-react";
import { motion } from "motion/react";
import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { type Session } from "../data/demo";
import { useProject } from "../hooks/useWorkspace";
import { useParsedEvents } from "../hooks/useSession";
import type { AgentEvent, ParsedEvent } from "../lib/events";
import { clock } from "../lib/format";
import { useStore } from "../state/store";
import { t } from "../lib/i18n";
import {
  Changeset,
  ErrorBlock,
  ImageBlock,
  InvalidEventBlock,
  NoticeBlock,
  PermissionCard,
  ProgressCard,
  TestSummary,
  ToolGroup,
  UserBubble,
} from "./blocks/EventBlocks";
import { HandoffBlock, HandOffMenu, PlanCard } from "./blocks/HandOff";
import { RelayBar, RelayBlock, useRelayFor } from "./blocks/Relay";
import { SignInHelp } from "./blocks/SignIn";
import { DeniedCard } from "./blocks/Denied";
import { QuestionCard } from "./blocks/Question";
import { FlowBar, FlowBlock } from "./blocks/Workflow";
import { useFlowFor } from "../lib/workflows";
import { Markdown } from "./blocks/Markdown";
import { AgentMark, cx } from "./ui";

type Run =
  | { kind: "user"; item: ParsedEvent & { ok: true } }
  | { kind: "relay"; item: ParsedEvent & { ok: true }; session: Session }
  | { kind: "agent"; items: ParsedEvent[]; session: Session };

type Tagged = { p: ParsedEvent; session: Session };

/** Splits a timeline into user messages, relay cards and runs of one agent's work. */
function groupRuns(events: Tagged[]): Run[] {
  const runs: Run[] = [];
  for (const { p, session } of events) {
    if (p.ok && p.event.type === "user_message") {
      runs.push({ kind: "user", item: p });
      continue;
    }
    if (p.ok && (p.event.type === "relay" || p.event.type === "flow") && p.event.payload.kind !== "ended") {
      runs.push({ kind: "relay", item: p, session });
      continue;
    }
    const last = runs[runs.length - 1];
    if (last?.kind === "agent" && last.session.id === session.id) last.items.push(p);
    else runs.push({ kind: "agent", items: [p], session });
  }
  return runs;
}

/** Runs (a message, or one agent's stretch of work) drawn at a time in long conversations. */
const RUNS_PAGE = 80;

const tsOf = (p: ParsedEvent) => (p.ok ? p.event.ts : p.ts);

/** Both sides of a relay as one conversation, ordered by time. */
function mergeRelay(own: Session, ownEvents: ParsedEvent[], peer: Session, peerEvents: ParsedEvent[], relayId: string): Tagged[] {
  const keep = (p: ParsedEvent, isOwn: boolean) => {
    if (!p.ok || p.event.type !== "relay") return true;
    const r = p.event.payload;
    if (r.relayId !== relayId) return true;
    // One "started" card (the side that got the task) and one "ended" line.
    if (r.kind === "started") return r.starts;
    if (r.kind === "ended") return isOwn;
    return true;
  };
  const all: Tagged[] = [
    ...ownEvents.filter((p) => keep(p, true)).map((p) => ({ p, session: own })),
    ...peerEvents.filter((p) => keep(p, false)).map((p) => ({ p, session: peer })),
  ];
  return all.sort((x, y) => tsOf(x.p) - tsOf(y.p));
}

function nodeFor(e: AgentEvent): ReactNode {
  const cls = "relative z-10 flex h-[22px] w-[22px] items-center justify-center rounded-full border bg-bg";
  switch (e.type) {
    case "assistant_text":
      return (
        <span className={cx(cls, "border-transparent")}>
          <span className="h-[7px] w-[7px] rounded-full bg-violet" />
        </span>
      );
    case "tool_group":
      return (
        <span className={cx(cls, "border-line text-fg-3")}>
          <SquareTerminal size={11} />
        </span>
      );
    case "changeset":
      return (
        <span className={cx(cls, "border-violet/50 text-violet-text")}>
          <FileCode2 size={11} />
        </span>
      );
    case "test_result":
      return (
        <span className={cx(cls, "border-line text-success")}>
          <FlaskConical size={11} />
        </span>
      );
    case "image":
      return (
        <span className={cx(cls, "border-line text-blue")}>
          <ImageIcon size={11} />
        </span>
      );
    case "permission":
      return (
        <span className={cx(cls, e.payload.state === "pending" ? "border-warning/60 text-warning" : "border-line text-fg-3")}>
          <ShieldAlert size={11} />
        </span>
      );
    case "question":
      return (
        <span className={cx(cls, e.payload.state === "pending" ? "border-violet/60 text-violet-text" : "border-line text-fg-3")}>
          <MessageCircleQuestion size={11} />
        </span>
      );
    case "denied":
      return (
        <span className={cx(cls, "border-warning/60 text-warning")}>
          <ShieldAlert size={11} />
        </span>
      );
    case "error":
      return (
        <span className={cx(cls, "border-error/60 text-error")}>
          <XCircle size={11} />
        </span>
      );
    case "plan":
      return (
        <span className={cx(cls, e.payload.state === "pending" ? "border-violet/60 text-violet-text" : "border-line text-fg-3")}>
          <ClipboardList size={11} />
        </span>
      );
    case "handoff":
      return (
        <span className={cx(cls, "border-line text-violet-text")}>
          <Send size={11} />
        </span>
      );
    default:
      return <span className={cx(cls, "border-transparent")} />;
  }
}

export function EventBody({ e, session }: { e: AgentEvent; session: Session }) {
  switch (e.type) {
    case "assistant_text":
      return session.live && e.payload.markdown.length > 80 ? (
        <div className="group/reply">
          <Markdown text={e.payload.markdown} />
          <div className="mt-1 -ml-2 opacity-0 transition-opacity focus-within:opacity-100 group-hover/reply:opacity-100">
            <HandOffMenu session={session} text={e.payload.markdown} />
          </div>
        </div>
      ) : (
        <Markdown text={e.payload.markdown} />
      );
    case "plan":
      return <PlanCard e={e} session={session} />;
    case "handoff":
      return <HandoffBlock e={e} session={session} />;
    case "relay":
      return <RelayBlock e={e} session={session} />;
    case "flow":
      return <FlowBlock e={e} session={session} />;
    case "tool_group":
      return <ToolGroup e={e} />;
    case "changeset":
      return <Changeset e={e} />;
    case "test_result":
      return <TestSummary e={e} />;
    case "image":
      return <ImageBlock e={e} />;
    case "permission":
      return <PermissionCard e={e} provider={agentLabel(session)} />;
    case "denied":
      return <DeniedCard e={e} session={session} />;
    case "question":
      return <QuestionCard e={e} session={session} />;
    case "error":
      return (
        <div>
          <ErrorBlock e={e} />
          <SignInHelp session={session} e={e} />
        </div>
      );
    case "notice":
      return <NoticeBlock e={e} />;
    case "progress":
      return <ProgressCard e={e} />;
    default:
      return null;
  }
}

function AgentRun({ items, session, live }: { items: ParsedEvent[]; session: Session; live: boolean }) {
  const streaming = useStore((s) => (live ? (s.streaming[session.id] ?? "") : ""));
  const firstTs = items.find((i) => i.ok)?.ok ? (items.find((i) => i.ok) as { event: AgentEvent }).event.ts : undefined;
  return (
    <div className="relative">
      <div className="mb-3 flex items-center gap-2.5">
        <AgentMark provider={session.provider} size={22} />
        <span className="text-[13px] font-medium">{agentLabel(session)}</span>
        {firstTs !== undefined && <span className="text-xs text-fg-3">{clock(firstTs)}</span>}
      </div>
      <div className="relative">
        {/* The thread rail */}
        <span aria-hidden className="absolute bottom-3 start-[10px] top-3 w-px bg-line" />
        <ol className="space-y-5">
          {items.map((p) => (
            <li key={p.ok ? p.event.eventId : p.eventId} className="grid grid-cols-[22px_1fr] gap-x-4">
              <div className="pt-[1px]">{p.ok ? nodeFor(p.event) : nodeFor({ type: "error" } as AgentEvent)}</div>
              <div className="min-w-0">{p.ok ? <EventBody e={p.event} session={session} /> : <InvalidEventBlock reason={p.reason} />}</div>
            </li>
          ))}
          {live && streaming && session.status === "running" && (
            <li className="grid grid-cols-[22px_1fr] gap-x-4">
              <div className="pt-[1px]">
                <span className="relative z-10 flex h-[22px] w-[22px] items-center justify-center rounded-full bg-bg">
                  <span className="breathe h-[7px] w-[7px] rounded-full bg-violet text-violet" />
                </span>
              </div>
              <div className="min-w-0">
                <Markdown text={streaming} />
              </div>
            </li>
          )}
          {live && (
            <li className="grid grid-cols-[22px_1fr] gap-x-4" aria-live="polite">
              <div className="flex justify-center">
                <span className="rail-live block h-7 w-[2px] rounded-full" />
              </div>
              <p className="pt-0.5 text-xs text-fg-3">
                {session.status === "waiting"
                  ? items.some((p) => p.ok && p.event.type === "plan" && p.event.payload.state === "pending")
                    ? t("Waiting for your decision on the plan.")
                    : items.some((p) => p.ok && p.event.type === "question" && p.event.payload.state === "pending")
                      ? t("Waiting for your answer above.")
                      : t("Paused until you allow or deny the request above.")
                  : streaming
                    ? t("Writing…")
                    : t("Working…")}
              </p>
            </li>
          )}
        </ol>
      </div>
    </div>
  );
}

const DEMO_SUGGESTIONS = /* i18n */ [
  "Find why the bookings list re-renders on every scroll and fix it",
  "Add golden tests for the login screen in light and dark themes",
  "Explain how routing is set up in lib/app/router.dart",
];
const CHAT_SUGGESTIONS = /* i18n */ [
  "Explain how OAuth refresh tokens work, with a small example",
  "Write a shell one-liner that finds the 10 largest files here",
  "Start a new Vite + React app in a folder called my-app",
];

const LIVE_SUGGESTIONS = /* i18n */ [
  "Explain what this project does and how it's organized",
  "Run the tests and tell me what fails",
  "Find one small bug and propose a fix before changing anything",
];

/** A translated sentence with elements (paths, branch names) in place of its {placeholders}. */
function rich(text: string, parts: Record<string, ReactNode>): ReactNode[] {
  return text.split(/(\{\w+\})/).map((seg, i) => {
    const key = /^\{(\w+)\}$/.exec(seg)?.[1];
    return key && key in parts ? <Fragment key={i}>{parts[key]}</Fragment> : seg;
  });
}

function EmptySession({ session }: { session: Session }) {
  const setDraft = useStore((s) => s.setDraft);
  const setMode = useStore((s) => s.setSessionMode);
  const openRelay = useStore((s) => s.openRelayDialog);
  const focusComposer = useStore((s) => s.focusComposer);
  const project = useProject(session.projectId);
  const isChat = project?.kind === "chats";
  return (
    <div className="mx-auto flex h-full max-w-[560px] flex-col justify-center px-8 pb-16">
      <div className="mb-5 flex h-10 w-10 items-center justify-center rounded-xl bg-violet-soft text-violet-text">
        <Sparkles size={18} />
      </div>
      <h2 className="text-[20px] font-semibold tracking-[-0.01em]">
        {isChat ? t("Ask {agent} anything", { agent: agentLabel(session) }) : t("What should {agent} work on?", { agent: agentLabel(session) })}
      </h2>
      {isChat ? (
        <p className="mt-1.5 text-[14px] text-fg-2">
          {rich(t("No project needed. Files it makes go to {path}; move the session into a project later from its {menu} menu."), {
            path: <span className="font-mono text-[13px] text-fg">{project?.path.replace(/^\/(Users|home)\/[^/]+/, "~")}</span>,
            menu: "⋯",
          })}
        </p>
      ) : (
        <p className="mt-1.5 text-[14px] text-fg-2">
          {session.worktree ? (
            <>
              {rich(t("The session works in its own worktree, {path}, on the branch {branch}. The project folder stays as it is until you merge."), {
                path: <span className="font-mono text-[13px] text-fg">{session.worktree.replace(/^\/(Users|home)\/[^/]+/, "~")}</span>,
                branch: <span className="font-mono text-[13px] text-fg">{session.branch}</span>,
              })}
            </>
          ) : (
            <>
              {rich(t("The session runs in {path}. Describe the outcome you want; you'll approve anything risky before it runs."), {
                path: <span className="font-mono text-[13px] text-fg">{project?.path}</span>,
              })}
            </>
          )}
        </p>
      )}
      {session.live && (
        <div className="mt-5 grid gap-2 sm:grid-cols-2">
          <button
            type="button"
            onClick={() => setMode(session.id, "plan")}
            className="rounded-xl border border-line-soft bg-surface/50 px-3.5 py-2.5 text-start hover:border-line hover:bg-surface"
          >
            <span className="flex items-center gap-2 text-[13.5px] text-fg">
              <ClipboardList size={14} className="text-violet-text" /> {t("Plan first")}
            </span>
            <span className="mt-0.5 block text-[12px] leading-[1.5] text-fg-3">{t("It reads and proposes a plan; build it here or hand it to another agent.")}</span>
          </button>
          <button
            type="button"
            onClick={() => openRelay(session.id)}
            className="rounded-xl border border-line-soft bg-surface/50 px-3.5 py-2.5 text-start hover:border-line hover:bg-surface"
          >
            <span className="flex items-center gap-2 text-[13.5px] text-fg">
              <Repeat2 size={14} className="text-violet-text" /> {t("Plan, build, review")}
            </span>
            <span className="mt-0.5 block text-[12px] leading-[1.5] text-fg-3">{t("One agent plans, another builds, a third reviews. You pick who.")}</span>
          </button>
        </div>
      )}
      <div className="mt-6 space-y-2">
        {(isChat ? CHAT_SUGGESTIONS : session.live ? LIVE_SUGGESTIONS : DEMO_SUGGESTIONS).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => {
              setDraft(session.id, t(s));
              focusComposer();
            }}
            className="block w-full rounded-xl border border-line-soft bg-surface/50 px-3.5 py-2.5 text-start text-[13.5px] text-fg-2 hover:border-line hover:bg-surface hover:text-fg"
          >
            {t(s)}
          </button>
        ))}
      </div>
    </div>
  );
}

export function Conversation({ session }: { session: Session }) {
  const relay = useRelayFor(session.id);
  const flow = useFlowFor(session.id);
  // The newest of the two is the one this session is part of now.
  const showFlow = Boolean(flow && (!relay || flow.createdAt >= relay.createdAt));
  const relayView = useStore((s) => s.relayView);
  const peer = useStore((s) => (relay ? s.sessions.find((x) => x.id === (relay.aSession === session.id ? relay.bSession : relay.aSession)) : undefined));
  const ensureEvents = useStore((s) => s.ensureEvents);
  useEffect(() => {
    if (peer) void ensureEvents(peer.id);
  }, [peer?.id]); // eslint-disable-line react-hooks/exhaustive-deps
  const ownEvents = useParsedEvents(session);
  const peerEvents = useParsedEvents(peer);
  const merged = Boolean(relay && peer && relayView === "both");
  const tagged = useMemo<Tagged[]>(
    () => (merged && peer && relay ? mergeRelay(session, ownEvents, peer, peerEvents, relay.id) : ownEvents.map((p) => ({ p, session }))),
    [merged, peer, relay, session, ownEvents, peerEvents],
  );
  const events = useMemo(() => tagged.map((x) => x.p), [tagged]);
  const busy = (s?: Session) => Boolean(s && (s.status === "running" || s.status === "waiting"));
  const live = busy(session) || (merged && busy(peer));
  const runs = useMemo(() => {
    const r = groupRuns(tagged);
    // An agent is working but hasn't produced anything yet: show its row so progress is visible.
    const working = busy(session) ? session : merged && busy(peer) ? peer : undefined;
    const last = r[r.length - 1];
    if (working && (last?.kind !== "agent" || last.session.id !== working.id)) r.push({ kind: "agent", items: [], session: working });
    return r;
  }, [tagged, session, peer, merged]); // eslint-disable-line react-hooks/exhaustive-deps
  const streamLen = useStore((s) => s.streaming[session.id]?.length ?? 0);
  const loaded = useStore((s) => !session.live || s.liveEvents[session.id] !== undefined);
  const scroller = useRef<HTMLDivElement>(null);
  const prevLen = useRef(0);
  // Long conversations draw their latest part; earlier runs come in pages.
  const [page, setPage] = useState({ id: session.id, n: RUNS_PAGE });
  const shownN = page.id === session.id ? page.n : RUNS_PAGE;
  const hidden = Math.max(0, runs.length - shownN);
  const visible = hidden ? runs.slice(hidden) : runs;
  const keepFromBottom = useRef<number | null>(null);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && keepFromBottom.current !== null) el.scrollTop = el.scrollHeight - keepFromBottom.current;
    keepFromBottom.current = null;
  }, [shownN]);

  // Jump to the latest activity when switching sessions; follow new events smoothly.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
    prevLen.current = events.length;
  }, [session.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const el = scroller.current;
    if (!el || events.length === prevLen.current) return;
    const first = prevLen.current === 0;
    prevLen.current = events.length;
    const reduce = document.documentElement.dataset.motion === "reduced";
    // History that just loaded opens at the bottom at once; new events scroll there smoothly.
    el.scrollTo({ top: el.scrollHeight, behavior: reduce || first ? "auto" : "smooth" });
  }, [events.length]);

  // While text streams in, stay pinned to the bottom unless the reader scrolled up.
  useEffect(() => {
    const el = scroller.current;
    if (!el || !streamLen) return;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 160) el.scrollTop = el.scrollHeight;
  }, [streamLen]);

  if (!loaded)
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-fg-3" role="status">
        {t("Loading conversation…")}
      </div>
    );
  if (events.length === 0) return <EmptySession session={session} />;

  return (
    <div className="flex h-full flex-col">
      {showFlow && flow ? <FlowBar session={session} flow={flow} /> : relay && <RelayBar session={session} relay={relay} />}
      <div ref={scroller} className="scroll-quiet relative min-h-0 flex-1 overflow-y-auto" role="log" aria-label={t("{title} conversation", { title: session.title })}>
        <motion.div
          key={session.id}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.22, ease: [0.2, 0.8, 0.2, 1] }}
          className="mx-auto max-w-[860px] space-y-8 px-8 pb-10 pt-8"
        >
          {hidden > 0 && (
            <div className="flex justify-center">
              <button
                type="button"
                onClick={() => {
                  const el = scroller.current;
                  if (el) keepFromBottom.current = el.scrollHeight - el.scrollTop;
                  setPage({ id: session.id, n: shownN + RUNS_PAGE });
                }}
                className="rounded-full border border-line-soft px-3 py-1 text-[12.5px] text-fg-2 hover:bg-surface hover:text-fg"
              >
                {t("Show earlier ({n} more)", { n: hidden })}
              </button>
            </div>
          )}
          {visible.map((r, j) => {
            const i = j + hidden;
            return (
              <Fragment key={r.kind === "agent" ? `run-${r.session.id}-${i}` : r.item.event.eventId}>
                {r.kind === "user" ? (
                  r.item.event.type === "user_message" ? (
                    <UserBubble e={r.item.event} />
                  ) : null
                ) : r.kind === "relay" ? (
                  r.item.event.type === "relay" ? (
                    <RelayBlock e={r.item.event} session={r.session} compact={merged && r.item.event.payload.kind === "message"} />
                  ) : r.item.event.type === "flow" ? (
                    <FlowBlock e={r.item.event} session={r.session} />
                  ) : null
                ) : (
                  <AgentRun items={r.items} session={r.session} live={live && i === runs.length - 1} />
                )}
              </Fragment>
            );
          })}
        </motion.div>
      </div>
    </div>
  );
}
