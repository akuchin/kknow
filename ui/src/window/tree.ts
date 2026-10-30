// The sidebar tree: a root per vault, its folders and notes, the collapsed
// and expanded state, and the sidebar itself.
//
// The tree keeps the notes, folders and trash entries of each loaded vault.
// Its row menus and the "+" button reach higher zones, so `main.ts` passes
// them to `initTree` as hooks. The module does not touch the DOM on import.

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
import { emptyItem, markActive, rowActions, rowMenu } from "./rows";
import { state } from "./state";
import { fail } from "./status";
import { markTrashViewing, trashItem, viewingTrash } from "./trash";
import { readList } from "./util";

/** Steps of higher zones that the tree starts. */
export interface TreeHooks {
  /** The menu of a vault row. */
  openVaultMenu: (anchor: HTMLElement, entry: VaultView) => void;
  /** The menu of a folder or note row. */
  openCommandMenu: (anchor: HTMLElement, target: CommandTarget) => Promise<void>;
  /** Makes a vault active: the core creates notes in the active vault. */
  switchVault: (id: string) => Promise<void>;
  /** Opens the new note or folder form with the folder as a prefix. */
  openNewNoteForm: (prefix: string, mode: "note" | "folder") => void;
  /** Opens the git changes panel of the active vault. */
  openGit: () => Promise<void>;
}

let hooks: TreeHooks;

/** Takes the hooks, follows the open note and wires the sidebar buttons. */
export function initTree(calls: TreeHooks): void {
  hooks = calls;
  // The tree follows the open note: it marks the note and expands the branch
  // that holds it.
  state.on("note", (note) => {
    // The note can be in a collapsed branch or vault; then redraw the tree.
    if (note && !note.file && state.vault && reveal(state.vault.id, note.id)) renderTree();
    markActive(note?.id ?? null);
  });

  /** Collapses every vault. Folders inside keep their state for the next expand. */
  el.collapseAll.onclick = () => {
    expanded.clear();
    rememberExpanded();
    renderTree();
  };
  el.toggleSidebar.onclick = () => setSidebarHidden(!sidebarHidden);
}

/**
 * Notes of each vault: every vault has its own tree.
 *
 * The sidebar shows all vaults at once, so one note list is not enough. A tree
 * loads lazily when its vault is expanded. The active vault always loads: the
 * palette, the counter and note restore depend on it.
 */
const trees = new Map<string, NoteSummary[]>();

/** Vault folders, separate from notes: empty folders are not in the note list. */
const folderLists = new Map<string, string[]>();

/** Trash entries of each loaded vault, the newest first: the last tree node. */
const trashes = new Map<string, TrashEntryView[]>();

/** Sidebar collapsed state; it persists between runs. */
const SIDEBAR = "kknow:sidebar";

let sidebarHidden = localStorage.getItem(SIDEBAR) === "hidden";
/** Collapses the sidebar; the editor takes its space. */
function setSidebarHidden(hidden: boolean): void {
  sidebarHidden = hidden;
  localStorage.setItem(SIDEBAR, hidden ? "hidden" : "shown");
  app.dataset.sidebar = hidden ? "hidden" : "shown";
  // This is "is the sidebar expanded", not "is the button on": no accent
  // color, the sidebar itself is visible.
  el.toggleSidebar.setAttribute("aria-expanded", String(!hidden));
  const label = t(hidden ? "topbar.sidebar.show" : "topbar.sidebar.hide");
  el.toggleSidebar.title = withShortcut(label, "sidebar.toggle");
  el.toggleSidebar.setAttribute("aria-label", label);
}

/**
 * The sidebar is a workspace: the tree roots are vaults, not folders of one
 * directory.
 *
 * This shows everything at once. A click on a note from another vault opens
 * it and switches the vault automatically.
 */
export function renderTree(): void {
  el.noteList.replaceChildren();

  if (state.vaults.length === 0) {
    el.noteList.append(emptyItem(t("sidebar.vaults.empty")));
    return;
  }

  renderLabelFilter();
  el.noteList.append(...shownVaults().map(vaultItem));
  // While a trash note covers the open note, only the trash note is marked.
  markActive(viewingTrash() ? null : (state.note?.id ?? null));
  markTrashViewing();
}

/** The picked labels of the filter; they persist between runs. */
const LABEL_FILTER = "kknow:labels:filter";
let pickedLabels = readList(LABEL_FILTER);

/**
 * The vaults the label filter leaves, all of them with no filter. A label can
 * go away in the settings: then it no longer filters.
 */
function shownVaults(): VaultView[] {
  pickedLabels = livePicks(pickedLabels, state.vaults);
  return filterVaults(state.vaults, pickedLabels, state.vault?.id ?? null);
}

/** Ids for the palette search, or `null` with no filter: then all vaults. */
export function filteredVaults(): string[] | null {
  return pickedLabels.length === 0 ? null : shownVaults().map((entry) => entry.id);
}

/** The picked labels as text for the palette: `#work, #ai`. Empty with no filter. */
export function pickedLabelsText(): string {
  return pickedLabels.map((label) => `#${label}`).join(", ");
}

/**
 * The row of labels above the tree. A click picks a label or drops it; with
 * several picked, a vault with any of them stays, as in the MCP float.
 */
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

/**
 * Collapsed folders, one set per vault.
 *
 * There was one set when the sidebar had one tree. Now there is a tree per
 * vault, and a shared set would collapse a folder with the same name in
 * another vault.
 */
const collapsed = new Map<string, Set<string>>();

/** Expanded vaults: only their trees are visible and loaded from disk. */
const expanded = new Set<string>(readList("kknow:vaults:open"));

/** Vaults that are loading now; for a large vault this is parsing and indexing. */
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

/** Expanded vaults persist between runs, like collapsed folders. */
function rememberExpanded(): void {
  localStorage.setItem("kknow:vaults:open", JSON.stringify([...expanded]));
}

/** Tree order follows the window language rules, so accented letters sort correctly. */
export const collator = new Intl.Collator(locale);

/**
 * A vault as a tree root: a row with a label and an expanded or collapsed tree.
 *
 * An unavailable vault (for example, an external disk was unplugged) stays in
 * the list but does not expand: there is nothing to read, and an external
 * cause must not remove the entry.
 */
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
  // The labels of a vault are not on its row: the filter row lists them all
  // once. The tooltip tells which ones this vault has.
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
  // A right click opens the same menu, as users expect in a tree.
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
    // No tree yet: the core parses and indexes a vault on first visit, which
    // takes time on a large folder. "No notes" would be wrong here: an empty
    // tree and an unloaded tree are different things.
    nested.append(emptyItem(t("sidebar.vault.loading")));
  } else if (notes.length === 0 && folders.length === 0) {
    nested.append(emptyItem(t("sidebar.empty")));
  } else {
    nested.append(...treeItems(buildTree(notes, folders, collator), entry.id));
  }
  // The trash is always the last node, also in an empty vault: a deleted
  // note must stay one click away.
  if (notes) nested.append(trashItem(entry, trashes.get(entry.id) ?? []));
  item.append(nested);
  return item;
}

/** Expands or collapses a vault. On first expand it loads from disk. */
async function toggleVault(entry: VaultView): Promise<void> {
  if (!entry.available) return;

  if (expanded.delete(entry.id)) {
    rememberExpanded();
    renderTree();
    return;
  }

  expanded.add(entry.id);
  rememberExpanded();
  // A loaded tree shows at once. A tree that is loading shows when it is done:
  // a second request for the same folder adds nothing.
  if (trees.has(entry.id) || reading.has(entry.id)) {
    renderTree();
    return;
  }

  reading.add(entry.id);
  renderTree();
  try {
    await loadNotes(entry.id);
  } catch (error) {
    // On a load failure the branch stays collapsed: expanded and empty, it
    // would look like a vault without notes.
    expanded.delete(entry.id);
    rememberExpanded();
    fail(error);
  } finally {
    reading.delete(entry.id);
    renderTree();
  }
}

/**
 * Loads the vaults that were expanded in the previous run.
 *
 * The expanded state persists, but the trees do not. Without this a branch
 * would stay expanded and empty until clicked, which falsely says "no notes".
 * This runs after the active vault loads: the user needs that tree first.
 */
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
        // On a load failure the branch collapses: expanded and empty, it would
        // look like a vault without notes.
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

/** Folder content as list rows; subfolders get their own nested `<ul>`. */
function treeItems(folder: Folder, vaultId: string): HTMLLIElement[] {
  const items = folder.folders.map((child) => folderItem(child, vaultId));
  items.push(...folder.notes.map((note) => noteItem(note, vaultId)));
  return items;
}

/**
 * The "+" button of a vault or folder row: a new note inside it.
 *
 * Creation is the most frequent action on a folder, so it gets a button of
 * its own next to "⋯", as in the sidebar of Claude. An inactive vault becomes
 * active first, because the core creates in the active vault.
 */
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

/**
 * The git button of a vault row: the changes panel of that vault.
 *
 * The status bar segment opens the same panel, but only for the active vault,
 * and nothing there says that it is a git vault. The panel works on the active
 * vault, so an inactive one becomes active first.
 */
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
  // Inline rename finds the row by these attributes.
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

/**
 * Expands the folders above a note and its vault.
 *
 * A note can open from outside the tree (a link, search, creation). In a
 * collapsed branch its active mark would be invisible.
 */
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


/** Note row in the tree. It also knows its vault: the sidebar has many. */
function noteItem(note: NoteSummary, vaultId: string): HTMLLIElement {
  const item = document.createElement("li");
  const button = document.createElement("button");
  button.className = "note";
  button.dataset.id = note.id;
  button.dataset.vault = vaultId;
  // The list truncates the name; the tooltip shows it in full.
  button.title = note.id;
  // A note from another vault switches to that vault first: themes, sync and
  // the status bar belong to the active vault.
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
  row.append(button, rowActions(more));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    void hooks.openCommandMenu(more, target);
  };
  item.append(row);
  return item;
}

/** Notes of the active vault, for the palette and the counter. */
export function activeNotes(): NoteSummary[] {
  return state.vault ? (trees.get(state.vault.id) ?? []) : [];
}

/**
 * Reloads the tree of one vault.
 *
 * For an inactive vault the core opens it, parses the folder and builds the
 * index. So call this only for the active vault and expanded vaults.
 */
export async function loadNotes(id: string): Promise<NoteSummary[]> {
  const [notes, folders, trash] = await Promise.all([
    api.listNotes(id),
    api.listFolders(id),
    // A broken trash folder must not hide the notes.
    api.listTrash(id).catch(() => []),
  ]);
  trees.set(id, notes);
  folderLists.set(id, folders);
  trashes.set(id, trash);
  return notes;
}

/** Reloads the tree only if it is shown. */
export async function refreshTree(id: string): Promise<void> {
  if (!trees.has(id)) return;
  try {
    await loadNotes(id);
  } catch (error) {
    fail(error);
  }
  renderTree();
}

/** Shows or hides the sidebar: the button, the palette and the key. */
export function toggleSidebar(): void {
  setSidebarHidden(!sidebarHidden);
}

/** Opens a collapsed sidebar: a form or an input in it needs the space. */
export function openSidebar(): void {
  if (sidebarHidden) setSidebarHidden(false);
}

/** Applies the sidebar state of the previous run. */
export function restoreSidebar(): void {
  setSidebarHidden(sidebarHidden);
}

/** Folders of a loaded vault; empty folders are here too. */
export function foldersIn(vaultId: string): string[] {
  return folderLists.get(vaultId) ?? [];
}

/** Expands the vault in the sidebar: the user came for it. */
export function expandVault(id: string): void {
  expanded.add(id);
  rememberExpanded();
}

/** Forgets the tree of a removed vault. Its notes stay on disk. */
export function forgetTree(id: string): void {
  trees.delete(id);
  folderLists.delete(id);
  collapsed.delete(id);
  expanded.delete(id);
  rememberExpanded();
}
