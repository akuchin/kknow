import type { ViewMode } from "./api";

const STORAGE = "kknow:note-mode";

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
  }
}

export function modeOf(key: string, fallback: ViewMode): ViewMode {
  const mode = modes[key];
  return isMode(mode) ? mode : fallback;
}

export function rememberMode(key: string, mode: ViewMode): void {
  modes[key] = mode;
  store();
}

export function moveMode(from: string, to: string): void {
  if (!(from in modes)) return;
  modes[to] = modes[from];
  delete modes[from];
  store();
}

export function moveFolderMode(from: string, to: string): void {
  const prefix = `${from}/`;
  const keys = Object.keys(modes).filter((key) => key.startsWith(prefix));
  if (keys.length === 0) return;
  for (const key of keys) {
    modes[`${to}/${key.slice(prefix.length)}`] = modes[key];
    delete modes[key];
  }
  store();
}

export function nextMode(mode: ViewMode): ViewMode {
  return MODES[(MODES.indexOf(mode) + 1) % MODES.length];
}
