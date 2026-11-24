import { open } from "@tauri-apps/plugin-dialog";

import { formKeys, formatKey, vaultKey } from "../actions";
import { api, type VaultInfo, type VaultView } from "../api";
import { describeError, t } from "../i18n";
import { openMenu, type MenuItem } from "../menu";
import { refreshThemes } from "./appearance";
import { refreshAsideLinks } from "./aside";
import { el } from "./dom";
import { openCommandMenu, openNewNoteForm, renameInTree } from "./edits";
import { editor } from "./editor";
import { loadVaultMemory } from "./marks";
import {
  applyNote,
  flushPendingSave,
  openNote,
  redrawPreview,
  releaseNote,
  showHeading,
  showPreview,
  stashPosition,
  syncWithDisk,
} from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";
import { refreshSync } from "./sync";
import { expandVault, forgetTree, loadNotes, refreshTree, renderTree } from "./tree";
import { openSettings } from "./util";

export function initVaults(): void {
  el.addVault.onclick = () => void pickVault();
}

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

const LABEL_COLORS = ["blue", "violet", "amber", "red", "green", "teal"] as const;

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
          detail: entry.labels.map((label) => `#${label}`).join(" ") || undefined,
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

function startRename(entry: VaultView): void {
  const row = el.noteList.querySelector<HTMLElement>(
    `.vault-row[data-vault="${CSS.escape(entry.id)}"]`,
  );
  renameInTree(row, "button.vault", entry.name, (wanted) => renameVault(entry.id, wanted));
}

async function renameVault(id: string, name: string): Promise<void> {
  try {
    state.setVaults(await api.renameVault(id, name));
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

async function forgetVault(entry: VaultView): Promise<void> {
  try {
    state.setVaults(await api.removeVault(entry.id));
  } catch (error) {
    fail(error);
    return;
  }

  forgetTree(entry.id);

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

export async function useVault(info: VaultInfo): Promise<void> {
  stashPosition();
  state.setVault(info);
  releaseNote();
  status.setVault(info.name, info.color);

  loadVaultMemory(info.id);

  expandVault(info.id);
  const notes = await loadNotes(info.id);
  renderTree();

  await refreshThemes();
  await refreshSync();
  setStatus(t("status.notes", { count: info.noteCount }));

  const wanted = localStorage.getItem(`kknow:note:${info.id}`);
  const restore = notes.find((note) => note.id === wanted) ?? notes[0];
  if (restore) {
    await openNote(restore.id);
  } else {
    showHeading(null);
    status.setWords(0);
    editor.setText("");
    showPreview("");
  }
}

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

export async function refreshVaultsOnFocus(): Promise<void> {
  try {
    const fresh = await api.listVaults();
    if (JSON.stringify(fresh) === JSON.stringify(state.vaults)) return;
    state.setVaults(fresh);
    renderTree();
  } catch (error) {
    console.error(error);
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
    if (state.note && !state.note.file) applyNote(await api.loadNote(state.note.id), { reload: true });
  } catch (error) {
    fail(error);
  }
}

export async function switchByHotkey(number: number): Promise<void> {
  try {
    const id = await api.vaultByHotkey(number);
    if (id) await switchVault(id);
  } catch (error) {
    fail(error);
  }
}

export async function onExternalChange(
  changedVault: string,
  updated: string[],
  removed: string[],
): Promise<void> {
  await refreshTree(changedVault);
  if (changedVault !== state.vault?.id || !state.note || state.note.file) return;
  refreshAsideLinks();
  const embeds = [...el.preview.querySelectorAll<HTMLElement>(".embed[data-note]")];
  if (embeds.some((embed) => updated.includes(embed.dataset.note ?? ""))) redrawPreview();

  if (removed.includes(state.note.id)) {
    setStatus(t("status.noteRemoved"), "error");
    return;
  }
  if (!updated.includes(state.note.id)) return;

  await syncWithDisk();
}
