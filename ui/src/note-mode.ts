/**
 * Mode of the open note: "Live preview", "Side by side" or "Read".
 *
 * The mode is remembered per file, like the font size: people read a reference
 * note but edit a draft, so one shared setting would get in the way.
 * A note without its own mode opens in the default mode from the settings.
 * It lives in `localStorage` for the same reason as the scale: it is a window
 * convenience, not a property of the note.
 *
 * The key is the same as for the scale and the remembered position.
 */

import type { ViewMode } from "./api";

const STORAGE = "kknow:note-mode";

/** The order in which ⌘E cycles through the modes. */
export const MODES: ViewMode[] = ["edit", "split", "read"];

function isMode(value: unknown): value is ViewMode {
  return MODES.includes(value as ViewMode);
}

function load(): Record<string, ViewMode> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE) ?? "{}");
    return saved && typeof saved === "object" ? saved : {};
  } catch {
    return {};
  }
}

const modes = load();

function store(): void {
  try {
    localStorage.setItem(STORAGE, JSON.stringify(modes));
  } catch {
    // Browser storage can refuse; then the mode lasts until restart.
  }
}

/** `fallback` is for a file without its own mode. */
export function modeOf(key: string, fallback: ViewMode): ViewMode {
  const mode = modes[key];
  return isMode(mode) ? mode : fallback;
}

/**
 * Stores the mode even when it equals the default: a mode picked by hand must
 * not change when the default changes in the settings.
 */
export function rememberMode(key: string, mode: ViewMode): void {
  modes[key] = mode;
  store();
}

/** The file was renamed or moved; its mode moves with it. */
export function moveMode(from: string, to: string): void {
  if (!(from in modes)) return;
  modes[to] = modes[from];
  delete modes[from];
  store();
}

/** The next mode in the cycle, for ⌘E. */
export function nextMode(mode: ViewMode): ViewMode {
  return MODES[(MODES.indexOf(mode) + 1) % MODES.length];
}
