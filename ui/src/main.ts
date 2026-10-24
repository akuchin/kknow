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
  type CommandTarget,
  type VaultInfo,
  type VaultView,
} from "./api";
import {
  formKeys,
  formatKey,
  matches,
  matchesKey,
  useKeymap,
  vaultKey,
  type Action,
  type Keymap,
} from "./actions";
import { describeError, locale, t } from "./i18n";
import { openMenu, type MenuItem, type MenuSection } from "./menu";
import { MODES, moveMode, nextMode } from "./note-mode";
import { moveScale } from "./note-scale";
import { Palette } from "./palette";
import "./styles.css";
import { folderOf, join, leafOf } from "./tree-model";
import {
  applySettings,
  loadSettings,
  openMoreMenu,
  pickTheme,
  refreshThemes,
  toggleLineNumbers,
} from "./window/appearance";
import { el, initDom } from "./window/dom";
import { editor, initEditor } from "./window/editor";
import {
  LAST_FILE,
  applyNote,
  cancelPendingSave,
  changeCurrentScale,
  closeNote,
  defaultMode,
  flushPendingSave,
  initNote,
  noteKey,
  onEdit,
  openFileView,
  openFound,
  openNote,
  releaseNote,
  setMode,
  showHeading,
  showPreview,
  stashPosition,
  syncWithDisk,
  travel,
} from "./window/note";
import {
  commandItems,
  currentTarget,
  editCommandsSection,
  initCommands,
  lastCommand,
  openNoteCommands,
  runCommand,
  showRunOutput,
} from "./window/commands";
import { describeRemote, initSync, openChipMenu, refreshSync, runSync, takeStatus } from "./window/sync";
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
import {
  goToHeading,
  loadVaultMemory,
  openBookmark,
  openTask,
  recentNotes,
  rememberOpened,
  saveBookmarks,
  toggleCurrentBookmark,
} from "./window/marks";
import { closeTrashView, initTrash } from "./window/trash";
import {
  activeNotes,
  collator,
  expandVault,
  foldersIn,
  forgetTree,
  initTree,
  loadNotes,
  openSidebar,
  refreshTree,
  renderTree,
  restoreExpanded,
  restoreSidebar,
  toggleSidebar,
} from "./window/tree";
import { copyText, openSettings } from "./window/util";
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

/**
 * Vault menu: the list, add and settings.
 *
 * An unavailable vault (for example, the disk is unplugged) stays in the list
 * but is disabled. A silent removal would lose its settings for an external
 * reason.
 */
function openVaultMenu(anchor: HTMLElement): void {
  const items: MenuItem[] = state.vaults.map((entry, index) => ({
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
      if (vaultId !== state.vault?.id) await switchVault(vaultId);
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

initCommands();

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
              if (entry.id !== state.vault?.id) await switchVault(entry.id);
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
  openSidebar();

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
    state.setVaults(await api.renameVault(id, name));
    // The status bar also shows the active vault name.
    if (state.vault?.id === id) {
      state.setVault({ ...state.vault, name });
      status.setVault(name, state.vault.color);
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
  const choices = ["", ...foldersIn(vaultId)]
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
  const isCurrent = vaultId === state.vault?.id && state.note?.id === id;
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
  const inside =
    vaultId === state.vault?.id && state.note?.id.startsWith(prefix) ? state.note.id : null;
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
    vaultId === state.vault?.id
      ? state.vault.root
      : state.vaults.find((entry) => entry.id === vaultId)?.path;
  return root ? `${root}/${relative}` : null;
}

async function recolorVault(id: string, color: string): Promise<void> {
  try {
    state.setVaults(await api.setVaultColor(id, color));
    if (state.vault?.id === id) {
      state.setVault({ ...state.vault, color });
      status.setVault(state.vault.name, color);
    }
  } catch (error) {
    fail(error);
  }
  renderTree();
}

/** Forgets a vault: the entry leaves the list, the notes stay on disk. */
async function forgetVault(entry: VaultView): Promise<void> {
  try {
    state.setVaults(await api.removeVault(entry.id));
  } catch (error) {
    fail(error);
    return;
  }

  forgetTree(entry.id);

  // If the active vault was removed, the core already picked another; open it.
  if (entry.id === state.vault?.id) {
    const info = await api.currentVault().catch(() => null);
    if (info) {
      await useVault(info);
    } else {
      state.setVault(null);
      releaseNote();
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

initFiles({ refreshVaults, adoptVaults });

/** Shows a vault: tree, themes, sync and its last note. */
async function useVault(info: VaultInfo): Promise<void> {
  // The position key includes the old vault, so save before the switch.
  stashPosition();
  state.setVault(info);
  releaseNote();
  status.setVault(info.name, info.color);

  // Collapsed folders and recent notes are per vault: ⌘2 must return to where
  // the user left, not to another vault's tree.
  loadVaultMemory(info.id);

  // Expand the active vault in the sidebar: the user came for it.
  expandVault(info.id);
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
  if (state.vault?.id === id) return;
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
    state.setVaults(await api.listVaults());
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
      problem ??= describeError(error);
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

async function createNote(name: string): Promise<void> {
  if (!state.vault) return;
  try {
    const view = await api.createNote(name);
    await refreshTree(state.vault.id);
    applyNote(view);
    rememberOpened(view.id);
    editor.focus();
    setStatus(t("status.created"));
  } catch (error) {
    fail(error);
  }
}

async function createFolder(name: string): Promise<void> {
  if (!state.vault) return;
  try {
    await api.createFolder(name);
    await refreshTree(state.vault.id);
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
  const isCurrent = vaultId === state.vault?.id && state.note?.id === id;
  if (isCurrent) cancelPendingSave();

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
  const isCurrent = vaultId === state.vault?.id && state.note?.id.startsWith(prefix);
  if (isCurrent) cancelPendingSave();

  try {
    await api.deleteFolder(name, vaultId);
    if (isCurrent) closeNote();
    await refreshTree(vaultId);
    setStatus(t("status.folderDeleted"));
  } catch (error) {
    fail(error);
  }
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

el.addVault.onclick = () => void pickVault();

async function reloadVault(): Promise<void> {
  if (!state.vault) return;
  await flushPendingSave();
  try {
    const info = await api.reload();
    await refreshTree(state.vault.id);
    setStatus(t("status.notes", { count: info.noteCount }));
    if (state.note && !state.note.file) applyNote(await api.loadNote(state.note.id));
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
  if (state.note?.file) return "";
  const parts = state.note?.id.split("/") ?? [];
  parts.pop();
  return parts.length > 0 ? `${parts.join("/")}/` : "";
}

/**
 * Sidebar buttons and ⌘N. A second click on the same button hides the form.
 * The form is in the sidebar, so a collapsed sidebar opens first.
 */
function toggleNewNoteForm(mode: "note" | "folder"): void {
  if (!el.newNoteForm.hidden && newNoteMode === mode) {
    el.newNoteForm.hidden = true;
    return;
  }
  openSidebar();
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
  bookmarks: () => (state.vault ? api.listBookmarks() : Promise.resolve([])),
  openBookmark: (id, line) => void openBookmark(id, line),
  tasks: () => (state.vault ? api.listTasks() : Promise.resolve([])),
  openTask: (id, line) => void openTask(id, line),
  recent: () => recentNotes(activeNotes()),
  total: () => activeNotes().length,
  activeVault: () => state.vault?.id ?? null,
  open: (id, from) => void openFound(id, from),
  create: (name) => void createNote(name),
  // The editor has the text of a standalone file too, so it gets the outline.
  headings: () => editor.getHeadings(),
  goToHeading,
  actions: () => [
    // Commands of the open note come first: the other actions are about the
    // window, and a command is about what is open in it.
    ...openNoteCommands().map((command) => ({
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
    { label: t("palette.action.sidebar"), action: "sidebar.toggle", run: () => toggleSidebar() },
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
