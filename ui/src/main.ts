
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
import { locale, t } from "./i18n";
import "./styles.css";
import {
  applySettings,
  loadSettings,
  openMoreMenu,
  pickTheme,
  toggleLineNumbers,
} from "./window/appearance";
import { el, initDom } from "./window/dom";
import { initEditor } from "./window/editor";
import {
  LAST_FILE,
  changeCurrentScale,
  defaultMode,
  flushPendingSave,
  initNote,
  onEdit,
  openFileView,
  setMode,
  syncWithDisk,
} from "./window/note";
import { initCommands, showRunOutput } from "./window/commands";
import { describeRemote, initSync, openChipMenu, runSync, takeStatus } from "./window/sync";
import { initNotesFloat, showNotes } from "./window/notes-float";
import { fail, initStatus, setStatus, showUpdate, status } from "./window/status";
import { initFiles, refreshFiles, savePastedImage, takeDrop, takeOpened } from "./window/files";
import { initEdits, openCommandMenu, openNewNoteForm } from "./window/edits";
import { rememberOpened, saveBookmarks } from "./window/marks";
import { closeTrashView, initTrash } from "./window/trash";
import {
  initVaults,
  openVaultMenu,
  openVaultRowMenu,
  switchVault,
  useVault,
  refreshVaults,
} from "./window/vaults";
import { initPalette, resolveConflicts } from "./window/palette";
import { initToolbar } from "./window/toolbar";
import { applyKeymap, initKeys, refreshKeymap } from "./window/keys";
import { initTree, refreshTree, renderTree, restoreExpanded, restoreSidebar } from "./window/tree";
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
initToolbar();

initSync({ resolve: resolveConflicts });
initVaults();

initEdits({ switchVault });

initPalette();

initKeys();

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
