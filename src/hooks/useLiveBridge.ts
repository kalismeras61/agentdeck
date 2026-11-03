import { useEffect } from "react";
import type { StoredEvent } from "../lib/claude";
import { errorText, liveApi, subscribeLive } from "../lib/live";
import { useLimits } from "../lib/limits";
import { notifyStatus } from "../lib/notify";
import { useStore } from "../state/store";

/** Loads projects, sessions and their stored history from the desktop app, then follows live updates. */
/** Sessions whose history loads at startup: the 30 most recent, plus any agent at work. */
export const EAGER_SESSIONS = 30;
export function eagerRows<T extends { updatedAt: number; status: string }>(rows: T[]): T[] {
  const recent = [...rows].sort((a, b) => b.updatedAt - a.updatedAt).slice(0, EAGER_SESSIONS);
  const busy = rows.filter((r) => (r.status === "running" || r.status === "waiting") && !recent.includes(r));
  return [...recent, ...busy];
}

export function useLiveBridge() {
  useEffect(() => {
    if (!liveApi.available) return;
    let off: (() => void) | undefined;
    let offSelect: (() => void) | undefined;
    let cancelled = false;
    const st = useStore.getState();
    // Buffer events that arrive while history is loading so nothing is lost or applied twice.
    const early: StoredEvent[] = [];
    let ready = false;
    (async () => {
      try {
        off = await subscribeLive({
          onEvent: (e) => (ready ? useStore.getState().appendLiveEvent(e) : early.push(e)),
          onSession: (s) => {
            const prev = useStore.getState().sessions.find((x) => x.id === s.id)?.status;
            useStore.getState().upsertLiveSession(s);
            void notifyStatus(prev, s);
          },
          onDelta: (d) => useStore.getState().applyDelta(d.sessionId, d.text, d.reset),
          onRelay: (r) => useStore.getState().upsertRelay(r),
          onFlow: (f) => useStore.getState().upsertFlow(f),
          onBot: (b) => useStore.getState().upsertBot(b),
          onBotTask: (t) => useStore.getState().upsertBotTask(t),
          onBotSchedule: (s) => useStore.getState().upsertBotSchedule(s),
          onLimits: (l) => useLimits.getState().put(l),
        });
        liveApi
          .planLimits()
          .then((ls) => ls.forEach((l) => useLimits.getState().put(l)))
          .catch(() => undefined);
        const [projects, rows] = await Promise.all([liveApi.listProjects(), liveApi.listSessions()]);
        // Only recent and busy sessions load their history now; the rest load when opened.
        const eager = eagerRows(rows);
        const lists = await Promise.all(eager.map((r) => liveApi.loadEvents(r.id)));
        if (cancelled) return;
        const events: Record<string, StoredEvent[]> = {};
        eager.forEach((r, i) => (events[r.id] = lists[i] ?? []));
        // Profiles first, so sessions are named after their model from the first render.
        await useStore.getState().loadProfiles();
        st.hydrateLive(projects, rows, events);
        // Whatever session is open loads its history, however it got selected.
        void useStore.getState().ensureEvents(useStore.getState().selectedId ?? "");
        offSelect = useStore.subscribe((s, prev) => {
          if (s.selectedId && s.selectedId !== prev.selectedId) void s.ensureEvents(s.selectedId);
        });
        liveApi
          .listRelays()
          .then((rs) => rs.forEach((r) => useStore.getState().upsertRelay(r)))
          .catch(() => undefined);
        liveApi
          .listFlows()
          .then((fs) => fs.forEach((f) => useStore.getState().upsertFlow(f)))
          .catch(() => undefined);
        void useStore.getState().loadWorkflows();
        void useStore.getState().loadBots();
        ready = true;
        early.forEach((e) => useStore.getState().appendLiveEvent(e));
      } catch (e) {
        st.setLiveState("error", errorText(e));
      }
    })();
    return () => {
      cancelled = true;
      off?.();
      offSelect?.();
    };
  }, []);
}
