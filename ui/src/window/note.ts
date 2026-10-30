// The open note: open, show, save and close it; the preview, the heading,
// reading positions, the font size, backlinks, the mode and back and forward.
//
// This zone is low: almost every other zone opens or reads the note. A few
// steps reach higher zones (the trash view, the tree, the file list, the
// recent notes, a vault switch); `main.ts` passes them to `initNote` as hooks.
// Only this module calls `state.setNote` and `state.setMode`.
//
// The module does not touch the DOM on import.

import { api, type NoteView, type ViewMode } from "../api";
import type { EditorPosition } from "../editor";
import { renderDiagrams } from "../diagram";
import { paintCodeBlocks } from "../highlight";
import { t } from "../i18n";
import { paintImages, setImageOwner } from "../images";
import { History, type Place } from "../navigation";
import { modeOf, rememberMode } from "../note-mode";
import { defaultScale, scaleOf, stepScale } from "../note-scale";
import { markLine, paintBookmarks } from "../preview-lines";
import { SaveQueue } from "../session";
import { countWords } from "../statusbar";
import { leafOf } from "../tree-model";
import { app, el } from "./dom";
import { editor } from "./editor";
import { previewNotes, tableNotes } from "./notes-float";
import { state } from "./state";
import { fail, setStatus, status } from "./status";

/** Steps of higher zones that the note zone starts. */
export interface NoteHooks {
  /** Closes the trash and git views: a note the user opens replaces them. */
  closeViews: () => void;
  /** Reloads the separate file list: a conflict copy or a missing file. */
  refreshFiles: () => Promise<void>;
  /** Reloads the tree of a vault if it is shown: a conflict copy. */
  refreshTree: (vaultId: string) => Promise<void>;
  /** The user opened a vault note: the recent notes remember it. */
  opened: (id: string) => void;
  /** Makes another vault active before its note opens. */
  switchVault: (id: string) => Promise<void>;
}

let hooks: NoteHooks;

/**
 * Delay after the last key press before the file is written.
 *
 * It is not a setting: nobody tunes it, and leaving the window or the note
 * writes at once anyway (see `flushPendingSave`). A short pause keeps a crash
 * from losing more than a second and keeps sync and the CLI near the editor.
 */
const SAVE_DELAY = 1000;

const PREVIEW_DELAY = 180;

/** The word count does not need to update on every key press. */
const WORDS_DELAY = 300;

/**
 * Reading position of each note, to continue from the same place on return.
 *
 * Memory only, while the window is open: disk state between runs is not worth
 * it. The key includes the vault: equal paths in different vaults are
 * different notes.
 */
const positions = new Map<string, { editor: EditorPosition; preview: number }>();

/**
 * Last open separate file: the window reopens it if it closed on it. The key
 * is global, not per vault, because the file belongs to no vault.
 */
export const LAST_FILE = "kknow:file";

let previewTimer: number | undefined;

let wordsTimer: number | undefined;

export function onEdit(text: string): void {
  if (!state.note) return;
  setStatus(t("status.notSaved"), "work");
  scheduleWords(text);

  saves.schedule(text);

  window.clearTimeout(previewTimer);
  // Skip a hidden preview: each refresh is a core call on every edit.
  if (state.mode !== "edit") {
    previewTimer = window.setTimeout(() => void refreshPreview(text), PREVIEW_DELAY);
  }
}

/**
 * Sets the note mode.
 *
 * These are three ways to work with text, not one panel: "Live preview" draws
 * markup in the text, "Side by side" shows plain markdown and the preview, "Read"
 * shows a full-width preview. The first two never draw the same thing twice
 * side by side, and the full source is always visible somewhere.
 *
 * A mode the user picks is saved for the note (`remember`). The mode a note
 * gets on open needs no save. `refresh` recomputes a hidden preview; on note
 * open the preview arrives anyway.
 */
export function setMode(
  next: ViewMode,
  options: { remember?: boolean; refresh?: boolean } = {},
): void {
  const wasHidden = state.mode === "edit";
  state.setMode(next);
  previewNotes.hide();
  tableNotes.hide();
  if (options.remember !== false && state.note) rememberMode(positionKey(state.note), next);

  app.dataset.mode = next;
  for (const button of el.modeButtons) {
    button.setAttribute("aria-pressed", String(button.dataset.mode === next));
  }
  editor.setLive(next === "edit");

  // The preview did not update while hidden, so recompute it.
  if (wasHidden && next !== "edit" && state.note && options.refresh !== false) {
    void refreshPreview(editor.getText());
  }
}

/** Default mode for notes that have no saved mode. */
export function defaultMode(): ViewMode {
  return state.settings?.viewMode ?? "split";
}

/** Back and forward between notes; see `navigation.ts`. */
const history = new History();

function placeOf(view: NoteView): Place {
  return view.file
    ? { file: true, path: view.id }
    : { file: false, vault: state.vault?.id ?? "", id: view.id };
}

function rememberPlace(view: NoteView): void {
  if (history.remember(placeOf(view))) showHistory();
}

function showHistory(): void {
  el.navBack.disabled = !history.canBack;
  el.navForward.disabled = !history.canForward;
}

export async function travel(step: -1 | 1): Promise<void> {
  await history.travel(
    step,
    (place) => (place.file ? openFileView(place.path) : openFound(place.id, place.vault)),
    () => (state.note ? placeOf(state.note) : null),
  );
  showHistory();
}

/** Autosave of the open note: the timer and the queue; see `session.ts`. */
const saves = new SaveQueue((text) => saveNow(text), SAVE_DELAY);

/** True if the editor has text that is not on disk. */
function isDirty(): boolean {
  return state.note !== null && editor.getText() !== state.note.content;
}

/**
 * Writes the text only if the file on disk is still the one the edit started
 * from (`state.note.content`). If it changed outside, the core keeps the external
 * version and puts ours next to it as a copy. Neither one is lost.
 */
async function saveNow(text: string): Promise<void> {
  const note = state.note;
  if (!note) return;
  // The edit was reverted to the disk content, so there is nothing to write.
  if (text === note.content) {
    setStatus(t("status.saved"));
    return;
  }
  try {
    const outcome = note.file
      ? await api.saveFile(note.id, text, note.content)
      : await api.saveNote(note.id, text, note.content, editor.getBookmarkMarks());
    // The window can switch to another note during the write.
    if (state.note?.id !== note.id) return;

    if (outcome.status === "saved") {
      applyNote(outcome.note, { keepEditor: true });
      setStatus(t("status.saved"));
      // The git changes panel follows saves; an event keeps the note zone
      // free of a dependency on it.
      document.dispatchEvent(new Event("kknow:note-saved"));
      return;
    }

    // Text typed during the write is not in the copy yet, so add it.
    const latest = editor.getText();
    if (latest !== text) {
      await (note.file
        ? api.saveFile(outcome.copy, latest, null)
        : api.saveNote(outcome.copy, latest, null));
    }
    saves.cancel();
    applyNote(outcome.note);
    // A copy of a separate file goes next to it and appears in the file list.
    if (note.file) await hooks.refreshFiles();
    else if (state.vault) await hooks.refreshTree(state.vault.id);
    setStatus(t("status.externalConflict", { name: outcome.copy }), "error");
  } catch (error) {
    fail(error);
  }
}

/**
 * Checks the open note against the disk.
 *
 * A clean note is re-read. A note with unsaved edits is saved at once: if the
 * file changed, the guard in `saveNow` works instead of a silent overwrite.
 */
export async function syncWithDisk(): Promise<void> {
  await flushPendingSave();
  if (!state.note || isDirty()) return;
  const id = state.note.id;
  const file = state.note.file === true;
  try {
    const view = file ? await api.loadFile(id) : await api.loadNote(id);
    // Compare first, so the cursor and undo history do not reset for nothing.
    if (state.note?.id !== id || isDirty() || view.content === state.note.content) return;
    applyNote(view, { reload: true });
    setStatus(t("status.reloaded"));
  } catch (error) {
    // The separate file was deleted or moved. The core already forgot it; the
    // window closes it and redraws the list. No edits are lost: the flush
    // above saved them and would have restored the file.
    if (file && (await fileGone(id))) return;
    fail(error);
  }
}

/** Checks if a separate file is gone. If so, closes it and tells the user. */
export async function fileGone(path: string): Promise<boolean> {
  await hooks.refreshFiles();
  if (state.files.some((entry) => entry.path === path)) return false;
  if (state.note?.id === path) closeNote();
  localStorage.removeItem(LAST_FILE);
  setStatus(t("status.fileGone", { name: leafOf(path) }), "error");
  return true;
}

async function refreshPreview(text: string): Promise<void> {
  if (!state.note) return;
  try {
    const id = state.note.id;
    showPreview(
      state.note.file ? await api.previewFile(id, text) : await api.preview(id, text),
    );
  } catch (error) {
    fail(error);
  }
}

/**
 * Diagrams and code highlighting run after the HTML is inserted: the core
 * returns diagram source and code blocks with a language name, and the window
 * paints them.
 */
export function showPreview(html: string): void {
  if (state.mode === "edit") return;
  el.preview.innerHTML = html;
  enableTaskBoxes();
  paintBookmarks(el.preview, editor.getBookmarks());
  paintCodeBlocks(el.preview);
  paintImages(el.preview);
  void renderDiagrams(el.preview);
}

/**
 * Word count and reading time.
 *
 * The window counts from the editor text, not the core: the number changes
 * while the user types, and a Rust call on each key press costs too much. In
 * the terminal, `kknow stats` prints the same data.
 */
function scheduleWords(text: string): void {
  window.clearTimeout(wordsTimer);
  wordsTimer = window.setTimeout(() => status.setWords(countWords(text)), WORDS_DELAY);
}

/**
 * Shows what is open in the header: the file name and its location (the vault
 * with its label and the folder). A separate file shows its directory instead.
 *
 * The file name, not the title: the title is already the first line of the
 * text, and "Rename" in the same menu changes the file name.
 */
export function showHeading(view: NoteView | null): void {
  el.noteTitle.textContent = view ? headingName(view) : t("topbar.note.empty");
  el.noteMenu.disabled = view === null;
  el.noteFolder.hidden = view === null;
  if (!view) return;

  const slash = view.id.lastIndexOf("/");
  const folder = slash > 0 ? view.id.slice(0, slash) : "";
  if (view.file) {
    el.noteVaultLabel.hidden = true;
    el.noteFolderText.textContent = folder.split("/").pop() ?? "";
    el.noteFolder.title = view.id;
    return;
  }
  el.noteVaultLabel.hidden = false;
  el.noteVaultLabel.dataset.color = state.vault?.color ?? "blue";
  el.noteFolderText.textContent = [state.vault?.name, folder].filter(Boolean).join(" / ");
  el.noteFolder.title = state.vault ? `${state.vault.root}/${view.id}.md` : view.id;
}

/**
 * The last id segment without `.md`. A vault note id has no extension; a
 * standalone file id is a full path with one.
 */
function headingName(view: NoteView): string {
  return view.id.slice(view.id.lastIndexOf("/") + 1).replace(/\.md$/i, "");
}

/** Empty state after the open note is deleted: nothing is open. */
export function closeNote(): void {
  hooks.closeViews();
  state.setNote(null);
  setImageOwner(null);
  showHeading(null);
  editor.setText("");
  // Bypass showPreview: it skips a hidden pane, but the pane must be empty
  // when it opens again.
  el.preview.innerHTML = "";
  el.backlinks.replaceChildren();
  status.setWords(0);
  applyScale();
  renderNoteTools();
}

/**
 * Shows a note. Use `keepEditor` when the editor text is already current, and
 * `reload` when the same note is re-read from the disk: the user did not open
 * it, so a view on top of it stays.
 */
export function applyNote(
  view: NoteView,
  options: { keepEditor?: boolean; reload?: boolean } = {},
): void {
  if (!options.keepEditor) {
    stashPosition();
    // A note the user opens replaces the view on top of it.
    if (!options.reload) hooks.closeViews();
  }
  state.setNote(view);
  // Before the editor text: its widgets request images for this note at once.
  setImageOwner(
    view.file
      ? { file: true, path: view.id }
      : { file: false, vault: state.vault?.id ?? "", id: view.id },
  );
  if (!options.keepEditor) {
    editor.setText(view.content);
  }
  showHeading(view);
  status.setWords(countWords(view.content));
  if (!options.keepEditor) {
    // Mode before the preview: "Live preview" mode does not draw the preview at all.
    setMode(modeOf(positionKey(view), defaultMode()), { remember: false, refresh: false });
    rememberPlace(view);
  }
  showPreview(view.html);
  if (!options.keepEditor) {
    // After the preview: the scroll position applies only when the text is in place.
    const position = positions.get(positionKey(view));
    editor.setBookmarks(view.bookmarks ?? [], !view.file);
    // The preview was drawn earlier with the bookmarks of the previous note.
    paintBookmarks(el.preview, editor.getBookmarks());
    editor.setPosition(position?.editor);
    el.preview.scrollTop = position?.preview ?? 0;
  }
  applyScale();
  renderBacklinks(view);
  // The tree, the note commands and the run button follow the "note" event.
  // The toolbar is drawn here: it counts the bookmarks, and the editor gets
  // them above.
  renderNoteTools();
}

/** Saves the reading position of the open note before it changes. */
export function stashPosition(): void {
  if (!state.note) return;
  positions.set(positionKey(state.note), {
    editor: editor.getPosition(),
    preview: el.preview.scrollTop,
  });
}

/** Position key: vault and id for a note, the path for a file. */
export function positionKey(view: NoteView): string {
  return view.file ? `file:${view.id}` : noteKey(state.vault?.id ?? "", view.id);
}

/** The same key for a note that may not be open (its menu in the tree). */
export function noteKey(vaultId: string, id: string): string {
  return `${vaultId}/${id}`;
}

/** Applies the font size of the open file to the editor and the preview. */
export function applyScale(): void {
  const scale = state.note ? scaleOf(positionKey(state.note)) : defaultScale();
  editor.setScale(scale);
  el.preview.style.fontSize = scale === 1 ? "" : `${scale * 100}%`;
  // Without an open file there is nothing to scale: the control hides.
  status.setScale(state.note ? scale : null, defaultScale());
}

/**
 * Larger, smaller or default (`steps` = 0) for the file with this key.
 *
 * No status message: the status bar control shows the value, and a message
 * with the same number on each slider step was noise.
 */
function changeScale(key: string, steps: number): void {
  stepScale(key, steps);
  if (state.note && positionKey(state.note) === key) applyScale();
}

/** The same for the open file: shortcuts, the palette and the status bar. */
export function changeCurrentScale(steps: number): void {
  if (state.note) changeScale(positionKey(state.note), steps);
}

function renderBacklinks(view: NoteView): void {
  el.backlinks.replaceChildren();

  if (view.backlinks.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = t("panel.backlinks.empty");
    el.backlinks.append(empty);
    return;
  }

  for (const link of view.backlinks) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.className = "link";
    button.textContent = link.title;
    button.onclick = () => void openNote(link.id);

    item.append(button);
    if (link.label) {
      const label = document.createElement("span");
      label.className = "muted";
      label.textContent = `«${link.label}»`;
      item.append(label);
    }
    el.backlinks.append(item);
  }
}

/**
 * Updates the note toolbar: it shows the state of the open note.
 *
 * Call it when the note or its bookmarks change. The run button has its own
 * `renderRunButton`: the commands change apart from the note.
 */
export function renderNoteTools(): void {
  el.noteTools.hidden = state.note === null;
  if (!state.note) return;

  // The core keeps bookmarks and links per vault: a standalone file has none.
  el.noteBookmarks.hidden = state.note.file === true;
  el.noteBacklinks.hidden = state.note.file === true;

  const marks = editor.getBookmarks().length;
  el.bookmarksCount.textContent = marks > 0 ? String(marks) : "";
  el.noteBookmarks.dataset.on = String(marks > 0);

  // The button stays in place with zero links: a toolbar that changes its
  // width from note to note moves the buttons from under the cursor.
  const links = state.note.backlinks.length;
  el.backlinksCount.textContent = String(links);
  el.noteBacklinks.disabled = links === 0;
}

/**
 * Opens a palette result. A result from another vault switches the vault first.
 *
 * A switch loads a new tree, themes and sync settings, so the note opens
 * after it, not in parallel.
 */
export async function openFound(id: string, from?: string): Promise<void> {
  if (from && from !== state.vault?.id) {
    await hooks.switchVault(from);
  }
  await openNote(id);
}

export async function openNote(id: string): Promise<void> {
  await flushPendingSave();
  try {
    applyNote(await api.loadNote(id));
    hooks.opened(id);
    // The user moved from a separate file to a vault; start with the vault next time.
    localStorage.removeItem(LAST_FILE);
    setStatus("");
    editor.focus();
  } catch (error) {
    fail(error);
  }
}

/**
 * The core renders task boxes as disabled inputs, and a disabled input gets
 * no clicks. A vault note makes them clickable; a separate file has no line
 * marks, so its boxes stay as they are.
 */
function enableTaskBoxes(): void {
  if (!state.note || state.note.file) return;
  for (const box of el.preview.querySelectorAll<HTMLInputElement>("li input[type=checkbox]")) {
    box.disabled = false;
  }
}

/**
 * A click on a task box in the preview.
 *
 * The core changes the file, not the window: it checks that the line is still
 * a task and writes through the usual save. The pending edits go to disk
 * first, so the core sees the text of the editor. Then the editor changes the
 * same character, and its text is again equal to the disk.
 */
async function setTask(line: number, done: boolean): Promise<void> {
  if (!state.note || state.note.file) return;
  const id = state.note.id;
  await flushPendingSave();
  if (state.note?.id !== id) return;
  try {
    const view = await api.setTask(id, line, done);
    if (state.note?.id !== id) return;
    applyNote(view, { keepEditor: true });
    editor.setTask(line, done);
    setStatus(t(done ? "status.taskDone" : "status.taskOpen"));
  } catch (error) {
    fail(error);
  }
}

/** Saves pending edits before leaving a note. */
export function flushPendingSave(): Promise<void> {
  return saves.flush(() => editor.getText());
}

export async function openFileView(path: string): Promise<void> {
  await flushPendingSave();
  try {
    applyNote(await api.loadFile(path));
    localStorage.setItem(LAST_FILE, path);
    setStatus("");
    editor.focus();
  } catch (error) {
    if (await fileGone(path)) return;
    fail(error);
  }
}

/** Takes the hooks and wires the mode switch, back and forward and the preview. */
export function initNote(calls: NoteHooks): void {
  hooks = calls;
  for (const button of el.modeButtons) {
    button.onclick = () => setMode(button.dataset.mode as ViewMode);
  }
  el.navBack.onclick = () => void travel(-1);
  el.navForward.onclick = () => void travel(1);

  // A task box in the preview changes the file, not only the picture. The
  // browser already flipped the box, so `checked` is the wanted state; the
  // redraw from the core shows the real one.
  el.preview.addEventListener("click", (event) => {
    const box = event.target;
    if (!(box instanceof HTMLInputElement) || box.type !== "checkbox") return;
    event.preventDefault();
    const line = markLine(el.preview, box);
    if (line !== null) void setTask(line, box.checked);
  });
}

/**
 * Forgets the open note without the steps of `closeNote`: a vault switch
 * draws the header, the editor and the preview on its own.
 */
export function releaseNote(): void {
  state.setNote(null);
}

/** Drops the pending edits: the note is about to be deleted. */
export function cancelPendingSave(): void {
  saves.cancel();
}
