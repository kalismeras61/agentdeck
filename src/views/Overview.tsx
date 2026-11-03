import { GitCompareArrows, ShieldAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { AgentMark, cx, StatusPill } from "../components/ui";
import { providerLabel, type Session, type SessionStatus } from "../data/demo";
import { useWorkspaceSessions } from "../hooks/useWorkspace";
import { sessionChanges } from "../hooks/useDerived";
import { getParsed } from "../hooks/useSession";
import type { ProviderId } from "../lib/events";
import { ago } from "../lib/format";
import { useStore } from "../state/store";
import { t, tn } from "../lib/i18n";
import { ViewBody, ViewHeader } from "./ViewHeader";

const GROUPS: { id: string; label: string; statuses: SessionStatus[]; empty: string }[] = /* i18n */ [
  { id: "attention", label: "Needs you", statuses: ["waiting", "error"], empty: "Nothing is waiting on you." },
  { id: "running", label: "Running", statuses: ["running"], empty: "No agent is working right now." },
  { id: "idle", label: "Idle", statuses: ["idle"], empty: "No idle sessions." },
  { id: "done", label: "Completed", statuses: ["done"], empty: "No completed sessions yet." },
];

function lastTask(s: Session): string {
  const events = getParsed(s);
  for (let i = events.length - 1; i >= 0; i--) {
    const p = events[i]!;
    if (p.ok && p.event.type === "user_message") return p.event.payload.text;
  }
  return t("No task yet");
}

function duration(from: number, to: number): string {
  const m = Math.max(1, Math.round((to - from) / 60000));
  if (m < 60) return t("{m} min", { m });
  const h = Math.floor(m / 60);
  return t("{h} h {m} min", { h, m: m % 60 });
}

export function Overview() {
  const sessions = useWorkspaceSessions();
  const projects = useStore((s) => s.projects);
  const select = useStore((s) => s.select);
  const openReview = useStore((s) => s.openReview);
  const setView = useStore((s) => s.setView);
  const [provider, setProvider] = useState<ProviderId | "all">("all");

  const filtered = useMemo(() => sessions.filter((s) => provider === "all" || s.provider === provider), [sessions, provider]);

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title={t("All sessions")} />
      <ViewBody
        wide
        heading={t("All sessions")}
        lede={t("Every session in this workspace, grouped by what it needs from you. Times come from each session's own events.")}
      >
        <div role="radiogroup" aria-label={t("Filter by agent")} className="mb-6 inline-flex rounded-lg border border-line bg-bg/40 p-0.5">
          {(
            [
              ["all", t("All agents")],
              ["claude-code", "Claude Code"],
              ["codex", "Codex"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={provider === id}
              onClick={() => setProvider(id)}
              className={cx("h-7 rounded-md px-3 text-[12.5px]", provider === id ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg")}
            >
              {label}
            </button>
          ))}
        </div>

        <div className="space-y-8">
          {GROUPS.map((g) => {
            const list = filtered.filter((s) => g.statuses.includes(s.status)).sort((a, b) => b.updatedAt - a.updatedAt);
            return (
              <section key={g.id} aria-labelledby={`grp-${g.id}`}>
                <h3 id={`grp-${g.id}`} className="mb-2 flex items-baseline gap-2 text-[13px] font-medium">
                  {t(g.label)}
                  <span className="text-xs tabular-nums text-fg-3">{list.length}</span>
                </h3>
                {list.length === 0 ? (
                  <p className="rounded-xl border border-dashed border-line-soft px-4 py-3 text-[13px] text-fg-3">{t(g.empty)}</p>
                ) : (
                  <ul className="divide-y divide-line-soft overflow-hidden rounded-xl border border-line bg-surface/50">
                    {list.map((s) => {
                      const project = projects.find((p) => p.id === s.projectId);
                      const files = sessionChanges(s);
                      return (
                        <li key={s.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-2 px-4 py-3 lg:grid-cols-[minmax(0,1fr)_150px_130px_auto]">
                          <div className="flex min-w-0 items-start gap-3">
                            <span className="pt-0.5">
                              <AgentMark provider={s.provider} size={22} />
                            </span>
                            <div className="min-w-0">
                              <button type="button" onClick={() => select(s.id)} className="max-w-full truncate rounded text-start text-[14px] font-medium text-fg hover:underline">
                                {s.title === "New session" ? t("New session") : s.title}
                              </button>
                              <p className="truncate text-[12.5px] text-fg-2" title={lastTask(s)}>
                                {lastTask(s)}
                              </p>
                              <p className="mt-0.5 text-xs text-fg-3">
                                {project?.hidden ? t("{name}'s workspace", { name: project.name }) : project?.name}, {providerLabel[s.provider]} {s.model}
                              </p>
                              {s.status === "error" && <p dir="auto" className="mt-1 text-xs text-error">{s.snippet}</p>}
                            </div>
                          </div>
                          <div className="hidden lg:block">
                            <StatusPill status={s.status} />
                          </div>
                          <div className="hidden text-xs leading-[1.6] text-fg-3 lg:block">
                            <span className="block tabular-nums">{t("Active {time} ago", { time: ago(s.updatedAt) })}</span>
                            <span className="block tabular-nums">{t("Ran for {time}", { time: duration(s.startedAt, s.updatedAt) })}</span>
                          </div>
                          <div className="flex items-center justify-end gap-1.5">
                            {s.status === "waiting" && (
                              <button
                                type="button"
                                onClick={() => setView("permissions")}
                                className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-warning/40 px-2.5 text-[12.5px] text-warning hover:bg-warning/10"
                              >
                                <ShieldAlert size={14} /> {t("Review request")}
                              </button>
                            )}
                            {files.length > 0 && (
                              <button
                                type="button"
                                onClick={() => openReview(s.id)}
                                className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg"
                              >
                                <GitCompareArrows size={14} /> {tn(files.length, "{n} file", "{n} files")}
                              </button>
                            )}
                            <button type="button" onClick={() => select(s.id)} className="h-8 rounded-lg border border-line px-3 text-[12.5px] font-medium text-fg hover:bg-elevated">
                              {t("Open")}
                            </button>
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </section>
            );
          })}
        </div>
      </ViewBody>
    </div>
  );
}
