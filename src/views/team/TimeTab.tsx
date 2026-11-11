import { Check, ChevronRight, Clipboard, Clock } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { cx } from "../../components/ui";
import { DEMO_TIME_SESSIONS, demoTimeRows } from "../../data/teamDemo";
import { t, tn } from "../../lib/i18n";
import { errorText } from "../../lib/live";
import type { Presence } from "../../lib/team";
import { groupBy, hm, lastDays, perDay, sharedIn, sum, timeApi, toCsv, total, type TimeRow, type Totals } from "../../lib/time";
import { useStore } from "../../state/store";

const RANGES = /* i18n */ [
  { days: 1, label: "Today" },
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
] as const;

/** Series in stacking order, bottom first. Colors are validated chart tokens (index.css). */
const LABELS = /* i18n */ ["Agents working", "Waiting for you", "Your time"] as const;
const SERIES = [
  { key: "agentMs", label: LABELS[0], color: "var(--chart-agent)" },
  { key: "waitingMs", label: LABELS[1], color: "var(--chart-waiting)" },
  { key: "youMs", label: LABELS[2], color: "var(--chart-you)" },
] as const;

function Legend() {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-fg-2">
      {SERIES.map((s) => (
        <span key={s.key} className="flex items-center gap-1.5">
          <span aria-hidden className="h-2.5 w-2.5 rounded-[3px]" style={{ background: s.color }} />
          {t(s.label)}
        </span>
      ))}
    </div>
  );
}

function Tile({ label, value, color, note }: { label: string; value: number; color: string; note?: string }) {
  return (
    <div className="min-w-0 flex-1 rounded-xl border border-line-soft px-4 py-3">
      <p className="flex items-center gap-1.5 text-[12px] text-fg-3">
        <span aria-hidden className="h-2 w-2 rounded-full" style={{ background: color }} />
        {label}
      </p>
      <p className="mt-1 text-[22px] font-semibold tabular-nums tracking-[-0.01em]">{hm(value)}</p>
      {note && <p className="mt-0.5 truncate text-[11.5px] text-fg-3">{note}</p>}
    </div>
  );
}

/** Stacked columns per day: agents, waiting, you. Hover a column for its numbers. */
function DayChart({ days }: { days: { day: string; totals: Totals }[] }) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(...days.map((d) => total(d.totals)), 1);
  // Clean hour ticks: 1, 2, 4… hours so that there are at most four lines.
  const hourMs = 3_600_000;
  const stepH = [0.5, 1, 2, 3, 4, 6, 8, 12].find((h) => (max / (h * hourMs)) <= 4) ?? 24;
  const top = Math.max(stepH * hourMs, Math.ceil(max / (stepH * hourMs)) * stepH * hourMs);
  const ticks = Array.from({ length: Math.round(top / (stepH * hourMs)) + 1 }, (_, i) => i * stepH * hourMs);
  const label = (d: string) => new Date(d + "T12:00:00").toLocaleDateString(undefined, days.length > 10 ? { day: "numeric" } : { weekday: "short", day: "numeric" });
  const h = hover !== null ? days[hover] : null;
  return (
    <div className="relative">
      <div className="relative ms-10 h-[180px]" onMouseLeave={() => setHover(null)}>
        {ticks.map((v) => (
          <div key={v} className="absolute inset-x-0 border-t border-line-soft" style={{ bottom: `${(v / top) * 100}%` }}>
            <span className="absolute -start-10 -translate-y-1/2 text-[11px] tabular-nums text-fg-3">{v === 0 ? "0" : `${v / hourMs}h`}</span>
          </div>
        ))}
        <div className="absolute inset-0 flex items-end">
          {days.map((d, i) => {
            const tot = total(d.totals);
            const parts = SERIES.map((s) => ({ ...s, ms: d.totals[s.key] })).filter((p) => p.ms > 0);
            return (
              <button
                key={d.day}
                type="button"
                aria-label={`${label(d.day)}: ${SERIES.map((s) => `${t(s.label)} ${hm(d.totals[s.key])}`).join(", ")}`}
                onMouseEnter={() => setHover(i)}
                onFocus={() => setHover(i)}
                onBlur={() => setHover(null)}
                className={cx("flex h-full min-w-0 flex-1 flex-col items-center justify-end rounded-md outline-none", hover === i && "bg-elevated/60")}
              >
                <span className="flex w-full max-w-[24px] flex-col-reverse gap-[2px]" style={{ height: `${(tot / top) * 100}%` }}>
                  {parts.map((p, k) => (
                    <span key={p.key} className={cx("block w-full", k === parts.length - 1 && "rounded-t-[4px]")} style={{ flexGrow: p.ms, flexBasis: 0, minHeight: 2, background: p.color }} />
                  ))}
                </span>
              </button>
            );
          })}
        </div>
        {h && hover !== null && (
          <div
            role="tooltip"
            className="pointer-events-none absolute top-0 z-10 w-[190px] rounded-lg border border-line bg-surface p-2.5 text-[12px] shadow-lg"
            style={{ insetInlineStart: `min(calc(${((hover + 0.5) / days.length) * 100}% + 12px), calc(100% - 190px))` }}
          >
            <p className="mb-1 font-medium text-fg">{new Date(h.day + "T12:00:00").toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "short" })}</p>
            {SERIES.map((s) => (
              <p key={s.key} className="flex items-center gap-1.5 text-fg-2">
                <span aria-hidden className="h-2 w-2 rounded-[2px]" style={{ background: s.color }} />
                {t(s.label)}
                <span className="ms-auto tabular-nums text-fg">{hm(h.totals[s.key])}</span>
              </p>
            ))}
          </div>
        )}
      </div>
      <div className="ms-10 mt-1 flex">
        {days.map((d, i) => (
          <span key={d.day} className={cx("min-w-0 flex-1 truncate text-center text-[10.5px] tabular-nums text-fg-3", days.length > 10 && i % 3 !== 0 && i !== days.length - 1 && "invisible")}>
            {label(d.day)}
          </span>
        ))}
      </div>
    </div>
  );
}

const cells = (x: Totals) => (
  <>
    <td className="px-3 py-1.5 text-end tabular-nums text-fg">{hm(x.agentMs)}</td>
    <td className="px-3 py-1.5 text-end tabular-nums text-fg-2">{hm(x.waitingMs)}</td>
    <td className="px-3 py-1.5 text-end tabular-nums text-fg">{hm(x.youMs)}</td>
  </>
);

/**
 * Time: how long your agents worked and waited for you, and how long you spent with them, by
 * day and branch; plus what teammates who share their work spent on their branches.
 */
export function TimeTab({ projectId, presence, me }: { projectId: string | null; presence: Presence[]; me: string }) {
  const projects = useStore((s) => s.projects);
  const sessions = useStore((s) => s.sessions);
  const select = useStore((s) => s.select);
  const demo = !timeApi.available;
  const [range, setRange] = useState<number>(7);
  const [all, setAll] = useState(false);
  const [rows, setRows] = useState<TimeRow[] | null>(demo ? demoTimeRows() : null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (demo) return;
    let live = true;
    const load = () =>
      timeApi
        .report(30)
        .then((r) => live && (setRows(r), setError(null)))
        .catch((e: unknown) => live && setError(errorText(e)));
    load();
    // Running agents keep adding time; refresh while the tab is open.
    const id = window.setInterval(load, 60_000);
    return () => {
      live = false;
      window.clearInterval(id);
    };
  }, [demo]);

  const days = useMemo(() => lastDays(range), [range]);
  const shown = useMemo(() => {
    const set = new Set(days);
    return (rows ?? []).filter((r) => set.has(r.day) && (all || demo || r.projectId === projectId));
  }, [rows, days, all, projectId, demo]);
  const totals = sum(shown);
  const branches = useMemo(() => groupBy(shown, (r) => r.branch ?? ""), [shown]);
  const projectName = (id: string) => projects.find((p) => p.id === id)?.name ?? (demo ? "shop-app" : id);
  const sessionName = (id: string) => sessions.find((s) => s.id === id)?.title ?? DEMO_TIME_SESSIONS[id] ?? t("Removed session");
  const team = presence
    .filter((p) => p.email !== me)
    .map((p) => ({ p, rows: groupBy(sharedIn(p.time, days), (r) => r.branch) }))
    .filter((x) => x.rows.length > 0);

  const copyCsv = () => {
    void navigator.clipboard?.writeText(toCsv(shown, { project: projectName, session: sessionName })).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[980px] space-y-5 px-6 py-5">
        <div className="flex flex-wrap items-center gap-2">
          <div role="tablist" aria-label={t("Time range")} className="flex rounded-lg border border-line-soft p-0.5">
            {RANGES.map((r) => (
              <button
                key={r.days}
                type="button"
                role="tab"
                aria-selected={range === r.days}
                onClick={() => setRange(r.days)}
                className={cx("h-6 rounded-md px-2.5 text-[12px]", range === r.days ? "bg-elevated text-fg" : "text-fg-3 hover:text-fg")}
              >
                {t(r.label)}
              </button>
            ))}
          </div>
          {!demo && (
            <label className="flex items-center gap-1.5 text-[12.5px] text-fg-3">
              <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} className="accent-[var(--violet)]" />
              {t("All projects")}
            </label>
          )}
          <button
            type="button"
            onClick={copyCsv}
            disabled={shown.length === 0}
            title={t("A timesheet with one line per day, project, branch and session, in hours")}
            className="ms-auto flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12px] text-fg-2 hover:bg-elevated hover:text-fg disabled:opacity-50"
          >
            {copied ? <Check size={13} /> : <Clipboard size={13} />}
            {copied ? t("Copied") : t("Copy as CSV")}
          </button>
        </div>

        {error && <p className="text-[12.5px] text-error">{error}</p>}

        <div className="flex flex-wrap gap-3">
          <Tile label={t("Agents working")} value={totals.agentMs} color="var(--chart-agent)" />
          <Tile label={t("Waiting for you")} value={totals.waitingMs} color="var(--chart-waiting)" note={total(totals) ? t("{share} of the agents' time", { share: new Intl.NumberFormat(undefined, { style: "percent" }).format(totals.waitingMs / Math.max(1, totals.agentMs + totals.waitingMs)) }) : undefined} />
          <Tile label={t("Your time")} value={totals.youMs} color="var(--chart-you)" note={t("Estimated from your messages and approvals")} />
        </div>

        {range > 1 && (
          <section aria-label={t("Time per day")} className="rounded-xl border border-line-soft p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <h3 className="text-[13px] font-semibold">{t("Per day")}</h3>
              <Legend />
            </div>
            <DayChart days={perDay(shown, days)} />
          </section>
        )}

        <section aria-label={t("Time per branch")}>
          <h3 className="mb-2 text-[13px] font-semibold">{t("Per branch")}</h3>
          {rows === null ? (
            <p className="text-[12.5px] text-fg-3">{t("Reading the time…")}</p>
          ) : branches.length === 0 ? (
            <div className="rounded-xl border border-dashed border-line px-6 py-8 text-center">
              <Clock size={18} className="mx-auto text-fg-3" />
              <p className="mt-2 text-[13px] text-fg-2">{t("Nothing tracked in this range yet.")}</p>
              <p className="mt-1 text-[12px] text-fg-3">{t("Time is counted while an agent works or waits for you, and when you write to it.")}</p>
            </div>
          ) : (
            <div className="overflow-x-auto rounded-xl border border-line-soft">
              <table className="w-full text-[12.5px]">
                <thead>
                  <tr className="border-b border-line-soft text-[11.5px] text-fg-3">
                    <th className="px-3 py-2 text-start font-medium">{t("Branch")}</th>
                    {SERIES.map((s) => (
                      <th key={s.key} className="px-3 py-2 text-end font-medium">
                        {t(s.label)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {branches.map((b) => {
                    const isOpen = open === b.key;
                    const bySession = groupBy(b.rows, (r) => r.sessionId);
                    return [
                      <tr key={b.key} className="border-b border-line-soft last:border-0 hover:bg-surface">
                        <td className="px-3 py-1.5">
                          <button type="button" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : b.key)} className="flex min-w-0 items-center gap-1.5 text-start">
                            <ChevronRight size={12} className={cx("shrink-0 text-fg-3 transition-transform rtl:-scale-x-100", isOpen && "rotate-90 rtl:-rotate-90")} />
                            <span dir="ltr" className="truncate font-mono text-fg">
                              {b.key || t("No branch")}
                            </span>
                            <span className="shrink-0 text-[11px] text-fg-3">{bySession.length > 1 ? tn(bySession.length, "{n} session", "{n} sessions") : ""}</span>
                          </button>
                        </td>
                        {cells(b.totals)}
                      </tr>,
                      ...(isOpen
                        ? bySession.map((s) => (
                            <tr key={b.key + s.key} className="border-b border-line-soft bg-surface/50">
                              <td className="py-1 ps-8 pe-3">
                                <button type="button" onClick={() => sessions.some((x) => x.id === s.key) && select(s.key)} className="block max-w-[340px] truncate text-start text-fg-2 hover:text-violet-text hover:underline">
                                  {sessionName(s.key)}
                                  {all && <span className="text-fg-3"> · {projectName(s.rows[0]!.projectId)}</span>}
                                </button>
                              </td>
                              {cells(s.totals)}
                            </tr>
                          ))
                        : []),
                    ];
                  })}
                </tbody>
              </table>
            </div>
          )}
        </section>

        {team.length > 0 && (
          <section aria-label={t("Teammates' time")}>
            <h3 className="mb-2 text-[13px] font-semibold">{t("Teammates")}</h3>
            <div className="overflow-x-auto rounded-xl border border-line-soft">
              <table className="w-full text-[12.5px]">
                <thead>
                  <tr className="border-b border-line-soft text-[11.5px] text-fg-3">
                    <th className="px-3 py-2 text-start font-medium">{t("Person and branch")}</th>
                    {SERIES.map((s) => (
                      <th key={s.key} className="px-3 py-2 text-end font-medium">
                        {t(s.label)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {team.flatMap(({ p, rows: rs }) => [
                    <tr key={p.email} className="border-b border-line-soft">
                      <td className="px-3 pt-2 pb-1 font-medium text-fg">{p.name}</td>
                      {cells(sum(rs.map((r) => r.totals)))}
                    </tr>,
                    ...rs.map((r) => (
                      <tr key={p.email + r.key} className="border-b border-line-soft last:border-0">
                        <td dir="ltr" className="py-1 ps-6 pe-3 text-start font-mono text-fg-2">
                          {r.key}
                        </td>
                        {cells(r.totals)}
                      </tr>
                    )),
                  ])}
                </tbody>
              </table>
            </div>
            <p className="mt-1.5 text-[11.5px] text-fg-3">{t("From teammates who turned on “Share my work”, as of their last update.")}</p>
          </section>
        )}

        <p className="text-[11.5px] leading-[1.6] text-fg-3">
          {t("Agent and waiting time are measured from when each session starts working, stops, or waits for an approval or an answer. Your time is an estimate: the time between your messages, approvals and answers counts when they're less than 15 minutes apart, otherwise 2 minutes each. Bots' tasks count as agent time only. Days are your local days.")}
        </p>
      </div>
    </div>
  );
}
