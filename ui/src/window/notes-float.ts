// The agent notes float and the two selection popups (preview and table cell).
//
// The editor reports the notes of the open text; this zone lists them and
// jumps to each. The module does not touch the DOM on import: `main.ts` calls
// `initNotesFloat` right after the editor exists.

import type { AgentNote } from "../agent-notes";
import { t } from "../i18n";
import { PreviewNotes } from "../preview-notes";
import { cellOf } from "../table-widget";
import { el } from "./dom";
import { editor } from "./editor";
import { state } from "./state";
import { setStatus } from "./status";

/** Agent notes in the open text, listed in the float. */
let agentNotes: AgentNote[] = [];

/** Agent notes from the preview: in "Read" mode the editor is hidden. */
export let previewNotes: PreviewNotes;

/**
 * Agent notes from a closed table cell in "Live preview" mode. A drag there
 * selects text for reading and does not open the cell, so the selection belongs to
 * the browser, not to the editor: it gets the same popup as the preview.
 */
export let tableNotes: PreviewNotes;

/** Creates the two popups and the buttons of the float. */
export function initNotesFloat(): void {
  previewNotes = new PreviewNotes(el.preview, {
    enabled: () => state.mode !== "edit" && state.note !== null,
    add: (words, index, total, text) => editor.addNoteFromPreview(words, index, total, text),
    fail: (message) => setStatus(message, "error"),
    bookmarkable: () => state.note !== null && !state.note.file,
    bookmark: (line) => editor.toggleBookmarkLine(line),
  });

  tableNotes = new PreviewNotes(editor.scroller, {
    enabled: () => state.mode === "edit" && state.note !== null,
    scope: cellOf,
    add: (words, index, total, text, cell) => editor.addNoteInCell(cell, words, index, total, text),
    fail: (message) => setStatus(message, "error"),
    bookmarkable: () => false,
    bookmark: () => {},
  });

  el.toggleNotes.onclick = () => showNotesFloat(el.notesFloat.hidden);
  el.notesHide.onclick = () => showNotesFloat(false);
}

/**
 * Updates the agent notes button and float.
 *
 * The button is visible only when notes exist: most texts have none, and an
 * empty counter would be noise. An open float stays open after the last note
 * is deleted: a window that disappears under the cursor is worse than an
 * empty one.
 */
export function showNotes(notes: AgentNote[]): void {
  agentNotes = notes;
  el.notesCount.textContent = String(notes.length);
  el.toggleNotes.hidden = notes.length === 0 && el.notesFloat.hidden;
  if (!el.notesFloat.hidden) renderNotes();
}

function renderNotes(): void {
  el.notesList.replaceChildren();

  if (agentNotes.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = t("panel.notes.empty");
    el.notesList.append(empty);
    return;
  }

  for (const note of agentNotes) {
    const button = document.createElement("button");
    button.className = "notes-item";
    // CSS truncates the text; the tooltip keeps the full text.
    button.title = note.anchor ? `«${note.anchor}»\n${note.text}` : note.text;

    const words = document.createElement("span");
    words.className = "notes-words";
    words.textContent = note.anchor || t("panel.notes.noWords");
    // A note without words is about its paragraph; it did not lose them.
    if (note.anchorFrom === null && note.anchor !== "") {
      words.classList.add("is-detached");
      words.title = t("editor.note.detached");
    }

    const text = document.createElement("span");
    text.className = "notes-text";
    text.textContent = note.text;

    button.append(words, text);
    button.onclick = () => revealNote(note);

    const item = document.createElement("li");
    item.append(button);
    el.notesList.append(item);
  }
}

/**
 * Jumps to a note from the list. In "Read" mode the editor is hidden, so the
 * preview scrolls to the highlight instead. Highlights are matched by note
 * text, not by order: the core does not highlight words that start inside
 * markup.
 */
function revealNote(note: AgentNote): void {
  if (state.mode !== "read") {
    editor.revealNote(note);
    return;
  }
  const same = agentNotes.filter((each) => each.text === note.text);
  const marks = [...el.preview.querySelectorAll<HTMLElement>(".agent-note")].filter(
    (mark) => mark.title === note.text,
  );
  marks[same.indexOf(note)]?.scrollIntoView({ block: "center", behavior: "smooth" });
}

export function showNotesFloat(show: boolean): void {
  el.notesFloat.hidden = !show;
  el.toggleNotes.setAttribute("aria-pressed", String(show));
  el.toggleNotes.hidden = !show && agentNotes.length === 0;
  if (show) renderNotes();
}
