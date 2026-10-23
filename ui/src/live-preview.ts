/**
 * Live preview: the markup is drawn right in the editor.
 *
 * There are two rules. Inline markup (`**bold**`, `` `code` ``, links) is
 * revealed per element: the asterisks show only while the cursor or selection
 * touches that element, and its neighbors in the paragraph stay drawn
 * (`touched`). Headings and blocks are revealed by the cursor line
 * (`revealed`): there the element is the line, so blocks need no separate
 * toggle; it is enough to put the cursor in the block.
 *
 * Before, everything was revealed by line. But a markdown paragraph is one
 * line, so "reveal the line" meant "reveal the paragraph": touching it with the
 * cursor showed all its markup at once, and the text reflowed under the hand.
 *
 * The second reason to reveal a block is note search (⌘F): otherwise a match
 * hidden in a widget would not be visible. See `found`.
 *
 * A state field provides the decorations, not a view plugin: CodeMirror
 * requires this for anything that changes block layout, or it cannot compute
 * heights. The cost is a rebuild over the whole document, not only the visible
 * part; for a note this is cheap.
 */

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

import { addAgentNote } from "./agent-notes";
import { DIAGRAM_CLASS, DIAGRAM_LANG, renderDiagrams } from "./diagram";
import { paintCode } from "./highlight";
import { icon } from "./icons";
import { imageElement, isImage, paintImages } from "./images";
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

/** Syntax tree and node types, taken from CodeMirror itself. */
type Tree = ReturnType<typeof syntaxTree>;
type Node = ReturnType<Tree["resolveInner"]>;

/** Hides markup characters. */
const hidden = Decoration.replace({});

/** The visible part of a link, both `[[this]]` and `[this](url)`. */
const link = Decoration.mark({ class: "cm-live-link" });

const inlineCode = Decoration.mark({ class: "cm-inline-code" });

/** A code block line while it is edited; open and close are the fence lines. */
const codeLine = Decoration.line({ class: "cm-code-line" });
const codeOpen = Decoration.line({ class: "cm-code-open" });
const codeClose = Decoration.line({ class: "cm-code-close" });

/**
 * A table line while the cursor is in the table: a monospace font, or the
 * padding that keeps the columns straight would not line them up.
 */
const tableLine = Decoration.line({ class: "cm-table-line" });
const tableDelimiter = Decoration.line({ class: "cm-table-line cm-table-delimiter" });
/** The header line carries the toolbar, so it is the toolbar's anchor. */
const tableHead = Decoration.line({ class: "cm-table-line cm-table-head" });

/**
 * The markdown grammar has no wikilinks, so we find them with a text search,
 * not in the syntax tree.
 */
const WIKILINK = /\[\[([^\]\n]+)\]\]/g;

/** An embedded image `![[file.png|caption]]`, as the parser returns it. */
const WIKI_IMAGE = /^!\[\[[^\]|\n]+\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)\s*(\|[^\]\n]*)?\]\]$/i;

/** The theme changed: mermaid takes colors from its config, not from CSS. */
export const themeChanged = StateEffect.define<void>();

/** Whether to draw markup: with the preview open, the editor shows plain markdown. */
export const liveEnabled = StateEffect.define<boolean>();

/**
 * Whether live preview is on.
 *
 * This is a state field, not a `Compartment` with an extension: a field added
 * through a compartment at runtime gives no decorations, because CodeMirror
 * collects them when it creates the state. It is also simpler: live preview off
 * is an empty decoration set, and the rest of the code does not know about it.
 */
const enabled = StateField.define<boolean>({
  create: () => true,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(liveEnabled)) return effect.value;
    }
    return value;
  },
});

/**
 * Diagram generation number. CodeMirror treats a widget with the same source as
 * unchanged and keeps the drawn SVG; on a theme change that is wrong.
 */
const generation = StateField.define<number>({
  create: () => 0,
  update: (value, tr) =>
    tr.effects.some((effect) => effect.is(themeChanged)) ? value + 1 : value,
});

interface Live {
  decorations: DecorationSet;
  /** Drawn tables: the cursor steps over them, and cells open in their own editors. */
  atomic: DecorationSet;
}

const live = StateField.define<Live>({
  create: (state) => build(state),
  update(value, tr) {
    const redraw = tr.effects.some((effect) => effect.is(themeChanged));
    const switched = tr.startState.field(enabled) !== tr.state.field(enabled);
    const tables =
      tr.startState.field(tableStamp) !== tr.state.field(tableStamp) ||
      tr.startState.field(sourceTable) !== tr.state.field(sourceTable);
    // Parsing is lazy: when a note opens, the tree can be incomplete, and the
    // rest is built in the background. Each step comes as a transaction without
    // edits or cursor moves. Without this check the end of the note would have
    // no widgets until something moves the cursor.
    const parsed = syntaxTree(tr.startState) !== syntaxTree(tr.state);
    // A cursor move changes the set of revealed lines.
    return tr.docChanged || tr.selection || redraw || switched || tables || parsed || searchChanged(tr)
      ? build(tr.state)
      : value;
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value) => value.decorations),
    EditorView.atomicRanges.of((view) => view.state.field(field).atomic),
  ],
});

/** Note search changed: the set of revealed blocks depends on it too. */
function searchChanged(tr: Transaction): boolean {
  if (searchPanelOpen(tr.startState) !== searchPanelOpen(tr.state)) return true;
  return !getSearchQuery(tr.startState).eq(getSearchQuery(tr.state));
}

/** A diagram in place of a ```` ```mermaid ```` block. */
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

    // Same markup as the core outputs for the preview, so the same code draws it.
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
    // The button or a double click opens the source. A single click does not,
    // on purpose: a diagram is a picture, and a random click must not break it into text.
    box.ondblclick = () => editSource(view, box);
    return box;
  }

  /** A picture is not text: no reason to place the cursor or select in it. */
  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * An image in place of `![caption](file)` or `![[file.png]]`. A click opens it
 * over the whole window; the `</>` button opens the source, as for a diagram:
 * an image is not text, and a random click must not turn it into markup.
 */
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

    // The line height is known only after the image loads: without a new
    // measure, CodeMirror would miss clicks below it.
    paintImages(box, () => view.requestMeasure());
    return box;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * A code block widget: the same `<pre>` as in the preview, but one click opens
 * it for editing.
 *
 * Dragging belongs to the browser: people read and copy from a block more often
 * than they edit it. The click that ends a drag does not open editing, or it
 * would destroy the new selection (`click` also fires after a drag).
 */
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
      // A drag selects text, it does not ask for editing. The selection cannot
      // tell us this: the browser does not clear the old selection on a press in
      // a non-editable island, so after the first selection any click would
      // look like the end of a drag.
      if (draggedFrom(event)) return;
      editSource(view, box, clickedOffset(box, event));
    };
    return box;
  }

  /** The browser handles selection and clicks inside the block, not CodeMirror. */
  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * Shows the block source: puts the cursor inside, and the general rule does the
 * rest (the cursor line shows as is). The first block line is the fence, so the
 * cursor goes to the second line, at `offset`, where the user clicked.
 */
function editSource(view: EditorView, dom: HTMLElement, offset = 0): void {
  const { doc } = view.state;
  const fence = doc.lineAt(view.posAtDOM(dom));
  const body = doc.line(Math.min(fence.number + 1, doc.lines));

  view.dispatch({ selection: { anchor: Math.min(body.from + offset, doc.length) } });
  view.focus();
}

/**
 * The source character under the click.
 *
 * The widget text equals the block body in the document, so the offset from
 * the click maps one to one. A click outside the text goes to the block start.
 */
function clickedOffset(dom: HTMLElement, event: MouseEvent): number {
  const range = document.caretRangeFromPoint?.(event.clientX, event.clientY);
  const node = range?.startContainer;
  if (!range || !(node instanceof Text) || !dom.contains(node)) return 0;

  // A highlighted block is a chain of `span`s, not one text node: add all the
  // text before the node to the offset inside it.
  let offset = range.startOffset;
  const walker = document.createTreeWalker(dom, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text && text !== node; text = walker.nextNode()) {
    offset += (text as Text).length;
  }
  return offset;
}

/**
 * Whether the cursor or selection touches the element, bounds inclusive.
 *
 * Inclusive, so the user can add a letter at the end of a bold word without
 * revealing it by hand, and the cursor never lands inside hidden characters:
 * it reveals the element as it approaches. A selection reveals everything it
 * touches, ⌘A too: this shows exactly what gets copied.
 */
function touched(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((range) => range.from <= to && range.to >= from);
}

/**
 * Whether to show a heading or block as is: yes if the cursor or a single-line
 * selection touches its lines.
 *
 * A multiline selection does not reveal a block. It means an action on a
 * chunk (copy, delete, replace), not an edit, so there is nothing to reveal.
 * Before, ⌘A turned diagrams and code blocks into raw text, and the user saw
 * hashes and pipes instead of the note.
 */
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

/**
 * Whether the selection keeps a revealed block open: yes if the range is fully
 * inside the block, even across several lines.
 *
 * The "multiline selection does not reveal" rule does not fit here: it is
 * about ⌘A and drags through a block, while selecting several code lines inside
 * the block is an edit. Without this exception the block collapsed into a
 * widget in the middle of a drag, together with the selection and edit point.
 */
function heldOpen(state: EditorState, from: number, to: number): boolean {
  const first = state.doc.lineAt(from).from;
  const last = state.doc.lineAt(to).to;

  return state.selection.ranges.some((range) => range.from >= first && range.to <= last);
}

/**
 * Whether note search found anything inside the range.
 *
 * This is for blocks: lines hidden in a widget get no decorations, so a match
 * in a code block or table would be invisible until the user reaches it with
 * "next". So a block with a match reveals itself, as it does for the cursor.
 */
function found(state: EditorState, from: number, to: number): boolean {
  // A closed panel leaves the query in the state: without this check, blocks
  // would stay revealed after the search ends.
  if (!searchPanelOpen(state)) return false;

  const query = getSearchQuery(state);
  if (!query.valid) return false;
  return !query.getCursor(state, from, to).next().done;
}

function build(state: EditorState): Live {
  // Live preview off means no decorations: the text shows as it is in the file.
  if (!state.field(enabled)) return { decorations: Decoration.none, atomic: Decoration.none };

  const marks: Range<Decoration>[] = [];
  const tables: Range<Decoration>[] = [];
  const opened = state.field(sourceTable);
  /** Ranges shown fully as source: nothing is decorated there. */
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
        // No need to go inside the block: it has no markup.
        return false;
      }

      if (node.name === "Table") {
        // A table is always drawn, and its cells open in place (as in
        // Obsidian). The markup shows on the `</>` button or when ⌘F finds a
        // match in it; then the whole table is markup, because one row is one
        // line, and the Advanced Tables keys format it.
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
        // The parser also treats `![[diagram.png]]` as an image, but without a
        // URL: `addWikilinks` draws it, and the brackets inside need no hiding.
        if (WIKI_IMAGE.test(state.doc.sliceString(node.from, node.to))) return false;
        // A hidden image skips its children: the widget already shows the caption.
        return !addImage(state, node.node, marks);
      }
      // Marks hide based on their owner: `StrongEmphasis` for `**`, `InlineCode`
      // for a backtick, `Link` for brackets. A cursor inside the owner shows its
      // markup; the rest of the paragraph stays drawn.
      const owner = node.node.parent;

      switch (node.name) {
        case "HeaderMark": {
          // A heading is revealed by line: the line is the heading.
          if (revealed(state, node.from, node.to)) break;
          // Hashes exist only in `## Text` headings; in underlined headings the
          // same mark covers a whole line, and it must not be hidden.
          if (!owner?.name.startsWith("ATXHeading")) break;
          let to = node.to;
          // The space after the hashes is markup too: otherwise the text shifts right.
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
          // The background stays with visible backticks: it is still code.
          marks.push(inlineCode.range(node.from, node.to));
          break;
        case "LinkMark":
        case "URL":
          // An image has the same marks, but they must not hide:
          // `![caption](file)` would leave a stub `!caption`.
          if (owner?.name !== "Link") break;
          if (touched(state, owner.from, owner.to)) break;
          marks.push(hidden.range(node.from, node.to));
          break;
        case "Link":
          marks.push(link.range(node.from, node.to));
          break;
        case "Comment":
          // `<!-- @agent … -->`: the words are highlighted, the comment is hidden.
          addAgentNote(state, node.from, node.to, marks);
          break;
      }
      return true;
    },
  });

  addWikilinks(state, tree, raw, marks);
  return {
    // Decorations come from two passes, so they need sorting.
    decorations: Decoration.set(marks, true),
    atomic: Decoration.set(tables, true),
  };
}

/**
 * `![caption](file)`: a widget replaces the markup. While the cursor touches
 * it, the markup shows with the image below it, so the user sees what changes.
 * Returns whether the markup is hidden.
 */
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

/** A whole block: mermaid as a picture, the rest as the same `<pre>` as in the preview. */
function addBlock(state: EditorState, node: Node, age: number, marks: Range<Decoration>[]): void {
  const info = node.getChild("CodeInfo");
  const named = info ? state.doc.sliceString(info.from, info.to).trim() : "";
  const language = named.split(/\s+/)[0].toLowerCase();

  const body = node.getChild("CodeText");
  // The line break before the closing fence is not part of the code.
  const source = body ? state.doc.sliceString(body.from, body.to).replace(/\n$/, "") : "";

  const widget =
    language === DIAGRAM_LANG
      ? new DiagramWidget(source, age)
      : new CodeWidget(source, language);

  // A block decoration must cover whole lines.
  const from = state.doc.lineAt(node.from).from;
  const to = state.doc.lineAt(node.to).to;
  marks.push(Decoration.replace({ widget, block: true }).range(from, to));
}

/**
 * A code block with the cursor in it: the background and border stay, but the
 * text is normal.
 *
 * Line decorations, not a widget: the edit must change the real text. Line
 * wrapping is on here: CodeMirror computes caret coordinates from the content,
 * and a line with its own scroll would put the caret in the wrong place.
 */
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
  // Parsing failed: better to keep the markup than to lose it.
  if (!model) return;

  const widget = new TableWidget(source, model, tableDom(state, model), state.field(tableStamp));
  marks.push(Decoration.replace({ widget, block: true }).range(from, to));
  tables.push(hidden.range(from, to));
}

/**
 * A table with the cursor in it: the markup in a monospace font and the
 * toolbar above the header. The toolbar shows only for the cursor table: a
 * table that search revealed has no cursor cell to act on.
 */
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

/**
 * Builds a `<table>` from the model.
 *
 * Cells carry row and column numbers: editing and the toolbar buttons use them
 * to find cells.
 *
 * Row and column handles sit at the edges: a press on a handle selects the
 * whole row or column. This tells the toolbar buttons what to remove, and it
 * also helps reading: in a wide table the eye loses the row.
 */
function tableDom(state: EditorState, model: Table): HTMLElement {
  const table = document.createElement("table");
  const head = document.createElement("thead");
  const body = document.createElement("tbody");

  head.append(handleRow(model));

  for (const [index, cells] of model.rows.entries()) {
    const header = index === 0;
    const tr = document.createElement("tr");
    tr.append(handle("row", index, t("editor.table.selectRow")));

    for (let column = 0; column < model.columns; column += 1) {
      const cell = cells[column];
      const td = document.createElement(header ? "th" : "td");
      td.dataset.row = String(index);
      td.dataset.column = String(column);

      const align = model.aligns[column];
      if (align) td.style.textAlign = align;
      // The cell can be missing: a row shorter than others is valid.
      if (cell) td.append(inlineDom(state, cell.from, cell.to));
      tr.append(td);
    }

    (header ? head : body).append(tr);
  }

  table.append(head);
  if (body.childElementCount > 0) table.append(body);
  return table;
}

/** The handle row above the table: the corner and one per column. */
function handleRow(model: Table): HTMLElement {
  const tr = document.createElement("tr");
  tr.className = "cm-table-handles";
  tr.append(handle("corner", 0, t("editor.table.selectNone")));

  for (let column = 0; column < model.columns; column += 1) {
    tr.append(handle("column", column, t("editor.table.selectColumn")));
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

/** Inline markup in a cell gets the same class that the decorations use. */
const INLINE_CLASS: Record<string, string> = {
  InlineCode: "cm-inline-code",
  Link: "cm-live-link",
  StrongEmphasis: "cm-strong",
  Emphasis: "cm-emphasis",
};

/**
 * Cell content as ready DOM.
 *
 * The rules match the text decorations, but the table is a widget, and
 * decorations do not reach inside a widget, so the markup is parsed again.
 * Text goes in only as text nodes: it can come from someone else's vault.
 */
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
        // `\|` is a pipe in the text, not a cell border: show only the pipe.
        hidden.push([node.from, node.from + 1]);
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

/** Splits the range into runs with the same style and builds DOM from them. */
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
    // The last match is the innermost: the tree walk goes top down.
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
  // Scan the whole note text: notes are short, a per-line walk does not pay off.
  for (const match of state.doc.toString().matchAll(WIKILINK)) {
    const from = match.index ?? 0;
    const to = from + match[0].length;
    if (raw.some(([start, end]) => from >= start && from < end)) continue;

    // `![[diagram.png|300]]` is an embedded image: the target is before the
    // first pipe, the caption with size after it, as in `![300](diagram.png)`.
    // The parser sees an `Image` without a URL, so `insideMarkup` would reject
    // it; here only code matters.
    const [target, ...caption] = match[1].split("|");
    if (state.doc.sliceString(from - 1, from) === "!" && isImage(target)) {
      if (!insideCode(tree, from)) {
        placeImage(state, from - 1, to, target.trim(), caption.join("|"), marks);
      }
      continue;
    }
    if (touched(state, from, to) || insideMarkup(tree, from)) continue;

    marks.push(link.range(from, to));
    marks.push(hidden.range(from, from + 2));
    marks.push(hidden.range(to - 2, to));
  }
}

/**
 * Inside code, `[[this]]` is plain text, and inside a link the parser already
 * hides these brackets: a second decoration over the first would be a bug.
 */
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
  // One listener for the window: adding the same handler again does nothing,
  // so a new state (every note) does not multiply it.
  document.addEventListener("mousedown", onPress, true);

  // Order matters: the diagram generation, the toggle and the table state
  // must update before the decorations that read them.
  return [generation, enabled, tableWidgets(), live];
}
