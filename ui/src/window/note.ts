import { api, type LinkRef, type NoteView, type ViewMode } from "../api";
import type { EditorPosition } from "../editor";
import { renderDiagrams } from "../diagram";
import { addCopyButtons } from "../code-copy";
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
import { followMode, resetFindPane } from "./find";
import { previewNotes, tableNotes } from "./notes-float";
import { state } from "./state";
import { fail, setStatus, status } from "./status";

export interface NoteHooks {
  closeViews: () => void;
  refreshFiles: () => Promise<void>;
  refreshTree: (vaultId: string) => Promise<void>;
  opened: (id: string) => void;
  switchVault: (id: string) => Promise<void>;
}

let hooks: NoteHooks;

const SAVE_DELAY = 1000;

const PREVIEW_DELAY = 180;

const WORDS_DELAY = 300;

const positions = new Map<string, { editor: EditorPosition; preview: number }>();

export const LAST_FILE = "kknow:file";

let previewTimer: number | undefined;

let wordsTimer: number | undefined;

export function onEdit(text: string): void {
  if (!state.note || state.note.closed) return;
  setStatus(t("status.notSaved"), "work");
  scheduleWords(text);

  saves.schedule(text);

  window.clearTimeout(previewTimer);
  if (state.mode !== "edit") {
    previewTimer = window.setTimeout(() => void refreshPreview(text), PREVIEW_DELAY);
  }
}

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
  followMode(next);

  if (wasHidden && next !== "edit" && state.note && options.refresh !== false) {
    void refreshPreview(editor.getText());
  }
}

export function defaultMode(): ViewMode {
  return state.settings?.viewMode ?? "split";
}

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

const saves = new SaveQueue((text) => saveNow(text), SAVE_DELAY);

function isDirty(): boolean {
  return state.note !== null && editor.getText() !== state.note.content;
}

async function saveNow(text: string): Promise<void> {
  const note = state.note;
  if (!note || note.closed) return;
  const ticket = state.noteTicket();
  if (text === note.content) {
    setStatus(t("status.saved"));
    return;
  }
  try {
    const outcome = note.file
      ? await api.saveFile(note.id, text, note.content)
      : await api.saveNote(note.id, text, note.content, editor.getBookmarkMarks());
    if (!ticket.isCurrent()) return;

    if (outcome.status === "saved") {
      applyNote(outcome.note, { keepEditor: true });
      setStatus(t("status.saved"));
      document.dispatchEvent(new Event("kknow:note-saved"));
      return;
    }

    const latest = editor.getText();
    if (latest !== text) {
      await (note.file
        ? api.saveFile(outcome.copy, latest, null)
        : api.saveNote(outcome.copy, latest, null));
    }
    saves.cancel();
    applyNote(outcome.note);
    if (note.file) await hooks.refreshFiles();
    else if (state.vault) await hooks.refreshTree(state.vault.id);
    setStatus(t("status.externalConflict", { name: outcome.copy }), "error");
  } catch (error) {
    fail(error);
  }
}

export async function syncWithDisk(): Promise<void> {
  await flushPendingSave();
  if (!state.note || isDirty()) return;
  const id = state.note.id;
  const ticket = state.noteTicket();
  const file = state.note.file === true;
  try {
    const view = file ? await api.loadFile(id) : await api.loadNote(id);
    if (!ticket.isCurrent() || isDirty() || view.content === state.note?.content) return;
    applyNote(view, { reload: true });
    setStatus(t("status.reloaded"));
  } catch (error) {
    if (file && (await fileGone(id))) return;
    fail(error);
  }
}

export async function fileGone(path: string): Promise<boolean> {
  await hooks.refreshFiles();
  if (state.files.some((entry) => entry.path === path)) return false;
  if (state.isOpen(state.vault?.id, path)) closeNote();
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

export function showPreview(html: string): void {
  if (state.mode === "edit") return;
  el.preview.innerHTML = html;
  enableTaskBoxes();
  explainStoppedEmbeds();
  paintBookmarks(el.preview, editor.getBookmarks());
  paintCodeBlocks(el.preview);
  addCopyButtons(el.preview);
  paintImages(el.preview);
  void renderDiagrams(el.preview);
}

function scheduleWords(text: string): void {
  window.clearTimeout(wordsTimer);
  wordsTimer = window.setTimeout(() => status.setWords(countWords(text)), WORDS_DELAY);
}

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

function headingName(view: NoteView): string {
  return view.id.slice(view.id.lastIndexOf("/") + 1).replace(/\.md$/i, "");
}

export function closeNote(): void {
  hooks.closeViews();
  state.setNote(null);
  setImageOwner(null);
  showHeading(null);
  editor.setText("");
  el.preview.innerHTML = "";
  status.setWords(0);
  applyScale();
  renderNoteTools();
}

export function applyNote(
  view: NoteView,
  options: { keepEditor?: boolean; reload?: boolean } = {},
): void {
  if (!options.keepEditor) {
    stashPosition();
    if (!options.reload) hooks.closeViews();
  }
  state.setNote(view);
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
    setMode(modeOf(positionKey(view), defaultMode()), { remember: false, refresh: false });
    rememberPlace(view);
  }
  showPreview(view.html);
  if (!options.keepEditor) {
    const position = positions.get(positionKey(view));
    editor.setBookmarks(view.bookmarks ?? [], !view.file && !view.locked);
    paintBookmarks(el.preview, editor.getBookmarks());
    editor.setPosition(position?.editor);
    el.preview.scrollTop = position?.preview ?? 0;
  }
  applyScale();
  renderNoteTools();
}

export function stashPosition(): void {
  if (!state.note) return;
  positions.set(positionKey(state.note), {
    editor: editor.getPosition(),
    preview: el.preview.scrollTop,
  });
}

export function positionKey(view: NoteView): string {
  return view.file ? `file:${view.id}` : noteKey(state.vault?.id ?? "", view.id);
}

export function noteKey(vaultId: string, id: string): string {
  return `${vaultId}/${id}`;
}

export function applyScale(): void {
  const scale = state.note ? scaleOf(positionKey(state.note)) : defaultScale();
  editor.setScale(scale);
  el.preview.style.fontSize = scale === 1 ? "" : `${scale * 100}%`;
  status.setScale(state.note ? scale : null, defaultScale());
}

function changeScale(key: string, steps: number): void {
  stepScale(key, steps);
  if (state.note && positionKey(state.note) === key) applyScale();
}

export function changeCurrentScale(steps: number): void {
  if (state.note) changeScale(positionKey(state.note), steps);
}

export function setBacklinks(backlinks: LinkRef[]): void {
  if (!state.note) return;
  state.setNote({ ...state.note, backlinks });
  renderNoteTools();
}

export function renderNoteTools(): void {
  el.noteTools.hidden = state.note === null;
  if (!state.note) return;

  el.noteBookmarks.hidden = state.note.file === true || state.note.locked === true;
  el.noteBacklinks.hidden = state.note.file === true;

  const marks = editor.getBookmarks().length;
  el.bookmarksCount.textContent = marks > 0 ? String(marks) : "";
  el.noteBookmarks.dataset.on = String(marks > 0);

  const links = state.note.backlinks.length;
  el.backlinksCount.textContent = String(links);
  el.noteBacklinks.disabled = links === 0;
}

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
    resetFindPane();
    hooks.opened(id);
    localStorage.removeItem(LAST_FILE);
    setStatus("");
    editor.focus();
  } catch (error) {
    fail(error);
  }
}

function enableTaskBoxes(): void {
  if (!state.note || state.note.file) return;
  for (const box of el.preview.querySelectorAll<HTMLInputElement>("li input[type=checkbox]")) {
    if (!box.closest(".embed")) box.disabled = false;
  }
}

function explainStoppedEmbeds(): void {
  for (const embed of el.preview.querySelectorAll<HTMLElement>(".embed-stopped")) {
    embed.title = t(embed.dataset.reason === "loop" ? "embed.loop" : "embed.depth");
  }
}

export function redrawPreview(): void {
  if (state.note && state.mode !== "edit") void refreshPreview(editor.getText());
}

async function setTask(line: number, done: boolean): Promise<void> {
  if (!state.note || state.note.file) return;
  const id = state.note.id;
  const ticket = state.noteTicket();
  await flushPendingSave();
  if (!ticket.isCurrent()) return;
  try {
    const view = await api.setTask(id, line, done);
    if (!ticket.isCurrent()) return;
    applyNote(view, { keepEditor: true });
    editor.setTask(line, done);
    setStatus(t(done ? "status.taskDone" : "status.taskOpen"));
  } catch (error) {
    fail(error);
  }
}

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

export function initNote(calls: NoteHooks): void {
  hooks = calls;
  for (const button of el.modeButtons) {
    button.onclick = () => setMode(button.dataset.mode as ViewMode);
  }
  el.navBack.onclick = () => void travel(-1);
  el.navForward.onclick = () => void travel(1);

  el.preview.addEventListener("click", (event) => {
    const box = event.target;
    if (!(box instanceof HTMLInputElement) || box.type !== "checkbox") return;
    event.preventDefault();
    const line = markLine(el.preview, box);
    if (line !== null) void setTask(line, box.checked);
  });
}

export function releaseNote(): void {
  state.setNote(null);
}

export function cancelPendingSave(): void {
  saves.cancel();
}
