import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  onCloseRequested,
  onDragDrop,
  onFileOpened,
  onKeymapChanged,
  onSettingsChanged,
  onSyncStatus,
  onUpdateStatus,
  onVaultChanged,
  onVaultOpened,
  parseLink,
  type AppSettings,
  type BookmarkMark,
  type CommandTarget,
  type CommandView,
  type FileEntry,
  type NoteSummary,
  type NoteView,
  type Opened,
  type RunOutcome,
  type SyncConflict,
  type SyncReport,
  type SyncSettings,
  type SyncStatus,
  type Theme,
  type UpdateStatus,
  type VaultInfo,
  type VaultView,
  type ViewMode,
} from "./api";
import {
  formKeys,
  formatKey,
  matches,
  matchesKey,
  shortcut,
  useKeymap,
  vaultKey,
  withShortcut,
  type Action,
  type Keymap,
} from "./actions";
import type { BookmarkChange } from "./bookmarks";
import { renderDiagrams, setDiagramBase } from "./diagram";
import { paintCodeBlocks } from "./highlight";
import { markLine, paintBookmarks, scrollToLine, topLine } from "./preview-lines";
import { imageLink } from "./attachment-name";
import { isImage, paintImages, setImageOwner } from "./images";
import type { AgentNote } from "./agent-notes";
import { PreviewNotes } from "./preview-notes";
import { cellOf } from "./table-widget";
import { Editor, type EditorPosition } from "./editor";
import { icon, iconMarkup, type IconName } from "./icons";
import { LOCALE_NAMES, locale, locales, setLocale, t, type Key } from "./i18n";
import { openMenu, type MenuItem, type MenuSection } from "./menu";
import { MODES, modeOf, moveMode, nextMode, rememberMode } from "./note-mode";
import { defaultScale, moveScale, scaleOf, setDefaultScale, stepScale } from "./note-scale";
import type { Heading } from "./outline";
import { Palette } from "./palette";
import { StatusBar, countWords } from "./statusbar";
import "./styles.css";

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
/** How many recently opened notes the palette remembers. */
const RECENT_LIMIT = 12;

// The browser uses the document language for hyphenation and word selection.
document.documentElement.lang = locale;

const MODE_ICONS: Record<ViewMode, IconName> = { edit: "pencil", split: "split", read: "book" };

const app = document.querySelector<HTMLDivElement>("#app")!;
app.innerHTML = `
  <header class="topbar">
    <!-- Above the sidebar: the sidebar toggle and navigation history. It has the
         sidebar width, so the note name sits above the editor. -->
    <div class="topbar-nav">
      <button id="toggle-sidebar" class="ghost bare" aria-expanded="true" aria-controls="sidebar">
        ${iconMarkup("sidebar")}
      </button>
      <span class="spacer"></span>
      <button id="nav-back" class="ghost bare" disabled
              title="${withShortcut(t("topbar.back"), "history.back")}"
              aria-label="${t("topbar.back")}">
        ${iconMarkup("back")}
      </button>
      <button id="nav-forward" class="ghost bare" disabled
              title="${withShortcut(t("topbar.forward"), "history.forward")}"
              aria-label="${t("topbar.forward")}">
        ${iconMarkup("chevron")}
      </button>
    </div>

    <div class="topbar-note">
      <button id="note-menu" class="ghost topbar-note-title" aria-haspopup="menu" disabled
              title="${t("topbar.note.menu")}">
        <span id="note-title">${t("topbar.note.empty")}</span>${iconMarkup("caret")}
      </button>
      <span id="note-folder" class="topbar-note-folder" hidden>
        <span class="vault-label" id="note-vault-label"></span>
        <span id="note-folder-text"></span>
      </span>
    </div>

    <div class="topbar-actions">
      <button id="toggle-notes" class="ghost" aria-pressed="false" hidden
              title="${t("topbar.notes.title")}">
        ${iconMarkup("comment")}<span id="notes-count"></span>
      </button>
      <!-- Note toolbar: the frequent note actions that the note menu hides.
           The menu keeps them too; these buttons only make them one click
           away and show their state. -->
      <div id="note-tools" class="note-tools" role="toolbar"
           aria-label="${t("topbar.tools")}" hidden>
        <button id="note-bookmarks" class="ghost bare" aria-haspopup="menu"
                title="${t("topbar.bookmarks.title")}" aria-label="${t("topbar.bookmarks.title")}">
          ${iconMarkup("bookmark")}<span id="bookmarks-count"></span>
        </button>
        <div class="split-button">
          <button id="run-last" class="ghost bare" title="${t("topbar.commands.title")}"
                  aria-label="${t("topbar.commands.title")}">
            ${iconMarkup("play")}
          </button>
          <button id="run-menu" class="ghost bare" aria-haspopup="menu"
                  title="${t("topbar.commands.title")}" aria-label="${t("topbar.commands.title")}">
            ${iconMarkup("caret")}
          </button>
        </div>
        <button id="note-backlinks" class="ghost bare" aria-haspopup="menu"
                title="${t("topbar.backlinks.title")}" aria-label="${t("topbar.backlinks.title")}">
          ${iconMarkup("backlink")}<span id="backlinks-count"></span>
        </button>
      </div>
      <div class="mode-switch" role="group" aria-label="${t("topbar.mode")}"
           title="${withShortcut(t("topbar.mode"), "note.mode")}">
        ${MODES.map(
          (mode) => `
        <button data-mode="${mode}" aria-pressed="false" aria-label="${t(`mode.${mode}`)}"
                title="${t(`mode.${mode}.title`)}">
          ${iconMarkup(MODE_ICONS[mode])}
        </button>`,
        ).join("")}
      </div>
      <button id="open-palette" class="search-button"
              title="${withShortcut(t("topbar.search.title"), "palette.open")}">
        ${iconMarkup("search")}
        <span>${t("topbar.search")}</span>
        <kbd>${shortcut("palette.open")}</kbd>
      </button>
    </div>
  </header>

  <aside class="sidebar" id="sidebar">
    <!-- Sidebar toolbar: actions on the tree. New note and folder go to the
         folder of the open note in the active vault, as ⌘N does. -->
    <div class="sidebar-head">
      <span class="sidebar-title">${t("sidebar.vaults")}</span>
      <div class="sidebar-tools" role="toolbar" aria-label="${t("sidebar.tools")}">
        <button id="new-note" class="ghost bare"
                title="${withShortcut(t("topbar.newNote.title"), "note.new")}"
                aria-label="${t("topbar.newNote.title")}">
          ${iconMarkup("filePlus")}
        </button>
        <button id="new-folder" class="ghost bare" title="${t("sidebar.newFolder.title")}"
                aria-label="${t("sidebar.newFolder.title")}">
          ${iconMarkup("folderPlus")}
        </button>
        <button id="collapse-all" class="ghost bare" title="${t("sidebar.collapseAll")}"
                aria-label="${t("sidebar.collapseAll")}">
          ${iconMarkup("collapse")}
        </button>
        <button id="add-vault" class="ghost bare" title="${t("menu.vault.add")}"
                aria-label="${t("menu.vault.add")}">
          ${iconMarkup("folder")}
        </button>
      </div>
    </div>
    <form id="new-note-form" class="new-note" hidden>
      <input id="new-note-name" type="text" placeholder="${t("sidebar.newNote.placeholder")}"
             autocomplete="off" />
    </form>
    <ul id="note-list" class="note-list"></ul>
    <!-- Separate files: markdown opened without a vault. They get their own
         section below the tree, not a root in it: a file has no folders, no
         search and no sync, which is what makes a row a vault. -->
    <section class="files" aria-labelledby="files-title">
      <div class="sidebar-head">
        <span id="files-title" class="sidebar-title">${t("sidebar.files")}</span>
        <button id="open-file" class="ghost bare" title="${withShortcut(t("sidebar.files.open"), "file.open")}"
                aria-label="${t("sidebar.files.open")}">
          ${iconMarkup("note")}
        </button>
      </div>
      <ul id="file-list" class="note-list file-list"></ul>
    </section>
  </aside>

  <main class="editor" id="editor"></main>

  <section class="preview-pane">
    <article id="preview" class="preview"></article>
    <section id="sync-panel" class="panel" hidden>
      <h2>
        ${t("panel.sync.title")}
        <button id="sync-hide" class="ghost bare" title="${t("panel.sync.hide")}"
                aria-label="${t("panel.sync.hide")}">
          ${iconMarkup("close")}
        </button>
      </h2>
      <p id="sync-summary" class="muted"></p>
      <ul id="sync-list"></ul>
    </section>
    <section class="backlinks">
      <h2>${t("panel.backlinks.title")}</h2>
      <ul id="backlink-list"></ul>
    </section>
  </section>

  <footer class="statusbar" id="statusbar"></footer>

  <!-- Agent notes in the open text. A float over the editor, not a section of
       the right pane: the right pane hides with the preview, and the list is
       most useful during editing. -->
  <section id="notes-float" class="notes-float" role="dialog" aria-labelledby="notes-title" hidden>
    <h2>
      <span id="notes-title">${t("panel.notes.title")}</span>
      <button id="notes-hide" class="ghost bare" title="${t("panel.notes.hide")}"
              aria-label="${t("panel.notes.hide")}">
        ${iconMarkup("close")}
      </button>
    </h2>
    <ul id="notes-list" class="notes-list"></ul>
  </section>

  <!-- Command output: a float near the status bar, not a panel in the right
       pane. Commands also run with the preview closed, and usually the "done"
       line in the status bar is enough. -->
  <section id="run-float" class="run-float" role="dialog" aria-labelledby="run-title" hidden>
    <h2>
      <span id="run-title">${t("panel.run.title")}</span>
      <button id="run-hide" class="ghost bare" title="${t("panel.run.hide")}"
              aria-label="${t("panel.run.hide")}">
        ${iconMarkup("close")}
      </button>
    </h2>
    <p id="run-summary" class="muted"></p>
    <pre id="run-output" class="run-output"></pre>
  </section>
`;

const el = {
  noteMenu: document.querySelector<HTMLButtonElement>("#note-menu")!,
  noteTitle: document.querySelector<HTMLSpanElement>("#note-title")!,
  noteFolder: document.querySelector<HTMLSpanElement>("#note-folder")!,
  noteFolderText: document.querySelector<HTMLSpanElement>("#note-folder-text")!,
  noteVaultLabel: document.querySelector<HTMLSpanElement>("#note-vault-label")!,
  toggleSidebar: document.querySelector<HTMLButtonElement>("#toggle-sidebar")!,
  navBack: document.querySelector<HTMLButtonElement>("#nav-back")!,
  navForward: document.querySelector<HTMLButtonElement>("#nav-forward")!,
  modeButtons: [...document.querySelectorAll<HTMLButtonElement>(".mode-switch button")],
  statusbar: document.querySelector<HTMLElement>("#statusbar")!,
  openPalette: document.querySelector<HTMLButtonElement>("#open-palette")!,
  newNote: document.querySelector<HTMLButtonElement>("#new-note")!,
  newNoteForm: document.querySelector<HTMLFormElement>("#new-note-form")!,
  newNoteName: document.querySelector<HTMLInputElement>("#new-note-name")!,
  addVault: document.querySelector<HTMLButtonElement>("#add-vault")!,
  noteList: document.querySelector<HTMLUListElement>("#note-list")!,
  openFile: document.querySelector<HTMLButtonElement>("#open-file")!,
  fileList: document.querySelector<HTMLUListElement>("#file-list")!,
  editor: document.querySelector<HTMLElement>("#editor")!,
  preview: document.querySelector<HTMLElement>("#preview")!,
  backlinks: document.querySelector<HTMLUListElement>("#backlink-list")!,
  syncPanel: document.querySelector<HTMLElement>("#sync-panel")!,
  syncSummary: document.querySelector<HTMLParagraphElement>("#sync-summary")!,
  syncList: document.querySelector<HTMLUListElement>("#sync-list")!,
  syncHide: document.querySelector<HTMLButtonElement>("#sync-hide")!,
  runFloat: document.querySelector<HTMLElement>("#run-float")!,
  runTitle: document.querySelector<HTMLSpanElement>("#run-title")!,
  runSummary: document.querySelector<HTMLParagraphElement>("#run-summary")!,
  runOutput: document.querySelector<HTMLPreElement>("#run-output")!,
  runHide: document.querySelector<HTMLButtonElement>("#run-hide")!,
  toggleNotes: document.querySelector<HTMLButtonElement>("#toggle-notes")!,
  notesCount: document.querySelector<HTMLSpanElement>("#notes-count")!,
  noteTools: document.querySelector<HTMLElement>("#note-tools")!,
  noteBookmarks: document.querySelector<HTMLButtonElement>("#note-bookmarks")!,
  bookmarksCount: document.querySelector<HTMLSpanElement>("#bookmarks-count")!,
  runLast: document.querySelector<HTMLButtonElement>("#run-last")!,
  runMenu: document.querySelector<HTMLButtonElement>("#run-menu")!,
  noteBacklinks: document.querySelector<HTMLButtonElement>("#note-backlinks")!,
  backlinksCount: document.querySelector<HTMLSpanElement>("#backlinks-count")!,
  newFolder: document.querySelector<HTMLButtonElement>("#new-folder")!,
  collapseAll: document.querySelector<HTMLButtonElement>("#collapse-all")!,
  notesFloat: document.querySelector<HTMLElement>("#notes-float")!,
  notesList: document.querySelector<HTMLUListElement>("#notes-list")!,
  notesHide: document.querySelector<HTMLButtonElement>("#notes-hide")!,
};

/**
 * Notes of each vault: every vault has its own tree.
 *
 * The sidebar shows all vaults at once, so one note list is not enough. A tree
 * loads lazily when its vault is expanded. The active vault always loads: the
 * palette, the counter and note restore depend on it.
 */
const trees = new Map<string, NoteSummary[]>();

/** Vault folders, separate from notes: empty folders are not in the note list. */
const folderLists = new Map<string, string[]>();
let current: NoteView | null = null;
/**
 * Reading position of each note, to continue from the same place on return.
 *
 * Memory only, while the window is open: disk state between runs is not worth
 * it. The key includes the vault: equal paths in different vaults are
 * different notes.
 */
const positions = new Map<string, { editor: EditorPosition; preview: number }>();
/** Active vault: the tree, the editor and the status bar work with it. */
let vault: VaultInfo | null = null;
/** Separate files in open order. The core keeps this list. */
let files: FileEntry[] = [];
/**
 * Last open separate file: the window reopens it if it closed on it. The key
 * is global, not per vault, because the file belongs to no vault.
 */
const LAST_FILE = "kknow:file";
/** All vaults. This list is also the switcher menu. */
let vaults: VaultView[] = [];
/** Sync settings; `null` means this folder has none. */
let remote: SyncSettings | null = null;
let syncing = false;
/** Sync state per vault: the active one shows in the status bar, others as a chip. */
let statuses = new Map<string, SyncStatus>();
/**
 * Mode of the open note: "Live preview" (markup in the text, no preview),
 * "Side by side" (markdown and preview) or "Read" (full-width preview only).
 */
let mode: ViewMode = "split";
/** Sidebar collapsed state; it persists between runs. */
const SIDEBAR = "kknow:sidebar";
let sidebarHidden = localStorage.getItem(SIDEBAR) === "hidden";
/** App settings: shared with the settings window and stored in the core. */
let appSettings: AppSettings | null = null;
let saveTimer: number | undefined;
let previewTimer: number | undefined;
let wordsTimer: number | undefined;

/** Agent notes in the open text, listed in the float. */
let agentNotes: AgentNote[] = [];

const editor = new Editor(el.editor, {
  onChange: onEdit,
  onNotes: showNotes,
  onBookmark: (marks, change) => void saveBookmarks(marks, change),
  saveImage: savePastedImage,
});

/** Agent notes from the preview: in "Read" mode the editor is hidden. */
const previewNotes = new PreviewNotes(el.preview, {
  enabled: () => mode !== "edit" && current !== null,
  add: (words, index, total, text) => editor.addNoteFromPreview(words, index, total, text),
  fail: (message) => setStatus(message, "error"),
  bookmarkable: () => current !== null && !current.file,
  bookmark: (line) => editor.toggleBookmarkLine(line),
});

/**
 * Agent notes from a closed table cell in "Live preview" mode. A drag there
 * selects text for reading and does not open the cell, so the selection belongs to
 * the browser, not to the editor: it gets the same popup as the preview.
 */
const tableNotes = new PreviewNotes(editor.scroller, {
  enabled: () => mode === "edit" && current !== null,
  scope: cellOf,
  add: (words, index, total, text, cell) => editor.addNoteInCell(cell, words, index, total, text),
  fail: (message) => setStatus(message, "error"),
  bookmarkable: () => false,
  bookmark: () => {},
});

/**
 * Updates the agent notes button and float.
 *
 * The button is visible only when notes exist: most texts have none, and an
 * empty counter would be noise. An open float stays open after the last note
 * is deleted: a window that disappears under the cursor is worse than an
 * empty one.
 */
function showNotes(notes: AgentNote[]): void {
  agentNotes = notes;
  el.notesCount.textContent = String(notes.length);
  el.toggleNotes.hidden = notes.length === 0 && el.notesFloat.hidden;
  if (!el.notesFloat.hidden) renderNotes();
}

function renderNotes(): void {
  el.notesList.replaceChildren();

  if (agentNotes.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = t("panel.notes.empty");
    el.notesList.append(empty);
    return;
  }

  for (const note of agentNotes) {
    const button = document.createElement("button");
    button.className = "notes-item";
    // CSS truncates the text; the tooltip keeps the full text.
    button.title = note.anchor ? `«${note.anchor}»\n${note.text}` : note.text;

    const words = document.createElement("span");
    words.className = "notes-words";
    words.textContent = note.anchor || t("panel.notes.noWords");
    // A note without words is about its paragraph; it did not lose them.
    if (note.anchorFrom === null && note.anchor !== "") {
      words.classList.add("is-detached");
      words.title = t("editor.note.detached");
    }

    const text = document.createElement("span");
    text.className = "notes-text";
    text.textContent = note.text;

    button.append(words, text);
    button.onclick = () => revealNote(note);

    const item = document.createElement("li");
    item.append(button);
    el.notesList.append(item);
  }
}

/**
 * Jumps to a note from the list. In "Read" mode the editor is hidden, so the
 * preview scrolls to the highlight instead. Highlights are matched by note
 * text, not by order: the core does not highlight words that start inside
 * markup.
 */
function revealNote(note: AgentNote): void {
  if (mode !== "read") {
    editor.revealNote(note);
    return;
  }
  const same = agentNotes.filter((each) => each.text === note.text);
  const marks = [...el.preview.querySelectorAll<HTMLElement>(".agent-note")].filter(
    (mark) => mark.title === note.text,
  );
  marks[same.indexOf(note)]?.scrollIntoView({ block: "center", behavior: "smooth" });
}

function showNotesFloat(show: boolean): void {
  el.notesFloat.hidden = !show;
  el.toggleNotes.setAttribute("aria-pressed", String(show));
  el.toggleNotes.hidden = !show && agentNotes.length === 0;
  if (show) renderNotes();
}

/**
 * Status bar: app state and quick toggles.
 *
 * Handlers get the button because the menu attaches to it, not to the window.
 */
const status = new StatusBar(el.statusbar, {
  vault: (anchor) => openVaultMenu(anchor),
  sync: () => void runSync(),
  run: () => showRunOutput(el.runFloat.hidden),
  chip: (anchor) => openChipMenu(anchor),
  update: () => void restartToUpdate(),
  scale: (steps) => changeCurrentScale(steps),
  lines: () => void toggleLineNumbers(),
  more: (anchor) => openMoreMenu(anchor),
});

/** Window messages go to the status bar through this one entry point. */
function setStatus(text: string, tone: "idle" | "work" | "error" = "idle"): void {
  status.setMessage(text, tone);
}

function fail(error: unknown): void {
  setStatus(String(error), "error");
}

function showUpdate(update: UpdateStatus): void {
  status.setUpdate(update.state === "ready" ? update.version : null);
}

/**
 * "Restart" in the status bar. The note goes to disk first: the restart does
 * not wait for the autosave timer. On success the app is gone, so only a
 * failure comes back.
 */
async function restartToUpdate(): Promise<void> {
  await flushPendingSave();
  setStatus(t("status.updating"), "work");
  try {
    await api.restartToUpdate();
  } catch (error) {
    fail(error);
  }
}

function onEdit(text: string): void {
  if (!current) return;
  setStatus(t("status.notSaved"), "work");
  scheduleWords(text);

  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => void save(text), SAVE_DELAY);

  window.clearTimeout(previewTimer);
  // Skip a hidden preview: each refresh is a core call on every edit.
  if (mode !== "edit") {
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
function setMode(next: ViewMode, options: { remember?: boolean; refresh?: boolean } = {}): void {
  const wasHidden = mode === "edit";
  mode = next;
  previewNotes.hide();
  tableNotes.hide();
  if (options.remember !== false && current) rememberMode(positionKey(current), next);

  app.dataset.mode = next;
  for (const button of el.modeButtons) {
    button.setAttribute("aria-pressed", String(button.dataset.mode === next));
  }
  editor.setLive(next === "edit");

  // The preview did not update while hidden, so recompute it.
  if (wasHidden && next !== "edit" && current && options.refresh !== false) {
    void refreshPreview(editor.getText());
  }
}

/** Default mode for notes that have no saved mode. */
function defaultMode(): ViewMode {
  return appSettings?.viewMode ?? "split";
}

/** Collapses the sidebar; the editor takes its space. */
function setSidebarHidden(hidden: boolean): void {
  sidebarHidden = hidden;
  localStorage.setItem(SIDEBAR, hidden ? "hidden" : "shown");
  app.dataset.sidebar = hidden ? "hidden" : "shown";
  // This is "is the sidebar expanded", not "is the button on": no accent
  // color, the sidebar itself is visible.
  el.toggleSidebar.setAttribute("aria-expanded", String(!hidden));
  const label = t(hidden ? "topbar.sidebar.show" : "topbar.sidebar.hide");
  el.toggleSidebar.title = withShortcut(label, "sidebar.toggle");
  el.toggleSidebar.setAttribute("aria-label", label);
}

/**
 * Navigation history: back and forward, as in a browser.
 *
 * A place is a note in its vault or a separate file. Going back can lead to
 * another vault, which must become active first. While the window walks the
 * history (`travelling`), opens are not recorded: else "back" would add a new
 * step and never move.
 */
type Place = { file: true; path: string } | { file: false; vault: string; id: string };

const HISTORY_LIMIT = 100;
const places: Place[] = [];
let placeAt = -1;
let travelling = false;

function placeOf(view: NoteView): Place {
  return view.file ? { file: true, path: view.id } : { file: false, vault: vault?.id ?? "", id: view.id };
}

function samePlace(a: Place, b: Place): boolean {
  return a.file
    ? b.file && a.path === b.path
    : !b.file && a.vault === b.vault && a.id === b.id;
}

function rememberPlace(view: NoteView): void {
  if (travelling) return;
  const place = placeOf(view);
  if (placeAt >= 0 && samePlace(places[placeAt], place)) return;
  // A new step after "back" drops the forward entries, as in a browser.
  places.splice(placeAt + 1);
  places.push(place);
  if (places.length > HISTORY_LIMIT) places.shift();
  placeAt = places.length - 1;
  showHistory();
}

function showHistory(): void {
  el.navBack.disabled = placeAt <= 0;
  el.navForward.disabled = placeAt >= places.length - 1;
}

/**
 * One step in the history. The place can be gone (the note was deleted or
 * renamed): then we remove it instead of getting stuck on it.
 */
async function travel(step: -1 | 1): Promise<void> {
  const target = placeAt + step;
  if (target < 0 || target >= places.length) return;
  const place = places[target];

  travelling = true;
  try {
    if (place.file) await openFileView(place.path);
    else await openFound(place.id, place.vault);
  } finally {
    travelling = false;
  }

  if (current && samePlace(placeOf(current), place)) {
    placeAt = target;
  } else {
    places.splice(target, 1);
    if (target < placeAt) placeAt -= 1;
  }
  showHistory();
}

/**
 * Saves run strictly one at a time: each one needs the previous result as
 * `base`, else the second save would treat the first as an external edit.
 */
let saving: Promise<void> = Promise.resolve();

/** True if the editor has text that is not on disk. */
function isDirty(): boolean {
  return current !== null && editor.getText() !== current.content;
}

function save(text: string): Promise<void> {
  // The timer fired, so no edit is pending. Before this fix the timer stayed
  // armed, and the window always saw the note as unsaved: it ignored
  // external edits, and the next save erased them.
  window.clearTimeout(saveTimer);
  saveTimer = undefined;
  saving = saving.then(() => saveNow(text));
  return saving;
}

/**
 * Writes the text only if the file on disk is still the one the edit started
 * from (`current.content`). If it changed outside, the core keeps the external
 * version and puts ours next to it as a copy. Neither one is lost.
 */
async function saveNow(text: string): Promise<void> {
  const note = current;
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
    if (current?.id !== note.id) return;

    if (outcome.status === "saved") {
      applyNote(outcome.note, { keepEditor: true });
      setStatus(t("status.saved"));
      return;
    }

    // Text typed during the write is not in the copy yet, so add it.
    const latest = editor.getText();
    if (latest !== text) {
      await (note.file
        ? api.saveFile(outcome.copy, latest, null)
        : api.saveNote(outcome.copy, latest, null));
    }
    window.clearTimeout(saveTimer);
    saveTimer = undefined;
    applyNote(outcome.note);
    // A copy of a separate file goes next to it and appears in the file list.
    if (note.file) await refreshFiles();
    else if (vault) await refreshTree(vault.id);
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
async function syncWithDisk(): Promise<void> {
  await flushPendingSave();
  if (!current || isDirty()) return;
  const id = current.id;
  const file = current.file === true;
  try {
    const view = file ? await api.loadFile(id) : await api.loadNote(id);
    // Compare first, so the cursor and undo history do not reset for nothing.
    if (current?.id !== id || isDirty() || view.content === current.content) return;
    applyNote(view);
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
async function fileGone(path: string): Promise<boolean> {
  await refreshFiles();
  if (files.some((entry) => entry.path === path)) return false;
  if (current?.id === path) closeNote();
  localStorage.removeItem(LAST_FILE);
  setStatus(t("status.fileGone", { name: leafOf(path) }), "error");
  return true;
}

async function refreshPreview(text: string): Promise<void> {
  if (!current) return;
  try {
    const id = current.id;
    showPreview(
      current.file ? await api.previewFile(id, text) : await api.preview(id, text),
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
function showPreview(html: string): void {
  if (mode === "edit") return;
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
function showHeading(view: NoteView | null): void {
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
  el.noteVaultLabel.dataset.color = vault?.color ?? "blue";
  el.noteFolderText.textContent = [vault?.name, folder].filter(Boolean).join(" / ");
  el.noteFolder.title = vault ? `${vault.root}/${view.id}.md` : view.id;
}

/**
 * The last id segment without `.md`. A vault note id has no extension; a
 * standalone file id is a full path with one.
 */
function headingName(view: NoteView): string {
  return view.id.slice(view.id.lastIndexOf("/") + 1).replace(/\.md$/i, "");
}

/** Empty state after the open note is deleted: nothing is open. */
function closeNote(): void {
  current = null;
  setImageOwner(null);
  showHeading(null);
  editor.setText("");
  // Bypass showPreview: it skips a hidden pane, but the pane must be empty
  // when it opens again.
  el.preview.innerHTML = "";
  el.backlinks.replaceChildren();
  status.setWords(0);
  applyScale();
  markActive(null);
  renderNoteTools();
}

/** Shows a note. Use `keepEditor` when the editor text is already current. */
function applyNote(view: NoteView, options: { keepEditor?: boolean } = {}): void {
  if (!options.keepEditor) {
    stashPosition();
  }
  current = view;
  // Before the editor text: its widgets request images for this note at once.
  setImageOwner(
    view.file ? { file: true, path: view.id } : { file: false, vault: vault?.id ?? "", id: view.id },
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
  // The note can be in a collapsed branch or vault; then redraw the tree.
  if (!view.file && vault && reveal(vault.id, view.id)) renderTree();
  markActive(view.id);
  // The palette needs this note's commands and collects actions synchronously.
  // The refresh drops the old list before its first await, so the toolbar
  // after it does not offer the command of the previous note.
  void refreshNoteCommands();
  renderNoteTools();
}

/** Saves the reading position of the open note before it changes. */
function stashPosition(): void {
  if (!current) return;
  positions.set(positionKey(current), {
    editor: editor.getPosition(),
    preview: el.preview.scrollTop,
  });
}

/** Position key: vault and id for a note, the path for a file. */
function positionKey(view: NoteView): string {
  return view.file ? `file:${view.id}` : noteKey(vault?.id ?? "", view.id);
}

/** The same key for a note that may not be open (its menu in the tree). */
function noteKey(vaultId: string, id: string): string {
  return `${vaultId}/${id}`;
}

/** Applies the font size of the open file to the editor and the preview. */
function applyScale(): void {
  const scale = current ? scaleOf(positionKey(current)) : defaultScale();
  editor.setScale(scale);
  el.preview.style.fontSize = scale === 1 ? "" : `${scale * 100}%`;
  // Without an open file there is nothing to scale: the control hides.
  status.setScale(current ? scale : null, defaultScale());
}

/**
 * Larger, smaller or default (`steps` = 0) for the file with this key.
 *
 * No status message: the status bar control shows the value, and a message
 * with the same number on each slider step was noise.
 */
function changeScale(key: string, steps: number): void {
  stepScale(key, steps);
  if (current && positionKey(current) === key) applyScale();
}

/** The same for the open file: shortcuts, the palette and the status bar. */
function changeCurrentScale(steps: number): void {
  if (current) changeScale(positionKey(current), steps);
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

/** Marks the open note. Equal names can exist in different vaults. */
function markActive(id: string | null): void {
  const file = current?.file === true;
  for (const button of el.noteList.querySelectorAll<HTMLButtonElement>("button.note")) {
    const mine = !file && button.dataset.vault === vault?.id && button.dataset.id === id;
    button.classList.toggle("active", mine);
  }
  for (const button of el.fileList.querySelectorAll<HTMLButtonElement>("button.note")) {
    button.classList.toggle("active", file && button.dataset.path === id);
  }
}

/**
 * The sidebar is a workspace: the tree roots are vaults, not folders of one
 * directory.
 *
 * This shows everything at once. A click on a note from another vault opens
 * it and switches the vault automatically.
 */
function renderTree(): void {
  el.noteList.replaceChildren();

  if (vaults.length === 0) {
    el.noteList.append(emptyItem(t("sidebar.vaults.empty")));
    return;
  }

  el.noteList.append(...vaults.map(vaultItem));
  markActive(current?.id ?? null);
}

function emptyItem(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "empty";
  item.textContent = text;
  return item;
}

/** Tree node: a folder with its subfolders and notes. */
interface Folder {
  /** Path from the root. It is also the key for the collapsed state. */
  path: string;
  name: string;
  folders: Folder[];
  notes: NoteSummary[];
}

/**
 * Collapsed folders, one set per vault.
 *
 * There was one set when the sidebar had one tree. Now there is a tree per
 * vault, and a shared set would collapse a folder with the same name in
 * another vault.
 */
const collapsed = new Map<string, Set<string>>();

/** Expanded vaults: only their trees are visible and loaded from disk. */
const expanded = new Set<string>(readList("kknow:vaults:open"));

/** Vaults that are loading now; for a large vault this is parsing and indexing. */
const reading = new Set<string>();

function foldersOf(id: string): Set<string> {
  let folders = collapsed.get(id);
  if (!folders) {
    folders = new Set(readList(`kknow:folders:${id}`));
    collapsed.set(id, folders);
  }
  return folders;
}

/**
 * Per-vault memory: what was opened last in it.
 *
 * Keys include the vault id: each vault has its own history, and a shared
 * list would show notes from other vaults in the palette.
 */
function loadVaultMemory(id: string): void {
  // The previous version used app-wide keys. Nothing reads them now, so remove
  // them instead of leaving garbage in browser storage.
  for (const stale of ["kknow:folders", "kknow:recent", "kknow:vault"]) {
    localStorage.removeItem(stale);
  }

  recent = readList(`kknow:recent:${id}`);
}

function readList(key: string): string[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(saved) ? saved.filter((item) => typeof item === "string") : [];
  } catch {
    // Silently start over on a corrupt entry: it must not break the tree.
    return [];
  }
}

function rememberFolders(id: string): void {
  localStorage.setItem(`kknow:folders:${id}`, JSON.stringify([...foldersOf(id)]));
}

/** Expanded vaults persist between runs, like collapsed folders. */
function rememberExpanded(): void {
  localStorage.setItem("kknow:vaults:open", JSON.stringify([...expanded]));
}

/** Tree order follows the window language rules, so accented letters sort correctly. */
const collator = new Intl.Collator(locale);

/** Walks down to the folder at the path and creates missing nodes. */
function descend(root: Folder, folderPath: string): Folder {
  let node = root;
  for (const part of folderPath ? folderPath.split("/") : []) {
    let next = node.folders.find((folder) => folder.name === part);
    if (!next) {
      next = {
        path: node.path ? `${node.path}/${part}` : part,
        name: part,
        folders: [],
        notes: [],
      };
      node.folders.push(next);
    }
    node = next;
  }
  return node;
}

/**
 * Builds the tree from note paths: `Projects/Archive/Ideas` is two folders and
 * a leaf.
 *
 * Folders also come as a separate list: notes cannot produce an empty folder,
 * but it must be visible because the user just created it to fill it.
 */
function buildTree(items: NoteSummary[], folders: string[] = []): Folder {
  const root: Folder = { path: "", name: "", folders: [], notes: [] };

  for (const folder of folders) descend(root, folder);
  for (const note of items) descend(root, note.folder ?? "").notes.push(note);

  sortFolder(root);
  return root;
}

/** Note file name: the last segment of the id, without folders. */
function fileName(note: NoteSummary): string {
  return note.id.slice(note.id.lastIndexOf("/") + 1);
}

/**
 * Folders come before notes: first the structure, then the content.
 * Notes sort by file name, not title, so the order matches the folder on disk.
 */
function sortFolder(folder: Folder): void {
  folder.folders.sort((a, b) => collator.compare(a.name, b.name));
  folder.notes.sort((a, b) => collator.compare(fileName(a), fileName(b)));
  for (const child of folder.folders) sortFolder(child);
}

/**
 * A vault as a tree root: a row with a label and an expanded or collapsed tree.
 *
 * An unavailable vault (for example, an external disk was unplugged) stays in
 * the list but does not expand: there is nothing to read, and an external
 * cause must not remove the entry.
 */
function vaultItem(entry: VaultView): HTMLLIElement {
  const item = document.createElement("li");
  const open = expanded.has(entry.id) && entry.available;

  const row = document.createElement("div");
  row.className = "tree-row vault-row";
  row.dataset.vault = entry.id;
  if (entry.active) row.dataset.active = "true";
  if (!entry.available) row.dataset.missing = "true";

  const button = document.createElement("button");
  button.className = "vault";
  button.dataset.vault = entry.id;
  button.disabled = !entry.available;
  button.setAttribute("aria-expanded", String(open));
  button.title = entry.available
    ? t(open ? "sidebar.vault.collapse" : "sidebar.vault.expand", { name: entry.name })
    : t("menu.vault.missing");
  button.onclick = () => void toggleVault(entry);

  const arrow = icon("chevron");
  arrow.classList.add("chevron");
  const label = document.createElement("span");
  label.className = "vault-label";
  label.dataset.color = entry.color;
  const name = document.createElement("span");
  name.className = "note-title";
  name.textContent = entry.name;
  button.append(arrow, label, name);

  const more = rowMenu(t("sidebar.vault.menu", { name: entry.name }), (anchor) =>
    openVaultRowMenu(anchor, entry),
  );

  const plus = rowPlus(t("sidebar.folder.newNote", { name: entry.name }), entry.id, "");
  plus.disabled = !entry.available;
  row.append(button, rowActions(more, plus));
  // A right click opens the same menu, as users expect in a tree.
  row.oncontextmenu = (event) => {
    event.preventDefault();
    openVaultRowMenu(more, entry);
  };
  item.append(row);

  if (!open) return item;

  const nested = document.createElement("ul");
  const notes = trees.get(entry.id);
  const folders = folderLists.get(entry.id) ?? [];
  if (!notes) {
    // No tree yet: the core parses and indexes a vault on first visit, which
    // takes time on a large folder. "No notes" would be wrong here: an empty
    // tree and an unloaded tree are different things.
    nested.append(emptyItem(t("sidebar.vault.loading")));
  } else if (notes.length === 0 && folders.length === 0) {
    nested.append(emptyItem(t("sidebar.empty")));
  } else {
    nested.append(...treeItems(buildTree(notes, folders), entry.id));
  }
  item.append(nested);
  return item;
}

/** Expands or collapses a vault. On first expand it loads from disk. */
async function toggleVault(entry: VaultView): Promise<void> {
  if (!entry.available) return;

  if (expanded.delete(entry.id)) {
    rememberExpanded();
    renderTree();
    return;
  }

  expanded.add(entry.id);
  rememberExpanded();
  // A loaded tree shows at once. A tree that is loading shows when it is done:
  // a second request for the same folder adds nothing.
  if (trees.has(entry.id) || reading.has(entry.id)) {
    renderTree();
    return;
  }

  reading.add(entry.id);
  renderTree();
  try {
    await loadNotes(entry.id);
  } catch (error) {
    // On a load failure the branch stays collapsed: expanded and empty, it
    // would look like a vault without notes.
    expanded.delete(entry.id);
    rememberExpanded();
    fail(error);
  } finally {
    reading.delete(entry.id);
    renderTree();
  }
}

/**
 * Loads the vaults that were expanded in the previous run.
 *
 * The expanded state persists, but the trees do not. Without this a branch
 * would stay expanded and empty until clicked, which falsely says "no notes".
 * This runs after the active vault loads: the user needs that tree first.
 */
async function restoreExpanded(): Promise<void> {
  const pending = vaults.filter(
    (entry) =>
      entry.available &&
      expanded.has(entry.id) &&
      !trees.has(entry.id) &&
      !reading.has(entry.id),
  );
  if (pending.length === 0) return;

  for (const entry of pending) reading.add(entry.id);
  renderTree();

  await Promise.all(
    pending.map(async (entry) => {
      try {
        await loadNotes(entry.id);
      } catch (error) {
        // On a load failure the branch collapses: expanded and empty, it would
        // look like a vault without notes.
        expanded.delete(entry.id);
        fail(error);
      } finally {
        reading.delete(entry.id);
      }
    }),
  );
  rememberExpanded();
  renderTree();
}

/** Folder content as list rows; subfolders get their own nested `<ul>`. */
function treeItems(folder: Folder, vaultId: string): HTMLLIElement[] {
  const items = folder.folders.map((child) => folderItem(child, vaultId));
  items.push(...folder.notes.map((note) => noteItem(note, vaultId)));
  return items;
}

/**
 * The "⋯" button of a tree row.
 *
 * All three row kinds (vault, folder, note) use it: the menus differ, the
 * button is the same. It appears on hover; a permanent column of dots in the
 * tree is noise.
 */
function rowMenu(title: string, open: (anchor: HTMLElement) => void): HTMLButtonElement {
  const more = document.createElement("button");
  more.className = "row-menu";
  more.title = title;
  more.setAttribute("aria-label", title);
  more.setAttribute("aria-haspopup", "menu");
  more.append(icon("dots"));
  more.onclick = () => open(more);
  return more;
}

/**
 * The "+" button of a vault or folder row: a new note inside it.
 *
 * Creation is the most frequent action on a folder, so it gets a button of
 * its own next to "⋯", as in the sidebar of Claude. An inactive vault becomes
 * active first, because the core creates in the active vault.
 */
function rowPlus(title: string, vaultId: string, folder: string): HTMLButtonElement {
  const plus = document.createElement("button");
  plus.className = "row-plus";
  plus.title = title;
  plus.setAttribute("aria-label", title);
  plus.append(icon("plus"));
  plus.onclick = async () => {
    if (vaultId !== vault?.id) await switchVault(vaultId);
    openNewNoteForm(folder ? `${folder}/` : "", "note");
  };
  return plus;
}

/**
 * The row buttons on the right. They take no width until hover or keyboard
 * focus, so the name gets the whole row.
 */
function rowActions(...buttons: HTMLButtonElement[]): HTMLSpanElement {
  const actions = document.createElement("span");
  actions.className = "row-actions";
  actions.append(...buttons);
  return actions;
}

function folderItem(folder: Folder, vaultId: string): HTMLLIElement {
  const item = document.createElement("li");
  const open = !foldersOf(vaultId).has(folder.path);

  const button = document.createElement("button");
  button.className = "folder";
  button.setAttribute("aria-expanded", String(open));
  button.title = t(open ? "sidebar.folder.collapse" : "sidebar.folder.expand", {
    name: folder.name,
  });
  button.onclick = () => toggleFolder(vaultId, folder.path);

  const arrow = icon("chevron");
  arrow.classList.add("chevron");
  const name = document.createElement("span");
  name.className = "note-title";
  name.textContent = folder.name;
  button.append(arrow, name);

  const target: CommandTarget = { kind: "folder", name: folder.path, vault: vaultId };
  const more = rowMenu(t("sidebar.folder.menu", { name: folder.name }), (anchor) =>
    void openCommandMenu(anchor, target),
  );

  const row = document.createElement("div");
  row.className = "tree-row folder-row";
  // Inline rename finds the row by these attributes.
  row.dataset.path = folder.path;
  row.dataset.vault = vaultId;
  const plus = rowPlus(t("sidebar.folder.newNote", { name: folder.name }), vaultId, folder.path);
  row.append(button, rowActions(more, plus));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    void openCommandMenu(more, target);
  };
  item.append(row);

  if (open) {
    const nested = document.createElement("ul");
    nested.append(...treeItems(folder, vaultId));
    item.append(nested);
  }
  return item;
}

function toggleFolder(vaultId: string, path: string): void {
  const folders = foldersOf(vaultId);
  if (!folders.delete(path)) folders.add(path);
  rememberFolders(vaultId);
  renderTree();
}

/**
 * Expands the folders above a note and its vault.
 *
 * A note can open from outside the tree (a link, search, creation). In a
 * collapsed branch its active mark would be invisible.
 */
function reveal(vaultId: string, id: string): boolean {
  let opened = false;
  if (!expanded.has(vaultId)) {
    expanded.add(vaultId);
    rememberExpanded();
    opened = true;
  }

  const parts = id.split("/");
  parts.pop();

  const folders = foldersOf(vaultId);
  let path = "";
  let unfolded = false;
  for (const part of parts) {
    path = path ? `${path}/${part}` : part;
    if (folders.delete(path)) unfolded = true;
  }

  if (unfolded) rememberFolders(vaultId);
  return opened || unfolded;
}

/** Note row in the tree. It also knows its vault: the sidebar has many. */
function noteItem(note: NoteSummary, vaultId: string): HTMLLIElement {
  const item = document.createElement("li");
  const button = document.createElement("button");
  button.className = "note";
  button.dataset.id = note.id;
  button.dataset.vault = vaultId;
  // The list truncates the name; the tooltip shows it in full.
  button.title = note.id;
  // A note from another vault switches to that vault first: themes, sync and
  // the status bar belong to the active vault.
  button.onclick = () => void openFound(note.id, vaultId);

  const heading = document.createElement("span");
  heading.className = "note-title";
  heading.textContent = appSettings?.noteLabel === "file" ? fileName(note) : note.title;
  button.append(heading);

  const target: CommandTarget = { kind: "note", name: note.id, vault: vaultId };
  const more = rowMenu(t("sidebar.note.menu", { name: note.title }), (anchor) =>
    void openCommandMenu(anchor, target),
  );

  const row = document.createElement("div");
  row.className = "tree-row note-row";
  row.dataset.id = note.id;
  row.dataset.vault = vaultId;
  row.append(button, rowActions(more));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    void openCommandMenu(more, target);
  };
  item.append(row);
  return item;
}

/**
 * Opens a palette result. A result from another vault switches the vault first.
 *
 * A switch loads a new tree, themes and sync settings, so the note opens
 * after it, not in parallel.
 */
async function openFound(id: string, from?: string): Promise<void> {
  if (from && from !== vault?.id) {
    await switchVault(from);
  }
  await openNote(id);
}

async function openNote(id: string): Promise<void> {
  await flushPendingSave();
  try {
    applyNote(await api.loadNote(id));
    rememberOpened(id);
    // The user moved from a separate file to a vault; start with the vault next time.
    localStorage.removeItem(LAST_FILE);
    setStatus("");
    editor.focus();
  } catch (error) {
    fail(error);
  }
}

/**
 * Toggles a bookmark on the cursor line and sends it to the core at once:
 * there is no reason to wait for a note save, and there can be no edits.
 */
function toggleCurrentBookmark(): void {
  if (!current) return;
  if (current.file) {
    setStatus(t("status.bookmarkFile"), "error");
    return;
  }
  if (mode !== "read") {
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
async function saveBookmarks(marks: BookmarkMark[], change: BookmarkChange): Promise<void> {
  if (!current || current.file) return;
  paintBookmarks(el.preview, marks.map((mark) => mark.line));
  renderNoteTools();
  try {
    await api.setBookmarks(current.id, marks, editor.getText());
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
async function openBookmark(id: string, line: number): Promise<void> {
  if (current?.id !== id || current.file) await openNote(id);
  if (current?.id !== id) return;
  // "Read" mode hides the editor, but the line must be visible.
  if (mode === "read") setMode("split", { remember: false });
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
async function openTask(id: string, line: number): Promise<void> {
  if (current?.id !== id || current.file) await openNote(id);
  if (current?.id !== id) return;
  // "Read" mode hides the editor, but the line must be visible.
  if (mode === "read") setMode("split", { remember: false });
  editor.goToLine(line);
}

/**
 * The core renders task boxes as disabled inputs, and a disabled input gets
 * no clicks. A vault note makes them clickable; a separate file has no line
 * marks, so its boxes stay as they are.
 */
function enableTaskBoxes(): void {
  if (!current || current.file) return;
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
  if (!current || current.file) return;
  const id = current.id;
  await flushPendingSave();
  if (current?.id !== id) return;
  try {
    const view = await api.setTask(id, line, done);
    if (current?.id !== id) return;
    applyNote(view, { keepEditor: true });
    editor.setTask(line, done);
    setStatus(t(done ? "status.taskDone" : "status.taskOpen"));
  } catch (error) {
    fail(error);
  }
}

/**
 * Goes to a heading from the outline. The editor cursor moves in each mode,
 * and an open preview scrolls to the heading too. "Read" keeps its mode: the
 * user reads there.
 */
function goToHeading(heading: Heading): void {
  editor.goToLine(heading.line);
  if (mode !== "edit") scrollToLine(el.preview, heading.line);
}

/**
 * Recently opened notes. The palette shows them while the query is empty.
 *
 * The core does not return note edit times, so "recent" means opened in the
 * window, not changed on disk.
 */
let recent: string[] = [];

function rememberOpened(id: string): void {
  if (!vault) return;
  recent = [id, ...recent.filter((other) => other !== id)].slice(0, RECENT_LIMIT);
  localStorage.setItem(`kknow:recent:${vault.id}`, JSON.stringify(recent));
  // A vault reopens where it was closed: remember the last note.
  localStorage.setItem(`kknow:note:${vault.id}`, id);
}

/** Notes of the active vault, for the palette and the counter. */
function activeNotes(): NoteSummary[] {
  return vault ? (trees.get(vault.id) ?? []) : [];
}

/**
 * Recent notes first, then the rest of the vault: a new window has no history,
 * but the palette must show something when it opens.
 */
function recentNotes(): NoteSummary[] {
  const notes = activeNotes();
  const byId = new Map(notes.map((note) => [note.id, note]));
  const picked = recent.map((id) => byId.get(id)).filter((note) => note !== undefined);
  const rest = notes.filter((note) => !recent.includes(note.id));
  return [...picked, ...rest];
}

/** Saves pending edits before leaving a note. */
async function flushPendingSave(): Promise<void> {
  if (saveTimer === undefined) {
    // A write can be in progress; wait for it instead of leaving in the middle.
    await saving;
    return;
  }
  await save(editor.getText());
}

/**
 * Reloads the tree of one vault.
 *
 * For an inactive vault the core opens it, parses the folder and builds the
 * index. So call this only for the active vault and expanded vaults.
 */
async function loadNotes(id: string): Promise<NoteSummary[]> {
  const [notes, folders] = await Promise.all([api.listNotes(id), api.listFolders(id)]);
  trees.set(id, notes);
  folderLists.set(id, folders);
  return notes;
}

/** Reloads the tree only if it is shown. */
async function refreshTree(id: string): Promise<void> {
  if (!trees.has(id)) return;
  try {
    await loadNotes(id);
  } catch (error) {
    fail(error);
  }
  renderTree();
}

/**
 * Vault menu: the list, add and settings.
 *
 * An unavailable vault (for example, the disk is unplugged) stays in the list
 * but is disabled. A silent removal would lose its settings for an external
 * reason.
 */
function openVaultMenu(anchor: HTMLElement): void {
  const items: MenuItem[] = vaults.map((entry, index) => ({
    label: entry.name,
    detail: entry.available ? entry.path : t("menu.vault.missing"),
    color: entry.color,
    checked: entry.active,
    disabled: !entry.available,
    tone: entry.available ? undefined : "warning",
    hint: index < 9 ? formatKey(vaultKey(index + 1)) : undefined,
    run: () => void switchVault(entry.id),
  }));

  openMenu(anchor, [
    { title: t("menu.vaults"), items },
    {
      items: [
        { label: t("menu.vault.add"), glyph: "folder", key: "N", run: () => void pickVault() },
        {
          label: t("menu.vault.settings"),
          glyph: "gear",
          action: "settings.open",
          run: () => void openSettings("vaults"),
        },
      ],
    },
  ]);
}

/**
 * Command menu for a target.
 *
 * The core returns the full list: the manifest decides if a command applies to
 * this note, folder or vault. A copy of this rule in the window could drift
 * from the terminal.
 */
async function openCommandMenu(anchor: HTMLElement, target: CommandTarget): Promise<void> {
  const items = await commandItems(target);
  if (!items) return;

  // Section order follows file managers: create, then rename and move, then
  // clipboard and system, then user commands, and delete last, away from the
  // rest.
  const creation: MenuSection[] = [];
  const files: MenuSection[] = [];
  const system: MenuSection[] = [];
  const removal: MenuSection[] = [];
  const vaultId = target.vault;

  // Rename and move are shared by folders and notes: the core does both as a
  // move to a new path.
  const rename: MenuItem = {
    label: t("commands.rename"),
    glyph: "pencil",
    detail: t("commands.rename.hint", formKeys()),
    key: "R",
    run: () => startTreeRename(target),
  };
  const move: MenuItem = {
    label: t("commands.move"),
    glyph: "folderMove",
    key: "M",
    run: () => openMoveMenu(anchor, target),
  };

  // A folder menu starts with creation in it. An inactive vault becomes active
  // first, because the core creates in the active vault.
  if (target.kind === "folder" && target.name && vaultId) {
    const folder = target.name;
    const inMine = async (mode: "note" | "folder") => {
      if (vaultId !== vault?.id) await switchVault(vaultId);
      openNewNoteForm(`${folder}/`, mode);
    };
    creation.push({
      items: [
        {
          label: t("commands.folder.newNote"),
          glyph: "filePlus",
          key: "N",
          run: () => void inMine("note"),
        },
        {
          label: t("commands.folder.newFolder"),
          glyph: "folderPlus",
          key: "F",
          run: () => void inMine("folder"),
        },
      ],
    });
    files.push({ items: [rename, move] });
    system.push({
      items: [
        {
          label: t("commands.copyPath"),
          glyph: "copy",
          key: "C",
          run: () => void copyText(absolutePath(vaultId, folder)),
        },
        {
          label: t("commands.reveal"),
          glyph: "folder",
          key: "S",
          run: () => void revealFolder(folder, vaultId),
        },
      ],
    });
    removal.push({
      items: [
        {
          label: t("commands.folder.delete"),
          glyph: "close",
          tone: "danger",
          key: "⌫",
          run: () => void removeFolder(folder, vaultId),
        },
      ],
    });
  }

  if (target.kind === "note" && target.name && vaultId) {
    const note = target.name;
    files.push({
      items: [
        rename,
        move,
        {
          label: t("commands.note.duplicate"),
          glyph: "filePlus",
          key: "D",
          run: () => void duplicateNote(note, vaultId),
        },
      ],
    });
    system.push({
      items: [
        {
          label: t("commands.copyPath"),
          glyph: "copy",
          key: "C",
          run: () => void copyText(absolutePath(vaultId, `${note}.md`)),
        },
        {
          label: t("commands.note.copyLink"),
          glyph: "link",
          key: "L",
          run: () => void copyText(`[[${note}]]`),
        },
        {
          label: t("commands.note.openDefault"),
          glyph: "external",
          key: "O",
          run: () => void openExternally(note, vaultId),
        },
        {
          label: t("commands.reveal"),
          glyph: "folder",
          key: "S",
          run: () => void revealNoteIn(note, vaultId),
        },
      ],
    });
    removal.push({
      items: [
        {
          label: t("commands.note.delete"),
          glyph: "close",
          tone: "danger",
          key: "⌫",
          run: () => void removeNote(note, vaultId),
        },
      ],
    });
  }

  openMenu(anchor, [
    ...creation,
    ...files,
    ...system,
    // The menu opens from the row or the header, so it is clear whose
    // commands these are: the title does not repeat the name.
    { title: t("commands.title"), items },
    ...removal,
    editCommandsSection(),
  ]);
}

/**
 * Menu items for the commands of a target, or `null` if the core failed.
 *
 * The note menu and the standalone file menu share it, so both report a typo
 * in the commands file in the same way.
 */
async function commandItems(target: CommandTarget): Promise<MenuItem[] | null> {
  let commands: CommandView[] = [];
  try {
    const listed = await api.listCommands(target);
    commands = listed.commands;
    // Report a typo in the commands file. Else the command just does not
    // appear in the menu, and kknow looks broken.
    if (listed.problems.length > 0) setStatus(listed.problems[0], "error");
  } catch (error) {
    fail(error);
    return null;
  }

  return commands.length > 0
    ? commands.map((command, index) => ({
        label: command.name,
        detail: command.kind,
        glyph: "terminal" as const,
        // The first nine commands run by digit: they have no letters to
        // spare, and their names are the user's.
        key: index < 9 ? String(index + 1) : undefined,
        run: () => void runCommand(command, target),
      }))
    : [{ label: t("commands.none"), disabled: true, run: () => {} }];
}

/** The last menu section: the way to the command settings. */
function editCommandsSection(): MenuSection {
  return {
    items: [
      {
        label: t("commands.edit"),
        detail: t("commands.edit.detail"),
        glyph: "code",
        key: "E",
        run: () => void editCommands(),
      },
    ],
  };
}

/**
 * Runs a command and shows the result.
 *
 * Pending edits are saved first: the command reads the file from disk and
 * must see what the editor shows.
 */
async function runCommand(command: CommandView, target: CommandTarget): Promise<void> {
  if (command.confirm && !window.confirm(t("commands.confirm", { name: command.name }))) {
    return;
  }

  await flushPendingSave();
  localStorage.setItem(LAST_COMMAND, command.id);
  renderNoteTools();
  setStatus(t("commands.running", { name: command.name }), "work");

  try {
    showRunOutcome(command, await api.runCommand(command.id, target));
  } catch (error) {
    fail(error);
  }
}

/**
 * Command result: a line in the status bar and the output in its float.
 *
 * The command does not touch the preview: it shows the open note, not
 * reports, and a run button must not replace it with `git status` output. A
 * success shows in the status bar and a click opens the output. A failure
 * opens the output at once, because the user usually wants to read it.
 *
 * The output goes in as text (`textContent`), not HTML: it comes from an
 * external program, and a `<script>` in it would run with app rights. This is
 * the same reason the preview strips raw HTML.
 */
function showRunOutcome(command: CommandView, outcome: RunOutcome): void {
  const seconds = (outcome.millis / 1000).toFixed(1);
  const failed = outcome.timedOut || outcome.code !== 0;
  const summary = outcome.timedOut
    ? t("panel.run.timedOut", { seconds })
    : outcome.code === null
      ? t("panel.run.killed", { seconds })
      : outcome.code === 0
        ? t("panel.run.ok", { seconds })
        : t("panel.run.failed", { code: outcome.code, seconds });

  el.runTitle.textContent = command.name;
  el.runSummary.textContent = summary;
  el.runFloat.dataset.tone = failed ? "error" : "ok";

  // Both streams in a row: scripts print to both, and we do not judge which
  // one matters more.
  const printed = [outcome.stdout, outcome.stderr].filter((part) => part.length > 0).join("\n");
  const tail = outcome.truncated ? `\n\n[${t("panel.run.truncated")}]` : "";
  el.runOutput.textContent = printed ? `${printed}${tail}` : t("panel.run.empty");

  status.setRun(command.name, failed ? "error" : "ok", t("statusbar.run.title", { summary }));
  setStatus(`${command.name}: ${summary}`, failed ? "error" : "idle");
  showRunOutput(failed);
}

/** Shows or hides the output float. The status bar segment toggles it. */
function showRunOutput(show: boolean): void {
  el.runFloat.hidden = !show;
  status.setRunOpen(show);
}

/**
 * Command settings are a section of the settings window.
 *
 * Before, this opened Finder to edit the manifest by hand. The form knows more
 * than a text editor: the run kinds, the possible targets and the environment
 * the command gets. It also shows each problem next to the field that caused it.
 */
async function editCommands(): Promise<void> {
  try {
    await api.openSettings("commands");
  } catch (error) {
    fail(error);
  }
}

/**
 * Commands of the open note, shown by the palette.
 *
 * The list loads when the note changes and stays ready, because the palette
 * collects actions synchronously and cannot wait for the core.
 */
let noteCommands: CommandView[] = [];
/** The note the list was loaded for, as a "vault/note" key. */
let noteCommandsFor: string | null = null;

async function refreshNoteCommands(): Promise<void> {
  const target = currentTarget();
  // A standalone file is keyed by its path: it has no vault.
  const key = target ? (target.file ?? `${target.vault}/${target.name}`) : null;
  if (key === noteCommandsFor) return;

  noteCommandsFor = key;
  noteCommands = [];
  if (!target) return;

  try {
    const listed = await api.listCommands(target);
    // Another note can open during the core call; then the answer is stale.
    if (noteCommandsFor === key) {
      noteCommands = listed.commands;
      renderNoteTools();
    }
  } catch (error) {
    fail(error);
  }
}

/**
 * The open note as a command target. A standalone file is a note without a
 * vault: the core gives it the commands that are not tied to a vault or a path.
 */
function currentTarget(): CommandTarget | null {
  if (!current) return null;
  if (current.file) return { kind: "note", file: current.id };
  return vault ? { kind: "note", name: current.id, vault: vault.id } : null;
}

/** Label colors. The theme sets the actual values (`--label-*`). */
const LABEL_COLORS = ["blue", "violet", "amber", "red", "green", "teal"] as const;

/**
 * Menu of a vault row in the sidebar: actions for the whole folder.
 *
 * The "⋯" button and a right click open it. Sync settings stay in their own
 * window because they are large. This menu has the frequent actions: make
 * active, rename, reveal in Finder, remove from the list.
 */
function openVaultRowMenu(anchor: HTMLElement, entry: VaultView): void {
  openMenu(anchor, [
    {
      title: entry.name,
      items: [
        {
          label: t("sidebar.vault.activate"),
          glyph: "check",
          key: "A",
          disabled: entry.active || !entry.available,
          run: () => void switchVault(entry.id),
        },
        {
          label: t("sidebar.vault.rename"),
          glyph: "note",
          detail: t("sidebar.vault.rename.hint", formKeys()),
          key: "R",
          run: () => startRename(entry),
        },
        {
          label: t("sidebar.vault.newFolder"),
          glyph: "folderPlus",
          key: "F",
          disabled: !entry.available,
          run: () =>
            void (async () => {
              if (entry.id !== vault?.id) await switchVault(entry.id);
              openNewNoteForm("", "folder");
            })(),
        },
        {
          label: t("settings.vaults.finder"),
          glyph: "folder",
          key: "S",
          disabled: !entry.available,
          run: () => void revealVault(entry.id),
        },
        {
          // A separate item, not an inline section: the core reads the folder
          // to list commands, but this menu must open at once. It has "Remove
          // from the list", which must not wait for a folder busy with sync.
          label: t("commands.open"),
          glyph: "terminal",
          key: "K",
          disabled: !entry.available,
          run: () =>
            void openCommandMenu(anchor, { kind: "vault", vault: entry.id }),
        },
        {
          label: t("menu.vault.settings"),
          glyph: "gear",
          action: "settings.open",
          run: () => void openSettings("vaults"),
        },
      ],
    },
    {
      title: t("sidebar.vault.color"),
      items: LABEL_COLORS.map((color) => ({
        label: t(`color.${color}`),
        color,
        checked: color === entry.color,
        run: () => void recolorVault(entry.id, color),
      })),
    },
    {
      items: [
        {
          label: t("sidebar.vault.remove"),
          glyph: "unlink",
          tone: "danger",
          detail: t("settings.vaults.remove.detail"),
          key: "⌫",
          run: () => void forgetVault(entry),
        },
      ],
    },
  ]);
}

/**
 * Inline rename in the tree: an input replaces the row.
 *
 * The name identifies the vault in the sidebar and in search results, so the
 * user renames it where they see it, not in the settings window.
 */
function startRename(entry: VaultView): void {
  const row = el.noteList.querySelector<HTMLElement>(
    `.vault-row[data-vault="${CSS.escape(entry.id)}"]`,
  );
  renameInTree(row, "button.vault", entry.name, (wanted) => renameVault(entry.id, wanted));
}

/**
 * Inline rename of a note or folder in the tree.
 *
 * The input holds the file name, not the title: the title lives in the note
 * text and is edited there. The folder stays the same; "Move to…" changes it.
 */
function startTreeRename(target: CommandTarget): void {
  const { name, vault: vaultId } = target;
  if (!name || !vaultId) return;
  // The input goes into a tree row. A rename from the header menu cannot
  // reach it while the sidebar is collapsed, so open the sidebar.
  if (sidebarHidden) setSidebarHidden(false);

  const folder = folderOf(name);
  const row =
    target.kind === "note"
      ? el.noteList.querySelector<HTMLElement>(
          `.note-row[data-vault="${CSS.escape(vaultId)}"][data-id="${CSS.escape(name)}"]`,
        )
      : el.noteList.querySelector<HTMLElement>(
          `.folder-row[data-vault="${CSS.escape(vaultId)}"][data-path="${CSS.escape(name)}"]`,
        );
  const button = target.kind === "note" ? "button.note" : "button.folder";

  renameInTree(row, button, leafOf(name), (wanted) =>
    target.kind === "note"
      ? renameNote(name, join(folder, wanted), vaultId, "status.renamed")
      : renameFolder(name, join(folder, wanted), vaultId, "status.renamed"),
  );
}

/**
 * Rename input in place of a tree row. Vaults, folders and notes share it:
 * their rows have the same layout, a button plus the row buttons.
 *
 * The input is not inside the row button (it would not get clicks there);
 * the button hides instead.
 */
function renameInTree(
  row: HTMLElement | null,
  buttonSelector: string,
  initial: string,
  save: (wanted: string) => Promise<void>,
): void {
  const button = row?.querySelector<HTMLButtonElement>(buttonSelector);
  if (!row || !button) return;

  const input = document.createElement("input");
  input.className = "tree-rename";
  input.value = initial;
  input.setAttribute("aria-label", t("sidebar.vault.rename"));
  button.hidden = true;
  row.prepend(input);
  input.focus();
  input.select();

  let done = false;
  const finish = async (saving: boolean): Promise<void> => {
    if (done) return;
    done = true;

    const wanted = input.value.trim();
    if (saving && wanted && wanted !== initial) {
      await save(wanted);
    }
    // The redraw restores the row with either the new or the old name.
    renderTree();
  };

  input.onkeydown = (event) => {
    if (matches(event, "submit")) {
      event.preventDefault();
      void finish(true);
    } else if (matches(event, "cancel")) {
      event.preventDefault();
      void finish(false);
    }
  };
  // Blur saves, as tree rename does everywhere.
  input.onblur = () => void finish(true);
}

async function renameVault(id: string, name: string): Promise<void> {
  try {
    vaults = await api.renameVault(id, name);
    // The status bar also shows the active vault name.
    if (vault?.id === id) {
      vault = { ...vault, name };
      status.setVault(name, vault.color);
    }
  } catch (error) {
    fail(error);
  }
}

/**
 * "Move to…" menu: a list of vault folders instead of a submenu.
 *
 * Our menu has no submenus, and one item does not justify them: a second list
 * in the same place reads just as well.
 */
function openMoveMenu(anchor: HTMLElement, target: CommandTarget): void {
  const { name, vault: vaultId } = target;
  if (!name || !vaultId) return;

  const leaf = leafOf(name);
  const parent = folderOf(name);
  const choices = ["", ...(folderLists.get(vaultId) ?? [])]
    .filter((folder) => folder !== parent)
    // A folder cannot move into itself.
    .filter(
      (folder) =>
        target.kind !== "folder" || (folder !== name && !folder.startsWith(`${name}/`)),
    )
    .sort((a, b) => collator.compare(a, b));

  const items: MenuItem[] =
    choices.length > 0
      ? choices.map((folder) => ({
          label: folder || t("commands.move.root"),
          glyph: "folder" as const,
          run: () =>
            void (target.kind === "note"
              ? renameNote(name, join(folder, leaf), vaultId, "status.moved")
              : renameFolder(name, join(folder, leaf), vaultId, "status.moved")),
        }))
      : [{ label: t("commands.move.none"), disabled: true, run: () => {} }];

  openMenu(anchor, [{ title: t("commands.move.title", { name: leaf }), items }]);
}

/** Parent folder of a path: `a/b/c` → `a/b`; an empty string for the root. */
function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at > 0 ? path.slice(0, at) : "";
}

function leafOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

function join(folder: string, name: string): string {
  return folder ? `${folder}/${name}` : name;
}

/**
 * Renames or moves a note. An open note stays open under the new name.
 * Pending edits are saved before the move: else they would go to the old path
 * and bring the file back.
 */
async function renameNote(
  id: string,
  to: string,
  vaultId: string,
  done: "status.renamed" | "status.moved",
): Promise<void> {
  const isCurrent = vaultId === vault?.id && current?.id === id;
  if (isCurrent) await flushPendingSave();

  try {
    const { id: moved, relinked } = await api.renameNote(id, to, vaultId);
    moveScale(noteKey(vaultId, id), noteKey(vaultId, moved));
    moveMode(noteKey(vaultId, id), noteKey(vaultId, moved));
    await refreshTree(vaultId);
    if (isCurrent) await openNote(moved);
    setStatus(movedStatus(done, relinked));
  } catch (error) {
    fail(error);
  }
}

/** Renames or moves a folder. An open note inside it moves too. */
async function renameFolder(
  name: string,
  to: string,
  vaultId: string,
  done: "status.renamed" | "status.moved",
): Promise<void> {
  const prefix = `${name}/`;
  const inside = vaultId === vault?.id && current?.id.startsWith(prefix) ? current.id : null;
  if (inside) await flushPendingSave();

  try {
    const relinked = await api.renameFolder(name, to, vaultId);
    await refreshTree(vaultId);
    if (inside) await openNote(`${to}/${inside.slice(prefix.length)}`);
    setStatus(movedStatus(done, relinked));
  } catch (error) {
    fail(error);
  }
}

/** "Renamed", plus the number of notes whose links the core rewrote. */
function movedStatus(done: "status.renamed" | "status.moved", relinked: number): string {
  return relinked > 0 ? `${t(done)} · ${t("status.relinked", { count: relinked })}` : t(done);
}

/** Copies a note next to it and opens the copy, since that is the goal. */
async function duplicateNote(id: string, vaultId: string): Promise<void> {
  try {
    const copy = await api.duplicateNote(id, vaultId);
    await refreshTree(vaultId);
    setStatus(t("status.duplicated"));
    await openFound(copy, vaultId);
  } catch (error) {
    fail(error);
  }
}

/** Opens the note file in an external app after pending edits are saved. */
async function openExternally(id: string, vaultId: string): Promise<void> {
  await flushPendingSave();
  try {
    await api.openExternally(id, vaultId);
  } catch (error) {
    fail(error);
  }
}

async function revealNoteIn(id: string, vaultId: string): Promise<void> {
  try {
    await api.revealNote(id, vaultId);
  } catch (error) {
    fail(error);
  }
}

async function revealFolder(name: string, vaultId: string): Promise<void> {
  try {
    await api.revealFolder(name, vaultId);
  } catch (error) {
    fail(error);
  }
}

/** Absolute path inside a vault, for the clipboard. */
function absolutePath(vaultId: string, relative: string): string | null {
  const root =
    vaultId === vault?.id ? vault.root : vaults.find((entry) => entry.id === vaultId)?.path;
  return root ? `${root}/${relative}` : null;
}

/**
 * Puts text on the clipboard.
 *
 * First try `navigator.clipboard`. If the webview blocks it (no permission or
 * an insecure context), fall back to the old `execCommand("copy")` with a
 * hidden field: it works with any user gesture, and a menu click is one.
 */
async function copyText(text: string | null): Promise<void> {
  if (text === null) return;
  try {
    await navigator.clipboard.writeText(text);
    setStatus(t("status.copied"));
    return;
  } catch {
    // Fall back below.
  }

  const field = document.createElement("textarea");
  field.value = text;
  field.setAttribute("readonly", "");
  field.style.position = "fixed";
  field.style.opacity = "0";
  document.body.append(field);
  field.select();
  const copied = document.execCommand("copy");
  field.remove();
  setStatus(t(copied ? "status.copied" : "status.copyFailed"), copied ? "idle" : "error");
}

async function recolorVault(id: string, color: string): Promise<void> {
  try {
    vaults = await api.setVaultColor(id, color);
    if (vault?.id === id) {
      vault = { ...vault, color };
      status.setVault(vault.name, color);
    }
  } catch (error) {
    fail(error);
  }
  renderTree();
}

/** Forgets a vault: the entry leaves the list, the notes stay on disk. */
async function forgetVault(entry: VaultView): Promise<void> {
  try {
    vaults = await api.removeVault(entry.id);
  } catch (error) {
    fail(error);
    return;
  }

  trees.delete(entry.id);
  folderLists.delete(entry.id);
  collapsed.delete(entry.id);
  expanded.delete(entry.id);
  rememberExpanded();

  // If the active vault was removed, the core already picked another; open it.
  if (entry.id === vault?.id) {
    const info = await api.currentVault().catch(() => null);
    if (info) {
      await useVault(info);
    } else {
      vault = null;
      current = null;
      status.setVault(null);
      showHeading(null);
      editor.setText("");
    }
  }

  renderTree();
  setStatus(t("sidebar.vault.removed", { name: entry.name }));
}


async function revealVault(id: string): Promise<void> {
  try {
    await api.revealNote(undefined, id);
  } catch (error) {
    fail(error);
  }
}

/**
 * "Drop here" hint while a folder is dragged over the window.
 *
 * It lives in `body`, not `#app`: `#app` is a grid, and the hint needs the
 * whole window, not a grid cell. Pointer events pass through it: the shell
 * handles the drag, and the hint has nothing to click.
 */
const dropHint = document.createElement("div");
dropHint.className = "drop-hint";
dropHint.textContent = t("sidebar.vault.drop");
dropHint.hidden = true;
document.body.append(dropHint);

/** Shows a vault: tree, themes, sync and its last note. */
async function useVault(info: VaultInfo): Promise<void> {
  // The position key includes the old vault, so save before the switch.
  stashPosition();
  vault = info;
  current = null;
  status.setVault(info.name, info.color);

  // Collapsed folders and recent notes are per vault: ⌘2 must return to where
  // the user left, not to another vault's tree.
  loadVaultMemory(info.id);

  // Expand the active vault in the sidebar: the user came for it.
  expanded.add(info.id);
  rememberExpanded();
  const notes = await loadNotes(info.id);
  renderTree();

  // Themes and sync settings live in the vault folder, so reload both after a
  // switch.
  await refreshThemes();
  await refreshSync();
  setStatus(t("status.notes", { count: info.noteCount }));

  const wanted = localStorage.getItem(`kknow:note:${info.id}`);
  const restore = notes.find((note) => note.id === wanted) ?? notes[0];
  if (restore) {
    await openNote(restore.id);
  } else {
    // Empty folder: no note is open, so the header, status bar and editor must
    // not keep a note from the previous vault.
    showHeading(null);
    status.setWords(0);
    editor.setText("");
    showPreview("");
  }
}

/** Switches the active vault: ⌘1…⌘9, the menu, or a result from another vault. */
async function switchVault(id: string): Promise<void> {
  if (vault?.id === id) return;
  await flushPendingSave();

  try {
    const info = await api.activateVault(id);
    await refreshVaults();
    await useVault(info);
  } catch (error) {
    fail(error);
  }
}

async function refreshVaults(): Promise<void> {
  try {
    vaults = await api.listVaults();
  } catch (error) {
    fail(error);
  }
}

async function pickVault(): Promise<void> {
  const picked = await open({ directory: true, multiple: false });
  if (typeof picked !== "string") return;
  await adoptVaults([picked]);
}

/**
 * Adds vaults from paths. Both the folder picker and drag and drop come here.
 *
 * Several folders can arrive, and one bad path must not cancel the others.
 * The core rejects files, duplicates and nested folders: we report those and
 * add the valid ones.
 */
async function adoptVaults(paths: string[]): Promise<void> {
  let added: VaultInfo | null = null;
  let problem: string | null = null;

  for (const path of paths) {
    try {
      added = await api.addVault(path);
    } catch (error) {
      problem ??= String(error);
    }
  }

  await refreshVaults();
  if (added) {
    // The core already made the last added vault active: the user wants it.
    await useVault(added);
  } else {
    renderTree();
  }
  if (problem) setStatus(problem, "error");
}

/** Extensions the window opens as a separate file. They match the vault ones. */
const MARKDOWN = /\.(md|markdown|mdown)$/i;

/** Reloads the separate file list. The core forgets missing files on the way. */
async function refreshFiles(): Promise<void> {
  try {
    files = await api.listFiles();
  } catch (error) {
    fail(error);
  }
  renderFiles();
}

/**
 * Separate files as a flat list in open order.
 *
 * Rows show the file name, not the title: titles would need a disk read per
 * file, and people know `README.md` by its name. The folder on the second
 * line tells two `README.md` files apart.
 */
function renderFiles(): void {
  el.fileList.replaceChildren(...files.map(fileItem));
  markActive(current?.id ?? null);
}

function fileItem(entry: FileEntry): HTMLLIElement {
  const item = document.createElement("li");
  const button = document.createElement("button");
  button.className = "note";
  button.dataset.path = entry.path;
  button.title = entry.path;
  button.onclick = () => void openFileView(entry.path);

  const name = document.createElement("span");
  name.className = "note-title";
  name.textContent = entry.name;
  const folder = document.createElement("span");
  folder.className = "file-folder";
  folder.textContent = entry.folder;
  button.append(name, folder);

  const more = rowMenu(t("sidebar.file.menu", { name: entry.name }), (anchor) =>
    openFileMenu(anchor, entry),
  );

  const row = document.createElement("div");
  row.className = "tree-row note-row";
  row.append(button, rowActions(more));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    void openFileMenu(more, entry);
  };
  item.append(row);
  return item;
}

/**
 * Menu of a standalone file. It has the note commands too: a file is a note
 * without a vault. The order is the same as in the note menu.
 */
async function openFileMenu(anchor: HTMLElement, entry: FileEntry): Promise<void> {
  const target: CommandTarget = { kind: "note", file: entry.path };
  const items = await commandItems(target);
  if (!items) return;

  openMenu(anchor, [
    {
      title: entry.name,
      items: [
        {
          label: t("commands.reveal"),
          glyph: "folder",
          key: "S",
          run: () =>
            void api.revealFile(entry.path).catch((error) => {
              void fileGone(entry.path).then((gone) => gone || fail(error));
            }),
        },
        {
          label: t("commands.copyPath"),
          glyph: "copy",
          key: "C",
          run: () => void copyText(entry.path),
        },
      ],
    },
    { title: t("commands.title"), items },
    {
      items: [
        {
          label: t("sidebar.vault.remove"),
          glyph: "unlink",
          detail: t("sidebar.file.remove.detail"),
          key: "⌫",
          run: () => void forgetFile(entry),
        },
      ],
    },
    editCommandsSection(),
  ]);
}

/** Removes a file from the list. An open file closes; the file on disk stays. */
async function forgetFile(entry: FileEntry): Promise<void> {
  if (current?.id === entry.path) {
    await flushPendingSave();
    closeNote();
    localStorage.removeItem(LAST_FILE);
  }
  try {
    files = await api.forgetFile(entry.path);
    renderFiles();
    setStatus(t("sidebar.file.removed", { name: entry.name }));
  } catch (error) {
    fail(error);
  }
}

async function openFileView(path: string): Promise<void> {
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

/**
 * Shows what the user asked to open: a file from a known vault opens as its
 * note, anything else as a separate file.
 */
async function openPath(opened: Opened): Promise<void> {
  if (opened.kind === "note") {
    await refreshVaults();
    await openFound(opened.id, opened.vault);
    return;
  }
  await refreshFiles();
  await openFileView(opened.path);
}

/** An open request from Finder. The core holds it until the window takes it. */
async function takeOpened(): Promise<boolean> {
  try {
    const opened = await api.takeOpenedFile();
    if (!opened) return false;
    await openPath(opened);
    return true;
  } catch (error) {
    fail(error);
    return false;
  }
}

/** "Open file…": the dialog, ⌘O and the button above the file list. */
async function pickFile(): Promise<void> {
  const picked = await open({
    multiple: false,
    filters: [{ name: "Markdown", extensions: ["md", "markdown", "mdown"] }],
  });
  if (typeof picked !== "string") return;
  await openFiles([picked]);
}

/**
 * Opens files by path, from the dialog or drag and drop. All go into the list,
 * the last one opens, and a bad path does not cancel the others.
 */
async function openFiles(paths: string[]): Promise<void> {
  let last: Opened | null = null;
  let problem: string | null = null;
  for (const path of paths) {
    try {
      last = await api.openFile(path);
    } catch (error) {
      problem ??= String(error);
    }
  }

  if (last) await openPath(last);
  else await refreshFiles();
  if (problem) setStatus(problem, "error");
}

/** Dropped paths: folders become vaults, markdown files open. */
async function adoptDropped(paths: string[]): Promise<void> {
  const markdown = paths.filter((path) => MARKDOWN.test(path));
  const folders = paths.filter((path) => !MARKDOWN.test(path));
  if (folders.length > 0) await adoptVaults(folders);
  if (markdown.length > 0) await openFiles(markdown);
}

/**
 * Saves a pasted image of the open note. The core puts it into `attachments/`
 * of the vault, or next to a standalone file. Returns the path for the link.
 */
async function savePastedImage(name: string, bytes: Uint8Array): Promise<string | null> {
  if (!current) return null;
  try {
    return await api.saveAttachment(name, bytes, current.file ? current.id : null);
  } catch (error) {
    setStatus(t("image.saveFailed", { error: String(error) }), "error");
    return null;
  }
}

/**
 * Image files dropped on the editor: the core copies them into `attachments/`,
 * and the links go to the drop point. The note can change during the copy;
 * then the links have no place to go.
 */
async function attachDropped(paths: string[], at: number): Promise<void> {
  const note = current;
  if (!note) return;
  const saved: string[] = [];
  let problem: string | null = null;
  for (const path of paths) {
    try {
      saved.push(await api.attachFile(path, note.file ? note.id : null));
    } catch (error) {
      problem ??= String(error);
    }
  }
  if (!current || positionKey(current) !== positionKey(note)) return;
  editor.insertImages(saved.map(imageLink), at);
  if (problem) setStatus(t("image.saveFailed", { error: problem }), "error");
}

async function createNote(name: string): Promise<void> {
  if (!vault) return;
  try {
    const view = await api.createNote(name);
    await refreshTree(vault.id);
    applyNote(view);
    rememberOpened(view.id);
    editor.focus();
    setStatus(t("status.created"));
  } catch (error) {
    fail(error);
  }
}

async function createFolder(name: string): Promise<void> {
  if (!vault) return;
  try {
    await api.createFolder(name);
    await refreshTree(vault.id);
    setStatus(t("status.folderCreated"));
  } catch (error) {
    fail(error);
  }
}

/** Deletes a note after confirmation. An open note also leaves the editor. */
async function removeNote(id: string, vaultId: string): Promise<void> {
  if (!window.confirm(t("confirm.deleteNote", { name: id }))) return;

  // Drop pending edits of the deleted note: a save after the delete would
  // create the file again.
  const isCurrent = vaultId === vault?.id && current?.id === id;
  if (isCurrent && saveTimer !== undefined) {
    window.clearTimeout(saveTimer);
    saveTimer = undefined;
  }

  try {
    await api.deleteNote(id, vaultId);
    if (isCurrent) closeNote();
    await refreshTree(vaultId);
    setStatus(t("status.noteDeleted"));
  } catch (error) {
    fail(error);
  }
}

/** Deletes a folder with all its notes after confirmation. */
async function removeFolder(name: string, vaultId: string): Promise<void> {
  if (!window.confirm(t("confirm.deleteFolder", { name }))) return;

  const prefix = `${name}/`;
  const isCurrent = vaultId === vault?.id && current?.id.startsWith(prefix);
  if (isCurrent && saveTimer !== undefined) {
    window.clearTimeout(saveTimer);
    saveTimer = undefined;
  }

  try {
    await api.deleteFolder(name, vaultId);
    if (isCurrent) closeNote();
    await refreshTree(vaultId);
    setStatus(t("status.folderDeleted"));
  } catch (error) {
    fail(error);
  }
}

/** Shows the sync result: a summary on top, details as a list. */
function showSyncPanel(summary: string, items: HTMLLIElement[]): void {
  // The sync result lives in the right pane, so the pane must be visible.
  // The mode is not saved for the note: the user did not pick it.
  if (mode === "edit") setMode("split", { remember: false });
  el.syncSummary.textContent = summary;
  el.syncList.replaceChildren(...items);
  el.syncPanel.hidden = false;
}

/** Conflicts: the local version stays in place, the remote one is a copy next to it. */
function conflictItems(conflicts: SyncConflict[]): HTMLLIElement[] {
  return conflicts.map((conflict) => {
    const item = document.createElement("li");

    const mine = document.createElement("button");
    mine.className = "link";
    mine.textContent = conflict.id;
    mine.onclick = () => void openNote(conflict.id);

    const arrow = document.createElement("span");
    arrow.className = "muted";
    arrow.textContent = t("sync.conflict.arrow");

    const theirs = document.createElement("button");
    theirs.className = "link";
    theirs.textContent = conflict.copy;
    theirs.onclick = () => void openNote(conflict.copy);

    item.append(mine, arrow, theirs);
    return item;
  });
}

function describeReport(report: SyncReport): string {
  const counts: [Key, number][] = [
    ["sync.report.uploaded", report.uploaded],
    ["sync.report.downloaded", report.downloaded],
    ["sync.report.removedLocal", report.removedLocal],
    ["sync.report.removedRemote", report.removedRemote],
    ["sync.report.conflicts", report.conflicts.length],
  ];

  const parts = counts
    .filter(([, count]) => count > 0)
    .map(([label, count]) => t("sync.report.entry", { label: t(label), count }));

  return parts.length > 0 ? parts.join(", ") : t("sync.report.upToDate");
}

async function runSync(): Promise<void> {
  // A vault without sync settings cannot sync: open the settings section that
  // configures it.
  if (!remote) {
    void openSettings("vaults");
    return;
  }
  if (syncing) return;

  syncing = true;
  setStatus(t("status.syncing"), "work");
  // Save pending edits now, else they would sync only next time.
  await flushPendingSave();

  try {
    const report = await api.syncNow();
    showSyncPanel(describeReport(report), conflictItems(report.conflicts));
    setStatus(
      report.conflicts.length > 0
        ? t("status.conflicts", { count: report.conflicts.length })
        : t("status.synced"),
    );
  } catch (error) {
    // The core already set the segment state to error; only the report is left.
    showSyncPanel(String(error), []);
    fail(error);
  } finally {
    syncing = false;
    await refreshStatuses();
  }
}

/** Syncs an inactive vault from the chip. The active vault does not change. */
async function syncVault(id: string): Promise<void> {
  try {
    await api.syncNow(id);
  } catch (error) {
    fail(error);
  }
  await refreshStatuses();
}

/** Sync states of all open vaults. The chip shows them too. */
async function refreshStatuses(): Promise<void> {
  try {
    const all = await api.syncStatuses();
    statuses = new Map(all.map((entry) => [entry.vault, entry]));
  } catch (error) {
    fail(error);
  }
  describeRemote();
}

/** Short sync target: a bucket or a folder name. */
function shortLocation(location: string | null): string {
  if (!location) return t("statusbar.sync.off");
  // The folder name for a folder, the address for a bucket: both are recognizable.
  const tail = location.replace(/\/+$/, "").split("/").filter(Boolean).pop();
  return tail ?? location;
}

/**
 * Time since the last successful sync.
 *
 * The window computes it, not the core: "2 min ago" depends on the language,
 * and the time comes as a number.
 */
function timeAgo(at: number): string {
  const minutes = Math.floor((Date.now() - at) / 60000);
  if (minutes < 1) return t("statusbar.sync.justNow");
  if (minutes < 60) return t("statusbar.sync.minutesAgo", { count: minutes });
  return t("statusbar.sync.hoursAgo", { count: Math.floor(minutes / 60) });
}

/**
 * Sync state in the status bar: off, ready, busy, error.
 *
 * Only this function decides what to show. Else, after an error, another
 * handler could switch the segment back to "ready".
 */
function describeRemote(): void {
  const own = vault ? statuses.get(vault.id) : undefined;
  showChip();

  if (!own || own.state === "off") {
    // Unconfigured sync is muted: a click opens the form, not a sync.
    status.setSync(
      t("statusbar.sync.off"),
      "off",
      withShortcut(t("sync.title.unset"), "settings.open"),
    );
    return;
  }

  const place = shortLocation(own.location);
  if (own.state === "syncing") {
    const of = t("statusbar.sync.progress", { done: own.done, total: own.total });
    status.setSync(own.total > 0 ? of : t("statusbar.sync.busy"), "busy", t("status.syncing"));
  } else if (own.state === "error") {
    status.setSync(t("statusbar.sync.error"), "error", own.message ?? "");
  } else {
    const when = own.at ? ` · ${timeAgo(own.at)}` : "";
    status.setSync(
      `${place}${when}`,
      own.state === "conflict" ? "conflict" : "ready",
      own.state === "conflict"
        ? t("statusbar.sync.conflicts", { count: own.conflicts })
        : t("sync.title", {
            location: own.location ?? place,
            sync: shortcut("vault.sync"),
            settings: shortcut("settings.open"),
          }),
    );
  }
}

/**
 * Chip about a problem elsewhere: a conflict or a sync failure in an inactive
 * vault.
 *
 * The active vault uses the sync segment, so the chip shows only other vaults,
 * and only one of them.
 */
function showChip(): void {
  const other = [...statuses.values()]
    .filter((entry) => entry.vault !== vault?.id)
    .find((entry) => entry.state === "conflict" || entry.state === "error");

  if (!other) {
    status.setChip(null);
    return;
  }

  const name = vaults.find((entry) => entry.id === other.vault)?.name ?? other.vault;
  status.setChip(
    other.state === "conflict"
      ? t("statusbar.chip.conflicts", { name, count: other.conflicts })
      : t("statusbar.chip.error", { name }),
    other.state === "conflict" ? "warning" : "danger",
  );
}

/** Chip menu: go to the vault with the problem and resolve it. */
function openChipMenu(anchor: HTMLElement): void {
  const other = [...statuses.values()]
    .filter((entry) => entry.vault !== vault?.id)
    .find((entry) => entry.state === "conflict" || entry.state === "error");
  if (!other) return;

  const name = vaults.find((entry) => entry.id === other.vault)?.name ?? other.vault;
  openMenu(anchor, [
    {
      title: name,
      items: [
        {
          label: t("statusbar.chip.resolve"),
          glyph: "search",
          key: "R",
          run: () => void resolveConflicts(other.vault),
        },
        {
          label: t("palette.action.sync"),
          glyph: "cloud",
          key: "S",
          run: () => void syncVault(other.vault),
        },
      ],
    },
  ]);
}

/**
 * "Resolve": switch to the vault and show the conflict copies.
 *
 * A copy is a file with a marker in its name, so search is the way to find
 * it: a query for the marker lists everything to resolve.
 */
async function resolveConflicts(id: string): Promise<void> {
  await switchVault(id);
  palette.open(t("statusbar.chip.query"));
}

/** Sync settings belong to the vault folder, so they load with it. */
async function refreshSync(): Promise<void> {
  el.syncPanel.hidden = true;
  try {
    remote = await api.syncSettings();
  } catch (error) {
    remote = null;
    fail(error);
  }
  await refreshStatuses();
}

/** The settings window is separate, so the core opens it, not this page. */
async function openSettings(section?: string): Promise<void> {
  try {
    await api.openSettings(section);
  } catch (error) {
    fail(error);
  }
}

/** Writes settings to the core. Both windows learn about the change from an event. */
async function saveSettings(change: Partial<AppSettings>): Promise<void> {
  if (!appSettings) return;
  try {
    appSettings = await api.setSettings({ ...appSettings, ...change });
  } catch (error) {
    fail(error);
  }
}

/**
 * The status bar button. It changes the setting, not the editor: the settings
 * window shows the same switch, and both learn about the change the same way.
 */
async function toggleLineNumbers(): Promise<void> {
  if (!appSettings) return;
  await saveSettings({ lineNumbers: !appSettings.lineNumbers });
  // The event also comes here, but the mock does not send events.
  showLineNumbers(appSettings.lineNumbers);
}

function showLineNumbers(on: boolean): void {
  editor.setLineNumbers(on);
  status.setLineNumbers(on);
}

/** Applies settings from the core: language, theme, preview, line numbers. */
function applySettings(incoming: AppSettings): void {
  const localeChanged = incoming.locale !== locale;
  const defaultChanged = incoming.viewMode !== appSettings?.viewMode;
  const scaleChanged = incoming.noteScale !== appSettings?.noteScale;
  const labelChanged = incoming.noteLabel !== appSettings?.noteLabel;
  appSettings = incoming;
  if (labelChanged) renderTree();
  showLineNumbers(incoming.lineNumbers);
  // A new default size affects only a file without its own size.
  if (scaleChanged) {
    setDefaultScale(incoming.noteScale);
    applyScale();
  }

  if (incoming.theme !== currentTheme) applyTheme(incoming.theme, { remember: false });
  // A new default affects only a note without a saved mode.
  if (defaultChanged) {
    setMode(current ? modeOf(positionKey(current), incoming.viewMode) : incoming.viewMode, {
      remember: false,
    });
  }

  // Labels are set in the markup when the window builds, so a language change
  // needs a reload. The localStorage mirror updates before it.
  if (localeChanged) {
    setLocale(incoming.locale);
    void flushPendingSave().then(() => location.reload());
  }
}

for (const button of el.modeButtons) {
  button.onclick = () => setMode(button.dataset.mode as ViewMode);
}
el.toggleSidebar.onclick = () => setSidebarHidden(!sidebarHidden);
el.navBack.onclick = () => void travel(-1);
el.navForward.onclick = () => void travel(1);
el.noteMenu.onclick = () => openNoteMenu();

/**
 * Menu of the open note. It is the same as its tree row menu: commands,
 * rename, clipboard, Finder. One menu for two places keeps them in
 * sync. A separate file uses its row menu from the file list.
 */
function openNoteMenu(): void {
  if (!current) return;
  if (current.file) {
    const path = current.id;
    const entry = files.find((candidate) => candidate.path === path);
    if (entry) void openFileMenu(el.noteMenu, entry);
    return;
  }
  if (!vault) return;
  void openCommandMenu(el.noteMenu, { kind: "note", name: current.id, vault: vault.id });
}

/**
 * The last command run from any menu. The run button of the note toolbar
 * repeats it: usually one command is run again and again, for example a
 * build or a publish.
 */
const LAST_COMMAND = "kknow:command:last";

/** The last command, if the open note has it. */
function lastCommand(): CommandView | null {
  const id = localStorage.getItem(LAST_COMMAND);
  return noteCommands.find((command) => command.id === id) ?? null;
}

/**
 * Updates the note toolbar: it shows the state of the open note.
 *
 * Call it when the note, its bookmarks or its commands change.
 */
function renderNoteTools(): void {
  el.noteTools.hidden = current === null;
  if (!current) return;

  // The core keeps bookmarks and links per vault: a standalone file has none.
  el.noteBookmarks.hidden = current.file === true;
  el.noteBacklinks.hidden = current.file === true;

  const marks = editor.getBookmarks().length;
  el.bookmarksCount.textContent = marks > 0 ? String(marks) : "";
  el.noteBookmarks.dataset.on = String(marks > 0);

  // The button stays in place with zero links: a toolbar that changes its
  // width from note to note moves the buttons from under the cursor.
  const links = current.backlinks.length;
  el.backlinksCount.textContent = String(links);
  el.noteBacklinks.disabled = links === 0;

  const last = lastCommand();
  const title = last
    ? t("topbar.commands.run", { name: last.name })
    : t("topbar.commands.title");
  el.runLast.title = title;
  el.runLast.setAttribute("aria-label", title);
}

/**
 * Bookmarks of the open note: a jump to each, then the toggle and the list of
 * all bookmarks. The toolbar has no line of its own, so the toggle works on
 * the cursor line, as ⌘⇧B does.
 */
function openBookmarksMenu(): void {
  if (!current || current.file) return;
  const id = current.id;
  const marks = editor.getBookmarkMarks();
  const text = editor.getText().split("\n");

  const jumps: MenuItem[] =
    marks.length > 0
      ? marks.map(({ line, name }, index) => ({
          label: name ?? bookmarkLabel(text[line - 1] ?? ""),
          detail: t("topbar.bookmarks.line", { line }),
          glyph: "bookmark",
          key: index < 9 ? String(index + 1) : undefined,
          run: () => void openBookmark(id, line),
        }))
      : [{ label: t("topbar.bookmarks.none"), disabled: true, run: () => {} }];

  openMenu(el.noteBookmarks, [
    { title: t("topbar.bookmarks.here"), items: jumps },
    {
      items: [
        {
          label: t("palette.action.bookmark"),
          glyph: "bookmark",
          action: "bookmark.toggle",
          run: () => toggleCurrentBookmark(),
        },
        {
          label: t("topbar.bookmarks.all"),
          glyph: "search",
          action: "bookmark.list",
          run: () => palette.open("", "bookmarks"),
        },
      ],
    },
  ]);
}

/**
 * A bookmark line as a menu label: the text without the markdown marker and
 * without agent notes, which are HTML comments in the text.
 */
function bookmarkLabel(line: string): string {
  const text = line
    .replace(/<!--.*?(-->|$)/g, "")
    .replace(/^\s*(#{1,6}\s+|[-*+]\s+(\[.\]\s+)?|\d+[.)]\s+|>\s*)/, "")
    .trim();
  return text || t("topbar.bookmarks.blank");
}

/** Commands of the open note, the same list as in its menu. */
async function openRunMenu(): Promise<void> {
  const target = currentTarget();
  if (!target) return;
  const items = await commandItems(target);
  if (!items) return;
  openMenu(el.runMenu, [{ title: t("commands.title"), items }, editCommandsSection()]);
}

/** Notes that link to the open one. The panel under the preview lists them too. */
function openBacklinksMenu(): void {
  if (!current) return;
  openMenu(el.noteBacklinks, [
    {
      title: t("panel.backlinks.title"),
      items: current.backlinks.map((link, index) => ({
        label: link.title,
        detail: link.label ? `«${link.label}»` : undefined,
        glyph: "note",
        key: index < 9 ? String(index + 1) : undefined,
        run: () => void openNote(link.id),
      })),
    },
  ]);
}

el.noteBookmarks.onclick = () => openBookmarksMenu();
el.runLast.onclick = () => {
  const last = lastCommand();
  const target = currentTarget();
  // Nothing to repeat yet: the menu shows what can run.
  if (last && target) void runCommand(last, target);
  else void openRunMenu();
};
el.runMenu.onclick = () => void openRunMenu();
el.noteBacklinks.onclick = () => openBacklinksMenu();

el.syncHide.onclick = () => {
  el.syncPanel.hidden = true;
};
el.runHide.onclick = () => showRunOutput(false);
el.toggleNotes.onclick = () => showNotesFloat(el.notesFloat.hidden);
el.notesHide.onclick = () => showNotesFloat(false);

el.addVault.onclick = () => void pickVault();

/** Collapses every vault. Folders inside keep their state for the next expand. */
el.collapseAll.onclick = () => {
  expanded.clear();
  rememberExpanded();
  renderTree();
};

async function reloadVault(): Promise<void> {
  if (!vault) return;
  await flushPendingSave();
  try {
    const info = await api.reload();
    await refreshTree(vault.id);
    setStatus(t("status.notes", { count: info.noteCount }));
    if (current && !current.file) applyNote(await api.loadNote(current.id));
  } catch (error) {
    fail(error);
  }
}

/** What the form creates. Notes and folders share the input; only the action differs. */
let newNoteMode: "note" | "folder" = "note";

/**
 * Opens the new note or folder form. The prefix is the target folder.
 *
 * The folder goes into the input as text, not into hidden state: the user sees
 * the path and can erase it. The form always does exactly what it shows.
 */
function openNewNoteForm(prefix = "", mode: "note" | "folder" = "note"): void {
  newNoteMode = mode;
  el.newNoteName.placeholder = t(
    mode === "note" ? "sidebar.newNote.placeholder" : "sidebar.newFolder.placeholder",
  );
  el.newNoteForm.hidden = false;
  el.newNoteName.value = prefix;
  el.newNoteName.focus();
  // Cursor at the end: the user types the name after the folder.
  el.newNoteName.setSelectionRange(prefix.length, prefix.length);
}

/** Folder of the open note. ⌘N creates the next note there. */
function currentFolderPrefix(): string {
  // A separate file path is not a vault folder: the new note goes to the root.
  if (current?.file) return "";
  const parts = current?.id.split("/") ?? [];
  parts.pop();
  return parts.length > 0 ? `${parts.join("/")}/` : "";
}

el.openFile.onclick = () => void pickFile();

/**
 * Sidebar buttons and ⌘N. A second click on the same button hides the form.
 * The form is in the sidebar, so a collapsed sidebar opens first.
 */
function toggleNewNoteForm(mode: "note" | "folder"): void {
  if (!el.newNoteForm.hidden && newNoteMode === mode) {
    el.newNoteForm.hidden = true;
    return;
  }
  if (sidebarHidden) setSidebarHidden(false);
  openNewNoteForm(currentFolderPrefix(), mode);
}

el.newNote.onclick = () => toggleNewNoteForm("note");
el.newFolder.onclick = () => toggleNewNoteForm("folder");

el.newNoteForm.onsubmit = (event) => {
  event.preventDefault();
  const name = el.newNoteName.value.trim();
  if (!name) return;

  el.newNoteName.value = "";
  el.newNoteForm.hidden = true;
  void (newNoteMode === "note" ? createNote(name) : createFolder(name));
};

el.newNoteName.onkeydown = (event) => {
  if (matches(event, "cancel")) {
    el.newNoteForm.hidden = true;
    el.newNoteName.value = "";
  }
};

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

// Links in the preview navigate inside the vault, not to the outside.
el.preview.addEventListener("click", (event) => {
  const anchor = (event.target as HTMLElement).closest("a");
  if (!anchor) return;

  const link = parseLink(anchor.getAttribute("href") ?? "");
  if (!link) return;
  event.preventDefault();

  if (link.kind === "note") {
    void openNote(link.value);
  } else if (link.kind === "missing") {
    void createNote(link.value);
  } else if (link.kind === "tag") {
    // A tag in the preview is a ready query: the palette opens with it.
    palette.open(`tag:${link.value}`);
  }
});

/**
 * Search palette.
 *
 * It has no state of its own: notes, actions and open history live here. The
 * palette only shows them and calls back.
 */
const palette = new Palette({
  // Search all vaults at once: the user wants the note, the vault is secondary.
  // The active vault comes first; results from others carry their vault label.
  search: (query) => api.searchAll(query),
  bookmarks: () => (vault ? api.listBookmarks() : Promise.resolve([])),
  openBookmark: (id, line) => void openBookmark(id, line),
  tasks: () => (vault ? api.listTasks() : Promise.resolve([])),
  openTask: (id, line) => void openTask(id, line),
  recent: recentNotes,
  total: () => activeNotes().length,
  activeVault: () => vault?.id ?? null,
  open: (id, from) => void openFound(id, from),
  create: (name) => void createNote(name),
  // The editor has the text of a standalone file too, so it gets the outline.
  headings: () => editor.getHeadings(),
  goToHeading,
  actions: () => [
    // Commands of the open note come first: the other actions are about the
    // window, and a command is about what is open in it.
    ...noteCommands.map((command) => ({
      label: command.name,
      // Commands have no keys: each user has a different set, and a key for
      // them would take one from the window.
      run: () => {
        const target = currentTarget();
        if (target) void runCommand(command, target);
      },
    })),
    { label: t("palette.action.newNote"), action: "note.new", run: () => el.newNote.click() },
    { label: t("palette.action.openFile"), action: "file.open", run: () => void pickFile() },
    ...MODES.map((each) => ({
      label: t("palette.action.mode", { mode: t(`mode.${each}`) }),
      run: () => setMode(each),
    })),
    { label: t("palette.action.bookmark"), action: "bookmark.toggle", run: () => toggleCurrentBookmark() },
    { label: t("palette.action.bookmarks"), action: "bookmark.list", run: () => palette.open("", "bookmarks") },
    { label: t("palette.action.outline"), action: "note.outline", run: () => palette.openOutline() },
    { label: t("palette.action.tasks"), action: "task.list", run: () => palette.open("", "tasks") },
    { label: t("palette.action.sidebar"), action: "sidebar.toggle", run: () => setSidebarHidden(!sidebarHidden) },
    { label: t("palette.action.back"), action: "history.back", run: () => void travel(-1) },
    { label: t("palette.action.forward"), action: "history.forward", run: () => void travel(1) },
    { label: t("palette.action.sync"), action: "vault.sync", run: () => void runSync() },
    {
      label: t("palette.action.syncSettings"),
      action: "settings.open",
      run: () => void openSettings("vaults"),
    },
    { label: t("palette.action.reload"), run: () => void reloadVault() },
    { label: t("menu.scale.larger"), action: "scale.larger", run: () => changeCurrentScale(1) },
    { label: t("menu.scale.smaller"), action: "scale.smaller", run: () => changeCurrentScale(-1) },
    { label: t("menu.scale.reset"), action: "scale.reset", run: () => changeCurrentScale(0) },
  ],
});

el.openPalette.onclick = () => palette.open();

/** Built-in themes. Their values are in styles.css. */
const BUILT_IN: [string, string][] = [
  ["light", t("theme.light")],
  ["dark", t("theme.dark")],
];

let themes: Theme[] = [];
/** Selected theme. The status bar menu marks it with a check. */
let currentTheme = "light";
/** Variables set by the previous theme. They must be removed. */
let painted: string[] = [];

/**
 * Applies a theme: `data-theme` picks the built-in base, and the manifest
 * variables go on top of it.
 *
 * The core already validated the values, so `setProperty` gets them as is.
 */
function applyTheme(id: string, options: { remember?: boolean } = {}): void {
  const root = document.documentElement;
  for (const name of painted) root.style.removeProperty(name);
  painted = [];

  const theme = themes.find((candidate) => candidate.id === id);
  const base = theme ? theme.base : id === "dark" ? "dark" : "light";
  root.dataset.theme = base;

  for (const [name, value] of Object.entries(theme?.variables ?? {})) {
    root.style.setProperty(`--${name}`, value);
    painted.push(`--${name}`);
  }

  // Diagram colors are a mermaid setting, not CSS, so diagrams must redraw.
  // Preview first, then the editor: the editor redraws its diagrams and needs
  // the new setting already applied.
  void setDiagramBase(base, el.preview).then(() => editor.redrawDiagrams());

  currentTheme = id;
  // The theme lives in settings shared with the settings window. When the
  // theme came from there as an event, do not write it back.
  if (options.remember !== false) void saveSettings({ theme: id });

  if (theme && theme.warnings.length > 0) {
    setStatus(t("status.themeWarning", { name: theme.name, warning: theme.warnings[0] }), "error");
  }
}

/** Themes live in the vault folder, so they load with it. */
async function refreshThemes(): Promise<void> {
  try {
    const set = await api.listThemes();
    themes = set.themes;
    if (set.problems.length > 0) setStatus(set.problems[0], "error");
  } catch (error) {
    themes = [];
    fail(error);
  }
  pickTheme();
}

/** Available themes: built-in ones and the ones in the vault folder. */
function themeList(): [string, string][] {
  return [...BUILT_IN, ...themes.map((theme): [string, string] => [theme.id, theme.name])];
}

/** Applies the theme from settings. It can be gone: the file was deleted or the vault changed. */
function pickTheme(): void {
  const wanted = appSettings?.theme ?? "light";
  const known = themeList().some(([id]) => id === wanted);
  // A missing theme falls back to light, but the choice stays saved: it applies
  // again when the vault with that theme returns.
  applyTheme(known ? wanted : "light", { remember: known });
}

/**
 * Status bar gear: theme, language and settings in one menu.
 *
 * These were three labeled segments, and together they took more space than
 * everything else. Users change them rarely, and a check marks the current
 * choice.
 */
function openMoreMenu(anchor: HTMLElement): void {
  openMenu(anchor, [
    {
      title: t("menu.theme"),
      items: themeList().map(([id, name]) => ({
        label: name,
        checked: id === currentTheme,
        run: () => applyTheme(id),
      })),
    },
    {
      title: t("menu.locale"),
      items: locales.map((name) => ({
        label: LOCALE_NAMES[name] ?? name,
        checked: name === locale,
        run: () => void switchLocale(name),
      })),
    },
    {
      items: [
        { label: t("menu.settings"), glyph: "gear", action: "settings.open", run: () => void openSettings() },
      ],
    },
  ]);
}

/** Labels are set in the markup when the window builds, so the window reloads. */
async function switchLocale(name: string): Promise<void> {
  if (name === locale) return;
  await flushPendingSave();
  // The language goes to settings. The localStorage mirror updates at once, so
  // after the reload the labels use the new language.
  setLocale(name);
  await saveSettings({ locale: name });
  location.reload();
}

/**
 * The window actions. A run that returns `false` did not take the key: the
 * event goes on as if nobody handled it.
 */
const windowActions: [Action, () => boolean | void][] = [
  ["note.new", () => el.newNote.click()],
  ["file.open", () => void pickFile()],
  ["vault.sync", () => void runSync()],
  ["note.save", () => void flushPendingSave()],
  // In the editor its own keymap handles the key; here it is for a preview selection.
  ["note.comment", () => previewNotes.openForm() || tableNotes.openForm()],
  ["note.mode", () => setMode(nextMode(mode))],
  // ⌘B is bold in the editor, so the bookmark uses Shift.
  ["bookmark.toggle", () => toggleCurrentBookmark()],
  ["bookmark.list", () => palette.open("", "bookmarks")],
  ["task.list", () => palette.open("", "tasks")],
  ["sidebar.toggle", () => setSidebarHidden(!sidebarHidden)],
  // ⌘⌥← and ⌘⌥→ by convention: the editor uses ⌘[ and ⌘] for indentation.
  ["history.back", () => void travel(-1)],
  ["history.forward", () => void travel(1)],
  // ⌘⇧P is the common command palette key, and users press it by habit.
  // Plain ⌘P stays free: it is print.
  ["palette.open", () => palette.open()],
  // Two searches: ⌘F in the open note, ⌘K in all vaults.
  ["note.find", () => editor.openSearch()],
  // ⌘⇧O as in VS Code: "Go to symbol" in the open file.
  ["note.outline", () => palette.openOutline()],
  ["settings.open", () => void openSettings()],
  ["scale.larger", () => changeCurrentScale(1)],
  ["scale.smaller", () => changeCurrentScale(-1)],
  // Zero is not a vault: ⌘1…⌘9 are vault numbers, ⌘0 resets the size as in a browser.
  ["scale.reset", () => changeCurrentScale(0)],
];

document.addEventListener("keydown", (event) => {
  // The editor can handle the key itself: its note search takes ⌘F and ⌘G.
  if (event.defaultPrevented) return;

  // The command output float covers the work, so Escape closes it like any
  // overlay.
  if (matches(event, "cancel") && !el.runFloat.hidden) {
    event.preventDefault();
    showRunOutput(false);
    return;
  }
  if (matches(event, "cancel") && !el.notesFloat.hidden) {
    event.preventDefault();
    showNotesFloat(false);
    return;
  }

  const found = windowActions.find(([action]) => matches(event, action));
  if (found) {
    if (found[1]() !== false) event.preventDefault();
    return;
  }

  // ⌘1…⌘9 pick a vault by its position in the list.
  for (let number = 1; number <= 9; number++) {
    if (matchesKey(event, vaultKey(number))) {
      event.preventDefault();
      void switchByHotkey(number);
      return;
    }
  }
});

/**
 * Takes the changed keys. Hints are spread over the markup, like labels, so a
 * change reloads the window, as a language change does. Returns `true` if the
 * window reloads.
 */
async function applyKeymap(keys: Keymap): Promise<boolean> {
  if (!useKeymap(keys)) return false;
  await flushPendingSave();
  location.reload();
  return true;
}

/** Reads the keys file again: the user may have edited it by hand. */
async function refreshKeymap(): Promise<boolean> {
  try {
    return await applyKeymap((await api.getKeymap()).keys);
  } catch (error) {
    console.error(error);
    return false;
  }
}

/** Switches the vault by hotkey. The core maps the number, so there is one list. */
async function switchByHotkey(number: number): Promise<void> {
  try {
    const id = await api.vaultByHotkey(number);
    if (id) await switchVault(id);
  } catch (error) {
    fail(error);
  }
}

/** Edits made outside the app: an external editor, git, sync. */
async function onExternalChange(
  changedVault: string,
  updated: string[],
  removed: string[],
): Promise<void> {
  // Each open vault has a watcher, and the sidebar shows all vaults: an edit
  // in another vault must show in its branch.
  await refreshTree(changedVault);
  if (changedVault !== vault?.id || !current || current.file) return;

  if (removed.includes(current.id)) {
    setStatus(t("status.noteRemoved"), "error");
    return;
  }
  if (!updated.includes(current.id)) return;

  await syncWithDisk();
}

async function boot(): Promise<void> {
  try {
    appSettings = await api.getSettings();
  } catch (error) {
    fail(error);
  }

  if (appSettings) {
    showLineNumbers(appSettings.lineNumbers);
    setDefaultScale(appSettings.noteScale);
    // The window language comes from settings. `i18n` reads its mirror
    // synchronously; they differ only after a manual edit of the file.
    if (appSettings.locale !== locale) {
      setLocale(appSettings.locale);
      location.reload();
      return;
    }
  }
  // The window was built with the keys from the `localStorage` copy. The file
  // differs from it only after a hand edit or on the first start.
  if (await refreshKeymap()) return;

  setMode(defaultMode(), { remember: false });
  setSidebarHidden(sidebarHidden);
  status.setVault(null);
  describeRemote();
  // Until a vault opens, only built-in themes exist: the others live in the
  // vault.
  pickTheme();
  void onVaultChanged(({ vault: changed, updated, removed }) =>
    void onExternalChange(changed, updated, removed),
  );
  // The watcher can miss an edit (laptop sleep, network disk), so the note is
  // checked against the disk again when the window gets focus.
  window.addEventListener("focus", () => void syncWithDisk());
  // The user went to another app: the note goes to disk now, not after the
  // timer. Sync, git and the CLI then see what the user sees.
  window.addEventListener("blur", () => void flushPendingSave());
  // A close does not wait for the timer either. The handler runs before the
  // window goes away; a failed save must not keep the window open.
  void onCloseRequested(() => flushPendingSave().catch(fail));
  // A hand edit of `keybindings.json` shows when the user comes back.
  window.addEventListener("focus", () => void refreshKeymap());
  void onKeymapChanged(({ keys }) => void applyKeymap(keys));
  // Sync reports itself: progress, conflicts and failures come as events,
  // also from auto-sync that nobody started by hand.
  void onSyncStatus((incoming) => {
    statuses.set(incoming.vault, incoming);
    describeRemote();
  });
  // The update downloads in the background. The window asks once, because
  // the download can finish before the window loads, and then follows events.
  void api.updateStatus().then(showUpdate, console.error);
  void onUpdateStatus(showUpdate);
  // Settings are shared with the settings window: language, theme and editor
  // behavior change there and arrive here as events.
  void onSettingsChanged(applySettings);
  // The core already added and activated a folder opened from outside
  // (`open -a kknow ~/notes`); the window only switches to it.
  void onVaultOpened((id) => void switchVault(id));
  // The core already stored a file opened from outside (a double click on
  // `.md`); the window takes the request and shows it.
  void onFileOpened(() => void takeOpened());
  // A folder dropped on the window becomes a vault; a markdown file opens as a
  // separate file. An image dropped on the open note goes into the note.
  void onDragDrop(({ over, paths, point }) => {
    dropHint.hidden = !over;
    if (paths.length === 0) return;
    const at = current && point ? editor.dropPosition(point.x, point.y) : null;
    const images = at === null ? [] : paths.filter((path) => isImage(path));
    if (at !== null && images.length > 0) void attachDropped(images, at);
    const rest = paths.filter((path) => !images.includes(path));
    if (rest.length > 0) void adoptDropped(rest);
  });
  // "2 min ago" gets old without any event, so the segment redraws every minute.
  window.setInterval(describeRemote, 60000);

  // Read this before the vault: opening a vault note erases the key.
  const lastFile = localStorage.getItem(LAST_FILE);

  try {
    await refreshVaults();
    // The vault list is already the tree: an empty list shows a prompt to add
    // a folder, not a blank space.
    renderTree();
    await refreshFiles();

    const info = await api.currentVault();
    if (info) {
      await useVault(info);
    } else if (files.length === 0) {
      setStatus(t("status.pickVault"));
    }
  } catch (error) {
    fail(error);
  }

  // A double click on `.md` at cold start: the request waited for the window.
  // Without one, the window returns to the separate file it closed on. The
  // core already forgot a missing file, so it is not in the list.
  if (!(await takeOpened()) && lastFile && files.some((entry) => entry.path === lastFile)) {
    await openFileView(lastFile);
  }

  // Other expanded branches load last: the active vault matters more, and
  // they must not show as empty before that.
  await restoreExpanded();
}

void boot();
