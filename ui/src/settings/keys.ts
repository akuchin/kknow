/** The "Keys" section: the keys of each action and key recording. */

import { defaultKeymap, historyKeymap } from "@codemirror/commands";
import { searchKeymap } from "@codemirror/search";
import type { KeyBinding } from "@codemirror/view";

import { api, type KeymapView } from "../api";
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
import { t, type Key } from "../i18n";
import { heading, row } from "./controls";
import { view } from "./state";

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
  view.render();
}

/** Takes the keys from the core; repaints the page if they changed. */
export function takeKeymap(incoming: KeymapView): void {
  const changed = useKeymap(incoming.keys);
  const problems = JSON.stringify(incoming.problems) !== JSON.stringify(keymap?.problems ?? []);
  keymap = incoming;
  if ((changed || problems) && view.section() === "keys" && !recording) view.render();
}

function startRecording(action: Action): void {
  recording = action;
  keyNotice = null;
  view.render();
}

function stopRecording(): void {
  if (!recording) return;
  recording = null;
  keyNotice = null;
  view.render();
}

/**
 * A key press while an action records. The capture phase comes before the
 * buttons and the window keys, so ⌘, here is a key, not "open settings".
 */
function recordKey(event: KeyboardEvent): void {
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
    view.render();
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
  view.render();
}

/** Listens for the keys of a recording. `main.ts` calls it once, on start. */
export function initKeys(): void {
  document.addEventListener("keydown", recordKey, true);
  // A key press in another app must not land in the recording.
  window.addEventListener("blur", stopRecording);
}

/** Reads the keys: the copy in `localStorage` may be old after a hand edit. */
export async function loadKeymap(): Promise<void> {
  try {
    keymap = await api.getKeymap();
    useKeymap(keymap.keys);
  } catch (error) {
    console.error(error);
  }
}

export function renderKeys(): void {
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

  view.body.append(page);
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
      view.render();
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
