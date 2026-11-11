import { useEffect } from "react";
import { t } from "../lib/i18n";
import { notifyText } from "../lib/notify";
import { teamApi } from "../lib/team";
import { feedLine, type FeedEvent } from "../lib/teamchat";
import { useStore } from "../state/store";

/** How often followed projects are fetched and your presence shared again. */
const EVERY_MS = 2 * 60_000;

/** Tells you about what matters in the team (a message to you, a branch touching yours…). */
function announce(events: FeedEvent[]) {
  if (events.length) useStore.setState((s) => ({ teamTick: s.teamTick + 1 }));
  const important = events.filter((e) => e.notify);
  const msgs = important.filter((e) => e.kind === "message" || e.kind === "review").length;
  if (msgs) useStore.setState((s) => ({ teamUnread: s.teamUnread + msgs }));
  for (const e of important.slice(0, 3)) void notifyText(t("Team"), e.text && (e.kind === "message" || e.kind === "review") ? `${feedLine(e)}: ${e.text}` : feedLine(e));
  if (important.length > 3) void notifyText(t("Team"), t("{n} more updates in the Team view", { n: important.length - 3 }));
}
/** After a session starts, stops or waits for you, share that soon, but not on every change. */
const SETTLE_MS = 15_000;

/**
 * Fetches the projects you follow (opened in the Team view) and keeps your presence fresh where
 * you share your work; announces what matters. Nothing happens for projects you never opened there.
 */
export function useTeamHeartbeat() {
  useEffect(() => {
    if (!teamApi.available) return;
    const beat = () =>
      void teamApi
        .heartbeat()
        .then(announce)
        .catch(() => {});
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
    // Relays answer the moment something arrives: keep one long poll open.
    let stopped = false;
    const poll = async () => {
      while (!stopped) {
        try {
          const r = await teamApi.wait();
          if (stopped) return;
          announce(r.events);
          if (!r.relay) await new Promise((res) => setTimeout(res, 60_000));
        } catch {
          await new Promise((res) => setTimeout(res, 15_000));
        }
      }
    };
    void poll();
    return () => {
      stopped = true;
      window.clearTimeout(first);
      window.clearInterval(id);
      window.clearTimeout(timer);
      unsub();
    };
  }, []);
}
