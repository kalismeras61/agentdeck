import { useMemo } from "react";
import type { Project, Session } from "../data/demo";
import { inWorkspace, useStore } from "../state/store";

export function useWorkspaceProjects(): Project[] {
  const projects = useStore((s) => s.projects);
  const ws = useStore((s) => s.workspaceId);
  return useMemo(() => projects.filter((p) => p.workspaceId === ws), [projects, ws]);
}

export function useWorkspaceSessions(): Session[] {
  const sessions = useStore((s) => s.sessions);
  const projects = useStore((s) => s.projects);
  const ws = useStore((s) => s.workspaceId);
  return useMemo(() => inWorkspace(sessions, projects, ws), [sessions, projects, ws]);
}

export function useProject(projectId: string | undefined): Project | undefined {
  const projects = useStore((s) => s.projects);
  return useMemo(() => projects.find((p) => p.id === projectId), [projects, projectId]);
}
