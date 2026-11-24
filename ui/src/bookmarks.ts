import {
  RangeSet,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
} from "@codemirror/state";
import { EditorView, GutterMarker, type ViewUpdate } from "@codemirror/view";

import type { BookmarkMark } from "./api";

const setMarks = StateEffect.define<BookmarkMark[]>();

export type BookmarkChange = "set" | "removed" | "named" | "unnamed";

const changed = StateEffect.define<{ line: number; change: BookmarkChange }>();

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
    return value.map((placed) => ({ ...placed, pos: tr.changes.mapPos(placed.pos, 1) }));
  },
});

class BookmarkMarker extends GutterMarker {
  toDOM(): Node {
    const dom = document.createElement("span");
    dom.className = "cm-bookmark-gutter";
    return dom;
  }
}

const marker = new BookmarkMarker();

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

export function bookmarkMarkers(state: EditorState): RangeSet<GutterMarker> {
  return state.field(markers, false) ?? RangeSet.empty;
}

export function bookmarksEnabled(state: EditorState): boolean {
  return state.field(enabled, false) ?? false;
}

export function hasBookmark(state: EditorState, line: number): boolean {
  return linesOf(state).includes(line);
}

export function bookmarkName(state: EditorState, line: number): string | null {
  return marksOf(state).find((mark) => mark.line === line)?.name ?? null;
}

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

export function bookmarkMarks(state: EditorState): BookmarkMark[] {
  return marksOf(state);
}

export function setBookmarkMarks(view: EditorView, marks: BookmarkMark[], on: boolean): void {
  view.dispatch({ effects: [setEnabled.of(on), setMarks.of(on ? marks : [])] });
}

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

export function toggleBookmark(view: EditorView): void {
  const { state } = view;
  toggleBookmarkAt(view, state.doc.lineAt(state.selection.main.head).number);
}

export function adjacentBookmark(
  lines: number[],
  cursorLine: number,
  direction: 1 | -1,
): number | null {
  if (lines.length === 0) return null;
  if (direction === 1) return lines.find((line) => line > cursorLine) ?? lines[0];
  const before = lines.filter((line) => line < cursorLine);
  return before.length > 0 ? before[before.length - 1] : lines[lines.length - 1];
}

export function goToBookmark(view: EditorView, direction: 1 | -1): number | null {
  const { state } = view;
  const cursor = state.doc.lineAt(state.selection.main.head).number;
  const line = adjacentBookmark(bookmarkLines(state), cursor, direction);
  if (line !== null) goToLine(view, line);
  return line;
}

export function goToLine(view: EditorView, line: number, at: "center" | "start" = "center"): void {
  const target = Math.min(Math.max(line, 1), view.state.doc.lines);
  const pos = view.state.doc.line(target).from;
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: at }),
  });
}
