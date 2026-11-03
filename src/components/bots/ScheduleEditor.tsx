import * as Dialog from "@radix-ui/react-dialog";
import { Plus, Repeat, X } from "lucide-react";
import { useState } from "react";
import { scheduleSummary, weekdayName } from "../../lib/bots";
import { t } from "../../lib/i18n";
import type { Bot, BotSchedule } from "../../lib/live";
import { useStore } from "../../state/store";
import { cx, IconButton } from "../ui";
import { PlacePicker } from "./PlacePicker";

const field =
  "mt-1 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";
const trigger =
  "mt-1 flex h-9 w-full items-center gap-2 rounded-lg border border-line bg-surface px-2.5 text-start text-[13px] text-fg hover:bg-elevated data-[state=open]:bg-elevated";
const DAYS = [1, 2, 3, 4, 5, 6, 7];

/** Make or change a schedule: what to do, where, and when (daily at times on chosen weekdays, or every N hours or minutes). */
export function ScheduleEditor({ bot, schedule, open, onClose }: { bot: Bot; schedule: BotSchedule | null; open: boolean; onClose: () => void }) {
  return (
    <Dialog.Root open={open} onOpenChange={(v) => !v && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="scroll-quiet fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-48px)] w-[min(480px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          {open && <Form key={schedule?.id ?? "new"} bot={bot} schedule={schedule} onDone={onClose} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Form({ bot, schedule, onDone }: { bot: Bot; schedule: BotSchedule | null; onDone: () => void }) {
  const save = useStore((s) => s.saveBotSchedule);
  const [text, setText] = useState(schedule?.text ?? "");
  const [place, setPlace] = useState<string | null>(schedule ? schedule.projectId : (bot.projectId ?? null));
  const [kind, setKind] = useState<"daily" | "interval">(schedule?.kind ?? "daily");
  const [times, setTimes] = useState<string[]>(schedule?.kind === "daily" && schedule.times.length ? schedule.times : ["09:00"]);
  const [days, setDays] = useState<number[]>(schedule?.kind === "daily" && schedule.weekdays.length ? schedule.weekdays : DAYS);
  const startMinutes = schedule?.kind === "interval" ? (schedule.everyMinutes ?? 60) : 360;
  const [unit, setUnit] = useState<"hours" | "minutes">(startMinutes % 60 === 0 ? "hours" : "minutes");
  const [every, setEvery] = useState(String(startMinutes % 60 === 0 ? startMinutes / 60 : startMinutes));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const minutes = Math.round(Number(every) * (unit === "hours" ? 60 : 1));
  const cleanTimes = [...new Set(times.filter((x) => /^\d{2}:\d{2}$/.test(x)))].sort();
  const problem =
    !text.trim()
      ? t("Say what the bot should do.")
      : kind === "daily"
        ? cleanTimes.length === 0
          ? t("Pick at least one time of day.")
          : days.length === 0
            ? t("Pick at least one day.")
            : null
        : !Number.isFinite(minutes) || minutes < 15
          ? t("Every 15 minutes at most.")
          : minutes > 7 * 24 * 60
            ? t("At least once a week.")
            : null;
  const summary = kind === "daily" ? scheduleSummary({ kind, times: cleanTimes, weekdays: days, everyMinutes: null }) : scheduleSummary({ kind, times: [], weekdays: [], everyMinutes: minutes });

  return (
    <>
      <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
        <Repeat size={16} className="text-violet-text" /> {schedule ? t("Edit schedule") : t("New schedule")}
      </Dialog.Title>
      <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
        {t("{name} gets this task at the times you set, in your Mac's time zone. Runs missed while AgentDeck or the Mac was off aren't made up.", { name: bot.name })}
      </Dialog.Description>
      <form
        className="mt-4 space-y-3.5"
        onSubmit={async (e) => {
          e.preventDefault();
          if (problem) {
            setError(problem);
            return;
          }
          setBusy(true);
          const err = await save({
            ...(schedule ? { id: schedule.id } : {}),
            botId: bot.id,
            text: text.trim(),
            projectId: place,
            kind,
            times: kind === "daily" ? cleanTimes : [],
            weekdays: kind === "daily" ? (days.length === 7 ? [] : [...days].sort()) : [],
            everyMinutes: kind === "interval" ? minutes : null,
            enabled: schedule?.enabled ?? true,
          });
          setBusy(false);
          if (err) setError(err);
          else onDone();
        }}
      >
        <label className="block text-[12px] text-fg-2">
          {t("What to do")}
          <textarea
            autoFocus
            value={text}
            rows={3}
            maxLength={100000}
            onChange={(e) => {
              setText(e.target.value);
              setError(null);
            }}
            placeholder={t("Review yesterday's merged pull requests and list anything that needs a follow-up.")}
            className={cx(field, "resize-y py-2 leading-[1.55]")}
          />
        </label>
        <div className="text-[12px] text-fg-2">
          {t("Where")}
          <PlacePicker bot={bot} value={place} onChange={setPlace} className={trigger} full />
        </div>
        <fieldset className="text-[12px] text-fg-2">
          <legend>{t("When")}</legend>
          <div role="radiogroup" aria-label={t("When")} className="mt-1 inline-flex rounded-lg border border-line bg-bg/40 p-0.5">
            {(
              [
                ["daily", t("At set times")],
                ["interval", t("Every few hours")],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={kind === id}
                onClick={() => {
                  setKind(id);
                  setError(null);
                }}
                className={cx("h-7 rounded-md px-3 text-[12.5px]", kind === id ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg")}
              >
                {label}
              </button>
            ))}
          </div>
          {kind === "daily" ? (
            <div className="mt-3 space-y-3">
              <div>
                <p className="text-[12px] text-fg-2">{t("Times")}</p>
                <div className="mt-1 flex flex-wrap items-center gap-1.5">
                  {times.map((x, i) => (
                    <span key={i} className="inline-flex h-8 items-center rounded-lg border border-line bg-surface ps-2 pe-0.5">
                      <input
                        type="time"
                        value={x}
                        step={60}
                        aria-label={t("Time {n}", { n: i + 1 })}
                        onChange={(e) => {
                          const next = [...times];
                          next[i] = e.target.value;
                          setTimes(next);
                          setError(null);
                        }}
                        className="bg-transparent text-[13px] tabular-nums text-fg outline-none"
                      />
                      <IconButton label={t("Remove this time")} size="sm" disabled={times.length === 1} onClick={() => setTimes(times.filter((_, j) => j !== i))} className="h-6 w-6">
                        <X size={12} />
                      </IconButton>
                    </span>
                  ))}
                  {times.length < 24 && (
                    <button
                      type="button"
                      onClick={() => setTimes([...times, "12:00"])}
                      className="inline-flex h-8 items-center gap-1 rounded-lg px-2 text-[12.5px] text-violet-text hover:bg-surface"
                    >
                      <Plus size={13} /> {t("Add a time")}
                    </button>
                  )}
                </div>
              </div>
              <div>
                <p className="text-[12px] text-fg-2">{t("On")}</p>
                <div role="group" aria-label={t("Days")} className="mt-1 flex gap-1">
                  {DAYS.map((d) => {
                    const on = days.includes(d);
                    return (
                      <button
                        key={d}
                        type="button"
                        aria-pressed={on}
                        aria-label={weekdayName(d, "long")}
                        onClick={() => {
                          setDays(on ? days.filter((x) => x !== d) : [...days, d].sort());
                          setError(null);
                        }}
                        className={cx(
                          "h-8 min-w-9 rounded-lg border px-1.5 text-[12px]",
                          on ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-violet-text" : "border-line text-fg-3 hover:bg-surface hover:text-fg",
                        )}
                      >
                        {weekdayName(d)}
                      </button>
                    );
                  })}
                </div>
                <div className="mt-1.5 flex gap-3 text-[11.5px]">
                  <button type="button" onClick={() => setDays(DAYS)} className="text-fg-3 hover:text-fg">
                    {t("Every day")}
                  </button>
                  <button type="button" onClick={() => setDays([1, 2, 3, 4, 5])} className="text-fg-3 hover:text-fg">
                    {t("Weekdays")}
                  </button>
                </div>
              </div>
            </div>
          ) : (
            <div className="mt-3 flex items-center gap-2 text-[13px] text-fg">
              <span className="text-[12px] text-fg-2">{t("Every")}</span>
              <input
                type="number"
                min={unit === "hours" ? 1 : 15}
                step={1}
                value={every}
                aria-label={t("How often")}
                onChange={(e) => {
                  setEvery(e.target.value);
                  setError(null);
                }}
                className="h-8 w-20 rounded-lg border border-line bg-surface px-2.5 text-[13px] tabular-nums text-fg outline-none focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]"
              />
              <div role="radiogroup" aria-label={t("Unit")} className="inline-flex rounded-lg border border-line bg-bg/40 p-0.5">
                {(
                  [
                    ["hours", t("hours")],
                    ["minutes", t("minutes")],
                  ] as const
                ).map(([id, label]) => (
                  <button
                    key={id}
                    type="button"
                    role="radio"
                    aria-checked={unit === id}
                    onClick={() => setUnit(id)}
                    className={cx("h-7 rounded-md px-2.5 text-[12.5px]", unit === id ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg")}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          )}
        </fieldset>
        {!problem && <p className="text-[12px] text-fg-3">{summary}</p>}
        {error && (
          <p role="alert" className="text-[12.5px] text-error">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
          <button type="submit" disabled={busy} className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50">
            {busy ? t("Saving…") : schedule ? t("Save") : t("Add schedule")}
          </button>
        </div>
      </form>
    </>
  );
}
