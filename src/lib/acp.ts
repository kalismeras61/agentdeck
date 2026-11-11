import { agentdeckItem, agentdeckTool } from "./agentdeckTools";
import { imageFromBlock, screenshotPath } from "./images";
import { attachmentsOf, addedFileDiff, diffFromStrings, groupTitle, relPath, type NormalizeContext, type StoredEvent } from "./claude";
import type { ProviderId, ToolItem } from "./events";
import { t } from "./i18n";

/**
 * Normalizer for agents that speak the Agent Client Protocol (Gemini CLI, Grok Build).
 * AgentDeck stores agent text as `agent_message`, ACP `session/update`s it shows as
 * `update/<kind>`, permission requests as `approval` and the prompt result as `turn/ended`.
 */

type Json = Record<string, unknown>;
type Item = ToolItem;

const MAX_OUTPUT = 200_000;
const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const obj = (v: unknown): Json => (v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {});
const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

const HIGH_RISK =
  /(\brm\s+-[a-z]*[rf]|\bsudo\b|\bchmod\b|\bchown\b|\bmkfs\b|\bdd\s+if=|curl[^|]*\|\s*(ba)?sh|wget[^|]*\|\s*(ba)?sh|git\s+push|git\s+reset\s+--hard|git\s+clean\s+-[a-z]*f|>\s*\/dev\/)/;

const LABEL: Record<string, string> = { "gemini-cli": "Gemini CLI", "grok-build": "Grok Build", "kimi-code": "Kimi Code", antigravity: "Antigravity" };

/** A tool call's title; the agentdeck tools (bots handing work on) get a readable one. */
function titleOf(title: string | undefined, input: Json): string | undefined {
  const ad = agentdeckTool(title);
  return ad ? agentdeckItem(ad, input).label : title;
}

function kindOf(acpKind: string | undefined, created: boolean): Item["kind"] {
  switch (acpKind) {
    case "read":
    case "fetch":
      return "read";
    case "search":
      return "search";
    case "edit":
    case "delete":
    case "move":
      return created ? "create" : "edit";
    case "execute":
      return "command";
    default:
      return "analyze";
  }
}

const statusOf = (s: string | undefined): Item["status"] => (s === "completed" ? "ok" : s === "failed" ? "error" : "running");

function diffsOf(content: unknown[], root: string) {
  return content
    .map(obj)
    .filter((c) => c.type === "diff")
    .map((c) => {
      const path = str(c.path) ?? "";
      const before = str(c.oldText) ?? "";
      const after = str(c.newText) ?? "";
      const created = !before && c.oldText !== undefined ? true : obj(c._meta).kind === "add";
      return {
        path,
        rel: relPath(path, root) ?? (path.startsWith("/") ? undefined : path),
        created,
        diff: created ? addedFileDiff(after) : diffFromStrings(before, after),
      };
    });
}

function textOf(content: unknown[]): string {
  return content
    .map(obj)
    .filter((c) => c.type === "content")
    .map((c) => str(obj(c.content).text) ?? "")
    .filter(Boolean)
    .join("\n");
}

export function normalizeAcp(provider: ProviderId, events: StoredEvent[], ctx: NormalizeContext): unknown[] {
  const root = ctx.projectPath;
  const name = LABEL[provider] ?? t("The agent");
  const out: Json[] = [];
  const items = new Map<string, { item: Item; raw: Json; changed: boolean; shown: boolean }>();
  const perms = new Map<string, Json>();
  let group: Json | null = null;
  let opened = 0;
  let lastApprovals: unknown = undefined;
  let version: string | undefined;

  const base = (e: StoredEvent, id: string) => ({ eventId: id, sessionId: ctx.sessionId, ts: e.ts, provider });
  const push = (ev: Json) => {
    out.push(ev);
    if (ev.type !== "tool_group" && ev.type !== "changeset") group = null;
  };
  const expirePending = () => {
    for (const p of perms.values()) if (obj(p.payload).state === "pending") obj(p.payload).state = "expired";
    for (const { item } of items.values())
      if (item.status === "running") {
        item.status = "warning";
        item.meta = t("Not run");
      }
  };

  const applyUpdate = (e: StoredEvent, u: Json) => {
    const id = str(u.toolCallId) ?? `t-${e.seq}`;
    const content = arr(u.content);
    const diffs = diffsOf(content, root);
    const existing = items.get(id);
    const created = diffs.length > 0 && diffs.every((d) => d.created);
    if (!existing) {
      const loc = str(obj(arr(u.locations)[0]).path);
      const raw = obj(u.rawInput);
      const target = str(raw.command) ?? (loc ? (relPath(loc, root) ?? loc) : undefined);
      const item: Item = {
        id,
        kind: kindOf(str(u.kind), created),
        label: titleOf(str(u.title), raw) ?? t("Used a tool"),
        target: target && target !== str(u.title) ? target : undefined,
        status: statusOf(str(u.status)),
      };
      if (item.kind === "command" && /\b(test|pytest|jest|vitest|cargo test|go test|npm (run )?test)\b/.test(item.target ?? "")) item.kind = "test";
      if (!group) {
        group = { ...base(e, `g-${id}`), type: "tool_group", payload: { title: "", items: [] as Item[] } };
        out.push(group);
      }
      (obj(group.payload).items as Item[]).push(item);
      items.set(id, { item, raw: u, changed: false, shown: false });
    }
    const ref = items.get(id)!;
    if (str(u.title)) ref.item.label = titleOf(str(u.title), obj(u.rawInput))!;
    if (u.status) ref.item.status = statusOf(str(u.status));
    const text = textOf(content);
    if (text) ref.item.output = text.length > MAX_OUTPUT ? `${text.slice(0, MAX_OUTPUT)}\n… ${t("output truncated")}` : text;
    if (ref.item.status === "error" && /cancel|denied|reject/i.test(text)) ref.item.meta = t("Denied");
    if (ref.item.status === "ok" && !ref.shown) {
      ref.shown = true;
      content.forEach((c, i) => {
        const inner = obj(obj(c).content);
        const img = imageFromBlock(obj(c).type === "content" ? inner : obj(c));
        if (img) out.push({ ...base(e, `img-${e.seq}-${id}-${i}`), type: "image", payload: { ...img, alt: t("Image from {name}", { name: ref.item.label }), caption: ref.item.target ?? ref.item.label } });
      });
      const shot = ref.item.kind === "command" ? screenshotPath(ref.item.target ?? "", root) : null;
      if (shot) out.push({ ...base(e, `shot-${e.seq}-${id}`), type: "image", payload: { path: shot, alt: t("Screenshot"), caption: relPath(shot, root) || shot } });
    }
    if (ref.item.status === "ok" && !ref.changed) {
      const files = diffs
        .filter((d) => d.rel && d.rel !== "." && d.diff)
        .map((d) => ({ path: d.rel!, change: d.created ? "added" : "modified", diff: d.diff }));
      if (files.length) {
        ref.changed = true;
        out.push({ ...base(e, `c-${e.seq}-${id}`), type: "changeset", payload: { files } });
      }
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
            const pl = obj(perm.payload);
            pl.state = d === "deny" ? "denied" : d === "allow_session" ? "allowed_session" : "allowed";
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
        case "process_exit":
          expirePending();
          if (!p.expected && p.code !== 0) {
            push({
              ...base(e, `x-${e.seq}`),
              type: "error",
              payload: {
                title: t("{name} stopped unexpectedly", { name }),
                message:
                  p.code === null || p.code === undefined ? t("The process was ended by a signal.") : t("The process exited with code {code}.", { code: String(p.code) }),
                detail: str(p.stderrTail) || undefined,
              },
            });
          }
          break;
        case "app_restart":
          expirePending();
          push({ ...base(e, `r-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.message) ?? t("AgentDeck restarted.") } });
          break;
        case "error":
          push({ ...base(e, `e-${e.seq}`), type: "error", payload: { title: str(p.title) ?? t("Something went wrong"), message: str(p.message) ?? "" } });
          break;
        case "denied_actions": {
          const actions = arr(p.actions)
            .map((a) =>
              typeof a === "string"
                ? a
                : [str(obj(a).action) ?? str(obj(a).tool) ?? str(obj(a).name), str(obj(a).target) ?? str(obj(a).command) ?? str(obj(a).path)].filter(Boolean).join(": ") ||
                  JSON.stringify(a),
            )
            .slice(0, 20);
          const rules = arr(p.rules).map((r) => str(r) ?? "").filter(Boolean).slice(0, 20);
          push({ ...base(e, `d-${e.seq}`), type: "denied", payload: { actions, rules, notice: str(p.notice) || undefined } });
          break;
        }
        case "stdout_text":
          push({ ...base(e, `t-${e.seq}`), type: "notice", payload: { tone: "info", text: str(p.text) ?? "" } });
          break;
      }
      continue;
    }

    switch (e.kind) {
      case "initialized":
        version = str(obj(p.agentInfo).version);
        break;
      case "session/opened": {
        opened += 1;
        const text = p.resumed
          ? t("{name} resumed this conversation.", { name })
          : opened === 1
            ? t("{name} started.", { name: version ? `${name} ${version}` : name })
            : t("{name} started a new conversation; it couldn't reopen the previous one.", { name });
        push({ ...base(e, `o-${e.seq}`), type: "notice", payload: { tone: "info", text } });
        if (provider === "antigravity" && (opened === 1 || p.approvals !== lastApprovals)) {
          lastApprovals = p.approvals;
          push({
            ...base(e, `oa-${e.seq}`),
            type: "notice",
            payload: {
              tone: "info",
              text:
                p.approvals === "all"
                  ? t("Antigravity runs without asking: every command and file change goes ahead (you chose “Everything” for its approvals). Review what it did in Changes.")
                  : p.approvals === "auto"
                    ? t("Antigravity is in Auto mode: it edits files and runs commands it judges safe without asking; anything irreversible is skipped and listed.")
                    : t("Antigravity can't ask before acting here: it edits files in the project without asking (review them in Changes), and anything else that needs your OK is skipped and listed."),
            },
          });
        }
        break;
      }
      case "agent_message":
        if (str(p.text)?.trim()) push({ ...base(e, `a-${e.seq}`), type: "assistant_text", payload: { markdown: str(p.text) } });
        break;
      case "update/tool_call":
      case "update/tool_call_update":
        applyUpdate(e, p);
        break;
      case "update/plan": {
        const steps = arr(p.entries)
          .map(obj)
          .map((x) => ({ label: str(x.content) ?? t("Step"), status: x.status === "completed" ? "done" : x.status === "in_progress" ? "active" : "pending" }));
        if (steps.length) push({ ...base(e, `pl-${e.seq}`), type: "progress", payload: { title: t("Plan"), steps } });
        break;
      }
      case "approval": {
        const call = obj(p.toolCall);
        const options = arr(p.options).map(obj);
        const always = options.find((o) => o.kind === "allow_always");
        const raw = obj(call.rawInput);
        const content = arr(call.content);
        const diffs = diffsOf(content, root);
        const command = str(raw.command) ?? str(call.title) ?? "Use a tool";
        const paths = [
          ...new Set(
            arr(call.locations)
              .map((l) => relPath(str(obj(l).path), root))
              .filter((x): x is string => Boolean(x) && x !== "."),
          ),
        ];
        const outside = arr(call.locations).some((l) => str(obj(l).path) && relPath(str(obj(l).path), root) === undefined);
        const isCmd = call.kind === "execute";
        const risk = isCmd
          ? HIGH_RISK.test(command)
            ? "high"
            : "medium"
          : outside
            ? "high"
            : call.kind === "edit" || call.kind === "delete" || call.kind === "move"
              ? "low"
              : "medium";
        let reason = isCmd
          ? t("Runs a shell command in the project folder.")
          : call.kind === "edit"
            ? t("Changes files in the project.")
            : str(call.title)
              ? t("Asks to {action}.", { action: str(call.title)! })
              : t("Asks to use a tool.");
        if (outside) reason += ` ${t("It touches a path outside the project folder.")}`;
        if (risk === "high" && isCmd) reason += ` ${t("This command can delete data, change permissions, or affect other machines.")}`;
        const perm = {
          ...base(e, `perm-${str(p.requestId)}`),
          type: "permission",
          payload: {
            requestId: str(p.requestId) ?? "",
            toolUseId: str(call.toolCallId),
            tool: str(call.kind) ?? "tool",
            command,
            cwd: root,
            paths,
            risk,
            reason,
            diff: diffs[0]?.diff || undefined,
            supportsPersistentAllow: Boolean(always),
            persistentLabel: always ? (str(always.name) ?? t("Always allow")) : undefined,
            state: "pending",
          },
        };
        perms.set(perm.payload.requestId, perm);
        push(perm);
        break;
      }
      case "turn/ended": {
        group = null;
        for (const { item } of items.values()) if (item.status === "running") item.status = "warning";
        const stop = str(p.stopReason);
        if (stop === "refusal")
          push({ ...base(e, `te-${e.seq}`), type: "error", payload: { title: t("{name} refused this request", { name }), message: t("The agent declined to continue.") } });
        else if (stop === "max_tokens" || stop === "max_turn_requests")
          push({
            ...base(e, `te-${e.seq}`),
            type: "notice",
            payload: {
              tone: "info",
              text: stop === "max_tokens" ? t("{name} stopped early: it hit its output limit.", { name }) : t("{name} stopped early: it hit its turn limit.", { name }),
            },
          });
        break;
      }
    }
  }

  for (const ev of out) if (ev.type === "tool_group") obj(ev.payload).title = groupTitle(obj(ev.payload).items as Item[]);
  return out;
}
