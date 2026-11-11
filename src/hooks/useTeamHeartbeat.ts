import { useEffect } from "react";
import { teamApi } from "../lib/team";
import { useStore } from "../state/store";

/** How often your presence is shared again while AgentDeck runs (teammates see you as online). */
const EVERY_MS = 4 * 60_000;
/** After a session starts, stops or waits for you, share that soon, but not on every change. */
const SETTLE_MS = 15_000;

/**
 * Keeps your presence fresh in every project where you turned on "Share my work". The desktop
 * app does nothing for projects you don't share, so this is cheap when sharing is off.
 */
export function useTeamHeartbeat() {
  useEffect(() => {
    if (!teamApi.available) return;
    const beat = () => void teamApi.heartbeat().catch(() => {});
    const first = window.setTimeout(beat, 20_000);
    const id = window.setInterval(beat, EVERY_MS);
    let timer: number | undefined;
    const sig = (s: ReturnType<typeof useStore.getState>) =>
      s.sessions
        .filter((x) => x.live)
        .map((x) => `${x.id}:${x.status}:${x.worktree ?? ""}`)
        .join("|");
    let last = sig(useStore.getState());
    const unsub = useStore.subscribe((s) => {
      const now = sig(s);
      if (now === last) return;
      last = now;
      window.clearTimeout(timer);
      timer = window.setTimeout(beat, SETTLE_MS);
    });
    return () => {
      window.clearTimeout(first);
      window.clearInterval(id);
      window.clearTimeout(timer);
      unsub();
    };
  }, []);
}
