import * as Dialog from "@radix-ui/react-dialog";
import { Download, MessageCircle, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { providerLabel } from "../data/demo";
import { ago } from "../lib/format";
import { errorText, liveApi, type ImportFound } from "../lib/live";
import { useStore } from "../state/store";
import { AgentMark, cx } from "./ui";

const AGENTS = ["claude-code", "codex", "gemini-cli"] as const;
const MONTH = 30 * 24 * 60 * 60 * 1000;

function folderName(p: string) {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}

/** Import sessions the agents saved on their own, each into the project for its folder. */
export function ImportDialog() {
  const open = useStore((s) => s.importOpen);
  const setOpen = useStore((s) => s.setImportOpen);
  const projects = useStore((s) => s.projects);
  const reload = useStore((s) => s.reloadLive);
  const select = useStore((s) => s.select);
  const [found, setFound] = useState<ImportFound[] | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [agent, setAgent] = useState<"all" | (typeof AGENTS)[number]>("all");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const load = () =>
    liveApi
      .scanImports()
      .then((f) => {
        setFound(f);
        // Recent sessions not in AgentDeck yet are picked to start with.
        setPicked(new Set(f.filter((x) => !x.existing && Date.now() - x.updatedAt < MONTH).map((x) => x.file)));
      })
      .catch((e: unknown) => setError(errorText(e)));

  const scan = () => {
    setFound(null);
    setError(null);
    setDone(null);
    void load();
  };

  useEffect(() => {
    if (open) void load();
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps

  const onOpenChange = (v: boolean) => {
    setOpen(v);
    if (!v) {
      setFound(null);
      setError(null);
      setDone(null);
      setAgent("all");
    }
  };

  const shown = useMemo(() => (found ?? []).filter((f) => agent === "all" || f.provider === agent), [found, agent]);
  const importable = shown.filter((f) => !f.existing);
  const count = (a: string) => (found ?? []).filter((f) => (a === "all" || f.provider === a) && !f.existing).length;

  const destination = (f: ImportFound) => {
    if (!f.cwd || !f.folderExists) return { name: "Chats", note: f.cwd ? "its folder is gone" : "no folder recorded", chats: true };
    const known = projects.find((p) => p.path === f.cwd);
    return { name: known?.name ?? folderName(f.cwd), note: known ? "" : "new project", chats: false };
  };

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const picks = shown.filter((f) => picked.has(f.file) && !f.existing).map((f) => ({ provider: f.provider, file: f.file }));
      const res = await liveApi.importSessions(picks);
      await reload();
      const n = res.sessions.length;
      const places = res.projects.length;
      setDone(n ? `Imported ${n} ${n === 1 ? "session" : "sessions"} into ${places} ${places === 1 ? "place" : "places"}.` : "Nothing new to import.");
      if (res.sessions[0]) select(res.sessions[0].id);
      setFound((cur) => (cur ?? []).map((f) => (picked.has(f.file) ? { ...f, existing: f.existing ?? "imported" } : f)));
      setPicked(new Set());
    } catch (e: unknown) {
      setError(errorText(e));
    }
    setBusy(false);
  };

  const toggle = (file: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (next.has(file)) next.delete(file);
      else next.add(file);
      return next;
    });
  const pickedCount = shown.filter((f) => picked.has(f.file) && !f.existing).length;

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 flex max-h-[calc(100vh-48px)] w-[min(680px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          <div className="flex items-start gap-3">
            <div className="min-w-0 flex-1">
              <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
                <Download size={16} className="text-violet-text" /> Import sessions
              </Dialog.Title>
              <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
                Conversations Claude Code, Codex and Gemini CLI saved on this Mac. Each goes into the project for the folder it ran in; when that folder is
                unknown or gone, into Chats.
              </Dialog.Description>
            </div>
            <button
              type="button"
              onClick={scan}
              disabled={found === null && !error}
              aria-label="Look again"
              className="rounded-lg p-1.5 text-fg-3 hover:bg-elevated hover:text-fg disabled:opacity-50"
            >
              <RefreshCw size={14} className={cx(found === null && !error && "animate-spin motion-reduce:animate-none")} />
            </button>
          </div>

          <div role="radiogroup" aria-label="Agent" className="mt-4 flex flex-wrap gap-1.5">
            {(["all", ...AGENTS] as const).map((a) => (
              <button
                key={a}
                type="button"
                role="radio"
                aria-checked={agent === a}
                onClick={() => setAgent(a)}
                className={cx(
                  "inline-flex h-7 items-center gap-1.5 rounded-full border px-3 text-[12.5px]",
                  agent === a
                    ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-fg"
                    : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
                )}
              >
                {a !== "all" && <AgentMark provider={a} size={14} />}
                {a === "all" ? "All" : providerLabel[a]}
                <span className="text-fg-3">{found ? count(a) : ""}</span>
              </button>
            ))}
          </div>

          <div className="scroll-quiet mt-3 min-h-[160px] flex-1 overflow-y-auto rounded-xl border border-line-soft">
            {error ? (
              <p role="alert" className="p-4 text-[13px] text-error">
                {error}
              </p>
            ) : found === null ? (
              <p className="p-6 text-center text-[13px] text-fg-3">Looking for sessions…</p>
            ) : shown.length === 0 ? (
              <p className="p-6 text-center text-[13px] text-fg-3">No saved sessions found for this agent.</p>
            ) : (
              <ul className="divide-y divide-line-soft">
                {shown.map((f) => {
                  const d = destination(f);
                  const isIn = Boolean(f.existing);
                  return (
                    <li key={f.file}>
                      <label className={cx("flex items-start gap-3 px-3 py-2.5", isIn ? "opacity-55" : "cursor-pointer hover:bg-surface")}>
                        <input
                          type="checkbox"
                          disabled={isIn}
                          checked={isIn || picked.has(f.file)}
                          onChange={() => toggle(f.file)}
                          className="mt-1 accent-[var(--violet)]"
                        />
                        <AgentMark provider={f.provider} size={18} />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[13px] text-fg">{f.title}</span>
                          <span className="mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] text-fg-3">
                            <span className="inline-flex items-center gap-1 text-fg-2" title={f.cwd ?? undefined}>
                              {d.chats && <MessageCircle size={11} />}
                              {d.name}
                            </span>
                            {d.note && <span>({d.note})</span>}
                            <span>
                              {f.messages} {f.messages === 1 ? "message" : "messages"}
                            </span>
                            <span>{ago(f.updatedAt)}</span>
                            {isIn && <span className="text-fg-2">Already in AgentDeck</span>}
                          </span>
                        </span>
                      </label>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          {done && (
            <p role="status" className="mt-3 text-[12.5px] text-success">
              {done}
            </p>
          )}
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <span className="flex gap-1 text-[12.5px]">
              <button
                type="button"
                onClick={() => setPicked((cur) => new Set([...cur, ...importable.map((f) => f.file)]))}
                className="rounded-md px-2 py-1 text-fg-2 hover:bg-elevated hover:text-fg"
              >
                Select all
              </button>
              <button
                type="button"
                onClick={() => setPicked((cur) => new Set([...cur].filter((x) => !shown.some((f) => f.file === x))))}
                className="rounded-md px-2 py-1 text-fg-2 hover:bg-elevated hover:text-fg"
              >
                None
              </button>
            </span>
            <span className="text-[11.5px] text-fg-3">Grok Build sessions can't be imported yet.</span>
            <span className="ml-auto flex gap-2">
              <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Close</Dialog.Close>
              <button
                type="button"
                disabled={busy || pickedCount === 0}
                onClick={() => void run()}
                className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
              >
                {busy ? "Importing…" : `Import ${pickedCount || ""} ${pickedCount === 1 ? "session" : "sessions"}`.replace("  ", " ")}
              </button>
            </span>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
