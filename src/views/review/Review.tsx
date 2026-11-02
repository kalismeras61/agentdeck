import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ChevronUp, Columns2, FileCode2, GitCompareArrows, Rows2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { CopyButton } from "../../components/blocks/code";
import { ChangeStat } from "../../components/blocks/EventBlocks";
import { AgentMark, cx, IconButton, Kbd } from "../../components/ui";
import { providerLabel } from "../../data/demo";
import { useAllChanges } from "../../hooks/useDerived";
import { useResolvedTheme } from "../../hooks/useTheme";
import { sidesFromDiff } from "../../lib/events";
import { basename, dirname } from "../../lib/format";
import { useStore } from "../../state/store";
import { ViewHeader } from "../ViewHeader";
import { MonacoDiff, type DiffHandle } from "./MonacoDiff";

const MONACO_LANG: Record<string, string> = { dart: "dart", json: "json", yaml: "yaml", ts: "typescript", tsx: "typescript", js: "javascript" };
const langOf = (path: string) => MONACO_LANG[path.split(".").pop() ?? ""] ?? "plaintext";

export function Review() {
  const all = useAllChanges();
  const reviewSessionId = useStore((s) => s.reviewSessionId);
  const reviewPath = useStore((s) => s.reviewPath);
  const selectedId = useStore((s) => s.selectedId);
  const layout = useStore((s) => s.reviewLayout);
  const setLayout = useStore((s) => s.setReviewLayout);
  const setPath = useStore((s) => s.setReviewPath);
  const setSession = useStore((s) => s.setReviewSession);
  const viewed = useStore((s) => s.viewed);
  const toggleViewed = useStore((s) => s.toggleViewed);
  const select = useStore((s) => s.select);
  const theme = useResolvedTheme();
  const [fileOnly, setFileOnly] = useState(false);
  const diff = useRef<DiffHandle>(null);

  const current = all.find((c) => c.session.id === reviewSessionId) ?? all.find((c) => c.session.id === selectedId) ?? all[0];
  const files = useMemo(() => current?.files ?? [], [current]);
  const file = files.find((f) => f.path === reviewPath) ?? files[0];
  const idx = file ? files.indexOf(file) : -1;
  const sides = useMemo(() => (file ? sidesFromDiff(file.diff) : { original: "", modified: "" }), [file]);
  const key = (path: string) => `${current?.session.id}::${path}`;
  const viewedCount = files.filter((f) => viewed[key(f.path)]).length;
  const isViewed = file ? Boolean(viewed[key(file.path)]) : false;

  // Review keys work while focus is outside text inputs and the editor (F7 / ⇧F7 work inside Monaco).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (!file || !current || e.metaKey || e.ctrlKey || e.altKey) return;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      if (e.key === "j" && idx < files.length - 1) setPath(files[idx + 1]!.path);
      else if (e.key === "k" && idx > 0) setPath(files[idx - 1]!.path);
      else if (e.key === "n") diff.current?.next();
      else if (e.key === "p") diff.current?.previous();
      else if (e.key === "v") toggleViewed(current.session.id, file.path);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [file, current, idx, files, setPath, toggleViewed]);

  if (!current || !file) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Review changes" />
        <div className="flex flex-1 flex-col items-center justify-center px-8 text-center">
          <GitCompareArrows size={20} className="mb-3 text-fg-3" />
          <p className="text-[14px] font-medium text-fg-2">No changes to review</p>
          <p className="mt-1 max-w-[360px] text-[13px] text-fg-3">When an agent edits files, they show up here with a side-by-side diff.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Review changes" />
      <div className="flex min-h-0 flex-1">
        {/* File list */}
        <aside className="flex w-[280px] shrink-0 flex-col border-r border-line-soft bg-sidebar">
          <div className="border-b border-line-soft p-3">
            <Dropdown.Root>
              <Dropdown.Trigger className="flex w-full items-center gap-2 rounded-lg border border-line bg-bg/40 px-2.5 py-2 text-left hover:bg-surface data-[state=open]:bg-surface">
                <AgentMark provider={current.session.provider} size={18} />
                <span className="min-w-0 flex-1 truncate text-[13px]">{current.session.title}</span>
                <ChevronDown size={14} className="text-fg-3" />
              </Dropdown.Trigger>
              <Dropdown.Portal>
                <Dropdown.Content align="start" sideOffset={6} className="z-50 w-[256px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
                  <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">Sessions with changes</Dropdown.Label>
                  {all.map((c) => (
                    <Dropdown.Item
                      key={c.session.id}
                      onSelect={() => setSession(c.session.id)}
                      className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface"
                    >
                      <AgentMark provider={c.session.provider} size={16} />
                      <span className="min-w-0 flex-1 truncate">{c.session.title}</span>
                      {c.session.id === current.session.id && <Check size={14} className="text-violet-text" />}
                    </Dropdown.Item>
                  ))}
                </Dropdown.Content>
              </Dropdown.Portal>
            </Dropdown.Root>
            <div className="mt-3 flex items-baseline justify-between text-xs text-fg-3">
              <span>
                {files.length} {files.length === 1 ? "file" : "files"}
              </span>
              <span className="tabular-nums">
                {viewedCount} of {files.length} viewed
              </span>
            </div>
            <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-elevated">
              <div className="h-full rounded-full bg-violet transition-[width] duration-300" style={{ width: `${(viewedCount / files.length) * 100}%` }} />
            </div>
          </div>
          <ul className="scroll-quiet min-h-0 flex-1 overflow-y-auto p-2" aria-label="Changed files">
            {files.map((f) => {
              const v = Boolean(viewed[key(f.path)]);
              const active = f.path === file.path;
              return (
                <li key={f.path} className="group relative">
                  <button
                    type="button"
                    onClick={() => setPath(f.path)}
                    aria-current={active ? "true" : undefined}
                    className={cx("w-full rounded-lg py-2 pl-2.5 pr-9 text-left", active ? "bg-elevated" : "hover:bg-surface")}
                  >
                    <span className="flex items-center gap-2">
                      <span className={cx("w-3 shrink-0 font-mono text-[11px] font-medium", f.change === "added" ? "text-success" : f.change === "deleted" ? "text-error" : "text-warning")}>
                        {f.change === "added" ? "A" : f.change === "deleted" ? "D" : "M"}
                      </span>
                      <span className={cx("truncate font-mono text-[12.5px]", v ? "text-fg-3" : "text-fg")}>{basename(f.path)}</span>
                    </span>
                    <span className="mt-0.5 flex items-center gap-2 pl-5">
                      <span className="min-w-0 truncate font-mono text-[11px] text-fg-3">{dirname(f.path)}</span>
                      <ChangeStat additions={f.additions} deletions={f.deletions} className="ml-auto shrink-0" />
                    </span>
                  </button>
                  <button
                    type="button"
                    role="checkbox"
                    aria-checked={v}
                    aria-label={`Viewed ${basename(f.path)}`}
                    onClick={() => toggleViewed(current.session.id, f.path)}
                    className={cx(
                      "absolute right-2 top-2.5 flex h-[18px] w-[18px] items-center justify-center rounded-[5px] border",
                      v ? "border-violet bg-violet text-white" : "border-line text-transparent hover:border-fg-3",
                    )}
                  >
                    <Check size={12} strokeWidth={3} />
                  </button>
                </li>
              );
            })}
          </ul>
          <p className="border-t border-line-soft px-4 py-3 text-[11.5px] leading-[1.55] text-fg-3">
            Viewed marks stay on this device. They don't commit, push, or approve anything, and the agent isn't told.
          </p>
        </aside>

        {/* Diff */}
        <section className="flex min-w-0 flex-1 flex-col" aria-label={`Diff of ${file.path}`}>
          <div className="flex h-12 shrink-0 items-center gap-2 border-b border-line-soft pl-4 pr-2">
            <FileCode2 size={15} className="shrink-0 text-fg-3" />
            <span className="min-w-0 truncate font-mono text-[12.5px]" title={file.path}>
              {dirname(file.path) && <span className="text-fg-3">{dirname(file.path)}/</span>}
              <span className="text-fg">{basename(file.path)}</span>
            </span>
            <ChangeStat additions={file.additions} deletions={file.deletions} className="shrink-0" />
            {file.change === "added" && !fileOnly && <span className="hidden shrink-0 text-xs text-fg-3 2xl:inline">New file, shown unified</span>}
            <div className="ml-auto flex shrink-0 items-center gap-1">
              <div role="radiogroup" aria-label="Diff layout" className="mr-1 flex rounded-lg border border-line bg-bg/40 p-0.5">
                {(
                  [
                    ["split", "Side by side", Columns2],
                    ["unified", "Unified", Rows2],
                  ] as const
                ).map(([id, label, Icon]) => (
                  <button
                    key={id}
                    type="button"
                    role="radio"
                    aria-checked={!fileOnly && layout === id}
                    onClick={() => {
                      setLayout(id);
                      setFileOnly(false);
                    }}
                    className={cx(
                      "inline-flex h-6 items-center gap-1.5 rounded-md px-2 text-xs",
                      !fileOnly && layout === id ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg",
                    )}
                  >
                    <Icon size={13} />
                    <span className="hidden xl:inline">{label}</span>
                  </button>
                ))}
                <button
                  type="button"
                  role="radio"
                  aria-checked={fileOnly}
                  onClick={() => setFileOnly(true)}
                  className={cx("inline-flex h-6 items-center gap-1.5 rounded-md px-2 text-xs", fileOnly ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg")}
                >
                  <FileCode2 size={13} />
                  <span className="hidden xl:inline">Whole file</span>
                </button>
              </div>
              <IconButton label="Previous change" shortcut="P" size="sm" disabled={fileOnly} onClick={() => diff.current?.previous()}>
                <ChevronUp size={15} />
              </IconButton>
              <IconButton label="Next change" shortcut="N" size="sm" disabled={fileOnly} onClick={() => diff.current?.next()}>
                <ChevronDown size={15} />
              </IconButton>
              <CopyButton text={file.path} label="Copy path" />
              <button
                type="button"
                onClick={() => toggleViewed(current.session.id, file.path)}
                aria-pressed={isViewed}
                className={cx(
                  "ml-1 inline-flex h-8 items-center gap-1.5 rounded-lg px-3 text-[12.5px] font-medium",
                  isViewed ? "border border-line text-fg-2 hover:bg-elevated" : "bg-violet-strong text-white hover:brightness-110",
                )}
              >
                {isViewed && <Check size={14} />}
                {isViewed ? "Viewed" : "Mark viewed"}
              </button>
            </div>
          </div>
          <div className="relative min-h-0 flex-1">
            <MonacoDiff
              ref={diff}
              original={sides.original}
              modified={sides.modified}
              language={langOf(file.path)}
              sideBySide={layout === "split"}
              fileOnly={fileOnly}
              dark={theme === "dark"}
              label={file.path}
            />
          </div>
          <div className="flex h-9 shrink-0 items-center gap-3 border-t border-line-soft px-4 text-[11.5px] text-fg-3">
            <span className="inline-flex items-center gap-1.5">
              <Kbd>J</Kbd>
              <Kbd>K</Kbd> next / previous file
            </span>
            <span className="inline-flex items-center gap-1.5">
              <Kbd>N</Kbd>
              <Kbd>P</Kbd> next / previous change
            </span>
            <span className="inline-flex items-center gap-1.5">
              <Kbd>V</Kbd> mark viewed
            </span>
            <button type="button" onClick={() => select(current.session.id)} className="ml-auto inline-flex items-center gap-1.5 rounded text-fg-2 hover:text-fg">
              <AgentMark provider={current.session.provider} size={14} /> Open in {providerLabel[current.session.provider]} session
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}
