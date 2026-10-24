// Separate files: the file list, opening from Finder and from the dialog,
// drag and drop and image attachments.
//
// Only this module calls `state.setFiles`. The module does not touch the DOM
// on import.

import { open } from "@tauri-apps/plugin-dialog";

import { api, type CommandTarget, type DragDrop, type FileEntry, type Opened } from "../api";
import { imageLink } from "../attachment-name";
import { describeError, t } from "../i18n";
import { isImage } from "../images";
import { openMenu } from "../menu";
import { commandItems, editCommandsSection } from "./commands";
import { el } from "./dom";
import { editor } from "./editor";
import {
  LAST_FILE,
  closeNote,
  fileGone,
  flushPendingSave,
  openFileView,
  openFound,
  positionKey,
} from "./note";
import { markActive, rowActions, rowMenu } from "./rows";
import { state } from "./state";
import { fail, setStatus } from "./status";
import { copyText } from "./util";

/** Steps of higher zones that the file zone starts. */
export interface FileHooks {
  /** Reloads the vault list: a file from a known vault opens as its note. */
  refreshVaults: () => Promise<void>;
  /** Adds dropped folders as vaults. */
  adoptVaults: (paths: string[]) => Promise<void>;
}

let hooks: FileHooks;

/**
 * "Drop here" hint while a folder is dragged over the window.
 *
 * It lives in `body`, not `#app`: `#app` is a grid, and the hint needs the
 * whole window, not a grid cell. Pointer events pass through it: the shell
 * handles the drag, and the hint has nothing to click.
 */
let dropHint: HTMLDivElement;

/** Takes the hooks, adds the drop hint and wires the "Open file…" button. */
export function initFiles(calls: FileHooks): void {
  hooks = calls;
  dropHint = document.createElement("div");
  dropHint.className = "drop-hint";
  dropHint.textContent = t("sidebar.vault.drop");
  dropHint.hidden = true;
  document.body.append(dropHint);
  el.openFile.onclick = () => void pickFile();
}

/** Extensions the window opens as a separate file. They match the vault ones. */
const MARKDOWN = /\.(md|markdown|mdown)$/i;

/** Reloads the separate file list. The core forgets missing files on the way. */
export async function refreshFiles(): Promise<void> {
  try {
    state.setFiles(await api.listFiles());
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
  el.fileList.replaceChildren(...state.files.map(fileItem));
  markActive(state.note?.id ?? null);
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
export async function openFileMenu(anchor: HTMLElement, entry: FileEntry): Promise<void> {
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
  if (state.note?.id === entry.path) {
    await flushPendingSave();
    closeNote();
    localStorage.removeItem(LAST_FILE);
  }
  try {
    state.setFiles(await api.forgetFile(entry.path));
    renderFiles();
    setStatus(t("sidebar.file.removed", { name: entry.name }));
  } catch (error) {
    fail(error);
  }
}

/**
 * Shows what the user asked to open: a file from a known vault opens as its
 * note, anything else as a separate file.
 */
async function openPath(opened: Opened): Promise<void> {
  if (opened.kind === "note") {
    await hooks.refreshVaults();
    await openFound(opened.id, opened.vault);
    return;
  }
  await refreshFiles();
  await openFileView(opened.path);
}

/** An open request from Finder. The core holds it until the window takes it. */
export async function takeOpened(): Promise<boolean> {
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
export async function pickFile(): Promise<void> {
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
      problem ??= describeError(error);
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
  if (folders.length > 0) await hooks.adoptVaults(folders);
  if (markdown.length > 0) await openFiles(markdown);
}

/**
 * Saves a pasted image of the open note. The core puts it into `attachments/`
 * of the vault, or next to a standalone file. Returns the path for the link.
 */
export async function savePastedImage(name: string, bytes: Uint8Array): Promise<string | null> {
  if (!state.note) return null;
  try {
    return await api.saveAttachment(name, bytes, state.note.file ? state.note.id : null);
  } catch (error) {
    setStatus(t("image.saveFailed", { error: describeError(error) }), "error");
    return null;
  }
}

/**
 * Image files dropped on the editor: the core copies them into `attachments/`,
 * and the links go to the drop point. The note can change during the copy;
 * then the links have no place to go.
 */
async function attachDropped(paths: string[], at: number): Promise<void> {
  const note = state.note;
  if (!note) return;
  const saved: string[] = [];
  let problem: string | null = null;
  for (const path of paths) {
    try {
      saved.push(await api.attachFile(path, note.file ? note.id : null));
    } catch (error) {
      problem ??= describeError(error);
    }
  }
  if (!state.note || positionKey(state.note) !== positionKey(note)) return;
  editor.insertImages(saved.map(imageLink), at);
  if (problem) setStatus(t("image.saveFailed", { error: problem }), "error");
}

/**
 * A drag over the window or a drop. A folder dropped on the window becomes a
 * vault; a markdown file opens as a separate file. An image dropped on the
 * open note goes into the note.
 */
export function takeDrop({ over, paths, point }: DragDrop): void {
  dropHint.hidden = !over;
  if (paths.length === 0) return;
  const at = state.note && point ? editor.dropPosition(point.x, point.y) : null;
  const images = at === null ? [] : paths.filter((path) => isImage(path));
  if (at !== null && images.length > 0) void attachDropped(images, at);
  const rest = paths.filter((path) => !images.includes(path));
  if (rest.length > 0) void adoptDropped(rest);
}
