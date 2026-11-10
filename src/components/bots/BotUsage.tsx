import { useEffect, useState } from "react";
import { t } from "../../lib/i18n";
import { botExtrasApi, liveApi, type Bot, type BotUsage as Usage } from "../../lib/live";
import { PlanLimitsLine } from "../PlanLimits";
import type { Session } from "../../data/demo";
import { cx } from "../ui";
import { dollars, tokens } from "../../lib/usage";

const RANGES = /* i18n */ [
  ["day", "24 hours"],
  ["week", "7 days"],
  ["month", "30 days"],
] as const;

/** What a bot's tasks used lately, and its agent's plan limits. */
export function BotUsage({ bot }: { bot: Bot }) {
  const [usage, setUsage] = useState<Record<"day" | "week" | "month", Usage> | null>(null);
  const [range, setRange] = useState<"day" | "week" | "month">("week");
  const live = liveApi.available && !bot.id.startsWith("b-demo-");
  // Read again when a task ends (the bot's counts change).
  useEffect(() => {
    if (!live) return;
    botExtrasApi.usage(bot.id).then(setUsage, () => setUsage(null));
  }, [bot.id, bot.runningTaskId, bot.queued, bot.updatedAt, live]);
  if (!live || !usage) return null;
  const u = usage[range];
  const asSession = { id: bot.id, provider: bot.provider, profileId: bot.profileId, live: true } as unknown as Session;
  return (
    <div className="mx-3 mt-3 rounded-lg border border-line-soft px-2.5 py-2">
      <div className="flex items-center gap-2">
        <p className="min-w-0 flex-1 text-[11.5px] font-medium text-fg-2">{t("Usage")}</p>
        <div role="radiogroup" aria-label={t("Usage")} className="flex gap-0.5">
          {RANGES.map(([k, label]) => (
            <button
              key={k}
              type="button"
              role="radio"
              aria-checked={range === k}
              onClick={() => setRange(k)}
              className={cx("h-6 rounded-md px-1.5 text-[11px]", range === k ? "bg-elevated text-fg" : "text-fg-3 hover:text-fg")}
            >
              {t(label)}
            </button>
          ))}
        </div>
      </div>
      <p className="mt-1 text-[12px] tabular-nums text-fg">
        {u.tasks === 0
          ? t("No tasks finished")
          : t("{n} tasks · {done} done · {failed} failed", { n: u.tasks, done: u.done, failed: u.failed })}
      </p>
      {u.measured > 0 ? (
        <p className="mt-0.5 text-[11.5px] tabular-nums text-fg-2">
          {t("{in} in · {out} out · ≈ {cost} at API prices", { in: tokens(u.tokensIn), out: tokens(u.tokensOut), cost: dollars(u.costUsd) })}
        </p>
      ) : (
        u.tasks > 0 && <p className="mt-0.5 text-[11.5px] text-fg-3">{t("This agent doesn't report what a task used.")}</p>
      )}
      <div className="-mx-3">
        <PlanLimitsLine session={asSession} />
      </div>
    </div>
  );
}
