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
import { initAside } from "./window/aside";
import { initCommands, showRunOutput } from "./window/commands";
import { el, initDom } from "./window/dom";
import { initEdits, openCommandMenu, openNewNoteForm } from "./window/edits";
import { initEditor } from "./window/editor";
import { initFiles, refreshFiles, savePastedImage, takeDrop, takeOpened } from "./window/files";
import { applyKeymap, initKeys, refreshKeymap } from "./window/keys";
import { initLockView } from "./window/lock-view";
import { goToAdjacentBookmark, rememberOpened, saveBookmarks } from "./window/marks";
import {
  LAST_FILE,
  applyNote,
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
import { initProposalsFloat } from "./window/proposals-float";
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

const REMOTE_REFRESH_MS = 60_000;

async function boot(): Promise<void> {
  document.documentElement.lang = locale;

  initDom();
  initEditor({
    onChange: onEdit,
    onNotes: showNotes,
    onBookmark: saveBookmarks,
    onBookmarkStep: goToAdjacentBookmark,
    saveImage: savePastedImage,
  });
  const noteCalls = { apply: applyNote, flush: flushPendingSave };
  initNotesFloat(noteCalls);
  initProposalsFloat(noteCalls);
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
  const closeViews = () => (closeTrashView(), hideGit());
  initNote({ closeViews, refreshFiles, refreshTree, opened: rememberOpened, switchVault });
  initLockView();
  initAside();
  initToolbar();
  initSync({ resolve: resolveConflicts });
  initGit({ sync: runSync, closeTrashView, refreshTree });
  initVaults();
  initEdits({ switchVault });
  initPalette();
  initKeys();

  if (await loadSettings()) return;
  if (await refreshKeymap()) return;

  setMode(defaultMode(), { remember: false });
  restoreSidebar();
  status.setVault(null);
  describeRemote();
  pickTheme();
  void onVaultChanged(({ vault: changed, updated, removed }) =>
    void onExternalChange(changed, updated, removed),
  );
  window.addEventListener("focus", () => void syncWithDisk());
  window.addEventListener("blur", () => void flushPendingSave());
  void onCloseRequested(() => flushPendingSave().catch(fail));
  window.addEventListener("focus", () => void refreshKeymap());
  window.addEventListener("focus", () => void refreshVaultsOnFocus());
  void onKeymapChanged(({ keys }) => void applyKeymap(keys));
  void onSyncStatus(takeStatus);
  void api.updateStatus().then(showUpdate, console.error);
  void onUpdateStatus(showUpdate);
  void onSettingsChanged(applySettings);
  void onVaultOpened((id) => void switchVault(id));
  void onFileOpened(() => void takeOpened());
  void onDragDrop(takeDrop);
  window.setInterval(describeRemote, REMOTE_REFRESH_MS);

  const lastFile = localStorage.getItem(LAST_FILE);
  try {
    await refreshVaults();
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

  if (!(await takeOpened()) && lastFile && state.files.some((entry) => entry.path === lastFile)) {
    await openFileView(lastFile);
  }

  await restoreExpanded();
}

void boot();
