import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";
import { liveApi, type EffortStatus } from "../lib/live";
import { effortName, effortsFor, modelName, useModels } from "../lib/models";
import { cx, Tip } from "./ui";

/**
 * The model for a live session: every model the agent reports, plus any model id typed in.
 * A change is used from the next message, even while the agent is running.
 *
 * With `onEffortChange`, also the reasoning effort: the levels the selected model takes, plus
 * Default (the agent's own). For a live session (`sessionId`) the effort the agent uses right
 * now is asked for whenever the menu opens or something changes.
 */
export function ModelMenu({
  provider,
  agent,
  value,
  onChange,
  effort,
  onEffortChange,
  sessionId,
  status,
  tip = t("Model for this session"),
  note = t("Used from your next message."),
  side = "top",
}: {
  provider: string;
  /** Display name of the agent ("Codex"). */
  agent: string;
  value: string;
  onChange: (model: string) => void;
  /** The picked effort; null = the agent's default. */
  effort?: string | null;
  onEffortChange?: (effort: string | null) => void;
  /** A live session whose agent can say which effort it uses. */
  sessionId?: string;
  /** The session's status, so the effort in use is asked for again after a start. */
  status?: string;
  tip?: string;
  note?: string;
  /** Where the list opens: above the message box, below a settings row. */
  side?: "top" | "bottom";
}) {
  const [open, setOpen] = useState(false);
  const [typing, setTyping] = useState(false);
  const { list, loading, refresh } = useModels(provider, open || value !== "default" || Boolean(effort));
  const models = list?.models ?? [];
  const input = useRef<HTMLInputElement>(null);
  const known = value === "default" || models.some((m) => m.id === value);
  const levels = onEffortChange ? effortsFor(models, value) : [];
  const now = useEffortInUse(open && levels.length > 0 ? sessionId : undefined, [effort, value, status]);
  const modelDefault = models.find((m) => m.id === (value || "default"))?.defaultEffort;
  // What Default means right now: what the running agent reports, else the model's own default.
  const picked = effort ?? null;
  const defaultIs = (!picked && now?.running ? now.effort : null) ?? modelDefault ?? null;
  // A level the model doesn't list (picked for another model) stays visible so it can be changed.
  const shown = picked && !levels.includes(picked) ? [...levels, picked] : levels;

  if (typing) {
    return (
      <form
        className="flex items-center gap-1"
        onSubmit={(e) => {
          e.preventDefault();
          const id = input.current?.value.trim();
          if (id) onChange(id);
          setTyping(false);
        }}
      >
        <input
          ref={input}
          autoFocus
          aria-label={t("{agent} model id", { agent })}
          defaultValue={known ? "" : value}
          placeholder={t("Model id")}
          spellCheck={false}
          onKeyDown={(e) => e.key === "Escape" && setTyping(false)}
          onBlur={(e) => !e.relatedTarget && setTyping(false)}
          className="h-7 w-[180px] rounded-md border border-line bg-bg px-2 font-mono text-[11.5px] text-fg outline-none focus:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))]"
        />
        <button type="submit" className="h-7 rounded-md px-2 text-[12px] text-violet-text hover:bg-elevated">
          {t("Use")}
        </button>
      </form>
    );
  }

  return (
    <Dropdown.Root open={open} onOpenChange={setOpen}>
      <Tip label={tip} side="top">
        <Dropdown.Trigger className="inline-flex h-8 max-w-[240px] items-center gap-1.5 rounded-lg px-2 text-xs text-fg-2 hover:bg-elevated hover:text-fg data-[state=open]:bg-elevated">
          <span className={cx("truncate", !known && "font-mono text-[11.5px]")}>{modelName(models, value)}</span>
          {onEffortChange && picked && <span className="shrink-0 text-fg-3">· {effortName(picked)}</span>}
          <ChevronDown size={13} className="shrink-0" />
        </Dropdown.Trigger>
      </Tip>
      <Dropdown.Portal>
        <Dropdown.Content
          side={side}
          align={side === "top" ? "start" : "end"}
          sideOffset={6}
          collisionPadding={12}
          className="z-50 flex max-h-[min(460px,var(--radix-dropdown-menu-content-available-height))] w-[300px] flex-col rounded-xl border border-line bg-elevated p-1 shadow-pop"
        >
          <div className="flex items-center justify-between px-2 pb-1 pt-1.5">
            <Dropdown.Label className="text-[11px] text-fg-3">{t("{agent} models", { agent })}</Dropdown.Label>
            {loading && <RefreshCw size={12} aria-label={t("Loading")} className="animate-spin text-fg-3 motion-reduce:animate-none" />}
          </div>
          <div className="scroll-quiet min-h-0 flex-1 overflow-y-auto">
            {models.length === 0 && (
              <p className="px-2 py-2 text-[12.5px] text-fg-3">{loading ? t("Asking {agent} which models it offers…", { agent }) : t("{agent} didn't list any models.", { agent })}</p>
            )}
            {models.map((m) => (
              <Dropdown.Item
                key={m.id}
                onSelect={() => onChange(m.id)}
                className="flex items-start gap-2 rounded-lg px-2 py-1.5 outline-none data-[highlighted]:bg-surface"
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] text-fg">{m.label}</span>
                  {m.description && <span className="line-clamp-2 text-[11.5px] leading-[1.45] text-fg-3">{m.description}</span>}
                  {m.label !== m.id && m.id !== "default" && <span className="block truncate font-mono text-[10.5px] text-fg-3">{m.id}</span>}
                </span>
                {m.id === value && <Check size={14} className="mt-0.5 shrink-0 text-violet-text" />}
              </Dropdown.Item>
            ))}
            {!known && (
              <Dropdown.Item className="flex items-center gap-2 rounded-lg px-2 py-1.5 outline-none data-[highlighted]:bg-surface" onSelect={() => setTyping(true)}>
                <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{value}</span>
                <Check size={14} className="shrink-0 text-violet-text" />
              </Dropdown.Item>
            )}
          </div>
          {shown.length > 0 && onEffortChange && (
            <>
              <Dropdown.Separator className="my-1 h-px bg-line-soft" />
              <Dropdown.Label className="px-2 pb-1 pt-0.5 text-[11px] text-fg-3">{t("Effort")}</Dropdown.Label>
              <Dropdown.RadioGroup
                value={picked ?? "default"}
                onValueChange={(v) => onEffortChange(v === "default" ? null : v)}
                className="flex flex-wrap gap-1 px-2 pb-1"
              >
                {["default", ...shown].map((lvl) => (
                  <Dropdown.RadioItem
                    key={lvl}
                    value={lvl}
                    onSelect={(e) => e.preventDefault()}
                    className="h-7 cursor-default rounded-md border border-line px-2 text-[12px] leading-[26px] text-fg-2 outline-none data-[highlighted]:bg-surface data-[state=checked]:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] data-[state=checked]:text-fg"
                  >
                    {lvl === "default"
                      ? defaultIs
                        ? t("Default ({effort})", { effort: effortName(defaultIs) })
                        : t("Default")
                      : effortName(lvl)}
                  </Dropdown.RadioItem>
                ))}
              </Dropdown.RadioGroup>
              {now?.env ? (
                <p className="max-w-[290px] px-2 pb-1 text-[11px] leading-[1.5] text-warning">
                  {t("{name} is set to {value} in your shell environment, so {agent} uses that whatever you pick here.", { name: now.env[0], value: now.env[1], agent })}
                </p>
              ) : (
                now?.running &&
                picked &&
                now.effort &&
                now.effort !== picked && (
                  <p className="max-w-[290px] px-2 pb-1 text-[11px] leading-[1.5] text-fg-3">
                    {t("{agent} is using {effort} right now.", { agent, effort: effortName(now.effort) })}
                  </p>
                )
              )}
            </>
          )}
          <Dropdown.Separator className="my-1 h-px bg-line-soft" />
          <Dropdown.Item
            onSelect={() => setTyping(true)}
            className="rounded-lg px-2 py-1.5 text-[12.5px] text-fg-2 outline-none data-[highlighted]:bg-surface data-[highlighted]:text-fg"
          >
            {t("Use another model id…")}
          </Dropdown.Item>
          <Dropdown.Item
            disabled={loading}
            onSelect={(e) => {
              e.preventDefault();
              void refresh();
            }}
            className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[12.5px] text-fg-2 outline-none data-[disabled]:opacity-50 data-[highlighted]:bg-surface data-[highlighted]:text-fg"
          >
            <RefreshCw size={12} /> {t("Ask {agent} again", { agent })}
          </Dropdown.Item>
          <p className="max-w-[290px] px-2 pb-1 pt-1.5 text-[11px] leading-[1.5] text-fg-3">
            {list?.error && list.source !== "agent"
              ? list.source === "saved"
                ? `${t("Couldn't reach {agent} just now, so this is the list from last time.", { agent })} `
                : `${t("Couldn't ask {agent} for its models: {error}.", { agent, error: list.error })} `
              : ""}
            {note}
          </p>
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

/** What a live session's agent says about its effort; asked again whenever `deps` change. */
function useEffortInUse(sessionId: string | undefined, deps: unknown[]): EffortStatus | null {
  const [now, setNow] = useState<EffortStatus | null>(null);
  const key = JSON.stringify(deps);
  useEffect(() => {
    if (!sessionId || !liveApi.available) return;
    let gone = false;
    liveApi
      .sessionEffort(sessionId)
      .then((s) => !gone && setNow(s))
      .catch(() => !gone && setNow(null));
    return () => {
      gone = true;
    };
  }, [sessionId, key]);
  return sessionId ? now : null;
}
