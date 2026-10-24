/**
 * Settings window.
 *
 * A separate window, not a panel in the main one: you see settings next to
 * what you change, and closing them moves nothing in your work. Sections on
 * the left, content on the right; vaults also get a list between them,
 * because there can be many.
 *
 * The window keeps almost no state: settings live in the core, the vault list
 * in the registry. Only what is open on the screen stays here.
 */

import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  onKeymapChanged,
  onSettingsChanged,
  onSyncStatus,
  onUpdateStatus,
  type ConflictPolicy,
  type Provider,
  type SyncSettings,
  type VaultStats,
  type VaultView,
} from "../api";
import { formatKey, vaultKey } from "../actions";
import { iconMarkup } from "../icons";
import { describeError, locale, setLocale, t, type Key } from "../i18n";
import "../styles.css";
import { renderAppearance } from "./appearance";
import { renderCommands } from "./commands";
import { heading, row, select } from "./controls";
import { renderEditor } from "./editor";
import { renderGeneral, repaintUpdate, setUpdate } from "./general";
import { initKeys, loadKeymap, renderKeys, takeKeymap } from "./keys";
import "./settings.css";
import { state, view } from "./state";

/** Window sections: URL anchor and label. */
const SECTIONS: { id: string; label: Key }[] = [
  { id: "general", label: "settings.section.general" },
  { id: "appearance", label: "settings.section.appearance" },
  { id: "vaults", label: "settings.section.vaults" },
  { id: "commands", label: "settings.section.commands" },
  { id: "keys", label: "settings.section.keys" },
  { id: "editor", label: "settings.section.editor" },
];

/** Label colors: the same set as in the vault registry. */
const COLORS = ["blue", "violet", "amber", "red", "green", "teal"];

/** Auto sync intervals in minutes. */
const INTERVALS = [0, 5, 15, 60];

document.documentElement.lang = locale;

const root = document.querySelector<HTMLDivElement>("#settings")!;
root.innerHTML = `
  <nav class="settings-sections" id="settings-sections"></nav>
  <div class="settings-body" id="settings-body"></div>
`;

const el = {
  sections: document.querySelector<HTMLElement>("#settings-sections")!,
  body: document.querySelector<HTMLElement>("#settings-body")!,
};

view.body = el.body;
view.section = section;
view.render = render;

/** The vault open in the "Vaults" section. */
let picked: string | null = null;
/** The "Last sync" text on the right. An event updates it, not a redraw. */
let lastSyncHint: HTMLElement | null = null;

function section(): string {
  const wanted = location.hash.replace("#", "");
  return SECTIONS.some((entry) => entry.id === wanted) ? wanted : "general";
}

function renderSections(): void {
  el.sections.replaceChildren();

  for (const entry of SECTIONS) {
    const button = document.createElement("button");
    button.className = "settings-section";
    button.textContent = t(entry.label);
    button.setAttribute("aria-current", String(entry.id === section()));
    button.onclick = () => {
      location.hash = entry.id;
    };
    el.sections.append(button);
  }

  const version = document.createElement("span");
  version.className = "settings-version";
  version.textContent = "kknow";
  // The version comes from the app, not from a constant here: a release takes
  // it from the git tag, and a constant would stay at the first release.
  void getVersion()
    .then((value) => {
      version.textContent = `kknow ${value}`;
    })
    .catch(console.error);
  el.sections.append(version);
}

function render(): void {
  renderSections();
  el.body.replaceChildren();

  if (!state.settings) return;
  switch (section()) {
    case "appearance":
      void renderAppearance();
      break;
    case "vaults":
      void renderVaults();
      break;
    case "commands":
      void renderCommands();
      break;
    case "keys":
      renderKeys();
      break;
    case "editor":
      renderEditor();
      break;
    default:
      renderGeneral();
  }
}


/**
 * Vaults section: the list on the left, the selected vault settings on the
 * right.
 *
 * The whole section redraws only when the vault list changes: add, remove,
 * rename. Sync state comes as an event and changes only the text. Otherwise
 * the panel would flicker and lose typed field values.
 */
async function renderVaults(): Promise<void> {
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
  el.body.replaceChildren(layout);

  const vault = state.vaults.find((entry) => entry.id === picked);
  if (vault) await renderVaultDetails(details, vault);
}

/** Updates in place what depends on the sync state. */
function repaintVaults(): void {
  for (const vault of state.vaults) {
    const button = el.body.querySelector<HTMLElement>(
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

/**
 * Changes the provider from the list: "do not sync" turns sync off, folder asks
 * for a folder, S3 switches the provider and shows the key form below.
 */
async function switchProvider(
  vault: VaultView,
  remote: SyncSettings | null,
  kind: string,
): Promise<void> {
  if (kind === "off") {
    try {
      await api.forgetSyncSettings(vault.id);
    } catch (error) {
      console.error(error);
    }
    await renderVaults();
    return;
  }

  if (kind === "folder") {
    await pickFolder(vault, remote);
    return;
  }

  // The bucket and keys go into the form below; it appears once the provider
  // is S3. The core does not save an empty bucket, so we use a placeholder
  // name that the user edits right away.
  await saveSync(vault, remote, {
    provider: {
      kind: "s3",
      bucket: remote?.provider.kind === "s3" ? remote.provider.bucket : "kknow",
      endpoint: null,
      region: null,
      prefix: null,
      accessKeyId: null,
    },
  });
}

/**
 * S3 fields: bucket, endpoint, region, prefix, access key and secret.
 *
 * A button saves them, not each keystroke: half a typed endpoint is not a
 * setting yet. The secret goes to the OS keychain and never comes back, so an
 * empty field means "keep the old one".
 */
function s3Form(vault: VaultView, remote: SyncSettings): HTMLElement {
  const s3 = remote.provider.kind === "s3" ? remote.provider : null;
  const form = document.createElement("div");
  form.className = "settings-s3";

  const fields: Record<string, HTMLInputElement> = {};
  const field = (name: string, label: Key, value: string | null, placeholder = ""): void => {
    const line = document.createElement("label");
    const caption = document.createElement("span");
    caption.textContent = t(label);

    const input = document.createElement("input");
    input.value = value ?? "";
    input.placeholder = placeholder;
    input.autocomplete = "off";
    if (name === "secret") input.type = "password";

    fields[name] = input;
    line.append(caption, input);
    form.append(line);
  };

  field("bucket", "dialog.sync.bucket", s3?.bucket ?? "");
  field("endpoint", "dialog.sync.endpoint", s3?.endpoint ?? null, t("dialog.sync.endpoint.placeholder"));
  field("region", "dialog.sync.region", s3?.region ?? null, t("dialog.sync.region.placeholder"));
  field("prefix", "dialog.sync.prefix", s3?.prefix ?? null, t("dialog.sync.prefix.placeholder"));
  field("accessKeyId", "dialog.sync.key", s3?.accessKeyId ?? null, t("dialog.sync.key.placeholder"));
  field(
    "secret",
    "dialog.sync.secret",
    "",
    t(remote.hasSecret ? "dialog.sync.secret.saved" : "dialog.sync.secret.placeholder"),
  );

  const tools = document.createElement("menu");
  const save = document.createElement("button");
  save.className = "primary";
  save.textContent = t("dialog.sync.save");
  save.onclick = () => {
    const text = (name: string): string | null => fields[name].value.trim() || null;
    void saveSync(
      vault,
      remote,
      {
        provider: {
          kind: "s3",
          bucket: fields.bucket.value.trim(),
          endpoint: text("endpoint"),
          region: text("region"),
          prefix: text("prefix"),
          accessKeyId: text("accessKeyId"),
        },
      },
      text("secret"),
    );
  };

  tools.append(save);
  form.append(tools);
  return form;
}

async function pickFolder(vault: VaultView, remote: SyncSettings | null): Promise<void> {
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== "string") return;

  await saveSync(vault, remote, { provider: { kind: "folder", path } });
}

/** Writes the vault sync settings with one field changed. */
async function saveSync(
  vault: VaultView,
  remote: SyncSettings | null,
  change: Partial<{ provider: Provider; policy: ConflictPolicy; autoSyncMinutes: number }>,
  secret: string | null = null,
): Promise<void> {
  const base = {
    provider: remote?.provider ?? { kind: "folder" as const, path: "" },
    policy: remote?.policy ?? ("keepBoth" as ConflictPolicy),
    autoSyncMinutes: remote?.autoSyncMinutes ?? 0,
  };

  try {
    await api.saveSyncSettings({ ...base, ...change }, secret, vault.id);
  } catch (error) {
    console.error(error);
  }
  await renderVaults();
}

initKeys();
window.addEventListener("hashchange", render);
// Esc and ⌘W close the window, like any settings window in the system.
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") window.close();
});

async function boot(): Promise<void> {
  try {
    state.settings = await api.getSettings();
  } catch (error) {
    console.error(error);
  }

  // The window language comes from settings, which the main window shares.
  // `i18n` reads its copy in `localStorage` synchronously on load.
  if (state.settings && state.settings.locale !== locale) {
    setLocale(state.settings.locale);
    location.reload();
    return;
  }

  // Keys: the copy in `localStorage` may be old after a hand edit.
  await loadKeymap();
  void onKeymapChanged(takeKeymap);
  // A hand edit of the file shows when the user comes back to the window.
  window.addEventListener("focus", () => void api.getKeymap().then(takeKeymap, console.error));

  void onSettingsChanged((incoming) => {
    const languageChanged = state.settings?.locale !== incoming.locale;
    state.settings = incoming;
    if (languageChanged) {
      setLocale(incoming.locale);
      location.reload();
      return;
    }
    render();
  });

  // Sync runs without this window too; its state comes as an event.
  void onSyncStatus((incoming) => {
    state.statuses.set(incoming.vault, incoming);
    if (section() === "vaults") repaintVaults();
  });

  // The same for updates: the menu item opens this window and starts a check,
  // so the first events can come before the window asks.
  void onUpdateStatus((incoming) => {
    setUpdate(incoming);
    if (section() === "general") repaintUpdate();
  });
  try {
    setUpdate(await api.updateStatus());
  } catch (error) {
    console.error(error);
  }

  render();
}

void boot();
