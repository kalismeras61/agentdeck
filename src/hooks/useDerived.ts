import { useMemo } from "react";
import type { Session } from "../data/demo";
import { diffStats, type EventOf, type FileChange } from "../lib/events";
import { useWorkspaceSessions } from "./useWorkspace";
import { getParsed } from "./useSession";

export interface PendingPermission {
  session: Session;
  event: EventOf<"permission">;
}

/** Every permission request across sessions, oldest first. */
export function usePermissions() {
  const sessions = useWorkspaceSessions();
  return useMemo(() => {
    const pending: PendingPermission[] = [];
    const decided: PendingPermission[] = [];
    for (const session of sessions) {
      for (const p of getParsed(session)) {
        if (!p.ok || p.event.type !== "permission") continue;
        (p.event.payload.state === "pending" ? pending : decided).push({ session, event: p.event });
      }
    }
    pending.sort((a, b) => a.event.ts - b.event.ts);
    decided.sort((a, b) => (b.event.payload.decidedAt ?? b.event.ts) - (a.event.payload.decidedAt ?? a.event.ts));
    return { pending, decided };
  }, [sessions]);
}

export interface SessionChanges {
  session: Session;
  files: (FileChange & { additions: number; deletions: number })[];
}

/** Latest version of each changed file per session. */
export function sessionChanges(session: Session): SessionChanges["files"] {
  const map = new Map<string, FileChange>();
  for (const p of getParsed(session)) if (p.ok && p.event.type === "changeset") for (const f of p.event.payload.files) map.set(f.path, f);
  return [...map.values()].map((f) => ({ ...f, ...diffStats(f.diff) }));
}

export function useAllChanges(): SessionChanges[] {
  const sessions = useWorkspaceSessions();
  return useMemo(() => sessions.map((session) => ({ session, files: sessionChanges(session) })).filter((x) => x.files.length > 0), [sessions]);
}
