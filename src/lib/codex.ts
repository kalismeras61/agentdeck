/**
 * Codex App Server notifications → AgentDeck's normalized events.
 * Same contract as the Claude normalizer: a pure function of the stored log, validated afterwards.
 */
import { answerQuestion, attachmentsOf, relPath, type NormalizeContext, type StoredEvent } from "./claude";

type Json = Record<string, unknown>;
type Item = {
  id: string;
  kind: "read" | "search" | "edit" | "create" | "command" | "test" | "analyze";
  label: string;
  target?: string;
  status: "ok" | "warning" | "error" | "running";
  meta?: string;
  output?: string;
  durationMs?: number;
};

const MAX_OUTPUT = 200_000;
const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const num = (v: unknown): number | undefined => (typeof v === "number" && Number.isFinite(v) ? v : undefined);
const obj = (v: unknown): Json => (v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {});
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

const HIGH_RISK =
  /(\brm\s+-[a-z]*[rf]|\bsudo\b|\bchmod\b|\bchown\b|\bmkfs\b|\bdd\s+if=|curl[^|]*\|\s*(ba)?sh|wget[^|]*\|\s*(ba)?sh|git\s+push|git\s+reset\s+--hard|git\s+clean\s+-[a-z]*f|>\s*\/dev\/)/;

/** "/bin/bash -lc 'echo hi'" → "echo hi", preferring Codex's own parsed form. */
function displayCommand(command: string | undefined, actions: unknown): string {
  const parsed = arr(actions)
    .map((a) => str(obj(a).command))
    .filter((c): c is string => Boolean(c));
  if (parsed.length) return parsed.join(" && ");
  const m = /^\S*(?:ba|z)?sh\s+-l?c\s+(['"])([\s\S]*)\1$/.exec(command ?? "");
  return m?.[2] ?? command ?? "";
}

function isTest(cmd: string) {
  return /\b(test|pytest|jest|vitest|cargo test|go test|flutter test|npm (run )?test|unittest)\b/.test(cmd);
}

function toUnified(diff: string, kind: string): string {
  const at = diff.indexOf("@@");
  if (at >= 0) return diff.slice(at).endsWith("\n") ? diff.slice(at) : `${diff.slice(at)}\n`;
  const lines = diff.replace(/\n$/, "").split("\n");
  if (kind === "add") return `@@ -0,0 +1,${lines.length} @@\n${lines.map((l) => `+${l.replace(/^\+/, "")}`).join("\n")}\n`;
  if (kind === "delete") return `@@ -1,${lines.length} +0,0 @@\n${lines.map((l) => `-${l.replace(/^-/, "")}`).join("\n")}\n`;
  return "";
}

function groupTitle(all: Item[]): string {
  const running = all.filter((i) => i.status === "running").length;
  const notRun = all.filter((i) => i.status !== "running" && (i.meta === "Denied" || i.meta === "Not run")).length;
  const done = all.filter((i) => i.status !== "running" && i.meta !== "Denied" && i.meta !== "Not run");
  const n = (c: number, one: string, many: string) => `${c} ${c === 1 ? one : many}`;
  const parts: string[] = [];
  const cmds = done.filter((i) => i.kind === "command" || i.kind === "test").length;
  const edits = done.filter((i) => i.kind === "edit" || i.kind === "create").length;
  const other = done.length - cmds - edits;
  if (cmds) parts.push(`ran ${n(cmds, "command", "commands")}`);
  if (edits) parts.push(`changed ${n(edits, "file", "files")}`);
  if (other) parts.push(n(other, "other step", "other steps"));
  if (notRun) parts.push(notRun === 1 ? "1 step didn't run" : `${notRun} steps didn't run`);
  if (running) parts.push(`${running} in progress`);
  const s = parts.join(", ") || (running ? "Starting a step" : "Used tools");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function normalizeCodex(events: StoredEvent[], ctx: NormalizeContext): unknown[] {
  const root = ctx.projectPath;
  const out: Json[] = [];
  const items = new Map<string, { item: Item; group: Json }>();
  const fileItems = new Map<string, Json>(); // fileChange items by id, for approval context
  const perms = new Map<string, Json>();
  const plans = new Map<string, Json>();
  let group: Json | null = null;
  let opened = 0;

  const base = (e: StoredEvent, id: string) => ({ eventId: id, sessionId: ctx.sessionId, ts: e.ts, provider: "codex" });
  const push = (ev: Json) => {
    out.push(ev);
    if (ev.type !== "tool_group" && ev.type !== "changeset") group = null;
  };
  const expire = () => {
    for (const p of perms.values()) if (obj(p.payload).state === "pending") obj(p.payload).state = "expired";
    for (const { item } of items.values())
      if (item.status === "running") {
        item.status = "warning";
        item.meta = "Not run";
      }
  };
  const ensureItem = (e: StoredEvent, raw: Json): Item | undefined => {
    const id = str(raw.id) ?? "";
    const existing = items.get(id);
    if (existing) return existing.item;
    const type = str(raw.type);
    let item: Item;
    if (type === "commandExecution") {
      const cmd = displayCommand(str(raw.command), raw.commandActions);
      item = { id, kind: isTest(cmd) ? "test" : "command", label: "Ran", target: cmd, status: "running" };
    } else if (type === "fileChange") {
      const paths = arr(raw.changes).map((c) => relPath(str(obj(c).path), root) ?? str(obj(c).path) ?? "");
      item = { id, kind: "edit", label: "Edited", target: paths.join(", "), status: "running" };
    } else if (type === "mcpToolCall" || type === "dynamicToolCall") {
      item = { id, kind: "analyze", label: `Used ${str(raw.tool) ?? "a tool"}`, target: str(raw.server), status: "running" };
    } else if (type === "webSearch") {
      item = { id, kind: "search", label: "Searched the web for", target: str(raw.query), status: "running" };
    } else return undefined;
    if (!group) {
      group = { ...base(e, `g-${id}`), type: "tool_group", payload: { title: "", items: [] as Item[] } };
      out.push(group);
    }
    (obj(group.payload).items as Item[]).push(item);
    items.set(id, { item, group });
    return item;
  };

  for (const e of events) {
    const p = e.payload;
    if (e.source === "agentdeck") {
      switch (e.kind) {
        case "user_message":
          push({ ...base(e, `u-${e.seq}`), type: "user_message", payload: { text: str(p.text) ?? "", attachments: attachmentsOf(p) } });
          break;
        case "imported":
          // A resumable import continues Codex's own thread.
          if (p.resumable === true) opened = 1;
          break;
        case "question_answered":
          answerQuestion(perms.get(str(p.requestId) ?? ""), p, e.ts);
          break;
        case "permission_decision": {
          const perm = perms.get(str(p.requestId) ?? "");
          if (perm) {
            const d = str(p.decision);
            obj(perm.payload).state = d === "deny" ? "denied" : d === "allow_session" ? "allowed_session" : "allowed";
            if (p.auto === "plan_mode") obj(perm.payload).autoReason = "Refused by plan mode";
            else if (p.auto === "workflow") obj(perm.payload).autoReason = "Allowed by the workflow";
            obj(perm.payload).decidedAt = e.ts;
          }
          break;
        }
        case "interrupt_requested":
          push({ ...base(e, `n-${e.seq}`), type: "notice", payload: { tone: "info", text: "You asked the agent to stop." } });
          break;
        case "process_exit":
          expire();
          if (!p.expected && p.code !== 0) {
            push({
              ...base(e, `x-${e.seq}`),
              type: "error",
              payload: {
                title: "Codex stopped unexpectedly",
                message: p.code === null || p.code === undefined ? "The process was ended by a signal." : `The process exited with code ${String(p.code)}.`,
                detail: str(p.stderrTail) || undefined,
              },
            });
          }
          break;
        case "app_restart":
          expire();
          push({ ...base(e, `r-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.message) ?? "AgentDeck restarted." } });
          break;
        case "error":
          push({ ...base(e, `e-${e.seq}`), type: "error", payload: { title: str(p.title) ?? "Something went wrong", message: str(p.message) ?? "" } });
          break;
        case "stdout_text":
          push({ ...base(e, `t-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.text) ?? "" } });
          break;
      }
      continue;
    }

    switch (e.kind) {
      case "thread/opened": {
        opened += 1;
        const model = str(p.model);
        push({
          ...base(e, `o-${e.seq}`),
          type: "notice",
          payload: {
            tone: "info",
            text: opened === 1 ? `Codex started${model ? ` with ${model}` : ""}.` : `Codex resumed this conversation${model ? ` with ${model}` : ""}.`,
          },
        });
        break;
      }
      case "warning": {
        const msg = str(p.message);
        if (msg) push({ ...base(e, `w-${e.seq}`), type: "notice", payload: { tone: "info", text: msg } });
        break;
      }
      case "error": {
        const err = obj(p.error);
        push({
          ...base(e, `ce-${e.seq}`),
          type: "error",
          payload: { title: "Codex reported an error", message: str(err.message) ?? str(p.message) ?? "No details", detail: str(err.additionalDetails) },
        });
        break;
      }
      case "item/started": {
        const raw = obj(p.item);
        if (str(raw.type) === "fileChange") fileItems.set(str(raw.id) ?? "", raw);
        ensureItem(e, raw);
        break;
      }
      case "item/completed": {
        const raw = obj(p.item);
        const type = str(raw.type);
        if (type === "agentMessage") {
          const text = str(raw.text);
          if (text?.trim()) push({ ...base(e, `a-${str(raw.id) ?? e.seq}`), type: "assistant_text", payload: { markdown: text } });
          break;
        }
        if (type === "fileChange") fileItems.set(str(raw.id) ?? "", raw);
        const item = ensureItem(e, raw);
        if (!item) break;
        const status = str(raw.status);
        item.durationMs = num(raw.durationMs);
        if (status === "declined") {
          item.status = "error";
          item.meta = "Denied";
          item.label = item.kind === "edit" ? "Edit" : "Run";
        } else if (status === "failed" || (num(raw.exitCode) ?? 0) !== 0) {
          item.status = "error";
          if (num(raw.exitCode) !== undefined) item.meta = `Exit code ${num(raw.exitCode)}`;
        } else item.status = "ok";
        const output = str(raw.aggregatedOutput);
        if (output) item.output = output.length > MAX_OUTPUT ? `${output.slice(0, MAX_OUTPUT)}\n… output truncated` : output;
        if (type === "fileChange" && status === "completed") {
          const files = arr(raw.changes)
            .map((c) => {
              const ch = obj(c);
              const kind = str(obj(ch.kind).type) ?? "update";
              const rel = relPath(str(ch.path), root);
              const diff = toUnified(str(ch.diff) ?? "", kind);
              return rel && rel !== "." && diff ? { path: rel, change: kind === "add" ? "added" : kind === "delete" ? "deleted" : "modified", diff } : null;
            })
            .filter(Boolean);
          if (files.length) {
            if (files.every((f) => f!.change === "added")) item.kind = "create";
            out.push({ ...base(e, `c-${str(raw.id)}`), type: "changeset", payload: { files } });
          }
        }
        break;
      }
      case "turn/plan/updated": {
        const steps = arr(p.plan).map((s) => {
          const o = obj(s);
          const st = str(o.status);
          return { label: str(o.step) ?? "Step", status: st === "completed" ? "done" : st === "inProgress" ? "active" : "pending" };
        });
        if (!steps.length) break;
        const key = str(p.turnId) ?? String(e.seq);
        const existing = plans.get(key);
        if (existing) obj(existing.payload).steps = steps;
        else {
          const ev = { ...base(e, `plan-${key}`), type: "progress", payload: { title: "Plan", steps } };
          plans.set(key, ev);
          push(ev);
        }
        break;
      }
      case "user_input": {
        const params = obj(p.params);
        const rid = str(p.requestId) ?? "";
        const questions = arr(params.questions)
          .map((x) => obj(x))
          .map((q) => {
            const options = arr(q.options)
              .map((o) => obj(o))
              .map((o) => ({ label: str(o.label) ?? "", description: str(o.description) }))
              .filter((o) => o.label);
            return {
              id: str(q.id) ?? "",
              header: str(q.header),
              question: str(q.question) ?? "",
              options,
              multiSelect: false,
              other: q.isOther === true || options.length === 0,
              secret: q.isSecret === true,
            };
          })
          .filter((q) => q.id && q.question);
        if (!questions.length) break;
        const card = { ...base(e, `q-${rid}`), type: "question", payload: { requestId: rid, questions, state: "pending" } };
        perms.set(rid, card);
        push(card);
        break;
      }
      case "approval": {
        const params = obj(p.params);
        const method = str(p.method) ?? "";
        const rid = str(p.requestId) ?? "";
        const isFile = method.includes("fileChange") || method === "applyPatchApproval";
        const available = arr(params.availableDecisions);
        let command: string;
        let paths: string[] = [];
        let diff: string | undefined;
        let risk: "low" | "medium" | "high";
        let reason = str(params.reason) ?? "";
        if (isFile) {
          const fc = fileItems.get(str(params.itemId) ?? "");
          const changes = arr(fc?.changes).map((c) => obj(c));
          paths = changes.map((c) => relPath(str(c.path), root)).filter((x): x is string => Boolean(x) && x !== ".");
          const outside = changes.some((c) => relPath(str(c.path), root) === undefined);
          diff = changes.map((c) => toUnified(str(c.diff) ?? "", str(obj(c.kind).type) ?? "update")).join("") || undefined;
          command = paths.length ? `Edit ${paths.join(", ")}` : "Edit files";
          risk = outside ? "high" : "low";
          reason = reason || (outside ? "Changes files outside the project folder." : "Changes files in the project.");
          if (str(params.grantRoot)) reason += ` Also asks to allow writes under ${str(params.grantRoot)} for this session.`;
        } else {
          command = displayCommand(str(params.command), params.commandActions);
          risk = HIGH_RISK.test(command) ? "high" : "medium";
          reason = reason || "Runs a shell command in the project folder.";
          if (risk === "high") reason += " This command can delete data, change permissions, or affect other machines.";
          if (obj(params.networkApprovalContext) && Object.keys(obj(params.networkApprovalContext)).length) reason += " It needs network access.";
        }
        const perm = {
          ...base(e, `perm-${rid}-${e.seq}`),
          type: "permission",
          payload: {
            requestId: rid,
            tool: isFile ? "apply_patch" : "shell",
            command,
            cwd: (str(params.cwd) ?? root).replace(/^\/Users\/[^/]+/, "~"),
            paths,
            risk,
            reason,
            diff,
            supportsPersistentAllow: available.includes("acceptForSession"),
            persistentLabel: available.includes("acceptForSession") ? "Allow for this session" : undefined,
            state: "pending",
          },
        };
        perms.set(rid, perm);
        push(perm);
        break;
      }
      case "serverRequest/resolved": {
        const perm = perms.get(String(p.requestId ?? ""));
        if (perm && obj(perm.payload).state === "pending") obj(perm.payload).state = "expired";
        break;
      }
      case "turn/completed": {
        group = null;
        const turn = obj(p.turn);
        for (const { item } of items.values()) if (item.status === "running") item.status = "warning";
        if (turn.status === "failed") {
          const err = obj(turn.error);
          push({
            ...base(e, `tf-${e.seq}`),
            type: "error",
            payload: { title: "The turn failed", message: str(err.message) ?? "Codex reported a failure without details.", detail: str(err.additionalDetails) },
          });
        } else if (turn.status === "interrupted") {
          push({ ...base(e, `ti-${e.seq}`), type: "notice", payload: { tone: "info", text: "Stopped." } });
        }
        break;
      }
    }
  }
  for (const ev of out) if (ev.type === "tool_group") obj(ev.payload).title = groupTitle(obj(ev.payload).items as Item[]);
  return out;
}
