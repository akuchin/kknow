import { formatKey, shortcut, type Action } from "./actions";
import { icon, type IconName } from "./icons";

export interface MenuItem {
  label: string;
  detail?: string;
  action?: Action;
  hint?: string;
  key?: string;
  glyph?: IconName;
  color?: string;
  checked?: boolean;
  disabled?: boolean;
  tone?: "danger" | "warning";
  run: () => void;
}

export interface MenuSection {
  title?: string;
  items: MenuItem[];
}

let current: { root: HTMLElement; anchor: HTMLElement; toggle: boolean } | null = null;

export function openMenu(anchor: HTMLElement, sections: MenuSection[]): void {
  const sameAnchor = current?.anchor === anchor;
  closeMenu();
  if (sameAnchor) return;
  show(sections, anchor, anchor.getBoundingClientRect(), true);
  anchor.setAttribute("aria-haspopup", "menu");
  anchor.setAttribute("aria-expanded", "true");
}

export function openMenuAt(x: number, y: number, focus: HTMLElement, sections: MenuSection[]): void {
  closeMenu();
  show(sections, focus, new DOMRect(x, y, 0, 0), false);
}

function show(sections: MenuSection[], anchor: HTMLElement, rect: DOMRect, toggle: boolean): void {
  const root = document.createElement("div");
  root.className = "menu";
  root.setAttribute("role", "menu");

  for (const section of sections) {
    if (section.items.length === 0) continue;
    if (section.title) {
      const title = document.createElement("div");
      title.className = "menu-title";
      title.textContent = section.title;
      root.append(title);
    } else if (root.childElementCount > 0) {
      const line = document.createElement("div");
      line.className = "menu-separator";
      line.setAttribute("role", "separator");
      root.append(line);
    }
    for (const item of section.items) root.append(itemButton(item));
  }

  document.body.append(root);
  current = { root, anchor, toggle };

  place(root, rect);
  root.addEventListener("keydown", onKeyDown);
  const items = enabledItems(root);
  (items.find((button) => button.dataset.checked === "true") ?? items[0])?.focus();

  document.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("resize", closeMenu);
  window.addEventListener("scroll", closeMenu, true);
}

export function closeMenu(): void {
  if (!current) return;
  const { root, anchor, toggle } = current;
  current = null;

  document.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("resize", closeMenu);
  window.removeEventListener("scroll", closeMenu, true);

  const returnFocus = root.contains(document.activeElement);
  root.remove();
  if (toggle) anchor.setAttribute("aria-expanded", "false");
  if (returnFocus) anchor.focus();
}

function itemButton(item: MenuItem): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "menu-item";
  button.type = "button";
  button.setAttribute("role", "menuitem");
  button.disabled = Boolean(item.disabled);
  if (item.checked) {
    button.dataset.checked = "true";
    button.setAttribute("aria-checked", "true");
  }
  if (item.tone) button.dataset.tone = item.tone;

  if (item.color) {
    const label = document.createElement("span");
    label.className = "vault-label";
    label.dataset.color = item.color;
    button.append(label);
  } else if (item.glyph) {
    button.append(icon(item.glyph));
  }

  const text = document.createElement("span");
  text.className = "menu-text";

  const label = document.createElement("span");
  label.className = "menu-label";
  label.textContent = item.label;
  text.append(label);

  if (item.detail) {
    const detail = document.createElement("span");
    detail.className = "menu-detail";
    detail.textContent = item.detail;
    text.append(detail);
  }
  button.append(text);

  if (item.key) button.dataset.key = keyCode(item.key);
  const shown = item.key
    ? formatKey(keyCode(item.key))
    : (item.hint ?? (item.action ? shortcut(item.action) : ""));
  if (shown) {
    const hint = document.createElement("kbd");
    hint.className = "menu-key";
    hint.textContent = shown;
    button.append(hint);
  }

  button.onclick = () => {
    closeMenu();
    item.run();
  };
  return button;
}

function place(root: HTMLElement, rect: DOMRect): void {
  const size = root.getBoundingClientRect();
  const margin = 6;

  const left = Math.max(
    margin,
    Math.min(rect.left, window.innerWidth - size.width - margin),
  );
  root.style.left = `${left}px`;

  if (rect.bottom + size.height + margin < window.innerHeight) {
    root.style.top = `${rect.bottom + 4}px`;
  } else {
    root.style.top = `${Math.max(margin, rect.top - size.height - 4)}px`;
  }
}

function keyCode(key: string): string {
  if (/^[a-z]$/i.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  if (key === ",") return "Comma";
  if (key === "⌫") return "Backspace";
  return key;
}

function enabledItems(root: HTMLElement): HTMLButtonElement[] {
  return [...root.querySelectorAll<HTMLButtonElement>(".menu-item:not(:disabled)")];
}

function onKeyDown(event: KeyboardEvent): void {
  if (!current) return;
  const items = enabledItems(current.root);
  const index = items.indexOf(document.activeElement as HTMLButtonElement);

  if (event.key === "Escape" || event.key === "Tab") {
    event.preventDefault();
    closeMenu();
  } else if (event.key === "ArrowDown") {
    event.preventDefault();
    items[(index + 1) % items.length]?.focus();
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    items[(index - 1 + items.length) % items.length]?.focus();
  } else if (event.key === "Home") {
    event.preventDefault();
    items[0]?.focus();
  } else if (event.key === "End") {
    event.preventDefault();
    items[items.length - 1]?.focus();
  } else if (!event.metaKey && !event.ctrlKey && !event.altKey) {
    const item = items.find((button) => button.dataset.key === event.code);
    if (item) {
      event.preventDefault();
      item.click();
    }
  }
}

function onPointerDown(event: PointerEvent): void {
  if (!current) return;
  const target = event.target as Node;
  if (current.root.contains(target)) return;
  if (current.toggle && current.anchor.contains(target)) return;
  closeMenu();
}
