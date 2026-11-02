import { ArrowDownToLine, Loader2, X } from "lucide-react";
import { useUpdates } from "../lib/updates";
import { useStore } from "../state/store";

function mb(n: number) {
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** What installing does to work in progress, said before you click. */
export function useRunningAgents(): number {
  return useStore((s) => s.sessions.filter((x) => x.live && (x.status === "running" || x.status === "waiting")).length);
}

export function UpdateProgress() {
  const { phase, progress } = useUpdates();
  if (phase === "installing") return <span className="text-[12px] text-fg-2">Installing; AgentDeck restarts in a moment…</span>;
  if (phase !== "downloading" || !progress) return null;
  const pct = progress.total ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : null;
  return (
    <span className="flex items-center gap-2 text-[12px] text-fg-2">
      <span className="h-1.5 w-24 overflow-hidden rounded-full bg-surface" role="progressbar" aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100}>
        <span className="block h-full bg-violet transition-[width]" style={{ width: `${pct ?? 30}%` }} />
      </span>
      {mb(progress.downloaded)}
      {progress.total ? ` of ${mb(progress.total)}` : ""}
    </span>
  );
}

/** Sits above Settings in the sidebar when a newer AgentDeck is out. */
export function UpdateNotice() {
  const { info, phase, dismissed, install, dismiss, error } = useUpdates();
  const running = useRunningAgents();
  const version = info?.available;
  if (!version || dismissed === version) return null;
  const busy = phase === "downloading" || phase === "installing";
  return (
    <div className="mx-3 mb-2 rounded-xl border border-violet/40 bg-violet/10 px-3 py-2.5">
      <div className="flex items-start gap-2">
        <ArrowDownToLine size={14} className="mt-0.5 shrink-0 text-violet-text" />
        <div className="min-w-0 flex-1">
          <p className="text-[12.5px] font-medium text-fg">AgentDeck {version} is ready</p>
          <p className="mt-0.5 text-[11.5px] leading-[1.45] text-fg-3">
            {running > 0
              ? `Restarting stops ${running === 1 ? "the agent that's working" : `the ${running} agents that are working`}; their history stays.`
              : "Installs and restarts in under a minute."}
          </p>
          {busy ? (
            <div className="mt-2">
              <UpdateProgress />
            </div>
          ) : (
            <button
              type="button"
              onClick={() => void install()}
              className="mt-2 inline-flex h-7 items-center gap-1.5 rounded-lg bg-violet-strong px-2.5 text-[12px] font-medium text-white hover:brightness-110"
            >
              {phase === "error" ? "Try again" : "Install and restart"}
            </button>
          )}
          {phase === "error" && error && <p className="mt-1.5 text-[11.5px] text-error">{error}</p>}
        </div>
        {!busy && (
          <button type="button" onClick={dismiss} aria-label="Remind me later" title="Remind me later" className="rounded p-0.5 text-fg-3 hover:text-fg">
            <X size={13} />
          </button>
        )}
      </div>
    </div>
  );
}

/** The Updates rows in Settings. */
export function UpdateSettings() {
  const { info, phase, error, checkedAt, check, install } = useUpdates();
  const running = useRunningAgents();
  const busy = phase === "downloading" || phase === "installing";
  let status: string;
  if (phase === "checking") status = "Checking…";
  else if (phase === "error" && error) status = error;
  else if (!info) status = "Not checked yet.";
  else if (!info.enabled) status = "This build was made on this Mac and doesn't update itself. Released builds do.";
  else if (info.available) status = `AgentDeck ${info.available} is available${info.date ? ` (released ${info.date})` : ""}.`;
  else status = "AgentDeck is up to date.";
  return (
    <div className="space-y-3 py-3">
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-[13.5px] text-fg">Version {info?.current ?? "—"}</span>
        <span className={phase === "error" ? "text-[12.5px] text-error" : "text-[12.5px] text-fg-2"}>{status}</span>
      </div>
      {info?.available && info.notes && (
        <pre className="scroll-quiet max-h-40 overflow-auto whitespace-pre-wrap rounded-lg border border-line bg-surface px-3 py-2 font-sans text-[12.5px] leading-[1.55] text-fg-2">
          {info.notes}
        </pre>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          disabled={phase === "checking" || busy}
          onClick={() => void check()}
          className="inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated disabled:opacity-50"
        >
          {phase === "checking" && <Loader2 size={13} className="animate-spin" />} Check for updates
        </button>
        {info?.available && !busy && (
          <button
            type="button"
            onClick={() => void install()}
            className="inline-flex h-8 items-center rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white hover:brightness-110"
          >
            Install {info.available} and restart
          </button>
        )}
        <UpdateProgress />
      </div>
      <p className="text-[12px] leading-[1.55] text-fg-3">
        AgentDeck checks quietly when it starts and every six hours. Updates are signed; one whose signature doesn't match isn't installed.
        {info?.available && running > 0 ? ` Restarting stops ${running} working agent${running === 1 ? "" : "s"}; their history stays.` : ""}
        {checkedAt ? ` Last checked ${new Date(checkedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}.` : ""}
      </p>
    </div>
  );
}
