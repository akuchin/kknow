/**
 * Agent notes: `words<!-- @agent «words»: instruction -->`.
 *
 * The user selects words and leaves an instruction for an agent that reads
 * the file later. It is stored as a plain HTML comment right after the
 * selected words, so the file stays clean markdown: other viewers do not
 * render the comment, and the agent sees it as is.
 *
 * The words are repeated inside the comment on purpose. The agent does not
 * have to guess what the instruction refers to, and the editor and preview
 * know how many characters to highlight: the words right before the comment
 * are highlighted if they match the stored ones. If they differ (someone
 * edited the text by hand), an icon replaces the highlight.
 *
 * The core has the same parser (`kknow-index/src/agent_note.rs`); the
 * preview uses it. If you change the format, change both places.
 *
 * This file has the parser, the live preview highlight (called from
 * `live-preview.ts`) and the popup where the user writes and edits the
 * instruction.
 */

import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import {
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
  type Range,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  WidgetType,
  keymap,
  showTooltip,
  type Tooltip,
} from "@codemirror/view";

import { bookmarksEnabled, hasBookmark, toggleBookmarkAt } from "./bookmarks";
import { icon } from "./icons";
import { t } from "./i18n";

const MARKER = "@agent";

/** A note in the text: where the comment is and which words it refers to. */
export interface AgentNote {
  /** The `<!-- … -->` comment itself. */
  from: number;
  to: number;
  /** Words from `«…»`; empty means the note has no anchor. */
  anchor: string;
  text: string;
  /** Start of the words before the comment; `null` if the words do not match. */
  anchorFrom: number | null;
}

/** Parses a comment; returns `null` if it is not an agent note. */
export function parseNote(comment: string): { anchor: string; text: string } | null {
  const body = comment.trim();
  if (!body.startsWith("<!--") || !body.endsWith("-->")) return null;

  let rest = body.slice(4, -3).trim();
  if (!rest.startsWith(MARKER)) return null;
  rest = rest.slice(MARKER.length);
  // `@agentX` is not our marker.
  if (rest !== "" && !/^[\s:]/.test(rest)) return null;
  rest = rest.trimStart();

  let anchor = "";
  const close = rest.indexOf("»");
  if (rest.startsWith("«") && close > 0) {
    anchor = rest.slice(1, close);
    rest = rest.slice(close + 1);
  }
  rest = rest.trimStart();
  if (rest.startsWith(":")) rest = rest.slice(1);
  return { anchor, text: rest.trim() };
}

/**
 * Builds the comment from the words and the instruction.
 *
 * The instruction becomes one line: the comment is inside a paragraph, and an
 * empty line would split the paragraph. A `-->` would close the comment too
 * early, and the rest of the instruction would leak into the text.
 */
export function formatNote(anchor: string, text: string): string {
  const clean = text.replace(/\s+/g, " ").replaceAll("-->", "->").trim();
  return `<!-- ${MARKER} «${anchor}»: ${clean} -->`;
}

/** Whether the selected words can be an anchor: one line and no `»`. */
function anchorable(words: string): boolean {
  return words.trim() !== "" && !/[\n»]/.test(words);
}

/** The note for a `Comment` tree node; `null` for other comments. */
export function noteAt(state: EditorState, from: number, to: number): AgentNote | null {
  const parsed = parseNote(state.doc.sliceString(from, to));
  if (!parsed) return null;

  const start = from - parsed.anchor.length;
  const matches =
    parsed.anchor !== "" && start >= 0 && state.doc.sliceString(start, from) === parsed.anchor;
  return { from, to, ...parsed, anchorFrom: matches ? start : null };
}

/**
 * All notes of the document.
 *
 * Uses the syntax tree, not a text search: inside a code block `<!-- … -->` is
 * an example, not a note, and the tree already knows that. Only inline
 * comments count: a comment on its own line follows no words.
 */
export function notesOf(state: EditorState): AgentNote[] {
  const notes: AgentNote[] = [];
  syntaxTree(state).iterate({
    enter: (node) => {
      if (node.name !== "Comment") return;
      const note = noteAt(state, node.from, node.to);
      if (note) notes.push(note);
    },
  });
  return notes;
}

// ——— Live preview highlight ———

/** Icon for a note whose words no longer match the stored ones. */
class DetachedWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }

  eq(other: DetachedWidget): boolean {
    return other.text === this.text;
  }

  toDOM(): HTMLElement {
    const dom = document.createElement("span");
    dom.className = "cm-agent-note is-detached";
    dom.title = `${t("editor.note.detached")}: ${this.text}`;
    dom.append(icon("comment"));
    return dom;
  }

  // A click puts the cursor next to it, and the note pops up.
  ignoreEvent(): boolean {
    return false;
  }
}

/**
 * Decorations for one note: the words are highlighted, the comment is hidden.
 *
 * The live preview `build` calls it on a `Comment` node. The comment opens
 * only when the selection goes inside it. A cursor at the edge is the end of
 * the highlighted words, and a long comment that pops out at each such step
 * would get in the way of typing. The popup is the better place to edit.
 */
export function addAgentNote(
  state: EditorState,
  from: number,
  to: number,
  marks: Range<Decoration>[],
): void {
  const note = noteAt(state, from, to);
  if (!note) return;

  const inside = state.selection.ranges.some((range) => range.to > from && range.from < to);
  if (note.anchorFrom !== null) {
    marks.push(
      Decoration.mark({ class: "cm-agent-note", attributes: { title: note.text } }).range(
        note.anchorFrom,
        from,
      ),
    );
  }
  if (inside) return;
  marks.push(
    note.anchorFrom === null
      ? Decoration.replace({ widget: new DetachedWidget(note.text) }).range(from, to)
      : Decoration.replace({}).range(from, to),
  );
}

// ——— Popup ———

/** Opens or closes the instruction form ("For agent" button, ⌘⇧M, Esc). */
const noteForm = StateEffect.define<boolean>();

/**
 * What the popup shows.
 *
 * `add`: above a selection, offers a new note. `view`: the cursor is in the
 * highlighted words; shows the instruction and lets the user edit or remove it.
 */
type Popup =
  | { kind: "add"; from: number; to: number; words: string }
  | { kind: "view"; note: AgentNote };

interface PopupState {
  popup: Popup | null;
  form: boolean;
  tooltip: Tooltip | null;
}

function popupFor(state: EditorState): Popup | null {
  const range = state.selection.main;
  const notes = notesOf(state);

  if (!range.empty) {
    const words = state.sliceDoc(range.from, range.to);
    if (!anchorable(words)) return null;
    // A selection that touches the comment means a manual edit of its markup.
    if (notes.some((note) => range.to > note.from && range.from < note.to)) return null;
    return { kind: "add", from: range.from, to: range.to, words };
  }

  const note = notes.find((note) => {
    const start = note.anchorFrom ?? note.from;
    return range.head >= start && range.head <= note.to;
  });
  return note ? { kind: "view", note } : null;
}

/** Whether it is the same popup; if so, its DOM (and typed text) stays. */
function samePopup(a: Popup | null, b: Popup | null): boolean {
  if (!a || !b) return a === b;
  if (a.kind === "add" && b.kind === "add") return a.from === b.from && a.to === b.to;
  if (a.kind === "view" && b.kind === "view") {
    return a.note.from === b.note.from && a.note.to === b.note.to && a.note.text === b.note.text;
  }
  return false;
}

const popupField = StateField.define<PopupState>({
  create: (state) => {
    const popup = popupFor(state);
    return { popup, form: false, tooltip: popup && tooltipFor(popup, false) };
  },
  update(value, tr) {
    let form = value.form;
    for (const effect of tr.effects) {
      if (effect.is(noteForm)) form = effect.value;
    }
    if (!tr.docChanged && !tr.selection && form === value.form) return value;

    const popup = popupFor(tr.state);
    if (!popup) return { popup: null, form: false, tooltip: null };
    // Tab in a table selects the next cell to overwrite it, not to ask an
    // agent about it: the popup would cover the table at each step.
    if (popup.kind === "add" && tr.isUserEvent("input.table")) {
      return { popup: null, form: false, tooltip: null };
    }
    const same = samePopup(popup, value.popup);
    // The cursor moved to other words, so the open form closes.
    if (!same) form = false;
    if (same && form === value.form && value.tooltip) {
      // Same popup: CodeMirror knows it by `create` and keeps the DOM, but an
      // edit above may have moved the position.
      return { popup, form, tooltip: { ...value.tooltip, pos: anchorOf(popup) } };
    }
    return { popup, form, tooltip: tooltipFor(popup, form) };
  },
  provide: (field) => showTooltip.from(field, (value) => value.tooltip),
});

/** The popup sits above the start of the words. */
function anchorOf(popup: Popup): number {
  return popup.kind === "add" ? popup.from : (popup.note.anchorFrom ?? popup.note.from);
}

function tooltipFor(popup: Popup, form: boolean): Tooltip {
  return {
    pos: anchorOf(popup),
    above: true,
    create: (view) => {
      const dom = document.createElement("div");
      dom.className = "cm-agent-popup";
      if (form) {
        return { dom, mount: fillForm(view, dom, popup) };
      }
      if (popup.kind === "add") fillAdd(view, dom, popup.from);
      else fillView(view, dom, popup.note);
      return { dom };
    },
  };
}

function button(
  label: string,
  title: string,
  run: () => void,
  iconName?: "comment" | "trash" | "bookmark",
): HTMLButtonElement {
  const dom = document.createElement("button");
  dom.type = "button";
  dom.title = title;
  if (iconName) dom.append(icon(iconName));
  if (label) dom.append(label);
  // A press must not take focus or clear the editor selection.
  dom.addEventListener("mousedown", (event) => event.preventDefault());
  dom.addEventListener("click", run);
  return dom;
}

function fillAdd(view: EditorView, dom: HTMLElement, from: number): void {
  dom.append(
    button(t("editor.note.add"), t("editor.note.add.title"), () => openForm(view), "comment"),
  );
  // The bookmark goes on the line where the selection starts. It sits next to
  // "For agent" because there is only one popup over a selection.
  if (!bookmarksEnabled(view.state)) return;
  const line = view.state.doc.lineAt(from).number;
  const set = hasBookmark(view.state, line);
  dom.append(
    button(
      t(set ? "bookmark.remove" : "bookmark.add"),
      t("bookmark.toggle.title"),
      () => {
        toggleBookmarkAt(view, line);
        // The button label would change only with the selection, so clear the
        // selection; the popup goes with it, as in the preview.
        view.dispatch({ selection: { anchor: view.state.selection.main.head } });
      },
      "bookmark",
    ),
  );
}

function fillView(view: EditorView, dom: HTMLElement, note: AgentNote): void {
  const label = document.createElement("span");
  label.className = "cm-agent-popup-label";
  label.textContent = MARKER;

  const text = document.createElement("span");
  text.className = "cm-agent-popup-text";
  text.textContent = note.text;

  dom.append(
    label,
    text,
    button(t("editor.note.edit"), t("editor.note.edit"), () => openForm(view)),
    button("", t("editor.note.remove"), () => removeNote(view, note), "trash"),
  );
  if (note.anchorFrom === null) dom.title = t("editor.note.detached");
}

/** The instruction form. Returns `mount`, which focuses it once it is in the document. */
function fillForm(view: EditorView, dom: HTMLElement, popup: Popup): () => void {
  const words = popup.kind === "add" ? popup.words : popup.note.anchor;
  dom.classList.add("is-form");

  const field = document.createElement("textarea");
  field.className = "cm-agent-popup-field";
  field.rows = 2;
  field.placeholder = t("editor.note.placeholder", { words });
  if (popup.kind === "view") field.value = popup.note.text;

  const save = (): void => {
    if (field.value.trim() === "") {
      cancel();
      return;
    }
    if (popup.kind === "add") addNote(view, popup, field.value);
    else editNote(view, popup.note, field.value);
  };
  const cancel = (): void => {
    view.dispatch({ effects: noteForm.of(false) });
    view.focus();
  };

  field.addEventListener("keydown", (event) => {
    // An instruction is one line: Enter saves, line breaks are not needed.
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      save();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  });

  const actions = document.createElement("div");
  actions.className = "cm-agent-popup-actions";
  actions.append(
    button(t("editor.note.cancel"), t("editor.note.cancel"), cancel),
    button(t("editor.note.save"), t("editor.note.save"), save),
  );
  actions.lastElementChild?.classList.add("is-primary");

  dom.append(field, actions);
  return () => {
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  };
}

function openForm(view: EditorView): boolean {
  if (!view.state.field(popupField).popup) return false;
  view.dispatch({ effects: noteForm.of(true) });
  return true;
}

/** Inserts the comment right after the words and puts the cursor at their end, where the note shows. */
function addNote(view: EditorView, popup: Extract<Popup, { kind: "add" }>, text: string): void {
  view.dispatch({
    changes: { from: popup.to, insert: formatNote(popup.words, text) },
    selection: { anchor: popup.to },
    effects: noteForm.of(false),
    userEvent: "input",
  });
  view.focus();
}

function editNote(view: EditorView, note: AgentNote, text: string): void {
  view.dispatch({
    changes: { from: note.from, to: note.to, insert: formatNote(note.anchor, text) },
    effects: noteForm.of(false),
    userEvent: "input",
  });
  view.focus();
}

function removeNote(view: EditorView, note: AgentNote): void {
  view.dispatch({
    changes: { from: note.from, to: note.to },
    selection: { anchor: note.from },
    userEvent: "delete",
  });
  view.focus();
}

// ——— Note from the preview ———

/**
 * Nodes whose text the preview does not show, or does not show as paragraph
 * text: comments (also the words repeated inside a note), link URLs, raw HTML
 * and code. In code a comment would also stay literal text.
 */
const HIDDEN_NODES = new Set([
  "Comment",
  "CommentBlock",
  "HTMLTag",
  "HTMLBlock",
  "ProcessingInstruction",
  "URL",
  "LinkTitle",
  "LinkLabel",
  "InlineCode",
  "FencedCode",
  "CodeBlock",
]);

/** Whether the position is inside `[[…]]`. The tree has no such nodes, so check the line text. */
function insideWikiLink(state: EditorState, pos: number): boolean {
  const line = state.doc.lineAt(pos);
  const before = line.text.slice(0, pos - line.from);
  return before.lastIndexOf("[[") > before.lastIndexOf("]]");
}

/**
 * Where the words selected in the preview end in the source.
 *
 * The preview is HTML from the core, and it does not know where its text came
 * from in the markdown. So we search the source for the words, and the index
 * picks the match: the selection is match `index` of `total` in the visible
 * preview text. The index works only if the source has the same number of
 * matches, that is, all of them are visible. Otherwise a single match still
 * works. With several matches we must not guess, or the note would attach to
 * the wrong words. Then the result is `null`.
 *
 * `within` limits the search to a part of the source, such as a table cell.
 */
export function previewAnchor(
  state: EditorState,
  words: string,
  index: number,
  total: number,
  within = { from: 0, to: state.doc.length },
): number | null {
  if (!anchorable(words)) return null;
  // In "Read" mode the editor is hidden, and the tree of a long text may be
  // incomplete. Without it, comments and code look like plain text.
  const tree = ensureSyntaxTree(state, state.doc.length, 500) ?? syntaxTree(state);
  const text = state.doc.sliceString(within.from, within.to);
  const found: number[] = [];

  for (let at = text.indexOf(words); at !== -1; at = text.indexOf(words, at + 1)) {
    const from = within.from + at;
    const to = from + words.length;
    let hidden = false;
    tree.iterate({
      from,
      to,
      enter: (node) => {
        if (HIDDEN_NODES.has(node.name) && node.to > from && node.from < to) hidden = true;
        return !hidden;
      },
    });
    if (!hidden && !insideWikiLink(state, to)) found.push(to);
  }

  if (found.length === total) return found[index] ?? null;
  return found.length === 1 ? found[0] : null;
}

/**
 * Adds a note to words from the preview. The editor cursor does not move: the
 * editor is hidden or shows another place at that time.
 */
export function insertNote(view: EditorView, to: number, words: string, text: string): void {
  view.dispatch({
    changes: { from: to, insert: formatNote(words, text) },
    userEvent: "input",
  });
}

/**
 * Goes to a note from the list: the cursor goes to the start of its words and
 * the line to the middle of the screen. The cursor in the words opens the
 * popup, so the user sees where they are.
 */
export function revealNote(view: EditorView, note: AgentNote): void {
  const pos = note.anchorFrom ?? note.from;
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: "center" }),
  });
  view.focus();
}

/** Popup and ⌘⇧M: add a note to the selection or edit the one under the cursor. */
export function agentNotes(): Extension {
  return [popupField, keymap.of([{ key: "Mod-Shift-m", run: openForm }])];
}
