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
 * The window matches keys by `KeyboardEvent.code`, the physical key: with the
 * Russian layout on, ⌘N is still the key with the Latin "N".
 *
 * The module has no DOM and no i18n imports: the Node tests load it as is.
 */

import type { Command, KeyBinding } from "@codemirror/view";

const ACTIONS = {
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

export type Action = keyof typeof ACTIONS;

export const MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** The keys of an action; the first one is the main key. */
export function keysOf(action: Action): readonly string[] {
  return ACTIONS[action];
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
