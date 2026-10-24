/**
 * Settings window.
 *
 * A separate window, not a panel in the main one: you see settings next to
 * what you change, and closing them moves nothing in your work. Sections on
 * the left, content on the right; vaults also get a list between them,
 * because there can be many.
 *
 * The window keeps almost no state: settings live in the core, the vault list
 * in the registry. Only what is open on the screen stays here.
 */

import { defaultKeymap, historyKeymap } from "@codemirror/commands";
import { searchKeymap } from "@codemirror/search";
import type { KeyBinding } from "@codemirror/view";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  onKeymapChanged,
  onSettingsChanged,
  onSyncStatus,
  onUpdateStatus,
  type AppSettings,
  type CommandManifest,
  type CommandSpec,
  type KeymapView,
  type NoteLabel,
  type ConflictPolicy,
  type MarkdownAssociation,
  type Provider,
  type SyncSettings,
  type SyncStatus,
  type TargetKind,
  type Theme,
  type UpdateStatus,
  type VaultStats,
  type VaultView,
  type ViewMode,
} from "../api";
import {
  MAC,
  bind,
  canBind,
  conflictOf,
  defaultKeysOf,
  formatKey,
  isCustom,
  keyFromEvent,
  keysOf,
  matches,
  rebindable,
  sameKey,
  shortcut,
  unbind,
  useKeymap,
  vaultKey,
  type Action,
  type FixedAction,
  type Keymap,
} from "../actions";
import { iconMarkup } from "../icons";
import { LOCALE_NAMES, locale, locales, setLocale, t, type Key } from "../i18n";
import { MODES } from "../note-mode";
import { SCALE_PERCENT } from "../note-scale";
import "../styles.css";
import "./settings.css";

/** Window sections: URL anchor and label. */
const SECTIONS: { id: string; label: Key }[] = [
  { id: "general", label: "settings.section.general" },
  { id: "appearance", label: "settings.section.appearance" },
  { id: "vaults", label: "settings.section.vaults" },
  { id: "commands", label: "settings.section.commands" },
  { id: "keys", label: "settings.section.keys" },
  { id: "editor", label: "settings.section.editor" },
];

/** Target kinds in form order. */
const TARGETS: { kind: TargetKind; label: Key }[] = [
  { kind: "note", label: "settings.commands.target.note" },
  { kind: "folder", label: "settings.commands.target.folder" },
  { kind: "vault", label: "settings.commands.target.vault" },
];

/** Label colors: the same set as in the vault registry. */
const COLORS = ["blue", "violet", "amber", "red", "green", "teal"];

/** Auto sync intervals in minutes. */
const INTERVALS = [0, 5, 15, 60];

document.documentElement.lang = locale;

const root = document.querySelector<HTMLDivElement>("#settings")!;
root.innerHTML = `
  <nav class="settings-sections" id="settings-sections"></nav>
  <div class="settings-body" id="settings-body"></div>
`;

const el = {
  sections: document.querySelector<HTMLElement>("#settings-sections")!,
  body: document.querySelector<HTMLElement>("#settings-body")!,
};

let settings: AppSettings | null = null;
let vaults: VaultView[] = [];
let statuses = new Map<string, SyncStatus>();
/** The vault open in the "Vaults" section. */
let picked: string | null = null;
/** Command manifest: the form edits all of it and writes the file on each change. */
let manifest: CommandManifest | null = null;
/**
 * The command open in the "Commands" section, by index, not by ID. The form
 * shows empty and duplicate IDs so the user can fix them, and the selection
 * must survive that.
 */
let pickedCommand = 0;
/** The "Last sync" text on the right. An event updates it, not a redraw. */
let lastSyncHint: HTMLElement | null = null;

function section(): string {
  const wanted = location.hash.replace("#", "");
  return SECTIONS.some((entry) => entry.id === wanted) ? wanted : "general";
}

function renderSections(): void {
  el.sections.replaceChildren();

  for (const entry of SECTIONS) {
    const button = document.createElement("button");
    button.className = "settings-section";
    button.textContent = t(entry.label);
    button.setAttribute("aria-current", String(entry.id === section()));
    button.onclick = () => {
      location.hash = entry.id;
    };
    el.sections.append(button);
  }

  const version = document.createElement("span");
  version.className = "settings-version";
  version.textContent = "kknow";
  // The version comes from the app, not from a constant here: a release takes
  // it from the git tag, and a constant would stay at the first release.
  void getVersion()
    .then((value) => {
      version.textContent = `kknow ${value}`;
    })
    .catch(console.error);
  el.sections.append(version);
}

/** Settings row: label on the left, field on the right, hint under the label. */
function row(label: string, control: HTMLElement, detail?: string): HTMLElement {
  const item = document.createElement("div");
  item.className = "settings-row";

  const text = document.createElement("div");
  text.className = "settings-row-text";
  const title = document.createElement("span");
  title.textContent = label;
  text.append(title);

  if (detail) {
    const hint = document.createElement("span");
    hint.className = "settings-row-detail";
    hint.textContent = detail;
    text.append(hint);
  }

  item.append(text, control);
  return item;
}

function select(
  options: { value: string; label: string }[],
  current: string,
  onPick: (value: string) => void,
): HTMLSelectElement {
  const field = document.createElement("select");
  for (const option of options) {
    const item = document.createElement("option");
    item.value = option.value;
    item.textContent = option.label;
    field.append(item);
  }
  field.value = current;
  field.onchange = () => onPick(field.value);
  return field;
}

function toggle(on: boolean, onPick: (value: boolean) => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "settings-toggle";
  button.type = "button";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(on));
  button.onclick = () => onPick(button.getAttribute("aria-checked") !== "true");
  return button;
}

/**
 * Full-width field: label on top, field below it, hint at the bottom.
 *
 * Not [`row`]: the script and the pattern list need the full panel width, and
 * a "label left, field right" pair leaves them only half.
 */
function field(label: string, control: HTMLElement, detail?: string): HTMLElement {
  const item = document.createElement("label");
  item.className = "settings-field";

  const title = document.createElement("span");
  title.textContent = label;
  item.append(title, control);

  if (detail) {
    const hint = document.createElement("span");
    hint.className = "settings-row-detail";
    hint.textContent = detail;
    item.append(hint);
  }

  return item;
}

/**
 * A row of toggles: several can be on, and none is also a valid answer.
 *
 * Not a multi-select list: there are three or four short options, and seeing
 * the selection at once matters more than saving space.
 */
function chips(
  options: { value: string; label: string }[],
  chosen: string[],
  onToggle: (value: string, on: boolean) => void,
): HTMLElement {
  const group = document.createElement("div");
  group.className = "settings-chips";

  for (const option of options) {
    const chip = document.createElement("button");
    chip.className = "settings-chip";
    chip.type = "button";
    chip.textContent = option.label;
    chip.setAttribute("aria-pressed", String(chosen.includes(option.value)));
    chip.onclick = () => onToggle(option.value, chip.getAttribute("aria-pressed") !== "true");
    group.append(chip);
  }

  return group;
}

/**
 * Default font size: − / slider / + / value, as in the status bar. The value
 * resets to 100%.
 *
 * The slider writes on release, not on each step: a write renders the page
 * again, and a new slider under the pointer would stop the drag.
 */
function scaleControl(percent: number, onPick: (percent: number) => void): HTMLElement {
  const group = document.createElement("div");
  group.className = "settings-scale";
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", t("settings.editor.scale"));

  const smaller = document.createElement("button");
  smaller.className = "ghost";
  smaller.type = "button";
  smaller.textContent = "−";
  smaller.title = t("statusbar.scale.smaller");
  smaller.disabled = percent <= SCALE_PERCENT.min;
  smaller.onclick = () => onPick(percent - SCALE_PERCENT.step);

  const slider = document.createElement("input");
  slider.type = "range";
  slider.className = "scale-slider";
  slider.min = String(SCALE_PERCENT.min);
  slider.max = String(SCALE_PERCENT.max);
  slider.step = String(SCALE_PERCENT.step);
  slider.value = String(percent);
  slider.setAttribute("aria-label", t("settings.editor.scale"));

  const larger = document.createElement("button");
  larger.className = "ghost";
  larger.type = "button";
  larger.textContent = "+";
  larger.title = t("statusbar.scale.larger");
  larger.disabled = percent >= SCALE_PERCENT.max;
  larger.onclick = () => onPick(percent + SCALE_PERCENT.step);

  const value = document.createElement("button");
  value.className = "ghost settings-scale-value";
  value.type = "button";
  value.textContent = `${percent}%`;
  value.title = t("settings.editor.scale.reset");
  value.dataset.changed = String(percent !== 100);
  value.onclick = () => onPick(100);

  slider.oninput = () => (value.textContent = `${slider.value}%`);
  slider.onchange = () => onPick(Number(slider.value));

  group.append(smaller, slider, larger, value);
  return group;
}

function heading(text: string): HTMLElement {
  const title = document.createElement("h2");
  title.className = "settings-heading";
  title.textContent = text;
  return title;
}

/** Writes settings to the core; both windows learn about them from an event. */
async function patch(change: Partial<AppSettings>): Promise<void> {
  if (!settings) return;
  const next = { ...settings, ...change };
  try {
    settings = await api.setSettings(next);
  } catch (error) {
    console.error(error);
  }
  render();
}

function render(): void {
  renderSections();
  el.body.replaceChildren();

  if (!settings) return;
  switch (section()) {
    case "appearance":
      void renderAppearance();
      break;
    case "vaults":
      void renderVaults();
      break;
    case "commands":
      void renderCommands();
      break;
    case "keys":
      renderKeys();
      break;
    case "editor":
      renderEditor();
      break;
    default:
      renderGeneral();
  }
}

function renderGeneral(): void {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.general")));

  page.append(
    row(
      t("settings.general.locale"),
      select(
        locales.map((name) => ({ value: name, label: LOCALE_NAMES[name] ?? name })),
        settings!.locale,
        (value) => void patch({ locale: value }),
      ),
      t("settings.general.locale.detail"),
    ),
  );

  page.append(
    row(
      t("settings.general.openLast"),
      toggle(settings!.openLastNote, (value) => void patch({ openLastNote: value })),
      t("settings.general.openLast.detail"),
    ),
  );

  page.append(markdownRow());

  page.append(
    row(
      t("settings.general.autoUpdate"),
      toggle(settings!.autoUpdate, (value) => void patch({ autoUpdate: value })),
      t("settings.general.autoUpdate.detail"),
    ),
  );
  page.append(updateRow());

  el.body.append(page);
}

/** Auto-update state. It comes as events also while the window is closed to
 *  the "General" section, so it lives here, not in the row. */
let update: UpdateStatus = { state: "idle" };
/** "Check now" was pressed: then `idle` means "you have the latest version",
 *  not "no check yet". */
let checkedByHand = false;
/** Repaints the update row if it is on screen. */
let paintUpdate: (() => void) | null = null;

/**
 * "Updates": the button checks now, and the text under it tells the state.
 *
 * The button works with the setting off too: the setting is about the
 * background checks. The restart itself is in the status bar of the main
 * window, because that window saves the open note before it.
 */
function updateRow(): HTMLElement {
  const check = document.createElement("button");
  check.type = "button";
  check.textContent = t("settings.general.update.check");

  const item = row(t("settings.general.update"), check);
  const state = document.createElement("span");
  state.className = "settings-row-detail";
  item.querySelector(".settings-row-text")!.append(state);

  paintUpdate = () => {
    const busy = update.state === "checking" || update.state === "downloading";
    check.disabled = busy || update.state === "unavailable" || update.state === "ready";
    switch (update.state) {
      case "unavailable":
        state.textContent = t("settings.general.update.unavailable");
        break;
      case "idle":
        state.textContent = checkedByHand ? t("settings.general.update.latest") : "";
        break;
      case "checking":
        state.textContent = t("settings.general.update.checking");
        break;
      case "downloading":
        state.textContent = t("settings.general.update.downloading", {
          version: update.version,
        });
        break;
      case "ready":
        state.textContent = t("settings.general.update.ready", { version: update.version });
        break;
      case "failed":
        state.textContent = t("settings.general.update.failed", { message: update.message });
    }
  };

  check.onclick = async () => {
    checkedByHand = true;
    update = { state: "checking" };
    paintUpdate?.();
    try {
      update = await api.checkUpdate();
    } catch (error) {
      update = { state: "failed", message: String(error) };
    }
    paintUpdate?.();
  };

  paintUpdate();
  return item;
}

/**
 * "Open .md files in kknow": the button makes the app the default one, and the
 * text under it tells which app opens markdown now.
 *
 * The window asks the system each time the section shows and stores nothing.
 * The user can change the default app outside kknow, and the window must not
 * show stale data.
 */
function markdownRow(): HTMLElement {
  const claim = document.createElement("button");
  claim.type = "button";
  claim.className = "primary";
  claim.textContent = t("settings.general.markdown.claim");
  claim.disabled = true;

  const item = row(
    t("settings.general.markdown"),
    claim,
    t("settings.general.markdown.detail"),
  );
  const state = document.createElement("span");
  state.className = "settings-row-detail";
  item.querySelector(".settings-row-text")!.append(state);

  const show = (association: MarkdownAssociation) => {
    claim.disabled = !association.supported || association.ours;
    state.textContent = !association.supported
      ? t("settings.general.markdown.unsupported")
      : association.ours
        ? t("settings.general.markdown.ours")
        : association.current
          ? t("settings.general.markdown.current", { app: association.current })
          : t("settings.general.markdown.none");
  };

  claim.onclick = async () => {
    claim.disabled = true;
    try {
      show(await api.claimMarkdown());
      state.textContent = t("settings.general.markdown.done");
    } catch (error) {
      claim.disabled = false;
      state.textContent = String(error);
    }
  };

  api.markdownAssociation().then(show, (error) => {
    state.textContent = String(error);
  });
  return item;
}

async function renderAppearance(): Promise<void> {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.appearance")));
  el.body.replaceChildren(page);

  let themes: Theme[] = [];
  let problems: string[] = [];
  try {
    const set = await api.listThemes();
    themes = set.themes;
    problems = set.problems;
  } catch (error) {
    problems = [String(error)];
  }

  const options = [
    { value: "light", label: t("theme.light") },
    { value: "dark", label: t("theme.dark") },
    ...themes.map((theme) => ({ value: theme.id, label: theme.name })),
  ];

  page.append(
    row(
      t("settings.appearance.theme"),
      select(options, settings!.theme, (value) => void patch({ theme: value })),
      t("settings.appearance.theme.detail"),
    ),
  );

  page.append(
    row(
      t("settings.appearance.noteLabel"),
      select(
        [
          { value: "title", label: t("settings.appearance.noteLabel.title") },
          { value: "file", label: t("settings.appearance.noteLabel.file") },
        ],
        settings!.noteLabel,
        (value) => void patch({ noteLabel: value as NoteLabel }),
      ),
      t("settings.appearance.noteLabel.detail"),
    ),
  );

  // Theme problems show here too: a theme lives in the vault or in the shared
  // folder, and the person who put it there will fix it.
  for (const problem of [...problems, ...themes.flatMap((theme) => theme.warnings)]) {
    const note = document.createElement("p");
    note.className = "settings-problem";
    note.textContent = problem;
    page.append(note);
  }

  const folder = document.createElement("button");
  folder.className = "ghost";
  folder.textContent = t("settings.appearance.folder.open");
  folder.onclick = () => void api.revealThemes().catch(showProblem);
  page.append(
    row(t("settings.appearance.folder"), folder, t("settings.appearance.folder.detail")),
  );

  page.append(heading(t("settings.appearance.themes")));
  if (themes.length === 0) {
    const empty = document.createElement("p");
    empty.className = "settings-row-detail";
    empty.textContent = t("settings.appearance.themes.none");
    page.append(empty);
  }
  for (const theme of themes) page.append(themeRow(theme));

  function showProblem(error: unknown): void {
    const note = document.createElement("p");
    note.className = "settings-problem";
    note.textContent = String(error);
    page.append(note);
  }
}

/**
 * A custom theme row: its source and, for a vault theme, "Make shared".
 *
 * A shared or built-in theme needs no button: all vaults already see it. A vault theme
 * with a shared namesake hides that namesake, so it gets the same button, but
 * the button updates the shared copy.
 */
function themeRow(theme: Theme): HTMLElement {
  if (theme.source !== "vault") {
    const label = document.createElement("span");
    label.className = "settings-row-detail";
    label.textContent = t(
      theme.source === "global"
        ? "settings.appearance.source.global"
        : "settings.appearance.source.builtIn",
    );
    return row(theme.name, label);
  }

  const shared = theme.shadowsGlobal;
  const share = document.createElement("button");
  share.className = "ghost";
  share.textContent = t(shared ? "settings.appearance.update" : "settings.appearance.share");
  share.onclick = async () => {
    share.disabled = true;
    try {
      await api.shareTheme(theme.id);
      await renderAppearance();
    } catch (error) {
      share.disabled = false;
      share.title = String(error);
      console.error(error);
    }
  };

  const source = t(shared ? "settings.appearance.source.both" : "settings.appearance.source.vault");
  const detail = t(shared ? "settings.appearance.update.detail" : "settings.appearance.share.detail");
  return row(`${theme.name} · ${source}`, share, detail);
}

/** The label of each action on the keys page. */
const ACTION_LABELS: Record<Exclude<Action, FixedAction>, Key> = {
  "palette.open": "settings.keys.action.palette.open",
  "note.new": "settings.keys.action.note.new",
  "file.open": "settings.keys.action.file.open",
  "note.save": "settings.keys.action.note.save",
  "note.find": "settings.keys.action.note.find",
  "note.outline": "settings.keys.action.note.outline",
  "note.mode": "settings.keys.action.note.mode",
  "note.comment": "settings.keys.action.note.comment",
  "bookmark.toggle": "settings.keys.action.bookmark.toggle",
  "bookmark.list": "settings.keys.action.bookmark.list",
  "task.list": "settings.keys.action.task.list",
  "sidebar.toggle": "settings.keys.action.sidebar.toggle",
  "history.back": "settings.keys.action.history.back",
  "history.forward": "settings.keys.action.history.forward",
  "scale.larger": "settings.keys.action.scale.larger",
  "scale.smaller": "settings.keys.action.scale.smaller",
  "scale.reset": "settings.keys.action.scale.reset",
  "vault.sync": "settings.keys.action.vault.sync",
  "settings.open": "settings.keys.action.settings.open",
  "format.bold": "settings.keys.action.format.bold",
  "format.italic": "settings.keys.action.format.italic",
  "format.strike": "settings.keys.action.format.strike",
  "table.rowUp": "settings.keys.action.table.rowUp",
  "table.rowDown": "settings.keys.action.table.rowDown",
  "table.columnLeft": "settings.keys.action.table.columnLeft",
  "table.columnRight": "settings.keys.action.table.columnRight",
};

/** Groups of the keys page, by the first part of the action ID. */
const KEY_GROUPS: { label: Key; has: (action: Action) => boolean }[] = [
  { label: "settings.keys.group.app", has: (action) => !/^(format|table)\./.test(action) },
  { label: "settings.keys.group.format", has: (action) => action.startsWith("format.") },
  { label: "settings.keys.group.table", has: (action) => action.startsWith("table.") },
];

/**
 * The keys of the editor's own commands: indentation, comments, search. An
 * action that takes one of them would lose it inside the note, because the
 * editor handles the key first.
 */
const EDITOR_KEYS: readonly string[] = [...defaultKeymap, ...historyKeymap, ...searchKeymap]
  .map((binding: KeyBinding) => (MAC ? binding.mac : binding.win ?? binding.linux) ?? binding.key)
  .filter((key): key is string => typeof key === "string" && !key.includes(" "))
  .filter((key) => {
    try {
      return sameKey(key, key);
    } catch {
      return false;
    }
  });

/** The keys file as the core read it. */
let keymap: KeymapView | null = null;
/** The action that waits for a key press. */
let recording: Action | null = null;
/** A message under an action: a key that did not fit, maybe with a way out. */
let keyNotice: { action: Action; text: string; reassign?: string } | null = null;

/** Writes the changed keys; both windows learn about them from an event. */
async function saveKeymap(next: Keymap): Promise<void> {
  try {
    keymap = await api.setKeymap(next);
    useKeymap(keymap.keys);
  } catch (error) {
    console.error(error);
  }
  recording = null;
  keyNotice = null;
  render();
}

/** Takes the keys from the core; repaints the page if they changed. */
function takeKeymap(view: KeymapView): void {
  const changed = useKeymap(view.keys);
  const problems = JSON.stringify(view.problems) !== JSON.stringify(keymap?.problems ?? []);
  keymap = view;
  if ((changed || problems) && section() === "keys" && !recording) render();
}

function startRecording(action: Action): void {
  recording = action;
  keyNotice = null;
  render();
}

function stopRecording(): void {
  if (!recording) return;
  recording = null;
  keyNotice = null;
  render();
}

/**
 * A key press while an action records. The capture phase comes before the
 * buttons and the window keys, so ⌘, here is a key, not "open settings".
 */
document.addEventListener(
  "keydown",
  (event) => {
    const action = recording;
    if (!action) return;
    event.preventDefault();
    event.stopPropagation();

    if (matches(event, "cancel")) {
      stopRecording();
      return;
    }
    const key = keyFromEvent(event);
    if (!key) return;

    const shown = formatKey(key);
    if (!canBind(key)) {
      keyNotice = { action, text: t("settings.keys.needsModifier", { key: shown }) };
      render();
      return;
    }

    const conflict = conflictOf(key, action, EDITOR_KEYS);
    if (!conflict) {
      void saveKeymap(bind(action, key));
      return;
    }
    if (conflict.kind === "action") {
      recording = null;
      keyNotice = {
        action,
        text: t("settings.keys.conflict.action", { key: shown, action: t(ACTION_LABELS[conflict.action]) }),
        reassign: key,
      };
    } else {
      keyNotice = { action, text: t(`settings.keys.conflict.${conflict.kind}`, { key: shown }) };
    }
    render();
  },
  true,
);

// A key press in another app must not land in the recording.
window.addEventListener("blur", stopRecording);

function renderKeys(): void {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.keys")));

  for (const group of KEY_GROUPS) {
    const actions = rebindable().filter(group.has);
    page.append(subheading(t(group.label)));
    for (const action of actions) page.append(keyRow(action));
  }

  // ⌘1…⌘9 follow the vault list, so they do not change.
  const vaults = document.createElement("span");
  vaults.className = "settings-key-combo";
  vaults.append(kbd(`${formatKey(vaultKey(1))}…${formatKey(vaultKey(9))}`));
  page.append(row(t("settings.keys.vaults"), vaults));

  const custom = rebindable().some(isCustom);
  const resetAll = document.createElement("button");
  resetAll.className = "ghost";
  resetAll.textContent = t("settings.keys.resetAll");
  resetAll.disabled = !custom;
  // Entries of actions that this version does not know stay in the file.
  resetAll.onclick = () => void saveKeymap(unbind(...rebindable()));

  if (keymap?.problems.length) page.append(keyProblems(keymap.problems));
  if (keymap?.file) page.append(keymapFile(keymap.file, resetAll));
  else page.append(resetAll);

  const hint = document.createElement("p");
  hint.className = "settings-row-detail";
  hint.textContent = t("settings.keys.hint");
  page.append(hint);

  el.body.append(page);
}

function subheading(text: string): HTMLElement {
  const title = document.createElement("h3");
  title.className = "settings-subheading";
  title.textContent = text;
  return title;
}

function kbd(text: string): HTMLElement {
  const key = document.createElement("kbd");
  key.textContent = text;
  return key;
}

/** One action: its keys, "Change", and "Reset" if the user changed them. */
function keyRow(action: Exclude<Action, FixedAction>): HTMLElement {
  const control = document.createElement("div");
  control.className = "settings-key-control";

  const combo = document.createElement("span");
  combo.className = "settings-key-combo";
  if (recording === action) {
    const waiting = kbd(t("settings.keys.record"));
    waiting.className = "recording";
    combo.append(waiting);
  } else {
    const keys = keysOf(action);
    if (keys.length) combo.append(...keys.map((key) => kbd(formatKey(key))));
    else combo.textContent = t("settings.keys.none");
  }
  control.append(combo);

  if (recording === action) {
    const cancel = document.createElement("button");
    cancel.className = "ghost";
    cancel.textContent = t("settings.keys.cancel");
    cancel.onclick = stopRecording;
    control.append(cancel);
  } else {
    const change = document.createElement("button");
    change.className = "ghost";
    change.textContent = t("settings.keys.change");
    change.title = t("settings.keys.change.title");
    change.onclick = () => startRecording(action);
    control.append(change);
  }

  if (isCustom(action) && recording !== action) {
    const reset = document.createElement("button");
    reset.className = "ghost";
    reset.textContent = t("settings.keys.reset");
    const original = defaultKeysOf(action)[0];
    reset.title = t("settings.keys.reset.title", {
      key: original ? formatKey(original) : t("settings.keys.none"),
    });
    reset.onclick = () => void saveKeymap(unbind(action));
    control.append(reset);
  }

  const notice = keyNotice?.action === action ? keyNotice : null;
  const detail = notice?.text ?? (recording === action
    ? t("settings.keys.record.hint", { cancel: shortcut("cancel") })
    : undefined);
  const line = row(t(ACTION_LABELS[action]), control, detail);
  line.classList.toggle("custom", isCustom(action));
  if (notice) line.classList.add("has-notice");

  if (notice?.reassign) {
    const key = notice.reassign;
    const reassign = document.createElement("button");
    reassign.className = "ghost";
    reassign.textContent = t("settings.keys.conflict.replace");
    reassign.onclick = () => void saveKeymap(bind(action, key));
    const cancel = document.createElement("button");
    cancel.className = "ghost";
    cancel.textContent = t("settings.keys.cancel");
    cancel.onclick = () => {
      keyNotice = null;
      render();
    };
    control.replaceChildren(combo, reassign, cancel);
  }
  return line;
}

/** Entries of a hand-edited file that the core skipped. */
function keyProblems(problems: string[]): HTMLElement {
  const box = document.createElement("div");
  box.className = "settings-key-problems";
  const title = document.createElement("p");
  title.textContent = t("settings.keys.problems");
  const list = document.createElement("ul");
  for (const problem of problems) {
    const item = document.createElement("li");
    item.textContent = problem;
    list.append(item);
  }
  box.append(title, list);
  return box;
}

/** The file location with a reveal button, like the commands file. */
function keymapFile(file: string, resetAll: HTMLElement): HTMLElement {
  const path = document.createElement("span");
  path.className = "settings-path";
  path.textContent = file;
  path.title = file;

  const finder = document.createElement("button");
  finder.className = "ghost";
  finder.textContent = t("settings.vaults.finder");
  finder.onclick = () => void api.revealKeymap().catch(console.error);

  const line = document.createElement("div");
  line.className = "settings-vault-head settings-key-file";
  const names = document.createElement("div");
  names.className = "settings-vault-names";
  const title = document.createElement("span");
  title.textContent = t("settings.keys.file.detail");
  title.className = "settings-row-detail";
  names.append(path, title);
  const buttons = document.createElement("div");
  buttons.className = "settings-key-control";
  buttons.append(resetAll, finder);
  line.append(names, buttons);
  return line;
}

function renderEditor(): void {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.editor")));

  page.append(
    row(
      t("settings.editor.mode"),
      select(
        MODES.map((mode) => ({ value: mode, label: t(`mode.${mode}`) })),
        settings!.viewMode,
        (value) => void patch({ viewMode: value as ViewMode }),
      ),
      t("settings.editor.mode.detail"),
    ),
  );

  page.append(
    row(
      t("settings.editor.lineNumbers"),
      toggle(settings!.lineNumbers, (value) => void patch({ lineNumbers: value })),
      t("settings.editor.lineNumbers.detail"),
    ),
  );

  page.append(
    row(
      t("settings.editor.scale"),
      scaleControl(settings!.noteScale, (value) => void patch({ noteScale: value })),
      t("settings.editor.scale.detail"),
    ),
  );

  el.body.append(page);
}

/**
 * Commands section: the list on the left, the selected command form on the
 * right.
 *
 * The manifest used to be edited in a text editor: the menu revealed the file
 * in the file manager. The form knows what a text editor does not: the
 * available runners, the possible targets, and the environment the command
 * gets. It also shows core problems next to the list, not after the first
 * failed run.
 *
 * The form edits the whole manifest and writes it on each change. There is no
 * "save" button, as in other settings. An invalid entry is saved like the
 * others: a command without a script is unfinished, not a reason to block the
 * window from closing.
 */
async function renderCommands(): Promise<void> {
  const page = document.createElement("div");
  page.className = "settings-page";

  try {
    // The vault list is for the vaults field: a command can apply to some
    // vaults only, and picking them by name is easier.
    [manifest, vaults] = await Promise.all([api.editCommands(), api.listVaults()]);
  } catch (error) {
    page.append(heading(t("settings.section.commands")), problem(String(error)));
    el.body.replaceChildren(page);
    return;
  }

  // Broken JSON is the only case the form refuses to edit: it could not read
  // the file, and a save would erase what is there.
  if (manifest.broken) {
    page.append(
      heading(t("settings.section.commands")),
      problem(t("settings.commands.broken")),
      problem(manifest.broken),
      commandsFile(),
    );
    el.body.replaceChildren(page);
    return;
  }

  if (pickedCommand >= manifest.commands.length) pickedCommand = 0;

  const layout = document.createElement("div");
  layout.className = "settings-commands";

  const list = document.createElement("div");
  list.className = "settings-command-list";
  layout.append(list, page);
  el.body.replaceChildren(layout);

  fillCommandList(list);
  renderCommandForm(page);
}

/** Command list on the left: label, runner and whether it has problems. */
function fillCommandList(list: HTMLElement): void {
  const count = document.createElement("p");
  count.className = "settings-vault-count";
  count.textContent = t("settings.commands.count", { count: manifest!.commands.length });
  list.replaceChildren(count);

  manifest!.commands.forEach((command, index) => {
    const button = document.createElement("button");
    button.className = "settings-command";
    button.setAttribute("aria-current", String(index === pickedCommand));
    button.onclick = () => {
      pickedCommand = index;
      void renderCommands();
    };

    const text = document.createElement("span");
    text.className = "settings-vault-text";
    const name = document.createElement("span");
    name.textContent = command.name.trim() || t("settings.commands.untitled");
    const detail = document.createElement("span");
    detail.className = "settings-row-detail";
    detail.textContent = [command.kind, ...command.targets.map(targetLabel)].join(" · ");
    text.append(name, detail);
    button.append(text);

    // The core sends problems as "id: what is wrong" strings; the prefix finds
    // the command. The window has no own validation on purpose: it could only
    // differ from the core for the worse.
    if (manifest!.problems.some((entry) => entry.startsWith(`${command.id}:`))) {
      const dot = document.createElement("span");
      dot.className = "status-dot";
      dot.dataset.tone = "error";
      button.append(dot);
    }

    list.append(button);
  });

  const tools = document.createElement("div");
  tools.className = "settings-command-tools";

  const add = document.createElement("button");
  add.className = "ghost";
  add.innerHTML = `${iconMarkup("terminal")}<span>${t("settings.commands.add")}</span>`;
  add.onclick = () => void addCommand();

  const remove = document.createElement("button");
  remove.className = "ghost danger";
  remove.innerHTML = `${iconMarkup("close")}<span>${t("settings.commands.remove")}</span>`;
  remove.disabled = manifest!.commands.length === 0;
  remove.onclick = () => void removeCommand();

  tools.append(add, remove);
  list.append(tools);
}

/** The form of one command, with the file-wide settings below it. */
function renderCommandForm(page: HTMLElement): void {
  page.replaceChildren();

  const command = manifest!.commands[pickedCommand];
  if (!command) {
    page.append(heading(t("settings.section.commands")));
    const empty = document.createElement("p");
    empty.className = "settings-row-detail";
    empty.textContent = t("settings.commands.empty");
    page.append(empty, ...fileSection());
    return;
  }

  const name = document.createElement("input");
  name.className = "settings-name";
  name.value = command.name;
  name.placeholder = t("settings.commands.untitled");
  // On `change`, not on each keystroke: each save writes the file, and one
  // label would mean thirty writes.
  name.onchange = () => {
    command.name = name.value;
    void saveCommands();
  };
  page.append(field(t("settings.commands.name"), name, t("settings.commands.name.detail")));

  const id = document.createElement("input");
  id.className = "settings-mono";
  id.value = command.id;
  id.autocapitalize = "off";
  id.spellcheck = false;
  id.onchange = () => {
    command.id = id.value.trim();
    void saveCommands();
  };
  page.append(
    field(t("settings.commands.id"), id, t("settings.commands.id.detail", { id: command.id })),
  );

  // The runner in the file can be unknown to the core: a typo there is the
  // most common problem. Then it goes first in the list. An empty field would
  // hide exactly what the core complains about, and the user would fix it
  // blind.
  const kinds = manifest!.kinds.includes(command.kind)
    ? manifest!.kinds
    : [command.kind, ...manifest!.kinds];

  page.append(
    row(
      t("settings.commands.kind"),
      select(
        kinds.map((kind) => ({ value: kind, label: kind })),
        command.kind,
        (value) => {
          command.kind = value;
          void saveCommands();
        },
      ),
    ),
  );

  const program = document.createElement("input");
  program.className = "settings-mono";
  program.value = command.program ?? "";
  program.placeholder = t("settings.commands.program.placeholder");
  program.spellcheck = false;
  program.onchange = () => {
    // An empty field means "the runner default", not a program with an empty name.
    command.program = program.value.trim() || null;
    void saveCommands();
  };
  page.append(
    field(t("settings.commands.program"), program, t("settings.commands.program.detail")),
  );

  const script = document.createElement("textarea");
  script.className = "settings-script";
  script.rows = 6;
  script.value = command.script;
  script.spellcheck = false;
  script.onchange = () => {
    command.script = script.value;
    void saveCommands();
  };
  page.append(field(t("settings.commands.script"), script, t("settings.commands.script.detail")));

  page.append(commandVars(command));

  page.append(heading(t("settings.commands.where")));
  page.append(
    row(
      t("settings.commands.targets"),
      chips(
        TARGETS.map((entry) => ({ value: entry.kind, label: t(entry.label) })),
        command.targets,
        (value, on) => {
          const kind = value as TargetKind;
          command.targets = on
            ? TARGETS.map((entry) => entry.kind).filter(
                (entry) => entry === kind || command.targets.includes(entry),
              )
            : command.targets.filter((entry) => entry !== kind);
          void saveCommands(true);
        },
      ),
      t("settings.commands.targets.detail"),
    ),
  );

  page.append(
    row(
      t("settings.commands.vaults"),
      chips(
        vaults.map((vault) => ({ value: vault.id, label: vault.name })),
        command.vaults,
        (value, on) => {
          command.vaults = on
            ? [...command.vaults, value]
            : command.vaults.filter((entry) => entry !== value);
          void saveCommands(true);
        },
      ),
      t("settings.commands.vaults.detail"),
    ),
  );

  const patterns = document.createElement("textarea");
  patterns.className = "settings-script";
  patterns.rows = 2;
  patterns.value = command.match.join("\n");
  patterns.placeholder = t("settings.commands.match.placeholder");
  patterns.spellcheck = false;
  patterns.onchange = () => {
    command.match = lines(patterns.value);
    void saveCommands();
  };
  page.append(field(t("settings.commands.match"), patterns, t("settings.commands.match.detail")));

  const requires = document.createElement("input");
  requires.className = "settings-mono";
  requires.value = command.requires.join(", ");
  requires.placeholder = t("settings.commands.requires.placeholder");
  requires.spellcheck = false;
  requires.onchange = () => {
    command.requires = requires.value
      .split(",")
      .map((entry) => entry.trim())
      .filter(Boolean);
    // Note fields reach the script as variables, so the list under the script
    // is rebuilt right here: this is where the user looks at it.
    void saveCommands(true);
  };
  page.append(
    field(t("settings.commands.requires"), requires, t("settings.commands.requires.detail")),
  );

  page.append(heading(t("settings.commands.how")));
  page.append(
    row(
      t("settings.commands.cwd"),
      select(
        [
          { value: "vault", label: t("settings.commands.cwd.vault") },
          { value: "target", label: t("settings.commands.cwd.target") },
        ],
        command.cwd,
        (value) => {
          command.cwd = value as CommandSpec["cwd"];
          void saveCommands();
        },
      ),
    ),
  );

  const timeout = document.createElement("input");
  timeout.type = "number";
  timeout.min = "1";
  timeout.value = String(command.timeoutSeconds);
  timeout.onchange = () => {
    command.timeoutSeconds = Math.max(1, Math.round(Number(timeout.value) || 0));
    timeout.value = String(command.timeoutSeconds);
    void saveCommands();
  };
  page.append(row(t("settings.commands.timeout"), timeout));

  page.append(
    row(
      t("settings.commands.confirm"),
      toggle(command.confirm, (value) => {
        command.confirm = value;
        void saveCommands(true);
      }),
    ),
  );

  page.append(
    row(
      t("settings.commands.login"),
      toggle(command.login, (value) => {
        command.login = value;
        void saveCommands(true);
      }),
      t("settings.commands.login.detail"),
    ),
  );

  page.append(
    field(
      t("settings.commands.env"),
      envEditor(command.env ?? {}, (env) => {
        command.env = env;
        void saveCommands();
      }),
      t("settings.commands.env.detail"),
    ),
  );

  page.append(...fileSection());
}

/** File-wide part: problems, path and variables shared by all commands. */
function fileSection(): HTMLElement[] {
  // Problems have their own block because they change most often. The core
  // recomputes them on each change, and a fixed command must stop showing
  // problems at once, not after a section switch.
  const problems = document.createElement("div");
  problems.className = "settings-problems";
  fillProblems(problems);

  const parts: HTMLElement[] = [heading(t("settings.commands.file")), problems];

  parts.push(
    field(
      t("settings.commands.env.shared"),
      envEditor(manifest!.env, (env) => {
        manifest!.env = env;
        void saveCommands();
      }),
      t("settings.commands.env.shared.detail"),
    ),
    commandsFile(),
  );

  return parts;
}

/** The file location row with a reveal button. */
function commandsFile(): HTMLElement {
  const path = document.createElement("span");
  path.className = "settings-path";
  path.textContent = manifest!.path;
  // The path is cut with an ellipsis; the tooltip shows all of it.
  path.title = manifest!.path;

  const finder = document.createElement("button");
  finder.className = "ghost";
  finder.textContent = t("settings.vaults.finder");
  finder.onclick = () => void api.revealCommands().catch(console.error);

  const line = document.createElement("div");
  line.className = "settings-vault-head";
  const names = document.createElement("div");
  names.className = "settings-vault-names";
  const title = document.createElement("span");
  title.textContent = t("settings.commands.file.detail");
  title.className = "settings-row-detail";
  names.append(path, title);
  line.append(names, finder);
  return line;
}

/**
 * The environment the script gets.
 *
 * The list is not decoration. The target path comes as a variable, not as a
 * text substitution, and without the list it is not clear how a command
 * learns its target. The list depends on the command: `$KKNOW_NOTE` shows only
 * if the command targets notes, and the required fields turn into the exact
 * names to type in the script.
 */
function commandVars(command: CommandSpec): HTMLElement {
  const box = document.createElement("details");
  box.className = "settings-vars";

  const title = document.createElement("summary");
  title.textContent = t("settings.commands.vars");
  box.append(title);

  const list = document.createElement("dl");
  const put = (name: string, meaning: string): void => {
    const key = document.createElement("dt");
    key.textContent = `$${name}`;
    const value = document.createElement("dd");
    value.textContent = meaning;
    list.append(key, value);
  };

  put("KKNOW_PATH", t("settings.commands.vars.path"));
  put("KKNOW_TARGET", t("settings.commands.vars.target"));
  put("KKNOW_RELATIVE", t("settings.commands.vars.relative"));
  put("KKNOW_VAULT", t("settings.commands.vars.vault"));
  put("KKNOW_VAULT_ID", t("settings.commands.vars.vaultId"));
  put("KKNOW_VAULT_NAME", t("settings.commands.vars.vaultName"));
  if (command.targets.includes("note")) {
    put("KKNOW_NOTE", t("settings.commands.vars.note"));
  }

  for (const key of command.requires) {
    const name = variableName(key);
    if (name) put(name, t("settings.commands.vars.fm", { field: key }));
  }
  put("KKNOW_FM_…", t("settings.commands.vars.fm.any"));

  box.append(list);
  return box;
}

/**
 * Variable name for a note field: `confluence-id` → `KKNOW_FM_CONFLUENCE_ID`.
 *
 * It repeats `variable_name` from `crates/kknow-run/src/lib.rs`. The copy is
 * intentional and safe: it is a hint in the form, not a rule. The core does
 * the real work, so if the rules differ, only the hint is wrong, not the run.
 */
function variableName(key: string): string | null {
  let name = "";
  for (const symbol of key) {
    if (/[a-z0-9]/i.test(symbol)) name += symbol.toUpperCase();
    else if (!name.endsWith("_")) name += "_";
  }

  const trimmed = name.replace(/^_+|_+$/g, "");
  if (!trimmed || /^\d/.test(trimmed)) return null;
  return `KKNOW_FM_${trimmed}`;
}

/**
 * Name and value pairs.
 *
 * A row without a name is not a variable: an empty row lives in the form, not
 * in the manifest. So "add a variable" appends it straight to the markup, and
 * saves and rebuilds nothing. A rebuild would move the cursor out of the field
 * where the user is about to type the name.
 */
function envEditor(
  env: Record<string, string>,
  onChange: (env: Record<string, string>) => void,
): HTMLElement {
  const box = document.createElement("div");
  box.className = "settings-env";

  const collect = (): Record<string, string> => {
    const next: Record<string, string> = {};
    for (const line of box.querySelectorAll<HTMLElement>(".settings-env-row")) {
      const [name, value] = line.querySelectorAll("input");
      if (name.value.trim()) next[name.value.trim()] = value.value;
    }
    return next;
  };

  // The button goes in first, and rows are inserted before it. It marks the
  // end of the list, so it does not need to move down for each new variable.
  const add = document.createElement("button");
  add.className = "ghost";
  add.type = "button";
  add.textContent = t("settings.commands.env.add");
  box.append(add);

  const addRow = (key: string, value: string): HTMLInputElement => {
    const line = document.createElement("div");
    line.className = "settings-env-row";

    const name = document.createElement("input");
    name.className = "settings-mono";
    name.value = key;
    name.placeholder = t("settings.commands.env.name");
    name.spellcheck = false;

    const text = document.createElement("input");
    text.className = "settings-mono";
    text.value = value;
    text.placeholder = t("settings.commands.env.value");
    text.spellcheck = false;

    // An erased name removes the variable, so a row needs no remove button.
    name.onchange = () => onChange(collect());
    text.onchange = () => onChange(collect());

    line.append(name, text);
    box.insertBefore(line, add);
    return name;
  };

  for (const [key, value] of Object.entries(env)) addRow(key, value);

  add.onclick = () => addRow("", "").focus();
  if (Object.keys(env).length === 0) addRow("", "");

  return box;
}

/** Non-empty, trimmed lines of a text field. */
function lines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function targetLabel(kind: TargetKind): string {
  const found = TARGETS.find((entry) => entry.kind === kind);
  return found ? t(found.label) : kind;
}

function problem(text: string): HTMLElement {
  const note = document.createElement("p");
  note.className = "settings-problem";
  note.textContent = text;
  return note;
}

function fillProblems(box: HTMLElement): void {
  box.replaceChildren(...manifest!.problems.map(problem));
}

/**
 * Writes the manifest and updates what depends on it.
 *
 * `rebuild` rebuilds the form. Toggles and lists that change the form content
 * use it. Typed text does not: a rebuild would move the cursor out of the
 * field.
 */
async function saveCommands(rebuild = false): Promise<void> {
  if (!manifest) return;

  try {
    // Take only the problems from the answer, not the whole manifest. Form
    // fields hold references to entries of this manifest. A new manifest would
    // send the next edit to an entry that is no longer in the list.
    manifest.problems = (await api.saveCommands(manifest.commands, manifest.env)).problems;
  } catch (error) {
    console.error(error);
  }

  const list = el.body.querySelector<HTMLElement>(".settings-command-list");
  if (list) fillCommandList(list);

  const problems = el.body.querySelector<HTMLElement>(".settings-problems");
  if (problems) fillProblems(problems);

  const page = el.body.querySelector<HTMLElement>(".settings-commands .settings-page");
  if (page && rebuild) renderCommandForm(page);
}

async function addCommand(): Promise<void> {
  if (!manifest) return;

  manifest.commands.push({
    id: freshId(),
    name: t("settings.commands.new"),
    // The first runner is `shell`: it exists everywhere and needs nothing extra.
    kind: manifest.kinds[0] ?? "shell",
    script: "",
    targets: ["note", "folder", "vault"],
    vaults: [],
    match: [],
    requires: [],
    login: false,
    program: null,
    cwd: "vault",
    timeoutSeconds: 60,
    confirm: false,
    env: {},
  });

  pickedCommand = manifest.commands.length - 1;
  await saveCommands(true);
}

/** An unused ID. The terminal uses it to call the command, so it is ASCII. */
function freshId(): string {
  const used = new Set(manifest!.commands.map((command) => command.id));
  if (!used.has("command")) return "command";

  for (let number = 2; ; number += 1) {
    if (!used.has(`command-${number}`)) return `command-${number}`;
  }
}

async function removeCommand(): Promise<void> {
  const command = manifest?.commands[pickedCommand];
  if (!command) return;

  const name = command.name.trim() || command.id || t("settings.commands.untitled");
  if (!window.confirm(t("settings.commands.remove.confirm", { name }))) return;

  manifest!.commands.splice(pickedCommand, 1);
  if (pickedCommand >= manifest!.commands.length) {
    pickedCommand = Math.max(0, manifest!.commands.length - 1);
  }
  await saveCommands(true);
}

/**
 * Vaults section: the list on the left, the selected vault settings on the
 * right.
 *
 * The whole section redraws only when the vault list changes: add, remove,
 * rename. Sync state comes as an event and changes only the text. Otherwise
 * the panel would flicker and lose typed field values.
 */
async function renderVaults(): Promise<void> {
  lastSyncHint = null;
  await refreshVaults();

  const layout = document.createElement("div");
  layout.className = "settings-vaults";

  const list = document.createElement("div");
  list.className = "settings-vault-list";

  const count = document.createElement("p");
  count.className = "settings-vault-count";
  count.textContent = t("settings.vaults.count", { count: vaults.length });
  list.append(count);

  for (const vault of vaults) {
    const button = document.createElement("button");
    button.className = "settings-vault";
    button.dataset.vault = vault.id;
    button.setAttribute("aria-current", String(vault.id === picked));
    button.onclick = () => {
      picked = vault.id;
      void renderVaults();
    };

    const label = document.createElement("span");
    label.className = "vault-label";
    label.dataset.color = vault.color;

    const text = document.createElement("span");
    text.className = "settings-vault-text";
    const name = document.createElement("span");
    name.textContent = vault.name;
    const state = document.createElement("span");
    state.className = "settings-row-detail";
    state.textContent = describeVault(vault);
    text.append(name, state);

    const dot = document.createElement("span");
    dot.className = "status-dot";
    dot.dataset.tone = toneOf(vault);

    button.append(label, text, dot);
    list.append(button);
  }

  const tools = document.createElement("div");
  tools.className = "settings-vault-tools";

  const add = document.createElement("button");
  add.className = "ghost";
  add.title = t("menu.vault.add");
  add.innerHTML = `${iconMarkup("folder")}<span>${t("settings.vaults.add")}</span>`;
  add.onclick = () => void addVault();

  const remove = document.createElement("button");
  remove.className = "ghost danger";
  remove.innerHTML = `${iconMarkup("unlink")}<span>${t("settings.vaults.remove")}</span>`;
  remove.disabled = picked === null || vaults.length < 2;
  remove.title = t("settings.vaults.remove.detail");
  remove.onclick = () => void removeVault();

  tools.append(add, remove);
  list.append(tools);

  const details = document.createElement("div");
  details.className = "settings-page";
  layout.append(list, details);
  el.body.replaceChildren(layout);

  const vault = vaults.find((entry) => entry.id === picked);
  if (vault) await renderVaultDetails(details, vault);
}

/** Updates in place what depends on the sync state. */
function repaintVaults(): void {
  for (const vault of vaults) {
    const button = el.body.querySelector<HTMLElement>(
      `.settings-vault[data-vault="${CSS.escape(vault.id)}"]`,
    );
    const state = button?.querySelector<HTMLElement>(".settings-row-detail");
    if (state) state.textContent = describeVault(vault);

    const dot = button?.querySelector<HTMLElement>(".status-dot");
    if (dot) dot.dataset.tone = toneOf(vault);
  }

  if (lastSyncHint) lastSyncHint.textContent = describeLastSync(picked);
}

function describeVault(vault: VaultView): string {
  if (!vault.available) return t("menu.vault.missing");
  const status = statuses.get(vault.id);
  if (!status || status.state === "off") return t("statusbar.sync.off");
  if (status.state === "conflict") {
    return t("settings.vaults.conflicts", { count: status.conflicts });
  }
  if (status.state === "error") return t("statusbar.sync.error");
  return status.location ?? t("statusbar.sync.off");
}

function toneOf(vault: VaultView): string {
  if (!vault.available) return "off";
  const status = statuses.get(vault.id);
  if (!status) return "off";
  if (status.state === "error") return "error";
  if (status.state === "conflict") return "conflict";
  if (status.state === "off") return "off";
  return "ready";
}

/** Settings of one vault: name, label, path, sync, statistics. */
async function renderVaultDetails(page: HTMLElement, vault: VaultView): Promise<void> {
  const head = document.createElement("div");
  head.className = "settings-vault-head";

  const name = document.createElement("input");
  name.className = "settings-name";
  name.value = vault.name;
  name.onchange = () => void rename(vault.id, name.value);

  const path = document.createElement("span");
  path.className = "settings-path";
  path.textContent = vault.path;

  const finder = document.createElement("button");
  finder.className = "ghost";
  finder.textContent = t("settings.vaults.finder");
  // Show the vault selected in the list, not the active one: this window
  // configures what the user looks at.
  finder.onclick = () => void api.revealNote(undefined, vault.id).catch(console.error);

  const names = document.createElement("div");
  names.className = "settings-vault-names";
  names.append(name, path);
  head.append(names, finder);
  page.append(head);

  // Label: the set is fixed, so it is a row of circles, not a color field.
  const colors = document.createElement("div");
  colors.className = "settings-colors";
  for (const color of COLORS) {
    const swatch = document.createElement("button");
    swatch.className = "settings-color";
    swatch.dataset.color = color;
    swatch.setAttribute("aria-pressed", String(color === vault.color));
    swatch.title = color;
    swatch.onclick = () => void recolor(vault.id, color);
    colors.append(swatch);
  }
  page.append(row(t("settings.vaults.color"), colors));

  page.append(heading(t("settings.vaults.sync")));
  await renderVaultSync(page, vault);
  await renderVaultStats(page, vault);
}

async function renderVaultSync(page: HTMLElement, vault: VaultView): Promise<void> {
  let remote: SyncSettings | null = null;
  try {
    remote = await api.syncSettings(vault.id);
  } catch (error) {
    console.error(error);
  }

  const kind = remote?.provider.kind ?? "off";
  page.append(
    row(
      t("settings.vaults.provider"),
      select(
        [
          { value: "off", label: t("settings.vaults.provider.off") },
          { value: "folder", label: t("dialog.sync.provider.folder") },
          { value: "s3", label: t("dialog.sync.provider.s3") },
        ],
        kind,
        (value) => void switchProvider(vault, remote, value),
      ),
    ),
  );

  if (!remote) return;

  if (remote.provider.kind === "folder") {
    const folder = document.createElement("button");
    folder.className = "ghost";
    folder.textContent = remote.provider.path;
    folder.onclick = () => void pickFolder(vault, remote);
    page.append(row(t("dialog.sync.folder"), folder));
  } else {
    page.append(s3Form(vault, remote));
  }

  page.append(
    row(
      t("dialog.sync.auto"),
      select(
        INTERVALS.map((minutes) => ({
          value: String(minutes),
          label: minutes === 0
            ? t("dialog.sync.auto.off")
            : t("dialog.sync.auto.minutes", { count: minutes }),
        })),
        String(remote.autoSyncMinutes),
        (value) => void saveSync(vault, { ...remote }, { autoSyncMinutes: Number(value) }),
      ),
    ),
  );

  page.append(
    row(
      t("dialog.sync.policy"),
      select(
        [
          { value: "keepBoth", label: t("dialog.sync.policy.keepBoth") },
          { value: "preferLocal", label: t("dialog.sync.policy.preferLocal") },
          { value: "preferRemote", label: t("dialog.sync.policy.preferRemote") },
        ],
        remote.policy,
        (value) => void saveSync(vault, { ...remote }, { policy: value as ConflictPolicy }),
      ),
    ),
  );

  const sync = document.createElement("button");
  sync.className = "primary";
  sync.textContent = t("palette.action.sync");
  sync.onclick = () => void syncNow(vault.id);

  const last = row(t("settings.vaults.lastSync"), sync, describeLastSync(vault.id));
  lastSyncHint = last.querySelector<HTMLElement>(".settings-row-detail");
  page.append(last);
}

/** When the vault last synced. */
function describeLastSync(id: string | null): string {
  const at = id ? statuses.get(id)?.at : undefined;
  return at ? new Date(at).toLocaleString(locale) : t("settings.vaults.lastSync.never");
}

async function renderVaultStats(page: HTMLElement, vault: VaultView): Promise<void> {
  page.append(heading(t("settings.vaults.stats")));

  const line = document.createElement("p");
  line.className = "settings-stats";
  line.textContent = t("settings.vaults.stats.loading");
  page.append(line);

  if (!vault.available) {
    line.textContent = t("menu.vault.missing");
    return;
  }

  try {
    const stats: VaultStats = await api.vaultStats(vault.id);
    const hotkey = vaults.findIndex((entry) => entry.id === vault.id) + 1;
    line.textContent = [
      t("status.notes", { count: stats.notes }),
      t("settings.vaults.folders", { count: stats.folders }),
      formatSize(stats.bytes),
      hotkey <= 9 ? t("settings.vaults.hotkey", { key: formatKey(vaultKey(hotkey)) }) : "",
    ]
      .filter(Boolean)
      .join(" · ");
  } catch (error) {
    line.textContent = String(error);
  }
}

/** Human-readable size: bytes become KB and MB. */
function formatSize(bytes: number): string {
  if (bytes < 1024) return t("settings.vaults.bytes", { count: bytes });
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} КБ`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
}

async function refreshVaults(): Promise<void> {
  try {
    vaults = await api.listVaults();
    const all = await api.syncStatuses();
    statuses = new Map(all.map((entry) => [entry.vault, entry]));
  } catch (error) {
    console.error(error);
  }

  if (!vaults.some((entry) => entry.id === picked)) {
    picked = vaults.find((entry) => entry.active)?.id ?? vaults[0]?.id ?? null;
  }
}

async function addVault(): Promise<void> {
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== "string") return;

  try {
    const added = await api.addVault(path);
    picked = added.id;
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function removeVault(): Promise<void> {
  if (!picked) return;
  try {
    await api.removeVault(picked);
    picked = null;
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function rename(id: string, name: string): Promise<void> {
  try {
    await api.renameVault(id, name);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function recolor(id: string, color: string): Promise<void> {
  try {
    await api.setVaultColor(id, color);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function syncNow(id: string): Promise<void> {
  try {
    await api.syncNow(id);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

/**
 * Changes the provider from the list: "do not sync" turns sync off, folder asks
 * for a folder, S3 switches the provider and shows the key form below.
 */
async function switchProvider(
  vault: VaultView,
  remote: SyncSettings | null,
  kind: string,
): Promise<void> {
  if (kind === "off") {
    try {
      await api.forgetSyncSettings(vault.id);
    } catch (error) {
      console.error(error);
    }
    await renderVaults();
    return;
  }

  if (kind === "folder") {
    await pickFolder(vault, remote);
    return;
  }

  // The bucket and keys go into the form below; it appears once the provider
  // is S3. The core does not save an empty bucket, so we use a placeholder
  // name that the user edits right away.
  await saveSync(vault, remote, {
    provider: {
      kind: "s3",
      bucket: remote?.provider.kind === "s3" ? remote.provider.bucket : "kknow",
      endpoint: null,
      region: null,
      prefix: null,
      accessKeyId: null,
    },
  });
}

/**
 * S3 fields: bucket, endpoint, region, prefix, access key and secret.
 *
 * A button saves them, not each keystroke: half a typed endpoint is not a
 * setting yet. The secret goes to the OS keychain and never comes back, so an
 * empty field means "keep the old one".
 */
function s3Form(vault: VaultView, remote: SyncSettings): HTMLElement {
  const s3 = remote.provider.kind === "s3" ? remote.provider : null;
  const form = document.createElement("div");
  form.className = "settings-s3";

  const fields: Record<string, HTMLInputElement> = {};
  const field = (name: string, label: Key, value: string | null, placeholder = ""): void => {
    const line = document.createElement("label");
    const caption = document.createElement("span");
    caption.textContent = t(label);

    const input = document.createElement("input");
    input.value = value ?? "";
    input.placeholder = placeholder;
    input.autocomplete = "off";
    if (name === "secret") input.type = "password";

    fields[name] = input;
    line.append(caption, input);
    form.append(line);
  };

  field("bucket", "dialog.sync.bucket", s3?.bucket ?? "");
  field("endpoint", "dialog.sync.endpoint", s3?.endpoint ?? null, t("dialog.sync.endpoint.placeholder"));
  field("region", "dialog.sync.region", s3?.region ?? null, t("dialog.sync.region.placeholder"));
  field("prefix", "dialog.sync.prefix", s3?.prefix ?? null, t("dialog.sync.prefix.placeholder"));
  field("accessKeyId", "dialog.sync.key", s3?.accessKeyId ?? null, t("dialog.sync.key.placeholder"));
  field(
    "secret",
    "dialog.sync.secret",
    "",
    t(remote.hasSecret ? "dialog.sync.secret.saved" : "dialog.sync.secret.placeholder"),
  );

  const tools = document.createElement("menu");
  const save = document.createElement("button");
  save.className = "primary";
  save.textContent = t("dialog.sync.save");
  save.onclick = () => {
    const text = (name: string): string | null => fields[name].value.trim() || null;
    void saveSync(
      vault,
      remote,
      {
        provider: {
          kind: "s3",
          bucket: fields.bucket.value.trim(),
          endpoint: text("endpoint"),
          region: text("region"),
          prefix: text("prefix"),
          accessKeyId: text("accessKeyId"),
        },
      },
      text("secret"),
    );
  };

  tools.append(save);
  form.append(tools);
  return form;
}

async function pickFolder(vault: VaultView, remote: SyncSettings | null): Promise<void> {
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== "string") return;

  await saveSync(vault, remote, { provider: { kind: "folder", path } });
}

/** Writes the vault sync settings with one field changed. */
async function saveSync(
  vault: VaultView,
  remote: SyncSettings | null,
  change: Partial<{ provider: Provider; policy: ConflictPolicy; autoSyncMinutes: number }>,
  secret: string | null = null,
): Promise<void> {
  const base = {
    provider: remote?.provider ?? { kind: "folder" as const, path: "" },
    policy: remote?.policy ?? ("keepBoth" as ConflictPolicy),
    autoSyncMinutes: remote?.autoSyncMinutes ?? 0,
  };

  try {
    await api.saveSyncSettings({ ...base, ...change }, secret, vault.id);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

window.addEventListener("hashchange", render);
// Esc and ⌘W close the window, like any settings window in the system.
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") window.close();
});

async function boot(): Promise<void> {
  try {
    settings = await api.getSettings();
  } catch (error) {
    console.error(error);
  }

  // The window language comes from settings, which the main window shares.
  // `i18n` reads its copy in `localStorage` synchronously on load.
  if (settings && settings.locale !== locale) {
    setLocale(settings.locale);
    location.reload();
    return;
  }

  // Keys: the copy in `localStorage` may be old after a hand edit.
  try {
    keymap = await api.getKeymap();
    useKeymap(keymap.keys);
  } catch (error) {
    console.error(error);
  }
  void onKeymapChanged(takeKeymap);
  // A hand edit of the file shows when the user comes back to the window.
  window.addEventListener("focus", () => void api.getKeymap().then(takeKeymap, console.error));

  void onSettingsChanged((incoming) => {
    const languageChanged = settings?.locale !== incoming.locale;
    settings = incoming;
    if (languageChanged) {
      setLocale(incoming.locale);
      location.reload();
      return;
    }
    render();
  });

  // Sync runs without this window too; its state comes as an event.
  void onSyncStatus((incoming) => {
    statuses.set(incoming.vault, incoming);
    if (section() === "vaults") repaintVaults();
  });

  // The same for updates: the menu item opens this window and starts a check,
  // so the first events can come before the window asks.
  void onUpdateStatus((incoming) => {
    update = incoming;
    if (section() === "general") paintUpdate?.();
  });
  try {
    update = await api.updateStatus();
  } catch (error) {
    console.error(error);
  }

  render();
}

void boot();
