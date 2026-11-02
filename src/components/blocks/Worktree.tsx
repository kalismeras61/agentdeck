import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { ChevronDown, GitBranch, GitMerge, History, SquareTerminal, Trash2, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import type { Session } from "../../data/demo";
import { errorText, gitApi, type MergeResult, type WorktreeState } from "../../lib/live";
import { useStore } from "../../state/store";
import { cx, Tip } from "../ui";

const home = (p: string) => p.replace(/^\/(Users|home)\/[^/]+/, "~");
const item = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface data-[disabled]:opacity-50";

function useWorktree(session: Session, open: boolean) {
  const [state, setState] = useState<WorktreeState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = () =>
    gitApi
      .sessionWorktree(session.id)
      .then((s) => {
        setState(s);
        setError(null);
      })
      .catch((e: unknown) => setError(errorText(e)));
  useEffect(() => {
    if (open) void load();
  }, [open, session.id, session.status]); // eslint-disable-line react-hooks/exhaustive-deps
  return { state, error, reload: load };
}

/** The branch a worktree session is on, with what you can do with it. */
export function WorktreeBadge({ session }: { session: Session }) {
  const [menu, setMenu] = useState(false);
  const [dialog, setDialog] = useState<"merge" | "remove" | null>(null);
  const { state } = useWorktree(session, menu);
  const openGit = useStore((s) => s.openGit);
  const setMode = useStore((s) => s.setViewMode);
  if (!session.worktree || !session.branch) return null;
  const changed = state ? state.files.length : null;
  return (
    <>
      <Dropdown.Root open={menu} onOpenChange={setMenu}>
        <Tip label={`Works in its own worktree: ${home(session.worktree)}`}>
          <Dropdown.Trigger className="no-drag inline-flex h-6 max-w-[240px] shrink-0 items-center gap-1.5 rounded-full border border-[color-mix(in_srgb,var(--violet)_40%,var(--border))] bg-violet-soft px-2 text-[12px] text-fg hover:brightness-110">
            <GitBranch size={12} className="shrink-0 text-violet-text" />
            <span className="truncate font-mono text-[11.5px]">{session.branch}</span>
            <ChevronDown size={12} className="shrink-0 text-fg-3" />
          </Dropdown.Trigger>
        </Tip>
        <Dropdown.Portal>
          <Dropdown.Content align="start" sideOffset={6} className="z-50 w-[280px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
            <div className="px-2 pb-1.5 pt-1.5 text-[11.5px] leading-[1.5] text-fg-3">
              {state ? (
                <>
                  {changed ? `${changed} ${changed === 1 ? "file" : "files"} changed` : "No changes yet"}
                  {state.base ? ` compared with ${state.base}` : ""}
                  {state.uncommitted.length ? `, ${state.uncommitted.length} not committed` : ""}
                  {state.behind ? `. ${state.base} has ${state.behind} newer ${state.behind === 1 ? "commit" : "commits"}.` : "."}
                </>
              ) : (
                "Looking at the worktree…"
              )}
            </div>
            <Dropdown.Item className={item} onSelect={() => openGit(session.projectId, session.worktree)}>
              <History size={14} className="text-fg-3" /> See its changes and history
            </Dropdown.Item>
            <Dropdown.Item className={item} onSelect={() => setMode("terminal")}>
              <SquareTerminal size={14} className="text-fg-3" /> Open a terminal there
            </Dropdown.Item>
            <Dropdown.Separator className="my-1 h-px bg-line" />
            <Dropdown.Item className={item} disabled={!state?.base} onSelect={() => setDialog("merge")}>
              <GitMerge size={14} className="text-fg-3" /> Merge into {state?.base ?? "the project's branch"}…
            </Dropdown.Item>
            <Dropdown.Item className={cx(item, "text-error")} onSelect={() => setDialog("remove")}>
              <Trash2 size={14} /> Remove the worktree…
            </Dropdown.Item>
          </Dropdown.Content>
        </Dropdown.Portal>
      </Dropdown.Root>
      {dialog === "merge" && <MergeDialog session={session} onClose={() => setDialog(null)} />}
      {dialog === "remove" && <RemoveDialog session={session} onClose={() => setDialog(null)} />}
    </>
  );
}

function Shell({ title, children, onClose }: { title: string; children: React.ReactNode; onClose: () => void }) {
  return (
    <Dialog.Root open onOpenChange={(v) => !v && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 flex max-h-[calc(100vh-48px)] w-[min(560px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          <Dialog.Title className="text-[15px] font-semibold">{title}</Dialog.Title>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function FileList({ files }: { files: { path: string; change: string }[] }) {
  return (
    <ul className="scroll-quiet mt-2 max-h-[180px] overflow-y-auto rounded-lg border border-line-soft py-1">
      {files.map((f) => (
        <li key={f.path} className="flex items-center gap-2 px-2.5 py-0.5 font-mono text-[11.5px]">
          <span className={cx("w-3 shrink-0 text-center", f.change === "added" || f.change === "untracked" ? "text-success" : f.change === "deleted" ? "text-error" : "text-warning")}>
            {f.change === "added" || f.change === "untracked" ? "A" : f.change === "deleted" ? "D" : f.change === "renamed" ? "R" : "M"}
          </span>
          <span className="truncate text-fg-2">{f.path}</span>
        </li>
      ))}
    </ul>
  );
}

function MergeDialog({ session, onClose }: { session: Session; onClose: () => void }) {
  const { state, error: loadError } = useWorktree(session, true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<MergeResult | null>(null);
  const send = useStore((s) => s.sendPrompt);
  const working = session.status === "running" || session.status === "waiting";

  const merge = () => {
    setBusy(true);
    setError(null);
    gitApi
      .merge(session.id)
      .then((r) => {
        setResult(r);
        setBusy(false);
      })
      .catch((e: unknown) => {
        setError(errorText(e));
        setBusy(false);
      });
  };

  const askAgent = () => {
    if (!result) return;
    send(
      session.id,
      `Merging this branch into ${result.base} conflicts in: ${result.conflicts.join(", ")}. Run \`git merge ${result.base}\` here, resolve the conflicts keeping both sides' intent, run the tests, and commit the merge.`,
      [],
    );
    onClose();
  };

  return (
    <Shell title={`Merge ${session.branch} into ${state?.base ?? "the project's branch"}`} onClose={onClose}>
      {result ? (
        result.merged ? (
          <>
            <p role="status" className="mt-2 text-[13px] leading-[1.6] text-success">
              {result.message}
            </p>
            <p className="mt-1 text-[12.5px] leading-[1.6] text-fg-2">
              The project folder now has these changes. The worktree stays until you remove it, so the agent can keep working there.
            </p>
            <div className="mt-4 flex justify-end">
              <Dialog.Close className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110">Done</Dialog.Close>
            </div>
          </>
        ) : result.conflicts.length ? (
          <>
            <p className="mt-2 flex items-start gap-2 text-[13px] leading-[1.6] text-fg">
              <TriangleAlert size={15} className="mt-0.5 shrink-0 text-warning" />
              {result.message} These files changed on both sides:
            </p>
            <FileList files={result.conflicts.map((p) => ({ path: p, change: "modified" }))} />
            <p className="mt-2 text-[12.5px] leading-[1.6] text-fg-2">
              The agent can bring {result.base} into its branch and resolve them in the worktree; then merge again.
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Close</Dialog.Close>
              <button type="button" onClick={askAgent} className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110">
                Ask the agent to resolve them
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="mt-2 text-[13px] text-fg-2">{result.message}</p>
            <div className="mt-4 flex justify-end">
              <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Close</Dialog.Close>
            </div>
          </>
        )
      ) : (
        <>
          {loadError && <p className="mt-2 text-[13px] text-error">{loadError}</p>}
          {state && (
            <>
              <p className="mt-2 text-[13px] leading-[1.6] text-fg-2">
                {state.files.length ? `${state.files.length} ${state.files.length === 1 ? "file" : "files"} will come into ${state.base}` : "Nothing has changed in the worktree yet."}
                {state.uncommitted.length
                  ? `. ${
                      state.uncommitted.length === 1 && state.files.length === 1
                        ? "It isn't committed yet"
                        : `${state.uncommitted.length} ${state.uncommitted.length === 1 ? "isn't" : "aren't"} committed yet`
                    }; AgentDeck commits ${state.uncommitted.length === 1 ? "it" : "them"} first as “${session.title} (AgentDeck)”.`
                  : "."}
              </p>
              {state.files.length > 0 && <FileList files={state.files} />}
              <p className="mt-2 text-[12px] leading-[1.6] text-fg-3">
                A merge commit is made in the project folder ({`git merge --no-ff ${state.branch}`}). If both sides changed the same lines nothing is merged and you see
                which files.
              </p>
            </>
          )}
          {working && <p className="mt-2 text-[12.5px] text-warning">The agent is still working; merge when it's done.</p>}
          {error && (
            <p role="alert" className="mt-2 text-[12.5px] text-error">
              {error}
            </p>
          )}
          <div className="mt-4 flex justify-end gap-2">
            <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Cancel</Dialog.Close>
            <button
              type="button"
              disabled={busy || !state || state.files.length === 0 || working}
              onClick={merge}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
            >
              <GitMerge size={14} />
              {busy ? "Merging…" : state?.uncommitted.length ? `Commit and merge into ${state.base}` : `Merge into ${state?.base ?? ""}`}
            </button>
          </div>
        </>
      )}
    </Shell>
  );
}

function RemoveDialog({ session, onClose }: { session: Session; onClose: () => void }) {
  const { state } = useWorktree(session, true);
  const [deleteBranch, setDeleteBranch] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const upsert = useStore((s) => s.upsertLiveSession);
  const lost = state?.uncommitted.length ?? 0;
  const unmerged = state?.ahead ?? 0;

  const remove = () => {
    setBusy(true);
    setError(null);
    gitApi
      .remove(session.id, deleteBranch)
      .then((row) => {
        upsert(row);
        onClose();
      })
      .catch((e: unknown) => {
        setError(errorText(e));
        setBusy(false);
      });
  };

  return (
    <Shell title="Remove the worktree" onClose={onClose}>
      <p className="mt-2 text-[13px] leading-[1.6] text-fg-2">
        Deletes the folder <span className="font-mono text-[12px] text-fg">{home(session.worktree ?? "")}</span>. The session stays, and goes back to the project folder with a fresh
        conversation.
      </p>
      {lost > 0 && (
        <p className="mt-2 flex items-start gap-2 text-[13px] leading-[1.6] text-error">
          <TriangleAlert size={15} className="mt-0.5 shrink-0" />
          {lost} uncommitted {lost === 1 ? "change is" : "changes are"} lost with it. Merge first to keep {lost === 1 ? "it" : "them"}.
        </p>
      )}
      <label className="mt-3 flex items-start gap-2 text-[13px]">
        <input type="checkbox" checked={deleteBranch} onChange={(e) => setDeleteBranch(e.target.checked)} className="mt-1 accent-[var(--violet)]" />
        <span>
          Also delete the branch <span className="font-mono text-[12px]">{session.branch}</span>
          {unmerged > 0 && deleteBranch && (
            <span className="block text-[12px] text-error">
              It has {unmerged} {unmerged === 1 ? "commit" : "commits"} that {state?.base ?? "the project's branch"} doesn't; they'd be gone.
            </span>
          )}
        </span>
      </label>
      {error && (
        <p role="alert" className="mt-2 text-[12.5px] text-error">
          {error}
        </p>
      )}
      <div className="mt-4 flex justify-end gap-2">
        <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">Cancel</Dialog.Close>
        <button
          type="button"
          disabled={busy}
          onClick={remove}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-error px-3.5 text-[13px] font-medium text-[color:var(--bg)] hover:brightness-110 disabled:opacity-50"
        >
          <Trash2 size={14} /> {busy ? "Removing…" : "Remove worktree"}
        </button>
      </div>
    </Shell>
  );
}
