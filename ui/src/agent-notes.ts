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
 * A note can also have no words: `<!-- @agent: instruction -->` at the end of
 * a paragraph is about the whole paragraph. The user leaves one with the
 * cursor in the paragraph and nothing selected.
 *
 * The UI calls these notes "comments" ("заметки"); the marker stays `@agent`,
 * because agents look for it.
 *
 * This file has the parser, the live preview highlight (called from
 * `live-preview.ts`), the gutter marker and the form where the user writes
 * and edits the instruction. The editor context menu (`editor-menu.ts`) opens
 * the form.
 */

import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import {
  RangeSet,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
  type Range,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  WidgetType,
  keymap,
  showTooltip,
  type Tooltip,
} from "@codemirror/view";

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
 * Builds the comment from the words and the instruction. Without words the
 * `«»` part is left out: `<!-- @agent: instruction -->`.
 *
 * The instruction becomes one line: the comment is inside a paragraph, and an
 * empty line would split the paragraph. A `-->` would close the comment too
 * early, and the rest of the instruction would leak into the text.
 */
export function formatNote(anchor: string, text: string): string {
  const clean = text.replace(/\s+/g, " ").replaceAll("-->", "->").trim();
  return anchor === ""
    ? `<!-- ${MARKER}: ${clean} -->`
    : `<!-- ${MARKER} «${anchor}»: ${clean} -->`;
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

/**
 * Icon for a note without words: one about the whole paragraph, or one whose
 * words no longer match the stored ones.
 */
class DetachedWidget extends WidgetType {
  constructor(
    readonly text: string,
    readonly anchored: boolean,
  ) {
    super();
  }

  eq(other: DetachedWidget): boolean {
    return other.text === this.text && other.anchored === this.anchored;
  }

  toDOM(): HTMLElement {
    const dom = document.createElement("span");
    dom.className = "cm-agent-note is-detached";
    dom.title = this.anchored ? `${t("editor.note.detached")}: ${this.text}` : this.text;
    dom.append(icon("comment"));
    return dom;
  }

  // A click puts the cursor next to it, where the context menu edits the note.
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
 * would get in the way of typing. The note form is the better place to edit.
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
      ? Decoration.replace({ widget: new DetachedWidget(note.text, note.anchor !== "") }).range(from, to)
      : Decoration.replace({}).range(from, to),
  );
}

// ——— Gutter marker ———

/**
 * The icon in the gutter (`gutter.ts`) on a line with notes. A click opens the
 * popup of the first note on the line. The highlight stays: it shows which
 * words the note is about, and the icon shows that a long line has a note.
 */
class CommentMarker extends GutterMarker {
  constructor(readonly text: string) {
    super();
  }

  eq(other: CommentMarker): boolean {
    return other.text === this.text;
  }

  toDOM(): Node {
    const dom = document.createElement("span");
    dom.className = "cm-comment-gutter";
    dom.title = this.text;
    dom.append(icon("comment"));
    return dom;
  }
}

/** Where a note shows: its words, or the comment if the words do not match. */
function startOf(note: AgentNote): number {
  return note.anchorFrom ?? note.from;
}

function commentMarkersOf(state: EditorState): RangeSet<GutterMarker> {
  const lines = new Map<number, string[]>();
  for (const note of notesOf(state)) {
    const from = state.doc.lineAt(startOf(note)).from;
    lines.set(from, [...(lines.get(from) ?? []), note.text]);
  }
  return RangeSet.of(
    [...lines].map(([from, texts]) => new CommentMarker(texts.join("\n")).range(from)),
    true,
  );
}

/**
 * The gutter asks for markers on each update, and the notes come from the
 * syntax tree. So the markers are built again only when the text or the tree
 * changes: a long note builds its tree in the background after opening.
 */
const commentMarkers = StateField.define<RangeSet<GutterMarker>>({
  create: (state) => commentMarkersOf(state),
  update: (value, tr) =>
    tr.docChanged || syntaxTree(tr.startState) !== syntaxTree(tr.state)
      ? commentMarkersOf(tr.state)
      : value,
});

/** Note markers for the gutter. */
export function noteMarkers(state: EditorState): RangeSet<GutterMarker> {
  return state.field(commentMarkers, false) ?? RangeSet.empty;
}

/**
 * Opens the form of the first note on the line. A cursor move closes the
 * form, so the cursor goes to the note words first and the form opens after.
 */
export function openNoteOnLine(view: EditorView, line: number): boolean {
  const { from, to } = view.state.doc.line(line);
  const note = notesOf(view.state).find((each) => startOf(each) >= from && startOf(each) <= to);
  if (!note) return false;
  view.dispatch({ selection: { anchor: startOf(note) } });
  openNoteForm(view, { kind: "edit", note });
  return true;
}

// ——— Form ———

/**
 * Where the form adds or edits a note.
 *
 * `add`: a new note goes at `at`. With words it is the end of the selection;
 * without words it is the end of the paragraph, and the note has no anchor:
 * `<!-- @agent: text -->`. `edit`: the note under the cursor.
 */
export type NoteTarget =
  | { kind: "add"; at: number; words: string; pos: number }
  | { kind: "edit"; note: AgentNote };

/** Opens the form for a target, or closes it with `null`. */
const setForm = StateEffect.define<NoteTarget | null>();

interface FormState {
  target: NoteTarget | null;
  tooltip: Tooltip | null;
}

const formField = StateField.define<FormState>({
  create: () => ({ target: null, tooltip: null }),
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setForm)) {
        const target = effect.value;
        return { target, tooltip: target && tooltipFor(target) };
      }
    }
    // The form is for one place. An edit or a move of the cursor (a click in
    // the text) means the user went on with something else.
    if (value.target && (tr.docChanged || tr.selection)) return { target: null, tooltip: null };
    return value;
  },
  provide: (field) => showTooltip.from(field, (value) => value.tooltip),
});

/**
 * Block whose end takes a note without words: a paragraph or an ATX heading.
 * Other blocks (code, tables, empty lines) have no place for an inline
 * comment, and a setext heading ends with its `===` line, so a note there is
 * not possible.
 */
function blockAt(state: EditorState, pos: number): { from: number; to: number } | null {
  const tree = ensureSyntaxTree(state, pos, 200) ?? syntaxTree(state);
  // `-1` looks at the node before the cursor, `1` at the one after: at the
  // start or the end of a paragraph only one of them is inside it.
  for (const side of [-1, 1] as const) {
    const inner = tree.resolveInner(pos, side);
    for (let node: typeof inner | null = inner; node; node = node.parent) {
      if (node.name === "Paragraph" || /^ATXHeading\d$/.test(node.name)) {
        return { from: node.from, to: node.to };
      }
    }
  }
  return null;
}

/**
 * The note under the cursor: its words or its comment contain the cursor. A
 * note without words belongs to its whole paragraph.
 */
export function noteUnder(state: EditorState, pos: number): AgentNote | null {
  const notes = notesOf(state);
  const direct = notes.find((note) => pos >= (note.anchorFrom ?? note.from) && pos <= note.to);
  if (direct) return direct;
  const block = blockAt(state, pos);
  if (!block) return null;
  return (
    notes.find((note) => note.anchor === "" && note.from >= block.from && note.to <= block.to) ??
    null
  );
}

/**
 * What a "Comment" command does now: edits the note under the cursor, adds
 * one to the selected words, or adds one without words to the paragraph.
 * `null` if a note is not possible here.
 */
export function noteTarget(state: EditorState): NoteTarget | null {
  const range = state.selection.main;
  if (!range.empty) {
    const words = state.sliceDoc(range.from, range.to);
    if (!anchorable(words)) return null;
    // A selection that touches the comment means a manual edit of its markup.
    const notes = notesOf(state);
    if (notes.some((note) => range.to > note.from && range.from < note.to)) return null;
    return { kind: "add", at: range.to, words, pos: range.from };
  }

  const note = noteUnder(state, range.head);
  if (note) return { kind: "edit", note };
  const block = blockAt(state, range.head);
  return block && { kind: "add", at: block.to, words: "", pos: range.head };
}

/** The form sits above the words, or above the cursor for a note without words. */
function anchorOf(target: NoteTarget): number {
  if (target.kind === "add") return target.pos;
  return target.note.anchorFrom ?? target.note.from;
}

function tooltipFor(target: NoteTarget): Tooltip {
  return {
    pos: anchorOf(target),
    above: true,
    create: (view) => {
      const dom = document.createElement("div");
      dom.className = "cm-agent-popup is-form";
      return { dom, mount: fillForm(view, dom, target) };
    },
  };
}

function button(label: string, run: () => void): HTMLButtonElement {
  const dom = document.createElement("button");
  dom.type = "button";
  dom.textContent = label;
  // A press must not take focus or clear the editor selection.
  dom.addEventListener("mousedown", (event) => event.preventDefault());
  dom.addEventListener("click", run);
  return dom;
}

/** The note form. Returns `mount`, which focuses it once it is in the document. */
function fillForm(view: EditorView, dom: HTMLElement, target: NoteTarget): () => void {
  const words = target.kind === "add" ? target.words : target.note.anchor;

  const field = document.createElement("textarea");
  field.className = "cm-agent-popup-field";
  field.rows = 2;
  field.placeholder =
    words === ""
      ? t("editor.note.placeholder.block")
      : t("editor.note.placeholder", { words });
  if (target.kind === "edit") field.value = target.note.text;

  // An emptied note means "remove it": the form has no other way to say so.
  const save = (): void => {
    if (field.value.trim() === "") {
      if (target.kind === "edit") removeNote(view, target.note);
      else cancel();
      return;
    }
    if (target.kind === "add") addNote(view, target, field.value);
    else editNote(view, target.note, field.value);
  };
  const cancel = (): void => {
    view.dispatch({ effects: setForm.of(null) });
    view.focus();
  };

  field.addEventListener("keydown", (event) => {
    // A note is one line: Enter saves, line breaks are not needed.
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
  // The gutter icon, ⌘⇧M and the comments panel open this form, not the
  // context menu, so the form must remove a note too.
  if (target.kind === "edit") {
    const remove = button(t("editor.note.remove"), () => removeNote(view, target.note));
    remove.classList.add("is-remove");
    remove.prepend(icon("trash"));
    actions.append(remove);
  }
  actions.append(button(t("editor.note.cancel"), cancel), button(t("editor.note.save"), save));
  actions.lastElementChild?.classList.add("is-primary");

  dom.append(field, actions);
  return () => {
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  };
}

/** Opens the form for a target; the context menu and ⌘⇧M use it. */
export function openNoteForm(view: EditorView, target: NoteTarget): void {
  view.dispatch({ effects: setForm.of(target) });
}

/** ⌘⇧M: the form for whatever `noteTarget` finds at the cursor. */
function openForm(view: EditorView): boolean {
  const target = noteTarget(view.state);
  if (!target) return false;
  openNoteForm(view, target);
  return true;
}

/** Inserts the comment and puts the cursor where the note shows. */
function addNote(view: EditorView, target: Extract<NoteTarget, { kind: "add" }>, text: string): void {
  view.dispatch({
    changes: { from: target.at, insert: formatNote(target.words, text) },
    selection: { anchor: target.words === "" ? target.pos : target.at },
    effects: setForm.of(null),
    userEvent: "input",
  });
  view.focus();
}

function editNote(view: EditorView, note: AgentNote, text: string): void {
  view.dispatch({
    changes: { from: note.from, to: note.to, insert: formatNote(note.anchor, text) },
    effects: setForm.of(null),
    userEvent: "input",
  });
  view.focus();
}

/** Removes the comment; the cursor goes to the end of the words it was about. */
export function removeNote(view: EditorView, note: AgentNote): void {
  view.dispatch({
    changes: { from: note.from, to: note.to },
    selection: note.anchorFrom === null ? undefined : { anchor: note.from },
    effects: setForm.of(null),
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
 * the line to the middle of the screen.
 */
export function revealNote(view: EditorView, note: AgentNote): void {
  const pos = note.anchorFrom ?? note.from;
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: "center" }),
  });
  view.focus();
}

/** The form and ⌘⇧M: add a note to the selection or paragraph, or edit the one under the cursor. */
export function agentNotes(): Extension {
  return [formField, commentMarkers, keymap.of([{ key: "Mod-Shift-m", run: openForm }])];
}
