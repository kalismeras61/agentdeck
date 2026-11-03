/**
 * Bridge to the desktop app's session commands. Only used inside Tauri; the web preview never
 * imports the Tauri APIs at runtime.
 */
import type { StoredEvent } from "./claude";
import { isTauri } from "./native";
import { t } from "./i18n";

export interface LiveProject {
  id: string;
  name: string;
  path: string;
  /** "chats": where sessions without a project go. */
  kind: "folder" | "chats";
  createdAt: number;
  /** A bot's own workspace: its sessions work there, but it's never listed as a project. */
  hidden?: boolean;
}

export interface LiveSessionRow {
  id: string;
  projectId: string;
  provider: LiveProvider;
  model: string;
  title: string;
  status: "idle" | "running" | "waiting" | "error";
  snippet: string;
  providerSessionId: string | null;
  createdAt: number;
  updatedAt: number;
  mode: SessionMode;
  profileId: string | null;
  /** A git worktree the session works in instead of the project folder. */
  worktree?: string | null;
  branch?: string | null;
  /** Reasoning effort picked for the session; null = the agent's own default. */
  effort?: string | null;
}

export type SessionMode = "default" | "plan";
export type LiveProvider = "claude-code" | "codex" | "gemini-cli" | "grok-build" | "kimi-code" | "antigravity" | "api";

export type AgyApprovals = "edits" | "auto" | "all";

/* git */
export type GitBranch = { name: string; hash: string; current: boolean; upstream: string | null; ahead: number; behind: number; worktree: string | null };
export type GitWorktree = { path: string; branch: string | null; hash: string; main: boolean; locked: boolean; prunable: boolean };
export type GitFileStatus = { path: string; change: "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflict"; staged: boolean; from: string | null };
export type GitOverview = {
  root: string;
  head: string | null;
  branch: string | null;
  branches: GitBranch[];
  remotes: GitBranch[];
  tags: string[];
  worktrees: GitWorktree[];
  status: GitFileStatus[];
};
export type GitCommit = { hash: string; parents: string[]; author: string; email: string; time: number; subject: string; refs: string[] };
export type GitChangedFile = { path: string; change: "added" | "modified" | "deleted" | "renamed"; from: string | null };
export type GitFilePair = { old: string; new: string; binary: boolean };
export type WorktreeState = {
  path: string;
  branch: string;
  base: string | null;
  exists: boolean;
  ahead: number;
  behind: number;
  uncommitted: GitFileStatus[];
  files: GitChangedFile[];
};
export type MergeResult = { merged: boolean; base: string; conflicts: string[]; message: string };

export const gitApi = {
  overview: (projectId: string, dir?: string | null) => call<GitOverview>("git_overview", { projectId, dir: dir ?? null }),
  log: (projectId: string, skip: number, limit: number) => call<GitCommit[]>("git_log", { projectId, skip, limit }),
  commitFiles: (projectId: string, hash: string) => call<GitChangedFile[]>("git_commit_files", { projectId, hash }),
  /** newRev "WORKTREE" = the file on disk in dir. */
  filePair: (projectId: string, dir: string | null, oldRev: string, newRev: string, path: string, from?: string | null) =>
    call<GitFilePair>("git_file_pair", { projectId, dir, oldRev, newRev, path, from: from ?? null }),
  sessionWorktree: (sessionId: string) => call<WorktreeState>("session_worktree", { sessionId }),
  /** Before the first message only. */
  setSessionWorktree: (sessionId: string, on: boolean) => call<LiveSessionRow>("set_session_worktree", { sessionId, on }),
  merge: (sessionId: string) => call<MergeResult>("merge_session_worktree", { sessionId }),
  remove: (sessionId: string, deleteBranch: boolean) => call<LiveSessionRow>("remove_session_worktree", { sessionId, deleteBranch }),
};

/** A file that shapes an agent: instructions (CLAUDE.md…), a skill, a slash command or a subagent. */
export type ContextItem = {
  kind: "instructions" | "skill" | "command" | "subagent";
  scope: "project" | "global";
  agents: LiveProvider[];
  name: string;
  description?: string | null;
  path: string;
  exists: boolean;
  size: number;
  modified: number;
  linked: boolean;
};
export type ContextFile = { path: string; text: string; modified: number; exists: boolean };

/**
 * One model an agent offers. "default" leaves the agent's own setting.
 * efforts: the reasoning efforts it takes, lowest first (absent: no effort setting).
 * defaultEffort: the one the agent uses when none is picked, if it says.
 */
export type ModelChoice = { id: string; label: string; description?: string; efforts?: string[]; defaultEffort?: string };
/** What a session's agent says about its effort. effort: in use now (null: not running or unknown). env: [name, value] of a variable that overrides any pick. */
export type EffortStatus = { running: boolean; effort: string | null; env: [string, string] | null };
/** source: "agent" = the agent reported it this run, "saved" = from an earlier run, "builtin" = couldn't ask. */
export type ModelList = { provider: string; models: ModelChoice[]; source: "agent" | "saved" | "builtin"; error?: string };

/** A custom model endpoint. The key never comes back from the desktop app, only whether one is saved. */
export interface ModelProfile {
  id: string;
  name: string;
  kind: "anthropic" | "openai";
  baseUrl: string;
  model: string;
  smallModel: string;
  createdAt: number;
  hasKey?: boolean;
}

export interface RelayRow {
  id: string;
  aSession: string;
  bSession: string;
  aRole: string;
  bRole: string;
  maxRounds: number;
  rounds: number;
  status: "active" | "done" | "stopped";
  reason: string | null;
  createdAt: number;
  updatedAt: number;
}

/** A saved workflow. An agent is a provider id or "profile:<id>"; "" skips the stage. */
export interface Workflow {
  id: string;
  name: string;
  planAgent: string;
  buildAgent: string;
  reviewAgent: string;
  gatePlan: boolean;
  maxReviews: number;
  createdAt: number;
}

export type FlowStage = "plan" | "approve" | "build" | "review";

export interface FlowRow {
  id: string;
  projectId: string;
  name: string;
  task: string;
  planSession: string | null;
  buildSession: string;
  reviewSession: string | null;
  gatePlan: boolean;
  maxReviews: number;
  reviews: number;
  stage: FlowStage;
  status: "active" | "done" | "stopped";
  reason: string | null;
  plan: string | null;
  createdAt: number;
  updatedAt: number;
}

/** Starting a workflow from a session; "this" in a stage means that session. */
export interface StartFlow {
  sessionId: string;
  name: string;
  task: string;
  planAgent: string;
  buildAgent: string;
  reviewAgent: string;
  gatePlan: boolean;
  maxReviews: number;
  /** All stages work in one new git worktree. */
  worktree?: boolean;
}

/** Accent names a bot's mark can have. */
export type BotColor = "violet" | "blue" | "green" | "amber" | "rose" | "teal";

/** A bot: an agent that stays, with a job, its own workspace, a memory file and a queue of tasks. */
export interface Bot {
  id: string;
  name: string;
  role: string;
  instructions: string;
  provider: LiveProvider;
  model: string;
  effort: string | null;
  profileId: string | null;
  /** Where a task works when it names no place; null: its workspace. */
  projectId: string | null;
  /** The hidden project that is its workspace, and that folder (~/AgentDeck/Bots/<Name>). */
  workspaceId: string | null;
  workspace: string;
  /** Its ongoing session in each place it has worked (projectId null: the workspace). */
  places: BotPlace[];
  color: BotColor;
  paused: boolean;
  createdAt: number;
  updatedAt: number;
  /** Tasks waiting to start. */
  queued: number;
  runningTaskId: string | null;
  runningText: string | null;
  /** Where the running task works (null: the workspace) and its session. */
  runningProjectId: string | null;
  runningSessionId: string | null;
  /** The soonest run of its schedules that are on. */
  nextRunAt: number | null;
  /** A request or a question is waiting for you. */
  needsYou: boolean;
}

export interface BotPlace {
  /** null: the bot's workspace. */
  projectId: string | null;
  sessionId: string;
}

export type BotTaskStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export interface BotTask {
  id: string;
  botId: string;
  text: string;
  /** "you", "schedule", "bot:<id>" (another bot handed it over; later "web", "telegram"). */
  source: string;
  status: BotTaskStatus;
  result: string | null;
  error: string | null;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  parentId: string | null;
  /** The project it works in; null: the bot's workspace. */
  projectId: string | null;
  /** The session it ran in, once started. */
  sessionId: string | null;
  /** How many tasks it handed to other bots. */
  children: number;
}

/** What the bot editor saves; no id makes a new bot. */
export interface BotInput {
  id?: string;
  name: string;
  role: string;
  instructions: string;
  provider: LiveProvider;
  model: string;
  effort: string | null;
  profileId: string | null;
  /** Its default project; null: tasks work in its workspace unless they name a project. */
  projectId: string | null;
  color: BotColor;
}

export interface BotMemory {
  path: string;
  text: string;
}

/** A task that repeats: daily at times (local "HH:MM") on chosen weekdays, or every N minutes. */
export interface BotSchedule {
  id: string;
  botId: string;
  text: string;
  /** null: the bot's workspace. */
  projectId: string | null;
  kind: "daily" | "interval";
  times: string[];
  /** 1 (Monday) to 7 (Sunday); empty: every day. */
  weekdays: number[];
  everyMinutes: number | null;
  enabled: boolean;
  lastRunAt: number | null;
  nextRunAt: number | null;
  lastTaskId: string | null;
  /** When it last skipped a run because its previous task hadn't finished. */
  skippedAt: number | null;
  createdAt: number;
}

/** What the schedule editor saves; no id makes a new one. */
export interface BotScheduleInput {
  id?: string;
  botId: string;
  text: string;
  projectId: string | null;
  kind: "daily" | "interval";
  times: string[];
  weekdays: number[];
  everyMinutes: number | null;
  enabled: boolean;
}

/** A session an agent saved on its own, found by scanning its history folder. */
export interface ImportFound {
  provider: "claude-code" | "codex" | "gemini-cli";
  agentId: string;
  file: string;
  title: string;
  cwd: string | null;
  folderExists: boolean;
  messages: number;
  startedAt: number;
  updatedAt: number;
  /** The AgentDeck session it already is. */
  existing: string | null;
}

export interface StartRelay {
  sessionId: string;
  partnerSessionId?: string;
  partnerProvider?: LiveProvider;
  partnerProfileId?: string;
  task: string;
  role: string;
  partnerRole: string;
  maxRounds: number;
}

export interface LiveDelta {
  sessionId: string;
  text: string;
  reset: boolean;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const liveApi = {
  available: isTauri,
  listProjects: () => call<LiveProject[]>("list_projects"),
  addProject: (path: string) => call<LiveProject>("add_project", { path }),
  /** ~/AgentDeck/Chats, made on first use. */
  openChats: () => call<LiveProject>("open_chats"),
  /** Makes ~/AgentDeck/<name> and adds it. */
  createProject: (name: string) => call<LiveProject>("create_project", { name }),
  moveSession: (sessionId: string, projectId: string) => call<LiveSessionRow>("move_session", { sessionId, projectId }),
  renameSession: (sessionId: string, title: string) => call<LiveSessionRow>("rename_session", { sessionId, title }),
  /** Which agents have an API key saved in AgentDeck (never the keys). */
  agentKeys: () => call<Record<string, boolean>>("agent_keys"),
  /** Saves an agent's API key in the Keychain; "" removes it. Returns whether one is saved. */
  setAgentKey: (provider: string, key: string) => call<boolean>("set_agent_key", { provider, key }),
  scanImports: () => call<ImportFound[]>("scan_imports"),
  importSessions: (picks: { provider: string; file: string }[]) =>
    call<{ sessions: LiveSessionRow[]; projects: LiveProject[]; skipped: number }>("import_sessions", { picks }),
  removeProject: (projectId: string) => call<void>("remove_project", { projectId }),
  listSessions: () => call<LiveSessionRow[]>("list_sessions"),
  createSession: (projectId: string, provider: LiveProvider, model: string, profileId?: string, worktree?: boolean, effort?: string | null) =>
    call<LiveSessionRow>("create_session", { projectId, provider, model, profileId: profileId ?? null, worktree: worktree ?? null, effort: effort ?? null }),
  listProfiles: () => call<ModelProfile[]>("list_profiles"),
  /** key: undefined keeps the saved key, "" removes it. */
  saveProfile: (profile: ModelProfile, key?: string) => call<ModelProfile>("save_profile", { profile, key: key ?? null }),
  deleteProfile: (profileId: string) => call<void>("delete_profile", { profileId }),
  testProfile: (profileId: string) => call<string>("test_profile", { profileId }),
  /** Changes the agent of a session that has no messages yet. */
  setAgent: (sessionId: string, provider: LiveProvider, model: string, profileId?: string, effort?: string | null) =>
    call<LiveSessionRow>("set_session_agent", { sessionId, provider, model, profileId: profileId ?? null, effort: effort ?? null }),
  setModel: (sessionId: string, model: string) => call<LiveSessionRow>("set_session_model", { sessionId, model }),
  /** null = the agent's default. A running Claude Code switches at once. */
  setEffort: (sessionId: string, effort: string | null) => call<LiveSessionRow>("set_session_effort", { sessionId, effort }),
  /** The effort the session's agent uses right now. */
  sessionEffort: (sessionId: string) => call<EffortStatus>("session_effort", { sessionId }),
  scanContext: (projectId: string | null) => call<ContextItem[]>("scan_context", { projectId }),
  readContextFile: (path: string) => call<ContextFile>("read_context_file", { path }),
  /** expectedModified: what was loaded (0 = new file); refused if it changed on disk since. */
  saveContextFile: (path: string, text: string, expectedModified: number) => call<ContextFile>("save_context_file", { path, text, expectedModified }),
  newContextItem: (projectId: string | null, kind: string, scope: string, agent: string, name: string) =>
    call<ContextFile>("new_context_item", { projectId, kind, scope, agent, name }),
  /** The models an agent offers, as the agent reports them. refresh asks the agent again. */
  listModels: (provider: string, refresh = false) => call<ModelList>("list_models", { provider, refresh }),
  loadEvents: (sessionId: string) => call<StoredEvent[]>("load_events", { sessionId }),
  /** files: absolute paths the user dropped, pasted or picked. */
  send: (sessionId: string, text: string, files: string[] = []) => call<void>("send_message", { sessionId, text, files: files.length ? files : null }),
  /** Keeps a pasted image (base64, no data: prefix) in AgentDeck's data folder; returns its path. */
  stashImage: (name: string, data: string) => call<string>("stash_pasted_image", { name, data }),
  /** The system clipboard's image saved as a file, or null when it holds none. */
  pasteClipboardImage: () => call<string | null>("paste_clipboard_image"),
  respond: (sessionId: string, requestId: string, decision: string) => call<void>("respond_permission", { sessionId, requestId, decision }),
  interrupt: (sessionId: string) => call<void>("interrupt_session", { sessionId }),
  stop: (sessionId: string) => call<void>("stop_session", { sessionId }),
  /** How much Antigravity may do without asking: "edits" | "auto" | "all". */
  antigravityApprovals: () => call<AgyApprovals>("antigravity_approvals"),
  setAntigravityApprovals: (level: AgyApprovals, sessionId?: string) => call<AgyApprovals>("set_antigravity_approvals", { level, sessionId: sessionId ?? null }),
  /** Adds allow rules to Antigravity's settings (permissions.allow); returns the file. */
  allowAntigravityRules: (rules: string[]) => call<string>("allow_antigravity_rules", { rules }),
  /** Restarts the agent and sends the last message again (after signing in). */
  retryLast: (sessionId: string) => call<void>("retry_last_message", { sessionId }),
  /** Answers an agent's question; null skips it. */
  answerQuestion: (sessionId: string, requestId: string, answers: Record<string, string[]> | null) =>
    call<void>("answer_question", { sessionId, requestId, answers }),
  remove: (sessionId: string) => call<void>("delete_session", { sessionId }),
  startRelay: (r: StartRelay) =>
    call<RelayRow>("start_relay", {
      sessionId: r.sessionId,
      partnerSessionId: r.partnerSessionId ?? null,
      partnerProvider: r.partnerProvider ?? null,
      partnerProfileId: r.partnerProfileId ?? null,
      task: r.task,
      role: r.role,
      partnerRole: r.partnerRole,
      maxRounds: r.maxRounds,
    }),
  stopRelay: (relayId: string) => call<RelayRow>("stop_relay", { relayId }),
  listWorkflows: () => call<Workflow[]>("list_workflows"),
  saveWorkflow: (workflow: Workflow) => call<Workflow>("save_workflow", { workflow }),
  deleteWorkflow: (id: string) => call<void>("delete_workflow", { id }),
  listFlows: () => call<FlowRow[]>("list_flows"),
  startFlow: (f: StartFlow) => call<FlowRow>("start_flow", { ...f }),
  stopFlow: (flowId: string) => call<FlowRow>("stop_flow", { flowId }),
  approveFlowPlan: (flowId: string) => call<FlowRow>("approve_flow_plan", { flowId }),
  reviseFlowPlan: (flowId: string, notes: string) => call<FlowRow>("revise_flow_plan", { flowId, notes }),
  listBots: () => call<Bot[]>("list_bots"),
  saveBot: (bot: BotInput) => call<Bot>("save_bot", { bot }),
  deleteBot: (id: string) => call<void>("delete_bot", { id }),
  /** Queued and running tasks, then up to `limit` finished ones (newest first). */
  botTasks: (botId: string, limit = 50) => call<BotTask[]>("bot_tasks", { botId, limit }),
  /** projectId null: the task works in the bot's workspace. */
  addBotTask: (botId: string, text: string, projectId: string | null) => call<BotTask>("add_bot_task", { botId, text, projectId }),
  /** What a task handed to other bots, oldest first. */
  botTaskChildren: (taskId: string) => call<BotTask[]>("bot_task_children", { taskId }),
  cancelBotTask: (id: string) => call<BotTask>("cancel_bot_task", { id }),
  retryBotTask: (id: string) => call<BotTask>("retry_bot_task", { id }),
  setBotPaused: (id: string, paused: boolean) => call<Bot>("set_bot_paused", { id, paused }),
  /** The bot's next task starts a new session. */
  restartBotSession: (id: string) => call<Bot>("restart_bot_session", { id }),
  readBotMemory: (id: string) => call<BotMemory>("read_bot_memory", { id }),
  writeBotMemory: (id: string, text: string) => call<BotMemory>("write_bot_memory", { id, text }),
  botSchedules: (botId: string) => call<BotSchedule[]>("bot_schedules", { botId }),
  /** Its next run is counted from now. */
  saveBotSchedule: (schedule: BotScheduleInput) => call<BotSchedule>("save_bot_schedule", { schedule }),
  deleteBotSchedule: (id: string) => call<void>("delete_bot_schedule", { id }),
  setBotScheduleEnabled: (id: string, enabled: boolean) => call<BotSchedule>("set_bot_schedule_enabled", { id, enabled }),
  /** Adds the schedule's task now; its next run stays. */
  runBotSchedule: (id: string) => call<BotTask>("run_bot_schedule", { id }),
  listRelays: () => call<RelayRow[]>("list_relays"),
  setMode: (sessionId: string, mode: SessionMode) => call<LiveSessionRow>("set_session_mode", { sessionId, mode }),
  handOff: (fromSessionId: string, provider: LiveProvider, plan: string, planRequestId?: string, profileId?: string) =>
    call<LiveSessionRow>("hand_off", { fromSessionId, provider, profileId: profileId ?? null, plan, planRequestId: planRequestId ?? null }),
};

export interface TermInfo {
  termId: string;
  cwd: string;
  shell: string;
  running: boolean;
  scrollback: string;
}

export const termApi = {
  open: (projectId: string, cols: number, rows: number, sessionId?: string) => call<TermInfo>("term_open", { projectId, sessionId: sessionId ?? null, cols, rows }),
  info: (termId: string) => call<TermInfo>("term_info", { termId }),
  write: (termId: string, data: string) => call<void>("term_write", { termId, data }),
  resize: (termId: string, cols: number, rows: number) => call<void>("term_resize", { termId, cols, rows }),
  close: (termId: string) => call<void>("term_close", { termId }),
  /** Runs an agent's own sign-in ("codex login", "gemini", "agy"…) in this terminal; returns the command. */
  signIn: (termId: string, agent: string) => call<string>("term_sign_in", { termId, agent }),
  /** Opens an http(s) link in the default browser. */
  openLink: (url: string) => call<void>("open_link", { url }),
};

export async function subscribeTerm(handlers: {
  onOutput: (termId: string, data: string) => void;
  onExit: (termId: string, code: number | null) => void;
}): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const offs = await Promise.all([
    listen<{ termId: string; data: string }>("agentdeck://term-output", (e) => handlers.onOutput(e.payload.termId, e.payload.data)),
    listen<{ termId: string; code: number | null }>("agentdeck://term-exit", (e) => handlers.onExit(e.payload.termId, e.payload.code)),
  ]);
  return () => offs.forEach((off) => off());
}

/** Files dragged over or dropped on the window (desktop app only; paths are absolute). */
export async function subscribeFileDrop(h: { onHover: (over: boolean) => void; onDrop: (paths: string[]) => void }): Promise<() => void> {
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((e) => {
    const p = e.payload;
    if (p.type === "enter") h.onHover(p.paths.length > 0);
    else if (p.type === "leave") h.onHover(false);
    else if (p.type === "drop") {
      h.onHover(false);
      if (p.paths.length) h.onDrop(p.paths);
    }
  });
}

export async function pickFiles(): Promise<string[]> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: true, directory: false, title: t("Attach files") });
  return Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
}

export async function pickFolder(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title: t("Choose a project folder") });
  return typeof picked === "string" ? picked : null;
}

export async function subscribeLive(handlers: {
  onEvent: (e: StoredEvent) => void;
  onSession: (s: LiveSessionRow) => void;
  onDelta: (d: LiveDelta) => void;
  onRelay: (r: RelayRow) => void;
  onFlow: (f: FlowRow) => void;
  onBot: (b: Bot) => void;
  onBotTask: (t: BotTask) => void;
  onBotSchedule: (s: BotSchedule) => void;
}): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const offs = await Promise.all([
    listen<StoredEvent>("agentdeck://event", (e) => handlers.onEvent(e.payload)),
    listen<LiveSessionRow>("agentdeck://session", (e) => handlers.onSession(e.payload)),
    listen<LiveDelta>("agentdeck://delta", (e) => handlers.onDelta(e.payload)),
    listen<RelayRow>("agentdeck://relay", (e) => handlers.onRelay(e.payload)),
    listen<FlowRow>("agentdeck://flow", (e) => handlers.onFlow(e.payload)),
    listen<Bot>("agentdeck://bot", (e) => handlers.onBot(e.payload)),
    listen<BotTask>("agentdeck://bot-task", (e) => handlers.onBotTask(e.payload)),
    listen<BotSchedule>("agentdeck://bot-schedule", (e) => handlers.onBotSchedule(e.payload)),
  ]);
  return () => offs.forEach((off) => off());
}

/** "Couldn't start…" style text from a rejected invoke. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return t("Something went wrong talking to the desktop app.");
}
