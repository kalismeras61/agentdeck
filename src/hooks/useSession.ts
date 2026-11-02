import { useMemo } from "react";
import { parseEvents, type ParsedEvent } from "../lib/events";
import { useStore } from "../state/store";
import type { Session } from "../data/demo";

const parsedCache = new WeakMap<unknown[], ParsedEvent[]>();

export function getParsed(session: Session): ParsedEvent[] {
  let p = parsedCache.get(session.raw);
  if (!p) {
    p = parseEvents(session.raw);
    parsedCache.set(session.raw, p);
  }
  return p;
}

export function useSelectedSession(): Session | undefined {
  const id = useStore((s) => s.selectedId);
  const sessions = useStore((s) => s.sessions);
  return useMemo(() => sessions.find((s) => s.id === id), [sessions, id]);
}

export function useParsedEvents(session: Session | undefined): ParsedEvent[] {
  return useMemo(() => (session ? getParsed(session) : []), [session]);
}
