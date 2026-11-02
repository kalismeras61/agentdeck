import { attachmentsOf, addedFileDiff, diffFromStrings, groupTitle, type NormalizeContext, type StoredEvent } from "./claude";
import type { ToolItem } from "./events";

/**
 * Normalizer for AgentDeck's own agent on an OpenAI-compatible API (DeepSeek, Kimi, Ollama, …).
 * Stored kinds: `assistant` {text, toolCalls}, `approval`, `tool_result`, `turn/ended`.
 */

type Json = Record<string, unknown>;
type Item = ToolItem;

const MAX_OUTPUT = 200_000;
const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const obj = (v: unknown): Json => (v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {});
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);
const parse = (s: unknown): Json => {
  try {
    return obj(JSON.parse(str(s) ?? "{}"));
  } catch {
    return {};
  }
};

const HIGH_RISK =
  /(\brm\s+-[a-z]*[rf]|\bsudo\b|\bchmod\b|\bchown\b|\bmkfs\b|\bdd\s+if=|curl[^|]*\|\s*(ba)?sh|wget[^|]*\|\s*(ba)?sh|git\s+push|git\s+reset\s+--hard|git\s+clean\s+-[a-z]*f|>\s*\/dev\/)/;
const isTest = (c: string) => /\b(test|pytest|jest|vitest|cargo test|go test|flutter test|npm (run )?test)\b/.test(c);

function itemFor(id: string, name: string, args: Json): Item {
  const path = str(args.path);
  switch (name) {
    case "list_files":
      return { id, kind: "search", label: "Listed", target: path && path !== "." ? path : "the project", status: "running" };
    case "read_file":
      return { id, kind: "read", label: "Read", target: path, status: "running" };
    case "search":
      return { id, kind: "search", label: "Searched for", target: str(args.pattern), status: "running" };
    case "write_file":
      return { id, kind: "create", label: "Wrote", target: path, status: "running" };
    case "edit_file":
      return { id, kind: "edit", label: "Edited", target: path, status: "running" };
    case "run_command": {
      const cmd = str(args.command) ?? "";
      return { id, kind: isTest(cmd) ? "test" : "command", label: "Ran", target: cmd, status: "running" };
    }
    default:
      return { id, kind: "analyze", label: `Used ${name}`, status: "running" };
  }
}

function changeDiff(p: Json): string {
  const after = str(p.after) ?? "";
  return p.created === true || p.before === null || p.before === undefined ? addedFileDiff(after) : diffFromStrings(str(p.before) ?? "", after);
}

export function normalizeApi(events: StoredEvent[], ctx: NormalizeContext, name = "The model"): unknown[] {
  const out: Json[] = [];
  const items = new Map<string, Item>();
  const perms = new Map<string, Json>();
  let group: Json | null = null;

  const base = (e: StoredEvent, id: string) => ({ eventId: id, sessionId: ctx.sessionId, ts: e.ts, provider: "api" });
  const push = (ev: Json) => {
    out.push(ev);
    if (ev.type !== "tool_group" && ev.type !== "changeset") group = null;
  };
  const expirePending = () => {
    for (const p of perms.values()) if (obj(p.payload).state === "pending") obj(p.payload).state = "expired";
    for (const item of items.values())
      if (item.status === "running") {
        item.status = "warning";
        item.meta = "Not run";
      }
  };

  for (const e of events) {
    const p = e.payload;
    if (e.source === "agentdeck") {
      switch (e.kind) {
        case "user_message":
          push({ ...base(e, `u-${e.seq}`), type: "user_message", payload: { text: str(p.text) ?? "", attachments: attachmentsOf(p) } });
          break;
        case "permission_decision": {
          const perm = perms.get(str(p.requestId) ?? "");
          if (perm) {
            const d = str(p.decision);
            obj(perm.payload).state = d === "deny" ? "denied" : d === "allow_session" ? "allowed_session" : "allowed";
            obj(perm.payload).decidedAt = e.ts;
          }
          break;
        }
        case "interrupt_requested":
          push({ ...base(e, `n-${e.seq}`), type: "notice", payload: { tone: "info", text: "You asked the agent to stop." } });
          break;
        case "app_restart":
          expirePending();
          push({ ...base(e, `r-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.message) ?? "AgentDeck restarted." } });
          break;
        case "error":
          push({ ...base(e, `e-${e.seq}`), type: "error", payload: { title: str(p.title) ?? "Something went wrong", message: str(p.message) ?? "" } });
          break;
      }
      continue;
    }
    if (e.source !== "api") continue;
    switch (e.kind) {
      case "assistant": {
        const text = str(p.text)?.trim();
        if (text) push({ ...base(e, `a-${e.seq}`), type: "assistant_text", payload: { markdown: text } });
        for (const raw of arr(p.toolCalls)) {
          const c = obj(raw);
          const id = str(c.id) ?? `${e.seq}`;
          const item = itemFor(id, str(c.name) ?? "tool", parse(c.arguments));
          if (!group) {
            group = { ...base(e, `g-${id}`), type: "tool_group", payload: { title: "", items: [] as Item[] } };
            out.push(group);
          }
          (obj(group.payload).items as Item[]).push(item);
          items.set(id, item);
        }
        break;
      }
      case "approval": {
        const tool = str(p.tool) ?? "tool";
        const isCmd = tool === "run_command";
        const command = isCmd ? (str(p.command) ?? "") : `${tool === "write_file" ? (p.created ? "Create" : "Write") : "Edit"} ${str(p.path) ?? "a file"}`;
        const high = isCmd && HIGH_RISK.test(command);
        const perm = {
          ...base(e, `perm-${str(p.requestId)}`),
          type: "permission",
          payload: {
            requestId: str(p.requestId) ?? "",
            toolUseId: str(p.toolCallId),
            tool,
            command,
            cwd: ctx.projectPath,
            paths: !isCmd && str(p.path) ? [str(p.path)!] : [],
            risk: isCmd ? (high ? "high" : "medium") : "low",
            reason: isCmd
              ? `Runs a shell command in the project folder.${high ? " This command can delete data, change permissions, or affect other machines." : ""}`
              : p.created
                ? "Creates this file."
                : "Changes this file.",
            diff: isCmd ? undefined : changeDiff(p),
            supportsPersistentAllow: true,
            persistentLabel: isCmd ? "Allow this command for this session" : "Allow edits for this session",
            state: "pending",
          },
        };
        perms.set(perm.payload.requestId, perm);
        push(perm);
        break;
      }
      case "tool_result": {
        const item = items.get(str(p.toolCallId) ?? "");
        if (!item) break;
        item.status = p.ok === true ? "ok" : "error";
        if (p.denied) item.meta = "Denied";
        else if (p.refused === "plan_mode") {
          item.meta = "Plan mode";
          item.label = "Held back";
        } else if (typeof p.exitCode === "number" && p.exitCode !== 0) item.meta = `Exit code ${p.exitCode}`;
        const output = str(p.output);
        if (output && (item.kind === "command" || item.kind === "test" || p.ok !== true))
          item.output = output.length > MAX_OUTPUT ? `${output.slice(0, MAX_OUTPUT)}\n… output truncated` : output;
        if (p.ok === true && str(p.after) !== undefined && str(p.path)) {
          const change = { path: str(p.path)!, change: p.created ? "added" : "modified", diff: changeDiff(p) };
          const last = out[out.length - 1];
          if (last?.type === "changeset") (obj(last.payload).files as Json[]).push(change);
          else out.push({ ...base(e, `c-${e.seq}`), type: "changeset", payload: { files: [change] } });
        }
        break;
      }
      case "turn/ended": {
        group = null;
        for (const item of items.values()) if (item.status === "running") item.status = "warning";
        const stop = str(p.stopReason);
        if (stop === "max_tokens")
          push({ ...base(e, `te-${e.seq}`), type: "notice", payload: { tone: "info", text: `${name} stopped early: it hit its output limit.` } });
        break;
      }
    }
  }

  for (const ev of out) if (ev.type === "tool_group") obj(ev.payload).title = groupTitle(obj(ev.payload).items as Item[]);
  return out;
}
