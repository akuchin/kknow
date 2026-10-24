/** The "Editor" section: the default mode, line numbers and font size. */

import type { ViewMode } from "../api";
import { t } from "../i18n";
import { MODES } from "../note-mode";
import { heading, row, scaleControl, select, toggle } from "./controls";
import { patch, state, view } from "./state";

export function renderEditor(): void {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.editor")));

  page.append(
    row(
      t("settings.editor.mode"),
      select(
        MODES.map((mode) => ({ value: mode, label: t(`mode.${mode}`) })),
        state.settings!.viewMode,
        (value) => void patch({ viewMode: value as ViewMode }),
      ),
      t("settings.editor.mode.detail"),
    ),
  );

  page.append(
    row(
      t("settings.editor.lineNumbers"),
      toggle(state.settings!.lineNumbers, (value) => void patch({ lineNumbers: value })),
      t("settings.editor.lineNumbers.detail"),
    ),
  );

  page.append(
    row(
      t("settings.editor.scale"),
      scaleControl(state.settings!.noteScale, (value) => void patch({ noteScale: value })),
      t("settings.editor.scale.detail"),
    ),
  );

  view.body.append(page);
}
