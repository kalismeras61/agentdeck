import { useEffect } from "react";
import { isMac } from "../lib/format";
import { inWorkspace, useStore, type AppView, type ContextTab } from "../state/store";

const TAB_KEYS: Record<string, ContextTab> = { "1": "files", "2": "changes", "3": "preview", "4": "activity" };
const VIEW_KEYS: Record<string, AppView> = { s: "overview", d: "review", a: "permissions", g: "gallery", e: "setup", h: "git" };

/** Ordered as the sidebar shows them, so ⌥↑/↓ moves visually. */
function orderedIds(): string[] {
  const { sessions: all, search, projects: allProjects, workspaceId } = useStore.getState();
  const projects = allProjects.filter((p) => p.workspaceId === workspaceId && !p.hidden);
  const sessions = inWorkspace(all, allProjects, workspaceId);
  const q = search.trim().toLowerCase();
  return projects.flatMap((p) =>
    sessions.filter((s) => s.projectId === p.id && (!q || s.title.toLowerCase().includes(q) || s.snippet.toLowerCase().includes(q))).map((s) => s.id),
  );
}

export function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const st = useStore.getState();
      const mod = isMac ? e.metaKey : e.ctrlKey;
      const target = e.target as HTMLElement | null;
      const typing = !!target && (target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.isContentEditable);
      // Open menus and dialogs own the keyboard.
      if (document.querySelector('[role="dialog"], [role="menu"]')) return;

      if (mod && !e.shiftKey && !e.altKey) {
        const k = e.key.toLowerCase();
        if (k === "n") st.newSession();
        else if (k === "k") st.focusSearch();
        else if (k === "b") st.toggleSidebar();
        else if (k === "." && st.view === "workspace") st.toggleContext();
        else if (k === "l") st.focusComposer();
        else if (k === ",") st.setView("settings");
        else if (TAB_KEYS[k] && st.view === "workspace") st.setContextTab(TAB_KEYS[k]);
        else return;
        e.preventDefault();
        return;
      }
      if (mod && e.shiftKey && !e.altKey) {
        const k = e.key.toLowerCase();
        if (k === "r") st.setViewMode("rich");
        else if (k === "t") st.setViewMode("terminal");
        else if (VIEW_KEYS[k]) st.setView(VIEW_KEYS[k]);
        else return;
        e.preventDefault();
        return;
      }
      // A menu or popover closed by this Escape (Radix marks it handled) keeps the view open.
      if (e.key === "Escape" && !typing && st.view !== "workspace" && !e.defaultPrevented) {
        st.setView("workspace");
        e.preventDefault();
        return;
      }
      if (e.altKey && !mod && !typing && st.view === "workspace" && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        const ids = orderedIds();
        const i = ids.indexOf(st.selectedId ?? "");
        const next = ids[e.key === "ArrowDown" ? Math.min(ids.length - 1, i + 1) : Math.max(0, i - 1)];
        if (next && next !== st.selectedId) st.select(next);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
