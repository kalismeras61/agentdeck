import { providerLabel } from "../data/demo";
import type { ProviderId } from "./events";
import type { FlowRow, ModelProfile, Workflow } from "./live";
import { useStore } from "../state/store";
import { useMemo } from "react";
import { t } from "./i18n";

export type StageId = "plan" | "build" | "review";

/** English labels and hints, shown with t(); `{word}` in a hint is the protocol word APPROVED. */
export const STAGES: { id: StageId; label: string; verb: string; hint: string }[] = /* i18n */ [
  { id: "plan", label: "Plan", verb: "plans", hint: "Reads the code and writes the plan. Can't change files." },
  { id: "build", label: "Build", verb: "builds", hint: "Makes the changes. Risky steps still ask you." },
  {
    id: "review",
    label: "Review",
    verb: "reviews",
    hint: "Checks the work and sends notes back until it writes {word}. Can't change files.",
  },
];

export const CLI_AGENTS = ["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code", "antigravity"] as const;

/** Ready-made workflows; saved ones come from the desktop app. Names are shown with t(). */
export const BUILT_IN: Workflow[] = /* i18n */ [
  {
    id: "builtin:pbr",
    name: "Plan, build, review",
    planAgent: "claude-code",
    buildAgent: "codex",
    reviewAgent: "claude-code",
    gatePlan: false,
    maxReviews: 3,
    createdAt: 0,
  },
  { id: "builtin:pb", name: "Plan, then build", planAgent: "claude-code", buildAgent: "codex", reviewAgent: "", gatePlan: false, maxReviews: 3, createdAt: 0 },
  {
    id: "builtin:br",
    name: "Build, then review",
    planAgent: "",
    buildAgent: "claude-code",
    reviewAgent: "codex",
    gatePlan: false,
    maxReviews: 3,
    createdAt: 0,
  },
];

/** The agent value for a session: its model profile, or its CLI. */
export function agentKeyOf(s: { provider: ProviderId; profileId?: string | null }): string {
  return s.profileId ? `profile:${s.profileId}` : s.provider;
}

export function agentInfo(key: string, profiles: ModelProfile[]): { name: string; provider: ProviderId } | null {
  if (!key) return null;
  if (key.startsWith("profile:")) {
    const p = profiles.find((m) => `profile:${m.id}` === key);
    return p ? { name: p.name, provider: p.kind === "anthropic" ? "claude-code" : "api" } : { name: t("A deleted model"), provider: "api" };
  }
  const provider = key as ProviderId;
  return { name: providerLabel[provider] ?? key, provider };
}

/** "Claude Code → Codex → Claude Code" for a saved workflow. */
export function lineup(w: Pick<Workflow, "planAgent" | "buildAgent" | "reviewAgent">, profiles: ModelProfile[]): string {
  return [w.planAgent, w.buildAgent, w.reviewAgent]
    .map((k) => agentInfo(k, profiles)?.name)
    .filter(Boolean)
    .join(" → ");
}

/** The newest workflow run this session takes part in. */
export function useFlowFor(sessionId: string): FlowRow | undefined {
  const flows = useStore((s) => s.flows);
  return useMemo(() => {
    let best: FlowRow | undefined;
    for (const f of Object.values(flows)) {
      if (f.planSession !== sessionId && f.buildSession !== sessionId && f.reviewSession !== sessionId) continue;
      if (!best || f.createdAt > best.createdAt) best = f;
    }
    return best;
  }, [flows, sessionId]);
}

export type Slots = Pick<Workflow, "planAgent" | "buildAgent" | "reviewAgent" | "gatePlan" | "maxReviews">;
