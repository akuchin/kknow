// Sync of the vaults: the sync panel, the status bar segment, the chip about
// other vaults and their states.
//
// The core reports each state as an event, also from auto-sync; `main.ts`
// hands it to `takeStatus`. The module does not touch the DOM on import.

import { api, type SyncConflict, type SyncSettings, type SyncStatus } from "../api";
import { shortcut, withShortcut } from "../actions";
import { describeError, t } from "../i18n";
import { openMenu } from "../menu";
import { describeReport, syncSegment, type SyncWords } from "../sync-text";
import { el } from "./dom";
import { hideGit, refreshGit, toggleGit } from "./git";
import { flushPendingSave, openNote, setMode } from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";
import { openSettings } from "./util";

/** Steps of higher zones that the sync zone starts. */
export interface SyncHooks {
  /** "Resolve" in the chip menu: go to the vault and find its conflict copies. */
  resolve: (vaultId: string) => Promise<void>;
}

let hooks: SyncHooks;

/** Takes the hooks and wires the panel. */
export function initSync(calls: SyncHooks): void {
  hooks = calls;
  el.syncHide.onclick = () => {
    el.syncPanel.hidden = true;
  };
}

/** Sync settings; `null` means this folder has none. */
let remote: SyncSettings | null = null;
let syncing = false;

/** Sync state per vault: the active one shows in the status bar, others as a chip. */
let statuses = new Map<string, SyncStatus>();

/** Shows the sync result: a summary on top, details as a list. */
function showSyncPanel(summary: string, items: HTMLLIElement[]): void {
  // The sync result lives in the right pane, so the pane must be visible.
  // The mode is not saved for the note: the user did not pick it.
  if (state.mode === "edit") setMode("split", { remember: false });
  el.syncSummary.textContent = summary;
  el.syncList.replaceChildren(...items);
  el.syncPanel.hidden = false;
  // Conflicts need the notes: the changes view would hide the list.
  if (items.length > 0) hideGit();
}

/** Conflicts: the local version stays in place, the remote one is a copy next to it. */
function conflictItems(conflicts: SyncConflict[]): HTMLLIElement[] {
  return conflicts.map((conflict) => {
    const item = document.createElement("li");

    const mine = document.createElement("button");
    mine.className = "link";
    mine.textContent = conflict.id;
    mine.onclick = () => void openNote(conflict.id);

    const arrow = document.createElement("span");
    arrow.className = "muted";
    arrow.textContent = t("sync.conflict.arrow");

    const theirs = document.createElement("button");
    theirs.className = "link";
    theirs.textContent = conflict.copy;
    theirs.onclick = () => void openNote(conflict.copy);

    item.append(mine, arrow, theirs);
    return item;
  });
}

/** Whether the active vault syncs through git. */
export function isGit(): boolean {
  return remote?.provider.kind === "git";
}

/**
 * A click on the sync segment. A git vault opens its changes panel: commits
 * come first, and the panel has the sync button. Other vaults sync at once.
 */
export async function clickSync(): Promise<void> {
  if (isGit()) {
    await toggleGit();
    return;
  }
  await runSync();
}

export async function runSync(): Promise<void> {
  // A vault without sync settings cannot sync: open the settings section that
  // configures it.
  if (!remote) {
    void openSettings("vaults");
    return;
  }
  if (syncing) return;

  syncing = true;
  setStatus(t("status.syncing"), "work");
  // Save pending edits now, else they would sync only next time.
  await flushPendingSave();

  try {
    const report = await api.syncNow();
    showSyncPanel(describeReport(report, syncWords), conflictItems(report.conflicts));
    setStatus(
      report.conflicts.length > 0
        ? t("status.conflicts", { count: report.conflicts.length })
        : t("status.synced"),
    );
  } catch (error) {
    // The core already set the segment state to error; only the report is left.
    showSyncPanel(describeError(error), []);
    fail(error);
  } finally {
    syncing = false;
    await refreshStatuses();
    await refreshGit();
  }
}

/** Syncs an inactive vault from the chip. The active vault does not change. */
async function syncVault(id: string): Promise<void> {
  try {
    await api.syncNow(id);
  } catch (error) {
    fail(error);
  }
  await refreshStatuses();
}

/** Sync states of all open vaults. The chip shows them too. */
async function refreshStatuses(): Promise<void> {
  try {
    const all = await api.syncStatuses();
    statuses = new Map(all.map((entry) => [entry.vault, entry]));
  } catch (error) {
    fail(error);
  }
  describeRemote();
}

/** Sync texts get the window language and keys through this object. */
const syncWords: SyncWords = { t, shortcut, withShortcut, describeError };

/** Shows the sync state of the active vault in the status bar and the chip. */
export function describeRemote(): void {
  const own = state.vault ? statuses.get(state.vault.id) : undefined;
  showChip();
  const segment = syncSegment(own, syncWords);
  status.setSync(segment.text, segment.tone, segment.title);
}

/**
 * Chip about a problem elsewhere: a conflict or a sync failure in an inactive
 * vault.
 *
 * The active vault uses the sync segment, so the chip shows only other vaults,
 * and only one of them.
 */
function showChip(): void {
  const other = [...statuses.values()]
    .filter((entry) => entry.vault !== state.vault?.id)
    .find((entry) => entry.state === "conflict" || entry.state === "error");

  if (!other) {
    status.setChip(null);
    return;
  }

  const name = state.vaults.find((entry) => entry.id === other.vault)?.name ?? other.vault;
  status.setChip(
    other.state === "conflict"
      ? t("statusbar.chip.conflicts", { name, count: other.conflicts })
      : t("statusbar.chip.error", { name }),
    other.state === "conflict" ? "warning" : "danger",
  );
}

/** Chip menu: go to the vault with the problem and resolve it. */
export function openChipMenu(anchor: HTMLElement): void {
  const other = [...statuses.values()]
    .filter((entry) => entry.vault !== state.vault?.id)
    .find((entry) => entry.state === "conflict" || entry.state === "error");
  if (!other) return;

  const name = state.vaults.find((entry) => entry.id === other.vault)?.name ?? other.vault;
  openMenu(anchor, [
    {
      title: name,
      items: [
        {
          label: t("statusbar.chip.resolve"),
          glyph: "search",
          key: "R",
          run: () => void hooks.resolve(other.vault),
        },
        {
          label: t("palette.action.sync"),
          glyph: "cloud",
          key: "S",
          run: () => void syncVault(other.vault),
        },
      ],
    },
  ]);
}

/** Sync settings belong to the vault folder, so they load with it. */
export async function refreshSync(): Promise<void> {
  el.syncPanel.hidden = true;
  hideGit();
  try {
    remote = await api.syncSettings();
  } catch (error) {
    remote = null;
    fail(error);
  }
  await refreshStatuses();
}

/** A state from the core: progress, conflicts or a failure of one vault. */
export function takeStatus(incoming: SyncStatus): void {
  statuses.set(incoming.vault, incoming);
  describeRemote();
}
