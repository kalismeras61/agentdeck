import { Check, Copy, CreditCard, GitBranch, Server, Users } from "lucide-react";
import { useEffect, useState } from "react";
import { cx } from "../../components/ui";
import { t, tn } from "../../lib/i18n";
import { errorText, termApi } from "../../lib/live";
import { planLabel, teamApi, type RelayBilling, type TeamOverview } from "../../lib/team";

type Props = {
  o: TeamOverview;
  projectId: string | null;
  busy: boolean;
  /** An invite to show right after creating a team. */
  invite: string | null;
  onJoin: (url: string, invite: string) => void;
  onCreate: (url: string, team: string) => void;
  onLeave: () => void;
  onClose: () => void;
};

const day = (ms: number | null) => (ms ? new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" }) : "");

/** The relay team's plan: status, seats, and (for admins) subscribing and managing it. */
function Plan({ projectId }: { projectId: string }) {
  const [b, setB] = useState<RelayBilling | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [seats, setSeats] = useState(1);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    teamApi
      .relayBilling(projectId)
      .then((x) => {
        setB(x);
        setSeats(Math.max(x.used, x.plan.seats ?? x.used, 1));
      })
      .catch((e: unknown) => setError(errorText(e)));
  }, [projectId]);
  if (error) return <p className="text-[12px] text-error">{error}</p>;
  if (!b) return <p className="text-[12px] text-fg-3">{t("Reading the plan…")}</p>;
  if (!b.enabled) return <p className="text-[12px] text-fg-3">{t("This relay doesn't charge: no limits.")}</p>;
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const bad = !b.access.canWrite;
  const warn = !!b.access.note && !bad;
  return (
    <div className="space-y-2.5">
      <div className="flex flex-wrap items-center gap-2">
        <span
          className={cx(
            "rounded-full px-2 py-0.5 text-[11.5px] font-medium",
            bad ? "bg-error/15 text-error" : warn ? "bg-warning/15 text-warning" : "bg-success/15 text-success",
          )}
        >
          {t(planLabel(b))}
        </span>
        <span className="flex items-center gap-1 text-[12px] text-fg-2">
          <Users size={12} />
          {b.plan.seats ? t("{used} of {seats} seats", { used: b.used, seats: b.plan.seats }) : tn(b.used, "{n} person", "{n} people")}
        </span>
        <span className="text-[12px] text-fg-3">
          {b.plan.status === "trialing" || b.plan.status === "on_trial"
            ? t("Trial until {date}", { date: day(b.plan.trialEndsAt) })
            : b.plan.endsAt
              ? t("Ends {date}", { date: day(b.plan.endsAt) })
              : b.plan.renewsAt
                ? t("Renews {date}", { date: day(b.plan.renewsAt) })
                : ""}
        </span>
      </div>
      {b.access.note && <p className={cx("text-[12px] leading-[1.5]", bad ? "text-error" : "text-warning")}>{b.access.note}</p>}
      {b.admin ? (
        <div className="flex flex-wrap items-center gap-2">
          <label className="flex items-center gap-1.5 text-[12px] text-fg-3">
            {t("Seats")}
            <input
              type="number"
              min={Math.max(1, b.used)}
              max={1000}
              value={seats}
              onChange={(e) => setSeats(Math.max(1, Math.min(1000, Number(e.target.value) || 1)))}
              className="h-7 w-[72px] rounded-md border border-line bg-bg px-2 text-[12.5px] tabular-nums text-fg outline-none"
            />
          </label>
          {!b.subscribed || b.plan.status === "expired" ? (
            <button
              type="button"
              disabled={busy || seats < b.used}
              onClick={() => void run(async () => termApi.openLink(await teamApi.relayCheckout(projectId, seats)))}
              className="flex h-7 items-center gap-1.5 rounded-lg bg-violet-strong px-2.5 text-[12px] font-medium text-white disabled:opacity-50"
            >
              <CreditCard size={13} />
              {t("Subscribe")}
            </button>
          ) : (
            <button
              type="button"
              disabled={busy || seats < b.used || seats === b.plan.seats}
              onClick={() => void run(async () => setB(await teamApi.relaySeats(projectId, seats)))}
              className="h-7 rounded-lg border border-line px-2.5 text-[12px] text-fg-2 hover:bg-elevated hover:text-fg disabled:opacity-50"
            >
              {t("Change seats")}
            </button>
          )}
          {b.plan.status === "past_due" && b.paymentUrl && (
            <button type="button" onClick={() => void termApi.openLink(b.paymentUrl!)} className="h-7 rounded-lg border border-line px-2.5 text-[12px] text-warning hover:bg-elevated">
              {t("Update the card")}
            </button>
          )}
          {b.portalUrl && (
            <button type="button" onClick={() => void termApi.openLink(b.portalUrl!)} className="h-7 rounded-lg px-2.5 text-[12px] text-fg-3 hover:bg-elevated hover:text-fg">
              {t("Invoices and billing")}
            </button>
          )}
        </div>
      ) : (
        <p className="text-[11.5px] text-fg-3">{t("A team admin manages the plan.")}</p>
      )}
      {b.admin && <p className="text-[11.5px] text-fg-3">{t("Paid per seat each month through Lemon Squeezy, which handles VAT and invoices. Changing seats is prorated.")}</p>}
    </div>
  );
}

/** How presence and messages travel for a project: the git remote, or an AgentDeck Relay. */
export function ConnectionDialog({ o, projectId, busy, invite, onJoin, onCreate, onLeave, onClose }: Props) {
  const [url, setUrl] = useState("https://");
  const [code, setCode] = useState("");
  const [mode, setMode] = useState<"join" | "create">("join");
  const [teamName, setTeamName] = useState("");
  const [copied, setCopied] = useState(false);
  const relay = o.relay ?? null;
  const option = (on: boolean, Icon: typeof Server, title: string, body: string) => (
    <div className={cx("flex gap-3 rounded-xl border p-3", on ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft/30" : "border-line-soft")}>
      <Icon size={16} className="mt-0.5 shrink-0 text-fg-3" />
      <div className="min-w-0 flex-1">
        <p className="flex items-center gap-1.5 text-[13px] font-medium text-fg">
          {title}
          {on && <Check size={13} className="text-violet-text" />}
        </p>
        <p className="mt-0.5 text-[12px] leading-[1.55] text-fg-3">{body}</p>
      </div>
    </div>
  );
  return (
    <div role="dialog" aria-modal aria-label={t("Team connection")} className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6">
      <div className="scroll-quiet max-h-full w-full max-w-[560px] overflow-y-auto rounded-xl border border-line bg-surface p-5 shadow-xl">
        <h2 className="text-[15px] font-semibold">{t("Team connection")}</h2>
        <p className="mt-1.5 text-[12.5px] leading-[1.6] text-fg-3">{t("How your presence and messages reach teammates in this project. Branches always come from git.")}</p>
        <div className="mt-4 space-y-2">
          {option(
            !relay,
            GitBranch,
            t("Git remote (free)"),
            t("Through {remote} under refs/agentdeck. No server; updates every couple of minutes; needs push access.", { remote: o.remote ?? "origin" }),
          )}
          {option(
            !!relay,
            Server,
            relay ? t("AgentDeck Relay: {team}", { team: relay.team }) : t("AgentDeck Relay"),
            relay
              ? t("{url}, as {repo}. Instant, no push access needed.", { url: relay.url, repo: relay.repo })
              : t("A relay your team runs (or a hosted one). Instant delivery, no push access needed, several repositories per team. It carries presence and messages, never code."),
          )}
        </div>
        {relay ? (
          <>
            {invite && (
              <div className="mt-4 rounded-xl border border-line-soft p-3">
                <p className="text-[12.5px] font-medium text-fg">{t("Your team is ready. Share this invite with your teammates:")}</p>
                <div className="mt-1.5 flex items-center gap-2">
                  <code dir="ltr" className="min-w-0 flex-1 truncate rounded-md bg-bg px-2 py-1 font-mono text-[12px]">
                    {invite}
                  </code>
                  <button
                    type="button"
                    onClick={() => void navigator.clipboard?.writeText(`${relay.url}\n${invite}`).then(() => setCopied(true))}
                    className="flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-fg-2 hover:bg-elevated"
                  >
                    {copied ? <Check size={12} /> : <Copy size={12} />}
                    {t("Copy with the address")}
                  </button>
                </div>
              </div>
            )}
            {projectId && (
              <section className="mt-4 rounded-xl border border-line-soft p-3">
                <h3 className="mb-2 text-[12.5px] font-semibold">{t("Plan")}</h3>
                <Plan projectId={projectId} />
              </section>
            )}
            <div className="mt-4 flex justify-between gap-2">
              <button type="button" disabled={busy} onClick={onLeave} className="h-8 rounded-lg px-3 text-[13px] text-error hover:bg-elevated disabled:opacity-50">
                {t("Leave the relay")}
              </button>
              <button type="button" onClick={onClose} className="h-8 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated">
                {t("Close")}
              </button>
            </div>
          </>
        ) : (
          <form
            className="mt-4 space-y-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (mode === "join") onJoin(url, code);
              else onCreate(url, teamName);
            }}
          >
            <div role="tablist" aria-label={t("Team connection")} className="flex w-fit rounded-lg border border-line-soft p-0.5">
              {(
                [
                  ["join", t("Join with an invite")],
                  ["create", t("Create a team")],
                ] as const
              ).map(([id, label]) => (
                <button key={id} type="button" role="tab" aria-selected={mode === id} onClick={() => setMode(id)} className={cx("h-7 rounded-md px-2.5 text-[12px]", mode === id ? "bg-elevated text-fg" : "text-fg-3 hover:text-fg")}>
                  {label}
                </button>
              ))}
            </div>
            <label className="block text-[12px] text-fg-3">
              {t("Relay address")}
              <input dir="ltr" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://relay.example.com" className="mt-1 h-8 w-full rounded-md border border-line bg-bg px-2 font-mono text-[12.5px] text-fg outline-none" />
            </label>
            {mode === "join" ? (
              <label className="block text-[12px] text-fg-3">
                {t("Invite code")}
                <input dir="ltr" value={code} onChange={(e) => setCode(e.target.value)} placeholder="adi_…" className="mt-1 h-8 w-full rounded-md border border-line bg-bg px-2 font-mono text-[12.5px] text-fg outline-none" />
              </label>
            ) : (
              <label className="block text-[12px] text-fg-3">
                {t("Team name")}
                <input value={teamName} onChange={(e) => setTeamName(e.target.value)} placeholder={t("Acme Mobile")} className="mt-1 h-8 w-full rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg outline-none" />
              </label>
            )}
            <p className="text-[11.5px] leading-[1.5] text-fg-3">
              {mode === "join"
                ? t("Joining shares your work with that team: your name and email from git, your branch and what your agents are doing. The relay's token is kept in your keychain.")
                : t("You become the team's admin and get an invite to share. On a paid relay the team starts with a free trial; you subscribe later, per seat.")}
            </p>
            <div className="flex justify-end gap-2 pt-1">
              <button type="button" onClick={onClose} className="h-8 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated">
                {t("Cancel")}
              </button>
              <button
                type="submit"
                disabled={busy || url.length < 10 || (mode === "join" ? !code.trim() : !teamName.trim())}
                className="h-8 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white disabled:opacity-50"
              >
                {busy ? t("Joining…") : mode === "join" ? t("Join the team") : t("Create the team")}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
