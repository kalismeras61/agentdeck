import { withoutHidden } from "../../lib/setup";
import {
  ArrowRight,
  ArrowUpRight,
  Check,
  CheckCircle2,
  CircleSlash,
  ClipboardList,
  Hammer,
  SearchCheck,
  Square,
  User,
  Workflow as WorkflowIcon,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { providerLabel, type Session } from "../../data/demo";
import { agentLabel } from "../../lib/agentName";
import type { EventOf } from "../../lib/events";
import type { FlowRow, Workflow } from "../../lib/live";
import { agentKeyOf, BUILT_IN, CLI_AGENTS, lineup, STAGES, type Slots, type StageId } from "../../lib/workflows";
import { useStore } from "../../state/store";
import { AgentMark, cx } from "../ui";
import { Markdown } from "./Markdown";

type FlowEvent = EventOf<"flow">;

const STAGE_ICON = { plan: ClipboardList, build: Hammer, review: SearchCheck } as const;

const field =
  "h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";

/** Choose an agent for one stage: a CLI, one of your models, or (when starting) this session. */
function AgentSelect({
  value,
  onChange,
  stage,
  session,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  stage: StageId;
  session?: Session;
  label: string;
}) {
  const profiles = useStore((s) => s.profiles);
  const agyEnabled = useStore((s) => s.agyEnabled);
  return (
    <select aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} className={field}>
      {stage !== "build" && <option value="">Skip this stage</option>}
      {session && <option value="this">This session ({agentLabel(session)})</option>}
      <optgroup label="Agents on this Mac">
        {withoutHidden(CLI_AGENTS, agyEnabled).map((p) => (
          <option key={p} value={p}>
            {providerLabel[p]}
          </option>
        ))}
      </optgroup>
      {profiles.length > 0 && (
        <optgroup label="Your models">
          {profiles.map((m) => (
            <option key={m.id} value={`profile:${m.id}`}>
              {m.name}
            </option>
          ))}
        </optgroup>
      )}
    </select>
  );
}

const slotKey = { plan: "planAgent", build: "buildAgent", review: "reviewAgent" } as const;

/** The three stages and their options; shared by the start dialog and Settings. */
export function StageEditor({ value, onChange, session }: { value: Slots; onChange: (v: Slots) => void; session?: Session }) {
  return (
    <div className="space-y-3">
      <ol className="relative space-y-2">
        {STAGES.map((st, i) => {
          const v = value[slotKey[st.id]];
          const skipped = !v;
          const Icon = STAGE_ICON[st.id];
          return (
            <li key={st.id} className="relative grid grid-cols-[28px_minmax(0,1fr)_minmax(0,220px)] items-center gap-x-3">
              {i < STAGES.length - 1 && <span aria-hidden className="absolute left-[13.5px] top-[30px] h-[calc(100%-14px)] w-px bg-line" />}
              <span
                aria-hidden
                className={cx(
                  "relative z-[1] flex h-7 w-7 items-center justify-center rounded-lg border",
                  skipped ? "border-line-soft bg-bg text-fg-3" : "border-[color-mix(in_srgb,var(--violet)_45%,var(--border))] bg-violet-soft text-violet-text",
                )}
              >
                <Icon size={14} />
              </span>
              <div className={cx("min-w-0", skipped && "opacity-60")}>
                <div className="text-[13px] font-medium text-fg">{st.label}</div>
                <div className="text-[11.5px] leading-[1.45] text-fg-3">{st.hint}</div>
              </div>
              <AgentSelect label={`Who ${st.verb}`} stage={st.id} session={session} value={v} onChange={(nv) => onChange({ ...value, [slotKey[st.id]]: nv })} />
            </li>
          );
        })}
      </ol>
      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-lg border border-line-soft bg-surface/50 px-3 py-2 text-[12.5px]">
        <label className={cx("inline-flex items-center gap-2", !value.planAgent && "opacity-50")}>
          <input
            type="checkbox"
            disabled={!value.planAgent}
            checked={value.gatePlan && Boolean(value.planAgent)}
            onChange={(e) => onChange({ ...value, gatePlan: e.target.checked })}
            className="accent-[var(--violet)]"
          />
          Show me the plan before building
        </label>
        <label className={cx("inline-flex items-center gap-2", !value.reviewAgent && "opacity-50")}>
          Send back for fixes at most
          <select
            disabled={!value.reviewAgent}
            value={value.maxReviews}
            onChange={(e) => onChange({ ...value, maxReviews: Number(e.target.value) })}
            className="h-7 rounded-md border border-line bg-surface px-1.5 text-[12.5px] text-fg"
          >
            {[1, 2, 3, 4, 5].map((n) => (
              <option key={n} value={n}>
                {n} {n === 1 ? "time" : "times"}
              </option>
            ))}
          </select>
        </label>
      </div>
      <p className="text-[12px] leading-[1.55] text-fg-3">
        Runs on its own: each agent hands over as soon as it's done, review notes go straight back to the builder, and the agents' requests are approved
        for them. Risky ones (deleting files, pushing or rewriting git history, publishing, deploying, credentials, anything outside the project) still
        wait for you.
      </p>
    </div>
  );
}

/** Puts this session into the first stage that uses its agent. */
function withThis(w: Slots, session: Session): Slots {
  const key = agentKeyOf(session);
  const out = { ...w };
  for (const st of STAGES) {
    if (out[slotKey[st.id]] === key) {
      out[slotKey[st.id]] = "this";
      break;
    }
  }
  return out;
}

/** Start a workflow from a session: pick a template, adjust the stages, write the task. */
export function WorkflowForm({ session, onDone, footerStart }: { session: Session; onDone: () => void; footerStart?: ReactNode }) {
  const saved = useStore((s) => s.workflows);
  const profiles = useStore((s) => s.profiles);
  const startFlow = useStore((s) => s.startFlow);
  const saveWorkflow = useStore((s) => s.saveWorkflow);
  const select = useStore((s) => s.select);
  const templates = [...BUILT_IN, ...saved];
  const [picked, setPicked] = useState(templates[0]!.id);
  const [slots, setSlots] = useState<Slots>(() => withThis(BUILT_IN[0]!, session));
  const [task, setTask] = useState("");
  const [worktree, setWorktree] = useState(Boolean(session.worktree));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState<string | null>(null);
  const [savedNote, setSavedNote] = useState<string | null>(null);
  const template = templates.find((t) => t.id === picked);
  const name = template?.name ?? "Custom workflow";

  const pick = (t: Workflow) => {
    setPicked(t.id);
    setSlots(withThis(t, session));
    setSavedNote(null);
  };
  const edit = (v: Slots) => {
    setSlots(v);
    setPicked("custom");
    setSavedNote(null);
  };
  const resolved = (v: string) => (v === "this" ? agentKeyOf(session) : v);

  const submit = async () => {
    setBusy(true);
    setError(null);
    const err = await startFlow({ sessionId: session.id, name, task, ...slots, gatePlan: slots.gatePlan && Boolean(slots.planAgent), worktree });
    setBusy(false);
    if (err) setError(err);
    else {
      setTask("");
      onDone();
      select(session.id);
    }
  };

  const save = async () => {
    if (saving === null) return;
    const err = await saveWorkflow({
      id: "",
      name: saving,
      planAgent: resolved(slots.planAgent),
      buildAgent: resolved(slots.buildAgent),
      reviewAgent: resolved(slots.reviewAgent),
      gatePlan: slots.gatePlan && Boolean(slots.planAgent),
      maxReviews: slots.maxReviews,
      createdAt: 0,
    });
    if (err) setError(err);
    else {
      setSaving(null);
      setSavedNote(`Saved “${saving}”. Find it in Settings → Workflows.`);
    }
  };

  return (
    <form
      className="space-y-4"
      onSubmit={(ev) => {
        ev.preventDefault();
        void submit();
      }}
    >
      <div role="radiogroup" aria-label="Workflow" className="flex flex-wrap gap-1.5">
        {templates.map((t) => (
          <button
            key={t.id}
            type="button"
            role="radio"
            aria-checked={picked === t.id}
            onClick={() => pick(t)}
            title={lineup(t, profiles)}
            className={cx(
              "h-7 rounded-full border px-3 text-[12.5px] transition-colors",
              picked === t.id
                ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-fg"
                : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
            )}
          >
            {t.name}
          </button>
        ))}
        {picked === "custom" && (
          <span className="inline-flex h-7 items-center rounded-full border border-dashed border-line px-3 text-[12.5px] text-fg-2">Custom</span>
        )}
      </div>

      <StageEditor value={slots} onChange={edit} session={session} />

      <label className="block text-[12px] text-fg-2">
        Task
        <textarea
          value={task}
          onChange={(ev) => setTask(ev.target.value)}
          rows={3}
          placeholder="Fix the discount being applied after tax in invoice.py, with a test."
          className={cx(field, "mt-1 h-auto resize-none py-2 leading-[1.55]")}
        />
      </label>

      <label className="flex items-start gap-2 text-[12.5px] text-fg-2">
        <input
          type="checkbox"
          checked={worktree}
          disabled={Boolean(session.worktree)}
          onChange={(e) => setWorktree(e.target.checked)}
          className="mt-0.5 accent-[var(--violet)]"
        />
        <span>
          {session.worktree ? (
            <>All stages work in this session's worktree ({session.branch}).</>
          ) : (
            <>
              Work in a new git worktree: the agents change a copy on its own branch, and you merge it into the project when you're happy.
            </>
          )}
        </span>
      </label>

      {error && (
        <p role="alert" className="rounded-lg border border-error/35 bg-error/[0.06] px-2.5 py-1.5 text-[12.5px] text-fg">
          {error}
        </p>
      )}
      {savedNote && (
        <p role="status" className="text-[12.5px] text-success">
          {savedNote}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2 pt-1">
        {saving === null ? (
          <button
            type="button"
            onClick={() => setSaving(picked === "custom" ? "" : `${name} (copy)`)}
            className="h-8 rounded-lg px-2 text-[13px] text-fg-2 hover:bg-elevated hover:text-fg"
          >
            Save as workflow…
          </button>
        ) : (
          <span className="inline-flex items-center gap-1.5">
            <input
              autoFocus
              aria-label="Workflow name"
              value={saving}
              maxLength={80}
              onChange={(e) => setSaving(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void save();
                }
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setSaving(null);
                }
              }}
              placeholder="Name, e.g. DeepSeek builds"
              className={cx(field, "h-8 w-[200px]")}
            />
            <button
              type="button"
              disabled={!saving.trim()}
              onClick={() => void save()}
              className="h-8 rounded-lg border border-line px-2.5 text-[13px] hover:bg-elevated disabled:opacity-50"
            >
              Save
            </button>
          </span>
        )}
        <span className="ml-auto flex gap-2">
          {footerStart}
          <button
            type="submit"
            disabled={busy || !task.trim()}
            className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
          >
            {busy ? "Starting…" : "Start workflow"}
          </button>
        </span>
      </div>
    </form>
  );
}

/* ------------------------------------------------------------ live run */

function stagesOf(f: FlowRow): { id: StageId; sessionId: string }[] {
  const out: { id: StageId; sessionId: string }[] = [];
  if (f.planSession) out.push({ id: "plan", sessionId: f.planSession });
  out.push({ id: "build", sessionId: f.buildSession });
  if (f.reviewSession) out.push({ id: "review", sessionId: f.reviewSession });
  return out;
}

/** Strip above the conversation of every session in a workflow: where the work is, and the plan gate. */
export function FlowBar({ session, flow }: { session: Session; flow: FlowRow }) {
  const sessions = useStore((s) => s.sessions);
  const select = useStore((s) => s.select);
  const stop = useStore((s) => s.stopFlow);
  const approve = useStore((s) => s.approveFlowPlan);
  const revise = useStore((s) => s.reviseFlowPlan);
  const [notes, setNotes] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stages = stagesOf(flow);
  const current = flow.stage === "approve" ? "plan" : flow.stage;
  const active = flow.status === "active";
  const order = stages.map((s) => s.id);
  const curIdx = order.indexOf(current);
  const byId = (id: string) => sessions.find((s) => s.id === id);
  const curSession = byId(stages[curIdx]?.sessionId ?? "");
  const curName = curSession ? agentLabel(curSession) : "The agent";
  const builder = byId(flow.buildSession);

  let status: string;
  if (!active) status = `${flow.status === "done" ? "Finished" : "Stopped"}: ${flow.reason ?? ""}`;
  else if (flow.stage === "approve") status = "The plan is ready for you";
  else if (curSession?.status === "waiting") status = `${curName} is waiting for you`;
  else if (flow.stage === "plan") status = `${curName} is planning`;
  else if (flow.stage === "review") status = `${curName} is reviewing`;
  else status = flow.reviews > 0 ? `${curName} is fixing the review notes (round ${flow.reviews} of ${flow.maxReviews})` : `${curName} is building`;

  const act = async (fn: () => Promise<string | null>) => {
    setBusy(true);
    setError(null);
    const err = await fn();
    setBusy(false);
    if (err) setError(err);
    else setNotes(null);
  };

  return (
    <div className="border-b border-line-soft bg-surface/60">
      <div className="mx-auto flex max-w-[860px] flex-wrap items-center gap-x-3 gap-y-2 px-8 py-2 text-[12.5px]">
        <WorkflowIcon size={14} className={active ? "text-violet-text" : "text-fg-3"} aria-hidden />
        <ol aria-label={`${flow.name} stages`} className="flex flex-wrap items-center gap-1">
          {stages.map((st, i) => {
            const s = byId(st.sessionId);
            const done = flow.status === "done" || i < curIdx;
            const now = active && i === curIdx;
            const meta = STAGES.find((x) => x.id === st.id)!;
            return (
              <li key={st.id} className="flex items-center gap-1">
                {i > 0 && <ArrowRight size={12} className="text-fg-3" aria-hidden />}
                <button
                  type="button"
                  onClick={() => select(st.sessionId)}
                  aria-current={st.sessionId === session.id ? "page" : undefined}
                  title={s ? `Open ${s.title}` : undefined}
                  className={cx(
                    "inline-flex h-7 items-center gap-1.5 rounded-lg border px-2 transition-colors",
                    st.sessionId === session.id
                      ? "border-[color-mix(in_srgb,var(--violet)_50%,var(--border))] bg-violet-soft"
                      : "border-line-soft hover:bg-elevated",
                  )}
                >
                  {done ? (
                    <Check size={12} className="text-success" aria-label="done" />
                  ) : now ? (
                    <span className="h-2 w-2 animate-pulse rounded-full bg-violet motion-reduce:animate-none" aria-label="working" />
                  ) : (
                    <span className="h-2 w-2 rounded-full border border-fg-3" aria-hidden />
                  )}
                  <span className="text-fg">{meta.label}</span>
                  {s && <AgentMark provider={s.provider} size={15} />}
                  <span className="max-w-[120px] truncate text-fg-2">{s ? agentLabel(s) : "deleted"}</span>
                </button>
              </li>
            );
          })}
        </ol>
        <span className={cx("min-w-0 flex-1 truncate", active ? "text-fg-2" : "text-fg-3")} aria-live="polite">
          {status}
        </span>
        {active && (
          <button
            type="button"
            onClick={() => stop(flow.id)}
            className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 font-medium text-fg hover:bg-elevated"
          >
            <Square size={10} fill="currentColor" /> Stop
          </button>
        )}
      </div>
      {active && flow.stage === "approve" && (
        <div className="mx-auto max-w-[860px] px-8 pb-2.5">
          <div className="rounded-xl border border-[color-mix(in_srgb,var(--violet)_40%,var(--border))] bg-violet-soft/40 px-3 py-2.5 text-[13px]">
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-fg">
                Approve the plan and {builder ? agentLabel(builder) : "the builder"} starts building.
                {flow.planSession && flow.planSession !== session.id && (
                  <button
                    type="button"
                    onClick={() => select(flow.planSession!)}
                    className="ml-1.5 inline-flex items-center gap-0.5 text-violet-text hover:underline"
                  >
                    Read the plan <ArrowUpRight size={12} />
                  </button>
                )}
              </span>
              <span className="ml-auto flex gap-2">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setNotes((v) => (v === null ? "" : null))}
                  className="h-8 rounded-lg border border-line px-3 text-[13px] text-fg hover:bg-elevated disabled:opacity-50"
                >
                  Ask for changes
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void act(() => approve(flow.id))}
                  className="h-8 rounded-lg bg-violet-strong px-3 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
                >
                  Approve and build
                </button>
              </span>
            </div>
            {notes !== null && (
              <form
                className="mt-2 flex gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  void act(() => revise(flow.id, notes));
                }}
              >
                <input
                  autoFocus
                  aria-label="What should change in the plan"
                  value={notes}
                  onChange={(e) => setNotes(e.target.value)}
                  placeholder="What should change in the plan?"
                  className={cx(field, "h-8")}
                />
                <button
                  type="submit"
                  disabled={busy || !notes.trim()}
                  className="h-8 shrink-0 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated disabled:opacity-50"
                >
                  Send to planner
                </button>
              </form>
            )}
            {error && (
              <p role="alert" className="mt-2 text-[12.5px] text-error">
                {error}
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Workflow events in a session's own timeline. */
export function FlowBlock({ e, session }: { e: FlowEvent; session: Session }) {
  const p = e.payload;
  const select = useStore((s) => s.select);
  const [open, setOpen] = useState(false);

  if (p.kind === "started") {
    const mine = STAGES.find((x) => x.id === p.stage)!;
    return (
      <section aria-label="Workflow started" className="rounded-xl border border-line-soft bg-surface/60 px-3.5 py-2.5 text-[13px]">
        <div className="flex flex-wrap items-center gap-2">
          <WorkflowIcon size={15} className="text-violet-text" />
          <span className="text-fg">{p.name}</span>
          <span className="text-fg-3">
            {p.stages.map((s) => `${s.name} ${STAGES.find((x) => x.id === s.stage)!.verb}`).join(", ")}. This session {mine.verb}.
          </span>
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            className="ml-auto rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
          >
            {open ? "Hide task" : "Show task"}
          </button>
        </div>
        {open && (
          <div className="mt-2 border-t border-line-soft pt-2">
            <Markdown text={p.task} />
          </div>
        )}
      </section>
    );
  }

  if (p.kind === "ended") {
    const done = p.status === "done";
    return (
      <div className="flex items-center gap-2 px-1 text-[12.5px]">
        {done ? <CheckCircle2 size={14} className="text-success" /> : <CircleSlash size={14} className="text-fg-3" />}
        <span className="text-fg">{done ? "Workflow finished" : "Workflow stopped"}</span>
        <span className="text-fg-3">{p.reason}</span>
      </div>
    );
  }

  // Work handed to this session by the workflow: the task, a plan, a report or review notes.
  const fromAgent = Boolean(p.fromProvider && p.fromSessionId);
  return (
    <section aria-label={p.label} className="rounded-2xl border border-line bg-surface">
      <header className="flex items-center gap-2 border-b border-line-soft px-4 py-2.5 text-[13px]">
        {fromAgent ? (
          <AgentMark provider={p.fromProvider!} size={18} />
        ) : (
          <span className="flex h-[18px] w-[18px] items-center justify-center rounded-md bg-elevated text-fg-2">
            <User size={12} />
          </span>
        )}
        <span className="font-medium text-fg">{p.label}</span>
        <ArrowRight size={13} className="text-fg-3" />
        <AgentMark provider={session.provider} size={18} />
        <span className="text-fg-2">{agentLabel(session)}</span>
        {fromAgent && (
          <button
            type="button"
            onClick={() => select(p.fromSessionId!)}
            className="ml-auto inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-fg-2 hover:bg-elevated hover:text-fg"
          >
            Open <ArrowUpRight size={13} />
          </button>
        )}
      </header>
      <div className="max-h-[420px] overflow-y-auto px-4 py-3">
        <Markdown text={p.markdown} />
      </div>
    </section>
  );
}
