/** Sample team for the web preview's Team view: labeled as demo data wherever it shows. */
import type { GitFilePair } from "../lib/live";
import type { BranchDetail, TeamBranch, TeamOverview } from "../lib/team";
import { lastDays, type BranchDay, type TimeRow } from "../lib/time";

const MIN = 60_000;
const HOUR = 60 * MIN;

/** Sample sessions behind the demo time rows: id → title. */
export const DEMO_TIME_SESSIONS: Record<string, string> = {
  "demo-checkout": "Address step for checkout",
  "demo-cart": "Keep the cart offline",
  "demo-review": "Review fix/payment-timeout",
  "demo-tests": "Fix the flaky checkout tests",
};

/** A steady, made-up working pattern: the same every time for a given day. */
function wave(day: string, salt: number, max: number): number {
  let h = salt;
  for (const c of day) h = (h * 33 + c.charCodeAt(0)) >>> 0;
  return Math.round(((h % 1000) / 1000) * max);
}

export function demoTimeRows(now = Date.now()): TimeRow[] {
  const days = lastDays(30, now);
  const rows: TimeRow[] = [];
  days.forEach((day, i) => {
    const weekend = new Date(day + "T12:00:00").getDay() % 6 === 0;
    if (weekend && i % 3) return;
    const scale = weekend ? 0.3 : 1;
    const sessions: [string, string, number][] = [
      ["demo-checkout", "feature/checkout-v2", 1],
      ["demo-cart", "spike/offline-cart", i % 2],
      ["demo-review", "fix/payment-timeout", i % 4 === 0 ? 1 : 0],
      ["demo-tests", "feature/checkout-v2", i % 3 === 1 ? 1 : 0],
    ];
    sessions.forEach(([sessionId, branch, on], k) => {
      if (!on) return;
      rows.push({
        projectId: "shop-app",
        sessionId,
        branch,
        day,
        agentMs: Math.round(scale * (25 * MIN + wave(day, k + 1, 110 * MIN))),
        waitingMs: Math.round(scale * (2 * MIN + wave(day, k + 7, 18 * MIN))),
        youMs: Math.round(scale * (8 * MIN + wave(day, k + 13, 40 * MIN))),
      });
    });
  });
  return rows;
}

function demoShared(now: number, branch: string, salt: number): BranchDay[] {
  return lastDays(14, now)
    .filter((d) => new Date(d + "T12:00:00").getDay() % 6 !== 0)
    .map((day) => ({ branch, day, agentMs: 30 * MIN + wave(day, salt, 150 * MIN), waitingMs: 3 * MIN + wave(day, salt + 1, 20 * MIN), youMs: 10 * MIN + wave(day, salt + 2, 60 * MIN) }));
}

function branch(b: Partial<TeamBranch> & Pick<TeamBranch, "name" | "short" | "subject" | "author" | "email" | "time">): TeamBranch {
  return {
    localOnly: false,
    hash: Math.abs([...b.name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7)).toString(16).padEnd(40, "0"),
    authors: [{ name: b.author, email: b.email, commits: b.ahead ?? 1, last: b.time }],
    ahead: 1,
    behind: 0,
    files: [],
    moreFiles: false,
    checkedOut: null,
    local: null,
    unpushed: 0,
    unpulled: 0,
    ...b,
  };
}

export function demoTeam(now = Date.now()): TeamOverview {
  return {
    root: "~/Projects/shop-app",
    remote: "origin",
    remoteUrl: "git@github.com:acme/shop-app.git",
    base: "origin/main",
    current: "feature/checkout-v2",
    me: { name: "You", email: "you@acme.dev" },
    sharing: true,
    fetchedAt: now - 2 * MIN,
    syncError: null,
    branches: [
      branch({
        name: "origin/feature/login-redesign",
        short: "feature/login-redesign",
        subject: "Login: new layout and validation messages",
        author: "Ayşe Yılmaz",
        email: "ayse@acme.dev",
        time: now - 6 * MIN,
        ahead: 7,
        behind: 2,
        files: ["lib/auth/login_screen.dart", "lib/auth/validators.dart", "lib/theme/colors.dart", "test/auth/login_test.dart"],
      }),
      branch({
        name: "origin/fix/payment-timeout",
        short: "fix/payment-timeout",
        subject: "Retry the payment request once on a timeout",
        author: "Mert Kaya",
        email: "mert@acme.dev",
        time: now - 25 * MIN,
        ahead: 3,
        files: ["lib/payment/client.dart", "lib/payment/retry.dart", "lib/theme/colors.dart"],
        authors: [
          { name: "Mert Kaya", email: "mert@acme.dev", commits: 2, last: now - 25 * MIN },
          { name: "Zeynep Arslan", email: "zeynep@acme.dev", commits: 1, last: now - 2 * HOUR },
        ],
      }),
      branch({
        name: "origin/feature/checkout-v2",
        short: "feature/checkout-v2",
        subject: "Checkout: address step",
        author: "You",
        email: "you@acme.dev",
        time: now - 50 * MIN,
        ahead: 5,
        behind: 1,
        local: "feature/checkout-v2",
        unpushed: 2,
        files: ["lib/checkout/address_step.dart", "lib/payment/client.dart", "lib/checkout/cart.dart"],
        checkedOut: "~/Projects/shop-app",
      }),
      branch({
        name: "origin/chore/deps",
        short: "chore/deps",
        subject: "Update Flutter dependencies",
        author: "Zeynep Arslan",
        email: "zeynep@acme.dev",
        time: now - 26 * HOUR,
        ahead: 1,
        behind: 9,
        files: ["pubspec.yaml", "pubspec.lock"],
      }),
      branch({
        name: "spike/offline-cart",
        short: "spike/offline-cart",
        localOnly: true,
        subject: "Try keeping the cart offline",
        author: "You",
        email: "you@acme.dev",
        time: now - 3 * HOUR,
        ahead: 2,
        files: ["lib/checkout/cart.dart", "lib/storage/cache.dart"],
      }),
      branch({
        name: "origin/release/2.4",
        short: "release/2.4",
        subject: "Bump version to 2.4.0",
        author: "Deniz Şahin",
        email: "deniz@acme.dev",
        time: now - 9 * 86_400_000,
        ahead: 0,
        behind: 31,
        authors: [],
      }),
    ],
    presence: [
      {
        version: 1,
        name: "Ayşe Yılmaz",
        email: "ayse@acme.dev",
        branch: "main",
        worktrees: ["feature/login-redesign"],
        updatedAt: now - 1 * MIN,
        time: demoShared(now, "feature/login-redesign", 3),
        sessions: [
          { title: "Redesign the login screen", agent: "Claude Code", branch: "feature/login-redesign", status: "running", updatedAt: now - MIN },
          { title: "Write tests for the validators", agent: "Codex", branch: "feature/login-redesign", status: "waiting", updatedAt: now - 4 * MIN },
        ],
      },
      {
        version: 1,
        name: "Mert Kaya",
        email: "mert@acme.dev",
        branch: "fix/payment-timeout",
        worktrees: [],
        updatedAt: now - 3 * MIN,
        time: demoShared(now, "fix/payment-timeout", 11),
        sessions: [{ title: "Why does the payment time out on 3G?", agent: "Gemini CLI", branch: "fix/payment-timeout", status: "idle", updatedAt: now - 20 * MIN }],
      },
      {
        version: 1,
        name: "You",
        email: "you@acme.dev",
        branch: "feature/checkout-v2",
        worktrees: [],
        updatedAt: now - 2 * MIN,
        sessions: [{ title: "Address step for checkout", agent: "Claude Code", branch: "feature/checkout-v2", status: "idle", updatedAt: now - 50 * MIN }],
      },
      {
        version: 1,
        name: "Zeynep Arslan",
        email: "zeynep@acme.dev",
        branch: "chore/deps",
        worktrees: [],
        updatedAt: now - 5 * HOUR,
        sessions: [],
      },
    ],
    reviews: [{ path: "~/Projects/shop-app.worktrees/review-fix-payment-timeout", branch: "agentdeck/review-fix-payment-timeout", upstream: "origin/fix/payment-timeout", behind: 1, ahead: 0, dirty: false }],
  };
}

export function demoBranchDetail(b: TeamBranch, base: string): BranchDetail {
  const subjects = [b.subject, "Address review notes", "Tests for the new behavior", "Small cleanups", "Split the widget", "Wire it up", "First pass"];
  return {
    branch: b.name,
    base,
    mergeBase: "0000000",
    commits: Array.from({ length: Math.min(b.ahead, subjects.length) }, (_, i) => ({
      hash: (i + 1).toString(16).padStart(2, "0") + b.hash.slice(2),
      parents: [],
      author: b.authors[i % Math.max(1, b.authors.length)]?.name ?? b.author,
      email: b.email,
      time: b.time - i * 40 * MIN,
      subject: subjects[i]!,
      refs: [],
    })),
    files: b.files.map((path, i) => ({ path, change: i === b.files.length - 1 && path.startsWith("test/") ? "added" : "modified", from: null })),
  };
}

export function demoFilePair(path: string): GitFilePair {
  return {
    old: `// ${path}\nclass Example {\n  final int retries = 0;\n\n  void run() {\n    send();\n  }\n}\n`,
    new: `// ${path}\nclass Example {\n  final int retries = 1;\n\n  void run() {\n    for (var i = 0; i <= retries; i++) {\n      if (send()) return;\n    }\n  }\n}\n`,
    binary: false,
  };
}

/* ---------------------------------------------------------------- messages and feed */

export function demoMessages(now = Date.now()): import("../lib/teamchat").TeamMessages {
  const ayse = { name: "Ayşe Yılmaz", email: "ayse@acme.dev" };
  const mert = { name: "Mert Kaya", email: "mert@acme.dev" };
  const me = { name: "You", email: "you@acme.dev" };
  return {
    me,
    read: ["m1", "m2", "m3"],
    messages: [
      { id: "m1", thread: "m1", from: me, to: "mert@acme.dev", kind: "question", text: "Does the payment client retry on 502 too, or only on timeouts? Checkout shows a spinner forever when the gateway returns 502.", branch: "feature/checkout-v2", agent: "Claude Code", createdAt: now - 95 * MIN },
      { id: "m2", thread: "m1", from: mert, to: "you@acme.dev", kind: "answer", text: "Only timeouts for now. 502 throws PaymentGatewayError right away (lib/payment/client.dart:88). I'll add 502/503 to the retry list on fix/payment-timeout today.", branch: "feature/checkout-v2", agent: "Gemini CLI", createdAt: now - 70 * MIN },
      { id: "m3", thread: "m3", from: ayse, to: "*", kind: "note", text: "Heads up: I'm moving the brand colors to lib/theme/colors.dart. If your branch touches colors, rebase after my merge this afternoon.", branch: null, agent: null, createdAt: now - 3 * HOUR },
      { id: "m4", thread: "m4", from: ayse, to: "you@acme.dev", kind: "question", text: "The address step uses AddressForm from lib/checkout — is the postcode validation in there or in the API? I want the login screen to use the same validator.", branch: "feature/login-redesign", agent: "Claude Code", createdAt: now - 9 * MIN },
      { id: "m5", thread: "m5", from: mert, to: "you@acme.dev", kind: "review", text: "Could you review fix/payment-timeout? Retry is in, with tests for timeouts and 502.", branch: "fix/payment-timeout", agent: null, createdAt: now - 22 * MIN },
    ],
  };
}

export function demoFeed(now = Date.now()): import("../lib/teamchat").FeedEvent[] {
  const e = (id: number, ago: number, kind: import("../lib/teamchat").FeedEvent["kind"], actor: string, branch: string | null, other: string | null, n: number | null, text = "", notify = false, link: string | null = null) => ({
    id,
    projectId: "shop-app",
    ts: now - ago,
    kind,
    actor,
    branch,
    other,
    n,
    text,
    link,
    notify,
  });
  return [
    e(9, 9 * MIN, "message", "Ayşe Yılmaz (Claude Code)", "feature/login-redesign", "question", null, "The address step uses AddressForm from lib/checkout — is the postcode validation in there or in the API?", true, "m4"),
    e(8, 22 * MIN, "review", "Mert Kaya", "fix/payment-timeout", "review", null, "Could you review fix/payment-timeout? Retry is in, with tests for timeouts and 502.", true, "m5"),
    e(7, 25 * MIN, "push", "Mert Kaya", "fix/payment-timeout", null, 2),
    e(6, 26 * MIN, "overlap", "Mert Kaya", "fix/payment-timeout", "feature/checkout-v2", null, "", true),
    e(5, 48 * MIN, "push", "Ayşe Yılmaz", "feature/login-redesign", null, 3),
    e(4, 2 * HOUR, "base", "Deniz Şahin", "origin/main", "behind", 4, "", true),
    e(3, 3 * HOUR, "merged", "Zeynep Arslan", "chore/lints", "origin/main", null),
    e(2, 5 * HOUR, "branch_new", "Ayşe Yılmaz", "feature/login-redesign", null, null),
    e(1, 26 * HOUR, "branch_new", "Zeynep Arslan", "chore/deps", null, null),
  ];
}
