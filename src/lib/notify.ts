import type { LiveSessionRow } from "./live";
import { useStore } from "../state/store";

const LABEL: Record<string, string> = { "claude-code": "Claude Code", codex: "Codex", "gemini-cli": "Gemini CLI", "grok-build": "Grok Build", antigravity: "Antigravity" };

let allowed: boolean | null = null;

async function ensurePermission(): Promise<boolean> {
  if (allowed !== null) return allowed;
  const n = await import("@tauri-apps/plugin-notification");
  allowed = await n.isPermissionGranted();
  if (!allowed) allowed = (await n.requestPermission()) === "granted";
  return allowed;
}

/** What a status change is worth telling someone who isn't looking, if anything. */
export function notificationFor(prev: string | undefined, row: LiveSessionRow, prefs: { approval: boolean; finished: boolean; errors: boolean }) {
  if (!prev || prev === row.status) return null;
  const who = LABEL[row.provider] ?? "The agent";
  if (row.status === "waiting" && prefs.approval) return { title: `${who} needs you`, body: `${row.title}: ${row.snippet}` };
  if (row.status === "error" && prefs.errors) return { title: `${who} hit an error`, body: `${row.title}: ${row.snippet}` };
  if (row.status === "idle" && (prev === "running" || prev === "waiting") && prefs.finished)
    return { title: `${who} finished`, body: `${row.title}: ${row.snippet}` };
  return null;
}

/** Shows a system notification for a session status change while AgentDeck isn't focused. */
export async function notifyStatus(prev: string | undefined, row: LiveSessionRow) {
  if (typeof document !== "undefined" && document.hasFocus()) return;
  const msg = notificationFor(prev, row, useStore.getState().notifications);
  if (!msg) return;
  try {
    if (!(await ensurePermission())) return;
    const n = await import("@tauri-apps/plugin-notification");
    n.sendNotification({ title: msg.title, body: msg.body.slice(0, 180) });
  } catch {
    /* notifications are best effort */
  }
}
