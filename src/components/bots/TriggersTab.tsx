import * as Dialog from "@radix-ui/react-dialog";
import { CircleAlert, Copy, FolderOpen, Globe, Pencil, Play, Plus, Trash2, Webhook, X, Zap } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { whenText } from "../../lib/bots";
import { t } from "../../lib/i18n";
import { errorText, liveApi, pickFolder, triggersApi, type Bot, type BotTrigger } from "../../lib/live";
import { cx, IconButton, Switch } from "../ui";
import { PlacePicker } from "./PlacePicker";

const field =
  "mt-1 h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";
const trigger =
  "mt-1 flex h-9 w-full items-center gap-2 rounded-lg border border-line bg-surface px-2.5 text-start text-[13px] text-fg hover:bg-elevated data-[state=open]:bg-elevated";

const KINDS = /* i18n */ [
  ["page", "A web page changes"],
  ["folder", "New files in a folder"],
  ["github", "A GitHub label"],
  ["hook", "A call from this Mac"],
] as const;

const icon = (k: BotTrigger["kind"], size = 13) =>
  k === "page" ? <Globe size={size} /> : k === "folder" ? <FolderOpen size={size} /> : k === "github" ? <Zap size={size} /> : <Webhook size={size} />;

function summary(x: BotTrigger): string {
  const c = x.config;
  if (x.kind === "page") return t("{url}, every {n} min", { url: c.url ?? "", n: c.everyMinutes ?? 30 });
  if (x.kind === "folder") return c.path ?? "";
  if (x.kind === "github") return t("{repo}, label {label}", { repo: c.repo ?? "", label: c.label ?? "agentdeck" });
  return t("POST to this Mac");
}

const hookUrl = (port: number | null, secret?: string) => (port && secret ? `http://127.0.0.1:${port}/hooks/${secret}` : null);

/** A bot's triggers: what gives it a task on its own. */
export function TriggersTab({ bot }: { bot: Bot }) {
  const live = liveApi.available && !bot.id.startsWith("b-demo-");
  const [list, setList] = useState<BotTrigger[] | null>(null);
  const [port, setPort] = useState<number | null>(null);
  const [editing, setEditing] = useState<BotTrigger | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(() => {
    if (!live) return;
    triggersApi.list(bot.id).then(setList, (e: unknown) => setError(errorText(e)));
  }, [bot.id, live]);
  useEffect(() => {
    reload();
    if (live) triggersApi.hookInfo().then((h) => setPort(h.port), () => undefined);
  }, [reload, live]);

  if (!live) return <p className="p-4 text-[12.5px] text-fg-3">{t("Triggers work in the desktop app.")}</p>;
  if (!list) return <p className="p-4 text-[12.5px] text-fg-3">{t("Loading…")}</p>;
  return (
    <div className="space-y-3 p-3">
      <div className="flex items-center gap-2 px-1">
        <p className="min-w-0 flex-1 text-[11.5px] leading-[1.5] text-fg-3">{t("Things that give the bot a task on their own: a page changing, new files, a GitHub label, a call from a script.")}</p>
        <button
          type="button"
          onClick={() => setEditing(null)}
          className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
        >
          <Plus size={12} /> {t("New trigger")}
        </button>
      </div>
      {error && (
        <p role="alert" className="flex items-start gap-2 rounded-lg border border-error/35 bg-error/[0.06] px-2.5 py-2 text-[12.5px] text-fg">
          <span className="min-w-0 flex-1 break-words">{error}</span>
          <button type="button" onClick={() => setError(null)} aria-label={t("Dismiss")} className="shrink-0 rounded p-0.5 text-fg-3 hover:text-fg">
            <X size={13} />
          </button>
        </p>
      )}
      {list.length === 0 ? (
        <p className="px-1 py-6 text-center text-[12.5px] leading-[1.6] text-fg-3">{t("No triggers yet. Watch a page for a price, a folder for invoices, or a repository for issues labeled agentdeck.")}</p>
      ) : (
        <ul className="space-y-2">
          {list.map((x) => (
            <TriggerRow key={x.id} trigger={x} port={port} onEdit={() => setEditing(x)} onChange={reload} onError={setError} />
          ))}
        </ul>
      )}
      <Dialog.Root open={editing !== undefined} onOpenChange={(v) => !v && setEditing(undefined)}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
          <Dialog.Content className="scroll-quiet fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-48px)] w-[min(480px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
            {editing !== undefined && (
              <Form
                key={editing?.id ?? "new"}
                bot={bot}
                trigger={editing}
                onDone={() => {
                  setEditing(undefined);
                  reload();
                }}
              />
            )}
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  );
}

function TriggerRow({ trigger: x, port, onEdit, onChange, onError }: { trigger: BotTrigger; port: number | null; onEdit: () => void; onChange: () => void; onError: (e: string | null) => void }) {
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const url = x.kind === "hook" ? hookUrl(port, x.config.secret) : null;
  const act = async (p: Promise<unknown>) => {
    try {
      await p;
      onError(null);
    } catch (e: unknown) {
      onError(errorText(e));
    }
    onChange();
  };
  const curl = url ? `curl -X POST ${url} -d "What it should know"` : "";
  return (
    <li className={cx("rounded-lg border bg-surface px-2.5 py-2", x.enabled ? "border-line" : "border-line-soft")}>
      <div className="flex items-start gap-2">
        <p dir="auto" className={cx("line-clamp-3 min-w-0 flex-1 whitespace-pre-wrap break-words text-[13px] leading-[1.5]", x.enabled ? "text-fg" : "text-fg-2")}>
          {x.text}
        </p>
        <Switch label={x.enabled ? t("Turn this trigger off") : t("Turn this trigger on")} checked={x.enabled} onChange={(v) => void act(triggersApi.save({ ...x, enabled: v }))} />
      </div>
      <p className="mt-1 flex min-w-0 items-center gap-1.5 text-[11.5px] text-fg-3">
        <span className="shrink-0">{icon(x.kind, 11)}</span>
        <span dir="ltr" className="truncate">
          {summary(x)}
        </span>
      </p>
      {url && (
        <div className="mt-1.5 flex items-center gap-1.5">
          <code dir="ltr" className="min-w-0 flex-1 truncate rounded bg-bg px-1.5 py-1 font-mono text-[11px] text-fg-2">
            {curl}
          </code>
          <IconButton
            label={copied ? t("Copied") : t("Copy")}
            size="sm"
            onClick={() =>
              navigator.clipboard.writeText(curl).then(
                () => {
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1500);
                },
                () => undefined,
              )
            }
            className="h-7 w-7"
          >
            <Copy size={12} />
          </IconButton>
        </div>
      )}
      {x.kind === "hook" && !port && <p className="mt-1 text-[11.5px] text-warning">{t("No port was free for hooks. Quit apps using ports 47810–47814 and reopen AgentDeck.")}</p>}
      <p className="mt-0.5 text-[11.5px] text-fg-3">{x.lastFiredAt ? t("Last fired {time}", { time: whenText(x.lastFiredAt) }) : t("Hasn't fired yet")}</p>
      {x.lastError && (
        <p className="mt-1 flex items-start gap-1.5 text-[11.5px] leading-[1.45] text-warning">
          <CircleAlert size={12} className="mt-px shrink-0" />
          <span className="min-w-0 break-words">{x.lastError}</span>
        </p>
      )}
      <div className="mt-1.5 flex items-center gap-1">
        {x.kind !== "hook" && (
          <button
            type="button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              await act(triggersApi.check(x.id));
              setBusy(false);
            }}
            className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2.5 text-xs text-fg-2 hover:bg-elevated hover:text-fg disabled:opacity-50"
          >
            <Play size={12} /> {busy ? t("Looking…") : t("Check now")}
          </button>
        )}
        <span className="ms-auto flex items-center gap-0.5">
          <IconButton label={t("Edit trigger")} size="sm" onClick={onEdit} className="h-7 w-7">
            <Pencil size={13} />
          </IconButton>
          {confirm ? (
            <button type="button" onClick={() => void act(triggersApi.remove(x.id))} onBlur={() => setConfirm(false)} autoFocus className="h-7 rounded-lg bg-error/90 px-2.5 text-xs font-medium text-white hover:bg-error">
              {t("Delete")}
            </button>
          ) : (
            <IconButton label={t("Delete trigger")} size="sm" onClick={() => setConfirm(true)} className="h-7 w-7">
              <Trash2 size={13} />
            </IconButton>
          )}
        </span>
      </div>
    </li>
  );
}

function Form({ bot, trigger: x, onDone }: { bot: Bot; trigger: BotTrigger | null; onDone: () => void }) {
  const [kind, setKind] = useState<BotTrigger["kind"]>(x?.kind ?? "page");
  const [text, setText] = useState(x?.text ?? "");
  const [place, setPlace] = useState<string | null>(x ? x.projectId : (bot.projectId ?? null));
  const [url, setUrl] = useState(x?.config.url ?? "");
  const [every, setEvery] = useState(String(x?.config.everyMinutes ?? 30));
  const [path, setPath] = useState(x?.config.path ?? "");
  const [repo, setRepo] = useState(x?.config.repo ?? "");
  const [label, setLabel] = useState(x?.config.label ?? "agentdeck");
  const [users, setUsers] = useState((x?.config.users ?? []).join(", "));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const placeholder =
    kind === "page"
      ? t("Tell me what changed and whether it matters. If the price is under 1,000 TL, use notify_me.")
      : kind === "folder"
        ? t("Read each new invoice and add it to invoices.csv in your workspace.")
        : kind === "github"
          ? t("Fix the issue on a new branch, run the tests and open a pull request.")
          : t("Do what the call asks, then report.");
  const config =
    kind === "page"
      ? { url: url.trim(), everyMinutes: Math.max(5, Math.round(Number(every) || 30)) }
      : kind === "folder"
        ? { path: path.trim() }
        : kind === "github"
          ? { repo: repo.trim(), label: label.trim(), users: users.split(/[,\s]+/).filter(Boolean) }
          : { secret: x?.config.secret ?? "" };
  return (
    <>
      <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
        <Zap size={16} className="text-violet-text" /> {x ? t("Edit trigger") : t("New trigger")}
      </Dialog.Title>
      <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
        {t("When it fires, {name} gets a task: what you write here, with what's new underneath.", { name: bot.name })}
      </Dialog.Description>
      <form
        className="mt-4 space-y-3.5"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setError(null);
          try {
            await triggersApi.save({
              id: x?.id ?? "",
              botId: bot.id,
              kind,
              text,
              projectId: place,
              enabled: x?.enabled ?? true,
              config,
              nextCheckAt: 0,
              lastFiredAt: null,
              lastError: null,
              createdAt: 0,
            });
            onDone();
          } catch (err: unknown) {
            setError(errorText(err));
          } finally {
            setBusy(false);
          }
        }}
      >
        <div role="radiogroup" aria-label={t("When")} className="grid grid-cols-2 gap-1.5">
          {KINDS.map(([k, name]) => (
            <button
              key={k}
              type="button"
              role="radio"
              aria-checked={kind === k}
              onClick={() => setKind(k)}
              className={cx(
                "flex h-9 items-center gap-2 rounded-lg border px-2.5 text-start text-[12.5px]",
                kind === k ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-violet-text" : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
              )}
            >
              {icon(k)} {t(name)}
            </button>
          ))}
        </div>
        {kind === "page" && (
          <div className="flex gap-2">
            <label className="block min-w-0 flex-1 text-[12px] text-fg-2">
              {t("Page")}
              <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://…" spellCheck={false} className={cx(field, "font-mono text-[12.5px]")} />
            </label>
            <label className="block w-28 text-[12px] text-fg-2">
              {t("Every (min)")}
              <input type="number" min={5} value={every} onChange={(e) => setEvery(e.target.value)} className={cx(field, "tabular-nums")} />
            </label>
          </div>
        )}
        {kind === "folder" && (
          <label className="block text-[12px] text-fg-2">
            {t("Folder")}
            <div className="flex gap-2">
              <input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/Users/you/Downloads/Invoices" spellCheck={false} className={cx(field, "min-w-0 flex-1 font-mono text-[12.5px]")} />
              <button
                type="button"
                onClick={async () => {
                  const p = await pickFolder();
                  if (p) setPath(p);
                }}
                className="mt-1 h-9 shrink-0 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated"
              >
                {t("Choose…")}
              </button>
            </div>
          </label>
        )}
        {kind === "github" && (
          <>
            <div className="flex gap-2">
              <label className="block min-w-0 flex-1 text-[12px] text-fg-2">
                {t("Repository")}
                <input value={repo} onChange={(e) => setRepo(e.target.value)} placeholder="owner/name" spellCheck={false} className={cx(field, "font-mono text-[12.5px]")} />
              </label>
              <label className="block w-32 text-[12px] text-fg-2">
                {t("Label")}
                <input value={label} onChange={(e) => setLabel(e.target.value)} spellCheck={false} className={cx(field, "font-mono text-[12.5px]")} />
              </label>
            </div>
            <label className="block text-[12px] text-fg-2">
              {t("From")} <span className="text-fg-3">{t("(optional)")}</span>
              <input value={users} onChange={(e) => setUsers(e.target.value)} placeholder={t("GitHub usernames, comma-separated")} spellCheck={false} className={field} />
              <span className="mt-1 block text-[11.5px] leading-[1.5] text-fg-3">{t("Only issues and pull requests from these people fire it. Empty: the repository's owner, members and collaborators. Private repositories need the GitHub connection.")}</span>
            </label>
          </>
        )}
        {kind === "hook" && <p className="rounded-lg bg-surface px-2.5 py-2 text-[12px] leading-[1.5] text-fg-2">{t("After you save it, the trigger shows an address on this Mac (127.0.0.1) with a secret in it. A POST there, from Shortcuts, Raycast, cron or a script, gives the bot a task with what was sent. Web pages can't call it.")}</p>}
        <label className="block text-[12px] text-fg-2">
          {t("What the bot should do")}
          <textarea value={text} onChange={(e) => setText(e.target.value)} rows={3} placeholder={placeholder} className={cx(field, "h-auto resize-y py-2 leading-[1.55]")} />
        </label>
        <div className="text-[12px] text-fg-2">
          {t("Works in")}
          <PlacePicker bot={bot} value={place} onChange={setPlace} className={trigger} full />
        </div>
        {error && (
          <p role="alert" className="text-[12.5px] text-error">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
          <button type="submit" disabled={busy || !text.trim()} className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50">
            {busy ? t("Saving…") : x ? t("Save") : t("Add trigger")}
          </button>
        </div>
      </form>
    </>
  );
}
