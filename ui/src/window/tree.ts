import { withShortcut } from "../actions";
import {
  api,
  type CommandTarget,
  type NoteSummary,
  type TrashEntryView,
  type VaultView,
} from "../api";
import { icon } from "../icons";
import { locale, t } from "../i18n";
import { buildTree, fileName, type Folder } from "../tree-model";
import { filterVaults, labelsOf, livePicks } from "../vault-labels";
import { app, el } from "./dom";
import { openFound } from "./note";
import { isUnlocked, refreshLock } from "./locks";
import { emptyItem, markActive, rowActions, rowMenu } from "./rows";
import { state } from "./state";
import { fail } from "./status";
import { markTrashViewing, trashItem, viewingTrash } from "./trash";
import { readList } from "./util";

export interface TreeHooks {
  openVaultMenu: (anchor: HTMLElement, entry: VaultView) => void;
  openCommandMenu: (anchor: HTMLElement, target: CommandTarget) => Promise<void>;
  switchVault: (id: string) => Promise<void>;
  openNewNoteForm: (prefix: string, mode: "note" | "folder") => void;
  openGit: () => Promise<void>;
}

export const LOCK_ALL_EVENT = "kknow:lock-all";

let hooks: TreeHooks;

export function initTree(calls: TreeHooks): void {
  hooks = calls;
  state.on("note", (note) => {
    if (note && !note.file && state.vault && reveal(state.vault.id, note.id)) renderTree();
    markActive(note?.id ?? null);
  });

  el.collapseAll.onclick = () => {
    expanded.clear();
    rememberExpanded();
    renderTree();
  };
  el.toggleSidebar.onclick = () => setSidebarHidden(!sidebarHidden);
}

const trees = new Map<string, NoteSummary[]>();

const folderLists = new Map<string, string[]>();

const trashes = new Map<string, TrashEntryView[]>();

const SIDEBAR = "kknow:sidebar";

let sidebarHidden = localStorage.getItem(SIDEBAR) === "hidden";
function setSidebarHidden(hidden: boolean): void {
  sidebarHidden = hidden;
  localStorage.setItem(SIDEBAR, hidden ? "hidden" : "shown");
  app.dataset.sidebar = hidden ? "hidden" : "shown";
  el.toggleSidebar.setAttribute("aria-expanded", String(!hidden));
  const label = t(hidden ? "topbar.sidebar.show" : "topbar.sidebar.hide");
  el.toggleSidebar.title = withShortcut(label, "sidebar.toggle");
  el.toggleSidebar.setAttribute("aria-label", label);
}

export function renderTree(): void {
  el.noteList.replaceChildren();

  if (state.vaults.length === 0) {
    el.noteList.append(emptyItem(t("sidebar.vaults.empty")));
    return;
  }

  renderLabelFilter();
  el.noteList.append(...shownVaults().map(vaultItem));
  markActive(viewingTrash() ? null : (state.note?.id ?? null));
  markTrashViewing();
}

const LABEL_FILTER = "kknow:labels:filter";
let pickedLabels = readList(LABEL_FILTER);

function shownVaults(): VaultView[] {
  pickedLabels = livePicks(pickedLabels, state.vaults);
  return filterVaults(state.vaults, pickedLabels, state.vault?.id ?? null);
}

export function filteredVaults(): string[] | null {
  return pickedLabels.length === 0 ? null : shownVaults().map((entry) => entry.id);
}

export function pickedLabelsText(): string {
  return pickedLabels.map((label) => `#${label}`).join(", ");
}

function renderLabelFilter(): void {
  const labels = labelsOf(state.vaults);
  el.labelFilter.hidden = labels.length === 0;
  el.labelFilter.replaceChildren(
    ...labels.map((label) => {
      const chip = document.createElement("button");
      chip.className = "label-chip";
      chip.setAttribute("role", "checkbox");
      chip.setAttribute("aria-checked", String(pickedLabels.includes(label)));
      chip.textContent = `#${label}`;
      chip.title = t("sidebar.labels.pick", { label });
      chip.onclick = () => {
        pickedLabels = pickedLabels.includes(label)
          ? pickedLabels.filter((other) => other !== label)
          : [...pickedLabels, label];
        localStorage.setItem(LABEL_FILTER, JSON.stringify(pickedLabels));
        renderTree();
      };
      return chip;
    }),
  );
}

const collapsed = new Map<string, Set<string>>();

const expanded = new Set<string>(readList("kknow:vaults:open"));

const reading = new Set<string>();

function foldersOf(id: string): Set<string> {
  let folders = collapsed.get(id);
  if (!folders) {
    folders = new Set(readList(`kknow:folders:${id}`));
    collapsed.set(id, folders);
  }
  return folders;
}

function rememberFolders(id: string): void {
  localStorage.setItem(`kknow:folders:${id}`, JSON.stringify([...foldersOf(id)]));
}

function rememberExpanded(): void {
  localStorage.setItem("kknow:vaults:open", JSON.stringify([...expanded]));
}

export const collator = new Intl.Collator(locale);

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
  if (entry.labels.length > 0) {
    button.title += `\n${entry.labels.map((label) => `#${label}`).join(" ")}`;
  }
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
    hooks.openVaultMenu(anchor, entry),
  );

  const plus = rowPlus(t("sidebar.folder.newNote", { name: entry.name }), entry.id, "");
  plus.disabled = !entry.available;
  const buttons = [more, plus];
  if (entry.git) buttons.unshift(rowGit(entry));
  row.append(button, rowActions(...buttons));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    hooks.openVaultMenu(more, entry);
  };
  item.append(row);

  if (!open) return item;

  const nested = document.createElement("ul");
  const notes = trees.get(entry.id);
  const folders = folderLists.get(entry.id) ?? [];
  if (!notes) {
    nested.append(emptyItem(t("sidebar.vault.loading")));
  } else if (notes.length === 0 && folders.length === 0) {
    nested.append(emptyItem(t("sidebar.empty")));
  } else {
    nested.append(...treeItems(buildTree(notes, folders, collator), entry.id));
  }
  if (notes) nested.append(trashItem(entry, trashes.get(entry.id) ?? []));
  item.append(nested);
  return item;
}

async function toggleVault(entry: VaultView): Promise<void> {
  if (!entry.available) return;

  if (expanded.delete(entry.id)) {
    rememberExpanded();
    renderTree();
    return;
  }

  expanded.add(entry.id);
  rememberExpanded();
  if (trees.has(entry.id) || reading.has(entry.id)) {
    renderTree();
    return;
  }

  reading.add(entry.id);
  renderTree();
  try {
    await loadNotes(entry.id);
  } catch (error) {
    expanded.delete(entry.id);
    rememberExpanded();
    fail(error);
  } finally {
    reading.delete(entry.id);
    renderTree();
  }
}

export async function restoreExpanded(): Promise<void> {
  const pending = state.vaults.filter(
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

function treeItems(folder: Folder, vaultId: string): HTMLLIElement[] {
  const items = folder.folders.map((child) => folderItem(child, vaultId));
  items.push(...folder.notes.map((note) => noteItem(note, vaultId)));
  return items;
}

function rowPlus(title: string, vaultId: string, folder: string): HTMLButtonElement {
  const plus = document.createElement("button");
  plus.className = "row-plus";
  plus.title = title;
  plus.setAttribute("aria-label", title);
  plus.append(icon("plus"));
  plus.onclick = async () => {
    if (vaultId !== state.vault?.id) await hooks.switchVault(vaultId);
    hooks.openNewNoteForm(folder ? `${folder}/` : "", "note");
  };
  return plus;
}

function rowGit(entry: VaultView): HTMLButtonElement {
  const git = document.createElement("button");
  git.className = "row-git";
  const title = t("sidebar.vault.git", { name: entry.name });
  git.title = title;
  git.setAttribute("aria-label", title);
  git.append(icon("git"));
  git.disabled = !entry.available;
  git.onclick = async () => {
    if (entry.id !== state.vault?.id) await hooks.switchVault(entry.id);
    await hooks.openGit();
  };
  return git;
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
    void hooks.openCommandMenu(anchor, target),
  );

  const row = document.createElement("div");
  row.className = "tree-row folder-row";
  row.dataset.path = folder.path;
  row.dataset.vault = vaultId;
  const plus = rowPlus(t("sidebar.folder.newNote", { name: folder.name }), vaultId, folder.path);
  row.append(button, rowActions(more, plus));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    void hooks.openCommandMenu(more, target);
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


function noteItem(note: NoteSummary, vaultId: string): HTMLLIElement {
  const item = document.createElement("li");
  const button = document.createElement("button");
  button.className = "note";
  button.dataset.id = note.id;
  button.dataset.vault = vaultId;
  button.title = note.id;
  button.onclick = () => void openFound(note.id, vaultId);

  const heading = document.createElement("span");
  heading.className = "note-title";
  heading.textContent = state.settings?.noteLabel === "file" ? fileName(note) : note.title;
  button.append(heading);

  const target: CommandTarget = { kind: "note", name: note.id, vault: vaultId };
  const more = rowMenu(t("sidebar.note.menu", { name: note.title }), (anchor) =>
    void hooks.openCommandMenu(anchor, target),
  );

  const row = document.createElement("div");
  row.className = "tree-row note-row";
  row.dataset.id = note.id;
  row.dataset.vault = vaultId;
  row.append(button);
  if (note.locked) row.append(lockButton(note, vaultId));
  row.append(rowActions(more));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    void hooks.openCommandMenu(more, target);
  };
  item.append(row);
  return item;
}

function lockButton(note: NoteSummary, vaultId: string): HTMLButtonElement {
  const open = isUnlocked(vaultId);
  const lock = document.createElement("button");
  lock.className = "note-lock";
  lock.dataset.open = String(open);
  const label = t(open ? "lock.icon.open" : "lock.icon.closed");
  lock.title = label;
  lock.setAttribute("aria-label", label);
  lock.append(icon(open ? "lockOpen" : "lock"));
  lock.onclick = () => {
    if (open) document.dispatchEvent(new CustomEvent(LOCK_ALL_EVENT, { detail: vaultId }));
    else void openFound(note.id, vaultId);
  };
  return lock;
}

export function isLockedNote(vaultId: string, id: string): boolean {
  return trees.get(vaultId)?.some((note) => note.id === id && note.locked) === true;
}

export function activeNotes(): NoteSummary[] {
  return state.vault ? (trees.get(state.vault.id) ?? []) : [];
}

export async function loadNotes(id: string): Promise<NoteSummary[]> {
  const [notes, folders, trash] = await Promise.all([
    api.listNotes(id),
    api.listFolders(id),
    api.listTrash(id).catch(() => []),
  ]);
  if (notes.some((note) => note.locked)) await refreshLock(id).catch(() => undefined);
  trees.set(id, notes);
  folderLists.set(id, folders);
  trashes.set(id, trash);
  return notes;
}

export async function refreshTree(id: string): Promise<void> {
  if (!trees.has(id)) return;
  try {
    await loadNotes(id);
  } catch (error) {
    fail(error);
  }
  renderTree();
}

export function toggleSidebar(): void {
  setSidebarHidden(!sidebarHidden);
}

export function openSidebar(): void {
  if (sidebarHidden) setSidebarHidden(false);
}

export function restoreSidebar(): void {
  setSidebarHidden(sidebarHidden);
}

export function foldersIn(vaultId: string): string[] {
  return folderLists.get(vaultId) ?? [];
}

export function expandVault(id: string): void {
  expanded.add(id);
  rememberExpanded();
}

export function forgetTree(id: string): void {
  trees.delete(id);
  folderLists.delete(id);
  collapsed.delete(id);
  expanded.delete(id);
  rememberExpanded();
}
