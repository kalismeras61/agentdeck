import { Check, CircleSlash, MessageCircleQuestion, SkipForward } from "lucide-react";
import { useState } from "react";
import type { Session } from "../../data/demo";
import { agentLabel } from "../../lib/agentName";
import type { EventOf } from "../../lib/events";
import { clock } from "../../lib/format";
import { errorText, liveApi } from "../../lib/live";
import { t, tn } from "../../lib/i18n";
import { cx } from "../ui";

type Q = EventOf<"question">["payload"]["questions"][number];

/** One question: options to pick (one or several), and your own answer where the agent allows it. */
function QuestionField({
  q,
  picked,
  other,
  onPick,
  onOther,
  disabled,
}: {
  q: Q;
  picked: string[];
  other: string;
  onPick: (labels: string[]) => void;
  onOther: (text: string) => void;
  disabled: boolean;
}) {
  const toggle = (label: string) => {
    if (q.multiSelect) onPick(picked.includes(label) ? picked.filter((x) => x !== label) : [...picked, label]);
    else {
      onPick(picked[0] === label ? [] : [label]);
      onOther("");
    }
  };
  return (
    <fieldset className="space-y-2" disabled={disabled}>
      <legend className="mb-2 flex flex-wrap items-center gap-2">
        {q.header && <span className="rounded-md bg-violet/12 px-1.5 py-0.5 text-[11px] font-medium text-violet-text">{q.header}</span>}
        <span className="text-[13.5px] font-medium leading-[1.5] text-fg">{q.question}</span>
        {q.multiSelect && <span className="text-[11.5px] text-fg-3">{t("Pick any")}</span>}
      </legend>
      {q.options.length > 0 && (
        <div role={q.multiSelect ? "group" : "radiogroup"} className="grid gap-1.5">
          {q.options.map((o) => {
            const on = picked.includes(o.label);
            return (
              <button
                key={o.label}
                type="button"
                role={q.multiSelect ? "checkbox" : "radio"}
                aria-checked={on}
                onClick={() => toggle(o.label)}
                className={cx(
                  "flex items-start gap-2.5 rounded-xl border px-3 py-2 text-start transition-colors",
                  on ? "border-violet bg-violet/10" : "border-line bg-bg hover:border-violet/50 hover:bg-elevated",
                )}
              >
                <span
                  aria-hidden
                  className={cx(
                    "mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center border",
                    q.multiSelect ? "rounded" : "rounded-full",
                    on ? "border-violet bg-violet text-white" : "border-line",
                  )}
                >
                  {on && <Check size={11} strokeWidth={3} />}
                </span>
                <span className="min-w-0">
                  <span className="block text-[13px] text-fg">{o.label}</span>
                  {o.description && <span className="mt-0.5 block text-[12px] leading-[1.5] text-fg-3">{o.description}</span>}
                </span>
              </button>
            );
          })}
        </div>
      )}
      {q.other && (
        <input
          type={q.secret ? "password" : "text"}
          value={other}
          onChange={(e) => {
            onOther(e.target.value);
            if (!q.multiSelect && e.target.value) onPick([]);
          }}
          placeholder={q.options.length ? t("Or write your own answer") : t("Your answer")}
          aria-label={t("Your answer to: {question}", { question: q.question })}
          maxLength={4000}
          autoComplete="off"
          className="h-9 w-full rounded-lg border border-line bg-bg px-3 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-violet"
        />
      )}
    </fieldset>
  );
}

/** An agent's question (Claude Code's AskUserQuestion, Codex's request_user_input). */
export function QuestionCard({ e, session }: { e: EventOf<"question">; session: Session }) {
  const p = e.payload;
  const agent = agentLabel(session);
  const [picked, setPicked] = useState<Record<string, string[]>>({});
  const [other, setOther] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (p.state !== "pending") {
    const answers = p.answers ?? {};
    return (
      <div className="rounded-xl border border-line-soft bg-surface/60 px-3.5 py-2.5 text-[13px]">
        <div className="flex items-center gap-2">
          {p.state === "answered" ? <Check size={15} className="text-success" /> : <CircleSlash size={15} className="text-fg-3" />}
          <span className="text-fg">{p.state === "answered" ? t("You answered") : p.state === "skipped" ? t("You skipped the question") : t("Expired, never answered")}</span>
        </div>
        {p.state === "answered" && (
          <ul className="mt-1.5 space-y-1 ps-[23px]">
            {p.questions.map((q) => (
              <li key={q.id} className="text-[12.5px] leading-[1.5] text-fg-2">
                <span className="text-fg-3">{q.header ?? q.question}: </span>
                {q.secret ? t("(hidden)") : (answers[q.id] ?? []).join(t(", ")) || "—"}
              </li>
            ))}
          </ul>
        )}
      </div>
    );
  }

  const answerOf = (q: Q) => {
    const own = (other[q.id] ?? "").trim();
    return [...(picked[q.id] ?? []), ...(own ? [own] : [])];
  };
  const ready = p.questions.every((q) => answerOf(q).length > 0);
  const send = async (skip: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await liveApi.answerQuestion(session.id, p.requestId, skip ? null : Object.fromEntries(p.questions.map((q) => [q.id, answerOf(q)])));
    } catch (err: unknown) {
      setError(errorText(err));
    }
    setBusy(false);
  };

  return (
    <section aria-label={t("Question from {agent}", { agent })} className="overflow-hidden rounded-2xl border border-violet/45 bg-surface shadow-card">
      <header className="flex items-center gap-2.5 border-b border-line-soft bg-violet/[0.07] px-4 py-3">
        <MessageCircleQuestion size={17} className="shrink-0 text-violet-text" />
        <h3 className="text-[14px] font-semibold">
          {tn(p.questions.length, "{agent} has a question", "{agent} has {n} questions", { agent })}
        </h3>
        <span className="ms-auto shrink-0 text-xs text-fg-3">{clock(e.ts)}</span>
      </header>
      <form
        className="space-y-5 px-4 py-4"
        onSubmit={(ev) => {
          ev.preventDefault();
          if (ready && !busy) void send(false);
        }}
      >
        {p.questions.map((q) => (
          <QuestionField
            key={q.id}
            q={q}
            picked={picked[q.id] ?? []}
            other={other[q.id] ?? ""}
            onPick={(labels) => setPicked((s) => ({ ...s, [q.id]: labels }))}
            onOther={(text) => setOther((s) => ({ ...s, [q.id]: text }))}
            disabled={busy || !session.live}
          />
        ))}
        {error && (
          <p role="alert" className="text-[12.5px] text-error">
            {error}
          </p>
        )}
        <div className="flex flex-wrap items-center gap-2 border-t border-line-soft pt-3.5">
          <button
            type="submit"
            disabled={!ready || busy || !session.live}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-45"
          >
            <Check size={14} /> {t("Send answer")}
          </button>
          <button
            type="button"
            disabled={busy || !session.live}
            onClick={() => void send(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated hover:text-fg disabled:opacity-45"
          >
            <SkipForward size={14} /> {t("Skip, let {agent} decide", { agent })}
          </button>
          <span className="ms-auto text-[11.5px] text-fg-3">{t("{agent} waits until you answer.", { agent })}</span>
        </div>
      </form>
    </section>
  );
}
