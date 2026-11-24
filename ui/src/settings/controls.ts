import { t } from "../i18n";
import { SCALE_PERCENT } from "../note-scale";

export function row(label: string, control: HTMLElement, detail?: string): HTMLElement {
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

export function select(
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

export function toggle(on: boolean, onPick: (value: boolean) => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "settings-toggle";
  button.type = "button";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(on));
  button.onclick = () => onPick(button.getAttribute("aria-checked") !== "true");
  return button;
}

export function field(label: string, control: HTMLElement, detail?: string): HTMLElement {
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

export function chips(
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

export function scaleControl(percent: number, onPick: (percent: number) => void): HTMLElement {
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

export function heading(text: string): HTMLElement {
  const title = document.createElement("h2");
  title.className = "settings-heading";
  title.textContent = text;
  return title;
}
