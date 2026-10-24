/** The "General" section: language, the last note, `.md` files, updates. */

import { api, type MarkdownAssociation, type UpdateStatus } from "../api";
import { LOCALE_NAMES, describeError, locales, t } from "../i18n";
import { heading, row, select, toggle } from "./controls";
import { patch, state, view } from "./state";

export function renderGeneral(): void {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.general")));

  page.append(
    row(
      t("settings.general.locale"),
      select(
        locales.map((name) => ({ value: name, label: LOCALE_NAMES[name] ?? name })),
        state.settings!.locale,
        (value) => void patch({ locale: value }),
      ),
      t("settings.general.locale.detail"),
    ),
  );

  page.append(
    row(
      t("settings.general.openLast"),
      toggle(state.settings!.openLastNote, (value) => void patch({ openLastNote: value })),
      t("settings.general.openLast.detail"),
    ),
  );

  page.append(markdownRow());

  page.append(
    row(
      t("settings.general.autoUpdate"),
      toggle(state.settings!.autoUpdate, (value) => void patch({ autoUpdate: value })),
      t("settings.general.autoUpdate.detail"),
    ),
  );
  page.append(updateRow());

  view.body.append(page);
}

/** Auto-update state. It comes as events also while the window is closed to
 *  the "General" section, so it lives here, not in the row. */
let update: UpdateStatus = { state: "idle" };
/** "Check now" was pressed: then `idle` means "you have the latest version",
 *  not "no check yet". */
let checkedByHand = false;
/** Repaints the update row if it is on screen. */
let paintUpdate: (() => void) | null = null;

/**
 * "Updates": the button checks now, and the text under it tells the state.
 *
 * The button works with the setting off too: the setting is about the
 * background checks. The restart itself is in the status bar of the main
 * window, because that window saves the open note before it.
 */
function updateRow(): HTMLElement {
  const check = document.createElement("button");
  check.type = "button";
  check.textContent = t("settings.general.update.check");

  const item = row(t("settings.general.update"), check);
  const state = document.createElement("span");
  state.className = "settings-row-detail";
  item.querySelector(".settings-row-text")!.append(state);

  paintUpdate = () => {
    const busy = update.state === "checking" || update.state === "downloading";
    check.disabled = busy || update.state === "unavailable" || update.state === "ready";
    switch (update.state) {
      case "unavailable":
        state.textContent = t("settings.general.update.unavailable");
        break;
      case "idle":
        state.textContent = checkedByHand ? t("settings.general.update.latest") : "";
        break;
      case "checking":
        state.textContent = t("settings.general.update.checking");
        break;
      case "downloading":
        state.textContent = t("settings.general.update.downloading", {
          version: update.version,
        });
        break;
      case "ready":
        state.textContent = t("settings.general.update.ready", { version: update.version });
        break;
      case "failed":
        state.textContent = t("settings.general.update.failed", { message: update.message });
    }
  };

  check.onclick = async () => {
    checkedByHand = true;
    update = { state: "checking" };
    paintUpdate?.();
    try {
      update = await api.checkUpdate();
    } catch (error) {
      update = { state: "failed", message: describeError(error) };
    }
    paintUpdate?.();
  };

  paintUpdate();
  return item;
}

/**
 * "Open .md files in kknow": the button makes the app the default one, and the
 * text under it tells which app opens markdown now.
 *
 * The window asks the system each time the section shows and stores nothing.
 * The user can change the default app outside kknow, and the window must not
 * show stale data.
 */
function markdownRow(): HTMLElement {
  const claim = document.createElement("button");
  claim.type = "button";
  claim.className = "primary";
  claim.textContent = t("settings.general.markdown.claim");
  claim.disabled = true;

  const item = row(
    t("settings.general.markdown"),
    claim,
    t("settings.general.markdown.detail"),
  );
  const state = document.createElement("span");
  state.className = "settings-row-detail";
  item.querySelector(".settings-row-text")!.append(state);

  const show = (association: MarkdownAssociation) => {
    claim.disabled = !association.supported || association.ours;
    state.textContent = !association.supported
      ? t("settings.general.markdown.unsupported")
      : association.ours
        ? t("settings.general.markdown.ours")
        : association.current
          ? t("settings.general.markdown.current", { app: association.current })
          : t("settings.general.markdown.none");
  };

  claim.onclick = async () => {
    claim.disabled = true;
    try {
      show(await api.claimMarkdown());
      state.textContent = t("settings.general.markdown.done");
    } catch (error) {
      claim.disabled = false;
      state.textContent = describeError(error);
    }
  };

  api.markdownAssociation().then(show, (error) => {
    state.textContent = describeError(error);
  });
  return item;
}

/** Takes a new update state. The row repaints only on `repaintUpdate`. */
export function setUpdate(status: UpdateStatus): void {
  update = status;
}

/** Repaints the update row if it is on screen. */
export function repaintUpdate(): void {
  paintUpdate?.();
}
