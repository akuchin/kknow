/**
 * UI strings.
 *
 * The code knows only keys. The texts live in `locales/<locale>.json` at the
 * repository root, one flat file per language, in the i18next JSON v4 format.
 * Translation tools (Weblate, Crowdin, Lokalise, i18n Ally) read that format
 * without a custom parser. The desktop menu in Rust reads the same files.
 *
 * The keys are flat and contain dots, so tools must use the flat key style:
 * `statusbar.vault.empty` and `statusbar.vault.empty.title` cannot both live
 * in a nested object.
 *
 * Substitution is `{{name}}`, as in i18next. Plurals use key suffixes with
 * the `Intl.PluralRules` categories: `statusbar.words_one`,
 * `statusbar.words_few`, `statusbar.words_other`. The code asks for
 * `statusbar.words` with a `count`. A missing category falls back to
 * `_other`.
 *
 * The Russian file is the reference: the `Key` type comes from it, so a typo
 * in a key is a build error. `tests/i18n.test.ts` checks that every other
 * file has the same keys and placeholders.
 */

import type ru from "../../locales/ru.json";

/** Values for `{{name}}` substitution. */
export type Params = Record<string, string | number>;

/** Plural categories, with the same names as in `Intl.PluralRules`. */
type Category = "zero" | "one" | "two" | "few" | "many" | "other";

/** A key without its plural suffix: the form that the code uses. */
type Base<K> = K extends `${infer B}_${Category}` ? B : K;

/** String key. The Russian file defines the list. */
export type Key = Base<keyof typeof ru>;

type Dict = Record<string, string>;

/**
 * Every file in `locales/` is a language. A new language is one new file,
 * plus its entry in the Rust menu (`src-tauri/src/menu.rs`).
 */
const LOCALES: Record<string, Dict> = Object.fromEntries(
  Object.entries(
    import.meta.glob<Dict>("../../locales/*.json", { eager: true, import: "default" }),
  ).map(([path, dict]) => [path.replace(/^.*\/(.+)\.json$/, "$1"), dict]),
);

const FALLBACK = "ru";

const SETTING = "kknow:locale";

/**
 * Language names, each in its own language: the `locale.name` key of its own
 * file. "Русский" stays "Русский" in the English locale. Otherwise a user who
 * picked a foreign language could not find their own in the list.
 */
export const LOCALE_NAMES: Record<string, string> = Object.fromEntries(
  Object.entries(LOCALES).map(([name, dict]) => [name, dict["locale.name"] ?? name]),
);

/**
 * The language to show.
 *
 * The system language is ignored on purpose: the app is written in Russian,
 * and on an English macOS it would silently switch to a partial translation.
 * The choice is explicit: it lives in `localStorage`, and the status bar menu
 * changes it.
 */
function pickLocale(): string {
  const wanted = localStorage.getItem(SETTING) ?? FALLBACK;
  return wanted in LOCALES ? wanted : FALLBACK;
}

export const locale = pickLocale();

/** The reference language first, then the others by code. */
export const locales = Object.keys(LOCALES).sort((a, b) =>
  a === FALLBACK ? -1 : b === FALLBACK ? 1 : a.localeCompare(b),
);

/**
 * Stores the language. The window must reload after that: labels are spread
 * over the markup, the input fields and the CodeMirror config. Rebuilding
 * them costs more and is less reliable than a page reload.
 */
export function setLocale(name: string): void {
  if (!(name in LOCALES)) return;
  localStorage.setItem(SETTING, name);
}

const dict = LOCALES[locale];
const reference = LOCALES[FALLBACK];
const plurals = new Intl.PluralRules(locale);

/**
 * The label for a key; `{{name}}` placeholders get values from `params`.
 * A key that this language does not have yet comes from the Russian file:
 * a half-done translation shows mixed text, not raw keys.
 */
export function t(key: Key, params: Params = {}): string {
  const text = lookup(dict, key, params) ?? lookup(reference, key, params) ?? key;

  return text.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/** The plain text, or the plural form for `count`, which falls back to `_other`. */
function lookup(from: Dict, key: string, params: Params): string | undefined {
  if (key in from) return from[key];
  const category = plurals.select(Number(params.count ?? 0));
  return from[`${key}_${category}`] ?? from[`${key}_other`];
}
