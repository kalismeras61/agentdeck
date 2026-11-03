import { useCallback } from "react";
import { t, useLocale } from "../lib/i18n";
import { useStore } from "../state/store";

/** "Workspace" or the project's name. */
export function usePlaceName(): (projectId: string | null) => string {
  const projects = useStore((s) => s.projects);
  useLocale();
  return useCallback(
    (projectId: string | null) => (projectId ? (projects.find((p) => p.id === projectId)?.name ?? t("A removed project")) : t("Workspace")),
    [projects],
  );
}

