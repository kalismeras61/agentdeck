import { ShieldAlert, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import type { Session } from "../../data/demo";
import type { EventOf } from "../../lib/events";
import { errorText, liveApi } from "../../lib/live";
import { useStore } from "../../state/store";
import { cx } from "../ui";
import { useAgyApprovals } from "../AgyApprovals";
import type { AgyApprovals } from "../../lib/live";

/**
 * Actions Antigravity skipped because they need approval it can't ask for here. Allowing adds
 * the rules agy named to its own settings, then the last message goes again.
 */
/** Rules that would let it delete, push or change permissions without asking start unticked. */
const RISKY = /(\brm\s+-|\bsudo\b|\bchmod\b|\bchown\b|\bmkfs\b|\bdd\s|git\s+push|git\s+reset\s+--hard|git\s+clean|curl[^)]*\|\s*(ba)?sh|^command\(\*?\)$)/;

export function DeniedCard({ e, session }: { e: EventOf<"denied">; session: Session }) {
  const [picked, setPicked] = useState<Set<string>>(() => new Set(e.payload.rules.filter((r) => !RISKY.test(r))));
  const [state, setState] = useState<"idle" | "busy" | "done">("idle");
  const [error, setError] = useState<string | null>(null);
  const [file, setFile] = useState<string | null>(null);
  const startSignIn = useStore((s) => s.startSignIn);
  const clearError = useStore((s) => s.clearSessionError);
  const rules = e.payload.rules;

  const allow = async () => {
    setState("busy");
    setError(null);
    try {
      setFile(await liveApi.allowAntigravityRules([...picked]));
      clearError(session.id);
      await liveApi.retryLast(session.id);
      setState("done");
    } catch (er: unknown) {
      setError(errorText(er));
      setState("idle");
    }
  };

  return (
    <div className="rounded-xl border border-warning/35 bg-warning/[0.06] px-3.5 py-3">
      <div className="flex items-start gap-2.5">
        <ShieldAlert size={16} className="mt-0.5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-medium text-fg">Antigravity skipped what needs your OK</p>
          <p className="mt-0.5 text-[12.5px] leading-[1.55] text-fg-2">
            It can't ask in AgentDeck, so these didn't run: <span className="font-mono text-[12px] text-fg">{e.payload.actions.join(", ")}</span>
          </p>
          {e.payload.notice && <pre className="mt-1.5 whitespace-pre-wrap font-mono text-[11.5px] leading-[1.5] text-fg-3">{e.payload.notice}</pre>}

          {state === "done" ? (
            <p role="status" className="mt-2 flex items-center gap-1.5 text-[12.5px] text-success">
              <ShieldCheck size={14} /> Allowed in Antigravity's settings{file ? ` (${file.replace(/^\/(Users|home)\/[^/]+/, "~")})` : ""}. Your message went again.
            </p>
          ) : rules.length > 0 ? (
            <div className="mt-2.5 space-y-2">
              <ul className="space-y-1">
                {rules.map((r) => (
                  <li key={r}>
                    <label className="flex items-center gap-2 text-[12.5px]">
                      <input
                        type="checkbox"
                        checked={picked.has(r)}
                        onChange={() =>
                          setPicked((cur) => {
                            const n = new Set(cur);
                            if (n.has(r)) n.delete(r);
                            else n.add(r);
                            return n;
                          })
                        }
                        className="accent-[var(--violet)]"
                      />
                      <code className="rounded bg-elevated px-1.5 py-0.5 font-mono text-[12px] text-fg">{r}</code>
                      {RISKY.test(r) && <span className="text-[11.5px] text-warning">can delete or change things for good; allowing it is up to you</span>}
                    </label>
                  </li>
                ))}
              </ul>
              <p className="text-[11.5px] leading-[1.5] text-fg-3">
                Allowing adds {picked.size === 1 ? "this rule" : "these rules"} to Antigravity's own settings: it then does this without asking, in every project. You can remove
                rules there or with /permissions in agy.
              </p>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  disabled={state === "busy" || picked.size === 0}
                  onClick={() => void allow()}
                  className="inline-flex h-7 items-center gap-1.5 rounded-lg bg-violet-strong px-3 text-[12.5px] font-medium text-white hover:brightness-110 disabled:opacity-50"
                >
                  <ShieldCheck size={13} /> {state === "busy" ? "Allowing…" : "Allow and send again"}
                </button>
              </div>
            </div>
          ) : (
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <p className="text-[12px] leading-[1.5] text-fg-3">
                agy didn't say which rule would allow it. Add one with /permissions in agy (for example{" "}
                <code className="font-mono text-[11.5px]">command(npm test)</code>), then send your message again.
              </p>
              <button
                type="button"
                onClick={() => startSignIn(session.id, "antigravity")}
                className={cx("inline-flex h-7 items-center rounded-lg border border-line px-2.5 text-[12.5px] text-fg hover:bg-elevated")}
              >
                Open agy in the terminal
              </button>
            </div>
          )}
          {state !== "done" && <AutoChoices session={session} />}
          {error && (
            <p role="alert" className="mt-2 text-[12.5px] text-error">
              {error}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

/** Instead of allowing one rule: let Antigravity decide (Auto) or skip approvals altogether. */
function AutoChoices({ session }: { session: Session }) {
  const { level, load, set } = useAgyApprovals();
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (level === null) void load().catch(() => {});
  }, [level, load]);
  const go = async (l: AgyApprovals) => {
    setBusy(true);
    setError(null);
    try {
      // The backend redoes the skipped steps under the new level.
      await set(l, session.id);
    } catch (e: unknown) {
      setError(errorText(e));
    }
    setBusy(false);
  };
  if (level === "all") return null;
  return (
    <div className="mt-3 border-t border-warning/20 pt-2.5">
      <p className="text-[11.5px] text-fg-3">Or change what Antigravity may do without asking, for all its sessions:</p>
      <div className="mt-1.5 flex flex-wrap items-center gap-2">
        {level !== "auto" && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void go("auto")}
            title="Antigravity's own Auto mode: safe edits and commands go ahead, irreversible ones are still skipped"
            className="h-7 rounded-lg border border-line px-2.5 text-[12.5px] text-fg hover:bg-elevated disabled:opacity-50"
          >
            Use Auto mode and send again
          </button>
        )}
        <button
          type="button"
          disabled={busy}
          onClick={() => (confirm ? void go("all") : setConfirm(true))}
          className={cx(
            "h-7 rounded-lg border px-2.5 text-[12.5px] disabled:opacity-50",
            confirm ? "border-error/60 bg-error/10 text-error" : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
          )}
        >
          {confirm ? "Yes, skip every approval and send again" : "Skip all approvals…"}
        </button>
        {confirm && <span className="text-[11.5px] text-error">Commands, deletions and pushes then run without asking.</span>}
      </div>
      {error && <p className="mt-1 text-[12px] text-error">{error}</p>}
    </div>
  );
}
