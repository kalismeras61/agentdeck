import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, RefreshCw } from "lucide-react";
import { useRef, useState } from "react";
import { t } from "../lib/i18n";
import { modelName, useModels } from "../lib/models";
import { cx, Tip } from "./ui";

/**
 * The model for a live session: every model the agent reports, plus any model id typed in.
 * A change is used from the next message, even while the agent is running.
 */
export function ModelMenu({
  provider,
  agent,
  value,
  onChange,
  tip = t("Model for this session"),
  note = t("Used from your next message."),
  side = "top",
}: {
  provider: string;
  /** Display name of the agent ("Codex"). */
  agent: string;
  value: string;
  onChange: (model: string) => void;
  tip?: string;
  note?: string;
  /** Where the list opens: above the message box, below a settings row. */
  side?: "top" | "bottom";
}) {
  const [open, setOpen] = useState(false);
  const [typing, setTyping] = useState(false);
  const { list, loading, refresh } = useModels(provider, open || value !== "default");
  const models = list?.models ?? [];
  const input = useRef<HTMLInputElement>(null);
  const known = value === "default" || models.some((m) => m.id === value);

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
        <Dropdown.Trigger className="inline-flex h-8 max-w-[200px] items-center gap-1.5 rounded-lg px-2 text-xs text-fg-2 hover:bg-elevated hover:text-fg data-[state=open]:bg-elevated">
          <span className={cx("truncate", !known && "font-mono text-[11.5px]")}>{modelName(models, value)}</span>
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
