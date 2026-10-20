/**
 * Bookmarks in the editor: the line marker and how it moves with edits.
 *
 * The core stores bookmarks (`.kknow/bookmarks.json`), but the editor knows
 * where they are now. Edits shift lines, so the line number from the open
 * call soon becomes wrong. That is why the state keeps text positions, not
 * numbers, and each edit maps them. Line numbers come from the positions on
 * request; on save they go to the core.
 */

import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, type ViewUpdate } from "@codemirror/view";

/** Replaces all bookmarks. Line numbers start at 1. */
const setLines = StateEffect.define<number[]>();

/**
 * The user set or removed a bookmark. It comes together with `setLines`. The
 * window uses it to tell this from a note load and sends bookmarks to the core.
 */
const toggled = StateEffect.define<{ line: number; set: boolean }>();

/** Whether bookmarks are allowed: standalone files have none. */
const setEnabled = StateEffect.define<boolean>();

const enabled = StateField.define<boolean>({
  create: () => false,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setEnabled)) return effect.value;
    }
    return value;
  },
});

/** Bookmark positions: the line start at the time the bookmark was set. */
const positions = StateField.define<number[]>({
  create: () => [],
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setLines)) return toPositions(tr.state, effect.value);
    }
    if (!tr.docChanged) return value;
    // `1`: an insert exactly at the position pushes the position right. So
    // Enter at the start of a bookmarked line moves the bookmark with the
    // text and does not leave it on the new empty line.
    return value.map((pos) => tr.changes.mapPos(pos, 1));
  },
});

const mark = Decoration.line({ class: "cm-bookmark" });

const decorations = EditorView.decorations.compute([positions], (state) =>
  Decoration.set(
    linesOf(state).map((line) => mark.range(state.doc.line(line).from)),
  ),
);

const theme = EditorView.baseTheme({
  // The marker sits in the left text margin. The editor has no gutter, and
  // adding one only for bookmarks would shift all text.
  ".cm-line.cm-bookmark": { position: "relative" },
  ".cm-line.cm-bookmark::before": {
    content: '""',
    position: "absolute",
    left: "-18px",
    top: "0.35em",
    width: "8px",
    height: "0.9em",
    backgroundColor: "var(--accent)",
    clipPath: "polygon(0 0, 100% 0, 100% 100%, 50% 72%, 0 100%)",
  },
});

function toPositions(state: EditorState, lines: number[]): number[] {
  return lines
    .filter((line) => line >= 1 && line <= state.doc.lines)
    .map((line) => state.doc.line(line).from);
}

/** Sorted bookmark lines without duplicates: lines merged by an edit count once. */
function linesOf(state: EditorState): number[] {
  const lines = state.field(positions).map((pos) => state.doc.lineAt(pos).number);
  return [...new Set(lines)].sort((a, b) => a - b);
}

export function bookmarks(): Extension {
  return [enabled, positions, decorations, theme];
}

export function bookmarksEnabled(state: EditorState): boolean {
  return state.field(enabled, false) ?? false;
}

export function hasBookmark(state: EditorState, line: number): boolean {
  return linesOf(state).includes(line);
}

/** The bookmark set or removed in this update, or `null`. */
export function bookmarkToggled(update: ViewUpdate): { line: number; set: boolean } | null {
  for (const tr of update.transactions) {
    for (const effect of tr.effects) {
      if (effect.is(toggled)) return effect.value;
    }
  }
  return null;
}

export function bookmarkLines(state: EditorState): number[] {
  return linesOf(state);
}

/** Bookmarks of the open note; `on` tells whether bookmarks are allowed. */
export function setBookmarkLines(view: EditorView, lines: number[], on: boolean): void {
  view.dispatch({ effects: [setEnabled.of(on), setLines.of(on ? lines : [])] });
}

/**
 * Sets or removes the bookmark on `line`. The window sends bookmarks to the
 * core: it learns about the change from `bookmarkToggled`.
 */
export function toggleBookmarkAt(view: EditorView, line: number): void {
  const { state } = view;
  if (!bookmarksEnabled(state)) return;
  const lines = linesOf(state);
  const set = !lines.includes(line);
  const next = set ? [...lines, line].sort((a, b) => a - b) : lines.filter((each) => each !== line);
  view.dispatch({ effects: [setLines.of(next), toggled.of({ line, set })] });
}

/** Toggles the bookmark on the cursor line. */
export function toggleBookmark(view: EditorView): void {
  const { state } = view;
  toggleBookmarkAt(view, state.doc.lineAt(state.selection.main.head).number);
}

/** Puts the cursor at the line start and scrolls the line to the middle. */
export function goToLine(view: EditorView, line: number): void {
  const target = Math.min(Math.max(line, 1), view.state.doc.lines);
  const pos = view.state.doc.line(target).from;
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: "center" }),
  });
}
