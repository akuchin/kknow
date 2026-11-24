import { syntaxTree } from "@codemirror/language";
import { getSearchQuery, searchPanelOpen } from "@codemirror/search";
import {
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
  type Range,
  type Transaction,
} from "@codemirror/state";
import { Decoration, EditorView, WidgetType, type DecorationSet } from "@codemirror/view";

import { addAgentNote, noteAt } from "./agent-notes";
import { addPageComment } from "./confluence";
import { addEmbed, embedGeneration, embedsChanged } from "./live-embeds";
import { addListMark } from "./live-lists";
import { DIAGRAM_CLASS, DIAGRAM_LANG, renderDiagrams } from "./diagram";
import { copyButton } from "./code-copy";
import { paintCode } from "./highlight";
import { icon } from "./icons";
import { imageElement, isImage, paintImages } from "./images";
import { formatKey } from "./actions";
import { t } from "./i18n";
import { draggedFrom, onPress } from "./pointer";
import { isDelimiter, parseTable, type Table } from "./table";
import { canRunAtCursor } from "./table-editing";
import {
  COMMANDS,
  TableToolsWidget,
  TableWidget,
  sourceTable,
  tableStamp,
  tableWidgets,
} from "./table-widget";

type Tree = ReturnType<typeof syntaxTree>;
type Node = ReturnType<Tree["resolveInner"]>;

const hidden = Decoration.replace({});

const link = Decoration.mark({ class: "cm-live-link" });

const inlineCode = Decoration.mark({ class: "cm-inline-code" });

const codeLine = Decoration.line({ class: "cm-code-line" });
const codeOpen = Decoration.line({ class: "cm-code-open" });
const codeClose = Decoration.line({ class: "cm-code-close" });

const tableLine = Decoration.line({ class: "cm-table-line" });
const tableDelimiter = Decoration.line({ class: "cm-table-line cm-table-delimiter" });
const tableHead = Decoration.line({ class: "cm-table-line cm-table-head" });

const WIKILINK = /\[\[([^\]\n]+)\]\]/g;

const WIKI_IMAGE = /^!\[\[[^\]|\n]+\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)\s*(\|[^\]\n]*)?\]\]$/i;

export const themeChanged = StateEffect.define<void>();

export const liveEnabled = StateEffect.define<boolean>();

const enabled = StateField.define<boolean>({
  create: () => true,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(liveEnabled)) return effect.value;
    }
    return value;
  },
});

const generation = StateField.define<number>({
  create: () => 0,
  update: (value, tr) =>
    tr.effects.some((effect) => effect.is(themeChanged)) ? value + 1 : value,
});

interface Live {
  decorations: DecorationSet;
  atomic: DecorationSet;
}

const live = StateField.define<Live>({
  create: (state) => build(state),
  update(value, tr) {
    const redraw = tr.effects.some((effect) => effect.is(themeChanged) || effect.is(embedsChanged));
    const switched = tr.startState.field(enabled) !== tr.state.field(enabled);
    const tables =
      tr.startState.field(tableStamp) !== tr.state.field(tableStamp) ||
      tr.startState.field(sourceTable) !== tr.state.field(sourceTable);
    const parsed = syntaxTree(tr.startState) !== syntaxTree(tr.state);
    return tr.docChanged || tr.selection || redraw || switched || tables || parsed || searchChanged(tr)
      ? build(tr.state)
      : value;
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value) => value.decorations),
    EditorView.atomicRanges.of((view) => view.state.field(field).atomic),
  ],
});

function searchChanged(tr: Transaction): boolean {
  if (searchPanelOpen(tr.startState) !== searchPanelOpen(tr.state)) return true;
  return !getSearchQuery(tr.startState).eq(getSearchQuery(tr.state));
}

class DiagramWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly generation: number,
  ) {
    super();
  }

  eq(other: DiagramWidget): boolean {
    return other.source === this.source && other.generation === this.generation;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    box.className = "cm-diagram";

    const block = document.createElement("pre");
    block.className = DIAGRAM_CLASS;
    block.textContent = this.source;
    box.append(block);

    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "cm-diagram-edit";
    edit.title = t("editor.diagram.source");
    edit.ariaLabel = edit.title;
    edit.append(icon("code"));
    edit.onclick = () => editSource(view, box);
    box.append(edit);

    void renderDiagrams(box);
    box.ondblclick = () => editSource(view, box);
    return box;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class ImageWidget extends WidgetType {
  constructor(
    private readonly target: string,
    private readonly caption: string,
  ) {
    super();
  }

  eq(other: ImageWidget): boolean {
    return other.target === this.target && other.caption === this.caption;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("span");
    box.className = "cm-image";
    box.append(imageElement(this.target, this.caption));

    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "cm-diagram-edit";
    edit.title = t("editor.diagram.source");
    edit.ariaLabel = edit.title;
    edit.append(icon("code"));
    edit.onclick = () => {
      view.dispatch({ selection: { anchor: view.posAtDOM(box) } });
      view.focus();
    };
    box.append(edit);

    paintImages(box, () => view.requestMeasure());
    return box;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

class CodeWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly language: string,
  ) {
    super();
  }

  eq(other: CodeWidget): boolean {
    return other.source === this.source && other.language === this.language;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("pre");
    box.className = "cm-code-block";
    paintCode(box, this.source, this.language);
    box.title = t("editor.code.edit");
    box.onclick = (event) => {
      if (draggedFrom(event)) return;
      editSource(view, box, clickedOffset(box, event));
    };

    const wrapper = document.createElement("div");
    wrapper.className = "cm-code-widget";
    wrapper.append(box, copyButton(() => this.source));
    return wrapper;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function editSource(view: EditorView, dom: HTMLElement, offset = 0): void {
  const { doc } = view.state;
  const fence = doc.lineAt(view.posAtDOM(dom));
  const body = doc.line(Math.min(fence.number + 1, doc.lines));

  view.dispatch({ selection: { anchor: Math.min(body.from + offset, doc.length) } });
  view.focus();
}

function clickedOffset(dom: HTMLElement, event: MouseEvent): number {
  const range = document.caretRangeFromPoint?.(event.clientX, event.clientY);
  const node = range?.startContainer;
  if (!range || !(node instanceof Text) || !dom.contains(node)) return 0;

  let offset = range.startOffset;
  const walker = document.createTreeWalker(dom, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text && text !== node; text = walker.nextNode()) {
    offset += (text as Text).length;
  }
  return offset;
}

function touched(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((range) => range.from <= to && range.to >= from);
}

function wikiTouched(state: EditorState, from: number, to: number): boolean {
  const wiki = state.doc.sliceString(from - 1, from) === "[" && state.doc.sliceString(to, to + 1) === "]";
  if (!wiki) return touched(state, from, to);
  const bang = state.doc.sliceString(from - 2, from - 1) === "!" ? 1 : 0;
  return touched(state, from - 1 - bang, to + 1);
}

function revealed(state: EditorState, from: number, to: number): boolean {
  const first = state.doc.lineAt(from).from;
  const last = state.doc.lineAt(to).to;

  return state.selection.ranges.some(
    (range) =>
      range.from <= last &&
      range.to >= first &&
      state.doc.lineAt(range.from).from === state.doc.lineAt(range.to).from,
  );
}

function heldOpen(state: EditorState, from: number, to: number): boolean {
  const first = state.doc.lineAt(from).from;
  const last = state.doc.lineAt(to).to;

  return state.selection.ranges.some((range) => range.from >= first && range.to <= last);
}

function found(state: EditorState, from: number, to: number): boolean {
  if (!searchPanelOpen(state)) return false;

  const query = getSearchQuery(state);
  if (!query.valid) return false;
  return !query.getCursor(state, from, to).next().done;
}

function build(state: EditorState): Live {
  if (!state.field(enabled)) return { decorations: Decoration.none, atomic: Decoration.none };

  const marks: Range<Decoration>[] = [];
  const tables: Range<Decoration>[] = [];
  const opened = state.field(sourceTable);
  const raw: [number, number][] = [];
  const tree = syntaxTree(state);
  const age = state.field(generation);

  tree.iterate({
    enter: (node) => {
      if (node.name === "FencedCode") {
        if (
          revealed(state, node.from, node.to) ||
          heldOpen(state, node.from, node.to) ||
          found(state, node.from, node.to)
        ) {
          addCodeLines(state, node.from, node.to, marks);
        } else {
          addBlock(state, node.node, age, marks);
        }
        return false;
      }

      if (node.name === "Table") {
        const asked = opened !== null && opened.from <= node.from && opened.to >= node.to;
        if (asked || found(state, node.from, node.to)) {
          raw.push([node.from, node.to]);
          addTableLines(state, node.from, node.to, marks);
        } else {
          addTable(state, node.from, node.to, marks, tables);
        }
        return false;
      }
      if (node.name === "Image") {
        if (WIKI_IMAGE.test(state.doc.sliceString(node.from, node.to))) return false;
        return !addImage(state, node.node, marks);
      }
      const owner = node.node.parent;

      switch (node.name) {
        case "HeaderMark": {
          if (revealed(state, node.from, node.to)) break;
          if (!owner?.name.startsWith("ATXHeading")) break;
          let to = node.to;
          while (state.doc.sliceString(to, to + 1) === " ") to += 1;
          marks.push(hidden.range(node.from, to));
          break;
        }
        case "EmphasisMark":
        case "CodeMark":
          if (owner && touched(state, owner.from, owner.to)) break;
          marks.push(hidden.range(node.from, node.to));
          break;
        case "InlineCode":
          marks.push(inlineCode.range(node.from, node.to));
          break;
        case "LinkMark":
        case "URL":
          if (owner?.name !== "Link") break;
          if (wikiTouched(state, owner.from, owner.to)) break;
          marks.push(hidden.range(node.from, node.to));
          break;
        case "Link":
          marks.push(link.range(node.from, node.to));
          break;
        case "ListMark":
          addListMark(state, node.node, marks);
          break;
        case "Comment":
          addAgentNote(state, node.from, node.to, marks);
          addPageComment(state, node.from, node.to, marks);
          break;
        case "CommentBlock":
          addPageComment(state, node.from, node.to, marks);
          break;
      }
      return true;
    },
  });

  addWikilinks(state, tree, raw, marks);
  return {
    decorations: Decoration.set(marks, true),
    atomic: Decoration.set(tables, true),
  };
}

function addImage(state: EditorState, node: Node, marks: Range<Decoration>[]): boolean {
  const [open, close] = node.getChildren("LinkMark");
  const url = node.getChild("URL");
  if (!open || !close || !url) return false;

  const caption = state.doc.sliceString(open.to, close.from);
  const target = state.doc.sliceString(url.from, url.to).replace(/^<(.*)>$/, "$1");
  return placeImage(state, node.from, node.to, target, caption, marks);
}

function placeImage(
  state: EditorState,
  from: number,
  to: number,
  target: string,
  caption: string,
  marks: Range<Decoration>[],
): boolean {
  const widget = new ImageWidget(target, caption);
  if (touched(state, from, to)) {
    marks.push(Decoration.widget({ widget, side: 1 }).range(to));
    return false;
  }
  marks.push(Decoration.replace({ widget }).range(from, to));
  return true;
}

function addBlock(state: EditorState, node: Node, age: number, marks: Range<Decoration>[]): void {
  const info = node.getChild("CodeInfo");
  const named = info ? state.doc.sliceString(info.from, info.to).trim() : "";
  const language = named.split(/\s+/)[0].toLowerCase();

  const body = node.getChild("CodeText");
  const source = body ? state.doc.sliceString(body.from, body.to).replace(/\n$/, "") : "";

  const widget =
    language === DIAGRAM_LANG
      ? new DiagramWidget(source, age)
      : new CodeWidget(source, language);

  const from = state.doc.lineAt(node.from).from;
  const to = state.doc.lineAt(node.to).to;
  marks.push(Decoration.replace({ widget, block: true }).range(from, to));
}

function addCodeLines(
  state: EditorState,
  from: number,
  to: number,
  marks: Range<Decoration>[],
): void {
  const first = state.doc.lineAt(from).number;
  const last = state.doc.lineAt(to).number;

  for (let number = first; number <= last; number += 1) {
    const line = state.doc.line(number);
    marks.push(codeLine.range(line.from));
    if (number === first) marks.push(codeOpen.range(line.from));
    if (number === last) marks.push(codeClose.range(line.from));
  }
}

function addTable(
  state: EditorState,
  nodeFrom: number,
  nodeTo: number,
  marks: Range<Decoration>[],
  tables: Range<Decoration>[],
): void {
  const from = state.doc.lineAt(nodeFrom).from;
  const to = state.doc.lineAt(nodeTo).to;
  const source = state.doc.sliceString(from, to);

  const model = parseTable(source, from);
  if (!model) return;

  const widget = new TableWidget(source, model, tableDom(state, model), state.field(tableStamp));
  marks.push(Decoration.replace({ widget, block: true }).range(from, to));
  tables.push(hidden.range(from, to));
}

function addTableLines(
  state: EditorState,
  from: number,
  to: number,
  marks: Range<Decoration>[],
): void {
  const first = state.doc.lineAt(from);
  const last = state.doc.lineAt(to).number;
  const cursor = state.selection.main.head;
  const here = cursor >= first.from && cursor <= state.doc.line(last).to;

  for (let number = first.number; number <= last; number += 1) {
    const line = state.doc.line(number);
    const style =
      number === first.number && here
        ? tableHead
        : number === first.number + 1 && isDelimiter(line.text)
          ? tableDelimiter
          : tableLine;
    marks.push(style.range(line.from));
  }

  if (!here) return;
  const off = COMMANDS.filter((command) => !canRunAtCursor(state, command)).join(",");
  marks.push(Decoration.widget({ widget: new TableToolsWidget(off), side: 1 }).range(first.to));
}

function tableDom(state: EditorState, model: Table): HTMLElement {
  const table = document.createElement("table");
  const head = document.createElement("thead");
  const body = document.createElement("tbody");

  head.append(handleRow(model));

  for (const [index, cells] of model.rows.entries()) {
    const header = index === 0;
    const tr = document.createElement("tr");
    tr.append(handle("row", index, t("editor.table.selectRow", { key: formatKey("Shift") })));

    for (let column = 0; column < model.columns; column += 1) {
      const cell = cells[column];
      const td = document.createElement(header ? "th" : "td");
      td.dataset.row = String(index);
      td.dataset.column = String(column);

      const align = model.aligns[column];
      if (align) td.style.textAlign = align;
      if (cell) td.append(inlineDom(state, cell.from, cell.to));
      tr.append(td);
    }

    (header ? head : body).append(tr);
  }

  table.append(head);
  if (body.childElementCount > 0) table.append(body);
  return table;
}

function handleRow(model: Table): HTMLElement {
  const tr = document.createElement("tr");
  tr.className = "cm-table-handles";
  tr.append(handle("corner", 0, t("editor.table.selectNone")));

  for (let column = 0; column < model.columns; column += 1) {
    tr.append(handle("column", column, t("editor.table.selectColumn", { key: formatKey("Shift") })));
  }
  return tr;
}

function handle(kind: "row" | "column" | "corner", index: number, title: string): HTMLElement {
  const cell = document.createElement("th");
  cell.className = kind === "corner" ? "cm-table-corner" : "cm-table-handle";
  cell.dataset.handle = kind;
  cell.dataset.index = String(index);
  cell.title = title;
  cell.ariaLabel = title;
  return cell;
}

const INLINE_CLASS: Record<string, string> = {
  InlineCode: "cm-inline-code",
  Link: "cm-live-link",
  StrongEmphasis: "cm-strong",
  Emphasis: "cm-emphasis",
};

function inlineDom(state: EditorState, from: number, to: number): DocumentFragment {
  const tree = syntaxTree(state);
  const hidden: [number, number][] = [];
  const styled: [number, number, string][] = [];

  tree.iterate({
    from,
    to,
    enter: (node) => {
      const style = INLINE_CLASS[node.name];
      if (style) {
        styled.push([node.from, node.to, style]);
      } else if (node.name === "EmphasisMark" || node.name === "CodeMark") {
        hidden.push([node.from, node.to]);
      } else if (node.name === "LinkMark" || node.name === "URL") {
        if (node.node.parent?.name === "Link") hidden.push([node.from, node.to]);
      } else if (node.name === "Escape") {
        hidden.push([node.from, node.from + 1]);
      } else if (node.name === "Comment") {
        const note = noteAt(state, node.from, node.to);
        if (note) {
          hidden.push([note.from, note.to]);
          if (note.anchorFrom !== null) styled.push([note.anchorFrom, note.from, "cm-agent-note"]);
        }
      }
      return true;
    },
  });

  for (const match of state.doc.sliceString(from, to).matchAll(WIKILINK)) {
    const start = from + (match.index ?? 0);
    const end = start + match[0].length;
    if (insideMarkup(tree, start)) continue;
    styled.push([start, end, "cm-live-link"]);
    hidden.push([start, start + 2], [end - 2, end]);
  }

  return assemble(state, from, to, hidden, styled);
}

function assemble(
  state: EditorState,
  from: number,
  to: number,
  hidden: [number, number][],
  styled: [number, number, string][],
): DocumentFragment {
  const parts = document.createDocumentFragment();
  let text = "";
  let style = "";

  const flush = () => {
    if (text === "") return;
    if (style === "") {
      parts.append(text);
    } else {
      const span = document.createElement("span");
      span.className = style;
      span.textContent = text;
      parts.append(span);
    }
    text = "";
  };

  for (let pos = from; pos < to; pos += 1) {
    if (hidden.some(([start, end]) => pos >= start && pos < end)) continue;
    const inner = styled.filter(([start, end]) => pos >= start && pos < end).at(-1);
    const next = inner ? inner[2] : "";

    if (next !== style) {
      flush();
      style = next;
    }
    text += state.doc.sliceString(pos, pos + 1);
  }
  flush();

  return parts;
}

function addWikilinks(
  state: EditorState,
  tree: Tree,
  raw: [number, number][],
  marks: Range<Decoration>[],
): void {
  for (const match of state.doc.toString().matchAll(WIKILINK)) {
    const from = match.index ?? 0;
    const to = from + match[0].length;
    if (raw.some(([start, end]) => from >= start && from < end)) continue;

    const [target, ...caption] = match[1].split("|");
    const embed = state.doc.sliceString(from - 1, from) === "!";
    if (embed && isImage(target)) {
      if (!insideCode(tree, from)) {
        placeImage(state, from - 1, to, target.trim(), caption.join("|"), marks);
      }
      continue;
    }
    if (embed && !insideCode(tree, from)) {
      const onLine = revealed(state, from - 1, to);
      if (addEmbed(state, from - 1, to, onLine, marks) || onLine) continue;
    }
    if (touched(state, from, to) || insideMarkup(tree, from)) continue;

    marks.push(link.range(from, to));
    const pipe = match[1].indexOf("|");
    marks.push(hidden.range(from, from + 2 + (pipe < 0 ? 0 : pipe + 1)));
    marks.push(hidden.range(to - 2, to));
  }
}

function insideMarkup(tree: Tree, pos: number): boolean {
  for (let node: Node | null = tree.resolveInner(pos, 1); node; node = node.parent) {
    if (node.name.includes("Code") || node.name.includes("Link")) return true;
  }
  return false;
}

function insideCode(tree: Tree, pos: number): boolean {
  for (let node: Node | null = tree.resolveInner(pos, 1); node; node = node.parent) {
    if (node.name.includes("Code")) return true;
  }
  return false;
}

export function livePreview(): Extension {
  document.addEventListener("mousedown", onPress, true);

  return [generation, embedGeneration, enabled, tableWidgets(), live];
}
