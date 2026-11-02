import { z } from "zod";

/**
 * Normalized event stream shared by every provider adapter.
 * Provider-specific metadata is kept separately (see `raw` on StoredEvent) for debugging,
 * so renderers only ever see validated, provider-neutral payloads.
 */

export const ProviderId = z.enum(["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code", "antigravity", "api"]);
export type ProviderId = z.infer<typeof ProviderId>;

/** Text with a size bound. Longer text (a huge shell command, a long tool target) is cut with
 * an ellipsis rather than dropping the whole event: these fields are only ever shown as text. */
const safeText = (max: number) => z.string().transform((s) => (s.length > max ? `${s.slice(0, Math.max(0, max - 1))}…` : s));

/** Only inline raster/svg data or bundled assets. Never remote URLs or script schemes. */
const SafeImageSrc = z
  .string()
  .max(20_000_000)
  .refine((s) => /^data:image\/(png|jpe?g|webp|gif|svg\+xml)[;,]/i.test(s) || /^\/[\w\-./]+\.(png|jpe?g|webp|svg)$/i.test(s), {
    message: "Image source must be inline data or a bundled asset",
  });

const RelPath = z
  .string()
  .min(1)
  .max(1024)
  .refine((p) => !p.includes("\0") && !p.split("/").includes(".."), { message: "Path must not escape the project" });

const base = {
  eventId: z.string().min(1).max(128),
  sessionId: z.string().min(1).max(128),
  ts: z.number().int().nonnegative(),
  provider: ProviderId,
};

export const ToolItem = z.object({
  id: z.string(),
  kind: z.enum(["read", "search", "edit", "create", "command", "test", "analyze"]),
  label: safeText(500),
  target: safeText(1024).optional(),
  status: z.enum(["ok", "warning", "error", "running"]),
  durationMs: z.number().nonnegative().optional(),
  meta: safeText(200).optional(),
  /** Large logs are accepted; renderers show a bounded tail. */
  output: safeText(5_000_000).optional(),
  json: z.unknown().optional(),
});
export type ToolItem = z.infer<typeof ToolItem>;

export const FileChange = z.object({
  path: RelPath,
  change: z.enum(["modified", "added", "deleted"]),
  diff: safeText(500_000),
});
export type FileChange = z.infer<typeof FileChange>;

export const AgentEvent = z.discriminatedUnion("type", [
  z.object({
    ...base,
    type: z.literal("user_message"),
    payload: z.object({ text: safeText(100_000), attachments: z.array(safeText(1024)).max(20).optional() }),
  }),
  z.object({ ...base, type: z.literal("assistant_text"), payload: z.object({ markdown: safeText(200_000) }) }),
  z.object({
    ...base,
    type: z.literal("tool_group"),
    payload: z.object({ title: safeText(200), items: z.array(ToolItem).max(500) }),
  }),
  z.object({
    ...base,
    type: z.literal("changeset"),
    payload: z.object({ files: z.array(FileChange).min(1).max(500), previewPath: RelPath.optional() }),
  }),
  z.object({
    ...base,
    type: z.literal("test_result"),
    payload: z.object({
      command: safeText(1000),
      passed: z.number().int().nonnegative(),
      failed: z.number().int().nonnegative(),
      skipped: z.number().int().nonnegative(),
      durationMs: z.number().nonnegative(),
      cases: z.array(z.object({ name: safeText(500), status: z.enum(["passed", "failed", "skipped"]), message: safeText(5000).optional() })).max(2000),
    }),
  }),
  z.object({
    ...base,
    type: z.literal("image"),
    payload: z
      .object({
        /** Inline image data, or… */
        src: SafeImageSrc.optional(),
        /** …an image file on this Mac the agent made or opened, loaded when shown. */
        path: z
          .string()
          .max(1024)
          .refine((p) => p.startsWith("/") && /\.(png|jpe?g|gif|webp)$/i.test(p), { message: "Only absolute paths to images" })
          .optional(),
        alt: safeText(500),
        caption: safeText(500).optional(),
        width: z.number().optional(),
        height: z.number().optional(),
      })
      .refine((p) => p.src || p.path, { message: "An image needs data or a file" }),
  }),
  z.object({
    ...base,
    type: z.literal("permission"),
    payload: z.object({
      requestId: z.string().min(1),
      toolUseId: z.string().optional(),
      tool: safeText(100),
      command: safeText(4000),
      cwd: safeText(1024),
      paths: z.array(RelPath).max(200),
      risk: z.enum(["low", "medium", "high"]),
      reason: safeText(2000),
      /** Optional preview of a file edit the agent wants to apply. */
      diff: safeText(500_000).optional(),
      supportsPersistentAllow: z.boolean(),
      /** Button text for the session-scoped allow, from what the provider actually offered. */
      persistentLabel: safeText(80).optional(),
      /** "expired": the agent process ended before anyone answered. */
      state: z.enum(["pending", "allowed", "allowed_session", "denied", "expired"]),
      decidedAt: z.number().int().nonnegative().optional(),
      /** Set when AgentDeck answered by itself, e.g. "Plan mode" refusing a change. */
      autoReason: safeText(200).optional(),
    }),
  }),
  z.object({
    ...base,
    type: z.literal("question"),
    payload: z.object({
      requestId: z.string().max(256),
      questions: z
        .array(
          z.object({
            /** What the answer is keyed by when it goes back to the agent. */
            id: safeText(2000),
            header: safeText(80).optional(),
            question: safeText(2000),
            options: z.array(z.object({ label: safeText(200), description: safeText(1000).optional() })).max(12),
            multiSelect: z.boolean(),
            /** Your own text is accepted besides the options. */
            other: z.boolean(),
            secret: z.boolean(),
          }),
        )
        .min(1)
        .max(10),
      state: z.enum(["pending", "answered", "skipped", "expired"]),
      answers: z.record(z.string(), z.array(safeText(4000))).optional(),
      decidedAt: z.number().int().nonnegative().optional(),
    }),
  }),
  z.object({
    ...base,
    type: z.literal("plan"),
    payload: z.object({
      /** The agent's pending request to leave plan mode, when there is one to answer. */
      requestId: z.string().optional(),
      markdown: safeText(200_000),
      filePath: safeText(1024).optional(),
      /** "workflow": a workflow took the plan for its next stage. */
      state: z.enum(["pending", "approved", "kept", "handed_off", "workflow", "expired"]),
      handedTo: z.object({ provider: ProviderId, sessionId: z.string().max(128) }).optional(),
      decidedAt: z.number().int().nonnegative().optional(),
    }),
  }),
  z.object({
    ...base,
    type: z.literal("relay"),
    payload: z.discriminatedUnion("kind", [
      z.object({
        kind: z.literal("started"),
        relayId: z.string().max(128),
        peerProvider: ProviderId,
        peerSessionId: z.string().max(128),
        task: safeText(100_000),
        maxRounds: z.number().int().positive(),
        /** This session got the task first. */
        starts: z.boolean(),
      }),
      z.object({
        kind: z.literal("message"),
        relayId: z.string().max(128),
        fromProvider: ProviderId,
        fromSessionId: z.string().max(128),
        markdown: safeText(200_000),
        round: z.number().int().positive(),
        maxRounds: z.number().int().positive(),
      }),
      z.object({
        kind: z.literal("ended"),
        relayId: z.string().max(128),
        status: z.enum(["done", "stopped"]),
        reason: safeText(2000),
        rounds: z.number().int().nonnegative(),
      }),
    ]),
  }),
  z.object({
    ...base,
    type: z.literal("flow"),
    payload: z.discriminatedUnion("kind", [
      z.object({
        kind: z.literal("started"),
        flowId: z.string().max(128),
        name: safeText(200),
        task: safeText(100_000),
        /** This session's stage. */
        stage: z.enum(["plan", "build", "review"]),
        stages: z
          .array(z.object({ stage: z.enum(["plan", "build", "review"]), sessionId: z.string().max(128), provider: ProviderId, name: safeText(200) }))
          .max(3),
      }),
      z.object({
        kind: z.literal("input"),
        flowId: z.string().max(128),
        stage: z.enum(["plan", "build", "review"]),
        /** "Task", "Plan from Claude Code", "Review notes from Codex", … */
        label: safeText(300),
        fromProvider: ProviderId.optional(),
        fromSessionId: z.string().max(128).optional(),
        markdown: safeText(200_000),
      }),
      z.object({
        kind: z.literal("ended"),
        flowId: z.string().max(128),
        status: z.enum(["done", "stopped"]),
        reason: safeText(2000),
      }),
    ]),
  }),
  z.object({
    ...base,
    type: z.literal("handoff"),
    payload: z.object({
      direction: z.enum(["in", "out"]),
      /** The other side: who sent it (in) or who received it (out). */
      provider: ProviderId,
      sessionId: z.string().max(128),
      title: safeText(300).optional(),
      plan: safeText(200_000),
    }),
  }),
  z.object({
    ...base,
    type: z.literal("error"),
    payload: z.object({ title: safeText(300), message: safeText(5000), detail: safeText(50_000).optional() }),
  }),
  z.object({
    ...base,
    type: z.literal("progress"),
    payload: z.object({
      title: safeText(300),
      steps: z
        .array(z.object({ label: safeText(500), status: z.enum(["done", "active", "pending", "failed"]) }))
        .min(1)
        .max(200),
    }),
  }),
  z.object({ ...base, type: z.literal("notice"), payload: z.object({ tone: z.enum(["info", "demo"]), text: safeText(2000) }) }),
  z.object({
    ...base,
    type: z.literal("denied"),
    payload: z.object({ actions: z.array(safeText(500)).max(20), rules: z.array(safeText(300)).max(20), notice: safeText(4000).optional() }),
  }),
]);
export type AgentEvent = z.infer<typeof AgentEvent>;
export type AgentEventType = AgentEvent["type"];
export type EventOf<T extends AgentEventType> = Extract<AgentEvent, { type: T }>;

export type ParsedEvent = { ok: true; event: AgentEvent } | { ok: false; eventId: string; ts: number; reason: string };

/** Validate untrusted provider output. Invalid events become explicit, renderable failures. */
export function parseEvent(raw: unknown): ParsedEvent {
  const res = AgentEvent.safeParse(raw);
  if (res.success) return { ok: true, event: res.data };
  const r = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
  const first = res.error.issues[0];
  return {
    ok: false,
    eventId: typeof r.eventId === "string" ? r.eventId : `invalid-${Math.random().toString(36).slice(2, 8)}`,
    ts: typeof r.ts === "number" ? r.ts : 0,
    reason: first ? `${first.path.join(".") || "event"}: ${first.message}` : "Unknown validation error",
  };
}

/** Parse a batch, dropping exact duplicates (same eventId) and keeping stream order by timestamp. */
export function parseEvents(raw: unknown[]): ParsedEvent[] {
  const seen = new Set<string>();
  const out: ParsedEvent[] = [];
  for (const r of raw) {
    const p = parseEvent(r);
    const id = p.ok ? p.event.eventId : p.eventId;
    if (seen.has(id)) continue;
    seen.add(id);
    out.push(p);
  }
  return out.sort((a, b) => (a.ok ? a.event.ts : a.ts) - (b.ok ? b.event.ts : b.ts));
}

export function diffStats(diff: string): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const line of diff.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) additions++;
    else if (line.startsWith("-")) deletions++;
  }
  return { additions, deletions };
}

export interface DiffLine {
  kind: "add" | "del" | "ctx" | "hunk";
  text: string;
  oldNo?: number;
  newNo?: number;
}

export function parseUnifiedDiff(diff: string): DiffLine[] {
  const lines: DiffLine[] = [];
  let oldNo = 0;
  let newNo = 0;
  for (const raw of diff.replace(/\n$/, "").split("\n")) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/.exec(raw);
    if (hunk) {
      oldNo = Number(hunk[1]);
      newNo = Number(hunk[2]);
      lines.push({ kind: "hunk", text: raw });
      continue;
    }
    if (raw.startsWith("+++") || raw.startsWith("---")) continue;
    const mark = raw[0];
    const text = raw.slice(1);
    if (mark === "+") lines.push({ kind: "add", text, newNo: newNo++ });
    else if (mark === "-") lines.push({ kind: "del", text, oldNo: oldNo++ });
    else lines.push({ kind: "ctx", text, oldNo: oldNo++, newNo: newNo++ });
  }
  return lines;
}

/** Rebuilds the before/after text a unified diff describes (the full file when hunks cover it). */
export function sidesFromDiff(diff: string): { original: string; modified: string } {
  const original: string[] = [];
  const modified: string[] = [];
  for (const l of parseUnifiedDiff(diff)) {
    if (l.kind === "ctx") {
      original.push(l.text);
      modified.push(l.text);
    } else if (l.kind === "del") original.push(l.text);
    else if (l.kind === "add") modified.push(l.text);
  }
  return { original: original.join("\n"), modified: modified.join("\n") };
}
