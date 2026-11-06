import * as Dialog from "@radix-ui/react-dialog";
import { Globe, KeyRound, Laptop, Plug, Plus, Terminal, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { cx, IconButton, Switch } from "../components/ui";
import { joinArgs, PRESETS, presetFor, splitArgs, type Preset } from "../lib/connections";
import { t } from "../lib/i18n";
import { connectionsApi, errorText, liveApi, type Connection, type ConnectionField } from "../lib/live";

const field =
  "mt-1 h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";
const primary = "h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50";
const quiet = "inline-flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated disabled:opacity-50";

/** The connections everyone has: listed here, and picked per bot in the bot's settings. */
export function useConnections() {
  const [list, setList] = useState<Connection[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = () =>
    connectionsApi.list().then(
      (l) => {
        setList(l);
        setError(null);
      },
      (e: unknown) => setError(errorText(e)),
    );
  useEffect(() => {
    if (liveApi.available) void reload();
  }, []);
  return { list, error, reload };
}

const kindIcon = (k: Connection["kind"]) =>
  k === "mac" ? <Laptop size={15} /> : k === "browser" ? <Globe size={15} /> : k === "http" || k === "github" ? <Plug size={15} /> : <Terminal size={15} />;

/** Settings → Connections. */
export function ConnectionsSettings() {
  const { list, error, reload } = useConnections();
  const [editing, setEditing] = useState<{ kind: Connection["kind"]; conn: Connection | null } | null>(null);
  const [adding, setAdding] = useState(false);
  if (!liveApi.available) return <p className="text-[13px] text-fg-3">{t("Connections work in the desktop app.")}</p>;
  if (!list) return error ? <p className="text-[13px] text-error">{error}</p> : null;
  const hasMac = list.some((c) => c.kind === "mac");
  return (
    <div className="max-w-[640px] space-y-3">
      {list.length === 0 ? (
        <p className="text-[13px] text-fg-3">{t("None yet. Add one, then pick it in a bot's settings.")}</p>
      ) : (
        <ul className="divide-y divide-line-soft rounded-lg border border-line-soft">
          {list.map((c) => (
            <li key={c.id} className="flex items-center gap-3 px-3 py-2.5">
              <span className="shrink-0 text-fg-2">{kindIcon(c.kind)}</span>
              <button type="button" onClick={() => setEditing({ kind: c.kind, conn: c })} className="min-w-0 flex-1 text-start">
                <p className="truncate text-[13.5px] text-fg">{c.name}</p>
                <p className="truncate text-xs text-fg-3">
                  {c.kind === "mac" ? t("Calendar, Reminders, Mail and Notes") : c.url || joinArgs([c.command, ...c.args])}
                  {" · "}
                  {c.actAlone ? t("acts on its own") : t("changes wait for you")}
                </p>
              </button>
              <ConfirmDelete onDelete={() => connectionsApi.remove(c.id).then(reload)} label={t("Remove {name}", { name: c.name })} />
            </li>
          ))}
        </ul>
      )}
      {adding ? (
        <div className="rounded-xl border border-line-soft p-3">
          <div className="flex items-center">
            <p className="flex-1 text-[13px] font-medium">{t("Add a connection")}</p>
            <IconButton label={t("Close")} size="sm" onClick={() => setAdding(false)}>
              <X size={14} />
            </IconButton>
          </div>
          <div className="mt-2 grid gap-2 sm:grid-cols-2">
            {PRESETS.filter((p) => p.kind !== "mac" || !hasMac).map((p) => (
              <button
                key={p.kind}
                type="button"
                onClick={() => {
                  setAdding(false);
                  setEditing({ kind: p.kind, conn: null });
                }}
                className="rounded-lg border border-line px-3 py-2.5 text-start hover:bg-elevated"
              >
                <span className="flex items-center gap-2 text-[13px] text-fg">
                  {kindIcon(p.kind)} {t(p.name)}
                </span>
                <span className="mt-1 line-clamp-3 block text-xs leading-[1.5] text-fg-3">{t(p.blurb)}</span>
              </button>
            ))}
            {(
              [
                ["stdio", t("Your own: a command"), t("Any MCP server that runs on this Mac, like npx -y some-mcp-server.")],
                ["http", t("Your own: an address"), t("Any MCP server on the web, by its https:// address.")],
              ] as const
            ).map(([kind, name, blurb]) => (
              <button
                key={kind}
                type="button"
                onClick={() => {
                  setAdding(false);
                  setEditing({ kind, conn: null });
                }}
                className="rounded-lg border border-dashed border-line px-3 py-2.5 text-start hover:bg-elevated"
              >
                <span className="flex items-center gap-2 text-[13px] text-fg">
                  {kindIcon(kind)} {name}
                </span>
                <span className="mt-1 block text-xs leading-[1.5] text-fg-3">{blurb}</span>
              </button>
            ))}
          </div>
        </div>
      ) : (
        <button type="button" onClick={() => setAdding(true)} className={quiet}>
          <Plus size={14} /> {t("Add a connection")}
        </button>
      )}
      <p className="text-xs leading-[1.55] text-fg-3">
        {t("Tokens stay in your Mac's keychain. While a bot works on a task, what a connection reads is allowed; what it changes waits for you (here or on Telegram), unless you let that connection act on its own.")}
      </p>
      <ConnectionEditor editing={editing} onClose={() => setEditing(null)} onSaved={() => void reload()} />
    </div>
  );
}

function ConfirmDelete({ onDelete, label }: { onDelete: () => Promise<unknown>; label: string }) {
  const [sure, setSure] = useState(false);
  return sure ? (
    <button type="button" autoFocus onBlur={() => setSure(false)} onClick={() => void onDelete()} className="h-7 rounded-lg bg-error/90 px-2.5 text-xs font-medium text-white hover:bg-error">
      {t("Remove")}
    </button>
  ) : (
    <IconButton label={label} size="sm" onClick={() => setSure(true)}>
      <Trash2 size={14} />
    </IconButton>
  );
}

function ConnectionEditor({ editing, onClose, onSaved }: { editing: { kind: Connection["kind"]; conn: Connection | null } | null; onClose: () => void; onSaved: () => void }) {
  return (
    <Dialog.Root open={Boolean(editing)} onOpenChange={(v) => !v && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="scroll-quiet fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-48px)] w-[min(520px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          {editing && (
            <Form
              key={editing.conn?.id ?? editing.kind}
              kind={editing.kind}
              conn={editing.conn}
              onDone={() => {
                onSaved();
                onClose();
              }}
            />
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Form({ kind, conn, onDone }: { kind: Connection["kind"]; conn: Connection | null; onDone: () => void }) {
  const preset: Preset | undefined = presetFor(kind);
  const own = kind === "stdio" || kind === "http";
  const [name, setName] = useState(conn?.name ?? (preset ? t(preset.name).split(":")[0]! : ""));
  const [secret, setSecret] = useState("");
  const [command, setCommand] = useState(conn ? joinArgs([conn.command, ...conn.args].filter(Boolean)) : "");
  const [url, setUrl] = useState(conn?.url ?? "");
  const [fields, setFields] = useState<ConnectionField[]>(conn ? (kind === "http" ? conn.headers : conn.env) : []);
  const [actAlone, setActAlone] = useState(conn?.actAlone ?? preset?.actAlone ?? false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const savedSecret = Boolean(conn && [...conn.env, ...conn.headers].some((f) => f.secret && f.saved));

  const submit = async () => {
    setError(null);
    let body: Pick<Connection, "command" | "args" | "url" | "env" | "headers">;
    if (preset) {
      if (preset.secret && !secret.trim() && !savedSecret) {
        setError(t("Paste the {what} first.", { what: t(preset.secret.label).toLowerCase() }));
        return;
      }
      body = preset.build(secret);
    } else if (kind === "http") {
      body = { command: "", args: [], url: url.trim(), env: [], headers: fields };
    } else {
      const parts = splitArgs(command.trim());
      body = { command: parts[0] ?? "", args: parts.slice(1), url: "", env: fields, headers: [] };
    }
    setBusy(true);
    try {
      await connectionsApi.save({ id: conn?.id ?? "", name, server: "", kind, actAlone, createdAt: 0, ...body });
      onDone();
    } catch (e: unknown) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
        {kindIcon(kind)} {conn ? t("Edit {name}", { name: conn.name }) : preset ? t(preset.name) : kind === "http" ? t("Your own: an address") : t("Your own: a command")}
      </Dialog.Title>
      <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
        {preset ? t(preset.blurb) : kind === "http" ? t("Any MCP server on the web, by its https:// address.") : t("Any MCP server that runs on this Mac, like npx -y some-mcp-server.")}
      </Dialog.Description>
      <form
        className="mt-4 space-y-3.5"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <label className="block text-[12px] text-fg-2">
          {t("Name")}
          <input value={name} onChange={(e) => setName(e.target.value)} maxLength={40} className={field} />
        </label>
        {preset?.secret && (
          <label className="block text-[12px] text-fg-2">
            {t(preset.secret.label)}
            <input
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              placeholder={savedSecret ? t("Saved. Type a new one to replace it.") : preset.secret.placeholder}
              className={cx(field, "font-mono text-[12.5px]")}
            />
            <span className="mt-1 block text-xs leading-[1.5] text-fg-3">{t(preset.secret.help)}</span>
          </label>
        )}
        {kind === "stdio" && (
          <label className="block text-[12px] text-fg-2">
            {t("Command")}
            <input value={command} onChange={(e) => setCommand(e.target.value)} placeholder="npx -y @modelcontextprotocol/server-everything" spellCheck={false} className={cx(field, "font-mono text-[12.5px]")} />
          </label>
        )}
        {kind === "http" && (
          <label className="block text-[12px] text-fg-2">
            {t("Address")}
            <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://mcp.example.com/mcp" spellCheck={false} className={cx(field, "font-mono text-[12.5px]")} />
          </label>
        )}
        {own && <Fields kind={kind} fields={fields} onChange={setFields} />}
        <div className="flex items-start justify-between gap-4 rounded-lg border border-line-soft px-3 py-2.5">
          <div>
            <p className="text-[13px] text-fg">{t("Let it act without asking")}</p>
            <p className="mt-0.5 text-xs leading-[1.5] text-fg-3">
              {actAlone ? t("Bots use every tool of it on their own while they work on a task.") : t("Reading is allowed; anything that changes something waits for you.")}
            </p>
          </div>
          <Switch label={t("Let it act without asking")} checked={actAlone} onChange={setActAlone} />
        </div>
        {error && (
          <p role="alert" className="text-[12.5px] text-error">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
          <button type="submit" disabled={busy || !name.trim()} className={primary}>
            {busy ? t("Saving…") : conn ? t("Save") : t("Add")}
          </button>
        </div>
      </form>
    </>
  );
}

/** A command's environment or an address's headers; a secret's value goes to the keychain. */
function Fields({ kind, fields, onChange }: { kind: "stdio" | "http"; fields: ConnectionField[]; onChange: (f: ConnectionField[]) => void }) {
  const set = (i: number, patch: Partial<ConnectionField>) => onChange(fields.map((f, j) => (j === i ? { ...f, ...patch } : f)));
  return (
    <fieldset>
      <legend className="text-[12px] text-fg-2">{kind === "http" ? t("Headers") : t("Environment")}</legend>
      <div className="mt-1 space-y-1.5">
        {fields.map((f, i) => (
          <div key={i} className="flex items-center gap-1.5">
            <input value={f.key} onChange={(e) => set(i, { key: e.target.value })} placeholder={kind === "http" ? "Authorization" : "API_KEY"} aria-label={t("Name")} spellCheck={false} className={cx(field, "mt-0 w-[38%] font-mono text-[12px]")} />
            <input
              type={f.secret ? "password" : "text"}
              value={f.value}
              onChange={(e) => set(i, { value: e.target.value })}
              placeholder={f.secret && f.saved ? t("Saved") : t("Value")}
              aria-label={t("Value")}
              spellCheck={false}
              className={cx(field, "mt-0 min-w-0 flex-1 font-mono text-[12px]")}
            />
            <button
              type="button"
              aria-pressed={f.secret}
              onClick={() => set(i, { secret: !f.secret })}
              title={t("Keep it in the keychain")}
              className={cx("grid h-9 w-9 shrink-0 place-items-center rounded-lg border", f.secret ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-violet-text" : "border-line text-fg-3 hover:bg-elevated")}
            >
              <KeyRound size={14} />
            </button>
            <IconButton label={t("Remove")} size="sm" onClick={() => onChange(fields.filter((_, j) => j !== i))}>
              <X size={14} />
            </IconButton>
          </div>
        ))}
        <button type="button" onClick={() => onChange([...fields, { key: "", value: "", secret: true }])} className="inline-flex h-8 items-center gap-1 rounded-lg px-2 text-[12.5px] text-violet-text hover:bg-surface">
          <Plus size={13} /> {kind === "http" ? t("Add a header") : t("Add a variable")}
        </button>
      </div>
    </fieldset>
  );
}
