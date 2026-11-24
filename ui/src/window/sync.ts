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

export interface SyncHooks {
  resolve: (vaultId: string) => Promise<void>;
}

let hooks: SyncHooks;

export function initSync(calls: SyncHooks): void {
  hooks = calls;
  el.syncHide.onclick = () => {
    el.syncPanel.hidden = true;
  };
}

let remote: SyncSettings | null = null;
let syncing = false;

let statuses = new Map<string, SyncStatus>();

function showSyncPanel(summary: string, items: HTMLLIElement[]): void {
  if (state.mode === "edit") setMode("split", { remember: false });
  el.syncSummary.textContent = summary;
  el.syncList.replaceChildren(...items);
  el.syncPanel.hidden = false;
  if (items.length > 0) hideGit();
}

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

export function isGit(): boolean {
  return remote?.provider.kind === "git";
}

export async function clickSync(): Promise<void> {
  if (isGit()) {
    await toggleGit();
    return;
  }
  await runSync();
}

export async function runSync(): Promise<void> {
  if (!remote) {
    void openSettings("vaults");
    return;
  }
  if (syncing) return;

  syncing = true;
  setStatus(t("status.syncing"), "work");
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
    showSyncPanel(describeError(error), []);
    fail(error);
  } finally {
    syncing = false;
    await refreshStatuses();
    await refreshGit();
  }
}

async function syncVault(id: string): Promise<void> {
  try {
    await api.syncNow(id);
  } catch (error) {
    fail(error);
  }
  await refreshStatuses();
}

async function refreshStatuses(): Promise<void> {
  try {
    const all = await api.syncStatuses();
    statuses = new Map(all.map((entry) => [entry.vault, entry]));
  } catch (error) {
    fail(error);
  }
  describeRemote();
}

const syncWords: SyncWords = { t, shortcut, withShortcut, describeError };

export function describeRemote(): void {
  const own = state.vault ? statuses.get(state.vault.id) : undefined;
  showChip();
  const segment = syncSegment(own, syncWords);
  status.setSync(segment.text, segment.tone, segment.title);
}

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

export function takeStatus(incoming: SyncStatus): void {
  statuses.set(incoming.vault, incoming);
  describeRemote();
}
