import { Bot as BotIcon } from "lucide-react";
import { BOT_MARK, botInitials } from "../../lib/bots";
import type { BotColor } from "../../lib/live";
import { cx } from "../ui";

/** A bot's mark: its initials on its accent, with a small bot badge so it reads apart from agents. */
export function BotMark({ name, color, size = 20, className }: { name: string; color: BotColor; size?: number; className?: string }) {
  return (
    <span aria-hidden className={cx("relative inline-flex shrink-0", className)} style={{ width: size, height: size }}>
      <span
        className={cx("inline-flex h-full w-full items-center justify-center rounded-full font-medium tracking-tight", BOT_MARK[color] ?? BOT_MARK.violet)}
        style={{ fontSize: Math.max(8, Math.round(size * 0.4)) }}
      >
        {botInitials(name)}
      </span>
      {size >= 28 && (
        <span className="absolute -bottom-0.5 -end-0.5 inline-flex h-3.5 w-3.5 items-center justify-center rounded-full border border-line bg-bg text-fg-2">
          <BotIcon size={9} />
        </span>
      )}
    </span>
  );
}
