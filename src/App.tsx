import * as Tooltip from "@radix-ui/react-tooltip";
import { MotionConfig } from "motion/react";
import { lazy, Suspense, useEffect, useState } from "react";
import { Monitor } from "lucide-react";
import { Composer } from "./components/Composer";

const GitView = lazy(() => import("./views/GitView").then((m) => ({ default: m.GitView })));
const AgentSetup = lazy(() => import("./views/AgentSetup").then((m) => ({ default: m.AgentSetup })));
const Gallery = lazy(() => import("./views/Gallery").then((m) => ({ default: m.Gallery })));
const Review = lazy(() => import("./views/review/Review").then((m) => ({ default: m.Review })));
const Welcome = lazy(() => import("./views/Welcome").then((m) => ({ default: m.Welcome })));
const Settings = lazy(() => import("./views/Settings").then((m) => ({ default: m.Settings })));
// xterm.js only loads when someone opens a terminal.
const TerminalView = lazy(() => import("./components/Terminal").then((m) => ({ default: m.TerminalView })));
import { ContextPanel } from "./components/ContextPanel";
import { Conversation } from "./components/Conversation";
import { Resizer, SessionHeader } from "./components/Shell";
import { Sidebar } from "./components/Sidebar";
import { useSelectedSession } from "./hooks/useSession";
import { useLiveBridge } from "./hooks/useLiveBridge";
import { useShortcuts } from "./hooks/useShortcuts";
import { useMedia } from "./hooks/useTheme";
import { CONTEXT, SIDEBAR, useStore } from "./state/store";
import { Overview } from "./views/Overview";
import { Permissions } from "./views/Permissions";
import { startUpdateChecks } from "./lib/updates";
import { setupApi } from "./lib/setup";

const CENTER_MIN = 560;

function useWidth() {
  const [w, setW] = useState(() => window.innerWidth);
  useEffect(() => {
    const on = () => setW(window.innerWidth);
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return w;
}

function Workspace({ width }: { width: number }) {
  const session = useSelectedSession();
  const sidebarOpen = useStore((s) => s.sidebarOpen);
  const sidebarWidth = useStore((s) => s.sidebarWidth);
  const setSidebarWidth = useStore((s) => s.setSidebarWidth);
  const contextOpen = useStore((s) => s.contextOpen);
  const contextWidth = useStore((s) => s.contextWidth);
  const setContextWidth = useStore((s) => s.setContextWidth);
  const toggleContext = useStore((s) => s.toggleContext);
  const contextOverlay = useStore((s) => s.contextOverlay);
  const setNarrow = useStore((s) => s.setNarrow);
  const viewMode = useStore((s) => s.viewMode);
  const view = useStore((s) => s.view);
  const theme = useStore((s) => s.theme);
  const motion = useStore((s) => s.motion);
  const prefersLight = useMedia("(prefers-color-scheme: light)");
  const prefersReduced = useMedia("(prefers-reduced-motion: reduce)");
  useShortcuts();
  useLiveBridge();
  useEffect(() => startUpdateChecks(), []);
  // First run: show Get started. Also learns whether Antigravity is turned on.
  useEffect(() => {
    if (!setupApi.available) return;
    void setupApi
      .status()
      .then((st) => {
        useStore.getState().setAgyEnabled(st.antigravityEnabled);
        if (!st.welcomeDone) useStore.getState().setView("welcome");
      })
      .catch(() => undefined);
  }, []);

  const resolvedTheme = theme === "system" ? (prefersLight ? "light" : "dark") : theme;
  const reduced = motion === "reduced" || (motion === "system" && prefersReduced);
  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    document.documentElement.dataset.motion = reduced ? "reduced" : "full";
  }, [resolvedTheme, reduced]);

  // Keep the conversation at least CENTER_MIN wide: when space runs out the panel floats over it instead.
  const sideW = sidebarOpen ? sidebarWidth + 1 : 0;
  const docked = width - sideW - (contextWidth + 1) >= CENTER_MIN;
  useEffect(() => setNarrow(!docked), [docked, setNarrow]);

  return (
    <MotionConfig reducedMotion={reduced ? "always" : motion === "full" ? "never" : "user"}>
      <Tooltip.Provider>
        <div className="flex h-full w-full overflow-hidden bg-bg text-fg">
          {sidebarOpen && (
            <>
              <aside style={{ width: sidebarWidth }} className="h-full shrink-0">
                <Sidebar />
              </aside>
              <Resizer
                side="left"
                label="Resize sidebar"
                value={sidebarWidth}
                min={SIDEBAR.min}
                max={SIDEBAR.max}
                def={SIDEBAR.def}
                onChange={setSidebarWidth}
              />
            </>
          )}

          <main className="relative flex min-w-0 flex-1 flex-col" style={{ minWidth: Math.min(CENTER_MIN, width - sideW) }}>
            <Suspense fallback={null}>
              {view === "overview" ? (
                <Overview />
              ) : view === "review" ? (
                <Review />
              ) : view === "permissions" ? (
                <Permissions />
              ) : view === "gallery" ? (
                <Gallery />
              ) : view === "welcome" ? (
                <Welcome />
              ) : view === "setup" ? (
                <AgentSetup />
              ) : view === "git" ? (
                <GitView />
              ) : view === "settings" ? (
                <Settings />
              ) : session ? (
                <>
                  <SessionHeader session={session} />
                  <div className="min-h-0 flex-1">
                    {viewMode === "terminal" ? (
                      <Suspense fallback={null}>
                        <TerminalView session={session} />
                      </Suspense>
                    ) : (
                      <Conversation session={session} />
                    )}
                  </div>
                  {viewMode === "rich" && <Composer session={session} />}
                </>
              ) : (
                <NoSession />
              )}
            </Suspense>

            {view === "workspace" && session && contextOverlay && !docked && (
              <>
                <button type="button" aria-label="Close panel" onClick={toggleContext} className="absolute inset-0 z-20 bg-black/40" />
                <aside style={{ width: Math.min(contextWidth, width - 24) }} className="absolute bottom-0 right-0 top-0 z-30 border-l border-line shadow-pop">
                  <ContextPanel session={session} />
                </aside>
              </>
            )}
          </main>

          {view === "workspace" && session && contextOpen && docked && (
            <>
              <Resizer
                side="right"
                label="Resize context panel"
                value={contextWidth}
                min={CONTEXT.min}
                max={CONTEXT.max}
                def={CONTEXT.def}
                onChange={setContextWidth}
              />
              <aside style={{ width: contextWidth }} className="h-full shrink-0">
                <ContextPanel session={session} />
              </aside>
            </>
          )}
        </div>
      </Tooltip.Provider>
    </MotionConfig>
  );
}

function NoSession() {
  const ws = useStore((s) => s.workspaceId);
  const projects = useStore((s) => s.projects);
  const addProjectFolder = useStore((s) => s.addProjectFolder);
  const newSession = useStore((s) => s.newSession);
  const hasProjects = projects.some((p) => p.workspaceId === ws);
  return (
    <div className="flex h-full flex-col">
      <div data-tauri-drag-region className="drag h-12 shrink-0 border-b border-line-soft" />
      <div className="flex flex-1 items-center justify-center px-8 pb-12">
        <div className="max-w-[440px]">
          <h2 className="text-[19px] font-semibold tracking-[-0.01em]">{hasProjects ? "Start a session" : "Add your first project"}</h2>
          <p className="mt-2 text-[14px] leading-[1.6] text-fg-2">
            {ws === "local"
              ? hasProjects
                ? "Pick a session in the sidebar, or start a new Claude Code session in one of your projects."
                : "Choose a folder on this Mac. Claude Code runs in that folder with your own sign-in, and asks here before it edits files or runs commands."
              : "Pick a demo session in the sidebar."}
          </p>
          {ws === "local" && (
            <button
              type="button"
              onClick={() => (hasProjects ? newSession() : void addProjectFolder())}
              className="mt-5 h-9 rounded-lg bg-violet-strong px-4 text-[13px] font-medium text-white hover:brightness-110"
            >
              {hasProjects ? "New Claude Code session" : "Add project folder"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

const DESIGN_WIDTH = 1280;
const SMALL = 900;

/**
 * AgentDeck is a desktop app (the window's minimum is 1000px). On a phone-sized web preview,
 * explain that and offer a scaled, view-only fit instead of a broken layout.
 */
export default function App() {
  const width = useWidth();
  const [fit, setFit] = useState(false);
  const isTauri = "__TAURI_INTERNALS__" in window;
  if (isTauri || width >= SMALL) return <Workspace width={width} />;

  if (!fit) {
    return (
      <div className="flex h-full items-center bg-bg px-6 text-fg">
        <div className="mx-auto max-w-[360px]">
          <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-xl bg-violet-soft text-violet-text">
            <Monitor size={18} />
          </div>
          <h1 className="text-[19px] font-semibold tracking-[-0.01em]">AgentDeck is a desktop app</h1>
          <p className="mt-2 text-[14px] leading-[1.6] text-fg-2">
            Its window starts at 1000px wide. Open this preview on a computer to click through it, or see the whole layout scaled down here.
          </p>
          <button type="button" onClick={() => setFit(true)} className="mt-5 h-9 rounded-lg bg-violet-strong px-4 text-[13px] font-medium text-white">
            Show scaled layout
          </button>
        </div>
      </div>
    );
  }
  const scale = width / DESIGN_WIDTH;
  return (
    <div className="h-full overflow-hidden bg-bg">
      <div style={{ width: DESIGN_WIDTH, height: window.innerHeight / scale, transform: `scale(${scale})`, transformOrigin: "top left" }}>
        <Workspace width={DESIGN_WIDTH} />
      </div>
    </div>
  );
}
