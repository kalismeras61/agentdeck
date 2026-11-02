import { Check, ExternalLink, FolderPlus, KeyRound, Loader2, RefreshCw, TerminalSquare } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { AgentMark, cx } from "../components/ui";
import { errorText, termApi } from "../lib/live";
import { setupApi, type AgentSetupState, type SetupStatus } from "../lib/setup";
import { useStore } from "../state/store";
import { ViewHeader } from "./ViewHeader";

function StatusPill({ a }: { a: AgentSetupState }) {
  if (!a.installed) return <span className="rounded-full bg-elevated px-2 py-0.5 text-[11.5px] text-fg-3">Not installed</span>;
  if (a.error) return <span className="rounded-full bg-warning/12 px-2 py-0.5 text-[11.5px] text-warning">Installed, not answering</span>;
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-success/12 px-2 py-0.5 text-[11.5px] text-success">
      <Check size={11} /> {a.signedIn === true ? "Ready" : "Installed"}
    </span>
  );
}

function AgentCard({ a, onRan }: { a: AgentSetupState; onRan: (msg: string) => void }) {
  const [busy, setBusy] = useState<null | "install" | "sign_in">(null);
  const run = async (action: "install" | "sign_in") => {
    setBusy(action);
    try {
      const cmd = await setupApi.openTerminal(a.id, action);
      onRan(`Terminal is running “${cmd}”. Come back here when it's done.`);
    } catch (e: unknown) {
      onRan(errorText(e));
    }
    setBusy(null);
  };
  const btn = "inline-flex h-8 items-center gap-1.5 rounded-lg px-3 text-[12.5px] disabled:opacity-50";
  return (
    <li className="flex flex-col rounded-2xl border border-line bg-surface/40 p-4">
      <div className="flex items-center gap-3">
        <AgentMark provider={a.id} size={26} />
        <div className="min-w-0">
          <p className="text-[14px] font-medium text-fg">{a.name}</p>
          <p className="text-[12px] text-fg-3">
            {a.maker}
            {a.version ? ` · ${a.version.replace(/\s*\(.*\)$/, "")}` : ""}
          </p>
        </div>
        <span className="ml-auto shrink-0">
          <StatusPill a={a} />
        </span>
      </div>
      <p className="mt-3 text-[12.5px] leading-[1.55] text-fg-2">{a.account}</p>
      {!a.installed && a.install && (
        <code className="mt-2.5 block overflow-x-auto whitespace-nowrap rounded-lg border border-line bg-bg px-2.5 py-1.5 font-mono text-[11.5px] text-fg-2">
          {a.install}
        </code>
      )}
      {!a.installed && !a.install && a.needs && (
        <p className="mt-2.5 text-[12px] leading-[1.5] text-warning">Install {a.needs} first, then press Check again.</p>
      )}
      {a.signedIn === false && <p className="mt-2.5 text-[12px] text-warning">Installed but not signed in.</p>}
      <div className="mt-auto flex flex-wrap items-center gap-2 pt-3.5">
        {!a.installed && a.install && (
          <button type="button" disabled={busy !== null} onClick={() => void run("install")} className={cx(btn, "bg-violet-strong font-medium text-white hover:brightness-110")}>
            {busy === "install" ? <Loader2 size={13} className="animate-spin" /> : <TerminalSquare size={13} />} Install in Terminal
          </button>
        )}
        {a.installed && a.signIn && a.signedIn !== true && (
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => void run("sign_in")}
            className={cx(btn, a.signedIn === false ? "bg-violet-strong font-medium text-white hover:brightness-110" : "border border-line text-fg hover:bg-elevated")}
          >
            {busy === "sign_in" ? <Loader2 size={13} className="animate-spin" /> : <KeyRound size={13} />} Sign in
          </button>
        )}
        <button type="button" onClick={() => void termApi.openLink(a.docs).catch(() => undefined)} className={cx(btn, "text-fg-3 hover:text-fg")}>
          <ExternalLink size={13} /> Docs
        </button>
      </div>
    </li>
  );
}

/** First run: which agents are here, how to install and sign in to the rest, then a project. */
export function Welcome() {
  const setView = useStore((s) => s.setView);
  const setAgyEnabled = useStore((s) => s.setAgyEnabled);
  const addProjectFolder = useStore((s) => s.addProjectFolder);
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const check = useCallback(async () => {
    if (!setupApi.available) return;
    setChecking(true);
    try {
      const s = await setupApi.status();
      setStatus(s);
      setAgyEnabled(s.antigravityEnabled);
    } catch (e: unknown) {
      setNote(errorText(e));
    }
    setChecking(false);
  }, [setAgyEnabled]);

  useEffect(() => {
    void check();
    // Coming back from Terminal: look again.
    const onFocus = () => void check();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [check]);

  const ready = status?.agents.filter((a) => a.installed && !a.error) ?? [];
  const finish = async (thenAddFolder: boolean) => {
    if (setupApi.available) await setupApi.finishWelcome().catch(() => undefined);
    setView("workspace");
    if (thenAddFolder) void addProjectFolder();
  };

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Get started" />
      <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[920px] px-10 pb-16 pt-10">
          <h2 className="text-[24px] font-semibold tracking-[-0.015em] [text-wrap:balance]">Set up your agents</h2>
          <p className="mt-2 max-w-[64ch] text-[14px] leading-[1.6] text-fg-2">
            AgentDeck runs the coding agents you already use, signed in with your own accounts, the same way they run in Terminal. It never sees your
            passwords or tokens. One agent is enough to start.
          </p>

          {!setupApi.available ? (
            <p className="mt-8 rounded-xl border border-line px-4 py-3 text-[13px] text-fg-2">Setup runs in the desktop app; this web preview can't look at your Mac.</p>
          ) : !status ? (
            <p className="mt-8 flex items-center gap-2 text-[13px] text-fg-3">
              <Loader2 size={14} className="animate-spin" /> Looking for agents on this Mac…
            </p>
          ) : (
            <>
              <ul className="mt-8 grid gap-3 md:grid-cols-2">
                {status.agents.map((a) => (
                  <AgentCard key={a.id} a={a} onRan={setNote} />
                ))}
              </ul>
              {!status.antigravityEnabled && (
                <p className="mt-4 text-[12.5px] leading-[1.55] text-fg-3">
                  Google's Antigravity is off by default because Google's terms restrict third-party tools; you can turn it on in Settings → Providers.
                </p>
              )}
            </>
          )}

          {note && <p className="mt-5 rounded-xl border border-violet/30 bg-violet/8 px-4 py-2.5 text-[13px] text-fg-2" role="status">{note}</p>}

          <div className="mt-8 flex flex-wrap items-center gap-3 border-t border-line-soft pt-6">
            <button
              type="button"
              disabled={!setupApi.available || checking}
              onClick={() => void check()}
              className="inline-flex h-9 items-center gap-2 rounded-lg border border-line px-3.5 text-[13px] hover:bg-elevated disabled:opacity-50"
            >
              <RefreshCw size={14} className={cx(checking && "animate-spin motion-reduce:animate-none")} /> Check again
            </button>
            <span className="text-[12.5px] text-fg-3">
              {status ? (ready.length ? `Installed: ${ready.map((a) => a.name).join(", ")}.` : "No agent installed yet.") : ""}
            </span>
            <div className="ml-auto flex items-center gap-2">
              <button type="button" onClick={() => void finish(false)} className="h-9 rounded-lg px-3.5 text-[13px] text-fg-2 hover:bg-elevated hover:text-fg">
                {ready.length ? "Skip" : "Later"}
              </button>
              <button
                type="button"
                disabled={setupApi.available && ready.length === 0}
                onClick={() => void finish(true)}
                className="inline-flex h-9 items-center gap-2 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-45"
              >
                <FolderPlus size={14} /> Open a project folder
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
