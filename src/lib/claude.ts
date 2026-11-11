import { agentdeckItem, agentdeckTool } from "./agentdeckTools";
import { imageFromBlock, screenshotPath } from "./images";
import { t, tn, useI18n } from "./i18n";
/**
 * Claude Code stream-json → AgentDeck's normalized events.
 *
 * Input is the ordered log the desktop app stores: provider lines (source "claude") and
 * AgentDeck's own records (source "agentdeck"). The output goes through the same Zod validation
 * as every other event, so malformed provider data is contained the same way.
 * This is a pure function of the log: history after a restart renders exactly as it did live.
 */
import { structuredPatch } from "diff";

export interface StoredEvent {
  sessionId: string;
  seq: number;
  ts: number;
  source: "claude" | "codex" | "acp" | "api" | "agentdeck";
  kind: string;
  payload: Record<string, unknown> & { [k: string]: unknown };
}

export interface NormalizeContext {
  sessionId: string;
  projectPath: string;
}

type Json = Record<string, unknown>;
type Item = {
  id: string;
  kind: "read" | "search" | "edit" | "create" | "command" | "test" | "analyze";
  label: string;
  target?: string;
  status: "ok" | "warning" | "error" | "running";
  meta?: string;
  output?: string;
};

const MAX_OUTPUT = 200_000;
const MAX_DIFF_LINES = 4000;

const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const obj = (v: unknown): Json => (v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {});
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

export function relPath(path: string | undefined, root: string): string | undefined {
  if (!path) return undefined;
  const r = root.replace(/\/+$/, "");
  if (path === r) return ".";
  if (path.startsWith(`${r}/`)) return path.slice(r.length + 1);
  if (!path.startsWith("/") && !path.split("/").includes("..")) return path;
  return undefined;
}

function shortPath(path: string | undefined, root: string): string | undefined {
  if (!path) return undefined;
  return relPath(path, root) ?? path.replace(/^\/Users\/[^/]+/, "~");
}

function toolText(content: unknown): string {
  if (typeof content === "string") return content;
  return arr(content)
    .map((b) => (obj(b).type === "text" ? (str(obj(b).text) ?? "") : obj(b).type === "image" ? `[${t("image")}]` : ""))
    .filter(Boolean)
    .join("\n");
}

/** Unified diff text from the CLI's structuredPatch hunks. */
function diffFromHunks(hunks: unknown[]): string | undefined {
  const parts: string[] = [];
  let lines = 0;
  for (const h of hunks) {
    const o = obj(h);
    const body = arr(o.lines).filter((l): l is string => typeof l === "string");
    if (lines + body.length > MAX_DIFF_LINES) break;
    lines += body.length;
    parts.push(`@@ -${o.oldStart},${o.oldLines} +${o.newStart},${o.newLines} @@`, ...body);
  }
  return parts.length ? `${parts.join("\n")}\n` : undefined;
}

export function diffFromStrings(before: string, after: string): string {
  const p = structuredPatch("a", "b", before, after, "", "", { context: 3 });
  return diffFromHunks(p.hunks) ?? "";
}

export function addedFileDiff(content: string): string {
  const lines = content.replace(/\n$/, "").split("\n").slice(0, MAX_DIFF_LINES);
  return `@@ -0,0 +1,${lines.length} @@\n${lines.map((l) => `+${l}`).join("\n")}\n`;
}

const EDIT_TOOLS = new Set(["Edit", "MultiEdit", "Write", "NotebookEdit"]);

function itemFor(name: string, input: Json, root: string): Item {
  const file = str(input.file_path) ?? str(input.notebook_path) ?? str(input.path);
  switch (name) {
    case "Read":
      return { id: "", kind: "read", label: t("Read"), target: shortPath(file, root), status: "running" };
    case "Grep":
      return { id: "", kind: "search", label: t("Searched"), target: str(input.pattern), status: "running", meta: shortPath(file, root) };
    case "Glob":
      return { id: "", kind: "search", label: t("Listed"), target: str(input.pattern), status: "running" };
    case "LS":
      return { id: "", kind: "search", label: t("Listed"), target: shortPath(file, root) ?? ".", status: "running" };
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return { id: "", kind: "edit", label: t("Edited"), target: shortPath(file, root), status: "running" };
    case "Write":
      return { id: "", kind: "create", label: t("Wrote"), target: shortPath(file, root), status: "running" };
    case "Bash": {
      const cmd = str(input.command) ?? "";
      const isTest = /\b(test|pytest|jest|vitest|cargo test|go test|flutter test|npm (run )?test)\b/.test(cmd);
      return { id: "", kind: isTest ? "test" : "command", label: t("Ran"), target: cmd, status: "running", meta: str(input.description) };
    }
    case "WebFetch":
      return { id: "", kind: "search", label: t("Fetched"), target: str(input.url), status: "running" };
    case "WebSearch":
      return { id: "", kind: "search", label: t("Searched the web for"), target: str(input.query), status: "running" };
    case "Task":
    case "Agent":
      return { id: "", kind: "analyze", label: t("Started a subagent"), target: str(input.description) ?? str(input.subagent_type), status: "running" };
    default: {
      // A bot handing work to other bots (the agentdeck MCP server).
      const ad = agentdeckTool(name);
      if (ad) return { id: "", kind: "analyze", ...agentdeckItem(ad, input), status: "running" };
      return { id: "", kind: "analyze", label: t("Used {name}", { name }), target: file ? shortPath(file, root) : undefined, status: "running" };
    }
  }
}

/** Compared with what the normalizer just wrote, in the same language. */
const didNotRun = (i: Item) => i.status === "error" || i.meta === t("Not run") || i.meta === t("Denied");

export function groupTitle(all: Item[]): string {
  const running = all.filter((i) => i.status === "running").length;
  const failed = all.filter((i) => i.status !== "running" && didNotRun(i)).length;
  const items = all.filter((i) => i.status !== "running" && !didNotRun(i));
  if (!items.length && !failed) return tn(running, "Starting a step", "Starting {n} steps");
  const count = (k: Item["kind"][]) => items.filter((i) => k.includes(i.kind)).length;
  const parts: string[] = [];
  const reads = count(["read"]);
  const searches = count(["search"]);
  const edits = count(["edit", "create"]);
  const cmds = count(["command", "test"]);
  const other = count(["analyze"]);
  if (reads) parts.push(tn(reads, "read {n} file", "read {n} files"));
  if (searches) parts.push(tn(searches, "{n} search", "{n} searches"));
  if (edits) parts.push(tn(edits, "changed {n} file", "changed {n} files"));
  if (cmds) parts.push(tn(cmds, "ran {n} command", "ran {n} commands"));
  if (other) parts.push(tn(other, "{n} other step", "{n} other steps"));
  if (failed) parts.push(tn(failed, "{n} step didn't run", "{n} steps didn't run"));
  if (running) parts.push(tn(running, "{n} in progress", "{n} in progress"));
  return capitalize(parts.join(t(", ")) || t("Used tools"));
}

/** Group titles start with a capital; the pieces are lower case so they can be joined. */
export function capitalize(s: string): string {
  return s.charAt(0).toLocaleUpperCase(useI18n.getState().locale) + s.slice(1);
}

const HIGH_RISK =
  /(\brm\s+-[a-z]*[rf]|\bsudo\b|\bchmod\b|\bchown\b|\bmkfs\b|\bdd\s+if=|curl[^|]*\|\s*(ba)?sh|wget[^|]*\|\s*(ba)?sh|git\s+push|git\s+reset\s+--hard|git\s+clean\s+-[a-z]*f|>\s*\/dev\/|:\(\)\s*\{)/;

function persistentLabel(suggestions: unknown[]): string | undefined {
  if (!suggestions.length) return undefined;
  const s = obj(suggestions[0]);
  if (s.type === "setMode" && s.mode === "acceptEdits") return t("Allow edits for this session");
  if (s.type === "addRules") return t("Allow similar for this session");
  if (s.type === "addDirectories") return t("Allow this folder for this session");
  return t("Allow for this session");
}

function permissionFrom(e: StoredEvent, root: string) {
  const req = obj(e.payload.request);
  const tool = str(req.tool_name) ?? "Tool";
  const input = obj(req.input);
  const file = str(input.file_path) ?? str(input.notebook_path);
  const rel = relPath(file, root);
  const outside = Boolean(file) && rel === undefined;
  const suggestions = arr(req.permission_suggestions);
  let command: string;
  let risk: "low" | "medium" | "high" = "medium";
  let reason: string;
  let diff: string | undefined;

  if (tool === "Bash") {
    command = str(input.command) ?? "";
    risk = HIGH_RISK.test(command) ? "high" : "medium";
    reason = str(input.description) ? `${str(input.description)}.` : t("Runs a shell command in the project folder.");
    if (risk === "high") reason += ` ${t("This command can delete data, change permissions, or affect other machines.")}`;
  } else if (EDIT_TOOLS.has(tool)) {
    const verb = tool === "Write" ? "Write" : "Edit";
    command = `${verb} ${shortPath(file, root) ?? "a file"}`;
    risk = outside ? "high" : "low";
    if (tool === "Write") {
      diff = addedFileDiff(str(input.content) ?? "");
      reason = t("Creates the file, or replaces it if it exists.");
    } else if (tool === "Edit") {
      diff = diffFromStrings(str(input.old_string) ?? "", str(input.new_string) ?? "");
      reason = t("Changes part of this file.");
    } else {
      reason = t("Changes this file.");
    }
    if (outside) reason += ` ${t("The file is outside the project folder.")}`;
  } else if (tool === "WebFetch") {
    command = `Fetch ${str(input.url) ?? ""}`;
    reason = t("Downloads a web page so the agent can read it.");
  } else {
    command = `${tool} ${JSON.stringify(input).slice(0, 300)}`;
    reason = str(req.description) ?? t("Uses the {tool} tool.", { tool });
  }
  const decision = str(req.decision_reason);
  if (decision && !reason.includes(decision)) reason += ` ${decision}`;

  return {
    eventId: `perm-${str(e.payload.request_id)}`,
    sessionId: e.sessionId,
    ts: e.ts,
    provider: "claude-code",
    type: "permission",
    payload: {
      requestId: str(e.payload.request_id) ?? "",
      toolUseId: str(req.tool_use_id),
      tool,
      command,
      cwd: shortPath(root, root) === "." ? root.replace(/^\/Users\/[^/]+/, "~") : root,
      paths: rel && rel !== "." ? [rel] : [],
      risk,
      reason,
      diff,
      supportsPersistentAllow: suggestions.length > 0,
      persistentLabel: persistentLabel(suggestions),
      state: "pending",
    },
  };
}

/** Converts the stored log of one Claude Code session into normalized (not yet validated) events. */
/** Files the user attached to a message, as shown in the bubble. */
export function attachmentsOf(p: Record<string, unknown>): string[] | undefined {
  const a = Array.isArray(p.attachments) ? p.attachments.filter((x): x is string => typeof x === "string").slice(0, 20) : [];
  return a.length ? a : undefined;
}

export function normalizeClaude(events: StoredEvent[], ctx: NormalizeContext): unknown[] {
  const root = ctx.projectPath;
  const out: Json[] = [];
  const items = new Map<string, { group: Json; item: Item; name: string; input: Json }>();
  const perms = new Map<string, Json>();
  /** Plan files Claude Code writes in plan mode (outside the project), by path. */
  const planFiles = new Map<string, string>();
  let group: Json | null = null; // tool group currently accepting new tool calls
  let inits = 0;
  /** Claude Code repeats system/init every turn; only the first one after a process start is news. */
  let freshProcess = true;

  const base = (e: StoredEvent, id: string) => ({ eventId: id, sessionId: ctx.sessionId, ts: e.ts, provider: "claude-code" });
  const push = (ev: Json) => {
    out.push(ev);
    if (ev.type !== "tool_group" && ev.type !== "changeset") group = null;
  };
  const expirePending = () => {
    for (const p of perms.values()) {
      const pl = obj(p.payload);
      if (pl.state === "pending") pl.state = "expired";
    }
    for (const { item } of items.values())
      if (item.status === "running") {
        item.status = "warning";
        item.meta = t("Not run");
      }
  };

  for (const e of events) {
    const p = e.payload;
    if (e.source === "agentdeck") {
      switch (e.kind) {
        case "user_message":
          push({ ...base(e, `u-${e.seq}`), type: "user_message", payload: { text: str(p.text) ?? "", attachments: attachmentsOf(p) } });
          break;
        case "question_answered":
          answerQuestion(perms.get(str(p.requestId) ?? ""), p, e.ts);
          break;
        case "permission_decision": {
          const perm = perms.get(str(p.requestId) ?? "");
          if (perm) {
            const d = str(p.decision);
            const pl = obj(perm.payload);
            if (perm.type === "plan") pl.state = d === "deny" ? "kept" : "approved";
            else pl.state = d === "deny" ? "denied" : d === "allow_session" ? "allowed_session" : "allowed";
            if (p.auto === "plan_mode") pl.autoReason = t("Refused by plan mode");
            else if (p.auto === "workflow") pl.autoReason = t("Allowed by the workflow");
            else if (p.auto === "bot") pl.autoReason = t("Allowed for the bot");
            else if (p.auto === "approvals") pl.autoReason = t("Allowed by your approval setting");
            pl.decidedAt = e.ts;
          }
          break;
        }
        case "interrupt_requested":
          push({ ...base(e, `n-${e.seq}`), type: "notice", payload: { tone: "info", text: t("You asked the agent to stop.") } });
          break;
        case "imported":
          // A resumable import continues the agent's own conversation.
          if (p.resumable === true) inits = 1;
          break;
        case "session_moved":
          // Moves keep the conversation when the backend could carry it over.
          if (p.kept !== true) inits = 0;
          freshProcess = true;
          break;
        case "worktree_removed":
          inits = 0;
          freshProcess = true;
          break;
        case "process_exit":
          freshProcess = true;
          expirePending();
          if (!p.expected && p.code !== 0) {
            push({
              ...base(e, `x-${e.seq}`),
              type: "error",
              payload: {
                title: t("{name} stopped unexpectedly", { name: "Claude Code" }),
                message:
                  p.code === null || p.code === undefined ? t("The process was ended by a signal.") : t("The process exited with code {code}.", { code: String(p.code) }),
                detail: str(p.stderrTail) || undefined,
              },
            });
          }
          break;
        case "app_restart":
          freshProcess = true;
          expirePending();
          push({ ...base(e, `r-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.message) ?? t("AgentDeck restarted.") } });
          break;
        case "error":
          push({ ...base(e, `e-${e.seq}`), type: "error", payload: { title: str(p.title) ?? t("Something went wrong"), message: str(p.message) ?? "" } });
          break;
        case "stdout_text":
          push({ ...base(e, `t-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.text) ?? "" } });
          break;
      }
      continue;
    }

    // Provider events. Subagent traffic (parent_tool_use_id set) stays inside its Task tool.
    if (p.parent_tool_use_id) continue;
    switch (e.kind) {
      case "system/init": {
        if (!freshProcess) break;
        freshProcess = false;
        inits += 1;
        const version = str(p.claude_code_version);
        const model = str(p.model);
        const name = version ? `Claude Code ${version}` : "Claude Code";
        const text =
          inits === 1
            ? model
              ? t("{name} started with {model}.", { name, model })
              : t("{name} started.", { name })
            : model
              ? t("{name} resumed this conversation with {model}.", { name: "Claude Code", model })
              : t("{name} resumed this conversation.", { name: "Claude Code" });
        push({ ...base(e, `i-${e.seq}`), type: "notice", payload: { tone: "info", text } });
        break;
      }
      case "system/api_retry":
        push({
          ...base(e, `ar-${e.seq}`),
          type: "notice",
          payload: { tone: "info", text: t("The API returned an error, retrying (attempt {attempt} of {max}).", { attempt: String(p.attempt ?? "?"), max: String(p.max_retries ?? "?") }) },
        });
        break;
      case "assistant": {
        const msg = obj(p.message);
        arr(msg.content).forEach((raw, i) => {
          const b = obj(raw);
          if (b.type === "text" && str(b.text)?.trim()) {
            push({ ...base(e, `a-${e.seq}-${i}`), type: "assistant_text", payload: { markdown: str(b.text) } });
          } else if (b.type === "tool_use") {
            const name = str(b.name) ?? "tool";
            const input = obj(b.input);
            const id = str(b.id) ?? `${e.seq}-${i}`;
            // Plan-mode bookkeeping is shown as the plan card, not as tool steps.
            if (name === "ExitPlanMode" || name === "ToolSearch") return;
            const planPath = str(input.file_path);
            if (name === "Write" && planPath && /\/\.claude\/plans\/[^/]+\.md$/.test(planPath)) {
              planFiles.set(planPath, str(input.content) ?? "");
              return;
            }
            if (name === "TodoWrite") {
              const todos = arr(input.todos).map((x) => obj(x));
              if (todos.length) {
                push({
                  ...base(e, `todo-${id}`),
                  type: "progress",
                  payload: {
                    title: t("Plan"),
                    steps: todos.map((td) => ({
                      label: str(td.content) ?? str(td.activeForm) ?? t("Step"),
                      status: td.status === "completed" ? "done" : td.status === "in_progress" ? "active" : "pending",
                    })),
                  },
                });
              }
              return;
            }
            const item = { ...itemFor(name, input, root), id };
            if (!group) {
              group = { ...base(e, `g-${id}`), type: "tool_group", payload: { title: "", items: [] as Item[] } };
              out.push(group);
            }
            (obj(group.payload).items as Item[]).push(item);
            items.set(id, { group, item, name, input });
          }
        });
        break;
      }
      case "user": {
        const msg = obj(p.message);
        const result = obj(p.tool_use_result);
        for (const raw of arr(msg.content)) {
          const b = obj(raw);
          if (b.type !== "tool_result") continue;
          const ref = items.get(str(b.tool_use_id) ?? "");
          if (!ref) continue;
          const text = toolText(b.content);
          const isError = b.is_error === true;
          ref.item.status = isError ? "error" : "ok";
          if (isError && /denied/i.test(text)) ref.item.meta = t("Denied");
          if (isError && /in plan mode/i.test(text)) {
            ref.item.meta = t("Plan mode");
            ref.item.label = t("Held back");
          }
          if (text && (ref.item.kind === "command" || ref.item.kind === "test" || isError)) {
            ref.item.output = text.length > MAX_OUTPUT ? `${text.slice(0, MAX_OUTPUT)}\n… ${t("output truncated")}` : text;
          }
          // Pictures the agent looked at (Read on an image, screenshot tools) and screenshots it saved.
          if (!isError) {
            const file = str(ref.input.file_path);
            const caption = (file && relPath(file, root)) || file || ref.name;
            arr(b.content).forEach((c, i) => {
              const img = imageFromBlock(obj(c));
              if (img) out.push({ ...base(e, `img-${e.seq}-${String(b.tool_use_id)}-${i}`), type: "image", payload: { ...img, alt: t("Image from {name}", { name: ref.name }), caption } });
            });
            const shot = ref.name === "Bash" ? screenshotPath(str(ref.input.command) ?? "", root) : null;
            if (shot) out.push({ ...base(e, `shot-${e.seq}-${String(b.tool_use_id)}`), type: "image", payload: { path: shot, alt: t("Screenshot"), caption: relPath(shot, root) || shot } });
          }
          if (!isError && EDIT_TOOLS.has(ref.name)) {
            const file = str(result.filePath) ?? str(ref.input.file_path) ?? str(ref.input.notebook_path);
            const rel = relPath(file, root);
            if (!rel || rel === ".") continue;
            const created = result.type === "create" || (ref.name === "Write" && result.originalFile === null);
            let diff = diffFromHunks(arr(result.structuredPatch));
            if (!diff && created) diff = addedFileDiff(str(result.content) ?? str(ref.input.content) ?? "");
            if (!diff && ref.name === "Edit") diff = diffFromStrings(str(ref.input.old_string) ?? "", str(ref.input.new_string) ?? "");
            if (!diff) continue;
            if (created) ref.item.kind = "create";
            const change = { path: rel, change: created ? "added" : "modified", diff };
            const last = out[out.length - 1];
            if (last?.type === "changeset") {
              const files = obj(last.payload).files as Json[];
              const at = files.findIndex((f) => f.path === rel);
              if (at >= 0) {
                const prev = files[at]!;
                files[at] = { ...change, change: prev.change === "added" ? "added" : change.change, diff: `${String(prev.diff)}${diff}` };
              } else files.push(change);
            } else {
              out.push({ ...base(e, `c-${e.seq}-${String(b.tool_use_id)}`), type: "changeset", payload: { files: [change] } });
            }
          }
        }
        break;
      }
      case "control_request/can_use_tool": {
        const req = obj(p.request);
        if (req.tool_name === "ExitPlanMode") {
          const input = obj(req.input);
          const filePath = str(input.planFilePath);
          const markdown = str(input.plan) ?? (filePath ? planFiles.get(filePath) : undefined) ?? [...planFiles.values()].pop() ?? "";
          const plan = {
            ...base(e, `plan-${str(p.request_id)}`),
            type: "plan",
            payload: { requestId: str(p.request_id), markdown, filePath: filePath?.replace(/^\/(Users|home)\/[^/]+|^\/root/, "~"), state: "pending" },
          };
          perms.set(str(p.request_id) ?? "", plan);
          push(plan);
          break;
        }
        if (req.tool_name === "AskUserQuestion") {
          const q = { ...base(e, `q-${str(p.request_id)}`), type: "question", payload: questionFromClaude(obj(req.input), str(p.request_id) ?? "") };
          perms.set(str(p.request_id) ?? "", q);
          push(q);
          break;
        }
        const perm = permissionFrom(e, root);
        perms.set(perm.payload.requestId, perm);
        push(perm);
        break;
      }

      case "control_cancel_request": {
        const perm = perms.get(str(p.request_id) ?? "");
        if (perm && obj(perm.payload).state === "pending") obj(perm.payload).state = "expired";
        break;
      }
      default:
        if (e.kind.startsWith("result")) {
          group = null;
          for (const { item } of items.values()) if (item.status === "running") item.status = "warning";
          const isError = p.is_error === true || e.kind !== "result/success";
          if (isError && !/interrupt/i.test(String(p.result ?? "") + String(p.subtype ?? ""))) {
            const errors = arr(p.errors).filter((x): x is string => typeof x === "string");
            push({
              ...base(e, `re-${e.seq}`),
              type: "error",
              payload: {
                title: e.kind === "result/error_max_turns" ? t("The agent hit its turn limit") : t("The turn ended with an error"),
                message: str(p.result) || errors[0] || t("{name} reported an error without details.", { name: "Claude Code" }),
                detail: errors.length > 1 ? errors.join("\n") : undefined,
              },
            });
          }
        }
    }
  }

  for (const ev of out) if (ev.type === "tool_group") obj(ev.payload).title = groupTitle(obj(ev.payload).items as Item[]);
  return out;
}

/** Claude Code's AskUserQuestion as a question card; answers are keyed by the question text. */
function questionFromClaude(input: Json, requestId: string): Json {
  const questions = arr(input.questions)
    .map((x) => obj(x))
    .map((q) => ({
      id: str(q.question) ?? "",
      header: str(q.header),
      question: str(q.question) ?? "",
      options: arr(q.options)
        .map((o) => obj(o))
        .map((o) => ({ label: str(o.label) ?? "", description: str(o.description) }))
        .filter((o) => o.label),
      multiSelect: q.multiSelect === true,
      other: true,
      secret: false,
    }))
    .filter((q) => q.question);
  return { requestId, questions, state: "pending" };
}

/** Marks a question card answered (or skipped) from AgentDeck's record of the answer. */
export function answerQuestion(card: Json | undefined, p: Json, ts: number) {
  if (!card || card.type !== "question") return;
  const pl = obj(card.payload);
  pl.state = p.skipped === true ? "skipped" : "answered";
  const a = obj(p.answers);
  if (Object.keys(a).length) pl.answers = Object.fromEntries(Object.entries(a).map(([k, v]) => [k, arr(v).filter((x): x is string => typeof x === "string")]));
  pl.decidedAt = ts;
}
