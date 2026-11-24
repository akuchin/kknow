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
import { initEncryption } from "./encryption";
import { initSyncForm } from "./sync-form";
import { renderVaults, repaintVaults } from "./vaults";
import { showError } from "./errors";

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
initSyncForm(renderVaults);
initEncryption(renderVaults);

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
  void getVersion()
    .then((value) => {
      version.textContent = `kknow ${value}`;
    })
    .catch(showError);
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
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") window.close();
});

async function boot(): Promise<void> {
  try {
    state.settings = await api.getSettings();
  } catch (error) {
    showError(error);
  }

  if (state.settings && state.settings.locale !== locale) {
    setLocale(state.settings.locale);
    location.reload();
    return;
  }

  document.title = t("settings.windowTitle");

  await loadKeymap();
  void onKeymapChanged(takeKeymap);
  window.addEventListener("focus", () => void api.getKeymap().then(takeKeymap, showError));

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

  void onSyncStatus((incoming) => {
    state.statuses.set(incoming.vault, incoming);
    if (section() === "vaults") repaintVaults();
  });

  void onUpdateStatus((incoming) => {
    setUpdate(incoming);
    if (section() === "general") repaintUpdate();
  });
  try {
    setUpdate(await api.updateStatus());
  } catch (error) {
    showError(error);
  }

  render();
}

void boot();
