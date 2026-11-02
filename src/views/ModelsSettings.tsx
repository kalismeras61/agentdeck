import { KeyRound, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { cx } from "../components/ui";
import { errorText, liveApi, type ModelProfile } from "../lib/live";
import { useStore } from "../state/store";
import { t } from "../lib/i18n";

type Draft = Omit<ModelProfile, "createdAt" | "hasKey"> & { key: string };

/** Starting points. Model names change often: check the provider's current list. */
const PRESETS: { label: string; note: string; draft: Omit<Draft, "id" | "key"> }[] = /* i18n */ [
  {
    label: "DeepSeek in Claude Code",
    note: "DeepSeek's Anthropic-compatible API",
    draft: { name: "DeepSeek", kind: "anthropic", baseUrl: "https://api.deepseek.com/anthropic", model: "deepseek-v4-pro", smallModel: "deepseek-flash" },
  },
  {
    label: "DeepSeek, AgentDeck agent",
    note: "OpenAI-compatible API",
    draft: { name: "DeepSeek API", kind: "openai", baseUrl: "https://api.deepseek.com", model: "deepseek-v4-pro", smallModel: "" },
  },
  {
    label: "Kimi in Claude Code",
    note: "Moonshot's Anthropic-compatible API",
    draft: { name: "Kimi", kind: "anthropic", baseUrl: "https://api.moonshot.ai/anthropic", model: "kimi-k3", smallModel: "kimi-k3" },
  },
  {
    label: "Kimi, AgentDeck agent",
    note: "OpenAI-compatible API",
    draft: { name: "Kimi API", kind: "openai", baseUrl: "https://api.moonshot.ai/v1", model: "kimi-k3", smallModel: "" },
  },
  {
    label: "GLM (Z.ai) in Claude Code",
    note: "Z.ai's Anthropic-compatible API, with a GLM Coding Plan key",
    draft: { name: "GLM", kind: "anthropic", baseUrl: "https://api.z.ai/api/anthropic", model: "glm-5.3", smallModel: "glm-5.3-flash" },
  },
  {
    label: "Ollama in Claude Code",
    note: "Local; no key needed",
    draft: { name: "Ollama", kind: "anthropic", baseUrl: "http://localhost:11434", model: "qwen3-coder", smallModel: "" },
  },
  {
    label: "Ollama, AgentDeck agent",
    note: "Local; no key needed",
    draft: { name: "Ollama (agent)", kind: "openai", baseUrl: "http://localhost:11434/v1", model: "qwen3-coder", smallModel: "" },
  },
  {
    label: "LM Studio, AgentDeck agent",
    note: "Local server on port 1234",
    draft: { name: "LM Studio", kind: "openai", baseUrl: "http://localhost:1234/v1", model: "local-model", smallModel: "" },
  },
  {
    label: "OpenRouter, AgentDeck agent",
    note: "Many models behind one key",
    draft: { name: "OpenRouter", kind: "openai", baseUrl: "https://openrouter.ai/api/v1", model: "deepseek/deepseek-v4-pro", smallModel: "" },
  },
];

const field =
  "h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";

const blank = (): Draft => ({ id: "", name: "", kind: "openai", baseUrl: "", model: "", smallModel: "", key: "" });

function Editor({ initial, hasKey, onDone }: { initial: Draft; hasKey: boolean; onDone: () => void }) {
  const loadProfiles = useStore((s) => s.loadProfiles);
  const newSession = useStore((s) => s.newSession);
  const [d, setD] = useState<Draft>(initial);
  const [keyTouched, setKeyTouched] = useState(false);
  const [busy, setBusy] = useState<"save" | "test" | "delete" | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const set = (k: keyof Draft) => (ev: { target: { value: string } }) => setD((x) => ({ ...x, [k]: ev.target.value }));
  const local = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])/.test(d.baseUrl);

  const save = async (): Promise<ModelProfile | null> => {
    setBusy("save");
    setMsg(null);
    try {
      const saved = await liveApi.saveProfile({ ...d, createdAt: 0 }, keyTouched ? d.key : undefined);
      setD((x) => ({ ...x, id: saved.id, key: "" }));
      setKeyTouched(false);
      await loadProfiles();
      setMsg({ ok: true, text: t("Saved.") });
      return saved;
    } catch (e) {
      setMsg({ ok: false, text: errorText(e) });
      return null;
    } finally {
      setBusy(null);
    }
  };

  const test = async () => {
    const saved = await save();
    if (!saved) return;
    setBusy("test");
    try {
      setMsg({ ok: true, text: await liveApi.testProfile(saved.id) });
    } catch (e) {
      setMsg({ ok: false, text: errorText(e) });
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    setBusy("delete");
    try {
      await liveApi.deleteProfile(d.id);
      await loadProfiles();
      onDone();
    } catch (e) {
      setMsg({ ok: false, text: errorText(e) });
      setBusy(null);
    }
  };

  return (
    <form
      className="space-y-3 rounded-xl border border-line-soft bg-surface/50 p-4"
      onSubmit={(ev) => {
        ev.preventDefault();
        void save();
      }}
    >
      <div className="grid grid-cols-2 gap-3">
        <label className="block text-[12px] text-fg-2">
          {t("Name")}
          <input value={d.name} onChange={set("name")} placeholder="DeepSeek" className={cx(field, "mt-1")} />
        </label>
        <label className="block text-[12px] text-fg-2">
          {t("Runs on")}
          <select value={d.kind} onChange={set("kind")} className={cx(field, "mt-1")}>
            <option value="anthropic">{t("Claude Code (Anthropic-compatible API)")}</option>
            <option value="openai">{t("AgentDeck agent (OpenAI-compatible API)")}</option>
          </select>
        </label>
      </div>
      <label className="block text-[12px] text-fg-2">
        {t("Address")}
        <input
          value={d.baseUrl}
          onChange={set("baseUrl")}
          placeholder={d.kind === "anthropic" ? "https://api.deepseek.com/anthropic" : "https://api.deepseek.com"}
          className={cx(field, "mt-1 font-mono text-[12.5px]")}
        />
      </label>
      <div className="grid grid-cols-2 gap-3">
        <label className="block text-[12px] text-fg-2">
          {t("Model")}
          <input value={d.model} onChange={set("model")} placeholder="deepseek-v4-pro" className={cx(field, "mt-1 font-mono text-[12.5px]")} />
        </label>
        {d.kind === "anthropic" ? (
          <label className="block text-[12px] text-fg-2">
            {t("Fast model (optional)")}
            <input value={d.smallModel} onChange={set("smallModel")} placeholder={t("Same as model")} className={cx(field, "mt-1 font-mono text-[12.5px]")} />
          </label>
        ) : (
          <div />
        )}
      </div>
      <label className="block text-[12px] text-fg-2">
        {t("API key")}
        <div className="mt-1 flex items-center gap-2">
          <input
            type="password"
            autoComplete="off"
            value={d.key}
            onChange={(ev) => {
              setKeyTouched(true);
              setD((x) => ({ ...x, key: ev.target.value }));
            }}
            placeholder={hasKey && !keyTouched ? t("Saved in your keychain. Type to replace it.") : local ? t("Not needed for a local server") : t("Paste the key")}
            className={cx(field, "font-mono text-[12.5px]")}
          />
          {hasKey && !keyTouched && (
            <button
              type="button"
              onClick={() => {
                setKeyTouched(true);
                setD((x) => ({ ...x, key: "" }));
              }}
              className="h-9 shrink-0 rounded-lg border border-line px-2.5 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg"
            >
              {t("Remove key")}
            </button>
          )}
        </div>
        <span className="mt-1 flex items-center gap-1.5 text-[11.5px] text-fg-3">
          <KeyRound size={12} /> {t("Stored in the macOS keychain, never in AgentDeck's database or logs, and never shown again.")}
        </span>
      </label>
      {msg && (
        <p
          role="status"
          className={cx("rounded-lg px-2.5 py-1.5 text-[12.5px]", msg.ok ? "bg-success/10 text-success" : "border border-error/35 bg-error/[0.06] text-fg")}
        >
          {msg.text}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {d.id &&
          (confirmDelete ? (
            <span className="mr-auto flex items-center gap-2 text-[12.5px] text-fg-2">
              {t("Delete “{name}”? Its sessions keep their history.", { name: d.name })}
              <button
                type="button"
                onClick={() => void remove()}
                className="h-8 rounded-lg bg-error/90 px-3 text-[13px] font-medium text-white hover:brightness-110"
              >
                {t("Delete")}
              </button>
              <button type="button" onClick={() => setConfirmDelete(false)} className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">
                {t("Cancel")}
              </button>
            </span>
          ) : (
            <button
              type="button"
              onClick={() => setConfirmDelete(true)}
              className="mr-auto inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-[13px] text-fg-3 hover:bg-elevated hover:text-error"
            >
              <Trash2 size={13} /> {t("Delete")}
            </button>
          ))}
        {!d.id && <span className="mr-auto" />}
        <button type="button" onClick={onDone} className="h-8 rounded-lg px-3 text-[13px] text-fg-2 hover:bg-elevated hover:text-fg">
          {t("Close")}
        </button>
        <button
          type="button"
          disabled={busy !== null}
          onClick={() => void test()}
          className="h-8 rounded-lg border border-line px-3 text-[13px] text-fg hover:bg-elevated disabled:opacity-50"
        >
          {busy === "test" ? t("Testing…") : t("Save and test")}
        </button>
        {d.id && (
          <button
            type="button"
            onClick={() => newSession(undefined, d.kind === "anthropic" ? "claude-code" : "api", d.id)}
            className="h-8 rounded-lg border border-line px-3 text-[13px] text-fg hover:bg-elevated"
          >
            {t("New session")}
          </button>
        )}
        <button
          type="submit"
          disabled={busy !== null}
          className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          {busy === "save" ? t("Saving…") : t("Save")}
        </button>
      </div>
    </form>
  );
}

/** Settings → Models: custom endpoints for DeepSeek, Kimi, local models and anything compatible. */
export function ModelsSettings() {
  const profiles = useStore((s) => s.profiles);
  const loadProfiles = useStore((s) => s.loadProfiles);
  const [editing, setEditing] = useState<{ draft: Draft; hasKey: boolean } | null>(null);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  if (!liveApi.available) return <p className="py-3 text-[13px] text-fg-3">{t("Model profiles live in the desktop app, next to your keychain.")}</p>;

  return (
    <div className="space-y-3 pt-2">
      {profiles.length > 0 && (
        <ul className="divide-y divide-line-soft rounded-xl border border-line-soft">
          {profiles.map((p) => (
            <li key={p.id} className="flex items-center gap-3 px-3.5 py-2.5">
              <div className="min-w-0 flex-1">
                <p className="text-[13.5px] text-fg">{p.name}</p>
                <p className="truncate font-mono text-[11.5px] text-fg-3">
                  {p.model} · {p.baseUrl}
                </p>
              </div>
              <span className="shrink-0 rounded-full bg-elevated px-2 py-0.5 text-[11px] text-fg-2">
                {p.kind === "anthropic" ? "Claude Code" : t("AgentDeck agent")}
              </span>
              <span className={cx("shrink-0 text-[11px]", p.hasKey ? "text-success" : "text-fg-3")}>{p.hasKey ? t("Key saved") : t("No key")}</span>
              <button
                type="button"
                onClick={() =>
                  setEditing({
                    draft: { id: p.id, name: p.name, kind: p.kind, baseUrl: p.baseUrl, model: p.model, smallModel: p.smallModel, key: "" },
                    hasKey: Boolean(p.hasKey),
                  })
                }
                className="h-7 shrink-0 rounded-lg border border-line px-2.5 text-[12.5px] text-fg hover:bg-elevated"
              >
                {t("Edit")}
              </button>
            </li>
          ))}
        </ul>
      )}
      {editing ? (
        <Editor key={editing.draft.id || "new"} initial={editing.draft} hasKey={editing.hasKey} onDone={() => setEditing(null)} />
      ) : (
        <div className="space-y-2">
          <p className="text-[12.5px] text-fg-2">{t("Add a model:")}</p>
          <div className="flex flex-wrap gap-1.5">
            {PRESETS.map((p) => (
              <button
                key={p.label}
                type="button"
                title={t(p.note)}
                onClick={() => setEditing({ draft: { id: "", key: "", ...p.draft }, hasKey: false })}
                className="h-8 rounded-lg border border-line-soft bg-surface/60 px-2.5 text-[12.5px] text-fg-2 hover:border-line hover:text-fg"
              >
                {t(p.label)}
              </button>
            ))}
            <button
              type="button"
              onClick={() => setEditing({ draft: blank(), hasKey: false })}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-line-soft bg-surface/60 px-2.5 text-[12.5px] text-fg-2 hover:border-line hover:text-fg"
            >
              <Plus size={13} /> {t("Custom")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
