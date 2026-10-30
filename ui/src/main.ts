// The entry point of the main window: `boot` starts the zones of
// `window/` and subscribes to the events of the core.
//
// The zones live in `window/`, one module each; see
// `docs/refactor-ui-2026.md`. A lower zone reaches a higher one only through
// a hook that `boot` passes to its `init`.

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
import { applySettings, loadSettings, openMoreMenu, pickTheme, toggleLineNumbers } from "./window/appearance";
import { initCommands, showRunOutput } from "./window/commands";
import { el, initDom } from "./window/dom";
import { initEdits, openCommandMenu, openNewNoteForm } from "./window/edits";
import { initEditor } from "./window/editor";
import { initFiles, refreshFiles, savePastedImage, takeDrop, takeOpened } from "./window/files";
import { applyKeymap, initKeys, refreshKeymap } from "./window/keys";
import { rememberOpened, saveBookmarks } from "./window/marks";
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
import { initNotesFloat, showNotes } from "./window/notes-float";
import { initMcp, mcpFloat, showMcpSetup } from "./window/mcp";
import { initPalette, resolveConflicts } from "./window/palette";
import { state } from "./window/state";
import { fail, initStatus, setStatus, showUpdate, status } from "./window/status";
import { clickSync, describeRemote, initSync, openChipMenu, runSync, takeStatus } from "./window/sync";
import { hideGit, initGit, showGit } from "./window/git";
import { initToolbar } from "./window/toolbar";
import { initTree, refreshTree, renderTree, restoreExpanded, restoreSidebar } from "./window/tree";
import { closeTrashView, initTrash } from "./window/trash";
import {
  initVaults,
  onExternalChange,
  openVaultMenu,
  openVaultRowMenu,
  refreshVaults,
  refreshVaultsOnFocus,
  switchVault,
  useVault,
} from "./window/vaults";

async function boot(): Promise<void> {
  // The browser uses the document language for hyphenation and word selection.
  document.documentElement.lang = locale;

  // The zones start in the order of the old top-level code: the markup, the
  // editor, its floats and the status bar first; the tree follows the note
  // event before the commands; the task boxes of the preview take clicks
  // before its links; the palette exists before the keys.
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
      sync: () => void clickSync(),
      run: () => showRunOutput(el.runFloat.hidden),
      chip: (anchor) => openChipMenu(anchor),
      scale: (steps) => changeCurrentScale(steps),
      lines: () => void toggleLineNumbers(),
      mcp: () => showMcpSetup(mcpFloat.hidden),
      more: (anchor) => openMoreMenu(anchor),
    },
    () => flushPendingSave(),
  );
  initMcp();
  initTree({
    openVaultMenu: openVaultRowMenu,
    openCommandMenu,
    switchVault,
    openNewNoteForm,
    openGit: showGit,
  });
  initCommands();
  initFiles();
  initTrash({ renderTree, refreshTree });
  // A note the user opens replaces the trash note and the changes view.
  const closeViews = () => (closeTrashView(), hideGit());
  initNote({ closeViews, refreshFiles, refreshTree, opened: rememberOpened, switchVault });
  initToolbar();
  initSync({ resolve: resolveConflicts });
  initGit({ sync: runSync, closeTrashView, refreshTree });
  initVaults();
  initEdits({ switchVault });
  initPalette();
  initKeys();

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
  // Names, colors and labels change in the settings window.
  window.addEventListener("focus", () => void refreshVaultsOnFocus());
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
