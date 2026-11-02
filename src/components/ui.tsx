import * as Tooltip from "@radix-ui/react-tooltip";
import clsx from "clsx";
import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import type { SessionStatus } from "../data/demo";
import type { ProviderId } from "../lib/events";

export const cx = clsx;

export function Tip({ label, shortcut, children, side = "bottom" }: { label: string; shortcut?: string; children: ReactNode; side?: "top" | "bottom" | "left" | "right" }) {
  return (
    <Tooltip.Root delayDuration={350}>
      <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side={side}
          sideOffset={6}
          className="z-50 flex items-center gap-2 rounded-lg border border-line bg-elevated px-2.5 py-1.5 text-xs text-fg shadow-pop"
        >
          {label}
          {shortcut && <Kbd>{shortcut}</Kbd>}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  shortcut?: string;
  active?: boolean;
  size?: "sm" | "md";
  tipSide?: "top" | "bottom" | "left" | "right";
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, shortcut, active, size = "md", className, children, tipSide, ...rest },
  ref,
) {
  const btn = (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      aria-pressed={active}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-lg text-fg-2 transition-colors duration-150",
        "hover:bg-elevated hover:text-fg disabled:pointer-events-none disabled:opacity-40",
        size === "sm" ? "h-7 w-7" : "h-8 w-8",
        active && "bg-elevated text-fg",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
  return (
    <Tip label={label} shortcut={shortcut} side={tipSide}>
      {btn}
    </Tip>
  );
});

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-[5px] border border-line bg-surface px-1.5 py-px font-sans text-[11px] leading-4 text-fg-2">{children}</kbd>
  );
}

const statusMeta: Record<SessionStatus, { label: string; color: string; live: boolean }> = {
  running: { label: "Running", color: "text-violet", live: true },
  waiting: { label: "Needs approval", color: "text-warning", live: true },
  idle: { label: "Idle", color: "text-fg-3", live: false },
  error: { label: "Error", color: "text-error", live: false },
  done: { label: "Completed", color: "text-success", live: false },
};

export function statusLabel(s: SessionStatus) {
  return statusMeta[s].label;
}

export function StatusDot({ status, className }: { status: SessionStatus; className?: string }) {
  const m = statusMeta[status];
  return (
    <span
      role="img"
      aria-label={m.label}
      className={cx("inline-block h-2 w-2 shrink-0 rounded-full bg-current", m.color, m.live && "breathe", className)}
    />
  );
}

export function StatusPill({ status }: { status: SessionStatus }) {
  const m = statusMeta[status];
  return (
    <span className={cx("inline-flex items-center gap-1.5 rounded-full border border-line px-2 py-0.5 text-xs", m.color)}>
      <StatusDot status={status} />
      <span className="text-fg-2">{m.label}</span>
    </span>
  );
}

/** Neutral monograms. Provider logos are trademarks and are not reproduced. */
export function AgentMark({ provider, size = 20 }: { provider: ProviderId; size?: number }) {
  const map = {
    "claude-code": { t: "CC", c: "bg-violet-soft text-violet-text" },
    codex: { t: "CX", c: "bg-[color-mix(in_srgb,var(--blue)_14%,transparent)] text-blue" },
    "gemini-cli": { t: "GM", c: "bg-[color-mix(in_srgb,var(--success)_14%,transparent)] text-success" },
    "grok-build": { t: "GK", c: "bg-elevated text-fg" },
    antigravity: { t: "AG", c: "bg-[color-mix(in_srgb,var(--blue)_10%,var(--elevated))] text-fg" },
    api: { t: "AI", c: "bg-[color-mix(in_srgb,var(--warning)_14%,transparent)] text-warning" },
  }[provider];
  return (
    <span
      aria-hidden
      className={cx("inline-flex shrink-0 items-center justify-center rounded-md font-mono font-medium tracking-tight", map.c)}
      style={{ width: size, height: size, fontSize: Math.round(size * 0.42) }}
    >
      {map.t}
    </span>
  );
}

export function DemoBadge({ className }: { className?: string }) {
  return (
    <Tip label="Everything shown is sample data. No provider is connected.">
      <span
        tabIndex={0}
        className={cx(
          "inline-flex items-center gap-1.5 rounded-full border border-warning/35 bg-warning/10 px-2 py-0.5 text-[11px] font-medium text-warning",
          className,
        )}
      >
        <span className="h-1.5 w-1.5 rounded-full bg-warning" />
        Demo data
      </span>
    </Tip>
  );
}
