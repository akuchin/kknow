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

export interface NoteCalls {
  apply: (view: NoteView, options?: { keepEditor?: boolean; reload?: boolean }) => void;
  flush: () => Promise<void>;
}

let calls: NoteCalls;

let agentNotes: AgentNote[] = [];

export let previewNotes: PreviewNotes;

export let tableNotes: PreviewNotes;

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
  state.on("note", () => showNotes(agentNotes));
}

function threadOf(note: AgentNote): Thread | undefined {
  if (note.id === null) return undefined;
  return state.note?.threads?.find((thread) => thread.id === note.id);
}

function asking(): boolean {
  return agentNotes.some((note) => threadOf(note)?.status === "question");
}

export function showNotes(notes: AgentNote[]): void {
  agentNotes = notes;
  el.notesCount.textContent = String(notes.length);
  el.toggleNotes.hidden = notes.length === 0 && el.notesFloat.hidden;
  el.toggleNotes.classList.toggle("has-question", asking());
  el.toggleNotes.title = asking() ? t("topbar.notes.question") : t("topbar.notes.title");
  if (!el.notesFloat.hidden) renderNotes();
}

function replyKey(note: AgentNote): string {
  return `reply:${state.note?.id ?? ""}:${note.id ?? `new:${note.text}`}`;
}

const openReplies = new Set<string>();

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
    button.title = note.anchor ? `«${note.anchor}»\n${note.text}` : note.text;

    const words = document.createElement("span");
    words.className = "notes-words";
    words.textContent = note.anchor || t("panel.notes.noWords");
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
    clearDraft(key);
    openReplies.delete(key);
    if (state.note?.id !== id) return;
    calls.apply(view, thread ? { keepEditor: true } : { reload: true });
    setStatus(t("status.replySent"));
  } catch (error) {
    fail(error);
  }
}

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
