import { Loader2, Sparkles, SquareSlash, User, Zap } from "lucide-react";
import { useEffect, useRef } from "react";
import type { SlashCommand } from "../lib/commands";
import { t } from "../lib/i18n";
import { cx } from "./ui";

const GROUPS: { source: SlashCommand["source"]; label: (agent: string) => string; Icon: typeof Zap }[] = [
  { source: "custom", label: () => t("Your commands"), Icon: User },
  { source: "agent", label: (agent) => agent, Icon: SquareSlash },
  { source: "skill", label: () => t("Skills"), Icon: Sparkles },
  { source: "agentdeck", label: () => "AgentDeck", Icon: Zap },
];

/** The list "/" opens above the message box. Keyboard handling lives in the composer. */
export function SlashMenu({
  items,
  active,
  agent,
  loading,
  onPick,
  onHover,
}: {
  items: SlashCommand[];
  active: number;
  agent: string;
  loading: boolean;
  onPick: (c: SlashCommand) => void;
  onHover: (i: number) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);
  return (
    <div
      ref={list}
      id="slash-menu"
      role="listbox"
      aria-label={t("Commands")}
      className="scroll-quiet absolute bottom-full left-0 right-0 z-30 mb-2 max-h-[320px] overflow-y-auto rounded-xl border border-line bg-elevated p-1 shadow-pop"
    >
      {items.length === 0 && (
        <p className="flex items-center gap-2 px-3 py-2.5 text-[12.5px] text-fg-3">
          {loading ? (
            <>
              <Loader2 size={13} className="animate-spin" /> {t("Asking {agent} for its commands…", { agent })}
            </>
          ) : (
            t("No command matches. Keep typing to send it as a message.")
          )}
        </p>
      )}
      {GROUPS.map(({ source, label, Icon }) => {
        const group = items.filter((c) => c.source === source);
        if (!group.length) return null;
        return (
          <div key={source} role="group" aria-label={label(agent)}>
            <p className="flex items-center gap-1.5 px-2 pb-1 pt-2 text-[11px] text-fg-3">
              <Icon size={11} /> {label(agent)}
            </p>
            {group.map((c) => {
              const i = items.indexOf(c);
              return (
                <button
                  key={`${source}-${c.name}`}
                  type="button"
                  role="option"
                  aria-selected={i === active}
                  data-index={i}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => onPick(c)}
                  onMouseMove={() => onHover(i)}
                  className={cx("flex w-full items-baseline gap-2 rounded-lg px-2 py-1.5 text-left", i === active ? "bg-surface" : "")}
                >
                  <span className="shrink-0 font-mono text-[12.5px] text-fg">/{c.name}</span>
                  {c.hint && <span className="shrink-0 font-mono text-[11.5px] text-fg-3">{c.hint}</span>}
                  {c.description && <span className="min-w-0 truncate text-[12px] text-fg-3">{c.source === "agentdeck" ? t(c.description) : c.description.replace(/\s*\((user|project)\)\s*$/, "")}</span>}
                </button>
              );
            })}
          </div>
        );
      })}
      {loading && items.length > 0 && (
        <p className="flex items-center gap-2 px-2 py-1.5 text-[11.5px] text-fg-3">
          <Loader2 size={11} className="animate-spin" /> {t("Loading {agent}'s commands…", { agent })}
        </p>
      )}
    </div>
  );
}
