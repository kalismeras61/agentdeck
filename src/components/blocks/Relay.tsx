import { withoutHidden } from "../../lib/setup";
import { agentLabel } from "../../lib/agentName";
import * as Dialog from "@radix-ui/react-dialog";
import { ArrowLeftRight, ArrowRight, ArrowUpRight, CheckCircle2, CircleSlash, Repeat2, Square } from "lucide-react";
import { useMemo, useState } from "react";
import { providerLabel, type Session } from "../../data/demo";
import type { EventOf, ProviderId } from "../../lib/events";
import type { LiveProvider, RelayRow } from "../../lib/live";
import { useStore } from "../../state/store";
import { AgentMark, cx } from "../ui";
import { WorkflowForm } from "./Workflow";
import { Markdown } from "./Markdown";

type RelayEvent = EventOf<"relay">;

/** The newest relay this session took part in, if any. */
export function useRelayFor(sessionId: string): RelayRow | undefined {
  const relays = useStore((s) => s.relays);
  return useMemo(() => {
    let best: RelayRow | undefined;
    for (const r of Object.values(relays)) {
      if (r.aSession !== sessionId && r.bSession !== sessionId) continue;
      if (!best || r.createdAt > best.createdAt) best = r;
    }
    return best;
  }, [relays, sessionId]);
}

/** Who speaks next in an active relay: the session whose turn is running, or the one a reply was just sent to. */
function whoseTurn(a?: Session, b?: Session): Session | undefined {
  const busy = (s?: Session) => s && (s.status === "running" || s.status === "waiting");
  if (busy(a)) return a;
  if (busy(b)) return b;
  return undefined;
}

/** Strip above the conversation while two agents are (or were) talking. */
export function RelayBar({ session, relay }: { session: Session; relay: RelayRow }) {
  const sessions = useStore((s) => s.sessions);
  const stop = useStore((s) => s.stopRelay);
  const view = useStore((s) => s.relayView);
  const setView = useStore((s) => s.setRelayView);
  const select = useStore((s) => s.select);
  const a = sessions.find((s) => s.id === relay.aSession);
  const b = sessions.find((s) => s.id === relay.bSession);
  const peer = session.id === relay.aSession ? b : a;
  const active = relay.status === "active";
  const turn = active ? whoseTurn(a, b) : undefined;

  return (
    <div className="border-b border-line-soft bg-surface/60">
      <div className="mx-auto flex max-w-[860px] flex-wrap items-center gap-x-3 gap-y-1.5 px-8 py-2 text-[12.5px]">
        <span className="inline-flex items-center gap-1.5">
          {a && <AgentMark provider={a.provider} size={18} />}
          <ArrowLeftRight size={13} className={active ? "text-violet-text" : "text-fg-3"} />
          {b && <AgentMark provider={b.provider} size={18} />}
        </span>
        <span className="text-fg">
          {active ? "Relay" : relay.status === "done" ? "Relay finished" : "Relay stopped"}
          <span className="text-fg-2">
            {" "}
            · round {relay.rounds} of {relay.maxRounds}
          </span>
        </span>
        {active && (
          <span className="text-fg-2" aria-live="polite">
            {turn ? `${providerLabel[turn.provider]} is ${turn.status === "waiting" ? "waiting for you" : "replying"}` : "Passing the reply on…"}
          </span>
        )}
        {!active && relay.reason && <span className="min-w-0 truncate text-fg-3">{relay.reason}</span>}
        <span className="ml-auto flex items-center gap-1">
          <div role="radiogroup" aria-label="Relay view" className="flex rounded-lg border border-line-soft p-0.5">
            {(
              [
                ["both", "Both agents"],
                ["one", "This session"],
              ] as const
            ).map(([v, label]) => (
              <button
                key={v}
                type="button"
                role="radio"
                aria-checked={view === v}
                onClick={() => setView(v)}
                className={cx("rounded-md px-2 py-0.5", view === v ? "bg-elevated text-fg" : "text-fg-2 hover:text-fg")}
              >
                {label}
              </button>
            ))}
          </div>
          {peer && (
            <button
              type="button"
              onClick={() => select(peer.id)}
              className="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
            >
              Open {providerLabel[peer.provider]} <ArrowUpRight size={13} />
            </button>
          )}
          {active && (
            <button
              type="button"
              onClick={() => stop(relay.id)}
              className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 font-medium text-fg hover:bg-elevated"
            >
              <Square size={10} fill="currentColor" /> Stop relay
            </button>
          )}
        </span>
      </div>
    </div>
  );
}

/** A relay event in one session's own timeline. */
export function RelayBlock({ e, session, compact = false }: { e: RelayEvent; session: Session; compact?: boolean }) {
  const p = e.payload;
  const select = useStore((s) => s.select);
  const [open, setOpen] = useState(false);

  if (p.kind === "started") {
    return (
      <section aria-label="Relay started" className="rounded-xl border border-line-soft bg-surface/60 px-3.5 py-2.5 text-[13px]">
        <div className="flex flex-wrap items-center gap-2">
          <Repeat2 size={15} className="text-violet-text" />
          <span className="text-fg">
            {p.starts ? `Relay with ${providerLabel[p.peerProvider]}` : `${providerLabel[p.peerProvider]} started a relay with this session`}
          </span>
          <span className="text-fg-3">up to {p.maxRounds} rounds</span>
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="ml-auto rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
          >
            {open ? "Hide task" : "Show task"}
          </button>
        </div>
        {open && (
          <div className="mt-2 border-t border-line-soft pt-2">
            <Markdown text={p.task} />
          </div>
        )}
      </section>
    );
  }

  if (p.kind === "ended") {
    const done = p.status === "done";
    return (
      <div className="flex items-center gap-2 px-1 text-[12.5px]">
        {done ? <CheckCircle2 size={14} className="text-success" /> : <CircleSlash size={14} className="text-fg-3" />}
        <span className="text-fg">{done ? "Relay finished" : "Relay stopped"}</span>
        <span className="text-fg-3">
          {p.reason} · {p.rounds} {p.rounds === 1 ? "round" : "rounds"}
        </span>
      </div>
    );
  }

  // A reply from the other agent.
  if (compact) {
    return (
      <div className="flex items-center gap-2 px-1 text-[12px] text-fg-3">
        <span className="h-px w-6 bg-line" />
        Passed to {agentLabel(session)} · round {p.round} of {p.maxRounds}
        <ArrowRight size={12} />
        <span className="h-px flex-1 bg-line" />
      </div>
    );
  }
  return (
    <section aria-label={`Message from ${providerLabel[p.fromProvider]}`} className="rounded-2xl border border-line bg-surface">
      <header className="flex items-center gap-2 border-b border-line-soft px-4 py-2.5 text-[13px]">
        <AgentMark provider={p.fromProvider} size={18} />
        <span className="font-medium text-fg">{providerLabel[p.fromProvider]}</span>
        <ArrowRight size={13} className="text-fg-3" />
        <AgentMark provider={session.provider} size={18} />
        <span className="text-fg-2">{agentLabel(session)}</span>
        <span className="text-fg-3">
          · round {p.round} of {p.maxRounds}
        </span>
        <button
          type="button"
          onClick={() => select(p.fromSessionId)}
          className="ml-auto inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
        >
          Open <ArrowUpRight size={13} />
        </button>
      </header>
      <div className="px-4 py-3">
        <Markdown text={p.markdown} />
      </div>
    </section>
  );
}

const PRESETS: { id: string; label: (me: string, peer: string) => string; hint: (me: string, peer: string) => string; me: string; peer: string }[] = [
  {
    id: "plan-build",
    label: (me, peer) => `${me} plans and reviews, ${peer} builds`,
    hint: (me, peer) => `${me} writes the plan and reviews each change; ${peer} implements it and reports back.`,
    me: "plan the work and review what the other agent builds",
    peer: "implement the plan and report what you changed",
  },
  {
    id: "build-review",
    label: (me, peer) => `${me} builds, ${peer} reviews`,
    hint: (me, peer) => `${me} implements the task; ${peer} reviews each change and points out problems.`,
    me: "implement the task and report what you changed",
    peer: "review each change and point out problems",
  },
  {
    id: "pair",
    label: () => "Equal partners",
    hint: () => "Both agents work on the task and check each other's work.",
    me: "work on the task together with the other agent",
    peer: "work on the task together with the other agent",
  },
];

/** Start a relay from this session: pick the partner, roles, rounds and the task. */
export function StartRelayDialog({ session, open, onOpenChange }: { session: Session; open: boolean; onOpenChange: (v: boolean) => void }) {
  const sessions = useStore((s) => s.sessions);
  const startRelay = useStore((s) => s.startRelay);
  const select = useStore((s) => s.select);
  const others = sessions.filter((s) => s.live && s.projectId === session.projectId && s.id !== session.id && s.provider !== undefined);
  const defaultPartner = session.provider === "codex" ? "new:claude-code" : "new:codex";
  const [partner, setPartner] = useState(defaultPartner);
  const [preset, setPreset] = useState(PRESETS[0]!.id);
  const [rounds, setRounds] = useState(6);
  const [task, setTask] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"workflow" | "relay">("workflow");
  const p = PRESETS.find((x) => x.id === preset)!;
  const profiles = useStore((s) => s.profiles);
  const agyEnabled = useStore((s) => s.agyEnabled);
  const partnerProfile = partner.startsWith("profile:") ? profiles.find((m) => `profile:${m.id}` === partner) : undefined;
  const partnerProvider: ProviderId | undefined = partner.startsWith("new:")
    ? (partner.slice(4) as ProviderId)
    : partnerProfile
      ? partnerProfile.kind === "anthropic"
        ? "claude-code"
        : "api"
      : others.find((s) => s.id === partner)?.provider;

  const submit = async () => {
    setBusy(true);
    setError(null);
    const err = await startRelay({
      sessionId: session.id,
      partnerSessionId: partner.startsWith("new:") || partnerProfile ? undefined : partner,
      partnerProvider: partner.startsWith("new:") ? (partner.slice(4) as LiveProvider) : partnerProfile ? (partnerProvider as LiveProvider) : undefined,
      partnerProfileId: partnerProfile?.id,
      task,
      role: p.me,
      partnerRole: p.peer,
      maxRounds: rounds,
    });
    setBusy(false);
    if (err) setError(err);
    else {
      onOpenChange(false);
      setTask("");
      select(session.id);
    }
  };

  const field =
    "h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-48px)] w-[min(640px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          <div className="flex items-start gap-3">
            <div className="min-w-0 flex-1">
              <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
                <Repeat2 size={16} className="text-violet-text" /> Work with other agents
              </Dialog.Title>
              <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
                {tab === "workflow"
                  ? "Give each stage to the agent you trust with it. Each stage's result goes to the next one automatically; every approval still comes to you."
                  : "Two agents pass replies back and forth until one of them says DONE, the round limit, or you press Stop. Every approval still comes to you."}
              </Dialog.Description>
            </div>
            <div role="tablist" aria-label="How the agents work together" className="flex shrink-0 rounded-lg border border-line bg-bg/40 p-0.5">
              {(
                [
                  ["workflow", "Workflow"],
                  ["relay", "Conversation"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  role="tab"
                  aria-selected={tab === id}
                  onClick={() => setTab(id)}
                  className={cx("h-7 rounded-md px-2.5 text-xs transition-colors", tab === id ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg")}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          {tab === "workflow" ? (
            <div className="mt-4">
              <WorkflowForm
                session={session}
                onDone={() => onOpenChange(false)}
                footerStart={<Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Cancel</Dialog.Close>}
              />
            </div>
          ) : (
            <form
              className="mt-4 space-y-3.5"
              onSubmit={(ev) => {
                ev.preventDefault();
                void submit();
              }}
            >
              <div className="grid grid-cols-2 gap-3">
                <label className="block text-[12px] text-fg-2">
                  Partner
                  <select value={partner} onChange={(ev) => setPartner(ev.target.value)} className={cx(field, "mt-1")}>
                    {withoutHidden(["codex", "claude-code", "gemini-cli", "grok-build", "antigravity"] as const, agyEnabled).map((p) => (
                      <option key={p} value={`new:${p}`}>
                        New {providerLabel[p]} session
                      </option>
                    ))}
                    {profiles.map((m) => (
                      <option key={m.id} value={`profile:${m.id}`}>
                        New {m.name} session
                      </option>
                    ))}
                    {others.map((s) => (
                      <option key={s.id} value={s.id}>
                        {agentLabel(s)}: {s.title}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="block text-[12px] text-fg-2">
                  Rounds at most
                  <select value={rounds} onChange={(ev) => setRounds(Number(ev.target.value))} className={cx(field, "mt-1")}>
                    {[2, 4, 6, 8, 10, 14, 20].map((n) => (
                      <option key={n} value={n}>
                        {n}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <label className="block text-[12px] text-fg-2">
                Roles
                <select value={preset} onChange={(ev) => setPreset(ev.target.value)} className={cx(field, "mt-1")}>
                  {PRESETS.map((x) => (
                    <option key={x.id} value={x.id}>
                      {x.label(agentLabel(session), partnerProfile ? partnerProfile.name : partnerProvider ? providerLabel[partnerProvider] : "the partner")}
                    </option>
                  ))}
                </select>
                <span className="mt-1 block text-[11.5px] leading-[1.5] text-fg-3">
                  {p.hint(agentLabel(session), partnerProfile ? partnerProfile.name : partnerProvider ? providerLabel[partnerProvider] : "the partner")}
                </span>
              </label>
              <label className="block text-[12px] text-fg-2">
                Task
                <textarea
                  value={task}
                  onChange={(ev) => setTask(ev.target.value)}
                  rows={4}
                  placeholder="Fix the discount being applied after tax in invoice.py, with a test."
                  className={cx(field, "mt-1 h-auto resize-none py-2 leading-[1.55]")}
                />
              </label>
              {error && (
                <p role="alert" className="rounded-lg border border-error/35 bg-error/[0.06] px-2.5 py-1.5 text-[12.5px] text-fg">
                  {error}
                </p>
              )}
              <div className="flex justify-end gap-2 pt-1">
                <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Cancel</Dialog.Close>
                <button
                  type="submit"
                  disabled={busy || !task.trim()}
                  className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
                >
                  {busy ? "Starting…" : "Start relay"}
                </button>
              </div>
            </form>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
