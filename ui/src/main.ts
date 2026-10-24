
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
} from "./api";
import { matches, matchesKey, useKeymap, vaultKey, type Action, type Keymap } from "./actions";
import { locale, t } from "./i18n";
import { openMenu, type MenuItem } from "./menu";
import { nextMode } from "./note-mode";
import "./styles.css";
import {
  applySettings,
  loadSettings,
  openMoreMenu,
  pickTheme,
  toggleLineNumbers,
} from "./window/appearance";
import { el, initDom } from "./window/dom";
import { editor, initEditor } from "./window/editor";
import {
  LAST_FILE,
  changeCurrentScale,
  defaultMode,
  flushPendingSave,
  initNote,
  onEdit,
  openFileView,
  openNote,
  setMode,
  syncWithDisk,
  travel,
} from "./window/note";
import {
  commandItems,
  currentTarget,
  editCommandsSection,
  initCommands,
  lastCommand,
  runCommand,
  showRunOutput,
} from "./window/commands";
import { describeRemote, initSync, openChipMenu, runSync, takeStatus } from "./window/sync";
import {
  initNotesFloat,
  previewNotes,
  showNotes,
  showNotesFloat,
  tableNotes,
} from "./window/notes-float";
import { fail, initStatus, setStatus, showUpdate, status } from "./window/status";
import {
  initFiles,
  openFileMenu,
  pickFile,
  refreshFiles,
  savePastedImage,
  takeDrop,
  takeOpened,
} from "./window/files";
import { initEdits, openCommandMenu, openNewNoteForm } from "./window/edits";
import { openBookmark, rememberOpened, saveBookmarks, toggleCurrentBookmark } from "./window/marks";
import { closeTrashView, initTrash } from "./window/trash";
import {
  initVaults,
  openVaultMenu,
  openVaultRowMenu,
  switchByHotkey,
  switchVault,
  useVault,
  refreshVaults,
} from "./window/vaults";
import { initPalette, palette, resolveConflicts } from "./window/palette";
import {
  initTree,
  refreshTree,
  renderTree,
  restoreExpanded,
  restoreSidebar,
  toggleSidebar,
} from "./window/tree";
import { openSettings } from "./window/util";
import { state } from "./window/state";

// The browser uses the document language for hyphenation and word selection.
document.documentElement.lang = locale;

initDom();

initEditor({
  onChange: onEdit,
  onNotes: showNotes,
  onBookmark: (marks, change) => void saveBookmarks(marks, change),
  saveImage: savePastedImage,
});
initNotesFloat();

initStatus(
  {
    vault: (anchor) => openVaultMenu(anchor),
    sync: () => void runSync(),
    run: () => showRunOutput(el.runFloat.hidden),
    chip: (anchor) => openChipMenu(anchor),
    scale: (steps) => changeCurrentScale(steps),
    lines: () => void toggleLineNumbers(),
    more: (anchor) => openMoreMenu(anchor),
  },
  () => flushPendingSave(),
);

initTree({
  openVaultMenu: openVaultRowMenu,
  openCommandMenu,
  switchVault,
  openNewNoteForm,
});

initCommands();


initFiles();

initTrash({ renderTree, refreshTree });
initNote({
  closeTrashView,
  refreshFiles,
  refreshTree,
  opened: rememberOpened,
  switchVault,
});
el.noteMenu.onclick = () => openNoteMenu();

/**
 * Menu of the open note. It is the same as its tree row menu: commands,
 * rename, clipboard, Finder. One menu for two places keeps them in
 * sync. A separate file uses its row menu from the file list.
 */
function openNoteMenu(): void {
  if (!state.note) return;
  if (state.note.file) {
    const path = state.note.id;
    const entry = state.files.find((candidate) => candidate.path === path);
    if (entry) void openFileMenu(el.noteMenu, entry);
    return;
  }
  if (!state.vault) return;
  void openCommandMenu(el.noteMenu, { kind: "note", name: state.note.id, vault: state.vault.id });
}

/**
 * Bookmarks of the open note: a jump to each, then the toggle and the list of
 * all bookmarks. The toolbar has no line of its own, so the toggle works on
 * the cursor line, as ⌘⇧B does.
 */
function openBookmarksMenu(): void {
  if (!state.note || state.note.file) return;
  const id = state.note.id;
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
  if (!state.note) return;
  openMenu(el.noteBacklinks, [
    {
      title: t("panel.backlinks.title"),
      items: state.note.backlinks.map((link, index) => ({
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

initSync({ resolve: resolveConflicts });
initVaults();

initEdits({ switchVault });

initPalette();

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
  ["note.mode", () => setMode(nextMode(state.mode))],
  // ⌘B is bold in the editor, so the bookmark uses Shift.
  ["bookmark.toggle", () => toggleCurrentBookmark()],
  ["bookmark.list", () => palette.open("", "bookmarks")],
  ["task.list", () => palette.open("", "tasks")],
  ["sidebar.toggle", () => toggleSidebar()],
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
  if (matches(event, "cancel") && !el.trashView.hidden) {
    event.preventDefault();
    closeTrashView();
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

/** Edits made outside the app: an external editor, git, sync. */
async function onExternalChange(
  changedVault: string,
  updated: string[],
  removed: string[],
): Promise<void> {
  // Each open vault has a watcher, and the sidebar shows all vaults: an edit
  // in another vault must show in its branch.
  await refreshTree(changedVault);
  if (changedVault !== state.vault?.id || !state.note || state.note.file) return;

  if (removed.includes(state.note.id)) {
    setStatus(t("status.noteRemoved"), "error");
    return;
  }
  if (!updated.includes(state.note.id)) return;

  await syncWithDisk();
}

async function boot(): Promise<void> {
  if (await loadSettings()) return;
  // The window was built with the keys from the `localStorage` copy. The file
  // differs from it only after a hand edit or on the first start.
  if (await refreshKeymap()) return;

  setMode(defaultMode(), { remember: false });
  restoreSidebar();
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
  void onSyncStatus(takeStatus);
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
  // Folders and files dropped on the window; see `takeDrop`.
  void onDragDrop(takeDrop);
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
    } else if (state.files.length === 0) {
      setStatus(t("status.pickVault"));
    }
  } catch (error) {
    fail(error);
  }

  // A double click on `.md` at cold start: the request waited for the window.
  // Without one, the window returns to the separate file it closed on. The
  // core already forgot a missing file, so it is not in the list.
  if (!(await takeOpened()) && lastFile && state.files.some((entry) => entry.path === lastFile)) {
    await openFileView(lastFile);
  }

  // Other expanded branches load last: the active vault matters more, and
  // they must not show as empty before that.
  await restoreExpanded();
}

void boot();
