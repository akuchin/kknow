import { Compartment, type Extension } from "@codemirror/state";
import { EditorView, gutter, lineNumbers } from "@codemirror/view";

import { noteMarkers, openNoteOnLine } from "./agent-notes";
import { bookmarkMarkers, bookmarksEnabled, toggleBookmarkAt } from "./bookmarks";

const LINE = "calc(15px * var(--note-scale, 1) * 1.65)";

const numbers = new Compartment();

const marks = gutter({
  class: "cm-marks",
  markers: (view) => [bookmarkMarkers(view.state), noteMarkers(view.state)],
  renderEmptyElements: true,
  domEventHandlers: {
    mousedown(view, block, event) {
      const mouse = event as MouseEvent;
      if (mouse.button !== 0 || mouse.ctrlKey) return false;
      const line = view.state.doc.lineAt(block.from).number;
      const target = mouse.target as Element | null;
      if (target?.closest(".cm-comment-gutter")) return openNoteOnLine(view, line);
      toggleBookmarkAt(view, line);
      return true;
    },
  },
});

const bookmarkable = EditorView.editorAttributes.of((view) =>
  bookmarksEnabled(view.state) ? { "data-bookmarks": "on" } : null,
);

const theme = EditorView.theme({
  ".cm-gutters": {
    backgroundColor: "transparent",
    color: "var(--text-muted)",
    border: "none",
  },
  ".cm-gutterElement": { lineHeight: LINE },

  ".cm-lineNumbers .cm-gutterElement": {
    minWidth: "2ch",
    padding: "0 2px 0 10px",
    fontFamily: "var(--font-mono)",
    fontSize: "0.75em",
    fontVariantNumeric: "tabular-nums",
    color: "color-mix(in oklab, var(--text-muted) 75%, transparent)",
  },
  ".cm-lineNumbers .cm-gutterElement.cm-gutterElement": { lineHeight: LINE },

  ".cm-marks .cm-gutterElement": {
    boxSizing: "border-box",
    width: "22px",
    display: "flex",
    justifyContent: "center",
    alignItems: "flex-start",
    gap: "2px",
    cursor: "pointer",
  },
  ".cm-bookmark-gutter, .cm-marks .cm-gutterElement::before": {
    flex: "none",
    width: "7px",
    height: "0.9em",
    marginTop: "calc((1.65em - 0.9em) / 2)",
    backgroundColor: "var(--accent)",
    clipPath: "polygon(0 0, 100% 0, 100% 100%, 50% 72%, 0 100%)",
  },
  "&[data-bookmarks=on] .cm-marks .cm-gutterElement:hover:not(:has(.cm-bookmark-gutter)):not(:has(.cm-comment-gutter:hover))::before":
    {
      content: '""',
      opacity: "0.3",
    },
  ".cm-marks .cm-gutterElement::before": { order: "-1" },
  ".cm-comment-gutter": {
    display: "flex",
    height: LINE,
    alignItems: "center",
    color: "var(--warning)",
  },
  ".cm-comment-gutter .icon": { width: "11px", height: "11px" },
  ".cm-comment-gutter:hover": { color: "var(--text)" },
});

export function editorGutter(withNumbers: boolean): Extension {
  return [marks, numbers.of(withNumbers ? lineNumbers() : []), bookmarkable, theme];
}

export function setLineNumbers(view: EditorView, on: boolean): void {
  view.dispatch({ effects: numbers.reconfigure(on ? lineNumbers() : []) });
}
