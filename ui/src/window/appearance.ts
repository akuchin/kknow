// The look of the window: app settings, themes, the language and line numbers.
//
// Settings are shared with the settings window and arrive as events; only this
// module calls `state.setSettings`. Themes live in the vault folder, so the
// vault zone reloads them on a switch. The module does not touch the DOM on
// import.

import { api, type AppSettings, type Theme } from "../api";
import { setDiagramBase } from "../diagram";
import { LOCALE_NAMES, locale, locales, setLocale, t } from "../i18n";
import { openMenu } from "../menu";
import { modeOf } from "../note-mode";
import { setDefaultScale } from "../note-scale";
import { el } from "./dom";
import { editor } from "./editor";
import { applyScale, flushPendingSave, positionKey, setMode } from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";
import { renderTree } from "./tree";
import { openSettings } from "./util";

/** Built-in themes. Their values are in styles.css. */
const BUILT_IN: [string, string][] = [
  ["light", t("theme.light")],
  ["dark", t("theme.dark")],
];

let themes: Theme[] = [];

/** Selected theme. The status bar menu marks it with a check. */
let currentTheme = "light";

/** Variables set by the previous theme. They must be removed. */
let painted: string[] = [];

/**
 * Applies a theme: `data-theme` picks the built-in base, and the manifest
 * variables go on top of it.
 *
 * The core already validated the values, so `setProperty` gets them as is.
 */
export function applyTheme(id: string, options: { remember?: boolean } = {}): void {
  const root = document.documentElement;
  for (const name of painted) root.style.removeProperty(name);
  painted = [];

  const theme = themes.find((candidate) => candidate.id === id);
  const base = theme ? theme.base : id === "dark" ? "dark" : "light";
  root.dataset.theme = base;

  for (const [name, value] of Object.entries(theme?.variables ?? {})) {
    root.style.setProperty(`--${name}`, value);
    painted.push(`--${name}`);
  }

  // Diagram colors are a mermaid setting, not CSS, so diagrams must redraw.
  // Preview first, then the editor: the editor redraws its diagrams and needs
  // the new setting already applied.
  void setDiagramBase(base, el.preview).then(() => editor.redrawDiagrams());

  currentTheme = id;
  // The theme lives in settings shared with the settings window. When the
  // theme came from there as an event, do not write it back.
  if (options.remember !== false) void saveSettings({ theme: id });

  if (theme && theme.warnings.length > 0) {
    setStatus(t("status.themeWarning", { name: theme.name, warning: theme.warnings[0] }), "error");
  }
}

/** Themes live in the vault folder, so they load with it. */
export async function refreshThemes(): Promise<void> {
  try {
    const set = await api.listThemes();
    themes = set.themes;
    if (set.problems.length > 0) setStatus(set.problems[0], "error");
  } catch (error) {
    themes = [];
    fail(error);
  }
  pickTheme();
}

/** Available themes: built-in ones and the ones in the vault folder. */
function themeList(): [string, string][] {
  return [...BUILT_IN, ...themes.map((theme): [string, string] => [theme.id, theme.name])];
}

/** Applies the theme from settings. It can be gone: the file was deleted or the vault changed. */
export function pickTheme(): void {
  const wanted = state.settings?.theme ?? "light";
  const known = themeList().some(([id]) => id === wanted);
  // A missing theme falls back to light, but the choice stays saved: it applies
  // again when the vault with that theme returns.
  applyTheme(known ? wanted : "light", { remember: known });
}

/**
 * Status bar gear: theme, language and settings in one menu.
 *
 * These were three labeled segments, and together they took more space than
 * everything else. Users change them rarely, and a check marks the current
 * choice.
 */
export function openMoreMenu(anchor: HTMLElement): void {
  openMenu(anchor, [
    {
      title: t("menu.theme"),
      items: themeList().map(([id, name]) => ({
        label: name,
        checked: id === currentTheme,
        run: () => applyTheme(id),
      })),
    },
    {
      title: t("menu.locale"),
      items: locales.map((name) => ({
        label: LOCALE_NAMES[name] ?? name,
        checked: name === locale,
        run: () => void switchLocale(name),
      })),
    },
    {
      items: [
        { label: t("menu.settings"), glyph: "gear", action: "settings.open", run: () => void openSettings() },
      ],
    },
  ]);
}

/** Labels are set in the markup when the window builds, so the window reloads. */
async function switchLocale(name: string): Promise<void> {
  if (name === locale) return;
  await flushPendingSave();
  // The language goes to settings. The localStorage mirror updates at once, so
  // after the reload the labels use the new language.
  setLocale(name);
  await saveSettings({ locale: name });
  location.reload();
}

/** Writes settings to the core. Both windows learn about the change from an event. */
export async function saveSettings(change: Partial<AppSettings>): Promise<void> {
  if (!state.settings) return;
  try {
    state.setSettings(await api.setSettings({ ...state.settings, ...change }));
  } catch (error) {
    fail(error);
  }
}

/**
 * The status bar button. It changes the setting, not the editor: the settings
 * window shows the same switch, and both learn about the change the same way.
 */
export async function toggleLineNumbers(): Promise<void> {
  if (!state.settings) return;
  await saveSettings({ lineNumbers: !state.settings.lineNumbers });
  // The event also comes here; the button does not wait for it.
  showLineNumbers(state.settings.lineNumbers);
}

function showLineNumbers(on: boolean): void {
  editor.setLineNumbers(on);
  status.setLineNumbers(on);
}

/** Applies settings from the core: language, theme, preview, line numbers. */
export function applySettings(incoming: AppSettings): void {
  const localeChanged = incoming.locale !== locale;
  const defaultChanged = incoming.viewMode !== state.settings?.viewMode;
  const scaleChanged = incoming.noteScale !== state.settings?.noteScale;
  const labelChanged = incoming.noteLabel !== state.settings?.noteLabel;
  state.setSettings(incoming);
  if (labelChanged) renderTree();
  showLineNumbers(incoming.lineNumbers);
  // A new default size affects only a file without its own size.
  if (scaleChanged) {
    setDefaultScale(incoming.noteScale);
    applyScale();
  }

  if (incoming.theme !== currentTheme) applyTheme(incoming.theme, { remember: false });
  // A new default affects only a note without a saved mode.
  if (defaultChanged) {
    setMode(state.note ? modeOf(positionKey(state.note), incoming.viewMode) : incoming.viewMode, {
      remember: false,
    });
  }

  // Labels are set in the markup when the window builds, so a language change
  // needs a reload. The localStorage mirror updates before it.
  if (localeChanged) {
    setLocale(incoming.locale);
    void flushPendingSave().then(() => location.reload());
  }
}

/**
 * Reads the settings at start and applies the line numbers and the default
 * size. Returns `true` if the window reloads in the language of the settings.
 */
export async function loadSettings(): Promise<boolean> {
  try {
    state.setSettings(await api.getSettings());
  } catch (error) {
    fail(error);
  }

  if (state.settings) {
    showLineNumbers(state.settings.lineNumbers);
    setDefaultScale(state.settings.noteScale);
    // The window language comes from settings. `i18n` reads its mirror
    // synchronously; they differ only after a manual edit of the file.
    if (state.settings.locale !== locale) {
      setLocale(state.settings.locale);
      location.reload();
      return true;
    }
  }
  return false;
}
