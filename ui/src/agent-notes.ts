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
import { bindings, formKeys, matches } from "./actions";
import { t } from "./i18n";

const MARKER = "@agent";

export interface AgentNote {
  from: number;
  to: number;
  id: string | null;
  anchor: string;
  text: string;
  anchorFrom: number | null;
}

export function parseNote(
  comment: string,
): { id: string | null; anchor: string; text: string } | null {
  const body = comment.trim();
  if (!body.startsWith("<!--") || !body.endsWith("-->")) return null;

  let rest = body.slice(4, -3).trim();
  if (!rest.startsWith(MARKER)) return null;
  rest = rest.slice(MARKER.length);
  let id: string | null = null;
  const tag = /^#([A-Za-z0-9_-]{1,32})/.exec(rest);
  if (tag) {
    id = tag[1];
    rest = rest.slice(tag[0].length);
  } else if (rest.startsWith("#")) {
    return null;
  }
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
  return { id, anchor, text: rest.trim() };
}

export function formatNote(anchor: string, text: string, id: string | null = null): string {
  const clean = text.replace(/\s+/g, " ").replaceAll("-->", "->").trim();
  const marker = id === null ? MARKER : `${MARKER}#${id}`;
  return anchor === ""
    ? `<!-- ${marker}: ${clean} -->`
    : `<!-- ${marker} «${anchor}»: ${clean} -->`;
}

function anchorable(words: string): boolean {
  return words.trim() !== "" && !/[\n»]/.test(words);
}

export function noteAt(state: EditorState, from: number, to: number): AgentNote | null {
  const parsed = parseNote(state.doc.sliceString(from, to));
  if (!parsed) return null;

  const start = from - parsed.anchor.length;
  const matches =
    parsed.anchor !== "" && start >= 0 && state.doc.sliceString(start, from) === parsed.anchor;
  return { from, to, ...parsed, anchorFrom: matches ? start : null };
}

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

  ignoreEvent(): boolean {
    return false;
  }
}

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

const commentMarkers = StateField.define<RangeSet<GutterMarker>>({
  create: (state) => commentMarkersOf(state),
  update: (value, tr) =>
    tr.docChanged || syntaxTree(tr.startState) !== syntaxTree(tr.state)
      ? commentMarkersOf(tr.state)
      : value,
});

export function noteMarkers(state: EditorState): RangeSet<GutterMarker> {
  return state.field(commentMarkers, false) ?? RangeSet.empty;
}

export function openNoteOnLine(view: EditorView, line: number): boolean {
  const { from, to } = view.state.doc.line(line);
  const note = notesOf(view.state).find((each) => startOf(each) >= from && startOf(each) <= to);
  if (!note) return false;
  view.dispatch({ selection: { anchor: startOf(note) } });
  openNoteForm(view, { kind: "edit", note });
  return true;
}

export type NoteTarget =
  | { kind: "add"; at: number; words: string; pos: number }
  | { kind: "edit"; note: AgentNote };

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
    if (value.target && (tr.docChanged || tr.selection)) return { target: null, tooltip: null };
    return value;
  },
  provide: (field) => showTooltip.from(field, (value) => value.tooltip),
});

function blockAt(state: EditorState, pos: number): { from: number; to: number } | null {
  const tree = ensureSyntaxTree(state, pos, 200) ?? syntaxTree(state);
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

export function noteTarget(state: EditorState): NoteTarget | null {
  const range = state.selection.main;
  if (!range.empty) {
    const words = state.sliceDoc(range.from, range.to);
    if (!anchorable(words)) return null;
    const notes = notesOf(state);
    if (notes.some((note) => range.to > note.from && range.from < note.to)) return null;
    return { kind: "add", at: range.to, words, pos: range.from };
  }

  const note = noteUnder(state, range.head);
  if (note) return { kind: "edit", note };
  const block = blockAt(state, range.head);
  return block && { kind: "add", at: block.to, words: "", pos: range.head };
}

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
  dom.addEventListener("mousedown", (event) => event.preventDefault());
  dom.addEventListener("click", run);
  return dom;
}

function fillForm(view: EditorView, dom: HTMLElement, target: NoteTarget): () => void {
  const words = target.kind === "add" ? target.words : target.note.anchor;

  const field = document.createElement("textarea");
  field.className = "cm-agent-popup-field";
  field.rows = 2;
  field.placeholder =
    words === ""
      ? t("editor.note.placeholder.block", formKeys())
      : t("editor.note.placeholder", { words, ...formKeys() });
  if (target.kind === "edit") field.value = target.note.text;

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
    if (matches(event, "submit") && !event.isComposing) {
      event.preventDefault();
      save();
    } else if (matches(event, "cancel")) {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  });

  const actions = document.createElement("div");
  actions.className = "cm-agent-popup-actions";
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

export function openNoteForm(view: EditorView, target: NoteTarget): void {
  view.dispatch({ effects: setForm.of(target) });
}

function openForm(view: EditorView): boolean {
  const target = noteTarget(view.state);
  if (!target) return false;
  openNoteForm(view, target);
  return true;
}

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
    changes: { from: note.from, to: note.to, insert: formatNote(note.anchor, text, note.id) },
    effects: setForm.of(null),
    userEvent: "input",
  });
  view.focus();
}

export function removeNote(view: EditorView, note: AgentNote): void {
  view.dispatch({
    changes: { from: note.from, to: note.to },
    selection: note.anchorFrom === null ? undefined : { anchor: note.from },
    effects: setForm.of(null),
    userEvent: "delete",
  });
  view.focus();
}

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

function insideWikiLink(state: EditorState, pos: number): boolean {
  const line = state.doc.lineAt(pos);
  const before = line.text.slice(0, pos - line.from);
  return before.lastIndexOf("[[") > before.lastIndexOf("]]");
}

export function previewAnchor(
  state: EditorState,
  words: string,
  index: number,
  total: number,
  within = { from: 0, to: state.doc.length },
): number | null {
  if (!anchorable(words)) return null;
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

export function insertNote(view: EditorView, to: number, words: string, text: string): void {
  view.dispatch({
    changes: { from: to, insert: formatNote(words, text) },
    userEvent: "input",
  });
}

export function revealNote(view: EditorView, note: AgentNote): void {
  const pos = note.anchorFrom ?? note.from;
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: "center" }),
  });
  view.focus();
}

export function agentNotes(): Extension {
  return [formField, commentMarkers, keymap.of(bindings("note.comment", openForm))];
}
