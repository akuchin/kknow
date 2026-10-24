/**
 * Bookmarks in the editor: the gutter marker and how it moves with edits.
 *
 * The core stores bookmarks (`.kknow/bookmarks.json`), but the editor knows
 * where they are now. Edits shift lines, so the line number from the open
 * call soon becomes wrong. That is why the state keeps text positions, not
 * numbers, and each edit maps them. Line numbers come from the positions on
 * request; on save they go to the core.
 */

import {
  RangeSet,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
} from "@codemirror/state";
import { EditorView, GutterMarker, type ViewUpdate } from "@codemirror/view";

import { t } from "./i18n";

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

/** The ribbon in the gutter (`gutter.ts`). A click on it removes the bookmark. */
class BookmarkMarker extends GutterMarker {
  toDOM(): Node {
    const dom = document.createElement("span");
    dom.className = "cm-bookmark-gutter";
    dom.title = t("bookmark.remove");
    return dom;
  }
}

const marker = new BookmarkMarker();

/**
 * The gutter asks for its markers on each update, so they are kept in a field
 * and built again only when the positions change.
 */
const markers = StateField.define<RangeSet<GutterMarker>>({
  create: (state) => markersOf(state),
  update: (value, tr) =>
    tr.state.field(positions) === tr.startState.field(positions) ? value : markersOf(tr.state),
});

function markersOf(state: EditorState): RangeSet<GutterMarker> {
  return RangeSet.of(linesOf(state).map((line) => marker.range(state.doc.line(line).from)));
}

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
  return [enabled, positions, markers];
}

/** Bookmark markers for the gutter. */
export function bookmarkMarkers(state: EditorState): RangeSet<GutterMarker> {
  return state.field(markers, false) ?? RangeSet.empty;
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
