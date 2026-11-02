import { PanelLeftOpen } from "lucide-react";
import type { ReactNode } from "react";
import { DemoBadge, IconButton } from "../components/ui";
import { MOD } from "../lib/format";
import { useStore } from "../state/store";
import { t } from "../lib/i18n";

/** Title bar row for full-screen views; doubles as the window drag region. */
export function ViewHeader({ title, children }: { title: string; children?: ReactNode }) {
  const demo = useStore((s) => s.workspaceId === "demo");
  const sidebarOpen = useStore((s) => s.sidebarOpen);
  const toggleSidebar = useStore((s) => s.toggleSidebar);
  return (
    <header data-tauri-drag-region className="drag relative flex h-12 shrink-0 items-center gap-3 border-b border-line-soft px-3">
      {!sidebarOpen && (
        <div className="flex items-center gap-2 pl-[66px]">
          <span aria-hidden className="absolute left-4 top-[18px] flex gap-2" data-web-only>
            <span className="h-3 w-3 rounded-full bg-[#ff5f57]" />
            <span className="h-3 w-3 rounded-full bg-[#febc2e]" />
            <span className="h-3 w-3 rounded-full bg-[#28c840]" />
          </span>
          <IconButton label={t("Show sidebar")} shortcut={`${MOD}B`} onClick={toggleSidebar} size="sm">
            <PanelLeftOpen size={16} />
          </IconButton>
        </div>
      )}
      <h1 className="truncate pl-1 text-[13px] font-medium text-fg">{title}</h1>
      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        {children}
        {demo && <DemoBadge />}
      </div>
    </header>
  );
}

/** Shared page scaffold: a readable column with a heading and one-line description. */
export function ViewBody({ heading, lede, children, wide = false }: { heading: string; lede: ReactNode; children: ReactNode; wide?: boolean }) {
  return (
    <div className="scroll-quiet relative h-full overflow-y-auto">
      <div className={wide ? "mx-auto max-w-[1080px] px-8 pb-16 pt-8" : "mx-auto max-w-[860px] px-8 pb-16 pt-8"}>
        <h2 className="text-[22px] font-semibold tracking-[-0.015em] [text-wrap:balance]">{heading}</h2>
        <p className="mt-1.5 max-w-[68ch] text-[14px] leading-[1.6] text-fg-2">{lede}</p>
        <div className="mt-7">{children}</div>
      </div>
    </div>
  );
}
