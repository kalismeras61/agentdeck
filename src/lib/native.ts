import type { ProviderId } from "./events";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface ProviderProbe {
  id: ProviderId;
  binary: string;
  path: string | null;
  version: string | null;
  error: string | null;
}

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
}

async function call<T>(cmd: string): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd);
}

/** Desktop only. Returns null in the web preview, which can't look at the computer. */
export async function detectProviders(): Promise<ProviderProbe[] | null> {
  return isTauri ? call<ProviderProbe[]>("detect_providers") : null;
}

export async function appInfo(): Promise<AppInfo | null> {
  return isTauri ? call<AppInfo>("app_info") : null;
}
