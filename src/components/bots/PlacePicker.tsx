import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, FolderGit2, House } from "lucide-react";
import { usePlaceName } from "../../hooks/usePlaceName";
import { t } from "../../lib/i18n";
import type { Bot } from "../../lib/live";
import { listed, useStore } from "../../state/store";
import { cx } from "../ui";

const placeMenuItem = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface";

/** Where a task (or schedule) works: the bot's workspace or one of your projects. */
export function PlacePicker({
  bot,
  value,
  onChange,
  className,
  full = false,
}: {
  bot: Bot;
  value: string | null;
  onChange: (projectId: string | null) => void;
  className: string;
  full?: boolean;
}) {
  const projects = useStore((s) => s.projects);
  const workspaceId = useStore((s) => s.workspaceId);
  const placeName = usePlaceName();
  const folders = projects.filter((p) => p.workspaceId === workspaceId && p.kind !== "chats" && listed(p));
  const item = (pid: string | null, name: string, path: string) => (
    <Dropdown.Item key={pid ?? "ws"} onSelect={() => onChange(pid)} className={cx(placeMenuItem, "items-start")}>
      {pid ? <FolderGit2 size={14} className="mt-0.5 shrink-0 text-fg-3" /> : <House size={14} className="mt-0.5 shrink-0 text-fg-3" />}
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span className="truncate">{name}</span>
          {pid === (bot.projectId ?? null) && <span className="shrink-0 text-[11px] text-fg-3">{t("default")}</span>}
        </span>
        <span dir="ltr" className="block truncate text-start font-mono text-[11px] text-fg-3">
          {path}
        </span>
      </span>
      {value === pid && <Check size={14} className="mt-0.5 shrink-0 text-violet-text" />}
    </Dropdown.Item>
  );
  return (
    <Dropdown.Root>
      <Dropdown.Trigger className={className} aria-label={t("Works in: {place}", { place: placeName(value) })}>
        {value ? <FolderGit2 size={13} className="shrink-0 text-fg-3" /> : <House size={13} className="shrink-0 text-fg-3" />}
        <span className="truncate">{full ? placeName(value) : t("In: {place}", { place: value ? placeName(value) : t("its workspace") })}</span>
        <ChevronDown size={12} className={cx("shrink-0 text-fg-3", full && "ms-auto")} />
      </Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content
          align="start"
          side={full ? "bottom" : "top"}
          sideOffset={6}
          className="z-50 max-h-[360px] w-[280px] overflow-y-auto rounded-xl border border-line bg-elevated p-1 shadow-pop"
        >
          <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Where it works")}</Dropdown.Label>
          {item(null, t("Its workspace"), bot.workspace)}
          {folders.length > 0 && <Dropdown.Separator className="my-1 h-px bg-line" />}
          {folders.map((p) => item(p.id, p.name, p.path))}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

