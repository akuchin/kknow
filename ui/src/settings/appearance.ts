/** The "Appearance" section: the theme, note labels and custom themes. */

import { api, type NoteLabel, type Theme } from "../api";
import { describeError, t } from "../i18n";
import { heading, row, select } from "./controls";
import { patch, state, view } from "./state";

export async function renderAppearance(): Promise<void> {
  const page = document.createElement("div");
  page.className = "settings-page";
  page.append(heading(t("settings.section.appearance")));
  view.body.replaceChildren(page);

  let themes: Theme[] = [];
  let problems: string[] = [];
  try {
    const set = await api.listThemes();
    themes = set.themes;
    problems = set.problems;
  } catch (error) {
    problems = [describeError(error)];
  }

  const options = [
    { value: "light", label: t("theme.light") },
    { value: "dark", label: t("theme.dark") },
    ...themes.map((theme) => ({ value: theme.id, label: theme.name })),
  ];

  page.append(
    row(
      t("settings.appearance.theme"),
      select(options, state.settings!.theme, (value) => void patch({ theme: value })),
      t("settings.appearance.theme.detail"),
    ),
  );

  page.append(
    row(
      t("settings.appearance.noteLabel"),
      select(
        [
          { value: "title", label: t("settings.appearance.noteLabel.title") },
          { value: "file", label: t("settings.appearance.noteLabel.file") },
        ],
        state.settings!.noteLabel,
        (value) => void patch({ noteLabel: value as NoteLabel }),
      ),
      t("settings.appearance.noteLabel.detail"),
    ),
  );

  // Theme problems show here too: a theme lives in the vault or in the shared
  // folder, and the person who put it there will fix it.
  for (const problem of [...problems, ...themes.flatMap((theme) => theme.warnings)]) {
    const note = document.createElement("p");
    note.className = "settings-problem";
    note.textContent = problem;
    page.append(note);
  }

  const folder = document.createElement("button");
  folder.className = "ghost";
  folder.textContent = t("settings.appearance.folder.open");
  folder.onclick = () => void api.revealThemes().catch(showProblem);
  page.append(
    row(t("settings.appearance.folder"), folder, t("settings.appearance.folder.detail")),
  );

  page.append(heading(t("settings.appearance.themes")));
  if (themes.length === 0) {
    const empty = document.createElement("p");
    empty.className = "settings-row-detail";
    empty.textContent = t("settings.appearance.themes.none");
    page.append(empty);
  }
  for (const theme of themes) page.append(themeRow(theme));

  function showProblem(error: unknown): void {
    const note = document.createElement("p");
    note.className = "settings-problem";
    note.textContent = describeError(error);
    page.append(note);
  }
}

/**
 * A custom theme row: its source and, for a vault theme, "Make shared".
 *
 * A shared or built-in theme needs no button: all vaults already see it. A vault theme
 * with a shared namesake hides that namesake, so it gets the same button, but
 * the button updates the shared copy.
 */
function themeRow(theme: Theme): HTMLElement {
  if (theme.source !== "vault") {
    const label = document.createElement("span");
    label.className = "settings-row-detail";
    label.textContent = t(
      theme.source === "global"
        ? "settings.appearance.source.global"
        : "settings.appearance.source.builtIn",
    );
    return row(theme.name, label);
  }

  const shared = theme.shadowsGlobal;
  const share = document.createElement("button");
  share.className = "ghost";
  share.textContent = t(shared ? "settings.appearance.update" : "settings.appearance.share");
  share.onclick = async () => {
    share.disabled = true;
    try {
      await api.shareTheme(theme.id);
      await renderAppearance();
    } catch (error) {
      share.disabled = false;
      share.title = describeError(error);
      console.error(error);
    }
  };

  const source = t(shared ? "settings.appearance.source.both" : "settings.appearance.source.vault");
  const detail = t(shared ? "settings.appearance.update.detail" : "settings.appearance.share.detail");
  return row(`${theme.name} · ${source}`, share, detail);
}
