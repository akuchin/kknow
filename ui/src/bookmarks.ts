/**
 * Bookmarks in the editor: the gutter marker and how it moves with edits.
 *
 * The core stores bookmarks (`.kknow/bookmarks.json`), but the editor knows
 * where they are now. Edits shift lines, so the line number from the open
 * call soon becomes wrong. That is why the state keeps text positions, not
 * numbers, and each edit maps them. Line numbers come from the positions on
 * request; on save they go to the core.
 *
 * Each position carries the bookmark name. The user edits names here (the
 * marker menu, `bookmark-menu.ts`), so names go to the core with the lines.
 */

import {
  RangeSet,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
} from "@codemirror/state";
import { EditorView, GutterMarker, type ViewUpdate } from "@codemirror/view";

import type { BookmarkMark } from "./api";

/** Replaces all bookmarks. Line numbers start at 1. */
const setMarks = StateEffect.define<BookmarkMark[]>();

/** What the user did to a bookmark. */
export type BookmarkChange = "set" | "removed" | "named" | "unnamed";

/**
 * The user changed a bookmark. It comes together with `setMarks`. The window
 * uses it to tell this from a note load and sends bookmarks to the core.
 */
const changed = StateEffect.define<{ line: number; change: BookmarkChange }>();

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

/** A bookmark in the state: its line start when it was set, and its name. */
interface Placed {
  pos: number;
  name?: string;
}

const positions = StateField.define<Placed[]>({
  create: () => [],
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setMarks)) return toPositions(tr.state, effect.value);
    }
    if (!tr.docChanged) return value;
    // `1`: an insert exactly at the position pushes the position right. So
    // Enter at the start of a bookmarked line moves the bookmark with the
    // text and does not leave it on the new empty line.
    return value.map((placed) => ({ ...placed, pos: tr.changes.mapPos(placed.pos, 1) }));
  },
});

/**
 * The ribbon in the gutter (`gutter.ts`). A click on it removes the bookmark.
 * It has no `title`: `bookmark-menu.ts` shows the name on hover.
 */
class BookmarkMarker extends GutterMarker {
  toDOM(): Node {
    const dom = document.createElement("span");
    dom.className = "cm-bookmark-gutter";
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

function toPositions(state: EditorState, marks: BookmarkMark[]): Placed[] {
  return marks
    .filter(({ line }) => line >= 1 && line <= state.doc.lines)
    .map(({ line, name }) => ({ pos: state.doc.line(line).from, ...(name ? { name } : {}) }));
}

/**
 * Sorted bookmarks without duplicates: lines merged by an edit count once.
 * Of merged bookmarks, the first one with a name keeps it.
 */
function marksOf(state: EditorState): BookmarkMark[] {
  const byLine = new Map<number, BookmarkMark>();
  for (const { pos, name } of state.field(positions)) {
    const line = state.doc.lineAt(pos).number;
    const known = byLine.get(line);
    if (!known) byLine.set(line, name ? { line, name } : { line });
    else if (!known.name && name) known.name = name;
  }
  return [...byLine.values()].sort((a, b) => a.line - b.line);
}

function linesOf(state: EditorState): number[] {
  return marksOf(state).map(({ line }) => line);
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

/** The name of the bookmark on `line`, or `null` if it has none. */
export function bookmarkName(state: EditorState, line: number): string | null {
  return marksOf(state).find((mark) => mark.line === line)?.name ?? null;
}

/** The bookmark the user changed in this update, or `null`. */
export function bookmarkChanged(
  update: ViewUpdate,
): { line: number; change: BookmarkChange } | null {
  for (const tr of update.transactions) {
    for (const effect of tr.effects) {
      if (effect.is(changed)) return effect.value;
    }
  }
  return null;
}

export function bookmarkLines(state: EditorState): number[] {
  return linesOf(state);
}

/** Bookmarks with names, sorted by line. They go to the core on save. */
export function bookmarkMarks(state: EditorState): BookmarkMark[] {
  return marksOf(state);
}

/** Bookmarks of the open note; `on` tells whether bookmarks are allowed. */
export function setBookmarkMarks(view: EditorView, marks: BookmarkMark[], on: boolean): void {
  view.dispatch({ effects: [setEnabled.of(on), setMarks.of(on ? marks : [])] });
}

/**
 * Sets or removes the bookmark on `line`. The window sends bookmarks to the
 * core: it learns about the change from `bookmarkToggled`.
 */
export function toggleBookmarkAt(view: EditorView, line: number): void {
  const { state } = view;
  if (!bookmarksEnabled(state)) return;
  const marks = marksOf(state);
  const set = !marks.some((mark) => mark.line === line);
  const next = set
    ? [...marks, { line }].sort((a, b) => a.line - b.line)
    : marks.filter((mark) => mark.line !== line);
  view.dispatch({
    effects: [setMarks.of(next), changed.of({ line, change: set ? "set" : "removed" })],
  });
}

/**
 * Names the bookmark on `line`; an empty `name` removes the name. A line
 * without a bookmark is left alone. The window learns from `bookmarkChanged`.
 */
export function nameBookmarkAt(view: EditorView, line: number, name: string): void {
  const marks = marksOf(view.state);
  const mark = marks.find((each) => each.line === line);
  if (!mark) return;
  const wanted = name.trim();
  if ((mark.name ?? "") === wanted) return;
  if (wanted) mark.name = wanted;
  else delete mark.name;
  view.dispatch({
    effects: [setMarks.of(marks), changed.of({ line, change: wanted ? "named" : "unnamed" })],
  });
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
