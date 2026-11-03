import { useImageSrc } from "../../lib/images";
import * as Dialog from "@radix-ui/react-dialog";
import {
  AlertTriangle,
  Ban,
  CheckCircle2,
  ChevronRight,
  CircleSlash,
  FileCode2,
  FilePlus2,
  FileSearch,
  FlaskConical,
  Info,
  ListTodo,
  Minus,
  PanelRight,
  Pencil,
  Plus,
  ShieldAlert,
  ShieldCheck,
  SquareTerminal,
  TextSearch,
  X,
  XCircle,
  FileText,
  Folder,
  Image as ImageIcon,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { diffStats, type EventOf, type ToolItem } from "../../lib/events";
import { basename, clock, dirname, duration } from "../../lib/format";
import { langForPath } from "../../lib/highlight";
import { t, tn } from "../../lib/i18n";
import { useStore } from "../../state/store";
import { cx } from "../ui";
import { AnsiText, CopyButton, DiffView, JsonTree } from "./code";

const reveal = {
  initial: { height: 0, opacity: 0 },
  animate: { height: "auto", opacity: 1 },
  exit: { height: 0, opacity: 0 },
  transition: { duration: 0.2, ease: [0.2, 0.8, 0.2, 1] as const },
};

/* ---------------------------------------------------------------- user */

export function UserBubble({ e }: { e: EventOf<"user_message"> }) {
  return (
    <div className="flex justify-end">
      <div className="user-text max-w-[78%] rounded-2xl rounded-ee-md border border-line bg-elevated px-4 py-2.5 text-[14px] leading-[1.6] text-fg shadow-card">
        {e.payload.label && (
          <p className="mb-0.5 flex items-center gap-1 text-[11.5px] font-medium text-fg-3">
            <ListTodo size={12} aria-hidden /> {t(e.payload.label)}
          </p>
        )}
        {e.payload.text.split(/^```[^\n]*\n?/m).map((part, i) =>
          i % 2 === 1 ? (
            <pre key={i} className="scroll-quiet my-2 max-h-72 overflow-auto rounded-lg border border-line-soft bg-bg px-3 py-2 font-mono text-[12px] leading-[1.55] text-fg-2">
              {part.replace(/\n$/, "")}
            </pre>
          ) : part.trim() ? (
            <p key={i} className="whitespace-pre-wrap break-words">
              {part.trim()}
            </p>
          ) : null,
        )}
        {e.payload.attachments && (
          <div className="mt-2 flex flex-wrap gap-1.5">
            {e.payload.attachments.map((a) => {
              const Icon = /\.(png|jpe?g|gif|webp)$/i.test(a) ? ImageIcon : a.endsWith("/") ? Folder : FileText;
              return (
                <span key={a} title={a} className="inline-flex max-w-[240px] items-center gap-1.5 rounded-md bg-surface px-2 py-0.5 text-[11.5px] text-fg-2">
                  <Icon size={12} className="shrink-0 text-fg-3" />
                  <span className="truncate">{a.split(/[\\/]/).filter(Boolean).pop() ?? a}</span>
                </span>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- tools */

const toolIcon: Record<ToolItem["kind"], typeof FileSearch> = {
  read: FileSearch,
  search: TextSearch,
  edit: Pencil,
  create: FilePlus2,
  command: SquareTerminal,
  test: FlaskConical,
  analyze: SquareTerminal,
};

function StatusGlyph({ status }: { status: ToolItem["status"] }) {
  if (status === "ok") return <CheckCircle2 size={14} className="text-success" aria-label={t("Succeeded")} />;
  if (status === "warning") return <AlertTriangle size={14} className="text-warning" aria-label={t("Warning")} />;
  if (status === "error") return <XCircle size={14} className="text-error" aria-label={t("Failed")} />;
  return <span className="breathe inline-block h-2 w-2 rounded-full bg-violet text-violet" aria-label={t("Running")} />;
}

export function ToolGroup({ e }: { e: EventOf<"tool_group"> }) {
  const { items, title } = e.payload;
  const worst = items.some((i) => i.status === "error") ? "error" : items.some((i) => i.status === "warning") ? "warning" : "ok";
  const [open, setOpen] = useState(worst === "error");
  const total = items.reduce((s, i) => s + (i.durationMs ?? 0), 0);
  const Lead = items.some((i) => i.kind === "command" || i.kind === "test" || i.kind === "analyze") ? SquareTerminal : toolIcon[items[0]?.kind ?? "read"];
  return (
    <div className="rounded-xl border border-line-soft bg-surface/60">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-start text-[13px] hover:bg-surface"
      >
        <Lead size={15} className="text-fg-3" />
        <span className="font-medium text-fg">{title}</span>
        {worst !== "ok" && <StatusGlyph status={worst} />}
        <span className="ms-auto flex items-center gap-3 text-xs text-fg-3">
          {total > 0 && <span className="tabular-nums">{duration(total)}</span>}
          <ChevronRight size={14} className={cx("rtl:-scale-x-100 transition-transform duration-200", open && "rotate-90 rtl:-rotate-90")} />
        </span>
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.div {...reveal} className="overflow-hidden">
            <ul className="border-t border-line-soft px-1.5 py-1.5">
              {items.map((it) => (
                <ToolRow key={it.id} it={it} />
              ))}
            </ul>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function ToolRow({ it }: { it: ToolItem }) {
  const Icon = toolIcon[it.kind];
  const hasDetail = Boolean(it.output || it.json !== undefined);
  const [open, setOpen] = useState(it.status === "error");
  const openChange = useStore((s) => s.openChange);
  const isFile = it.kind === "read" || it.kind === "edit" || it.kind === "create";
  const head = (
    <>
      <Icon size={14} className="shrink-0 text-fg-3" />
      <span className="shrink-0 text-fg-2">{it.label}</span>
      <span className={cx("min-w-0 truncate font-mono text-[12px]", it.kind === "command" || it.kind === "test" || it.kind === "analyze" ? "text-fg" : "text-fg/85")}>
        {it.target}
      </span>
      <span className="ms-auto flex shrink-0 items-center gap-2.5 ps-3 text-xs text-fg-3">
        {it.meta && <span>{it.meta}</span>}
        {it.durationMs !== undefined && it.durationMs >= 1000 && <span className="tabular-nums">{duration(it.durationMs)}</span>}
        <StatusGlyph status={it.status} />
        {hasDetail && <ChevronRight size={13} className={cx("rtl:-scale-x-100 transition-transform", open && "rotate-90 rtl:-rotate-90")} />}
      </span>
    </>
  );
  return (
    <li>
      {hasDetail ? (
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-start text-[13px] hover:bg-elevated/60"
        >
          {head}
        </button>
      ) : isFile && it.kind !== "read" && it.target ? (
        <button
          type="button"
          onClick={() => openChange(it.target!)}
          className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-start text-[13px] hover:bg-elevated/60"
        >
          {head}
        </button>
      ) : (
        <div className="flex items-center gap-2.5 px-2 py-1.5 text-[13px]">{head}</div>
      )}
      <AnimatePresence initial={false}>
        {open && hasDetail && (
          <motion.div {...reveal} className="overflow-hidden">
            <div className="mb-1.5 ms-[30px] me-2 mt-0.5 border-s-2 border-line ps-3">
              {it.output && <BoundedLog text={it.output} />}
              {it.json !== undefined && (
                <div className="scroll-quiet max-h-72 overflow-auto py-1">
                  <JsonTree value={it.json} />
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </li>
  );
}

const LOG_TAIL_LINES = 400;

/** Huge logs are common. Only the tail is rendered until the reader asks for more. */
function BoundedLog({ text }: { text: string }) {
  const [all, setAll] = useState(false);
  const lines = useMemo(() => text.split("\n"), [text]);
  const hidden = all ? 0 : Math.max(0, lines.length - LOG_TAIL_LINES);
  const pre = useRef<HTMLPreElement>(null);
  // Logs are read from the end: start scrolled to the newest lines.
  useEffect(() => {
    if (pre.current) pre.current.scrollTop = pre.current.scrollHeight;
  }, [text]);
  const shown = hidden ? lines.slice(-LOG_TAIL_LINES).join("\n") : text;
  return (
    <div>
      {hidden > 0 && (
        <button type="button" onClick={() => setAll(true)} disabled={lines.length > 20_000} className="mb-1 rounded text-xs text-fg-2 hover:text-fg disabled:text-fg-3">
          {lines.length > 20_000
            ? tn(hidden, "{count} earlier line hidden. Too long to show here.", "{count} earlier lines hidden. Too long to show here.", { count: hidden.toLocaleString() })
            : tn(hidden, "Show {count} earlier line", "Show {count} earlier lines", { count: hidden.toLocaleString() })}
        </button>
      )}
      <pre ref={pre} className="scroll-quiet max-h-56 overflow-auto whitespace-pre-wrap py-1 font-mono text-[12px] leading-[1.6] text-fg-2">
        <AnsiText text={shown} />
      </pre>
    </div>
  );
}

/* ---------------------------------------------------------------- changes */

export function ChangeStat({ additions, deletions, className }: { additions: number; deletions: number; className?: string }) {
  return (
    <span className={cx("inline-flex items-center gap-1.5 font-mono text-[11.5px] tabular-nums", className)}>
      <span className="text-success">+{additions}</span>
      <span className="text-error">−{deletions}</span>
    </span>
  );
}

export function Changeset({ e }: { e: EventOf<"changeset"> }) {
  const openReview = useStore((s) => s.openReview);
  const files = useMemo(() => e.payload.files.map((f) => ({ ...f, ...diffStats(f.diff) })), [e.payload.files]);
  const totals = files.reduce((s, f) => ({ a: s.a + f.additions, d: s.d + f.deletions }), { a: 0, d: 0 });
  const preview = files.find((f) => f.path === e.payload.previewPath) ?? files[0]!;
  return (
    <div className="overflow-hidden rounded-xl border border-line bg-surface shadow-card">
      <div className="flex items-center gap-2.5 px-3.5 py-2.5">
        <FileCode2 size={15} className="text-violet-text" />
        <span className="text-[13px] font-medium">
          {tn(files.length, "{n} file changed", "{n} files changed")}
        </span>
        <ChangeStat additions={totals.a} deletions={totals.d} />
        <span className="ms-auto text-xs text-fg-3">{t("Not committed")}</span>
      </div>
      <ul className="border-t border-line-soft">
        {files.map((f) => (
          <li key={f.path} className="group flex items-center gap-2.5 px-3.5 py-1.5 text-[13px] hover:bg-elevated/50">
            <span
              className={cx(
                "w-3 text-center font-mono text-[11px] font-medium",
                f.change === "added" ? "text-success" : f.change === "deleted" ? "text-error" : "text-warning",
              )}
              aria-label={f.change === "added" ? t("added") : f.change === "deleted" ? t("deleted") : t("modified")}
            >
              {f.change === "added" ? "A" : f.change === "deleted" ? "D" : "M"}
            </span>
            <span className="min-w-0 truncate font-mono text-[12px]">
              {dirname(f.path) && <span className="text-fg-3">{dirname(f.path)}/</span>}
              <span className="text-fg">{basename(f.path)}</span>
            </span>
            <ChangeStat additions={f.additions} deletions={f.deletions} className="ms-auto ps-2" />
            <button
              type="button"
              onClick={() => openReview(e.sessionId, f.path)}
              className="rounded-md px-2 py-0.5 text-xs text-fg-2 opacity-70 hover:bg-elevated hover:text-fg group-hover:opacity-100 focus-visible:opacity-100"
            >
              {t("Review diff")}
            </button>
          </li>
        ))}
      </ul>
      <div className="border-t border-line-soft">
        <div className="flex items-center justify-between px-3.5 pb-1 pt-2.5 text-xs text-fg-3">
          <span className="font-mono">{basename(preview.path)}</span>
        </div>
        <DiffView diff={preview.diff} lang={langForPath(preview.path)} limit={14} className="pb-2" />
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- tests */

export function TestSummary({ e }: { e: EventOf<"test_result"> }) {
  const { passed, failed, skipped, durationMs, command, cases } = e.payload;
  const total = passed + failed + skipped || 1;
  const [all, setAll] = useState(false);
  const shown = all ? cases : cases.slice(0, 4);
  const ok = failed === 0;
  return (
    <div className="rounded-xl border border-line bg-surface px-3.5 py-3 shadow-card">
      <div className="flex items-center gap-2.5">
        {ok ? <CheckCircle2 size={16} className="text-success" /> : <XCircle size={16} className="text-error" />}
        <span className="text-[13px] font-medium">{ok ? t("Tests passed") : tn(failed, "{n} test failed", "{n} tests failed")}</span>
        <span className="text-xs text-fg-2">
          {failed > 0 && skipped > 0
            ? t("{passed} passed, {failed} failed, {skipped} skipped", { passed, failed, skipped })
            : failed > 0
              ? t("{passed} passed, {failed} failed", { passed, failed })
              : skipped > 0
                ? t("{passed} passed, {skipped} skipped", { passed, skipped })
                : t("{passed} passed", { passed })}
        </span>
        <span className="ms-auto text-xs tabular-nums text-fg-3">{duration(durationMs)}</span>
      </div>
      <div className="mt-2.5 flex h-1.5 overflow-hidden rounded-full bg-elevated" role="img" aria-label={t("{passed} passed, {failed} failed, {skipped} skipped", { passed, failed, skipped })}>
        <span className="bg-success" style={{ width: `${(passed / total) * 100}%` }} />
        <span className="bg-error" style={{ width: `${(failed / total) * 100}%` }} />
        <span className="bg-fg-3/50" style={{ width: `${(skipped / total) * 100}%` }} />
      </div>
      <p className="mt-2 font-mono text-[11.5px] text-fg-3">{command}</p>
      <ul className="mt-2 space-y-1">
        {shown.map((c) => (
          <li key={c.name} className="flex items-start gap-2 text-[12.5px]">
            {c.status === "passed" ? (
              <CheckCircle2 size={13} className="mt-[3px] shrink-0 text-success" />
            ) : c.status === "failed" ? (
              <XCircle size={13} className="mt-[3px] shrink-0 text-error" />
            ) : (
              <CircleSlash size={13} className="mt-[3px] shrink-0 text-fg-3" />
            )}
            <span className="text-fg/85">
              {c.name}
              {c.message && <span className="block text-fg-3">{c.message}</span>}
            </span>
          </li>
        ))}
      </ul>
      {cases.length > 4 && (
        <button type="button" onClick={() => setAll(!all)} className="mt-1.5 rounded text-xs text-fg-2 hover:text-fg">
          {all ? t("Show fewer") : t("Show all {n} reported cases", { n: cases.length })}
        </button>
      )}
    </div>
  );
}

/* ---------------------------------------------------------------- progress */

export function ProgressCard({ e }: { e: EventOf<"progress"> }) {
  const { title, steps } = e.payload;
  const done = steps.filter((x) => x.status === "done").length;
  const failed = steps.some((x) => x.status === "failed");
  return (
    <div className="rounded-xl border border-line bg-surface px-3.5 py-3 shadow-card">
      <div className="flex items-baseline gap-2.5">
        <span className="text-[13px] font-medium">{title}</span>
        <span className="ms-auto text-xs tabular-nums text-fg-3">
          {t("{done} of {total} done", { done, total: steps.length })}
        </span>
      </div>
      <div className="mt-2.5 h-1.5 overflow-hidden rounded-full bg-elevated" role="progressbar" aria-valuemin={0} aria-valuemax={steps.length} aria-valuenow={done} aria-label={title}>
        <div className={cx("h-full rounded-full transition-[width] duration-500", failed ? "bg-error" : "bg-violet")} style={{ width: `${(done / steps.length) * 100}%` }} />
      </div>
      <ol className="mt-3 space-y-1.5">
        {steps.map((st, i) => (
          <li key={i} className="flex items-start gap-2.5 text-[12.5px]">
            {st.status === "done" ? (
              <CheckCircle2 size={14} className="mt-[2px] shrink-0 text-success" aria-label={t("Done")} />
            ) : st.status === "failed" ? (
              <XCircle size={14} className="mt-[2px] shrink-0 text-error" aria-label={t("Failed")} />
            ) : st.status === "active" ? (
              <span className="mt-[5px] ms-[3px] me-[3px] inline-block h-2 w-2 shrink-0 rounded-full bg-violet text-violet breathe" aria-label={t("In progress")} />
            ) : (
              <span className="mt-[3px] inline-block h-3 w-3 shrink-0 rounded-full border border-fg-3" aria-label={t("Not started")} />
            )}
            <span className={cx(st.status === "done" ? "text-fg-2" : "text-fg")}>{st.label}</span>
          </li>
        ))}
      </ol>
    </div>
  );
}

/* ---------------------------------------------------------------- image */

export function ImageBlock({ e }: { e: EventOf<"image"> }) {
  const { alt, caption, width, height } = e.payload;
  const { src, error } = useImageSrc(e.payload);
  const setTab = useStore((s) => s.setContextTab);
  return (
    <figure className="inline-block max-w-full">
      <div className="group relative overflow-hidden rounded-xl border border-line bg-surface p-3 shadow-card">
        {src ? (
          <ImageZoom src={src} alt={alt} caption={caption} naturalWidth={width}>
            <button type="button" className="block cursor-zoom-in rounded-lg" aria-label={t("Zoom {name}", { name: caption ?? alt })}>
              <img src={src} alt={alt} width={width} height={height} className="h-auto max-h-[340px] w-auto max-w-full rounded-lg" draggable={false} />
            </button>
          </ImageZoom>
        ) : (
          <div className="flex h-24 w-64 items-center justify-center text-center text-[12px] text-fg-3">{error ?? t("Loading image…")}</div>
        )}
      </div>
      <figcaption className="mt-2 flex items-center gap-3 text-xs text-fg-3">
        <span className="font-mono">{caption}</span>
        <button type="button" onClick={() => setTab("preview")} className="inline-flex items-center gap-1 rounded text-fg-2 hover:text-fg">
          <PanelRight size={12} /> {t("Show in panel")}
        </button>
      </figcaption>
    </figure>
  );
}

export function ImageZoom({ src, alt, caption, naturalWidth, children }: { src: string; alt: string; caption?: string; naturalWidth?: number; children: ReactNode }) {
  const [scale, setScale] = useState(1);
  const [natural, setNatural] = useState(naturalWidth ?? 0);
  return (
    <Dialog.Root onOpenChange={(o) => o && setScale(1)}>
      <Dialog.Trigger asChild>{children}</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/70 backdrop-blur-[2px] data-[state=open]:animate-in" />
        <Dialog.Content className="fixed inset-6 z-50 flex flex-col overflow-hidden rounded-2xl border border-line bg-bg shadow-pop focus:outline-none">
          <div className="flex h-12 shrink-0 items-center gap-2 border-b border-line px-3">
            <Dialog.Title className="font-mono text-xs text-fg-2">{caption ?? alt}</Dialog.Title>
            <Dialog.Description className="sr-only">{alt}</Dialog.Description>
            <div className="ms-auto flex items-center gap-1">
              <button type="button" aria-label={t("Zoom out")} onClick={() => setScale((s) => Math.max(0.5, s - 0.25))} className="rounded-lg p-1.5 text-fg-2 hover:bg-elevated hover:text-fg">
                <Minus size={15} />
              </button>
              <button type="button" onClick={() => setScale(1)} className="w-14 rounded-lg py-1 text-center text-xs tabular-nums text-fg-2 hover:bg-elevated">
                {Math.round(scale * 100)}%
              </button>
              <button type="button" aria-label={t("Zoom in")} onClick={() => setScale((s) => Math.min(4, s + 0.25))} className="rounded-lg p-1.5 text-fg-2 hover:bg-elevated hover:text-fg">
                <Plus size={15} />
              </button>
              <Dialog.Close className="ms-2 rounded-lg p-1.5 text-fg-2 hover:bg-elevated hover:text-fg" aria-label={t("Close")}>
                <X size={16} />
              </Dialog.Close>
            </div>
          </div>
          <div className="scroll-quiet flex flex-1 items-start justify-center overflow-auto p-8">
            <img
              src={src}
              alt={alt}
              onLoad={(ev) => !naturalWidth && setNatural(ev.currentTarget.naturalWidth)}
              style={natural ? { width: natural * scale } : undefined}
              className="max-w-none rounded-lg shadow-pop"
              draggable={false}
            />
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/* ---------------------------------------------------------------- permission */

const riskCopy = /* i18n */ {
  low: { label: "Low risk", cls: "text-success" },
  medium: { label: "Medium risk", cls: "text-warning" },
  high: { label: "High risk", cls: "text-error" },
};

export function permissionTitle(p: EventOf<"permission">["payload"], provider: string) {
  const isEdit = /patch|edit|write/i.test(p.tool);
  return isEdit ? t("{provider} wants to edit a file", { provider }) : t("{provider} wants to run a command", { provider });
}

const decidedLabel = /* i18n */ { allowed: "Allowed once", allowed_session: "Allowed for this session", denied: "Denied", expired: "Expired, never answered" } as const;

export function PermissionCard({
  e,
  provider,
  variant = "inline",
  context,
}: {
  e: EventOf<"permission">;
  provider: string;
  variant?: "inline" | "focused";
  context?: ReactNode;
}) {
  const p = e.payload;
  const inFlight = useStore((s) => s.permissionInFlight[p.requestId]);
  const resolve = useStore((s) => s.resolvePermission);
  const risk = riskCopy[p.risk];
  const isEdit = /patch|edit|write/i.test(p.tool);
  const focused = variant === "focused";

  if (p.state !== "pending") {
    const allowed = p.state === "allowed" || p.state === "allowed_session";
    return (
      <div className="flex items-center gap-2.5 rounded-xl border border-line-soft bg-surface/60 px-3.5 py-2.5 text-[13px]">
        {allowed ? (
          <ShieldCheck size={15} className="shrink-0 text-success" />
        ) : p.state === "expired" ? (
          <CircleSlash size={15} className="shrink-0 text-fg-3" />
        ) : (
          <Ban size={15} className="shrink-0 text-error" />
        )}
        <span className="shrink-0 text-fg">{p.autoReason ?? t(decidedLabel[p.state])}</span>
        <code className="min-w-0 truncate font-mono text-[12px] text-fg-2">{p.command}</code>
      </div>
    );
  }

  return (
    <section
      aria-label={t("Permission request")}
      className={cx("overflow-hidden rounded-2xl border border-warning/40 bg-surface", focused ? "shadow-pop" : "shadow-card")}
    >
      <header className="flex items-center gap-2.5 border-b border-line-soft bg-warning/[0.06] px-4 py-3">
        <ShieldAlert size={17} className="shrink-0 text-warning" />
        <h3 className={cx("font-semibold", focused ? "text-[15px]" : "text-[14px]")}>{permissionTitle(p, provider)}</h3>
        <span className="ms-auto shrink-0 text-xs text-fg-3">{clock(e.ts)}</span>
      </header>
      <div className="space-y-3.5 px-4 py-3.5">
        {context}
        <div className="flex items-start gap-2 rounded-xl border border-line bg-bg px-3 py-2.5">
          <span className="select-none font-mono text-[12.5px] text-fg-3">{isEdit ? "~" : "$"}</span>
          <code className="min-w-0 flex-1 break-all font-mono text-[12.5px] leading-[1.6] text-fg">{p.command}</code>
          <CopyButton text={p.command} label={isEdit ? t("Copy action") : t("Copy command")} />
        </div>
        {p.diff && (
          <div className="scroll-quiet relative max-h-64 overflow-auto rounded-xl border border-line-soft bg-bg py-1">
            <DiffView diff={p.diff} lang={langForPath(p.paths[0] ?? "")} />
          </div>
        )}
        <dl className="grid grid-cols-[112px_1fr] gap-x-3 gap-y-2.5 text-[13px]">
          <dt className="text-fg-3">{t("Tool")}</dt>
          <dd className="font-mono text-[12.5px] text-fg/90">{p.tool}</dd>
          <dt className="text-fg-3">{t("Directory")}</dt>
          <dd className="break-all font-mono text-[12.5px] text-fg/90">{p.cwd}</dd>
          <dt className="text-fg-3">{t("Affects")}</dt>
          <dd className="flex flex-wrap gap-1.5">
            {p.paths.length === 0 && <span className="text-fg-3">{t("No paths reported")}</span>}
            {p.paths.map((path) => (
              <span key={path} className="rounded-md border border-line-soft bg-elevated/60 px-1.5 py-px font-mono text-[11.5px] text-fg/90">
                {path}
              </span>
            ))}
          </dd>
          <dt className={cx("font-medium", risk.cls)}>{t(risk.label)}</dt>
          <dd dir="auto" className="text-fg-2">{p.reason}</dd>
        </dl>
      </div>
      <footer className="flex flex-wrap items-center gap-2 border-t border-line-soft px-4 py-3">
        <p className="me-auto max-w-[360px] text-xs leading-[1.5] text-fg-3">
          {inFlight
            ? t("Sent. Waiting for {provider} to confirm.", { provider })
            : p.supportsPersistentAllow
              ? t("{provider} offers a rule for the rest of this session.", { provider })
              : t("{provider} doesn't offer a lasting rule for this request, so it's one decision at a time.", { provider })}
        </p>
        <button
          type="button"
          disabled={Boolean(inFlight)}
          onClick={() => resolve(e.sessionId, p.requestId, "deny")}
          className="h-8 rounded-lg border border-line px-3.5 text-[13px] font-medium text-fg hover:bg-elevated disabled:opacity-50"
        >
          {inFlight === "deny" ? t("Denying…") : t("Deny")}
        </button>
        {p.supportsPersistentAllow && (
          <button
            type="button"
            disabled={Boolean(inFlight)}
            onClick={() => resolve(e.sessionId, p.requestId, "allow_session")}
            className="h-8 rounded-lg border border-line px-3.5 text-[13px] font-medium text-fg hover:bg-elevated disabled:opacity-50"
          >
            {inFlight === "allow_session" ? t("Allowing…") : (p.persistentLabel ?? t("Allow for session"))}
          </button>
        )}
        <button
          type="button"
          disabled={Boolean(inFlight)}
          onClick={() => resolve(e.sessionId, p.requestId, "allow")}
          className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white shadow-[0_1px_0_rgb(255_255_255/0.15)_inset] hover:brightness-110 disabled:opacity-60"
        >
          {inFlight === "allow" ? t("Allowing…") : t("Allow once")}
        </button>
      </footer>
    </section>
  );
}

/* ---------------------------------------------------------------- status */

export function ErrorBlock({ e }: { e: EventOf<"error"> }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-xl border border-error/35 bg-error/[0.06] px-3.5 py-3">
      <div className="flex items-start gap-2.5">
        <XCircle size={16} className="mt-0.5 shrink-0 text-error" />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-medium text-fg">{e.payload.title}</p>
          <p className="mt-0.5 text-[13px] text-fg-2">{e.payload.message}</p>
          {e.payload.detail && (
            <>
              <button type="button" onClick={() => setOpen(!open)} aria-expanded={open} className="mt-1.5 inline-flex items-center gap-1 rounded text-xs text-fg-2 hover:text-fg">
                <ChevronRight size={12} className={cx("rtl:-scale-x-100 transition-transform", open && "rotate-90 rtl:-rotate-90")} /> {t("Provider output")}
              </button>
              {open && <pre className="mt-1.5 whitespace-pre-wrap font-mono text-[12px] text-fg-2">{e.payload.detail}</pre>}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export function NoticeBlock({ e }: { e: EventOf<"notice"> }) {
  return (
    <div className="flex items-start gap-2 px-1 text-xs leading-[1.6] text-fg-3">
      <Info size={13} className={cx("mt-0.5 shrink-0", e.payload.tone === "demo" ? "text-warning" : "text-fg-3")} />
      <span>{e.payload.text}</span>
    </div>
  );
}

export function InvalidEventBlock({ reason }: { reason: string }) {
  return (
    <div className="flex items-start gap-2 rounded-xl border border-dashed border-line px-3.5 py-2.5 text-xs text-fg-2">
      <AlertTriangle size={14} className="mt-px shrink-0 text-warning" />
      <span>
        {t("One event from the provider couldn't be displayed and was skipped.")} <span className="font-mono text-fg-3">{reason}</span>
      </span>
    </div>
  );
}

