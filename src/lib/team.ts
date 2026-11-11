/**
 * Team: who works on which branch, read from the project's git remote (see src-tauri/src/team.rs).
 * Types and calls for the desktop app, and the pure parts the Team view is built from (people,
 * overlapping files, how fresh someone's presence is), kept here so they're tested.
 */
import { isTauri } from "./native";
import type { GitChangedFile, GitCommit, LiveProvider, LiveSessionRow } from "./live";
import { tzOffset, type BranchDay } from "./time";

export type TeamAuthor = { name: string; email: string; commits: number; last: number };
export type TeamBranch = {
  name: string;
  short: string;
  localOnly: boolean;
  hash: string;
  author: string;
  email: string;
  time: number;
  subject: string;
  authors: TeamAuthor[];
  ahead: number;
  behind: number;
  files: string[];
  moreFiles: boolean;
  checkedOut: string | null;
  local: string | null;
  unpushed: number;
  unpulled: number;
};
export type PresenceSession = { title: string; agent: string; branch: string | null; status: "idle" | "running" | "waiting" | "error"; updatedAt: number };
/** time: per branch and day, the last two weeks (version 2 and later). */
export type Presence = { version: number; name: string; email: string; branch: string | null; worktrees: string[]; sessions: PresenceSession[]; time?: BranchDay[]; updatedAt: number };
export type ReviewCheckout = { path: string; branch: string; upstream: string | null; behind: number; ahead: number; dirty: boolean };
export type TeamOverview = {
  root: string;
  remote: string | null;
  remoteUrl: string | null;
  base: string | null;
  current: string | null;
  me: { name: string; email: string };
  branches: TeamBranch[];
  presence: Presence[];
  reviews: ReviewCheckout[];
  sharing: boolean;
  fetchedAt: number | null;
  syncError: string | null;
  /** The relay presence and messages go through; null: the git remote. */
  relay?: { url: string; team: string; repo: string } | null;
  /** Everyone in the relay's team besides you. */
  members?: { name: string; email: string }[];
};
export type BranchDetail = { branch: string; base: string; mergeBase: string; commits: GitCommit[]; files: GitChangedFile[] };

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const teamApi = {
  available: isTauri,
  overview: (projectId: string, days: number) => call<TeamOverview>("team_overview", { projectId, days }),
  /** Fetches the remote first; a failed fetch comes back as syncError, not as an error. */
  fetch: (projectId: string, days: number) => call<TeamOverview>("team_fetch", { projectId, days, tzOffset: tzOffset() }),
  presencePreview: (projectId: string) => call<Presence>("team_presence_preview", { projectId }),
  setSharing: (projectId: string, on: boolean, days: number) => call<TeamOverview>("team_set_sharing", { projectId, on, days }),
  /** Fetches followed projects and shares again where you share; returns the new feed events. */
  heartbeat: () => call<import("./teamchat").FeedEvent[]>("team_heartbeat", { tzOffset: tzOffset() }),
  branch: (projectId: string, branch: string, base: string) => call<BranchDetail>("team_branch", { projectId, branch, base }),
  checkout: (projectId: string, branch: string) => call<ReviewCheckout>("team_review_checkout", { projectId, branch }),
  update: (projectId: string, path: string) => call<ReviewCheckout>("team_review_update", { projectId, path }),
  remove: (projectId: string, path: string) => call<void>("team_review_remove", { projectId, path }),
  /** Joins a team on an AgentDeck Relay for this project (shares your work with it). */
  relayJoin: (projectId: string, url: string, invite: string, days: number) => call<TeamOverview>("team_relay_join", { projectId, url, invite, days }),
  relayLeave: (projectId: string, days: number) => call<TeamOverview>("team_relay_leave", { projectId, days }),
  /** Long polls the relays (≈25 s); relay false: no project uses one. */
  wait: () => call<{ relay: boolean; events: import("./teamchat").FeedEvent[] }>("team_wait"),
  reviewSession: (projectId: string, branch: string, provider: LiveProvider, model: string, profileId: string | null, effort: string | null) =>
    call<LiveSessionRow>("team_review_session", { projectId, branch, provider, model, profileId, effort }),
};

/** Presence older than this reads as "away": they closed AgentDeck or stopped sharing. */
export const ONLINE_MS = 10 * 60_000;
/** A branch with a commit this recent counts as being worked on. */
export const ACTIVE_MS = 3 * 86_400_000;

export const isOnline = (p: Presence, now = Date.now()) => now - p.updatedAt < ONLINE_MS;

export type Person = {
  email: string;
  name: string;
  me: boolean;
  presence: Presence | null;
  online: boolean;
  /** Branches they have commits on (not merged yet), newest first. */
  branches: TeamBranch[];
  /** Their newest commit or presence update. */
  last: number;
  /** Sessions working right now. */
  working: number;
};

/** Everyone who has unmerged commits on a branch or shares their presence, the active first. */
export function people(o: Pick<TeamOverview, "branches" | "presence" | "me" | "members">, now = Date.now()): Person[] {
  const byEmail = new Map<string, Person>();
  const meEmail = o.me.email.toLowerCase();
  const get = (email: string, name: string) => {
    const key = email.toLowerCase();
    let p = byEmail.get(key);
    if (!p) {
      p = { email: key, name: name || key, me: !!meEmail && key === meEmail, presence: null, online: false, branches: [], last: 0, working: 0 };
      byEmail.set(key, p);
    }
    return p;
  };
  for (const b of o.branches) {
    for (const a of b.authors) {
      const p = get(a.email, a.name);
      if (!p.branches.includes(b)) p.branches.push(b);
      p.last = Math.max(p.last, a.last);
    }
  }
  for (const m of o.members ?? []) get(m.email, m.name);
  for (const pr of o.presence) {
    const p = get(pr.email, pr.name);
    p.presence = pr;
    if (pr.name) p.name = pr.name;
    p.online = isOnline(pr, now);
    p.working = p.online ? pr.sessions.filter((s) => s.status === "running" || s.status === "waiting").length : 0;
    p.last = Math.max(p.last, pr.updatedAt);
  }
  for (const p of byEmail.values()) p.branches.sort((a, b) => b.time - a.time);
  // Teammates before you: you know what you're doing.
  return [...byEmail.values()].sort((a, b) => Number(b.online) - Number(a.online) || b.working - a.working || Number(a.me) - Number(b.me) || b.last - a.last);
}

export type Overlap = { path: string; branches: TeamBranch[] };

/**
 * Files changed on more than one unmerged branch: where two people (or two agents) are likely
 * to conflict. The most-shared first.
 */
export function overlaps(branches: TeamBranch[]): Overlap[] {
  const by = new Map<string, TeamBranch[]>();
  for (const b of branches) {
    if (b.ahead === 0) continue;
    for (const f of b.files) {
      const list = by.get(f);
      if (list) list.push(b);
      else by.set(f, [b]);
    }
  }
  return [...by.entries()]
    .filter(([, bs]) => bs.length > 1)
    .map(([path, bs]) => ({ path, branches: bs }))
    .sort((a, b) => b.branches.length - a.branches.length || a.path.localeCompare(b.path));
}

/** The other branches that change files this one changes, with those files. */
export function overlapsWith(branch: TeamBranch, all: TeamBranch[]): { branch: TeamBranch; files: string[] }[] {
  if (branch.ahead === 0) return [];
  const mine = new Set(branch.files);
  return all
    .filter((b) => b.name !== branch.name && b.ahead > 0)
    .map((b) => ({ branch: b, files: b.files.filter((f) => mine.has(f)) }))
    .filter((x) => x.files.length > 0)
    .sort((a, b) => b.files.length - a.files.length);
}

/** Who a branch belongs to, for its row: the people with commits on it, else its newest commit's author. */
export function ownersOf(b: TeamBranch): { name: string; email: string }[] {
  return b.authors.length ? b.authors.map((a) => ({ name: a.name, email: a.email })) : [{ name: b.author, email: b.email }];
}

export type BranchFilter = "active" | "all" | "mine" | "conflicts";

export function filterBranches(o: Pick<TeamOverview, "branches" | "me">, filter: BranchFilter, query: string, now = Date.now()): TeamBranch[] {
  const q = query.trim().toLowerCase();
  const me = o.me.email.toLowerCase();
  const clash = new Set(overlaps(o.branches).flatMap((x) => x.branches.map((b) => b.name)));
  return o.branches.filter((b) => {
    if (filter === "active" && !(b.ahead > 0 && now - b.time < ACTIVE_MS) && !b.localOnly) return false;
    if (filter === "mine" && !(b.localOnly || b.local || ownersOf(b).some((p) => p.email === me))) return false;
    if (filter === "conflicts" && !clash.has(b.name)) return false;
    if (!q) return true;
    return b.short.toLowerCase().includes(q) || b.subject.toLowerCase().includes(q) || ownersOf(b).some((p) => p.name.toLowerCase().includes(q) || p.email.includes(q));
  });
}

/** Sessions in someone's presence on a given branch (their worktree branch or the folder's). */
export function sessionsOn(p: Presence, branchShort: string): PresenceSession[] {
  return p.sessions.filter((s) => s.branch === branchShort);
}

/** Initials for an avatar: "Ayşe Yılmaz" → "AY", "mert@x.dev" → "ME". */
export function initials(name: string): string {
  const words = name.replace(/@.*/, "").split(/[\s._-]+/).filter(Boolean);
  const s = words.length > 1 ? words[0]![0]! + words[1]![0]! : (words[0] ?? "?").slice(0, 2);
  return s.toLocaleUpperCase();
}

/** A stable hue for a person, so they keep their color everywhere. */
export function hueOf(email: string): number {
  let h = 0;
  for (const c of email.toLowerCase()) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h % 360;
}

/** The shell commands to check a branch out by hand, for people who'd rather use their terminal. */
export function checkoutCommand(remote: string | null, b: Pick<TeamBranch, "short" | "localOnly">): string {
  if (b.localOnly || !remote) return `git switch ${b.short}`;
  return `git fetch ${remote} && git switch --track ${remote}/${b.short}`;
}

/** The message a review session starts with. Kept in English: it goes to the agent, not to you. */
export function reviewPrompt(b: Pick<TeamBranch, "name" | "short" | "authors" | "author">, base: string | null): string {
  const who = b.authors.length ? b.authors.map((a) => a.name).join(", ") : b.author;
  const against = base ?? "the main branch";
  return [
    `Review the branch ${b.name} (by ${who}). It's checked out in this folder.`,
    `Compare it with ${against}: run \`git log --oneline ${against}..HEAD\` and \`git diff ${against}...HEAD\` to see what changed.`,
    "Look for bugs, missing tests, risky changes and anything that doesn't match how the rest of the code is written. Run the project's tests and checks if you can.",
    "Don't change any files. End with a short summary, then the findings ordered by severity, each with the file and line.",
  ].join("\n\n");
}
