import { useEffect } from "react";
import { create } from "zustand";
import { liveApi, type ModelChoice, type ModelList } from "./live";

type ModelsState = {
  lists: Record<string, ModelList>;
  loading: Record<string, boolean>;
  load: (provider: string, refresh?: boolean) => Promise<void>;
};

/** Model lists per agent, asked for once per run (asking can take a few seconds). */
export const useModelLists = create<ModelsState>((set, get) => ({
  lists: {},
  loading: {},
  load: async (provider, refresh = false) => {
    if (get().loading[provider] || (!refresh && get().lists[provider])) return;
    set((s) => ({ loading: { ...s.loading, [provider]: true } }));
    try {
      const list = await liveApi.listModels(provider, refresh);
      set((s) => ({ lists: { ...s.lists, [provider]: list } }));
    } catch (e: unknown) {
      const error = e instanceof Error ? e.message : String(e);
      set((s) => ({ lists: { ...s.lists, [provider]: s.lists[provider] ?? { provider, models: [], source: "builtin", error } } }));
    } finally {
      set((s) => ({ loading: { ...s.loading, [provider]: false } }));
    }
  },
}));

/** The models for an agent; starts asking the first time it's needed. */
export function useModels(provider: string | undefined, enabled = true) {
  const list = useModelLists((s) => (provider ? s.lists[provider] : undefined));
  const loading = useModelLists((s) => (provider ? Boolean(s.loading[provider]) : false));
  const load = useModelLists((s) => s.load);
  useEffect(() => {
    if (enabled && provider && liveApi.available) void load(provider);
  }, [enabled, provider, load]);
  return { list, loading, refresh: () => provider && load(provider, true) };
}

/** A short name for a model id: the agent's own label when it has one. */
export function modelName(models: ModelChoice[] | undefined, id: string): string {
  if (!id || id === "default") return "Default model";
  const m = models?.find((x) => x.id === id);
  return m ? m.label.replace(/\s*\(recommended\)$/i, "") : id;
}
