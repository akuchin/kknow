import type ru from "../../locales/ru.json";
import { errorText } from "./error-view";
import { fill, lookup, type Dict, type Params } from "./translate";

export type { Params };

type Category = "zero" | "one" | "two" | "few" | "many" | "other";

type Base<K> = K extends `${infer B}_${Category}` ? B : K;

export type Key = Base<keyof typeof ru>;

const LOCALES: Record<string, Dict> = Object.fromEntries(
  Object.entries(
    import.meta.glob<Dict>("../../locales/*.json", { eager: true, import: "default" }),
  ).map(([path, dict]) => [path.replace(/^.*\/(.+)\.json$/, "$1"), dict]),
);

const FALLBACK = "ru";

const SETTING = "kknow:locale";

export const LOCALE_NAMES: Record<string, string> = Object.fromEntries(
  Object.entries(LOCALES).map(([name, dict]) => [name, dict["locale.name"] ?? name]),
);

function pickLocale(): string {
  const wanted = localStorage.getItem(SETTING) ?? FALLBACK;
  return wanted in LOCALES ? wanted : FALLBACK;
}

export const locale = pickLocale();

export const locales = Object.keys(LOCALES).sort((a, b) =>
  a === FALLBACK ? -1 : b === FALLBACK ? 1 : a.localeCompare(b),
);

export function setLocale(name: string): void {
  if (!(name in LOCALES)) return;
  localStorage.setItem(SETTING, name);
}

const dict = LOCALES[locale];
const reference = LOCALES[FALLBACK];
const plurals = new Intl.PluralRules(locale);

const dictionaries = [dict, reference];

export function t(key: Key, params: Params = {}): string {
  return fill(lookup(dictionaries, plurals, key, params) ?? key, params);
}

export function maybe(key: string, params: Params = {}): string | undefined {
  if (lookup(dictionaries, plurals, key, params) === undefined) return undefined;
  return t(key as Key, params);
}

export function describeError(error: unknown): string {
  return errorText(error, maybe);
}

export function textIn(name: string, key: string): string | undefined {
  return LOCALES[name]?.[key];
}
