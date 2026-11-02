import { useEffect, useState } from "react";
import { isTauri } from "./native";
import { t } from "./i18n";

type Json = Record<string, unknown>;

/** An image block in an agent's tool result (Claude Code's Read on a picture, MCP screenshots). */
export function imageFromBlock(b: Json): { src: string } | null {
  if (b.type !== "image") return null;
  const source = (b.source ?? {}) as Json;
  const mime = typeof source.media_type === "string" ? source.media_type : typeof b.mimeType === "string" ? b.mimeType : "";
  const data = typeof source.data === "string" ? source.data : typeof b.data === "string" ? b.data : "";
  if (!/^image\/(png|jpe?g|gif|webp)$/.test(mime) || !data || data.length > 19_000_000) return null;
  return { src: `data:${mime};base64,${data}` };
}

const SHOT = /\b(screenshot|screencap(ture)?|simctl\s+io\b|capture)/i;

/** The file a screenshot command writes to (screencapture, simctl io … screenshot, adb screencap,
 * playwright screenshot…), resolved against the folder it ran in. */
export function screenshotPath(cmd: string, cwd: string): string | null {
  if (!SHOT.test(cmd)) return null;
  const files = cmd.match(/(?:"[^"]+\.(?:png|jpe?g|webp)"|'[^']+\.(?:png|jpe?g|webp)'|[^\s"'>|;&]+\.(?:png|jpe?g|webp))/gi);
  const last = files?.[files.length - 1]?.replace(/^["']|["']$/g, "");
  if (!last) return null;
  if (last.startsWith("/")) return last;
  if (last.startsWith("~")) return null;
  return cwd ? `${cwd.replace(/\/$/, "")}/${last.replace(/^\.\//, "")}` : null;
}

const cache = new Map<string, Promise<string>>();

/** The image to show: inline data as is, or the file loaded through the desktop app. */
export function useImageSrc(p: { src?: string; path?: string }): { src: string | null; error: string | null } {
  const [state, setState] = useState<{ src: string | null; error: string | null }>({ src: p.src ?? null, error: null });
  useEffect(() => {
    if (p.src || !p.path) {
      setState({ src: p.src ?? null, error: null });
      return;
    }
    if (!isTauri) {
      setState({ src: null, error: t("Images from files show in the desktop app.") });
      return;
    }
    let gone = false;
    let load = cache.get(p.path);
    if (!load) {
      load = import("@tauri-apps/api/core").then(({ invoke }) => invoke<string>("read_image", { path: p.path }));
      cache.set(p.path, load);
      load.catch(() => cache.delete(p.path!));
    }
    load.then(
      (src) => !gone && setState({ src, error: null }),
      (e: unknown) => !gone && setState({ src: null, error: String(e) }),
    );
    return () => {
      gone = true;
    };
  }, [p.src, p.path]);
  return state;
}
