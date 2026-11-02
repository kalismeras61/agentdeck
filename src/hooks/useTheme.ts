import { useEffect, useState } from "react";
import { useStore } from "../state/store";

export function useMedia(query: string) {
  const [match, setMatch] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const m = window.matchMedia(query);
    const on = () => setMatch(m.matches);
    m.addEventListener("change", on);
    return () => m.removeEventListener("change", on);
  }, [query]);
  return match;
}

export function useResolvedTheme(): "dark" | "light" {
  const theme = useStore((s) => s.theme);
  const prefersLight = useMedia("(prefers-color-scheme: light)");
  return theme === "system" ? (prefersLight ? "light" : "dark") : theme;
}
