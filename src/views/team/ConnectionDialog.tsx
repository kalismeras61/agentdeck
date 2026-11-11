import { Check, GitBranch, Server } from "lucide-react";
import { useState } from "react";
import { cx } from "../../components/ui";
import { t } from "../../lib/i18n";
import type { TeamOverview } from "../../lib/team";

type Props = {
  o: TeamOverview;
  busy: boolean;
  onJoin: (url: string, invite: string) => void;
  onLeave: () => void;
  onClose: () => void;
};

/** How presence and messages travel for a project: the git remote, or an AgentDeck Relay. */
export function ConnectionDialog({ o, busy, onJoin, onLeave, onClose }: Props) {
  const [url, setUrl] = useState("https://");
  const [invite, setInvite] = useState("");
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
      <div className="w-full max-w-[540px] rounded-xl border border-line bg-surface p-5 shadow-xl">
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
          <div className="mt-4 flex justify-between gap-2">
            <button type="button" disabled={busy} onClick={onLeave} className="h-8 rounded-lg px-3 text-[13px] text-error hover:bg-elevated disabled:opacity-50">
              {t("Leave the relay")}
            </button>
            <button type="button" onClick={onClose} className="h-8 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated">
              {t("Close")}
            </button>
          </div>
        ) : (
          <form
            className="mt-4 space-y-2"
            onSubmit={(e) => {
              e.preventDefault();
              onJoin(url, invite);
            }}
          >
            <label className="block text-[12px] text-fg-3">
              {t("Relay address")}
              <input dir="ltr" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://relay.example.com" className="mt-1 h-8 w-full rounded-md border border-line bg-bg px-2 font-mono text-[12.5px] text-fg outline-none" />
            </label>
            <label className="block text-[12px] text-fg-3">
              {t("Invite code")}
              <input dir="ltr" value={invite} onChange={(e) => setInvite(e.target.value)} placeholder="adi_…" className="mt-1 h-8 w-full rounded-md border border-line bg-bg px-2 font-mono text-[12.5px] text-fg outline-none" />
            </label>
            <p className="text-[11.5px] leading-[1.5] text-fg-3">{t("Joining shares your work with that team: your name and email from git, your branch and what your agents are doing. The relay's token is kept in your keychain.")}</p>
            <div className="flex justify-end gap-2 pt-1">
              <button type="button" onClick={onClose} className="h-8 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated">
                {t("Cancel")}
              </button>
              <button type="submit" disabled={busy || url.length < 10 || !invite.trim()} className="h-8 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white disabled:opacity-50">
                {busy ? t("Joining…") : t("Join the team")}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
