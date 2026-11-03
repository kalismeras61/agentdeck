/**
 * The `agentdeck` MCP tools bots hand work to each other with (src-tauri/src/mcp.rs), as the
 * conversation shows them: "Delegated to Docs Writer", "Waited for 2 tasks". Each agent names
 * them its own way: Claude Code `mcp__agentdeck__delegate_task`, Codex the server and tool,
 * Gemini CLI "delegate_task (agentdeck MCP Server)".
 */
import { firstLine } from "./bots";
import { t, tn } from "./i18n";

export const AGENTDECK_TOOLS = ["list_bots", "list_projects", "delegate_task", "wait_for_tasks", "task_status"] as const;
export type AgentdeckTool = (typeof AGENTDECK_TOOLS)[number];

const isTool = (s: string): s is AgentdeckTool => (AGENTDECK_TOOLS as readonly string[]).includes(s);

/** The agentdeck tool a call is for, or null. `server`: the MCP server, when the agent says it apart. */
export function agentdeckTool(name: string | undefined, server?: string): AgentdeckTool | null {
  if (!name) return null;
  if (server !== undefined) return server === "agentdeck" && isTool(name) ? name : null;
  const claude = /^mcp__agentdeck__(\w+)$/.exec(name);
  if (claude) return isTool(claude[1]!) ? claude[1] : null;
  const acp = /^(\w+) \(agentdeck MCP Server\)$/.exec(name.trim());
  if (acp) return isTool(acp[1]!) ? acp[1] : null;
  return null;
}

const str = (v: unknown) => (typeof v === "string" ? v : undefined);

/** A readable label (and what it was about) for an agentdeck tool call. */
export function agentdeckItem(tool: AgentdeckTool, input: Record<string, unknown> = {}): { label: string; target?: string } {
  const ids = Array.isArray(input.task_ids) ? input.task_ids.length : 0;
  switch (tool) {
    case "delegate_task": {
      const bot = str(input.bot)?.trim();
      const task = str(input.task);
      return { label: bot ? t("Delegated to {bot}", { bot }) : t("Delegated a task"), target: task ? firstLine(task, 120) : undefined };
    }
    case "wait_for_tasks":
      return { label: ids ? tn(ids, "Waited for {n} task", "Waited for {n} tasks") : t("Waited for other bots") };
    case "task_status":
      return { label: ids ? tn(ids, "Checked {n} task", "Checked {n} tasks") : t("Checked other bots' tasks") };
    case "list_projects":
      return { label: t("Listed the projects") };
    default:
      return { label: t("Listed the bots") };
  }
}
