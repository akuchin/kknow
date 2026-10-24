/**
 * Actions and their keys.
 *
 * An action is a named thing the user can do: "new note", "next mode",
 * "move the row up". The keys of each action live here, in one list. The
 * window handler, the CodeMirror keymaps and every hint (tooltips, menus, the
 * palette, the settings page) read them from this list. So a key changes in
 * one place, and the locales have no keys: a label is the action name, and the
 * code adds the key that this system shows.
 *
 * A key is written in the CodeMirror format: `Mod-Shift-b`, `Mod-Alt-ArrowLeft`,
 * `Mod--`. `Mod` is ⌘ on macOS and Ctrl on Windows and Linux. The first key of
 * an action is the one that hints show; the others are aliases.
 *
 * The list here is the default. The user changes keys in the settings or in
 * `keybindings.json`; the core reads the file, and `useKeymap` lays it over
 * the defaults. The window reads keys synchronously while it builds, so the
 * changes have a copy in `localStorage`, like the language.
 *
 * The window matches keys by `KeyboardEvent.code`, the physical key: with the
 * Russian layout on, ⌘N is still the key with the Latin "N".
 *
 * The module has no DOM and no i18n imports: the Node tests load it as is.
 */

import type { Command, KeyBinding } from "@codemirror/view";

const DEFAULTS = {
  "palette.open": ["Mod-k", "Mod-Shift-p"],
  "note.new": ["Mod-n"],
  "file.open": ["Mod-o"],
  "note.save": ["Mod-s"],
  "note.find": ["Mod-f"],
  "note.mode": ["Mod-e"],
  "note.comment": ["Mod-Shift-m"],
  "bookmark.toggle": ["Mod-Shift-b"],
  "bookmark.list": ["Mod-Alt-b"],
  "sidebar.toggle": ["Mod-\\"],
  "history.back": ["Mod-Alt-ArrowLeft"],
  "history.forward": ["Mod-Alt-ArrowRight"],
  // ⌘= and ⌘+ are one key (plus needs Shift), so Shift is not required.
  "scale.larger": ["Mod-=", "Mod-Shift-=", "Mod-NumpadAdd"],
  "scale.smaller": ["Mod--", "Mod-NumpadSubtract"],
  "scale.reset": ["Mod-0", "Mod-Numpad0"],
  "vault.sync": ["Mod-Shift-s"],
  "settings.open": ["Mod-,"],
  "format.bold": ["Mod-b"],
  "format.italic": ["Mod-i"],
  "format.strike": ["Mod-Shift-x"],
  "table.rowUp": ["Alt-Shift-ArrowUp"],
  "table.rowDown": ["Alt-Shift-ArrowDown"],
  "table.columnLeft": ["Alt-Shift-ArrowLeft"],
  "table.columnRight": ["Alt-Shift-ArrowRight"],
  // The system and the browser handle these; the menus only show them.
  "edit.cut": ["Mod-x"],
  "edit.copy": ["Mod-c"],
  "edit.paste": ["Mod-v"],
  // Keys of a list or a text field.
  "list.previous": ["ArrowUp"],
  "list.next": ["ArrowDown"],
  submit: ["Enter"],
  cancel: ["Escape"],
} satisfies Record<string, readonly string[]>;

export type Action = keyof typeof DEFAULTS;

/**
 * Actions whose keys do not change: the system, the browser or a text field
 * handles them, and the app only shows them.
 */
const FIXED_ACTIONS = [
  "edit.cut",
  "edit.copy",
  "edit.paste",
  "list.previous",
  "list.next",
  "submit",
  "cancel",
] as const satisfies readonly Action[];

export type FixedAction = (typeof FIXED_ACTIONS)[number];

const FIXED: ReadonlySet<Action> = new Set<Action>(FIXED_ACTIONS);

/**
 * Keys that no action may take: select all and undo in a text field, and the
 * macOS keys that quit, close, hide and minimize the window.
 */
const RESERVED = ["Mod-a", "Mod-z", "Mod-Shift-z", "Mod-y", "Mod-q", "Mod-w", "Mod-h", "Mod-m"];

/** Action ID → its keys: only the actions that the user changed. */
export type Keymap = Record<string, readonly string[]>;

/** The `localStorage` copy of the changed keys. */
const MIRROR = "kknow:keymap";

function readMirror(): Keymap {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(MIRROR) ?? "{}");
    return parsed && typeof parsed === "object" ? (parsed as Keymap) : {};
  } catch {
    // No `localStorage` in the Node tests, or a broken copy: the defaults.
    return {};
  }
}

let custom: Keymap = readMirror();

export const MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** The keys of an action; the first one is the main key. */
export function keysOf(action: Action): readonly string[] {
  const own = custom[action];
  if (FIXED.has(action) || !Array.isArray(own)) return DEFAULTS[action];
  return own.filter(isKey);
}

/** The keys of an action before the user changed them. */
export function defaultKeysOf(action: Action): readonly string[] {
  return DEFAULTS[action];
}

/** The actions whose keys the user can change, in the list order. */
export function rebindable(): Exclude<Action, FixedAction>[] {
  return (Object.keys(DEFAULTS) as Action[]).filter(
    (action): action is Exclude<Action, FixedAction> => !FIXED.has(action),
  );
}

/** Did the user change the keys of this action? */
export function isCustom(action: Action): boolean {
  return !FIXED.has(action) && Array.isArray(custom[action]);
}

/** The changed keys as they are, also of actions this version does not know. */
export function customKeys(): Keymap {
  return { ...custom };
}

/**
 * Takes the changed keys from the core. Returns `true` if they differ from
 * the ones in use: the main window then reloads, because hints are spread
 * over the markup, like labels.
 */
export function useKeymap(next: Keymap, store = true): boolean {
  const changed = canonical(next) !== canonical(custom);
  custom = { ...next };
  if (changed && store) {
    try {
      localStorage.setItem(MIRROR, JSON.stringify(custom));
    } catch {
      // No storage: a reload would read the old copy again and reload
      // forever. The keys work from memory; only the hints are old.
      return false;
    }
  }
  return changed;
}

/** A form that does not depend on the order of the entries. */
function canonical(keys: Keymap): string {
  return JSON.stringify(Object.entries(keys).sort(([a], [b]) => a.localeCompare(b)));
}

/**
 * The changed keys after the action gets this key. The key leaves every other
 * action that has it: one key runs one action. An action that ends with its
 * default keys drops out of the list.
 */
export function bind(action: Action, key: string, mac = MAC): Keymap {
  const next: Keymap = { ...custom };
  for (const other of rebindable()) {
    if (other === action) continue;
    const keys = keysOf(other);
    if (keys.some((each) => sameKey(each, key, mac))) {
      next[other] = keys.filter((each) => !sameKey(each, key, mac));
    }
  }
  next[action] = [key];
  for (const each of rebindable()) {
    if (Array.isArray(next[each]) && sameList(next[each], DEFAULTS[each])) delete next[each];
  }
  return next;
}

/** The changed keys without these actions: they get their default keys back. */
export function unbind(...actions: Action[]): Keymap {
  const next: Keymap = { ...custom };
  for (const action of actions) delete next[action];
  return next;
}

function sameList(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((key, index) => key === b[index]);
}

/** Do two keys press the same thing? `Mod-k` and `Cmd-k` do on macOS. */
export function sameKey(a: string, b: string, mac = MAC): boolean {
  const one = parse(a, mac);
  const two = parse(b, mac);
  return (
    one.code === two.code &&
    one.meta === two.meta &&
    one.ctrl === two.ctrl &&
    one.alt === two.alt &&
    one.shift === two.shift
  );
}

/** Who already has the key. */
export type Conflict =
  | { kind: "action"; action: Exclude<Action, FixedAction> }
  /** The system or a text field: see `RESERVED` and `FIXED`. */
  | { kind: "system" }
  /** ⌘1…⌘9. */
  | { kind: "vault" }
  /** An editor command: indentation, comments, search. */
  | { kind: "editor" };

/**
 * Checks a key for the action. `editor` is the keys of the editor's own
 * commands. Another action's key is a conflict that the user can resolve:
 * the key moves. The others cannot move.
 */
export function conflictOf(
  key: string,
  action: Action,
  editor: readonly string[] = [],
  mac = MAC,
): Conflict | null {
  const same = (each: string) => sameKey(each, key, mac);
  const fixed = [...FIXED].flatMap((each) => DEFAULTS[each]);
  if (RESERVED.some(same) || fixed.some(same)) return { kind: "system" };
  for (let number = 1; number <= 9; number++) {
    if (same(vaultKey(number))) return { kind: "vault" };
  }
  const other = rebindable().find((each) => each !== action && keysOf(each).some(same));
  if (other) return { kind: "action", action: other };
  // The default keys of the actions win over the editor commands: ⌘I is
  // italic, not "select the parent node". So such a key is free.
  const ours = rebindable().some((each) => DEFAULTS[each].some(same));
  if (!ours && editor.some(same)) return { kind: "editor" };
  return null;
}

/** Modifier keys: pressed alone, they are not a key yet. */
const MODIFIER_CODES = /^(Meta|Control|Alt|Shift|OS)(Left|Right)?$|^CapsLock$|^Fn$/;

/**
 * The key that the event presses, in the CodeMirror format. `null` while only
 * modifiers are down. On macOS ⌘ is `Mod` and ⌃ is `Ctrl`; elsewhere Ctrl is
 * `Mod` and the Windows key is `Meta`.
 */
export function keyFromEvent(event: KeyboardEvent, mac = MAC): string | null {
  if (!event.code || MODIFIER_CODES.test(event.code)) return null;
  const code = event.code === "NumpadEnter" ? "Enter" : event.code;
  const mods = mac
    ? [event.metaKey && "Mod", event.ctrlKey && "Ctrl"]
    : [event.ctrlKey && "Mod", event.metaKey && "Meta"];
  mods.push(event.altKey && "Alt", event.shiftKey && "Shift");
  return [...mods.filter(Boolean), keyName(code)].join("-");
}

/** `KeyB` → `b`, `Digit1` → `1`, `Backslash` → `\`; other codes stay. */
function keyName(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  return Object.entries(CODES).find(([, each]) => each === code)?.[0] ?? code;
}

/**
 * Can a key run an action? A plain letter or Shift with a letter types text,
 * so a key needs ⌘, Ctrl or ⌥. Function keys type nothing and need nothing.
 */
export function canBind(key: string, mac = MAC): boolean {
  const { code, meta, ctrl, alt } = parse(key, mac);
  return meta || ctrl || alt || /^F([1-9]|1[0-9]|2[0-4])$/.test(code);
}

/** Does the key parse? A hand-edited file can have anything. */
function isKey(key: unknown): key is string {
  if (typeof key !== "string") return false;
  try {
    parse(key, false);
    return true;
  } catch {
    return false;
  }
}

/** ⌘1…⌘9 pick a vault by its place in the list. */
export function vaultKey(number: number): string {
  return `Mod-${number}`;
}

interface Parsed {
  code: string;
  meta: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
}

/** Characters that are written as themselves, by their `code`. */
const CODES: Record<string, string> = {
  "\\": "Backslash",
  ",": "Comma",
  ".": "Period",
  "/": "Slash",
  "=": "Equal",
  "-": "Minus",
  ";": "Semicolon",
  "'": "Quote",
  "`": "Backquote",
  "[": "BracketLeft",
  "]": "BracketRight",
};

/** `Mod-Shift-b` → the code and the modifiers. A dash at the end is the key. */
function parse(key: string, mac: boolean): Parsed {
  const parts = key.split(/-(?!$)/);
  const name = parts.pop()!;
  const result: Parsed = { code: codeOf(name), meta: false, ctrl: false, alt: false, shift: false };
  for (const part of parts) {
    if (part === "Mod") result[mac ? "meta" : "ctrl"] = true;
    else if (part === "Cmd" || part === "Meta") result.meta = true;
    else if (part === "Ctrl") result.ctrl = true;
    else if (part === "Alt") result.alt = true;
    else if (part === "Shift") result.shift = true;
    else throw new Error(`unknown modifier in ${key}`);
  }
  return result;
}

function codeOf(name: string): string {
  if (/^[a-z]$/i.test(name)) return `Key${name.toUpperCase()}`;
  if (/^[0-9]$/.test(name)) return `Digit${name}`;
  return CODES[name] ?? name;
}

/** Does the event press this key? The modifiers must match exactly. */
export function matchesKey(event: KeyboardEvent, key: string, mac = MAC): boolean {
  const want = parse(key, mac);
  const code = event.code === "NumpadEnter" ? "Enter" : event.code;
  return (
    code === want.code &&
    event.metaKey === want.meta &&
    event.ctrlKey === want.ctrl &&
    event.altKey === want.alt &&
    event.shiftKey === want.shift
  );
}

/** Does the event press any key of the action? */
export function matches(event: KeyboardEvent, action: Action, mac = MAC): boolean {
  return keysOf(action).some((key) => matchesKey(event, key, mac));
}

/** The CodeMirror bindings of an action: one per key. */
export function bindings(action: Action, run: Command): KeyBinding[] {
  return keysOf(action).map((key) => ({ key, run }));
}

const MAC_NAMES: Record<string, string> = {
  Minus: "−",
  NumpadSubtract: "−",
  NumpadAdd: "+",
  Enter: "↵",
  Escape: "Esc",
  Backspace: "⌫",
  Delete: "⌦",
  Tab: "⇥",
  Space: "Space",
};

const OTHER_NAMES: Record<string, string> = {
  Minus: "−",
  NumpadSubtract: "−",
  NumpadAdd: "+",
  Escape: "Esc",
};

const ARROWS: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
};

/** The name of one key without modifiers, as this system writes it. */
function nameOf(code: string, mac: boolean): string {
  const own = ARROWS[code] ?? (mac ? MAC_NAMES : OTHER_NAMES)[code];
  if (own) return own;
  const symbol = Object.entries(CODES).find(([, each]) => each === code)?.[0];
  if (symbol) return symbol;
  return code.replace(/^(Key|Digit|Numpad)/, "");
}

/** Modifiers on their own: macOS, other systems. */
const LONE: Record<string, [string, string]> = {
  Shift: ["⇧", "Shift"],
  Alt: ["⌥", "Alt"],
  Ctrl: ["⌃", "Ctrl"],
};

/**
 * A key as the system writes it: `⇧⌘B` on macOS (in the Apple order: ⌃⌥⇧⌘),
 * `Ctrl+Shift+B` on Windows and Linux. A lone modifier (`Shift`) is the
 * modifier itself: the tables hint at Shift with a click.
 */
export function formatKey(key: string, mac = MAC): string {
  const lone = LONE[key];
  if (lone) return lone[mac ? 0 : 1];

  const { code, meta, ctrl, alt, shift } = parse(key, mac);
  const name = nameOf(code, mac);
  if (mac) {
    return `${ctrl ? "⌃" : ""}${alt ? "⌥" : ""}${shift ? "⇧" : ""}${meta ? "⌘" : ""}${name}`;
  }
  const mods = [ctrl && "Ctrl", alt && "Alt", shift && "Shift", meta && "Win"].filter(Boolean);
  return [...mods, name].join("+");
}

/** The main key of an action, ready to show; empty if the action has none. */
export function shortcut(action: Action, mac = MAC): string {
  const [key] = keysOf(action);
  return key ? formatKey(key, mac) : "";
}

/**
 * A tooltip with the keys: `Back (⌥⌘←)`. Several actions show their main
 * keys with a slash. Actions without keys add nothing.
 */
export function withShortcut(text: string, ...actions: Action[]): string {
  const keys = actions.map((action) => shortcut(action)).filter(Boolean);
  return keys.length ? `${text} (${keys.join(" / ")})` : text;
}

/** The keys of a text field, for hints like "↵ to save, Esc to cancel". */
export function formKeys(): { submit: string; cancel: string } {
  return { submit: shortcut("submit"), cancel: shortcut("cancel") };
}
