/**
 * Popup menu attached to a button.
 *
 * One implementation serves all window menus: vault switcher, theme,
 * language, conflict resolution. It does not know what it shows; callers
 * give it ready items.
 *
 * An item can have a key of its own: while the menu is open, that key runs the
 * item, as in the context menus of macOS apps. The key shows on the right,
 * where a global shortcut shows on other items.
 *
 * Only one menu is open at a time: opening a second one closes the first.
 * The menu lives in `document.body` because its anchor button is in the
 * header or the status bar, and both clip their content.
 */

import { icon, type IconName } from "./icons";

/** Menu item. Everything except the label and the action is optional. */
export interface MenuItem {
  label: string;
  /** Second line: provider, path, last sync time. */
  detail?: string;
  /** Global shortcut shown on the right, for example `⌘,`. */
  hint?: string;
  /**
   * Key that runs the item while the menu is open: a Latin letter, a digit,
   * `,` or `⌫`. It is shown instead of `hint`.
   */
  key?: string;
  glyph?: IconName;
  /** Vault color label: a color name from the set in `styles.css`. */
  color?: string;
  /** Current choice: theme, language, active vault. */
  checked?: boolean;
  disabled?: boolean;
  tone?: "danger" | "warning";
  run: () => void;
}

/** Group of items with an optional title. */
export interface MenuSection {
  title?: string;
  items: MenuItem[];
}

/** The open menu. Close it before opening another one. */
let current: { root: HTMLElement; anchor: HTMLElement } | null = null;

/**
 * Opens the menu below the button (or above it, if the button is at the
 * bottom of the window).
 *
 * A second click on the same button closes the menu, as users expect from
 * toggles.
 */
export function openMenu(anchor: HTMLElement, sections: MenuSection[]): void {
  const sameAnchor = current?.anchor === anchor;
  closeMenu();
  if (sameAnchor) return;

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
      // A group without a title still needs a border from the group above.
      const line = document.createElement("div");
      line.className = "menu-separator";
      line.setAttribute("role", "separator");
      root.append(line);
    }
    for (const item of section.items) root.append(itemButton(item));
  }

  document.body.append(root);
  current = { root, anchor };
  anchor.setAttribute("aria-haspopup", "menu");
  anchor.setAttribute("aria-expanded", "true");

  place(root, anchor);
  root.addEventListener("keydown", onKeyDown);
  // Focus the current choice first: arrow keys move from there.
  const items = enabledItems(root);
  (items.find((button) => button.dataset.checked === "true") ?? items[0])?.focus();

  document.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("resize", closeMenu);
  // The menu has a fixed position at the button; after a scroll it is in the
  // wrong place.
  window.addEventListener("scroll", closeMenu, true);
}

/** Closes the menu and returns focus to the button that opened it. */
export function closeMenu(): void {
  if (!current) return;
  const { root, anchor } = current;
  current = null;

  document.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("resize", closeMenu);
  window.removeEventListener("scroll", closeMenu, true);

  const returnFocus = root.contains(document.activeElement);
  root.remove();
  anchor.setAttribute("aria-expanded", "false");
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
  const shortcut = item.key ?? item.hint;
  if (shortcut) {
    const hint = document.createElement("kbd");
    hint.className = "menu-key";
    hint.textContent = shortcut;
    button.append(hint);
  }

  // Close first, then act: the action can open a dialog, and the menu would
  // stay on top of it.
  button.onclick = () => {
    closeMenu();
    item.run();
  };
  return button;
}

/**
 * Places the menu below the button, or above it if there is no room below.
 *
 * The status bar is at the bottom edge, so its menus always open upward.
 */
function place(root: HTMLElement, anchor: HTMLElement): void {
  const rect = anchor.getBoundingClientRect();
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

/**
 * Physical key of a menu key, as in `KeyboardEvent.code`.
 *
 * The code does not depend on the layout: with the Russian layout on, the key
 * with the Latin "R" still runs the item that shows "R".
 */
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
    // Tab also closes: the next stop is the window, not the next item.
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
    // The item's own key. With a modifier it is a global shortcut, not ours.
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
  // A click on the anchor button does not close here: the button toggles the
  // menu itself, else the menu would close and open again.
  if (current.root.contains(target) || current.anchor.contains(target)) return;
  closeMenu();
}
