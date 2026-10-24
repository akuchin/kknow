// Keys of the window: the window actions, the keydown handler and the keymap.
//
// The module does not touch the DOM on import: `initKeys` adds the handler
// where the old top-level code added it, after the palette.

import { matches, matchesKey, useKeymap, vaultKey, type Action, type Keymap } from "../actions";
import { api } from "../api";
import { nextMode } from "../note-mode";
import { showRunOutput } from "./commands";
import { el } from "./dom";
import { editor } from "./editor";
import { pickFile } from "./files";
import { toggleCurrentBookmark } from "./marks";
import { changeCurrentScale, flushPendingSave, setMode, travel } from "./note";
import { previewNotes, showNotesFloat, tableNotes } from "./notes-float";
import { palette } from "./palette";
import { state } from "./state";
import { runSync } from "./sync";
import { closeTrashView } from "./trash";
import { toggleSidebar } from "./tree";
import { openSettings } from "./util";
import { switchByHotkey } from "./vaults";

/**
 * The window actions. A run that returns `false` did not take the key: the
 * event goes on as if nobody handled it.
 */
const windowActions: [Action, () => boolean | void][] = [
  ["note.new", () => el.newNote.click()],
  ["file.open", () => void pickFile()],
  ["vault.sync", () => void runSync()],
  ["note.save", () => void flushPendingSave()],
  // In the editor its own keymap handles the key; here it is for a preview selection.
  ["note.comment", () => previewNotes.openForm() || tableNotes.openForm()],
  ["note.mode", () => setMode(nextMode(state.mode))],
  // ⌘B is bold in the editor, so the bookmark uses Shift.
  ["bookmark.toggle", () => toggleCurrentBookmark()],
  ["bookmark.list", () => palette.open("", "bookmarks")],
  ["task.list", () => palette.open("", "tasks")],
  ["sidebar.toggle", () => toggleSidebar()],
  // ⌘⌥← and ⌘⌥→ by convention: the editor uses ⌘[ and ⌘] for indentation.
  ["history.back", () => void travel(-1)],
  ["history.forward", () => void travel(1)],
  // ⌘⇧P is the common command palette key, and users press it by habit.
  // Plain ⌘P stays free: it is print.
  ["palette.open", () => palette.open()],
  // Two searches: ⌘F in the open note, ⌘K in all vaults.
  ["note.find", () => editor.openSearch()],
  // ⌘⇧O as in VS Code: "Go to symbol" in the open file.
  ["note.outline", () => palette.openOutline()],
  ["settings.open", () => void openSettings()],
  ["scale.larger", () => changeCurrentScale(1)],
  ["scale.smaller", () => changeCurrentScale(-1)],
  // Zero is not a vault: ⌘1…⌘9 are vault numbers, ⌘0 resets the size as in a browser.
  ["scale.reset", () => changeCurrentScale(0)],
];

/** Adds the keydown handler of the window. */
export function initKeys(): void {
  document.addEventListener("keydown", (event) => {
    // The editor can handle the key itself: its note search takes ⌘F and ⌘G.
    if (event.defaultPrevented) return;

    // The command output float covers the work, so Escape closes it like any
    // overlay.
    if (matches(event, "cancel") && !el.runFloat.hidden) {
      event.preventDefault();
      showRunOutput(false);
      return;
    }
    if (matches(event, "cancel") && !el.notesFloat.hidden) {
      event.preventDefault();
      showNotesFloat(false);
      return;
    }
    if (matches(event, "cancel") && !el.trashView.hidden) {
      event.preventDefault();
      closeTrashView();
      return;
    }

    const found = windowActions.find(([action]) => matches(event, action));
    if (found) {
      if (found[1]() !== false) event.preventDefault();
      return;
    }

    // ⌘1…⌘9 pick a vault by its position in the list.
    for (let number = 1; number <= 9; number++) {
      if (matchesKey(event, vaultKey(number))) {
        event.preventDefault();
        void switchByHotkey(number);
        return;
      }
    }
  });
}

/**
 * Takes the changed keys. Hints are spread over the markup, like labels, so a
 * change reloads the window, as a language change does. Returns `true` if the
 * window reloads.
 */
export async function applyKeymap(keys: Keymap): Promise<boolean> {
  if (!useKeymap(keys)) return false;
  await flushPendingSave();
  location.reload();
  return true;
}

/** Reads the keys file again: the user may have edited it by hand. */
export async function refreshKeymap(): Promise<boolean> {
  try {
    return await applyKeymap((await api.getKeymap()).keys);
  } catch (error) {
    console.error(error);
    return false;
  }
}
