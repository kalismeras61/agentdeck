import { agentLabel } from "../lib/agentName";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal as XTerm, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { CornerDownLeft, KeyRound, MessageSquareText, PanelRightClose, PanelRightOpen, RotateCcw, Sparkles, SquareTerminal, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { providerLabel, type Session } from "../data/demo";
import { useParsedEvents } from "../hooks/useSession";
import { useProject } from "../hooks/useWorkspace";
import { t } from "../lib/i18n";
import { errorText, liveApi, subscribeTerm, termApi } from "../lib/live";
import { useStore } from "../state/store";
import { AgentMark, cx, IconButton, Tip, withNodes } from "./ui";

/* eslint-disable react-hooks/exhaustive-deps */

const ANSI = /\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[PX^_][^\x1b]*\x1b\\|\x1b[@-Z\\-_]/g; // eslint-disable-line no-control-regex
export const stripAnsi = (s: string) => s.replace(ANSI, "");

/** Lines that look like a failure worth asking about. */
export const ERROR_LINE = /\b(error|failed|failure|exception|traceback|panicked|fatal|segmentation fault|command not found|no such file)\b/i;

function themeFromCss(): ITheme {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  return {
    background: v("--bg", "#0b0d14"),
    foreground: v("--text", "#f4f5fa"),
    cursor: v("--violet", "#8b5cf6"),
    cursorAccent: v("--bg", "#0b0d14"),
    selectionBackground: "rgba(139, 92, 246, 0.35)",
    black: v("--elevated", "#202438"),
    brightBlack: v("--text-3", "#8a91a7"),
    red: v("--error", "#fb7185"),
    brightRed: v("--error", "#fb7185"),
    green: v("--success", "#34d399"),
    brightGreen: v("--success", "#34d399"),
    yellow: v("--warning", "#fbbf24"),
    brightYellow: v("--warning", "#fbbf24"),
    blue: v("--blue", "#60a5fa"),
    brightBlue: v("--blue", "#60a5fa"),
    magenta: v("--violet-text", "#c4b5fd"),
    brightMagenta: v("--violet-text", "#c4b5fd"),
    cyan: "#67e8f9",
    brightCyan: "#a5f3fc",
    white: v("--text-2", "#9ca3b8"),
    brightWhite: v("--text", "#f4f5fa"),
  };
}

/** The last `n` non-empty lines of the terminal, without colors. */
function tailText(term: XTerm, n: number): string {
  const buf = term.buffer.active;
  const lines: string[] = [];
  for (let i = buf.length - 1; i >= 0 && lines.length < n; i--) {
    const line = buf.getLine(i)?.translateToString(true) ?? "";
    if (lines.length || line.trim()) lines.push(line);
  }
  return lines.reverse().join("\n").trimEnd();
}

/** Commands the agent ran in this session, newest first, without duplicates. */
function useAgentCommands(session: Session): string[] {
  const events = useParsedEvents(session);
  return useMemo(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (let i = events.length - 1; i >= 0; i--) {
      const p = events[i]!;
      if (!p.ok || p.event.type !== "tool_group") continue;
      for (const item of [...p.event.payload.items].reverse()) {
        if ((item.kind === "command" || item.kind === "test") && item.target && !seen.has(item.target)) {
          seen.add(item.target);
          out.push(item.target);
        }
      }
    }
    return out.slice(0, 30);
  }, [events]);
}

function WebOnly({ session }: { session: Session }) {
  const setMode = useStore((s) => s.setViewMode);
  return (
    <div className="flex h-full items-center justify-center px-8">
      <div className="max-w-[460px]">
        <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-xl bg-elevated text-fg-2">
          <SquareTerminal size={18} />
        </div>
        <h2 className="text-[17px] font-semibold tracking-[-0.01em]">{t("Terminal needs the desktop app")}</h2>
        <p className="mt-2 text-[13.5px] leading-[1.65] text-fg-2">
          {t("A real shell runs through a pseudo-terminal inside the AgentDeck desktop app. This web preview can't open one, so nothing is simulated here.")}
        </p>
        <p className="mt-3 text-[13px] leading-[1.65] text-fg-3">
          {t("{agent} talks to AgentDeck through a structured interface; the terminal is a separate shell in the project folder.", { agent: agentLabel(session) })}
        </p>
        <button type="button" onClick={() => setMode("rich")} className="mt-5 h-8 rounded-lg border border-line px-3 text-[13px] text-fg hover:bg-elevated">
          {t("Back to rich view")}
        </button>
      </div>
    </div>
  );
}

export function TerminalView({ session }: { session: Session }) {
  if (!liveApi.available || !session.live) return <WebOnly session={session} />;
  return <LiveTerminal key={session.id} session={session} />;
}

function LiveTerminal({ session }: { session: Session }) {
  const project = useProject(session.projectId);
  const termId = useStore((s) => s.terminals[session.id]);
  const setTerminal = useStore((s) => s.setTerminal);
  const sendPrompt = useStore((s) => s.sendPrompt);
  const select = useStore((s) => s.select);
  const setMode = useStore((s) => s.setViewMode);
  const sessions = useStore((s) => s.sessions);
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<XTerm | null>(null);
  const idRef = useRef<string | null>(termId ?? null);
  const [error, setError] = useState<string | null>(null);
  const [exited, setExited] = useState<number | null | undefined>(undefined);
  const [shell, setShell] = useState("");
  const [sawError, setSawError] = useState(false);
  const [asking, setAsking] = useState(false);
  const [question, setQuestion] = useState("");
  const [target, setTarget] = useState(session.id);
  const commands = useAgentCommands(session);
  const [panel, setPanel] = useState(commands.length > 0);
  const peers = sessions.filter((s) => s.live && s.projectId === session.projectId);
  const [restart, setRestart] = useState(0);
  const signIn = useStore((s) => s.signIns[session.id]);
  const markSignInStarted = useStore((s) => s.markSignInStarted);
  const finishSignIn = useStore((s) => s.finishSignIn);
  const [ready, setReady] = useState<string | null>(null);
  const [signInCmd, setSignInCmd] = useState<string | null>(null);

  // A sign-in asked for from the conversation: run the agent's own login once the shell is up.
  useEffect(() => {
    if (!signIn || signIn.started || !ready) return;
    markSignInStarted(session.id);
    termApi
      .signIn(ready, signIn.agent)
      .then((cmd) => {
        setSignInCmd(cmd);
        term.current?.focus();
      })
      .catch((e: unknown) => setError(errorText(e)));
  }, [signIn, ready]);

  useEffect(() => {
    if (!host.current) return;
    const t = new XTerm({
      fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim() || "ui-monospace, Menlo, monospace",
      fontSize: 13,
      lineHeight: 1.25,
      cursorBlink: true,
      scrollback: 5000,
      allowProposedApi: false,
      theme: themeFromCss(),
    });
    const fit = new FitAddon();
    t.loadAddon(fit);
    // Links the CLIs print (sign-in pages) open in your browser.
    t.loadAddon(new WebLinksAddon((_ev, uri) => void termApi.openLink(uri).catch((e: unknown) => setError(errorText(e)))));
    t.open(host.current);
    term.current = t;
    try {
      fit.fit();
    } catch {
      /* not laid out yet */
    }
    let off: (() => void) | undefined;
    let disposed = false;

    const onOutput = (id: string, data: string) => {
      if (id !== idRef.current) return;
      t.write(data);
      if (ERROR_LINE.test(stripAnsi(data))) setSawError(true);
    };
    (async () => {
      off = await subscribeTerm({
        onOutput,
        onExit: (id, code) => {
          if (id === idRef.current) setExited(code);
        },
      });
      try {
        let info = idRef.current ? await termApi.info(idRef.current).catch(() => null) : null;
        if (!info) {
          info = await termApi.open(session.projectId, t.cols, t.rows, session.id);
          idRef.current = info.termId;
          setTerminal(session.id, info.termId);
        } else {
          t.write(info.scrollback);
          if (ERROR_LINE.test(stripAnsi(info.scrollback.slice(-4000)))) setSawError(true);
          if (!info.running) setExited(null);
          void termApi.resize(info.termId, t.cols, t.rows);
        }
        if (disposed) return;
        setShell(info.shell.split("/").pop() ?? info.shell);
        setReady(info.termId);
        t.focus();
      } catch (e) {
        setError(errorText(e));
      }
    })();

    const input = t.onData((d) => {
      if (idRef.current) void termApi.write(idRef.current, d).catch((e: unknown) => setError(errorText(e)));
    });
    const ro = new ResizeObserver(() => {
      try {
        fit.fit();
        if (idRef.current) void termApi.resize(idRef.current, t.cols, t.rows);
      } catch {
        /* hidden */
      }
    });
    ro.observe(host.current);
    // Follow the app theme.
    const mo = new MutationObserver(() => (t.options.theme = themeFromCss()));
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      disposed = true;
      off?.();
      input.dispose();
      ro.disconnect();
      mo.disconnect();
      t.dispose();
      term.current = null;
    };
  }, [restart]);

  const send = (text: string) => {
    const t = term.current;
    if (!t) return;
    const picked = t.hasSelection() ? t.getSelection() : tailText(t, 80);
    const out = stripAnsi(picked).trim();
    const q = text.trim() || "What went wrong here, and how do I fix it?";
    const where = session.worktree ?? project?.path ?? "the project folder";
    const msg = `${q}\n\nOutput from my terminal in ${where}:\n\n\`\`\`\n${out.slice(-20_000)}\n\`\`\``;
    sendPrompt(target, msg, []);
    setAsking(false);
    setQuestion("");
    setSawError(false);
    select(target);
    setMode("rich");
  };

  const insert = (cmd: string) => {
    if (idRef.current) void termApi.write(idRef.current, cmd);
    term.current?.focus();
  };

  const reopen = () => {
    if (idRef.current) void termApi.close(idRef.current);
    idRef.current = null;
    setTerminal(session.id, null);
    setExited(undefined);
    setError(null);
    setRestart((n) => n + 1);
  };

  const targetSession = peers.find((s) => s.id === target) ?? session;

  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-line-soft px-4 py-1.5 text-[12.5px]">
          <SquareTerminal size={14} className="text-fg-3" />
          <span className="text-fg">{shell || t("Shell")}</span>
          <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-fg-3" title={session.worktree ?? project?.path}>
            {(session.worktree ?? project?.path)?.replace(/^\/(Users|home)\/[^/]+/, "~")}
          </span>
          {exited !== undefined && <span className="text-fg-3">· {exited !== null ? t("ended (exit {code})", { code: exited }) : t("ended")}</span>}
          <span className="flex shrink-0 items-center gap-1">
            {sawError && !asking && (
              <button
                type="button"
                onClick={() => send("")}
                className="inline-flex h-7 items-center gap-1.5 rounded-lg bg-warning/10 px-2.5 font-medium text-warning hover:bg-warning/15"
              >
                <Sparkles size={13} /> {t("Explain this error")}
              </button>
            )}
            <button
              type="button"
              onClick={() => setAsking((v) => !v)}
              aria-expanded={asking}
              className="inline-flex h-7 items-center gap-1.5 rounded-lg px-2.5 text-fg-2 hover:bg-elevated hover:text-fg aria-expanded:bg-elevated aria-expanded:text-fg"
            >
              <MessageSquareText size={13} /> {t("Ask an agent")}
            </button>
            <IconButton label={t("Clear the screen")} size="sm" onClick={() => term.current?.clear()}>
              <Trash2 size={14} />
            </IconButton>
            <IconButton label={t("Restart the shell")} size="sm" onClick={reopen}>
              <RotateCcw size={14} />
            </IconButton>
            <IconButton label={panel ? t("Hide the agent's commands") : t("Show the agent's commands")} size="sm" onClick={() => setPanel((v) => !v)}>
              {panel ? <PanelRightClose size={14} /> : <PanelRightOpen size={14} />}
            </IconButton>
          </span>
        </div>
        {asking && (
          <form
            className="flex flex-wrap items-center gap-2 border-b border-line-soft bg-surface/60 px-4 py-2"
            onSubmit={(ev) => {
              ev.preventDefault();
              send(question);
            }}
          >
            <select
              aria-label={t("Agent to ask")}
              value={target}
              onChange={(ev) => setTarget(ev.target.value)}
              className="h-8 rounded-lg border border-line bg-surface px-2 text-[12.5px] text-fg outline-none focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]"
            >
              {peers.map((s) => (
                <option key={s.id} value={s.id}>
                  {providerLabel[s.provider]}: {s.title}
                </option>
              ))}
            </select>
            <input
              autoFocus
              value={question}
              onChange={(ev) => setQuestion(ev.target.value)}
              placeholder={t("What went wrong here, and how do I fix it?")}
              aria-label={t("Question")}
              className="h-8 min-w-[220px] flex-1 rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]"
            />
            <button type="submit" className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white hover:brightness-110">
              <AgentMark provider={targetSession.provider} size={16} /> {t("Send")}
            </button>
            <span className="basis-full text-[11.5px] text-fg-3">
              {t("Sends your selection, or the last {n} lines if nothing is selected, to {agent} as a new message.", { n: 80, agent: providerLabel[targetSession.provider] })}
            </span>
          </form>
        )}
        {signIn && (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-[color-mix(in_srgb,var(--violet)_35%,var(--border))] bg-violet-soft px-4 py-2.5 text-[12.5px]">
            <KeyRound size={14} className="shrink-0 text-violet-text" />
            <span className="min-w-0 flex-1 leading-[1.5] text-fg">
              {signIn.agent === "antigravity" && session.provider !== "antigravity" ? (
                withNodes(t("Antigravity runs here as Google's own app; you use it yourself, AgentDeck doesn't drive it. Not installed? Run {cmd}."), {
                  cmd: <code className="font-mono text-[11.5px]">brew install antigravity-cli</code>,
                })
              ) : (
                <>
                  {signInCmd
                    ? withNodes(t("Sign in below with {agent}'s own login ({cmd}).", { agent: signInLabel(signIn.agent) }), {
                        cmd: <code className="font-mono text-[11.5px]">{signInCmd}</code>,
                      })
                    : t("Sign in below with {agent}'s own login.", { agent: signInLabel(signIn.agent) })}
                  {signIn.agent === "gemini-cli" ? (
                    <>
                      {" "}
                      {withNodes(t("Type {auth} to choose a Google Workspace account or Cloud project, then {quit}."), {
                        auth: <code className="font-mono text-[11.5px]">/auth</code>,
                        quit: <code className="font-mono text-[11.5px]">/quit</code>,
                      })}
                    </>
                  ) : signIn.agent === "antigravity" ? (
                    <>
                      {" "}
                      {withNodes(t("Sign in if it asks, or type {cmd} to allow actions."), {
                        cmd: <code className="font-mono text-[11.5px]">/permissions</code>,
                      })}
                    </>
                  ) : null}{" "}
                  {t("Links open in your browser. When you're done, come back here.")}
                </>
              )}
            </span>
            {(signIn.agent !== "antigravity" || session.provider === "antigravity") && (
              <button
                type="button"
                onClick={() => finishSignIn(session.id, true)}
                className="h-7 rounded-lg bg-violet-strong px-3 font-medium text-white hover:brightness-110"
              >
                {t("I'm signed in, try again")}
              </button>
            )}
            <button type="button" onClick={() => finishSignIn(session.id, false)} className="h-7 rounded-lg px-2.5 text-fg-2 hover:bg-elevated hover:text-fg">
              {signIn.agent === "antigravity" && session.provider !== "antigravity" ? t("Back to the conversation") : t("Cancel")}
            </button>
          </div>
        )}
        {error && (
          <div role="alert" className="flex items-center gap-2 border-b border-error/30 bg-error/[0.06] px-4 py-2 text-[12.5px] text-fg">
            {error}
            <button type="button" onClick={reopen} className="ml-auto rounded-md px-2 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg">
              {t("Try again")}
            </button>
          </div>
        )}
        <div className="min-h-0 flex-1 bg-bg px-3 py-2">
          <div ref={host} className="h-full w-full" />
        </div>
      </div>
      {panel && (
        <aside aria-label={t("Commands the agent ran")} className="flex w-[230px] shrink-0 flex-col border-l border-line-soft">
          <div className="border-b border-line-soft px-3.5 py-2 text-[12.5px] text-fg">{t("{agent} ran", { agent: agentLabel(session) })}</div>
          <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto p-2">
            {commands.length === 0 ? (
              <p className="px-1.5 py-1 text-[12px] leading-[1.55] text-fg-3">{t("Commands the agent runs in this session show up here, so you can rerun them yourself.")}</p>
            ) : (
              <ul className="space-y-1">
                {commands.map((c) => (
                  <li key={c}>
                    <Tip label={t("Type it into the terminal (you press Return)")}>
                      <button
                        type="button"
                        onClick={() => insert(c)}
                        className={cx("group flex w-full items-start gap-2 rounded-lg px-2 py-1.5 text-left hover:bg-surface")}
                      >
                        <code className="min-w-0 flex-1 break-all font-mono text-[11.5px] leading-[1.5] text-fg-2 group-hover:text-fg">{c}</code>
                        <CornerDownLeft size={12} className="mt-0.5 shrink-0 text-fg-3 opacity-0 group-hover:opacity-100" />
                      </button>
                    </Tip>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </aside>
      )}
    </div>
  );
}

function signInLabel(agent: string) {
  return agent === "antigravity" ? "Antigravity" : (providerLabel as Record<string, string>)[agent] ?? agent;
}
