import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import type { Project } from "../data/demo";
import { t, tn } from "../lib/i18n";
import { errorText } from "../lib/live";
import { useStore } from "../state/store";

/** Asks before a project leaves AgentDeck: its sessions go, the folder stays as it is. */
export function RemoveProjectDialog({ project, onClose }: { project: Project; onClose: () => void }) {
  const removeProject = useStore((s) => s.removeProject);
  const sessions = useStore((s) => s.sessions.filter((x) => x.projectId === project.id).length);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const remove = () => {
    setBusy(true);
    setError(null);
    removeProject(project.id).then(onClose, (e: unknown) => {
      setBusy(false);
      setError(errorText(e));
    });
  };
  return (
    <Dialog.Root open onOpenChange={(v) => !v && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/55" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 w-[min(460px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-bg p-5 shadow-pop focus:outline-none">
          <Dialog.Title className="text-[15px] font-semibold">{t("Remove {name} from AgentDeck?", { name: project.name })}</Dialog.Title>
          <Dialog.Description className="mt-2 text-[13px] leading-[1.55] text-fg-2">
            {sessions > 0
              ? tn(sessions, "Its {n} session is deleted from AgentDeck, and a running agent there is stopped.", "Its {n} sessions are deleted from AgentDeck, and running agents there are stopped.")
              : t("It has no sessions.")}{" "}
            {t("Nothing in the folder itself is touched, and you can add it again any time.")}
          </Dialog.Description>
          <p dir="ltr" className="mt-2 truncate rounded-lg bg-surface px-2.5 py-1.5 font-mono text-[11.5px] text-fg-3">
            {project.path}
          </p>
          {error && <p className="mt-3 text-[12.5px] text-error">{error}</p>}
          <div className="mt-5 flex justify-end gap-2">
            <Dialog.Close className="h-8 rounded-lg border border-line px-3 text-[13px] hover:bg-elevated">{t("Cancel")}</Dialog.Close>
            <button
              type="button"
              disabled={busy}
              onClick={remove}
              className="h-8 rounded-lg bg-error/90 px-3 text-[13px] font-medium text-white hover:bg-error disabled:opacity-60"
            >
              {t("Remove from AgentDeck")}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
