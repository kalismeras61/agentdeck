import { KeyRound, Rocket, Settings2 } from "lucide-react";
import type { Session } from "../../data/demo";
import { providerLabel } from "../../data/demo";
import type { EventOf } from "../../lib/events";
import { useStore } from "../../state/store";
import { t } from "../../lib/i18n";

/** Google no longer serves Gemini CLI to personal accounts; signing in again doesn't help. */
const BLOCKED = /no longer supported for Gemini Code Assist|migrate to the Antigravity/i;
const SIGN_IN = /\b(sign(ed)? ?in|log ?in|logged in|\/login|not logged|unauthori[sz]ed|invalid api key|api key (is )?(missing|invalid)|authenticat\w*|credentials?|oauth|401)\b/i;

/** What an agent error asks of you: nothing, signing in, or another way in (Gemini personal accounts). */
export function signInNeed(provider: string, text: string): "none" | "sign_in" | "gemini_blocked" {
  if (provider === "api") return "none";
  if (provider === "gemini-cli" && BLOCKED.test(text)) return "gemini_blocked";
  return SIGN_IN.test(text) ? "sign_in" : "none";
}

const btn = "inline-flex h-7 items-center gap-1.5 rounded-lg px-2.5 text-[12.5px]";

/** Buttons under an agent error that asks you to sign in. */
export function SignInHelp({ session, e }: { session: Session; e: EventOf<"error"> }) {
  const startSignIn = useStore((s) => s.startSignIn);
  const setView = useStore((s) => s.setView);
  if (!session.live) return null;
  const need = signInNeed(session.provider, `${e.payload.title} ${e.payload.message} ${e.payload.detail ?? ""}`);
  if (need === "none") return null;
  const agent = providerLabel[session.provider];

  if (need === "sign_in") {
    return (
      <div className="mt-2 flex flex-wrap items-center gap-2 ps-[26px]">
        <button type="button" onClick={() => startSignIn(session.id, session.provider)} className={`${btn} bg-violet-strong font-medium text-white hover:brightness-110`}>
          <KeyRound size={13} /> {t("Sign in to {agent}", { agent })}
        </button>
        <span className="text-[12px] text-fg-3">{t("Runs {agent}'s own login in AgentDeck's terminal, then sends your message again.", { agent })}</span>
      </div>
    );
  }
  return (
    <div className="mt-2 space-y-2 ps-[26px]">
      <p className="max-w-[68ch] text-[12.5px] leading-[1.55] text-fg-2">
        {t("Signing in again won't help: Google stopped serving Gemini CLI to personal Google accounts. Ways that still work:")}
      </p>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => {
            setView("settings");
            // Settings mounts lazily; jump to Providers once it's there.
            let tries = 0;
            const go = () => {
              const el = document.getElementById("set-providers");
              if (el) el.scrollIntoView({ block: "start" });
              else if (tries++ < 20) setTimeout(go, 50);
            };
            go();
          }}
          className={`${btn} border border-line text-fg hover:bg-elevated`}>
          <Settings2 size={13} /> {t("Add a Gemini API key")}
        </button>
        <button type="button" onClick={() => startSignIn(session.id, "gemini-cli")} className={`${btn} border border-line text-fg hover:bg-elevated`}>
          <KeyRound size={13} /> {t("Sign in with a work or Cloud account")}
        </button>
        <button type="button" onClick={() => startSignIn(session.id, "antigravity")} className={`${btn} border border-line text-fg hover:bg-elevated`}>
          <Rocket size={13} /> {t("Open Antigravity in the terminal")}
        </button>
      </div>
      <p className="max-w-[68ch] text-[11.5px] leading-[1.5] text-fg-3">
        {t(
          "An API key comes from {url}. A Google Workspace account or a Google Cloud project signs in through Gemini CLI's own {command}. Antigravity is Google's replacement for personal accounts: open it here to use it yourself, or start an Antigravity session from New session.",
          { url: "aistudio.google.com/apikey", command: "/auth" },
        )}
      </p>
    </div>
  );
}
