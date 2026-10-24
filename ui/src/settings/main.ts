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

import { api, onKeymapChanged, onSettingsChanged, onSyncStatus, onUpdateStatus } from "../api";
import { locale, setLocale, t, type Key } from "../i18n";
import "../styles.css";
import { renderAppearance } from "./appearance";
import { renderCommands } from "./commands";
import { renderEditor } from "./editor";
import { renderGeneral, repaintUpdate, setUpdate } from "./general";
import { initKeys, loadKeymap, renderKeys, takeKeymap } from "./keys";
import "./settings.css";
import { state, view } from "./state";
import { renderVaults, repaintVaults } from "./vaults";

/** Window sections: URL anchor and label. */
const SECTIONS: { id: string; label: Key }[] = [
  { id: "general", label: "settings.section.general" },
  { id: "appearance", label: "settings.section.appearance" },
  { id: "vaults", label: "settings.section.vaults" },
  { id: "commands", label: "settings.section.commands" },
  { id: "keys", label: "settings.section.keys" },
  { id: "editor", label: "settings.section.editor" },
];

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
