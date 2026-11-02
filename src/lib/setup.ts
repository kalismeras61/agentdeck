import { isTauri } from "./native";

export interface AgentSetupState {
  id: "claude-code" | "codex" | "gemini-cli" | "grok-build" | "kimi-code" | "antigravity";
  name: string;
  maker: string;
  installed: boolean;
  version: string | null;
  path: string | null;
  error: string | null;
  install: string | null;
  needs: string | null;
  signIn: string | null;
  signedIn: boolean | null;
  account: string;
  docs: string;
}

export interface SetupStatus {
  agents: AgentSetupState[];
  antigravityEnabled: boolean;
  welcomeDone: boolean;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const setupApi = {
  available: isTauri,
  status: () => call<SetupStatus>("setup_status"),
  /** Runs the agent's official install or sign-in command in macOS Terminal; returns the command. */
  openTerminal: (agent: string, action: "install" | "sign_in") => call<string>("open_setup_terminal", { agent, action }),
  finishWelcome: () => call<void>("finish_welcome"),
  setAntigravity: (enabled: boolean) => call<boolean>("set_antigravity_enabled", { enabled }),
};

/** Leaves Antigravity out of agent pickers unless it's turned on. */
export function withoutHidden<T extends string>(list: readonly T[], agyEnabled: boolean): T[] {
  return list.filter((p) => p !== "antigravity" || agyEnabled);
}
