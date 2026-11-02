import { Pencil, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { StageEditor } from "../components/blocks/Workflow";
import { BUILT_IN, lineup, type Slots } from "../lib/workflows";
import { cx } from "../components/ui";
import type { Workflow } from "../lib/live";
import { isTauri } from "../lib/native";
import { useStore } from "../state/store";
import { t, tn } from "../lib/i18n";

const EMPTY: Workflow = {
  id: "",
  name: "",
  planAgent: "claude-code",
  buildAgent: "codex",
  reviewAgent: "claude-code",
  gatePlan: false,
  maxReviews: 3,
  createdAt: 0,
};

function Editor({ initial, onClose }: { initial: Workflow; onClose: () => void }) {
  const save = useStore((s) => s.saveWorkflow);
  const [w, setW] = useState<Workflow>(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <form
      className="space-y-3 rounded-xl border border-line bg-surface/50 p-3.5"
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        const err = await save({ ...w, gatePlan: w.gatePlan && Boolean(w.planAgent) });
        setBusy(false);
        if (err) setError(err);
        else onClose();
      }}
    >
      <label className="block text-[12px] text-fg-2">
        {t("Name")}
        <input
          autoFocus
          value={w.name}
          maxLength={80}
          onChange={(e) => setW({ ...w, name: e.target.value })}
          placeholder={t("e.g. Claude plans, DeepSeek builds, Codex reviews")}
          className="mt-1 h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]"
        />
      </label>
      <StageEditor value={w} onChange={(v: Slots) => setW({ ...w, ...v })} />
      {error && (
        <p role="alert" className="text-[12.5px] text-error">
          {error}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button type="button" onClick={onClose} className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">
          {t("Cancel")}
        </button>
        <button
          type="submit"
          disabled={busy || !w.name.trim()}
          className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          {w.id ? t("Save changes") : t("Save workflow")}
        </button>
      </div>
    </form>
  );
}

/** Settings → Workflows: the ready-made ones and the user's own. */
export function WorkflowsSettings() {
  const saved = useStore((s) => s.workflows);
  const profiles = useStore((s) => s.profiles);
  const load = useStore((s) => s.loadWorkflows);
  const remove = useStore((s) => s.deleteWorkflow);
  const [editing, setEditing] = useState<Workflow | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const row = (w: Workflow, builtIn: boolean) => (
    <li key={w.id} className="flex min-h-[48px] flex-wrap items-center gap-x-3 gap-y-1 px-3.5 py-2">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] text-fg">
          {builtIn ? t(w.name) : w.name}
          {builtIn && <span className="ml-2 text-[11.5px] text-fg-3">{t("Ready-made")}</span>}
        </div>
        <div className="truncate text-[12px] text-fg-3">
          {lineup(w, profiles)}
          {w.gatePlan ? ` · ${t("asks you before building")}` : ""}
          {w.reviewAgent ? ` · ${tn(w.maxReviews, "up to {n} fix round", "up to {n} fix rounds")}` : ""}
        </div>
      </div>
      {!builtIn &&
        (confirm === w.id ? (
          <span className="flex items-center gap-1.5 text-[12.5px]">
            {t("Delete “{name}”?", { name: w.name })}
            <button type="button" onClick={() => setConfirm(null)} className="h-7 rounded-md border border-line px-2 hover:bg-elevated">
              {t("Keep")}
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirm(null);
                void remove(w.id);
              }}
              className="h-7 rounded-md bg-error/90 px-2 font-medium text-white hover:bg-error"
            >
              {t("Delete")}
            </button>
          </span>
        ) : (
          <span className="flex gap-1">
            <button
              type="button"
              aria-label={t("Edit {name}", { name: w.name })}
              onClick={() => setEditing(w)}
              className="rounded-md p-1.5 text-fg-2 hover:bg-elevated hover:text-fg"
            >
              <Pencil size={14} />
            </button>
            <button
              type="button"
              aria-label={t("Delete {name}", { name: w.name })}
              onClick={() => setConfirm(w.id)}
              className="rounded-md p-1.5 text-fg-2 hover:bg-elevated hover:text-error"
            >
              <Trash2 size={14} />
            </button>
          </span>
        ))}
      {builtIn && (
        <button
          type="button"
          onClick={() => setEditing({ ...w, id: "", name: t("{name} (my version)", { name: t(w.name) }) })}
          className="h-7 rounded-md px-2 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg"
        >
          {t("Make a copy")}
        </button>
      )}
    </li>
  );

  return (
    <div className="space-y-3">
      <ul className="divide-y divide-line-soft rounded-xl border border-line-soft">
        {BUILT_IN.map((w) => row(w, true))}
        {saved.map((w) => row(w, false))}
      </ul>
      {editing ? (
        <Editor key={editing.id || editing.name} initial={editing} onClose={() => setEditing(null)} />
      ) : (
        <button
          type="button"
          disabled={!isTauri}
          onClick={() => setEditing(EMPTY)}
          className={cx("inline-flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-[13px] text-fg hover:bg-elevated disabled:opacity-50")}
        >
          <Plus size={14} /> {t("New workflow")}
        </button>
      )}
      {!isTauri && <p className="text-[12px] text-fg-3">{t("Saving workflows needs the desktop app.")}</p>}
    </div>
  );
}
