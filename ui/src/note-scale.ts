/**
 * Font size of the open note.
 *
 * The size is remembered per file: a short summary reads better large, a long
 * reference note reads better dense, and one shared setting fits neither.
 * It lives in `localStorage`, not in the core settings: it is a convenience of
 * this window, not a property of the note, and it must not go to sync.
 *
 * The key is the same as for the remembered position: for a note in a vault,
 * the vault key; for a separate file, its path.
 *
 * A file without its own size takes the default size. It is in the core
 * settings (`noteScale`), because the settings window changes it.
 */

const STORAGE = "kknow:note-scale";

/** Below 70% the text is unreadable; above 200% a line holds three words. */
const STEP = 0.1;
const MIN = 0.7;
const MAX = 2;

/** The same bounds in percents, for the status bar slider. */
export const SCALE_PERCENT = { min: MIN * 100, max: MAX * 100, step: STEP * 100 };

/**
 * Only scales other than the default are stored: new files have no entry
 * anyway. A file at the default size thus follows a new default.
 */
function load(): Record<string, number> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE) ?? "{}");
    return saved && typeof saved === "object" ? saved : {};
  } catch {
    return {};
  }
}

const scales = load();

/** The size of a file without its own size, from the settings. */
let fallback = 1;

/** Takes the default size from the settings, in percents. */
export function setDefaultScale(percent: number): void {
  fallback = Math.min(MAX * 100, Math.max(MIN * 100, Math.round(percent))) / 100;
}

export function defaultScale(): number {
  return fallback;
}

function store(): void {
  try {
    localStorage.setItem(STORAGE, JSON.stringify(scales));
  } catch {
    // Browser storage can refuse; then the scale lasts until restart.
  }
}

/** 1 is 100%; a file without its own size gets the default. */
export function scaleOf(key: string): number {
  const scale = scales[key];
  return typeof scale === "number" && scale >= MIN && scale <= MAX ? scale : fallback;
}

/**
 * Moves the scale by `steps` steps (0 resets it to the default) and stores it.
 *
 * The math uses whole percents: with 0.1 steps, errors like
 * 1.2000000000000002 add up, and a reset would not find exactly 1.
 */
export function stepScale(key: string, steps: number): number {
  const percent =
    steps === 0
      ? Math.round(fallback * 100)
      : Math.round(scaleOf(key) * 100) + Math.round(steps * STEP * 100);
  const scale = Math.min(MAX * 100, Math.max(MIN * 100, percent)) / 100;
  if (scale === fallback) delete scales[key];
  else scales[key] = scale;
  store();
  return scale;
}

/** The file was renamed or moved; its scale moves with it. */
export function moveScale(from: string, to: string): void {
  if (!(from in scales)) return;
  scales[to] = scales[from];
  delete scales[from];
  store();
}

/**
 * A folder was renamed or moved; the scales of all files inside move with it.
 * `from` and `to` are the folder keys, without the trailing slash.
 */
export function moveFolderScale(from: string, to: string): void {
  const prefix = `${from}/`;
  const keys = Object.keys(scales).filter((key) => key.startsWith(prefix));
  if (keys.length === 0) return;
  for (const key of keys) {
    scales[`${to}/${key.slice(prefix.length)}`] = scales[key];
    delete scales[key];
  }
  store();
}
