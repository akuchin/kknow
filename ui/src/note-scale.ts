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
 */

const STORAGE = "kknow:note-scale";

/** Below 70% the text is unreadable; above 200% a line holds three words. */
const STEP = 0.1;
const MIN = 0.7;
const MAX = 2;

/** The same bounds in percents, for the status bar slider. */
export const SCALE_PERCENT = { min: MIN * 100, max: MAX * 100, step: STEP * 100 };

/** Only scales other than 100% are stored: new files have no entry anyway. */
function load(): Record<string, number> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE) ?? "{}");
    return saved && typeof saved === "object" ? saved : {};
  } catch {
    return {};
  }
}

const scales = load();

function store(): void {
  try {
    localStorage.setItem(STORAGE, JSON.stringify(scales));
  } catch {
    // Browser storage can refuse; then the scale lasts until restart.
  }
}

/** 1 is the normal size. */
export function scaleOf(key: string): number {
  const scale = scales[key];
  return typeof scale === "number" && scale >= MIN && scale <= MAX ? scale : 1;
}

/**
 * Moves the scale by `steps` steps (0 resets it) and stores it.
 *
 * The math uses whole percents: with 0.1 steps, errors like
 * 1.2000000000000002 add up, and a reset would not find exactly 1.
 */
export function stepScale(key: string, steps: number): number {
  const percent =
    steps === 0 ? 100 : Math.round(scaleOf(key) * 100) + Math.round(steps * STEP * 100);
  const scale = Math.min(MAX * 100, Math.max(MIN * 100, percent)) / 100;
  if (scale === 1) delete scales[key];
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

/** Label, for example "120%". */
export function scaleLabel(scale: number): string {
  return `${Math.round(scale * 100)}%`;
}
