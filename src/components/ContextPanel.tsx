import { useImageSrc } from "../lib/images";
import { agentLabel } from "../lib/agentName";
import * as Tabs from "@radix-ui/react-tabs";
import { ChevronDown, ChevronRight, ChevronUp, File, FileCode2, Folder, FolderOpen, ImageIcon, Maximize2, PanelRightClose } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { demoProjectFiles, type Session } from "../data/demo";
import { useParsedEvents } from "../hooks/useSession";
import { useProject } from "../hooks/useWorkspace";
import { diffStats, type AgentEvent, type EventOf, type FileChange } from "../lib/events";
import { basename, clock, dirname, MOD } from "../lib/format";
import { langForPath } from "../lib/highlight";
import { useStore, type ContextTab } from "../state/store";
import { CopyButton, DiffView } from "./blocks/code";
import { ChangeStat, ImageZoom } from "./blocks/EventBlocks";
import { cx, IconButton } from "./ui";

const TABS: { id: ContextTab; label: string; key: string }[] = [
  { id: "files", label: "Files", key: "1" },
  { id: "changes", label: "Changes", key: "2" },
  { id: "preview", label: "Preview", key: "3" },
  { id: "activity", label: "Activity", key: "4" },
];

function useSessionDerived(session: Session) {
  const events = useParsedEvents(session);
  return useMemo(() => {
    const ok = events.flatMap((p) => (p.ok ? [p.event] : []));
    const changes = new Map<string, FileChange>();
    for (const e of ok) if (e.type === "changeset") for (const f of e.payload.files) changes.set(f.path, f);
    const images = ok.filter((e): e is EventOf<"image"> => e.type === "image");
    return { ok, changes: [...changes.values()], image: images[images.length - 1] };
  }, [events]);
}

export function ContextPanel({ session }: { session: Session }) {
  const tab = useStore((s) => s.contextTab);
  const setTab = useStore((s) => s.setContextTab);
  const toggle = useStore((s) => s.toggleContext);
  const d = useSessionDerived(session);

  return (
    <Tabs.Root value={tab} onValueChange={(v) => setTab(v as ContextTab)} className="flex h-full flex-col bg-sidebar">
      <div data-tauri-drag-region className="drag flex h-12 shrink-0 items-center gap-1 border-b border-line-soft pl-3 pr-2">
        <Tabs.List aria-label="Context" className="flex gap-0.5">
          {TABS.map((t) => (
            <Tabs.Trigger
              key={t.id}
              value={t.id}
              title={`${t.label} (${MOD}${t.key})`}
              className="relative h-7 rounded-md px-2.5 text-[12.5px] text-fg-2 hover:text-fg data-[state=active]:bg-elevated data-[state=active]:text-fg"
            >
              {t.label}
              {t.id === "changes" && d.changes.length > 0 && <span className="ml-1.5 text-[11px] tabular-nums text-fg-3">{d.changes.length}</span>}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        <IconButton label="Hide panel" shortcut={`${MOD}.`} onClick={toggle} size="sm" className="ml-auto">
          <PanelRightClose size={16} />
        </IconButton>
      </div>
      <Tabs.Content value="files" className="min-h-0 flex-1 focus-visible:outline-offset-[-2px]">
        <FilesTab changes={d.changes} session={session} />
      </Tabs.Content>
      <Tabs.Content value="changes" className="min-h-0 flex-1 focus-visible:outline-offset-[-2px]">
        <ChangesTab changes={d.changes} sessionId={session.id} />
      </Tabs.Content>
      <Tabs.Content value="preview" className="min-h-0 flex-1 focus-visible:outline-offset-[-2px]">
        <PreviewTab image={d.image} />
      </Tabs.Content>
      <Tabs.Content value="activity" className="min-h-0 flex-1 focus-visible:outline-offset-[-2px]">
        <ActivityTab session={session} events={d.ok} />
      </Tabs.Content>
    </Tabs.Root>
  );
}

function Empty({ icon: Icon, title, body }: { icon: typeof File; title: string; body: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-8 pb-12 text-center">
      <Icon size={20} className="mb-3 text-fg-3" />
      <p className="text-[13px] font-medium text-fg-2">{title}</p>
      <p className="mt-1 text-xs leading-[1.6] text-fg-3">{body}</p>
    </div>
  );
}

/* ---------------------------------------------------------------- files */

interface TreeNode {
  name: string;
  path: string;
  children?: TreeNode[];
}

function buildTree(paths: string[]): TreeNode[] {
  const root: TreeNode = { name: "", path: "", children: [] };
  for (const p of paths) {
    let cur = root;
    p.split("/").forEach((part, i, arr) => {
      const path = arr.slice(0, i + 1).join("/");
      const leaf = i === arr.length - 1;
      let next = cur.children!.find((c) => c.name === part);
      if (!next) {
        next = leaf ? { name: part, path } : { name: part, path, children: [] };
        cur.children!.push(next);
      }
      cur = next;
    });
  }
  const sort = (n: TreeNode[]): TreeNode[] =>
    n
      .sort((a, b) => (a.children ? 0 : 1) - (b.children ? 0 : 1) || a.name.localeCompare(b.name))
      .map((c) => (c.children ? { ...c, children: sort(c.children) } : c));
  return sort(root.children!);
}

function FilesTab({ changes, session }: { changes: FileChange[]; session: Session }) {
  const openChange = useStore((s) => s.openChange);
  const project = useProject(session.projectId);
  const changed = useMemo(() => new Map(changes.map((c) => [c.path, c.change])), [changes]);
  // Live sessions show only files the agent touched: AgentDeck doesn't read your project folder itself.
  const tree = useMemo(() => buildTree(Array.from(new Set([...(session.live ? [] : demoProjectFiles), ...changed.keys()]))), [changed, session.live]);
  const [open, setOpen] = useState<Record<string, boolean>>(() => {
    const all: Record<string, boolean> = { lib: true, "lib/features": true, "lib/features/auth": true, "lib/features/auth/widgets": true, test: true };
    for (const path of changed.keys())
      path
        .split("/")
        .slice(0, -1)
        .forEach((_, i, parts) => (all[parts.slice(0, i + 1).join("/")] = true));
    return all;
  });
  if (session.live && changes.length === 0) {
    return (
      <Empty
        icon={File}
        title="No files changed yet"
        body="Files the agent creates or edits in this session appear here. AgentDeck doesn't browse your project folder on its own."
      />
    );
  }

  const render = (nodes: TreeNode[], depth: number) =>
    nodes.map((n) => {
      const pad = { paddingLeft: 10 + depth * 14 };
      if (n.children) {
        const isOpen = open[n.path] ?? false;
        const hasChange = [...changed.keys()].some((k) => k.startsWith(`${n.path}/`));
        return (
          <li key={n.path} role="treeitem" aria-expanded={isOpen}>
            <button
              type="button"
              style={pad}
              onClick={() => setOpen({ ...open, [n.path]: !isOpen })}
              className="flex h-7 w-full items-center gap-1.5 rounded-md pr-2 text-left text-[13px] text-fg-2 hover:bg-surface hover:text-fg"
            >
              <ChevronRight size={12} className={cx("shrink-0 text-fg-3 transition-transform", isOpen && "rotate-90")} />
              {isOpen ? <FolderOpen size={14} className="shrink-0 text-fg-3" /> : <Folder size={14} className="shrink-0 text-fg-3" />}
              <span className="truncate">{n.name}</span>
              {hasChange && !isOpen && <span className="ml-auto h-1.5 w-1.5 rounded-full bg-warning" aria-label="Contains changes" />}
            </button>
            {isOpen && <ul role="group">{render(n.children, depth + 1)}</ul>}
          </li>
        );
      }
      const change = changed.get(n.path);
      const inner = (
        <>
          <span className="w-3 shrink-0" />
          <FileCode2 size={14} className={cx("shrink-0", change ? "text-fg-2" : "text-fg-3")} />
          <span className={cx("truncate", change ? "text-fg" : "text-fg-2")}>{n.name}</span>
          {change && (
            <span className={cx("ml-auto font-mono text-[11px] font-medium", change === "added" ? "text-success" : "text-warning")}>
              {change === "added" ? "A" : "M"}
            </span>
          )}
        </>
      );
      return (
        <li key={n.path} role="treeitem">
          {change ? (
            <button
              type="button"
              style={pad}
              onClick={() => openChange(n.path)}
              title="Show changes"
              className="flex h-7 w-full items-center gap-1.5 rounded-md pr-2.5 text-left text-[13px] hover:bg-surface"
            >
              {inner}
            </button>
          ) : (
            <div style={pad} className="flex h-7 items-center gap-1.5 pr-2.5 text-[13px]">
              {inner}
            </div>
          )}
        </li>
      );
    });

  return (
    <div className="scroll-quiet h-full overflow-y-auto px-2 py-2">
      <p className="px-2.5 pb-2 text-xs text-fg-3">{session.live ? `Changed in ${project?.name ?? "this project"}` : project?.name}</p>
      <ul role="tree" aria-label="Project files">
        {render(tree, 0)}
      </ul>
    </div>
  );
}

/* ---------------------------------------------------------------- changes */

function ChangesTab({ changes, sessionId }: { changes: FileChange[]; sessionId: string }) {
  const selected = useStore((s) => s.selectedChangePath);
  const openReview = useStore((s) => s.openReview);
  const openChange = useStore((s) => s.openChange);
  const files = useMemo(() => changes.map((c) => ({ ...c, ...diffStats(c.diff) })), [changes]);
  const current = files.find((f) => f.path === selected) ?? files[0];
  const idx = current ? files.indexOf(current) : -1;

  useEffect(() => {
    document.getElementById("diff-scroll")?.scrollTo({ top: 0 });
  }, [current?.path]);

  if (!current) return <Empty icon={FileCode2} title="No changes yet" body="Files the agent edits in this session show up here with a line-by-line diff." />;

  return (
    <div className="flex h-full flex-col">
      <ul className="shrink-0 border-b border-line-soft px-2 py-2">
        {files.map((f) => (
          <li key={f.path}>
            <button
              type="button"
              onClick={() => openChange(f.path)}
              aria-current={f.path === current.path}
              className={cx(
                "flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[12.5px]",
                f.path === current.path ? "bg-elevated text-fg" : "text-fg-2 hover:bg-surface hover:text-fg",
              )}
            >
              <span className={cx("w-3 font-mono text-[11px] font-medium", f.change === "added" ? "text-success" : "text-warning")}>
                {f.change === "added" ? "A" : "M"}
              </span>
              <span className="truncate font-mono">{basename(f.path)}</span>
              <ChangeStat additions={f.additions} deletions={f.deletions} className="ml-auto" />
            </button>
          </li>
        ))}
      </ul>
      <div className="flex shrink-0 items-center gap-1 border-b border-line-soft py-1.5 pl-3 pr-1.5">
        <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-fg-3" title={current.path}>
          {dirname(current.path) ? `${dirname(current.path)}/` : "Project root"}
        </span>
        <CopyButton text={current.path} label="Copy path" />
        <IconButton label="Previous file" size="sm" disabled={idx <= 0} onClick={() => openChange(files[idx - 1]!.path)}>
          <ChevronUp size={15} />
        </IconButton>
        <IconButton label="Next file" size="sm" disabled={idx >= files.length - 1} onClick={() => openChange(files[idx + 1]!.path)}>
          <ChevronDown size={15} />
        </IconButton>
        <IconButton label="Open in diff review" size="sm" onClick={() => openReview(sessionId, current.path)}>
          <Maximize2 size={14} />
        </IconButton>
      </div>
      <div id="diff-scroll" className="scroll-quiet relative min-h-0 flex-1 overflow-auto py-1">
        <DiffView diff={current.diff} lang={langForPath(current.path)} />
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- preview */

function PreviewTab({ image }: { image: EventOf<"image"> | undefined }) {
  if (!image) return <Empty icon={ImageIcon} title="Nothing to preview" body="Screenshots and images the agent produces appear here." />;
  return <PreviewImage key={image.eventId} image={image} />;
}

function PreviewImage({ image }: { image: EventOf<"image"> }) {
  const { alt, caption, width, height } = image.payload;
  const { src, error } = useImageSrc(image.payload);
  if (!src) return <Empty icon={ImageIcon} title={error ? "Couldn't show the image" : "Loading image…"} body={error ?? image.payload.path ?? ""} />;
  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line-soft py-1.5 pl-3 pr-1.5">
        <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-fg-3">{caption}</span>
        {width && height ? (
          <span className="text-[11px] tabular-nums text-fg-3">
            {width}×{height}
          </span>
        ) : null}
        <ImageZoom src={src} alt={alt} caption={caption} naturalWidth={width}>
          <IconButton label="Open full size" size="sm">
            <Maximize2 size={14} />
          </IconButton>
        </ImageZoom>
      </div>
      <div className="scroll-quiet flex min-h-0 flex-1 items-start justify-center overflow-auto bg-[radial-gradient(var(--border-soft)_1px,transparent_1px)] p-6 [background-size:14px_14px]">
        <ImageZoom src={src} alt={alt} caption={caption} naturalWidth={width}>
          <button type="button" className="cursor-zoom-in" aria-label={`Zoom ${caption ?? alt}`}>
            <img src={src} alt={alt} className="h-auto max-h-full w-auto max-w-full rounded-xl shadow-pop" draggable={false} />
          </button>
        </ImageZoom>
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- activity */

function describe(e: AgentEvent, provider: string): string {
  switch (e.type) {
    case "user_message":
      return "You sent a task";
    case "assistant_text":
      return `${provider} replied`;
    case "tool_group":
      return e.payload.title;
    case "changeset":
      return `Changed ${e.payload.files.length} ${e.payload.files.length === 1 ? "file" : "files"}`;
    case "test_result":
      return `Tests: ${e.payload.passed} passed, ${e.payload.failed} failed`;
    case "image":
      return "Attached a screenshot";
    case "permission":
      return e.payload.state === "pending" ? `Asked: ${e.payload.command}` : e.payload.state === "denied" ? "Request denied" : "Request allowed";
    case "error":
      return e.payload.title;
    case "notice":
      return e.payload.tone === "demo" ? "Demo note" : e.payload.text;
    case "progress":
      return e.payload.title;
    case "question":
      return e.payload.state === "pending" ? `Asked: ${e.payload.questions[0]?.question ?? "a question"}` : "Question answered";
    case "plan":
      return e.payload.state === "pending"
        ? "Proposed a plan"
        : e.payload.state === "handed_off" || e.payload.state === "workflow"
          ? "Plan handed off"
          : e.payload.state === "approved"
            ? "Plan approved"
            : "Plan";
    case "relay":
      return e.payload.kind === "message"
        ? `Message from ${e.payload.fromProvider === "codex" ? "Codex" : "Claude Code"} (round ${e.payload.round})`
        : e.payload.kind === "started"
          ? "Relay started"
          : "Relay ended";
    case "handoff":
      return e.payload.direction === "in" ? "Received a plan from another session" : "Handed a plan to another session";
    case "flow":
      return e.payload.kind === "input" ? e.payload.label : e.payload.kind === "started" ? `Workflow started: ${e.payload.name}` : "Workflow ended";
    case "denied":
      return `Skipped ${e.payload.actions.length} ${e.payload.actions.length === 1 ? "action" : "actions"} that need your OK`;
  }
}

function ActivityTab({ session, events }: { session: Session; events: AgentEvent[] }) {
  const tasks = useMemo(() => {
    for (let i = events.length - 1; i >= 0; i--) {
      const e = events[i]!;
      if (e.type !== "assistant_text") continue;
      const all = [...e.payload.markdown.matchAll(/^\s*[-*] \[( |x)\] (.+)$/gim)];
      if (all.length) return all.map((m) => ({ done: m[1]!.toLowerCase() === "x", text: m[2]! }));
    }
    return [];
  }, [events]);
  const done = tasks.filter((t) => t.done).length;
  const timeline = events.filter((e) => e.type !== "notice");

  if (events.length === 0) return <Empty icon={File} title="No activity yet" body="Agent steps, tool runs and approvals are listed here with their times." />;

  return (
    <div className="scroll-quiet h-full overflow-y-auto px-4 py-4">
      {tasks.length > 0 && (
        <section className="mb-6">
          <div className="mb-2 flex items-baseline justify-between">
            <h3 className="text-[13px] font-medium">Plan</h3>
            <span className="text-xs tabular-nums text-fg-3">
              {done} of {tasks.length} done
            </span>
          </div>
          <div className="mb-3 h-1 overflow-hidden rounded-full bg-elevated">
            <div className="h-full rounded-full bg-violet transition-[width] duration-500" style={{ width: `${(done / tasks.length) * 100}%` }} />
          </div>
          <ul className="space-y-1.5">
            {tasks.map((t) => (
              <li key={t.text} className="flex items-start gap-2 text-[12.5px]">
                <span className={cx("mt-[5px] h-2 w-2 shrink-0 rounded-full", t.done ? "bg-violet" : "border border-fg-3")} />
                <span className={t.done ? "text-fg-2" : "text-fg"}>{t.text.replace(/`/g, "")}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
      <h3 className="mb-2 text-[13px] font-medium">Timeline</h3>
      <ol className="relative">
        <span aria-hidden className="absolute bottom-2 left-[3px] top-2 w-px bg-line" />
        {timeline.map((e) => (
          <li key={e.eventId} className="relative flex items-start gap-3 py-1.5 pl-4 text-[12.5px]">
            <span
              className={cx(
                "absolute left-0 top-[11px] h-[7px] w-[7px] rounded-full ring-2 ring-sidebar",
                e.type === "error"
                  ? "bg-error"
                  : e.type === "permission" && e.payload.state === "pending"
                    ? "bg-warning"
                    : e.type === "user_message"
                      ? "bg-fg-2"
                      : "bg-fg-3",
              )}
            />
            <span className="min-w-0 flex-1 text-fg/85">{describe(e, agentLabel(session))}</span>
            <span className="shrink-0 tabular-nums text-fg-3">{clock(e.ts)}</span>
          </li>
        ))}
      </ol>
    </div>
  );
}
