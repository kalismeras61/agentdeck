import { createHighlighterCore, createCssVariablesTheme, type HighlighterCore, type ThemedToken } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";

export type Lang = "dart" | "shellscript" | "json" | "yaml" | "text";

// Colors come from CSS variables, so code follows the app theme with no re-highlighting.
const theme = createCssVariablesTheme({ name: "agentdeck", variablePrefix: "--shiki-", fontStyle: true });

let highlighter: Promise<HighlighterCore> | null = null;
function getHighlighter() {
  highlighter ??= createHighlighterCore({
    themes: [theme],
    langs: [import("@shikijs/langs/dart"), import("@shikijs/langs/shellscript"), import("@shikijs/langs/json"), import("@shikijs/langs/yaml")],
    engine: createJavaScriptRegexEngine(),
  });
  return highlighter;
}

const cache = new Map<string, ThemedToken[][]>();
const MAX_HIGHLIGHT_CHARS = 200_000;

export function langForPath(path: string): Lang {
  if (path.endsWith(".dart")) return "dart";
  if (path.endsWith(".json")) return "json";
  if (path.endsWith(".yaml") || path.endsWith(".yml")) return "yaml";
  if (path.endsWith(".sh")) return "shellscript";
  return "text";
}

export function cachedTokens(code: string, lang: Lang): ThemedToken[][] | undefined {
  return cache.get(`${lang}\u0000${code}`);
}

/** Returns null for plain text or oversized input; callers render unstyled text in that case. */
export async function tokenize(code: string, lang: Lang): Promise<ThemedToken[][] | null> {
  if (lang === "text" || code.length > MAX_HIGHLIGHT_CHARS) return null;
  const key = `${lang}\u0000${code}`;
  const hit = cache.get(key);
  if (hit) return hit;
  try {
    const h = await getHighlighter();
    const tokens = h.codeToTokensBase(code, { lang, theme: "agentdeck" });
    if (cache.size > 200) cache.clear();
    cache.set(key, tokens);
    return tokens;
  } catch {
    return null;
  }
}
