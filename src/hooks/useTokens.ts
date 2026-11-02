import { useEffect, useState } from "react";
import type { ThemedToken } from "shiki/core";
import { cachedTokens, tokenize, type Lang } from "../lib/highlight";

export function useTokens(code: string, lang: Lang): ThemedToken[][] | null {
  const [tokens, setTokens] = useState<ThemedToken[][] | null>(() => cachedTokens(code, lang) ?? null);
  useEffect(() => {
    let alive = true;
    const hit = cachedTokens(code, lang);
    if (hit) {
      setTokens(hit);
      return;
    }
    setTokens(null);
    void tokenize(code, lang).then((t) => {
      if (alive) setTokens(t);
    });
    return () => {
      alive = false;
    };
  }, [code, lang]);
  return tokens;
}
