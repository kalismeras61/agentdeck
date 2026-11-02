import * as Dialog from "@radix-ui/react-dialog";
import { FolderPlus } from "lucide-react";
import { useState } from "react";
import { t } from "../lib/i18n";
import { useStore } from "../state/store";

/** New project: AgentDeck makes ~/AgentDeck/<name>, or you point it at a folder you already have. */
export function NewProjectDialog() {
  const open = useStore((s) => s.newProjectOpen);
  const setOpen = useStore((s) => s.setNewProjectOpen);
  const create = useStore((s) => s.createProject);
  const addFolder = useStore((s) => s.addProjectFolder);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const clean = name.trim();

  const close = (v: boolean) => {
    setOpen(v);
    if (!v) {
      setName("");
      setError(null);
    }
  };

  return (
    <Dialog.Root open={open} onOpenChange={close}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 w-[min(440px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          <Dialog.Title className="flex items-center gap-2 text-[15px] font-semibold">
            <FolderPlus size={16} className="text-violet-text" /> {t("New project")}
          </Dialog.Title>
          <Dialog.Description className="mt-1.5 text-[13px] leading-[1.6] text-fg-2">
            {t("AgentDeck makes a folder for it in your home folder and starts a session there.")}
          </Dialog.Description>
          <form
            className="mt-4 space-y-3"
            onSubmit={async (e) => {
              e.preventDefault();
              setBusy(true);
              const err = await create(clean);
              setBusy(false);
              if (err) setError(err);
              else close(false);
            }}
          >
            <label className="block text-[12px] text-fg-2">
              {t("Name")}
              <input
                autoFocus
                value={name}
                maxLength={80}
                onChange={(e) => {
                  setName(e.target.value);
                  setError(null);
                }}
                placeholder="weather-app"
                className="mt-1 h-9 w-full rounded-lg border border-line bg-surface px-2.5 text-[13px] text-fg outline-none focus-visible:border-[color-mix(in_srgb,var(--violet)_60%,var(--border))] focus-visible:shadow-[0_0_0_3px_var(--violet-soft)]"
              />
            </label>
            <p className="font-mono text-[12px] text-fg-3">~/AgentDeck/{clean || "…"}</p>
            {error && (
              <p role="alert" className="text-[12.5px] text-error">
                {error}
              </p>
            )}
            <div className="flex items-center gap-2 pt-1">
              <button
                type="button"
                onClick={() => {
                  close(false);
                  void addFolder();
                }}
                className="h-8 rounded-lg px-2 text-[13px] text-fg-2 hover:bg-elevated hover:text-fg"
              >
                {t("Use a folder I have…")}
              </button>
              <span className="ms-auto flex gap-2">
                <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
                <button
                  type="submit"
                  disabled={busy || !clean}
                  className="h-8 rounded-lg bg-violet-strong px-3.5 text-[13px] font-medium text-white hover:brightness-110 disabled:opacity-50"
                >
                  {busy ? t("Creating…") : t("Create project")}
                </button>
              </span>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
