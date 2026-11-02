import { Check, ChevronRight, Copy } from "lucide-react";
import { Fragment, useMemo, useState, type ReactNode } from "react";
import { useTokens } from "../../hooks/useTokens";
import { parseUnifiedDiff, type DiffLine } from "../../lib/events";
import type { Lang } from "../../lib/highlight";
import { cx, IconButton } from "../ui";
import type { ThemedToken } from "shiki/core";

function Tokens({ line, fallback }: { line: ThemedToken[] | undefined; fallback: string }) {
  if (!line) return <>{fallback || " "}</>;
  if (line.length === 0) return <> </>;
  return (
    <>
      {line.map((t, i) => (
        <span key={i} style={{ color: t.color, fontStyle: t.fontStyle === 1 ? "italic" : undefined }}>
          {t.content}
        </span>
      ))}
    </>
  );
}

export function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [done, setDone] = useState(false);
  return (
    <IconButton
      label={done ? "Copied" : label}
      size="sm"
      onClick={() => {
        void navigator.clipboard
          ?.writeText(text)
          .then(() => {
            setDone(true);
            window.setTimeout(() => setDone(false), 1400);
          })
          .catch(() => undefined);
      }}
    >
      {done ? <Check size={14} className="text-success" /> : <Copy size={14} />}
    </IconButton>
  );
}

export function CodeBlock({ code, lang, title, maxHeight = 360 }: { code: string; lang: Lang; title?: ReactNode; maxHeight?: number }) {
  const tokens = useTokens(code, lang);
  const lines = useMemo(() => code.replace(/\n$/, "").split("\n"), [code]);
  return (
    <div className="overflow-hidden rounded-xl border border-line bg-surface">
      <div className="flex h-9 items-center justify-between border-b border-line-soft pl-3 pr-1.5 text-xs text-fg-2">
        <span className="truncate font-mono">{title ?? lang}</span>
        <CopyButton text={code} label="Copy code" />
      </div>
      <pre className="scroll-quiet overflow-auto py-2.5 font-mono text-[12.5px] leading-[1.6]" style={{ maxHeight }}>
        <code>
          {lines.map((l, i) => (
            <div key={i} className="px-3.5 whitespace-pre">
              <Tokens line={tokens?.[i]} fallback={l} />
            </div>
          ))}
        </code>
      </pre>
    </div>
  );
}

/* ---------------------------------------------------------------- diff */

export function DiffView({ diff, lang, limit, className }: { diff: string; lang: Lang; limit?: number; className?: string }) {
  const all = useMemo(() => parseUnifiedDiff(diff), [diff]);
  const lines = limit ? all.slice(0, limit) : all;
  // Tokenize old and new sides as continuous text so the grammar keeps its state across lines.
  const newText = useMemo(() => all.filter((l) => l.kind !== "del" && l.kind !== "hunk").map((l) => l.text).join("\n"), [all]);
  const oldText = useMemo(() => all.filter((l) => l.kind === "del").map((l) => l.text).join("\n"), [all]);
  const newTokens = useTokens(newText, lang);
  const oldTokens = useTokens(oldText, lang);

  const indexed = useMemo(() => {
    let n = 0;
    let o = 0;
    return all.map((l) => {
      if (l.kind === "hunk") return { l, idx: -1 };
      if (l.kind === "del") return { l, idx: o++ };
      return { l, idx: n++ };
    });
  }, [all]);

  return (
    <div className={cx("font-mono text-[12px] leading-[1.65]", className)}>
      {lines.map((l, i) => {
        const meta = indexed[i]!;
        const tok = l.kind === "del" ? oldTokens?.[meta.idx] : l.kind === "hunk" ? undefined : newTokens?.[meta.idx];
        return <DiffRow key={i} line={l} tokens={tok} />;
      })}
      {limit && all.length > limit && (
        <div className="px-3 py-1.5 text-fg-3">{all.length - limit} more lines</div>
      )}
    </div>
  );
}

function DiffRow({ line, tokens }: { line: DiffLine; tokens: ThemedToken[] | undefined }) {
  if (line.kind === "hunk") {
    return <div className="select-none bg-elevated/40 px-3 py-1 text-fg-3">{line.text}</div>;
  }
  return (
    <div
      className={cx(
        "relative grid grid-cols-[36px_36px_16px_1fr]",
        line.kind === "add" && "bg-add",
        line.kind === "del" && "bg-del",
      )}
    >
      <span className="select-none pr-2 text-right text-fg-3/70">{line.oldNo ?? ""}</span>
      <span className="select-none pr-2 text-right text-fg-3/70">{line.newNo ?? ""}</span>
      <span
        aria-hidden
        className={cx("select-none text-center", line.kind === "add" ? "text-success" : line.kind === "del" ? "text-error" : "text-transparent")}
      >
        {line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
      </span>
      <span className="whitespace-pre pr-4">
        <span className="sr-only">{line.kind === "add" ? "Added: " : line.kind === "del" ? "Removed: " : ""}</span>
        <Tokens line={tokens} fallback={line.text} />
      </span>
    </div>
  );
}

/* ---------------------------------------------------------------- ansi */

const ANSI_COLORS: Record<number, string> = {
  30: "text-fg-3",
  31: "text-error",
  32: "text-success",
  33: "text-warning",
  34: "text-blue",
  35: "text-violet-text",
  36: "text-blue",
  37: "text-fg",
  90: "text-fg-3",
};

/** Renders SGR color codes; every other escape sequence is stripped. Output is text, never HTML. */
export function AnsiText({ text }: { text: string }) {
  const parts = useMemo(() => {
    const out: { text: string; cls: string; bold: boolean }[] = [];
    let cls = "";
    let bold = false;
    // oxlint-disable-next-line no-control-regex -- ANSI escapes are the point here
    const re = /\u001b\[([0-9;]*)m/g;
    let last = 0;
    let m: RegExpExecArray | null;
    /* oxlint-disable no-control-regex -- stripping terminal escapes and control bytes is the point */
    const clean = (s: string) =>
      s
        .replace(/\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)?/g, "") // OSC (window titles, hyperlinks)
        .replace(/\u001b\[[0-9;?]*[A-Za-ln-z]/g, "") // CSI other than colors (cursor moves, clears)
        .replace(/\u001b[@-Z\\-_]/g, "") // two-byte escapes
        .replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, "");
    /* oxlint-enable no-control-regex */
    while ((m = re.exec(text))) {
      if (m.index > last) out.push({ text: clean(text.slice(last, m.index)), cls, bold });
      for (const code of (m[1] || "0").split(";").map(Number)) {
        if (code === 0) {
          cls = "";
          bold = false;
        } else if (code === 1) bold = true;
        else if (ANSI_COLORS[code]) cls = ANSI_COLORS[code];
      }
      last = re.lastIndex;
    }
    if (last < text.length) out.push({ text: clean(text.slice(last)), cls, bold });
    return out;
  }, [text]);
  return (
    <>
      {parts.map((p, i) => (
        <span key={i} className={cx(p.cls, p.bold && "font-semibold")}>
          {p.text}
        </span>
      ))}
    </>
  );
}

/* ---------------------------------------------------------------- json */

const MAX_DEPTH = 12;
const MAX_KEYS = 200;

export function JsonTree({ value }: { value: unknown }) {
  return (
    <div role="tree" className="font-mono text-[12px] leading-[1.7]">
      <JsonNode name={null} value={value} depth={0} defaultOpen />
    </div>
  );
}

function JsonPrimitive({ value }: { value: unknown }) {
  if (value === null) return <span className="text-fg-3">null</span>;
  switch (typeof value) {
    case "string":
      return <span style={{ color: "var(--shiki-token-string)" }}>"{value.length > 500 ? `${value.slice(0, 500)}…` : value}"</span>;
    case "number":
      return <span style={{ color: "var(--shiki-token-constant)" }}>{String(value)}</span>;
    case "boolean":
      return <span style={{ color: "var(--shiki-token-keyword)" }}>{String(value)}</span>;
    default:
      return <span className="text-fg-3">{typeof value}</span>;
  }
}

function JsonNode({ name, value, depth, defaultOpen = false }: { name: string | null; value: unknown; depth: number; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen || depth < 2);
  const isObj = typeof value === "object" && value !== null;
  const label = name !== null && <span className="text-fg-2">{name}: </span>;
  if (!isObj || depth >= MAX_DEPTH) {
    return (
      <div role="treeitem" className="pl-5">
        {label}
        {isObj ? <span className="text-fg-3">…</span> : <JsonPrimitive value={value} />}
      </div>
    );
  }
  const entries = Array.isArray(value) ? value.map((v, i) => [String(i), v] as const) : Object.entries(value as Record<string, unknown>);
  const shown = entries.slice(0, MAX_KEYS);
  const [o, c] = Array.isArray(value) ? ["[", "]"] : ["{", "}"];
  return (
    <div role="treeitem" aria-expanded={open}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="group -ml-0.5 inline-flex items-center rounded px-0.5 text-left hover:bg-elevated"
      >
        <ChevronRight size={13} className={cx("mr-1 text-fg-3 transition-transform", open && "rotate-90")} />
        {label}
        <span className="text-fg-3">
          {o}
          {!open && ` ${entries.length} ${entries.length === 1 ? "item" : "items"} ${c}`}
        </span>
      </button>
      {open && (
        <div role="group" className="ml-[7px] border-l border-line-soft pl-2">
          {shown.map(([k, v]) => (
            <Fragment key={k}>
              <JsonNode name={k} value={v} depth={depth + 1} />
            </Fragment>
          ))}
          {entries.length > MAX_KEYS && <div className="pl-5 text-fg-3">{entries.length - MAX_KEYS} more</div>}
        </div>
      )}
      {open && <div className="pl-5 text-fg-3">{c}</div>}
    </div>
  );
}
