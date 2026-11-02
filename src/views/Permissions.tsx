import { agentLabel } from "../lib/agentName";
import { Ban, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { permissionTitle, PermissionCard } from "../components/blocks/EventBlocks";
import { AgentMark, cx } from "../components/ui";
import { providerLabel } from "../data/demo";
import { usePermissions } from "../hooks/useDerived";
import { ago, clock } from "../lib/format";
import { useStore } from "../state/store";
import { ViewHeader } from "./ViewHeader";

const decidedLabel = { allowed: "Allowed once", allowed_session: "Allowed for session", denied: "Denied", expired: "Expired", pending: "Pending" } as const;

export function Permissions() {
  const { pending, decided } = usePermissions();
  const select = useStore((s) => s.select);
  const projects = useStore((s) => s.projects);
  const [chosen, setChosen] = useState<string | null>(null);
  const focus = pending.find((p) => p.event.payload.requestId === chosen) ?? pending[0];

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Permissions" />
      <div className="scroll-quiet relative min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1080px] px-8 pb-16 pt-8">
          <h2 className="text-[22px] font-semibold tracking-[-0.015em]">Permissions</h2>
          <p className="mt-1.5 max-w-[68ch] text-[14px] leading-[1.6] text-fg-2">
            Agents pause here when they want to do something that needs your approval. Each decision goes back to the exact request it answers, and the agent stays paused until its provider confirms.
          </p>

          {pending.length === 0 ? (
            <div className="mt-8 flex items-center gap-3 rounded-2xl border border-line-soft bg-surface/50 px-5 py-5">
              <ShieldCheck size={20} className="text-success" />
              <div>
                <p className="text-[14px] font-medium">No requests waiting</p>
                <p className="text-[13px] text-fg-3">New requests from any session will appear here and in the sidebar.</p>
              </div>
            </div>
          ) : (
            <div className="mt-8 grid gap-6 lg:grid-cols-[280px_minmax(0,1fr)]">
              <section aria-labelledby="queue-h">
                <h3 id="queue-h" className="mb-2 flex items-baseline gap-2 text-[13px] font-medium">
                  Waiting
                  <span className="rounded-full bg-warning/15 px-1.5 text-[11px] font-medium tabular-nums text-warning">{pending.length}</span>
                </h3>
                <ul className="space-y-1">
                  {pending.map(({ session, event }) => {
                    const active = focus?.event.payload.requestId === event.payload.requestId;
                    return (
                      <li key={event.payload.requestId}>
                        <button
                          type="button"
                          onClick={() => setChosen(event.payload.requestId)}
                          aria-current={active ? "true" : undefined}
                          className={cx(
                            "relative w-full rounded-xl border px-3 py-2.5 text-left",
                            active ? "border-warning/40 bg-surface" : "border-transparent hover:bg-surface/60",
                          )}
                        >
                          <span className="flex items-center gap-2">
                            <AgentMark provider={session.provider} size={18} />
                            <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{session.title}</span>
                          </span>
                          <span className="mt-1 block truncate font-mono text-[11.5px] text-fg-2">{event.payload.command}</span>
                          <span className="mt-1 block text-[11.5px] text-fg-3">Waiting {ago(event.ts)}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>

              {focus && (
                <div className="min-w-0">
                  <PermissionCard
                    key={focus.event.payload.requestId}
                    e={focus.event}
                    provider={providerLabel[focus.session.provider]}
                    variant="focused"
                    context={
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-fg-2">
                        <span className="inline-flex items-center gap-1.5">
                          <AgentMark provider={focus.session.provider} size={16} />
                          {providerLabel[focus.session.provider]} {focus.session.model}
                        </span>
                        <span className="text-fg-3">in</span>
                        <span>{projects.find((p) => p.id === focus.session.projectId)?.name}</span>
                        <button type="button" onClick={() => select(focus.session.id)} className="ml-auto rounded text-violet-text hover:underline">
                          Open “{focus.session.title}”
                        </button>
                      </div>
                    }
                  />
                  <p className="mt-3 text-xs leading-[1.6] text-fg-3">
                    There is no keyboard shortcut that approves a request. Tab to a button and press Enter, or click it.
                  </p>
                </div>
              )}
            </div>
          )}

          <section className="mt-12" aria-labelledby="history-h">
            <h3 id="history-h" className="mb-2 text-[13px] font-medium">
              Decided in this window
            </h3>
            {decided.length === 0 ? (
              <p className="rounded-xl border border-dashed border-line-soft px-4 py-3 text-[13px] text-fg-3">Decisions you make appear here until the app closes.</p>
            ) : (
              <ul className="divide-y divide-line-soft overflow-hidden rounded-xl border border-line bg-surface/50">
                {decided.map(({ session, event }) => (
                  <li key={event.payload.requestId} className="flex items-center gap-3 px-4 py-2.5 text-[13px]">
                    {event.payload.state === "denied" || event.payload.state === "expired" ? <Ban size={15} className={event.payload.state === "denied" ? "shrink-0 text-error" : "shrink-0 text-fg-3"} /> : <ShieldCheck size={15} className="shrink-0 text-success" />}
                    <span className="w-[150px] shrink-0 text-fg">{decidedLabel[event.payload.state]}</span>
                    <span className="min-w-0 flex-1 truncate" title={permissionTitle(event.payload, agentLabel(session))}>
                      <span className="font-mono text-[12px] text-fg-2">{event.payload.command}</span>
                    </span>
                    <span className="hidden shrink-0 truncate text-xs text-fg-3 md:block">{session.title}</span>
                    <span className="shrink-0 text-xs tabular-nums text-fg-3">{clock(event.payload.decidedAt ?? event.ts)}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
