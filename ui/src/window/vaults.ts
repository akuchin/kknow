// The vaults: the vault menus, rename, colour, remove, switch, add and the
// switch by ⌘1…⌘9.
//
// Only this module calls `state.setVault` and `state.setVaults`. A switch
// reloads the tree, the themes, the sync settings and the last note of the
// vault. The module does not touch the DOM on import.

import { open } from "@tauri-apps/plugin-dialog";

import { formKeys, formatKey, vaultKey } from "../actions";
import { api, type VaultInfo, type VaultView } from "../api";
import { describeError, t } from "../i18n";
import { openMenu, type MenuItem } from "../menu";
import { refreshThemes } from "./appearance";
import { el } from "./dom";
import { openCommandMenu, openNewNoteForm, renameInTree } from "./edits";
import { editor } from "./editor";
import { loadVaultMemory } from "./marks";
import {
  applyNote,
  flushPendingSave,
  openNote,
  releaseNote,
  showHeading,
  showPreview,
  stashPosition,
} from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";
import { refreshSync } from "./sync";
import { expandVault, forgetTree, loadNotes, refreshTree, renderTree } from "./tree";
import { openSettings } from "./util";

/** Wires the "Add vault" button of the sidebar. */
export function initVaults(): void {
  el.addVault.onclick = () => void pickVault();
}

/**
 * Vault menu: the list, add and settings.
 *
 * An unavailable vault (for example, the disk is unplugged) stays in the list
 * but is disabled. A silent removal would lose its settings for an external
 * reason.
 */
export function openVaultMenu(anchor: HTMLElement): void {
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

/** Label colors. The theme sets the actual values (`--label-*`). */
const LABEL_COLORS = ["blue", "violet", "amber", "red", "green", "teal"] as const;

/**
 * Menu of a vault row in the sidebar: actions for the whole folder.
 *
 * The "⋯" button and a right click open it. Sync settings stay in their own
 * window because they are large. This menu has the frequent actions: make
 * active, rename, reveal in Finder, remove from the list.
 */
export function openVaultRowMenu(anchor: HTMLElement, entry: VaultView): void {
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

/** Shows a vault: tree, themes, sync and its last note. */
export async function useVault(info: VaultInfo): Promise<void> {
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
export async function switchVault(id: string): Promise<void> {
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

export async function refreshVaults(): Promise<void> {
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
export async function adoptVaults(paths: string[]): Promise<void> {
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

export async function reloadVault(): Promise<void> {
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

/** Switches the vault by hotkey. The core maps the number, so there is one list. */
export async function switchByHotkey(number: number): Promise<void> {
  try {
    const id = await api.vaultByHotkey(number);
    if (id) await switchVault(id);
  } catch (error) {
    fail(error);
  }
}
