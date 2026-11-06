import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Bot as BotIcon, Check, ChevronDown, FolderGit2, FolderPlus, House, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { liveProviders, providerLabel } from "../../data/demo";
import { BOT_COLORS, BOT_MARK, canDelegate, COLOR_NAMES, nameTaken, workspaceFolder } from "../../lib/bots";
import { t } from "../../lib/i18n";
import { connectionsApi, errorText, liveApi, pickFolder, type BotColor, type LiveProvider } from "../../lib/live";
import { TEMPLATES } from "../../lib/connections";
import { useConnections } from "../../views/ConnectionsSettings";
import { detectProviders } from "../../lib/native";
import { withoutHidden } from "../../lib/setup";
import { useStore } from "../../state/store";
import { ModelMenu } from "../ModelMenu";
import { AgentMark, cx } from "../ui";

const field =
  "mt-1 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]";
const item = "flex items-center gap-2 rounded-lg px-2 py-1.5 text-[13px] outline-none data-[highlighted]:bg-surface";
const trigger =
  "mt-1 flex h-9 w-full items-center gap-2 rounded-lg border border-line bg-surface px-2.5 text-start text-[13px] text-fg hover:bg-elevated data-[state=open]:bg-elevated";

/** Agents installed on this Mac (all of them in the web preview, which can't look). */
function useInstalled(): LiveProvider[] | null {
  const [found, setFound] = useState<LiveProvider[] | null>(null);
  useEffect(() => {
    let gone = false;
    detectProviders()
      .then((probes) => {
        if (gone) return;
        if (!probes) setFound(liveProviders as LiveProvider[]);
        else setFound(probes.filter((p) => p.path && !p.error).map((p) => p.id as LiveProvider));
      })
      .catch(() => !gone && setFound(liveProviders as LiveProvider[]));
    return () => {
      gone = true;
    };
  }, []);
  return found;
}

/** Make or change a bot: name, job, instructions, agent, model and effort, default project, color. */
export function BotEditor() {
  const editor = useStore((s) => s.botEditor);
  const close = useStore((s) => s.closeBotEditor);
  return (
    <Dialog.Root open={Boolean(editor)} onOpenChange={(v) => !v && close()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="scroll-quiet fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-48px)] w-[min(520px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          {editor && <EditorForm key={editor.botId ?? "new"} botId={editor.botId} projectId={editor.projectId} onDone={close} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function EditorForm({ botId, projectId, onDone }: { botId: string | null; projectId: string | null; onDone: () => void }) {
  const bots = useStore((s) => s.bots);
  const bot = botId ? bots[botId] : undefined;
  const projects = useStore((s) => s.projects);
  const workspaceId = useStore((s) => s.workspaceId);
  const profiles = useStore((s) => s.profiles);
  const agyEnabled = useStore((s) => s.agyEnabled);
  const defaults = {
    provider: useStore((s) => s.defaultProvider),
    models: useStore((s) => s.defaultModels),
    efforts: useStore((s) => s.defaultEfforts),
  };
  const saveBot = useStore((s) => s.saveBot);
  const installed = useInstalled();
  const folders = projects.filter((p) => p.workspaceId === workspaceId && p.kind !== "chats" && !p.hidden);
  const used = new Set(Object.values(bots).map((b) => b.color));

  const [name, setName] = useState(bot?.name ?? "");
  const [role, setRole] = useState(bot?.role ?? "");
  const [instructions, setInstructions] = useState(bot?.instructions ?? "");
  const [provider, setProvider] = useState<LiveProvider>(bot?.provider ?? (defaults.provider as LiveProvider));
  const [profileId, setProfileId] = useState<string | null>(bot?.profileId ?? null);
  const [model, setModel] = useState(bot?.model ?? defaults.models[defaults.provider] ?? "default");
  const [effort, setEffort] = useState<string | null>(bot ? bot.effort : (defaults.efforts[defaults.provider] ?? null));
  // The default project: where tasks work unless they say otherwise ("": its own workspace).
  const [project, setProject] = useState<string>(bot ? (bot.projectId ?? "") : (projectId ?? ""));
  const [color, setColor] = useState<BotColor>(bot?.color ?? BOT_COLORS.find((c) => !used.has(c)) ?? "violet");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { list: connections } = useConnections();
  const [conns, setConns] = useState<string[] | null>(bot ? null : []);
  const [template, setTemplate] = useState<string | null>(null);
  const picked = TEMPLATES.find((x) => x.id === template);
  useEffect(() => {
    if (!bot || !liveApi.available) {
      setConns([]);
      return;
    }
    connectionsApi.forBot(bot.id).then(setConns, () => setConns([]));
  }, [bot?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const agents = useMemo(() => withoutHidden((installed ?? []) as LiveProvider[], agyEnabled).filter((p) => p !== "api"), [installed, agyEnabled]);
  const profile = profileId ? profiles.find((m) => m.id === profileId) : undefined;
  const agentName = profile ? profile.name : providerLabel[provider];
  const folder = projects.find((p) => p.id === project);
  const clean = name.trim();
  const taken = clean.length > 0 && nameTaken(Object.values(bots), clean, botId ?? undefined);
  const movesSession = Boolean(bot?.places.length) && (bot!.provider !== provider || (bot!.profileId ?? null) !== profileId);
  const renames = Boolean(bot) && clean.length > 0 && workspaceFolder(bot!.name) !== workspaceFolder(clean);
  const wsPath = `~/AgentDeck/Bots/${workspaceFolder(clean || t("New bot"))}`;

  const pickAgent = (p: LiveProvider, pid: string | null) => {
    setProvider(p);
    setProfileId(pid);
    setModel(pid ? "default" : (defaults.models[p] ?? "default"));
    setEffort(pid ? null : (defaults.efforts[p] ?? null));
    setError(null);
  };

  const addFolder = async () => {
    try {
      const path = await pickFolder();
      if (!path) return;
      const p = await liveApi.addProject(path);
      useStore.setState((s) => ({
        projects: s.projects.some((x) => x.id === p.id) ? s.projects : [{ id: p.id, name: p.name, path: p.path, workspaceId: "local", kind: "folder" }, ...s.projects],
      }));
      setProject(p.id);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <>
      <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
        <BotIcon size={16} className="text-violet-text" /> {bot ? t("Edit {name}", { name: bot.name }) : t("New bot")}
      </Dialog.Title>
      <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
        {t("A bot is an agent that stays: give it a job, then hand it tasks. It works through them one at a time, in its own workspace or in a project you pick.")}
      </Dialog.Description>
      <form
        className="mt-4 space-y-3.5"
        onSubmit={async (e) => {
          e.preventDefault();
          if (!clean || taken) return;
          setBusy(true);
          const err = await saveBot(
            {
            ...(bot ? { id: bot.id } : {}),
            name: clean,
            role: role.trim(),
            instructions,
            provider,
            model,
            effort: profileId ? null : effort,
            profileId,
            projectId: project || null,
            color,
            },
            conns ?? undefined,
          );
          setBusy(false);
          if (err) setError(err);
          else onDone();
        }}
      >
        {!bot && (
          <div className="text-[12px] text-fg-2">
            <p className="flex items-center gap-1.5">
              <Sparkles size={12} className="text-violet-text" /> {t("Start from")}
            </p>
            <div className="mt-1 flex flex-wrap gap-1.5">
              {TEMPLATES.map((x) => (
                <button
                  key={x.id}
                  type="button"
                  aria-pressed={template === x.id}
                  onClick={() => {
                    setTemplate(x.id);
                    setName(t(x.name));
                    setRole(t(x.role));
                    setInstructions(t(x.instructions));
                    setConns((connections ?? []).filter((c) => x.connections.includes(c.kind)).map((c) => c.id));
                    setError(null);
                  }}
                  className={cx(
                    "h-7 rounded-lg border px-2.5 text-[12.5px]",
                    template === x.id ? "border-[color-mix(in_srgb,var(--violet)_55%,var(--border))] bg-violet-soft text-violet-text" : "border-line text-fg-2 hover:bg-elevated hover:text-fg",
                  )}
                >
                  {t(x.name)}
                </button>
              ))}
            </div>
          </div>
        )}
        <div className="flex gap-3">
          <label className="block flex-1 text-[12px] text-fg-2">
            {t("Name")}
            <input
              autoFocus
              value={name}
              maxLength={40}
              onChange={(e) => {
                setName(e.target.value);
                setError(null);
              }}
              placeholder={t("PR Reviewer")}
              className={cx(field, "h-9")}
            />
          </label>
          <fieldset className="text-[12px] text-fg-2">
            <legend>{t("Color")}</legend>
            <div role="radiogroup" className="mt-1 flex h-9 items-center gap-1.5">
              {BOT_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  role="radio"
                  aria-checked={color === c}
                  aria-label={t(COLOR_NAMES[c])}
                  onClick={() => setColor(c)}
                  className={cx(
                    "h-5 w-5 rounded-full outline-none ring-offset-2 ring-offset-bg focus-visible:ring-2 focus-visible:ring-violet",
                    BOT_MARK[c],
                    color === c && "ring-2 ring-fg-3",
                  )}
                >
                  <span className="block h-full w-full rounded-full bg-current opacity-60" />
                </button>
              ))}
            </div>
          </fieldset>
        </div>
        {taken && <p className="-mt-2 text-[12px] text-error">{t("There's already a bot with this name.")}</p>}
        <label className="block text-[12px] text-fg-2">
          {t("Job")}
          <input
            value={role}
            maxLength={200}
            onChange={(e) => setRole(e.target.value)}
            placeholder={t("Reviews pull requests in the shop repo")}
            className={cx(field, "h-9")}
          />
        </label>
        <label className="block text-[12px] text-fg-2">
          {t("Instructions")} <span className="text-fg-3">{t("(optional)")}</span>
          <textarea
            value={instructions}
            maxLength={20000}
            rows={4}
            onChange={(e) => setInstructions(e.target.value)}
            placeholder={t("How it should work: what to check, what to leave alone, how to report.")}
            className={cx(field, "resize-y py-2 leading-[1.55]")}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
          <div className="text-[12px] text-fg-2">
            {t("Agent")}
            <Dropdown.Root>
              <Dropdown.Trigger className={trigger}>
                <AgentMark provider={provider} size={16} />
                <span className="truncate">{agentName}</span>
                <ChevronDown size={13} className="ms-auto shrink-0 text-fg-3" />
              </Dropdown.Trigger>
              <Dropdown.Portal>
                <Dropdown.Content align="start" sideOffset={6} className="z-50 min-w-[220px] rounded-xl border border-line bg-elevated p-1 shadow-pop">
                  <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Agents on this Mac")}</Dropdown.Label>
                  {installed === null && <p className="px-2 py-1.5 text-[12.5px] text-fg-3">{t("Looking…")}</p>}
                  {installed !== null && agents.length === 0 && <p className="px-2 py-1.5 text-[12.5px] text-fg-3">{t("No agent installed yet.")}</p>}
                  {agents.map((p) => (
                    <Dropdown.Item key={p} onSelect={() => pickAgent(p, null)} className={item}>
                      <AgentMark provider={p} size={16} /> {providerLabel[p]}
                      {provider === p && !profileId && <Check size={14} className="ms-auto text-violet-text" />}
                    </Dropdown.Item>
                  ))}
                  {profiles.length > 0 && (
                    <>
                      <Dropdown.Separator className="my-1 h-px bg-line" />
                      <Dropdown.Label className="px-2 pb-1 pt-1.5 text-[11px] text-fg-3">{t("Your models")}</Dropdown.Label>
                      {profiles.map((m) => (
                        <Dropdown.Item key={m.id} onSelect={() => pickAgent(m.kind === "anthropic" ? "claude-code" : "api", m.id)} className={item}>
                          <AgentMark provider={m.kind === "anthropic" ? "claude-code" : "api"} size={16} />
                          <span className="truncate">{m.name}</span>
                          {profileId === m.id && <Check size={14} className="ms-auto text-violet-text" />}
                        </Dropdown.Item>
                      ))}
                    </>
                  )}
                </Dropdown.Content>
              </Dropdown.Portal>
            </Dropdown.Root>
          </div>
          <div className="text-[12px] text-fg-2">
            {t("Model")}
            <div className="mt-1 flex h-9 items-center rounded-lg border border-line bg-surface px-0.5">
              {profile ? (
                <span className="truncate px-2 font-mono text-[11.5px] text-fg-2">{profile.model}</span>
              ) : (
                <ModelMenu
                  provider={provider}
                  agent={agentName}
                  value={model}
                  onChange={setModel}
                  side="bottom"
                  tip={t("Model for this bot")}
                  note={t("Used from the bot's next task.")}
                  {...(provider === "claude-code" || provider === "codex" ? { effort, onEffortChange: setEffort } : {})}
                />
              )}
            </div>
          </div>
        </div>
        {!canDelegate(provider) && (
          <p className="-mt-1.5 text-[11.5px] leading-[1.5] text-fg-3">
            {t("{agent} can't hand work to other bots. Other bots can still hand work to it.", { agent: agentName })}
          </p>
        )}
        <div className="text-[12px] text-fg-2">
          {t("Default project")} <span className="text-fg-3">{t("(optional)")}</span>
          <Dropdown.Root>
            <Dropdown.Trigger className={trigger}>
              {folder ? <FolderGit2 size={14} className="shrink-0 text-fg-3" /> : <House size={14} className="shrink-0 text-fg-3" />}
              {folder ? <span className="truncate">{folder.name}</span> : <span className="truncate">{t("None: its workspace")}</span>}
              <span dir="ltr" className="ms-2 min-w-0 truncate font-mono text-[11px] text-fg-3">
                {folder ? folder.path : wsPath}
              </span>
              <ChevronDown size={13} className="ms-auto shrink-0 text-fg-3" />
            </Dropdown.Trigger>
            <Dropdown.Portal>
              <Dropdown.Content align="start" sideOffset={6} className="z-50 max-h-[320px] w-[var(--radix-dropdown-menu-trigger-width)] overflow-y-auto rounded-xl border border-line bg-elevated p-1 shadow-pop">
                <Dropdown.Item onSelect={() => setProject("")} className={item}>
                  <House size={14} className="shrink-0 text-fg-3" /> <span className="truncate">{t("None: its workspace")}</span>
                  {project === "" && <Check size={14} className="ms-auto text-violet-text" />}
                </Dropdown.Item>
                {folders.length > 0 && <Dropdown.Separator className="my-1 h-px bg-line" />}
                {folders.map((p) => (
                  <Dropdown.Item key={p.id} onSelect={() => setProject(p.id)} className={item}>
                    <FolderGit2 size={14} className="shrink-0 text-fg-3" /> <span className="truncate">{p.name}</span>
                    {project === p.id && <Check size={14} className="ms-auto text-violet-text" />}
                  </Dropdown.Item>
                ))}
                {liveApi.available && (
                  <>
                    {folders.length > 0 && <Dropdown.Separator className="my-1 h-px bg-line" />}
                    <Dropdown.Item onSelect={() => void addFolder()} className={item}>
                      <FolderPlus size={14} className="text-fg-3" /> {t("Add folder…")}
                    </Dropdown.Item>
                  </>
                )}
              </Dropdown.Content>
            </Dropdown.Portal>
          </Dropdown.Root>
          <p className="mt-1 text-[11.5px] leading-[1.5] text-fg-3">
            {t("Tasks work here unless you pick another place for them. Its own workspace, {path}, is where it keeps its memory file and the files it makes.", { path: wsPath })}
          </p>
        </div>
        <fieldset className="text-[12px] text-fg-2">
          <legend>{t("Connections")}</legend>
          {connections && connections.length > 0 ? (
            <div className="mt-1 space-y-1">
              {connections.map((c) => {
                const on = (conns ?? []).includes(c.id);
                return (
                  <label key={c.id} className="flex cursor-pointer items-center gap-2 text-[13px] text-fg">
                    <input
                      type="checkbox"
                      checked={on}
                      disabled={conns === null}
                      onChange={() => setConns((x) => (on ? (x ?? []).filter((i) => i !== c.id) : [...(x ?? []), c.id]))}
                      className="accent-[var(--violet)]"
                    />
                    <span className="truncate">{c.name}</span>
                    <span className="truncate text-[11.5px] text-fg-3">{c.actAlone ? t("acts on its own") : t("changes wait for you")}</span>
                  </label>
                );
              })}
              {picked && picked.connections.some((k) => !connections.some((c) => c.kind === k)) && (
                <p className="text-[11.5px] leading-[1.5] text-fg-3">{t("This kind of bot works best with a connection you haven't added yet (Settings → Connections).")}</p>
              )}
            </div>
          ) : (
            <p className="mt-1 text-[11.5px] leading-[1.5] text-fg-3">
              {t("None yet. In Settings → Connections you can add a browser, GitHub, Notion, your Mac's Calendar, Reminders, Mail and Notes, or any MCP server.")}
            </p>
          )}
          {bot && <p className="mt-1 text-[11.5px] text-fg-3">{t("A change applies from the bot's next task.")}</p>}
        </fieldset>
        {picked && <p className="rounded-lg bg-surface px-2.5 py-2 text-[12px] leading-[1.5] text-fg-2">{t("Next: {step}", { step: t(picked.next) })}</p>}
        {renames && <p className="text-[12px] text-fg-2">{t("Its workspace folder is renamed to match.")}</p>}
        {movesSession && <p className="text-[12px] text-fg-2">{t("A new agent means new sessions: the next task in each place starts one. The old sessions stay.")}</p>}
        {error && (
          <p role="alert" className="text-[12.5px] text-error">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-1">
          <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
          <button
            type="submit"
            disabled={busy || !clean || taken}
            className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
          >
            {busy ? t("Saving…") : bot ? t("Save") : t("Create bot")}
          </button>
        </div>
      </form>
    </>
  );
}
