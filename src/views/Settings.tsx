import { AVAILABLE, LOCALES, systemLocale } from "../lib/i18n";
import { setupApi } from "../lib/setup";
import { UpdateSettings } from "../components/UpdateNotice";
import { Check, Copy, FolderPlus, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { AgentMark, cx, Kbd } from "../components/ui";
import { liveProviders, providerLabel } from "../data/demo";
import { ModelMenu } from "../components/ModelMenu";
import { AgyApprovalsMenu } from "../components/AgyApprovals";
import { usePermissions } from "../hooks/useDerived";
import { getParsed } from "../hooks/useSession";
import type { ProviderId } from "../lib/events";
import { errorText, liveApi } from "../lib/live";
import { appInfo, detectProviders, isTauri, type AppInfo, type ProviderProbe } from "../lib/native";
import { SHORTCUTS } from "../lib/shortcuts";
import { useStore, type MotionPref, type NotificationPrefs, type ThemePref } from "../state/store";
import { ModelsSettings } from "./ModelsSettings";
import { WorkflowsSettings } from "./WorkflowsSettings";
import { ViewHeader } from "./ViewHeader";

const SECTIONS = [
  ["appearance", "Appearance"],
  ["providers", "Providers"],
  ["models", "Models"],
  ["workflows", "Workflows"],
  ["defaults", "Workspace defaults"],
  ["notifications", "Notifications"],
  ["shortcuts", "Shortcuts"],
  ["privacy", "Data and privacy"],
  ["updates", "Updates"],
  ["diagnostics", "Diagnostics"],
] as const;

const ROUTES: Record<ProviderId, string> = {
  "claude-code":
    "Runs your installed claude CLI unmodified and reads its structured JSON stream. Sign-in stays in Claude Code's own flow; AgentDeck never sees your credentials.",
  codex:
    "Runs `codex app-server` from your installed Codex and talks JSON-RPC over stdio. Asks before commands outside its read-only set (approval policy: untrusted, sandbox: workspace-write). Codex handles its own sign-in.",
  "gemini-cli":
    "Runs `gemini --acp` from your installed Gemini CLI and talks the Agent Client Protocol over stdio. Plan mode uses Gemini's own read-only mode. Signs in with the API key below when you save one, otherwise with Gemini CLI's own sign-in.",
  "grok-build":
    "Runs `grok agent stdio` from your installed Grok Build and talks the Agent Client Protocol over stdio. Not tested here: Grok Build couldn't be installed in AgentDeck's test environment. Grok Build handles its own sign-in.",
  "kimi-code":
    "Runs `kimi acp` from your installed Kimi Code CLI and talks the Agent Client Protocol over stdio. Kimi Code handles its own sign-in (`kimi login`: a Kimi Code plan or a Moonshot API key).",
  antigravity:
    "Runs your installed Antigravity CLI (`agy`) in its documented headless mode (`--input-format stream-json --output-format stream-json`), one process per session; plan mode is agy's own. agy can't ask before acting there, so it edits files in the project without asking (review them in Changes) and skips anything else that needs approval. Sign-in stays with agy. Google's terms limit using personal Antigravity sign-ins through other tools; an API key or Workspace account is the safer choice.",
  api: "AgentDeck's own agent on an OpenAI-compatible API (DeepSeek, Kimi, OpenRouter, Ollama, LM Studio). Reads freely inside the project; every file change and command asks you first. Set it up under Models.",
};

function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex shrink-0 rounded-lg border border-line bg-bg/40 p-0.5">
      {options.map(([v, l]) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={value === v}
          onClick={() => onChange(v)}
          className={cx("h-7 rounded-md px-3 text-[12.5px]", value === v ? "bg-elevated text-fg shadow-card" : "text-fg-2 hover:text-fg")}
        >
          {l}
        </button>
      ))}
    </div>
  );
}

function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx("relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors", checked ? "bg-violet-strong" : "bg-elevated ring-1 ring-line")}
    >
      <span className={cx("absolute top-[3px] h-4 w-4 rounded-full bg-white shadow transition-[left] duration-150", checked ? "left-[19px]" : "left-[3px]")} />
    </button>
  );
}

function Row({ title, hint, children }: { title: string; hint?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-line-soft py-4 last:border-b-0">
      <div className="min-w-0">
        <p className="text-[13.5px] text-fg">{title}</p>
        {hint && <div className="mt-0.5 text-xs leading-[1.55] text-fg-3">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

function Section({ id, title, lede, children }: { id: string; title: string; lede?: string; children: ReactNode }) {
  return (
    <section id={`set-${id}`} aria-labelledby={`seth-${id}`} className="scroll-mt-6">
      <h3 id={`seth-${id}`} className="text-[15px] font-semibold">
        {title}
      </h3>
      {lede && <p className="mt-1 max-w-[68ch] text-[13px] leading-[1.6] text-fg-3">{lede}</p>}
      <div className="mt-2">{children}</div>
    </section>
  );
}

export function Settings() {
  const st = useStore();
  const { pending } = usePermissions();
  const [probes, setProbes] = useState<ProviderProbe[] | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkedAt, setCheckedAt] = useState<number | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [copied, setCopied] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const localProjects = st.projects.filter((p) => p.workspaceId === "local" && p.kind !== "chats");

  const check = useCallback(async () => {
    if (!isTauri) return;
    setChecking(true);
    try {
      setProbes(await detectProviders());
      setCheckedAt(Date.now());
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void check();
    void appInfo().then(setInfo);
  }, [check]);

  const eventStats = useMemo(() => {
    let ok = 0;
    let bad = 0;
    for (const s of st.sessions)
      for (const p of getParsed(s)) {
        if (p.ok) ok++;
        else bad++;
      }
    return { ok, bad };
  }, [st.sessions]);

  const diagnostics = {
    app: "AgentDeck",
    version: info?.version ?? "unknown",
    runtime: isTauri ? "desktop" : "web preview",
    os: info?.os ?? navigator.platform,
    arch: info?.arch ?? null,
    userAgent: navigator.userAgent,
    sessions: st.sessions.length,
    validEvents: eventStats.ok,
    rejectedEvents: eventStats.bad,
    pendingPermissions: pending.length,
    providers: probes?.map((p) => ({ id: p.id, found: Boolean(p.path), version: p.version })) ?? "not checked",
    demoData: true,
  };

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Settings" />
      <div className="flex min-h-0 flex-1">
        <nav aria-label="Settings sections" className="hidden w-[200px] shrink-0 border-r border-line-soft p-3 lg:block">
          <ul className="space-y-px">
            {SECTIONS.map(([id, label]) => (
              <li key={id}>
                <a
                  href={`#set-${id}`}
                  onClick={(e) => {
                    e.preventDefault();
                    document.getElementById(`set-${id}`)?.scrollIntoView({ block: "start", behavior: "auto" });
                  }}
                  className="flex h-8 items-center rounded-lg px-2 text-[13px] text-fg-2 hover:bg-surface hover:text-fg"
                >
                  {label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
        <div className="scroll-quiet relative min-w-0 flex-1 overflow-y-auto">
          <div className="max-w-[820px] space-y-12 px-10 pb-20 pt-8">
            <div>
              <h2 className="text-[22px] font-semibold tracking-[-0.015em]">Settings</h2>
              <p className="mt-1.5 text-[14px] text-fg-2">Preferences stay on this device. Tokens and API keys are never shown or stored by AgentDeck.</p>
            </div>

            <Section id="appearance" title="Appearance">
              {AVAILABLE.length > 1 && <Row title="Language" hint="Menus, buttons and messages from AgentDeck. Agents answer in whatever language you write to them.">
                <select
                  aria-label="Language"
                  value={st.language}
                  onChange={(e) => st.setLanguage(e.target.value as typeof st.language)}
                  className="h-8 rounded-lg border border-line bg-surface px-2 text-[13px] text-fg"
                >
                  <option value="system">System ({LOCALES.find((l) => l.id === systemLocale())?.name})</option>
                  {AVAILABLE.map((l) => (
                    <option key={l.id} value={l.id}>
                      {l.name}
                      {l.id !== "en" ? ` · ${l.english}` : ""}
                    </option>
                  ))}
                </select>
              </Row>}
              <Row title="Theme" hint="Dark is the default. System follows your Mac.">
                <Segmented<ThemePref>
                  label="Theme"
                  value={st.theme}
                  onChange={st.setTheme}
                  options={[
                    ["dark", "Dark"],
                    ["light", "Light"],
                    ["system", "System"],
                  ]}
                />
              </Row>
              <Row title="Motion" hint="Reduced makes state changes instant and stops the activity pulse.">
                <Segmented<MotionPref>
                  label="Motion"
                  value={st.motion}
                  onChange={st.setMotion}
                  options={[
                    ["system", "System"],
                    ["reduced", "Reduced"],
                    ["full", "Full"],
                  ]}
                />
              </Row>
            </Section>

            <Section
              id="providers"
              title="Providers"
              lede={
                isTauri
                  ? "AgentDeck looks for each CLI on your PATH and in common install folders, then asks it for its version. Sign-in status is reported by each CLI when a session starts."
                  : "Detection runs in the desktop app. This web preview can't look at your computer, so nothing below has been checked."
              }
            >
              {isTauri && (
                <button
                  type="button"
                  onClick={() => st.setView("welcome")}
                  className="mb-1 mt-1 inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated"
                >
                  Install or sign in to an agent…
                </button>
              )}
              {(["claude-code", "codex", "gemini-cli", "grok-build", "kimi-code", "antigravity"] as const).map((id) => {
                const p = probes?.find((x) => x.id === id);
                const status = !isTauri
                  ? "Not checked"
                  : checking && !p
                    ? "Checking…"
                    : !p?.path
                      ? "Not found"
                      : p.error
                        ? "Found, but didn't respond"
                        : "Found";
                return (
                  <div key={id} className="border-b border-line-soft py-4 last:border-b-0">
                    <div className="flex items-center gap-3">
                      <AgentMark provider={id} size={22} />
                      <p className="text-[13.5px] font-medium">{providerLabel[id]}</p>
                      <span
                        className={cx(
                          "ml-auto rounded-full px-2 py-0.5 text-xs",
                          status === "Found"
                            ? "bg-success/12 text-success"
                            : status === "Not found" || status.startsWith("Found,")
                              ? "bg-warning/12 text-warning"
                              : "bg-elevated text-fg-3",
                        )}
                      >
                        {status}
                      </span>
                    </div>
                    <dl className="mt-2.5 grid grid-cols-[96px_minmax(0,1fr)] gap-x-3 gap-y-1.5 pl-[34px] text-[12.5px]">
                      <dt className="text-fg-3">Command</dt>
                      <dd className="font-mono text-fg-2">
                        {p?.binary ?? ({ "claude-code": "claude", codex: "codex", "gemini-cli": "gemini", "grok-build": "grok", "kimi-code": "kimi", antigravity: "agy" } as const)[id]}
                      </dd>
                      <dt className="text-fg-3">Path</dt>
                      <dd className="break-all font-mono text-fg-2">{p?.path ?? "—"}</dd>
                      <dt className="text-fg-3">Version</dt>
                      <dd className="text-fg-2">{p?.version ?? p?.error ?? "—"}</dd>
                      <dt className="text-fg-3">How it connects</dt>
                      <dd className="leading-[1.55] text-fg-2">{ROUTES[id]}</dd>
                      {id === "antigravity" && (
                        <>
                          <dt className="text-fg-3">In AgentDeck</dt>
                          <dd>
                            <AntigravitySwitch />
                          </dd>
                          {st.agyEnabled && (
                            <>
                              <dt className="text-fg-3">Approvals</dt>
                              <dd>
                                <AgyApprovalsMenu side="bottom" />
                              </dd>
                            </>
                          )}
                        </>
                      )}
                      {(id === "gemini-cli" || id === "grok-build") && (
                        <>
                          <dt className="text-fg-3">API key</dt>
                          <dd>
                            <AgentKey provider={id} />
                          </dd>
                        </>
                      )}
                    </dl>
                  </div>
                );
              })}
              <div className="flex items-center gap-3 pt-4">
                <button
                  type="button"
                  disabled={!isTauri || checking}
                  onClick={() => void check()}
                  className="inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-[13px] text-fg hover:bg-elevated disabled:opacity-50"
                >
                  <RefreshCw size={14} className={cx(checking && "animate-spin motion-reduce:animate-none")} /> Check again
                </button>
                <span className="text-xs text-fg-3">
                  {!isTauri
                    ? "Available in the desktop app"
                    : checkedAt
                      ? `Last checked ${new Date(checkedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
                      : ""}
                </span>
              </div>
            </Section>

            <Section
              id="models"
              title="Models"
              lede="Run DeepSeek, Kimi, local models (Ollama, LM Studio) or any compatible endpoint the same way as the built-in agents: sessions, plans, approvals, hand-offs and relays all work. An Anthropic-compatible endpoint runs inside your Claude Code; an OpenAI-compatible one runs on AgentDeck's own agent."
            >
              <ModelsSettings />
            </Section>

            <Section
              id="workflows"
              title="Workflows"
              lede="Who plans, who builds and who reviews. Any agent on this Mac or any of your models can take any stage. The planner and the reviewer only read; the reviewer sends notes back to the builder until it writes APPROVED. Start one from a session with Work with…"
            >
              <WorkflowsSettings />
            </Section>

            <Section
              id="defaults"
              title="Workspace defaults"
              lede="Used when you start a new session. Each session can still change its model from the message box."
            >
              {liveProviders.map((p) => (
                <Row
                  key={p}
                  title={`${providerLabel[p]} model`}
                  hint={`For new ${providerLabel[p]} sessions. The list comes from ${providerLabel[p]} itself; Default uses whatever it's set to.`}
                >
                  {liveApi.available ? (
                    <ModelMenu
                      provider={p}
                      agent={providerLabel[p]}
                      value={st.defaultModels[p] ?? "default"}
                      onChange={(m) => st.setDefaultModel(p, m)}
                      tip={`Model for new ${providerLabel[p]} sessions`}
                      note="Used for new sessions. Each session can still change it from the message box."
                      side="bottom"
                    />
                  ) : (
                    <span className="text-[12.5px] text-fg-3">In the desktop app</span>
                  )}
                </Row>
              ))}
              <Row title="Default agent" hint="Used by New session and ⌘N. The arrow next to New session picks another one.">
                <Segmented<ProviderId>
                  label="Default agent"
                  value={liveProviders.includes(st.defaultProvider) ? st.defaultProvider : "claude-code"}
                  onChange={st.setDefaultProvider}
                  options={liveProviders.map((p) => [p, providerLabel[p]] as const)}
                />
              </Row>
              <div className="border-b border-line-soft py-4">
                <div className="flex items-center justify-between gap-6">
                  <div>
                    <p className="text-[13.5px] text-fg">Project folders</p>
                    <p className="mt-0.5 text-xs text-fg-3">{isTauri ? "Folders on this Mac where agents can run." : "Available in the desktop app."}</p>
                  </div>
                  <button
                    type="button"
                    disabled={!isTauri}
                    onClick={() => void st.addProjectFolder()}
                    className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated disabled:opacity-50"
                  >
                    <FolderPlus size={14} /> Add folder
                  </button>
                </div>
                {localProjects.length > 0 && (
                  <ul className="mt-3 divide-y divide-line-soft rounded-xl border border-line-soft">
                    {localProjects.map((p) => (
                      <li key={p.id} className="flex items-center gap-3 px-3 py-2.5 text-[13px]">
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-fg">{p.name}</p>
                          <p className="truncate font-mono text-[11.5px] text-fg-3">{p.path}</p>
                        </div>
                        {removing === p.id ? (
                          <span className="flex shrink-0 items-center gap-1.5">
                            <button
                              type="button"
                              onClick={() => setRemoving(null)}
                              className="h-7 rounded-md border border-line px-2 text-xs hover:bg-elevated"
                            >
                              Keep
                            </button>
                            <button
                              type="button"
                              onClick={() => {
                                st.removeProject(p.id);
                                setRemoving(null);
                              }}
                              className="h-7 rounded-md bg-error/90 px-2 text-xs font-medium text-white hover:bg-error"
                            >
                              Remove from AgentDeck
                            </button>
                          </span>
                        ) : (
                          <button
                            type="button"
                            onClick={() => setRemoving(p.id)}
                            className="h-7 shrink-0 rounded-md px-2 text-xs text-fg-2 hover:bg-elevated hover:text-fg"
                          >
                            Remove…
                          </button>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
                {removing && (
                  <p className="mt-2 text-xs text-fg-3">Removing a folder deletes its sessions from AgentDeck. Nothing in the folder itself is touched.</p>
                )}
              </div>
            </Section>

            <Section
              id="notifications"
              title="Notifications"
              lede="System notifications appear only while AgentDeck isn't the window you're using. The sidebar badges always show what needs you."
            >
              {(
                [
                  ["approval", "An agent needs approval"],
                  ["finished", "A session finishes"],
                  ["errors", "A session stops with an error"],
                ] as [keyof NotificationPrefs, string][]
              ).map(([k, label]) => (
                <Row key={k} title={label}>
                  <Switch label={label} checked={st.notifications[k]} onChange={(v) => st.setNotification(k, v)} />
                </Row>
              ))}
            </Section>

            <Section id="shortcuts" title="Shortcuts" lede="Approving a permission request has no shortcut on purpose.">
              <div className="grid gap-x-10 gap-y-6 md:grid-cols-2">
                {SHORTCUTS.map((g) => (
                  <div key={g.group}>
                    <p className="mb-1 text-xs font-medium text-fg-3">{g.group}</p>
                    <ul>
                      {g.items.map(([what, keys]) => (
                        <li key={what} className="flex items-center justify-between gap-4 border-b border-line-soft py-2 text-[13px] last:border-b-0">
                          <span className="text-fg-2">{what}</span>
                          <Kbd>{keys}</Kbd>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            </Section>

            <Section id="privacy" title="Data and privacy">
              <Row title="Preferences and viewed marks" hint="Stored in this app's local storage on this device." />
              <Row
                title="Session history"
                hint="Stored in a local SQLite database in AgentDeck's app data folder. Well-known key formats (Anthropic, OpenAI, GitHub, AWS, Slack, private keys) are masked before anything is written. Demo sessions are never stored."
              />
              <Row title="Credentials" hint="Never stored or read. Claude Code and Codex keep their own sign-in." />
              <Row title="Network" hint="AgentDeck itself sends nothing anywhere. The CLIs you run talk to their own providers." />
              <Row title="Reset preferences" hint="Restores layout, theme, defaults and notifications, and clears viewed marks.">
                {confirmReset ? (
                  <div className="flex shrink-0 gap-2">
                    <button
                      type="button"
                      onClick={() => setConfirmReset(false)}
                      className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated"
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        st.resetPreferences();
                        setConfirmReset(false);
                      }}
                      className="h-8 rounded-lg bg-error/90 px-3 text-[13px] font-medium text-white hover:bg-error"
                    >
                      Reset now
                    </button>
                  </div>
                ) : (
                  <button
                    type="button"
                    onClick={() => setConfirmReset(true)}
                    className="h-8 shrink-0 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated"
                  >
                    Reset preferences
                  </button>
                )}
              </Row>
            </Section>

            <Section id="updates" title="Updates">
              <UpdateSettings />
            </Section>

            <Section id="diagnostics" title="Diagnostics" lede="Safe to share: no paths to your files, prompts, or credentials are included.">
              <pre className="scroll-quiet overflow-x-auto rounded-xl border border-line bg-surface px-4 py-3 font-mono text-[12px] leading-[1.6] text-fg-2">
                {JSON.stringify(diagnostics, null, 2)}
              </pre>
              <button
                type="button"
                onClick={() => {
                  void navigator.clipboard
                    ?.writeText(JSON.stringify(diagnostics, null, 2))
                    .then(() => {
                      setCopied(true);
                      window.setTimeout(() => setCopied(false), 1500);
                    })
                    .catch(() => undefined);
                }}
                className="mt-3 inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated"
              >
                {copied ? <Check size={14} className="text-success" /> : <Copy size={14} />} {copied ? "Copied" : "Copy diagnostics"}
              </button>
            </Section>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Antigravity is off until you turn it on, after reading why. */
function AntigravitySwitch() {
  const enabled = useStore((s) => s.agyEnabled);
  const setEnabled = useStore((s) => s.setAgyEnabled);
  const [agreed, setAgreed] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const change = async (v: boolean) => {
    setMsg(null);
    try {
      setEnabled(await setupApi.setAntigravity(v));
    } catch (e: unknown) {
      setMsg(errorText(e));
    }
  };
  return (
    <div className="space-y-2">
      <p className="max-w-[60ch] leading-[1.55] text-fg-2">
        Google's Antigravity terms forbid using third-party software to access the service, and Google has suspended Antigravity and Gemini CLI accounts
        over it. AgentDeck only starts Google's own <code className="font-mono">agy</code> unmodified and never touches its sign-in, but Google hasn't said
        whether that's allowed. Use it at your own risk.
      </p>
      {enabled ? (
        <button type="button" onClick={() => void change(false)} className="inline-flex h-7 items-center rounded-lg border border-line px-2.5 text-[12.5px] hover:bg-elevated">
          Turn Antigravity off
        </button>
      ) : (
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-[12.5px] text-fg-2">
            <input type="checkbox" checked={agreed} onChange={(e) => setAgreed(e.target.checked)} className="accent-[var(--violet)]" />
            I understand the risk to my Google account
          </label>
          <button
            type="button"
            disabled={!agreed || !isTauri}
            onClick={() => void change(true)}
            className="inline-flex h-7 items-center rounded-lg bg-violet-strong px-2.5 text-[12.5px] font-medium text-white hover:brightness-110 disabled:opacity-45"
          >
            Turn Antigravity on
          </button>
        </div>
      )}
      {msg && <p className="text-error">{msg}</p>}
    </div>
  );
}

const KEY_HINT: Record<string, string> = {
  "gemini-cli":
    "Optional. Gemini CLI signs in here the same way it does in your Terminal. Add a key only if that sign-in stops working: one from aistudio.google.com/apikey, kept in your Keychain and used only when AgentDeck starts Gemini CLI.",
  "grok-build": "Optional: an xAI API key from console.x.ai, used instead of Grok's own sign-in. Kept in your Keychain.",
};

/** Saves an agent's API key in the Keychain; the key itself is never shown again. */
function AgentKey({ provider }: { provider: "gemini-cli" | "grok-build" }) {
  const [saved, setSaved] = useState<boolean | null>(null);
  const [value, setValue] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!isTauri) return;
    liveApi
      .agentKeys()
      .then((k) => setSaved(Boolean(k[provider])))
      .catch(() => setSaved(false));
  }, [provider]);
  const save = async (key: string) => {
    setBusy(true);
    setMsg(null);
    try {
      const has = await liveApi.setAgentKey(provider, key);
      setSaved(has);
      setValue("");
      setMsg({ ok: true, text: has ? "Saved in your Keychain. The next message starts the agent with it." : "Removed." });
    } catch (e: unknown) {
      setMsg({ ok: false, text: errorText(e) });
    }
    setBusy(false);
  };
  return (
    <div className="space-y-1.5">
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void save(value);
        }}
      >
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          disabled={!isTauri}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={saved ? "Saved ••••••••  (type to replace)" : "Paste the key"}
          aria-label={`${provider === "gemini-cli" ? "Gemini" : "xAI"} API key`}
          className="h-8 w-[260px] rounded-lg border border-line bg-surface px-2.5 font-mono text-[12px] text-fg outline-none focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))]"
        />
        <button
          type="submit"
          disabled={busy || !value.trim()}
          className="h-8 rounded-lg border border-line px-3 text-[12.5px] text-fg hover:bg-elevated disabled:opacity-50"
        >
          Save
        </button>
        {saved && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void save("")}
            className="h-8 rounded-lg px-2 text-[12.5px] text-fg-2 hover:bg-elevated hover:text-fg"
          >
            Remove
          </button>
        )}
      </form>
      {msg && <p className={cx("text-[12px]", msg.ok ? "text-success" : "text-error")}>{msg.text}</p>}
      <p className="max-w-[60ch] text-[12px] leading-[1.5] text-fg-3">{KEY_HINT[provider]}</p>
    </div>
  );
}
