import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ShieldAlert, ShieldCheck, ShieldHalf, ShieldQuestion } from "lucide-react";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { providerLabel } from "../data/demo";
import type { ProviderId } from "../lib/events";
import { t } from "../lib/i18n";
import { errorText, liveApi, type AgentApprovals } from "../lib/live";
import { cx, Tip } from "./ui";

/** The agents with this setting (Antigravity has its own: it can't ask). */
export const APPROVAL_AGENTS: ProviderId[] = ["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code", "api"];

export const hasApprovals = (p: ProviderId) => APPROVAL_AGENTS.includes(p);

export const APPROVAL_LEVELS: { id: AgentApprovals; label: string; note: string }[] = /* i18n */ [
  { id: "ask", label: "Ask every time", note: "Every edit and command waits for your OK." },
  { id: "edits", label: "File edits", note: "Edits inside the project go ahead; commands and everything else wait for you." },
  {
    id: "auto",
    label: "Auto",
    note: "Edits and ordinary commands go ahead. Risky ones (deleting, pushing, deploying, secrets, sudo…) and anything outside the project wait for you.",
  },
  { id: "all", label: "Everything", note: "Nothing waits: deletions, pushes and deploys run without asking. Only for projects you can afford to lose work in." },
];

/** Every agent's level, shared by every place that shows it. */
export const useApprovals = create<{
  levels: Record<string, AgentApprovals> | null;
  load: () => Promise<void>;
  set: (provider: ProviderId, level: AgentApprovals) => Promise<number>;
}>((set, get) => ({
  levels: null,
  load: async () => {
    if (!liveApi.available) return;
    set({ levels: await liveApi.agentApprovals() });
  },
  set: async (provider, level) => {
    const answered = await liveApi.setAgentApprovals(provider, level);
    set({ levels: { ...(get().levels ?? {}), [provider]: level } });
    return answered;
  },
}));

const icon = (l: AgentApprovals | undefined, size = 13) =>
  l === "all" ? (
    <ShieldAlert size={size} className="text-error" />
  ) : l === "auto" ? (
    <ShieldHalf size={size} className="text-violet-text" />
  ) : l === "edits" ? (
    <ShieldCheck size={size} className="text-success" />
  ) : (
    <ShieldQuestion size={size} className="text-fg-3" />
  );

/** How much an agent may do without asking: a chip in the composer and in Settings. */
export function ApprovalsMenu({ provider, side = "top" }: { provider: ProviderId; side?: "top" | "bottom" }) {
  const { levels, load, set } = useApprovals();
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (levels === null) void load().catch(() => {});
  }, [levels, load]);
  const level = levels?.[provider] ?? "ask";
  const agent = providerLabel[provider];
  const pick = (l: AgentApprovals) => {
    setError(null);
    if (l === level) return;
    set(provider, l).catch((e: unknown) => setError(errorText(e)));
  };
  const current = APPROVAL_LEVELS.find((x) => x.id === level);
  return (
    <Dropdown.Root
      onOpenChange={(o) => {
        if (!o) setConfirm(false);
      }}
    >
      <Tip label={t("What {agent} may do without asking you", { agent })} side={side}>
        <Dropdown.Trigger
          className={cx(
            "inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-xs hover:bg-elevated data-[state=open]:bg-elevated",
            level === "all" ? "text-error" : "text-fg-2 hover:text-fg",
          )}
        >
          {icon(level)}
          {current ? t(current.label) : t("Approvals")}
          <ChevronDown size={13} />
        </Dropdown.Trigger>
      </Tip>
      <Dropdown.Portal>
        <Dropdown.Content side={side} align="start" sideOffset={6} className="z-50 w-[330px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
          <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] leading-[1.45] text-fg-3">
            {t("What {agent} may do without asking you. Questions and plans always wait for you.", { agent })}
          </Dropdown.Label>
          {APPROVAL_LEVELS.map((l) => (
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
                <span className={cx("block text-[13px]", l.id === "all" ? "text-error" : "text-fg")}>{t(l.label)}</span>
                <span className="block text-[11.5px] leading-[1.45] text-fg-3">{t(l.note)}</span>
                {l.id === "all" && confirm && level !== "all" && (
                  <span className="mt-1 block text-[11.5px] font-medium text-error">{t("Click again to turn it on for every {agent} session.", { agent })}</span>
                )}
              </span>
              {l.id === level && <Check size={14} className="mt-0.5 shrink-0 text-violet-text" />}
            </Dropdown.Item>
          ))}
          <p className="px-2 pb-1 pt-1.5 text-[11px] leading-[1.5] text-fg-3">
            {t("Applies right away to every {agent} session you run, including requests already waiting. Bots and workflows keep their own rules.", { agent })}
          </p>
          {error && <p className="px-2 pb-1 text-[11.5px] text-error">{error}</p>}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}
