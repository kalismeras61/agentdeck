import { AlertTriangle, GitBranch, GitMerge, GitPullRequestArrow, MessageSquare, Radio, Trash2, Upload } from "lucide-react";
import { cx } from "../../components/ui";
import { ago } from "../../lib/format";
import { t } from "../../lib/i18n";
import { feedLine, type FeedEvent } from "../../lib/teamchat";

const ICON = {
  branch_new: GitBranch,
  push: Upload,
  branch_gone: Trash2,
  merged: GitMerge,
  base: Radio,
  overlap: AlertTriangle,
  message: MessageSquare,
  review: GitPullRequestArrow,
} as const;

function dayLabel(ts: number): string {
  const d = new Date(ts);
  const today = new Date();
  const y = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return t("Today");
  if (d.toDateString() === y.toDateString()) return t("Yesterday");
  return d.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
}

/** What happened in the team: branches, pushes, merges, overlaps with yours, messages to you. */
export function ActivityTab({ feed, onBranch, onThread }: { feed: FeedEvent[] | null; onBranch: (short: string) => void; onThread: (thread: string) => void }) {
  if (feed === null) return <p className="m-auto text-[13px] text-fg-3">{t("Reading the activity…")}</p>;
  if (feed.length === 0)
    return (
      <div className="m-auto max-w-[46ch] px-8 text-center">
        <Radio size={20} className="mx-auto text-fg-3" />
        <p className="mt-3 text-[13.5px] text-fg">{t("Nothing new yet")}</p>
        <p className="mt-1.5 text-[12.5px] leading-[1.6] text-fg-3">
          {t("AgentDeck fetches this project every few minutes. New branches, pushes, merges, branches that start touching your files and messages to you show up here, and the important ones as a notification.")}
        </p>
      </div>
    );
  const groups: { day: string; items: FeedEvent[] }[] = [];
  for (const e of feed) {
    const day = dayLabel(e.ts);
    const g = groups[groups.length - 1];
    if (g && g.day === day) g.items.push(e);
    else groups.push({ day, items: [e] });
  }
  return (
    <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[760px] px-6 py-5">
        {groups.map((g) => (
          <section key={g.day} className="mb-5">
            <h3 className="mb-1.5 text-[12px] font-semibold text-fg-3">{g.day}</h3>
            <ol className="overflow-hidden rounded-xl border border-line-soft">
              {g.items.map((e) => {
                const Icon = ICON[e.kind] ?? Radio;
                const open = () => (e.link ? onThread(e.link) : e.branch && e.kind !== "branch_gone" && e.kind !== "merged" ? onBranch(e.branch) : undefined);
                return (
                  <li key={e.id} className="border-b border-line-soft last:border-0">
                    <button type="button" onClick={open} className="flex w-full items-start gap-3 px-4 py-2.5 text-start hover:bg-surface">
                      <span
                        className={cx(
                          "mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full",
                          e.notify ? "bg-violet-soft text-violet-text" : "bg-elevated text-fg-3",
                          e.kind === "overlap" && "bg-warning/15 text-warning",
                        )}
                      >
                        <Icon size={13} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span dir="auto" className="block text-[13px] text-fg">
                          {feedLine(e)}
                        </span>
                        {(e.kind === "message" || e.kind === "review") && e.text && (
                          <span dir="auto" className="mt-0.5 block truncate text-[12.5px] text-fg-3">
                            “{e.text}”
                          </span>
                        )}
                      </span>
                      <span className="shrink-0 text-[11.5px] tabular-nums text-fg-3">{ago(e.ts)}</span>
                    </button>
                  </li>
                );
              })}
            </ol>
          </section>
        ))}
      </div>
    </div>
  );
}
