import { api, type TrashEntryView, type VaultView } from "../api";
import { renderDiagrams } from "../diagram";
import { paintCodeBlocks } from "../highlight";
import { icon } from "../icons";
import { locale, t } from "../i18n";
import { openMenu } from "../menu";
import { el } from "./dom";
import { openFound } from "./note";
import { emptyItem, markActive, rowActions, rowMenu } from "./rows";
import { state } from "./state";
import { fail, setStatus } from "./status";
import { readList } from "./util";

export interface TrashHooks {
  renderTree: () => void;
  refreshTree: (vaultId: string) => Promise<void>;
}

let hooks: TrashHooks;

export function initTrash(calls: TrashHooks): void {
  hooks = calls;
  el.trashViewClose.onclick = () => closeTrashView();
}

const trashOpen = new Set<string>(readList("kknow:trash:open"));

const trashExpanded = new Set<string>();

let trashViewing: { vault: string; entry: string; note: string } | null = null;

export function trashItem(entry: VaultView, entries: TrashEntryView[]): HTMLLIElement {
  const item = document.createElement("li");
  const open = trashOpen.has(entry.id);
  const label = t("sidebar.trash");

  const button = document.createElement("button");
  button.className = "trash";
  button.setAttribute("aria-expanded", String(open));
  button.title = t(open ? "sidebar.folder.collapse" : "sidebar.folder.expand", { name: label });
  button.onclick = () => toggleTrash(entry.id);

  const arrow = icon("chevron");
  arrow.classList.add("chevron");
  const name = document.createElement("span");
  name.className = "note-title";
  name.textContent = label;
  button.append(arrow, icon("trash"), name);
  if (entries.length > 0) {
    const count = document.createElement("span");
    count.className = "tree-count";
    count.textContent = String(entries.length);
    button.append(count);
  }

  const more = rowMenu(t("sidebar.trash.menu"), (anchor) => openTrashMenu(anchor, entry, entries));
  const row = document.createElement("div");
  row.className = "tree-row trash-row";
  row.dataset.vault = entry.id;
  row.append(button, rowActions(more));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    openTrashMenu(more, entry, entries);
  };
  item.append(row);

  if (open) {
    const nested = document.createElement("ul");
    nested.append(
      ...(entries.length > 0
        ? entries.map((trashed) => trashEntryItem(trashed, entry))
        : [emptyItem(t("sidebar.trash.empty"))]),
    );
    item.append(nested);
  }
  return item;
}

function toggleTrash(vaultId: string): void {
  if (!trashOpen.delete(vaultId)) trashOpen.add(vaultId);
  localStorage.setItem("kknow:trash:open", JSON.stringify([...trashOpen]));
  hooks.renderTree();
}

function trashEntryItem(trashed: TrashEntryView, owner: VaultView): HTMLLIElement {
  const item = document.createElement("li");
  const single = trashed.path.endsWith(".md");
  const path = withoutExtension(trashed.path);
  const key = `${owner.id}/${trashed.id}`;
  const open = !single && trashExpanded.has(key);
  const days = Math.floor((Date.now() / 1000 - trashed.deleted) / (24 * 60 * 60));
  const age = days > 0 ? t("sidebar.trash.days", { count: days }) : t("sidebar.trash.today");

  const more = rowMenu(t("sidebar.trash.entry.menu", { name: path }), (anchor) =>
    openTrashEntryMenu(anchor, trashed, path, owner),
  );

  const button = document.createElement("button");
  button.className = single ? "trash-entry" : "trash-entry trash-folder";
  button.title = [
    trashed.path,
    t("sidebar.trash.notes", { count: trashed.notes.length }),
    new Date(trashed.deleted * 1000).toLocaleString(locale),
  ].join("\n");

  const name = document.createElement("span");
  name.className = "note-title";
  name.textContent = path;
  const when = document.createElement("span");
  when.className = "tree-count";
  when.textContent = age;
  if (single) {
    button.dataset.vault = owner.id;
    button.dataset.entry = trashed.id;
    button.dataset.note = trashed.notes[0] ?? "";
    button.onclick = () => void viewTrashNote(owner, trashed, trashed.notes[0] ?? trashed.path);
    button.append(icon("note"), name, when);
  } else {
    button.setAttribute("aria-expanded", String(open));
    button.onclick = () => {
      if (!trashExpanded.delete(key)) trashExpanded.add(key);
      hooks.renderTree();
    };
    const arrow = icon("chevron");
    arrow.classList.add("chevron");
    button.append(arrow, icon("folder"), name, when);
  }

  const row = document.createElement("div");
  row.className = "tree-row trash-entry-row";
  row.append(button, rowActions(more));
  row.oncontextmenu = (event) => {
    event.preventDefault();
    openTrashEntryMenu(more, trashed, path, owner);
  };
  item.append(row);

  if (open) {
    const nested = document.createElement("ul");
    nested.append(
      ...(trashed.notes.length > 0
        ? trashed.notes.map((note) => trashNoteItem(note, trashed, owner))
        : [emptyItem(t("sidebar.trash.noNotes"))]),
    );
    item.append(nested);
  }
  return item;
}

function trashNoteItem(note: string, trashed: TrashEntryView, owner: VaultView): HTMLLIElement {
  const item = document.createElement("li");
  const prefix = `${trashed.path}/`;

  const button = document.createElement("button");
  button.className = "trash-entry";
  button.dataset.vault = owner.id;
  button.dataset.entry = trashed.id;
  button.dataset.note = note;
  button.title = note;
  button.onclick = () => void viewTrashNote(owner, trashed, note);
  const name = document.createElement("span");
  name.className = "note-title";
  name.textContent = withoutExtension(note.startsWith(prefix) ? note.slice(prefix.length) : note);
  button.append(icon("note"), name);

  const row = document.createElement("div");
  row.className = "tree-row trash-entry-row";
  row.append(button);
  item.append(row);
  return item;
}

function withoutExtension(path: string): string {
  return path.replace(/\.md$/i, "");
}

async function viewTrashNote(owner: VaultView, trashed: TrashEntryView, note: string): Promise<void> {
  let html: string;
  try {
    html = await api.trashNote(trashed.id, note, owner.id);
  } catch (error) {
    fail(error);
    return;
  }

  trashViewing = { vault: owner.id, entry: trashed.id, note };
  el.trashViewTitle.textContent = withoutExtension(note);
  el.trashViewTitle.title = `${owner.name} / ${note}`;
  el.trashViewAge.textContent = t("trashView.deleted", {
    when: new Date(trashed.deleted * 1000).toLocaleString(locale),
  });
  const folder = !trashed.path.endsWith(".md");
  el.trashViewRestore.textContent = folder
    ? t("trashView.restoreFolder", { name: trashed.path })
    : t("sidebar.trash.restore");
  el.trashViewRestore.onclick = () => {
    closeTrashView();
    void restoreTrash(owner.id, trashed.id);
  };

  el.trashViewBody.innerHTML = html;
  el.trashViewBody.scrollTop = 0;
  paintCodeBlocks(el.trashViewBody);
  void renderDiagrams(el.trashViewBody);
  el.trashView.hidden = false;
  markActive(null);
  markTrashViewing();
  el.trashViewClose.focus();
}

export function closeTrashView(): void {
  if (el.trashView.hidden) return;
  el.trashView.hidden = true;
  el.trashViewBody.innerHTML = "";
  trashViewing = null;
  markTrashViewing();
  markActive(state.note?.id ?? null);
}

export function markTrashViewing(): void {
  for (const button of el.noteList.querySelectorAll<HTMLButtonElement>("button.trash-entry")) {
    const shown =
      trashViewing !== null &&
      button.dataset.vault === trashViewing.vault &&
      button.dataset.entry === trashViewing.entry &&
      button.dataset.note === trashViewing.note;
    button.classList.toggle("active", shown);
  }
}

function openTrashMenu(anchor: HTMLElement, owner: VaultView, entries: TrashEntryView[]): void {
  openMenu(anchor, [
    {
      title: t("sidebar.trash"),
      items: [
        {
          label: t("sidebar.trash.clear"),
          glyph: "trash",
          tone: "danger",
          disabled: entries.length === 0,
          run: () => void emptyTrash(owner),
        },
      ],
    },
  ]);
}

function openTrashEntryMenu(
  anchor: HTMLElement,
  trashed: TrashEntryView,
  path: string,
  owner: VaultView,
): void {
  openMenu(anchor, [
    {
      title: path,
      items: [
        {
          label: t("sidebar.trash.restore"),
          glyph: "refresh",
          key: "R",
          run: () => void restoreTrash(owner.id, trashed.id),
        },
      ],
    },
    {
      items: [
        {
          label: t("sidebar.trash.delete"),
          glyph: "trash",
          tone: "danger",
          key: "⌫",
          run: () => void deleteTrashEntry(owner.id, trashed.id, path),
        },
      ],
    },
  ]);
}

export async function restoreTrash(vaultId: string, entry: string): Promise<void> {
  try {
    const restored = await api.restoreTrash(entry, vaultId);
    await hooks.refreshTree(vaultId);
    if (restored.length === 1) await openFound(restored[0], vaultId);
    setStatus(t("status.trashRestored", { count: restored.length }));
  } catch (error) {
    fail(error);
  }
}

async function deleteTrashEntry(vaultId: string, entry: string, path: string): Promise<void> {
  if (!window.confirm(t("confirm.deleteTrashEntry", { name: path }))) return;
  try {
    await api.deleteTrashEntry(entry, vaultId);
    if (trashViewing?.vault === vaultId && trashViewing.entry === entry) closeTrashView();
    await hooks.refreshTree(vaultId);
    setStatus(t("status.trashEntryDeleted"));
  } catch (error) {
    fail(error);
  }
}

async function emptyTrash(target: VaultView): Promise<void> {
  if (!window.confirm(t("confirm.emptyTrash", { name: target.name }))) return;
  try {
    await api.emptyTrash(target.id);
    if (trashViewing?.vault === target.id) closeTrashView();
    await hooks.refreshTree(target.id);
    setStatus(t("status.trashEmptied"));
  } catch (error) {
    fail(error);
  }
}

export function viewingTrash(): boolean {
  return trashViewing !== null;
}
