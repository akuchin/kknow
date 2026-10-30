// The search palette: its sources and actions. The links of the preview live
// here too: a tag in the preview opens the palette with its query.
//
// The module does not touch the DOM on import: `initPalette` creates the
// palette where the old top-level code created it.

import { api, parseLink } from "../api";
import { t } from "../i18n";
import { MODES } from "../note-mode";
import { Palette } from "../palette";
import { currentTarget, openNoteCommands, runCommand } from "./commands";
import { el } from "./dom";
import { createNote } from "./edits";
import { editor } from "./editor";
import { pickFile } from "./files";
import { goToHeading, openBookmark, openTask, recentNotes, toggleCurrentBookmark } from "./marks";
import { changeCurrentScale, openFound, openNote, setMode, travel } from "./note";
import { state } from "./state";
import { showGit } from "./git";
import { isGit, runSync } from "./sync";
import { activeNotes, filteredVaults, pickedLabelsText, toggleSidebar } from "./tree";
import { openSettings } from "./util";
import { reloadVault, switchVault } from "./vaults";

/**
 * Search palette.
 *
 * It has no state of its own: notes, actions and open history live in the
 * zones of the window. The palette only shows them and calls back.
 */
export let palette: Palette;

/** Wires the links of the preview, then creates the palette and its button. */
export function initPalette(): void {
  // Links in the preview navigate inside the vault, not to the outside.
  el.preview.addEventListener("click", (event) => {
    const anchor = (event.target as HTMLElement).closest("a");
    if (!anchor) return;

    const link = parseLink(anchor.getAttribute("href") ?? "");
    if (!link) return;
    event.preventDefault();

    if (link.kind === "note") {
      void openNote(link.value);
    } else if (link.kind === "missing") {
      void createNote(link.value);
    } else if (link.kind === "tag") {
      // A tag in the preview is a ready query: the palette opens with it.
      palette.open(`tag:${link.value}`);
    }
  });

  palette = new Palette({
    // Search all vaults at once: the user wants the note, the vault is secondary.
    // The active vault comes first; results from others carry their vault label.
    // The label filter of the sidebar narrows the search too: the user picked
    // a group of vaults to work in.
    search: (query) => api.searchAll(query, undefined, filteredVaults() ?? undefined),
    scope: () => pickedLabelsText(),
    bookmarks: () => (state.vault ? api.listBookmarks() : Promise.resolve([])),
    openBookmark: (id, line) => void openBookmark(id, line),
    tasks: () => (state.vault ? api.listTasks() : Promise.resolve([])),
    openTask: (id, line) => void openTask(id, line),
    recent: () => recentNotes(activeNotes()),
    total: () => activeNotes().length,
    activeVault: () => state.vault?.id ?? null,
    open: (id, from) => void openFound(id, from),
    create: (name) => void createNote(name),
    // The editor has the text of a standalone file too, so it gets the outline.
    headings: () => editor.getHeadings(),
    goToHeading,
    actions: () => [
      // Commands of the open note come first: the other actions are about the
      // window, and a command is about what is open in it.
      ...openNoteCommands().map((command) => ({
        label: command.name,
        // Commands have no keys: each user has a different set, and a key for
        // them would take one from the window.
        run: () => {
          const target = currentTarget();
          if (target) void runCommand(command, target);
        },
      })),
      { label: t("palette.action.newNote"), action: "note.new", run: () => el.newNote.click() },
      { label: t("palette.action.openFile"), action: "file.open", run: () => void pickFile() },
      ...MODES.map((each) => ({
        label: t("palette.action.mode", { mode: t(`mode.${each}`) }),
        run: () => setMode(each),
      })),
      { label: t("palette.action.bookmark"), action: "bookmark.toggle", run: () => toggleCurrentBookmark() },
      { label: t("palette.action.bookmarks"), action: "bookmark.list", run: () => palette.open("", "bookmarks") },
      { label: t("palette.action.outline"), action: "note.outline", run: () => palette.openOutline() },
      { label: t("palette.action.tasks"), action: "task.list", run: () => palette.open("", "tasks") },
      { label: t("palette.action.sidebar"), action: "sidebar.toggle", run: () => toggleSidebar() },
      { label: t("palette.action.back"), action: "history.back", run: () => void travel(-1) },
      { label: t("palette.action.forward"), action: "history.forward", run: () => void travel(1) },
      { label: t("palette.action.sync"), action: "vault.sync", run: () => void runSync() },
      {
        label: t("palette.action.gitChanges"),
        run: () => (isGit() ? void showGit() : void openSettings("vaults")),
      },
      {
        label: t("palette.action.syncSettings"),
        action: "settings.open",
        run: () => void openSettings("vaults"),
      },
      { label: t("palette.action.reload"), run: () => void reloadVault() },
      { label: t("menu.scale.larger"), action: "scale.larger", run: () => changeCurrentScale(1) },
      { label: t("menu.scale.smaller"), action: "scale.smaller", run: () => changeCurrentScale(-1) },
      { label: t("menu.scale.reset"), action: "scale.reset", run: () => changeCurrentScale(0) },
    ],
  });
  el.openPalette.onclick = () => palette.open();
}

/**
 * "Resolve": switch to the vault and show the conflict copies.
 *
 * A copy is a file with a marker in its name, so search is the way to find
 * it: a query for the marker lists everything to resolve.
 */
export async function resolveConflicts(id: string): Promise<void> {
  await switchVault(id);
  palette.open(t("statusbar.chip.query"));
}
