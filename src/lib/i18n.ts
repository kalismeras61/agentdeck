/**
 * Interface language. The English text is the key: `t("New session")` shows the translation
 * for the current language, or the English when there is none yet. Agent output, code and
 * file names are never translated.
 *
 *   t("Moved to {name}", { name })            placeholders in braces
 *   tn(count, "{n} file", "{n} files")        plurals; catalogs give each language's forms
 *
 * Catalogs live in src/locales/<id>.json and load on demand. `scripts/i18n-extract.mjs` lists
 * every key used in the code into src/locales/en.json, the list translators start from.
 */
import { create } from "zustand";

export type LocaleId = "en" | "tr" | "de" | "ru" | "es" | "fr" | "pt" | "zh" | "ja" | "ko" | "ar";

export const LOCALES: { id: LocaleId; name: string; english: string; rtl?: boolean }[] = [
  { id: "en", name: "English", english: "English" },
  { id: "tr", name: "Türkçe", english: "Turkish" },
  { id: "de", name: "Deutsch", english: "German" },
  { id: "ru", name: "Русский", english: "Russian" },
  { id: "es", name: "Español", english: "Spanish" },
  { id: "fr", name: "Français", english: "French" },
  { id: "pt", name: "Português", english: "Portuguese" },
  { id: "zh", name: "简体中文", english: "Chinese (Simplified)" },
  { id: "ja", name: "日本語", english: "Japanese" },
  { id: "ko", name: "한국어", english: "Korean" },
  { id: "ar", name: "العربية", english: "Arabic", rtl: true },
];

/** A translation: one string, or plural forms by Intl.PluralRules category. */
type Entry = string | Partial<Record<Intl.LDMLPluralRule, string>>;
type Catalog = Record<string, Entry>;

const loaders = import.meta.glob<{ default: Catalog }>("../locales/*.json");

/** Languages with a translation shipped (English always). */
export const AVAILABLE = LOCALES.filter((l) => l.id === "en" || `../locales/${l.id}.json` in loaders);

/** The language the Mac prefers, if AgentDeck has it. */
export function systemLocale(langs: readonly string[] = typeof navigator !== "undefined" ? navigator.languages ?? [navigator.language] : []): LocaleId {
  for (const l of langs) {
    const base = l.toLowerCase().split("-")[0] as LocaleId;
    if (AVAILABLE.some((x) => x.id === base)) return base;
  }
  return "en";
}

interface I18nState {
  locale: LocaleId;
  catalog: Catalog;
  /** Changes with every loaded catalog, so views keyed on it re-render. */
  version: number;
  load: (locale: LocaleId) => Promise<void>;
}

export const useI18n = create<I18nState>((set, get) => ({
  locale: "en",
  catalog: {},
  version: 0,
  load: async (locale) => {
    let catalog: Catalog = {};
    if (locale !== "en") {
      const loader = loaders[`../locales/${locale}.json`];
      try {
        catalog = loader ? (await loader()).default : {};
      } catch {
        catalog = {};
      }
    }
    set({ locale, catalog, version: get().version + 1 });
    if (typeof document !== "undefined") {
      document.documentElement.lang = locale;
      document.documentElement.dir = LOCALES.find((l) => l.id === locale)?.rtl ? "rtl" : "ltr";
    }
  },
}));

const fill = (text: string, vars?: Record<string, string | number>) =>
  vars ? text.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : text;

/** The current language's text for `source` (English), with {placeholders} filled in. */
export function t(source: string, vars?: Record<string, string | number>): string {
  const e = useI18n.getState().catalog[source];
  return fill(typeof e === "string" && e ? e : source, vars);
}

/** Plural text: English `one` / `other` forms; translations give their own forms. `{n}` is the count. */
export function tn(n: number, one: string, other: string, vars?: Record<string, string | number>): string {
  const { catalog, locale } = useI18n.getState();
  const all = { n, ...vars };
  const e = catalog[`${one}|${other}`];
  if (e && typeof e === "object") {
    const form = new Intl.PluralRules(locale).select(n);
    const text = e[form] ?? e.other;
    if (text) return fill(text, all);
  }
  return fill(n === 1 ? one : other, all);
}

/** Re-render when the language changes (for components that compute text outside render). */
export function useLocale(): LocaleId {
  return useI18n((s) => s.locale);
}

/** Numbers and lists in the current language. */
export const fmt = {
  number: (n: number) => new Intl.NumberFormat(useI18n.getState().locale).format(n),
  list: (items: string[]) => new Intl.ListFormat(useI18n.getState().locale, { type: "conjunction" }).format(items),
};
