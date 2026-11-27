import type { ViewMode } from "../api";
import { PreviewFind } from "../preview-find";
import { el } from "./dom";
import { editor } from "./editor";
import { state } from "./state";

type Pane = "editor" | "preview";

export const previewFind = new PreviewFind(() => el.preview);
export const trashFind = new PreviewFind(() => el.trashViewBody);

let lastPane: Pane = "editor";

export function resetFindPane(): void {
  lastPane = "editor";
}

export function initFind(): void {
  const editorPane = document.querySelector<HTMLElement>("#editor")!;
  const previewPane = el.preview.closest<HTMLElement>(".preview-pane")!;
  const remember = (pane: Pane) => () => {
    lastPane = pane;
  };
  for (const type of ["pointerdown", "focusin"]) {
    editorPane.addEventListener(type, remember("editor"));
    previewPane.addEventListener(type, remember("preview"));
  }
}

function previewSelected(): boolean {
  const selection = window.getSelection();
  return (
    selection !== null &&
    !selection.isCollapsed &&
    selection.rangeCount > 0 &&
    el.preview.contains(selection.getRangeAt(0).commonAncestorContainer)
  );
}

function previewIsTarget(): boolean {
  if (state.mode === "read") return true;
  if (state.mode === "edit") return false;
  return lastPane === "preview" || previewSelected();
}

export function openFind(): void {
  if (!el.trashView.hidden) {
    previewFind.close();
    trashFind.open();
    return;
  }
  if (previewIsTarget()) previewFind.open();
  else editor.openSearch();
}

export function followMode(next: ViewMode): void {
  if (next === "edit" && previewFind.isOpen()) {
    const { query, ordinal } = previewFind.state();
    previewFind.close();
    editor.openSearch(query);
    editor.selectMatch(ordinal);
  } else if (next === "read" && editor.searchState().open) {
    const { query, ordinal } = editor.searchState();
    editor.closeSearch();
    previewFind.open(query, ordinal);
  }
}
