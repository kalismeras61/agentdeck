import { Check, Copy, Send, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { cx } from "../components/ui";
import { t } from "../lib/i18n";
import { errorText, liveApi, telegramApi, type TelegramStatus } from "../lib/live";

const field =
  "h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";
const primary = "h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50";
const quiet = "h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated disabled:opacity-50";

/** Settings → Telegram: your own bot (its token from @BotFather), the chats paired with it, and
 * which results it sends. */
export function TelegramSettings() {
  const [st, setSt] = useState<TelegramStatus | null>(null);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [confirmOff, setConfirmOff] = useState(false);

  useEffect(() => {
    if (!liveApi.available) return;
    let off: (() => void) | undefined;
    let live = true;
    telegramApi.status().then((s) => live && setSt(s), (e: unknown) => live && setError(errorText(e)));
    void telegramApi.subscribe((s) => setSt(s)).then((u) => {
      if (live) off = u;
      else u();
    });
    return () => {
      live = false;
      off?.();
    };
  }, []);

  // The pairing code's countdown.
  useEffect(() => {
    if (!st?.pairingUntil) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [st?.pairingUntil]);

  if (!liveApi.available) return <p className="text-[13px] text-fg-3">{t("Telegram works in the desktop app.")}</p>;
  if (!st) return error ? <p className="text-[13px] text-error">{error}</p> : null;

  const run = async (f: () => Promise<TelegramStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setSt(await f());
      return true;
    } catch (e: unknown) {
      setError(errorText(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  if (!st.connected) {
    return (
      <div className="max-w-[560px] space-y-3">
        <ol className="list-decimal space-y-1 ps-5 text-[13px] leading-[1.6] text-fg-2">
          <li>{t("In Telegram, open @BotFather and send /newbot. Give it a name; it's your own bot, only you will use it.")}</li>
          <li>{t("BotFather replies with a token like 123456789:AA…. Paste it here.")}</li>
        </ol>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            void run(() => telegramApi.connect(token)).then((ok) => ok && setToken(""));
          }}
        >
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="123456789:AA…"
            aria-label={t("Bot token")}
            className={cx(field, "font-mono text-[12.5px]")}
          />
          <button type="submit" disabled={busy || !token.trim()} className={cx(primary, "shrink-0")}>
            {busy ? t("Checking…") : t("Connect")}
          </button>
        </form>
        <p className="text-xs leading-[1.55] text-fg-3">
          {t("The token stays in your Mac's keychain. AgentDeck asks Telegram for new messages itself, so nothing on your Mac is opened to the internet and no server sits in between.")}
        </p>
        {error && (
          <p role="alert" className="text-[13px] text-error">
            {error}
          </p>
        )}
      </div>
    );
  }

  const left = st.pairingUntil ? Math.max(0, Math.round((st.pairingUntil - now) / 1000)) : 0;
  const pairing = st.pairingCode && left > 0;
  const handle = st.username ? `@${st.username}` : t("your bot");
  const pairText = st.pairingCode ? `/pair ${st.pairingCode}` : "";

  return (
    <div className="max-w-[620px] space-y-4">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Send size={15} className="shrink-0 text-violet-text" />
        <p className="min-w-0 flex-1 text-[13.5px]">
          {st.username ? t("Connected as {bot}", { bot: `@${st.username}` }) : t("Connected")}
          {st.error && <span className="ms-2 text-[12.5px] text-error">{st.error}</span>}
        </p>
        {confirmOff ? (
          <span className="flex items-center gap-2">
            <button type="button" onClick={() => void run(async () => telegramApi.disconnect()).then(() => setConfirmOff(false))} className="h-8 rounded-lg bg-error/90 px-3 text-[13px] font-medium text-white hover:brightness-110">
              {t("Disconnect")}
            </button>
            <button type="button" onClick={() => setConfirmOff(false)} className={quiet}>
              {t("Cancel")}
            </button>
          </span>
        ) : (
          <button type="button" onClick={() => setConfirmOff(true)} className={quiet}>
            {t("Disconnect")}
          </button>
        )}
      </div>
      {confirmOff && <p className="text-xs text-fg-3">{t("The token is removed from the keychain and paired chats are forgotten.")}</p>}

      <div>
        <p className="text-[13px] font-medium">{t("Paired chats")}</p>
        {st.chats.length === 0 ? (
          <p className="mt-1 text-[13px] text-fg-3">{t("None yet. Pair the chat you'll use, and only that chat can reach your bots.")}</p>
        ) : (
          <ul className="mt-1.5 divide-y divide-line-soft rounded-lg border border-line-soft">
            {st.chats.map((c) => (
              <li key={c.id} className="flex items-center gap-3 px-3 py-2 text-[13px]">
                <span className="min-w-0 flex-1 truncate">{c.name}</span>
                <button type="button" onClick={() => void run(() => telegramApi.unpair(c.id))} aria-label={t("Unpair {name}", { name: c.name })} className="rounded-md p-1.5 text-fg-3 hover:bg-elevated hover:text-error">
                  <Trash2 size={14} />
                </button>
              </li>
            ))}
          </ul>
        )}
        {pairing ? (
          <div className="mt-3 rounded-xl border border-[color-mix(in_srgb,var(--violet)_35%,var(--border))] bg-violet-soft px-3.5 py-3">
            <p className="text-[13px] text-fg">{t("Send this to {bot} in Telegram:", { bot: handle })}</p>
            <div className="mt-2 flex items-center gap-2">
              <code dir="ltr" className="rounded-md bg-surface px-2.5 py-1 font-mono text-[15px] tracking-wide">
                {pairText}
              </code>
              <button
                type="button"
                onClick={() => {
                  navigator.clipboard.writeText(pairText).then(
                    () => {
                      setCopied(true);
                      window.setTimeout(() => setCopied(false), 1500);
                    },
                    () => undefined,
                  );
                }}
                className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12.5px] text-fg-2 hover:bg-elevated"
              >
                {copied ? <Check size={13} /> : <Copy size={13} />}
                {copied ? t("Copied") : t("Copy")}
              </button>
              <span className="ms-auto text-xs tabular-nums text-fg-3">{t("{m}:{s} left", { m: Math.floor(left / 60), s: String(left % 60).padStart(2, "0") })}</span>
            </div>
          </div>
        ) : (
          <button type="button" disabled={busy} onClick={() => void run(() => telegramApi.pair())} className={cx(quiet, "mt-3")}>
            {t("Pair a chat")}
          </button>
        )}
      </div>

      <div>
        <p className="text-[13px] font-medium">{t("What it sends you")}</p>
        <p className="mt-0.5 text-xs leading-[1.55] text-fg-3">{t("Answers to what you send from Telegram, and anything a bot needs you for, always come. Besides those:")}</p>
        <div role="radiogroup" aria-label={t("What it sends you")} className="mt-2 space-y-1.5">
          {(
            [
              ["telegram", t("Nothing else")],
              ["schedules", t("Results of scheduled tasks")],
              ["all", t("Results of every bot task")],
            ] as [TelegramStatus["notify"], string][]
          ).map(([v, label]) => (
            <label key={v} className="flex cursor-pointer items-center gap-2 text-[13px]">
              <input type="radio" name="tg-notify" checked={st.notify === v} onChange={() => void run(() => telegramApi.setNotify(v))} className="accent-[var(--violet)]" />
              {label}
            </label>
          ))}
        </div>
      </div>

      <p className="text-xs leading-[1.55] text-fg-3">
        {t("In the chat: write a task and the bot you picked gets it; start with a bot's name (\"Reviewer: …\") to pick another. /bots lists them, /status and /cancel are about the current one.")}
      </p>
      {error && (
        <p role="alert" className="text-[13px] text-error">
          {error}
        </p>
      )}
    </div>
  );
}
