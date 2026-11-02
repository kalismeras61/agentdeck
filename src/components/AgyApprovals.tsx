import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ShieldAlert, ShieldCheck, ShieldHalf } from "lucide-react";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { errorText, liveApi, type AgyApprovals } from "../lib/live";
import { cx, Tip } from "./ui";

export const LEVELS: { id: AgyApprovals; label: string; note: string }[] = [
  { id: "edits", label: "File edits", note: "Edits files in the project without asking; anything else that needs your OK is skipped and listed." },
  { id: "auto", label: "Auto", note: "Antigravity's own Auto mode: edits and commands it judges safe go ahead; a second agent reviews commands that leave the sandbox; irreversible actions are skipped." },
  { id: "all", label: "Everything", note: "Skips every approval (--dangerously-skip-permissions): commands, deletions and pushes all run without asking. Only for projects you can afford to lose work in." },
];

/** The approval level, shared by every place that shows it. */
export const useAgyApprovals = create<{ level: AgyApprovals | null; load: () => Promise<void>; set: (l: AgyApprovals, sessionId?: string) => Promise<void> }>((set) => ({
  level: null,
  load: async () => {
    if (!liveApi.available) return;
    set({ level: await liveApi.antigravityApprovals() });
  },
  /** With a session: that session carries on under the new level right away. */
  set: async (l, sessionId) => {
    set({ level: await liveApi.setAntigravityApprovals(l, sessionId) });
  },
}));

const icon = (l: AgyApprovals | null, size = 13) =>
  l === "all" ? <ShieldAlert size={size} className="text-error" /> : l === "auto" ? <ShieldHalf size={size} className="text-violet-text" /> : <ShieldCheck size={size} className="text-fg-3" />;

/** Composer chip for Antigravity sessions: how much it may do without asking. */
export function AgyApprovalsMenu({ side = "top", sessionId }: { side?: "top" | "bottom"; sessionId?: string }) {
  const { level, load, set } = useAgyApprovals();
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (level === null) void load().catch(() => {});
  }, [level, load]);
  const pick = (l: AgyApprovals) => {
    setError(null);
    if (l === level) return;
    void set(l, sessionId).catch((e: unknown) => setError(errorText(e)));
  };
  const current = LEVELS.find((x) => x.id === level);
  return (
    <Dropdown.Root onOpenChange={(o) => !o && setConfirm(false)}>
      <Tip label="What Antigravity may do without asking (it can't ask in AgentDeck)" side={side}>
        <Dropdown.Trigger
          className={cx(
            "inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-xs hover:bg-elevated data-[state=open]:bg-elevated",
            level === "all" ? "text-error" : "text-fg-2 hover:text-fg",
          )}
        >
          {icon(level)}
          {current ? current.label : "Approvals"}
          <ChevronDown size={13} />
        </Dropdown.Trigger>
      </Tip>
      <Dropdown.Portal>
        <Dropdown.Content side={side} align="start" sideOffset={6} className="z-50 w-[320px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
          <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] leading-[1.45] text-fg-3">
            Antigravity may, without asking. Applies right away: a turn in progress picks up again, and skipped steps are redone.
          </Dropdown.Label>
          {LEVELS.map((l) => (
            <Dropdown.Item
              key={l.id}
              onSelect={(e) => {
                if (l.id === "all" && level !== "all" && !confirm) {
                  e.preventDefault();
                  setConfirm(true);
                  return;
                }
                pick(l.id);
              }}
              className="flex items-start gap-2 rounded-lg px-2 py-1.5 outline-none data-[highlighted]:bg-surface"
            >
              <span className="mt-0.5">{icon(l.id)}</span>
              <span className="min-w-0 flex-1">
                <span className={cx("block text-[13px]", l.id === "all" ? "text-error" : "text-fg")}>{l.label}</span>
                <span className="block text-[11.5px] leading-[1.45] text-fg-3">{l.note}</span>
                {l.id === "all" && confirm && level !== "all" && (
                  <span className="mt-1 block text-[11.5px] font-medium text-error">Click again to turn it on for every Antigravity session.</span>
                )}
              </span>
              {l.id === level && <Check size={14} className="mt-0.5 shrink-0 text-violet-text" />}
            </Dropdown.Item>
          ))}
          <p className="px-2 pb-1 pt-1.5 text-[11px] leading-[1.5] text-fg-3">Applies to every Antigravity session; running ones restart and continue their conversation.</p>
          {error && <p className="px-2 pb-1 text-[11.5px] text-error">{error}</p>}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}
