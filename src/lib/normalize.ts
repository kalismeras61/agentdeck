import { normalizeClaude, type NormalizeContext, type StoredEvent } from "./claude";
import { normalizeAcp } from "./acp";
import { normalizeApi } from "./api";
import { normalizeCodex } from "./codex";
import type { ProviderId } from "./events";

type Json = Record<string, unknown>;
const str = (v: unknown) => (typeof v === "string" ? v : undefined);
const asProvider = (v: unknown): ProviderId =>
  v === "codex" || v === "gemini-cli" || v === "grok-build" || v === "kimi-code" || v === "antigravity" || v === "api" ? v : "claude-code";

const MODE_TEXT: Record<string, string> = {
  plan: "Plan mode on. The agent can read the project but AgentDeck refuses any change until you leave plan mode.",
  default: "Plan mode off. The agent can make changes again, with your approval.",
};

/**
 * Picks the provider's normalizer for a stored session log, then adds what AgentDeck itself
 * records the same way for every provider: hand-offs between sessions and mode switches.
 */
export function normalizeFor(provider: ProviderId, events: StoredEvent[], ctx: NormalizeContext): unknown[] {
  const own: Json[] = [];
  const handedOut: {
    planRequestId?: string;
    provider: ProviderId;
    sessionId: string;
    ts: number;
    card: Json;
  }[] = [];
  const skipPrompts = new Set<string>();
  /** Plans a workflow took (by ExitPlanMode request id), and where they went. */
  const taken = new Set<string>();
  const passed: { provider: ProviderId; sessionId: string; ts: number }[] = [];
  const stageOf = (v: unknown) => (v === "plan" || v === "review" ? v : "build");
  const rest: StoredEvent[] = [];
  const base = (e: StoredEvent, id: string) => ({
    eventId: id,
    sessionId: ctx.sessionId,
    ts: e.ts,
    provider,
  });

  for (const e of events) {
    if (e.source !== "agentdeck") {
      rest.push(e);
      continue;
    }
    const p = e.payload;
    if (e.kind === "handoff_in") {
      const prompt = str(p.prompt);
      if (prompt) skipPrompts.add(prompt);
      own.push({
        ...base(e, `ho-${e.seq}`),
        type: "handoff",
        payload: {
          direction: "in",
          provider: asProvider(p.fromProvider),
          sessionId: str(p.fromSessionId) ?? "",
          title: str(p.fromTitle),
          plan: str(p.plan) ?? "",
        },
      });
    } else if (e.kind === "handoff_out") {
      const to = asProvider(p.toProvider);
      const card = {
        ...base(e, `ho-${e.seq}`),
        type: "handoff",
        payload: {
          direction: "out",
          provider: to,
          sessionId: str(p.toSessionId) ?? "",
          plan: str(p.plan) ?? "",
        },
      };
      handedOut.push({
        planRequestId: str(p.planRequestId),
        provider: to,
        sessionId: str(p.toSessionId) ?? "",
        ts: e.ts,
        card,
      });
      own.push(card);
    } else if (e.kind === "relay_started") {
      const prompt = str(p.prompt);
      if (prompt) skipPrompts.add(prompt);
      own.push({
        ...base(e, `rs-${e.seq}`),
        type: "relay",
        payload: {
          kind: "started",
          relayId: str(p.relayId) ?? "",
          peerProvider: asProvider(p.peerProvider),
          peerSessionId: str(p.peerSessionId) ?? "",
          task: str(p.task) ?? "",
          maxRounds: Number(p.maxRounds) || 1,
          starts: p.starts === true,
        },
      });
    } else if (e.kind === "relay_message") {
      const prompt = str(p.prompt);
      if (prompt) skipPrompts.add(prompt);
      own.push({
        ...base(e, `rm-${e.seq}`),
        type: "relay",
        payload: {
          kind: "message",
          relayId: str(p.relayId) ?? "",
          fromProvider: asProvider(p.fromProvider),
          fromSessionId: str(p.fromSessionId) ?? "",
          markdown: str(p.text) ?? "",
          round: Number(p.round) || 1,
          maxRounds: Number(p.maxRounds) || 1,
        },
      });
    } else if (e.kind === "relay_ended") {
      own.push({
        ...base(e, `re-${e.seq}`),
        type: "relay",
        payload: {
          kind: "ended",
          relayId: str(p.relayId) ?? "",
          status: p.status === "done" ? "done" : "stopped",
          reason: str(p.reason) ?? "",
          rounds: Number(p.rounds) || 0,
        },
      });
    } else if (e.kind === "flow_started") {
      own.push({
        ...base(e, `fs-${e.seq}`),
        type: "flow",
        payload: {
          kind: "started",
          flowId: str(p.flowId) ?? "",
          name: str(p.name) ?? "Workflow",
          task: str(p.task) ?? "",
          stage: stageOf(p.stage),
          stages: (Array.isArray(p.stages) ? p.stages : []).slice(0, 3).map((x) => {
            const o = (x ?? {}) as Json;
            return { stage: stageOf(o.stage), sessionId: str(o.sessionId) ?? "", provider: asProvider(o.provider), name: str(o.name) ?? "" };
          }),
        },
      });
    } else if (e.kind === "flow_input") {
      const prompt = str(p.prompt);
      if (prompt) skipPrompts.add(prompt);
      own.push({
        ...base(e, `fi-${e.seq}`),
        type: "flow",
        payload: {
          kind: "input",
          flowId: str(p.flowId) ?? "",
          stage: stageOf(p.stage),
          label: str(p.label) ?? "Message",
          fromProvider: p.fromProvider ? asProvider(p.fromProvider) : undefined,
          fromSessionId: str(p.fromSessionId),
          markdown: str(p.text) ?? "",
        },
      });
    } else if (e.kind === "flow_ended") {
      own.push({
        ...base(e, `fe-${e.seq}`),
        type: "flow",
        payload: { kind: "ended", flowId: str(p.flowId) ?? "", status: p.status === "done" ? "done" : "stopped", reason: str(p.reason) ?? "" },
      });
    } else if (e.kind === "flow_needs_you") {
      own.push({
        ...base(e, `fn-${e.seq}`),
        type: "notice",
        payload: { tone: "info", text: `The workflow answers this agent's requests on its own, but left the next one for you: ${str(p.why) ?? "it looked risky"}.` },
      });
    } else if (e.kind === "flow_plan_taken") {
      const rid = str(p.requestId);
      if (rid) taken.add(rid);
    } else if (e.kind === "flow_passed") {
      passed.push({ provider: asProvider(p.toProvider), sessionId: str(p.toSessionId) ?? "", ts: e.ts });
    } else if (e.kind === "flow_waiting") {
      // The workflow bar asks for approval; nothing to add to the timeline.
    } else if (e.kind === "imported") {
      rest.push(e);
      const n = Number(p.messages) || 0;
      own.push({
        ...base(e, `im-${e.seq}`),
        type: "notice",
        payload: {
          tone: "info",
          text:
            `Imported from ${str(p.from) ?? "the agent"}: ${n} ${n === 1 ? "message" : "messages"}.` +
            (p.resumable === true
              ? " Your next message continues the same conversation."
              : " Its folder is unknown or gone, so the agent starts a fresh conversation here."),
        },
      });
    } else if (e.kind === "imported_message") {
      const text = str(p.text) ?? "";
      own.push(
        p.role === "user"
          ? { ...base(e, `iu-${e.seq}`), type: "user_message", payload: { text } }
          : { ...base(e, `ia-${e.seq}`), type: "assistant_text", payload: { markdown: text } },
      );
    } else if (e.kind === "worktree_merge" || e.kind === "worktree_removed") {
      const merged = e.kind === "worktree_merge";
      if (!merged) rest.push(e); // a fresh conversation in the project folder, like a move
      const conflicts = Array.isArray(p.conflicts) ? (p.conflicts as unknown[]).map(String) : [];
      own.push({
        ...base(e, `wt-${e.seq}`),
        type: "notice",
        payload: {
          tone: "info",
          text: merged
            ? p.merged
              ? `Merged ${str(p.branch) ?? "the worktree's branch"} into ${str(p.base) ?? "the project's branch"}.`
              : conflicts.length
                ? `Merging into ${str(p.base) ?? "the project's branch"} stopped: ${conflicts.join(", ")} changed on both sides. Nothing was merged.`
                : (str(p.message) ?? "Nothing to merge.")
            : `The worktree was removed${p.branchDeleted ? ` with its branch ${str(p.branch) ?? ""}` : ""}. The agent starts a fresh conversation in the project folder; everything above stays here for you.`,
        },
      });
    } else if (e.kind === "approvals_changed") {
      const level = str(p.level);
      const name = level === "all" ? "Everything (no approvals)" : level === "auto" ? "Auto" : "File edits";
      own.push({
        ...base(e, `ap-${e.seq}`),
        type: "notice",
        payload: { tone: "info", text: `Approvals changed to ${name}.${p.continued === true ? " Antigravity carries on with it now." : " The next message uses it."}` },
      });
    } else if (e.kind === "session_moved") {
      rest.push(e); // the provider normalizer treats the next start as a fresh one
      const to = str(p.to) ?? "another project";
      own.push({
        ...base(e, `mv-${e.seq}`),
        type: "notice",
        payload: {
          tone: "info",
          text: `Moved to ${to === "Chats" ? "Chats" : `“${to}”`}${str(p.from) ? ` from ${str(p.from) === "Chats" ? "Chats" : `“${str(p.from)}”`}` : ""}. ${p.kept === true ? "The agent carries on the same conversation there." : "The agent starts a fresh conversation in the new folder; everything above stays here for you."}`,
        },
      });
    } else if (e.kind === "mode_changed") {
      const text = MODE_TEXT[str(p.mode) ?? ""];
      if (text)
        own.push({
          ...base(e, `m-${e.seq}`),
          type: "notice",
          payload: { tone: "info", text },
        });
    } else if (e.kind === "user_message" && skipPrompts.has(str(p.text) ?? "")) {
      // The hand-off card stands in for the long generated prompt.
      skipPrompts.delete(str(p.text) ?? "");
    } else rest.push(e);
  }

  const out = (
    provider === "codex"
      ? normalizeCodex(rest, ctx)
      : provider === "claude-code"
        ? normalizeClaude(rest, ctx)
        : provider === "api"
          ? normalizeApi(rest, ctx)
          : normalizeAcp(provider, rest, ctx)
  ) as Json[];

  for (const h of handedOut) {
    const plan = out.find((ev) => ev.type === "plan" && (h.planRequestId ? (ev.payload as Json).requestId === h.planRequestId : false));
    if (plan) {
      Object.assign(plan.payload as Json, {
        state: "handed_off",
        handedTo: { provider: h.provider, sessionId: h.sessionId },
        decidedAt: h.ts,
      });
      // The plan card already shows the link.
      own.splice(own.indexOf(h.card), 1);
    }
  }
  // Plans a workflow took: no buttons, and a link to the builder once it got them.
  const takenPlans = out.filter((ev) => ev.type === "plan" && taken.has(str((ev.payload as Json).requestId) ?? ""));
  for (const plan of takenPlans) {
    const pl = plan.payload as Json;
    pl.state = "workflow";
    delete pl.autoReason;
    const to = passed.find((x) => x.ts >= (plan.ts as number));
    if (to) pl.handedTo = { provider: to.provider, sessionId: to.sessionId };
  }
  if (!own.length) return out;
  // Stable merge by timestamp; AgentDeck's own events go after provider events with the same ts.
  const merged: Json[] = [];
  let i = 0;
  for (const ev of out) {
    while (i < own.length && (own[i]!.ts as number) < (ev.ts as number)) merged.push(own[i++]!);
    merged.push(ev);
  }
  while (i < own.length) merged.push(own[i++]!);
  return merged;
}
