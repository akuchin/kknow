// The agent notes float and the two selection popups (preview and table cell).
//
// The editor reports the notes of the open text; this zone lists them and
// jumps to each. The module does not touch the DOM on import: `main.ts` calls
// `initNotesFloat` right after the editor exists.

import type { AgentNote } from "../agent-notes";
import { api, type NoteView, type Thread } from "../api";
import { messageAuthor, messageText } from "../core-text";
import { bindDraft, clearDraft, hasDraft, keepFocus } from "../drafts";
import { t, type Key } from "../i18n";
import { PreviewNotes } from "../preview-notes";
import { cellOf } from "../table-widget";
import { el } from "./dom";
import { editor } from "./editor";
import { state } from "./state";
import { fail, setStatus } from "./status";

/**
 * What the floats need from the note zone. `main.ts` passes them: the note
 * zone imports this module, so an import back would be a cycle.
 */
export interface NoteCalls {
  /** Shows a new view of the open note (`applyNote`). */
  apply: (view: NoteView, options?: { keepEditor?: boolean; reload?: boolean }) => void;
  /** Writes the pending edits of the editor to disk. */
  flush: () => Promise<void>;
}

let calls: NoteCalls;

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
export function initNotesFloat(noteCalls: NoteCalls): void {
  calls = noteCalls;
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
  // Threads come with the note: a reply or a sync brings a new view.
  state.on("note", () => showNotes(agentNotes));
}

/** The thread of a note, if the agent or the person already wrote in it. */
function threadOf(note: AgentNote): Thread | undefined {
  if (note.id === null) return undefined;
  return state.note?.threads?.find((thread) => thread.id === note.id);
}

/** A question of the agent waits for the person: the button shows it. */
function asking(): boolean {
  return agentNotes.some((note) => threadOf(note)?.status === "question");
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
  el.toggleNotes.classList.toggle("has-question", asking());
  el.toggleNotes.title = asking() ? t("topbar.notes.question") : t("topbar.notes.title");
  if (!el.notesFloat.hidden) renderNotes();
}

/**
 * The draft key of the reply field under a note. A note without an ID goes
 * by its text: the core gives it an ID only with the first reply.
 */
function replyKey(note: AgentNote): string {
  return `reply:${state.note?.id ?? ""}:${note.id ?? `new:${note.text}`}`;
}

/** Reply fields the person opened: a render keeps them open. */
const openReplies = new Set<string>();

/**
 * Draws the list again. Each reply field keeps its text in a draft, so a
 * reply to one thread, a sync or an edit does not erase what the person typed
 * under another thread.
 */
function renderNotes(): void {
  const refocus = keepFocus(el.notesList);
  try {
    fillNotes();
  } finally {
    refocus();
  }
}

function fillNotes(): void {
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
    const thread = threadOf(note);
    if (thread) {
      words.after(statusChip(thread));
      item.append(threadMessages(thread));
    }
    if (thread?.status !== "done") item.append(replyForm(note, thread));
    el.notesList.append(item);
  }
}

function statusChip(thread: Thread): HTMLElement {
  const chip = document.createElement("span");
  chip.className = "thread-status";
  chip.dataset.status = thread.status;
  chip.textContent = t(`thread.status.${thread.status}` as Key);
  return chip;
}

/** The messages after the task: the task itself is already the item text. */
function threadMessages(thread: Thread): HTMLElement {
  const list = document.createElement("ol");
  list.className = "thread-messages";
  for (const message of thread.messages.slice(1)) {
    const row = document.createElement("li");
    row.dataset.from = message.from === "me" ? "me" : "agent";
    const who = document.createElement("span");
    who.className = "thread-author";
    who.textContent = messageAuthor(message);
    row.append(who, messageText(message));
    list.append(row);
  }
  return list;
}

/**
 * The reply field under a comment. Most comments never need it, so it is a
 * small link that opens the field; a question of the agent opens it at once.
 */
function replyForm(note: AgentNote, thread: Thread | undefined): HTMLElement {
  const box = document.createElement("div");
  box.className = "thread-reply";
  const open = document.createElement("button");
  open.className = "ghost bare thread-reply-open";
  open.textContent = t("panel.notes.reply");
  const key = replyKey(note);
  const field = document.createElement("textarea");
  field.rows = 2;
  field.placeholder = t("panel.notes.reply.placeholder");
  bindDraft(field, key);
  const send = document.createElement("button");
  send.className = "primary";
  send.textContent = t("panel.notes.send");
  const form = document.createElement("div");
  form.className = "thread-reply-form";
  form.append(field, send);
  form.hidden = thread?.status !== "question" && !hasDraft(key) && !openReplies.has(key);
  open.hidden = !form.hidden;
  open.onclick = () => {
    openReplies.add(key);
    form.hidden = false;
    open.hidden = true;
    field.focus();
  };
  const submit = () => {
    const text = field.value.trim();
    if (text !== "") void reply(note, thread, text, key);
  };
  send.onclick = submit;
  field.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      submit();
    }
  });
  box.append(open, form);
  return box;
}

/**
 * Sends the answer. A comment without an ID goes by its line and task: the
 * core gives it an ID, so the editor text changes, and the pending edits go
 * to disk first.
 */
async function reply(
  note: AgentNote,
  thread: Thread | undefined,
  text: string,
  key: string,
): Promise<void> {
  const current = state.note;
  if (!current || current.file) return;
  const id = current.id;
  await calls.flush();
  try {
    const target = thread
      ? { thread: thread.id }
      : {
          line: editor.getText().slice(0, note.from).split("\n").length,
          text: note.text,
        };
    const view = await api.replyThread(id, target, text);
    // Only this field is sent: the drafts of the other threads stay.
    clearDraft(key);
    openReplies.delete(key);
    if (state.note?.id !== id) return;
    // Without a thread the ID went into the text: the editor takes it.
    calls.apply(view, thread ? { keepEditor: true } : { reload: true });
    setStatus(t("status.replySent"));
  } catch (error) {
    fail(error);
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
