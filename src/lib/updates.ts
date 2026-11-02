import { create } from "zustand";
import { errorText } from "./live";
import { isTauri } from "./native";

export interface UpdateInfo {
  current: string;
  /** Whether this build can update itself (released builds can; local builds can't). */
  enabled: boolean;
  available: string | null;
  notes: string | null;
  date: string | null;
}

type Phase = "idle" | "checking" | "downloading" | "installing" | "error";

interface UpdateState {
  info: UpdateInfo | null;
  phase: Phase;
  error: string | null;
  checkedAt: number | null;
  progress: { downloaded: number; total: number | null } | null;
  dismissed: string | null;
  check: (quiet?: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
}

async function call<T>(cmd: string): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd);
}

export const useUpdates = create<UpdateState>((set, get) => ({
  info: null,
  phase: "idle",
  error: null,
  checkedAt: null,
  progress: null,
  dismissed: null,
  check: async (quiet = false) => {
    if (!isTauri || get().phase === "checking" || get().phase === "downloading") return;
    set({ phase: "checking", error: null });
    try {
      const info = await call<UpdateInfo>("check_update");
      set({ info, phase: "idle", checkedAt: Date.now() });
    } catch (e: unknown) {
      // A quiet check (at startup) never bothers you with network trouble.
      set({ phase: quiet ? "idle" : "error", error: quiet ? null : errorText(e), checkedAt: Date.now() });
    }
  },
  install: async () => {
    if (!isTauri) return;
    set({ phase: "downloading", error: null, progress: { downloaded: 0, total: null } });
    const { listen } = await import("@tauri-apps/api/event");
    const stop = await listen<{ downloaded: number; total: number | null; done: boolean }>("update-progress", (e) => {
      if (e.payload.done) set({ phase: "installing" });
      else set({ progress: { downloaded: e.payload.downloaded, total: e.payload.total } });
    });
    try {
      await call<void>("install_update"); // restarts the app when it works
    } catch (e: unknown) {
      set({ phase: "error", error: errorText(e) });
    } finally {
      stop();
    }
  },
  dismiss: () => set({ dismissed: get().info?.available ?? null }),
}));

/** Checks quietly once at startup, then every six hours while AgentDeck is open. */
export function startUpdateChecks(): () => void {
  if (!isTauri) return () => undefined;
  const run = () => void useUpdates.getState().check(true);
  const first = window.setTimeout(run, 8000);
  const every = window.setInterval(run, 6 * 60 * 60 * 1000);
  return () => {
    window.clearTimeout(first);
    window.clearInterval(every);
  };
}
