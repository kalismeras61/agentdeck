import { t, type LocaleId } from "../lib/i18n";
import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";
import { demoBots, demoBotSchedules, demoBotTasks, demoProjects, demoSessions, providerModels, type Project, type Session, type WorkspaceId } from "../data/demo";
import type { StoredEvent } from "../lib/claude";
import { normalizeFor } from "../lib/normalize";
import type { Bot, BotInput, BotSchedule, BotScheduleInput, BotTask, FlowRow, ModelProfile, RelayRow, StartFlow, StartRelay, Workflow } from "../lib/live";
import { childrenOf, mergeSchedule, mergeTask, soonest } from "../lib/bots";
import type { ProviderId } from "../lib/events";
import { botExtrasApi, connectionsApi, errorText, liveApi, pickFolder, remoteApi, type LiveProject, type LiveProvider, type LiveSessionRow, type BotOptions } from "../lib/live";

export type AppView = "workspace" | "overview" | "review" | "permissions" | "gallery" | "settings" | "setup" | "git" | "team" | "welcome" | "bot";
export type ContextTab = "files" | "changes" | "preview" | "activity";
export type ViewMode = "rich" | "terminal";
export type ThemePref = "dark" | "light" | "system";
export type MotionPref = "system" | "reduced" | "full";
export type PermissionDecision = "allow" | "allow_session" | "deny";
export type ReviewLayout = "split" | "unified";

export const SIDEBAR = { min: 220, max: 360, def: 260 };
export const CONTEXT = { min: 300, max: 520, def: 340 };

export interface NotificationPrefs {
  approval: boolean;
  finished: boolean;
  errors: boolean;
}

interface UiState {
  view: AppView;
  workspaceId: WorkspaceId;
  projects: Project[];
  sessions: Session[];
  /** Stored provider log per live session; `Session.raw` is derived from it. */
  liveEvents: Record<string, StoredEvent[]>;
  /** Reply text streaming in right now, per live session. */
  streaming: Record<string, string>;
  /** Last error from an action on a session (send, approve, stop), shown near the message box. */
  sessionErrors: Record<string, string | undefined>;
  liveState: "unavailable" | "loading" | "ready" | "error";
  liveError: string | null;
  selectedId: string | null;
  sidebarOpen: boolean;
  sidebarWidth: number;
  contextOpen: boolean;
  contextWidth: number;
  contextTab: ContextTab;
  /** True when the window is too narrow to dock the context panel; it then floats over the conversation. */
  narrow: boolean;
  contextOverlay: boolean;
  selectedChangePath: string | null;
  viewMode: ViewMode;
  search: string;
  theme: ThemePref;
  /** Interface language: "system" follows the Mac. */
  language: "system" | LocaleId;
  motion: MotionPref;
  /** Permission requests sent to the provider and awaiting its confirmation. */
  permissionInFlight: Record<string, PermissionDecision>;
  composerFocusTick: number;
  drafts: Record<string, string>;
  searchFocusTick: number;
  reviewSessionId: string | null;
  reviewPath: string | null;
  reviewLayout: ReviewLayout;
  /** Local review marks, keyed `${sessionId}::${path}`. UI metadata only: nothing is committed or approved. */
  viewed: Record<string, true>;
  defaultProvider: ProviderId;
  defaultModels: Partial<Record<ProviderId, string>>;
  /** Reasoning effort for new sessions, per agent; absent = the agent's default. */
  defaultEfforts: Partial<Record<ProviderId, string>>;
  notifications: NotificationPrefs;
}

interface Actions {
  setView: (v: AppView) => void;
  select: (id: string) => void;
  /** Loads a session's history if it hasn't been loaded yet (sessions load when opened). */
  ensureEvents: (sessionId: string) => Promise<void>;
  setWorkspace: (id: WorkspaceId) => void;
  toggleSidebar: () => void;
  setSidebarWidth: (w: number) => void;
  toggleContext: () => void;
  setNarrow: (n: boolean) => void;
  setContextWidth: (w: number) => void;
  setContextTab: (t: ContextTab) => void;
  openChange: (path: string) => void;
  openReview: (sessionId: string, path?: string) => void;
  setReviewPath: (path: string) => void;
  setReviewSession: (sessionId: string) => void;
  setReviewLayout: (l: ReviewLayout) => void;
  toggleViewed: (sessionId: string, path: string) => void;
  setViewMode: (m: ViewMode) => void;
  setSearch: (q: string) => void;
  setTheme: (t: ThemePref) => void;
  setLanguage: (l: "system" | LocaleId) => void;
  setMotion: (m: MotionPref) => void;
  setDefaultProvider: (p: ProviderId) => void;
  setDefaultModel: (p: ProviderId, model: string) => void;
  /** null: new sessions use the agent's default effort. */
  setDefaultEffort: (p: ProviderId, effort: string | null) => void;
  setNotification: (k: keyof NotificationPrefs, v: boolean) => void;
  resetPreferences: () => void;
  /** worktree: the session works in its own new git worktree of the project. */
  newSession: (projectId?: string, provider?: ProviderId, profileId?: string, worktree?: boolean) => void;
  /** Project (and folder: a worktree, or null for the project folder) shown in the Git view. */
  gitTarget: { projectId: string | null; dir: string | null };
  /** Unread team messages (Team view), for the sidebar. */
  teamUnread: number;
  setTeamUnread: (n: number) => void;
  /** Bumped when a relay brings news, so an open Team view reloads. */
  teamTick: number;
  openGit: (projectId: string | null, dir?: string | null) => void;
  /** A session without a project: it runs in ~/AgentDeck/Chats. */
  newChat: (provider?: ProviderId, profileId?: string) => void;
  /** Makes ~/AgentDeck/<name>, adds it and starts a session there. */
  createProject: (name: string) => Promise<string | null>;
  moveSession: (sessionId: string, projectId: string) => Promise<string | null>;
  /** Antigravity sessions are offered only when turned on (always in the demo). */
  agyEnabled: boolean;
  setAgyEnabled: (v: boolean) => void;
  /** Renames a session; returns an error to show, or null. */
  renameSession: (sessionId: string, title: string) => Promise<string | null>;
  /** The session whose name is being edited, and where. */
  renaming: { id: string; at: "header" | "sidebar" } | null;
  setRenaming: (r: { id: string; at: "header" | "sidebar" } | null) => void;
  /** Re-reads projects and sessions from the desktop app (after an import). */
  reloadLive: () => Promise<void>;
  importOpen: boolean;
  /** Project shown in Agent setup; null = the selected session's project. */
  setupProjectId: string | null;
  openSetup: (projectId: string | null) => void;
  setImportOpen: (v: boolean) => void;
  newProjectOpen: boolean;
  setNewProjectOpen: (v: boolean) => void;
  /** Custom model profiles (desktop only). */
  profiles: ModelProfile[];
  loadProfiles: () => Promise<void>;
  setModel: (sessionId: string, model: string) => void;
  /** null: the agent's default. A running Claude Code switches at once. */
  setEffort: (sessionId: string, effort: string | null) => void;
  /** Picks another agent for a session that hasn't started yet. */
  setSessionAgent: (sessionId: string, provider: LiveProvider, profileId?: string) => void;
  sendPrompt: (sessionId: string, text: string, attachments: string[]) => void;
  resolvePermission: (sessionId: string, requestId: string, decision: PermissionDecision) => void;
  focusComposer: () => void;
  setDraft: (sessionId: string, text: string) => void;
  interrupt: (sessionId: string) => void;
  setSessionMode: (sessionId: string, mode: "default" | "plan") => void;
  /** Starts a linked session for `provider` with this plan as its first task. */
  handOff: (fromSessionId: string, provider: LiveProvider, plan: string, planRequestId?: string, profileId?: string) => Promise<void>;
  handOffInFlight: Record<string, true>;
  /** Open terminal per session (desktop only). */
  terminals: Record<string, string>;
  /** A sign-in running in a session's terminal: which agent's login, and whether it was started. */
  signIns: Record<string, { agent: string; started: boolean }>;
  startSignIn: (sessionId: string, agent: string) => void;
  markSignInStarted: (sessionId: string) => void;
  /** Ends a sign-in; with retry, the agent restarts and gets the last message again. */
  finishSignIn: (sessionId: string, retry: boolean) => void;
  setTerminal: (sessionId: string, termId: string | null) => void;
  /** Conversations in Remote Control: its terminal, and the shell it took the place of. */
  remotes: Record<string, { termId: string; prev: string | null }>;
  /** Hands a Claude Code conversation to Remote Control and shows its terminal (QR code). */
  startRemote: (sessionId: string) => Promise<void>;
  /** Ends Remote Control; what was said there comes back into the conversation. */
  endRemote: (sessionId: string) => void;
  /** Back from a Remote Control terminal to the conversation. */
  leaveRemote: (sessionId: string) => void;
  /** Relays by id (live only, not persisted here: the desktop app owns them). */
  relays: Record<string, RelayRow>;
  upsertRelay: (r: RelayRow) => void;
  startRelay: (r: StartRelay) => Promise<string | null>;
  stopRelay: (relayId: string) => void;
  /** Rich view of a relay: this session only, or both sides interleaved. */
  relayView: "both" | "one";
  /** Workflow runs by id (live only; the desktop app owns them). */
  flows: Record<string, FlowRow>;
  upsertFlow: (f: FlowRow) => void;
  startFlow: (f: StartFlow) => Promise<string | null>;
  stopFlow: (flowId: string) => void;
  approveFlowPlan: (flowId: string) => Promise<string | null>;
  reviseFlowPlan: (flowId: string, notes: string) => Promise<string | null>;
  /** Saved workflow templates. */
  workflows: Workflow[];
  loadWorkflows: () => Promise<void>;
  saveWorkflow: (w: Workflow) => Promise<string | null>;
  deleteWorkflow: (id: string) => Promise<void>;
  /** Session whose "Work with another agent" dialog is open. */
  relayDialogFor: string | null;
  openRelayDialog: (sessionId: string | null) => void;
  setRelayView: (v: "both" | "one") => void;
  stopProcess: (sessionId: string) => void;
  deleteSession: (sessionId: string) => void;
  focusSearch: () => void;
  clearSessionError: (sessionId: string) => void;
  /* live bridge */
  hydrateLive: (projects: LiveProject[], rows: LiveSessionRow[], events: Record<string, StoredEvent[]>) => void;
  setLiveState: (s: UiState["liveState"], error?: string | null) => void;
  upsertLiveSession: (row: LiveSessionRow) => void;
  appendLiveEvent: (e: StoredEvent) => void;
  applyDelta: (sessionId: string, text: string, reset: boolean) => void;
  addProjectFolder: () => Promise<void>;
  /** Takes a project out of AgentDeck (its sessions too); the folder isn't touched. */
  removeProject: (projectId: string) => Promise<void>;
  /* bots */
  /** Bots by id: the desktop app's, plus the labeled demo ones. */
  bots: Record<string, Bot>;
  /** Each bot's queued, running and recent tasks. */
  botTasks: Record<string, BotTask[]>;
  /** Tasks a task handed to other bots, by the handing task's id (loaded when shown). */
  taskChildren: Record<string, BotTask[]>;
  loadTaskChildren: (parentId: string) => Promise<void>;
  /** The bot whose page is open. */
  selectedBotId: string | null;
  /** The bot editor: a bot to edit (null: a new one), and a default project to start with. */
  botEditor: { botId: string | null; projectId: string | null } | null;
  /** Each bot's schedules. */
  botSchedules: Record<string, BotSchedule[]>;
  /** The place shown on each bot's page (null: its workspace; absent: where it works now). */
  botPlace: Record<string, string | null>;
  setBotPlace: (botId: string, projectId: string | null) => void;
  upsertBotSchedule: (s: BotSchedule) => void;
  loadBotSchedules: (botId: string) => Promise<void>;
  /** Returns an error to show, or null. */
  saveBotSchedule: (input: BotScheduleInput) => Promise<string | null>;
  deleteBotSchedule: (id: string) => Promise<string | null>;
  setBotScheduleEnabled: (id: string, enabled: boolean) => Promise<string | null>;
  /** Adds the schedule's task now. */
  runBotSchedule: (id: string) => Promise<string | null>;
  /** Last error from an action on a bot, shown on its page. */
  botErrors: Record<string, string | undefined>;
  openBot: (botId: string) => void;
  openBotEditor: (botId: string | null, projectId?: string | null) => void;
  closeBotEditor: () => void;
  upsertBot: (b: Bot) => void;
  upsertBotTask: (t: BotTask) => void;
  loadBots: () => Promise<void>;
  loadBotTasks: (botId: string) => Promise<void>;
  /** Returns an error to show, or null. */
  /** connections: the bot's connections (ids), set after it's saved. */
  saveBot: (input: BotInput, connections?: string[], options?: BotOptions) => Promise<string | null>;
  deleteBot: (botId: string) => Promise<string | null>;
  /** projectId null: the task works in the bot's workspace. */
  addBotTask: (botId: string, text: string, projectId: string | null, source?: string) => Promise<string | null>;
  cancelBotTask: (taskId: string) => void;
  retryBotTask: (taskId: string) => void;
  setBotPaused: (botId: string, paused: boolean) => void;
  restartBotSession: (botId: string) => void;
  clearBotError: (botId: string) => void;
}

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/** localStorage can be missing or throw (private windows, previews). UI prefs are a convenience only. */
const safeStorage: StateStorage = {
  getItem: (k) => {
    try {
      return window.localStorage.getItem(k);
    } catch {
      return null;
    }
  },
  setItem: (k, v) => {
    try {
      window.localStorage.setItem(k, v);
    } catch {
      /* ignore */
    }
  },
  removeItem: (k) => {
    try {
      window.localStorage.removeItem(k);
    } catch {
      /* ignore */
    }
  },
};

let localSeq = 0;
const localId = (prefix: string) => `${prefix}-${Date.now().toString(36)}-${(localSeq++).toString(36)}`;

function appendEvents(s: Session, events: unknown[], patch: Partial<Session> = {}): Session {
  return { ...s, ...patch, raw: [...s.raw, ...events], updatedAt: Date.now() };
}

function shortCommand(cmd: string): string {
  const words = cmd.trim().split(/\s+/);
  const pick = words[0] === "dart" || words[0] === "flutter" || words[0] === "npx" ? words.slice(0, 3) : words.slice(0, 2);
  return pick.join(" ");
}

/** Sessions whose history is being loaded, with events that arrived meanwhile. */
const pendingLoads = new Map<string, StoredEvent[]>();

function toProject(p: LiveProject): Project {
  return { id: p.id, name: p.name, path: p.path, workspaceId: "local", kind: p.kind === "chats" ? "chats" : "folder", ...(p.hidden ? { hidden: true } : {}) };
}

/** Projects you added (not bots' own workspaces, which are never listed). */
export function listed(p: Project): boolean {
  return !p.hidden;
}

const isDemoBot = (id: string) => id.startsWith("b-demo-");

/** A demo bot's next run after its schedules changed (the desktop app works it out for real ones). */
function demoNextRun(botId: string) {
  const st = useStore.getState();
  const b = st.bots[botId];
  if (b) st.upsertBot({ ...b, nextRunAt: soonest(st.botSchedules[botId] ?? []) });
}

function toSession(row: LiveSessionRow, events: StoredEvent[], projectPath: string): Session {
  return {
    id: row.id,
    projectId: row.projectId,
    title: row.title,
    provider: row.provider,
    model: row.model,
    status: row.status,
    snippet: row.snippet,
    startedAt: row.createdAt,
    updatedAt: row.updatedAt,
    raw: normalizeFor(row.provider, events, { sessionId: row.id, projectPath }),
    demo: false,
    live: true,
    mode: row.mode ?? "default",
    profileId: row.profileId ?? null,
    worktree: row.worktree ?? null,
    branch: row.branch ?? null,
    effort: row.effort ?? null,
  };
}

export const defaultWorkspace: WorkspaceId = liveApi.available ? "local" : "demo";

const PREF_DEFAULTS = {
  sidebarOpen: true,
  sidebarWidth: SIDEBAR.def,
  contextOpen: true,
  contextWidth: CONTEXT.def,
  theme: "dark" as ThemePref,
  language: "system" as "system" | LocaleId,
  motion: "system" as MotionPref,
  viewed: {} as Record<string, true>,
  reviewLayout: "split" as ReviewLayout,
  defaultProvider: "claude-code" as ProviderId,
  defaultModels: {} as Partial<Record<ProviderId, string>>,
  defaultEfforts: {} as Partial<Record<ProviderId, string>>,
  notifications: { approval: true, finished: true, errors: true } as NotificationPrefs,
};

export const useStore = create<UiState & Actions>()(
  persist(
    (set, get) => {
      const sessionById = (id: string) => get().sessions.find((s) => s.id === id);
      const setError = (sessionId: string, e: unknown) => set((s) => ({ sessionErrors: { ...s.sessionErrors, [sessionId]: errorText(e) } }));

      return {
        ...PREF_DEFAULTS,
        view: "workspace",
        workspaceId: defaultWorkspace,
        projects: demoProjects,
        sessions: demoSessions,
        liveEvents: {},
        streaming: {},
        sessionErrors: {},
        liveState: liveApi.available ? "loading" : "unavailable",
        liveError: null,
        selectedId: defaultWorkspace === "demo" ? demoSessions[0]!.id : null,
        contextTab: "changes",
        narrow: false,
        contextOverlay: false,
        selectedChangePath: null,
        viewMode: "rich",
        search: "",
        permissionInFlight: {},
        composerFocusTick: 0,
        drafts: {},
        searchFocusTick: 0,
        reviewSessionId: null,
        reviewPath: null,

        setView: (v) => set({ view: v }),
        select: (id) => {
          set({ selectedId: id, selectedChangePath: null, viewMode: "rich", view: "workspace" });
          void get().ensureEvents(id);
        },
        ensureEvents: async (id) => {
          const s0 = get();
          const session = s0.sessions.find((x) => x.id === id);
          if (!session?.live || s0.liveEvents[id] !== undefined || pendingLoads.has(id)) return;
          pendingLoads.set(id, []);
          let loaded: StoredEvent[] = [];
          try {
            loaded = await liveApi.loadEvents(id);
          } catch {
            /* shown as empty; the next event or open tries again */
          }
          // Events that arrived while loading are merged in by sequence number.
          const extra = pendingLoads.get(id) ?? [];
          pendingLoads.delete(id);
          const seen = new Set(loaded.map((e) => e.seq));
          const events = [...loaded, ...extra.filter((e) => !seen.has(e.seq))].sort((a, b) => a.seq - b.seq);
          set((s) => {
            const x = s.sessions.find((y) => y.id === id);
            const path = x?.worktree || (s.projects.find((p) => p.id === x?.projectId)?.path ?? "");
            return {
              liveEvents: { ...s.liveEvents, [id]: events },
              sessions: x
                ? s.sessions.map((y) => (y.id === id ? { ...y, raw: normalizeFor(y.provider, events, { sessionId: id, projectPath: path }) } : y))
                : s.sessions,
            };
          });
        },
        setWorkspace: (id) => {
          if (id === "local" && !liveApi.available) return;
          const first = get()
            .sessions.filter((s) => {
              const p = get().projects.find((x) => x.id === s.projectId);
              return p?.workspaceId === id && listed(p);
            })
            .sort((a, b) => b.updatedAt - a.updatedAt)[0];
          set({ workspaceId: id, selectedId: first?.id ?? null, view: "workspace", search: "" });
        },
        toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
        setSidebarWidth: (w) => set({ sidebarWidth: clamp(Math.round(w), SIDEBAR.min, SIDEBAR.max) }),
        toggleContext: () => set((s) => (s.narrow ? { contextOverlay: !s.contextOverlay } : { contextOpen: !s.contextOpen })),
        setNarrow: (n) => set((s) => (s.narrow === n ? s : { narrow: n, contextOverlay: false })),
        setContextWidth: (w) => set({ contextWidth: clamp(Math.round(w), CONTEXT.min, CONTEXT.max) }),
        setContextTab: (t) => set((s) => ({ contextTab: t, contextOpen: true, contextOverlay: s.narrow })),
        openChange: (path) => set((s) => ({ selectedChangePath: path, contextTab: "changes", contextOpen: true, contextOverlay: s.narrow, view: "workspace" })),
        openReview: (sessionId, path) => set({ view: "review", reviewSessionId: sessionId, reviewPath: path ?? null }),
        setReviewPath: (path) => set({ reviewPath: path }),
        setReviewSession: (sessionId) => set({ reviewSessionId: sessionId, reviewPath: null }),
        setReviewLayout: (l) => set({ reviewLayout: l }),
        toggleViewed: (sessionId, path) =>
          set((s) => {
            const key = `${sessionId}::${path}`;
            const next = { ...s.viewed };
            if (next[key]) delete next[key];
            else next[key] = true;
            return { viewed: next };
          }),
        setViewMode: (m) => set({ viewMode: m, view: "workspace" }),
        setSearch: (q) => set({ search: q }),
        setTheme: (t) => set({ theme: t }),
        setLanguage: (language) => set({ language }),
        setMotion: (m) => set({ motion: m }),
        setDefaultProvider: (p) => set({ defaultProvider: p }),
        setDefaultModel: (p, model) => set((s) => ({ defaultModels: { ...s.defaultModels, [p]: model } })),
        setDefaultEffort: (p, effort) =>
          set((s) => {
            const next = { ...s.defaultEfforts };
            if (effort) next[p] = effort;
            else delete next[p];
            return { defaultEfforts: next };
          }),
        setNotification: (k, v) => set((s) => ({ notifications: { ...s.notifications, [k]: v } })),
        resetPreferences: () => set({ ...PREF_DEFAULTS }),
        clearSessionError: (sessionId) => set((s) => ({ sessionErrors: { ...s.sessionErrors, [sessionId]: undefined } })),

        profiles: [],
        loadProfiles: async () => {
          if (!liveApi.available) return;
          try {
            set({ profiles: await liveApi.listProfiles() });
          } catch {
            /* shown in Settings when it matters */
          }
        },
        gitTarget: { projectId: null, dir: null },
        teamUnread: 0,
        setTeamUnread: (n) => set({ teamUnread: n }),
        teamTick: 0,
        openGit: (projectId, dir) => set({ view: "git", gitTarget: { projectId, dir: dir ?? null } }),
        newSession: (projectId, provider, profileId, worktree) => {
          const st = get();
          const current = st.sessions.find((s) => s.id === st.selectedId);
          const inWs = st.projects.filter((p) => p.workspaceId === st.workspaceId && listed(p));
          const pid = projectId ?? (current && inWs.some((p) => p.id === current.projectId) ? current.projectId : inWs[0]?.id);

          if (st.workspaceId === "local") {
            if (!pid) {
              get().newChat(provider, profileId);
              return;
            }
            const prov: LiveProvider = provider ?? st.defaultProvider;
            const model = profileId ? "default" : (st.defaultModels[prov] ?? "default");
            const effort = profileId ? null : (st.defaultEfforts[prov] ?? null);
            liveApi
              .createSession(pid, prov, model, profileId, worktree, effort)
              .then((row) => {
                get().upsertLiveSession(row);
                set((s) => ({ selectedId: row.id, view: "workspace", viewMode: "rich", search: "", composerFocusTick: s.composerFocusTick + 1 }));
              })
              .catch((e: unknown) => set({ liveError: errorText(e) }));
            return;
          }

          const prov = provider ?? st.defaultProvider;
          const id = localId("s-new");
          const session: Session = {
            id,
            projectId: pid ?? demoProjects[0]!.id,
            title: "New session",
            provider: prov,
            model: st.defaultModels[prov] ?? providerModels[prov][0] ?? "Default",
            effort: st.defaultEfforts[prov] ?? null,
            status: "idle",
            snippet: t("No messages yet"),
            startedAt: Date.now(),
            updatedAt: Date.now(),
            raw: [],
            demo: true,
          };
          set((s) => ({
            sessions: [session, ...s.sessions],
            selectedId: id,
            view: "workspace",
            search: "",
            viewMode: "rich",
            selectedChangePath: null,
            composerFocusTick: s.composerFocusTick + 1,
          }));
        },

        setModel: (sessionId, model) => {
          const s0 = sessionById(sessionId);
          if (s0?.live) {
            liveApi
              .setModel(sessionId, model)
              .then((row) => get().upsertLiveSession(row))
              .catch((e: unknown) => setError(sessionId, e));
            return;
          }
          set((s) => ({ sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, model } : x)) }));
        },

        setEffort: (sessionId, effort) => {
          if (sessionById(sessionId)?.live) {
            liveApi
              .setEffort(sessionId, effort)
              .then((row) => get().upsertLiveSession(row))
              .catch((e: unknown) => setError(sessionId, e));
            return;
          }
          set((s) => ({ sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, effort } : x)) }));
        },

        sendPrompt: (sessionId, text, attachments) => {
          const trimmed = text.trim();
          if (!trimmed) return;
          if (sessionById(sessionId)?.live) {
            get().clearSessionError(sessionId);
            liveApi.send(sessionId, trimmed, attachments).catch((e: unknown) => setError(sessionId, e));
            return;
          }
          set((s) => ({
            sessions: s.sessions.map((x) => {
              if (x.id !== sessionId) return x;
              const now = Date.now();
              const user = {
                eventId: localId("u"),
                sessionId,
                ts: now,
                provider: x.provider,
                type: "user_message",
                payload: { text: trimmed, attachments: attachments.length ? attachments : undefined },
              };
              const notice = {
                eventId: localId("n"),
                sessionId,
                ts: now + 1,
                provider: x.provider,
                type: "notice",
                payload: {
                  tone: "demo",
                  text: t("Demo mode: your message is saved in this view only. Switch to This Mac to run a real Claude Code session."),
                },
              };
              const isNew = x.raw.length === 0;
              return appendEvents(x, [user, notice], {
                title: isNew ? trimmed.replace(/\s+/g, " ").slice(0, 48) : x.title,
                snippet: trimmed.replace(/\s+/g, " ").slice(0, 60),
              });
            }),
          }));
        },

        resolvePermission: (sessionId, requestId, decision) => {
          if (get().permissionInFlight[requestId]) return; // ignore double clicks
          set((s) => ({ permissionInFlight: { ...s.permissionInFlight, [requestId]: decision } }));
          const done = () =>
            set((s) => {
              const { [requestId]: _done, ...rest } = s.permissionInFlight;
              void _done;
              return { permissionInFlight: rest };
            });

          if (sessionById(sessionId)?.live) {
            // Cleared when the stored decision event comes back from the desktop app (see appendLiveEvent).
            liveApi.respond(sessionId, requestId, decision).catch((e: unknown) => {
              done();
              setError(sessionId, e);
            });
            return;
          }

          // Demo: the acknowledgement is simulated so the state transition can be reviewed.
          globalThis.setTimeout(() => {
            done();
            set((s) => ({
              sessions: s.sessions.map((x) => {
                if (x.id !== sessionId) return x;
                let command = "";
                const raw = x.raw.map((e) => {
                  const r = e as { type?: string; payload?: { requestId?: string; command?: string } };
                  if (r.type === "permission" && r.payload?.requestId === requestId) {
                    command = r.payload.command ?? "";
                    const state = decision === "allow" ? "allowed" : decision === "allow_session" ? "allowed_session" : "denied";
                    return { ...(e as object), payload: { ...r.payload, state, decidedAt: Date.now() } };
                  }
                  return e;
                });
                const notice = {
                  eventId: localId("n"),
                  sessionId,
                  ts: Date.now(),
                  provider: x.provider,
                  type: "notice",
                  payload: {
                    tone: "demo",
                    text:
                      decision === "deny"
                        ? t("Demo mode: denial recorded in this view. A connected agent would be told the action did not run.")
                        : t("Demo mode: approval recorded in this view. Nothing was executed because this is sample data."),
                  },
                };
                const stillPending = raw.some((e) => {
                  const r = e as { type?: string; payload?: { state?: string } };
                  return r.type === "permission" && r.payload?.state === "pending";
                });
                return {
                  ...x,
                  raw: [...raw, notice],
                  status: stillPending ? "waiting" : "idle",
                  snippet: decision === "deny" ? t("Denied {command} (demo)", { command: shortCommand(command) }) : t("Allowed {command} (demo)", { command: shortCommand(command) }),
                  updatedAt: Date.now(),
                };
              }),
            }));
          }, 650);
        },

        handOffInFlight: {},
        relays: {},
        terminals: {},
        signIns: {},
        startSignIn: (sessionId, agent) =>
          set((s) => ({ signIns: { ...s.signIns, [sessionId]: { agent, started: false } }, selectedId: sessionId, view: "workspace", viewMode: "terminal" })),
        markSignInStarted: (sessionId) =>
          set((s) => (s.signIns[sessionId] ? { signIns: { ...s.signIns, [sessionId]: { ...s.signIns[sessionId]!, started: true } } } : {})),
        finishSignIn: (sessionId, retry) => {
          set((s) => {
            const { [sessionId]: _done, ...rest } = s.signIns;
            return { signIns: rest, viewMode: "rich" };
          });
          if (retry) {
            get().clearSessionError(sessionId);
            liveApi.retryLast(sessionId).catch((e: unknown) => setError(sessionId, e));
          }
        },
        remotes: {},
        startRemote: async (sessionId) => {
          if (!liveApi.available) {
            setError(sessionId, t("Remote Control works in the desktop app."));
            return;
          }
          try {
            const info = await remoteApi.start(sessionId, 100, 30);
            set((s) => {
              const shell = s.terminals[sessionId];
              const prev = shell && shell !== info.termId ? shell : (s.remotes[sessionId]?.prev ?? null);
              return {
                remotes: { ...s.remotes, [sessionId]: { termId: info.termId, prev } },
                terminals: { ...s.terminals, [sessionId]: info.termId },
                selectedId: sessionId,
                view: "workspace",
                viewMode: "terminal",
              };
            });
          } catch (e: unknown) {
            setError(sessionId, e);
          }
        },
        endRemote: (sessionId) => {
          remoteApi.stop(sessionId).catch((e: unknown) => setError(sessionId, e));
        },
        leaveRemote: (sessionId) =>
          set((s) => {
            const r = s.remotes[sessionId];
            if (!r) return { viewMode: "rich" };
            const { [sessionId]: _done, ...remotes } = s.remotes;
            void _done;
            const { [sessionId]: _term, ...terminals } = s.terminals;
            void _term;
            return { remotes, terminals: r.prev ? { ...terminals, [sessionId]: r.prev } : terminals, viewMode: "rich" };
          }),
        setTerminal: (sessionId, termId) =>
          set((s) => {
            const { [sessionId]: _old, ...rest } = s.terminals;
            void _old;
            return { terminals: termId ? { ...rest, [sessionId]: termId } : rest };
          }),
        flows: {},
        upsertFlow: (f) => set((s) => ({ flows: { ...s.flows, [f.id]: f } })),
        startFlow: async (f) => {
          try {
            get().upsertFlow(await liveApi.startFlow(f));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        stopFlow: (flowId) => {
          liveApi
            .stopFlow(flowId)
            .then((row) => get().upsertFlow(row))
            .catch(() => undefined);
        },
        approveFlowPlan: async (flowId) => {
          try {
            get().upsertFlow(await liveApi.approveFlowPlan(flowId));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        reviseFlowPlan: async (flowId, notes) => {
          try {
            get().upsertFlow(await liveApi.reviseFlowPlan(flowId, notes));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        workflows: [],
        loadWorkflows: async () => {
          if (!liveApi.available) return;
          try {
            set({ workflows: await liveApi.listWorkflows() });
          } catch {
            /* keeps the last list */
          }
        },
        saveWorkflow: async (w) => {
          try {
            await liveApi.saveWorkflow(w);
            await get().loadWorkflows();
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        deleteWorkflow: async (id) => {
          await liveApi.deleteWorkflow(id).catch(() => undefined);
          await get().loadWorkflows();
        },
        relayView: "both",
        relayDialogFor: null,
        openRelayDialog: (sessionId) => set({ relayDialogFor: sessionId }),
        setRelayView: (v) => set({ relayView: v }),
        upsertRelay: (r) => set((s) => ({ relays: { ...s.relays, [r.id]: r } })),
        startRelay: async (r) => {
          try {
            const row = await liveApi.startRelay(r);
            get().upsertRelay(row);
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        stopRelay: (relayId) => {
          liveApi
            .stopRelay(relayId)
            .then((row) => get().upsertRelay(row))
            .catch(() => undefined);
        },
        setSessionMode: (sessionId, mode) => {
          const s0 = sessionById(sessionId);
          if (!s0?.live) {
            set((s) => ({ sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, mode } : x)) }));
            return;
          }
          liveApi
            .setMode(sessionId, mode)
            .then((row) => get().upsertLiveSession(row))
            .catch((e: unknown) => setError(sessionId, e));
        },
        handOff: async (fromSessionId, provider, plan, planRequestId, profileId) => {
          const key = `${fromSessionId}:${planRequestId ?? plan.length}`;
          if (get().handOffInFlight[key]) return;
          set((s) => ({ handOffInFlight: { ...s.handOffInFlight, [key]: true } }));
          try {
            const row = await liveApi.handOff(fromSessionId, provider, plan, planRequestId, profileId);
            get().upsertLiveSession(row);
            get().select(row.id);
          } catch (e: unknown) {
            setError(fromSessionId, e);
          } finally {
            set((s) => {
              const { [key]: _done, ...rest } = s.handOffInFlight;
              void _done;
              return { handOffInFlight: rest };
            });
          }
        },
        interrupt: (sessionId) => {
          const s0 = sessionById(sessionId);
          if (s0?.live) {
            liveApi.interrupt(sessionId).catch((e: unknown) => setError(sessionId, e));
            return;
          }
          set((s) => ({
            sessions: s.sessions.map((x) =>
              x.id !== sessionId || x.status !== "running"
                ? x
                : appendEvents(
                    x,
                    [
                      {
                        eventId: localId("n"),
                        sessionId,
                        ts: Date.now(),
                        provider: x.provider,
                        type: "notice",
                        payload: { tone: "demo", text: t("Demo mode: interrupt recorded.") },
                      },
                    ],
                    { status: "idle", snippet: t("Interrupted") },
                  ),
            ),
          }));
        },

        stopProcess: (sessionId) => {
          if (sessionById(sessionId)?.live) liveApi.stop(sessionId).catch((e: unknown) => setError(sessionId, e));
        },

        deleteSession: (sessionId) => {
          const s0 = sessionById(sessionId);
          const drop = () =>
            set((s) => {
              const remaining = s.sessions.filter((x) => x.id !== sessionId);
              const { [sessionId]: _gone, ...liveEvents } = s.liveEvents;
              void _gone;
              const ws = s.projects.find((p) => p.id === s0?.projectId)?.workspaceId;
              const next = remaining
                .filter((x) => s.projects.find((p) => p.id === x.projectId)?.workspaceId === ws)
                .sort((a, b) => b.updatedAt - a.updatedAt)[0];
              return { sessions: remaining, liveEvents, selectedId: s.selectedId === sessionId ? (next?.id ?? null) : s.selectedId };
            });
          if (s0?.live) {
            liveApi
              .remove(sessionId)
              .then(drop)
              .catch((e: unknown) => setError(sessionId, e));
          } else drop();
        },

        setDraft: (sessionId, text) => set((s) => ({ drafts: { ...s.drafts, [sessionId]: text } })),
        focusComposer: () => set((s) => ({ composerFocusTick: s.composerFocusTick + 1, view: "workspace", viewMode: "rich" })),
        focusSearch: () => set((s) => ({ searchFocusTick: s.searchFocusTick + 1, sidebarOpen: true })),

        /* ---------------------------------------------------------- live */

        setLiveState: (liveState, error = null) => set({ liveState, liveError: error }),

        hydrateLive: (liveProjects, rows, events) =>
          set((s) => {
            const projects = [...liveProjects.map(toProject), ...s.projects.filter((p) => p.workspaceId === "demo")];
            const pathOf = (pid: string) => projects.find((p) => p.id === pid)?.path ?? "";
            const live = rows.map((r) => toSession(r, events[r.id] ?? [], r.worktree || pathOf(r.projectId)));
            const sessions = [...live, ...s.sessions.filter((x) => !x.live)];
            const keepSelection = sessions.some((x) => x.id === s.selectedId && projects.find((p) => p.id === x.projectId)?.workspaceId === s.workspaceId);
            const firstLocal = live.filter((x) => !projects.find((p) => p.id === x.projectId)?.hidden).sort((a, b) => b.updatedAt - a.updatedAt)[0];
            return {
              projects,
              sessions,
              liveEvents: events,
              liveState: "ready",
              selectedId: keepSelection ? s.selectedId : s.workspaceId === "local" ? (firstLocal?.id ?? null) : s.selectedId,
            };
          }),

        upsertLiveSession: (row) =>
          set((s) => {
            const path = row.worktree || (s.projects.find((p) => p.id === row.projectId)?.path ?? "");
            const existing = s.sessions.find((x) => x.id === row.id);
            const moved = existing && (existing.projectId !== row.projectId || (existing.worktree ?? null) !== (row.worktree ?? null));
            const next = existing
              ? {
                  ...existing,
                  ...(moved
                    ? { projectId: row.projectId, raw: normalizeFor(row.provider, s.liveEvents[row.id] ?? [], { sessionId: row.id, projectPath: path }) }
                    : {}),
                  title: row.title,
                  provider: row.provider,
                  model: row.model,
                  status: row.status,
                  snippet: row.snippet,
                  updatedAt: row.updatedAt,
                  mode: row.mode ?? "default",
                  profileId: row.profileId ?? null,
                  worktree: row.worktree ?? null,
                  branch: row.branch ?? null,
                  effort: row.effort ?? null,
                }
              : toSession(row, s.liveEvents[row.id] ?? [], path);
            // A session made just now has no history to load.
            const liveEvents = existing || s.liveEvents[row.id] ? s.liveEvents : { ...s.liveEvents, [row.id]: [] };
            const streaming = row.status === "running" ? s.streaming : { ...s.streaming, [row.id]: "" };
            return { sessions: existing ? s.sessions.map((x) => (x.id === row.id ? next : x)) : [next, ...s.sessions], streaming, liveEvents };
          }),

        appendLiveEvent: (e) => {
          // Remote Control ended while you were elsewhere: its terminal gives way to your shell.
          if (e.kind === "remote_control" && e.payload.state === "off") {
            const st = get();
            const watching = st.viewMode === "terminal" && st.selectedId === e.sessionId && st.view === "workspace";
            if (st.remotes[e.sessionId] && !watching) {
              const mode = st.viewMode;
              st.leaveRemote(e.sessionId);
              set({ viewMode: mode });
            }
          }
          // A session whose history isn't loaded yet: load it all (this event included).
          if (get().liveEvents[e.sessionId] === undefined && get().sessions.some((x) => x.id === e.sessionId && x.live)) {
            const waiting = pendingLoads.get(e.sessionId);
            if (waiting) waiting.push(e);
            else void get().ensureEvents(e.sessionId);
            return;
          }
          set((s) => {
            const list = s.liveEvents[e.sessionId] ?? [];
            if (list.some((x) => x.seq === e.seq)) return s; // duplicate delivery
            const events = [...list, e].sort((a, b) => a.seq - b.seq);
            const session = s.sessions.find((x) => x.id === e.sessionId);
            const path = session?.worktree || (s.projects.find((p) => p.id === session?.projectId)?.path ?? "");
            const raw = session ? normalizeFor(session.provider, events, { sessionId: e.sessionId, projectPath: path }) : [];
            let permissionInFlight = s.permissionInFlight;
            if (e.kind === "permission_decision") {
              const rid = String(e.payload.requestId ?? "");
              const { [rid]: _done, ...rest } = s.permissionInFlight;
              void _done;
              permissionInFlight = rest;
            }
            const clearsStream = e.kind === "assistant" || e.kind.startsWith("result") || e.kind === "process_exit";
            return {
              liveEvents: { ...s.liveEvents, [e.sessionId]: events },
              sessions: s.sessions.map((x) => (x.id === e.sessionId ? { ...x, raw } : x)),
              permissionInFlight,
              streaming: clearsStream ? { ...s.streaming, [e.sessionId]: "" } : s.streaming,
            };
          });
        },

        applyDelta: (sessionId, text, reset) =>
          set((s) => ({ streaming: { ...s.streaming, [sessionId]: reset ? "" : (s.streaming[sessionId] ?? "") + text } })),

        setSessionAgent: (sessionId, provider, profileId) => {
          const st = get();
          const model = profileId ? "default" : (st.defaultModels[provider] ?? "default");
          const effort = profileId ? null : (st.defaultEfforts[provider] ?? null);
          liveApi
            .setAgent(sessionId, provider, model, profileId, effort)
            .then((row) => {
              get().upsertLiveSession(row);
              set((s) => ({ composerFocusTick: s.composerFocusTick + 1 }));
            })
            .catch((e: unknown) => setError(sessionId, e));
        },
        newChat: (provider, profileId) => {
          liveApi
            .openChats()
            .then((p) => {
              set((s) => ({ projects: s.projects.some((x) => x.id === p.id) ? s.projects : [toProject(p), ...s.projects], workspaceId: "local" }));
              get().newSession(p.id, provider, profileId);
            })
            .catch((e: unknown) => set({ liveError: errorText(e) }));
        },
        createProject: async (name) => {
          try {
            const p = await liveApi.createProject(name);
            set((s) => ({ projects: s.projects.some((x) => x.id === p.id) ? s.projects : [...s.projects, toProject(p)], workspaceId: "local" }));
            get().newSession(p.id);
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        agyEnabled: !liveApi.available,
        setAgyEnabled: (agyEnabled) => set({ agyEnabled }),
        renaming: null,
        setRenaming: (renaming) => set({ renaming }),
        renameSession: async (sessionId, title) => {
          const name = title.trim().replace(/\s+/g, " ");
          const session = get().sessions.find((x) => x.id === sessionId);
          if (!session || !name || name === session.title) return null;
          if (!session.live) {
            set((s) => ({ sessions: s.sessions.map((x) => (x.id === sessionId ? { ...x, title: name } : x)) }));
            return null;
          }
          try {
            get().upsertLiveSession(await liveApi.renameSession(sessionId, name));
            return null;
          } catch (e: unknown) {
            const msg = errorText(e);
            set({ liveError: msg });
            return msg;
          }
        },
        moveSession: async (sessionId, projectId) => {
          try {
            get().upsertLiveSession(await liveApi.moveSession(sessionId, projectId));
            return null;
          } catch (e: unknown) {
            const msg = errorText(e);
            set({ liveError: msg });
            return msg;
          }
        },
        reloadLive: async () => {
          const [projects, rows] = await Promise.all([liveApi.listProjects(), liveApi.listSessions()]);
          // Histories already loaded stay; the rest load when their session is opened.
          get().hydrateLive(projects, rows, { ...get().liveEvents });
        },
        importOpen: false,
        setupProjectId: null,
        openSetup: (projectId) => set({ view: "setup", setupProjectId: projectId }),
        setImportOpen: (v) => set({ importOpen: v }),
        newProjectOpen: false,
        setNewProjectOpen: (v) => set({ newProjectOpen: v }),
        addProjectFolder: async () => {
          try {
            const path = await pickFolder();
            if (!path) return;
            const p = await liveApi.addProject(path);
            set((s) => ({
              projects: s.projects.some((x) => x.id === p.id) ? s.projects : [toProject(p), ...s.projects],
              workspaceId: "local",
            }));
          } catch (e) {
            set({ liveError: errorText(e) });
          }
        },

        /* ---------------------------------------------------------- bots */

        bots: Object.fromEntries(demoBots.map((b) => [b.id, b])),
        botTasks: { ...demoBotTasks },
        taskChildren: {},
        loadTaskChildren: async (parentId) => {
          const parent = Object.values(get().botTasks)
            .flat()
            .find((x) => x.id === parentId);
          if (!liveApi.available || (parent && isDemoBot(parent.botId))) {
            set((s) => ({ taskChildren: { ...s.taskChildren, [parentId]: childrenOf(parentId, s.botTasks) } }));
            return;
          }
          try {
            const list = await liveApi.botTaskChildren(parentId);
            set((s) => ({ taskChildren: { ...s.taskChildren, [parentId]: list } }));
          } catch {
            /* keeps the last list */
          }
        },
        selectedBotId: null,
        botEditor: null,
        botErrors: {},
        openBot: (botId) => {
          set({ view: "bot", selectedBotId: botId });
          void get().loadBotTasks(botId);
          void get().loadBotSchedules(botId);
        },
        botSchedules: { ...demoBotSchedules },
        botPlace: {},
        setBotPlace: (botId, projectId) => set((s) => ({ botPlace: { ...s.botPlace, [botId]: projectId } })),
        upsertBotSchedule: (sc) => set((s) => ({ botSchedules: { ...s.botSchedules, [sc.botId]: mergeSchedule(s.botSchedules[sc.botId] ?? [], sc) } })),
        loadBotSchedules: async (botId) => {
          if (!liveApi.available || isDemoBot(botId)) return;
          try {
            const list = await liveApi.botSchedules(botId);
            set((s) => ({ botSchedules: { ...s.botSchedules, [botId]: list } }));
          } catch {
            /* keeps the last list */
          }
        },
        saveBotSchedule: async (input) => {
          if (isDemoBot(input.botId)) {
            const old = input.id ? (get().botSchedules[input.botId] ?? []).find((x) => x.id === input.id) : undefined;
            const sc: BotSchedule = {
              id: input.id ?? localId("sc-demo"),
              botId: input.botId,
              text: input.text.trim(),
              projectId: input.projectId,
              kind: input.kind,
              times: input.kind === "daily" ? [...input.times].sort() : [],
              weekdays: input.kind === "daily" && input.weekdays.length < 7 ? [...input.weekdays].sort() : [],
              everyMinutes: input.kind === "interval" ? input.everyMinutes : null,
              enabled: input.enabled,
              lastRunAt: old?.lastRunAt ?? null,
              nextRunAt: input.enabled ? Date.now() + (input.kind === "interval" ? (input.everyMinutes ?? 60) * 60_000 : 3_600_000) : null,
              lastTaskId: old?.lastTaskId ?? null,
              skippedAt: null,
              createdAt: old?.createdAt ?? Date.now(),
            };
            get().upsertBotSchedule(sc);
            demoNextRun(input.botId);
            return null;
          }
          try {
            get().upsertBotSchedule(await liveApi.saveBotSchedule(input));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        deleteBotSchedule: async (id) => {
          const sc = Object.values(get().botSchedules)
            .flat()
            .find((x) => x.id === id);
          if (!sc) return null;
          const drop = () => set((s) => ({ botSchedules: { ...s.botSchedules, [sc.botId]: (s.botSchedules[sc.botId] ?? []).filter((x) => x.id !== id) } }));
          if (isDemoBot(sc.botId)) {
            drop();
            demoNextRun(sc.botId);
            return null;
          }
          try {
            await liveApi.deleteBotSchedule(id);
            drop();
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        setBotScheduleEnabled: async (id, enabled) => {
          const sc = Object.values(get().botSchedules)
            .flat()
            .find((x) => x.id === id);
          if (!sc) return null;
          if (isDemoBot(sc.botId)) {
            get().upsertBotSchedule({ ...sc, enabled, nextRunAt: enabled ? Date.now() + 3_600_000 : null });
            demoNextRun(sc.botId);
            return null;
          }
          try {
            get().upsertBotSchedule(await liveApi.setBotScheduleEnabled(id, enabled));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        runBotSchedule: async (id) => {
          const sc = Object.values(get().botSchedules)
            .flat()
            .find((x) => x.id === id);
          if (!sc) return null;
          if (isDemoBot(sc.botId)) {
            const err = await get().addBotTask(sc.botId, sc.text, sc.projectId, "schedule");
            get().upsertBotSchedule({ ...sc, lastRunAt: Date.now() });
            return err;
          }
          try {
            get().upsertBotTask(await liveApi.runBotSchedule(id));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        openBotEditor: (botId, projectId) => set({ botEditor: { botId, projectId: projectId ?? null } }),
        closeBotEditor: () => set({ botEditor: null }),
        clearBotError: (botId) => set((s) => ({ botErrors: { ...s.botErrors, [botId]: undefined } })),
        upsertBot: (b) => set((s) => ({ bots: { ...s.bots, [b.id]: b } })),
        upsertBotTask: (task) =>
          set((s) => {
            const siblings = task.parentId ? s.taskChildren[task.parentId] : undefined;
            return {
              botTasks: { ...s.botTasks, [task.botId]: mergeTask(s.botTasks[task.botId] ?? [], task) },
              // A task handed over by another shows in that task's list as it changes.
              ...(task.parentId && siblings ? { taskChildren: { ...s.taskChildren, [task.parentId]: mergeTask(siblings, task) } } : {}),
            };
          }),
        loadBots: async () => {
          if (!liveApi.available) return;
          try {
            const list = await liveApi.listBots();
            set((s) => ({
              bots: { ...Object.fromEntries(Object.entries(s.bots).filter(([, b]) => b.id.startsWith("b-demo-"))), ...Object.fromEntries(list.map((b) => [b.id, b])) },
            }));
          } catch {
            /* the Bots section stays empty */
          }
        },
        loadBotTasks: async (botId) => {
          if (!liveApi.available || botId.startsWith("b-demo-")) return;
          try {
            const tasks = await liveApi.botTasks(botId);
            set((s) => ({ botTasks: { ...s.botTasks, [botId]: tasks } }));
          } catch {
            /* keeps the last list */
          }
        },
        saveBot: async (input, connections, options) => {
          if (!liveApi.available) return t("Bots run on this Mac. Open AgentDeck on your Mac to make one.");
          try {
            const b = await liveApi.saveBot(input);
            if (connections) await connectionsApi.setForBot(b.id, connections);
            if (options) await botExtrasApi.setOptions(b.id, options);
            get().upsertBot(b);
            if (!input.id) get().openBot(b.id);
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        deleteBot: async (botId) => {
          const drop = () =>
            set((s) => {
              const { [botId]: _gone, ...bots } = s.bots;
              const { [botId]: _tasks, ...botTasks } = s.botTasks;
              void _gone;
              void _tasks;
              return { bots, botTasks, ...(s.selectedBotId === botId ? { selectedBotId: null, view: "workspace" as AppView } : {}) };
            });
          if (botId.startsWith("b-demo-")) {
            drop();
            return null;
          }
          try {
            await liveApi.deleteBot(botId);
            drop();
            // Its workspace may now show as an ordinary project with its sessions.
            void get()
              .reloadLive()
              .catch(() => undefined);
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        addBotTask: async (botId, text, projectId, source = "you") => {
          const trimmed = text.trim();
          if (!trimmed) return null;
          if (botId.startsWith("b-demo-")) {
            const now = Date.now();
            get().upsertBotTask({
              id: localId("t-demo"),
              botId,
              text: trimmed,
              source,
              status: "queued",
              result: null,
              error: null,
              createdAt: now,
              startedAt: null,
              finishedAt: null,
              parentId: null,
              projectId,
              sessionId: null,
              children: 0,
              model: null,
              effort: null,
              worktree: null,
              branch: null,
              tokensIn: null,
              tokensOut: null,
              costUsd: null,
            });
            const b = get().bots[botId];
            if (b) get().upsertBot({ ...b, queued: b.queued + 1 });
            return null;
          }
          try {
            get().upsertBotTask(await liveApi.addBotTask(botId, trimmed, projectId));
            return null;
          } catch (e: unknown) {
            return errorText(e);
          }
        },
        cancelBotTask: (taskId) => {
          const task = Object.values(get().botTasks)
            .flat()
            .find((x) => x.id === taskId);
          if (!task) return;
          if (task.botId.startsWith("b-demo-")) {
            get().upsertBotTask({ ...task, status: "cancelled", finishedAt: Date.now() });
            const b = get().bots[task.botId];
            if (b && task.status === "queued") get().upsertBot({ ...b, queued: Math.max(0, b.queued - 1) });
            if (b && task.status === "running") get().upsertBot({ ...b, runningTaskId: null, runningText: null, runningProjectId: null, runningSessionId: null });
            return;
          }
          liveApi
            .cancelBotTask(taskId)
            .then((row) => get().upsertBotTask(row))
            .catch((e: unknown) => set((s) => ({ botErrors: { ...s.botErrors, [task.botId]: errorText(e) } })));
        },
        retryBotTask: (taskId) => {
          const task = Object.values(get().botTasks)
            .flat()
            .find((x) => x.id === taskId);
          if (!task) return;
          if (task.botId.startsWith("b-demo-")) {
            get().upsertBotTask({ ...task, status: "queued", result: null, error: null, startedAt: null, finishedAt: null });
            const b = get().bots[task.botId];
            if (b) get().upsertBot({ ...b, queued: b.queued + 1 });
            return;
          }
          liveApi
            .retryBotTask(taskId)
            .then((row) => get().upsertBotTask(row))
            .catch((e: unknown) => set((s) => ({ botErrors: { ...s.botErrors, [task.botId]: errorText(e) } })));
        },
        setBotPaused: (botId, paused) => {
          const b = get().bots[botId];
          if (!b) return;
          if (botId.startsWith("b-demo-")) {
            get().upsertBot({ ...b, paused });
            return;
          }
          liveApi
            .setBotPaused(botId, paused)
            .then((row) => get().upsertBot(row))
            .catch((e: unknown) => set((s) => ({ botErrors: { ...s.botErrors, [botId]: errorText(e) } })));
        },
        restartBotSession: (botId) => {
          const b = get().bots[botId];
          if (!b) return;
          if (botId.startsWith("b-demo-")) {
            get().upsertBot({ ...b, places: [] });
            return;
          }
          liveApi
            .restartBotSession(botId)
            .then((row) => get().upsertBot(row))
            .catch((e: unknown) => set((s) => ({ botErrors: { ...s.botErrors, [botId]: errorText(e) } })));
        },

        removeProject: (projectId) =>
          liveApi.removeProject(projectId).then(() => {
              set((s) => ({
                projects: s.projects.filter((p) => p.id !== projectId),
                // Bots stay; the ones that worked there by default now work in their workspace.
                bots: Object.fromEntries(
                  Object.entries(s.bots).map(([id, b]) => [
                    id,
                    { ...b, projectId: b.projectId === projectId ? null : b.projectId, places: b.places.filter((p) => p.projectId !== projectId) },
                  ]),
                ),
                botPlace: Object.fromEntries(Object.entries(s.botPlace).filter(([, p]) => p !== projectId)),
                sessions: s.sessions.filter((x) => x.projectId !== projectId),
                selectedId: s.sessions.find((x) => x.id === s.selectedId)?.projectId === projectId ? null : s.selectedId,
              }));
            }),
      };
    },
    {
      name: "agentdeck-ui",
      version: 3,
      storage: createJSONStorage(() => safeStorage),
      // Preferences and local review marks only. Sessions live in the desktop app's SQLite store.
      partialize: (s) => ({
        sidebarOpen: s.sidebarOpen,
        sidebarWidth: s.sidebarWidth,
        contextOpen: s.contextOpen,
        contextWidth: s.contextWidth,
        theme: s.theme,
        language: s.language,
        motion: s.motion,
        workspaceId: s.workspaceId,
        viewed: s.viewed,
        reviewLayout: s.reviewLayout,
        defaultProvider: s.defaultProvider,
        defaultModels: s.defaultModels,
        defaultEfforts: s.defaultEfforts,
        notifications: s.notifications,
      }),
      migrate: (persisted) => {
        const p = persisted as Partial<UiState>;
        return { ...p, workspaceId: defaultWorkspace, defaultModels: {} } as UiState & Actions;
      },
      merge: (persisted, current) => {
        const p = (persisted ?? {}) as Partial<UiState>;
        const ws =
          p.workspaceId === "local" && !liveApi.available ? "demo" : p.workspaceId === "demo" || p.workspaceId === "local" ? p.workspaceId : defaultWorkspace;
        return { ...current, ...p, workspaceId: ws, selectedId: ws === "demo" ? (current.selectedId ?? demoSessions[0]!.id) : null };
      },
    },
  ),
);

/** Projects and sessions of the current workspace. */
export function inWorkspace<T extends { projectId: string }>(items: T[], projects: Project[], ws: WorkspaceId): T[] {
  const ids = new Set(projects.filter((p) => p.workspaceId === ws).map((p) => p.id));
  return items.filter((i) => ids.has(i.projectId));
}
