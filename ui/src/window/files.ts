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
import { adoptVaults, refreshVaults } from "./vaults";

const FILES_OPEN = "kknow:files:open";

let dropHint: HTMLDivElement;
let filesOpen = readFilesOpen();

function readFilesOpen(): boolean {
  try {
    return localStorage.getItem(FILES_OPEN) !== "false";
  } catch {
    return true;
  }
}

function applyFilesOpen(): void {
  const label = t(filesOpen ? "sidebar.files.collapse" : "sidebar.files.expand");
  el.fileList.hidden = !filesOpen;
  el.filesToggle.setAttribute("aria-expanded", String(filesOpen));
  el.filesToggle.title = label;
  el.filesToggle.setAttribute("aria-label", label);
  el.fileList.closest("section")!.dataset.open = String(filesOpen);
}

function saveFilesOpen(): void {
  try {
    localStorage.setItem(FILES_OPEN, String(filesOpen));
  } catch {
    return;
  }
}

function setFilesOpen(open: boolean): void {
  filesOpen = open;
  saveFilesOpen();
  applyFilesOpen();
}

export function initFiles(): void {
  dropHint = document.createElement("div");
  dropHint.className = "drop-hint";
  dropHint.textContent = t("sidebar.vault.drop");
  dropHint.hidden = true;
  document.body.append(dropHint);
  el.openFile.onclick = () => void pickFile();
  el.filesToggle.onclick = () => setFilesOpen(!filesOpen);
  applyFilesOpen();
}

const MARKDOWN = /\.(md|markdown|mdown)$/i;

export async function refreshFiles(): Promise<void> {
  try {
    state.setFiles(await api.listFiles());
  } catch (error) {
    fail(error);
  }
  renderFiles();
}

function renderFiles(): void {
  el.fileList.replaceChildren(...state.files.map(fileItem));
  el.filesCount.textContent = String(state.files.length);
  el.filesCount.hidden = state.files.length === 0;
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

async function forgetFile(entry: FileEntry): Promise<void> {
  if (state.isOpen(state.vault?.id, entry.path)) {
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

async function openPath(opened: Opened): Promise<void> {
  if (opened.kind === "note") {
    await refreshVaults();
    await openFound(opened.id, opened.vault);
    return;
  }
  setFilesOpen(true);
  await refreshFiles();
  await openFileView(opened.path);
}

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

export async function pickFile(): Promise<void> {
  const picked = await open({
    multiple: false,
    filters: [{ name: "Markdown", extensions: ["md", "markdown", "mdown"] }],
  });
  if (typeof picked !== "string") return;
  await openFiles([picked]);
}

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

async function adoptDropped(paths: string[]): Promise<void> {
  const markdown = paths.filter((path) => MARKDOWN.test(path));
  const folders = paths.filter((path) => !MARKDOWN.test(path));
  if (folders.length > 0) await adoptVaults(folders);
  if (markdown.length > 0) await openFiles(markdown);
}

export async function savePastedImage(name: string, bytes: Uint8Array): Promise<string | null> {
  if (!state.note) return null;
  try {
    return await api.saveAttachment(name, bytes, state.note.file ? state.note.id : null);
  } catch (error) {
    setStatus(t("image.saveFailed", { error: describeError(error) }), "error");
    return null;
  }
}

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

export function takeDrop({ over, paths, point }: DragDrop): void {
  dropHint.hidden = !over;
  if (paths.length === 0) return;
  const at = state.note && point ? editor.dropPosition(point.x, point.y) : null;
  const images = at === null ? [] : paths.filter((path) => isImage(path));
  if (at !== null && images.length > 0) void attachDropped(images, at);
  const rest = paths.filter((path) => !images.includes(path));
  if (rest.length > 0) void adoptDropped(rest);
}
