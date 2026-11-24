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
import { lockNote, removeLock } from "./lock-view";
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
import { collator, foldersIn, isLockedNote, openSidebar, refreshTree, renderTree } from "./tree";
import { copyText } from "./util";

export interface EditHooks {
  switchVault: (id: string) => Promise<void>;
}

let hooks: EditHooks;

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

export async function openCommandMenu(anchor: HTMLElement, target: CommandTarget): Promise<void> {
  const items = await commandItems(target);
  if (!items) return;

  const creation: MenuSection[] = [];
  const files: MenuSection[] = [];
  const system: MenuSection[] = [];
  const removal: MenuSection[] = [];
  const vaultId = target.vault;

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
        isLockedNote(vaultId, note)
          ? {
              label: t("commands.note.removeLock"),
              glyph: "lockOpen",
              key: "U",
              run: () => void removeLock(note, vaultId),
            }
          : {
              label: t("commands.note.lock"),
              glyph: "lock",
              key: "K",
              run: () => void lockNote(note, vaultId),
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
    { title: t("commands.title"), items },
    ...removal,
    editCommandsSection(),
  ]);
}

function startTreeRename(target: CommandTarget): void {
  const { name, vault: vaultId } = target;
  if (!name || !vaultId) return;
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
  input.onblur = () => void finish(true);
}

function openMoveMenu(anchor: HTMLElement, target: CommandTarget): void {
  const { name, vault: vaultId } = target;
  if (!name || !vaultId) return;

  const leaf = leafOf(name);
  const parent = folderOf(name);
  const choices = ["", ...foldersIn(vaultId)]
    .filter((folder) => folder !== parent)
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

async function renameNote(
  id: string,
  to: string,
  vaultId: string,
  done: "status.renamed" | "status.moved",
): Promise<void> {
  const isCurrent = state.isOpen(vaultId, id);
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

function movedStatus(done: "status.renamed" | "status.moved", relinked: number): string {
  return relinked > 0 ? `${t(done)} · ${t("status.relinked", { count: relinked })}` : t(done);
}

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

async function removeNote(id: string, vaultId: string): Promise<void> {
  if (!window.confirm(t("confirm.deleteNote", { name: id }))) return;

  const isCurrent = state.isOpen(vaultId, id);
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

let newNoteMode: "note" | "folder" = "note";

export function openNewNoteForm(prefix = "", mode: "note" | "folder" = "note"): void {
  newNoteMode = mode;
  el.newNoteName.placeholder = t(
    mode === "note" ? "sidebar.newNote.placeholder" : "sidebar.newFolder.placeholder",
  );
  el.newNoteForm.hidden = false;
  el.newNoteName.value = prefix;
  el.newNoteName.focus();
  el.newNoteName.setSelectionRange(prefix.length, prefix.length);
}

function currentFolderPrefix(): string {
  if (state.note?.file) return "";
  const parts = state.note?.id.split("/") ?? [];
  parts.pop();
  return parts.length > 0 ? `${parts.join("/")}/` : "";
}

export function toggleNewNoteForm(mode: "note" | "folder"): void {
  if (!el.newNoteForm.hidden && newNoteMode === mode) {
    el.newNoteForm.hidden = true;
    return;
  }
  openSidebar();
  openNewNoteForm(currentFolderPrefix(), mode);
}
