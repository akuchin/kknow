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

const RECENT_LIMIT = 12;

let recent: string[] = [];

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

export async function openBookmark(id: string, line: number): Promise<void> {
  const vaultId = state.vault?.id;
  if (!state.isOpen(vaultId, id) || state.note?.file) await openNote(id);
  if (!state.isOpen(vaultId, id)) return;
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

export function goToAdjacentBookmark(direction: 1 | -1): void {
  if (editor.getBookmarks().length === 0) return;
  if (state.mode === "read") setMode("split", { remember: false });
  const line = editor.goToBookmark(direction);
  if (line !== null && state.mode !== "edit") scrollToLine(el.preview, line);
}

export async function openTask(id: string, line: number): Promise<void> {
  const vaultId = state.vault?.id;
  if (!state.isOpen(vaultId, id) || state.note?.file) await openNote(id);
  if (!state.isOpen(vaultId, id)) return;
  if (state.mode === "read") setMode("split", { remember: false });
  editor.goToLine(line);
}

export function goToHeading(heading: Heading): void {
  editor.goToLine(heading.line, "start");
  if (state.mode !== "edit") scrollToLine(el.preview, heading.line);
}

export function rememberOpened(id: string): void {
  if (!state.vault) return;
  recent = [id, ...recent.filter((other) => other !== id)].slice(0, RECENT_LIMIT);
  localStorage.setItem(`kknow:recent:${state.vault.id}`, JSON.stringify(recent));
  localStorage.setItem(`kknow:note:${state.vault.id}`, id);
}

export function loadVaultMemory(id: string): void {
  for (const stale of ["kknow:folders", "kknow:recent", "kknow:vault"]) {
    localStorage.removeItem(stale);
  }

  recent = readList(`kknow:recent:${id}`);
}

export function recentNotes(notes: NoteSummary[]): NoteSummary[] {
  const byId = new Map(notes.map((note) => [note.id, note]));
  const picked = recent.map((id) => byId.get(id)).filter((note) => note !== undefined);
  const rest = notes.filter((note) => !recent.includes(note.id));
  return [...picked, ...rest];
}
