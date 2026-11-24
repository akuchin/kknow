import { api, type AppSettings, type Theme } from "../api";
import { themeName, themeProblemText } from "../core-text";
import { setDiagramBase } from "../diagram";
import { LOCALE_NAMES, locale, locales, setLocale, t } from "../i18n";
import { openMenu } from "../menu";
import { modeOf } from "../note-mode";
import { setDefaultScale } from "../note-scale";
import { app, el } from "./dom";
import { editor } from "./editor";
import { applyScale, flushPendingSave, positionKey, setMode } from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";
import { renderTree } from "./tree";
import { openSettings } from "./util";

const BUILT_IN: [string, string][] = [
  ["light", t("theme.light")],
  ["dark", t("theme.dark")],
];

let themes: Theme[] = [];

let currentTheme = "light";

let painted: string[] = [];

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

  void setDiagramBase(base, el.preview).then(() => editor.redrawDiagrams());

  currentTheme = id;
  if (options.remember !== false) void saveSettings({ theme: id });

  if (theme && theme.warnings.length > 0) {
    const warning = themeProblemText(theme.warnings[0]);
    setStatus(t("status.themeWarning", { name: themeName(theme), warning }), "error");
  }
}

export async function refreshThemes(): Promise<void> {
  try {
    const set = await api.listThemes();
    themes = set.themes;
    if (set.problems.length > 0) setStatus(themeProblemText(set.problems[0]), "error");
  } catch (error) {
    themes = [];
    fail(error);
  }
  pickTheme();
}

function themeList(): [string, string][] {
  return [...BUILT_IN, ...themes.map((theme): [string, string] => [theme.id, themeName(theme)])];
}

export function pickTheme(): void {
  const wanted = state.settings?.theme ?? "light";
  const known = themeList().some(([id]) => id === wanted);
  applyTheme(known ? wanted : "light", { remember: known });
}

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

async function switchLocale(name: string): Promise<void> {
  if (name === locale) return;
  await flushPendingSave();
  setLocale(name);
  await saveSettings({ locale: name });
  location.reload();
}

export async function saveSettings(change: Partial<AppSettings>): Promise<void> {
  if (!state.settings) return;
  try {
    state.setSettings(await api.setSettings({ ...state.settings, ...change }));
  } catch (error) {
    fail(error);
  }
}

export async function toggleLineNumbers(): Promise<void> {
  if (!state.settings) return;
  await saveSettings({ lineNumbers: !state.settings.lineNumbers });
  showLineNumbers(state.settings.lineNumbers);
}

function showLineNumbers(on: boolean): void {
  editor.setLineNumbers(on);
  app.dataset.lineNumbers = String(on);
  status.setLineNumbers(on);
}

export function applySettings(incoming: AppSettings): void {
  const localeChanged = incoming.locale !== locale;
  const defaultChanged = incoming.viewMode !== state.settings?.viewMode;
  const scaleChanged = incoming.noteScale !== state.settings?.noteScale;
  const labelChanged = incoming.noteLabel !== state.settings?.noteLabel;
  state.setSettings(incoming);
  if (labelChanged) renderTree();
  showLineNumbers(incoming.lineNumbers);
  if (scaleChanged) {
    setDefaultScale(incoming.noteScale);
    applyScale();
  }

  if (incoming.theme !== currentTheme) applyTheme(incoming.theme, { remember: false });
  if (defaultChanged) {
    setMode(state.note ? modeOf(positionKey(state.note), incoming.viewMode) : incoming.viewMode, {
      remember: false,
    });
  }

  if (localeChanged) {
    setLocale(incoming.locale);
    void flushPendingSave().then(() => location.reload());
  }
}

export async function loadSettings(): Promise<boolean> {
  try {
    state.setSettings(await api.getSettings());
  } catch (error) {
    fail(error);
  }

  if (state.settings) {
    showLineNumbers(state.settings.lineNumbers);
    setDefaultScale(state.settings.noteScale);
    if (state.settings.locale !== locale) {
      setLocale(state.settings.locale);
      location.reload();
      return true;
    }
  }
  return false;
}
