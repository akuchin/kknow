/**
 * The editor gutter: a narrow column of markers and, if the user wants them,
 * line numbers.
 *
 * The marker column shows bookmarks (`bookmarks.ts`) and agent notes
 * (`agent-notes.ts`). It is always there, also without markers: if it came
 * and went with the first bookmark, all text would jump sideways. Its width
 * comes out of the left text padding, so the text stays where it was before
 * the gutter existed.
 *
 * A click in the marker column toggles the bookmark of the line, so a click
 * on the ribbon removes it. A click on the note icon opens the note popup.
 *
 * Line numbers are off by default. They are a window setting, not part of
 * the document, so they live in a compartment: the editor turns them on and
 * off without a new state, and the undo history stays.
 */

import { Compartment, type Extension } from "@codemirror/state";
import { EditorView, gutter, lineNumbers } from "@codemirror/view";

import { noteMarkers, openNoteOnLine } from "./agent-notes";
import { bookmarkMarkers, bookmarksEnabled, toggleBookmarkAt } from "./bookmarks";

/** Height of one text line: the gutter content aligns with the first line. */
const LINE = "calc(15px * var(--note-scale, 1) * 1.65)";

const numbers = new Compartment();

const marks = gutter({
  class: "cm-marks",
  // Bookmarks first: on a line with both, the ribbon is on the left.
  markers: (view) => [bookmarkMarkers(view.state), noteMarkers(view.state)],
  // Empty cells are needed for the hover hint on lines without a bookmark.
  renderEmptyElements: true,
  domEventHandlers: {
    mousedown(view, block, event) {
      const mouse = event as MouseEvent;
      // Right button and Control+click open the context menu, not a toggle.
      if (mouse.button !== 0 || mouse.ctrlKey) return false;
      const line = view.state.doc.lineAt(block.from).number;
      const target = mouse.target as Element | null;
      if (target?.closest(".cm-comment-gutter")) return openNoteOnLine(view, line);
      toggleBookmarkAt(view, line);
      // `true` cancels the press: the cursor and the focus stay in the text.
      return true;
    },
  },
});

/** The hover hint shows only where a click can set a bookmark. */
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
  // The line number is smaller than the text, so its line height is in the
  // text units: then both sit on the same baseline.
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
  // A faint ribbon on hover tells that a click sets a bookmark here. Over the
  // note icon a click opens the note, so the hint hides.
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

/** The gutter with line numbers on or off. */
export function editorGutter(withNumbers: boolean): Extension {
  return [marks, numbers.of(withNumbers ? lineNumbers() : []), bookmarkable, theme];
}

export function setLineNumbers(view: EditorView, on: boolean): void {
  view.dispatch({ effects: numbers.reconfigure(on ? lineNumbers() : []) });
}
