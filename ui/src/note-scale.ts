const STORAGE = "kknow:note-scale";

const STEP = 0.1;
const MIN = 0.7;
const MAX = 2;

export const SCALE_PERCENT = { min: MIN * 100, max: MAX * 100, step: STEP * 100 };

function load(): Record<string, number> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE) ?? "{}");
    return saved && typeof saved === "object" ? saved : {};
  } catch {
    return {};
  }
}

const scales = load();

let fallback = 1;

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
  }
}

export function scaleOf(key: string): number {
  const scale = scales[key];
  return typeof scale === "number" && scale >= MIN && scale <= MAX ? scale : fallback;
}

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

export function moveScale(from: string, to: string): void {
  if (!(from in scales)) return;
  scales[to] = scales[from];
  delete scales[from];
  store();
}

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
