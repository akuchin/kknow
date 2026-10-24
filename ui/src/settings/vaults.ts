/** The "Vaults" section: the vault list, the settings and statistics of one vault. */

import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  type ConflictPolicy,
  type SyncSettings,
  type VaultStats,
  type VaultView,
} from "../api";
import { formatKey, vaultKey } from "../actions";
import { describeError, locale, t } from "../i18n";
import { iconMarkup } from "../icons";
import { heading, row, select } from "./controls";
import { state, view } from "./state";
import { pickFolder, s3Form, saveSync, switchProvider } from "./sync-form";

/** Label colors: the same set as in the vault registry. */
const COLORS = ["blue", "violet", "amber", "red", "green", "teal"];

/** Auto sync intervals in minutes. */
const INTERVALS = [0, 5, 15, 60];

/** The vault open in the "Vaults" section. */
let picked: string | null = null;

/** The "Last sync" text on the right. An event updates it, not a redraw. */
let lastSyncHint: HTMLElement | null = null;

/**
 * Vaults section: the list on the left, the selected vault settings on the
 * right.
 *
 * The whole section redraws only when the vault list changes: add, remove,
 * rename. Sync state comes as an event and changes only the text. Otherwise
 * the panel would flicker and lose typed field values.
 */
export async function renderVaults(): Promise<void> {
  lastSyncHint = null;
  await refreshVaults();

  const layout = document.createElement("div");
  layout.className = "settings-vaults";

  const list = document.createElement("div");
  list.className = "settings-vault-list";

  const count = document.createElement("p");
  count.className = "settings-vault-count";
  count.textContent = t("settings.vaults.count", { count: state.vaults.length });
  list.append(count);

  for (const vault of state.vaults) {
    const button = document.createElement("button");
    button.className = "settings-vault";
    button.dataset.vault = vault.id;
    button.setAttribute("aria-current", String(vault.id === picked));
    button.onclick = () => {
      picked = vault.id;
      void renderVaults();
    };

    const label = document.createElement("span");
    label.className = "vault-label";
    label.dataset.color = vault.color;

    const text = document.createElement("span");
    text.className = "settings-vault-text";
    const name = document.createElement("span");
    name.textContent = vault.name;
    const state = document.createElement("span");
    state.className = "settings-row-detail";
    state.textContent = describeVault(vault);
    text.append(name, state);

    const dot = document.createElement("span");
    dot.className = "status-dot";
    dot.dataset.tone = toneOf(vault);

    button.append(label, text, dot);
    list.append(button);
  }

  const tools = document.createElement("div");
  tools.className = "settings-vault-tools";

  const add = document.createElement("button");
  add.className = "ghost";
  add.title = t("menu.vault.add");
  add.innerHTML = `${iconMarkup("folder")}<span>${t("settings.vaults.add")}</span>`;
  add.onclick = () => void addVault();

  const remove = document.createElement("button");
  remove.className = "ghost danger";
  remove.innerHTML = `${iconMarkup("unlink")}<span>${t("settings.vaults.remove")}</span>`;
  remove.disabled = picked === null || state.vaults.length < 2;
  remove.title = t("settings.vaults.remove.detail");
  remove.onclick = () => void removeVault();

  tools.append(add, remove);
  list.append(tools);

  const details = document.createElement("div");
  details.className = "settings-page";
  layout.append(list, details);
  view.body.replaceChildren(layout);

  const vault = state.vaults.find((entry) => entry.id === picked);
  if (vault) await renderVaultDetails(details, vault);
}

/** Updates in place what depends on the sync state. */
export function repaintVaults(): void {
  for (const vault of state.vaults) {
    const button = view.body.querySelector<HTMLElement>(
      `.settings-vault[data-vault="${CSS.escape(vault.id)}"]`,
    );
    const state = button?.querySelector<HTMLElement>(".settings-row-detail");
    if (state) state.textContent = describeVault(vault);

    const dot = button?.querySelector<HTMLElement>(".status-dot");
    if (dot) dot.dataset.tone = toneOf(vault);
  }

  if (lastSyncHint) lastSyncHint.textContent = describeLastSync(picked);
}

function describeVault(vault: VaultView): string {
  if (!vault.available) return t("menu.vault.missing");
  const status = state.statuses.get(vault.id);
  if (!status || status.state === "off") return t("statusbar.sync.off");
  if (status.state === "conflict") {
    return t("settings.vaults.conflicts", { count: status.conflicts });
  }
  if (status.state === "error") return t("statusbar.sync.error");
  return status.location ?? t("statusbar.sync.off");
}

function toneOf(vault: VaultView): string {
  if (!vault.available) return "off";
  const status = state.statuses.get(vault.id);
  if (!status) return "off";
  if (status.state === "error") return "error";
  if (status.state === "conflict") return "conflict";
  if (status.state === "off") return "off";
  return "ready";
}

/** Settings of one vault: name, label, path, sync, statistics. */
async function renderVaultDetails(page: HTMLElement, vault: VaultView): Promise<void> {
  const head = document.createElement("div");
  head.className = "settings-vault-head";

  const name = document.createElement("input");
  name.className = "settings-name";
  name.value = vault.name;
  name.onchange = () => void rename(vault.id, name.value);

  const path = document.createElement("span");
  path.className = "settings-path";
  path.textContent = vault.path;

  const finder = document.createElement("button");
  finder.className = "ghost";
  finder.textContent = t("settings.vaults.finder");
  // Show the vault selected in the list, not the active one: this window
  // configures what the user looks at.
  finder.onclick = () => void api.revealNote(undefined, vault.id).catch(console.error);

  const names = document.createElement("div");
  names.className = "settings-vault-names";
  names.append(name, path);
  head.append(names, finder);
  page.append(head);

  // Label: the set is fixed, so it is a row of circles, not a color field.
  const colors = document.createElement("div");
  colors.className = "settings-colors";
  for (const color of COLORS) {
    const swatch = document.createElement("button");
    swatch.className = "settings-color";
    swatch.dataset.color = color;
    swatch.setAttribute("aria-pressed", String(color === vault.color));
    swatch.title = color;
    swatch.onclick = () => void recolor(vault.id, color);
    colors.append(swatch);
  }
  page.append(row(t("settings.vaults.color"), colors));

  page.append(heading(t("settings.vaults.sync")));
  await renderVaultSync(page, vault);
  await renderVaultStats(page, vault);
}

async function renderVaultSync(page: HTMLElement, vault: VaultView): Promise<void> {
  let remote: SyncSettings | null = null;
  try {
    remote = await api.syncSettings(vault.id);
  } catch (error) {
    console.error(error);
  }

  const kind = remote?.provider.kind ?? "off";
  page.append(
    row(
      t("settings.vaults.provider"),
      select(
        [
          { value: "off", label: t("settings.vaults.provider.off") },
          { value: "folder", label: t("dialog.sync.provider.folder") },
          { value: "s3", label: t("dialog.sync.provider.s3") },
        ],
        kind,
        (value) => void switchProvider(vault, remote, value),
      ),
    ),
  );

  if (!remote) return;

  if (remote.provider.kind === "folder") {
    const folder = document.createElement("button");
    folder.className = "ghost";
    folder.textContent = remote.provider.path;
    folder.onclick = () => void pickFolder(vault, remote);
    page.append(row(t("dialog.sync.folder"), folder));
  } else {
    page.append(s3Form(vault, remote));
  }

  page.append(
    row(
      t("dialog.sync.auto"),
      select(
        INTERVALS.map((minutes) => ({
          value: String(minutes),
          label: minutes === 0
            ? t("dialog.sync.auto.off")
            : t("dialog.sync.auto.minutes", { count: minutes }),
        })),
        String(remote.autoSyncMinutes),
        (value) => void saveSync(vault, { ...remote }, { autoSyncMinutes: Number(value) }),
      ),
    ),
  );

  page.append(
    row(
      t("dialog.sync.policy"),
      select(
        [
          { value: "keepBoth", label: t("dialog.sync.policy.keepBoth") },
          { value: "preferLocal", label: t("dialog.sync.policy.preferLocal") },
          { value: "preferRemote", label: t("dialog.sync.policy.preferRemote") },
        ],
        remote.policy,
        (value) => void saveSync(vault, { ...remote }, { policy: value as ConflictPolicy }),
      ),
    ),
  );

  const sync = document.createElement("button");
  sync.className = "primary";
  sync.textContent = t("palette.action.sync");
  sync.onclick = () => void syncNow(vault.id);

  const last = row(t("settings.vaults.lastSync"), sync, describeLastSync(vault.id));
  lastSyncHint = last.querySelector<HTMLElement>(".settings-row-detail");
  page.append(last);
}

/** When the vault last synced. */
function describeLastSync(id: string | null): string {
  const at = id ? state.statuses.get(id)?.at : undefined;
  return at ? new Date(at).toLocaleString(locale) : t("settings.vaults.lastSync.never");
}

async function renderVaultStats(page: HTMLElement, vault: VaultView): Promise<void> {
  page.append(heading(t("settings.vaults.stats")));

  const line = document.createElement("p");
  line.className = "settings-stats";
  line.textContent = t("settings.vaults.stats.loading");
  page.append(line);

  if (!vault.available) {
    line.textContent = t("menu.vault.missing");
    return;
  }

  try {
    const stats: VaultStats = await api.vaultStats(vault.id);
    const hotkey = state.vaults.findIndex((entry) => entry.id === vault.id) + 1;
    line.textContent = [
      t("status.notes", { count: stats.notes }),
      t("settings.vaults.folders", { count: stats.folders }),
      formatSize(stats.bytes),
      hotkey <= 9 ? t("settings.vaults.hotkey", { key: formatKey(vaultKey(hotkey)) }) : "",
    ]
      .filter(Boolean)
      .join(" · ");
  } catch (error) {
    line.textContent = describeError(error);
  }
}

/** Human-readable size: bytes become KB and MB. */
function formatSize(bytes: number): string {
  if (bytes < 1024) return t("settings.vaults.bytes", { count: bytes });
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} КБ`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
}

async function refreshVaults(): Promise<void> {
  try {
    state.vaults = await api.listVaults();
    const all = await api.syncStatuses();
    state.statuses = new Map(all.map((entry) => [entry.vault, entry]));
  } catch (error) {
    console.error(error);
  }

  if (!state.vaults.some((entry) => entry.id === picked)) {
    picked = state.vaults.find((entry) => entry.active)?.id ?? state.vaults[0]?.id ?? null;
  }
}

async function addVault(): Promise<void> {
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== "string") return;

  try {
    const added = await api.addVault(path);
    picked = added.id;
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function removeVault(): Promise<void> {
  if (!picked) return;
  try {
    await api.removeVault(picked);
    picked = null;
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function rename(id: string, name: string): Promise<void> {
  try {
    await api.renameVault(id, name);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function recolor(id: string, color: string): Promise<void> {
  try {
    await api.setVaultColor(id, color);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

async function syncNow(id: string): Promise<void> {
  try {
    await api.syncNow(id);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}
