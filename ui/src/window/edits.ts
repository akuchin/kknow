// Changes to notes and folders: create, rename, move, duplicate and delete,
// the inline rename in the tree and the new note form. The row menu of a
// note or folder lives here too: most of its items are such changes.
//
// The module does not touch the DOM on import.

import { formKeys, matches } from "../actions";
import { api, type CommandTarget } from "../api";
import { t } from "../i18n";
import { openMenu, type MenuItem, type MenuSection } from "../menu";
import { moveFolderMode, moveMode } from "../note-mode";
import { moveFolderScale, moveScale } from "../note-scale";
import { folderOf, join, leafOf } from "../tree-model";
import { commandItems, editCommandsSection } from "./commands";
import { el } from "./dom";
import { editor } from "./editor";
import { rememberOpened } from "./marks";
import {
  applyNote,
  cancelPendingSave,
  closeNote,
  flushPendingSave,
  noteKey,
  openFound,
  openNote,
} from "./note";
import { state } from "./state";
import { fail, setStatus } from "./status";
import { collator, foldersIn, openSidebar, refreshTree, renderTree } from "./tree";
import { copyText } from "./util";

/** Steps of higher zones that the edits zone starts. */
export interface EditHooks {
  /** Makes a vault active: the core creates notes in the active vault. */
  switchVault: (id: string) => Promise<void>;
}

let hooks: EditHooks;

/** Takes the hooks and wires the new note form. */
export function initEdits(calls: EditHooks): void {
  hooks = calls;
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
}

/**
 * Command menu for a target.
 *
 * The core returns the full list: the manifest decides if a command applies to
 * this note, folder or vault. A copy of this rule in the window could drift
 * from the terminal.
 */
export async function openCommandMenu(anchor: HTMLElement, target: CommandTarget): Promise<void> {
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
      if (vaultId !== state.vault?.id) await hooks.switchVault(vaultId);
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
export function renameInTree(
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

/**
 * Renames or moves a folder. An open note inside it moves too, and so do the
 * mode and the font size of every note inside.
 */
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
    moveFolderScale(noteKey(vaultId, name), noteKey(vaultId, to));
    moveFolderMode(noteKey(vaultId, name), noteKey(vaultId, to));
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

export async function createNote(name: string): Promise<void> {
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

/** What the form creates. Notes and folders share the input; only the action differs. */
let newNoteMode: "note" | "folder" = "note";

/**
 * Opens the new note or folder form. The prefix is the target folder.
 *
 * The folder goes into the input as text, not into hidden state: the user sees
 * the path and can erase it. The form always does exactly what it shows.
 */
export function openNewNoteForm(prefix = "", mode: "note" | "folder" = "note"): void {
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
export function toggleNewNoteForm(mode: "note" | "folder"): void {
  if (!el.newNoteForm.hidden && newNoteMode === mode) {
    el.newNoteForm.hidden = true;
    return;
  }
  openSidebar();
  openNewNoteForm(currentFolderPrefix(), mode);
}
