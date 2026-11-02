import { useEffect, useState } from "react";
import { isTauri } from "./native";

export interface SlashCommand {
  name: string;
  description: string | null;
  hint: string | null;
  /** "agent": the agent's own; "custom": your command files; "skill"; "agentdeck": done by AgentDeck. */
  source: "agent" | "custom" | "skill" | "agentdeck";
  /** Text that goes into the message instead of "/name" (Codex prompts). */
  expand: string | null;
}

/** Commands AgentDeck carries out itself, for every agent. */
export const APP_COMMANDS: SlashCommand[] = [
  { name: "plan", description: "Turn plan mode on or off: the agent reads and plans, nothing changes until you approve", hint: null, source: "agentdeck", expand: null },
  { name: "terminal", description: "Switch to the terminal view of this session", hint: null, source: "agentdeck", expand: null },
  { name: "rename", description: "Rename this session", hint: null, source: "agentdeck", expand: null },
  { name: "new", description: "New session with the same agent in this project", hint: null, source: "agentdeck", expand: null },
  { name: "workflow", description: "Plan, build and review with other agents, or let two agents talk", hint: null, source: "agentdeck", expand: null },
  { name: "git", description: "Open the Git view for this project", hint: null, source: "agentdeck", expand: null },
  { name: "setup", description: "Open Agent setup: instructions, skills, commands", hint: null, source: "agentdeck", expand: null },
];

const cache = new Map<string, SlashCommand[]>();

/** The agent's own commands for a session (asked once per session, kept for the app's life). */
export function useAgentCommands(sessionId: string, enabled: boolean): { commands: SlashCommand[]; loading: boolean } {
  const [commands, setCommands] = useState<SlashCommand[]>(() => cache.get(sessionId) ?? []);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!enabled || !isTauri) return;
    const known = cache.get(sessionId);
    if (known) {
      setCommands(known);
      return;
    }
    let gone = false;
    setLoading(true);
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<SlashCommand[]>("list_commands", { sessionId }))
      .then((list) => {
        cache.set(sessionId, list);
        if (!gone) setCommands(list);
      })
      .catch(() => undefined)
      .finally(() => !gone && setLoading(false));
    return () => {
      gone = true;
    };
  }, [sessionId, enabled]);
  return { commands, loading };
}

/** Forget a session's list (after the agent restarts, its commands may have changed). */
export function forgetCommands(sessionId: string) {
  cache.delete(sessionId);
}

/** Commands matching what's typed after "/": name prefix first, then anywhere in the name. */
export function matchCommands(all: SlashCommand[], query: string): SlashCommand[] {
  const q = query.toLowerCase();
  const seen = new Set<string>();
  const unique = all.filter((c) => (seen.has(c.name) ? false : (seen.add(c.name), true)));
  const starts = unique.filter((c) => c.name.toLowerCase().startsWith(q));
  const contains = unique.filter((c) => !c.name.toLowerCase().startsWith(q) && c.name.toLowerCase().includes(q));
  return [...starts, ...contains].slice(0, 60);
}
