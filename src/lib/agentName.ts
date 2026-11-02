import { providerLabel } from "../data/demo";
import type { ProviderId } from "./events";
import { useStore } from "../state/store";

/** How to name a session's agent: its model profile when it has one ("DeepSeek"), else the CLI. */
export function agentLabel(s: { provider: ProviderId; profileId?: string | null }): string {
  const p = s.profileId ? useStore.getState().profiles.find((x) => x.id === s.profileId) : undefined;
  if (!p) return providerLabel[s.provider];
  return s.provider === "claude-code" ? `${p.name} (Claude Code)` : p.name;
}
