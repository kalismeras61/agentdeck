import { AlertTriangle, Server, Bot, Check, ChevronRight, Clock, GitPullRequestArrow, MessageSquare, Radio as Pulse, CloudOff, Copy, Download, FolderGit2, GitBranch, GitCommitHorizontal, Radio, RefreshCw, Search, Trash2, Users } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { AgentMark, cx, StatusDot, Switch } from "../components/ui";
import { demoBranchDetail, demoFilePair, demoTeam } from "../data/teamDemo";
import { providerLabel } from "../data/demo";
import { useResolvedTheme } from "../hooks/useTheme";
import type { ProviderId } from "../lib/events";
import { ago } from "../lib/format";
import { t, tn } from "../lib/i18n";
import { errorText, gitApi, type GitChangedFile, type GitFilePair } from "../lib/live";
import {
  checkoutCommand,
  filterBranches,
  hueOf,
  initials,
  isOnline,
  overlaps,
  overlapsWith,
  ownersOf,
  people,
  reviewPrompt,
  teamApi,
  type BranchDetail,
  type BranchFilter,
  type Person,
  type Presence,
  type PresenceSession,
  type ReviewCheckout,
  type TeamBranch,
  type TeamOverview,
} from "../lib/team";
import { useStore } from "../state/store";
import { hm, lastDays, sharedIn, sum, timeApi, total, type TimeRow } from "../lib/time";
import { demoTimeRows } from "../data/teamDemo";
import { MonacoDiff } from "./review/MonacoDiff";
import { TimeTab } from "./team/TimeTab";
import { ConnectionDialog } from "./team/ConnectionDialog";
import { ActivityTab } from "./team/ActivityTab";
import { MessagesTab } from "./team/MessagesTab";
import { chatApi, threads, unreadCount, type FeedEvent, type MessageKind, type TeamMessages } from "../lib/teamchat";
import { demoFeed, demoMessages } from "../data/teamDemo";
import { ViewHeader } from "./ViewHeader";

const DAYS = 30;
/** While the view is open, fetch this often so teammates' pushes and presence show up. */
const POLL_MS = 2 * 60_000;
const LANG: Record<string, string> = { dart: "dart", json: "json", yaml: "yaml", yml: "yaml", ts: "typescript", tsx: "typescript", js: "javascript", jsx: "javascript", md: "markdown", css: "css", html: "html", py: "python", rs: "rust", go: "go", java: "java", kt: "kotlin", swift: "swift", sql: "sql", sh: "shell" };
const langOf = (p: string) => LANG[p.split(".").pop()?.toLowerCase() ?? ""] ?? "plaintext";
const home = (p: string) => p.replace(/^\/(Users|home)\/[^/]+/, "~");
const FILTERS = /* i18n */ [
  { id: "active", label: "Active" },
  { id: "mine", label: "Mine" },
  { id: "conflicts", label: "Overlapping" },
  { id: "all", label: "All" },
] as const;

const agentOf = (label: string): ProviderId => (Object.entries(providerLabel).find(([, l]) => l === label)?.[0] as ProviderId | undefined) ?? "api";

function Avatar({ name, email, size = 22, online }: { name: string; email: string; size?: number; online?: boolean }) {
  const h = hueOf(email);
  return (
    <span className="relative inline-flex shrink-0" title={`${name} <${email}>`}>
      <span
        aria-hidden
        className="inline-flex items-center justify-center rounded-full font-medium text-white"
        style={{ width: size, height: size, fontSize: Math.round(size * 0.4), background: `hsl(${h} 55% 48%)` }}
      >
        {initials(name)}
      </span>
      {online !== undefined && (
        <span
          role="img"
          aria-label={online ? t("Online") : t("Away")}
          className={cx("absolute -bottom-px -end-px h-2 w-2 rounded-full ring-2 ring-bg", online ? "bg-success" : "bg-fg-3/60")}
        />
      )}
    </span>
  );
}

function Owners({ b }: { b: TeamBranch }) {
  const list = ownersOf(b);
  return (
    <span className="flex shrink-0 -space-x-1.5 rtl:space-x-reverse">
      {list.slice(0, 3).map((p) => (
        <Avatar key={p.email} name={p.name} email={p.email} size={18} />
      ))}
      {list.length > 3 && <span className="ms-2 text-[11px] text-fg-3">+{list.length - 3}</span>}
    </span>
  );
}

function SessionLine({ s, who }: { s: PresenceSession; who?: string }) {
  return (
    <div className="flex min-w-0 items-center gap-1.5 py-0.5 text-[11.5px]">
      <AgentMark provider={agentOf(s.agent)} size={15} />
      <StatusDot status={s.status} />
      <span dir="auto" className="min-w-0 truncate text-fg-2" title={s.title}>
        {s.title}
      </span>
      {who && <span className="shrink-0 text-fg-3">· {who}</span>}
    </div>
  );
}

function Section({ title, count, open, onToggle, children, extra }: { title: string; count?: number; open: boolean; onToggle: () => void; children: ReactNode; extra?: ReactNode }) {
  return (
    <section className="pt-2">
      <div className="flex items-center pe-2">
        <button type="button" aria-expanded={open} onClick={onToggle} className="flex flex-1 items-center gap-1.5 px-3 py-1 text-start text-[12px] font-semibold text-fg">
          <ChevronRight size={12} className={cx("rtl:-scale-x-100 text-fg-3 transition-transform", open && "rotate-90 rtl:-rotate-90")} />
          {title}
          {count !== undefined && <span className="font-normal tabular-nums text-fg-3">{count}</span>}
        </button>
        {extra}
      </div>
      {open && <div className="px-1.5 pb-1">{children}</div>}
    </section>
  );
}

/** What sharing puts on the remote, shown before it's turned on. */
function ShareDialog({ preview, remote, relayTeam, onCancel, onConfirm, busy }: { preview: Presence; remote: string; relayTeam: string | null; onCancel: () => void; onConfirm: () => void; busy: boolean }) {
  return (
    <div role="dialog" aria-modal aria-label={t("Share what you're working on")} className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6">
      <div className="w-full max-w-[520px] rounded-xl border border-line bg-surface p-5 shadow-xl">
        <h2 className="text-[15px] font-semibold">{t("Share what you're working on")}</h2>
        <p className="mt-2 text-[13px] leading-[1.6] text-fg-2">
          {relayTeam
            ? t("AgentDeck sends this to the {team} team on your relay. It's updated every few minutes while AgentDeck runs and removed when you turn sharing off.", { team: relayTeam })
            : t("AgentDeck pushes a small file to {remote} under refs/agentdeck/presence, outside your branches, so teammates who fetch see it. It's updated every few minutes while AgentDeck runs and removed when you turn sharing off.", { remote })}
        </p>
        <p className="mt-3 text-[12px] font-medium text-fg-2">{t("This is what they'll see:")}</p>
        <pre dir="ltr" className="scroll-quiet mt-1.5 max-h-[220px] overflow-auto rounded-lg border border-line-soft bg-bg p-3 font-mono text-[11.5px] leading-[1.5] text-fg-2">
          {JSON.stringify({ name: preview.name, email: preview.email, branch: preview.branch, worktrees: preview.worktrees, sessions: preview.sessions.map((s) => ({ title: s.title, agent: s.agent, branch: s.branch, status: s.status })) }, null, 2)}
        </pre>
        {!preview.email && <p className="mt-2 text-[12.5px] text-error">{t("Git doesn't know who you are here. Set user.name and user.email first.")}</p>}
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onCancel} className="h-8 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated">
            {t("Cancel")}
          </button>
          <button
            type="button"
            disabled={busy || !preview.email}
            onClick={onConfirm}
            className="h-8 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white disabled:opacity-50"
          >
            {busy ? t("Sharing…") : t("Share")}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Team: who works on which branch, what their agents are doing, and where branches overlap. */
export function TeamView() {
  const projects = useStore((s) => s.projects);
  const sessions = useStore((s) => s.sessions);
  const selectedId = useStore((s) => s.selectedId);
  const openGit = useStore((s) => s.openGit);
  const select = useStore((s) => s.select);
  const setDraft = useStore((s) => s.setDraft);
  const upsertLiveSession = useStore((s) => s.upsertLiveSession);
  const defaultProvider = useStore((s) => s.defaultProvider);
  const defaultModels = useStore((s) => s.defaultModels);
  const defaultEfforts = useStore((s) => s.defaultEfforts);
  const dark = useResolvedTheme() === "dark";
  const demo = !teamApi.available;

  const folders = projects.filter((p) => p.workspaceId === "local" && p.kind !== "chats" && !p.hidden);
  const fallback = sessions.find((s) => s.id === selectedId)?.projectId;
  const [chosen, setProjectId] = useState<string | null>(null);
  const projectId = chosen && folders.some((p) => p.id === chosen) ? chosen : folders.some((p) => p.id === fallback) ? fallback! : (folders[0]?.id ?? null);
  const project = folders.find((p) => p.id === projectId);

  const [o, setO] = useState<TeamOverview | null>(() => (demo ? demoTeam() : null));
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [filter, setFilter] = useState<BranchFilter>("active");
  const [query, setQuery] = useState("");
  const [person, setPerson] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [detail, setDetail] = useState<BranchDetail | null>(null);
  const [file, setFile] = useState<GitChangedFile | null>(null);
  const [pair, setPair] = useState<GitFilePair | null>(null);
  const [copied, setCopied] = useState(false);
  const [share, setShare] = useState<Presence | null>(null);
  const [open, setOpen] = useState<Record<string, boolean>>({ people: true, reviews: true, overlaps: true });
  const [tab, setTab] = useState<"branches" | "activity" | "messages" | "time">("branches");
  const [feed, setFeed] = useState<FeedEvent[] | null>(() => (demo ? demoFeed() : null));
  const [chat, setChat] = useState<TeamMessages | null>(() => (demo ? demoMessages() : null));
  const [focusThread, setFocusThread] = useState<string | null>(null);
  const [draftMsg, setDraftMsg] = useState<{ to: string; kind: MessageKind; branch: string } | null>(null);
  const setTeamUnread = useStore((s) => s.setTeamUnread);
  const teamTick = useStore((s) => s.teamTick);
  const [connection, setConnection] = useState(false);
  const [newInvite, setNewInvite] = useState<string | null>(null);
  const [plan, setPlan] = useState<import("../lib/team").RelayBilling | null>(null);
  // The relay team's plan, for the banner when it needs attention.
  useEffect(() => {
    setPlan(null);
    if (demo || !projectId || !o?.relay) return;
    teamApi
      .relayBilling(projectId)
      .then(setPlan)
      .catch(() => setPlan(null));
  }, [demo, projectId, o?.relay?.url, o?.fetchedAt]); // eslint-disable-line react-hooks/exhaustive-deps
  const loadChat = useCallback(() => {
    if (demo || !projectId) return;
    chatApi.feed(projectId).then(setFeed).catch(() => setFeed([]));
    chatApi.messages(projectId).then(setChat).catch(() => setChat(null));
  }, [demo, projectId]);
  // A relay brought news: show it now.
  useEffect(() => {
    if (teamTick) {
      load(false);
    }
  }, [teamTick]); // eslint-disable-line react-hooks/exhaustive-deps
  const unread = useMemo(() => (chat ? unreadCount(threads(chat.messages, chat.me.email, new Set(chat.read))) : 0), [chat]);
  useEffect(() => setTeamUnread(unread), [unread, setTeamUnread]);
  // Your own tracked time, the last 7 days, for the branch panel.
  const [myTime, setMyTime] = useState<TimeRow[]>(() => (demo ? demoTimeRows() : []));
  useEffect(() => {
    if (demo) return;
    timeApi
      .report(7)
      .then(setMyTime)
      .catch(() => setMyTime([]));
  }, [demo, picked]);
  const week = useMemo(() => lastDays(7), []);

  const load = useCallback(
    (fetch: boolean) => {
      if (demo || !projectId) return;
      setLoading(true);
      setError(null);
      (fetch ? teamApi.fetch(projectId, DAYS) : teamApi.overview(projectId, DAYS))
        .then((x) => {
          setO(x);
          loadChat();
        })
        .catch((e: unknown) => {
          setO(null);
          setError(errorText(e));
        })
        .finally(() => setLoading(false));
    },
    [demo, projectId, loadChat],
  );

  // What's known right away, then a fetch; then every few minutes while the view is open.
  useEffect(() => {
    setPicked(null);
    setPerson(null);
    setFeed(demo ? demoFeed() : null);
    setChat(demo ? demoMessages() : null);
    load(false);
    load(true);
    const id = window.setInterval(() => load(true), POLL_MS);
    return () => window.clearInterval(id);
  }, [load]);

  const team = useMemo(() => (o ? people(o) : []), [o]);
  const clashes = useMemo(() => (o ? overlaps(o.branches) : []), [o]);
  const shown = useMemo(() => {
    if (!o) return [];
    const list = filterBranches(o, filter, query);
    return person ? list.filter((b) => ownersOf(b).some((p) => p.email === person)) : list;
  }, [o, filter, query, person]);
  const branch = o?.branches.find((b) => b.name === picked) ?? null;
  const base = o?.base ?? null;

  // Who has an agent on a branch right now (from shared presence).
  const liveOn = useCallback(
    (b: TeamBranch) =>
      (o?.presence ?? [])
        .filter((p) => isOnline(p))
        .flatMap((p) => p.sessions.filter((s) => s.branch === b.short || s.branch === b.local).map((s) => ({ s, who: p.email === o?.me.email ? t("you") : p.name }))),
    [o],
  );

  // The picked branch: its commits and files.
  useEffect(() => {
    setDetail(null);
    setFile(null);
    setPair(null);
    if (!branch || !base || branch.ahead === 0) return;
    if (demo) return setDetail(demoBranchDetail(branch, base));
    teamApi
      .branch(projectId!, branch.name, base)
      .then(setDetail)
      .catch((e: unknown) => setNotice(errorText(e)));
  }, [branch?.name, branch?.hash, base]); // eslint-disable-line react-hooks/exhaustive-deps

  // The picked file, before (where the branch forked) and after.
  useEffect(() => {
    setPair(null);
    if (!file || !detail || !branch) return;
    if (demo) return setPair(demoFilePair(file.path));
    gitApi
      .filePair(projectId!, null, file.change === "added" ? "" : detail.mergeBase, file.change === "deleted" ? "" : branch.name, file.path, file.from)
      .then(setPair)
      .catch((e: unknown) => setNotice(errorText(e)));
  }, [file, detail]); // eslint-disable-line react-hooks/exhaustive-deps

  const act = async (key: string, fn: () => Promise<unknown>) => {
    if (demo) return setNotice(t("This works in the desktop app, on your own repositories."));
    setBusy(key);
    setNotice(null);
    try {
      await fn();
    } catch (e) {
      setNotice(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const checkout = (b: TeamBranch) =>
    act(`checkout:${b.name}`, async () => {
      const r = await teamApi.checkout(projectId!, b.name);
      setNotice(t("{branch} is checked out in {path}.", { branch: b.short, path: home(r.path) }));
      load(false);
    });

  const reviewWithAgent = (b: TeamBranch) =>
    act(`agent:${b.name}`, async () => {
      const p = defaultProvider;
      const row = await teamApi.reviewSession(projectId!, b.name, p, defaultModels[p] ?? "default", null, defaultEfforts[p] ?? null);
      upsertLiveSession(row);
      setDraft(row.id, reviewPrompt(b, base));
      select(row.id);
    });

  const updateReview = (r: ReviewCheckout) =>
    act(`update:${r.path}`, async () => {
      await teamApi.update(projectId!, r.path);
      load(false);
    });

  const removeReview = (r: ReviewCheckout) => {
    if (!demo && r.dirty && !window.confirm(t("The review checkout has changes that aren't committed. Remove it anyway? They'll be lost."))) return;
    return act(`remove:${r.path}`, async () => {
      await teamApi.remove(projectId!, r.path);
      load(false);
    });
  };

  const toggleSharing = (on: boolean) => {
    if (demo) return setNotice(t("This works in the desktop app, on your own repositories."));
    if (!projectId) return;
    if (on) {
      teamApi
        .presencePreview(projectId)
        .then(setShare)
        .catch((e: unknown) => setNotice(errorText(e)));
      return;
    }
    void act("share", async () => setO(await teamApi.setSharing(projectId, false, DAYS)));
  };

  const copy = (text: string) => {
    void navigator.clipboard?.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  };

  const reviewFor = (b: TeamBranch) => o?.reviews.find((r) => r.upstream === b.name || r.upstream === b.short);
  const sessionAt = (path: string) => sessions.find((s) => s.worktree === path);

  if (!demo && folders.length === 0) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title={t("Team")} />
        <p className="m-auto max-w-[44ch] text-center text-[13px] leading-[1.6] text-fg-3">{t("Add a project folder to see who works on which branch.")}</p>
      </div>
    );
  }

  /** Someone's tracked time over the last 7 days: yours from this Mac, theirs from what they share. */
  const weekOf = (p: Person) =>
    total(p.me ? sum(myTime.filter((r) => week.includes(r.day) && (demo || r.projectId === projectId))) : sum(sharedIn(p.presence?.time, week)));
  /** Time on a branch over the last 7 days, per person. */
  const timeOn = (b: TeamBranch) => {
    const names = [b.short, b.local].filter(Boolean) as string[];
    const out: { name: string; email: string; ms: ReturnType<typeof sum> }[] = [];
    const mine = sum(myTime.filter((r) => week.includes(r.day) && (demo || r.projectId === projectId) && names.includes(r.branch ?? "") ));
    if (total(mine) > 0) out.push({ name: t("you"), email: o?.me.email ?? "", ms: mine });
    for (const pr of o?.presence ?? []) {
      if (pr.email === o?.me.email) continue;
      const ms = sum(sharedIn(pr.time, week).filter((r) => names.includes(r.branch)));
      if (total(ms) > 0) out.push({ name: pr.name, email: pr.email, ms });
    }
    return out;
  };

  const personRow = (p: Person) => {
    const active = person === p.email;
    const live = p.online ? (p.presence?.sessions ?? []).filter((s) => Date.now() - s.updatedAt < 6 * 3_600_000).slice(0, 4) : [];
    return (
      <div key={p.email} className={cx("rounded-md", active && "bg-elevated")}>
        <button
          type="button"
          onClick={() => setPerson(active ? null : p.email)}
          aria-pressed={active}
          className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start hover:bg-surface"
        >
          <Avatar name={p.name} email={p.email} online={p.presence ? p.online : undefined} />
          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-1.5">
              <span dir="auto" className="truncate text-[12.5px] text-fg">
                {p.name}
              </span>
              {p.me && <span className="shrink-0 rounded bg-violet-soft px-1 text-[10px] text-violet-text">{t("you")}</span>}
            </span>
            <span className="block truncate text-[11px] text-fg-3">
              {weekOf(p) > 0 && <span className="tabular-nums">{t("{time} this week", { time: hm(weekOf(p)) })} · </span>}
              {p.working > 0
                ? tn(p.working, "{n} agent working", "{n} agents working")
                : p.branches.length
                  ? tn(p.branches.length, "{n} branch", "{n} branches")
                  : p.online
                    ? t("Online")
                    : t("Seen {when}", { when: ago(p.last) })}
            </span>
          </span>
        </button>
        {live.length > 0 && (
          <div className="pb-1 ps-[38px] pe-2">
            {live.map((s, i) => (
              <SessionLine key={i} s={s} />
            ))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title={t("Team")}
        sub={o?.remoteUrl ?? undefined}
        after={
          <div role="tablist" aria-label={t("Team")} className="no-drag flex rounded-lg border border-line-soft p-0.5">
            {(
              [
                ["branches", t("Branches"), GitBranch, 0],
                ["activity", t("Activity"), Pulse, 0],
                ["messages", t("Messages"), MessageSquare, unread],
                ["time", t("Time"), Clock, 0],
              ] as const
            ).map(([id, label, Icon, badge]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={tab === id}
                onClick={() => setTab(id)}
                className={cx("flex h-6 items-center gap-1.5 rounded-md px-2 text-[12px]", tab === id ? "bg-elevated text-fg" : "text-fg-3 hover:text-fg")}
              >
                <Icon size={12} />
                {label}
                {badge > 0 && <span className="rounded-full bg-violet-strong px-1.5 text-[10px] font-medium tabular-nums text-white">{badge}</span>}
              </button>
            ))}
          </div>
        }
      >
        {!demo && (
          <label className="flex items-center gap-2 text-[12.5px] text-fg-3">
            {t("Project")}
            <select
              value={projectId ?? ""}
              onChange={(e) => setProjectId(e.target.value || null)}
              className="h-7 max-w-[200px] rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg outline-none"
            >
              {folders.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {o && (
          <button
            type="button"
            onClick={() => setConnection(true)}
            title={t("Team connection")}
            className="no-drag flex h-7 items-center gap-1.5 rounded-md px-2 text-[12.5px] text-fg-3 hover:bg-elevated hover:text-fg"
          >
            {o.relay ? <Server size={13} className="text-success" /> : <GitBranch size={13} />}
            <span className="max-w-[140px] truncate">{o.relay ? o.relay.team : t("Git remote")}</span>
          </button>
        )}
        {(o?.remote || o?.relay) && (
          <label className="no-drag flex items-center gap-2 text-[12.5px] text-fg-3" title={t("Teammates see your branches and what your agents are doing, through {remote}.", { remote: o.relay ? o.relay.team : (o.remote ?? "") })}>
            <Radio size={13} className={o.sharing ? "text-success" : ""} />
            {t("Share my work")}
            <Switch checked={o.sharing} onChange={toggleSharing} label={t("Share what you're working on")} />
          </label>
        )}
        <button
          type="button"
          onClick={() => load(true)}
          disabled={loading || demo}
          title={o?.fetchedAt ? t("Fetched {when}", { when: ago(o.fetchedAt) }) : t("Fetch")}
          className="no-drag flex h-7 items-center gap-1.5 rounded-md px-2 text-[12.5px] text-fg-3 hover:bg-elevated hover:text-fg"
        >
          <RefreshCw size={13} className={cx(loading && "animate-spin motion-reduce:animate-none")} />
          <span className="tabular-nums">{o?.fetchedAt ? ago(o.fetchedAt) : t("Fetch")}</span>
        </button>
      </ViewHeader>

      {plan?.enabled && plan.access.note && (
        <div role="status" className={cx("flex items-center gap-2 border-b border-line-soft px-4 py-2 text-[12.5px]", plan.access.canWrite ? "text-warning" : "text-error")}>
          <AlertTriangle size={14} className="shrink-0" />
          <span className="min-w-0 flex-1">{plan.access.note}</span>
          <button type="button" onClick={() => setConnection(true)} className="shrink-0 rounded px-2 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg">
            {t("Plan")}
          </button>
        </div>
      )}
      {(o?.syncError || notice) && (
        <div role="status" className={cx("flex items-start gap-2 border-b border-line-soft px-4 py-2 text-[12.5px]", o?.syncError ? "text-warning" : "text-fg-2")}>
          {o?.syncError ? <CloudOff size={14} className="mt-px shrink-0" /> : <Check size={14} className="mt-px shrink-0 text-success" />}
          <span className="min-w-0 flex-1 break-words">{notice ?? t("Showing what was fetched before. {error}", { error: o?.syncError ?? "" })}</span>
          {notice && (
            <button type="button" onClick={() => setNotice(null)} className="shrink-0 text-fg-3 hover:text-fg">
              {t("Dismiss")}
            </button>
          )}
        </div>
      )}

      {tab === "activity" ? (
        <ActivityTab
          feed={feed}
          onBranch={(short) => {
            const b = o?.branches.find((x) => x.short === short || x.name === short);
            if (b) {
              setTab("branches");
              setFilter("all");
              setPicked(b.name);
            }
          }}
          onThread={(id) => {
            setFocusThread(id);
            setTab("messages");
          }}
        />
      ) : tab === "messages" ? (
        <MessagesTab
          projectId={projectId}
          data={chat}
          people={team.filter((p) => !p.me).map((p) => ({ name: p.name, email: p.email }))}
          branches={(o?.branches ?? []).filter((b) => b.local && !b.localOnly).map((b) => b.short)}
          sharing={!!o?.sharing}
          demo={demo}
          focus={focusThread}
          draft={draftMsg}
          onChanged={loadChat}
          onReview={(short) => {
            const b = o?.branches.find((x) => x.short === short);
            if (b) void reviewWithAgent(b);
            else setNotice(t("Fetch first: {branch} isn't here yet.", { branch: short }));
          }}
          onNotice={setNotice}
        />
      ) : tab === "time" ? (
        <TimeTab projectId={projectId} presence={o?.presence ?? []} me={o?.me.email ?? ""} />
      ) : error && !o ? (
        <div className="m-auto max-w-[52ch] px-8 text-center">
          <FolderGit2 size={22} className="mx-auto text-fg-3" />
          <p className="mt-3 text-[14px] text-fg">{t("{name} isn't a git repository", { name: project?.name ?? "" })}</p>
          <p className="mt-1.5 text-[13px] leading-[1.6] text-fg-3">{error}</p>
        </div>
      ) : !o ? (
        <p className="m-auto text-[13px] text-fg-3">{t("Reading the repository…")}</p>
      ) : (
        <div className="flex min-h-0 flex-1">
          {/* People, review checkouts, overlapping files */}
          <aside aria-label={t("People")} className="scroll-quiet w-[250px] shrink-0 overflow-y-auto border-e border-line-soft pb-4">
            {!o.remote && (
              <p className="m-3 rounded-lg border border-line-soft p-3 text-[12px] leading-[1.55] text-fg-3">
                {t("This repository has no remote, so only your own branches show here. Add the remote your team pushes to with git remote add origin <url>.")}
              </p>
            )}
            <Section title={t("People")} count={team.length} open={open.people!} onToggle={() => setOpen((x) => ({ ...x, people: !x.people }))}>
              {team.length === 0 ? (
                <p className="px-2 py-1 text-[12px] text-fg-3">{t("Nobody has unmerged work yet.")}</p>
              ) : (
                team.map(personRow)
              )}
              {!o.sharing && o.remote && (
                <p className="mx-2 mt-2 text-[11.5px] leading-[1.5] text-fg-3">{t("Turn on “Share my work” and ask teammates to do the same to see each other's agents live.")}</p>
              )}
            </Section>
            {o.reviews.length > 0 && (
              <Section title={t("Review checkouts")} count={o.reviews.length} open={open.reviews!} onToggle={() => setOpen((x) => ({ ...x, reviews: !x.reviews }))}>
                {o.reviews.map((r) => {
                  const s = sessionAt(r.path);
                  return (
                    <div key={r.path} className="rounded-md px-2 py-1.5 hover:bg-surface">
                      <button type="button" onClick={() => setPicked(o.branches.find((b) => b.name === r.upstream)?.name ?? null)} className="flex w-full items-center gap-1.5 text-start">
                        <FolderGit2 size={12} className="shrink-0 text-fg-3" />
                        <span className="truncate font-mono text-[11.5px] text-fg">{r.upstream ?? r.branch}</span>
                        {r.behind > 0 && <span className="ms-auto shrink-0 rounded-full bg-blue/15 px-1.5 text-[10.5px] text-blue">{tn(r.behind, "{n} new", "{n} new")}</span>}
                      </button>
                      {s && (
                        <button type="button" onClick={() => select(s.id)} className="ms-[18px] truncate text-start text-[11.5px] text-violet-text hover:underline">
                          {s.title}
                        </button>
                      )}
                      <div className="mt-1 flex gap-1 ps-[16px]">
                        {r.behind > 0 && (
                          <button type="button" disabled={!!busy} onClick={() => void updateReview(r)} className="flex h-6 items-center gap-1 rounded px-1.5 text-[11px] text-fg-2 hover:bg-elevated">
                            <Download size={11} />
                            {t("Update")}
                          </button>
                        )}
                        <button type="button" onClick={() => openGit(projectId, r.path)} className="flex h-6 items-center gap-1 rounded px-1.5 text-[11px] text-fg-2 hover:bg-elevated">
                          <GitBranch size={11} />
                          Git
                        </button>
                        <button type="button" disabled={!!busy} onClick={() => void removeReview(r)} aria-label={t("Remove the review checkout")} className="flex h-6 items-center gap-1 rounded px-1.5 text-[11px] text-fg-3 hover:bg-elevated hover:text-error">
                          <Trash2 size={11} />
                        </button>
                      </div>
                    </div>
                  );
                })}
              </Section>
            )}
            {clashes.length > 0 && (
              <Section title={t("Same files, different branches")} count={clashes.length} open={open.overlaps!} onToggle={() => setOpen((x) => ({ ...x, overlaps: !x.overlaps }))}>
                {clashes.slice(0, 12).map((c) => (
                  <button
                    key={c.path}
                    type="button"
                    onClick={() => {
                      setFilter("conflicts");
                      setPicked(c.branches[0]!.name);
                    }}
                    className="block w-full rounded-md px-2 py-1 text-start hover:bg-surface"
                    title={c.branches.map((b) => b.short).join(", ")}
                  >
                    <span className="flex items-center gap-1.5">
                      <AlertTriangle size={11} className="shrink-0 text-warning" />
                      <span dir="ltr" className="truncate font-mono text-[11.5px] text-fg-2">
                        {c.path}
                      </span>
                    </span>
                    <span className="block truncate ps-[17px] text-[11px] text-fg-3">{tn(c.branches.length, "on {n} branch", "on {n} branches")}</span>
                  </button>
                ))}
              </Section>
            )}
          </aside>

          {/* Branches */}
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex flex-wrap items-center gap-2 border-b border-line-soft px-3 py-2">
              <div role="tablist" aria-label={t("Which branches")} className="flex rounded-lg border border-line-soft p-0.5">
                {FILTERS.map((f) => (
                  <button
                    key={f.id}
                    type="button"
                    role="tab"
                    aria-selected={filter === f.id}
                    onClick={() => setFilter(f.id)}
                    className={cx("h-6 rounded-md px-2 text-[12px]", filter === f.id ? "bg-elevated text-fg" : "text-fg-3 hover:text-fg")}
                  >
                    {t(f.label)}
                  </button>
                ))}
              </div>
              <div className="relative min-w-[140px] flex-1">
                <Search size={13} className="pointer-events-none absolute start-2 top-1/2 -translate-y-1/2 text-fg-3" />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("Branch, person or commit")}
                  aria-label={t("Search branches")}
                  className="h-7 w-full rounded-md border border-line-soft bg-bg ps-7 pe-2 text-[12.5px] text-fg outline-none placeholder:text-fg-3"
                />
              </div>
              {person && (
                <button type="button" onClick={() => setPerson(null)} className="flex h-7 items-center gap-1 rounded-md border border-line-soft px-2 text-[12px] text-fg-2 hover:bg-elevated">
                  <Users size={12} />
                  {team.find((p) => p.email === person)?.name}
                  <span aria-hidden>×</span>
                </button>
              )}
            </div>
            <div role="list" aria-label={t("Branches")} className="scroll-quiet min-h-0 flex-1 overflow-y-auto">
              {shown.length === 0 && <p className="p-8 text-center text-[13px] text-fg-3">{filter === "active" ? t("No branch had commits in the last few days. Pick All to see older ones.") : t("No branches match.")}</p>}
              {shown.map((b) => {
                const live = liveOn(b);
                const clash = overlapsWith(b, o.branches);
                const mine = !!b.local || b.localOnly;
                return (
                  <button
                    key={b.name}
                    type="button"
                    role="listitem"
                    onClick={() => {
                      setPicked(b.name);
                      setNotice(null);
                    }}
                    aria-current={picked === b.name ? "true" : undefined}
                    className={cx("flex w-full items-center gap-3 border-b border-line-soft px-3 py-2 text-start", picked === b.name ? "bg-elevated" : "hover:bg-surface")}
                  >
                    <Owners b={b} />
                    <span className="min-w-0 flex-1">
                      <span className="flex min-w-0 items-center gap-1.5">
                        <span dir="ltr" className="truncate font-mono text-[12.5px] text-fg">
                          {b.short}
                        </span>
                        {b.localOnly && <span className="shrink-0 rounded border border-line px-1 text-[10px] text-fg-3">{t("not pushed")}</span>}
                        {mine && !b.localOnly && <span className="shrink-0 rounded bg-violet-soft px-1 text-[10px] text-violet-text">{t("yours")}</span>}
                        {b.ahead === 0 && <span className="shrink-0 rounded border border-line px-1 text-[10px] text-fg-3">{t("merged")}</span>}
                        {live.some((x) => x.s.status === "running" || x.s.status === "waiting") && (
                          <span className="flex shrink-0 items-center gap-1 text-[10.5px] text-success">
                            <span className="breathe h-1.5 w-1.5 rounded-full bg-success" />
                            {t("agent working")}
                          </span>
                        )}
                        {clash.length > 0 && (
                          <span title={t("Changes the same files as {branches}", { branches: clash.map((c) => c.branch.short).join(", ") })} className="flex shrink-0 items-center gap-0.5 text-[10.5px] text-warning">
                            <AlertTriangle size={11} />
                            {clash.length}
                          </span>
                        )}
                      </span>
                      <span dir="auto" className="mt-0.5 block truncate text-[12px] text-fg-3">
                        {b.subject}
                      </span>
                    </span>
                    <span className="flex shrink-0 flex-col items-end gap-0.5 text-[11px] tabular-nums text-fg-3">
                      <span>{ago(b.time)}</span>
                      <span className="flex gap-1.5">
                        {b.ahead > 0 && <span title={tn(b.ahead, "{n} commit not in {base}", "{n} commits not in {base}", { base: base ?? "" })}>+{b.ahead}</span>}
                        {b.behind > 0 && <span title={tn(b.behind, "{n} commit behind {base}", "{n} commits behind {base}", { base: base ?? "" })}>−{b.behind}</span>}
                        {b.files.length > 0 && <span>{tn(b.files.length, "{n} file", "{n} files")}</span>}
                      </span>
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* The picked branch */}
          <section aria-label={t("Branch details")} className="flex w-[40%] min-w-[340px] max-w-[720px] shrink-0 flex-col border-s border-line-soft">
            {!branch ? (
              <div className="m-auto max-w-[40ch] px-8 text-center">
                <GitBranch size={20} className="mx-auto text-fg-3" />
                <p className="mt-3 text-[13px] leading-[1.6] text-fg-3">{t("Pick a branch to see who's on it, what it changes and where it touches other branches. Check it out to run it locally, or let an agent review it.")}</p>
              </div>
            ) : (
              <>
                <div className="space-y-2.5 border-b border-line-soft px-4 py-3">
                  <div className="flex items-start gap-2">
                    <span dir="ltr" className="min-w-0 flex-1 break-all font-mono text-[13.5px] font-semibold">
                      {branch.short}
                    </span>
                    <button
                      type="button"
                      onClick={() => copy(checkoutCommand(o.remote, branch))}
                      title={checkoutCommand(o.remote, branch)}
                      className="flex h-6 shrink-0 items-center gap-1 rounded px-1.5 text-[11.5px] text-fg-3 hover:bg-elevated hover:text-fg"
                    >
                      {copied ? <Check size={11} /> : <Copy size={11} />}
                      {t("Copy command")}
                    </button>
                  </div>
                  <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-fg-3">
                    {ownersOf(branch).map((p) => (
                      <span key={p.email} className="flex items-center gap-1 text-fg-2">
                        <Avatar name={p.name} email={p.email} size={16} />
                        {p.name}
                      </span>
                    ))}
                    {base && branch.ahead > 0 && <span>{t("{ahead} ahead, {behind} behind {base}", { ahead: branch.ahead, behind: branch.behind, base })}</span>}
                    {branch.local && (branch.unpushed > 0 || branch.unpulled > 0) && (
                      <span className="text-blue">{t("Your copy: {up} to push, {down} to pull", { up: branch.unpushed, down: branch.unpulled })}</span>
                    )}
                  </p>
                  {timeOn(branch).length > 0 && (
                    <div className="rounded-lg border border-line-soft px-2.5 py-1.5">
                      <p className="mb-0.5 flex items-center gap-1 text-[11px] font-medium text-fg-3">
                        <Clock size={11} />
                        {t("Time on it, last 7 days")}
                      </p>
                      {timeOn(branch).map((x) => (
                        <p key={x.email + x.name} className="flex items-center gap-2 py-0.5 text-[11.5px]">
                          <span className="min-w-0 flex-1 truncate text-fg-2">{x.name}</span>
                          <span className="tabular-nums text-fg-3" title={t("Agents working")}>
                            <span aria-hidden className="me-1 inline-block h-1.5 w-1.5 rounded-full" style={{ background: "var(--chart-agent)" }} />
                            {hm(x.ms.agentMs)}
                          </span>
                          <span className="tabular-nums text-fg-3" title={t("Waiting for you")}>
                            <span aria-hidden className="me-1 inline-block h-1.5 w-1.5 rounded-full" style={{ background: "var(--chart-waiting)" }} />
                            {hm(x.ms.waitingMs)}
                          </span>
                          <span className="tabular-nums text-fg-3" title={t("Your time")}>
                            <span aria-hidden className="me-1 inline-block h-1.5 w-1.5 rounded-full" style={{ background: "var(--chart-you)" }} />
                            {hm(x.ms.youMs)}
                          </span>
                        </p>
                      ))}
                    </div>
                  )}
                  {liveOn(branch).length > 0 && (
                    <div className="rounded-lg border border-line-soft px-2.5 py-1.5">
                      <p className="mb-0.5 text-[11px] font-medium text-fg-3">{t("On it now")}</p>
                      {liveOn(branch).map((x, i) => (
                        <SessionLine key={i} s={x.s} who={x.who} />
                      ))}
                    </div>
                  )}
                  {overlapsWith(branch, o.branches).length > 0 && (
                    <div className="rounded-lg border border-[color-mix(in_srgb,var(--warning)_40%,var(--border))] bg-warning/5 px-2.5 py-1.5">
                      <p className="flex items-center gap-1 text-[11.5px] font-medium text-warning">
                        <AlertTriangle size={12} />
                        {t("May conflict with")}
                      </p>
                      {overlapsWith(branch, o.branches)
                        .slice(0, 5)
                        .map((x) => (
                          <button key={x.branch.name} type="button" onClick={() => setPicked(x.branch.name)} className="block w-full truncate py-0.5 text-start text-[11.5px] text-fg-2 hover:text-fg">
                            <span dir="ltr" className="font-mono">
                              {x.branch.short}
                            </span>
                            <span className="text-fg-3"> · {x.files.slice(0, 3).join(", ")}{x.files.length > 3 ? ` +${x.files.length - 3}` : ""}</span>
                          </button>
                        ))}
                    </div>
                  )}
                  <div className="flex flex-wrap gap-1.5">
                    {reviewFor(branch) ? (
                      <button type="button" onClick={() => openGit(projectId, reviewFor(branch)!.path)} className="flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12px] text-fg-2 hover:bg-elevated hover:text-fg">
                        <FolderGit2 size={13} />
                        {t("Open the checkout in Git")}
                      </button>
                    ) : (
                      !branch.checkedOut && (
                        <button
                          type="button"
                          disabled={!!busy}
                          onClick={() => void checkout(branch)}
                          title={t("Checks the branch out in its own folder next to the project; your working copy isn't touched.")}
                          className="flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12px] text-fg-2 hover:bg-elevated hover:text-fg disabled:opacity-50"
                        >
                          <Download size={13} />
                          {busy === `checkout:${branch.name}` ? t("Checking out…") : t("Check out locally")}
                        </button>
                      )
                    )}
                    {branch.ahead > 0 && (
                      <button
                        type="button"
                        disabled={!!busy}
                        onClick={() => void reviewWithAgent(branch)}
                        title={t("Checks it out and opens a session with {agent} there, with a review request ready to send.", { agent: providerLabel[defaultProvider] })}
                        className="flex h-7 items-center gap-1.5 rounded-lg bg-violet-strong px-2.5 text-[12px] font-medium text-white disabled:opacity-50"
                      >
                        <Bot size={13} />
                        {busy === `agent:${branch.name}` ? t("Preparing…") : t("Review with {agent}", { agent: providerLabel[defaultProvider] })}
                      </button>
                    )}
                    {branch.local && !branch.localOnly && branch.ahead > 0 && (
                      <button
                        type="button"
                        onClick={() => {
                          setDraftMsg({ to: "*", kind: "review", branch: branch.short });
                          setTab("messages");
                        }}
                        className="flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-[12px] text-fg-2 hover:bg-elevated hover:text-fg"
                      >
                        <GitPullRequestArrow size={13} />
                        {t("Ask for a review")}
                      </button>
                    )}
                    {branch.checkedOut && (
                      <span className="flex h-7 items-center gap-1 text-[11.5px] text-fg-3" title={branch.checkedOut}>
                        <Check size={12} />
                        {t("Checked out in {path}", { path: home(branch.checkedOut) })}
                      </span>
                    )}
                  </div>
                </div>
                {branch.ahead === 0 ? (
                  <p className="p-6 text-center text-[12.5px] text-fg-3">{t("Everything on this branch is already in {base}.", { base: base ?? "" })}</p>
                ) : !detail ? (
                  <p className="p-6 text-center text-[12.5px] text-fg-3">{t("Reading the changes…")}</p>
                ) : (
                  <>
                    <ul aria-label={t("Commits")} className="scroll-quiet max-h-[22%] shrink-0 overflow-y-auto border-b border-line-soft py-1">
                      {detail.commits.map((c) => (
                        <li key={c.hash} className="flex items-center gap-2 px-4 py-0.5 text-[12px]">
                          <GitCommitHorizontal size={12} className="shrink-0 text-fg-3" />
                          <span dir="auto" className="min-w-0 flex-1 truncate text-fg-2">
                            {c.subject}
                          </span>
                          <span className="shrink-0 text-fg-3">{c.author}</span>
                          <span className="w-[54px] shrink-0 text-end tabular-nums text-fg-3">{ago(c.time)}</span>
                        </li>
                      ))}
                    </ul>
                    <ul aria-label={t("Files")} className="scroll-quiet max-h-[26%] shrink-0 overflow-y-auto border-b border-line-soft py-1">
                      {detail.files.map((f) => {
                        const shared = o.branches.filter((x) => x.name !== branch.name && x.ahead > 0 && x.files.includes(f.path));
                        return (
                          <li key={f.path}>
                            <button
                              type="button"
                              onClick={() => setFile(f)}
                              className={cx("flex w-full items-center gap-2 px-4 py-1 text-start", file?.path === f.path ? "bg-elevated" : "hover:bg-surface")}
                            >
                              <span className={cx("w-3 shrink-0 text-center font-mono text-[11.5px]", f.change === "added" ? "text-success" : f.change === "deleted" ? "text-error" : f.change === "renamed" ? "text-blue" : "text-warning")}>
                                {f.change === "added" ? "A" : f.change === "deleted" ? "D" : f.change === "renamed" ? "R" : "M"}
                              </span>
                              <span dir="ltr" className="truncate font-mono text-[12px] text-fg-2">
                                {f.path}
                              </span>
                              {shared.length > 0 && (
                                <span title={t("Also changed on {branches}", { branches: shared.map((x) => x.short).join(", ") })} className="ms-auto shrink-0 text-warning">
                                  <AlertTriangle size={11} />
                                </span>
                              )}
                            </button>
                          </li>
                        );
                      })}
                      {branch.moreFiles && <li className="px-4 py-1 text-[11.5px] text-fg-3">{t("More files than shown; check it out to see them all.")}</li>}
                    </ul>
                    <div className="relative min-h-0 flex-1">
                      {!file ? (
                        <p className="p-6 text-center text-[12.5px] text-fg-3">{t("Pick a file to see the diff.")}</p>
                      ) : !pair ? (
                        <p className="p-6 text-center text-[12.5px] text-fg-3">{t("Opening {path}…", { path: file.path })}</p>
                      ) : pair.binary ? (
                        <p className="p-6 text-center text-[12.5px] text-fg-3">{t("{path} is a binary or very large file.", { path: file.path })}</p>
                      ) : (
                        <MonacoDiff original={pair.old} modified={pair.new} language={langOf(file.path)} sideBySide={false} fileOnly={false} dark={dark} label={file.path} />
                      )}
                    </div>
                  </>
                )}
              </>
            )}
          </section>
        </div>
      )}

      {connection && o && (
        <ConnectionDialog
          o={o}
          projectId={projectId}
          invite={newInvite}
          busy={busy === "relay"}
          onClose={() => {
            setConnection(false);
            setNewInvite(null);
          }}
          onCreate={(url, name) =>
            void act("relay", async () => {
              const [x, inv] = await teamApi.relayCreate(projectId!, url, name, DAYS);
              setO(x);
              setNewInvite(inv);
              loadChat();
            })
          }
          onJoin={(url, invite) =>
            void act("relay", async () => {
              setO(await teamApi.relayJoin(projectId!, url, invite, DAYS));
              setConnection(false);
              loadChat();
            })
          }
          onLeave={() =>
            void act("relay", async () => {
              setO(await teamApi.relayLeave(projectId!, DAYS));
              setConnection(false);
              loadChat();
            })
          }
        />
      )}
      {share && (o?.remote || o?.relay) && (
        <ShareDialog
          preview={share}
          remote={o.remote ?? ""}
          relayTeam={o.relay?.team ?? null}
          busy={busy === "share"}
          onCancel={() => setShare(null)}
          onConfirm={() =>
            void act("share", async () => {
              try {
                setO(await teamApi.setSharing(projectId!, true, DAYS));
              } finally {
                setShare(null);
              }
            })
          }
        />
      )}
    </div>
  );
}
