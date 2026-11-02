import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { parseEvents } from "../lib/events";
import { useStore } from "../state/store";

const initial = useStore.getState();

function permissionState(sessionId: string, requestId: string) {
  const s = useStore.getState().sessions.find((x) => x.id === sessionId)!;
  for (const p of parseEvents(s.raw)) if (p.ok && p.event.type === "permission" && p.event.payload.requestId === requestId) return p.event.payload.state;
  return undefined;
}

describe("store", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useStore.setState(initial, true);
  });
  afterEach(() => vi.useRealTimers());

  it("routes a permission decision to its own request and waits for acknowledgement", () => {
    useStore.getState().resolvePermission("s-login", "perm-1", "deny");
    expect(useStore.getState().permissionInFlight["perm-1"]).toBe("deny");
    expect(permissionState("s-login", "perm-1")).toBe("pending");
    vi.runAllTimers();
    expect(permissionState("s-login", "perm-1")).toBe("denied");
    expect(useStore.getState().permissionInFlight["perm-1"]).toBeUndefined();
  });

  it("ignores a second click while a decision is in flight", () => {
    const st = useStore.getState();
    st.resolvePermission("s-login", "perm-1", "allow");
    st.resolvePermission("s-login", "perm-1", "deny");
    vi.runAllTimers();
    expect(permissionState("s-login", "perm-1")).toBe("allowed");
  });

  it("keeps sessions isolated when sending prompts", () => {
    const before = useStore.getState().sessions.find((s) => s.id === "s-keyboard")!.raw.length;
    useStore.getState().sendPrompt("s-login", "hello", []);
    const after = useStore.getState().sessions;
    expect(after.find((s) => s.id === "s-keyboard")!.raw.length).toBe(before);
    expect(parseEvents(after.find((s) => s.id === "s-login")!.raw).every((p) => p.ok)).toBe(true);
  });

  it("ignores empty prompts", () => {
    const before = useStore.getState().sessions.find((s) => s.id === "s-login")!.raw.length;
    useStore.getState().sendPrompt("s-login", "   ", []);
    expect(useStore.getState().sessions.find((s) => s.id === "s-login")!.raw.length).toBe(before);
  });

  it("creates a new empty session and selects it", () => {
    useStore.getState().newSession("p-app", "codex");
    const st = useStore.getState();
    const s = st.sessions.find((x) => x.id === st.selectedId)!;
    expect(s).toMatchObject({ provider: "codex", projectId: "p-app", raw: [] });
  });

  it("clamps panel widths", () => {
    useStore.getState().setSidebarWidth(9999);
    useStore.getState().setContextWidth(10);
    expect(useStore.getState().sidebarWidth).toBe(360);
    expect(useStore.getState().contextWidth).toBe(300);
  });
});

describe("store, milestone 2", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useStore.setState(initial, true);
  });
  afterEach(() => vi.useRealTimers());

  it("records a session-scoped approval on the right request only", () => {
    useStore.getState().resolvePermission("s-keyboard", "perm-2", "allow_session");
    vi.runAllTimers();
    expect(permissionState("s-keyboard", "perm-2")).toBe("allowed_session");
    expect(permissionState("s-login", "perm-1")).toBe("pending");
    expect(useStore.getState().sessions.find((s) => s.id === "s-keyboard")!.status).toBe("idle");
  });

  it("opens review on a given file and toggles local viewed marks", () => {
    const st = useStore.getState();
    st.openReview("s-login", "lib/features/auth/login_screen.dart");
    expect(useStore.getState()).toMatchObject({ view: "review", reviewSessionId: "s-login", reviewPath: "lib/features/auth/login_screen.dart" });
    st.toggleViewed("s-login", "a.dart");
    expect(useStore.getState().viewed["s-login::a.dart"]).toBe(true);
    st.toggleViewed("s-login", "a.dart");
    expect(useStore.getState().viewed["s-login::a.dart"]).toBeUndefined();
  });

  it("selecting a session returns to the workspace view", () => {
    useStore.getState().setView("settings");
    useStore.getState().select("s-riverpod");
    expect(useStore.getState()).toMatchObject({ view: "workspace", selectedId: "s-riverpod" });
  });

  it("new sessions use the default agent and model", () => {
    const st = useStore.getState();
    st.setDefaultProvider("codex");
    st.newSession("p-app");
    const s = useStore.getState().sessions[0]!;
    expect(s).toMatchObject({ provider: "codex", model: "gpt-5-codex" });
  });
});

describe("store, live sessions", () => {
  beforeEach(() => useStore.setState(initial, true));

  const row = {
    id: "s-live",
    projectId: "p-live",
    provider: "claude-code" as const,
    model: "sonnet",
    title: "Live",
    status: "running" as const,
    snippet: "",
    providerSessionId: null,
    createdAt: 1,
    updatedAt: 2,
    mode: "default" as const,
    profileId: null,
  };
  const proj = { id: "p-live", name: "demo-project", path: "/Users/ada/demo-project", kind: "folder" as const, createdAt: 1 };
  const ev = (seq: number, kind: string, payload: Record<string, unknown>, source: "claude" | "agentdeck" = "claude") => ({
    sessionId: "s-live",
    seq,
    ts: seq,
    source,
    kind,
    payload,
  });

  it("hydrates projects and sessions into the local workspace without touching demo data", () => {
    useStore.getState().hydrateLive([proj], [row], { "s-live": [ev(1, "user_message", { text: "hi" }, "agentdeck")] });
    const st = useStore.getState();
    expect(st.projects.find((p) => p.id === "p-live")?.workspaceId).toBe("local");
    expect(st.sessions.find((s) => s.id === "s-live")).toMatchObject({ live: true, demo: false });
    expect(st.sessions.filter((s) => s.demo).length).toBe(initial.sessions.length);
    expect(st.liveState).toBe("ready");
  });

  it("loads an unopened session's history on demand, keeping events that arrive meanwhile", async () => {
    const { liveApi } = await import("../lib/live");
    let release: (v: ReturnType<typeof ev>[]) => void = () => {};
    const spy = vi.spyOn(liveApi, "loadEvents").mockImplementation(() => new Promise((r) => (release = r as typeof release)));
    const st = useStore.getState();
    st.hydrateLive([proj], [row], {}); // history not loaded yet
    expect(useStore.getState().liveEvents["s-live"]).toBeUndefined();
    st.appendLiveEvent(ev(3, "user_message", { text: "third" }, "agentdeck")); // starts the load
    st.appendLiveEvent(ev(4, "user_message", { text: "fourth" }, "agentdeck")); // arrives while loading
    release([ev(1, "user_message", { text: "first" }, "agentdeck"), ev(3, "user_message", { text: "third" }, "agentdeck")]);
    await vi.waitFor(() => expect(useStore.getState().liveEvents["s-live"]?.map((e) => e.seq)).toEqual([1, 3, 4]));
    expect(spy).toHaveBeenCalledTimes(1);
    await st.ensureEvents("s-live"); // already loaded: no second read
    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it("loads the newest sessions' history at startup and the rest when opened", async () => {
    const { eagerRows } = await import("../hooks/useLiveBridge");
    const rows = Array.from({ length: 40 }, (_, i) => ({ id: `s${i}`, updatedAt: i, status: i === 2 ? "waiting" : "idle" }));
    const eager = eagerRows(rows).map((r) => r.id);
    expect(eager.length).toBe(31);
    expect(eager[0]).toBe("s39");
    expect(eager).toContain("s2"); // waiting for an answer, so loaded even though it's old
  });

  it("appends streamed events once, in order, and re-derives the conversation", () => {
    const st = useStore.getState();
    st.hydrateLive([proj], [row], { "s-live": [] });
    st.appendLiveEvent(ev(2, "assistant", { message: { content: [{ type: "text", text: "second" }] } }));
    st.appendLiveEvent(ev(1, "user_message", { text: "first" }, "agentdeck"));
    st.appendLiveEvent(ev(2, "assistant", { message: { content: [{ type: "text", text: "second" }] } }));
    const s = useStore.getState().sessions.find((x) => x.id === "s-live")!;
    expect((s.raw as { type: string }[]).map((e) => e.type)).toEqual(["user_message", "assistant_text"]);
  });

  it("clears an in-flight decision when the stored decision comes back, and streaming text on the reply", () => {
    const st = useStore.getState();
    st.hydrateLive([proj], [row], { "s-live": [] });
    useStore.setState({ permissionInFlight: { r1: "allow" } });
    st.applyDelta("s-live", "Hel", false);
    st.applyDelta("s-live", "lo", false);
    expect(useStore.getState().streaming["s-live"]).toBe("Hello");
    st.appendLiveEvent(ev(1, "permission_decision", { requestId: "r1", decision: "allow" }, "agentdeck"));
    expect(useStore.getState().permissionInFlight.r1).toBeUndefined();
    st.appendLiveEvent(ev(2, "assistant", { message: { content: [{ type: "text", text: "Hello" }] } }));
    expect(useStore.getState().streaming["s-live"]).toBe("");
  });
});

describe("notifications", () => {
  it("tells you only about changes worth leaving the window for", async () => {
    const { notificationFor } = await import("../lib/notify");
    const prefs = { approval: true, finished: true, errors: true };
    const row = {
      id: "s",
      projectId: "p",
      provider: "codex" as const,
      model: "default",
      title: "Fix tax",
      status: "waiting" as const,
      snippet: "Wants to run a command",
      providerSessionId: null,
      createdAt: 1,
      updatedAt: 2,
      mode: "default" as const,
      profileId: null,
    };
    expect(notificationFor("running", row, prefs)?.title).toBe("Codex needs you");
    expect(notificationFor("waiting", row, prefs)).toBeNull();
    expect(notificationFor("running", { ...row, status: "idle" }, prefs)?.title).toBe("Codex finished");
    expect(notificationFor("idle", { ...row, status: "idle" }, prefs)).toBeNull();
    expect(notificationFor("running", { ...row, status: "error" }, { ...prefs, errors: false })).toBeNull();
  });
});
