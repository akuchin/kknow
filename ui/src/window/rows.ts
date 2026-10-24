// Rows of the sidebar: the parts that the tree, the trash and the file list
// share, and the mark of the open note.

import { icon } from "../icons";
import { el } from "./dom";
import { state } from "./state";

/** Marks the open note. Equal names can exist in different vaults. */
export function markActive(id: string | null): void {
  const file = state.note?.file === true;
  for (const button of el.noteList.querySelectorAll<HTMLButtonElement>("button.note")) {
    const mine = !file && button.dataset.vault === state.vault?.id && button.dataset.id === id;
    button.classList.toggle("active", mine);
  }
  for (const button of el.fileList.querySelectorAll<HTMLButtonElement>("button.note")) {
    button.classList.toggle("active", file && button.dataset.path === id);
  }
}

export function emptyItem(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "empty";
  item.textContent = text;
  return item;
}

/**
 * The "⋯" button of a tree row.
 *
 * All three row kinds (vault, folder, note) use it: the menus differ, the
 * button is the same. It appears on hover; a permanent column of dots in the
 * tree is noise.
 */
export function rowMenu(title: string, open: (anchor: HTMLElement) => void): HTMLButtonElement {
  const more = document.createElement("button");
  more.className = "row-menu";
  more.title = title;
  more.setAttribute("aria-label", title);
  more.setAttribute("aria-haspopup", "menu");
  more.append(icon("dots"));
  more.onclick = () => open(more);
  return more;
}

/**
 * The row buttons on the right. They take no width until hover or keyboard
 * focus, so the name gets the whole row.
 */
export function rowActions(...buttons: HTMLButtonElement[]): HTMLSpanElement {
  const actions = document.createElement("span");
  actions.className = "row-actions";
  actions.append(...buttons);
  return actions;
}
