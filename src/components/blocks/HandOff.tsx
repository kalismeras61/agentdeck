import { withoutHidden } from "../../lib/setup";
import { agentLabel } from "../../lib/agentName";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { ArrowRight, ArrowUpRight, ChevronDown, ClipboardList, Hammer, PencilLine, Send } from "lucide-react";
import { useState } from "react";
import { providerLabel, type Session } from "../../data/demo";
import type { EventOf } from "../../lib/events";
import type { LiveProvider } from "../../lib/live";
import { clock } from "../../lib/format";
import { useStore } from "../../state/store";
import { AgentMark, cx } from "../ui";
import { CopyButton } from "./code";
import { Markdown } from "./Markdown";

/** Agents a plan can go to, in menu order. */
const TARGETS: LiveProvider[] = ["codex", "claude-code", "gemini-cli", "grok-build", "kimi-code", "antigravity"];

const itemCls = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface data-[disabled]:opacity-45";

/**
 * "Hand off" menu: starts a linked session for another agent in the same project with this text
 * as its task. `planRequestId` also tells the source agent the plan went elsewhere.
 */
export function HandOffMenu({
  session,
  text,
  planRequestId,
  variant = "ghost",
}: {
  session: Session;
  text: string;
  planRequestId?: string;
  variant?: "ghost" | "button";
}) {
  const handOff = useStore((s) => s.handOff);
  const profiles = useStore((s) => s.profiles);
  const agyEnabled = useStore((s) => s.agyEnabled);
  const [busy, setBusy] = useState<string | null>(null);
  const go = (to: LiveProvider, label: string, profileId?: string) => {
    setBusy(label);
    void handOff(session.id, to, text, planRequestId, profileId).finally(() => setBusy(null));
  };
  return (
    <Dropdown.Root>
      <Dropdown.Trigger
        disabled={Boolean(busy)}
        aria-label="Hand off to another agent"
        className={cx(
          "inline-flex items-center gap-1.5 rounded-lg text-[13px] font-medium disabled:opacity-60",
          variant === "button" ? "h-8 border border-line px-3 text-fg hover:bg-elevated" : "h-7 px-2 text-fg-2 hover:bg-surface hover:text-fg",
          "data-[state=open]:bg-surface",
        )}
      >
        <Send size={13} />
        {busy ? `Handing to ${busy}…` : "Hand off"}
        <ChevronDown size={13} className="text-fg-3" />
      </Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content align="start" sideOffset={6} className="z-50 w-[250px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
          <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">New session in this project, with this as its task</Dropdown.Label>
          {withoutHidden(TARGETS, agyEnabled).map((t) => (
            <Dropdown.Item key={t} onSelect={() => go(t, providerLabel[t])} className={itemCls}>
              <AgentMark provider={t} size={18} />
              {providerLabel[t]}
              {t === session.provider && !session.profileId && <span className="ml-auto text-[11px] text-fg-3">fresh context</span>}
            </Dropdown.Item>
          ))}
          {profiles.length > 0 && <Dropdown.Separator className="my-1 h-px bg-line" />}
          {profiles.map((m) => (
            <Dropdown.Item key={m.id} onSelect={() => go(m.kind === "anthropic" ? "claude-code" : "api", m.name, m.id)} className={itemCls}>
              <AgentMark provider={m.kind === "anthropic" ? "claude-code" : "api"} size={18} />
              <span className="truncate">{m.name}</span>
              {m.id === session.profileId && <span className="ml-auto text-[11px] text-fg-3">fresh context</span>}
            </Dropdown.Item>
          ))}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

const planState = {
  approved: {
    text: "Approved. Building it in this session.",
    cls: "text-success",
  },
  kept: { text: "Sent back to keep planning.", cls: "text-fg-2" },
  handed_off: { text: "Handed off.", cls: "text-violet-text" },
  workflow: { text: "Taken by the workflow.", cls: "text-violet-text" },
  expired: { text: "Never answered; the agent stopped.", cls: "text-fg-3" },
} as const;

/** A plan proposed from plan mode. While pending it is the decision point for the session. */
export function PlanCard({ e, session }: { e: EventOf<"plan">; session: Session }) {
  const p = e.payload;
  const resolve = useStore((s) => s.resolvePermission);
  const inFlight = useStore((s) => (p.requestId ? s.permissionInFlight[p.requestId] : undefined));
  const select = useStore((s) => s.select);
  const linked = useStore((s) => (p.handedTo ? s.sessions.find((x) => x.id === p.handedTo!.sessionId) : undefined));
  const pending = p.state === "pending" && Boolean(p.requestId);
  const who = agentLabel(session);

  return (
    <section
      aria-label="Proposed plan"
      className={cx("overflow-hidden rounded-2xl border bg-surface", pending ? "border-violet/45 shadow-card" : "border-line-soft")}
    >
      <header className="flex items-center gap-2.5 border-b border-line-soft px-4 py-3">
        <ClipboardList size={16} className="shrink-0 text-violet-text" />
        <h3 className="text-[14px] font-semibold">{pending ? `${who} has a plan` : "Plan"}</h3>
        {p.filePath && <span className="min-w-0 truncate font-mono text-[11.5px] text-fg-3">{p.filePath}</span>}
        <span className="ml-auto shrink-0 text-xs text-fg-3">{clock(e.ts)}</span>
        <CopyButton text={p.markdown} label="Copy plan" />
      </header>
      <div className="scroll-quiet max-h-[420px] overflow-auto px-4 py-3">
        {p.markdown.trim() ? <Markdown text={p.markdown} /> : <p className="text-[13px] text-fg-3">The agent didn't include the plan text.</p>}
      </div>
      {pending ? (
        <footer className="flex flex-wrap items-center gap-2 border-t border-line-soft px-4 py-3">
          <p className="mr-auto max-w-[300px] text-xs leading-[1.5] text-fg-3">
            {inFlight ? `Sent. Waiting for ${who}.` : "Nothing has changed yet. Build it here, give it to another agent, or ask for changes."}
          </p>
          <button
            type="button"
            disabled={Boolean(inFlight)}
            onClick={() => resolve(e.sessionId, p.requestId!, "deny")}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-[13px] font-medium text-fg hover:bg-elevated disabled:opacity-50"
          >
            <PencilLine size={13} /> Keep planning
          </button>
          <HandOffMenu session={session} text={p.markdown} planRequestId={p.requestId} variant="button" />
          <button
            type="button"
            disabled={Boolean(inFlight)}
            onClick={() => resolve(e.sessionId, p.requestId!, "allow")}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white shadow-[0_1px_0_rgb(255_255_255/0.15)_inset] hover:brightness-110 disabled:opacity-60"
          >
            <Hammer size={13} /> {inFlight === "allow" ? "Starting…" : "Build it here"}
          </button>
        </footer>
      ) : (
        <footer className="flex flex-wrap items-center gap-2 border-t border-line-soft px-4 py-2.5 text-[12.5px]">
          {p.state !== "pending" && <span className={planState[p.state].cls}>{planState[p.state].text}</span>}
          {p.handedTo && (
            <button
              type="button"
              onClick={() => select(p.handedTo!.sessionId)}
              className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-fg hover:bg-elevated"
            >
              <AgentMark provider={p.handedTo.provider} size={16} />
              {linked?.title ?? `${providerLabel[p.handedTo.provider]} session`}
              <ArrowUpRight size={13} className="text-fg-3" />
            </button>
          )}
          {session.live && p.state !== "handed_off" && p.state !== "workflow" && (
            <span className="ml-auto">
              <HandOffMenu session={session} text={p.markdown} />
            </span>
          )}
        </footer>
      )}
    </section>
  );
}

/** The link between two sessions after a hand-off, shown on both sides. */
export function HandoffBlock({ e, session }: { e: EventOf<"handoff">; session: Session }) {
  const p = e.payload;
  const select = useStore((s) => s.select);
  const other = useStore((s) => s.sessions.find((x) => x.id === p.sessionId));
  const [open, setOpen] = useState(p.direction === "in");
  const incoming = p.direction === "in";
  return (
    <section aria-label={incoming ? "Plan received" : "Plan handed off"} className="overflow-hidden rounded-xl border border-line-soft bg-surface/60">
      <div className="flex flex-wrap items-center gap-2 px-3.5 py-2.5 text-[13px]">
        {incoming ? (
          <>
            <AgentMark provider={p.provider} size={18} />
            <ArrowRight size={13} className="text-fg-3" />
            <AgentMark provider={session.provider} size={18} />
            <span className="text-fg">
              Plan from {providerLabel[p.provider]}
              {p.title && <span className="text-fg-2"> · {p.title}</span>}
            </span>
          </>
        ) : (
          <>
            <AgentMark provider={session.provider} size={18} />
            <ArrowRight size={13} className="text-fg-3" />
            <AgentMark provider={p.provider} size={18} />
            <span className="text-fg">Handed to {providerLabel[p.provider]}</span>
          </>
        )}
        <span className="ml-auto flex items-center gap-1">
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
          >
            {open ? "Hide plan" : "Show plan"}
          </button>
          {other ? (
            <button
              type="button"
              onClick={() => select(other.id)}
              className="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
            >
              Open session <ArrowUpRight size={13} />
            </button>
          ) : (
            <span className="px-1.5 text-fg-3">Session deleted</span>
          )}
        </span>
      </div>
      {open && (
        <div className="scroll-quiet max-h-[360px] overflow-auto border-t border-line-soft px-3.5 py-3">
          <Markdown text={p.plan} />
        </div>
      )}
    </section>
  );
}
