// Places in the notes: bookmarks, tasks, headings and the recent notes of the
// palette.
//
// The module has no DOM work of its own and needs no `init`.

import { api, type BookmarkMark, type NoteSummary } from "../api";
import type { BookmarkChange } from "../bookmarks";
import { t, type Key } from "../i18n";
import type { Heading } from "../outline";
import { paintBookmarks, scrollToLine, topLine } from "../preview-lines";
import { el } from "./dom";
import { editor } from "./editor";
import { openNote, renderNoteTools, setMode } from "./note";
import { previewNotes } from "./notes-float";
import { state } from "./state";
import { fail, setStatus } from "./status";
import { readList } from "./util";

/** How many recently opened notes the palette remembers. */
const RECENT_LIMIT = 12;

/**
 * Recently opened notes. The palette shows them while the query is empty.
 *
 * The core does not return note edit times, so "recent" means opened in the
 * window, not changed on disk.
 */
let recent: string[] = [];

/**
 * Toggles a bookmark on the cursor line and sends it to the core at once:
 * there is no reason to wait for a note save, and there can be no edits.
 */
export function toggleCurrentBookmark(): void {
  if (!state.note) return;
  if (state.note.file) {
    setStatus(t("status.bookmarkFile"), "error");
    return;
  }
  if (state.mode !== "read") {
    editor.toggleBookmark();
    return;
  }
  // "Read" mode has no visible cursor: the bookmark goes on the selection, or,
  // without one, on the first block visible at the top of the preview.
  const line = previewNotes.selectedLine() ?? topLine(el.preview);
  if (line === null) {
    setStatus(t("status.bookmarkNowhere"), "error");
    return;
  }
  previewNotes.hide();
  editor.toggleBookmarkLine(line);
}

const BOOKMARK_STATUS: Record<BookmarkChange, Key> = {
  set: "status.bookmarkSet",
  removed: "status.bookmarkRemoved",
  named: "status.bookmarkNamed",
  unnamed: "status.bookmarkUnnamed",
};

/**
 * A bookmark was set, removed or renamed (shortcut, popup button, preview,
 * marker menu). It goes to the core at once, without a note save: there can
 * be no edits.
 */
export async function saveBookmarks(marks: BookmarkMark[], change: BookmarkChange): Promise<void> {
  if (!state.note || state.note.file) return;
  paintBookmarks(el.preview, marks.map((mark) => mark.line));
  renderNoteTools();
  try {
    await api.setBookmarks(state.note.id, marks, editor.getText());
    setStatus(t(BOOKMARK_STATUS[change]));
  } catch (error) {
    fail(error);
  }
}

/**
 * Opens a bookmark from the palette. The line number can be stale: the core
 * updates it on save, but the editor knows better. We take the nearest
 * bookmark of the open note.
 */
export async function openBookmark(id: string, line: number): Promise<void> {
  if (state.note?.id !== id || state.note.file) await openNote(id);
  if (state.note?.id !== id) return;
  // "Read" mode hides the editor, but the line must be visible.
  if (state.mode === "read") setMode("split", { remember: false });
  const nearest = editor
    .getBookmarks()
    .reduce<number | null>(
      (best, each) =>
        best === null || Math.abs(each - line) < Math.abs(best - line) ? each : best,
      null,
    );
  editor.goToLine(nearest ?? line);
}

/** Opens a task from the palette: the note at the task line. */
export async function openTask(id: string, line: number): Promise<void> {
  if (state.note?.id !== id || state.note.file) await openNote(id);
  if (state.note?.id !== id) return;
  // "Read" mode hides the editor, but the line must be visible.
  if (state.mode === "read") setMode("split", { remember: false });
  editor.goToLine(line);
}

/**
 * Goes to a heading from the outline. The editor cursor moves in each mode,
 * and an open preview scrolls to the heading too. "Read" keeps its mode: the
 * user reads there.
 */
export function goToHeading(heading: Heading): void {
  editor.goToLine(heading.line);
  if (state.mode !== "edit") scrollToLine(el.preview, heading.line);
}

export function rememberOpened(id: string): void {
  if (!state.vault) return;
  recent = [id, ...recent.filter((other) => other !== id)].slice(0, RECENT_LIMIT);
  localStorage.setItem(`kknow:recent:${state.vault.id}`, JSON.stringify(recent));
  // A vault reopens where it was closed: remember the last note.
  localStorage.setItem(`kknow:note:${state.vault.id}`, id);
}

/**
 * Per-vault memory: what was opened last in it.
 *
 * Keys include the vault id: each vault has its own history, and a shared
 * list would show notes from other vaults in the palette.
 */
export function loadVaultMemory(id: string): void {
  // The previous version used app-wide keys. Nothing reads them now, so remove
  // them instead of leaving garbage in browser storage.
  for (const stale of ["kknow:folders", "kknow:recent", "kknow:vault"]) {
    localStorage.removeItem(stale);
  }

  recent = readList(`kknow:recent:${id}`);
}

/**
 * Recent notes first, then the rest of the vault (`notes`): a new window has no history,
 * but the palette must show something when it opens.
 */
export function recentNotes(notes: NoteSummary[]): NoteSummary[] {
  const byId = new Map(notes.map((note) => [note.id, note]));
  const picked = recent.map((id) => byId.get(id)).filter((note) => note !== undefined);
  const rest = notes.filter((note) => !recent.includes(note.id));
  return [...picked, ...rest];
}
