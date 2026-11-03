import { useEffect, useState } from "react";
import { sampleLimits, type Session } from "../data/demo";
import { t, useLocale } from "../lib/i18n";
import { isTauri } from "../lib/native";
import { limitLevel, liveWindows, useLimits, windowDetail, windowLabel, windowSummary, type PlanLimits } from "../lib/limits";
import { cx, Tip } from "./ui";

/** Minute ticks, so reset times and finished windows stay current. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(id);
  }, []);
  return now;
}

/** The plan's 5-hour and weekly usage for the session's agent, under the message box. */
export function PlanLimitsLine({ session }: { session: Session }) {
  useLocale();
  const now = useNow();
  const own = session.provider === "claude-code" || session.provider === "codex";
  const live = useLimits((s) => s.byProvider[session.provider]);
  // Sessions on your own model profile run against another endpoint: no plan limits.
  // The browser demo shows made-up numbers on its sample sessions.
  const limits: PlanLimits | undefined = !own || session.profileId ? undefined : session.live ? live : isTauri ? undefined : sampleLimits(session.provider, now);
  const windows = liveWindows(limits, now);
  if (!limits || windows.length === 0) return null;
  return (
    <div role="group" aria-label={t("Plan usage limits")} className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 px-3 text-[11px] text-fg-3">
      {windows.map((w) => {
        const level = limitLevel(w);
        const pct = Math.min(100, Math.max(0, w.usedPercent));
        return (
          <Tip key={w.kind} label={windowDetail(w, limits.updatedAt, now)} side="top">
            <span tabIndex={0} className={cx("inline-flex items-center gap-1.5 rounded outline-none focus-visible:text-fg-2", level === "warning" && "text-warning", level === "error" && "text-error")}>
              <span className="font-medium">{windowLabel(w)}</span>
              <span aria-hidden className="h-1 w-8 overflow-hidden rounded-full bg-line">
                <span
                  className={cx("block h-full rounded-full", level === "ok" ? "bg-fg-3" : level === "warning" ? "bg-warning" : "bg-error")}
                  style={{ width: `${pct}%` }}
                />
              </span>
              <span className="tabular-nums">{windowSummary(w, now)}</span>
            </span>
          </Tip>
        );
      })}
    </div>
  );
}
