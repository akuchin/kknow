import { matches, matchesKey, useKeymap, vaultKey, type Action, type Keymap } from "../actions";
import { api } from "../api";
import { toggleAside } from "./aside";
import { nextMode } from "../note-mode";
import { showRunOutput } from "./commands";
import { el } from "./dom";
import { initFind, openFind } from "./find";
import { pickFile } from "./files";
import { goToAdjacentBookmark, toggleCurrentBookmark } from "./marks";
import { changeCurrentScale, flushPendingSave, setMode, travel } from "./note";
import { previewNotes, showNotesFloat, tableNotes } from "./notes-float";
import { mcpFloat, showMcpSetup } from "./mcp";
import { palette } from "./palette";
import { state } from "./state";
import { runSync } from "./sync";
import { hideGit } from "./git";
import { closeTrashView } from "./trash";
import { toggleSidebar } from "./tree";
import { openSettings } from "./util";
import { switchByHotkey } from "./vaults";

const windowActions: [Action, () => boolean | void][] = [
  ["note.new", () => el.newNote.click()],
  ["file.open", () => void pickFile()],
  ["vault.sync", () => void runSync()],
  ["note.save", () => void flushPendingSave()],
  ["note.comment", () => previewNotes.openForm() || tableNotes.openForm()],
  ["note.mode", () => setMode(nextMode(state.mode))],
  ["bookmark.toggle", () => toggleCurrentBookmark()],
  ["bookmark.list", () => palette.open("", "bookmarks")],
  ["task.list", () => palette.open("", "tasks")],
  ["sidebar.toggle", () => toggleSidebar()],
  ["aside.toggle", () => toggleAside()],
  ["history.back", () => void travel(-1)],
  ["history.forward", () => void travel(1)],
  ["bookmark.next", () => goToAdjacentBookmark(1)],
  ["bookmark.previous", () => goToAdjacentBookmark(-1)],
  ["palette.open", () => palette.open()],
  ["note.find", () => openFind()],
  ["note.outline", () => palette.openOutline()],
  ["settings.open", () => void openSettings()],
  ["scale.larger", () => changeCurrentScale(1)],
  ["scale.smaller", () => changeCurrentScale(-1)],
  ["scale.reset", () => changeCurrentScale(0)],
];

export function initKeys(): void {
  initFind();
  document.addEventListener("keydown", (event) => {
    if (event.defaultPrevented) return;

    if (matches(event, "cancel") && !el.runFloat.hidden) {
      event.preventDefault();
      showRunOutput(false);
      return;
    }
    if (matches(event, "cancel") && !mcpFloat.hidden) {
      event.preventDefault();
      showMcpSetup(false);
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
    if (matches(event, "cancel") && !el.gitView.hidden) {
      event.preventDefault();
      hideGit();
      return;
    }

    const found = windowActions.find(([action]) => matches(event, action));
    if (found) {
      if (found[1]() !== false) event.preventDefault();
      return;
    }

    for (let number = 1; number <= 9; number++) {
      if (matchesKey(event, vaultKey(number))) {
        event.preventDefault();
        void switchByHotkey(number);
        return;
      }
    }
  });
}

export async function applyKeymap(keys: Keymap): Promise<boolean> {
  if (!useKeymap(keys)) return false;
  await flushPendingSave();
  location.reload();
  return true;
}

export async function refreshKeymap(): Promise<boolean> {
  try {
    return await applyKeymap((await api.getKeymap()).keys);
  } catch (error) {
    console.error(error);
    return false;
  }
}
