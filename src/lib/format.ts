import { t, useI18n } from "./i18n";

const loc = () => useI18n.getState().locale;

export function clock(ts: number): string {
  return new Date(ts).toLocaleTimeString(loc(), { hour: "2-digit", minute: "2-digit", hour12: false });
}

/** "now", "5m", "3h", "2d" in the interface language. */
export function ago(ts: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ts) / 1000));
  if (s < 45) return t("now");
  const unit = (n: number, u: "minute" | "hour" | "day") =>
    new Intl.NumberFormat(loc(), { style: "unit", unit: u, unitDisplay: "narrow" }).format(n).replace(/\s+/g, loc() === "en" ? "" : " ");
  const m = Math.round(s / 60);
  if (m < 60) return unit(m, "minute");
  const h = Math.round(m / 60);
  if (h < 24) return unit(h, "hour");
  return unit(Math.round(h / 24), "day");
}

export function duration(ms: number): string {
  if (ms < 1000) return t("{n}ms", { n: Math.round(ms) });
  const s = ms / 1000;
  if (s < 60) return t("{n}s", { n: s.toFixed(s < 10 ? 1 : 0) });
  const m = Math.floor(s / 60);
  return t("{m}m {s}s", { m, s: Math.round(s % 60) });
}

export function basename(p: string): string {
  return p.split("/").pop() ?? p;
}

export function dirname(p: string): string {
  const i = p.lastIndexOf("/");
  return i === -1 ? "" : p.slice(0, i);
}

export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
export const MOD = isMac ? "⌘" : "Ctrl+";
export const ALT = isMac ? "⌥" : "Alt+";
