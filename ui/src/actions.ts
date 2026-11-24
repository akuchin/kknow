import type { Command, KeyBinding } from "@codemirror/view";

const DEFAULTS = {
  "palette.open": ["Mod-k", "Mod-Shift-p"],
  "note.new": ["Mod-n"],
  "file.open": ["Mod-o"],
  "note.save": ["Mod-s"],
  "note.find": ["Mod-f"],
  "note.outline": ["Mod-Shift-o"],
  "note.mode": ["Mod-e"],
  "note.comment": ["Mod-Shift-m"],
  "bookmark.toggle": ["Mod-Shift-b"],
  "bookmark.list": ["Mod-Alt-b"],
  "task.list": ["Mod-Alt-t"],
  "sidebar.toggle": ["Mod-\\"],
  "aside.toggle": ["Mod-Alt-p"],
  "history.back": ["Mod-Alt-ArrowLeft"],
  "history.forward": ["Mod-Alt-ArrowRight"],
  "bookmark.next": ["Mod-Alt-ArrowDown"],
  "bookmark.previous": ["Mod-Alt-ArrowUp"],
  "scale.larger": ["Mod-=", "Mod-Shift-=", "Mod-NumpadAdd"],
  "scale.smaller": ["Mod--", "Mod-NumpadSubtract"],
  "scale.reset": ["Mod-0", "Mod-Numpad0"],
  "vault.sync": ["Mod-Shift-s"],
  "settings.open": ["Mod-,"],
  "format.bold": ["Mod-b"],
  "format.italic": ["Mod-i"],
  "format.strike": ["Mod-Shift-x"],
  "link.follow": ["Alt-Enter"],
  "item.up": ["Alt-ArrowUp"],
  "item.down": ["Alt-ArrowDown"],
  "table.rowUp": ["Alt-Shift-ArrowUp"],
  "table.rowDown": ["Alt-Shift-ArrowDown"],
  "table.columnLeft": ["Alt-Shift-ArrowLeft"],
  "table.columnRight": ["Alt-Shift-ArrowRight"],
  "edit.cut": ["Mod-x"],
  "edit.copy": ["Mod-c"],
  "edit.paste": ["Mod-v"],
  "list.previous": ["ArrowUp"],
  "list.next": ["ArrowDown"],
  submit: ["Enter"],
  cancel: ["Escape"],
} satisfies Record<string, readonly string[]>;

export type Action = keyof typeof DEFAULTS;

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

const RESERVED = ["Mod-a", "Mod-z", "Mod-Shift-z", "Mod-y", "Mod-q", "Mod-w", "Mod-h", "Mod-m"];

export type Keymap = Record<string, readonly string[]>;

const MIRROR = "kknow:keymap";

function readMirror(): Keymap {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(MIRROR) ?? "{}");
    return parsed && typeof parsed === "object" ? (parsed as Keymap) : {};
  } catch {
    return {};
  }
}

let custom: Keymap = readMirror();

export const MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

export function keysOf(action: Action): readonly string[] {
  const own = custom[action];
  if (FIXED.has(action) || !Array.isArray(own)) return DEFAULTS[action];
  return own.filter(isKey);
}

export function defaultKeysOf(action: Action): readonly string[] {
  return DEFAULTS[action];
}

export function rebindable(): Exclude<Action, FixedAction>[] {
  return (Object.keys(DEFAULTS) as Action[]).filter(
    (action): action is Exclude<Action, FixedAction> => !FIXED.has(action),
  );
}

export function isCustom(action: Action): boolean {
  return !FIXED.has(action) && Array.isArray(custom[action]);
}

export function customKeys(): Keymap {
  return { ...custom };
}

export function useKeymap(next: Keymap, store = true): boolean {
  const changed = canonical(next) !== canonical(custom);
  custom = { ...next };
  if (changed && store) {
    try {
      localStorage.setItem(MIRROR, JSON.stringify(custom));
    } catch {
      return false;
    }
  }
  return changed;
}

function canonical(keys: Keymap): string {
  return JSON.stringify(Object.entries(keys).sort(([a], [b]) => a.localeCompare(b)));
}

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

export function unbind(...actions: Action[]): Keymap {
  const next: Keymap = { ...custom };
  for (const action of actions) delete next[action];
  return next;
}

function sameList(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((key, index) => key === b[index]);
}

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

export type Conflict =
  | { kind: "action"; action: Exclude<Action, FixedAction> }
  | { kind: "system" }
  | { kind: "vault" }
  | { kind: "editor" };

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
  const ours = rebindable().some((each) => DEFAULTS[each].some(same));
  if (!ours && editor.some(same)) return { kind: "editor" };
  return null;
}

const MODIFIER_CODES = /^(Meta|Control|Alt|Shift|OS)(Left|Right)?$|^CapsLock$|^Fn$/;

export function keyFromEvent(event: KeyboardEvent, mac = MAC): string | null {
  if (!event.code || MODIFIER_CODES.test(event.code)) return null;
  const code = event.code === "NumpadEnter" ? "Enter" : event.code;
  const mods = mac
    ? [event.metaKey && "Mod", event.ctrlKey && "Ctrl"]
    : [event.ctrlKey && "Mod", event.metaKey && "Meta"];
  mods.push(event.altKey && "Alt", event.shiftKey && "Shift");
  return [...mods.filter(Boolean), keyName(code)].join("-");
}

function keyName(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  return Object.entries(CODES).find(([, each]) => each === code)?.[0] ?? code;
}

export function canBind(key: string, mac = MAC): boolean {
  const { code, meta, ctrl, alt } = parse(key, mac);
  return meta || ctrl || alt || /^F([1-9]|1[0-9]|2[0-4])$/.test(code);
}

function isKey(key: unknown): key is string {
  if (typeof key !== "string") return false;
  try {
    parse(key, false);
    return true;
  } catch {
    return false;
  }
}

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

export function matches(event: KeyboardEvent, action: Action, mac = MAC): boolean {
  return keysOf(action).some((key) => matchesKey(event, key, mac));
}

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

function nameOf(code: string, mac: boolean): string {
  const own = ARROWS[code] ?? (mac ? MAC_NAMES : OTHER_NAMES)[code];
  if (own) return own;
  const symbol = Object.entries(CODES).find(([, each]) => each === code)?.[0];
  if (symbol) return symbol;
  return code.replace(/^(Key|Digit|Numpad)/, "");
}

const LONE: Record<string, [string, string]> = {
  Shift: ["⇧", "Shift"],
  Alt: ["⌥", "Alt"],
  Ctrl: ["⌃", "Ctrl"],
};

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

export function shortcut(action: Action, mac = MAC): string {
  const [key] = keysOf(action);
  return key ? formatKey(key, mac) : "";
}

export function withShortcut(text: string, ...actions: Action[]): string {
  const keys = actions.map((action) => shortcut(action)).filter(Boolean);
  return keys.length ? `${text} (${keys.join(" / ")})` : text;
}

export function formKeys(): { submit: string; cancel: string } {
  return { submit: shortcut("submit"), cancel: shortcut("cancel") };
}
