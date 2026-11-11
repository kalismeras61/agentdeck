import { Bot, GitPullRequestArrow, MessageSquarePlus, Send, Users } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { cx } from "../../components/ui";
import { providerLabel } from "../../data/demo";
import { ago } from "../../lib/format";
import { t } from "../../lib/i18n";
import { errorText, type LiveProvider } from "../../lib/live";
import { hueOf, initials } from "../../lib/team";
import { answerPrompt, chatApi, threads, type MessageKind, type TeamMessages, type Thread } from "../../lib/teamchat";
import { useStore } from "../../state/store";

/** Agents that can use AgentDeck's team tools (MCP). */
const TEAM_AGENTS: LiveProvider[] = ["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code"];
const KINDS = /* i18n */ [
  { id: "question", label: "Question" },
  { id: "note", label: "Note" },
  { id: "review", label: "Review request" },
] as const;
const KIND_LABEL: Record<MessageKind, string> = /* i18n */ { question: "Question", note: "Note", review: "Review request", answer: "Answer" };

function Face({ name, email, size = 26 }: { name: string; email: string; size?: number }) {
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center rounded-full font-medium text-white"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.4), background: email === "*" ? "var(--fg-3)" : `hsl(${hueOf(email)} 55% 48%)` }}
    >
      {email === "*" ? <Users size={Math.round(size * 0.5)} /> : initials(name)}
    </span>
  );
}

type Props = {
  projectId: string | null;
  data: TeamMessages | null;
  people: { name: string; email: string }[];
  /** Your branches, for a review request. */
  branches: string[];
  sharing: boolean;
  demo: boolean;
  focus: string | null;
  /** Start a new message with these filled in (from a branch's "Ask for review"). */
  draft: { to: string; kind: MessageKind; branch: string } | null;
  onChanged: () => void;
  onReview: (branch: string) => void;
  onNotice: (text: string) => void;
};

/** Conversations with teammates and their agents: ask, answer, or let an agent answer. */
export function MessagesTab({ projectId, data, people, branches, sharing, demo, focus, draft, onChanged, onReview, onNotice }: Props) {
  const defaultProvider = useStore((s) => s.defaultProvider);
  const defaultModels = useStore((s) => s.defaultModels);
  const defaultEfforts = useStore((s) => s.defaultEfforts);
  const upsertLiveSession = useStore((s) => s.upsertLiveSession);
  const setDraft = useStore((s) => s.setDraft);
  const select = useStore((s) => s.select);
  const [readLocal, setReadLocal] = useState<Set<string>>(new Set());
  const read = useMemo(() => new Set([...(data?.read ?? []), ...readLocal]), [data, readLocal]);
  const list = useMemo(() => (data ? threads(data.messages, data.me.email, read) : []), [data, read]);
  const [picked, setPicked] = useState<string | null>(focus);
  const [composing, setComposing] = useState(false);
  const [to, setTo] = useState("*");
  const [kind, setKind] = useState<MessageKind>("question");
  const [branch, setBranch] = useState("");
  const [text, setText] = useState("");
  const [reply, setReply] = useState("");
  const [busy, setBusy] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  const agent: LiveProvider = TEAM_AGENTS.includes(defaultProvider as LiveProvider) ? (defaultProvider as LiveProvider) : "claude-code";

  useEffect(() => setPicked(focus), [focus]);
  useEffect(() => {
    if (!draft) return;
    setComposing(true);
    setPicked(null);
    setTo(draft.to);
    setKind(draft.kind);
    setBranch(draft.branch);
  }, [draft]);
  const thread = list.find((x) => x.id === picked) ?? (composing ? null : (list[0] ?? null));

  // Opening a conversation reads it.
  useEffect(() => {
    if (!thread || !data) return;
    const ids = thread.messages.filter((m) => m.from.email !== data.me.email && !read.has(m.id)).map((m) => m.id);
    if (ids.length === 0) return;
    setReadLocal((s) => new Set([...s, ...ids]));
    if (!demo && projectId) void chatApi.markRead(projectId, ids).then(onChanged).catch(() => {});
    end.current?.scrollIntoView({ block: "end" });
  }, [thread?.id, thread?.messages.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const run = async (fn: () => Promise<void>) => {
    if (demo) return onNotice(t("This works in the desktop app, on your own repositories."));
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      onNotice(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const send = () =>
    run(async () => {
      const m = await chatApi.send(projectId!, to, text, kind, null, kind === "review" ? branch || null : null);
      setText("");
      setComposing(false);
      setPicked(m.thread);
      onChanged();
    });

  const answer = (th: Thread) =>
    run(async () => {
      const last = th.messages[th.messages.length - 1]!;
      const toWho = th.with === "everyone" ? (last.from.email === data!.me.email ? "*" : last.from.email) : th.with.email;
      await chatApi.send(projectId!, toWho, reply, th.messages.some((m) => m.from.email !== data!.me.email) ? "answer" : "note", th.id, th.branch);
      setReply("");
      onChanged();
    });

  const letAgentAnswer = (th: Thread) =>
    run(async () => {
      const row = await chatApi.answerSession(projectId!, th.id, agent, defaultModels[agent] ?? "default", defaultEfforts[agent] ?? null);
      upsertLiveSession(row);
      setDraft(row.id, answerPrompt(th));
      select(row.id);
    });

  if (!sharing && !demo) {
    return (
      <div className="m-auto max-w-[48ch] px-8 text-center">
        <Users size={20} className="mx-auto text-fg-3" />
        <p className="mt-3 text-[13.5px] text-fg">{t("Join your team to message them")}</p>
        <p className="mt-1.5 text-[12.5px] leading-[1.6] text-fg-3">
          {t("Turn on “Share my work” above. Then you, your teammates and your agents can ask each other questions here, through the repository's remote.")}
        </p>
      </div>
    );
  }

  const me = data?.me.email ?? "";
  return (
    <div className="flex min-h-0 flex-1">
      <aside aria-label={t("Conversations")} className="scroll-quiet flex w-[300px] shrink-0 flex-col overflow-y-auto border-e border-line-soft">
        <div className="p-2">
          <button
            type="button"
            onClick={() => {
              setComposing(true);
              setPicked(null);
            }}
            className="flex h-8 w-full items-center justify-center gap-1.5 rounded-lg border border-line text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg"
          >
            <MessageSquarePlus size={14} />
            {t("New message")}
          </button>
        </div>
        {list.length === 0 && <p className="px-4 py-6 text-center text-[12.5px] text-fg-3">{t("No messages yet.")}</p>}
        {list.map((th) => {
          const last = th.messages[th.messages.length - 1]!;
          const who = th.with === "everyone" ? { name: t("Everyone"), email: "*" } : th.with;
          return (
            <button
              key={th.id}
              type="button"
              onClick={() => {
                setPicked(th.id);
                setComposing(false);
              }}
              aria-current={thread?.id === th.id ? "true" : undefined}
              className={cx("flex w-full items-start gap-2.5 border-b border-line-soft px-3 py-2.5 text-start", thread?.id === th.id && !composing ? "bg-elevated" : "hover:bg-surface")}
            >
              <Face name={who.name} email={who.email} />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-1.5">
                  <span dir="auto" className={cx("truncate text-[12.5px]", th.unread ? "font-semibold text-fg" : "text-fg")}>
                    {who.name}
                  </span>
                  {th.kind === "review" && <GitPullRequestArrow size={11} className="shrink-0 text-fg-3" aria-label={t("Review request")} />}
                  <span className="ms-auto shrink-0 text-[11px] tabular-nums text-fg-3">{ago(th.last)}</span>
                </span>
                <span dir="auto" className="mt-0.5 block truncate text-[12px] text-fg-3">
                  {last.from.email === me ? `${t("You")}: ` : ""}
                  {last.text}
                </span>
                {(th.waitingOnYou || th.unread > 0) && (
                  <span className="mt-1 flex gap-1.5">
                    {th.waitingOnYou && <span className="rounded bg-warning/15 px-1.5 text-[10.5px] text-warning">{t("Waiting for your answer")}</span>}
                    {th.unread > 0 && <span className="rounded bg-violet-soft px-1.5 text-[10.5px] text-violet-text">{t("{n} new", { n: th.unread })}</span>}
                  </span>
                )}
              </span>
            </button>
          );
        })}
      </aside>

      <section aria-label={t("Conversation")} className="flex min-w-0 flex-1 flex-col">
        {composing || !thread ? (
          <div className="mx-auto w-full max-w-[640px] space-y-3 p-6">
            <h3 className="text-[15px] font-semibold">{t("New message")}</h3>
            <div className="flex flex-wrap gap-2">
              <label className="flex items-center gap-2 text-[12.5px] text-fg-3">
                {t("To")}
                <select value={to} onChange={(e) => setTo(e.target.value)} className="h-8 max-w-[240px] rounded-md border border-line bg-bg px-2 text-[12.5px] text-fg outline-none">
                  <option value="*">{t("Everyone")}</option>
                  {people.map((p) => (
                    <option key={p.email} value={p.email}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              <div role="radiogroup" aria-label={t("Kind")} className="flex rounded-lg border border-line-soft p-0.5">
                {KINDS.map((k) => (
                  <button key={k.id} type="button" role="radio" aria-checked={kind === k.id} onClick={() => setKind(k.id)} className={cx("h-7 rounded-md px-2.5 text-[12px]", kind === k.id ? "bg-elevated text-fg" : "text-fg-3 hover:text-fg")}>
                    {t(k.label)}
                  </button>
                ))}
              </div>
              {kind === "review" && (
                <select value={branch} onChange={(e) => setBranch(e.target.value)} aria-label={t("Branch")} className="h-8 max-w-[260px] rounded-md border border-line bg-bg px-2 font-mono text-[12px] text-fg outline-none">
                  <option value="">{t("Pick your branch")}</option>
                  {branches.map((b) => (
                    <option key={b} value={b}>
                      {b}
                    </option>
                  ))}
                </select>
              )}
            </div>
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={6}
              placeholder={kind === "question" ? t("What do you want to know? Add the context: files, branch, what you tried. Their agent may answer.") : kind === "review" ? t("What should they look at?") : t("What should the team know?")}
              className="w-full resize-y rounded-lg border border-line bg-bg p-3 text-[13px] leading-[1.55] text-fg outline-none placeholder:text-fg-3 focus:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))]"
            />
            <div className="flex items-center justify-between gap-3">
              <p className="text-[11.5px] text-fg-3">{t("Sent through the repository's remote; anyone who can fetch it can read it.")}</p>
              <button type="button" disabled={busy || !text.trim() || (kind === "review" && !branch)} onClick={() => void send()} className="flex h-8 items-center gap-1.5 rounded-lg bg-violet-strong px-3 text-[12.5px] font-medium text-white disabled:opacity-50">
                <Send size={13} />
                {busy ? t("Sending…") : t("Send")}
              </button>
            </div>
          </div>
        ) : (
          <>
            <div className="flex items-center gap-2 border-b border-line-soft px-4 py-2.5">
              <span className="text-[13.5px] font-semibold">{thread.with === "everyone" ? t("Everyone") : thread.with.name}</span>
              <span className="rounded border border-line px-1.5 text-[10.5px] text-fg-3">{t(KIND_LABEL[thread.kind])}</span>
              {thread.branch && (
                <span dir="ltr" className="truncate font-mono text-[11.5px] text-fg-3">
                  {thread.branch}
                </span>
              )}
            </div>
            <div className="scroll-quiet min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-4">
              {thread.messages.map((m) => {
                const mine = m.from.email === me;
                return (
                  <div key={m.id} className={cx("flex gap-2.5", mine && "flex-row-reverse")}>
                    <Face name={m.from.name} email={m.from.email} size={24} />
                    <div className={cx("max-w-[78%] rounded-xl border px-3 py-2", mine ? "border-[color-mix(in_srgb,var(--violet)_35%,var(--border))] bg-violet-soft/40" : "border-line-soft bg-surface")}>
                      <p className="mb-0.5 flex flex-wrap items-center gap-x-1.5 text-[11px] text-fg-3">
                        <span className="text-fg-2">{mine ? t("You") : m.from.name}</span>
                        {m.agent && (
                          <span className="inline-flex items-center gap-0.5">
                            <Bot size={10} />
                            {m.agent}
                          </span>
                        )}
                        <span className="tabular-nums">{ago(m.createdAt)}</span>
                      </p>
                      <p dir="auto" className="whitespace-pre-wrap break-words text-[13px] leading-[1.55] text-fg">
                        {m.text}
                      </p>
                    </div>
                  </div>
                );
              })}
              <div ref={end} />
            </div>
            <div className="border-t border-line-soft p-3">
              {thread.waitingOnYou && (
                <div className="mb-2 flex flex-wrap gap-1.5">
                  {thread.kind === "review" && thread.branch ? (
                    <button type="button" disabled={busy} onClick={() => onReview(thread.branch!)} className="flex h-7 items-center gap-1.5 rounded-lg bg-violet-strong px-2.5 text-[12px] font-medium text-white disabled:opacity-50">
                      <GitPullRequestArrow size={13} />
                      {t("Review with {agent}", { agent: providerLabel[defaultProvider] })}
                    </button>
                  ) : (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void letAgentAnswer(thread)}
                      title={t("Opens a session where {agent} reads the code and drafts the answer; you approve it before it's sent.", { agent: providerLabel[agent] })}
                      className="flex h-7 items-center gap-1.5 rounded-lg bg-violet-strong px-2.5 text-[12px] font-medium text-white disabled:opacity-50"
                    >
                      <Bot size={13} />
                      {t("Let {agent} answer", { agent: providerLabel[agent] })}
                    </button>
                  )}
                </div>
              )}
              <div className="flex items-end gap-2">
                <textarea
                  value={reply}
                  onChange={(e) => setReply(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && reply.trim()) void answer(thread);
                  }}
                  rows={2}
                  placeholder={t("Write a reply")}
                  aria-label={t("Write a reply")}
                  className="min-h-[38px] flex-1 resize-y rounded-lg border border-line bg-bg px-3 py-2 text-[13px] text-fg outline-none placeholder:text-fg-3"
                />
                <button type="button" disabled={busy || !reply.trim()} onClick={() => void answer(thread)} aria-label={t("Send")} className="flex h-9 w-9 items-center justify-center rounded-lg bg-violet-strong text-white disabled:opacity-50">
                  <Send size={14} />
                </button>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
